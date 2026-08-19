# AdSlicer OpenCV Milestones

## CV-0 — Stable Baseline
Freeze representative current FFmpeg results and known misses/false positives.

## CV-1 — OpenCV Evidence Probe
Feature-gated OpenCV build, per-frame metrics, manifests, synthetic controls, and
real VHS sample collection. **Current pass.**

## CV-2 — Production Frame Evidence Engine
FFmpeg-authoritative decode/timestamps → OpenCV measurements → cached evidence in
normal AdSlicer analysis runs. No planner changes yet.

## CV-3 — Frame-First Black / Near-Black Events
Preserve one-frame/two-frame events; strict black vs near-black; noisy-black score;
color/saturation safeguards; no duration-first gate.

## CV-4 — Temporal Boundary Analysis
Analyze neighborhoods of frames. Add fall/minimum/rise recognition, rapid fades,
hysteresis, micro-bridging, and scene discontinuity context.

## CV-5 — FFmpeg vs OpenCV A/B Evaluation
Run legacy and adaptive paths on the same corpus. Track recall, precision, false
positives, frame error, and runtime.

## CV-6 — VHS Regression Corpus
Expand real annotated clips across eras, stations, tape quality, interlace behavior,
tracking errors, slates, dark scenes, and different commercial transition styles.

## CV-7 — Planner Integration
Introduce detector-neutral `BoundaryEvent` / `BoundaryEvidence` structures and let
the existing planner consume validated OpenCV events without rewriting rendering.

## CV-8 — Source-Adaptive Calibration
Estimate tape-specific black floor / near-black range and preserve the calibration in
the run manifest for exact workflow recreation.

## CV-9 — Field-Aware Experiment
Measure whether top/bottom-field analysis materially improves real interlaced VHS
boundary accuracy. Keep only if the corpus proves value.

## CV-10 — OpenCV Primary Boundary Engine
Promote the adaptive engine after regression results prove it; retain legacy FFmpeg
black detection as fallback/corroboration.
