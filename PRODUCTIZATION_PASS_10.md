# AdSlicer Productization Pass 10 — Ready/Progress Consolidation

## Goal
Reduce vertical UI waste in the Processing section without changing analysis, segmentation, export, progress telemetry, or cancellation behavior.

## Changes
- Kept `READY` as the primary idle state.
- Moved the animated WAITING / READY TO RUN / ACTIVE indicator into the top-right of the READY/progress header.
- Kept the live percentage in the same compact top-right header area while processing.
- Removed the standalone instructional text: `Choose a recording and press Analyze & Export.`
- Removed the standalone estimate/explanation panel: `Select a recording to calculate an initial time estimate.`
- Reused the existing metrics row for pre-run timing: `EST. ETA` before analysis, then `ETA` once processing begins.
- Kept useful contextual detail only when there is actual information to show, such as recording duration, frame position, batch file, completion, or error state.
- Preserved the animated progress bar, work pips, elapsed time, ETA, rate, batch indication, Simple/Verbose Activity Log, Stop behavior, and retro visual language.

## Safety
This pass is UI-only. Rust processing sources are byte-identical to Productization Pass 09.
