# AdSlicer Productization Pass 06 — Broadcast Edge Recovery

## Purpose

Fix real Complete Segments misses discovered in long-form 1995 WCPX/CBS material without increasing global black sensitivity or changing the successful CV-6.1a raised-VHS-black detector.

## Changes

- Added candidate-to-candidate **broadcast cadence recovery** for strong ambiguous separators.
- Added **structured-dark entry/exit recovery** for credits, dark title cards, and similar mostly-black broadcast material.
- Added a low cadence context floor so the existing synthetic same-scene internal black fade remains protected.
- Added entry/exit delta values to structural diagnostics.
- Added promotion counts to structural summaries.
- Bumped structural diagnostic version to `cv6.2-broadcast-edge-recovery-v1`.
- Added an optional six-clip real-media regression runner/evaluator.

## Explicitly unchanged

- CV-1 frame thresholds
- CV-2 temporal rules / raised-uniform VHS-black recovery
- CV-3 diagnostic boundary scoring
- Every Separator selection semantics
- full-coverage invariant
- FFmpeg rendering behavior
- UI / progress / Stop behavior from Productization Pass 05
- Legacy detector

## Runtime validation

On the validated macOS/OpenCV development machine:

```bash
npm run test:opencv
```

Then, with the six supplied problem clips in one folder:

```bash
./test-problem-clips.sh "/path/to/WCPX-problem-clips"
```

Finally run the normal application against the longer original recording and compare the newly generated segment set.
