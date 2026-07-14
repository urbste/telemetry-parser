#!/bin/bash
# Build telemetry-parser for Android and produce the AAR.
# Rust is built by Gradle (buildRustLib); this script forwards ABI/profile and runs assembleRelease or assembleDebug.
#
# Requires: rustup, cargo-ndk (cargo install cargo-ndk), Android SDK + NDK (see README).
# Optional: ANDROID_NDK_HOME / NDK_HOME (AGP also uses ndkVersion from android/build.gradle.kts).
#
# Usage:
#   ./build-android.sh              # release library AAR (default)
#   ./build-android.sh debug        # debug library AAR (same native .so profile as release by default)
#
# Environment:
#   ABIS            Comma-separated ABI list (default: arm64-v8a)
#   CARGO_PROFILE   Cargo profile name: release or android-dev (default: release)

set -e
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

: "${ABIS:=arm64-v8a}"
: "${CARGO_PROFILE:=release}"

MODE="${1:-release}"

GRADLE_TASK=":android:assembleRelease"
OUT_NAME="telemetry-android-release.aar"
if [[ "$MODE" == "debug" ]]; then
  GRADLE_TASK=":android:assembleDebug"
  OUT_NAME="telemetry-android-debug.aar"
elif [[ "$MODE" != "release" ]]; then
  echo "Unknown mode: $MODE (use 'debug' or 'release')" >&2
  exit 1
fi

echo "Building Android AAR ($MODE, ABIs: $ABIS, Cargo profile: $CARGO_PROFILE)..."
./gradlew -PrustAbis="$ABIS" -PrustProfile="$CARGO_PROFILE" "$GRADLE_TASK"

echo "Done. AAR: android/build/outputs/aar/$OUT_NAME"
