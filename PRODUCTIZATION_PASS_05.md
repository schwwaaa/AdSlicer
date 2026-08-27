# AdSlicer Productization Pass 05 — Stop Acknowledgement + Single-Column Layout

## Purpose

Fix the remaining cancellation UI failure and reduce the application to a compact single-column workflow.

## Stop fix

Pass 04 added real cancellation checks inside OpenCV and FFmpeg work, but the compiled Tauri entrypoint (`src-tauri/src/main.rs`) did not emit the terminal cancellation acknowledgement expected by the frontend. A stale unused file (`src-tauri/src/adslicer/main.rs`) contained that logic, which made the implementation appear complete during source inspection while the compiled app could remain visually stuck in STOPPING.

Pass 05:

- emits `[cancelled] Job stopped by user` from the actual compiled entrypoint;
- emits an `adslicer-progress` event with stage `cancelled` only after `run_job` has returned;
- keeps STOPPING visible while the worker is still unwinding;
- transitions to STOPPED only after backend acknowledgement;
- removes the stale unused duplicate `src-tauri/src/adslicer/main.rs` to prevent future ambiguity.

During normal local-file OpenCV analysis, Stop should acknowledge within roughly a frame/decode cycle plus teardown, normally well under a second or a few seconds. It is not designed to wait for the entire analysis stage.

## Layout reduction

The former two-column split has been replaced with one centered workflow:

1. Source
2. Analysis
3. Output
4. Advanced / Compatibility
5. Processing
6. Activity Log

Advanced expands downward only when needed. Processing no longer occupies a permanently separate right-side column. The product identity header is compact, and Activity Log has a finite height rather than stretching to consume unused space.

The retro Win98/broadcast visual language is retained.

## Pre-run estimate

The existing Pass 03 behavior remains:

- selecting a single recording runs a metadata-only ffprobe;
- duration produces a deliberately broad initial estimate before Start;
- no OpenCV analysis is performed for this estimate;
- after analysis begins and enough frames have been measured, the pre-run estimate is replaced by live throughput-derived ETA.

Batch continues to show per-file progress rather than inventing an unreliable whole-batch ETA before all media has been measured.
