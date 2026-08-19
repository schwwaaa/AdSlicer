//! CV-5 watchable edit validation for AdSlicer.
//!
//! This stage does NOT alter the production AdSlicer plan. It renders short,
//! disposable validation media from CV-4 shadow removals so a human can judge
//! the proposed edit directly:
//!
//! 1. `remove-NNN_content.mp4` is the material the shadow plan proposes removing.
//! 2. `remove-NNN_result-preview.mp4` is a short splice from just before the
//!    removal to just after it, approximating what the resulting edit feels like.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::cv_shadow_plan::{ShadowIntervalTier, ShadowPlanResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditValidationConfig {
    /// Seconds retained before a proposed removal in the splice preview.
    pub context_before_s: f64,
    /// Seconds retained after a proposed removal in the splice preview.
    pub context_after_s: f64,
    /// Safety cap for long recordings. Zero means render every proposed removal.
    pub max_previews: usize,
    pub video_codec: String,
    pub video_crf: u8,
    pub video_preset: String,
    pub audio_codec: String,
    pub audio_bitrate: String,
}

impl Default for EditValidationConfig {
    fn default() -> Self {
        Self {
            context_before_s: 2.0,
            context_after_s: 2.0,
            max_previews: 20,
            video_codec: "libx264".to_string(),
            video_crf: 18,
            video_preset: "veryfast".to_string(),
            audio_codec: "aac".to_string(),
            audio_bitrate: "160k".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditValidationItem {
    pub interval_index: u64,
    pub start_s: f64,
    pub end_s: f64,
    pub duration_s: f64,
    pub interval_score: f64,
    pub tier: ShadowIntervalTier,
    pub before_context_start_s: f64,
    pub before_context_end_s: f64,
    pub after_context_start_s: f64,
    pub after_context_end_s: f64,
    pub source_has_audio: bool,
    pub status: String,
    pub removed_content_file: Option<String>,
    pub removed_content_duration_s: Option<f64>,
    pub result_preview_file: Option<String>,
    pub result_preview_duration_s: Option<f64>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditValidationSummary {
    pub shadow_removals_available: u64,
    pub previews_requested: u64,
    pub previews_rendered: u64,
    pub previews_failed: u64,
    pub previews_skipped_by_limit: u64,
    pub source_has_audio: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditValidationResult {
    pub schema_version: u32,
    pub engine: String,
    pub validation_version: String,
    pub production_cut_plan_modified: bool,
    pub production_render_performed: bool,
    pub config: EditValidationConfig,
    pub summary: EditValidationSummary,
    pub items: Vec<EditValidationItem>,
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
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout).trim().parse::<f64>().ok()
}

fn base_encode_args(cfg: &EditValidationConfig) -> Vec<String> {
    vec![
        "-c:v".into(), cfg.video_codec.clone(),
        "-preset".into(), cfg.video_preset.clone(),
        "-crf".into(), cfg.video_crf.to_string(),
        "-pix_fmt".into(), "yuv420p".into(),
        "-c:a".into(), cfg.audio_codec.clone(),
        "-b:a".into(), cfg.audio_bitrate.clone(),
        "-movflags".into(), "+faststart".into(),
    ]
}

fn render_removed_content(
    ffmpeg: &Path,
    input: &Path,
    start_s: f64,
    duration_s: f64,
    output: &Path,
    cfg: &EditValidationConfig,
) -> Result<()> {
    let mut args = vec![
        "-y".into(),
        "-hide_banner".into(),
        "-loglevel".into(), "error".into(),
        "-ss".into(), format!("{start_s:.6}"),
        "-t".into(), format!("{duration_s:.6}"),
        "-i".into(), input.display().to_string(),
        "-map".into(), "0:v:0".into(),
        "-map".into(), "0:a:0?".into(),
        "-sn".into(),
        "-dn".into(),
    ];
    args.extend(base_encode_args(cfg));
    args.push(output.display().to_string());
    run_ffmpeg(ffmpeg, &args, "removed-content preview")
}

fn render_result_preview(
    ffmpeg: &Path,
    input: &Path,
    source_duration_s: f64,
    remove_start_s: f64,
    remove_end_s: f64,
    has_audio: bool,
    output: &Path,
    cfg: &EditValidationConfig,
) -> Result<(f64, f64, f64, f64)> {
    let before_start = (remove_start_s - cfg.context_before_s).max(0.0);
    let before_end = remove_start_s.max(before_start);
    let after_start = remove_end_s.min(source_duration_s);
    let after_end = (remove_end_s + cfg.context_after_s).min(source_duration_s);
    let before_dur = (before_end - before_start).max(0.0);
    let after_dur = (after_end - after_start).max(0.0);

    if before_dur < 0.02 || after_dur < 0.02 {
        return Err(anyhow!(
            "not enough source context to render splice preview (before {:.3}s, after {:.3}s)",
            before_dur,
            after_dur
        ));
    }

    // Seek to the beginning of the small validation window, then trim two
    // pieces from that window.  This keeps long-source validation responsive
    // while still re-encoding a deterministic, frame-accurate preview.
    let window_start = before_start;
    let window_end = after_end;
    let window_duration = (window_end - window_start).max(0.02);
    let before_rel_end = before_end - window_start;
    let after_rel_start = after_start - window_start;
    let after_rel_end = after_end - window_start;

    let filter = if has_audio {
        format!(
            "[0:v:0]trim=start=0:end={before_rel_end:.6},setpts=PTS-STARTPTS[v0];\
             [0:v:0]trim=start={after_rel_start:.6}:end={after_rel_end:.6},setpts=PTS-STARTPTS[v1];\
             [v0][v1]concat=n=2:v=1:a=0[v];\
             [0:a:0]atrim=start=0:end={before_rel_end:.6},asetpts=PTS-STARTPTS[a0];\
             [0:a:0]atrim=start={after_rel_start:.6}:end={after_rel_end:.6},asetpts=PTS-STARTPTS[a1];\
             [a0][a1]concat=n=2:v=0:a=1[a]"
        )
    } else {
        format!(
            "[0:v:0]trim=start=0:end={before_rel_end:.6},setpts=PTS-STARTPTS[v0];\
             [0:v:0]trim=start={after_rel_start:.6}:end={after_rel_end:.6},setpts=PTS-STARTPTS[v1];\
             [v0][v1]concat=n=2:v=1:a=0[v]"
        )
    };

    let mut args = vec![
        "-y".into(),
        "-hide_banner".into(),
        "-loglevel".into(), "error".into(),
        "-ss".into(), format!("{window_start:.6}"),
        "-t".into(), format!("{window_duration:.6}"),
        "-i".into(), input.display().to_string(),
        "-filter_complex".into(), filter,
        "-map".into(), "[v]".into(),
    ];
    if has_audio {
        args.push("-map".into());
        args.push("[a]".into());
    }
    args.extend(base_encode_args(cfg));
    if !has_audio {
        args.push("-an".into());
    }
    args.push(output.display().to_string());
    run_ffmpeg(ffmpeg, &args, "result-splice preview")?;

    Ok((before_start, before_end, after_start, after_end))
}

pub fn render_edit_validation(
    ffmpeg: &Path,
    ffprobe: &Path,
    input: &Path,
    source_duration_s: f64,
    shadow: &ShadowPlanResult,
    outdir: &Path,
    cfg: &EditValidationConfig,
) -> Result<EditValidationResult> {
    fs::create_dir_all(outdir)?;
    let has_audio = source_has_audio(ffprobe, input);
    let total = shadow.removals.len();
    let limit = if cfg.max_previews == 0 { total } else { cfg.max_previews.min(total) };
    let mut items = Vec::new();
    let mut rendered = 0_u64;
    let mut failed = 0_u64;

    for interval in shadow.removals.iter().take(limit) {
        let n = interval.interval_index + 1;
        let content_path = outdir.join(format!("remove-{n:03}_content.mp4"));
        let result_path = outdir.join(format!("remove-{n:03}_result-preview.mp4"));
        let before_start = (interval.start_s - cfg.context_before_s).max(0.0);
        let before_end = interval.start_s;
        let after_start = interval.end_s;
        let after_end = (interval.end_s + cfg.context_after_s).min(source_duration_s);

        let content_render = render_removed_content(
            ffmpeg,
            input,
            interval.start_s,
            interval.duration_s,
            &content_path,
            cfg,
        );

        let result_render = if content_render.is_ok() {
            render_result_preview(
                ffmpeg,
                input,
                source_duration_s,
                interval.start_s,
                interval.end_s,
                has_audio,
                &result_path,
                cfg,
            )
        } else {
            Err(anyhow!("result preview skipped because removed-content render failed"))
        };

        match (content_render, result_render) {
            (Ok(()), Ok((bs, be, as_, ae))) => {
                rendered += 1;
                items.push(EditValidationItem {
                    interval_index: interval.interval_index,
                    start_s: interval.start_s,
                    end_s: interval.end_s,
                    duration_s: interval.duration_s,
                    interval_score: interval.interval_score,
                    tier: interval.tier,
                    before_context_start_s: bs,
                    before_context_end_s: be,
                    after_context_start_s: as_,
                    after_context_end_s: ae,
                    source_has_audio: has_audio,
                    status: "rendered".to_string(),
                    removed_content_file: Some(content_path.display().to_string()),
                    removed_content_duration_s: probe_duration(ffprobe, &content_path),
                    result_preview_file: Some(result_path.display().to_string()),
                    result_preview_duration_s: probe_duration(ffprobe, &result_path),
                    error: None,
                });
            }
            (content_result, preview_result) => {
                failed += 1;
                let mut errors = Vec::new();
                if let Err(e) = content_result { errors.push(format!("content: {e}")); }
                if let Err(e) = preview_result { errors.push(format!("splice: {e}")); }
                items.push(EditValidationItem {
                    interval_index: interval.interval_index,
                    start_s: interval.start_s,
                    end_s: interval.end_s,
                    duration_s: interval.duration_s,
                    interval_score: interval.interval_score,
                    tier: interval.tier,
                    before_context_start_s: before_start,
                    before_context_end_s: before_end,
                    after_context_start_s: after_start,
                    after_context_end_s: after_end,
                    source_has_audio: has_audio,
                    status: "failed".to_string(),
                    removed_content_file: if content_path.exists() { Some(content_path.display().to_string()) } else { None },
                    removed_content_duration_s: if content_path.exists() { probe_duration(ffprobe, &content_path) } else { None },
                    result_preview_file: if result_path.exists() { Some(result_path.display().to_string()) } else { None },
                    result_preview_duration_s: if result_path.exists() { probe_duration(ffprobe, &result_path) } else { None },
                    error: Some(errors.join("; ")),
                });
            }
        }
    }

    let result = EditValidationResult {
        schema_version: 1,
        engine: "opencv-edit-validation".to_string(),
        validation_version: "cv5-watchable-v1".to_string(),
        production_cut_plan_modified: false,
        production_render_performed: false,
        config: cfg.clone(),
        summary: EditValidationSummary {
            shadow_removals_available: total as u64,
            previews_requested: limit as u64,
            previews_rendered: rendered,
            previews_failed: failed,
            previews_skipped_by_limit: total.saturating_sub(limit) as u64,
            source_has_audio: has_audio,
        },
        items,
    };

    let manifest_path = outdir.join("edit_validation_manifest.json");
    fs::write(&manifest_path, serde_json::to_string_pretty(&result)?)?;

    let mut text = String::new();
    text.push_str("AdSlicer CV-5 Edit Validation\n");
    text.push_str("==============================\n");
    text.push_str("These are disposable validation renders from the OpenCV SHADOW plan.\n");
    text.push_str("Production AdSlicer cuts were NOT changed.\n\n");
    text.push_str(&format!("Shadow removals available: {}\n", result.summary.shadow_removals_available));
    text.push_str(&format!("Validation previews rendered: {}\n", result.summary.previews_rendered));
    text.push_str(&format!("Validation previews failed: {}\n", result.summary.previews_failed));
    text.push_str(&format!("Skipped by preview limit: {}\n", result.summary.previews_skipped_by_limit));
    text.push_str(&format!("Source audio detected: {}\n\n", if has_audio { "yes" } else { "no" }));
    for item in &result.items {
        text.push_str(&format!(
            "Edit {:03}: {:.3}s -> {:.3}s | {:.3}s | score {:.2} | {} | {}\n",
            item.interval_index + 1,
            item.start_s,
            item.end_s,
            item.duration_s,
            item.interval_score,
            item.tier.as_str(),
            item.status,
        ));
        if let Some(ref p) = item.removed_content_file {
            text.push_str(&format!("  removed content: {p}\n"));
        }
        if let Some(ref p) = item.result_preview_file {
            text.push_str(&format!("  resulting edit:  {p}\n"));
        }
        if let Some(ref e) = item.error {
            text.push_str(&format!("  ERROR: {e}\n"));
        }
    }
    fs::write(outdir.join("EDIT_VALIDATION_REPORT.txt"), text)?;

    Ok(result)
}
