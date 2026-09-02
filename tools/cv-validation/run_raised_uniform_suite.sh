#!/usr/bin/env bash
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$HERE/../.." && pwd)"
CORPUS="$HERE/corpus"
OUT="$HERE/results/10_raised_uniform_separator/cv61-pipeline"

if [[ ! -f "$CORPUS/10_raised_uniform_separator.mkv" ]]; then
  "$HERE/generate_synthetic_corpus.sh"
fi

rm -rf "$OUT"
echo "== CV-6.1 raised-uniform VHS-black regression =="
cargo run --quiet \
  --manifest-path "$PROJECT_ROOT/src-tauri/Cargo.toml" \
  --example adslicer_cv_pipeline \
  --features opencv-analysis \
  -- \
  --input "$CORPUS/10_raised_uniform_separator.mkv" \
  --output "$OUT" \
  --skip-edit-previews \
  --skip-legacy \
  --segment-render-mode complete

python3 "$HERE/evaluate_raised_uniform_separator.py" "$OUT/OPENCV_TEST_REPORT.json"
