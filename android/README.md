# Telemetry Parser Android Library

Android library for extracting telemetry metadata (GPS, accelerometer, gyroscope) from action camera video files.

**Supported cameras:** GoPro, DJI, Insta360

## Requirements

- Rust (stable) + `cargo-ndk`: `cargo install cargo-ndk`
- Android NDK (via Android Studio SDK Manager or `ANDROID_NDK_HOME`)
- Android SDK

## Building

From the telemetry-parser root:

```bash
# Option 1: Use the build script (recommended)
./build-android.sh              # release → telemetry-android-release.aar
./build-android.sh debug        # debug   → telemetry-android-debug.aar

# Option 2: Manual steps
cargo ndk -o android/src/main/jniLibs -t arm64-v8a -t armeabi-v7a -t x86_64 -t x86 -p telemetry-parser --release
cd android && ./gradlew assembleRelease   # or: ./gradlew assembleDebug
```

Artifacts: `android/build/outputs/aar/telemetry-android-release.aar` and `telemetry-android-debug.aar`.

## CI builds (no local Android SDK required)

GitHub Actions builds the release AAR on every push/PR to `master` or `main` that touches Rust or Android packaging files. You can also run it manually from **Actions → Android AAR → Run workflow** (optional `abis` input for multi-ABI builds).

After a successful run, download **telemetry-android-release.aar** from the workflow run’s **Artifacts** section.

- Regular CI / PR builds: **arm64-v8a** only (faster).
- Version tags (`v*`): all common ABIs (`arm64-v8a`, `armeabi-v7a`, `x86_64`, `x86`) and the AAR is attached to the GitHub Release.

## Testing

A runnable sample app lives at `../sample-app` in this repo:

```bash
./gradlew :sample-app:installDebug
```

- Prefer a **physical arm64 device**: the library’s default `rustAbis` is **`arm64-v8a` only** (`android/build.gradle.kts`), so an **x86/x86_64 emulator** will not load `libtelemetry_parser.so` unless you build native code for those ABIs, e.g.  
  `./gradlew :sample-app:installDebug -PrustAbis=arm64-v8a,x86_64`
- The native API needs a **filesystem path**, not a `content://` URI—copy the chosen file to cache or similar before `open(path)` (see `TelemetrySampleActivity`).

## Usage

```kotlin
implementation(files("path/to/telemetry-android-release.aar"))
// Or use the local project:
implementation(project(":android"))
```

```kotlin
TelemetryParser().use { parser ->
    parser.open("/path/to/gopro.mp4")
    println("Camera: ${parser.getCameraType()}")  // "GoPro", "DJI", or "Insta360"
    parser.getGpsData().forEach { pt ->
        println("${pt.latitude}, ${pt.longitude}")
    }
    parser.getAccelerometerData()
    parser.getGyroscopeData()
}
```
