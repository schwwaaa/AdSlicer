#[path = "adslicer_cv_modules.rs"]
mod adslicer;

use anyhow::{anyhow, Result};
use adslicer::cv_production::{build_opencv_production_plan, CvProductionPolicy};
use std::env;
use std::path::PathBuf;

fn usage() {
    eprintln!("AdSlicer CV-7 production integration probe");
    eprintln!("Usage: adslicer_cv7_production --input <video> --output <dir> [--policy complete_segments|every_separator]");
}

fn main() -> Result<()> {
    let mut args = env::args().skip(1);
    let mut input: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;
    let mut policy = CvProductionPolicy::CompleteSegments;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--input" | "-i" => input = args.next().map(PathBuf::from),
            "--output" | "-o" => output = args.next().map(PathBuf::from),
            "--policy" => {
                let value = args.next().ok_or_else(|| anyhow!("--policy requires a value"))?;
                policy = CvProductionPolicy::from_param(&value)?;
            }
            "--help" | "-h" => {
                usage();
                return Ok(());
            }
            other => return Err(anyhow!("Unknown option: {other}")),
        }
    }

    let input = input.ok_or_else(|| anyhow!("--input is required"))?;
    let output = output.ok_or_else(|| anyhow!("--output is required"))?;
    if !input.is_file() {
        return Err(anyhow!("Input video does not exist: {}", input.display()));
    }

    let plan = build_opencv_production_plan(&input, &output, policy)?;
    println!("CV-7 production integration PASS");
    println!("Engine: {}", plan.engine);
    println!("Policy: {}", plan.policy.display_name());
    println!("Segments: {}", plan.segments.len());
    println!("Coverage: {}", if plan.coverage.coverage_pass { "PASS" } else { "FAIL" });
    println!("Uncovered: {:.6}s", plan.coverage.uncovered_duration_s);
    println!("Overlap: {:.6}s", plan.coverage.overlap_duration_s);
    println!("Manifest: {}", output.join("opencv_production_manifest.json").display());
    Ok(())
}
