# AdSlicer Productization Pass 01 — UI Reduction

**Baseline:** CV-7 Production Integration
**Scope:** Productization roadmap Steps 1–4

## Goal

Reduce the normal AdSlicer interface to the smallest automation-first workflow that exposes the validated CV-7 product without removing expert/compatibility capability.

AdSlicer is not becoming an internal video editor. The product goal is automatic structural segmentation that gets source recordings most of the way to usable edits without human timeline work.

## Completed in this pass

### 1. Reduce the UI

The primary workspace is now only:

- Source
  - Single Recording / Batch Folder
  - Recording/folder picker
  - Destination picker
- Analysis
  - Split Behavior
  - Analyze Only
  - Test Export
- Output
  - Separate Clips / Chaptered Recording
  - Preserve Video / Frame Accurate
- Start / Stop + Activity Log

The existing late-90s / Win98 visual system is preserved.

### 2. Hide Legacy under Advanced / Compatibility

Adaptive analysis is now presented as normal AdSlicer behavior rather than a technology choice.

Legacy remains available at:

`Advanced / Compatibility → Analysis Compatibility → Legacy Compatibility`

Selecting Legacy reveals the original black-frame, commercial-planning, evidence/guard, trim, loudness, and post-processing controls. Those controls are hidden during normal Adaptive use.

No Rust detector/planner/export code was changed in this pass.

### 3. Translate encoder controls into user intent

Normal UI:

- **Preserve Video (Fast)** — current proven CV-7 default; video stream copy with compatible AAC audio behavior.
- **Frame Accurate (H.264)** — high-quality H.264 re-encode so cuts are not constrained by source keyframes.
- **Custom (Advanced)** — exposes the original codec/GPU/CRF/audio/deinterlace/scale controls.

Changing an expert output control automatically marks the output profile as Custom.

### 4. Make defaults excellent

- Adaptive analysis
- Complete Segments
- Separate Clips
- Preserve Video (Fast)
- Analyze Only off
- Test Export off
- Activity Detail = Normal instead of Debug
- No threshold tuning visible in the normal workflow

The validated CV-7 rendering behavior is preserved by default; this pass does not silently change the successful detector or export path.

## Batch decision

Batch Folder is retained for 1.0 because the backend implementation already exists and it is particularly valuable for archival/institutional use.

It is deliberately secondary:

- One small Single Recording / Batch Folder choice remains visible.
- The filename/glob matching field is hidden under Advanced and appears only for Batch Folder mode.

## Development command

`package.json` was restored as a development convenience wrapper:

```bash
npm run dev
npm run build
npm run test:opencv
npm run setup:bins
```

`npm run dev` calls `./build.sh dev`, so the Homebrew OpenCV/libclang discovery added in CV-7 is still applied.

The `.sh` file is **not** a production-user launch requirement. Packaged users run the normal Tauri `.app` / installer executable.

## Product identity correction

Tauri `productName` is now `AdSlicer` instead of the stale `AdSlicerProXP` value.

## Explicitly deferred

These belong to later productization roadmap steps:

- Step 5: wider real-recording corpus validation
- Step 6: fix only reproducible failures discovered by that corpus
- Step 7: installation/update behavior and release packaging, including first-class Linux release packaging
- Step 8: concise user documentation and deeper optional technical documentation
- Step 9: perpetual-license + timed-update licensing implementation
- Step 10: AdSlicer 1.0 release packaging/signing
- Decision on long-term diagnostic artifact retention/cleanup after measuring real storage cost
- Further preset/menu cleanup; current preset compatibility is preserved

No transport/timeline editor is planned for 1.0.
