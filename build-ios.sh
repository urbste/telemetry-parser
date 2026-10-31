#!/bin/bash
# Build telemetry-parser for iOS and produce an XCFramework for Swift Package Manager.
#
# Requires: macOS, Xcode + Command Line Tools, Rust (rustup).
#
# Usage:
#   ./build-ios.sh                    # release profile, arm64 device + simulator
#   ./build-ios.sh debug              # dev profile (Cargo's debug artifact directory)
#   CARGO_PROFILE=ios-dev ./build-ios.sh
#   INCLUDE_X86_SIM=1 ./build-ios.sh  # also build x86_64 simulator (Intel Mac simulators)
#   CARGO_TARGET_DIR=/tmp/parser-target TELEMETRY_PARSER_OUTPUT_DIR=/tmp/parser-out ./build-ios.sh
#
# Output: ios/build/TelemetryParser.xcframework

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

MODE="${1:-release}"
: "${INCLUDE_X86_SIM:=0}"
EXPECTED_RUST_VERSION="1.94.0"

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

if ! command -v rustc >/dev/null 2>&1 || ! rustc --version | grep -Eq "^rustc ${EXPECTED_RUST_VERSION//./\\.}([[:space:]]|$)"; then
  echo "Rust ${EXPECTED_RUST_VERSION} is required (found: $(rustc --version 2>/dev/null || echo missing))." >&2
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

if ! command -v rustup >/dev/null 2>&1; then
  echo "rustup is required to verify installed Apple targets." >&2
  exit 1
fi
for target in "${TARGETS[@]}"; do
  if ! rustup target list --installed | grep -Fxq "$target"; then
    echo "Rust target $target is not installed. Install it with: rustup target add $target" >&2
    exit 1
  fi
done

CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$SCRIPT_DIR/target}"
BUILD_DIR="${TELEMETRY_PARSER_OUTPUT_DIR:-$SCRIPT_DIR/ios/build}"
LIBS_DIR="$BUILD_DIR/libs"
OUT_XCF="$BUILD_DIR/TelemetryParser.xcframework"
PREBUILT_DIR="${TELEMETRY_PARSER_PREBUILT_DIR:-}"

if [[ -n "$PREBUILT_DIR" ]]; then
  for target in "${TARGETS[@]}"; do
    if [[ ! -s "$PREBUILT_DIR/libtelemetry_parser_${target}.a" ]]; then
      echo "Verified prebuilt archive is missing for $target: $PREBUILT_DIR/libtelemetry_parser_${target}.a" >&2
      exit 1
    fi
  done
fi

rm -rf "$OUT_XCF"
mkdir -p "$LIBS_DIR"

# Rust's LLVM can be newer than Xcode's. Inspect native Mach-O symbols rather
# than embedded LLVM bitcode; this also rejects a library without native exports.
if ! NM_HELP="$(xcrun nm --help 2>&1)" || ! grep -Fq -- '--no-llvm-bc' <<< "$NM_HELP"; then
  echo 'Xcode nm must support --no-llvm-bc for native Rust symbol inspection.' >&2
  exit 1
fi

XCFRAMEWORK_ARGS=()

echo "Building iOS XCFramework (mode=$MODE, profile=$CARGO_PROFILE, targets=${TARGETS[*]})..."

for target in "${TARGETS[@]}"; do
  if [[ -n "$PREBUILT_DIR" ]]; then
    lib_path="$PREBUILT_DIR/libtelemetry_parser_${target}.a"
    echo "  using verified cached archive for $target"
  else
    if [[ -n "$CARGO_FLAG" ]]; then
      echo "  cargo build --locked -p telemetry-parser --target $target $CARGO_FLAG"
    else
      echo "  cargo build --locked -p telemetry-parser --target $target (dev profile)"
    fi
    # shellcheck disable=SC2086
    CARGO_TARGET_DIR="$CARGO_TARGET_DIR" cargo build --locked -p telemetry-parser --target "$target" $CARGO_FLAG

    if [[ "$CARGO_PROFILE" == "dev" ]]; then
      ARTIFACT_PROFILE_DIR="debug"
    else
      ARTIFACT_PROFILE_DIR="$CARGO_PROFILE"
    fi
    lib_path="$CARGO_TARGET_DIR/$target/$ARTIFACT_PROFILE_DIR/libtelemetry_parser.a"
  fi
  if [[ ! -f "$lib_path" ]]; then
    echo "Expected static library not found: $lib_path" >&2
    exit 1
  fi

  staged="$LIBS_DIR/libtelemetry_parser_${target}.a"
  cp "$lib_path" "$staged"

  case "$target" in
    aarch64-apple-ios) arch=arm64 ;;
    aarch64-apple-ios-sim) arch=arm64 ;;
    x86_64-apple-ios) arch=x86_64 ;;
  esac
  # -verify_arch consumes all following arguments as architecture names.
  if ! xcrun lipo "$staged" -verify_arch "$arch"; then
    echo "Unexpected architecture in $staged (expected $arch)." >&2
    exit 1
  fi
  # Consume nm fully: grep -q in a pipe can cause SIGPIPE under pipefail.
  symbols_path="$staged.symbols"
  xcrun nm --no-llvm-bc -gU "$staged" > "$symbols_path"
  for symbol in telemetry_parser_open telemetry_parser_close telemetry_parser_free_string \
    telemetry_parser_last_error telemetry_parser_get_camera_type telemetry_parser_get_duration \
    telemetry_parser_get_video_info_json telemetry_parser_get_recording_metadata_json \
    telemetry_parser_get_camera_model telemetry_parser_get_lens_metadata_json \
    telemetry_parser_get_gps_json telemetry_parser_get_accelerometer_json \
    telemetry_parser_get_gyroscope_json telemetry_parser_get_gravity_json \
    telemetry_parser_get_camera_orientation_json telemetry_parser_get_image_orientation_json \
    telemetry_parser_get_orientation_combined_json; do
    if ! grep -q "_${symbol}$" "$symbols_path"; then
      echo "Required C entry point $symbol is absent from $staged." >&2
      exit 1
    fi
  done

  XCFRAMEWORK_ARGS+=(-library "$staged")
  XCFRAMEWORK_ARGS+=(-headers "$SCRIPT_DIR/ios/include")
done

xcodebuild -create-xcframework \
  "${XCFRAMEWORK_ARGS[@]}" \
  -output "$OUT_XCF"

[[ -s "$SCRIPT_DIR/ios/include/telemetry_parser.h" ]] || { echo "Missing C header." >&2; exit 1; }
[[ -s "$SCRIPT_DIR/ios/include/module.modulemap" ]] || { echo "Missing C module map." >&2; exit 1; }
[[ -s "$OUT_XCF/Info.plist" ]] || { echo "XCFramework metadata was not created." >&2; exit 1; }
[[ -s "$OUT_XCF/ios-arm64/Headers/telemetry_parser.h" ]] || { echo "Device slice is missing the C header." >&2; exit 1; }
[[ -s "$OUT_XCF/ios-arm64/Headers/module.modulemap" ]] || { echo "Device slice is missing the C module map." >&2; exit 1; }
[[ -s "$OUT_XCF/ios-arm64-simulator/Headers/telemetry_parser.h" ]] || { echo "Simulator slice is missing the C header." >&2; exit 1; }
if ! plutil -extract AvailableLibraries xml1 -o - "$OUT_XCF/Info.plist" | grep -q 'ios-arm64'; then
  echo "XCFramework is missing the arm64 iOS device slice." >&2
  exit 1
fi
if ! plutil -extract AvailableLibraries xml1 -o - "$OUT_XCF/Info.plist" | grep -q 'ios-arm64-simulator'; then
  echo "XCFramework is missing the arm64 iOS Simulator slice." >&2
  exit 1
fi
if [[ "$INCLUDE_X86_SIM" == "1" ]] && ! plutil -extract AvailableLibraries xml1 -o - "$OUT_XCF/Info.plist" | grep -q 'ios-x86_64-simulator'; then
  echo "XCFramework is missing the requested x86_64 iOS Simulator slice." >&2
  exit 1
fi

echo "Done. XCFramework: $OUT_XCF"
