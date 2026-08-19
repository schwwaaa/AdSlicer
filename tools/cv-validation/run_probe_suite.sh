#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")" && pwd)"
PROJECT="$(cd "$ROOT/../.." && pwd)"
CORPUS="$ROOT/corpus"
RESULTS="$ROOT/results"
mkdir -p "$RESULTS"

if [[ ! -d "$CORPUS" ]] || ! compgen -G "$CORPUS/*.mkv" > /dev/null || [[ ! -f "$CORPUS/08_commercial_block.mkv" ]]; then
  "$ROOT/generate_synthetic_corpus.sh"
fi

cd "$PROJECT/src-tauri"
for media in "$CORPUS"/*.mkv; do
  stem="$(basename "$media" .mkv)"
  echo "== probing $stem =="
  cargo run --quiet --example adslicer_cv_probe --features opencv-analysis -- \
    "$media" "$RESULTS/$stem"
done

echo
printf 'Probe suite complete. Results: %s\n' "$RESULTS"
