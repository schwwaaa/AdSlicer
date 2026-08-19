#[path = "adslicer_cv_modules.rs"]
mod adslicer;

use anyhow::{anyhow, Result};
use adslicer::cv_boundary::{score_boundary_candidates, write_boundary_candidates, BoundaryScoringConfig};
use adslicer::cv_edit_validation::{render_edit_validation, EditValidationConfig, EditValidationResult};
use adslicer::cv_detect::{analyze_video_with_opencv, sha256_file, write_cv_evidence, CvAnalysisConfig};
use adslicer::cv_shadow_plan::{
    build_shadow_plan, compare_with_legacy, write_legacy_plan, write_plan_comparison,
    write_shadow_plan, LegacyPlanInterval, ShadowPlanConfig,
};
use adslicer::cv_temporal::{analyze_temporal_events, write_temporal_events, TemporalAnalysisConfig};
use adslicer::cv_structural::{build_structural_segmentation, write_structural_segmentation, StructuralSegmentationConfig};
use adslicer::cv_segment_validation::{render_structural_segments, SegmentRenderConfig, SegmentRenderMode, SegmentRenderResult};
use adslicer::detect::{drop_too_short, merge_close, run_blackdetect};
use adslicer::cut::build_plan;
use serde_json::json;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const LEGACY_BLACK_MIN_DUR: f64 = 0.10;
const LEGACY_PIX_TH: f64 = 0.08;
const LEGACY_PIC_TH: f64 = 0.98;
const LEGACY_MERGE_GAP: f64 = 1.5;

fn usage() {
    eprintln!("AdSlicer OpenCV validation pipeline (CV-1 → CV-2 → CV-3 → CV-4 → CV-5 → CV-6)");
    eprintln!("Usage:");
    eprintln!("  adslicer_cv_pipeline --input <video> [--output <dir>] [options]");
    eprintln!();
    eprintln!("Core options:");
    eprintln!("  --input, -i <path>           Source video");
    eprintln!("  --output, -o <path>          Output directory");
    eprintln!("  --black-luma <0..255>        Strict OpenCV black threshold (default 16)");
    eprintln!("  --near-black <0..255>        OpenCV near-black threshold (default 32)");
    eprintln!("  --stride <N>                 Analyze every Nth frame (default 1)");
    eprintln!("  --max-frames <N>             Optional analyzed-frame limit");
    eprintln!();
    eprintln!("CV-4 shadow planner options:");
    eprintln!("  --min-commercial <sec>       Minimum proposed removal duration (default 5)");
    eprintln!("  --max-commercial <sec>       Maximum proposed removal duration (default 240)");
    eprintln!("  --min-show-segment <sec>     Minimum preserved gap guard (default 30)");
    eprintln!("  --edge-pad-pre <sec>         Start padding, same semantics as AdSlicer (default .20)");
    eprintln!("  --edge-pad-post <sec>        End padding, same semantics as AdSlicer (default .06)");
    eprintln!("  --skip-legacy                Skip current-AdSlicer shadow comparison pass");
    eprintln!();
    eprintln!("CV-5 watchable edit-validation options:");
    eprintln!("  --preview-context <sec>      Seconds before/after each proposed cut (default 2)");
    eprintln!("  --preview-limit <N>          Max shadow removals to render (default 20; 0 = all)");
    eprintln!("  --skip-edit-previews         Skip CV-5 validation renders");
    eprintln!();
    eprintln!("CV-6 structural segmentation options:");
    eprintln!("  --segment-render-mode <mode> Render full coverage plans: both|complete|every|none (default both)");
    eprintln!("  --help, -h                   Show this help");
    eprintln!();
    eprintln!("Backward-compatible positional form:");
    eprintln!("  adslicer_cv_pipeline <input-video> [output-dir]");
    eprintln!();
    eprintln!("CV-5/CV-6 render disposable validation media only. They do NOT modify or run AdSlicer's production cut plan.");
}

fn stem_for(input: &Path) -> String {
    input
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "media".to_string())
}

#[derive(Debug)]
struct CliArgs {
    input: PathBuf,
    output: Option<PathBuf>,
    black_luma: Option<u8>,
    near_black: Option<u8>,
    stride: Option<u32>,
    max_frames: Option<u64>,
    min_commercial: Option<f64>,
    max_commercial: Option<f64>,
    min_show_segment: Option<f64>,
    edge_pad_pre: Option<f64>,
    edge_pad_post: Option<f64>,
    legacy_compare: bool,
    preview_context: Option<f64>,
    preview_limit: Option<usize>,
    render_edit_previews: bool,
    segment_render_mode: SegmentRenderMode,
}

fn parse_value<T>(flag: &str, value: String) -> Result<T>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    value
        .parse::<T>()
        .map_err(|e| anyhow!("Invalid value for {flag}: {value} ({e})"))
}

fn next_value<I>(args: &mut I, flag: &str) -> Result<String>
where
    I: Iterator<Item = String>,
{
    args.next().ok_or_else(|| anyhow!("{flag} requires a value"))
}

fn parse_args() -> Result<Option<CliArgs>> {
    let raw: Vec<String> = env::args().skip(1).collect();
    if raw.iter().any(|a| a == "--help" || a == "-h") {
        usage();
        return Ok(None);
    }

    let mut input: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;
    let mut black_luma: Option<u8> = None;
    let mut near_black: Option<u8> = None;
    let mut stride: Option<u32> = None;
    let mut max_frames: Option<u64> = None;
    let mut min_commercial: Option<f64> = None;
    let mut max_commercial: Option<f64> = None;
    let mut min_show_segment: Option<f64> = None;
    let mut edge_pad_pre: Option<f64> = None;
    let mut edge_pad_post: Option<f64> = None;
    let mut legacy_compare = true;
    let mut preview_context: Option<f64> = None;
    let mut preview_limit: Option<usize> = None;
    let mut render_edit_previews = true;
    let mut segment_render_mode = SegmentRenderMode::Both;
    let mut positional: Vec<String> = Vec::new();

    let mut args = raw.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--input" | "-i" => input = Some(PathBuf::from(next_value(&mut args, "--input")?)),
            "--output" | "-o" => output = Some(PathBuf::from(next_value(&mut args, "--output")?)),
            "--black-luma" => black_luma = Some(parse_value("--black-luma", next_value(&mut args, "--black-luma")?)?),
            "--near-black" => near_black = Some(parse_value("--near-black", next_value(&mut args, "--near-black")?)?),
            "--stride" => stride = Some(parse_value("--stride", next_value(&mut args, "--stride")?)?),
            "--max-frames" => max_frames = Some(parse_value("--max-frames", next_value(&mut args, "--max-frames")?)?),
            "--min-commercial" => min_commercial = Some(parse_value("--min-commercial", next_value(&mut args, "--min-commercial")?)?),
            "--max-commercial" => max_commercial = Some(parse_value("--max-commercial", next_value(&mut args, "--max-commercial")?)?),
            "--min-show-segment" => min_show_segment = Some(parse_value("--min-show-segment", next_value(&mut args, "--min-show-segment")?)?),
            "--edge-pad-pre" => edge_pad_pre = Some(parse_value("--edge-pad-pre", next_value(&mut args, "--edge-pad-pre")?)?),
            "--edge-pad-post" => edge_pad_post = Some(parse_value("--edge-pad-post", next_value(&mut args, "--edge-pad-post")?)?),
            "--skip-legacy" => legacy_compare = false,
            "--preview-context" => preview_context = Some(parse_value("--preview-context", next_value(&mut args, "--preview-context")?)?),
            "--preview-limit" => preview_limit = Some(parse_value("--preview-limit", next_value(&mut args, "--preview-limit")?)?),
            "--skip-edit-previews" => render_edit_previews = false,
            "--segment-render-mode" => {
                let value = next_value(&mut args, "--segment-render-mode")?;
                segment_render_mode = match value.as_str() {
                    "both" => SegmentRenderMode::Both,
                    "complete" => SegmentRenderMode::Complete,
                    "every" => SegmentRenderMode::Every,
                    "none" => SegmentRenderMode::None,
                    _ => return Err(anyhow!("--segment-render-mode must be one of: both, complete, every, none")),
                };
            },
            "--" => {
                positional.extend(args.by_ref());
                break;
            }
            _ if arg.starts_with('-') => return Err(anyhow!("Unknown option: {arg}")),
            _ => positional.push(arg),
        }
    }

    if input.is_none() && !positional.is_empty() {
        input = Some(PathBuf::from(positional.remove(0)));
    }
    if output.is_none() && !positional.is_empty() {
        output = Some(PathBuf::from(positional.remove(0)));
    }
    if !positional.is_empty() {
        return Err(anyhow!("Too many positional arguments"));
    }

    let input = input.ok_or_else(|| anyhow!("Missing input video. Use --input <path>."))?;
    if let Some(0) = stride {
        return Err(anyhow!("--stride must be >= 1"));
    }
    if let (Some(black), Some(near)) = (black_luma, near_black) {
        if near < black {
            return Err(anyhow!("--near-black ({near}) must be >= --black-luma ({black})"));
        }
    }
    for (name, value) in [
        ("--min-commercial", min_commercial),
        ("--max-commercial", max_commercial),
        ("--min-show-segment", min_show_segment),
        ("--edge-pad-pre", edge_pad_pre),
        ("--edge-pad-post", edge_pad_post),
        ("--preview-context", preview_context),
    ] {
        if let Some(v) = value {
            if v < 0.0 { return Err(anyhow!("{name} must be >= 0")); }
        }
    }
    if let (Some(min), Some(max)) = (min_commercial, max_commercial) {
        if max < min { return Err(anyhow!("--max-commercial must be >= --min-commercial")); }
    }

    Ok(Some(CliArgs {
        input,
        output,
        black_luma,
        near_black,
        stride,
        max_frames,
        min_commercial,
        max_commercial,
        min_show_segment,
        edge_pad_pre,
        edge_pad_post,
        legacy_compare,
        preview_context,
        preview_limit,
        render_edit_previews,
        segment_render_mode,
    }))
}

fn source_duration_from_cv(cv: &adslicer::cv_detect::CvAnalysisResult) -> f64 {
    if cv.manifest.frame_count_reported > 0.0 && cv.manifest.fps_reported > 0.0 {
        cv.manifest.frame_count_reported / cv.manifest.fps_reported
    } else if let Some(last) = cv.manifest.last_pts_s {
        last + if cv.manifest.fps_reported > 0.0 { 1.0 / cv.manifest.fps_reported } else { 0.0 }
    } else {
        0.0
    }
}

fn bundled_binary(name: &str) -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let suffix = match (env::consts::OS, env::consts::ARCH) {
        ("macos", "aarch64") => Some("aarch64-apple-darwin"),
        ("macos", "x86_64") => Some("x86_64-apple-darwin"),
        _ => None,
    };
    if let Some(suffix) = suffix {
        let bundled = manifest.join("binaries").join(format!("{name}-{suffix}"));
        if bundled.exists() { return bundled; }
    }
    PathBuf::from(name)
}

fn build_legacy_default_plan(input: &Path) -> Result<(Vec<LegacyPlanInterval>, f64)> {
    let ffmpeg = bundled_binary("ffmpeg");
    let ffprobe = bundled_binary("ffprobe");
    let input_str = input.to_str().ok_or_else(|| anyhow!("Input path is not valid UTF-8"))?;

    // For the current reset/default configuration, silence, uniform, and scene
    // signals alter confidence only; they do not alter which intervals are cut.
    // One blackdetect pass therefore reproduces the default edit intervals.
    let (raw, duration, _log) = run_blackdetect(
        &ffmpeg,
        &ffprobe,
        input_str,
        LEGACY_BLACK_MIN_DUR,
        LEGACY_PIX_TH,
        LEGACY_PIC_TH,
        0,
    )?;
    let blacks = merge_close(&drop_too_short(&raw, LEGACY_BLACK_MIN_DUR), LEGACY_MERGE_GAP);
    let plan = build_plan(
        &blacks,
        &[],
        &[],
        &[],
        duration,
        false,
        0.20,
        0.06,
        5.0,
        240.0,
        30.0,
        0.0,
        0.0,
        -40.0,
        0.5,
        8.0,
        0.4,
        0.0,
        0.0,
        false,
        0.0,
        0.0,
    );

    let intervals = plan
        .commercials
        .iter()
        .enumerate()
        .map(|(i, c)| LegacyPlanInterval {
            interval_index: i as u64,
            start_s: c.start,
            end_s: c.end,
            duration_s: c.duration(),
        })
        .collect();
    Ok((intervals, duration))
}

fn main() -> Result<()> {
    let cli = match parse_args()? {
        Some(v) => v,
        None => return Ok(()),
    };

    let input = cli.input;
    if !input.is_file() {
        return Err(anyhow!("Input video does not exist or is not a file: {}", input.display()));
    }

    let outdir = cli.output.unwrap_or_else(|| PathBuf::from("cv-test-output").join(stem_for(&input)));
    let frames_dir = outdir.join("01-frame-evidence");
    let temporal_dir = outdir.join("02-temporal-events");
    let boundary_dir = outdir.join("03-boundary-candidates");
    let shadow_dir = outdir.join("04-shadow-edit-plan");
    let edit_validation_dir = outdir.join("05-edit-validation");
    let structural_dir = outdir.join("06-structural-segmentation");
    fs::create_dir_all(&outdir)?;

    println!("============================================================");
    println!("AdSlicer OpenCV end-to-end validation — CV-6");
    println!("============================================================");
    println!("Source: {}", input.display());
    println!("Output: {}", outdir.display());
    println!();

    println!("[1/6] Frame evidence (OpenCV)");
    let mut cv_cfg = CvAnalysisConfig::default();
    if let Some(v) = cli.black_luma { cv_cfg.black_luma_max = v; }
    if let Some(v) = cli.near_black { cv_cfg.near_black_luma_max = v; }
    if let Some(v) = cli.stride { cv_cfg.sample_stride = v; }
    if let Some(v) = cli.max_frames { cv_cfg.max_analyzed_frames = Some(v); }
    if cv_cfg.near_black_luma_max < cv_cfg.black_luma_max {
        return Err(anyhow!("near-black threshold must be >= strict-black threshold"));
    }
    println!("      Config: black<={} near-black<={} stride={} max-frames={}",
        cv_cfg.black_luma_max, cv_cfg.near_black_luma_max, cv_cfg.sample_stride,
        cv_cfg.max_analyzed_frames.map(|v| v.to_string()).unwrap_or_else(|| "all".to_string()));
    let cv = analyze_video_with_opencv(&input, &cv_cfg)?;
    let cv_paths = write_cv_evidence(&frames_dir, &cv)?;
    println!("      OpenCV: {}", cv.manifest.opencv_version);
    println!("      Frames decoded/analyzed: {}/{}", cv.manifest.frames_decoded, cv.manifest.frames_analyzed);

    println!("[2/6] Temporal evidence");
    let temporal_cfg = TemporalAnalysisConfig::default();
    let mut temporal = analyze_temporal_events(&cv.frames, &temporal_cfg);
    temporal.source_evidence_file = Some("01-frame-evidence/opencv_frame_metrics.jsonl".to_string());
    let evidence_jsonl = frames_dir.join("opencv_frame_metrics.jsonl");
    temporal.source_evidence_sha256 = Some(sha256_file(&evidence_jsonl)?);
    let temporal_paths = write_temporal_events(&temporal_dir, &temporal)?;
    println!("      Temporal events: {}", temporal.summary.total_events);
    for (kind, count) in &temporal.summary.counts_by_kind { println!("        {kind}: {count}"); }

    println!("[3/6] Shadow boundary candidates");
    let boundary_cfg = BoundaryScoringConfig::default();
    let mut boundary = score_boundary_candidates(&temporal, &boundary_cfg);
    boundary.source_temporal_file = Some("02-temporal-events/opencv_temporal_events.json".to_string());
    let temporal_json = temporal_dir.join("opencv_temporal_events.json");
    boundary.source_temporal_sha256 = Some(sha256_file(&temporal_json)?);
    let boundary_paths = write_boundary_candidates(&boundary_dir, &boundary)?;
    println!("      Boundary observations: {}", boundary.summary.total_observations);
    println!("      Candidates: {}", boundary.summary.total_candidates);
    println!("      Evidence-only: {}", boundary.summary.evidence_only);

    println!("[4/6] Shadow edit plan");
    let mut shadow_cfg = ShadowPlanConfig::default();
    if cv.manifest.fps_reported > 0.0 {
        shadow_cfg.frame_duration_s = 1.0 / cv.manifest.fps_reported;
    }
    if let Some(v) = cli.min_commercial { shadow_cfg.min_commercial = v; }
    if let Some(v) = cli.max_commercial { shadow_cfg.max_commercial = v; }
    if let Some(v) = cli.min_show_segment { shadow_cfg.min_show_segment = v; }
    if let Some(v) = cli.edge_pad_pre { shadow_cfg.edge_pad_pre = v; }
    if let Some(v) = cli.edge_pad_post { shadow_cfg.edge_pad_post = v; }
    if shadow_cfg.max_commercial < shadow_cfg.min_commercial {
        return Err(anyhow!("shadow max_commercial must be >= min_commercial"));
    }

    let full_source_duration_s = source_duration_from_cv(&cv);
    let shadow_duration_s = if cli.max_frames.is_some() {
        cv.manifest.last_pts_s.unwrap_or(full_source_duration_s)
            + if cv.manifest.fps_reported > 0.0 { 1.0 / cv.manifest.fps_reported } else { 0.0 }
    } else {
        full_source_duration_s
    };
    let shadow = build_shadow_plan(&boundary, shadow_duration_s, &shadow_cfg);
    let shadow_paths = write_shadow_plan(&shadow_dir, &shadow)?;
    println!("      Proposed removals: {}", shadow.summary.proposed_removals);
    println!("      Rejected pairs: {}", shadow.summary.rejected_pairs);
    println!("      Proposed remove duration: {:.3}s", shadow.summary.total_remove_s);
    for (tier, count) in &shadow.summary.counts_by_tier { println!("        {tier}: {count}"); }

    let legacy_allowed = cli.legacy_compare && cli.max_frames.is_none() && cv_cfg.sample_stride == 1;
    let mut legacy_intervals: Option<Vec<LegacyPlanInterval>> = None;
    let mut comparison = None;
    let mut legacy_paths: Vec<PathBuf> = Vec::new();
    let mut comparison_paths: Vec<PathBuf> = Vec::new();
    let legacy_note: String;

    if legacy_allowed {
        println!();
        println!("      Legacy comparison: current AdSlicer reset defaults");
        match build_legacy_default_plan(&input) {
            Ok((legacy, _duration)) => {
                println!("        Legacy proposed removals: {}", legacy.len());
                legacy_paths = write_legacy_plan(&shadow_dir, &legacy)?;
                let cmp = compare_with_legacy(&shadow, &legacy, &shadow_cfg);
                println!("        Matched intervals: {}", cmp.summary.matched_intervals);
                println!("        OpenCV-only: {}", cmp.summary.opencv_only_intervals);
                println!("        Legacy-only: {}", cmp.summary.legacy_only_intervals);
                if let Some(avg) = cmp.summary.average_abs_boundary_offset_s {
                    println!("        Avg matched boundary offset: {:.3}s", avg);
                }
                comparison_paths = write_plan_comparison(&shadow_dir, &cmp)?;
                comparison = Some(cmp);
                legacy_intervals = Some(legacy);
                legacy_note = "current AdSlicer reset defaults; one additional FFmpeg blackdetect pass".to_string();
            }
            Err(err) => {
                println!("        WARNING: legacy comparison unavailable: {err}");
                legacy_note = format!("legacy comparison failed non-fatally: {err}");
            }
        }
    } else if !cli.legacy_compare {
        legacy_note = "skipped by --skip-legacy".to_string();
    } else {
        legacy_note = "skipped because partial/strided analysis is not comparable to a full legacy plan".to_string();
    }

    println!();
    println!("Proposed shadow removals:");
    if shadow.removals.is_empty() {
        println!("  (none)");
    } else {
        for r in shadow.removals.iter().take(25) {
            println!("  {:>9.3}s → {:>9.3}s | {:>7.3}s | score {:.2} | {}",
                r.start_s, r.end_s, r.duration_s, r.interval_score, r.tier.as_str());
        }
    }

    println!();
    println!("[5/6] Watchable edit validation");
    let mut edit_validation: Option<EditValidationResult> = None;
    let edit_validation_note: String;
    if cli.render_edit_previews {
        let mut edit_cfg = EditValidationConfig::default();
        if let Some(v) = cli.preview_context {
            edit_cfg.context_before_s = v;
            edit_cfg.context_after_s = v;
        }
        if let Some(v) = cli.preview_limit { edit_cfg.max_previews = v; }
        let ffmpeg = bundled_binary("ffmpeg");
        let ffprobe = bundled_binary("ffprobe");
        let rendered = render_edit_validation(
            &ffmpeg,
            &ffprobe,
            &input,
            shadow_duration_s,
            &shadow,
            &edit_validation_dir,
            &edit_cfg,
        )?;
        println!("      Validation previews rendered: {}", rendered.summary.previews_rendered);
        println!("      Validation previews failed: {}", rendered.summary.previews_failed);
        println!("      Skipped by preview limit: {}", rendered.summary.previews_skipped_by_limit);
        println!("      Source audio detected: {}", if rendered.summary.source_has_audio { "yes" } else { "no" });
        edit_validation_note = "disposable validation media rendered from CV-4 shadow removals".to_string();
        edit_validation = Some(rendered);
    } else {
        println!("      skipped by --skip-edit-previews");
        edit_validation_note = "skipped by --skip-edit-previews".to_string();
    }

    println!();
    println!("[6/6] Structural segmentation + full timeline coverage");
    let structural_cfg = StructuralSegmentationConfig::default();
    let structural = build_structural_segmentation(
        &cv.frames,
        &temporal,
        shadow_duration_s,
        &structural_cfg,
    );
    let structural_paths = write_structural_segmentation(&structural_dir, &structural)?;
    println!("      Structural events: {}", structural.summary.structural_events);
    for (role, count) in &structural.summary.counts_by_role { println!("        {role}: {count}"); }
    println!("      Complete Segments: {} clips from {} boundaries",
        structural.summary.complete_segments, structural.summary.complete_boundaries);
    println!("      Every Separator:  {} clips from {} boundaries",
        structural.summary.every_separator_segments, structural.summary.every_separator_boundaries);
    println!("      Complete coverage: {} ({:.3}s uncovered, {:.3}s overlap)",
        if structural.complete_segments.coverage.coverage_pass { "PASS" } else { "FAIL" },
        structural.complete_segments.coverage.uncovered_duration_s,
        structural.complete_segments.coverage.overlap_duration_s);
    println!("      Every coverage:    {} ({:.3}s uncovered, {:.3}s overlap)",
        if structural.every_separator.coverage.coverage_pass { "PASS" } else { "FAIL" },
        structural.every_separator.coverage.uncovered_duration_s,
        structural.every_separator.coverage.overlap_duration_s);

    println!();
    println!("Complete Segments plan:");
    for seg in &structural.complete_segments.segments {
        println!("  segment-{:03}: {:>8.3}s → {:>8.3}s | {:>7.3}s{}",
            seg.segment_index + 1, seg.start_s, seg.end_s, seg.duration_s,
            seg.nearest_broadcast_duration_s
                .filter(|_| seg.broadcast_duration_fit >= 0.50)
                .map(|d| format!(" | ~{d:.0}s broadcast duration"))
                .unwrap_or_default());
    }
    if structural.every_separator.segments.len() != structural.complete_segments.segments.len() {
        println!();
        println!("Every Separator plan (diagnostic):");
        for seg in &structural.every_separator.segments {
            println!("  segment-{:03}: {:>8.3}s → {:>8.3}s | {:>7.3}s",
                seg.segment_index + 1, seg.start_s, seg.end_s, seg.duration_s);
        }
    }

    let mut segment_validation: Option<SegmentRenderResult> = None;
    if cli.segment_render_mode != SegmentRenderMode::None {
        let mut segment_cfg = SegmentRenderConfig::default();
        segment_cfg.mode = cli.segment_render_mode;
        let ffmpeg = bundled_binary("ffmpeg");
        let ffprobe = bundled_binary("ffprobe");
        let rendered = render_structural_segments(
            &ffmpeg,
            &ffprobe,
            &input,
            &structural,
            &structural_dir.join("renders"),
            &segment_cfg,
        )?;
        println!("      Full-segment renders: {} ok, {} failed (mode {})",
            rendered.summary.rendered_ok, rendered.summary.rendered_failed, segment_cfg.mode.as_str());
        segment_validation = Some(rendered);
    } else {
        println!("      Full-segment renders skipped (--segment-render-mode none)");
    }

    let report_path = outdir.join("OPENCV_TEST_REPORT.json");
    let report = json!({
        "schema_version": 4,
        "engine": "adslicer-opencv-validation-pipeline",
        "stages": [
            "cv1_frame_evidence",
            "cv2_temporal_evidence",
            "cv3_shadow_boundary_candidates",
            "cv4_shadow_edit_plan",
            "cv5_watchable_edit_validation",
            "cv6_structural_segmentation"
        ],
        "production_cut_plan_modified": false,
        "render_performed": false,
        "production_render_performed": false,
        "validation_preview_render_performed": edit_validation.as_ref().map(|v| v.summary.previews_rendered > 0).unwrap_or(false),
        "full_segment_validation_render_performed": segment_validation.as_ref().map(|v| v.summary.rendered_ok > 0).unwrap_or(false),
        "analysis_config": {
            "black_luma_max": cv_cfg.black_luma_max,
            "near_black_luma_max": cv_cfg.near_black_luma_max,
            "sample_stride": cv_cfg.sample_stride,
            "max_analyzed_frames": cv_cfg.max_analyzed_frames,
        },
        "shadow_plan_config": shadow_cfg,
        "source": {
            "path": cv.manifest.source_path,
            "file": cv.manifest.source_file,
            "sha256": cv.manifest.source_sha256,
            "size_bytes": cv.manifest.source_size_bytes,
            "width": cv.manifest.width_reported,
            "height": cv.manifest.height_reported,
            "fps": cv.manifest.fps_reported,
            "frames_analyzed": cv.manifest.frames_analyzed,
            "opencv_version": cv.manifest.opencv_version,
            "duration_s": full_source_duration_s,
        },
        "temporal": temporal.summary,
        "boundaries": boundary.summary,
        "shadow_plan": shadow.summary,
        "shadow_removals": shadow.removals,
        "legacy_comparison_note": legacy_note,
        "legacy_default_intervals": legacy_intervals,
        "plan_comparison": comparison,
        "edit_validation_note": edit_validation_note,
        "edit_validation": edit_validation,
        "structural_segmentation": structural,
        "full_segment_validation": segment_validation,
        "outputs": {
            "frame_evidence": cv_paths.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
            "temporal": temporal_paths.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
            "boundaries": boundary_paths.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
            "shadow_plan": shadow_paths.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
            "legacy_plan": legacy_paths.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
            "comparison": comparison_paths.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
            "edit_validation_dir": edit_validation_dir.display().to_string(),
            "structural_segmentation": structural_paths.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
            "structural_dir": structural_dir.display().to_string(),
        }
    });
    fs::write(&report_path, serde_json::to_string_pretty(&report)?)?;

    let text_report_path = outdir.join("OPENCV_TEST_REPORT.txt");
    let mut text = String::new();
    text.push_str("AdSlicer OpenCV Validation Report — CV-6\n");
    text.push_str("========================================\n");
    text.push_str(&format!("Source: {}\n", input.display()));
    text.push_str(&format!("OpenCV: {}\n", cv.manifest.opencv_version));
    text.push_str(&format!("Frames analyzed: {}\n", cv.manifest.frames_analyzed));
    text.push_str(&format!("Temporal events: {}\n", temporal.summary.total_events));
    text.push_str(&format!("Boundary candidates: {}\n", boundary.summary.total_candidates));
    text.push_str(&format!("Shadow proposed removals: {}\n", shadow.summary.proposed_removals));
    text.push_str(&format!("Shadow rejected pairs: {}\n", shadow.summary.rejected_pairs));
    text.push_str(&format!("Shadow remove duration: {:.3}s ({:.2}%)\n",
        shadow.summary.total_remove_s, shadow.summary.remove_ratio * 100.0));
    text.push_str("\nShadow removals:\n");
    if shadow.removals.is_empty() {
        text.push_str("  (none)\n");
    } else {
        for r in &shadow.removals {
            text.push_str(&format!("  {:>9.3}s -> {:>9.3}s | {:>7.3}s | score {:.2} | {}\n",
                r.start_s, r.end_s, r.duration_s, r.interval_score, r.tier.as_str()));
        }
    }
    text.push_str("\nLegacy comparison:\n");
    text.push_str(&format!("  {}\n", legacy_note));
    if let Some(ref cmp) = comparison {
        text.push_str(&format!("  Legacy removals: {}\n", cmp.summary.legacy_intervals));
        text.push_str(&format!("  Matched intervals: {}\n", cmp.summary.matched_intervals));
        text.push_str(&format!("  OpenCV-only intervals: {}\n", cmp.summary.opencv_only_intervals));
        text.push_str(&format!("  Legacy-only intervals: {}\n", cmp.summary.legacy_only_intervals));
        if let Some(avg) = cmp.summary.average_abs_boundary_offset_s {
            text.push_str(&format!("  Average abs matched boundary offset: {:.3}s\n", avg));
        }
        if let Some(max) = cmp.summary.max_abs_boundary_offset_s {
            text.push_str(&format!("  Maximum abs matched boundary offset: {:.3}s\n", max));
        }
    }
    text.push_str("\nCV-5 watchable edit validation:\n");
    text.push_str(&format!("  {}\n", edit_validation_note));
    if let Some(ref validation) = edit_validation {
        text.push_str(&format!("  Validation previews rendered: {}\n", validation.summary.previews_rendered));
        text.push_str(&format!("  Validation previews failed: {}\n", validation.summary.previews_failed));
        text.push_str(&format!("  Skipped by preview limit: {}\n", validation.summary.previews_skipped_by_limit));
        for item in &validation.items {
            text.push_str(&format!("  Edit {:03}: {:.3}s -> {:.3}s | {}\n",
                item.interval_index + 1, item.start_s, item.end_s, item.status));
            if let Some(ref p) = item.removed_content_file {
                text.push_str(&format!("    removed content: {}\n", p));
            }
            if let Some(ref p) = item.result_preview_file {
                text.push_str(&format!("    resulting edit:  {}\n", p));
            }
        }
    }
    text.push_str("\nProduction cut plan modified: NO\n");
    text.push_str("Production render performed: NO\n");
    text.push_str("CV-5/CV-6 validation media may have been rendered; these are disposable QC media only.\n");
    text.push_str("\nCV-6 STRUCTURAL SEGMENTATION\n");
    text.push_str(&format!("Structural events: {}\n", structural.summary.structural_events));
    for (role, count) in &structural.summary.counts_by_role {
        text.push_str(&format!("  {role}: {count}\n"));
    }
    text.push_str(&format!("Complete Segments: {} clips / {} boundaries / coverage {}\n",
        structural.summary.complete_segments, structural.summary.complete_boundaries,
        if structural.complete_segments.coverage.coverage_pass { "PASS" } else { "FAIL" }));
    text.push_str(&format!("Every Separator: {} clips / {} boundaries / coverage {}\n",
        structural.summary.every_separator_segments, structural.summary.every_separator_boundaries,
        if structural.every_separator.coverage.coverage_pass { "PASS" } else { "FAIL" }));
    text.push_str("Complete Segments plan:\n");
    for seg in &structural.complete_segments.segments {
        text.push_str(&format!("  segment-{:03}: {:.3}s -> {:.3}s ({:.3}s)\n",
            seg.segment_index + 1, seg.start_s, seg.end_s, seg.duration_s));
    }
    if structural.every_separator.segments.len() != structural.complete_segments.segments.len() {
        text.push_str("Every Separator plan (diagnostic):\n");
        for seg in &structural.every_separator.segments {
            text.push_str(&format!("  segment-{:03}: {:.3}s -> {:.3}s ({:.3}s)\n",
                seg.segment_index + 1, seg.start_s, seg.end_s, seg.duration_s));
        }
    }
    if let Some(ref rendered) = segment_validation {
        text.push_str(&format!("Full timeline validation renders: {} ok / {} failed\n",
            rendered.summary.rendered_ok, rendered.summary.rendered_failed));
    }

    fs::write(&text_report_path, text)?;

    println!();
    println!("============================================================");
    println!("DONE — CV-1 → CV-2 → CV-3 → CV-4 → CV-5 → CV-6 completed from one media command.");
    println!("OpenCV source decode: one pass. Legacy comparison, when enabled, uses one additional FFmpeg blackdetect pass.");
    println!("Production AdSlicer cuts were NOT changed. CV-5/CV-6 rendered validation media only; no production render was performed.");
    println!("Reports:\n  {}\n  {}", text_report_path.display(), report_path.display());
    println!("============================================================");

    Ok(())
}
