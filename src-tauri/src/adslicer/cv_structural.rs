//! CV-6 structural segmentation engine for AdSlicer.
//!
//! CV-1 through CV-5 prove that OpenCV can measure VHS/broadcast transitions,
//! group them temporally, score candidate boundaries, and render shadow edit
//! previews. CV-6 changes the *model* of the problem: every frame must belong to
//! exactly one output segment, and visual separator evidence is classified
//! independently from the user's segmentation policy.
//!
//! Two policies are produced from the same evidence:
//! - CompleteSegments: conservative structural separators only, with optional
//!   promotion of ambiguous events when broadcast-duration context strongly
//!   supports the split.
//! - EverySeparator: all separator-like events above the separator threshold.
//!
//! This module is still validation/shadow logic. It does not modify the live
//! AdSlicer production planner or renderer.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use super::cv_detect::FrameMetrics;
use super::cv_temporal::{TemporalAnalysisResult, TemporalEvent, TemporalEventKind};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructuralSegmentationConfig {
    /// Scene-delta threshold reused only for context-rate statistics.
    pub scene_delta_min: f64,
    /// Context window on each side of a candidate separator.
    pub context_window_s: f64,
    /// Exclude immediate transition frames from context summaries.
    pub context_guard_s: f64,

    /// A very uniform dark event is strong separator evidence. VHS black may
    /// be tinted/raised, so uniformity is intentionally independent of color.
    pub uniform_stddev_strong_max: f64,
    pub uniform_stddev_review_max: f64,

    /// Separator evidence required for Every Separator mode.
    pub every_separator_score_min: f64,
    /// Conservative structural role thresholds for Complete Segments.
    pub structural_separator_score_min: f64,
    pub structural_combined_score_min: f64,
    pub ambiguous_separator_score_min: f64,

    /// Ambiguous events are only promoted when they improve a poor duration
    /// interval; this avoids turning a clean 30-second commercial into two
    /// 15-second clips simply because both durations are plausible.
    pub ambiguous_promotion_structural_min: f64,
    pub ambiguous_promotion_duration_gain_min: f64,
    /// Allow a short source head/tail residual to promote an ambiguous separator
    /// when the adjacent interior segment lands cleanly on a broadcast duration.
    pub source_edge_max_residual_s: f64,
    pub source_edge_duration_fit_min: f64,

    /// Broadcast-duration priors are context, not truth. Source-head/tail
    /// residuals are never forced to match these values.
    pub preferred_durations_s: Vec<f64>,
    pub duration_match_tolerance_s: f64,

    /// Prevent duplicate boundaries if future CV stages emit multiple event
    /// representations around the same physical separator.
    pub boundary_dedupe_s: f64,
}

impl Default for StructuralSegmentationConfig {
    fn default() -> Self {
        Self {
            scene_delta_min: 40.0,
            context_window_s: 3.0,
            context_guard_s: 0.25,
            uniform_stddev_strong_max: 8.0,
            uniform_stddev_review_max: 16.0,
            every_separator_score_min: 0.58,
            structural_separator_score_min: 0.82,
            structural_combined_score_min: 0.20,
            ambiguous_separator_score_min: 0.55,
            ambiguous_promotion_structural_min: 0.18,
            ambiguous_promotion_duration_gain_min: 0.35,
            source_edge_max_residual_s: 7.0,
            source_edge_duration_fit_min: 0.70,
            preferred_durations_s: vec![15.0, 30.0, 60.0, 10.0, 20.0, 5.0, 45.0, 90.0],
            duration_match_tolerance_s: 2.0,
            boundary_dedupe_s: 0.35,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StructuralRole {
    StructuralSeparator,
    InternalTransition,
    Ambiguous,
}

impl StructuralRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::StructuralSeparator => "structural_separator",
            Self::InternalTransition => "internal_transition",
            Self::Ambiguous => "ambiguous",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SegmentationMode {
    CompleteSegments,
    EverySeparator,
}

impl SegmentationMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CompleteSegments => "complete_segments",
            Self::EverySeparator => "every_separator",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ContextSummary {
    pub frame_count: u64,
    pub duration_s: f64,
    pub mean_luma: f64,
    pub mean_stddev_luma: f64,
    pub mean_red: f64,
    pub mean_green: f64,
    pub mean_blue: f64,
    pub mean_saturation: f64,
    pub mean_frame_delta: f64,
    pub scene_changes: u64,
    pub scene_rate_per_s: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructuralEvent {
    pub structural_event_index: u64,
    pub source_temporal_event_index: u64,
    pub source_temporal_kind: TemporalEventKind,
    pub start_pts_s: f64,
    pub valley_pts_s: f64,
    pub end_pts_s: f64,
    pub anchor_pts_s: f64,
    pub duration_s: f64,

    pub min_mean_luma: f64,
    pub max_black_pixel_ratio: f64,
    pub max_near_black_pixel_ratio: f64,
    pub mean_stddev_luma: f64,
    pub mean_saturation: f64,
    pub peak_delta_mean: f64,

    /// "How separator-like is the event itself?" Independent from whether the
    /// separator should divide complete commercials/program pieces.
    pub separator_score: f64,
    /// "How different are the surrounding content regimes?"
    pub context_change_score: f64,
    /// Combined structural heuristic. Versioned score, not a probability.
    pub structural_score: f64,
    pub role: StructuralRole,

    pub before_context: ContextSummary,
    pub after_context: ContextSummary,
    pub reasons: Vec<String>,
    pub cautions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentBoundary {
    pub boundary_index: u64,
    pub pts_s: f64,
    pub source_structural_event_index: u64,
    pub source_temporal_event_index: u64,
    pub separator_score: f64,
    pub structural_score: f64,
    pub role: StructuralRole,
    pub promoted_from_ambiguous: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineSegment {
    pub segment_index: u64,
    pub start_s: f64,
    pub end_s: f64,
    pub duration_s: f64,
    pub left_boundary_index: Option<u64>,
    pub right_boundary_index: Option<u64>,
    pub nearest_broadcast_duration_s: Option<f64>,
    pub broadcast_duration_fit: f64,
    pub source_edge_segment: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverageSummary {
    pub source_duration_s: f64,
    pub segment_count: u64,
    pub boundary_count: u64,
    pub covered_duration_s: f64,
    pub uncovered_duration_s: f64,
    pub overlap_duration_s: f64,
    pub coverage_ratio: f64,
    pub coverage_pass: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentationPlan {
    pub mode: SegmentationMode,
    pub boundaries: Vec<SegmentBoundary>,
    pub segments: Vec<TimelineSegment>,
    pub coverage: CoverageSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructuralSegmentationSummary {
    pub structural_events: u64,
    pub counts_by_role: BTreeMap<String, u64>,
    pub complete_segments: u64,
    pub complete_boundaries: u64,
    pub every_separator_segments: u64,
    pub every_separator_boundaries: u64,
    pub ambiguous_promoted_for_complete: u64,
    pub complete_coverage_pass: bool,
    pub every_separator_coverage_pass: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructuralSegmentationResult {
    pub schema_version: u32,
    pub engine: String,
    pub structural_version: String,
    pub production_cut_plan_modified: bool,
    pub config: StructuralSegmentationConfig,
    pub summary: StructuralSegmentationSummary,
    pub events: Vec<StructuralEvent>,
    pub complete_segments: SegmentationPlan,
    pub every_separator: SegmentationPlan,
}

fn clamp01(v: f64) -> f64 { v.clamp(0.0, 1.0) }

fn mean(values: impl Iterator<Item = f64>) -> f64 {
    let mut n = 0u64;
    let mut total = 0.0;
    for v in values {
        n += 1;
        total += v;
    }
    if n == 0 { 0.0 } else { total / n as f64 }
}

fn context_summary(
    frames: &[FrameMetrics],
    start_s: f64,
    end_s: f64,
    scene_delta_min: f64,
) -> ContextSummary {
    if end_s <= start_s {
        return ContextSummary::default();
    }
    let selected: Vec<&FrameMetrics> = frames
        .iter()
        .filter(|f| f.pts_s >= start_s && f.pts_s <= end_s)
        .collect();
    if selected.is_empty() {
        return ContextSummary::default();
    }
    let scene_changes = selected
        .iter()
        .filter(|f| f.frame_delta_mean >= scene_delta_min)
        .count() as u64;
    let duration_s = (selected.last().unwrap().pts_s - selected.first().unwrap().pts_s).max(0.0);
    ContextSummary {
        frame_count: selected.len() as u64,
        duration_s,
        mean_luma: mean(selected.iter().map(|f| f.mean_luma)),
        mean_stddev_luma: mean(selected.iter().map(|f| f.stddev_luma)),
        mean_red: mean(selected.iter().map(|f| f.mean_red)),
        mean_green: mean(selected.iter().map(|f| f.mean_green)),
        mean_blue: mean(selected.iter().map(|f| f.mean_blue)),
        mean_saturation: mean(selected.iter().map(|f| f.mean_saturation)),
        mean_frame_delta: mean(selected.iter().map(|f| f.frame_delta_mean)),
        scene_changes,
        scene_rate_per_s: if duration_s > 0.05 { scene_changes as f64 / duration_s } else { 0.0 },
    }
}

fn uniformity_score(stddev: f64, cfg: &StructuralSegmentationConfig) -> f64 {
    let score: f64 = if stddev <= 4.0 {
        1.0
    } else if stddev <= cfg.uniform_stddev_strong_max {
        // ~0.90 at 8.0
        1.0 - 0.025 * (stddev - 4.0)
    } else if stddev <= 14.0 {
        // Rapidly reduce confidence once dark frames contain visible structure.
        0.70 - 0.05 * (stddev - 8.0)
    } else if stddev <= cfg.uniform_stddev_review_max {
        0.30 - 0.09 * (stddev - 14.0)
    } else if stddev <= 20.0 {
        0.12
    } else {
        0.05
    };
    score.clamp(0.0, 1.0)
}

fn darkness_score(event: &TemporalEvent) -> f64 {
    // Analog black can be raised above digital-black levels. A low mean and a
    // large near-black fraction are complementary rather than interchangeable.
    let luma_component = clamp01((40.0 - event.min_mean_luma) / 16.0);
    let ratio_component = clamp01(event.max_near_black_pixel_ratio);
    0.55 * luma_component + 0.45 * ratio_component
}

fn duration_shape_score(event: &TemporalEvent) -> f64 {
    match event.duration_frames {
        1..=30 => 1.0,
        31..=60 => 0.55,
        _ => 0.20,
    }
}

fn context_change(before: &ContextSummary, after: &ContextSummary) -> (f64, Vec<String>) {
    if before.frame_count == 0 || after.frame_count == 0 {
        return (0.25, vec!["source-edge/insufficient context".to_string()]);
    }

    let luma = clamp01((before.mean_luma - after.mean_luma).abs() / 80.0);
    let color_distance = (
        (before.mean_red - after.mean_red).powi(2)
        + (before.mean_green - after.mean_green).powi(2)
        + (before.mean_blue - after.mean_blue).powi(2)
    ).sqrt();
    let color = clamp01(color_distance / 220.0);
    let saturation = clamp01((before.mean_saturation - after.mean_saturation).abs() / 100.0);
    let texture = clamp01((before.mean_stddev_luma - after.mean_stddev_luma).abs() / 30.0);
    let motion = clamp01((before.mean_frame_delta - after.mean_frame_delta).abs() / 15.0);
    let shot_rate = clamp01((before.scene_rate_per_s - after.scene_rate_per_s).abs() / 2.0);

    let score = 0.15 * luma
        + 0.35 * color
        + 0.10 * saturation
        + 0.15 * texture
        + 0.10 * motion
        + 0.15 * shot_rate;

    let mut reasons = Vec::new();
    if color >= 0.35 { reasons.push(format!("context color reset {:.2}", color)); }
    if texture >= 0.35 { reasons.push(format!("context texture reset {:.2}", texture)); }
    if luma >= 0.35 { reasons.push(format!("context luminance reset {:.2}", luma)); }
    if shot_rate >= 0.35 { reasons.push(format!("shot-rate regime change {:.2}", shot_rate)); }
    if reasons.is_empty() { reasons.push(format!("modest context change {:.2}", score)); }
    (clamp01(score), reasons)
}

fn is_visual_transition_event(kind: TemporalEventKind) -> bool {
    !matches!(kind, TemporalEventKind::SceneDiscontinuity)
}

fn build_structural_event(
    structural_event_index: u64,
    event: &TemporalEvent,
    frames: &[FrameMetrics],
    source_duration_s: f64,
    cfg: &StructuralSegmentationConfig,
) -> StructuralEvent {
    let before_start = (event.start_pts_s - cfg.context_window_s).max(0.0);
    let before_end = (event.start_pts_s - cfg.context_guard_s).max(before_start);
    let after_start = (event.end_pts_s + cfg.context_guard_s).min(source_duration_s);
    let after_end = (event.end_pts_s + cfg.context_window_s).min(source_duration_s).max(after_start);
    let before_context = context_summary(frames, before_start, before_end, cfg.scene_delta_min);
    let after_context = context_summary(frames, after_start, after_end, cfg.scene_delta_min);

    let uniformity = uniformity_score(event.mean_stddev_luma, cfg);
    let darkness = darkness_score(event);
    let duration_shape = duration_shape_score(event);
    let mut separator_score = clamp01(0.55 * uniformity + 0.35 * darkness + 0.10 * duration_shape);
    // A saturated uniform blue/red slate can be numerically dark while clearly
    // not being black. Preserve moderately tinted VHS black as evidence, but
    // strongly down-rank highly saturated chromatic cards.
    if event.kind == TemporalEventKind::ChromaticDark {
        separator_score *= if event.mean_saturation >= 180.0 { 0.45 } else { 0.75 };
    }

    let (context_change_score, mut context_reasons) = context_change(&before_context, &after_context);
    let discontinuity_score = clamp01(event.peak_delta_mean / 80.0);
    // Any fade through black can create a huge immediate pixel delta. That is
    // separator evidence, not proof that a *new commercial* began. Structural
    // scoring therefore comes from the surrounding content regimes.
    let structural_score = context_change_score;

    let role = if separator_score >= cfg.structural_separator_score_min
        && structural_score >= cfg.structural_combined_score_min
    {
        StructuralRole::StructuralSeparator
    } else if separator_score >= cfg.ambiguous_separator_score_min {
        StructuralRole::Ambiguous
    } else {
        StructuralRole::InternalTransition
    };

    let mut reasons = vec![
        format!("separator score {:.2}", separator_score),
        format!("uniformity score {:.2} from luma stddev {:.2}", uniformity, event.mean_stddev_luma),
        format!("darkness score {:.2}", darkness),
        format!("context change {:.2}", context_change_score),
    ];
    reasons.append(&mut context_reasons);
    if discontinuity_score >= 0.50 {
        reasons.push(format!("strong immediate visual reset {:.2}", discontinuity_score));
    }

    let mut cautions = Vec::new();
    if event.mean_stddev_luma > cfg.uniform_stddev_review_max {
        cautions.push(format!(
            "dark event is spatially structured (luma stddev {:.2}); likely title card/image rather than blank separator",
            event.mean_stddev_luma
        ));
    } else if event.mean_stddev_luma > cfg.uniform_stddev_strong_max {
        cautions.push(format!("moderate structure remains in dark event (luma stddev {:.2})", event.mean_stddev_luma));
    }
    if event.chromatic_dark {
        cautions.push("dark event retains chromatic structure; color alone is not used to reject raised/tinted VHS black".to_string());
    }
    if role == StructuralRole::Ambiguous {
        cautions.push("separator-like event is ambiguous in Complete Segments mode".to_string());
    }

    StructuralEvent {
        structural_event_index,
        source_temporal_event_index: event.event_index,
        source_temporal_kind: event.kind,
        start_pts_s: event.start_pts_s,
        valley_pts_s: event.valley_pts_s,
        end_pts_s: event.end_pts_s,
        anchor_pts_s: event.valley_pts_s,
        duration_s: event.duration_s,
        min_mean_luma: event.min_mean_luma,
        max_black_pixel_ratio: event.max_black_pixel_ratio,
        max_near_black_pixel_ratio: event.max_near_black_pixel_ratio,
        mean_stddev_luma: event.mean_stddev_luma,
        mean_saturation: event.mean_saturation,
        peak_delta_mean: event.peak_delta_mean,
        separator_score,
        context_change_score,
        structural_score,
        role,
        before_context,
        after_context,
        reasons,
        cautions,
    }
}

fn duration_fit(duration_s: f64, cfg: &StructuralSegmentationConfig) -> (f64, Option<f64>) {
    if duration_s <= 0.0 || cfg.preferred_durations_s.is_empty() {
        return (0.0, None);
    }
    let (target, distance) = cfg
        .preferred_durations_s
        .iter()
        .map(|&target| (target, (duration_s - target).abs()))
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .unwrap();
    let fit = clamp01(1.0 - distance / cfg.duration_match_tolerance_s.max(0.001));
    (fit, Some(target))
}

fn dedupe_boundaries(mut boundaries: Vec<SegmentBoundary>, cfg: &StructuralSegmentationConfig) -> Vec<SegmentBoundary> {
    boundaries.sort_by(|a, b| a.pts_s.total_cmp(&b.pts_s));
    let mut out: Vec<SegmentBoundary> = Vec::new();
    for b in boundaries {
        if let Some(last) = out.last_mut() {
            if (b.pts_s - last.pts_s).abs() <= cfg.boundary_dedupe_s {
                // Keep the stronger interpretation of the same physical event.
                if b.structural_score > last.structural_score {
                    *last = b;
                }
                continue;
            }
        }
        out.push(b);
    }
    for (i, b) in out.iter_mut().enumerate() { b.boundary_index = i as u64; }
    out
}

fn boundary_from_event(event: &StructuralEvent, promoted: bool) -> SegmentBoundary {
    SegmentBoundary {
        boundary_index: 0,
        pts_s: event.anchor_pts_s,
        source_structural_event_index: event.structural_event_index,
        source_temporal_event_index: event.source_temporal_event_index,
        separator_score: event.separator_score,
        structural_score: event.structural_score,
        role: event.role,
        promoted_from_ambiguous: promoted,
    }
}

fn nearest_selected_neighbors(
    selected: &[SegmentBoundary],
    t: f64,
    source_duration_s: f64,
) -> (f64, f64) {
    let mut left: f64 = 0.0;
    let mut right: f64 = source_duration_s;
    for b in selected {
        if b.pts_s < t { left = left.max(b.pts_s); }
        if b.pts_s > t { right = right.min(b.pts_s); }
    }
    (left, right)
}

fn promote_ambiguous_boundaries(
    events: &[StructuralEvent],
    selected: &mut Vec<SegmentBoundary>,
    source_duration_s: f64,
    cfg: &StructuralSegmentationConfig,
) -> u64 {
    let mut promoted = 0u64;
    let mut ambiguous: Vec<&StructuralEvent> = events
        .iter()
        .filter(|e| e.role == StructuralRole::Ambiguous && e.separator_score >= cfg.every_separator_score_min)
        .collect();
    ambiguous.sort_by(|a, b| b.structural_score.total_cmp(&a.structural_score));

    for event in ambiguous {
        let t = event.anchor_pts_s;
        let (left, right) = nearest_selected_neighbors(selected, t, source_duration_s);
        if t <= left || t >= right { continue; }
        let unsplit = right - left;
        let left_d = t - left;
        let right_d = right - t;
        let (unsplit_fit, _) = duration_fit(unsplit, cfg);
        let (left_fit, _) = duration_fit(left_d, cfg);
        let (right_fit, _) = duration_fit(right_d, cfg);

        let is_head_candidate = left <= 0.0;
        let is_tail_candidate = right >= source_duration_s;
        let edge_supported = if is_head_candidate {
            left_d <= cfg.source_edge_max_residual_s && right_fit >= cfg.source_edge_duration_fit_min
        } else if is_tail_candidate {
            right_d <= cfg.source_edge_max_residual_s && left_fit >= cfg.source_edge_duration_fit_min
        } else {
            false
        };

        let split_fit = 0.5 * (left_fit + right_fit);
        let duration_gain = split_fit - unsplit_fit;
        let interior_supported = !is_head_candidate && !is_tail_candidate
            && event.structural_score >= cfg.ambiguous_promotion_structural_min
            && duration_gain >= cfg.ambiguous_promotion_duration_gain_min;

        if edge_supported || interior_supported {
            selected.push(boundary_from_event(event, true));
            *selected = dedupe_boundaries(std::mem::take(selected), cfg);
            promoted += 1;
        }
    }
    promoted
}

fn build_plan(
    mode: SegmentationMode,
    mut boundaries: Vec<SegmentBoundary>,
    source_duration_s: f64,
    cfg: &StructuralSegmentationConfig,
) -> SegmentationPlan {
    boundaries.retain(|b| b.pts_s > 0.0 && b.pts_s < source_duration_s);
    boundaries = dedupe_boundaries(boundaries, cfg);

    let mut segments = Vec::new();
    let mut cursor = 0.0;
    for (i, boundary) in boundaries.iter().enumerate() {
        let end = boundary.pts_s.max(cursor).min(source_duration_s);
        let duration = (end - cursor).max(0.0);
        let (fit, target) = duration_fit(duration, cfg);
        segments.push(TimelineSegment {
            segment_index: segments.len() as u64,
            start_s: cursor,
            end_s: end,
            duration_s: duration,
            left_boundary_index: if i == 0 { None } else { Some(boundaries[i - 1].boundary_index) },
            right_boundary_index: Some(boundary.boundary_index),
            nearest_broadcast_duration_s: target,
            broadcast_duration_fit: fit,
            source_edge_segment: i == 0,
        });
        cursor = end;
    }
    let tail_duration = (source_duration_s - cursor).max(0.0);
    let (tail_fit, tail_target) = duration_fit(tail_duration, cfg);
    segments.push(TimelineSegment {
        segment_index: segments.len() as u64,
        start_s: cursor,
        end_s: source_duration_s,
        duration_s: tail_duration,
        left_boundary_index: boundaries.last().map(|b| b.boundary_index),
        right_boundary_index: None,
        nearest_broadcast_duration_s: tail_target,
        broadcast_duration_fit: tail_fit,
        source_edge_segment: true,
    });

    // Half-open contiguous intervals are built from one cursor, so missing and
    // overlap duration should be mathematically zero. Keep explicit assertions
    // in the serialized result because full coverage is now a product invariant.
    let covered_duration_s: f64 = segments.iter().map(|s| s.duration_s).sum();
    let uncovered_duration_s = (source_duration_s - covered_duration_s).max(0.0);
    let overlap_duration_s = (covered_duration_s - source_duration_s).max(0.0);
    let epsilon = 0.002;
    let coverage_pass = uncovered_duration_s <= epsilon && overlap_duration_s <= epsilon;
    let coverage_ratio = if source_duration_s > 0.0 {
        (covered_duration_s / source_duration_s).clamp(0.0, 1.0)
    } else { 1.0 };

    SegmentationPlan {
        mode,
        coverage: CoverageSummary {
            source_duration_s,
            segment_count: segments.len() as u64,
            boundary_count: boundaries.len() as u64,
            covered_duration_s,
            uncovered_duration_s,
            overlap_duration_s,
            coverage_ratio,
            coverage_pass,
        },
        boundaries,
        segments,
    }
}

pub fn build_structural_segmentation(
    frames: &[FrameMetrics],
    temporal: &TemporalAnalysisResult,
    source_duration_s: f64,
    cfg: &StructuralSegmentationConfig,
) -> StructuralSegmentationResult {
    let mut events: Vec<StructuralEvent> = temporal
        .events
        .iter()
        .filter(|e| is_visual_transition_event(e.kind))
        .enumerate()
        .map(|(i, e)| build_structural_event(i as u64, e, frames, source_duration_s, cfg))
        .collect();
    events.sort_by(|a, b| a.anchor_pts_s.total_cmp(&b.anchor_pts_s));
    for (i, e) in events.iter_mut().enumerate() { e.structural_event_index = i as u64; }

    let mut complete_boundaries: Vec<SegmentBoundary> = events
        .iter()
        .filter(|e| e.role == StructuralRole::StructuralSeparator)
        .map(|e| boundary_from_event(e, false))
        .collect();
    complete_boundaries = dedupe_boundaries(complete_boundaries, cfg);
    let promoted = promote_ambiguous_boundaries(
        &events,
        &mut complete_boundaries,
        source_duration_s,
        cfg,
    );

    let every_boundaries: Vec<SegmentBoundary> = events
        .iter()
        .filter(|e| e.separator_score >= cfg.every_separator_score_min)
        .map(|e| boundary_from_event(e, false))
        .collect();

    let complete_segments = build_plan(
        SegmentationMode::CompleteSegments,
        complete_boundaries,
        source_duration_s,
        cfg,
    );
    let every_separator = build_plan(
        SegmentationMode::EverySeparator,
        every_boundaries,
        source_duration_s,
        cfg,
    );

    let mut counts_by_role = BTreeMap::new();
    for e in &events {
        *counts_by_role.entry(e.role.as_str().to_string()).or_insert(0) += 1;
    }

    let summary = StructuralSegmentationSummary {
        structural_events: events.len() as u64,
        counts_by_role,
        complete_segments: complete_segments.segments.len() as u64,
        complete_boundaries: complete_segments.boundaries.len() as u64,
        every_separator_segments: every_separator.segments.len() as u64,
        every_separator_boundaries: every_separator.boundaries.len() as u64,
        ambiguous_promoted_for_complete: promoted,
        complete_coverage_pass: complete_segments.coverage.coverage_pass,
        every_separator_coverage_pass: every_separator.coverage.coverage_pass,
    };

    StructuralSegmentationResult {
        schema_version: 1,
        engine: "opencv-structural-segmentation".to_string(),
        structural_version: "cv6-structural-v1".to_string(),
        production_cut_plan_modified: false,
        config: cfg.clone(),
        summary,
        events,
        complete_segments,
        every_separator,
    }
}

pub fn write_structural_segmentation(
    outdir: &Path,
    result: &StructuralSegmentationResult,
) -> Result<Vec<PathBuf>> {
    fs::create_dir_all(outdir)?;
    let full_path = outdir.join("opencv_structural_segmentation.json");
    fs::write(&full_path, serde_json::to_string_pretty(result)?)?;

    let summary_path = outdir.join("opencv_structural_summary.json");
    fs::write(&summary_path, serde_json::to_string_pretty(&result.summary)?)?;

    let events_path = outdir.join("opencv_structural_events.csv");
    let mut events_csv = BufWriter::new(fs::File::create(&events_path)?);
    writeln!(events_csv, "event_index,temporal_event_index,temporal_kind,anchor_pts_s,start_pts_s,end_pts_s,duration_s,min_mean_luma,max_black_pixel_ratio,max_near_black_pixel_ratio,mean_stddev_luma,mean_saturation,peak_delta_mean,separator_score,context_change_score,structural_score,role")?;
    for e in &result.events {
        writeln!(events_csv, "{},{},{},{:.6},{:.6},{:.6},{:.6},{:.6},{:.8},{:.8},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{}",
            e.structural_event_index,
            e.source_temporal_event_index,
            e.source_temporal_kind.as_str(),
            e.anchor_pts_s,
            e.start_pts_s,
            e.end_pts_s,
            e.duration_s,
            e.min_mean_luma,
            e.max_black_pixel_ratio,
            e.max_near_black_pixel_ratio,
            e.mean_stddev_luma,
            e.mean_saturation,
            e.peak_delta_mean,
            e.separator_score,
            e.context_change_score,
            e.structural_score,
            e.role.as_str(),
        )?;
    }
    events_csv.flush()?;

    let complete_path = outdir.join("complete_segments_plan.csv");
    write_plan_csv(&complete_path, &result.complete_segments)?;
    let every_path = outdir.join("every_separator_plan.csv");
    write_plan_csv(&every_path, &result.every_separator)?;

    Ok(vec![full_path, summary_path, events_path, complete_path, every_path])
}

fn write_plan_csv(path: &Path, plan: &SegmentationPlan) -> Result<()> {
    let mut csv = BufWriter::new(fs::File::create(path)?);
    writeln!(csv, "segment_index,start_s,end_s,duration_s,left_boundary_index,right_boundary_index,nearest_broadcast_duration_s,broadcast_duration_fit,source_edge_segment")?;
    for s in &plan.segments {
        writeln!(csv, "{},{:.6},{:.6},{:.6},{},{},{},{:.6},{}",
            s.segment_index,
            s.start_s,
            s.end_s,
            s.duration_s,
            s.left_boundary_index.map(|v| v.to_string()).unwrap_or_default(),
            s.right_boundary_index.map(|v| v.to_string()).unwrap_or_default(),
            s.nearest_broadcast_duration_s.map(|v| format!("{v:.3}")).unwrap_or_default(),
            s.broadcast_duration_fit,
            s.source_edge_segment,
        )?;
    }
    csv.flush()?;
    Ok(())
}
