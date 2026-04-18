# Scripts

## Dump all GPMF metadata (including lens FourCCs)

From the repo root, using `gyro2bb`:

```bash
cargo build -p gyro2bb --release
./target/release/gyro2bb /path/to/clip.MP4 --dump | tee metadata_dump.txt
```

Filter for lens-related lines:

```bash
./target/release/gyro2bb /path/to/clip.MP4 --dump \
  | grep -E 'FOVL|ZFOV|VFOV|PYCF| POLY |ZMPL|ARUW|ARWA|MXCF|MAPX|MYCF|MAPY|ABSC|DVID'
```

## JSON export (GPS, IMU, **lens** block)

Workspace build (recommended):

```bash
cargo build -p extract-metadata --release
./target/release/extract-metadata /path/to/clip.MP4 -o /path/to/clip_metadata.json
```

Or use [extract-and-compare.sh](extract-and-compare.sh) (telemetry-parser JSON + instructions for gpmf_android).

The `lens` object includes `vfov_mode` (L/W/S/H/M/N), `zfov_deg`, `dvid_primary`, and when the camera embeds them: `pycf_terms`, `poly_coeffs`, `mxcf_terms`, `mapx_coeffs`, `mycf_terms`, `mapy_coeffs`, `zmpl`, `aruw`, `arwa`, `absc`.

### VFOV modes (GoPro)

- **L** – Linear (in-camera rectilinear); undistort script uses pass-through.
- **W** – Wide (radial fisheye; fully supported below).
- **S** – **SuperView** (anamorphic 4:3 → 16:9 warp, **NOT supported**).
- **H** – **HyperView** (anamorphic 8:7 → 16:9 warp, **NOT supported**).
- **M** / **N** – other crops.

Some clips only store **ZFOV** + **VFOV** without **POLY**; full calibration is model / firmware dependent.

> **SuperView / HyperView are NOT supported by any of the undistortion scripts in this folder.**
> Those modes record with non-identity `MAPX` / `MAPY` (non-radial anamorphic warp). A 1-parameter radial model — and in fact any purely radial model — cannot reverse that warp. The scripts print a warning when they see non-identity MAPX/MAPY and, in the case of `undistort_theia.py`, exit with a non-zero status. For metadata-driven undistortion, record your footage in **Wide** (or **Linear** if you want pass-through).

## Undistort video (OpenCV + GPMF metadata)

Dependencies:

```bash
pip install numpy opencv-python
```

```bash
./scripts/undistort.py \
  --video /path/to/clip.MP4 \
  --lens  /path/to/clip_metadata.json \
  --out   /path/to/clip_undistorted.mp4 \
  [--output-fov-deg 90] \
  [--flip180]
```

- **Linear (L)** frames are copied unchanged (already undistorted in-camera).
- Other modes: if **POLY** is present, builds a remap from metadata; otherwise prints a warning and copies frames.
- **SuperView / HyperView** prints a warning and proceeds on radial-only (output will **not** be geometrically correct).
- `--flip180` rotates every written frame 180 degrees (use when the camera was mounted upside-down).

Output is **video only** (`mp4v`); audio is not copied.

## Fit the Theia DivisionUndistortionCameraModel from GoPro metadata

The GoPro **POLY / PYCF / ZMPL** polynomial is converted to Theia's single-parameter
[DivisionUndistortionCameraModel](https://github.com/sweeneychris/TheiaSfM) (5 intrinsics:
`focal_length`, `aspect_ratio`, `principal_pt_x`, `principal_pt_y`, `div_undist_distortion`).

```bash
# Requires numpy (and optionally scipy for a better nonlinear refine)
python3 scripts/fit_division_model.py \
  --lens  /path/to/clip_metadata.json \
  --video /path/to/clip.mp4 \
  --max-radius-frac 0.7 \
  --output-json     /path/to/clip_division_intrinsics.json \
  --make-undist-calib /path/to/clip_undist_pinhole.json
```

- `--max-radius-frac` controls the fit domain (default `1.0` = full diagonal). For a
  138° fisheye (`nina2.mp4`) the full-diagonal fit has RMS ≈ 58 px while restricting
  to 70 % of the diagonal drops RMS to ≈ 1.8 px over the entire short edge.
- `--make-undist-calib` emits a matching **PINHOLE** JSON (same principal point,
  zero distortion, same image size) that can be used directly as `--calib-undist`
  in `undistort_theia.py`.
- **Zooming the undistorted FOV out** (preserving more of the source):
  use `--undist-focal-scale S` on `--make-undist-calib`. `S < 1.0` **lowers** the
  pinhole focal, which **widens** the output FOV (`hFOV = 2·atan((W/2)/f)`). Or
  set the target explicitly with `--undist-fov-h-deg 120` etc. Defaults to `1.0`
  (same focal as the division model, i.e. the current paraxial match; for
  `nina2` that is hFOV ≈ 105°). Setting `--undist-focal-scale 0.8` gives
  hFOV ≈ 120°; `0.65` gives ≈ 137° (close to the lens's full horizontal sweep).
  Keep in mind: the further you push, the more corner stretching appears and
  the more of the output ends up as black where the source ray cone does not
  reach. The ZFOV of the lens is a hard upper bound.
- `--target-width N` (and/or `--target-height N`) rescales the intrinsics to a new
  resolution (correct scaling: `f ∝ s`, `cx,cy ∝ s`, `k ∝ 1/s²`).
- Fields like `final_reproj_error` / `nr_calib_images` from image-based calibrations
  are intentionally omitted; instead a `fit_stats` block records RMS radial error
  vs. the GoPro polynomial, and `source` is set to `derived_from_gopro_metadata`.

## Plot the distortion curves

```bash
pip install matplotlib

python3 scripts/plot_distortion.py \
  --fit /path/to/clip_division_intrinsics.json \
  [--lens /path/to/clip_metadata.json]   # only needed for pre-1.0 fits without gopro_lens
```

Three PNGs next to the fit JSON:
1. `..._theta_vs_r.png` – the GoPro POLY forward model `θ(r_n)`.
2. `..._r_u_vs_r_d.png` – undistorted-vs-distorted radius: GoPro (truth) vs division fit.
3. `..._residuals.png` – signed residual `r_u_div − r_u_gopro` in source pixels.

Add `--combined` for a single 3-panel figure.

## Undistort video via pyTheiaSfM

Uses the same `DivisionUndistortionCameraModel` your consumer backend uses. Builds
a remap LUT through the Theia intrinsics (`ImageToCameraCoordinates` /
`CameraToImageCoordinates`), so the exact same math is applied at undistort time
as in your downstream reconstruction pipeline.

```bash
pip install pytheia opencv-python numpy

python3 scripts/undistort_theia.py \
  --input  /path/to/clip.mp4 \
  --output /path/to/clip_undistorted.mp4 \
  --calib-dist  /path/to/clip_division_intrinsics.json \
  [--calib-undist /path/to/clip_undist_pinhole.json] \
  [--scale 0.5] \
  [--flip180] \
  [--dump-frames-dir /path/to/undist_frames]
```

- `--calib-dist` is the DIVISION_UNDISTORTION JSON from `fit_division_model.py`.
- `--calib-undist` is the PINHOLE target. If omitted, a matching PINHOLE is
  auto-generated (same principal point, zero distortion).
- `--undist-focal-scale S` **(only when `--calib-undist` is NOT given)** multiplies
  the division focal to derive the auto PINHOLE focal. `S < 1.0` **zooms the
  undistorted FOV out** (keeps more of the source image; default `1.0`).
- `--undist-fov-h-deg D` **(only when `--calib-undist` is NOT given)** explicit
  horizontal FOV for the auto PINHOLE target, overrides `--undist-focal-scale`.
- `--scale S` scales **both** calibrations by `S` and resizes source frames
  accordingly – the distortion parameter `k` is correctly rescaled to `k/S²`.
- `--flip180` rotates every output frame 180° (handy when the camera was mounted
  upside down).
- `--dump-frames-dir DIR` also writes per-frame JPEGs alongside the MP4.

### Limitations

- **SuperView / HyperView not supported.** The DIVISION_UNDISTORTION model is
  strictly radial; the anamorphic MAPX/MAPY warp cannot be inverted by it. If the
  source lens JSON carries non-identity MAPX/MAPY, `undistort_theia.py` refuses to
  run (exit code 2).
- The 1-parameter division model is accurate inside the inner ~70% of the image
  diagonal for GoPro-class fisheye lenses. For tighter corner accuracy, either
  crop the output FOV (`--max-radius-frac` at fit time) or move to a higher-order
  Theia model.

## Programmatic Camera loader ([theia_camera.py](theia_camera.py))

Small helper mirroring the consumer-app `Camera` class. Use it in your own
scripts / notebooks:

```python
from scripts.theia_camera import TheiaCamera

cam = TheiaCamera()
cam.load_from_file("clip_division_intrinsics.json", scale=1.0)
intrinsics = cam.get_camera().CameraIntrinsics()   # DivisionUndistortionCameraModel
ray = intrinsics.ImageToCameraCoordinates([u, v])
```

Supported `intrinsic_type`s: `DIVISION_UNDISTORTION`, `PINHOLE`, `FISHEYE`.
