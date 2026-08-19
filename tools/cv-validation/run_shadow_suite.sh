#!/usr/bin/env bash
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$HERE/../.." && pwd)"
CORPUS="$HERE/corpus"
OUT="$HERE/results/08_commercial_block/cv4-pipeline"

if [[ ! -f "$CORPUS/08_commercial_block.mkv" ]]; then
  "$HERE/generate_synthetic_corpus.sh"
fi

rm -rf "$OUT"
echo "== CV-4 unified shadow-plan regression =="
cargo run --quiet \
  --manifest-path "$PROJECT_ROOT/src-tauri/Cargo.toml" \
  --example adslicer_cv_pipeline \
  --features opencv-analysis \
  -- \
  --input "$CORPUS/08_commercial_block.mkv" \
  --output "$OUT" \
  --skip-edit-previews

python3 "$HERE/evaluate_shadow_plan.py" "$OUT/OPENCV_TEST_REPORT.json"
