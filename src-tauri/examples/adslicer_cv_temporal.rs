#[path = "../src/adslicer/cv_detect.rs"]
mod cv_detect;
#[path = "../src/adslicer/cv_temporal.rs"]
mod cv_temporal;

use anyhow::{anyhow, Result};
use cv_detect::{read_frame_metrics_jsonl, sha256_file};
use cv_temporal::{analyze_temporal_events, write_temporal_events, TemporalAnalysisConfig};
use std::env;
use std::path::PathBuf;

fn usage() {
    eprintln!("AdSlicer temporal evidence analyzer (CV-2)");
    eprintln!("Usage:");
    eprintln!("  cargo run --example adslicer_cv_temporal -- <opencv_frame_metrics.jsonl> [output-dir]");
    eprintln!();
    eprintln!("CV-2 consumes cached CV-1 evidence. It does not decode the video again and does not create cuts.");
}

fn main() -> Result<()> {
    let mut args = env::args().skip(1);
    let evidence_path = match args.next() {
        Some(v) => PathBuf::from(v),
        None => {
            usage();
            return Err(anyhow!("Missing opencv_frame_metrics.jsonl input"));
        }
    };
    let outdir = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("cv-temporal-output"));

    let frames = read_frame_metrics_jsonl(&evidence_path)?;
    if frames.is_empty() {
        return Err(anyhow!("No frame metrics found in {}", evidence_path.display()));
    }

    let config = TemporalAnalysisConfig::default();
    let mut result = analyze_temporal_events(&frames, &config);
    result.source_evidence_file = evidence_path
        .file_name()
        .map(|s| s.to_string_lossy().to_string());
    result.source_evidence_sha256 = Some(sha256_file(&evidence_path)?);

    println!("[cv2] evidence: {}", evidence_path.display());
    println!("[cv2] frames read: {}", frames.len());
    println!("[cv2] temporal events: {}", result.summary.total_events);
    for (kind, count) in &result.summary.counts_by_kind {
        println!("[cv2]   {kind}: {count}");
    }

    let paths = write_temporal_events(&outdir, &result)?;
    for p in paths {
        println!("[cv2] wrote: {}", p.display());
    }

    Ok(())
}
