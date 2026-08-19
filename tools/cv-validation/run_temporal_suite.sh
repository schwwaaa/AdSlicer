#!/usr/bin/env bash
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$HERE/../.." && pwd)"
RESULTS="$HERE/results"

if [[ ! -d "$RESULTS" ]]; then
  echo "Missing CV-1 results directory: $RESULTS"
  echo "Run ./run_probe_suite.sh first."
  exit 2
fi

found=0
for evidence in "$RESULTS"/*/opencv_frame_metrics.jsonl; do
  [[ -f "$evidence" ]] || continue
  found=1
  case_dir="$(dirname "$evidence")"
  case_name="$(basename "$case_dir")"
  outdir="$case_dir/temporal"
  echo "== temporal analysis $case_name =="
  cargo run --quiet \
    --manifest-path "$PROJECT_ROOT/src-tauri/Cargo.toml" \
    --example adslicer_cv_temporal \
    -- "$evidence" "$outdir"
done

if [[ "$found" -eq 0 ]]; then
  echo "No CV-1 evidence files found. Run ./run_probe_suite.sh first."
  exit 2
fi

echo
echo "Temporal suite complete."
python3 "$HERE/evaluate_temporal_events.py"
