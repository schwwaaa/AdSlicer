#!/bin/bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")" && pwd)"
CLIPS_DIR="${1:-}"
OUT_ROOT="${2:-$ROOT/cv-problem-clip-output}"
if [[ -z "$CLIPS_DIR" || ! -d "$CLIPS_DIR" ]]; then
  echo "usage: ./test-problem-clips.sh /path/to/problem-clips [output-root]" >&2
  exit 2
fi
clips=(
  1995-WCPX6-CBS_segment_0005.mp4
  1995-WCPX6-CBS_segment_0009.mp4
  1995-WCPX6-CBS_segment_0010.mp4
  1995-WCPX6-CBS_segment_0014.mp4
  1995-WCPX6-CBS_segment_0020.mp4
  1995-WCPX6-CBS_segment_0023.mp4
)
mkdir -p "$OUT_ROOT"
for name in "${clips[@]}"; do
  path="$CLIPS_DIR/$name"
  [[ -f "$path" ]] || { echo "ERROR: missing $path" >&2; exit 2; }
  stem="${name%.*}"
  echo "== $name =="
  "$ROOT/test-opencv.sh" --input "$path" --output "$OUT_ROOT/$stem" --skip-legacy --skip-edit-previews --segment-render-mode none
done
python3 "$ROOT/tools/cv-validation/problem-clips/evaluate_problem_clips.py" "$OUT_ROOT"
