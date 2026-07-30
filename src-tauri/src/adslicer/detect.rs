use crate::adslicer::models::{BlackSegment, SceneChange, SilenceSegment, UniformSegment};
use anyhow::{anyhow, Result};
use regex::Regex;
use std::path::Path;
use std::process::Command;

// ─── Black-frame detection ────────────────────────────────────────────────────

pub fn run_blackdetect(
    ffmpeg: &Path,
    ffprobe: &Path,
    input: &str,
    min_dur: f64,
    pix_th: f64,
    pic_th: f64,
    _verbosity: u8,
) -> Result<(Vec<BlackSegment>, f64, String)> {
    let vf = format!("blackdetect=d={}:pix_th={}:pic_th={}", min_dur, pix_th, pic_th);
    let output = Command::new(ffmpeg)
        .args(["-hide_banner", "-nostats", "-nostdin", "-i", input, "-vf", &vf, "-f", "null", "-"])
        .output()
        .map_err(|e| anyhow!("Failed to launch ffmpeg: {e}\n  (looked for: {})", ffmpeg.display()))?;

    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let re_start = Regex::new(r"black_start:(?P<s>[0-9.]+)")?;
    let re_end   = Regex::new(r"black_end:(?P<e>[0-9.]+)")?;
    let mut segments: Vec<BlackSegment> = Vec::new();
    let mut cur_start: Option<f64> = None;

    for line in stderr.lines() {
        if let Some(cap) = re_start.captures(line) {
            cur_start = Some(cap["s"].parse::<f64>().unwrap_or(0.0));
        }
        if let Some(cap) = re_end.captures(line) {
            let end = cap["e"].parse::<f64>().unwrap_or(0.0);
            if let Some(start) = cur_start.take() {
                segments.push(BlackSegment::new(start, end));
            }
        }
    }
    let duration = probe_duration(ffprobe, input)?;
    Ok((segments, duration, stderr))
}

// ─── Silence detection ────────────────────────────────────────────────────────
// Comskip: max_silence / min_silence / validate_silence

pub fn run_silencedetect(
    ffmpeg:   &Path,
    input:    &str,
    noise_db: f64,
    min_dur:  f64,
) -> Result<(Vec<SilenceSegment>, String)> {
    let af = format!("silencedetect=n={}dB:d={}", noise_db, min_dur);
    let output = Command::new(ffmpeg)
        .args(["-hide_banner", "-nostats", "-nostdin", "-i", input, "-af", &af, "-f", "null", "-"])
        .output()
        .map_err(|e| anyhow!("Failed to launch ffmpeg for silencedetect: {e}"))?;

    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let re_start = Regex::new(r"silence_start:\s*(?P<s>[0-9.]+)")?;
    let re_end   = Regex::new(r"silence_end:\s*(?P<e>[0-9.]+)")?;

    let mut segments: Vec<SilenceSegment> = Vec::new();
    let mut cur_start: Option<f64> = None;
    for line in stderr.lines() {
        if let Some(cap) = re_start.captures(line) {
            cur_start = Some(cap["s"].parse::<f64>().unwrap_or(0.0));
        }
        if let Some(cap) = re_end.captures(line) {
            let end = cap["e"].parse::<f64>().unwrap_or(0.0);
            if let Some(start) = cur_start.take() {
                segments.push(SilenceSegment::new(start, end, noise_db));
            }
        }
    }
    Ok((segments, stderr))
}

// ─── Uniform frame detection ──────────────────────────────────────────────────
//
// Comskip: non_uniformity / validate_uniform
//
// A "uniform frame" is one where the entire image has very low variance — a
// solid-colour slate (black, white, or any colour card).  Comskip uses this as
// an additional cut-point signal alongside black frames.
//
// We use ffmpeg's `showinfo` filter which reports per-frame statistics including
// the mean and standard deviation of luma (Y).  Frames where stddev_Y ≤
// `max_stddev` are classified as uniform.
//
// `max_stddev`   — luma standard deviation ceiling (0–255 scale).
//                  Typical solid-black VHS slug: stddev 1–4.
//                  Typical noisy black: 5–15.  Color card: 0–8.
//                  Comskip `non_uniformity` maps roughly to stddev * 30.
//                  Recommended default: 8.0.
// `min_dur`      — minimum run of consecutive uniform frames to register (s).

pub fn run_uniformdetect(
    ffmpeg:    &Path,
    input:     &str,
    max_stddev: f64,
    min_dur:   f64,
) -> Result<(Vec<UniformSegment>, String)> {
    // showinfo logs one line per frame containing:
    //   n:NNN pts:NNN pts_time:T.TTT ... mean:[Y U V A] stdev:[Y U V A]
    let output = Command::new(ffmpeg)
        .args([
            "-hide_banner", "-nostats", "-nostdin",
            "-i", input,
            "-vf", "showinfo",
            "-f", "null", "-",
        ])
        .output()
        .map_err(|e| anyhow!("Failed to launch ffmpeg for uniformdetect: {e}"))?;

    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    // Match: pts_time:T.TTT ... stdev:[Y ...]
    // showinfo line format (recent ffmpeg):
    //   [Parsed_showinfo_0 @ 0x…] n:   0 pts:      0 pts_time:0 ... mean:[  16  128  128  0] stdev:[ 0.0  0.0  0.0  0.0]
    let re_frame = Regex::new(
        r"pts_time:(?P<t>[0-9.]+).*?stdev:\[\s*(?P<sy>[0-9.]+)"
    )?;

    // Collect (pts, luma_stddev) pairs
    let mut frame_data: Vec<(f64, f64)> = Vec::new();
    for line in stderr.lines() {
        if let Some(cap) = re_frame.captures(line) {
            let pts  = cap["t"].parse::<f64>().unwrap_or(0.0);
            let stdev = cap["sy"].parse::<f64>().unwrap_or(999.0);
            frame_data.push((pts, stdev));
        }
    }

    // Group consecutive uniform frames into segments, require min_dur
    let mut segments: Vec<UniformSegment> = Vec::new();
    let mut seg_start: Option<f64> = None;
    let mut seg_end = 0.0_f64;
    let mut seg_stdev_sum = 0.0_f64;
    let mut seg_count = 0usize;

    // approximate frame duration from first two frames
    let frame_dur = if frame_data.len() >= 2 {
        (frame_data[1].0 - frame_data[0].0).max(0.033)
    } else {
        0.033
    };

    for (pts, stdev) in &frame_data {
        if *stdev <= max_stddev {
            if seg_start.is_none() {
                seg_start = Some(*pts);
                seg_stdev_sum = 0.0;
                seg_count = 0;
            }
            seg_end = pts + frame_dur;
            seg_stdev_sum += stdev;
            seg_count += 1;
        } else {
            if let Some(start) = seg_start.take() {
                let dur = seg_end - start;
                if dur >= min_dur {
                    let avg_stdev = if seg_count > 0 { seg_stdev_sum / seg_count as f64 } else { 0.0 };
                    segments.push(UniformSegment::new(start, seg_end, avg_stdev));
                }
            }
        }
    }
    // flush final segment
    if let Some(start) = seg_start {
        let dur = seg_end - start;
        if dur >= min_dur {
            let avg_stdev = if seg_count > 0 { seg_stdev_sum / seg_count as f64 } else { 0.0 };
            segments.push(UniformSegment::new(start, seg_end, avg_stdev));
        }
    }

    Ok((segments, stderr))
}

// ─── Scene change detection ───────────────────────────────────────────────────
//
// Comskip: schange_percent / schange_rate / validate_scenechange
//
// Comskip computes per-frame pixel diff and tracks scene change rate per block.
// Commercial blocks tend to have high scene change rates (rapid cuts); show
// content has lower rates.
//
// We use ffmpeg's `select=scene` filter which reports frames where the
// inter-frame difference exceeds a threshold.  We then aggregate into
// per-second rates over a sliding window.
//
// `threshold`     — frame difference threshold (0.0–1.0).  0.4 is a typical
//                   value for hard cuts; lower catches dissolves too.
//                   Comskip `schange_threshold` (0–100) maps to ~threshold*100.

pub fn run_scenechange(
    ffmpeg:    &Path,
    input:     &str,
    threshold: f64,
) -> Result<(Vec<SceneChange>, String)> {
    // select=scene reports matching frames to stderr via showinfo
    let vf = format!(
        "select='gt(scene,{})',showinfo",
        threshold
    );
    let output = Command::new(ffmpeg)
        .args([
            "-hide_banner", "-nostats", "-nostdin",
            "-i", input,
            "-vf", &vf,
            "-vsync", "vfr",
            "-f", "null", "-",
        ])
        .output()
        .map_err(|e| anyhow!("Failed to launch ffmpeg for scenechange: {e}"))?;

    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    // Extract pts_time for each detected scene change frame
    let re_pts = Regex::new(r"pts_time:(?P<t>[0-9.]+)")?;

    let mut changes: Vec<SceneChange> = Vec::new();
    for line in stderr.lines() {
        if line.contains("Parsed_showinfo") || line.contains("showinfo") {
            if let Some(cap) = re_pts.captures(line) {
                let pts = cap["t"].parse::<f64>().unwrap_or(0.0);
                changes.push(SceneChange::new(pts));
            }
        }
    }

    Ok((changes, stderr))
}

// ─── Duration probe ───────────────────────────────────────────────────────────

pub fn probe_duration(ffprobe: &Path, input: &str) -> Result<f64> {
    let output = Command::new(ffprobe)
        .args(["-v", "error", "-show_entries", "format=duration",
               "-of", "default=noprint_wrappers=1:nokey=1", input])
        .output()
        .map_err(|e| anyhow!("Failed to launch ffprobe: {e}\n  (looked for: {})", ffprobe.display()))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("ffprobe failed (status {}): {}", output.status, err.trim()));
    }
    let s = String::from_utf8_lossy(&output.stdout);
    s.trim().parse::<f64>()
        .map_err(|_| anyhow!("ffprobe returned non-numeric duration: '{}'", s.trim()))
}

// ─── Post-processing helpers ──────────────────────────────────────────────────

pub fn drop_too_short(blacks: &[BlackSegment], min_dur: f64) -> Vec<BlackSegment> {
    blacks.iter().cloned().filter(|b| b.duration() >= min_dur).collect()
}

pub fn merge_close(blacks: &[BlackSegment], gap: f64) -> Vec<BlackSegment> {
    if blacks.is_empty() { return Vec::new(); }
    let mut sorted = blacks.to_vec();
    sorted.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap_or(std::cmp::Ordering::Equal));
    let mut merged: Vec<BlackSegment> = Vec::new();
    let mut cur = sorted[0].clone();
    for seg in &sorted[1..] {
        if seg.start - cur.end <= gap {
            cur = BlackSegment::new(cur.start, cur.end.max(seg.end));
        } else {
            merged.push(cur);
            cur = seg.clone();
        }
    }
    merged.push(cur);
    merged
}

// ─── Overlap helpers ──────────────────────────────────────────────────────────

pub fn has_silence_overlap(
    silences: &[SilenceSegment],
    com_start: f64, com_end: f64, min_overlap_s: f64,
) -> bool {
    silences.iter().any(|s| {
        let os = s.start.max(com_start);
        let oe = s.end.min(com_end);
        oe - os >= min_overlap_s
    })
}

pub fn has_uniform_overlap(
    uniforms: &[UniformSegment],
    com_start: f64, com_end: f64, min_overlap_s: f64,
) -> bool {
    uniforms.iter().any(|u| {
        let os = u.start.max(com_start);
        let oe = u.end.min(com_end);
        oe - os >= min_overlap_s
    })
}

/// Count scene changes within [start, end] and return changes per second.
pub fn scene_change_rate(
    changes: &[SceneChange],
    start: f64, end: f64,
) -> f64 {
    let count = changes.iter().filter(|c| c.pts >= start && c.pts < end).count();
    let dur = (end - start).max(1e-9);
    count as f64 / dur
}

/// Compute the file-average scene change rate in changes/second.
pub fn avg_scene_change_rate(changes: &[SceneChange], duration: f64) -> f64 {
    if duration <= 0.0 { return 0.0; }
    changes.len() as f64 / duration
}
