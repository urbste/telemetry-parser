#!/usr/bin/env python3
"""
Plot the GoPro POLY forward model against the fitted 1-parameter division
model and visualise residuals.

Inputs:
    --fit  JSON produced by scripts/fit_division_model.py (contains intrinsics
           and, when available, the gopro_lens block with the source POLY).
    --lens Optional: original extract-metadata JSON. Used if the fit JSON does
           not embed the gopro_lens block (older fits).

Outputs three PNGs (or one multi-panel PNG with --combined):
    <out>_theta_vs_r.png    theta(r_norm) from GoPro POLY
    <out>_r_u_vs_r_d.png    r_u(r_d): GoPro (ground truth) vs division fit
    <out>_residuals.png     (r_u_div - r_u_gopro) vs r_d, in pixels
"""

from __future__ import annotations

import argparse
import json
import math
from pathlib import Path
from typing import Any

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt  # noqa: E402
import numpy as np  # noqa: E402


def load_sources(fit_path: Path, lens_path: Path | None) -> tuple[dict[str, Any], dict[str, Any]]:
    fit = json.loads(fit_path.read_text(encoding="utf-8"))
    lens = fit.get("gopro_lens")
    if not lens and lens_path is not None:
        full = json.loads(lens_path.read_text(encoding="utf-8"))
        lens = full.get("lens") or {}
        lens = dict(lens)
        lens.setdefault("source_width", int(fit["image_width"]))
        lens.setdefault("source_height", int(fit["image_height"]))
    if not lens:
        raise SystemExit(
            "No gopro_lens block found in fit JSON and no --lens was supplied. "
            "Re-run fit_division_model.py or pass --lens <extract-metadata JSON>.",
        )
    return fit, lens


def pycf_to_exps(pycf_terms: list[str], n: int) -> np.ndarray:
    if not pycf_terms or len(pycf_terms) != n:
        return np.arange(1, n + 1, dtype=np.float64)
    out = []
    for t in pycf_terms:
        t = t.strip().lower()
        if t.startswith("r") and t[1:].isdigit():
            out.append(int(t[1:]))
        else:
            out.append(len(out))
    while len(out) < n:
        out.append(len(out))
    return np.asarray(out[:n], dtype=np.float64)


def poly_theta(r_norm: np.ndarray, coeffs: np.ndarray, exps: np.ndarray) -> np.ndarray:
    acc = np.zeros_like(r_norm, dtype=np.float64)
    for c, e in zip(coeffs, exps):
        acc = acc + float(c) * np.power(r_norm, float(e))
    return acc


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--fit", required=True, type=Path, help="fit_division_model.py JSON")
    ap.add_argument("--lens", type=Path, default=None, help="extract-metadata JSON (optional fallback)")
    ap.add_argument(
        "--out-prefix",
        type=Path,
        default=None,
        help="Output prefix (default: next to --fit, using its stem).",
    )
    ap.add_argument("--combined", action="store_true", help="Render a single multi-panel PNG instead of three.")
    ap.add_argument("--samples", type=int, default=2000)
    args = ap.parse_args()

    fit, lens = load_sources(args.fit, args.lens)

    W = int(lens.get("source_width") or fit["image_width"])
    H = int(lens.get("source_height") or fit["image_height"])
    zmpl = float(lens["zmpl"])
    coeffs = np.asarray([float(c) for c in lens["poly_coeffs"]], dtype=np.float64)
    exps = pycf_to_exps(list(lens.get("pycf_terms") or []), len(coeffs))

    intr = fit["intrinsics"]
    # Map current intrinsics back to source-resolution pixel units for the
    # apples-to-apples comparison with the POLY (which is parameterised in
    # source-pixel / source-half-width).
    src_w = int(fit.get("resized_from", {}).get("source_width") or fit["image_width"])
    src_h = int(fit.get("resized_from", {}).get("source_height") or fit["image_height"])
    sx = float(fit["image_width"]) / float(src_w)
    f_src = float(intr["focal_length"]) / sx
    k_src = float(intr["div_undist_distortion"]) * (sx * sx)

    diag_half = 0.5 * math.hypot(src_w, src_h)
    max_frac = float(fit.get("fit_stats", {}).get("max_radius_frac") or 1.0)
    r_d = np.linspace(0.0, diag_half, int(args.samples), dtype=np.float64)
    r_d_fit = np.linspace(0.0, diag_half * max_frac, int(args.samples), dtype=np.float64)
    r_n = r_d / (src_w * 0.5)

    theta = poly_theta(zmpl * r_n, coeffs, exps)
    theta_c = np.clip(theta, -math.pi * 0.49, math.pi * 0.49)
    r_u_gopro = f_src * np.tan(theta_c)
    r_u_div = r_d / (1.0 + k_src * r_d * r_d)
    resid = r_u_div - r_u_gopro

    prefix = args.out_prefix
    if prefix is None:
        stem = args.fit.with_suffix("").name
        prefix = args.fit.parent / f"{stem}_plot"

    title_suffix = "  (nina2-style GoPro lens)" if "nina2" in args.fit.stem else ""
    short_edge = min(src_w, src_h) * 0.5

    if args.combined:
        fig, axes = plt.subplots(1, 3, figsize=(18, 5))
        ax_theta, ax_ru, ax_res = axes

        ax_theta.plot(r_n, np.degrees(theta), color="tab:blue")
        ax_theta.axvline(1.0, linestyle=":", color="gray", label="half-width")
        ax_theta.set_xlabel("normalized radius  r_n = r_d / (W/2)")
        ax_theta.set_ylabel("world angle θ (deg)")
        ax_theta.set_title("GoPro POLY forward model" + title_suffix)
        ax_theta.grid(True, alpha=0.3)
        ax_theta.legend()

        ax_ru.plot(r_d, r_u_gopro, label="GoPro f·tan θ", color="tab:blue")
        ax_ru.plot(r_d, r_u_div, label="Division model", color="tab:orange", linestyle="--")
        ax_ru.plot(r_d, r_d, label="identity r_u=r_d", color="gray", linestyle=":", linewidth=1)
        ax_ru.axvline(short_edge, linestyle=":", color="gray", alpha=0.6, label="short-edge")
        ax_ru.axvline(r_d_fit[-1], linestyle="-.", color="tab:green", alpha=0.6, label=f"fit r_max={max_frac:.2f}·diag")
        ax_ru.set_xlabel("distorted radius r_d (src px)")
        ax_ru.set_ylabel("undistorted radius r_u (src px)")
        ax_ru.set_title("r_u vs r_d: GoPro vs Division fit")
        ax_ru.grid(True, alpha=0.3)
        ax_ru.legend()

        ax_res.plot(r_d, resid, color="tab:red")
        ax_res.axhline(0.0, color="gray", linewidth=1)
        ax_res.axvline(short_edge, linestyle=":", color="gray", alpha=0.6, label="short-edge")
        ax_res.axvline(r_d_fit[-1], linestyle="-.", color="tab:green", alpha=0.6, label=f"fit r_max={max_frac:.2f}·diag")
        ax_res.set_xlabel("distorted radius r_d (src px)")
        ax_res.set_ylabel("residual r_u_div − r_u_gopro (px)")
        ax_res.set_title("Division model residual")
        ax_res.grid(True, alpha=0.3)
        ax_res.legend()

        fig.suptitle(f"Distortion: fit={args.fit.name}", fontsize=11)
        out_path = prefix.with_suffix(".png")
        fig.tight_layout(rect=(0, 0, 1, 0.96))
        fig.savefig(out_path, dpi=140)
        print(f"Wrote {out_path}")
        return

    # Separate files
    fig, ax = plt.subplots(figsize=(7, 4.5))
    ax.plot(r_n, np.degrees(theta), color="tab:blue")
    ax.axvline(1.0, linestyle=":", color="gray", label="half-width")
    ax.set_xlabel("normalized radius r_n = r_d / (W/2)")
    ax.set_ylabel("world angle θ (deg)")
    ax.set_title("GoPro POLY forward model" + title_suffix)
    ax.grid(True, alpha=0.3)
    ax.legend()
    p1 = prefix.parent / f"{prefix.name}_theta_vs_r.png"
    fig.tight_layout()
    fig.savefig(p1, dpi=140)
    print(f"Wrote {p1}")
    plt.close(fig)

    fig, ax = plt.subplots(figsize=(7, 4.5))
    ax.plot(r_d, r_u_gopro, label="GoPro f·tan θ", color="tab:blue")
    ax.plot(r_d, r_u_div, label="Division model", color="tab:orange", linestyle="--")
    ax.plot(r_d, r_d, label="identity r_u=r_d", color="gray", linestyle=":", linewidth=1)
    ax.axvline(short_edge, linestyle=":", color="gray", alpha=0.6, label="short-edge")
    ax.axvline(r_d_fit[-1], linestyle="-.", color="tab:green", alpha=0.6, label=f"fit r_max={max_frac:.2f}·diag")
    ax.set_xlabel("distorted radius r_d (src px)")
    ax.set_ylabel("undistorted radius r_u (src px)")
    ax.set_title("r_u vs r_d: GoPro vs Division fit")
    ax.grid(True, alpha=0.3)
    ax.legend()
    p2 = prefix.parent / f"{prefix.name}_r_u_vs_r_d.png"
    fig.tight_layout()
    fig.savefig(p2, dpi=140)
    print(f"Wrote {p2}")
    plt.close(fig)

    fig, ax = plt.subplots(figsize=(7, 4.5))
    ax.plot(r_d, resid, color="tab:red")
    ax.axhline(0.0, color="gray", linewidth=1)
    ax.axvline(short_edge, linestyle=":", color="gray", alpha=0.6, label="short-edge")
    ax.axvline(r_d_fit[-1], linestyle="-.", color="tab:green", alpha=0.6, label=f"fit r_max={max_frac:.2f}·diag")
    ax.set_xlabel("distorted radius r_d (src px)")
    ax.set_ylabel("residual r_u_div − r_u_gopro (px)")
    ax.set_title("Division model residual")
    ax.grid(True, alpha=0.3)
    ax.legend()
    p3 = prefix.parent / f"{prefix.name}_residuals.png"
    fig.tight_layout()
    fig.savefig(p3, dpi=140)
    print(f"Wrote {p3}")
    plt.close(fig)


if __name__ == "__main__":
    main()
