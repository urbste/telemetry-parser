#!/usr/bin/env python3
"""
Undistort a GoPro video using a fitted DivisionUndistortionCameraModel and
pyTheiaSfM. Emits an MP4 via OpenCV VideoWriter (video only, no audio).

Flow (output-driven remap):
  for every output (undistorted) pixel p_u:
    ray = cam_undist.ImageToCameraCoordinates(p_u)   # PINHOLE target
    p_d = cam_dist.CameraToImageCoordinates(ray)     # DIVISION source pixel
    map_x[p_u], map_y[p_u] = p_d.x, p_d.y
  cv2.remap(frame, map_x, map_y, ...)

If --calib-undist is not provided, a matching PINHOLE camera is built with
the same focal length, principal point and image size as the distorted one
(same paraxial scale, no distortion).

Flags:
  --flip180   rotate each frame 180° before writing (useful when the camera
              was mounted upside-down).

Notes:
- Only DIVISION_UNDISTORTION is supported for --calib-dist. SuperView /
  HyperView material (non-identity MAPX/MAPY in the GoPro lens) is NOT
  captured by the 1-parameter division model and cannot be undistorted
  correctly here. Tell the user.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any

# Import theia_camera (which imports pytheia) BEFORE numpy/cv2 to avoid a
# libstdc++ symbol conflict on some installations (e.g. anaconda on Linux).
sys.path.insert(0, str(Path(__file__).resolve().parent))
from theia_camera import TheiaCamera  # noqa: E402

import cv2  # noqa: E402
import numpy as np  # noqa: E402


def _pinhole_image_to_camera(
    u: np.ndarray,
    v: np.ndarray,
    f: float,
    aspect: float,
    cx: float,
    cy: float,
) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
    """Inverse of PinholeCameraModel::CameraToPixelCoordinates (no distortion)."""
    fx = f
    fy = f * aspect
    x = (u - cx) / fx
    y = (v - cy) / fy
    z = np.ones_like(x)
    return x, y, z


def _division_camera_to_image(
    x: np.ndarray,
    y: np.ndarray,
    f: float,
    aspect: float,
    cx: float,
    cy: float,
    k: float,
) -> tuple[np.ndarray, np.ndarray]:
    """Vectorised port of DivisionUndistortionCameraModel::CameraToPixelCoordinates."""
    fx = f
    fy = f * aspect
    ux = fx * x
    uy = fy * y

    r_u_sq = ux * ux + uy * uy
    # scale defaults to 1.0 (identity). The division formula is only evaluated
    # where it is numerically safe; otherwise Theia falls back to identity.
    scale = np.ones_like(r_u_sq)
    if abs(k) > 0.0:
        very_small = float(np.finfo(np.float64).eps)
        denom = 2.0 * k * r_u_sq
        inner = 1.0 - 4.0 * k * r_u_sq
        ok = (np.abs(denom) >= very_small) & (inner >= 0.0)
        inner_safe = np.where(ok, inner, 1.0)
        denom_safe = np.where(ok, denom, 1.0)
        computed = (1.0 - np.sqrt(inner_safe)) / denom_safe
        scale = np.where(ok, computed, 1.0)

    xd = ux * scale + cx
    yd = uy * scale + cy
    return xd, yd


def _intr_tuple_pinhole(cam: TheiaCamera) -> tuple[float, float, float, float]:
    ci = cam.get_camera().CameraIntrinsics()
    return (
        float(ci.FocalLength()),
        float(ci.AspectRatio()),
        float(ci.PrincipalPointX()),
        float(ci.PrincipalPointY()),
    )


def _intr_tuple_division(cam: TheiaCamera) -> tuple[float, float, float, float, float]:
    ci = cam.get_camera().CameraIntrinsics()
    return (
        float(ci.FocalLength()),
        float(ci.AspectRatio()),
        float(ci.PrincipalPointX()),
        float(ci.PrincipalPointY()),
        float(ci.RadialDistortion1()),
    )


def _verify_against_theia(
    cam_dist: TheiaCamera,
    cam_undist: TheiaCamera,
    n_checks: int = 25,
) -> float:
    """Round-trip pts_u -> ray -> pts_d with both numpy and pytheia; return max error."""
    w, h = cam_undist.width, cam_undist.height
    rng = np.random.default_rng(0)
    xs = rng.uniform(0, w - 1, n_checks)
    ys = rng.uniform(0, h - 1, n_checks)

    ci_u = cam_undist.get_camera().CameraIntrinsics()
    ci_d = cam_dist.get_camera().CameraIntrinsics()

    # numpy path
    if cam_undist.prior.camera_intrinsics_model_type != "PINHOLE":
        return float("nan")  # current script defaults to PINHOLE target
    fu, au, cxu, cyu = _intr_tuple_pinhole(cam_undist)
    fd, ad, cxd, cyd, kd = _intr_tuple_division(cam_dist)
    x, y, _ = _pinhole_image_to_camera(np.asarray(xs), np.asarray(ys), fu, au, cxu, cyu)
    xd_np, yd_np = _division_camera_to_image(x, y, fd, ad, cxd, cyd, kd)

    # pytheia path
    xd_t = np.empty_like(xs)
    yd_t = np.empty_like(ys)
    for i in range(n_checks):
        ray = np.asarray(ci_u.ImageToCameraCoordinates(np.array([xs[i], ys[i]])))
        pd = np.asarray(ci_d.CameraToImageCoordinates(ray))
        xd_t[i] = pd[0]
        yd_t[i] = pd[1]

    err = np.hypot(xd_np - xd_t, yd_np - yd_t)
    return float(err.max())


def build_maps_theia(
    cam_dist: TheiaCamera,
    cam_undist: TheiaCamera,
) -> tuple[np.ndarray, np.ndarray]:
    """Vectorised LUT build using the SAME math as Theia's intrinsics.

    Pytheia's CameraIntrinsics bindings only accept one point per call, so
    building a 4000x3000 LUT through pybind11 would take minutes-to-hours.
    Instead, we port the PINHOLE (target) + DIVISION_UNDISTORTION (source)
    maths to vectorised numpy and verify against pytheia at a grid of sample
    points so any drift would be caught immediately.
    """

    if cam_undist.prior.camera_intrinsics_model_type != "PINHOLE":
        raise SystemExit(
            f"--calib-undist must be PINHOLE (got {cam_undist.prior.camera_intrinsics_model_type!r}). "
            "Use the --make-undist-calib output of fit_division_model.py.",
        )
    if cam_dist.prior.camera_intrinsics_model_type != "DIVISION_UNDISTORTION":
        raise SystemExit(
            f"--calib-dist must be DIVISION_UNDISTORTION (got {cam_dist.prior.camera_intrinsics_model_type!r}).",
        )

    max_err = _verify_against_theia(cam_dist, cam_undist, n_checks=64)
    print(f"  numpy <-> pytheia max pixel mismatch at 64 random points: {max_err:.2e} px")

    w = cam_undist.width
    h = cam_undist.height
    fu, au, cxu, cyu = _intr_tuple_pinhole(cam_undist)
    fd, ad, cxd, cyd, kd = _intr_tuple_division(cam_dist)

    xs = np.arange(w, dtype=np.float64)
    ys = np.arange(h, dtype=np.float64)
    uu, vv = np.meshgrid(xs, ys)

    x, y, _ = _pinhole_image_to_camera(uu, vv, fu, au, cxu, cyu)
    xd, yd = _division_camera_to_image(x, y, fd, ad, cxd, cyd, kd)

    return xd.astype(np.float32), yd.astype(np.float32)


def default_undist_calib(
    dist_json: dict[str, Any],
    focal_scale: float = 1.0,
    fov_h_deg: float | None = None,
) -> dict[str, Any]:
    """Build a PINHOLE calib matching the distorted one's paraxial geometry.

    ``focal_scale`` multiplies the division focal to derive the pinhole focal
    (use <1.0 to zoom the undistorted FOV out, i.e. keep more of the source).
    ``fov_h_deg`` is an alternative: set the horizontal FOV directly.
    """
    import math as _math

    ins = dist_json["intrinsics"]
    w = float(dist_json["image_width"])
    h = float(dist_json["image_height"])
    f_div = float(ins["focal_length"])
    aspect = float(ins.get("aspect_ratio", 1.0))

    if fov_h_deg is not None:
        if not (0.0 < fov_h_deg < 180.0):
            raise SystemExit("--undist-fov-h-deg must be in (0, 180)")
        f_undist = (w * 0.5) / _math.tan(_math.radians(fov_h_deg) * 0.5)
        focal_scale = f_undist / f_div
    else:
        if focal_scale <= 0.0:
            raise SystemExit("--undist-focal-scale must be > 0")
        f_undist = f_div * focal_scale

    fov_h = 2.0 * _math.degrees(_math.atan2(w * 0.5, f_undist))
    fov_v = 2.0 * _math.degrees(_math.atan2(h * 0.5, f_undist / max(aspect, 1e-9)))

    return {
        "image_width": w,
        "image_height": h,
        "intrinsic_type": "PINHOLE",
        "intrinsics": {
            "aspect_ratio": aspect,
            "focal_length": f_undist,
            "principal_pt_x": float(ins["principal_pt_x"]),
            "principal_pt_y": float(ins["principal_pt_y"]),
            "skew": 0.0,
        },
        "fps": dist_json.get("fps"),
        "source": "derived_from_division_fit (auto)",
        "undist_focal_scale": focal_scale,
        "undist_fov_deg": {"h": fov_h, "v": fov_v},
    }


def main() -> None:
    ap = argparse.ArgumentParser(
        description="Undistort a GoPro video using pyTheiaSfM + a fitted DIVISION_UNDISTORTION calib.",
    )
    ap.add_argument("--input", required=True, type=Path, help="Input MP4/MOV")
    ap.add_argument("--output", required=True, type=Path, help="Output MP4")
    ap.add_argument(
        "--calib-dist",
        "--calib_dist",
        dest="calib_dist",
        required=True,
        type=Path,
        help="DIVISION_UNDISTORTION calibration JSON (from fit_division_model.py)",
    )
    ap.add_argument(
        "--calib-undist",
        "--calib_undist",
        dest="calib_undist",
        type=Path,
        default=None,
        help="PINHOLE target calibration JSON (default: auto-derived from --calib-dist).",
    )
    ap.add_argument(
        "--undist-focal-scale",
        type=float,
        default=1.0,
        help=(
            "Only used with the auto PINHOLE target. Multiply the DIVISION focal "
            "length by this factor; <1.0 widens the undistorted FOV (keeps more of "
            "the source image, at the cost of more corner stretching). Default: 1.0."
        ),
    )
    ap.add_argument(
        "--undist-fov-h-deg",
        type=float,
        default=None,
        help=(
            "Alternative to --undist-focal-scale for the auto PINHOLE target: set "
            "the horizontal FOV of the undistorted output in degrees."
        ),
    )
    ap.add_argument(
        "--scale",
        type=float,
        default=1.0,
        help="Rescale both calibs (and resize frames) by this factor before remapping.",
    )
    ap.add_argument(
        "--flip180",
        action="store_true",
        help="Rotate the output frame 180 degrees (use when camera was upside down).",
    )
    ap.add_argument(
        "--dump-frames-dir",
        type=Path,
        default=None,
        help="Optional directory to also dump per-frame JPEGs.",
    )
    args = ap.parse_args()

    dist_json = json.loads(args.calib_dist.read_text(encoding="utf-8"))
    if dist_json.get("intrinsic_type") != "DIVISION_UNDISTORTION":
        print(
            f"Warning: --calib-dist has intrinsic_type={dist_json.get('intrinsic_type')!r}; "
            "this script is designed for DIVISION_UNDISTORTION source calibrations.",
            file=sys.stderr,
        )

    # Warn on non-radial GoPro modes if the fit JSON retained the source lens.
    lens = dist_json.get("gopro_lens") or {}
    mxcf = lens.get("mxcf_terms") or []
    mycf = lens.get("mycf_terms") or []
    mapx = lens.get("mapx_coeffs") or []
    mapy = lens.get("mapy_coeffs") or []
    non_identity_map = (
        (mxcf and mapx and not (len(mapx) == 1 and abs(float(mapx[0]) - 1.0) < 1e-6)) or
        (mycf and mapy and not (len(mapy) == 1 and abs(float(mapy[0]) - 1.0) < 1e-6))
    )
    if non_identity_map:
        print(
            "ERROR: Source lens uses non-identity MAPX/MAPY (SuperView/HyperView).\n"
            "The DIVISION_UNDISTORTION model is purely radial and cannot undo that warp.\n"
            "Record the clip in Wide (or Linear) for metadata-driven undistortion, or use\n"
            "a per-lens calibration.",
            file=sys.stderr,
        )
        sys.exit(2)

    cam_dist = TheiaCamera()
    cam_dist.load_from_dict(dist_json, scale=float(args.scale))

    if args.calib_undist is not None:
        if args.undist_focal_scale != 1.0 or args.undist_fov_h_deg is not None:
            print(
                "Warning: --undist-focal-scale / --undist-fov-h-deg are ignored because "
                "--calib-undist was supplied.",
                file=sys.stderr,
            )
        cam_undist = TheiaCamera()
        cam_undist.load_from_file(args.calib_undist, scale=float(args.scale))
        auto_undist_dict: dict[str, Any] | None = None
    else:
        auto_undist_dict = default_undist_calib(
            dist_json,
            focal_scale=float(args.undist_focal_scale),
            fov_h_deg=args.undist_fov_h_deg,
        )
        cam_undist = TheiaCamera()
        cam_undist.load_from_dict(auto_undist_dict, scale=float(args.scale))

    print(f"Distorted calib  : {args.calib_dist.name}  ({cam_dist.width}x{cam_dist.height})")
    if args.calib_undist is not None:
        print(f"Undistorted calib: {args.calib_undist.name}  ({cam_undist.width}x{cam_undist.height})")
    else:
        assert auto_undist_dict is not None
        fov = auto_undist_dict["undist_fov_deg"]
        print(
            "Undistorted calib: <auto PINHOLE>  ({}x{}, focal={:.2f} px, scale={:.3f}, hFOV={:.2f} deg, vFOV={:.2f} deg)".format(
                cam_undist.width,
                cam_undist.height,
                auto_undist_dict["intrinsics"]["focal_length"],
                auto_undist_dict["undist_focal_scale"],
                fov["h"],
                fov["v"],
            ),
        )
    print(f"Scale            : {args.scale}")
    print(f"Flip 180°        : {bool(args.flip180)}")

    cap = cv2.VideoCapture(str(args.input))
    if not cap.isOpened():
        raise SystemExit(f"Cannot open input: {args.input}")

    src_w = int(cap.get(cv2.CAP_PROP_FRAME_WIDTH))
    src_h = int(cap.get(cv2.CAP_PROP_FRAME_HEIGHT))
    fps = cap.get(cv2.CAP_PROP_FPS) or float(dist_json.get("fps") or 30.0)

    dw, dh = cam_dist.width, cam_dist.height
    uw, uh = cam_undist.width, cam_undist.height
    if (src_w, src_h) != (dw, dh):
        print(f"Source {src_w}x{src_h} != calib-dist {dw}x{dh}; frames will be resized.")

    print("Building remap tables via Theia intrinsics...")
    map_x, map_y = build_maps_theia(cam_dist, cam_undist)

    dump_dir = args.dump_frames_dir
    if dump_dir is not None:
        dump_dir.mkdir(parents=True, exist_ok=True)
        print(f"Dumping per-frame JPEGs to {dump_dir}")

    fourcc = cv2.VideoWriter_fourcc(*"mp4v")
    writer = cv2.VideoWriter(str(args.output), fourcc, fps, (uw, uh))
    if not writer.isOpened():
        raise SystemExit(f"Cannot open output: {args.output}")

    n = 0
    while True:
        ok, frame = cap.read()
        if not ok:
            break
        if (frame.shape[1], frame.shape[0]) != (dw, dh):
            frame = cv2.resize(frame, (dw, dh))
        out = cv2.remap(
            frame,
            map_x,
            map_y,
            interpolation=cv2.INTER_LINEAR,
            borderMode=cv2.BORDER_CONSTANT,
        )
        if args.flip180:
            out = cv2.flip(out, -1)
        writer.write(out)
        if dump_dir is not None:
            cv2.imwrite(str(dump_dir / f"frame_{n:05d}.jpg"), out)
        n += 1
        if n % 200 == 0:
            print(f"  frames: {n}", flush=True)

    cap.release()
    writer.release()
    print(f"Wrote {n} frames to {args.output}")


if __name__ == "__main__":
    main()
