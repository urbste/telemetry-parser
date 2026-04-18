# telemetry-parser
A tool to parse real-time metadata embedded in video files or telemetry from other sources.

# Supported formats:
- [x] GoPro (HERO 5 and later)
- [x] Sony (a1, a7c, a7r V, a7 IV, a7s III, a9 II, a9 III, FX3, FX6, FX9, RX0 II, RX100 VII, ZV1, ZV-E10, ZV-E10 II, ZV-E1, a6700)
- [x] Insta360 (OneR, OneRS, SMO 4k, Go, GO2, GO3, GO3S, GOUltra, Caddx Peanut, Ace, Ace Pro, X5)
- [x] DJI (Avata, Avata 2, O3/O4 Air Unit, Action 2/4/5/6/Nano, Neo, Neo2)
- [x] XTRA (Edge, Edge Pro)
- [x] Blackmagic RAW (*.braw)
- [x] RED RAW (V-Raptor, KOMODO) (*.r3d)
- [x] Canon (C50, C80, C400, R6 Mk3, R5 Mk2) (*.mp4, *.mov, *.mxf, *.crm)
- [x] Freefly (Ember)
- [x] Betaflight blackbox (*.bfl, *.bbl, *.csv)
- [x] ArduPilot logs (*.bin, *.log)
- [x] Gyroflow [.gcsv log](https://docs.gyroflow.xyz/app/technical-details/gcsv-format)
- [x] iOS apps: [`Sensor Logger`](https://apps.apple.com/us/app/sensor-logger/id1531582925), [`G-Field Recorder`](https://apps.apple.com/at/app/g-field-recorder/id1154585693), [`Gyro`](https://apps.apple.com/us/app/gyro-record-device-motion-data/id1161532981)
- [x] Android apps: [`Sensor Logger`](https://play.google.com/store/apps/details?id=com.kelvin.sensorapp&hl=de_AT&gl=US), [`Sensor Record`](https://play.google.com/store/apps/details?id=de.martingolpashin.sensor_record), [`OpenCamera Sensors`](https://github.com/MobileRoboticsSkoltech/OpenCamera-Sensors), [`MotionCam Pro`](https://play.google.com/store/apps/details?id=com.motioncam.pro)
- [x] Runcam CSV (Runcam 5 Orange, iFlight GOCam GR, Runcam Thumb, Mobius Maxi 4K)
- [x] Hawkeye Firefly X Lite CSV
- [x] XTU (S2Pro, S3Pro)
- [x] WitMotion (WT901SDCL binary and *.txt)
- [x] Vuze (VuzeXR)
- [x] KanDao (Obisidian Pro, Qoocam EGO)
- [x] [CAMM format](https://developers.google.com/streetview/publish/camm-spec)
- [ ] TODO DJI flight logs (*.dat, *.txt)

# Example usage
Produce Betaflight blackbox CSV with gyroscope and accelerometer from the input file
```
gyro2bb file.mp4
```
Dump all metadata found in the source file.
```
gyro2bb --dump file.mp4
```


# Python module
Python module is available on [PyPI](https://pypi.org/project/telemetry-parser/).
Details in [bin/python-module](https://github.com/AdrianEddy/telemetry-parser/tree/master/bin/python-module)


# Building
1. Get latest stable Rust language from: https://rustup.rs/
2. Clone the repo: `git clone https://github.com/AdrianEddy/telemetry-parser.git`
3. Build the binary: `cd bin/gyro2bb ; cargo build --release`
4. Resulting file will be in `target/release/` directory

## Android library (AAR)

Rust JNI + Kotlin live under [`android/`](android/). The Gradle task `buildRustLib` runs **cargo-ndk** before packaging; you do not need to run `cargo ndk` manually when using `./build-android.sh` or `./gradlew :android:assembleRelease`.

**Prerequisites**

- [Rust / rustup](https://rustup.rs/) with Android targets (e.g. `rustup target add aarch64-linux-android`)
- [cargo-ndk](https://github.com/bbqsrc/cargo-ndk): `cargo install cargo-ndk`
- Android SDK (platform **android-34**) and NDK **26.3.11579264** (pinned in [`android/build.gradle.kts`](android/build.gradle.kts))
- Optional but recommended: `export ANDROID_NDK_HOME=$HOME/Android/Sdk/ndk/26.3.11579264` (adjust path to your SDK)

**Commands**

- Default (fast): single ABI **arm64-v8a**, Cargo **release** profile  
  `./build-android.sh`  
  Output: `android/build/outputs/aar/telemetry-android-release.aar`
- Fast iteration (lighter Rust profile `android-dev` from [`Cargo.toml`](Cargo.toml)):  
  `ABIS=arm64-v8a CARGO_PROFILE=android-dev ./build-android.sh`
- All common ABIs (slower; matches many emulators + 32-bit):  
  `ABIS=arm64-v8a,armeabi-v7a,x86_64,x86 ./build-android.sh`

Equivalent Gradle properties: `-PrustAbis=...` and `-PrustProfile=release` or `-PrustProfile=android-dev`.

**Notes**

- `buildRustLib` declares inputs/outputs so Gradle skips Rust when nothing changed (`UP-TO-DATE`).
- [gradle.properties](gradle.properties) enables parallel builds and build caching. Configuration cache is commented out by default (AGP 8.2 + JDK 21 often hits `JdkImageTransform` issues with it on); you can re-enable when using JDK 17 or a newer AGP.

<br>

#### License

<sup>
Licensed under either of <a href="LICENSE-APACHE">Apache License, Version
2.0</a> or <a href="LICENSE-MIT">MIT license</a> at your option.
</sup>

<br>

<sub>
Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in this crate by you, as defined in the Apache-2.0 license, shall
be dual licensed as above, without any additional terms or conditions.
</sub>