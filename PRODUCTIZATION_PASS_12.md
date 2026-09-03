# AdSlicer Productization Pass 12 — UI System Review

This pass treats the application as one visual system rather than continuing local layout nudges.

## Goals

- Improve readability without abandoning the retro Windows/broadcast identity.
- Standardize spacing and control geometry.
- Remove redundant explanatory prose from the primary workflow.
- Remove the standalone `Processing` label.
- Make one processing state authoritative instead of showing several competing labels.
- Preserve real progress, ETA, elapsed time, rate, Stop, Batch, Simple/Verbose logging, and all backend behavior.

## UI system

- Spacing scale: 4 / 8 / 12 / 16 px.
- Base functional text: 13 px.
- Main processing state: 15 px bold.
- Metric values: 13 px.
- Metric labels: 10 px minimum.
- Controls: 30 px standard height.
- Primary transport controls: 58 px desktop height.
- Activity Log: 17 px console text.

## Processing redesign

The separate `Processing` group label is removed. The processing shell itself is the instrument.

Visible state is now:

- one status LED;
- one authoritative stage label;
- optional animated work pips;
- one percentage/progress value;
- transport buttons;
- progress bar;
- elapsed / ETA / rate.

Internal elements used by JavaScript for progress bookkeeping remain in the DOM but redundant text is visually hidden.

## Copy reduction

Removed persistent primary-UI paragraphs such as:

- `Automatic analysis — no threshold tuning required.`
- `Fast default. Preserves the source video stream whenever possible.`
- `Adaptive is the normal AdSlicer analysis path...`

The existing control names and tooltips preserve contextual explanations without permanently consuming interface space.

## Scope

No Rust backend, detector, segmentation, export, cancellation, or progress-event logic was changed.
