use crate::adslicer::detect::{
    avg_scene_change_rate, has_silence_overlap, has_uniform_overlap, scene_change_rate,
};
use crate::adslicer::models::{
    BlackSegment, CutInterval, DatasetRecord, EncodeSettings, Plan, RunMeta,
    SceneChange, SilenceSegment, UniformSegment,
};
use anyhow::{anyhow, Result};
use dunce::canonicalize;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

// ─── Timestamp formatting ─────────────────────────────────────────────────────

pub fn format_ts(t: f64) -> String {
    let t = t.max(0.0);
    let total_ms = (t * 1000.0).round() as i64;
    let ms = total_ms % 1000;
    let ts = total_ms / 1000;
    let sec = ts % 60;
    let min = (ts / 60) % 60;
    let hour = ts / 3600;
    format!("{:02}:{:02}:{:02}.{:03}", hour, min, sec, ms)
}

// ─── Collision-safe output path ───────────────────────────────────────────────

fn safe_out(path: &Path) -> PathBuf {
    if !path.exists() { return path.to_path_buf(); }
    let dir  = path.parent().unwrap_or(Path::new("."));
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    let ext  = path.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    let mut i = 1u32;
    loop {
        let c = dir.join(format!("{}_{}{}", stem, i, ext));
        if !c.exists() { return c; }
        i += 1;
    }
}

// ─── Signal indicator helper ──────────────────────────────────────────────────

fn has_signal(signals: &[String], name: &str) -> f64 {
    if signals.iter().any(|s| s == name) { 1.0 } else { 0.0 }
}

// ─── Silence overlap metrics ──────────────────────────────────────────────────

fn silence_metrics(silences: &[SilenceSegment], start: f64, end: f64) -> (usize, f64, f64) {
    let mut count = 0usize;
    let mut total = 0.0_f64;
    for s in silences {
        let os = s.start.max(start);
        let oe = s.end.min(end);
        if oe > os { count += 1; total += oe - os; }
    }
    let coverage = (total / (end - start).max(1e-9)).min(1.0);
    (count, total, coverage)
}

// ─── Uniform overlap metrics ──────────────────────────────────────────────────

fn uniform_metrics(uniforms: &[UniformSegment], start: f64, end: f64) -> (usize, f64, f64) {
    let mut count = 0usize;
    let mut total = 0.0_f64;
    for u in uniforms {
        let os = u.start.max(start);
        let oe = u.end.min(end);
        if oe > os { count += 1; total += oe - os; }
    }
    let coverage = (total / (end - start).max(1e-9)).min(1.0);
    (count, total, coverage)
}

// ─── Black boundary durations ─────────────────────────────────────────────────

fn black_boundary_durs(blacks: &[BlackSegment], start: f64, end: f64) -> (f64, f64) {
    let left = blacks.iter()
        .filter(|b| b.end <= start + 0.1)
        .max_by(|a, b| a.end.partial_cmp(&b.end).unwrap())
        .map(|b| b.duration()).unwrap_or(0.0);
    let right = blacks.iter()
        .filter(|b| b.start >= end - 0.1)
        .min_by(|a, b| a.start.partial_cmp(&b.start).unwrap())
        .map(|b| b.duration()).unwrap_or(0.0);
    (left, right)
}

fn black_overlap_metrics(blacks: &[BlackSegment], start: f64, end: f64) -> (usize, f64) {
    let mut count = 0usize;
    let mut total = 0.0_f64;
    for b in blacks {
        let os = b.start.max(start);
        let oe = b.end.min(end);
        if oe > os { count += 1; total += oe - os; }
    }
    (count, total)
}

// ─── require_div5: snap to nearest 30s boundary ───────────────────────────────
//
// Comskip: require_div5 — only accept commercial breaks whose length is
// within `tolerance` of a multiple of 30 seconds.
//
// Real US TV commercials break in exact multiples of 15s (15, 30, 60, 90…).
// Comskip uses 5×fps ≈ 5 frames tolerance per boundary; we use 3.0s.

const DIV5_UNIT: f64 = 30.0;
const DIV5_TOLERANCE: f64 = 3.0;

fn is_near_div5(dur: f64) -> bool {
    if dur <= 0.0 { return false; }
    let remainder = dur % DIV5_UNIT;
    remainder <= DIV5_TOLERANCE || (DIV5_UNIT - remainder) <= DIV5_TOLERANCE
}

fn snap_to_div5(t: f64) -> f64 {
    (t / DIV5_UNIT).round() * DIV5_UNIT
}

// ─── Plan builder ─────────────────────────────────────────────────────────────
//
// Detection passes:
//   1. Black-frame candidate generation
//   2. Silence corroboration        (+confidence, +signal)
//   3. Uniform frame corroboration  (+confidence, +signal)  — NEW
//   4. Scene change rate scoring    (+confidence if high)   — NEW
//   5. Punish/reward vs file avg    (±confidence)           — NEW
//   6. require_div5 snapping        (drop or snap)          — NEW
//   7. remove_before / remove_after trim                    — NEW
//   8. min_show_segment guard       (demote if keep too short)
//   9. always_keep_first/last       (hard edge protection)
//  10. Keep assembly

#[allow(clippy::too_many_arguments)]
pub fn build_plan(
    blacks:            &[BlackSegment],
    silences:          &[SilenceSegment],
    uniforms:          &[UniformSegment],
    scene_changes:     &[SceneChange],
    duration:          f64,
    include_black:     bool,
    edge_pad_pre:      f64,
    edge_pad_post:     f64,
    min_commercial:    f64,
    max_commercial:    f64,
    min_show_segment:  f64,
    always_keep_first: f64,
    always_keep_last:  f64,
    silence_noise_db:  f64,
    silence_min_dur:   f64,
    uniform_max_stddev: f64,
    scene_threshold:   f64,
    remove_before:     f64,
    remove_after:      f64,
    require_div5:      bool,
    trim_head:         f64,
    trim_tail:         f64,
) -> Plan {
    let protect_head = always_keep_first.max(0.0);
    let protect_tail = (duration - always_keep_last.max(0.0)).max(protect_head);
    let file_avg_scene_rate = avg_scene_change_rate(scene_changes, duration);

    // ── Pass 1: candidate commercials from black-frame gaps ───────────────────
    let mut candidates: Vec<CutInterval> = Vec::new();
    for i in 0..blacks.len().saturating_sub(1) {
        let left  = &blacks[i];
        let right = &blacks[i + 1];
        let raw_start = if include_black { left.start  } else { left.end   };
        let raw_end   = if include_black { right.end   } else { right.start };
        let start = (raw_start - edge_pad_pre ).max(0.0);
        let end   = (raw_end   + edge_pad_post).min(duration);
        if end <= start { continue; }
        let dur = end - start;
        if dur < min_commercial || dur > max_commercial { continue; }
        if start < protect_head || end > protect_tail { continue; }

        let mut ci = CutInterval::new(start, end, "commercial");
        ci.add_signal("black_boundary");
        ci.add_signal("within_commercial_range");

        // ── Pass 2: silence corroboration ─────────────────────────────────────
        if has_silence_overlap(silences, start, end, 0.5) {
            ci.add_signal("silence_overlap");
            let b = (ci.confidence + 0.05).min(1.0);
            ci.set_confidence(b);
        }

        // ── Pass 3: uniform frame corroboration ───────────────────────────────
        // Comskip: validate_uniform — a commercial boundary backed by a uniform
        // frame (solid colour slate, white card) is more reliable.
        if uniform_max_stddev > 0.0 && has_uniform_overlap(uniforms, start, end, 0.3) {
            ci.add_signal("uniform_overlap");
            let b = (ci.confidence + 0.04).min(1.0);
            ci.set_confidence(b);
        }

        // ── Pass 4: scene change rate scoring ─────────────────────────────────
        // Comskip: validate_scenechange / schange_rate
        // Commercials have more scene changes per second than content.
        // If this block's rate exceeds the file average, boost confidence.
        if scene_threshold > 0.0 {
            let block_rate = scene_change_rate(scene_changes, start, end);
            if file_avg_scene_rate > 0.0 && block_rate > file_avg_scene_rate * 1.3 {
                ci.add_signal("high_scene_rate");
                let b = (ci.confidence + 0.04).min(1.0);
                ci.set_confidence(b);
            }
        }

        // ── Pass 5: punish — penalise if no corroborating signals ─────────────
        // Comskip: punish=0/threshold=1.3/modifier=2.0
        // If only the basic black_boundary + within_commercial_range signals
        // fired and nothing corroborated, nudge confidence down slightly.
        let corroborated = ci.signals.iter().any(|s| {
            matches!(s.as_str(), "silence_overlap" | "uniform_overlap" | "high_scene_rate")
        });
        if !corroborated {
            let p = (ci.confidence - 0.03).max(0.0);
            ci.set_confidence(p);
        }

        // ── Pass 6: require_div5 ──────────────────────────────────────────────
        // Comskip: require_div5 — if enabled, drop candidates whose duration is
        // not within DIV5_TOLERANCE seconds of a multiple of 30s.
        if require_div5 {
            if !is_near_div5(dur) {
                // Try snapping: find nearest 30s multiple and check if snapped
                // duration is still within [min_commercial, max_commercial].
                let snapped_dur = snap_to_div5(dur);
                if snapped_dur >= min_commercial && snapped_dur <= max_commercial {
                    // Adjust end to match snapped duration
                    let snapped_end = (start + snapped_dur).min(duration);
                    let mut snapped = CutInterval::new(start, snapped_end, "commercial");
                    for s in &ci.signals { snapped.add_signal(s); }
                    snapped.add_signal("div5_snapped");
                    snapped.set_confidence(ci.confidence);
                    candidates.push(snapped);
                }
                // Drop un-snappable candidate
                continue;
            }
        }

        candidates.push(ci);
    }

    // ── Pass 7: remove_before / remove_after asymmetric trim ─────────────────
    // Comskip: remove_before — trim seconds from the content side of each cut
    //           remove_after  — trim seconds from the ad side of each cut
    // This lets the user recover a few seconds of content that blackdetect
    // clips when the slug boundary is ambiguous on analog tape.
    if remove_before > 0.0 || remove_after > 0.0 {
        for ci in &mut candidates {
            if remove_before > 0.0 {
                ci.start = (ci.start + remove_before).min(ci.end);
                ci.dur = (ci.end - ci.start).max(0.0);
            }
            if remove_after > 0.0 {
                ci.end = (ci.end - remove_after).max(ci.start);
                ci.dur = (ci.end - ci.start).max(0.0);
            }
            // Re-check duration gate after trim
            ci.kind = if ci.duration() >= min_commercial { "commercial" } else { "keep" }.to_string();
        }
    }

    // ── Pass 8: min_show_segment guard ────────────────────────────────────────
    let mut commercials: Vec<CutInterval> = Vec::new();
    let mut cursor = 0.0_f64;
    for mut ci in candidates {
        let keep_before = ci.start - cursor;
        if keep_before < min_show_segment && cursor > 0.0 {
            ci.kind = "keep".to_string();
            ci.add_signal("demoted_min_show_segment");
            ci.set_confidence((ci.confidence * 0.4).max(0.0));
            commercials.push(ci);
        } else {
            commercials.push(ci);
            let last = commercials.last().unwrap();
            if last.kind == "commercial" { cursor = last.end; }
        }
    }

    let true_commercials: Vec<CutInterval> = commercials.into_iter()
        .filter(|c| c.kind == "commercial").collect();

    // ── Pass 9 / Pass 10: build keeps with guard signals ─────────────────────
    let mut keeps: Vec<CutInterval> = Vec::new();
    let mut cursor = 0.0_f64;
    for c in &true_commercials {
        if c.start > cursor {
            let mut k = CutInterval::new(cursor, c.start, "keep");
            k.add_signal("content_between_commercials");
            keeps.push(k);
        }
        cursor = c.end;
    }
    if cursor < duration {
        let mut k = CutInterval::new(cursor, duration, "keep");
        k.add_signal("content_after_last_commercial");
        keeps.push(k);
    }
    for k in &mut keeps {
        if always_keep_first > 0.0 && k.start < protect_head { k.add_signal("always_keep_first"); }
        if always_keep_last  > 0.0 && k.end   > protect_tail { k.add_signal("always_keep_last"); }
    }

    // ── Recording trim ────────────────────────────────────────────────────────
    // Applied after all guard passes: shrink the first/last keep to cut recording
    // artefacts (noise at tape start, deck shutoff at end) from the final output.
    // Unlike always_keep_first/last, trim_head/tail unconditionally remove content
    // from the recording edges regardless of what the commercial detector found.
    if trim_head > 0.0 {
        if let Some(first) = keeps.first_mut() {
            first.start = (first.start + trim_head).min(first.end);
            first.dur   = (first.end - first.start).max(0.0);
            first.add_signal("trim_head");
        }
    }
    if trim_tail > 0.0 {
        if let Some(last) = keeps.last_mut() {
            last.end = (last.end - trim_tail).max(last.start);
            last.dur = (last.end - last.start).max(0.0);
            last.add_signal("trim_tail");
        }
    }

    Plan {
        duration, include_black, edge_pad_pre, edge_pad_post,
        min_commercial, max_commercial, min_show_segment,
        always_keep_first, always_keep_last,
        silence_noise_db, silence_min_dur,
        uniform_max_stddev, scene_threshold,
        remove_before, remove_after, require_div5,
        trim_head, trim_tail,
        blacks: blacks.to_vec(),
        silences: silences.to_vec(),
        uniforms: uniforms.to_vec(),
        scene_changes: scene_changes.to_vec(),
        commercials: true_commercials,
        keeps,
    }
}

// ─── Operational log writers ──────────────────────────────────────────────────

pub fn write_logs(outdir: &Path, _base: &str, plan: &Plan) -> Result<()> {
    let logs_dir = outdir.join("logs");
    fs::create_dir_all(&logs_dir)?;

    // JSON
    fs::write(logs_dir.join("detect.json"), serde_json::to_string_pretty(plan)?)?;

    // CSV
    let csv_path = logs_dir.join("detect.csv");
    let mut f = fs::File::create(csv_path)?;
    writeln!(f, "type,start,end,dur,confidence,signals")?;
    for b in &plan.blacks {
        writeln!(f, "black,{:.3},{:.3},{:.3},,", b.start, b.end, b.duration())?;
    }
    for s in &plan.silences {
        writeln!(f, "silence,{:.3},{:.3},{:.3},,noise_db={:.1}", s.start, s.end, s.duration(), s.noise_db)?;
    }
    for u in &plan.uniforms {
        writeln!(f, "uniform,{:.3},{:.3},{:.3},,stddev={:.2}", u.start, u.end, u.duration(), u.avg_stddev)?;
    }
    for c in &plan.commercials {
        writeln!(f, "commercial,{:.3},{:.3},{:.3},{:.3},\"{}\"",
            c.start, c.end, c.duration(), c.confidence, c.signals.join("|"))?;
    }
    for k in &plan.keeps {
        writeln!(f, "keep,{:.3},{:.3},{:.3},{:.3},\"{}\"",
            k.start, k.end, k.duration(), k.confidence, k.signals.join("|"))?;
    }
    f.flush()?;

    // EDL
    let mut edl = fs::File::create(logs_dir.join("detect.edl"))?;
    for c in &plan.commercials {
        writeln!(edl, "{:.3} {:.3} 0", c.start, c.end)?;
    }
    edl.flush()?;

    // ffmeta chapters — NEW
    // Comskip: output_ffmeta / output_chapters
    // ffmeta format understood by ffmpeg -i input.mp4 -i chapters.ffmeta -map_metadata 1 output.mp4
    // Each keep segment becomes a chapter; commercial blocks are marked as "Advertisement".
    write_ffmeta(&logs_dir, plan)?;

    Ok(())
}

// ─── ffmeta chapter writer ────────────────────────────────────────────────────
//
// Comskip: output_ffmeta — writes a chapter file readable by ffmpeg and most
// modern media players (Kodi, VLC, mpv).
//
// Format:
//   ;FFMETADATA1
//   [CHAPTER]
//   TIMEBASE=1/1000
//   START=<ms>
//   END=<ms>
//   title=<title>

fn write_ffmeta(logs_dir: &Path, plan: &Plan) -> Result<()> {
    let path = logs_dir.join("chapters.ffmeta");
    let mut f = fs::File::create(path)?;
    writeln!(f, ";FFMETADATA1")?;

    // Merge commercials and keeps into a single sorted timeline
    let mut all: Vec<(&CutInterval, &str)> = Vec::new();
    for (idx, c) in plan.commercials.iter().enumerate() {
        let _ = idx;
        all.push((c, "Advertisement"));
    }
    for k in &plan.keeps {
        all.push((k, "Content"));
    }
    all.sort_by(|a, b| a.0.start.partial_cmp(&b.0.start).unwrap());

    for (i, (seg, kind)) in all.iter().enumerate() {
        let start_ms = (seg.start * 1000.0).round() as i64;
        let end_ms   = (seg.end   * 1000.0).round() as i64;
        writeln!(f, "\n[CHAPTER]")?;
        writeln!(f, "TIMEBASE=1/1000")?;
        writeln!(f, "START={}", start_ms)?;
        writeln!(f, "END={}", end_ms)?;
        writeln!(f, "title={} {}", kind, i + 1)?;
    }
    f.flush()?;
    Ok(())
}

// ─── Dataset writer ───────────────────────────────────────────────────────────

pub fn write_dataset(outdir: &Path, plan: &Plan, meta: &RunMeta) -> Result<()> {
    let logs_dir = outdir.join("logs");
    fs::create_dir_all(&logs_dir)?;

    fs::write(logs_dir.join("run_manifest.json"), serde_json::to_string_pretty(meta)?)?;

    let jsonl_path = logs_dir.join("dataset.jsonl");
    let mut jsonl = fs::File::create(&jsonl_path)?;
    let duration = plan.duration.max(1e-9);
    let file_avg_sc = avg_scene_change_rate(&plan.scene_changes, plan.duration);

    let mut all_segs: Vec<(&CutInterval, bool)> = plan.commercials.iter().map(|c| (c, true))
        .chain(plan.keeps.iter().map(|k| (k, false))).collect();
    all_segs.sort_by(|a, b| a.0.start.partial_cmp(&b.0.start).unwrap());

    let mut comm_idx = 0usize;
    let mut keep_idx = 0usize;

    for (tpos, (seg, is_comm)) in all_segs.iter().enumerate() {
        let seg_index = if *is_comm { let i = comm_idx; comm_idx += 1; i }
                        else        { let i = keep_idx;  keep_idx  += 1; i };

        let (sl_count, sl_total, sl_cov) = silence_metrics(&plan.silences, seg.start, seg.end);
        let (un_count, un_total, un_cov) = uniform_metrics(&plan.uniforms, seg.start, seg.end);
        let (bl_left, bl_right)          = black_boundary_durs(&plan.blacks, seg.start, seg.end);
        let (bl_count, bl_total)         = black_overlap_metrics(&plan.blacks, seg.start, seg.end);
        let sc_count = plan.scene_changes.iter().filter(|c| c.pts >= seg.start && c.pts < seg.end).count();
        let sc_rate  = sc_count as f64 / (seg.duration()).max(1e-9);
        let sc_vs_avg = if file_avg_sc > 0.0 { sc_rate / file_avg_sc } else { 1.0 };
        let sil_flag = if has_silence_overlap(&plan.silences, seg.start, seg.end, 0.5) { 1.0 } else { 0.0 };
        let uni_flag = if has_uniform_overlap(&plan.uniforms, seg.start, seg.end, 0.3) { 1.0 } else { 0.0 };

        let record = DatasetRecord {
            run_id: meta.run_id.clone(), source_file: meta.source_file.clone(),
            segment_index: seg_index, timeline_position: tpos,
            start_s: seg.start, end_s: seg.end, dur_s: seg.duration(),
            start_norm: seg.start / duration, end_norm: seg.end / duration,
            dur_norm: seg.duration() / duration,
            offset_from_start_s: seg.start, offset_from_end_s: (duration - seg.end).max(0.0),
            black_left_dur_s: bl_left, black_right_dur_s: bl_right,
            black_overlap_count: bl_count, black_overlap_total_s: bl_total,
            silence_overlap_count: sl_count, silence_overlap_total_s: sl_total,
            silence_coverage: sl_cov, has_silence_overlap: sil_flag,
            uniform_overlap_count: un_count, uniform_overlap_total_s: un_total,
            uniform_coverage: un_cov, has_uniform_overlap: uni_flag,
            scene_change_count: sc_count, scene_change_rate: sc_rate,
            scene_change_rate_vs_avg: sc_vs_avg,
            sig_black_boundary:               has_signal(&seg.signals, "black_boundary"),
            sig_within_commercial_range:      has_signal(&seg.signals, "within_commercial_range"),
            sig_silence_overlap:              has_signal(&seg.signals, "silence_overlap"),
            sig_uniform_overlap:              has_signal(&seg.signals, "uniform_overlap"),
            sig_high_scene_rate:              has_signal(&seg.signals, "high_scene_rate"),
            sig_demoted_min_show_segment:     has_signal(&seg.signals, "demoted_min_show_segment"),
            sig_always_keep_first:            has_signal(&seg.signals, "always_keep_first"),
            sig_always_keep_last:             has_signal(&seg.signals, "always_keep_last"),
            sig_content_between_commercials:  has_signal(&seg.signals, "content_between_commercials"),
            sig_content_after_last_commercial:has_signal(&seg.signals, "content_after_last_commercial"),
            sig_div5_snapped:                 has_signal(&seg.signals, "div5_snapped"),
            sig_trim_head:                    has_signal(&seg.signals, "trim_head"),
            sig_trim_tail:                    has_signal(&seg.signals, "trim_tail"),
            label: seg.kind.clone(), label_int: if *is_comm { 1 } else { 0 },
            confidence: seg.confidence,
            source_duration_s: meta.source_duration_s,
            ffmpeg_version: meta.ffmpeg_version.clone(), app_version: meta.app_version.clone(),
            param_black_min_dur: meta.param_black_min_dur, param_pix_th: meta.param_pix_th,
            param_pic_th: meta.param_pic_th, param_merge_gap: meta.param_merge_gap,
            param_edge_pad_pre: meta.param_edge_pad_pre, param_edge_pad_post: meta.param_edge_pad_post,
            param_min_commercial: meta.param_min_commercial, param_max_commercial: meta.param_max_commercial,
            param_include_black: meta.param_include_black,
            param_silence_noise_db: meta.param_silence_noise_db, param_silence_min_dur: meta.param_silence_min_dur,
            param_min_show_segment: meta.param_min_show_segment,
            param_always_keep_first: meta.param_always_keep_first, param_always_keep_last: meta.param_always_keep_last,
            param_uniform_max_stddev: meta.param_uniform_max_stddev,
            param_scene_threshold: meta.param_scene_threshold,
            param_remove_before: meta.param_remove_before, param_remove_after: meta.param_remove_after,
            param_require_div5: meta.param_require_div5,
            param_trim_head: meta.param_trim_head,
            param_trim_tail: meta.param_trim_tail,
            run_blacks_raw_count: meta.blacks_raw_count, run_blacks_filtered_count: meta.blacks_filtered_count,
            run_silences_count: meta.silences_count, run_uniforms_count: meta.uniforms_count,
            run_scene_changes_count: meta.scene_changes_count, run_avg_scene_rate: meta.avg_scene_rate,
            run_commercial_count: meta.commercial_count, run_keep_count: meta.keep_count,
            run_total_commercial_s: meta.total_commercial_s, run_total_keep_s: meta.total_keep_s,
            run_commercial_ratio: meta.commercial_ratio,
        };
        writeln!(jsonl, "{}", serde_json::to_string(&record)?)?;
    }
    jsonl.flush()?;
    Ok(())
}

// ─── ffmpeg helpers ───────────────────────────────────────────────────────────

// ─── Encode argument builder ─────────────────────────────────────────────────
//
// Translates EncodeSettings into the correct sequence of ffmpeg arguments.
// Called for every cut: commercial clips, keep parts, and the final concat.
// When mode is "copy" and loudnorm/deinterlace/scale are all off, produces
// "-c copy" for maximum speed.  Any filter forces a video re-encode.

fn build_encode_args(enc: &EncodeSettings, apply_loudnorm: bool) -> Vec<String> {
    let mut args: Vec<String> = Vec::new();

    // ── Video filters ─────────────────────────────────────────────────────────
    let mut vf_parts: Vec<String> = Vec::new();
    if enc.deinterlace {
        vf_parts.push("yadif".to_string());
    }
    if enc.scale_width > 0 {
        vf_parts.push(format!("scale={}:-2", enc.scale_width));
    }

    // ── Audio filters ─────────────────────────────────────────────────────────
    let mut af_parts: Vec<String> = Vec::new();
    if apply_loudnorm && enc.loudnorm {
        // EBU R128 single-pass loudnorm.  Target: -23 LUFS, -2 dBTP, 7 LRA.
        af_parts.push("loudnorm=I=-23:TP=-2:LRA=7".to_string());
    }

    let needs_video_encode = enc.mode != "copy" || !vf_parts.is_empty();
    let needs_audio_encode = enc.audio_codec != "copy"
        || !af_parts.is_empty()
        || enc.audio_bitrate_kbps > 0;

    if !needs_video_encode && !needs_audio_encode && enc.mode == "copy" {
        args.push("-c".to_string());
        args.push("copy".to_string());
        return args;
    }

    // ── Video codec ───────────────────────────────────────────────────────────
    if needs_video_encode {
        let vcodec = match (enc.mode.as_str(), enc.gpu_accel.as_str()) {
            ("h264",   "nvenc") => "h264_nvenc",
            ("h264",   "qsv")   => "h264_qsv",
            ("h264",   "amf")   => "h264_amf",
            ("h264",   _)       => "libx264",
            ("h265",   "nvenc") => "hevc_nvenc",
            ("h265",   "qsv")   => "hevc_qsv",
            ("h265",   "amf")   => "hevc_amf",
            ("h265",   _)       => "libx265",
            ("av1",    "nvenc") => "av1_nvenc",
            ("av1",    "qsv")   => "av1_qsv",
            ("av1",    "amf")   => "av1_amf",
            ("av1",    _)       => "libsvtav1",
            ("prores", _)       => "prores_ks",
            _                   => "libx264",  // fallback
        };
        args.extend(["-c:v".to_string(), vcodec.to_string()]);

        // CRF / quality
        match enc.mode.as_str() {
            "prores" => {
                // ProRes uses -profile:v instead of CRF
                // 0=proxy, 1=lt, 2=standard, 3=hq — map CRF loosely
                let profile = if enc.video_crf <= 15 { "3" }
                              else if enc.video_crf <= 20 { "2" }
                              else { "1" };
                args.extend(["-profile:v".to_string(), profile.to_string()]);
            }
            _ if enc.gpu_accel == "none" => {
                args.extend(["-crf".to_string(), enc.video_crf.to_string()]);
                args.extend(["-preset".to_string(), enc.video_preset.clone()]);
            }
            _ => {
                // GPU encoders use -rc and -qp instead of CRF
                args.extend(["-rc".to_string(), "vbr".to_string()]);
                args.extend(["-qp".to_string(), enc.video_crf.to_string()]);
            }
        }

        if !vf_parts.is_empty() {
            args.extend(["-vf".to_string(), vf_parts.join(",")]);
        }
    } else {
        args.extend(["-c:v".to_string(), "copy".to_string()]);
    }

    // ── Audio codec ───────────────────────────────────────────────────────────
    if needs_audio_encode {
        let acodec = match enc.audio_codec.as_str() {
            "ac3"  => "ac3",
            "copy" => "aac",   // copy requested but we have audio filters — need encode
            _      => "aac",
        };
        args.extend(["-c:a".to_string(), acodec.to_string()]);
        if enc.audio_bitrate_kbps > 0 {
            args.extend(["-b:a".to_string(), format!("{}k", enc.audio_bitrate_kbps)]);
        }
        if !af_parts.is_empty() {
            args.extend(["-af".to_string(), af_parts.join(",")]);
        }
    } else {
        args.extend(["-c:a".to_string(), "copy".to_string()]);
    }

    // MP4 fast-start for streaming
    if !matches!(enc.mode.as_str(), "prores") {
        args.extend(["-movflags".to_string(), "+faststart".to_string()]);
    }

    args
}

fn ffmpeg_cut(
    ffmpeg:        &Path,
    input:         &str,
    start:         f64,
    end:           f64,
    out_path:      &Path,
    enc:           &EncodeSettings,
    apply_loudnorm: bool,
    preview_dur:   f64,
) -> Result<()> {
    // Preview cap: truncate each segment to preview_dur seconds when enabled.
    // 0.0 = disabled (export in full).
    let raw_dur = (end - start).max(0.0);
    let dur = if preview_dur > 0.0 { raw_dur.min(preview_dur) } else { raw_dur };
    if dur <= 0.0 { return Ok(()); }

    let encode_args = build_encode_args(enc, apply_loudnorm);

    let mut cmd = Command::new(ffmpeg);
    cmd.args(["-y", "-hide_banner", "-nostats", "-nostdin",
              "-ss", &format!("{:.3}", start), "-i", input, "-t", &format!("{:.3}", dur)]);
    for arg in &encode_args { cmd.arg(arg); }
    cmd.arg(out_path);

    let status = cmd.status().map_err(|e| anyhow!("Failed to launch ffmpeg for cut: {e}"))?;
    if !status.success() {
        return Err(anyhow!("ffmpeg cut failed (status {}) for {}", status, out_path.display()));
    }
    Ok(())
}

// ─── Export functions ─────────────────────────────────────────────────────────

pub fn export_commercials(
    ffmpeg:      &Path,
    input:       &str,
    outdir:      &Path,
    base:        &str,
    plan:        &Plan,
    enc:         &EncodeSettings,
    preview_dur: f64,
    emit:        &dyn Fn(&str),
) -> Result<Vec<PathBuf>> {
    let comm_dir = outdir.join("commercials");
    fs::create_dir_all(&comm_dir)?;
    let mut paths = Vec::new();
    for (idx, c) in plan.commercials.iter().enumerate() {
        let dst = safe_out(&comm_dir.join(format!("{}_ad_{:04}.mp4", base, idx + 1)));
        let preview_label = if preview_dur > 0.0 {
            format!(" [preview ≤{:.0}s]", preview_dur)
        } else { String::new() };
        emit(&format!("Cut COMM {:02}: {} -> {} ({}){} conf={:.2} [{}] -> {}",
            idx + 1, format_ts(c.start), format_ts(c.end), format_ts(c.duration()),
            preview_label, c.confidence, c.signals.join(", "), dst.display()));
        ffmpeg_cut(ffmpeg, input, c.start, c.end, &dst, enc, false, preview_dur)?;
        paths.push(dst);
    }
    Ok(paths)
}

pub fn export_show(
    ffmpeg:      &Path,
    input:       &str,
    outdir:      &Path,
    base:        &str,
    plan:        &Plan,
    enc:         &EncodeSettings,
    preview_dur: f64,
    emit:        &dyn Fn(&str),
) -> Result<PathBuf> {
    if plan.keeps.is_empty() { return Err(anyhow!("No keep intervals — cannot build show file")); }
    let show_dir  = outdir.join("show");
    let parts_dir = show_dir.join("_parts");
    fs::create_dir_all(&parts_dir)?;

    // Cut each keep segment.  loudnorm is applied at the concat stage, not per-part,
    // because EBU R128 needs to analyse the full programme to set the gain correctly.
    // Per-part cuts therefore always use loudnorm=false.
    let mut parts: Vec<PathBuf> = Vec::new();
    for (idx, k) in plan.keeps.iter().enumerate() {
        let part = parts_dir.join(format!("part_{:04}.mp4", idx + 1));
        emit(&format!("Cut KEEP {:02}: {} -> {} ({}) -> {}",
            idx + 1, format_ts(k.start), format_ts(k.end), format_ts(k.duration()), part.display()));
        ffmpeg_cut(ffmpeg, input, k.start, k.end, &part, enc, false, preview_dur)?;
        parts.push(part);
    }

    let out_show = safe_out(&show_dir.join(format!("{}_show.mp4", base)));

    if parts.len() == 1 && !enc.loudnorm {
        // Single segment, no loudnorm — direct copy or already-encoded part
        emit(&format!("Single keep segment — copying directly to: {}", out_show.display()));
        fs::copy(&parts[0], &out_show)?;
        return Ok(out_show);
    }

    // Build concat list
    let list_txt = parts_dir.join("list.txt");
    {
        let mut f = fs::File::create(&list_txt)?;
        for p in &parts {
            let abs = canonicalize(p).unwrap_or_else(|_| p.clone()).to_string_lossy().replace('\\', "/");
            writeln!(f, "file '{}'", abs)?;
        }
        f.flush()?;
    }

    let mut cmd = Command::new(ffmpeg);
    cmd.args(["-y", "-hide_banner", "-nostats", "-nostdin",
              "-f", "concat", "-safe", "0", "-i", list_txt.to_string_lossy().as_ref()]);

    // Apply loudnorm and other encode settings at concat stage
    let encode_args = build_encode_args(enc, enc.loudnorm);
    for arg in &encode_args { cmd.arg(arg); }
    cmd.arg(&out_show);

    if enc.loudnorm {
        emit("Applying loudnorm (EBU R128) during concat…");
    }
    emit(&format!("Concatenating {} parts -> {}", parts.len(), out_show.display()));

    let status = cmd.status().map_err(|e| anyhow!("Failed to launch ffmpeg concat: {e}"))?;
    if !status.success() { return Err(anyhow!("ffmpeg concat failed with status {}", status)); }
    Ok(out_show)
}

// ─── Chapters-only export ─────────────────────────────────────────────────────
//
// Embeds the chapters.ffmeta chapter markers into a copy of the source file
// without removing any content.  The full broadcast is preserved; commercial
// blocks are marked as "Advertisement N" chapters and show segments as
// "Content N" chapters.  Players that read ffmpeg metadata (VLC, mpv, Kodi)
// will display chapter navigation over the uncut recording.
//
// The source file is copied with "-c copy" so the operation is fast and
// lossless regardless of EncodeSettings.  The chapter embed is the only
// ffmpeg operation performed.

pub fn export_chapters_only(
    ffmpeg:   &Path,
    input:    &str,
    outdir:   &Path,
    base:     &str,
    ffmeta:   &Path,
    emit:     &dyn Fn(&str),
) -> Result<PathBuf> {
    let show_dir = outdir.join("show");
    fs::create_dir_all(&show_dir)?;

    let out_path = safe_out(&show_dir.join(format!("{}_chaptered.mp4", base)));

    emit(&format!("Embedding chapters into source → {}", out_path.display()));

    let status = Command::new(ffmpeg)
        .args(["-y", "-hide_banner", "-nostats", "-nostdin",
               "-i", input,
               "-i", ffmeta.to_string_lossy().as_ref(),
               "-map_metadata", "1",
               "-map_chapters", "1",
               "-c", "copy",
               out_path.to_string_lossy().as_ref()])
        .status()
        .map_err(|e| anyhow!("Failed to launch ffmpeg for chapter embed: {e}"))?;

    if !status.success() {
        return Err(anyhow!("ffmpeg chapter embed failed for {}", out_path.display()));
    }

    emit(&format!("Chaptered file → {}", out_path.display()));
    Ok(out_path)
}

// ─── OpenCV structural-segment production export (CV-7) ──────────────────────
//
// OpenCV Adaptive produces a full-coverage structural timeline. It does not yet
// assign semantic "show" / "commercial" labels, so production export writes
// each selected segment independently instead of inventing classifications.

pub fn write_segment_ffmeta(outdir: &Path, segments: &[CutInterval]) -> Result<PathBuf> {
    let logs_dir = outdir.join("logs");
    fs::create_dir_all(&logs_dir)?;
    let path = logs_dir.join("chapters.ffmeta");
    let mut f = fs::File::create(&path)?;
    writeln!(f, ";FFMETADATA1")?;
    for (i, segment) in segments.iter().enumerate() {
        let start_ms = (segment.start * 1000.0).round() as i64;
        let end_ms = (segment.end * 1000.0).round() as i64;
        writeln!(f, "\n[CHAPTER]")?;
        writeln!(f, "TIMEBASE=1/1000")?;
        writeln!(f, "START={start_ms}")?;
        writeln!(f, "END={end_ms}")?;
        writeln!(f, "title=Segment {}", i + 1)?;
    }
    f.flush()?;
    Ok(path)
}

pub fn export_timeline_segments(
    ffmpeg:      &Path,
    input:       &str,
    outdir:      &Path,
    base:        &str,
    segments:    &[CutInterval],
    enc:         &EncodeSettings,
    preview_dur: f64,
    emit:        &dyn Fn(&str),
) -> Result<Vec<PathBuf>> {
    let segment_dir = outdir.join("segments");
    fs::create_dir_all(&segment_dir)?;
    let mut paths = Vec::with_capacity(segments.len());

    for (idx, segment) in segments.iter().enumerate() {
        let dst = safe_out(&segment_dir.join(format!("{}_segment_{:04}.mp4", base, idx + 1)));
        let preview_label = if preview_dur > 0.0 {
            format!(" [preview ≤{:.0}s]", preview_dur)
        } else {
            String::new()
        };
        emit(&format!(
            "Cut SEGMENT {:02}: {} -> {} ({}){} [{}] -> {}",
            idx + 1,
            format_ts(segment.start),
            format_ts(segment.end),
            format_ts(segment.duration()),
            preview_label,
            segment.signals.join(", "),
            dst.display(),
        ));
        ffmpeg_cut(
            ffmpeg,
            input,
            segment.start,
            segment.end,
            &dst,
            enc,
            false,
            preview_dur,
        )?;
        paths.push(dst);
    }

    Ok(paths)
}
