# Telemetry Parser iOS Library

Swift Package wrapping the Rust telemetry-parser core for iOS (and macOS for local development).

**Supported cameras:** GoPro, DJI, Insta360

## Requirements

- macOS with **Xcode** and Command Line Tools
- [Rust / rustup](https://rustup.rs/)
- iOS Rust targets (installed automatically by `build-ios.sh`):
  - `aarch64-apple-ios` (device)
  - `aarch64-apple-ios-sim` (Apple Silicon simulator)
  - optional: `x86_64-apple-ios` (Intel simulator, set `INCLUDE_X86_SIM=1`)

## Building the XCFramework

From the telemetry-parser repository root:

```bash
./build-ios.sh              # release → ios/build/TelemetryParser.xcframework
./build-ios.sh debug        # dev/unoptimized build
CARGO_PROFILE=ios-dev ./build-ios.sh   # fast iteration profile from Cargo.toml
INCLUDE_X86_SIM=1 ./build-ios.sh       # include Intel simulator slice
```

The Swift Package at `ios/` expects the XCFramework at `ios/build/TelemetryParser.xcframework`. Build it locally on macOS, or download a prebuilt artifact from GitHub Actions (see below).

## CI builds (no local Mac required)

GitHub Actions builds the XCFramework on every push/PR to `master` or `main` that touches Rust or iOS packaging files. You can also run it manually from **Actions → iOS XCFramework → Run workflow**.

After a successful run, download **TelemetryParser.xcframework** from the workflow run’s **Artifacts** section, then place it at:

```
ios/build/TelemetryParser.xcframework
```

Version tags (`v*`) additionally attach a zip to the GitHub Release.

## Swift Package Manager

Add a local or Git dependency:

```swift
dependencies: [
    .package(path: "../telemetry-parser/ios"),
]
```

```swift
import TelemetryParser

let parser = TelemetryParser()
defer { parser.close() }

try parser.open(path: "/path/to/video.mp4")
print(try parser.getCameraType())
print(try parser.getGpsData().count)
```

Or use the convenience helper:

```swift
try withTelemetryParser(path: videoPath) { parser in
    let gps = try parser.getGpsData()
}
```

## API

The Swift API mirrors the Android Kotlin library:

| Method | Description |
|--------|-------------|
| `open(path:)` | Open a video file (filesystem path required) |
| `close()` | Release native resources |
| `getCameraType()` | Camera family string |
| `getDuration()` | Clip duration in seconds |
| `getVideoInfo()` | Frame count and FPS |
| `getLensMetadataJson()` | DVID/FOVL lens JSON when present |
| `getGpsData()` | GPS samples |
| `getAccelerometerData()` | Accelerometer samples (m/s²) |
| `getGyroscopeData()` | Gyroscope samples (rad/s) |
| `getGravityVectorData()` | Gravity vector samples |
| `getCameraOrientationData()` | CORI quaternions |
| `getImageOrientationData()` | IORI quaternions |
| `getOrientationCombinedData()` | CORI × IORI product |

Sensor timestamps are in **nanoseconds** from clip start.

## iOS sandbox / file access

The native layer requires a **real filesystem path**, not a Photos asset URL or security-scoped bookmark alone.

Before calling `open(path:)`:

1. Copy the picked file to `FileManager.default.temporaryDirectory` (or app documents).
2. Pass the resulting path to `open(path:)`.

This matches the Android constraint documented in [`android/README.md`](../android/README.md).

## Sample app

See [`../ios-sample/`](../ios-sample/) for a command-line macOS sample and SwiftUI source you can drop into an Xcode iOS app.

## C API

Low-level C bindings are declared in [`include/telemetry_parser.h`](include/telemetry_parser.h). Swift callers should prefer the `TelemetryParser` class.

Strings returned by the C API must be freed with `telemetry_parser_free_string`.
