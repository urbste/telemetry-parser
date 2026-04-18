# extract-metadata

Extracts GoPro/action camera telemetry to JSON in gpmf_android-compatible format, plus a **`lens`** object when GPMF exposes it (ZFOV, VFOV, PYCF, POLY, MAPX/MAPY, ZMPL, ARUW/ARWA, DVID, etc.).

## Usage

From the **repository root** (Cargo workspace):

```bash
cargo build -p extract-metadata --release
./target/release/extract-metadata /path/to/video.MP4 -o /path/to/output.json
```

Example:

```bash
./target/release/extract-metadata /home/steffen/Data/GX011221_1766763892185.MP4 -o /home/steffen/Data/gopro_metadata_telemetry_parser.json
```

Without `-o`, outputs next to the input as `<stem>_metadata.json`.

See also [scripts/README.md](../../scripts/README.md) for full-metadata dump (`gyro2bb --dump`) and Python undistort.
