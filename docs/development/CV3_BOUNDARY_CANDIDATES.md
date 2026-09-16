# CV-3 — Shadow Boundary Candidates

## Purpose

CV-3 is the bridge between CV-2 temporal evidence and future edit planning.
It **does not change AdSlicer's active cuts**.

The temporal engine can report many observations in ordinary television footage.
CV-3 ranks those observations and separates likely boundary points from low-value
evidence without treating every scene cut as an edit.

## Core rule

`boundary_score` is a **versioned heuristic score**, not a probability.

Current score version:

```text
cv3-heuristic-v1
```

This makes score changes reproducible and prevents a development heuristic from
being presented as statistically calibrated confidence.

## Candidate roles

- `separator_anchor` — one/two-frame black or near-black separator
- `fade_valley` — local temporal luminance valley
- `interval_entry` — transition into a longer black/near-black interval
- `interval_exit` — transition out of a longer black/near-black interval
- `scene_only` — scene discontinuity with no dark/fade corroboration
- `chromatic_dark` — low-luma event that retains strong color structure

The final two roles are expected to remain evidence-only by default.

## Tiers

Default thresholds:

```text
strong        >= 0.80
review        >= 0.60
weak          >= 0.45
evidence_only <  0.45
```

These are development thresholds and can be changed later using the versioned
configuration.

## Outputs

```text
opencv_boundary_candidates.json
opencv_boundary_summary.json
opencv_boundary_candidates.csv
opencv_boundary_evidence_only.csv
```

The JSON preserves the reasons and cautions used to produce each score.

## Synthetic expectations

CV-3 is designed so the synthetic corpus produces:

- clean black interval → entry + exit candidates
- one-frame black → strong separator candidate
- two-frame near-black → separator candidate
- temporal fade valley → fade candidate
- dark textured program content → no candidate
- hard cut without black → scene evidence only
- uniform blue low-luma slate → chromatic evidence only

## Real reference sample

Using the previously validated 2,974-frame television sample as a development
reference, the independent scoring mirror yields:

```text
41 temporal events
48 scored observations
12 boundary candidates
36 evidence-only observations

candidate tiers:
  strong  1
  review  2
  weak    9
```

This is deliberately conservative. The point of CV-3 is to reduce the much
larger temporal event stream into a manageable set of candidate edit points.

## Safety boundary

CV-3 does not modify:

- `detect.rs`
- `cut.rs`
- `job.rs`
- `models.rs`
- frontend behavior
- render behavior

CV-4 will compare these shadow candidates against the existing FFmpeg-derived
plan before OpenCV is allowed to drive an active dry-run plan.
