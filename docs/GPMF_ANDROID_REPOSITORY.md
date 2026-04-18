# GPMF Android Repository Documentation

**Repository location:** `/home/steffen/projects/gpmf_android`  
**Source:** [github.com/steffen/gpmf-android](https://github.com/steffen/gpmf-android)

---

## 1. Directory Structure and Main Components

```
gpmf_android/
├── app/                          # Sample/demo Android application
│   ├── build.gradle.kts
│   └── src/main/
│       ├── kotlin/.../MainActivity.kt
│       └── res/
│
├── gpmf-android/                 # Main Android library module
│   ├── build.gradle.kts
│   └── src/main/
│       ├── cpp/                  # Native JNI and build
│       │   ├── CMakeLists.txt
│       │   └── gpmf_jni.c        # JNI bridge to Kotlin/Java
│       └── kotlin/io/github/gpmf/
│           ├── GpmfParser.kt     # Public API
│           ├── GpmfException.kt
│           └── model/            # Data classes
│               ├── GpsPoint.kt
│               ├── VideoInfo.kt
│               ├── AccelerometerSample.kt
│               ├── GyroscopeSample.kt
│               ├── GravityVectorSample.kt
│               ├── MagnetometerSample.kt
│               ├── CameraOrientationSample.kt
│               ├── ImageOrientationSample.kt
│               └── ShutterSpeedSample.kt
│
├── gpmf_parser/                  # Vendored C GPMF parser (from GoPro)
│   ├── CMakeLists.txt            # Standalone CMake (for host/build verification)
│   └── src/
│       ├── GPMF_parser.c         # Core GPMF KLV parsing
│       ├── GPMF_parser.h
│       ├── GPMF_mp4reader.c       # MP4/MOV metadata track reading
│       ├── GPMF_mp4reader.h
│       ├── GPMF_utils.c          # Utilities (incl. GetGPMFSampleRate)
│       ├── GPMF_utils.h
│       ├── GPMF_common.h          # Types, errors, FourCC macros
│       ├── GPMF_bitstream.h
│       ├── gpmf_parser.c         # Higher-level FFI wrapper (standalone use)
│       └── gpmf_bindings.c       # Alternative FFI bindings (standalone)
│
├── build.gradle.kts              # Root Gradle config
├── settings.gradle.kts
├── README.md
└── LICENSE
```

**Main components:**

| Component | Purpose |
|-----------|---------|
| `GpmfParser` (Kotlin) | Public API; `Closeable` parser, loads `libgpmf_android.so`, exposes typed data accessors |
| `gpmf_jni.c` | JNI layer: maps Kotlin calls to native C GPMF functions |
| `GPMF_parser.c/h` | Core GPMF KLV parsing (keys, types, scaled data extraction) |
| `GPMF_mp4reader.c/h` | MP4 file I/O, GPMF track discovery, payload extraction |
| `GPMF_utils.c/h` | Sample rate calculation, helper functions |

---

## 2. How It Parses GPMF/GoPro Metadata

### Data flow

1. **Open MP4** → `OpenMP4Source()` locates the GPMF track (`meta`/`gpmd`).
2. **Payload iteration** → For each payload index: `GetPayloadSize`, `GetPayloadResource`, `GetPayload`, `GetPayloadTime`.
3. **GPMF stream** → `GPMF_Init()` parses the payload as KLV (Key–Length–Value).
4. **Find streams** → `GPMF_FindNext()` searches for FourCC tags (e.g. `GPS5`, `GPS9`, `ACCL`, `GYRO`).
5. **Extract data** → `GPMF_ScaledData()` converts raw values using `SCAL`/`TYPE` metadata.

### FourCC tags supported

| Tag | Description |
|-----|-------------|
| `GPS5` | GPS (lat, lon, alt, 2D speed) – Hero 5–10 |
| `GPS9` | High-precision GPS (incl. days/since 2000, seconds, DOP, fix) – Hero 11/13 |
| `GPSU` | UTC time string (YYMMDDHHMMSS.sss) |
| `GPSF` | Fix status |
| `GPSP` | Precision (DOP × 100) |
| `ACCL` | Accelerometer (m/s²) |
| `GYRO` | Gyroscope (rad/s) |
| `GRAV` | Gravity vector |
| `MAGN` | Magnetometer (µT) |
| `CORI` | Camera orientation (quaternion) |
| `IORI` | Image orientation (quaternion) |
| `SHUT` | Shutter speed (exposure time) |

### Parsing details

- **GPS**: Prefers `GPS9`, falls back to `GPS5`; for GPS5, reads `GPSU`, `GPSF`, `GPSP` for UTC, fix, precision.
- **Sensors**: Generic path via `extractSensorData()`; iterates payloads, finds FourCC, uses `GPMF_ScaledData()` for double output.
- **Timestamps**: Each payload has `inTime`/`outTime`; per-sample timestamps interpolated using `sampleDuration = (outTime - inTime) / sampleCount`.

---

## 3. Android Build Setup

### Gradle

- **Root**: `build.gradle.kts` – Android Library 8.2.0, Kotlin 1.9.21.
- **Library** (`gpmf-android/build.gradle.kts`):
  - `minSdk = 21`, `compileSdk = 34`
  - `ndk.abiFilters`: `arm64-v8a`, `armeabi-v7a`, `x86_64`, `x86`
  - `externalNativeBuild.cmake.path`: `src/main/cpp/CMakeLists.txt`
  - CMake version: 3.22.1
  - JDK 8 compatibility

### CMake / NDK

- **CMake path**: `gpmf-android/src/main/cpp/CMakeLists.txt`
- **GPMF sources**: `gpmf_parser/src` via `GPMF_SRC_DIR`
- **Native library**: `gpmf_android` (produces `libgpmf_android.so` per ABI)
- **Linked libs**: `log` (Android logging)
- **C standard**: C11
- **Compiler flags**: `-Wall`, `-Wextra`, `-Wno-unused-parameter`, `-O2`

### Build commands

```bash
# Build release AAR
./gradlew :gpmf-android:assembleRelease

# Output: gpmf-android/build/outputs/aar/
```

### Requirements

- Android SDK with NDK
- CMake 3.22.1 (via Android SDK)
- JDK 11+

---

## 4. Native Code Language

- **Language**: **C** (C11)
- **JNI**: Yes, Kotlin/Java ↔ C
- **No C++** in the main GPMF path (CMake project type is `C`)

Source files:

| File | Role |
|------|------|
| `gpmf_jni.c` | JNI implementations for `GpmfParser` native methods |
| `GPMF_parser.c` | Core GPMF parsing |
| `GPMF_mp4reader.c` | MP4 reader |
| `GPMF_utils.c` | Utils (sample rate, etc.) |
| `gpmf_parser.c` | Standalone FFI wrapper (not used by Android JNI) |
| `gpmf_bindings.c` | Alternative FFI (not used by Android JNI) |

---

## 5. Entry Points for Parsing

### Kotlin/Java API

```kotlin
// Recommended: withGpmfParser()
val data = withGpmfParser("/path/to/gopro.mp4") { parser ->
    parser.getGpsData()
}

// Manual
GpmfParser().use { parser ->
    parser.open("/path/to/gopro.mp4")
    val gps = parser.getGpsData()
    val accel = parser.getAccelerometerData()
    // ...
}
```

### JNI methods (gpmf_jni.c)

| JNI function | Kotlin method | Role |
|--------------|---------------|------|
| `Java_io_github_gpmf_GpmfParser_nativeOpen` | `nativeOpen(String)` | Open MP4, return handle |
| `Java_io_github_gpmf_GpmfParser_nativeClose` | `nativeClose(Long)` | Release resources |
| `Java_io_github_gpmf_GpmfParser_nativeGetDuration` | `nativeGetDuration` | Duration (s) |
| `Java_io_github_gpmf_GpmfParser_nativeGetPayloadCount` | `nativeGetPayloadCount` | Number of GPMF payloads |
| `Java_io_github_gpmf_GpmfParser_nativeGetVideoInfo` | `nativeGetVideoInfo` | Frame count, fps |
| `Java_io_github_gpmf_GpmfParser_nativeGetGpsData` | `nativeGetGpsData` | GPS points |
| `Java_io_github_gpmf_GpmfParser_nativeGetAccelerometerData` | `nativeGetAccelerometerData` | ACCL |
| `Java_io_github_gpmf_GpmfParser_nativeGetGyroscopeData` | `nativeGetGyroscopeData` | GYRO |
| `Java_io_github_gpmf_GpmfParser_nativeGetGravityVectorData` | `nativeGetGravityVectorData` | GRAV |
| `Java_io_github_gpmf_GpmfParser_nativeGetMagnetometerData` | `nativeGetMagnetometerData` | MAGN |
| `Java_io_github_gpmf_GpmfParser_nativeGetCameraOrientationData` | `nativeGetCameraOrientationData` | CORI |
| `Java_io_github_gpmf_GpmfParser_nativeGetImageOrientationData` | `nativeGetImageOrientationData` | IORI |
| `Java_io_github_gpmf_GpmfParser_nativeGetShutterSpeedData` | `nativeGetShutterSpeedData` | SHUT |
| `Java_io_github_gpmf_GpmfParser_nativeGetSensorSampleRate` | `nativeGetSensorSampleRate` | Sample rate for FourCC |

### C entry points (gpmf_parser)

- `OpenMP4Source()` – open MP4, find GPMF track
- `GetPayload()`, `GetPayloadSize()`, `GetPayloadTime()` – payload access
- `GPMF_Init()`, `GPMF_FindNext()`, `GPMF_ScaledData()` – GPMF stream parsing

---

## 6. Dependencies on External GPMF Libraries

### Vendored GoPro GPMF parser

The project does **not** depend on an external package. The GPMF logic is **vendored** in `gpmf_parser/` and comes from the official [GoPro gpmf-parser](https://github.com/gopro/gpmf-parser) C library.

**Vendored files (GoPro origin):**

- `GPMF_parser.c`, `GPMF_parser.h`
- `GPMF_mp4reader.c`, `GPMF_mp4reader.h`
- `GPMF_utils.c`, `GPMF_utils.h`
- `GPMF_common.h`
- `GPMF_bitstream.h`

**License**: Apache 2.0 / MIT dual license (see headers and README).

**Android build**: Only `GPMF_parser.c`, `GPMF_mp4reader.c`, and `GPMF_utils.c` are compiled into the library; `gpmf_parser.c` and `gpmf_bindings.c` are for standalone/FFI use and are not part of the Android native build.

### Other dependencies

- **Android**: `androidx.core:core-ktx`, `androidx.annotation:annotation`
- **Native**: `log` (Android NDK)
- **No extra GPMF libraries or submodules**

---

## Summary

| Aspect | Details |
|--------|---------|
| **Structure** | `app` (demo), `gpmf-android` (library), `gpmf_parser` (vendored C) |
| **Parsing** | GPMF KLV via `GPMF_Init` / `GPMF_FindNext` / `GPMF_ScaledData`; MP4 via `GPMF_mp4reader` |
| **Build** | Gradle 8.2 + CMake 3.22.1 + NDK; C11; four ABIs |
| **Native** | C with JNI; no C++ in GPMF code |
| **Entry point** | `GpmfParser` → `nativeOpen` → `OpenMP4Source` |
| **External GPMF** | Vendored GoPro gpmf-parser; no external GPMF package |
