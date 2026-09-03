# AdSlicer Productization Pass 11 — Transport Consolidation

## Goal

Reduce vertical space in the Processing section while making it feel more like a single retro transport/status instrument.

## Changes

- Moved the live progress bar into the same horizontal transport row as Analyze & Export, Stop, and the runtime state indicator.
- Moved Elapsed / ETA / Rate directly underneath the progress bar inside that same transport module.
- Kept READY / current stage and WAITING / ACTIVE state in the compact Processing header above the transport.
- Converted the three large timing boxes into one compact sunken metric strip with three cells.
- Kept the existing progress IDs and event bindings unchanged, so all progress values remain driven by the same real backend telemetry.
- Added responsive wrapping so the transport remains usable at narrower window sizes without crushing the progress display.
- Preserved the existing retro Windows-style controls and bevel treatment.

## Behavior

No detection, segmentation, rendering, cancellation, batch, ETA calculation, or logging behavior changed in this pass.

This pass is presentation-only.
