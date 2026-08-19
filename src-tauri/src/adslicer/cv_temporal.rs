//! CV-2 temporal evidence engine for AdSlicer.
//!
//! This module interprets the per-frame measurements produced by CV-1 and
//! groups them into reproducible temporal events. It deliberately does NOT
//! create commercial intervals, alter `build_plan()`, or render cuts.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use super::cv_detect::FrameMetrics;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalAnalysisConfig {
    /// A frame is considered strongly near-black when this fraction of pixels
    /// are at/below the CV-1 near-black luma threshold.
    pub near_black_ratio_min: f64,
    /// Strong strict-black evidence threshold.
    pub strict_black_ratio_min: f64,
    /// Secondary dark criterion for raised-black analog material.
    pub dark_mean_luma_max: f64,
    /// Bridge tiny holes inside a dark event without invoking the higher-level
    /// commercial merge-gap logic.
    pub micro_bridge_frames: u64,
    /// Number of analyzed frames on each side used to describe the local shape.
    pub context_frames: usize,
    /// Minimum per-frame luma movement counted as a meaningful fade step.
    pub fade_step_luma_min: f64,
    /// Minimum number of falling AND rising steps required for a fade valley.
    pub fade_monotonic_steps_min: u32,
    /// Required fall/rise magnitude relative to the local valley.
    pub fade_magnitude_min: f64,
    /// Scene-discontinuity evidence threshold using CV-1 frame delta.
    pub scene_delta_min: f64,
    /// Do not emit separate scene-discontinuity events this close to a dark
    /// event because entry/exit deltas are already preserved on that event.
    pub scene_suppression_frames: u64,
    /// Short dark clusters receive an explicit short-event classification.
    pub short_event_max_frames: u64,
    /// Chromatic darkness diagnostics. These identify dark colored slates or
    /// tinted frames rather than deleting the event from the evidence stream.
    pub chromatic_saturation_min: f64,
    pub chromatic_channel_spread_min: f64,
}

impl Default for TemporalAnalysisConfig {
    fn default() -> Self {
        Self {
            near_black_ratio_min: 0.90,
            strict_black_ratio_min: 0.90,
            dark_mean_luma_max: 32.0,
            micro_bridge_frames: 2,
            context_frames: 5,
            fade_step_luma_min: 5.0,
            fade_monotonic_steps_min: 2,
            fade_magnitude_min: 20.0,
            scene_delta_min: 40.0,
            scene_suppression_frames: 5,
            short_event_max_frames: 2,
            chromatic_saturation_min: 120.0,
            chromatic_channel_spread_min: 40.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TemporalEventKind {
    BlackInterval,
    BlackSeparator,
    NearBlackInterval,
    NearBlackSeparator,
    FadeValley,
    ChromaticDark,
    SceneDiscontinuity,
}

impl TemporalEventKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::BlackInterval => "black_interval",
            Self::BlackSeparator => "black_separator",
            Self::NearBlackInterval => "near_black_interval",
            Self::NearBlackSeparator => "near_black_separator",
            Self::FadeValley => "fade_valley",
            Self::ChromaticDark => "chromatic_dark",
            Self::SceneDiscontinuity => "scene_discontinuity",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalEvent {
    pub event_index: u64,
    pub kind: TemporalEventKind,

    pub start_frame: u64,
    pub valley_frame: u64,
    pub end_frame: u64,
    pub start_pts_s: f64,
    pub valley_pts_s: f64,
    pub end_pts_s: f64,
    pub duration_frames: u64,
    pub duration_s: f64,

    pub min_mean_luma: f64,
    pub max_black_pixel_ratio: f64,
    pub max_near_black_pixel_ratio: f64,
    pub mean_saturation: f64,
    pub max_channel_spread: f64,
    pub chromatic_dark: bool,

    pub luma_before_mean: f64,
    pub luma_after_mean: f64,
    pub fall_magnitude: f64,
    pub rise_magnitude: f64,
    pub descending_steps: u32,
    pub ascending_steps: u32,

    pub entry_delta_mean: f64,
    pub exit_delta_mean: f64,
    pub peak_delta_mean: f64,

    /// Human-readable evidence only. This is intentionally not a commercial
    /// edit decision or planner confidence value.
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalSummary {
    pub total_events: u64,
    pub counts_by_kind: BTreeMap<String, u64>,
    pub first_event_pts_s: Option<f64>,
    pub last_event_pts_s: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalAnalysisResult {
    pub schema_version: u32,
    pub engine: String,
    pub source_evidence_file: Option<String>,
    pub source_evidence_sha256: Option<String>,
    pub config: TemporalAnalysisConfig,
    pub summary: TemporalSummary,
    pub events: Vec<TemporalEvent>,
}

fn mean(values: impl Iterator<Item = f64>) -> Option<f64> {
    let mut n = 0usize;
    let mut total = 0.0;
    for v in values {
        n += 1;
        total += v;
    }
    if n == 0 { None } else { Some(total / n as f64) }
}

fn max_channel_spread(f: &FrameMetrics) -> f64 {
    (f.mean_blue - f.mean_green)
        .abs()
        .max((f.mean_blue - f.mean_red).abs())
        .max((f.mean_green - f.mean_red).abs())
}

fn is_dark_frame(f: &FrameMetrics, cfg: &TemporalAnalysisConfig) -> bool {
    f.near_black_pixel_ratio >= cfg.near_black_ratio_min || f.mean_luma <= cfg.dark_mean_luma_max
}

fn build_dark_event(
    frames: &[FrameMetrics],
    start_pos: usize,
    end_pos: usize,
    cfg: &TemporalAnalysisConfig,
) -> TemporalEvent {
    let segment = &frames[start_pos..=end_pos];
    let valley_offset = segment
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| a.mean_luma.total_cmp(&b.mean_luma))
        .map(|(i, _)| i)
        .unwrap_or(0);
    let valley_pos = start_pos + valley_offset;
    let valley = &frames[valley_pos];

    let pre_start = start_pos.saturating_sub(cfg.context_frames);
    let post_end = (end_pos + cfg.context_frames).min(frames.len().saturating_sub(1));

    let luma_before_mean = mean(frames[pre_start..start_pos].iter().map(|f| f.mean_luma))
        .unwrap_or(frames[start_pos].mean_luma);
    let luma_after_mean = if end_pos + 1 < frames.len() {
        mean(frames[end_pos + 1..=post_end].iter().map(|f| f.mean_luma))
            .unwrap_or(frames[end_pos].mean_luma)
    } else {
        frames[end_pos].mean_luma
    };

    let descending_steps = frames[pre_start..=start_pos]
        .windows(2)
        .filter(|w| w[1].mean_luma - w[0].mean_luma <= -cfg.fade_step_luma_min)
        .count() as u32;
    let ascending_steps = frames[end_pos..=post_end]
        .windows(2)
        .filter(|w| w[1].mean_luma - w[0].mean_luma >= cfg.fade_step_luma_min)
        .count() as u32;

    let min_mean_luma = segment.iter().map(|f| f.mean_luma).fold(f64::INFINITY, f64::min);
    let max_black_pixel_ratio = segment.iter().map(|f| f.black_pixel_ratio).fold(0.0, f64::max);
    let max_near_black_pixel_ratio = segment.iter().map(|f| f.near_black_pixel_ratio).fold(0.0, f64::max);
    let mean_saturation = mean(segment.iter().map(|f| f.mean_saturation)).unwrap_or(0.0);
    let max_channel_spread = segment.iter().map(max_channel_spread).fold(0.0, f64::max);
    let chromatic_dark = mean_saturation >= cfg.chromatic_saturation_min
        && max_channel_spread >= cfg.chromatic_channel_spread_min;

    let fall_magnitude = (luma_before_mean - min_mean_luma).max(0.0);
    let rise_magnitude = (luma_after_mean - min_mean_luma).max(0.0);
    let duration_frames = frames[end_pos]
        .frame_index
        .saturating_sub(frames[start_pos].frame_index)
        .saturating_add(1);
    let duration_s = (frames[end_pos].pts_s - frames[start_pos].pts_s).max(0.0);

    let entry_delta_mean = frames[start_pos].frame_delta_mean;
    let exit_delta_mean = if end_pos + 1 < frames.len() {
        frames[end_pos + 1].frame_delta_mean
    } else {
        0.0
    };
    let peak_end = (end_pos + 1).min(frames.len().saturating_sub(1));
    let peak_delta_mean = frames[start_pos..=peak_end]
        .iter()
        .map(|f| f.frame_delta_mean)
        .fold(0.0, f64::max);

    let fade_shape = descending_steps >= cfg.fade_monotonic_steps_min
        && ascending_steps >= cfg.fade_monotonic_steps_min
        && fall_magnitude >= cfg.fade_magnitude_min
        && rise_magnitude >= cfg.fade_magnitude_min;

    let kind = if chromatic_dark && max_black_pixel_ratio < cfg.strict_black_ratio_min {
        TemporalEventKind::ChromaticDark
    } else if duration_frames <= cfg.short_event_max_frames
        && max_black_pixel_ratio >= cfg.strict_black_ratio_min
    {
        TemporalEventKind::BlackSeparator
    } else if duration_frames <= cfg.short_event_max_frames
        && max_near_black_pixel_ratio >= cfg.near_black_ratio_min
    {
        TemporalEventKind::NearBlackSeparator
    } else if fade_shape {
        TemporalEventKind::FadeValley
    } else if max_black_pixel_ratio >= cfg.strict_black_ratio_min {
        TemporalEventKind::BlackInterval
    } else {
        TemporalEventKind::NearBlackInterval
    };

    let mut evidence = Vec::new();
    evidence.push(format!("minimum mean luma {:.2}", min_mean_luma));
    evidence.push(format!("max near-black ratio {:.3}", max_near_black_pixel_ratio));
    if max_black_pixel_ratio >= cfg.strict_black_ratio_min {
        evidence.push(format!("strict-black ratio {:.3}", max_black_pixel_ratio));
    }
    if fade_shape {
        evidence.push(format!(
            "fall/rise pattern: {} descending, {} ascending steps",
            descending_steps, ascending_steps
        ));
    }
    if peak_delta_mean >= cfg.scene_delta_min {
        evidence.push(format!("strong visual discontinuity {:.2}", peak_delta_mean));
    }
    if chromatic_dark {
        evidence.push(format!(
            "dark chromatic evidence: saturation {:.2}, channel spread {:.2}",
            mean_saturation, max_channel_spread
        ));
    }

    TemporalEvent {
        event_index: 0,
        kind,
        start_frame: frames[start_pos].frame_index,
        valley_frame: valley.frame_index,
        end_frame: frames[end_pos].frame_index,
        start_pts_s: frames[start_pos].pts_s,
        valley_pts_s: valley.pts_s,
        end_pts_s: frames[end_pos].pts_s,
        duration_frames,
        duration_s,
        min_mean_luma,
        max_black_pixel_ratio,
        max_near_black_pixel_ratio,
        mean_saturation,
        max_channel_spread,
        chromatic_dark,
        luma_before_mean,
        luma_after_mean,
        fall_magnitude,
        rise_magnitude,
        descending_steps,
        ascending_steps,
        entry_delta_mean,
        exit_delta_mean,
        peak_delta_mean,
        evidence,
    }
}

fn frame_is_suppressed(frame_index: u64, dark_ranges: &[(u64, u64)], radius: u64) -> bool {
    dark_ranges.iter().any(|(start, end)| {
        let low = start.saturating_sub(radius);
        let high = end.saturating_add(radius);
        frame_index >= low && frame_index <= high
    })
}

pub fn analyze_temporal_events(
    frames: &[FrameMetrics],
    cfg: &TemporalAnalysisConfig,
) -> TemporalAnalysisResult {
    if frames.is_empty() {
        return TemporalAnalysisResult {
            schema_version: 1,
            engine: "opencv-temporal-evidence".to_string(),
            source_evidence_file: None,
            source_evidence_sha256: None,
            config: cfg.clone(),
            summary: TemporalSummary {
                total_events: 0,
                counts_by_kind: BTreeMap::new(),
                first_event_pts_s: None,
                last_event_pts_s: None,
            },
            events: Vec::new(),
        };
    }

    // First find dark-like observations, then group them with only a tiny
    // micro-bridge. This is deliberately separate from commercial merging.
    let dark_positions: Vec<usize> = frames
        .iter()
        .enumerate()
        .filter_map(|(i, f)| is_dark_frame(f, cfg).then_some(i))
        .collect();

    let mut grouped_positions: Vec<(usize, usize)> = Vec::new();
    if let Some(&first_pos) = dark_positions.first() {
        let mut start_pos = first_pos;
        let mut previous_pos = first_pos;
        for &pos in dark_positions.iter().skip(1) {
            let previous_frame = frames[previous_pos].frame_index;
            let current_frame = frames[pos].frame_index;
            let missing_frames = current_frame.saturating_sub(previous_frame).saturating_sub(1);
            if missing_frames <= cfg.micro_bridge_frames {
                previous_pos = pos;
            } else {
                grouped_positions.push((start_pos, previous_pos));
                start_pos = pos;
                previous_pos = pos;
            }
        }
        grouped_positions.push((start_pos, previous_pos));
    }

    let mut events: Vec<TemporalEvent> = grouped_positions
        .iter()
        .map(|&(start, end)| build_dark_event(frames, start, end, cfg))
        .collect();

    let dark_ranges: Vec<(u64, u64)> = events
        .iter()
        .map(|e| (e.start_frame, e.end_frame))
        .collect();

    // Scene discontinuities are preserved as evidence but suppressed around
    // dark events because their entry/exit deltas already describe those edges.
    if frames.len() >= 3 {
        for i in 1..frames.len() - 1 {
            let f = &frames[i];
            if f.frame_delta_mean < cfg.scene_delta_min
                || f.frame_delta_mean < frames[i - 1].frame_delta_mean
                || f.frame_delta_mean < frames[i + 1].frame_delta_mean
                || frame_is_suppressed(f.frame_index, &dark_ranges, cfg.scene_suppression_frames)
            {
                continue;
            }

            events.push(TemporalEvent {
                event_index: 0,
                kind: TemporalEventKind::SceneDiscontinuity,
                start_frame: f.frame_index,
                valley_frame: f.frame_index,
                end_frame: f.frame_index,
                start_pts_s: f.pts_s,
                valley_pts_s: f.pts_s,
                end_pts_s: f.pts_s,
                duration_frames: 1,
                duration_s: 0.0,
                min_mean_luma: f.mean_luma,
                max_black_pixel_ratio: f.black_pixel_ratio,
                max_near_black_pixel_ratio: f.near_black_pixel_ratio,
                mean_saturation: f.mean_saturation,
                max_channel_spread: max_channel_spread(f),
                chromatic_dark: false,
                luma_before_mean: frames[i - 1].mean_luma,
                luma_after_mean: frames[i + 1].mean_luma,
                fall_magnitude: (frames[i - 1].mean_luma - f.mean_luma).max(0.0),
                rise_magnitude: (frames[i + 1].mean_luma - f.mean_luma).max(0.0),
                descending_steps: 0,
                ascending_steps: 0,
                entry_delta_mean: f.frame_delta_mean,
                exit_delta_mean: frames[i + 1].frame_delta_mean,
                peak_delta_mean: f.frame_delta_mean,
                evidence: vec![format!("scene discontinuity delta {:.2}", f.frame_delta_mean)],
            });
        }
    }

    events.sort_by(|a, b| {
        a.start_frame
            .cmp(&b.start_frame)
            .then_with(|| a.end_frame.cmp(&b.end_frame))
            .then_with(|| a.kind.as_str().cmp(b.kind.as_str()))
    });
    for (i, event) in events.iter_mut().enumerate() {
        event.event_index = i as u64;
    }

    let mut counts_by_kind = BTreeMap::new();
    for event in &events {
        *counts_by_kind.entry(event.kind.as_str().to_string()).or_insert(0) += 1;
    }

    let summary = TemporalSummary {
        total_events: events.len() as u64,
        counts_by_kind,
        first_event_pts_s: events.first().map(|e| e.start_pts_s),
        last_event_pts_s: events.last().map(|e| e.end_pts_s),
    };

    TemporalAnalysisResult {
        schema_version: 1,
        engine: "opencv-temporal-evidence".to_string(),
        source_evidence_file: None,
        source_evidence_sha256: None,
        config: cfg.clone(),
        summary,
        events,
    }
}

pub fn write_temporal_events(outdir: &Path, result: &TemporalAnalysisResult) -> Result<Vec<PathBuf>> {
    fs::create_dir_all(outdir)?;

    let json_path = outdir.join("opencv_temporal_events.json");
    fs::write(&json_path, serde_json::to_string_pretty(result)?)?;

    let summary_path = outdir.join("opencv_temporal_summary.json");
    fs::write(&summary_path, serde_json::to_string_pretty(&result.summary)?)?;

    let csv_path = outdir.join("opencv_temporal_events.csv");
    let mut csv = BufWriter::new(fs::File::create(&csv_path)?);
    writeln!(
        csv,
        "event_index,kind,start_frame,valley_frame,end_frame,start_pts_s,valley_pts_s,end_pts_s,duration_frames,duration_s,min_mean_luma,max_black_pixel_ratio,max_near_black_pixel_ratio,mean_saturation,max_channel_spread,chromatic_dark,luma_before_mean,luma_after_mean,fall_magnitude,rise_magnitude,descending_steps,ascending_steps,entry_delta_mean,exit_delta_mean,peak_delta_mean"
    )?;
    for e in &result.events {
        writeln!(
            csv,
            "{},{},{},{},{},{:.6},{:.6},{:.6},{},{:.6},{:.6},{:.8},{:.8},{:.6},{:.6},{},{:.6},{:.6},{:.6},{:.6},{},{},{:.6},{:.6},{:.6}",
            e.event_index,
            e.kind.as_str(),
            e.start_frame,
            e.valley_frame,
            e.end_frame,
            e.start_pts_s,
            e.valley_pts_s,
            e.end_pts_s,
            e.duration_frames,
            e.duration_s,
            e.min_mean_luma,
            e.max_black_pixel_ratio,
            e.max_near_black_pixel_ratio,
            e.mean_saturation,
            e.max_channel_spread,
            e.chromatic_dark,
            e.luma_before_mean,
            e.luma_after_mean,
            e.fall_magnitude,
            e.rise_magnitude,
            e.descending_steps,
            e.ascending_steps,
            e.entry_delta_mean,
            e.exit_delta_mean,
            e.peak_delta_mean,
        )?;
    }
    csv.flush()?;

    Ok(vec![json_path, summary_path, csv_path])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temporal_defaults_keep_dark_and_scene_rules_separate() {
        let c = TemporalAnalysisConfig::default();
        assert!(c.near_black_ratio_min > 0.0 && c.near_black_ratio_min <= 1.0);
        assert!(c.strict_black_ratio_min > 0.0 && c.strict_black_ratio_min <= 1.0);
        assert!(c.micro_bridge_frames <= c.scene_suppression_frames);
        assert!(c.fade_monotonic_steps_min >= 1);
    }
}
