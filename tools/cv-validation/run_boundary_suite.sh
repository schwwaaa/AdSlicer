#!/usr/bin/env bash
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$HERE/../.." && pwd)"
RESULTS="$HERE/results"

if [[ ! -d "$RESULTS" ]]; then
  echo "Missing validation results: $RESULTS"
  echo "Run ./run_probe_suite.sh and ./run_temporal_suite.sh first."
  exit 2
fi

found=0
for temporal in "$RESULTS"/*/temporal/opencv_temporal_events.json; do
  [[ -f "$temporal" ]] || continue
  found=1
  case_dir="$(dirname "$(dirname "$temporal")")"
  case_name="$(basename "$case_dir")"
  outdir="$case_dir/boundary"
  echo "== boundary scoring $case_name =="
  cargo run --quiet \
    --manifest-path "$PROJECT_ROOT/src-tauri/Cargo.toml" \
    --example adslicer_cv_boundary \
    -- "$temporal" "$outdir"
done

if [[ "$found" -eq 0 ]]; then
  echo "No CV-2 temporal evidence found. Run ./run_temporal_suite.sh first."
  exit 2
fi

echo
echo "Boundary suite complete."
python3 "$HERE/evaluate_boundary_candidates.py"
