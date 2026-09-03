# AdSlicer Productization Pass 13 — Workflow Tabs

## Goal

Make the three primary workflow groups behave like the existing Advanced / Compatibility disclosure panel without changing application behavior.

## Changes

- Source is now a collapsible disclosure section.
- Analysis is now a collapsible disclosure section.
- Output is now a collapsible disclosure section.
- Source, Analysis, and Output are all expanded by default when AdSlicer opens.
- Advanced / Compatibility remains collapsed by default.
- The primary workflow sections reuse the same retro disclosure styling and native keyboard-accessible `<details>/<summary>` behavior as Advanced.
- Existing controls, IDs, defaults, event bindings, detector behavior, export behavior, progress telemetry, and cancellation behavior are unchanged.

## Product rationale

The normal workflow remains visible immediately on launch, but experienced users can collapse sections they have already configured to reclaim vertical space. This keeps the primary workflow approachable while allowing the application to become progressively more compact during use.
