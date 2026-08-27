use anyhow::{anyhow, Result};
use glob::Pattern;
use once_cell::sync::Lazy;
use serde::Deserialize;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{Emitter, Window};
use walkdir::WalkDir;

use crate::adslicer::cut::{
    build_plan, export_chapters_only, export_chapters_only_cancellable, export_commercials, export_show,
    export_timeline_segments_cancellable, format_ts, write_dataset, write_logs, write_segment_ffmeta,
};
use crate::adslicer::models::EncodeSettings;
use crate::adslicer::detect::{
    avg_scene_change_rate, drop_too_short, merge_close,
    run_blackdetect, run_scenechange, run_silencedetect, run_uniformdetect,
};
use crate::adslicer::models::RunMeta;
use crate::adslicer::cv_production::{build_opencv_production_plan_progress_cancel, CvProductionPolicy};

const APP_VERSION: &str = "0.1.0";

// ─── Cancellation ─────────────────────────────────────────────────────────────

static CANCEL_FLAG: Lazy<AtomicBool> = Lazy::new(|| AtomicBool::new(false));
pub fn cancel_current_job() { CANCEL_FLAG.store(true, Ordering::SeqCst); }
pub fn is_cancel_requested() -> bool { CANCEL_FLAG.load(Ordering::SeqCst) }

fn check_cancelled() -> Result<()> {
    if is_cancel_requested() { Err(anyhow!("Job cancelled by user.")) } else { Ok(()) }
}

// ─── Params ───────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobParams {
    pub input_mode:        String,
    pub input_path:        String,
    pub glob:              String,
    pub outdir:            String,
    #[allow(dead_code)]
    pub media_type:        String,
    /// Detection engine: "legacy" or "opencv_adaptive".
    #[serde(default = "default_detection_engine")]
    pub detection_engine:  String,
    /// OpenCV structural policy: "complete_segments" or "every_separator".
    #[serde(default = "default_segmentation_policy")]
    pub segmentation_policy: String,
    pub black_min_dur:     f64,
    pub pix_th:            f64,
    pub pic_th:            f64,
    pub merge_gap:         f64,
    pub edge_pad_pre:      f64,
    pub edge_pad_post:     f64,
    pub min_commercial:    f64,
    pub max_commercial:    f64,
    pub include_black:     bool,
    pub reencode:          bool,
    pub dry_run:           bool,
    /// Render preview: if > 0, each exported segment is capped at this many seconds.
    /// Detection and planning run in full; only the output clips are truncated.
    /// 0 = disabled (export full segments).  Typical values: 30, 60, 120.
    #[serde(default)]
    pub preview_dur:       f64,
    pub verbosity:         u8,
    #[serde(default = "default_silence_noise_db")]
    pub silence_noise_db:  f64,
    #[serde(default = "default_silence_min_dur")]
    pub silence_min_dur:   f64,
    #[serde(default = "default_min_show_segment")]
    pub min_show_segment:  f64,
    #[serde(default)]
    pub always_keep_first: f64,
    #[serde(default)]
    pub always_keep_last:  f64,
    // ── new Comskip features ──────────────────────────────────────────────────
    /// Comskip: non_uniformity — luma stddev ceiling for uniform detection (0 = off).
    #[serde(default = "default_uniform_max_stddev")]
    pub uniform_max_stddev: f64,
    /// Comskip: schange_threshold — scene change sensitivity 0.0–1.0 (0 = off).
    #[serde(default = "default_scene_threshold")]
    pub scene_threshold:   f64,
    /// Comskip: remove_before — trim seconds from content side of each cut.
    #[serde(default)]
    pub remove_before:     f64,
    /// Comskip: remove_after — trim seconds from ad side of each cut.
    #[serde(default)]
    pub remove_after:      f64,
    /// Comskip: require_div5 — only accept breaks near a multiple of 30s.
    #[serde(default)]
    pub require_div5:      bool,

    // ── Output mode ───────────────────────────────────────────────────────────
    /// "cut" (default) — remove commercials and export show + commercial clips.
    /// "chapters" — embed chapter markers into a copy of the source; no cutting.
    #[serde(default = "default_output_mode")]
    pub output_mode: String,

    // ── Recording trim ────────────────────────────────────────────────────────
    /// Trim seconds from the very start of the assembled show output.
    /// Removes tape leader, noise, or pre-roll artefacts independent of
    /// commercial detection.  Different from always_keep_first: that protects
    /// content from being classified as a commercial; this unconditionally
    /// removes content from the recording edge.  Default 0 (off).
    #[serde(default)]
    pub trim_head: f64,
    /// Trim seconds from the very end of the assembled show output.
    /// Removes deck shutoff noise and post-roll artefacts.  Default 0 (off).
    #[serde(default)]
    pub trim_tail: f64,

    // ── Post-processing hook ──────────────────────────────────────────────────
    /// Shell command to run after a successful export.  The following tokens
    /// are substituted before execution:
    ///   %show%    — absolute path to the exported show file
    ///   %outdir%  — absolute path to the per-file output directory
    ///   %input%   — absolute path to the source input file
    ///   %base%    — filename stem of the source (no path, no extension)
    /// Example: "cp '%show%' /Volumes/NAS/Shows/"
    /// Non-fatal: if the command fails, a warning is logged and the job continues.
    /// Leave empty to disable.
    #[serde(default)]
    pub post_command: String,

    // ── Encode settings ───────────────────────────────────────────────────────
    /// Full transcoder settings.  Replaces the old binary reencode flag.
    /// Backward compat: if encode is absent, falls back to reencode bool.
    #[serde(default)]
    pub encode: EncodeSettings,

}

fn default_detection_engine() -> String { "opencv_adaptive".to_string() }
fn default_segmentation_policy() -> String { "complete_segments".to_string() }
fn default_silence_noise_db()  -> f64 { -40.0 }
fn default_output_mode()       -> String { "cut".to_string() }
fn default_silence_min_dur()   -> f64 {   0.5 }
fn default_min_show_segment()  -> f64 {  30.0 }
fn default_uniform_max_stddev()-> f64 {   8.0 }
fn default_scene_threshold()   -> f64 {   0.4 }

// ─── Binary resolution ────────────────────────────────────────────────────────

fn resolve_binary(name: &str) -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            #[cfg(windows)] let c = dir.join(format!("{}.exe", name));
            #[cfg(not(windows))] let c = dir.join(name);
            if c.exists() { return c; }
        }
    }
    #[cfg(debug_assertions)] {
        let bins = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("binaries");
        if let Some(triple) = option_env!("TAURI_ENV_TARGET_TRIPLE") {
            #[cfg(windows)] let candidate = bins.join(format!("{}-{}.exe", name, triple));
            #[cfg(not(windows))] let candidate = bins.join(format!("{}-{}", name, triple));
            if candidate.exists() { return candidate; }
        }
    }
    PathBuf::from(name)
}

// ─── ffmpeg version probe ─────────────────────────────────────────────────────

fn probe_ffmpeg_version(ffmpeg: &PathBuf) -> String {
    std::process::Command::new(ffmpeg).arg("-version").output().ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).lines().next().unwrap_or("unknown").trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

// ─── ISO-8601 UTC timestamp ───────────────────────────────────────────────────

fn utc_now_iso8601() -> String {
    let s = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let sec = s % 60; let min = (s / 60) % 60; let hour = (s / 3600) % 24;
    let (y, m, d) = days_to_ymd(s / 86400);
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", y, m, d, hour, min, sec)
}

fn days_to_ymd(days: u64) -> (u64, u64, u64) {
    let z = days + 719468; let era = z / 146097; let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400; let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153; let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

// ─── Logging ─────────────────────────────────────────────────────────────────

fn emit_log(window: &Window, msg: &str) { let _ = window.emit("adslicer-log", msg.to_string()); }

fn emit_progress(
    window: &Window,
    stage: &str,
    label: &str,
    current: Option<u64>,
    total: Option<u64>,
    source_position_s: Option<f64>,
    source_duration_s: Option<f64>,
    processing_fps: Option<f64>,
    eta_s: Option<f64>,
    detail: Option<&str>,
) {
    let _ = window.emit("adslicer-progress", serde_json::json!({
        "stage": stage,
        "label": label,
        "current": current,
        "total": total,
        "sourcePositionS": source_position_s,
        "sourceDurationS": source_duration_s,
        "processingFps": processing_fps,
        "etaS": eta_s,
        "detail": detail,
    }));
}

fn emit_batch_progress(window: &Window, current: usize, total: usize, file: &str) {
    let _ = window.emit("adslicer-batch-progress", serde_json::json!({
        "current": current,
        "total": total,
        "file": file,
    }));
}

// ─── Batch input collection ───────────────────────────────────────────────────

fn collect_inputs(folder: &str, glob_pattern: &str) -> Vec<PathBuf> {
    let patterns: Vec<String> = glob_pattern.split(',').map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty()).collect();
    let mut files: Vec<PathBuf> = Vec::new();
    let mut seen: HashSet<PathBuf> = HashSet::new();
    for entry in WalkDir::new(folder).follow_links(true).into_iter().filter_map(|e| e.ok()) {
        if !entry.file_type().is_file() { continue; }
        let path = entry.path();
        let fname = path.file_name().unwrap_or_default().to_string_lossy();
        let fname_lower = fname.to_lowercase();
        for pat_str in &patterns {
            if Pattern::new(pat_str).map(|p| p.matches(&fname) || p.matches(&fname_lower)).unwrap_or(false) {
                let c = dunce::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
                if seen.insert(c.clone()) { files.push(c); }
                break;
            }
        }
    }
    files.sort(); files
}

// ─── Single-file processor ────────────────────────────────────────────────────

fn process_one_legacy(
    window: &Window, ffmpeg: &PathBuf, ffprobe: &PathBuf,
    input: &str, params: &JobParams, ffmpeg_version: &str,
) -> Result<()> {
    let base = std::path::Path::new(input).file_stem().unwrap_or_default().to_string_lossy().to_string();
    let outdir = std::path::Path::new(&params.outdir).join(&base);
    std::fs::create_dir_all(&outdir)?;
    let emit = |msg: &str| emit_log(window, msg);
    let run_id = utc_now_iso8601();

    // ── Black-frame detection ─────────────────────────────────────────────────
    emit(&format!("Analyzing: {}", input));
    let (blacks_raw, duration, raw_ff) = run_blackdetect(ffmpeg, ffprobe, input,
        params.black_min_dur, params.pix_th, params.pic_th, params.verbosity)?;
    emit(&format!("ffprobe duration: {:.3}s", duration));
    emit(&format!("Detected {} raw black segments", blacks_raw.len()));
    if params.verbosity >= 1 {
        let ld = outdir.join("logs"); std::fs::create_dir_all(&ld)?;
        std::fs::write(ld.join("ffmpeg_blackdetect.log"), &raw_ff)?;
    }
    let blacks = drop_too_short(&blacks_raw, params.black_min_dur);
    let blacks = merge_close(&blacks, params.merge_gap);
    emit(&format!("After filtering/merge: {} black segments", blacks.len()));

    // ── Silence detection ─────────────────────────────────────────────────────
    let silences = if params.silence_noise_db < 0.0 {
        emit(&format!("Running silence detection (noise_db={:.1}, min_dur={:.2}s)…",
            params.silence_noise_db, params.silence_min_dur));
        match run_silencedetect(ffmpeg, input, params.silence_noise_db, params.silence_min_dur) {
            Ok((segs, raw)) => {
                emit(&format!("Detected {} silence segments", segs.len()));
                if params.verbosity >= 1 {
                    let ld = outdir.join("logs"); std::fs::create_dir_all(&ld)?;
                    std::fs::write(ld.join("ffmpeg_silencedetect.log"), &raw)?;
                }
                segs
            }
            Err(e) => { emit(&format!("[warn] Silence detection failed (non-fatal): {e}")); Vec::new() }
        }
    } else { emit("Silence detection disabled (noise_db >= 0)."); Vec::new() };

    // ── Uniform frame detection ───────────────────────────────────────────────
    // Comskip: non_uniformity / validate_uniform
    let uniforms = if params.uniform_max_stddev > 0.0 {
        emit(&format!("Running uniform detection (max_stddev={:.1})…", params.uniform_max_stddev));
        match run_uniformdetect(ffmpeg, input, params.uniform_max_stddev, params.black_min_dur) {
            Ok((segs, raw)) => {
                emit(&format!("Detected {} uniform segments", segs.len()));
                if params.verbosity >= 2 {
                    let ld = outdir.join("logs"); std::fs::create_dir_all(&ld)?;
                    std::fs::write(ld.join("ffmpeg_uniformdetect.log"), &raw)?;
                }
                segs
            }
            Err(e) => { emit(&format!("[warn] Uniform detection failed (non-fatal): {e}")); Vec::new() }
        }
    } else { emit("Uniform detection disabled (max_stddev = 0)."); Vec::new() };

    // ── Scene change detection ────────────────────────────────────────────────
    // Comskip: schange_threshold / schange_rate / validate_scenechange
    let scene_changes = if params.scene_threshold > 0.0 {
        emit(&format!("Running scene change detection (threshold={:.2})…", params.scene_threshold));
        match run_scenechange(ffmpeg, input, params.scene_threshold) {
            Ok((changes, raw)) => {
                let rate = avg_scene_change_rate(&changes, duration);
                emit(&format!("Detected {} scene changes ({:.2}/s avg)", changes.len(), rate));
                if params.verbosity >= 2 {
                    let ld = outdir.join("logs"); std::fs::create_dir_all(&ld)?;
                    std::fs::write(ld.join("ffmpeg_scenechange.log"), &raw)?;
                }
                changes
            }
            Err(e) => { emit(&format!("[warn] Scene change detection failed (non-fatal): {e}")); Vec::new() }
        }
    } else { emit("Scene change detection disabled (threshold = 0)."); Vec::new() };

    // ── Plan ──────────────────────────────────────────────────────────────────
    let plan = build_plan(
        &blacks, &silences, &uniforms, &scene_changes, duration,
        params.include_black, params.edge_pad_pre, params.edge_pad_post,
        params.min_commercial, params.max_commercial,
        params.min_show_segment, params.always_keep_first, params.always_keep_last,
        params.silence_noise_db, params.silence_min_dur,
        params.uniform_max_stddev, params.scene_threshold,
        params.remove_before, params.remove_after, params.require_div5,
        params.trim_head, params.trim_tail,
    );

    emit(&format!("Plan: {} commercials, {} keeps", plan.commercials.len(), plan.keeps.len()));
    if plan.commercials.is_empty() {
        emit("No commercials detected.");
    } else {
        emit(&format!("Commercials ({}):", plan.commercials.len()));
        for (i, c) in plan.commercials.iter().enumerate() {
            emit(&format!("  {:>3} | {} -> {} | {} | conf={:.2} | [{}]",
                i + 1, format_ts(c.start), format_ts(c.end), format_ts(c.duration()),
                c.confidence, c.signals.join(", ")));
        }
    }

    // ── RunMeta ───────────────────────────────────────────────────────────────
    let total_commercial_s: f64 = plan.commercials.iter().map(|c| c.duration()).sum();
    let total_keep_s:       f64 = plan.keeps.iter().map(|k| k.duration()).sum();
    let commercial_ratio        = total_commercial_s / duration.max(1e-9);
    let avg_sc_rate             = avg_scene_change_rate(&scene_changes, duration);

    let meta = RunMeta {
        run_id: run_id.clone(), source_file: base.clone(),
        source_duration_s: duration, ffmpeg_version: ffmpeg_version.to_string(),
        app_version: APP_VERSION.to_string(),
        param_black_min_dur: params.black_min_dur, param_pix_th: params.pix_th,
        param_pic_th: params.pic_th, param_merge_gap: params.merge_gap,
        param_edge_pad_pre: params.edge_pad_pre, param_edge_pad_post: params.edge_pad_post,
        param_min_commercial: params.min_commercial, param_max_commercial: params.max_commercial,
        param_include_black: params.include_black,
        param_silence_noise_db: params.silence_noise_db, param_silence_min_dur: params.silence_min_dur,
        param_min_show_segment: params.min_show_segment,
        param_always_keep_first: params.always_keep_first, param_always_keep_last: params.always_keep_last,
        param_uniform_max_stddev: params.uniform_max_stddev,
        param_scene_threshold: params.scene_threshold,
        param_remove_before: params.remove_before, param_remove_after: params.remove_after,
        param_require_div5: params.require_div5,
        param_trim_head: params.trim_head, param_trim_tail: params.trim_tail,
        blacks_raw_count: blacks_raw.len(), blacks_filtered_count: blacks.len(),
        silences_count: silences.len(), uniforms_count: uniforms.len(),
        scene_changes_count: scene_changes.len(), avg_scene_rate: avg_sc_rate,
        total_commercial_s, total_keep_s, commercial_ratio,
        commercial_count: plan.commercials.len(), keep_count: plan.keeps.len(),
    };

    // ── Comskip feature summary ───────────────────────────────────────────────
    {
        let silence_boosted = plan.commercials.iter()
            .filter(|c| c.signals.iter().any(|s| s == "silence_overlap")).count();
        let uniform_boosted = plan.commercials.iter()
            .filter(|c| c.signals.iter().any(|s| s == "uniform_overlap")).count();
        let sc_boosted = plan.commercials.iter()
            .filter(|c| c.signals.iter().any(|s| s == "high_scene_rate")).count();
        let demoted = plan.keeps.iter()
            .filter(|k| k.signals.iter().any(|s| s == "demoted_min_show_segment")).count();
        let snapped = plan.commercials.iter()
            .filter(|c| c.signals.iter().any(|s| s == "div5_snapped")).count();
        let head_guard = if params.always_keep_first > 0.0 { format!("first {:.0}s protected", params.always_keep_first) } else { "off".to_string() };
        let tail_guard = if params.always_keep_last  > 0.0 { format!("last {:.0}s protected",  params.always_keep_last)  } else { "off".to_string() };

        emit("Detection summary:");
        emit(&format!("  Silence scan    : {} segment(s)  →  {} cut(s) boosted", silences.len(), silence_boosted));
        emit(&format!("  Uniform scan    : {} segment(s)  →  {} cut(s) boosted", uniforms.len(), uniform_boosted));
        emit(&format!("  Scene change    : {} change(s)  {:.2}/s avg  →  {} cut(s) boosted", scene_changes.len(), avg_sc_rate, sc_boosted));
        emit(&format!("  Show guard      : {} candidate(s) demoted  (min_show_segment = {:.0}s)", demoted, params.min_show_segment));
        emit(&format!("  Edge protection : head = {}  tail = {}", head_guard, tail_guard));
        if params.require_div5 { emit(&format!("  Div5 snapping   : {} cut(s) snapped to 30s boundary", snapped)); }
        if params.remove_before > 0.0 || params.remove_after > 0.0 {
            emit(&format!("  Asymmetric trim : remove_before={:.2}s  remove_after={:.2}s", params.remove_before, params.remove_after));
        }
        emit(&format!("  Commercial load : {:.1}s of {:.1}s total  ({:.1}%)",
            total_commercial_s, duration, commercial_ratio * 100.0));
        emit(&format!("  Keep content    : {:.1}s across {} segment(s)", total_keep_s, plan.keeps.len()));
        if params.trim_head > 0.0 || params.trim_tail > 0.0 {
            emit(&format!("  Recording trim  : head={:.2}s  tail={:.2}s",
                params.trim_head, params.trim_tail));
        }
    }

    // ── Write logs ────────────────────────────────────────────────────────────
    write_logs(&outdir, &base, &plan)?;
    write_dataset(&outdir, &plan, &meta)?;
    emit(&format!("Dataset written → {}/logs/dataset.jsonl", outdir.display()));

    if params.dry_run {
        emit(&format!("[dry-run] Logs only → {}", outdir.join("logs").display()));
        return Ok(());
    }

    // ── Resolve effective encode settings ─────────────────────────────────────
    // If the legacy reencode flag is set and no explicit encode mode was chosen,
    // promote to h264 for backward compatibility with old presets.
    let mut enc = params.encode.clone();
    if params.reencode && enc.mode == "copy" {
        enc.mode = "h264".to_string();
    }

    // ── Export ────────────────────────────────────────────────────────────────
    let ffmeta_path = outdir.join("logs").join("chapters.ffmeta");

    match params.output_mode.as_str() {
        // ── Chapters-only mode ────────────────────────────────────────────────
        // Embeds commercial/content chapter markers into a copy of the source.
        // No content is removed.  Fast: source is stream-copied.
        "chapters" => {
            if !ffmeta_path.exists() {
                return Err(anyhow!("chapters.ffmeta not found — run detection first"));
            }
            let chaptered = export_chapters_only(
                ffmpeg, input, &outdir, &base, &ffmeta_path,
                &|msg| emit_log(window, msg),
            )?;
            emit(&format!("DONE Done: {}", input));
            emit(&format!("Chaptered file → {}", chaptered.display()));
            emit(&format!("Logs: {}", outdir.join("logs").display()));
        }

        // ── Standard cut mode (default) ───────────────────────────────────────
        _ => {
            // Preview mode: log a clear banner so the user knows output is truncated
            if params.preview_dur > 0.0 {
                emit(&format!(
                    "Preview mode: segments capped at {:.0}s — full export disabled",
                    params.preview_dur
                ));
            }

            let com_paths = export_commercials(
                ffmpeg, input, &outdir, &base, &plan, &enc,
                params.preview_dur,
                &|msg| emit_log(window, msg),
            )?;
            let show_path = export_show(
                ffmpeg, input, &outdir, &base, &plan, &enc,
                params.preview_dur,
                &|msg| emit_log(window, msg),
            )?;
            emit(&format!("DONE Done: {}", input));
            emit(&format!("Commercial files: {} → {}", com_paths.len(), outdir.join("commercials").display()));
            emit(&format!("Show file: {}", show_path.display()));
            emit(&format!("Logs: {}", outdir.join("logs").display()));

            // Surface encode settings in log
            emit(&format!("Encode: mode={}  gpu={}  crf={}  audio={}{}{}",
                enc.mode, enc.gpu_accel, enc.video_crf,
                enc.audio_codec,
                if enc.loudnorm { "  loudnorm=on" } else { "" },
                if enc.deinterlace { "  deinterlace=on" } else { "" },
            ));
        }
    }

    // ── Post-processing hook ─────────────────────────────────────────────────
    if !params.post_command.is_empty() {
        // Determine show path for substitution.  In chapters mode the output is
        // _chaptered.mp4; in cut mode it's _show.mp4.  We use the show dir as a
        // safe fallback if we can't find an exact file.
        let show_glob = if params.output_mode == "chapters" {
            outdir.join("show").join(format!("{}_chaptered.mp4", base))
        } else {
            outdir.join("show").join(format!("{}_show.mp4", base))
        };
        let show_str = show_glob.to_string_lossy();
        let cmd_expanded = params.post_command
            .replace("%show%",   &show_str)
            .replace("%outdir%", &outdir.to_string_lossy().to_string())
            .replace("%input%",  input)
            .replace("%base%",   &base);

        emit(&format!("Post-processing: {}", cmd_expanded));

        #[cfg(windows)]
        let status = std::process::Command::new("cmd")
            .args(["/C", &cmd_expanded])
            .status();
        #[cfg(not(windows))]
        let status = std::process::Command::new("sh")
            .args(["-c", &cmd_expanded])
            .status();

        match status {
            Ok(s) if s.success() =>
                emit("Post-processing: completed successfully."),
            Ok(s) =>
                emit(&format!("[warn] Post-processing exited with status {} — continuing.", s)),
            Err(e) =>
                emit(&format!("[warn] Post-processing failed to launch: {e} — continuing.")),
        }
    }

    Ok(())
}


// ─── OpenCV Adaptive production path (CV-7) ──────────────────────────────────

fn process_one_opencv(
    window: &Window, ffmpeg: &PathBuf, _ffprobe: &PathBuf,
    input: &str, params: &JobParams,
) -> Result<()> {
    let input_path = std::path::Path::new(input);
    let base = input_path.file_stem().unwrap_or_default().to_string_lossy().to_string();
    let outdir = std::path::Path::new(&params.outdir).join(&base);
    std::fs::create_dir_all(&outdir)?;
    let emit = |msg: &str| emit_log(window, msg);

    let policy = CvProductionPolicy::from_param(&params.segmentation_policy)?;
    emit(&format!("Analyzing with OpenCV Adaptive: {}", input));
    emit(&format!("Segmentation policy: {}", policy.display_name()));
    emit("OpenCV Adaptive uses validated CV-6.1a thresholds; Legacy detector tuning fields are ignored.");

    let diagnostics_root = outdir.join("logs").join("opencv");
    let cv_plan = build_opencv_production_plan_progress_cancel(
        input_path,
        &diagnostics_root,
        policy,
        &|stage, frame_progress| {
            match (stage, frame_progress) {
                ("analyze_frames", Some(p)) => {
                    if p.scan_complete {
                        emit_progress(
                            window, "analysis_finalize", "Finalizing Analysis", None, None,
                            None, None, None, None, Some("Frame scan complete; verifying the source and finalizing evidence"),
                        );
                    } else {
                        emit_progress(
                            window,
                            "analysis",
                            "Analyzing Recording",
                            Some(p.frames_decoded),
                            p.total_frames,
                            p.source_position_s,
                            p.source_duration_s,
                            Some(p.processing_fps),
                            p.eta_s,
                            Some("Inspecting video frames for structural evidence"),
                        );
                    }
                },
                ("analyze_frames", None) => emit_progress(
                    window, "analysis", "Analyzing Recording", Some(0), None,
                    None, None, None, None, Some("Opening recording and preparing frame analysis"),
                ),
                ("finalize_analysis", _) => emit_progress(
                    window, "analysis_finalize", "Finalizing Analysis", None, None,
                    None, None, None, None, Some("Finishing frame evidence and source verification"),
                ),
                ("save_evidence", _) => emit_progress(
                    window, "analysis_finalize", "Saving Analysis Data", None, None,
                    None, None, None, None, Some("Writing reproducible analysis diagnostics"),
                ),
                ("temporal", _) => emit_progress(
                    window, "boundaries", "Detecting Boundaries", None, None,
                    None, None, None, None, Some("Evaluating temporal transitions and separators"),
                ),
                ("boundaries", _) => emit_progress(
                    window, "boundaries", "Detecting Boundaries", None, None,
                    None, None, None, None, Some("Scoring candidate boundaries"),
                ),
                ("structural", _) => emit_progress(
                    window, "segments", "Building Segments", None, None,
                    None, None, None, None, Some("Building the full-coverage structural timeline"),
                ),
                ("plan_ready", _) => emit_progress(
                    window, "segments", "Segments Ready", Some(1), Some(1),
                    None, None, None, Some(0.0), Some("Structural timeline passed coverage validation"),
                ),
                _ => {}
            }
        },
        &|| is_cancel_requested(),
    )?;

    check_cancelled()?;
    emit(&format!("OpenCV: {}", cv_plan.opencv_version));
    emit(&format!("Frames decoded/analyzed: {}/{}", cv_plan.frames_decoded, cv_plan.frames_analyzed));
    emit(&format!("Structural events: {}", cv_plan.structural_events));
    emit(&format!("Selected boundaries: {}", cv_plan.structural_boundaries));
    emit(&format!("Plan: {} full-coverage segments", cv_plan.segments.len()));
    emit(&format!(
        "Coverage: {} | source {:.3}s | covered {:.3}s | uncovered {:.6}s | overlap {:.6}s",
        if cv_plan.coverage.coverage_pass { "PASS" } else { "FAIL" },
        cv_plan.coverage.source_duration_s,
        cv_plan.coverage.covered_duration_s,
        cv_plan.coverage.uncovered_duration_s,
        cv_plan.coverage.overlap_duration_s,
    ));

    for (i, segment) in cv_plan.segments.iter().enumerate() {
        emit(&format!(
            "  {:>3} | {} -> {} | {} | [{}]",
            i + 1,
            format_ts(segment.start),
            format_ts(segment.end),
            format_ts(segment.duration()),
            segment.signals.join(", "),
        ));
    }

    check_cancelled()?;
    // Chapter metadata is useful both for chapters-only exports and as a
    // human-readable timeline artifact for dry runs.
    let ffmeta_path = write_segment_ffmeta(&outdir, &cv_plan.segments)?;

    if params.trim_head > 0.0 || params.trim_tail > 0.0 {
        emit("[warn] Trim Head/Tail ignored in OpenCV Adaptive mode to preserve the full-coverage invariant.");
    }

    if params.dry_run {
        check_cancelled()?;
        emit(&format!("[dry-run] OpenCV diagnostics only → {}", diagnostics_root.display()));
        emit_progress(window, "complete", "Analysis Complete", Some(1), Some(1), None, None, None, Some(0.0), Some("Analysis finished; no media was exported"));
        return Ok(());
    }

    let mut enc = params.encode.clone();
    if params.reencode && enc.mode == "copy" {
        enc.mode = "h264".to_string();
    }

    match params.output_mode.as_str() {
        "chapters" => {
            emit_progress(window, "export", "Writing Chaptered Recording", None, None, None, None, None, None, Some("Writing chapter markers and output media"));
            let chaptered = export_chapters_only_cancellable(
                ffmpeg, input, &outdir, &base, &ffmeta_path,
                &|msg| emit_log(window, msg),
                &|| is_cancel_requested(),
            )?;
            check_cancelled()?;
            emit(&format!("DONE Done: {}", input));
            emit(&format!("Chaptered full timeline → {}", chaptered.display()));
            emit_progress(window, "complete", "Complete", Some(1), Some(1), None, None, None, Some(0.0), Some("Chaptered recording finished"));
        }
        _ => {
            if params.preview_dur > 0.0 {
                emit(&format!(
                    "Preview mode: each structural segment capped at {:.0}s — full export disabled",
                    params.preview_dur,
                ));
            }
            let segment_total = cv_plan.segments.len() as u64;
            emit_progress(window, "export", "Exporting Clips", Some(0), Some(segment_total), None, None, None, None, Some("Writing structural clips"));
            let paths = export_timeline_segments_cancellable(
                ffmpeg, input, &outdir, &base, &cv_plan.segments, &enc,
                params.preview_dur,
                &|msg| emit_log(window, msg),
                &|completed, total| emit_progress(
                    window,
                    "export",
                    "Exporting Clips",
                    Some(completed as u64),
                    Some(total as u64),
                    None, None, None, None,
                    Some("Writing structural clips"),
                ),
                &|| is_cancel_requested(),
            )?;
            check_cancelled()?;
            emit(&format!("DONE Done: {}", input));
            emit(&format!("Structural segment files: {} → {}", paths.len(), outdir.join("segments").display()));
            emit(&format!("Diagnostics: {}", diagnostics_root.display()));
            emit(&format!("Encode: mode={}  gpu={}  crf={}  audio={}{}{}",
                enc.mode, enc.gpu_accel, enc.video_crf,
                enc.audio_codec,
                if enc.loudnorm { "  loudnorm=ignored-for-independent-segments" } else { "" },
                if enc.deinterlace { "  deinterlace=on" } else { "" },
            ));
            emit_progress(window, "complete", "Complete", Some(1), Some(1), None, None, None, Some(0.0), Some("All structural clips finished"));
        }
    }

    if !params.post_command.is_empty() {
        emit("[warn] Post-processing command skipped in OpenCV Adaptive mode for CV-7 because there is no semantic assembled show file yet.");
    }

    Ok(())
}

fn process_one(
    window: &Window, ffmpeg: &PathBuf, ffprobe: &PathBuf,
    input: &str, params: &JobParams, ffmpeg_version: &str,
) -> Result<()> {
    match params.detection_engine.as_str() {
        "legacy" => process_one_legacy(window, ffmpeg, ffprobe, input, params, ffmpeg_version),
        "opencv_adaptive" | "opencv" | "" => process_one_opencv(window, ffmpeg, ffprobe, input, params),
        other => Err(anyhow!("Unknown detection engine: {other}")),
    }
}

// ─── Public entry point ───────────────────────────────────────────────────────

pub fn run_job(window: &Window, params: JobParams) -> Result<()> {
    CANCEL_FLAG.store(false, Ordering::SeqCst);
    if params.outdir.is_empty()     { return Err(anyhow!("No output directory specified")); }
    if params.input_path.is_empty() { return Err(anyhow!("No input path specified")); }

    let ffmpeg  = resolve_binary("ffmpeg");
    let ffprobe = resolve_binary("ffprobe");
    emit_log(window, &format!("Using ffmpeg:  {}", ffmpeg.display()));
    emit_log(window, &format!("Using ffprobe: {}", ffprobe.display()));
    let ffmpeg_version = probe_ffmpeg_version(&ffmpeg);
    emit_log(window, &format!("ffmpeg: {}", ffmpeg_version));

    if params.input_mode == "batchDir" {
        let inputs = collect_inputs(&params.input_path, &params.glob);
        if inputs.is_empty() {
            return Err(anyhow!("No matching files found in: {} (glob: {})", params.input_path, params.glob));
        }
        emit_log(window, &format!("Batch mode: {} files found", inputs.len()));
        let mut processed = 0usize; let mut errors = 0usize;
        for (index, path) in inputs.iter().enumerate() {
            if CANCEL_FLAG.load(Ordering::SeqCst) { emit_log(window, "Job cancelled by user."); break; }
            let input_str = path.to_string_lossy().to_string();
            emit_batch_progress(window, index + 1, inputs.len(), &input_str);
            emit_log(window, &format!("- Processing {}", input_str));
            match process_one(window, &ffmpeg, &ffprobe, &input_str, &params, &ffmpeg_version) {
                Ok(()) => processed += 1,
                Err(_) if is_cancel_requested() => {
                    emit_log(window, "Cancellation acknowledged; stopping batch.");
                    break;
                }
                Err(e) => { errors += 1; emit_log(window, &format!("[error] Failed on {}: {}", input_str, e)); }
            }
        }
        if !is_cancel_requested() {
            emit_log(window, &format!("Done. Processed {} files, {} errors.", processed, errors));
        }
    } else {
        process_one(window, &ffmpeg, &ffprobe, &params.input_path, &params, &ffmpeg_version)?;
    }
    Ok(())
}
