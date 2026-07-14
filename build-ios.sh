#!/bin/bash
# Build telemetry-parser for iOS and produce an XCFramework for Swift Package Manager.
#
# Requires: macOS, Xcode + Command Line Tools, Rust (rustup).
#
# Usage:
#   ./build-ios.sh                    # release profile, arm64 device + simulator
#   ./build-ios.sh debug              # dev profile (unoptimized)
#   CARGO_PROFILE=ios-dev ./build-ios.sh
#   INCLUDE_X86_SIM=1 ./build-ios.sh  # also build x86_64 simulator (Intel Mac simulators)
#
# Output: ios/build/TelemetryParser.xcframework

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

MODE="${1:-release}"
: "${INCLUDE_X86_SIM:=0}"

if [[ "$MODE" == "debug" ]]; then
  CARGO_PROFILE="${CARGO_PROFILE:-dev}"
  CARGO_FLAG=""
elif [[ "$MODE" == "release" ]]; then
  CARGO_PROFILE="${CARGO_PROFILE:-release}"
  if [[ "$CARGO_PROFILE" == "release" ]]; then
    CARGO_FLAG="--release"
  else
    CARGO_FLAG="--profile $CARGO_PROFILE"
  fi
else
  echo "Unknown mode: $MODE (use 'debug' or 'release')" >&2
  exit 1
fi

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "iOS builds require macOS with Xcode installed." >&2
  exit 1
fi

if ! command -v xcodebuild >/dev/null 2>&1; then
  echo "xcodebuild not found. Install Xcode Command Line Tools." >&2
  exit 1
fi

TARGETS=(aarch64-apple-ios aarch64-apple-ios-sim)
if [[ "$INCLUDE_X86_SIM" == "1" ]]; then
  TARGETS+=(x86_64-apple-ios)
fi

for target in "${TARGETS[@]}"; do
  rustup target add "$target" >/dev/null 2>&1 || true
done

BUILD_DIR="$SCRIPT_DIR/ios/build"
LIBS_DIR="$BUILD_DIR/libs"
OUT_XCF="$BUILD_DIR/TelemetryParser.xcframework"

rm -rf "$OUT_XCF"
mkdir -p "$LIBS_DIR"

XCFRAMEWORK_ARGS=()

echo "Building iOS XCFramework (mode=$MODE, profile=$CARGO_PROFILE, targets=${TARGETS[*]})..."

for target in "${TARGETS[@]}"; do
  echo "  cargo build --target $target ${CARGO_FLAG:---release}"
  # shellcheck disable=SC2086
  cargo build -p telemetry-parser --target "$target" $CARGO_FLAG

  lib_path="$SCRIPT_DIR/target/$target/$CARGO_PROFILE/libtelemetry_parser.a"
  if [[ ! -f "$lib_path" ]]; then
    echo "Expected static library not found: $lib_path" >&2
    exit 1
  fi

  staged="$LIBS_DIR/libtelemetry_parser_${target}.a"
  cp "$lib_path" "$staged"

  XCFRAMEWORK_ARGS+=(-library "$staged")
  XCFRAMEWORK_ARGS+=(-headers "$SCRIPT_DIR/ios/include")
done

xcodebuild -create-xcframework \
  "${XCFRAMEWORK_ARGS[@]}" \
  -output "$OUT_XCF"

echo "Done. XCFramework: $OUT_XCF"
