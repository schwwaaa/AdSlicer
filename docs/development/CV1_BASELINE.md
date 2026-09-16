# CV-1 Authoritative Baseline Record

**Date:** 2026-08-16  
**Baseline source:** user-supplied `AdSlicer.zip`  
**Baseline Git HEAD:** `12ed5ea` — `updated ui of website`

This CV-1 artifact supersedes the earlier CV-1/CV-2 artifacts that were built from an older AdSlicer snapshot.

## Existing working-tree state preserved

The supplied baseline already contained these Git working-tree differences before CV-1 was reapplied:

```text
 M src-tauri/.DS_Store
 M src/main.js
?? .DS_Store
?? package-lock.json
```

CV-1 does not reset, replace, or reinterpret those changes.

## Intentional CV-1 production-code changes

Only these existing production files are intentionally modified:

```text
src-tauri/Cargo.toml
src-tauri/src/adslicer/mod.rs
```

New CV-1 files:

```text
src-tauri/src/adslicer/cv_detect.rs
src-tauri/examples/adslicer_cv_probe.rs
tools/cv-validation/**
docs/development/OpenCV_CV1.md
docs/development/OpenCV_Milestones.md
docs/development/CV1_VERIFICATION.md
docs/development/CV1_BASELINE.md
CV1_GIT_COMMIT_MESSAGE.txt
```

## Core files explicitly verified unchanged

SHA-256 comparison against the user-supplied baseline confirms these files are unchanged:

```text
src-tauri/src/adslicer/job.rs
src-tauri/src/adslicer/detect.rs
src-tauri/src/adslicer/cut.rs
src-tauri/src/adslicer/models.rs
src/main.js
src/index.html
src/style.css
```

Therefore CV-1 cannot alter normal detection, planning, export, or UI behavior unless the standalone OpenCV probe is explicitly invoked.

## Cargo lock note

`Cargo.lock` is preserved from the supplied baseline. The first OpenCV-enabled Cargo invocation may update it to resolve the newly optional `opencv` direct dependency. This is expected for CV-1 and should be committed after the target Mac successfully resolves/builds the OpenCV feature.

## Workspace isolation fix

This pass declares the `src-tauri` package as its own Cargo workspace root with an empty `[workspace]` table. This prevents Cargo from walking into unrelated parent-directory `Cargo.toml` workspace manifests when AdSlicer is stored inside a larger development folder. It does not change AdSlicer runtime behavior or detector logic.
