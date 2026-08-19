pub mod job;
pub mod detect;
pub mod cut;
pub mod models;

pub mod cv_detect;

pub mod cv_temporal;

pub mod cv_boundary;

pub mod cv_shadow_plan;
#[cfg(feature = "opencv-analysis")]
pub mod cv_edit_validation;

pub mod cv_structural;
#[cfg(feature = "opencv-analysis")]
pub mod cv_segment_validation;
