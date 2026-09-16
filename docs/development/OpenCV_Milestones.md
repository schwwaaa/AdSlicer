# AdSlicer OpenCV Development Milestones

## CV-0 — Stable Baseline
Freeze representative current FFmpeg results and known misses/false positives.

## CV-1 — OpenCV Frame Evidence Probe — ACCEPTED
Feature-gated OpenCV build, per-frame luminance/color/black/near-black/frame-delta
metrics, reproducible manifests, synthetic controls, and real VHS validation.

Accepted runtime results on the development Mac:

- OpenCV 4.14.0 discovered through the Rust bindings.
- 15/15 synthetic evidence checks passed.
- Real 99.2-second television/VHS-derived sample: 2,974/2,974 frames analyzed.
- Existing AdSlicer detector/planner/render behavior unchanged.

## CV-2 — Temporal Evidence Engine — CURRENT PASS
Interpret cached CV-1 frame metrics without re-decoding the video. Preserve temporal
observations as explicit events:

- strict-black intervals,
- one/two-frame black events,
- raised/near-black events,
- fall → valley → rise fade shapes,
- micro-bridged dark runs,
- chromatic-dark diagnostics,
- scene discontinuities,
- entry/exit frame-delta evidence.

CV-2 **does not create commercial intervals and does not change `build_plan()`**.

## CV-3 — Boundary Candidates + Evidence Strength
Translate selected temporal events into detector-neutral boundary candidates. Add
boundary timestamps, reason codes, evidence strength, ambiguity/review risk, and
explicit separation between observation confidence and commercial-cut policy.

## CV-4 — Shadow Edit Planner
Run the validated OpenCV boundary path beside the current FFmpeg detector. Generate
an OpenCV proposed segmentation plan for logs/A-B comparison while the legacy plan
remains authoritative for rendering.

## CV-5 — OpenCV Dry-Run Plan
Allow OpenCV boundary candidates to drive an explicit dry-run plan. Compare proposed
keep/remove ranges against annotated VHS material before enabling rendering.

## CV-6 — OpenCV Plan Rendering
Feed an approved OpenCV-derived plan into AdSlicer's existing FFmpeg rendering path.
OpenCV determines evidence/boundaries; FFmpeg remains the media cutting/encoding layer.

## CV-7 — VHS Regression Corpus Expansion
Grow the real annotated corpus across eras, stations, tape quality, interlace behavior,
tracking errors, slates, dark scenes, fades, and different commercial transition styles.
Track precision, recall, false positives, frame error, and runtime.

## CV-8 — Source-Adaptive Calibration
Estimate tape-specific black floor / near-black range and preserve the calibration in
the run manifest for exact workflow recreation.

## CV-9 — Field-Aware Experiment
Measure whether top/bottom-field analysis materially improves real interlaced VHS
boundary accuracy. Keep only if the corpus proves value.

## CV-10 — OpenCV Primary Boundary Engine
Promote the adaptive engine after regression results prove it; retain legacy FFmpeg
black detection as fallback/corroboration.
