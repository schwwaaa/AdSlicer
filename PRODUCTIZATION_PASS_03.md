# AdSlicer Productization Pass 03 — Status Clarity + Pre-Run Estimate

## Goal

Make long-form processing self-explanatory while continuing to reduce the normal interface.

This pass does not change detection or segmentation behavior. It improves the information AdSlicer exposes before, during, and after a run.

## Activity Log

The Activity Log now has two user-facing modes:

- **Simple** — default. Shows the milestones most users need: job start, batch/file transitions, analysis stages, boundary count, segment count, coverage validation, export result, warnings/errors, and completion.
- **Verbose** — troubleshooting/validation view. Retains the existing detailed detector, FFmpeg, environment, and processing messages.

The old Quiet / Info / Debug presentation was removed from the visible UI.

The `CLR` control is larger and visually prominent. Clearing the on-screen Activity Log does not delete saved diagnostics.

## Live / still-working state

The progress panel now uses multiple independent signals so a long operation does not look frozen:

- animated determinate progress stripes while measurable work is running,
- animated indeterminate bar for stages without an honest percentage,
- pulsing status LED,
- three-cell activity meter,
- continuously increasing elapsed time,
- `ACTIVE — RECEIVING UPDATES` while telemetry is arriving,
- `ACTIVE — STILL WORKING` when a stage has not emitted a new milestone recently.

The intent is deliberate redundancy: if a two-hour recording takes a long time, the user can still immediately tell that the application is alive.

## Pre-run time estimate

Selecting a single recording now triggers a lightweight `ffprobe` duration probe. This is metadata-only; it does not decode/analyze frames.

Before processing starts AdSlicer shows:

- source duration,
- a broad initial **analysis-only** time range,
- a note that export time is additional,
- whether the selected output path is likely to add a copy pass or a potentially substantial re-encode.

The initial estimate uses a deliberately conservative 3–6× realtime analysis range because no machine-specific throughput has been measured yet.

After the job starts:

1. the initial estimate remains visible while AdSlicer calibrates,
2. after several seconds / enough frames, the UI switches to **LIVE ETA**,
3. the live ETA is based on actual measured OpenCV processing throughput for the current recording and machine.

This prevents a duration-based guess from being presented as measured truth.

## Batch behavior

Batch remains part of the 1.0-oriented interface but stays secondary.

- Before start, AdSlicer does **not** invent a total batch ETA without probing every recording.
- During processing, the progress panel shows `BATCH X OF Y` and the current file.
- Each recording receives its own frame-analysis ETA.
- Completing one file no longer makes the UI look as though the whole batch has finished.
- Per-file errors are surfaced while the batch continues to the remaining files.
- The Simple Activity Log records file transitions and the final processed/error summary.

## UI reduction

Additional productization cleanup:

- Removed the top-level **Presets** menu. Preset Save/Load remains available under File.
- Removed the Advanced diagnostics verbosity selector; Activity Log mode now lives directly on the Activity Log itself.
- Removed unimplemented `Open Output Folder` / `Open Session Log` View items instead of shipping visible dead controls.
- View now contains only the two Activity Log display modes.
- Legacy and expert encoder settings remain hidden under Advanced / Compatibility.
- The existing retro Windows-era visual language is preserved.

## Detection safety contract

Compared with Productization Pass 02, the following files are byte-identical:

- `src-tauri/src/adslicer/cv_detect.rs`
- `src-tauri/src/adslicer/cv_temporal.rs`
- `src-tauri/src/adslicer/cv_boundary.rs`
- `src-tauri/src/adslicer/cv_structural.rs`
- `src-tauri/src/adslicer/cv_production.rs`
- `src-tauri/src/adslicer/detect.rs`
- `src-tauri/src/adslicer/models.rs`
- `src-tauri/src/adslicer/job.rs`

The only Rust change is a small Tauri command in `src-tauri/src/main.rs` that asks bundled `ffprobe` for source duration before a run.

## Runtime validation

This packaging environment does not include the Rust/Cargo toolchain, so the final Tauri/OpenCV compile remains a validation step on the established macOS development machine.

Recommended:

```bash
npm run test:opencv
npm run dev
```

Then validate:

1. select a long single recording,
2. confirm a pre-run estimate appears,
3. start the job,
4. confirm the estimate changes to measured LIVE ETA,
5. confirm the bar/heartbeat remains visibly active during long stages,
6. confirm Simple Activity contains only major milestones,
7. toggle Verbose and confirm detailed troubleshooting output,
8. confirm CLR visibly clears the log,
9. run a small batch and confirm file-to-file state does not report premature overall completion.
