#!/usr/bin/env bash
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$HERE/../.." && pwd)"
CORPUS="$HERE/corpus"
OUT="$HERE/results/08_commercial_block/cv5-pipeline"

if [[ ! -f "$CORPUS/08_commercial_block.mkv" ]]; then
  "$HERE/generate_synthetic_corpus.sh"
fi

rm -rf "$OUT"
echo "== CV-5 watchable edit-validation regression =="
cargo run --quiet \
  --manifest-path "$PROJECT_ROOT/src-tauri/Cargo.toml" \
  --example adslicer_cv_pipeline \
  --features opencv-analysis \
  -- \
  --input "$CORPUS/08_commercial_block.mkv" \
  --output "$OUT" \
  --preview-context 1 \
  --preview-limit 5 \
  --skip-legacy

python3 "$HERE/evaluate_edit_validation.py" "$OUT/05-edit-validation/edit_validation_manifest.json"
