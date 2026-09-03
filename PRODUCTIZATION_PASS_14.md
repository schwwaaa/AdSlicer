# AdSlicer Productization Pass 14 — ASCII Identity + Responsive Activity Log

## Goal

Improve the Activity Log identity and make the log use vertical space intelligently when the workflow disclosure sections are collapsed.

## Changes

- Replaced the boxed Activity Log startup banner with a compact ASCII `AdSlicer` wordmark.
- Removed the redundant startup instruction sentence; startup now resolves simply to `Ready.`
- Added responsive Activity Log sizing.
- The Activity Log now grows into genuinely available viewport space when Source, Analysis, Output, or Advanced / Compatibility are collapsed.
- Reopening workflow sections automatically returns space to those sections and reduces the log toward its readable baseline height.
- Log resizing is based on actual measured layout height rather than arbitrary per-tab increments.
- A `ResizeObserver` tracks workflow/processing layout changes, so Advanced content changes such as Legacy settings, Batch options, and Custom Output are also reflected.
- A native disclosure-toggle fallback remains for WebViews without `ResizeObserver` support.
- The Activity Log never shrinks below a 210 px readable baseline.

## Product rationale

The collapsible workflow tabs should reclaim usable workspace rather than leave dead space. The Activity Log is the most useful destination for that space because it is the running history and troubleshooting surface. This makes the interface adapt to whether the user is configuring the job or watching it run.

The ASCII startup wordmark gives the console a stronger terminal/broadcast identity without adding another decorative frame or changing the main application logo/header.

## Scope

UI only. No Rust, OpenCV, segmentation, export, progress, cancellation, or release-build behavior changed.
