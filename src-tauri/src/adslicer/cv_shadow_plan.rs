//! CV-4 shadow edit planning for AdSlicer.
//!
//! This stage converts CV-3 boundary candidates into proposed removal intervals
//! without touching the production `Plan` used by AdSlicer.  The pairing rules
//! mirror the semantics of the existing planner: when black is excluded, a
//! commercial candidate begins after the previous dark separator (interval exit)
//! and ends before the next dark separator (interval entry).

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use super::cv_boundary::{BoundaryAnalysisResult, BoundaryObservation, BoundaryRole};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShadowPlanConfig {
    pub include_black: bool,
    pub edge_pad_pre: f64,
    pub edge_pad_post: f64,
    /// Source frame duration used to convert the PTS of the last dark frame
    /// into the boundary immediately after that frame.
    pub frame_duration_s: f64,
    pub min_commercial: f64,
    pub max_commercial: f64,
    pub min_show_segment: f64,
    pub always_keep_first: f64,
    pub always_keep_last: f64,
    pub review_score_min: f64,
    pub strong_score_min: f64,
    pub legacy_match_tolerance_s: f64,
    pub legacy_match_iou_min: f64,
}

impl Default for ShadowPlanConfig {
    fn default() -> Self {
        Self {
            include_black: false,
            edge_pad_pre: 0.20,
            edge_pad_post: 0.06,
            frame_duration_s: 1.0 / 30.0,
            min_commercial: 5.0,
            max_commercial: 240.0,
            min_show_segment: 30.0,
            always_keep_first: 0.0,
            always_keep_last: 0.0,
            review_score_min: 0.60,
            strong_score_min: 0.80,
            legacy_match_tolerance_s: 0.75,
            legacy_match_iou_min: 0.80,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShadowIntervalTier {
    Strong,
    Review,
    Weak,
}

impl ShadowIntervalTier {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Strong => "strong",
            Self::Review => "review",
            Self::Weak => "weak",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShadowInterval {
    pub interval_index: u64,
    pub start_s: f64,
    pub end_s: f64,
    pub duration_s: f64,
    pub start_anchor_s: f64,
    pub end_anchor_s: f64,
    pub start_observation_index: u64,
    pub end_observation_index: u64,
    pub start_role: BoundaryRole,
    pub end_role: BoundaryRole,
    pub start_source_event_index: u64,
    pub end_source_event_index: u64,
    pub start_score: f64,
    pub end_score: f64,
    /// Versioned heuristic score for ranking/review only; not a probability.
    pub interval_score: f64,
    pub tier: ShadowIntervalTier,
    pub reasons: Vec<String>,
    pub cautions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RejectedShadowInterval {
    pub start_anchor_s: f64,
    pub end_anchor_s: f64,
    pub duration_s: f64,
    pub start_observation_index: u64,
    pub end_observation_index: u64,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShadowPlanSummary {
    pub source_duration_s: f64,
    pub boundary_candidates_consumed: u64,
    pub proposed_removals: u64,
    pub rejected_pairs: u64,
    pub total_remove_s: f64,
    pub total_keep_s: f64,
    pub remove_ratio: f64,
    pub counts_by_tier: BTreeMap<String, u64>,
    pub first_removal_start_s: Option<f64>,
    pub last_removal_end_s: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShadowPlanResult {
    pub schema_version: u32,
    pub engine: String,
    pub planner_version: String,
    pub config: ShadowPlanConfig,
    pub summary: ShadowPlanSummary,
    pub removals: Vec<ShadowInterval>,
    pub rejected: Vec<RejectedShadowInterval>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegacyPlanInterval {
    pub interval_index: u64,
    pub start_s: f64,
    pub end_s: f64,
    pub duration_s: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanMatch {
    pub shadow_interval_index: u64,
    pub legacy_interval_index: u64,
    pub shadow_start_s: f64,
    pub shadow_end_s: f64,
    pub legacy_start_s: f64,
    pub legacy_end_s: f64,
    pub start_offset_s: f64,
    pub end_offset_s: f64,
    pub intersection_over_union: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanComparisonSummary {
    pub shadow_intervals: u64,
    pub legacy_intervals: u64,
    pub matched_intervals: u64,
    pub opencv_only_intervals: u64,
    pub legacy_only_intervals: u64,
    pub average_abs_boundary_offset_s: Option<f64>,
    pub max_abs_boundary_offset_s: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanComparisonResult {
    pub schema_version: u32,
    pub engine: String,
    pub comparison_version: String,
    pub match_tolerance_s: f64,
    pub match_iou_min: f64,
    pub summary: PlanComparisonSummary,
    pub matches: Vec<PlanMatch>,
    pub opencv_only_interval_indices: Vec<u64>,
    pub legacy_only_interval_indices: Vec<u64>,
}

fn can_start(role: BoundaryRole, include_black: bool) -> bool {
    match role {
        BoundaryRole::SeparatorAnchor | BoundaryRole::FadeValley => true,
        BoundaryRole::IntervalEntry => include_black,
        BoundaryRole::IntervalExit => !include_black,
        BoundaryRole::SceneOnly | BoundaryRole::ChromaticDark => false,
    }
}

fn can_end(role: BoundaryRole, include_black: bool) -> bool {
    match role {
        BoundaryRole::SeparatorAnchor | BoundaryRole::FadeValley => true,
        BoundaryRole::IntervalEntry => !include_black,
        BoundaryRole::IntervalExit => include_black,
        BoundaryRole::SceneOnly | BoundaryRole::ChromaticDark => false,
    }
}

fn interval_score(a: &BoundaryObservation, b: &BoundaryObservation) -> f64 {
    // Conservative: the weaker endpoint has more influence than the average.
    let low = a.boundary_score.min(b.boundary_score);
    let avg = (a.boundary_score + b.boundary_score) * 0.5;
    (low * 0.65 + avg * 0.35).clamp(0.0, 1.0)
}

fn tier_for_score(score: f64, cfg: &ShadowPlanConfig) -> ShadowIntervalTier {
    if score >= cfg.strong_score_min {
        ShadowIntervalTier::Strong
    } else if score >= cfg.review_score_min {
        ShadowIntervalTier::Review
    } else {
        ShadowIntervalTier::Weak
    }
}

fn effective_anchor(o: &BoundaryObservation, as_start: bool, cfg: &ShadowPlanConfig) -> f64 {
    match o.role {
        BoundaryRole::IntervalExit => o.anchor_pts_s + cfg.frame_duration_s,
        BoundaryRole::IntervalEntry => o.anchor_pts_s,
        BoundaryRole::SeparatorAnchor => {
            if as_start {
                o.evidence_end_pts_s + cfg.frame_duration_s
            } else {
                o.evidence_start_pts_s
            }
        }
        BoundaryRole::FadeValley => o.anchor_pts_s,
        BoundaryRole::SceneOnly | BoundaryRole::ChromaticDark => o.anchor_pts_s,
    }
}

fn make_interval(
    start: &BoundaryObservation,
    end: &BoundaryObservation,
    source_duration_s: f64,
    cfg: &ShadowPlanConfig,
) -> Option<ShadowInterval> {
    if end.anchor_pts_s <= start.anchor_pts_s || start.source_event_index == end.source_event_index {
        return None;
    }

    let raw_start = effective_anchor(start, true, cfg);
    let raw_end = effective_anchor(end, false, cfg);
    let start_s = (raw_start - cfg.edge_pad_pre).max(0.0);
    let end_s = (raw_end + cfg.edge_pad_post).min(source_duration_s);
    if end_s <= start_s {
        return None;
    }
    let duration_s = end_s - start_s;
    let score = interval_score(start, end);
    let tier = tier_for_score(score, cfg);

    let mut reasons = vec![
        format!("start boundary {} score {:.3}", start.role.as_str(), start.boundary_score),
        format!("end boundary {} score {:.3}", end.role.as_str(), end.boundary_score),
        format!("duration {:.3}s within shadow planning window", duration_s),
    ];
    if cfg.edge_pad_pre > 0.0 || cfg.edge_pad_post > 0.0 {
        reasons.push(format!(
            "AdSlicer-style edge padding: -{:.3}s / +{:.3}s",
            cfg.edge_pad_pre, cfg.edge_pad_post
        ));
    }

    let mut cautions = Vec::new();
    if tier == ShadowIntervalTier::Weak {
        cautions.push("one or both boundary endpoints are weak; review before promotion".to_string());
    }

    Some(ShadowInterval {
        interval_index: 0,
        start_s,
        end_s,
        duration_s,
        start_anchor_s: raw_start,
        end_anchor_s: raw_end,
        start_observation_index: start.observation_index,
        end_observation_index: end.observation_index,
        start_role: start.role,
        end_role: end.role,
        start_source_event_index: start.source_event_index,
        end_source_event_index: end.source_event_index,
        start_score: start.boundary_score,
        end_score: end.boundary_score,
        interval_score: score,
        tier,
        reasons,
        cautions,
    })
}

pub fn build_shadow_plan(
    boundary: &BoundaryAnalysisResult,
    source_duration_s: f64,
    cfg: &ShadowPlanConfig,
) -> ShadowPlanResult {
    let mut candidates = boundary.candidates.clone();
    candidates.sort_by(|a, b| {
        a.anchor_pts_s
            .total_cmp(&b.anchor_pts_s)
            .then_with(|| a.observation_index.cmp(&b.observation_index))
    });

    let protect_head = cfg.always_keep_first.max(0.0);
    let protect_tail = (source_duration_s - cfg.always_keep_last.max(0.0)).max(protect_head);

    let mut pending_start: Option<BoundaryObservation> = None;
    let mut raw_intervals: Vec<ShadowInterval> = Vec::new();
    let mut rejected: Vec<RejectedShadowInterval> = Vec::new();

    for current in &candidates {
        let current_can_end = can_end(current.role, cfg.include_black);
        let current_can_start = can_start(current.role, cfg.include_black);

        if current_can_end {
            if let Some(start) = pending_start.as_ref() {
                if current.anchor_pts_s > start.anchor_pts_s
                    && current.source_event_index != start.source_event_index
                {
                    if let Some(interval) = make_interval(start, current, source_duration_s, cfg) {
                        let dur = interval.duration_s;
                        let rejection = if dur < cfg.min_commercial {
                            Some(format!("duration {:.3}s below min_commercial {:.3}s", dur, cfg.min_commercial))
                        } else if dur > cfg.max_commercial {
                            Some(format!("duration {:.3}s above max_commercial {:.3}s", dur, cfg.max_commercial))
                        } else if interval.start_s < protect_head {
                            Some(format!("starts inside protected head {:.3}s", protect_head))
                        } else if interval.end_s > protect_tail {
                            Some(format!("ends inside protected tail after {:.3}s", protect_tail))
                        } else {
                            None
                        };

                        if let Some(reason) = rejection {
                            rejected.push(RejectedShadowInterval {
                                start_anchor_s: interval.start_anchor_s,
                                end_anchor_s: interval.end_anchor_s,
                                duration_s: interval.duration_s,
                                start_observation_index: interval.start_observation_index,
                                end_observation_index: interval.end_observation_index,
                                reason,
                            });
                        } else {
                            raw_intervals.push(interval);
                        }
                    }
                }
            }
        }

        // A separator/fade can close the prior interval and immediately become
        // the start for the next one.  Interval exits also become starts when
        // black is excluded.  This mirrors the production black-gap semantics.
        if current_can_start {
            pending_start = Some(current.clone());
        }
    }

    // Match the current production min-show guard: don't allow a later removal
    // to leave a tiny preserved fragment between accepted removals.
    let mut removals = Vec::new();
    let mut cursor = 0.0_f64;
    for interval in raw_intervals {
        let keep_before = interval.start_s - cursor;
        if keep_before < cfg.min_show_segment && cursor > 0.0 {
            rejected.push(RejectedShadowInterval {
                start_anchor_s: interval.start_anchor_s,
                end_anchor_s: interval.end_anchor_s,
                duration_s: interval.duration_s,
                start_observation_index: interval.start_observation_index,
                end_observation_index: interval.end_observation_index,
                reason: format!(
                    "would leave {:.3}s preserved fragment below min_show_segment {:.3}s",
                    keep_before, cfg.min_show_segment
                ),
            });
            continue;
        }
        cursor = interval.end_s;
        removals.push(interval);
    }

    for (i, interval) in removals.iter_mut().enumerate() {
        interval.interval_index = i as u64;
    }

    let total_remove_s: f64 = removals.iter().map(|r| r.duration_s).sum();
    let total_keep_s = (source_duration_s - total_remove_s).max(0.0);
    let remove_ratio = if source_duration_s > 0.0 {
        total_remove_s / source_duration_s
    } else {
        0.0
    };
    let mut counts_by_tier = BTreeMap::new();
    for interval in &removals {
        *counts_by_tier.entry(interval.tier.as_str().to_string()).or_insert(0) += 1;
    }

    ShadowPlanResult {
        schema_version: 1,
        engine: "opencv-shadow-edit-plan".to_string(),
        planner_version: "cv4-shadow-v1".to_string(),
        config: cfg.clone(),
        summary: ShadowPlanSummary {
            source_duration_s,
            boundary_candidates_consumed: candidates.len() as u64,
            proposed_removals: removals.len() as u64,
            rejected_pairs: rejected.len() as u64,
            total_remove_s,
            total_keep_s,
            remove_ratio,
            counts_by_tier,
            first_removal_start_s: removals.first().map(|r| r.start_s),
            last_removal_end_s: removals.last().map(|r| r.end_s),
        },
        removals,
        rejected,
    }
}

fn overlap_iou(a_start: f64, a_end: f64, b_start: f64, b_end: f64) -> f64 {
    let intersection = (a_end.min(b_end) - a_start.max(b_start)).max(0.0);
    let union = (a_end.max(b_end) - a_start.min(b_start)).max(0.0);
    if union <= 0.0 { 0.0 } else { intersection / union }
}

pub fn compare_with_legacy(
    shadow: &ShadowPlanResult,
    legacy: &[LegacyPlanInterval],
    cfg: &ShadowPlanConfig,
) -> PlanComparisonResult {
    let mut legacy_used = vec![false; legacy.len()];
    let mut matches = Vec::new();
    let mut opencv_only = Vec::new();

    for s in &shadow.removals {
        let mut best: Option<(usize, f64, f64, f64)> = None;
        for (idx, l) in legacy.iter().enumerate() {
            if legacy_used[idx] { continue; }
            let start_off = s.start_s - l.start_s;
            let end_off = s.end_s - l.end_s;
            let iou = overlap_iou(s.start_s, s.end_s, l.start_s, l.end_s);
            let endpoints_close = start_off.abs() <= cfg.legacy_match_tolerance_s
                && end_off.abs() <= cfg.legacy_match_tolerance_s;
            if !endpoints_close && iou < cfg.legacy_match_iou_min { continue; }

            let quality = if endpoints_close { 2.0 + iou } else { iou };
            if best.as_ref().map(|b| quality > b.1).unwrap_or(true) {
                best = Some((idx, quality, start_off, end_off));
            }
        }

        if let Some((idx, _quality, start_off, end_off)) = best {
            legacy_used[idx] = true;
            let l = &legacy[idx];
            matches.push(PlanMatch {
                shadow_interval_index: s.interval_index,
                legacy_interval_index: l.interval_index,
                shadow_start_s: s.start_s,
                shadow_end_s: s.end_s,
                legacy_start_s: l.start_s,
                legacy_end_s: l.end_s,
                start_offset_s: start_off,
                end_offset_s: end_off,
                intersection_over_union: overlap_iou(s.start_s, s.end_s, l.start_s, l.end_s),
            });
        } else {
            opencv_only.push(s.interval_index);
        }
    }

    let legacy_only: Vec<u64> = legacy
        .iter()
        .enumerate()
        .filter_map(|(idx, l)| if legacy_used[idx] { None } else { Some(l.interval_index) })
        .collect();

    let mut boundary_offsets = Vec::new();
    for m in &matches {
        boundary_offsets.push(m.start_offset_s.abs());
        boundary_offsets.push(m.end_offset_s.abs());
    }
    let average_abs_boundary_offset_s = if boundary_offsets.is_empty() {
        None
    } else {
        Some(boundary_offsets.iter().sum::<f64>() / boundary_offsets.len() as f64)
    };
    let max_abs_boundary_offset_s = boundary_offsets.iter().copied().reduce(f64::max);

    PlanComparisonResult {
        schema_version: 1,
        engine: "opencv-vs-legacy-plan-comparison".to_string(),
        comparison_version: "cv4-comparison-v1".to_string(),
        match_tolerance_s: cfg.legacy_match_tolerance_s,
        match_iou_min: cfg.legacy_match_iou_min,
        summary: PlanComparisonSummary {
            shadow_intervals: shadow.removals.len() as u64,
            legacy_intervals: legacy.len() as u64,
            matched_intervals: matches.len() as u64,
            opencv_only_intervals: opencv_only.len() as u64,
            legacy_only_intervals: legacy_only.len() as u64,
            average_abs_boundary_offset_s,
            max_abs_boundary_offset_s,
        },
        matches,
        opencv_only_interval_indices: opencv_only,
        legacy_only_interval_indices: legacy_only,
    }
}

pub fn write_shadow_plan(outdir: &Path, result: &ShadowPlanResult) -> Result<Vec<PathBuf>> {
    fs::create_dir_all(outdir)?;
    let json_path = outdir.join("opencv_shadow_plan.json");
    fs::write(&json_path, serde_json::to_string_pretty(result)?)?;

    let summary_path = outdir.join("opencv_shadow_plan_summary.json");
    fs::write(&summary_path, serde_json::to_string_pretty(&result.summary)?)?;

    let csv_path = outdir.join("opencv_shadow_plan.csv");
    let mut csv = BufWriter::new(fs::File::create(&csv_path)?);
    writeln!(csv, "interval_index,start_s,end_s,duration_s,start_anchor_s,end_anchor_s,start_observation_index,end_observation_index,start_role,end_role,start_score,end_score,interval_score,tier")?;
    for r in &result.removals {
        writeln!(csv, "{},{:.6},{:.6},{:.6},{:.6},{:.6},{},{},{},{},{:.6},{:.6},{:.6},{}",
            r.interval_index, r.start_s, r.end_s, r.duration_s, r.start_anchor_s, r.end_anchor_s,
            r.start_observation_index, r.end_observation_index, r.start_role.as_str(), r.end_role.as_str(),
            r.start_score, r.end_score, r.interval_score, r.tier.as_str())?;
    }
    csv.flush()?;

    let rejected_path = outdir.join("opencv_shadow_rejected.csv");
    let mut rejected_csv = BufWriter::new(fs::File::create(&rejected_path)?);
    writeln!(rejected_csv, "start_anchor_s,end_anchor_s,duration_s,start_observation_index,end_observation_index,reason")?;
    for r in &result.rejected {
        let reason = r.reason.replace('"', "'");
        writeln!(rejected_csv, "{:.6},{:.6},{:.6},{},{},\"{}\"",
            r.start_anchor_s, r.end_anchor_s, r.duration_s,
            r.start_observation_index, r.end_observation_index, reason)?;
    }
    rejected_csv.flush()?;

    Ok(vec![json_path, summary_path, csv_path, rejected_path])
}

pub fn write_legacy_plan(outdir: &Path, legacy: &[LegacyPlanInterval]) -> Result<Vec<PathBuf>> {
    fs::create_dir_all(outdir)?;
    let json_path = outdir.join("legacy_default_plan.json");
    fs::write(&json_path, serde_json::to_string_pretty(legacy)?)?;

    let csv_path = outdir.join("legacy_default_plan.csv");
    let mut csv = BufWriter::new(fs::File::create(&csv_path)?);
    writeln!(csv, "interval_index,start_s,end_s,duration_s")?;
    for r in legacy {
        writeln!(csv, "{},{:.6},{:.6},{:.6}", r.interval_index, r.start_s, r.end_s, r.duration_s)?;
    }
    csv.flush()?;
    Ok(vec![json_path, csv_path])
}

pub fn write_plan_comparison(outdir: &Path, result: &PlanComparisonResult) -> Result<Vec<PathBuf>> {
    fs::create_dir_all(outdir)?;
    let json_path = outdir.join("opencv_vs_legacy_comparison.json");
    fs::write(&json_path, serde_json::to_string_pretty(result)?)?;

    let csv_path = outdir.join("opencv_vs_legacy_matches.csv");
    let mut csv = BufWriter::new(fs::File::create(&csv_path)?);
    writeln!(csv, "shadow_interval_index,legacy_interval_index,shadow_start_s,shadow_end_s,legacy_start_s,legacy_end_s,start_offset_s,end_offset_s,iou")?;
    for m in &result.matches {
        writeln!(csv, "{},{},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6}",
            m.shadow_interval_index, m.legacy_interval_index, m.shadow_start_s, m.shadow_end_s,
            m.legacy_start_s, m.legacy_end_s, m.start_offset_s, m.end_offset_s, m.intersection_over_union)?;
    }
    csv.flush()?;
    Ok(vec![json_path, csv_path])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_matches_current_adslicer_core_window() {
        let c = ShadowPlanConfig::default();
        assert_eq!(c.min_commercial, 5.0);
        assert_eq!(c.max_commercial, 240.0);
        assert_eq!(c.min_show_segment, 30.0);
        assert!(!c.include_black);
    }

    #[test]
    fn overlap_iou_behaves() {
        assert!((overlap_iou(0.0, 10.0, 0.0, 10.0) - 1.0).abs() < 1e-9);
        assert_eq!(overlap_iou(0.0, 1.0, 2.0, 3.0), 0.0);
    }
}
