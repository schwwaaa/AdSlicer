//! CV-3 shadow boundary candidate scoring for AdSlicer.
//!
//! CV-3 consumes CV-2 temporal events and produces ranked boundary observations.
//! It deliberately does NOT modify `build_plan()`, create remove/keep intervals,
//! or render media. Scores are versioned heuristics for development/QC and are
//! not calibrated probabilities.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use super::cv_temporal::{TemporalAnalysisResult, TemporalEvent, TemporalEventKind};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoundaryScoringConfig {
    /// Minimum heuristic score emitted as a boundary candidate. Lower-scored
    /// observations are preserved separately as evidence-only diagnostics.
    pub candidate_score_min: f64,
    pub review_score_min: f64,
    pub strong_score_min: f64,
    pub strict_black_bonus_ratio: f64,
    pub near_black_bonus_ratio: f64,
    pub strong_edge_delta: f64,
    pub medium_edge_delta: f64,
    pub strong_luma_contrast: f64,
    pub medium_luma_contrast: f64,
    pub chromatic_penalty: f64,
    /// Scene-only observations are intentionally capped until another evidence
    /// source corroborates them. Television naturally contains many scene cuts.
    pub scene_only_score_cap: f64,
    pub chromatic_dark_score_cap: f64,
}

impl Default for BoundaryScoringConfig {
    fn default() -> Self {
        Self {
            candidate_score_min: 0.45,
            review_score_min: 0.60,
            strong_score_min: 0.80,
            strict_black_bonus_ratio: 0.95,
            near_black_bonus_ratio: 0.98,
            strong_edge_delta: 80.0,
            medium_edge_delta: 40.0,
            strong_luma_contrast: 40.0,
            medium_luma_contrast: 20.0,
            chromatic_penalty: 0.25,
            scene_only_score_cap: 0.35,
            chromatic_dark_score_cap: 0.35,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoundaryRole {
    SeparatorAnchor,
    FadeValley,
    IntervalEntry,
    IntervalExit,
    SceneOnly,
    ChromaticDark,
}

impl BoundaryRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SeparatorAnchor => "separator_anchor",
            Self::FadeValley => "fade_valley",
            Self::IntervalEntry => "interval_entry",
            Self::IntervalExit => "interval_exit",
            Self::SceneOnly => "scene_only",
            Self::ChromaticDark => "chromatic_dark",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoundaryTier {
    Strong,
    Review,
    Weak,
    EvidenceOnly,
}

impl BoundaryTier {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Strong => "strong",
            Self::Review => "review",
            Self::Weak => "weak",
            Self::EvidenceOnly => "evidence_only",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoundaryObservation {
    pub observation_index: u64,
    pub source_event_index: u64,
    pub source_event_kind: TemporalEventKind,
    pub role: BoundaryRole,

    /// The point CV-3 proposes for later planner comparison. Interval events
    /// expose entry/exit edges separately; short separators and fades use one
    /// anchor point. This is NOT yet an active edit timestamp.
    pub anchor_frame: u64,
    pub anchor_pts_s: f64,
    pub evidence_start_frame: u64,
    pub evidence_end_frame: u64,
    pub evidence_start_pts_s: f64,
    pub evidence_end_pts_s: f64,

    /// Versioned heuristic [0,1], not a calibrated probability.
    pub boundary_score: f64,
    pub tier: BoundaryTier,
    pub is_candidate: bool,

    pub edge_delta: f64,
    pub luma_contrast: f64,
    pub max_black_pixel_ratio: f64,
    pub max_near_black_pixel_ratio: f64,
    pub chromatic_dark: bool,

    pub reasons: Vec<String>,
    pub cautions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoundarySummary {
    pub total_observations: u64,
    pub total_candidates: u64,
    pub evidence_only: u64,
    pub counts_by_tier: BTreeMap<String, u64>,
    pub counts_by_role: BTreeMap<String, u64>,
    pub first_candidate_pts_s: Option<f64>,
    pub last_candidate_pts_s: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoundaryAnalysisResult {
    pub schema_version: u32,
    pub engine: String,
    pub score_version: String,
    pub source_temporal_file: Option<String>,
    pub source_temporal_sha256: Option<String>,
    pub config: BoundaryScoringConfig,
    pub summary: BoundarySummary,
    pub candidates: Vec<BoundaryObservation>,
    pub evidence_only: Vec<BoundaryObservation>,
}

fn edge_bonus(delta: f64, cfg: &BoundaryScoringConfig) -> (f64, Option<String>) {
    if delta >= cfg.strong_edge_delta {
        (0.12, Some(format!("strong visual edge delta {:.2}", delta)))
    } else if delta >= cfg.medium_edge_delta {
        (0.08, Some(format!("visual edge delta {:.2}", delta)))
    } else {
        (0.0, None)
    }
}

fn contrast_bonus(contrast: f64, cfg: &BoundaryScoringConfig) -> (f64, Option<String>) {
    if contrast >= cfg.strong_luma_contrast {
        (0.10, Some(format!("strong luma contrast {:.2}", contrast)))
    } else if contrast >= cfg.medium_luma_contrast {
        (0.06, Some(format!("luma contrast {:.2}", contrast)))
    } else {
        (0.0, None)
    }
}

fn tier_for_score(score: f64, cfg: &BoundaryScoringConfig) -> BoundaryTier {
    if score >= cfg.strong_score_min {
        BoundaryTier::Strong
    } else if score >= cfg.review_score_min {
        BoundaryTier::Review
    } else if score >= cfg.candidate_score_min {
        BoundaryTier::Weak
    } else {
        BoundaryTier::EvidenceOnly
    }
}

fn score_observation(
    event: &TemporalEvent,
    role: BoundaryRole,
    anchor_frame: u64,
    anchor_pts_s: f64,
    edge_delta: f64,
    luma_contrast: f64,
    cfg: &BoundaryScoringConfig,
) -> BoundaryObservation {
    let mut reasons = Vec::new();
    let mut cautions = Vec::new();

    let mut score = match event.kind {
        TemporalEventKind::BlackSeparator => 0.72,
        TemporalEventKind::NearBlackSeparator => 0.66,
        TemporalEventKind::FadeValley => 0.64,
        TemporalEventKind::BlackInterval => 0.50,
        TemporalEventKind::NearBlackInterval => 0.44,
        TemporalEventKind::ChromaticDark => 0.18,
        TemporalEventKind::SceneDiscontinuity => 0.18,
    };

    reasons.push(format!("source temporal event {}", event.kind.as_str()));

    if event.max_black_pixel_ratio >= cfg.strict_black_bonus_ratio {
        score += 0.10;
        reasons.push(format!(
            "strict-black evidence {:.3}",
            event.max_black_pixel_ratio
        ));
    }
    if event.max_near_black_pixel_ratio >= cfg.near_black_bonus_ratio {
        score += 0.08;
        reasons.push(format!(
            "near-black evidence {:.3}",
            event.max_near_black_pixel_ratio
        ));
    }

    let (eb, er) = edge_bonus(edge_delta, cfg);
    score += eb;
    if let Some(r) = er { reasons.push(r); }

    let (cb, cr) = contrast_bonus(luma_contrast, cfg);
    score += cb;
    if let Some(r) = cr { reasons.push(r); }

    match event.kind {
        TemporalEventKind::BlackSeparator | TemporalEventKind::NearBlackSeparator => {
            score += 0.06;
            reasons.push(format!("short separator: {} frame(s)", event.duration_frames));
        }
        TemporalEventKind::FadeValley => {
            if event.descending_steps >= 2 && event.ascending_steps >= 2 {
                score += 0.08;
                reasons.push(format!(
                    "fade shape: {} falling / {} rising steps",
                    event.descending_steps, event.ascending_steps
                ));
            }
            if event.fall_magnitude >= 20.0 && event.rise_magnitude >= 20.0 {
                score += 0.06;
                reasons.push(format!(
                    "two-sided luminance valley: fall {:.2}, rise {:.2}",
                    event.fall_magnitude, event.rise_magnitude
                ));
            }
        }
        TemporalEventKind::SceneDiscontinuity => {
            cautions.push("scene change alone is not sufficient for a commercial boundary".to_string());
            score = score.min(cfg.scene_only_score_cap);
        }
        TemporalEventKind::ChromaticDark => {
            score -= cfg.chromatic_penalty;
            cautions.push(format!(
                "dark frame retains chromatic structure (sat {:.2}, spread {:.2})",
                event.mean_saturation, event.max_channel_spread
            ));
            score = score.min(cfg.chromatic_dark_score_cap);
        }
        _ => {}
    }

    if event.chromatic_dark && event.kind != TemporalEventKind::ChromaticDark {
        score -= cfg.chromatic_penalty;
        cautions.push("chromatic-dark evidence reduces black-boundary confidence".to_string());
    }

    score = score.clamp(0.0, 1.0);
    let tier = tier_for_score(score, cfg);
    let is_candidate = score >= cfg.candidate_score_min;

    BoundaryObservation {
        observation_index: 0,
        source_event_index: event.event_index,
        source_event_kind: event.kind,
        role,
        anchor_frame,
        anchor_pts_s,
        evidence_start_frame: event.start_frame,
        evidence_end_frame: event.end_frame,
        evidence_start_pts_s: event.start_pts_s,
        evidence_end_pts_s: event.end_pts_s,
        boundary_score: score,
        tier,
        is_candidate,
        edge_delta,
        luma_contrast,
        max_black_pixel_ratio: event.max_black_pixel_ratio,
        max_near_black_pixel_ratio: event.max_near_black_pixel_ratio,
        chromatic_dark: event.chromatic_dark,
        reasons,
        cautions,
    }
}

fn observations_for_event(event: &TemporalEvent, cfg: &BoundaryScoringConfig) -> Vec<BoundaryObservation> {
    match event.kind {
        TemporalEventKind::BlackInterval | TemporalEventKind::NearBlackInterval => vec![
            score_observation(
                event,
                BoundaryRole::IntervalEntry,
                event.start_frame,
                event.start_pts_s,
                event.entry_delta_mean,
                event.fall_magnitude,
                cfg,
            ),
            score_observation(
                event,
                BoundaryRole::IntervalExit,
                event.end_frame,
                event.end_pts_s,
                event.exit_delta_mean,
                event.rise_magnitude,
                cfg,
            ),
        ],
        TemporalEventKind::BlackSeparator | TemporalEventKind::NearBlackSeparator => vec![
            score_observation(
                event,
                BoundaryRole::SeparatorAnchor,
                event.valley_frame,
                event.valley_pts_s,
                event.peak_delta_mean,
                event.fall_magnitude.max(event.rise_magnitude),
                cfg,
            ),
        ],
        TemporalEventKind::FadeValley => vec![score_observation(
            event,
            BoundaryRole::FadeValley,
            event.valley_frame,
            event.valley_pts_s,
            event.peak_delta_mean,
            event.fall_magnitude.min(event.rise_magnitude),
            cfg,
        )],
        TemporalEventKind::SceneDiscontinuity => vec![score_observation(
            event,
            BoundaryRole::SceneOnly,
            event.start_frame,
            event.start_pts_s,
            event.peak_delta_mean,
            event.fall_magnitude.max(event.rise_magnitude),
            cfg,
        )],
        TemporalEventKind::ChromaticDark => vec![score_observation(
            event,
            BoundaryRole::ChromaticDark,
            event.valley_frame,
            event.valley_pts_s,
            event.peak_delta_mean,
            event.fall_magnitude.max(event.rise_magnitude),
            cfg,
        )],
    }
}

pub fn score_boundary_candidates(
    temporal: &TemporalAnalysisResult,
    cfg: &BoundaryScoringConfig,
) -> BoundaryAnalysisResult {
    let mut all = Vec::new();
    for event in &temporal.events {
        all.extend(observations_for_event(event, cfg));
    }

    all.sort_by(|a, b| {
        a.anchor_frame
            .cmp(&b.anchor_frame)
            .then_with(|| a.role.as_str().cmp(b.role.as_str()))
    });
    for (i, observation) in all.iter_mut().enumerate() {
        observation.observation_index = i as u64;
    }

    let candidates: Vec<_> = all.iter().filter(|o| o.is_candidate).cloned().collect();
    let evidence_only: Vec<_> = all.iter().filter(|o| !o.is_candidate).cloned().collect();

    let mut counts_by_tier = BTreeMap::new();
    let mut counts_by_role = BTreeMap::new();
    for observation in &all {
        *counts_by_tier.entry(observation.tier.as_str().to_string()).or_insert(0) += 1;
        *counts_by_role.entry(observation.role.as_str().to_string()).or_insert(0) += 1;
    }

    BoundaryAnalysisResult {
        schema_version: 1,
        engine: "opencv-boundary-candidates".to_string(),
        score_version: "cv3-heuristic-v1".to_string(),
        source_temporal_file: None,
        source_temporal_sha256: None,
        config: cfg.clone(),
        summary: BoundarySummary {
            total_observations: all.len() as u64,
            total_candidates: candidates.len() as u64,
            evidence_only: evidence_only.len() as u64,
            counts_by_tier,
            counts_by_role,
            first_candidate_pts_s: candidates.first().map(|o| o.anchor_pts_s),
            last_candidate_pts_s: candidates.last().map(|o| o.anchor_pts_s),
        },
        candidates,
        evidence_only,
    }
}

pub fn read_temporal_analysis_json(path: &Path) -> Result<TemporalAnalysisResult> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

pub fn write_boundary_candidates(outdir: &Path, result: &BoundaryAnalysisResult) -> Result<Vec<PathBuf>> {
    fs::create_dir_all(outdir)?;

    let json_path = outdir.join("opencv_boundary_candidates.json");
    fs::write(&json_path, serde_json::to_string_pretty(result)?)?;

    let summary_path = outdir.join("opencv_boundary_summary.json");
    fs::write(&summary_path, serde_json::to_string_pretty(&result.summary)?)?;

    let csv_path = outdir.join("opencv_boundary_candidates.csv");
    write_observation_csv(&csv_path, &result.candidates)?;

    let evidence_path = outdir.join("opencv_boundary_evidence_only.csv");
    write_observation_csv(&evidence_path, &result.evidence_only)?;

    Ok(vec![json_path, summary_path, csv_path, evidence_path])
}

fn write_observation_csv(path: &Path, rows: &[BoundaryObservation]) -> Result<()> {
    let mut csv = BufWriter::new(fs::File::create(path)?);
    writeln!(
        csv,
        "observation_index,source_event_index,source_event_kind,role,anchor_frame,anchor_pts_s,evidence_start_frame,evidence_end_frame,evidence_start_pts_s,evidence_end_pts_s,boundary_score,tier,edge_delta,luma_contrast,max_black_pixel_ratio,max_near_black_pixel_ratio,chromatic_dark"
    )?;
    for o in rows {
        writeln!(
            csv,
            "{},{},{},{},{},{:.6},{},{},{:.6},{:.6},{:.6},{},{:.6},{:.6},{:.8},{:.8},{}",
            o.observation_index,
            o.source_event_index,
            o.source_event_kind.as_str(),
            o.role.as_str(),
            o.anchor_frame,
            o.anchor_pts_s,
            o.evidence_start_frame,
            o.evidence_end_frame,
            o.evidence_start_pts_s,
            o.evidence_end_pts_s,
            o.boundary_score,
            o.tier.as_str(),
            o.edge_delta,
            o.luma_contrast,
            o.max_black_pixel_ratio,
            o.max_near_black_pixel_ratio,
            o.chromatic_dark,
        )?;
    }
    csv.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scoring_thresholds_are_ordered() {
        let c = BoundaryScoringConfig::default();
        assert!(c.candidate_score_min < c.review_score_min);
        assert!(c.review_score_min < c.strong_score_min);
        assert!(c.strong_score_min <= 1.0);
        assert!(c.scene_only_score_cap < c.candidate_score_min);
        assert!(c.chromatic_dark_score_cap < c.candidate_score_min);
    }
}
