# CV-6 — Structural Segmentation Engine

## Goal

CV-6 changes OpenCV development from removal-pair testing to **full timeline segmentation**.
Every source frame must belong to exactly one output segment. A dark/fade event is first
measured as separator evidence, then classified by structural role, then interpreted by a
user-facing segmentation policy.

## Two policies

### Complete Segments

Conservative mode intended to preserve complete commercials, promos, station IDs, and
program pieces. Only structural separators are used automatically. Ambiguous separators
may be promoted when surrounding timeline duration context strongly supports the split.

### Every Separator

Diagnostic/archival mode. Uses all sufficiently strong separator-like events, including
ambiguous internal fades. This mode intentionally may split one commercial into multiple
pieces.

## Structural cues added in CV-6

- dark/near-black evidence
- within-event luma standard deviation (uniform blank vs structured dark image)
- before/after luminance regime
- before/after RGB/color regime
- saturation change
- texture/statistical change
- local frame-delta regime
- local shot-change density
- broadcast-duration context as a secondary grouping cue

A black/fade event is **not** automatically a commercial boundary.

## Full coverage invariant

For each mode:

```text
source start
  ↓
segment 001
  ↓ boundary
segment 002
  ↓ boundary
...
segment N
  ↓
source end
```

CV-6 records:

- source duration
- covered duration
- uncovered duration
- overlap duration
- coverage ratio
- PASS/FAIL coverage assertion

Unclassified or uncertain material never disappears; it remains part of a timeline segment.

## Stable test command

```bash
./test-opencv.sh --input "/path/to/video.mp4"
```

CV-6 runs automatically behind the existing unified command.

By default it renders both policies:

```text
06-structural-segmentation/
  complete_segments_plan.csv
  every_separator_plan.csv
  opencv_structural_events.csv
  opencv_structural_segmentation.json
  renders/
    complete-segments/
      segment-001.mp4
      ...
    every-separator/
      segment-001.mp4
      ...
```

To render only Complete Segments:

```bash
./test-opencv.sh --input "/path/to/video.mp4" --segment-render-mode complete
```

Other values: `every`, `both`, `none`.

## Current real-world hard target

The accepted ~99.23 second VHS/television validation clip contains seven top-level pieces.
Independent feature calibration places the six expected structural separators approximately
at:

```text
~3.1s
~18.4s
~33.5s
~64.2s
~79.6s
~94.9s
```

Dark structured regions near ~20–23s should remain internal content. The dark/fade event
near ~75s is intentionally a useful ambiguous case: Complete Segments should preserve the
larger piece while Every Separator may expose the additional split.

Expected Complete Segments result for that sample:

```text
7 clips
6 boundaries
100% source coverage
0 missing residual
0 overlap
```

## Production safety

CV-6 remains a validation/shadow subsystem. It does not modify the production AdSlicer
`job.rs`, `detect.rs`, `cut.rs`, `models.rs`, or frontend behavior. Full-segment renders are
disposable validation media only.
