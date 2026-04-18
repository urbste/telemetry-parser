#!/usr/bin/env python3
"""
Undistort GoPro video using lens metadata from extract-metadata JSON (lens block).

- VFOV mode L (Linear): pass-through (camera already outputs rectilinear).
- Other modes: build inverse radial + MAPX/MAPY remap (when POLY/ZMPL/MAP* are present).

Requires: pip install numpy opencv-python

Note: Clips that only expose ZFOV/VFOV without POLY cannot be modelled accurately here;
      the script logs a warning and copies frames unchanged.
"""

from __future__ import annotations

import argparse
import json
import math
import sys
from pathlib import Path
from typing import Any

import cv2
import numpy as np


def load_lens(path: Path) -> dict[str, Any] | None:
    data = json.loads(path.read_text(encoding="utf-8"))
    lens = data.get("lens")
    if not lens:
        print("No 'lens' key in JSON; run extract-metadata on this clip first.", file=sys.stderr)
        return None
    return lens


def pycf_to_exponents(pycf_terms: list[str], n_coeffs: int) -> list[int]:
    """Map GPMF PYCF tokens like r0,r1,... to polynomial exponents on r."""
    if not pycf_terms or len(pycf_terms) != n_coeffs:
        # Default: no constant term, powers 1..n
        return list(range(1, n_coeffs + 1))
    exps: list[int] = []
    for t in pycf_terms:
        t = t.strip().lower()
        if t.startswith("r") and len(t) > 1 and t[1:].isdigit():
            exps.append(int(t[1:]))
        else:
            exps.append(len(exps))
    while len(exps) < n_coeffs:
        exps.append(len(exps))
    return exps[:n_coeffs]


def poly_eval(r: np.ndarray, coeffs: list[float], exponents: list[int]) -> np.ndarray:
    out = np.zeros_like(r, dtype=np.float64)
    for c, e in zip(coeffs, exponents):
        out = out + float(c) * np.power(r, e)
    return out


def invert_poly_theta(
    theta: np.ndarray,
    coeffs: list[float],
    exponents: list[int],
    r_max: float = 1.5,
    lut_size: int = 8192,
) -> np.ndarray:
    """theta(r) forward; invert via monotonic LUT + linear interp."""
    r_lut = np.linspace(0.0, r_max, lut_size, dtype=np.float64)
    th_lut = poly_eval(r_lut, coeffs, exponents)
    # enforce monotonic for interp: if not monotonic, sort by theta
    if np.any(np.diff(th_lut) <= 0):
        order = np.argsort(th_lut)
        th_lut = th_lut[order]
        r_lut = r_lut[order]
    th_flat = theta.reshape(-1)
    r_flat = np.interp(
        np.clip(th_flat, th_lut[0], th_lut[-1]),
        th_lut,
        r_lut,
    )
    return r_flat.reshape(theta.shape)


def parse_map_term(term: str) -> tuple[int, int]:
    """Return (x_exponent, y_exponent) for tokens like x1, y3, y1x2."""
    import re

    xe, ye = 0, 0
    for m in re.finditer(r"([xy])(\d+)", term.lower()):
        var, num = m.group(1), int(m.group(2))
        if var == "x":
            xe += num
        else:
            ye += num
    return xe, ye


def eval_map(terms: list[str], coeffs: list[float], x: np.ndarray, y: np.ndarray) -> np.ndarray:
    if not terms or not coeffs:
        raise ValueError("eval_map requires non-empty terms and coeffs")
    acc = np.zeros_like(x, dtype=np.float64)
    for term, c in zip(terms, coeffs):
        xe, ye = parse_map_term(term)
        acc = acc + float(c) * np.power(x, xe) * np.power(y, ye)
    return acc


def build_remap_maps(
    h: int,
    w: int,
    lens: dict[str, Any],
    output_fov_deg: float,
) -> tuple[np.ndarray, np.ndarray]:
    zfov = float(lens.get("zfov_deg") or output_fov_deg)
    zmpl = float(lens.get("zmpl") or 1.0)
    coeffs = [float(x) for x in lens.get("poly_coeffs") or []]
    pycf = list(lens.get("pycf_terms") or [])
    mxcf = list(lens.get("mxcf_terms") or [])
    mapx_c = [float(x) for x in lens.get("mapx_coeffs") or []]
    mycf = list(lens.get("mycf_terms") or [])
    mapy_c = [float(x) for x in lens.get("mapy_coeffs") or []]
    aruw = lens.get("aruw")
    arwa = lens.get("arwa")
    aruw_f = float(aruw) if aruw is not None else None
    arwa_f = float(arwa) if arwa is not None else None

    if not coeffs:
        raise ValueError("No POLY coefficients; cannot build lens remap.")

    exps = pycf_to_exponents(pycf, len(coeffs))

    cx = (w - 1) * 0.5
    cy = (h - 1) * 0.5
    # Focal length from horizontal FOV (output_fov_deg treated as horizontal)
    fov_rad = math.radians(output_fov_deg)
    fx = (w * 0.5) / math.tan(fov_rad * 0.5)
    fy = fx  # assume square pixels

    yy, xx = np.mgrid[0:h, 0:w].astype(np.float64)
    u = (xx - cx) / fx
    v = (yy - cy) / fy
    phi = np.arctan2(v, u)
    theta = np.arctan(np.sqrt(u * u + v * v))

    r_norm = invert_poly_theta(theta, coeffs, exps)
    r_img = r_norm / zmpl
    x_u = r_img * np.cos(phi)
    y_u = r_img * np.sin(phi)

    if aruw_f and arwa_f and arwa_f > 0:
        scale = math.sqrt(aruw_f / arwa_f)
        x_u = x_u * scale

    # MAPX/MAPY: forward from (x_u, y_u) normalized to warped plane
    if mxcf and mapx_c and len(mxcf) == len(mapx_c) and mycf and mapy_c and len(mycf) == len(mapy_c):
        xd = eval_map(mxcf, mapx_c, x_u, y_u)
        yd = eval_map(mycf, mapy_c, x_u, y_u)
    else:
        xd, yd = x_u, y_u

    src_x = xd * (w * 0.5) + cx
    src_y = yd * (h * 0.5) + cy

    map_x = src_x.astype(np.float32)
    map_y = src_y.astype(np.float32)
    return map_x, map_y


def main() -> None:
    ap = argparse.ArgumentParser(description="Undistort GoPro video using extract-metadata lens JSON.")
    ap.add_argument("--video", required=True, type=Path, help="Input MP4/MOV")
    ap.add_argument("--lens", required=True, type=Path, help="JSON from extract-metadata (with lens)")
    ap.add_argument("--out", required=True, type=Path, help="Output MP4 (video only, mp4v)")
    ap.add_argument(
        "--output-fov-deg",
        type=float,
        default=None,
        help="Horizontal rectilinear output FOV in degrees (default: 0.75 * ZFOV from metadata)",
    )
    ap.add_argument(
        "--flip180",
        action="store_true",
        help="Rotate each written frame 180 degrees (useful when the camera was mounted upside down).",
    )
    args = ap.parse_args()

    lens = load_lens(args.lens)
    if lens is None:
        sys.exit(1)

    mode = (lens.get("vfov_mode") or "").strip().upper()
    zfov = lens.get("zfov_deg")
    print(f"Lens: VFOV={mode or '?'} ZFOV={zfov} DVID={lens.get('dvid_primary')}")

    # SuperView (S) and HyperView (H) use non-radial anamorphic warps
    # (non-identity MAPX/MAPY). The remap built here is purely radial and
    # will not reverse that warp correctly.
    mxcf = lens.get("mxcf_terms") or []
    mapx = lens.get("mapx_coeffs") or []
    mycf = lens.get("mycf_terms") or []
    mapy = lens.get("mapy_coeffs") or []
    non_identity_map = (
        (mxcf and mapx and not (len(mapx) == 1 and abs(float(mapx[0]) - 1.0) < 1e-6)) or
        (mycf and mapy and not (len(mapy) == 1 and abs(float(mapy[0]) - 1.0) < 1e-6))
    )
    if mode in ("S", "H") or non_identity_map:
        print(
            "Warning: SuperView/HyperView (non-radial anamorphic warp) is NOT supported\n"
            "         by this script. Output will not be geometrically correct.\n"
            "         Record in Wide (W) or Linear (L) for metadata-based undistortion.",
            file=sys.stderr,
        )

    cap = cv2.VideoCapture(str(args.video))
    if not cap.isOpened():
        print(f"Cannot open video: {args.video}", file=sys.stderr)
        sys.exit(1)

    w = int(cap.get(cv2.CAP_PROP_FRAME_WIDTH))
    h = int(cap.get(cv2.CAP_PROP_FRAME_HEIGHT))
    fps = cap.get(cv2.CAP_PROP_FPS) or 30.0
    fourcc = cv2.VideoWriter_fourcc(*"mp4v")
    writer = cv2.VideoWriter(str(args.out), fourcc, fps, (w, h))
    if not writer.isOpened():
        print(f"Cannot open writer: {args.out}", file=sys.stderr)
        sys.exit(1)

    if mode == "L":
        print("Linear (L) mode: pass-through (no extra undistort).")
        while True:
            ok, frame = cap.read()
            if not ok:
                break
            if args.flip180:
                frame = cv2.flip(frame, -1)
            writer.write(frame)
        cap.release()
        writer.release()
        print(f"Wrote {args.out}")
        return

    out_fov = args.output_fov_deg
    if out_fov is None:
        z = float(zfov) if zfov is not None else 120.0
        out_fov = min(z * 0.75, 125.0)

    poly = lens.get("poly_coeffs") or []
    if not poly:
        print(
            "Warning: no POLY in metadata; cannot compute remap. Copying frames unchanged.",
            file=sys.stderr,
        )
        while True:
            ok, frame = cap.read()
            if not ok:
                break
            if args.flip180:
                frame = cv2.flip(frame, -1)
            writer.write(frame)
        cap.release()
        writer.release()
        print(f"Wrote {args.out}")
        return

    print(f"Building remap (output FOV ~{out_fov:.2f} deg)...")
    map_x, map_y = build_remap_maps(h, w, lens, out_fov)

    n = 0
    while True:
        ok, frame = cap.read()
        if not ok:
            break
        out = cv2.remap(frame, map_x, map_y, interpolation=cv2.INTER_LINEAR, borderMode=cv2.BORDER_CONSTANT)
        if args.flip180:
            out = cv2.flip(out, -1)
        writer.write(out)
        n += 1
        if n % 200 == 0:
            print(f"  frames: {n}", flush=True)

    cap.release()
    writer.release()
    print(f"Wrote {n} frames to {args.out}")


if __name__ == "__main__":
    main()
