#!/bin/bash
# Extract GoPro metadata with both telemetry-parser and gpmf_android, output to /home/steffen/Data
#
# Prerequisites:
#   - Rust: from repo root, `cargo build -p extract-metadata --release` (workspace members in root Cargo.toml)
#   - For gpmf_android: Android device/emulator with file pushed, run instrumented test
#
# Usage:
#   ./scripts/extract-and-compare.sh /home/steffen/Data/GX011221_1766763892185.MP4

set -e
INPUT="${1:-/home/steffen/Data/GX011221_1766763892185.MP4}"
OUTPUT_DIR="$(dirname "$INPUT")"

echo "Input: $INPUT"
echo "Output dir: $OUTPUT_DIR"

# 1. telemetry-parser (runs on host)
echo ""
echo "=== telemetry-parser ==="
cd "$(dirname "$0")/.."
cargo run -p extract-metadata --release -- "$INPUT" -o "$OUTPUT_DIR/gopro_metadata_telemetry_parser.json"
echo "Output: $OUTPUT_DIR/gopro_metadata_telemetry_parser.json"

# 2. gpmf_android (requires Android device/emulator)
echo ""
echo "=== gpmf_android ==="
echo "Push file to device: adb push $INPUT /sdcard/Download/"
echo "Run test: cd /path/to/gpmf_android && ./gradlew :app:connectedDebugAndroidTest -Pandroid.testInstrumentationRunner.arguments.filePath=/sdcard/Download/$(basename "$INPUT")"
echo "Pull result: adb pull /storage/emulated/0/Android/data/io.github.gpmf.sample/files/gopro_metadata_gpmf_android.json $OUTPUT_DIR/"
echo ""
echo "Then compare: diff $OUTPUT_DIR/gopro_metadata_telemetry_parser.json $OUTPUT_DIR/gopro_metadata_gpmf_android.json"
