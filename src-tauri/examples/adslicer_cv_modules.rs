// Example-only module shim.
//
// The production AdSlicer modules refer to one another through
// `crate::adslicer::...`, matching the real application crate layout.
// Examples are separate crates, so this shim recreates that namespace without
// modifying production source files or pulling in the Tauri job module.

#[path = "../src/adslicer/models.rs"]
pub mod models;
#[path = "../src/adslicer/detect.rs"]
pub mod detect;
#[path = "../src/adslicer/cut.rs"]
pub mod cut;
#[path = "../src/adslicer/cv_detect.rs"]
pub mod cv_detect;
#[path = "../src/adslicer/cv_temporal.rs"]
pub mod cv_temporal;
#[path = "../src/adslicer/cv_boundary.rs"]
pub mod cv_boundary;
#[path = "../src/adslicer/cv_shadow_plan.rs"]
pub mod cv_shadow_plan;
#[path = "../src/adslicer/cv_edit_validation.rs"]
pub mod cv_edit_validation;
#[path = "../src/adslicer/cv_structural.rs"]
pub mod cv_structural;
#[path = "../src/adslicer/cv_segment_validation.rs"]
pub mod cv_segment_validation;

#[path = "../src/adslicer/cv_production.rs"]
pub mod cv_production;
