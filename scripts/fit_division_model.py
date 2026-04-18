#!/usr/bin/env python3
"""
Fit Theia's DivisionUndistortionCameraModel (Fitzgibbon 2001) parameters from
the GoPro POLY/PYCF/ZMPL lens model produced by extract-metadata.

The GoPro forward model for a distorted pixel at radius r_d from the principal
point (image-center by convention) is:

    r_n   = r_d / (W / 2)            # normalized by half-width
    theta = POLY(ZMPL * r_n)         # world ray angle vs optical axis [rad]
    r_u   = f * tan(theta)           # equivalent rectilinear (pinhole) radius

Theia's 1-parameter division model is:

    r_u   = r_d / (1 + k * r_d^2)

We pick f from the paraxial derivative at r=0 (so the linear term matches the
GoPro model exactly) and then least-squares fit k over a dense radial grid.
A final joint nonlinear refinement of (f, k) minimises RMS radial error in
pixels across the image.

NOTE: If MAPX/MAPY are non-identity (SuperView / HyperView), the distortion is
not purely radial and a 1-parameter radial model cannot capture it. The script
warns in that case and fits anyway using the radial part only.

Output matches Theia's 5 intrinsic slots:
    [FOCAL_LENGTH, ASPECT_RATIO, PRINCIPAL_POINT_X, PRINCIPAL_POINT_Y,
     RADIAL_DISTORTION_1]
"""

from __future__ import annotations

import argparse
import json
import math
import sys
from pathlib import Path
from typing import Any

import numpy as np

try:
    from scipy.optimize import least_squares  # type: ignore
    HAVE_SCIPY = True
except Exception:
    HAVE_SCIPY = False


def load_lens(path: Path) -> dict[str, Any]:
    data = json.loads(path.read_text(encoding="utf-8"))
    lens = data.get("lens")
    if not lens:
        raise SystemExit("No 'lens' key in JSON; run extract-metadata first.")
    return lens


def pycf_to_exponents(pycf_terms: list[str], n_coeffs: int) -> np.ndarray:
    if not pycf_terms or len(pycf_terms) != n_coeffs:
        return np.arange(1, n_coeffs + 1, dtype=np.float64)
    out = []
    for t in pycf_terms:
        t = t.strip().lower()
        if t.startswith("r") and t[1:].isdigit():
            out.append(int(t[1:]))
        else:
            out.append(len(out))
    while len(out) < n_coeffs:
        out.append(len(out))
    return np.asarray(out[:n_coeffs], dtype=np.float64)


def get_video_info(video: Path | None) -> tuple[int, int, float | None]:
    """Return (width, height, fps) from the first video stream via ffprobe."""
    if video is None:
        raise SystemExit("--video (or --width/--height) is required")
    import subprocess

    r = subprocess.run(
        [
            "ffprobe", "-v", "error",
            "-select_streams", "v:0",
            "-show_entries", "stream=width,height,r_frame_rate,avg_frame_rate",
            "-of", "default=nw=1:nk=0",
            str(video),
        ],
        capture_output=True, text=True, check=True,
    )
    w = h = None
    fps: float | None = None
    for line in r.stdout.splitlines():
        line = line.strip()
        if line.startswith("width="):
            w = int(line.split("=", 1)[1])
        elif line.startswith("height="):
            h = int(line.split("=", 1)[1])
        elif fps is None and (line.startswith("r_frame_rate=") or line.startswith("avg_frame_rate=")):
            val = line.split("=", 1)[1]
            if "/" in val:
                num, den = val.split("/")
                try:
                    if float(den) != 0.0:
                        fps = float(num) / float(den)
                except ValueError:
                    pass
            else:
                try:
                    fps = float(val)
                except ValueError:
                    pass
    if not w or not h:
        raise SystemExit(f"Could not parse ffprobe output:\n{r.stdout}")
    return w, h, fps


def resize_intrinsics(
    intr: dict[str, float],
    src_w: int,
    src_h: int,
    dst_w: int,
    dst_h: int,
) -> tuple[dict[str, float], float, float]:
    """Scale a DIVISION_UNDISTORTION intrinsics dict to a new resolution.

    Center-of-pixel convention: p_dst = (p_src + 0.5) * s - 0.5
    Radial distortion has units of 1/px^2: k_dst = k_src / s_x^2
    For anisotropic rescales, the model is no longer strictly consistent
    (division distortion is isotropic radial); aspect_ratio absorbs the
    difference for fx/fy but the distortion stays keyed to the x-scale.
    """
    sx = dst_w / float(src_w)
    sy = dst_h / float(src_h)
    fx = float(intr["focal_length"])
    a = float(intr.get("aspect_ratio", 1.0))
    cx = float(intr["principal_pt_x"])
    cy = float(intr["principal_pt_y"])
    k = float(intr["div_undist_distortion"])
    skew = float(intr.get("skew", 0.0))

    fx_new = fx * sx
    a_new = a * (sy / sx) if sx != 0 else a
    cx_new = (cx + 0.5) * sx - 0.5
    cy_new = (cy + 0.5) * sy - 0.5
    k_new = k / (sx * sx)

    return (
        {
            "aspect_ratio": a_new,
            "div_undist_distortion": k_new,
            "focal_length": fx_new,
            "principal_pt_x": cx_new,
            "principal_pt_y": cy_new,
            "skew": skew,
        },
        sx,
        sy,
    )


def theta_of_rd(
    r_d_px: np.ndarray,
    W: int,
    zmpl: float,
    coeffs: np.ndarray,
    exps: np.ndarray,
) -> np.ndarray:
    """GoPro forward: pixel radius -> world angle (radians)."""
    r_n = r_d_px / (W * 0.5)
    r = zmpl * r_n
    # horner-ish: sum c_i * r^e_i
    acc = np.zeros_like(r, dtype=np.float64)
    for c, e in zip(coeffs, exps):
        acc = acc + float(c) * np.power(r, float(e))
    return acc


def paraxial_focal_px(
    W: int,
    zmpl: float,
    coeffs: np.ndarray,
    exps: np.ndarray,
) -> float:
    """f such that r_u = f * tan(theta) has unit slope at r_d=0.

    d(theta)/d(r_d) at 0 is the coefficient of the r^1 term times
    (zmpl / (W/2)).  So f = (W/2) / (a1 * zmpl).
    """
    a1 = 0.0
    for c, e in zip(coeffs, exps):
        if int(e) == 1:
            a1 += float(c)
    if a1 <= 0.0:
        raise SystemExit("POLY has no positive linear (r^1) term; cannot fit.")
    return (W * 0.5) / (a1 * zmpl)


def fit_k_closed_form(
    r_d: np.ndarray,
    r_u: np.ndarray,
) -> float:
    """Linear LS for  r_d = r_u + k * r_u * r_d^2  ->  k from least-squares."""
    # Only use points where both are > 0
    mask = (r_d > 1e-6) & (r_u > 1e-6)
    rd = r_d[mask]
    ru = r_u[mask]
    a = ru * rd * rd
    b = rd - ru
    k = float(np.dot(a, b) / np.dot(a, a))
    return k


def joint_refine_fk(
    r_d: np.ndarray,
    r_u_goal: np.ndarray,
    f0: float,
    k0: float,
    W: int,
    zmpl: float,
    coeffs: np.ndarray,
    exps: np.ndarray,
) -> tuple[float, float]:
    """Refine (f, k) by matching division-model r_u against GoPro r_u goal.

    r_u_div(f, k; r_d) = r_d / (1 + k * r_d^2)
    r_u_gopro(f; r_d) = f * tan(theta(r_d))

    Since r_u_goal depends on f, we jointly fit both so the division prediction
    matches the GoPro rectilinear projection with the same f.
    """

    th = theta_of_rd(r_d, W, zmpl, coeffs, exps)
    # Clamp to keep tan finite even if POLY slightly overshoots
    th = np.clip(th, -math.pi * 0.49, math.pi * 0.49)
    tan_th = np.tan(th)

    def residuals(x):
        f, k = x[0], x[1]
        r_u_gopro = f * tan_th
        r_u_div = r_d / (1.0 + k * r_d * r_d)
        return r_u_div - r_u_gopro

    if HAVE_SCIPY:
        res = least_squares(residuals, x0=[f0, k0], method="lm", max_nfev=200)
        return float(res.x[0]), float(res.x[1])

    # Fallback: damped Gauss-Newton on 2 params
    f, k = float(f0), float(k0)
    for _ in range(50):
        r_u_gopro = f * tan_th
        r_u_div = r_d / (1.0 + k * r_d * r_d)
        res = r_u_div - r_u_gopro
        # partial derivatives
        dr_df = -tan_th
        dr_dk = -r_d * r_d * r_d / np.square(1.0 + k * r_d * r_d)
        J = np.stack([dr_df, dr_dk], axis=1)
        JTJ = J.T @ J + 1e-12 * np.eye(2)
        JTr = J.T @ res
        step = np.linalg.solve(JTJ, JTr)
        f -= float(step[0])
        k -= float(step[1])
        if np.linalg.norm(step) < 1e-12:
            break
    return f, k


def main() -> None:
    ap = argparse.ArgumentParser(
        description="Fit DivisionUndistortionCameraModel (f, a, cx, cy, k) from GoPro POLY/ZMPL.",
    )
    ap.add_argument("--lens", required=True, type=Path, help="extract-metadata JSON")
    ap.add_argument("--video", type=Path, help="Source video (to read width/height via ffprobe)")
    ap.add_argument("--width", type=int, default=None)
    ap.add_argument("--height", type=int, default=None)
    ap.add_argument(
        "--aspect-ratio",
        type=float,
        default=1.0,
        help="Theia ASPECT_RATIO parameter = fy / fx (default 1.0)",
    )
    ap.add_argument(
        "--samples",
        type=int,
        default=4000,
        help="Number of radial samples used for the fit.",
    )
    ap.add_argument(
        "--max-radius-frac",
        type=float,
        default=1.0,
        help="Fit up to this fraction of the image diagonal/2 (default 1.0 = full corner).",
    )
    ap.add_argument(
        "--output-json",
        type=Path,
        default=None,
        help="Optional path to write the intrinsics JSON (requested schema).",
    )
    ap.add_argument(
        "--target-width",
        type=int,
        default=None,
        help="Resize intrinsics to this width (height auto-scaled to preserve source aspect unless --target-height is given).",
    )
    ap.add_argument(
        "--target-height",
        type=int,
        default=None,
        help="Resize intrinsics to this height (width auto-scaled to preserve source aspect unless --target-width is given).",
    )
    ap.add_argument(
        "--make-undist-calib",
        type=Path,
        default=None,
        help=(
            "Also emit a matching PINHOLE camera JSON (same focal / principal point, "
            "zero distortion, same image size after resize) suitable as the target of "
            "undistort_theia.py."
        ),
    )
    ap.add_argument(
        "--undist-focal-scale",
        type=float,
        default=1.0,
        help=(
            "Multiply the DIVISION focal length by this factor when producing the "
            "--make-undist-calib PINHOLE target. <1.0 widens the undistorted FOV "
            "(keeps more of the source image; stretches corners). Default: 1.0."
        ),
    )
    ap.add_argument(
        "--undist-fov-h-deg",
        type=float,
        default=None,
        help=(
            "Alternative to --undist-focal-scale: set the horizontal FOV of the "
            "PINHOLE target in degrees. Overrides --undist-focal-scale."
        ),
    )
    args = ap.parse_args()

    lens = load_lens(args.lens)
    coeffs_list = lens.get("poly_coeffs") or []
    pycf_terms = lens.get("pycf_terms") or []
    zmpl = lens.get("zmpl")
    if not coeffs_list or zmpl is None:
        raise SystemExit(
            "Lens JSON is missing POLY/ZMPL; cannot fit. "
            "This clip likely only has VFOV/ZFOV flags (no per-lens calibration).",
        )
    coeffs = np.asarray([float(c) for c in coeffs_list], dtype=np.float64)
    exps = pycf_to_exponents(list(pycf_terms), len(coeffs))
    zmpl = float(zmpl)

    mxcf = lens.get("mxcf_terms") or []
    mapx = lens.get("mapx_coeffs") or []
    mycf = lens.get("mycf_terms") or []
    mapy = lens.get("mapy_coeffs") or []
    non_identity_map = (
        (mxcf and mapx and not (len(mapx) == 1 and abs(float(mapx[0]) - 1.0) < 1e-6)) or
        (mycf and mapy and not (len(mapy) == 1 and abs(float(mapy[0]) - 1.0) < 1e-6))
    )
    if non_identity_map:
        print(
            "Warning: MAPX/MAPY are non-identity (SuperView/HyperView). "
            "The division model only captures radial distortion; residuals will be larger.",
            file=sys.stderr,
        )

    fps: float | None = None
    if args.width and args.height:
        W, H = int(args.width), int(args.height)
    else:
        W, H, fps = get_video_info(args.video)

    cx = (W - 1) * 0.5
    cy = (H - 1) * 0.5

    diag_half = 0.5 * math.hypot(W, H)
    r_max = diag_half * float(args.max_radius_frac)
    r_d = np.linspace(1.0, r_max, int(args.samples), dtype=np.float64)

    f0 = paraxial_focal_px(W, zmpl, coeffs, exps)
    theta = theta_of_rd(r_d, W, zmpl, coeffs, exps)
    theta = np.clip(theta, -math.pi * 0.49, math.pi * 0.49)
    r_u_goal = f0 * np.tan(theta)

    k0 = fit_k_closed_form(r_d, r_u_goal)
    f_fit, k_fit = joint_refine_fk(r_d, r_u_goal, f0, k0, W, zmpl, coeffs, exps)

    theta_fit = np.clip(theta_of_rd(r_d, W, zmpl, coeffs, exps), -math.pi * 0.49, math.pi * 0.49)
    r_u_div = r_d / (1.0 + k_fit * r_d * r_d)
    r_u_ref = f_fit * np.tan(theta_fit)
    resid = r_u_div - r_u_ref
    rms = float(np.sqrt(np.mean(resid ** 2)))
    max_abs = float(np.max(np.abs(resid)))

    fov_h = 2.0 * math.degrees(math.atan2(W * 0.5, f_fit))
    fov_v = 2.0 * math.degrees(math.atan2(H * 0.5, f_fit / max(args.aspect_ratio, 1e-9)))
    fov_d = 2.0 * math.degrees(math.atan2(diag_half, f_fit))

    print("Image size           : {} x {}".format(W, H))
    print("Principal point (cx,cy): ({:.3f}, {:.3f})".format(cx, cy))
    print("Aspect ratio (fy/fx) : {:.6f}".format(args.aspect_ratio))
    print("Paraxial focal f0    : {:.4f} px".format(f0))
    print("Closed-form k        : {:.6e}".format(k0))
    print("Refined focal f      : {:.4f} px   ({:.3f} * W)".format(f_fit, f_fit / W))
    print("Refined k            : {:.6e}".format(k_fit))
    print("Equivalent pinhole FOV: H={:.2f}  V={:.2f}  D={:.2f} deg".format(fov_h, fov_v, fov_d))
    print("Fit error (radial)    : RMS = {:.3f} px   max = {:.3f} px".format(rms, max_abs))

    intrinsics = {
        "aspect_ratio": float(args.aspect_ratio),
        "div_undist_distortion": k_fit,
        "focal_length": f_fit,
        "principal_pt_x": cx,
        "principal_pt_y": cy,
        "skew": 0.0,
    }

    out_w, out_h = W, H
    resized_from: dict[str, Any] | None = None
    if args.target_width or args.target_height:
        if args.target_width and args.target_height:
            dst_w, dst_h = int(args.target_width), int(args.target_height)
        elif args.target_width:
            dst_w = int(args.target_width)
            dst_h = int(round(H * (dst_w / float(W))))
        else:
            dst_h = int(args.target_height)
            dst_w = int(round(W * (dst_h / float(H))))
        intrinsics, sx, sy = resize_intrinsics(intrinsics, W, H, dst_w, dst_h)
        out_w, out_h = dst_w, dst_h
        resized_from = {
            "source_width": W,
            "source_height": H,
            "scale_x": sx,
            "scale_y": sy,
        }
        if abs(sx - sy) / max(sx, sy) > 1e-4:
            print(
                "Warning: non-uniform rescale (sx={:.6f}, sy={:.6f}). "
                "Division model is isotropic-radial; aspect_ratio absorbs fy/fx, "
                "but the distortion term is keyed to the x-scale only.".format(sx, sy),
                file=sys.stderr,
            )

    out_json: dict[str, Any] = {
        "image_width": float(out_w),
        "image_height": float(out_h),
        "intrinsic_type": "DIVISION_UNDISTORTION",
        "intrinsics": intrinsics,
        # Provenance: not a camera calibration, but derived from the GoPro GPMF
        # lens polynomial (POLY/PYCF/ZMPL). Keeps final_reproj_error / nr_calib_images
        # out of the output because they have no meaning in this context.
        "source": "derived_from_gopro_metadata",
        "fit_stats": {
            "paraxial_focal_length_px": f0,
            "rms_radial_error_px": rms,
            "max_radial_error_px": max_abs,
            "fov_h_deg": fov_h,
            "fov_v_deg": fov_v,
            "fov_diag_deg": fov_d,
            "max_radius_frac": float(args.max_radius_frac),
        },
        # Full source polynomial kept inside the JSON so plot_distortion.py
        # can reconstruct the ground-truth forward model without the original
        # extract-metadata JSON.
        "gopro_lens": {
            "zmpl": zmpl,
            "poly_coeffs": coeffs.tolist(),
            "pycf_terms": list(pycf_terms),
            "mxcf_terms": list(mxcf),
            "mapx_coeffs": [float(x) for x in mapx],
            "mycf_terms": list(mycf),
            "mapy_coeffs": [float(x) for x in mapy],
            "source_width": W,
            "source_height": H,
        },
    }
    if fps is not None:
        out_json["fps"] = float(fps)
    if resized_from is not None:
        out_json["resized_from"] = resized_from

    if args.make_undist_calib:
        # PINHOLE target: same principal point / aspect / image size as the
        # distorted calib. The focal length of the target controls how much
        # FOV the undistorted output preserves.
        f_div = float(intrinsics["focal_length"])
        if args.undist_fov_h_deg is not None:
            fov_h = float(args.undist_fov_h_deg)
            if not (0.0 < fov_h < 180.0):
                raise SystemExit("--undist-fov-h-deg must be in (0, 180)")
            f_undist = (out_w * 0.5) / math.tan(math.radians(fov_h) * 0.5)
            focal_scale = f_undist / f_div
        else:
            focal_scale = float(args.undist_focal_scale)
            if focal_scale <= 0.0:
                raise SystemExit("--undist-focal-scale must be > 0")
            f_undist = f_div * focal_scale
            fov_h = 2.0 * math.degrees(math.atan2(out_w * 0.5, f_undist))

        fov_v = 2.0 * math.degrees(math.atan2(out_h * 0.5, f_undist / max(float(intrinsics.get("aspect_ratio", 1.0)), 1e-9)))
        fov_d = 2.0 * math.degrees(math.atan2(0.5 * math.hypot(out_w, out_h), f_undist))

        undist_json = {
            "image_width": float(out_w),
            "image_height": float(out_h),
            "intrinsic_type": "PINHOLE",
            "intrinsics": {
                "aspect_ratio": intrinsics["aspect_ratio"],
                "focal_length": f_undist,
                "principal_pt_x": intrinsics["principal_pt_x"],
                "principal_pt_y": intrinsics["principal_pt_y"],
                "skew": 0.0,
            },
            "source": "derived_from_division_fit",
            "undist_focal_scale": focal_scale,
            "undist_fov_deg": {"h": fov_h, "v": fov_v, "d": fov_d},
        }
        if fps is not None:
            undist_json["fps"] = float(fps)
        args.make_undist_calib.write_text(json.dumps(undist_json, indent=2), encoding="utf-8")
        print(
            "Wrote undistorted PINHOLE calib: {}  (focal={:.2f} px, scale={:.3f}, hFOV={:.2f} deg)".format(
                args.make_undist_calib, f_undist, focal_scale, fov_h,
            ),
        )

    print("\nIntrinsics ({}x{}):".format(out_w, out_h))
    for k_name, v in intrinsics.items():
        print("  {:25s} = {}".format(k_name, v))

    if args.output_json:
        args.output_json.write_text(json.dumps(out_json, indent=2), encoding="utf-8")
        print("\nWrote {}".format(args.output_json))


if __name__ == "__main__":
    main()
