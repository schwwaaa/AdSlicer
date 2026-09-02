# AdSlicer — Productization Plateau Release Snapshot

This release preserves the current working AdSlicer product plateau after the OpenCV Adaptive integration and subsequent real-world broadcast refinements.

## Highlights

- OpenCV Adaptive structural segmentation is the normal AdSlicer workflow.
- Complete Segments and Every Separator modes are available.
- Full-source coverage validation prevents footage from silently disappearing.
- Raised VHS black recovery improves difficult analog transitions without globally loosening black thresholds.
- Broadcast-edge recovery improves difficult commercial / promo / credits boundaries observed in real 1990s television material.
- Long jobs expose live stage, progress, elapsed time, processing rate, and ETA.
- Stop now performs real backend cancellation and reports STOPPING → STOPPED accurately.
- The interface has been reduced to an automation-first workflow while retaining advanced compatibility controls when needed.
- Batch processing remains available for archive-scale workflows.
- macOS release builds now default to the native machine architecture so Apple Silicon releases correctly link against the validated arm64 OpenCV installation.

## Product direction

AdSlicer is intentionally an automatic segmentation utility rather than a general-purpose editor. The goal is to do the majority of repetitive structural cutting automatically, preserve source coverage, and leave genuinely ambiguous editorial decisions to downstream tools.

Development can remain conservative from this plateau: real failures should drive detector changes, while packaging, documentation, licensing, and operating-system validation lead toward AdSlicer 1.0.
