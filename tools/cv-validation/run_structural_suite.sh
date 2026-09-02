#!/usr/bin/env bash
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$HERE/../.." && pwd)"
CORPUS="$HERE/corpus"
OUT="$HERE/results/09_structural_full_timeline/cv6-pipeline"

if [[ ! -f "$CORPUS/09_structural_full_timeline.mkv" ]]; then
  "$HERE/generate_synthetic_corpus.sh"
fi

rm -rf "$OUT"
echo "== CV-6 structural full-timeline regression =="
cargo run --quiet \
  --manifest-path "$PROJECT_ROOT/src-tauri/Cargo.toml" \
  --example adslicer_cv_pipeline \
  --features opencv-analysis \
  -- \
  --input "$CORPUS/09_structural_full_timeline.mkv" \
  --output "$OUT" \
  --skip-edit-previews \
  --skip-legacy \
  --segment-render-mode complete

python3 "$HERE/evaluate_structural_segmentation.py" "$OUT/OPENCV_TEST_REPORT.json"
