#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")" && pwd)"
CORPUS="$ROOT/corpus"
OUT="$ROOT/legacy_blackdetect_baseline.txt"
FFMPEG="${FFMPEG:-ffmpeg}"

if [[ ! -d "$CORPUS" ]] || ! compgen -G "$CORPUS/*.mkv" > /dev/null; then
  "$ROOT/generate_synthetic_corpus.sh"
fi

{
  echo "AdSlicer legacy FFmpeg blackdetect baseline"
  echo "Parameters: d=0.10 pix_th=0.08 pic_th=0.98"
  "$FFMPEG" -version | head -n 1
  echo
  for media in "$CORPUS"/*.mkv; do
    echo "--- $(basename "$media")"
    found="$($FFMPEG -hide_banner -nostats -nostdin -i "$media" \
      -vf "blackdetect=d=0.10:pix_th=0.08:pic_th=0.98" -f null - 2>&1 \
      | grep -E 'black_start|black_end' || true)"
    if [[ -n "$found" ]]; then
      printf '%s\n' "$found"
    else
      echo "(no blackdetect event)"
    fi
  done
} | tee "$OUT"
