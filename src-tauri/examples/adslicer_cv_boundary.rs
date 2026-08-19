#[path = "../src/adslicer/cv_detect.rs"]
mod cv_detect;
#[path = "../src/adslicer/cv_temporal.rs"]
mod cv_temporal;
#[path = "../src/adslicer/cv_boundary.rs"]
mod cv_boundary;

use anyhow::{anyhow, Result};
use cv_boundary::{
    read_temporal_analysis_json, score_boundary_candidates, write_boundary_candidates,
    BoundaryScoringConfig,
};
use cv_detect::sha256_file;
use std::env;
use std::path::PathBuf;

fn usage() {
    eprintln!("AdSlicer shadow boundary candidate analyzer (CV-3)");
    eprintln!("Usage:");
    eprintln!("  cargo run --example adslicer_cv_boundary -- <opencv_temporal_events.json> [output-dir]");
    eprintln!();
    eprintln!("CV-3 scores temporal evidence only. It does not modify AdSlicer's active cut plan or render media.");
}

fn main() -> Result<()> {
    let mut args = env::args().skip(1);
    let temporal_path = match args.next() {
        Some(v) => PathBuf::from(v),
        None => {
            usage();
            return Err(anyhow!("Missing opencv_temporal_events.json input"));
        }
    };
    let outdir = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("cv-boundary-output"));

    let temporal = read_temporal_analysis_json(&temporal_path)?;
    let cfg = BoundaryScoringConfig::default();
    let mut result = score_boundary_candidates(&temporal, &cfg);
    result.source_temporal_file = temporal_path
        .file_name()
        .map(|s| s.to_string_lossy().to_string());
    result.source_temporal_sha256 = Some(sha256_file(&temporal_path)?);

    println!("[cv3] temporal evidence: {}", temporal_path.display());
    println!("[cv3] temporal events read: {}", temporal.summary.total_events);
    println!("[cv3] boundary observations: {}", result.summary.total_observations);
    println!("[cv3] boundary candidates: {}", result.summary.total_candidates);
    println!("[cv3] evidence-only: {}", result.summary.evidence_only);
    for (tier, count) in &result.summary.counts_by_tier {
        println!("[cv3]   {tier}: {count}");
    }

    let paths = write_boundary_candidates(&outdir, &result)?;
    for p in paths {
        println!("[cv3] wrote: {}", p.display());
    }
    Ok(())
}
