#[path = "../src/adslicer/cv_detect.rs"]
mod cv_detect;
#[path = "../src/adslicer/cv_temporal.rs"]
mod cv_temporal;
#[path = "../src/adslicer/cv_boundary.rs"]
mod cv_boundary;

use anyhow::{anyhow, Result};
use cv_boundary::{score_boundary_candidates, write_boundary_candidates, BoundaryScoringConfig};
use cv_detect::{analyze_video_with_opencv, sha256_file, write_cv_evidence, CvAnalysisConfig};
use cv_temporal::{analyze_temporal_events, write_temporal_events, TemporalAnalysisConfig};
use serde_json::json;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn usage() {
    eprintln!("AdSlicer OpenCV end-to-end validation pipeline (CV-1 → CV-2 → CV-3)");
    eprintln!("Usage:");
    eprintln!("  adslicer_cv_pipeline --input <video> [--output <dir>] [options]");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --input, -i <path>       Source video");
    eprintln!("  --output, -o <path>      Output directory");
    eprintln!("  --black-luma <0..255>    Strict black threshold (default 16)");
    eprintln!("  --near-black <0..255>    Near-black threshold (default 32)");
    eprintln!("  --stride <N>             Analyze every Nth frame (default 1)");
    eprintln!("  --max-frames <N>         Optional analyzed-frame limit");
    eprintln!("  --help, -h               Show this help");
    eprintln!();
    eprintln!("Backward-compatible positional form:");
    eprintln!("  adslicer_cv_pipeline <input-video> [output-dir]");
    eprintln!();
    eprintln!("This command performs all current OpenCV validation stages in one run.");
    eprintln!("It does NOT modify AdSlicer's active production cut plan or render media.");
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
    args.next()
        .ok_or_else(|| anyhow!("{flag} requires a value"))
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
    let mut positional: Vec<String> = Vec::new();

    let mut args = raw.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--input" | "-i" => {
                input = Some(PathBuf::from(next_value(&mut args, "--input")?));
            }
            "--output" | "-o" => {
                output = Some(PathBuf::from(next_value(&mut args, "--output")?));
            }
            "--black-luma" => {
                let value = next_value(&mut args, "--black-luma")?;
                black_luma = Some(parse_value("--black-luma", value)?);
            }
            "--near-black" => {
                let value = next_value(&mut args, "--near-black")?;
                near_black = Some(parse_value("--near-black", value)?);
            }
            "--stride" => {
                let value = next_value(&mut args, "--stride")?;
                stride = Some(parse_value("--stride", value)?);
            }
            "--max-frames" => {
                let value = next_value(&mut args, "--max-frames")?;
                max_frames = Some(parse_value("--max-frames", value)?);
            }
            "--" => {
                positional.extend(args.by_ref());
                break;
            },
            _ if arg.starts_with('-') => {
                return Err(anyhow!("Unknown option: {arg}"));
            }
            _ => positional.push(arg),
        }
    }

    // Preserve the old direct-executable positional syntax, but explicit flags
    // are the stable interface used by test-opencv.sh.
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
            return Err(anyhow!(
                "--near-black ({near}) must be >= --black-luma ({black})"
            ));
        }
    }

    Ok(Some(CliArgs {
        input,
        output,
        black_luma,
        near_black,
        stride,
        max_frames,
    }))
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

    let outdir = cli
        .output
        .unwrap_or_else(|| PathBuf::from("cv-test-output").join(stem_for(&input)));

    let frames_dir = outdir.join("01-frame-evidence");
    let temporal_dir = outdir.join("02-temporal-events");
    let boundary_dir = outdir.join("03-boundary-candidates");
    fs::create_dir_all(&outdir)?;

    println!("============================================================");
    println!("AdSlicer OpenCV end-to-end validation");
    println!("============================================================");
    println!("Source: {}", input.display());
    println!("Output: {}", outdir.display());
    println!();

    // CV-1: decode once and create per-frame evidence.
    println!("[1/3] Frame evidence (OpenCV)");
    let mut cv_cfg = CvAnalysisConfig::default();
    if let Some(v) = cli.black_luma {
        cv_cfg.black_luma_max = v;
    }
    if let Some(v) = cli.near_black {
        cv_cfg.near_black_luma_max = v;
    }
    if let Some(v) = cli.stride {
        cv_cfg.sample_stride = v;
    }
    if let Some(v) = cli.max_frames {
        cv_cfg.max_analyzed_frames = Some(v);
    }
    if cv_cfg.near_black_luma_max < cv_cfg.black_luma_max {
        return Err(anyhow!(
            "near-black threshold ({}) must be >= strict-black threshold ({})",
            cv_cfg.near_black_luma_max,
            cv_cfg.black_luma_max
        ));
    }
    println!(
        "      Config: black<={} near-black<={} stride={} max-frames={}",
        cv_cfg.black_luma_max,
        cv_cfg.near_black_luma_max,
        cv_cfg.sample_stride,
        cv_cfg
            .max_analyzed_frames
            .map(|v| v.to_string())
            .unwrap_or_else(|| "all".to_string())
    );
    let cv = analyze_video_with_opencv(&input, &cv_cfg)?;
    let cv_paths = write_cv_evidence(&frames_dir, &cv)?;
    println!("      OpenCV: {}", cv.manifest.opencv_version);
    println!("      Frames decoded/analyzed: {}/{}", cv.manifest.frames_decoded, cv.manifest.frames_analyzed);
    println!("      FPS: {:.6}", cv.manifest.fps_reported);

    // CV-2: use the in-memory frame evidence. No file hunting and no re-decode.
    println!("[2/3] Temporal evidence");
    let temporal_cfg = TemporalAnalysisConfig::default();
    let mut temporal = analyze_temporal_events(&cv.frames, &temporal_cfg);
    temporal.source_evidence_file = Some("01-frame-evidence/opencv_frame_metrics.jsonl".to_string());
    let evidence_jsonl = frames_dir.join("opencv_frame_metrics.jsonl");
    temporal.source_evidence_sha256 = Some(sha256_file(&evidence_jsonl)?);
    let temporal_paths = write_temporal_events(&temporal_dir, &temporal)?;
    println!("      Temporal events: {}", temporal.summary.total_events);
    for (kind, count) in &temporal.summary.counts_by_kind {
        println!("        {kind}: {count}");
    }

    // CV-3: score temporal evidence directly in memory.
    println!("[3/3] Shadow boundary candidates");
    let boundary_cfg = BoundaryScoringConfig::default();
    let mut boundary = score_boundary_candidates(&temporal, &boundary_cfg);
    boundary.source_temporal_file = Some("02-temporal-events/opencv_temporal_events.json".to_string());
    let temporal_json = temporal_dir.join("opencv_temporal_events.json");
    boundary.source_temporal_sha256 = Some(sha256_file(&temporal_json)?);
    let boundary_paths = write_boundary_candidates(&boundary_dir, &boundary)?;
    println!("      Boundary observations: {}", boundary.summary.total_observations);
    println!("      Candidates: {}", boundary.summary.total_candidates);
    println!("      Evidence-only: {}", boundary.summary.evidence_only);
    for (tier, count) in &boundary.summary.counts_by_tier {
        println!("        {tier}: {count}");
    }

    // Human-friendly top candidate list.
    let mut ranked = boundary.candidates.clone();
    ranked.sort_by(|a, b| {
        b.boundary_score
            .total_cmp(&a.boundary_score)
            .then_with(|| a.anchor_pts_s.total_cmp(&b.anchor_pts_s))
    });

    println!();
    println!("Top boundary candidates:");
    if ranked.is_empty() {
        println!("  (none above the current CV-3 candidate threshold)");
    } else {
        for c in ranked.iter().take(12) {
            println!(
                "  {:>9.3}s  score {:.2}  {:<7}  {:<18}  source={}",
                c.anchor_pts_s,
                c.boundary_score,
                c.tier.as_str(),
                c.role.as_str(),
                c.source_event_kind.as_str(),
            );
        }
    }

    let report_path = outdir.join("OPENCV_TEST_REPORT.json");
    let top_candidates: Vec<_> = ranked
        .iter()
        .take(25)
        .map(|c| json!({
            "time_s": c.anchor_pts_s,
            "frame": c.anchor_frame,
            "score": c.boundary_score,
            "tier": c.tier.as_str(),
            "role": c.role.as_str(),
            "source_event": c.source_event_kind.as_str(),
            "reasons": c.reasons.clone(),
            "cautions": c.cautions.clone(),
        }))
        .collect();

    let report = json!({
        "schema_version": 1,
        "engine": "adslicer-opencv-validation-pipeline",
        "stages": ["cv1_frame_evidence", "cv2_temporal_evidence", "cv3_shadow_boundary_candidates"],
        "production_cut_plan_modified": false,
        "analysis_config": {
            "black_luma_max": cv_cfg.black_luma_max,
            "near_black_luma_max": cv_cfg.near_black_luma_max,
            "sample_stride": cv_cfg.sample_stride,
            "max_analyzed_frames": cv_cfg.max_analyzed_frames,
        },
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
        },
        "temporal": temporal.summary,
        "boundaries": boundary.summary,
        "top_candidates": top_candidates,
        "outputs": {
            "frame_evidence": cv_paths.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
            "temporal": temporal_paths.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
            "boundaries": boundary_paths.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
        }
    });
    fs::write(&report_path, serde_json::to_string_pretty(&report)?)?;

    let text_report_path = outdir.join("OPENCV_TEST_REPORT.txt");
    let mut text = String::new();
    text.push_str("AdSlicer OpenCV Validation Report\n");
    text.push_str("=================================\n");
    text.push_str(&format!("Source: {}\n", input.display()));
    text.push_str(&format!("OpenCV: {}\n", cv.manifest.opencv_version));
    text.push_str(&format!(
        "Analysis config: black<={} near-black<={} stride={} max-frames={}\n",
        cv_cfg.black_luma_max,
        cv_cfg.near_black_luma_max,
        cv_cfg.sample_stride,
        cv_cfg
            .max_analyzed_frames
            .map(|v| v.to_string())
            .unwrap_or_else(|| "all".to_string())
    ));
    text.push_str(&format!("Frames analyzed: {}\n", cv.manifest.frames_analyzed));
    text.push_str(&format!("Temporal events: {}\n", temporal.summary.total_events));
    for (kind, count) in &temporal.summary.counts_by_kind {
        text.push_str(&format!("  {kind}: {count}\n"));
    }
    text.push_str(&format!("Boundary candidates: {}\n", boundary.summary.total_candidates));
    text.push_str(&format!("Evidence-only observations: {}\n", boundary.summary.evidence_only));
    for (tier, count) in &boundary.summary.counts_by_tier {
        text.push_str(&format!("  {tier}: {count}\n"));
    }
    text.push_str("\nTop boundary candidates:\n");
    if ranked.is_empty() {
        text.push_str("  (none above threshold)\n");
    } else {
        for c in ranked.iter().take(25) {
            text.push_str(&format!(
                "  {:>9.3}s | score {:.2} | {:<7} | {:<18} | {}\n",
                c.anchor_pts_s,
                c.boundary_score,
                c.tier.as_str(),
                c.role.as_str(),
                c.source_event_kind.as_str(),
            ));
        }
    }
    text.push_str("\nProduction cut plan modified: NO\n");
    fs::write(&text_report_path, text)?;

    println!();
    println!("============================================================");
    println!("DONE — one source file was decoded once and passed through CV-1 → CV-2 → CV-3.");
    println!("Production AdSlicer cuts were NOT changed.");
    println!("Reports:\n  {}\n  {}", text_report_path.display(), report_path.display());
    println!("============================================================");

    Ok(())
}
