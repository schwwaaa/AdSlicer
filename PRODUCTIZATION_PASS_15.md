# AdSlicer Productization Pass 15 — True Flexible Activity Log

## Goal

Make the Activity Log continuously fill all unused vertical application space.
There should be no large gray dead region below the log as the window is resized
or workflow sections are collapsed.

## Change

Pass 14 used JavaScript to measure the viewport and assign a pixel height to the
Activity Log. Pass 15 removes that calculation and makes the layout native flex:

- the application layout consumes the available center-window height;
- Source / Analysis / Output / Advanced / Processing keep their natural heights;
- Activity Log uses `flex: 1` and absorbs every remaining pixel;
- collapsing a workflow section immediately enlarges the Activity Log;
- expanding a section immediately gives that space back;
- resizing the application continuously resizes the log without waiting for JS;
- a 210 px minimum keeps the log usable in short windows;
- when the controls plus minimum log exceed the viewport, the workflow column
  scrolls instead of allowing the log to collapse to nothing.

## Scope

UI layout only. No Rust, OpenCV, detection, segmentation, export, progress,
cancellation, or job-processing behavior changed.
