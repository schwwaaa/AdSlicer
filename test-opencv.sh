#!/bin/bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")" && pwd)"

# Make the Homebrew OpenCV setup self-contained across new Terminal sessions.
# Existing user environment values always win.
if command -v brew >/dev/null 2>&1; then
  if command -v pkg-config >/dev/null 2>&1 && ! pkg-config --exists opencv4 2>/dev/null; then
    if OPENCV_PREFIX="$(brew --prefix opencv@4 2>/dev/null)"; then
      export PKG_CONFIG_PATH="$OPENCV_PREFIX/lib/pkgconfig${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
    elif OPENCV_PREFIX="$(brew --prefix opencv 2>/dev/null)"; then
      export PKG_CONFIG_PATH="$OPENCV_PREFIX/lib/pkgconfig${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
    fi
  fi

  if [[ -z "${LIBCLANG_PATH:-}" ]]; then
    if LLVM_PREFIX="$(brew --prefix llvm 2>/dev/null)"; then
      export LIBCLANG_PATH="$LLVM_PREFIX/lib"
    fi
  fi
fi

usage() {
  cat <<'USAGE'
AdSlicer OpenCV unified validation

REAL MEDIA — recommended stable form:
  ./test-opencv.sh --input "/path/to/video.mp4"

REAL MEDIA + custom output folder:
  ./test-opencv.sh --input "/path/to/video.mp4" --output "/path/to/output"

OPTIONAL ANALYSIS OVERRIDES:
  --black-luma N       Strict black luma threshold (default: 16)
  --near-black N       Near-black luma threshold (default: 32)
  --stride N           Analyze every Nth frame (default: 1; use 1 for validation)
  --max-frames N       Stop after N analyzed frames (default: entire source)

SYNTHETIC REGRESSION SUITE:
  ./test-opencv.sh --synthetic

HELP:
  ./test-opencv.sh --help

Backward-compatible shorthand also works:
  ./test-opencv.sh "/path/to/video.mp4"
  ./test-opencv.sh "/path/to/video.mp4" "/path/to/output"

The media test decodes the source once, then runs all current OpenCV stages
in memory. Intermediate JSON/JSONL files are written for troubleshooting only;
you never need to locate or pass them manually.
USAGE
}

INPUT=""
OUT=""
SYNTHETIC=0
BLACK_LUMA=""
NEAR_BLACK=""
STRIDE=""
MAX_FRAMES=""
POSITIONAL=()

while [[ $# -gt 0 ]]; do
  case "$1" in
    --help|-h)
      usage
      exit 0
      ;;
    --synthetic)
      SYNTHETIC=1
      shift
      ;;
    --input|-i)
      [[ $# -ge 2 ]] || { echo "ERROR: --input requires a path" >&2; exit 2; }
      INPUT="$2"
      shift 2
      ;;
    --output|-o)
      [[ $# -ge 2 ]] || { echo "ERROR: --output requires a path" >&2; exit 2; }
      OUT="$2"
      shift 2
      ;;
    --black-luma)
      [[ $# -ge 2 ]] || { echo "ERROR: --black-luma requires an integer" >&2; exit 2; }
      BLACK_LUMA="$2"
      shift 2
      ;;
    --near-black)
      [[ $# -ge 2 ]] || { echo "ERROR: --near-black requires an integer" >&2; exit 2; }
      NEAR_BLACK="$2"
      shift 2
      ;;
    --stride)
      [[ $# -ge 2 ]] || { echo "ERROR: --stride requires an integer" >&2; exit 2; }
      STRIDE="$2"
      shift 2
      ;;
    --max-frames)
      [[ $# -ge 2 ]] || { echo "ERROR: --max-frames requires an integer" >&2; exit 2; }
      MAX_FRAMES="$2"
      shift 2
      ;;
    --)
      shift
      while [[ $# -gt 0 ]]; do POSITIONAL+=("$1"); shift; done
      ;;
    -*)
      echo "ERROR: unknown option: $1" >&2
      usage >&2
      exit 2
      ;;
    *)
      POSITIONAL+=("$1")
      shift
      ;;
  esac
done

if [[ $SYNTHETIC -eq 1 ]]; then
  if [[ -n "$INPUT" || ${#POSITIONAL[@]} -gt 0 ]]; then
    echo "ERROR: --synthetic cannot be combined with a media input" >&2
    exit 2
  fi
  cd "$ROOT/tools/cv-validation"
  echo "== AdSlicer OpenCV synthetic regression =="
  ./run_probe_suite.sh
  ./run_temporal_suite.sh
  ./run_boundary_suite.sh
  echo
  echo "Synthetic regression complete."
  exit 0
fi

# Backward-compatible positional form.
if [[ -z "$INPUT" && ${#POSITIONAL[@]} -ge 1 ]]; then
  INPUT="${POSITIONAL[0]}"
fi
if [[ -z "$OUT" && ${#POSITIONAL[@]} -ge 2 ]]; then
  OUT="${POSITIONAL[1]}"
fi
if [[ ${#POSITIONAL[@]} -gt 2 ]]; then
  echo "ERROR: too many positional arguments" >&2
  usage >&2
  exit 2
fi

if [[ -z "$INPUT" ]]; then
  usage >&2
  exit 2
fi
if [[ ! -f "$INPUT" ]]; then
  echo "ERROR: media file not found: $INPUT" >&2
  exit 2
fi

# Normalize the input to an absolute path before invoking Cargo so changing
# working directories can never make a valid media path disappear.
INPUT_DIR="$(cd "$(dirname "$INPUT")" && pwd)"
INPUT="$INPUT_DIR/$(basename "$INPUT")"

if [[ -z "$OUT" ]]; then
  STEM="$(basename "$INPUT")"
  STEM="${STEM%.*}"
  OUT="$ROOT/cv-test-output/$STEM"
elif [[ "$OUT" != /* ]]; then
  OUT="$(pwd)/$OUT"
fi

mkdir -p "$OUT"

if command -v pkg-config >/dev/null 2>&1 && pkg-config --exists opencv4 2>/dev/null; then
  echo "OpenCV: $(pkg-config --modversion opencv4)"
else
  echo "WARNING: pkg-config cannot currently see opencv4; Cargo will try its other discovery methods." >&2
fi

echo "Input:  $INPUT"
echo "Output: $OUT"
echo

PIPELINE_ARGS=(
  --input "$INPUT"
  --output "$OUT"
)
[[ -n "$BLACK_LUMA" ]] && PIPELINE_ARGS+=(--black-luma "$BLACK_LUMA")
[[ -n "$NEAR_BLACK" ]] && PIPELINE_ARGS+=(--near-black "$NEAR_BLACK")
[[ -n "$STRIDE" ]] && PIPELINE_ARGS+=(--stride "$STRIDE")
[[ -n "$MAX_FRAMES" ]] && PIPELINE_ARGS+=(--max-frames "$MAX_FRAMES")

cargo run \
  --manifest-path "$ROOT/src-tauri/Cargo.toml" \
  --example adslicer_cv_pipeline \
  --features opencv-analysis \
  -- "${PIPELINE_ARGS[@]}"

echo
echo "OpenCV test output: $OUT"
echo "Main report: $OUT/OPENCV_TEST_REPORT.txt"
echo "Machine report: $OUT/OPENCV_TEST_REPORT.json"
