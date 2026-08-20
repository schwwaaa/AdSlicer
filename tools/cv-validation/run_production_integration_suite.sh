#!/usr/bin/env bash
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$HERE/../.." && pwd)"
CORPUS="$HERE/corpus"
OUT="$HERE/results/09_structural_full_timeline/cv7-production"

if [[ ! -f "$CORPUS/09_structural_full_timeline.mkv" ]]; then
  "$HERE/generate_synthetic_corpus.sh"
fi

rm -rf "$OUT"
mkdir -p "$OUT"

echo "== CV-7 production integration regression =="

cargo run --quiet \
  --manifest-path "$PROJECT_ROOT/src-tauri/Cargo.toml" \
  --example adslicer_cv7_production \
  --features opencv-analysis \
  -- \
  --input "$CORPUS/09_structural_full_timeline.mkv" \
  --output "$OUT/complete" \
  --policy complete_segments
python3 "$HERE/evaluate_cv7_production.py" \
  "$OUT/complete/opencv_production_manifest.json" complete_segments 4

cargo run --quiet \
  --manifest-path "$PROJECT_ROOT/src-tauri/Cargo.toml" \
  --example adslicer_cv7_production \
  --features opencv-analysis \
  -- \
  --input "$CORPUS/09_structural_full_timeline.mkv" \
  --output "$OUT/every" \
  --policy every_separator
python3 "$HERE/evaluate_cv7_production.py" \
  "$OUT/every/opencv_production_manifest.json" every_separator 5
