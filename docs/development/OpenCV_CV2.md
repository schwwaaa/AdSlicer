# CV-2 — Temporal Evidence Engine

## Purpose

CV-1 proved that AdSlicer can extract trustworthy per-frame OpenCV measurements from
real television/VHS-derived media. CV-2 answers the next question:

> Can those measurements be interpreted as meaningful temporal patterns without yet
> changing a single AdSlicer edit?

The answer must be proven before any OpenCV result is allowed to drive `build_plan()`.

## Key architecture

```text
video
  ↓
CV-1 OpenCV frame measurements
  ↓
opencv_frame_metrics.jsonl
  ↓
CV-2 Temporal Evidence Engine
  ↓
opencv_temporal_events.json / .csv
  ↓
(no planner integration yet)
```

A major design goal is **analyze once, reinterpret repeatedly**. CV-2 consumes the
cached JSONL evidence and does not decode the source video again.

## Event types

CV-2 currently preserves these observations:

- `black_interval`
- `black_separator`
- `near_black_interval`
- `near_black_separator`
- `fade_valley`
- `chromatic_dark`
- `scene_discontinuity`

These labels describe visual evidence. They are **not commercial classifications**.
A `scene_discontinuity`, for example, will be common inside normal programs and ads.

## Temporal measurements preserved per event

- start / valley / end frame and timestamp,
- event duration,
- minimum mean luma,
- maximum strict-black ratio,
- maximum near-black ratio,
- mean saturation,
- maximum RGB/BGR channel spread,
- chromatic-dark flag,
- local luma mean before/after,
- fall/rise magnitude,
- number of meaningful descending/ascending steps,
- entry frame delta,
- exit frame delta,
- peak frame delta,
- human-readable evidence strings.

## Current default rules

These defaults are intentionally conservative development parameters, not final VHS
profiles:

```text
near-black ratio           >= 0.90
strict-black ratio         >= 0.90
raised-black mean luma     <= 32
micro bridge               <= 2 frames
context                    5 frames each side
fade step                  >= 5 luma
fade shape                 >= 2 falling + 2 rising steps
fade fall/rise magnitude   >= 20 luma
scene discontinuity        >= 40 mean frame delta
scene suppression          5 frames around dark events
short dark event           <= 2 frames
chromatic saturation       >= 120
chromatic channel spread   >= 40
```

The thresholds live in `TemporalAnalysisConfig` so later VHS profiles and calibration
can change them without changing the event schema.

## Synthetic reference result

The development-only Python mirror of the CV-2 rules currently produces:

```text
[PASS] clean black          → black_interval 90–119
[PASS] single black frame   → black_separator 90
[PASS] two-frame near-black → near_black_separator 90–91
[PASS] temporal valley      → fade_valley 92–95, valley 94
[PASS] dark textured video  → no temporal-dark event
[PASS] hard cut / no black  → scene_discontinuity 90
[PASS] blue low-luma slate  → chromatic_dark 90–104

CV-2 reference temporal checks: 7/7 passed
```

The Rust implementation must still be runtime-validated on the development Mac before
CV-2 is accepted.

## Real-sample calibration observation

Using the accepted CV-1 metrics from the 99.2-second / 2,974-frame real television
sample, the same reference rules currently produce 41 observations:

- 7 `near_black_interval`
- 1 `fade_valley`
- 33 `scene_discontinuity`

This is **not a segmentation score**. The many scene discontinuities are expected in
normal edited television. CV-3 will decide which combinations of temporal evidence are
credible boundary candidates rather than treating every scene cut as an edit point.

## Runtime validation

First make sure CV-1 results exist:

```bash
cd tools/cv-validation
./run_probe_suite.sh
```

Then run the Rust temporal suite:

```bash
./run_temporal_suite.sh
```

Expected final result:

```text
CV-2 temporal checks: 7/7 passed
```

## Analyze the already-generated real VHS evidence

CV-2 does not need the source video again. Point it directly at the CV-1 JSONL file:

```bash
cargo run \
  --manifest-path src-tauri/Cargo.toml \
  --example adslicer_cv_temporal \
  -- "/path/to/opencv_frame_metrics.jsonl" \
     "cv-real-temporal"
```

Outputs:

```text
cv-real-temporal/
├── opencv_temporal_events.json
├── opencv_temporal_events.csv
└── opencv_temporal_summary.json
```

## Safety boundary

CV-2 intentionally does **not** modify:

- `detect.rs`
- `cut.rs`
- `job.rs`
- `models.rs`
- frontend behavior
- FFmpeg render behavior

No OpenCV temporal event can change an AdSlicer edit in this pass.
