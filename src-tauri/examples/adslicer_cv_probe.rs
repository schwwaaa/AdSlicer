#[path = "../src/adslicer/cv_detect.rs"]
mod cv_detect;

use anyhow::{anyhow, Result};
use cv_detect::{analyze_video_with_opencv, write_cv_evidence, CvAnalysisConfig};
use std::env;
use std::path::PathBuf;

fn usage() {
    eprintln!("AdSlicer OpenCV evidence probe (CV-1)");
    eprintln!("Usage:");
    eprintln!("  cargo run --example adslicer_cv_probe --features opencv-analysis -- <input-video> [output-dir]");
    eprintln!("Environment overrides:");
    eprintln!("  ADSLICER_CV_BLACK_MAX=16");
    eprintln!("  ADSLICER_CV_NEAR_BLACK_MAX=32");
    eprintln!("  ADSLICER_CV_MAX_FRAMES=300");
}

fn parse_u8_env(name: &str, default: u8) -> Result<u8> {
    match env::var(name) {
        Ok(v) => v.parse::<u8>().map_err(|e| anyhow!("Invalid {name}={v}: {e}")),
        Err(_) => Ok(default),
    }
}

fn main() -> Result<()> {
    let mut args = env::args().skip(1);
    let input = match args.next() {
        Some(v) => PathBuf::from(v),
        None => {
            usage();
            return Err(anyhow!("Missing input video"));
        }
    };
    let outdir = args.next()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("cv-probe-output"));

    let mut config = CvAnalysisConfig::default();
    config.black_luma_max = parse_u8_env("ADSLICER_CV_BLACK_MAX", config.black_luma_max)?;
    config.near_black_luma_max = parse_u8_env("ADSLICER_CV_NEAR_BLACK_MAX", config.near_black_luma_max)?;
    config.max_analyzed_frames = env::var("ADSLICER_CV_MAX_FRAMES")
        .ok()
        .map(|v| v.parse::<u64>().map_err(|e| anyhow!("Invalid ADSLICER_CV_MAX_FRAMES={v}: {e}")))
        .transpose()?;

    println!("[cv] source: {}", input.display());
    println!("[cv] strict black <= {}", config.black_luma_max);
    println!("[cv] near black   <= {}", config.near_black_luma_max);

    let result = analyze_video_with_opencv(&input, &config)?;
    let paths = write_cv_evidence(&outdir, &result)?;

    println!("[cv] OpenCV: {}", result.manifest.opencv_version);
    println!("[cv] decoded frames: {}", result.manifest.frames_decoded);
    println!("[cv] analyzed frames: {}", result.manifest.frames_analyzed);
    println!("[cv] fps reported: {:.6}", result.manifest.fps_reported);
    for p in paths {
        println!("[cv] wrote: {}", p.display());
    }

    Ok(())
}
