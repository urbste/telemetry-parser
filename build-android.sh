#!/bin/bash
# Build telemetry-parser for Android and produce the AAR.
# Rust is built by Gradle (buildRustLib); this script only forwards ABI/profile and runs assembleRelease.
#
# Requires: rustup, cargo-ndk (cargo install cargo-ndk), Android SDK + NDK (see README).
# Optional: ANDROID_NDK_HOME / NDK_HOME (AGP also uses ndkVersion from android/build.gradle.kts).
#
# Environment:
#   ABIS            Comma-separated ABI list (default: arm64-v8a)
#   CARGO_PROFILE   Cargo profile name: release or android-dev (default: release)

set -e
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

: "${ABIS:=arm64-v8a}"
: "${CARGO_PROFILE:=release}"

echo "Building Android AAR (ABIs: $ABIS, Cargo profile: $CARGO_PROFILE)..."
./gradlew -PrustAbis="$ABIS" -PrustProfile="$CARGO_PROFILE" :android:assembleRelease

echo "Done. AAR: android/build/outputs/aar/telemetry-android-release.aar"
