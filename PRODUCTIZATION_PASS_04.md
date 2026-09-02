# AdSlicer Productization Pass 04 — Reliable Stop / Cancellation

## Goal

Fix the Productization Pass 03 bug where pressing Stop changed the frontend state but did not stop the active OpenCV analysis. Late progress events could then make the UI appear to resume processing.

## Behavior

The normal Adaptive production path now follows this contract:

1. User presses **Stop**.
2. UI enters **STOPPING** and remains visibly active while cancellation is being acknowledged.
3. Frontend ignores late progress/batch telemetry queued before cancellation.
4. OpenCV frame analysis checks cancellation every frame.
5. Source hashing and frame-evidence writing check cancellation repeatedly.
6. Production planning checks cancellation between analysis stages.
7. Active FFmpeg structural-segment or chapter export is polled every 100 ms; cancellation kills the child process and removes the incomplete current output file.
8. Backend emits a distinct `cancelled` terminal state only after processing has actually ended.
9. UI enters **STOPPED** only after backend confirmation.
10. A new job cannot be started while the old backend job is still stopping.

## Detection Safety

No OpenCV detection thresholds, temporal rules, boundary scoring, structural classification, segmentation policy, or coverage logic were changed.

The changes are cancellation plumbing and process-lifecycle behavior only.

## Legacy Compatibility

Legacy remains a hidden compatibility mode. The production cancellation work in this pass is targeted at the normal OpenCV Adaptive path. Batch cancellation also stops the batch instead of continuing to later files.
