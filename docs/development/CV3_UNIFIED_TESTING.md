# CV-3 Unified Testing Contract

## Goal

OpenCV validation must not require the tester to locate CV-1 JSONL output, feed it into CV-2, then locate CV-2 JSON and feed it into CV-3.

The stable validation interface is one command against source media:

```bash
./test-opencv.sh --input "/path/to/video.mp4"
```

All current stages run automatically:

```text
source media
  -> CV-1 frame evidence
  -> CV-2 temporal evidence
  -> CV-3 shadow boundary candidates
  -> OPENCV_TEST_REPORT.txt / .json
```

The source is decoded once. CV-2 and CV-3 consume the in-memory results. Intermediate evidence files are still written for reproducibility and troubleshooting but are not user inputs.

## Stable CLI

Recommended form:

```bash
./test-opencv.sh --input "/path/to/video.mp4"
```

Custom output:

```bash
./test-opencv.sh \
  --input "/path/to/video.mp4" \
  --output "/path/to/output"
```

Optional CV-1 overrides:

```bash
./test-opencv.sh \
  --input "/path/to/video.mp4" \
  --black-luma 16 \
  --near-black 32 \
  --stride 1
```

Short probe:

```bash
./test-opencv.sh --input "/path/to/video.mp4" --max-frames 900
```

Synthetic regression:

```bash
./test-opencv.sh --synthetic
```

Help:

```bash
./test-opencv.sh --help
```

The old positional shorthand remains supported:

```bash
./test-opencv.sh "/path/to/video.mp4"
```

## Environment setup

The wrapper attempts to discover Homebrew `opencv@4` / `opencv` and Homebrew LLVM automatically. A new Terminal session should not require manually re-exporting `PKG_CONFIG_PATH` or `LIBCLANG_PATH` when those packages are installed through Homebrew.

## Output

Default output is predictable:

```text
cv-test-output/<source-stem>/
  OPENCV_TEST_REPORT.txt
  OPENCV_TEST_REPORT.json
  01-frame-evidence/
  02-temporal-events/
  03-boundary-candidates/
```

The top-level report is the primary validation artifact. The intermediate folders are diagnostic artifacts only.

## Contract for future CV stages

Future stages must be added behind this same command. Do not create a new manual file-to-file testing ladder.

```bash
./test-opencv.sh --input "/path/to/video.mp4"
```

should remain valid as CV-4, CV-5, and later stages are added.
