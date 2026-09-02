//! CV-6 full-timeline segmentation render validation.
//!
//! Renders complete coverage plans produced by `cv_structural` so the user can
//! inspect every resulting clip directly. Unlike CV-5 removal previews, this
//! stage never hides residual material: every source interval belongs to one
//! segment in each rendered mode.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::cv_structural::{SegmentationMode, SegmentationPlan, StructuralSegmentationResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SegmentRenderMode {
    Complete,
    Every,
    Both,
    None,
}

impl SegmentRenderMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::Every => "every",
            Self::Both => "both",
            Self::None => "none",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentRenderConfig {
    pub mode: SegmentRenderMode,
    pub video_codec: String,
    pub video_crf: u8,
    pub video_preset: String,
    pub audio_codec: String,
    pub audio_bitrate: String,
}

impl Default for SegmentRenderConfig {
    fn default() -> Self {
        Self {
            mode: SegmentRenderMode::Both,
            video_codec: "libx264".to_string(),
            video_crf: 18,
            video_preset: "veryfast".to_string(),
            audio_codec: "aac".to_string(),
            audio_bitrate: "160k".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderedSegment {
    pub mode: SegmentationMode,
    pub segment_index: u64,
    pub start_s: f64,
    pub end_s: f64,
    pub planned_duration_s: f64,
    pub output_file: String,
    pub output_duration_s: Option<f64>,
    pub status: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentRenderSummary {
    pub complete_segments_requested: u64,
    pub every_separator_segments_requested: u64,
    pub rendered_ok: u64,
    pub rendered_failed: u64,
    pub source_has_audio: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentRenderResult {
    pub schema_version: u32,
    pub engine: String,
    pub render_version: String,
    pub production_render_performed: bool,
    pub config: SegmentRenderConfig,
    pub summary: SegmentRenderSummary,
    pub items: Vec<RenderedSegment>,
}

fn run_ffmpeg(ffmpeg: &Path, args: &[String], purpose: &str) -> Result<()> {
    let status = Command::new(ffmpeg)
        .args(args)
        .status()
        .map_err(|e| anyhow!("Failed to launch ffmpeg for {purpose}: {e}"))?;
    if !status.success() {
        return Err(anyhow!("ffmpeg {purpose} failed with status {status}"));
    }
    Ok(())
}

fn source_has_audio(ffprobe: &Path, input: &Path) -> bool {
    Command::new(ffprobe)
        .args([
            "-v", "error",
            "-select_streams", "a:0",
            "-show_entries", "stream=index",
            "-of", "csv=p=0",
        ])
        .arg(input)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| !String::from_utf8_lossy(&o.stdout).trim().is_empty())
        .unwrap_or(false)
}

fn probe_duration(ffprobe: &Path, media: &Path) -> Option<f64> {
    let out = Command::new(ffprobe)
        .args([
            "-v", "error",
            "-show_entries", "format=duration",
            "-of", "default=noprint_wrappers=1:nokey=1",
        ])
        .arg(media)
        .output()
        .ok()?;
    if !out.status.success() { return None; }
    String::from_utf8_lossy(&out.stdout).trim().parse::<f64>().ok()
}

fn render_plan(
    ffmpeg: &Path,
    ffprobe: &Path,
    input: &Path,
    plan: &SegmentationPlan,
    mode_dir: &Path,
    cfg: &SegmentRenderConfig,
    items: &mut Vec<RenderedSegment>,
) -> Result<()> {
    fs::create_dir_all(mode_dir)?;
    for segment in &plan.segments {
        let filename = format!("segment-{:03}.mp4", segment.segment_index + 1);
        let output = mode_dir.join(filename);
        let duration = segment.duration_s.max(0.0);
        let mut item = RenderedSegment {
            mode: plan.mode,
            segment_index: segment.segment_index,
            start_s: segment.start_s,
            end_s: segment.end_s,
            planned_duration_s: duration,
            output_file: output.display().to_string(),
            output_duration_s: None,
            status: "pending".to_string(),
            error: None,
        };

        if duration < 0.001 {
            item.status = "skipped_zero_duration".to_string();
            items.push(item);
            continue;
        }

        let args = vec![
            "-y".to_string(),
            "-hide_banner".to_string(),
            "-loglevel".to_string(), "error".to_string(),
            "-ss".to_string(), format!("{:.6}", segment.start_s),
            "-t".to_string(), format!("{duration:.6}"),
            "-i".to_string(), input.display().to_string(),
            "-map".to_string(), "0:v:0".to_string(),
            "-map".to_string(), "0:a:0?".to_string(),
            "-sn".to_string(),
            "-dn".to_string(),
            "-c:v".to_string(), cfg.video_codec.clone(),
            "-preset".to_string(), cfg.video_preset.clone(),
            "-crf".to_string(), cfg.video_crf.to_string(),
            "-pix_fmt".to_string(), "yuv420p".to_string(),
            "-c:a".to_string(), cfg.audio_codec.clone(),
            "-b:a".to_string(), cfg.audio_bitrate.clone(),
            "-movflags".to_string(), "+faststart".to_string(),
            output.display().to_string(),
        ];

        match run_ffmpeg(ffmpeg, &args, "full timeline segment") {
            Ok(()) => {
                item.status = "rendered".to_string();
                item.output_duration_s = probe_duration(ffprobe, &output);
            }
            Err(err) => {
                item.status = "failed".to_string();
                item.error = Some(err.to_string());
            }
        }
        items.push(item);
    }
    Ok(())
}

pub fn render_structural_segments(
    ffmpeg: &Path,
    ffprobe: &Path,
    input: &Path,
    segmentation: &StructuralSegmentationResult,
    outdir: &Path,
    cfg: &SegmentRenderConfig,
) -> Result<SegmentRenderResult> {
    fs::create_dir_all(outdir)?;
    let has_audio = source_has_audio(ffprobe, input);
    let mut items = Vec::new();

    if matches!(cfg.mode, SegmentRenderMode::Complete | SegmentRenderMode::Both) {
        render_plan(
            ffmpeg,
            ffprobe,
            input,
            &segmentation.complete_segments,
            &outdir.join("complete-segments"),
            cfg,
            &mut items,
        )?;
    }
    if matches!(cfg.mode, SegmentRenderMode::Every | SegmentRenderMode::Both) {
        render_plan(
            ffmpeg,
            ffprobe,
            input,
            &segmentation.every_separator,
            &outdir.join("every-separator"),
            cfg,
            &mut items,
        )?;
    }

    let rendered_ok = items.iter().filter(|i| i.status == "rendered").count() as u64;
    let rendered_failed = items.iter().filter(|i| i.status == "failed").count() as u64;
    let result = SegmentRenderResult {
        schema_version: 1,
        engine: "opencv-full-segmentation-validation".to_string(),
        render_version: "cv6-segment-render-v1".to_string(),
        production_render_performed: false,
        config: cfg.clone(),
        summary: SegmentRenderSummary {
            complete_segments_requested: if matches!(cfg.mode, SegmentRenderMode::Complete | SegmentRenderMode::Both) {
                segmentation.complete_segments.segments.len() as u64
            } else { 0 },
            every_separator_segments_requested: if matches!(cfg.mode, SegmentRenderMode::Every | SegmentRenderMode::Both) {
                segmentation.every_separator.segments.len() as u64
            } else { 0 },
            rendered_ok,
            rendered_failed,
            source_has_audio: has_audio,
        },
        items,
    };

    fs::write(
        outdir.join("full_segmentation_render_manifest.json"),
        serde_json::to_string_pretty(&result)?,
    )?;
    Ok(result)
}
