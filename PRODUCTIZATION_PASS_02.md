# AdSlicer Productization Pass 02 — Live Progress + UI Reduction

## Goal

Make long-form processing visibly trustworthy without changing segmentation behavior.

AdSlicer is automation-first. A user should be able to hand it a long recording and immediately understand:

- what stage is running,
- how far the long frame scan has progressed,
- how long the current scan is likely to take,
- whether AdSlicer is finalizing, detecting boundaries, building segments, or exporting,
- and when the job is actually complete.

## UI reduction

The normal workflow remains:

1. Source
2. Analysis
3. Output
4. Analyze & Export

Changes in this pass:

- Removed redundant documentation links beneath the logo; Help remains the documentation entry point.
- Retained presets and deep compatibility features without promoting them into the normal workflow.
- Kept Legacy hidden under Advanced / Compatibility.
- Kept codec-level controls hidden under Advanced / Compatibility unless Custom output is selected.
- Selecting Custom output automatically opens Advanced / Compatibility.
- Primary action now reads `Analyze & Export`, or `Analyze Only` when Analyze Only is checked.
- Kept the existing retro Windows-era visual language.

## Live progress panel

The right side now contains a persistent job progress panel.

During the long OpenCV frame-analysis stage it reports real backend telemetry:

- Stage 1 — Analyzing Recording
- current source position / estimated source duration
- decoded frames / total reported frames
- percentage
- measured frame-processing rate
- elapsed job time
- rolling analysis ETA

Progress events are throttled to roughly two per second so UI telemetry does not flood the WebView.

When the frame scan completes, the UI moves to `Finalizing Analysis` while source verification and evidence finalization occur. It then reports:

- Stage 2 — Detecting Boundaries
- Stage 3 — Building Segments
- Stage 4 — Exporting Clips
- Complete

For Analyze Only, the job is presented as three stages because export is skipped.

Clip export reports `X of Y clips finished`. Batch mode additionally reports `Batch X of Y` and the current source file.

## ETA semantics

The progress bar does not invent an overall percentage for phases that cannot be measured reliably.

- Frame analysis: exact reported frame progress + measured rolling ETA.
- Boundary/segment planning: named indeterminate stage.
- Separate-clips export: completed clip count and an ETA inferred from actual completed-export rate.
- Chaptered output: named indeterminate export stage.

This avoids the common misleading behavior where a progress bar reaches 95% and then appears stuck during an unknown-length render.

## Detection safety contract

This pass does **not** change:

- OpenCV black/near-black thresholds
- raised-VHS-black behavior
- temporal event rules
- boundary scoring
- structural role classification
- Complete Segments behavior
- Every Separator behavior
- full-coverage validation
- Legacy detection behavior

`cv_detect.rs` is touched only to expose frame-loop progress telemetry while retaining the same per-frame evidence calculations.

## Development / runtime note

The new progress UI only applies to jobs started by this build. It cannot attach to a job that was already running in a previous build.

Production users still launch the packaged application normally. `npm run dev` remains the development command and continues to route through the OpenCV-aware build wrapper.
