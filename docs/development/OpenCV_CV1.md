# AdSlicer OpenCV Development — CV-1 Evidence Probe

**Product:** AdSlicer (free version)  
**Pass:** CV-1  
**Status:** Implementation candidate / runtime validation required on target Mac  
**Safety rule:** Existing FFmpeg detection, planner, and renderer are unchanged.
**Baseline:** Rebuilt from the user-supplied current `AdSlicer.zip` on 2026-08-16; this supersedes the earlier CV-1 artifact.

## Goal

Establish OpenCV inside the Rust/Tauri codebase and prove that it can produce a
reproducible per-frame evidence stream useful for VHS television boundary analysis.
CV-1 does **not** decide cuts. It measures frames so later milestones can build
adaptive black, near-black, fade-valley, color-slate, and temporal boundary logic.

## What changed

- Added optional Rust `opencv` dependency behind Cargo feature `opencv-analysis`.
- Added `src-tauri/src/adslicer/cv_detect.rs`.
- Added an OpenCV probe example at `src-tauri/examples/adslicer_cv_probe.rs`.
- Added SHA-256 source fingerprinting for reproducible evidence manifests.
- Added synthetic lossless validation media and automated checks under
  `tools/cv-validation/`.
- Existing `run_blackdetect`, `run_silencedetect`, `run_uniformdetect`,
  `run_scenechange`, `build_plan`, and export behavior remain untouched.

## Evidence captured per frame

- frame index,
- probe timestamp,
- dimensions,
- mean luma,
- luma standard deviation,
- min/max luma,
- strict-black pixel ratio,
- near-black pixel ratio,
- mean blue / green / red,
- mean HSV hue / saturation / value,
- mean absolute luma difference from the previous analyzed frame.

The evidence is written as both JSONL and CSV. The manifest records the source
path/name, source size, SHA-256 fingerprint, AdSlicer package version, OpenCV
version, reported FPS/dimensions/frame count, thresholds, and analyzed frame count.

## Why color evidence is already important

The synthetic `07_uniform_blue_slate.mkv` control intentionally demonstrates a
failure mode of luma-only logic: saturated blue has low grayscale luminance and can
fall inside a naive near-black threshold even though the picture is clearly blue.
Tracking saturation and channel means gives later boundary logic evidence to reject
that false interpretation.

This is directly relevant to broadcast/VHS material containing station slates,
solid color frames, tape artifacts, and transitions that are dark but chromatic.

## macOS setup

The Rust OpenCV bindings require a supported OpenCV installation and Clang. The
upstream bindings support OpenCV 4.x and 5.x; the selected crate version is 0.100.1.

Start with:

```bash
brew install opencv
rustc --version
```

`opencv` crate 0.100.1 requires Rust 1.88 or newer. If Rust is older:

```bash
rustup update stable
```

If discovery complains about Clang/pkg-config, install:

```bash
brew install llvm pkg-config
```

Useful diagnostics:

```bash
brew --prefix opencv
pkg-config --modversion opencv4
```

The dependency enables the crate's `clang-runtime`, `imgproc`, and `videoio`
features while disabling unnecessary default OpenCV modules.

## First runtime test

Generate deterministic media:

```bash
cd tools/cv-validation
./generate_synthetic_corpus.sh
```

Run all OpenCV probes:

```bash
./run_probe_suite.sh
```

Evaluate results:

```bash
python3 ./evaluate_probe_results.py
```

Expected target:

```text
CV-1 evidence checks: 15/15 passed
```

## Probe one real VHS excerpt

From `src-tauri/`:

```bash
cargo run --example adslicer_cv_probe --features opencv-analysis -- \
  "/path/to/vhs-test-clip.mov" \
  "/path/to/vhs-test-results"
```

Inspect:

```text
opencv_analysis_manifest.json
opencv_frame_metrics.jsonl
opencv_frame_metrics.csv
```

For initial real-media validation, use 10–30 second excerpts centered on known
commercial/show transitions. Do not start with a six-hour tape; first prove the
measurements on a small edge-case corpus.

## Recommended first real VHS set

1. Clean black commercial boundary.
2. Exactly one black frame if available.
3. Two-frame / short black separator.
4. Raised or noisy VHS black.
5. Rapid fade or fade-through-darkness.
6. Dark program scene that must not become a cut.
7. Tracking disturbance around an actual transition.
8. Saturated station/color slate.
9. Hard program/commercial cut with no black.
10. Interlaced/field-weird boundary.

Copy `tools/cv-validation/real_clip_annotation_template.json` for each sample so
expected behavior is recorded before tuning the detector.

## CV-1 pass criteria

CV-1 is accepted only when all of the following are true on the target development
machine:

1. Normal AdSlicer build/run still behaves as before.
2. OpenCV-enabled probe compiles and runs.
3. Synthetic suite produces frame evidence for every clip.
4. `evaluate_probe_results.py` reports 15/15.
5. At least 3 real VHS excerpts produce plausible metrics around known boundaries.
6. Evidence manifests are saved so results can be reproduced later.

## What CV-1 deliberately does not do

- It does not replace FFmpeg black detection.
- It does not alter `build_plan()`.
- It does not create automatic fade boundaries yet.
- It does not add AI/ML.
- It does not add the future Pro editor.
- It does not make OpenCV mandatory for the ordinary AdSlicer build yet.

## Next pass after CV-1 acceptance — CV-2

CV-2 should turn the evidence stream into the first reusable **Frame Evidence
Engine** integrated with AdSlicer's normal analysis job. Recommended changes:

- use FFmpeg as the authoritative media decoder/timestamp source,
- feed reduced-resolution decoded frames into OpenCV measurement code,
- persist evidence automatically under each AdSlicer run's `logs/` directory,
- add source/frame timing metadata robust enough for VFR/interlaced material,
- benchmark analysis cost on VHS-length sources,
- keep planning behavior unchanged.

CV-3 can then implement actual black/near-black frame scoring, and CV-4 can add
temporal fall → minimum → rise / fade-pattern recognition.

## Git commit message

```text
feat(cv): add feature-gated OpenCV frame evidence probe and validation corpus

- add optional OpenCV Rust integration for AdSlicer
- capture luma, color, saturation, black-ratio and frame-delta metrics
- persist reproducible CSV/JSONL evidence with source fingerprint manifest
- add synthetic one-frame black, near-black, luma-valley and false-positive tests
- keep existing FFmpeg detector, planner and renderer behavior unchanged
```

## Verification record

See `CV1_VERIFICATION.md` for what has already been exercised and what must still be
runtime-tested on the target Mac before this pass is accepted.
