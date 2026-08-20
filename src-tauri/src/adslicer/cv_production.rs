//! CV-7 production integration for the validated OpenCV structural pipeline.
//!
//! This module promotes the deterministic CV-1/CV-2/CV-3/CV-6 analysis path
//! into the normal AdSlicer job without deleting or altering the Legacy engine.
//! It deliberately exports structural timeline segments rather than inventing
//! semantic show/commercial labels: CV-6 validates boundaries and full source
//! coverage, not content classification.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs;
use std::path::Path;

use super::cv_boundary::{score_boundary_candidates, write_boundary_candidates, BoundaryScoringConfig};
use super::cv_detect::{analyze_video_with_opencv, sha256_file, write_cv_evidence, CvAnalysisConfig, CvAnalysisResult};
use super::cv_structural::{
    build_structural_segmentation, write_structural_segmentation, CoverageSummary,
    StructuralSegmentationConfig,
};
use super::cv_temporal::{analyze_temporal_events, write_temporal_events, TemporalAnalysisConfig};
use super::models::CutInterval;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CvProductionPolicy {
    CompleteSegments,
    EverySeparator,
}

impl CvProductionPolicy {
    pub fn from_param(value: &str) -> Result<Self> {
        match value {
            "complete_segments" | "complete" | "" => Ok(Self::CompleteSegments),
            "every_separator" | "every" => Ok(Self::EverySeparator),
            other => Err(anyhow!("Unknown OpenCV segmentation policy: {other}")),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::CompleteSegments => "complete_segments",
            Self::EverySeparator => "every_separator",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::CompleteSegments => "Complete Segments",
            Self::EverySeparator => "Every Separator",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CvProductionPlan {
    pub engine: String,
    pub policy: CvProductionPolicy,
    pub source_duration_s: f64,
    pub segments: Vec<CutInterval>,
    pub coverage: CoverageSummary,
    pub opencv_version: String,
    pub frames_decoded: u64,
    pub frames_analyzed: u64,
    pub structural_events: u64,
    pub structural_boundaries: u64,
    pub diagnostics_dir: String,
}

fn source_duration_from_cv(cv: &CvAnalysisResult) -> f64 {
    if cv.manifest.frame_count_reported > 0.0 && cv.manifest.fps_reported > 0.0 {
        cv.manifest.frame_count_reported / cv.manifest.fps_reported
    } else if let Some(last) = cv.manifest.last_pts_s {
        last + if cv.manifest.fps_reported > 0.0 {
            1.0 / cv.manifest.fps_reported
        } else {
            0.0
        }
    } else {
        0.0
    }
}

/// Run the validated OpenCV analysis once and materialize the selected
/// full-coverage structural segmentation for production rendering.
pub fn build_opencv_production_plan(
    input: &Path,
    diagnostics_root: &Path,
    policy: CvProductionPolicy,
) -> Result<CvProductionPlan> {
    let frames_dir = diagnostics_root.join("01-frame-evidence");
    let temporal_dir = diagnostics_root.join("02-temporal-events");
    let boundary_dir = diagnostics_root.join("03-boundary-candidates");
    let structural_dir = diagnostics_root.join("06-structural-segmentation");
    fs::create_dir_all(diagnostics_root)?;

    // CV-1 — one source decode, full frame cadence.
    let cv_cfg = CvAnalysisConfig::default();
    let cv = analyze_video_with_opencv(input, &cv_cfg)?;
    if cv.frames.is_empty() {
        return Err(anyhow!("OpenCV decoded no analyzable frames from {}", input.display()));
    }
    write_cv_evidence(&frames_dir, &cv)?;

    // CV-2 — temporal evidence, including CV-6.1a raised VHS black recovery.
    let temporal_cfg = TemporalAnalysisConfig::default();
    let mut temporal = analyze_temporal_events(&cv.frames, &temporal_cfg);
    temporal.source_evidence_file = Some("01-frame-evidence/opencv_frame_metrics.jsonl".to_string());
    let evidence_jsonl = frames_dir.join("opencv_frame_metrics.jsonl");
    temporal.source_evidence_sha256 = Some(sha256_file(&evidence_jsonl)?);
    write_temporal_events(&temporal_dir, &temporal)?;

    // CV-3 — retained as a diagnostic report. Structural segmentation does not
    // depend on the old removal-pair planner.
    let boundary_cfg = BoundaryScoringConfig::default();
    let mut boundary = score_boundary_candidates(&temporal, &boundary_cfg);
    boundary.source_temporal_file = Some("02-temporal-events/opencv_temporal_events.json".to_string());
    let temporal_json = temporal_dir.join("opencv_temporal_events.json");
    boundary.source_temporal_sha256 = Some(sha256_file(&temporal_json)?);
    write_boundary_candidates(&boundary_dir, &boundary)?;

    // CV-6/CV-6.1a — structural role classification + zero-loss segmentation.
    let source_duration_s = source_duration_from_cv(&cv);
    if source_duration_s <= 0.0 {
        return Err(anyhow!("OpenCV could not determine a positive source duration"));
    }
    let structural_cfg = StructuralSegmentationConfig::default();
    let structural = build_structural_segmentation(
        &cv.frames,
        &temporal,
        source_duration_s,
        &structural_cfg,
    );
    write_structural_segmentation(&structural_dir, &structural)?;

    let selected = match policy {
        CvProductionPolicy::CompleteSegments => &structural.complete_segments,
        CvProductionPolicy::EverySeparator => &structural.every_separator,
    };

    if !selected.coverage.coverage_pass {
        return Err(anyhow!(
            "OpenCV full-coverage invariant failed: {:.6}s uncovered, {:.6}s overlap",
            selected.coverage.uncovered_duration_s,
            selected.coverage.overlap_duration_s,
        ));
    }

    let mut segments = Vec::with_capacity(selected.segments.len());
    for segment in &selected.segments {
        let mut out = CutInterval::new(segment.start_s, segment.end_s, "segment");
        out.add_signal("opencv_adaptive");
        out.add_signal(policy.as_str());
        if segment.source_edge_segment {
            out.add_signal("source_edge_segment");
        }
        if segment.broadcast_duration_fit >= 0.50 {
            out.add_signal("broadcast_duration_fit");
        }
        segments.push(out);
    }

    let plan = CvProductionPlan {
        engine: "opencv_adaptive".to_string(),
        policy,
        source_duration_s,
        segments,
        coverage: selected.coverage.clone(),
        opencv_version: cv.manifest.opencv_version.clone(),
        frames_decoded: cv.manifest.frames_decoded,
        frames_analyzed: cv.manifest.frames_analyzed,
        structural_events: structural.summary.structural_events,
        structural_boundaries: selected.boundaries.len() as u64,
        diagnostics_dir: diagnostics_root.display().to_string(),
    };

    fs::write(
        diagnostics_root.join("opencv_production_manifest.json"),
        serde_json::to_string_pretty(&json!({
            "schema_version": 1,
            "engine": &plan.engine,
            "policy": plan.policy.as_str(),
            "source_file": input.display().to_string(),
            "source_duration_s": plan.source_duration_s,
            "opencv_version": &plan.opencv_version,
            "frames_decoded": plan.frames_decoded,
            "frames_analyzed": plan.frames_analyzed,
            "structural_events": plan.structural_events,
            "structural_boundaries": plan.structural_boundaries,
            "segment_count": plan.segments.len(),
            "coverage": &plan.coverage,
            "segments": &plan.segments,
            "production_semantics": "full_timeline_structural_segments_no_semantic_content_labels",
        }))?,
    )?;

    Ok(plan)
}
