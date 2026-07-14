# Telemetry Parser iOS Sample

Examples for using the [`ios/`](../ios/) Swift Package.

## macOS CLI (quick validation)

Build the XCFramework first (requires macOS):

```bash
cd ..
./build-ios.sh
```

Then run the sample CLI against a video file:

```bash
cd ios-sample
swift run TelemetrySampleCLI /path/to/gopro.mp4
```

## iOS app integration

1. Build the XCFramework: `./build-ios.sh` from the repo root.
2. Create a new iOS app in Xcode (or add to an existing project).
3. Add a local Swift Package dependency on `telemetry-parser/ios`.
4. Copy [`Sources/App/ContentView.swift`](Sources/App/ContentView.swift) as a starting point.
5. When the user picks a video, **copy it to a temp file** and pass the filesystem path to `TelemetryParser.open(path:)`.

The native parser cannot read Photos asset URLs or security-scoped URLs directly.

## Expected output (CLI)

```
Camera: GoPro
Duration: 12.34s
Video: 370 frames @ 29.97 fps
GPS: 120  Accel: 2400  Gyro: 2400  Gravity: 0
CORI: 800  IORI: 800
```
