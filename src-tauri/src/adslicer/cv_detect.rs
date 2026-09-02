//! OpenCV evidence probe for AdSlicer.
//!
//! Pass CV-1 deliberately does NOT change commercial planning or rendering.
//! It provides a feature-gated, reproducible per-frame evidence stream that can
//! be compared against the existing FFmpeg detectors before OpenCV is promoted
//! into the production decision path.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CvAnalysisConfig {
    /// Pixels at or below this luma value count as strict black evidence.
    pub black_luma_max: u8,
    /// Pixels at or below this luma value count as near-black evidence.
    pub near_black_luma_max: u8,
    /// Analyze every Nth decoded frame. Keep at 1 for boundary validation.
    pub sample_stride: u32,
    /// Optional hard limit for short probes. None analyzes the whole source.
    pub max_analyzed_frames: Option<u64>,
}

impl Default for CvAnalysisConfig {
    fn default() -> Self {
        Self {
            black_luma_max: 16,
            near_black_luma_max: 32,
            sample_stride: 1,
            max_analyzed_frames: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameMetrics {
    pub frame_index: u64,
    pub pts_s: f64,
    pub width: i32,
    pub height: i32,

    // Luma evidence
    pub mean_luma: f64,
    pub stddev_luma: f64,
    pub min_luma: u8,
    pub max_luma: u8,
    pub black_pixel_ratio: f64,
    pub near_black_pixel_ratio: f64,

    // Color evidence. OpenCV VideoCapture normally returns BGR frames.
    pub mean_blue: f64,
    pub mean_green: f64,
    pub mean_red: f64,
    pub mean_hue: f64,
    pub mean_saturation: f64,
    pub mean_value: f64,

    // Mean absolute luma difference from the previous analyzed frame.
    // This is intentionally a measurement, not yet a scene-cut decision.
    pub frame_delta_mean: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CvAnalysisManifest {
    pub schema_version: u32,
    pub engine: String,
    pub app_version: String,
    pub source_file: String,
    pub source_path: String,
    pub source_size_bytes: u64,
    pub source_sha256: String,
    pub opencv_version: String,
    pub fps_reported: f64,
    pub width_reported: i32,
    pub height_reported: i32,
    pub frame_count_reported: f64,
    pub frames_decoded: u64,
    pub frames_analyzed: u64,
    pub first_pts_s: Option<f64>,
    pub last_pts_s: Option<f64>,
    pub config: CvAnalysisConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CvAnalysisResult {
    pub manifest: CvAnalysisManifest,
    pub frames: Vec<FrameMetrics>,
}

/// Lightweight runtime telemetry for the product UI. This does not participate
/// in detection decisions; it only reports how far the existing frame analysis
/// has progressed.
#[derive(Debug, Clone, Copy)]
pub struct CvFrameProgress {
    pub frames_decoded: u64,
    pub frames_analyzed: u64,
    pub total_frames: Option<u64>,
    pub source_position_s: Option<f64>,
    pub source_duration_s: Option<f64>,
    pub elapsed_s: f64,
    pub processing_fps: f64,
    pub eta_s: Option<f64>,
    pub scan_complete: bool,
}


pub fn sha256_file(path: &Path) -> Result<String> {
    sha256_file_with_cancel(path, &|| false)
}

pub fn sha256_file_with_cancel(path: &Path, should_cancel: &dyn Fn() -> bool) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 1024 * 1024];
    loop {
        if should_cancel() {
            return Err(anyhow!("Job cancelled by user."));
        }
        let n = std::io::Read::read(&mut file, &mut buffer)?;
        if n == 0 { break; }
        hasher.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(feature = "opencv-analysis")]
pub fn analyze_video_with_opencv(input: &Path, config: &CvAnalysisConfig) -> Result<CvAnalysisResult> {
    analyze_video_with_opencv_progress(input, config, &|_| {})
}

#[cfg(feature = "opencv-analysis")]
pub fn analyze_video_with_opencv_progress(
    input: &Path,
    config: &CvAnalysisConfig,
    progress: &dyn Fn(CvFrameProgress),
) -> Result<CvAnalysisResult> {
    analyze_video_with_opencv_progress_cancel(input, config, progress, &|| false)
}

#[cfg(feature = "opencv-analysis")]
pub fn analyze_video_with_opencv_progress_cancel(
    input: &Path,
    config: &CvAnalysisConfig,
    progress: &dyn Fn(CvFrameProgress),
    should_cancel: &dyn Fn() -> bool,
) -> Result<CvAnalysisResult> {
    use opencv::{core, imgproc, prelude::*, videoio};

    if config.sample_stride == 0 {
        return Err(anyhow!("sample_stride must be >= 1"));
    }
    if config.near_black_luma_max < config.black_luma_max {
        return Err(anyhow!(
            "near_black_luma_max ({}) must be >= black_luma_max ({})",
            config.near_black_luma_max,
            config.black_luma_max
        ));
    }

    let input_str = input
        .to_str()
        .ok_or_else(|| anyhow!("Input path is not valid UTF-8: {}", input.display()))?;

    let mut cap = videoio::VideoCapture::from_file(input_str, videoio::CAP_ANY)
        .map_err(|e| anyhow!("OpenCV VideoCapture could not open {}: {e}", input.display()))?;

    if !cap.is_opened()? {
        return Err(anyhow!("OpenCV VideoCapture did not open source: {}", input.display()));
    }

    let fps_reported = cap.get(videoio::CAP_PROP_FPS).unwrap_or(0.0);
    let width_reported = cap.get(videoio::CAP_PROP_FRAME_WIDTH).unwrap_or(0.0).round() as i32;
    let height_reported = cap.get(videoio::CAP_PROP_FRAME_HEIGHT).unwrap_or(0.0).round() as i32;
    let frame_count_reported = cap.get(videoio::CAP_PROP_FRAME_COUNT).unwrap_or(0.0);

    let mut frames: Vec<FrameMetrics> = Vec::new();
    let mut decoded_index = 0u64;
    let mut analyzed_count = 0u64;
    let mut previous_gray: Option<Vec<u8>> = None;
    let analysis_started = Instant::now();
    let mut last_progress_emit = Instant::now() - Duration::from_secs(1);
    let total_frames = if frame_count_reported.is_finite() && frame_count_reported > 0.0 {
        Some(frame_count_reported.round() as u64)
    } else {
        None
    };
    let source_duration_s = total_frames.and_then(|total| {
        if fps_reported > 0.0 { Some(total as f64 / fps_reported) } else { None }
    });

    loop {
        if should_cancel() {
            return Err(anyhow!("Job cancelled by user."));
        }
        let mut frame = core::Mat::default();
        if !cap.read(&mut frame)? || frame.empty() {
            break;
        }

        let current_decoded_index = decoded_index;
        decoded_index += 1;

        if current_decoded_index % config.sample_stride as u64 != 0 {
            continue;
        }

        if let Some(limit) = config.max_analyzed_frames {
            if analyzed_count >= limit {
                break;
            }
        }

        let mut gray = core::Mat::default();
        imgproc::cvt_color_def(&frame, &mut gray, imgproc::COLOR_BGR2GRAY)?;

        let gray_bytes = gray.data_bytes()?;
        if gray_bytes.is_empty() {
            continue;
        }

        let mut sum = 0.0_f64;
        let mut sum_sq = 0.0_f64;
        let mut min_luma = u8::MAX;
        let mut max_luma = u8::MIN;
        let mut strict_black_count = 0usize;
        let mut near_black_count = 0usize;

        for &v in gray_bytes {
            let vf = v as f64;
            sum += vf;
            sum_sq += vf * vf;
            min_luma = min_luma.min(v);
            max_luma = max_luma.max(v);
            if v <= config.black_luma_max {
                strict_black_count += 1;
            }
            if v <= config.near_black_luma_max {
                near_black_count += 1;
            }
        }

        let n = gray_bytes.len() as f64;
        let mean_luma = sum / n;
        let variance = (sum_sq / n - mean_luma * mean_luma).max(0.0);
        let stddev_luma = variance.sqrt();

        let frame_delta_mean = previous_gray.as_ref().map(|prev| {
            if prev.len() != gray_bytes.len() {
                return 0.0;
            }
            let total: u64 = prev.iter()
                .zip(gray_bytes.iter())
                .map(|(a, b)| a.abs_diff(*b) as u64)
                .sum();
            total as f64 / n
        }).unwrap_or(0.0);

        let bgr_mean = core::mean_def(&frame)?;
        let mut hsv = core::Mat::default();
        imgproc::cvt_color_def(&frame, &mut hsv, imgproc::COLOR_BGR2HSV)?;
        let hsv_mean = core::mean_def(&hsv)?;

        // CAP_PROP_POS_MSEC is a useful probe timestamp for CFR/VFR media, but
        // some OpenCV backends report 0. Fall back to frame_index/fps for the
        // validation harness when that happens.
        let pos_ms = cap.get(videoio::CAP_PROP_POS_MSEC).unwrap_or(0.0);
        let pts_s = if pos_ms > 0.0 {
            pos_ms / 1000.0
        } else if fps_reported > 0.0 {
            current_decoded_index as f64 / fps_reported
        } else {
            current_decoded_index as f64
        };

        frames.push(FrameMetrics {
            frame_index: current_decoded_index,
            pts_s,
            width: frame.cols(),
            height: frame.rows(),
            mean_luma,
            stddev_luma,
            min_luma,
            max_luma,
            black_pixel_ratio: strict_black_count as f64 / n,
            near_black_pixel_ratio: near_black_count as f64 / n,
            mean_blue: bgr_mean[0],
            mean_green: bgr_mean[1],
            mean_red: bgr_mean[2],
            mean_hue: hsv_mean[0],
            mean_saturation: hsv_mean[1],
            mean_value: hsv_mean[2],
            frame_delta_mean,
        });

        previous_gray = Some(gray_bytes.to_vec());
        analyzed_count += 1;

        // Keep UI traffic bounded while still feeling live on long recordings.
        if analyzed_count == 1 || last_progress_emit.elapsed() >= Duration::from_millis(500) {
            let elapsed_s = analysis_started.elapsed().as_secs_f64().max(0.001);
            let processing_fps = analyzed_count as f64 / elapsed_s;
            let eta_s = total_frames.and_then(|total| {
                if processing_fps > 0.0 && decoded_index < total {
                    Some((total - decoded_index) as f64 / processing_fps)
                } else {
                    None
                }
            });
            progress(CvFrameProgress {
                frames_decoded: decoded_index,
                frames_analyzed: analyzed_count,
                total_frames,
                source_position_s: if fps_reported > 0.0 { Some(decoded_index as f64 / fps_reported) } else { None },
                source_duration_s,
                elapsed_s,
                processing_fps,
                eta_s,
                scan_complete: false,
            });
            last_progress_emit = Instant::now();
        }
    }

    if should_cancel() {
        return Err(anyhow!("Job cancelled by user."));
    }

    // Emit an exact terminal frame update before source hashing/finalization.
    let elapsed_s = analysis_started.elapsed().as_secs_f64().max(0.001);
    let processing_fps = analyzed_count as f64 / elapsed_s;
    progress(CvFrameProgress {
        frames_decoded: decoded_index,
        frames_analyzed: analyzed_count,
        total_frames,
        source_position_s: if fps_reported > 0.0 { Some(decoded_index as f64 / fps_reported) } else { None },
        source_duration_s,
        elapsed_s,
        processing_fps,
        eta_s: None,
        scan_complete: true,
    });

    let source_file = input.file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| input.display().to_string());
    let source_size_bytes = fs::metadata(input)?.len();
    let source_sha256 = sha256_file_with_cancel(input, should_cancel)?;

    let manifest = CvAnalysisManifest {
        schema_version: 1,
        engine: "opencv-evidence-probe".to_string(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        source_file,
        source_path: input.display().to_string(),
        source_size_bytes,
        source_sha256,
        opencv_version: core::CV_VERSION.to_string(),
        fps_reported,
        width_reported,
        height_reported,
        frame_count_reported,
        frames_decoded: decoded_index,
        frames_analyzed: frames.len() as u64,
        first_pts_s: frames.first().map(|f| f.pts_s),
        last_pts_s: frames.last().map(|f| f.pts_s),
        config: config.clone(),
    };

    Ok(CvAnalysisResult { manifest, frames })
}

#[cfg(not(feature = "opencv-analysis"))]
pub fn analyze_video_with_opencv(_input: &Path, _config: &CvAnalysisConfig) -> Result<CvAnalysisResult> {
    Err(anyhow!(
        "OpenCV analysis is not enabled. Build with --features opencv-analysis"
    ))
}

#[cfg(not(feature = "opencv-analysis"))]
pub fn analyze_video_with_opencv_progress(
    _input: &Path,
    _config: &CvAnalysisConfig,
    _progress: &dyn Fn(CvFrameProgress),
) -> Result<CvAnalysisResult> {
    Err(anyhow!(
        "OpenCV analysis is not enabled. Build with --features opencv-analysis"
    ))
}

#[cfg(not(feature = "opencv-analysis"))]
pub fn analyze_video_with_opencv_progress_cancel(
    _input: &Path,
    _config: &CvAnalysisConfig,
    _progress: &dyn Fn(CvFrameProgress),
    _should_cancel: &dyn Fn() -> bool,
) -> Result<CvAnalysisResult> {
    Err(anyhow!(
        "OpenCV analysis is not enabled. Build with --features opencv-analysis"
    ))
}

pub fn read_frame_metrics_jsonl(path: &Path) -> Result<Vec<FrameMetrics>> {
    let file = fs::File::open(path)?;
    let reader = BufReader::new(file);
    let mut frames = Vec::new();
    for (line_no, line) in reader.lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let frame: FrameMetrics = serde_json::from_str(&line)
            .map_err(|e| anyhow!("Invalid frame metrics JSONL at line {}: {e}", line_no + 1))?;
        frames.push(frame);
    }
    Ok(frames)
}

pub fn write_cv_evidence(outdir: &Path, result: &CvAnalysisResult) -> Result<Vec<PathBuf>> {
    write_cv_evidence_with_cancel(outdir, result, &|| false)
}

pub fn write_cv_evidence_with_cancel(
    outdir: &Path,
    result: &CvAnalysisResult,
    should_cancel: &dyn Fn() -> bool,
) -> Result<Vec<PathBuf>> {
    fs::create_dir_all(outdir)?;
    if should_cancel() { return Err(anyhow!("Job cancelled by user.")); }

    let manifest_path = outdir.join("opencv_analysis_manifest.json");
    fs::write(&manifest_path, serde_json::to_string_pretty(&result.manifest)?)?;

    let jsonl_path = outdir.join("opencv_frame_metrics.jsonl");
    let mut jsonl = BufWriter::new(fs::File::create(&jsonl_path)?);
    for (idx, frame) in result.frames.iter().enumerate() {
        if idx % 512 == 0 && should_cancel() {
            return Err(anyhow!("Job cancelled by user."));
        }
        serde_json::to_writer(&mut jsonl, frame)?;
        writeln!(&mut jsonl)?;
    }
    jsonl.flush()?;

    if should_cancel() { return Err(anyhow!("Job cancelled by user.")); }
    let csv_path = outdir.join("opencv_frame_metrics.csv");
    let mut csv = BufWriter::new(fs::File::create(&csv_path)?);
    writeln!(csv,
        "frame_index,pts_s,width,height,mean_luma,stddev_luma,min_luma,max_luma,black_pixel_ratio,near_black_pixel_ratio,mean_blue,mean_green,mean_red,mean_hue,mean_saturation,mean_value,frame_delta_mean"
    )?;
    for (idx, f) in result.frames.iter().enumerate() {
        if idx % 512 == 0 && should_cancel() {
            return Err(anyhow!("Job cancelled by user."));
        }
        writeln!(csv,
            "{},{:.6},{},{},{:.6},{:.6},{},{},{:.8},{:.8},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6}",
            f.frame_index, f.pts_s, f.width, f.height,
            f.mean_luma, f.stddev_luma, f.min_luma, f.max_luma,
            f.black_pixel_ratio, f.near_black_pixel_ratio,
            f.mean_blue, f.mean_green, f.mean_red,
            f.mean_hue, f.mean_saturation, f.mean_value, f.frame_delta_mean
        )?;
    }
    csv.flush()?;

    if should_cancel() { return Err(anyhow!("Job cancelled by user.")); }
    Ok(vec![manifest_path, jsonl_path, csv_path])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_thresholds_are_ordered() {
        let c = CvAnalysisConfig::default();
        assert!(c.near_black_luma_max >= c.black_luma_max);
        assert!(c.sample_stride >= 1);
    }
}
