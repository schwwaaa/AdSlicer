use serde::{Deserialize, Serialize};

// ─── BlackSegment ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlackSegment {
    pub start: f64,
    pub end: f64,
    pub dur: f64,
}
impl BlackSegment {
    pub fn new(start: f64, end: f64) -> Self { Self { start, end, dur: (end-start).max(0.0) } }
    pub fn duration(&self) -> f64 { (self.end - self.start).max(0.0) }
}

// ─── SilenceSegment ───────────────────────────────────────────────────────────
// Comskip: max_silence / min_silence / validate_silence

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SilenceSegment {
    pub start: f64,
    pub end: f64,
    pub dur: f64,
    pub noise_db: f64,
}
impl SilenceSegment {
    pub fn new(start: f64, end: f64, noise_db: f64) -> Self {
        Self { start, end, dur: (end-start).max(0.0), noise_db }
    }
    pub fn duration(&self) -> f64 { (self.end - self.start).max(0.0) }
}

// ─── UniformSegment ───────────────────────────────────────────────────────────
// Comskip: non_uniformity / validate_uniform
// A run of frames where luma stddev ≤ threshold — solid colour slates,
// colour cards, and noisy-but-uniform black slugs.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UniformSegment {
    pub start: f64,
    pub end: f64,
    pub dur: f64,
    /// Average luma standard deviation across frames in this segment (0–255 scale).
    pub avg_stddev: f64,
}
impl UniformSegment {
    pub fn new(start: f64, end: f64, avg_stddev: f64) -> Self {
        Self { start, end, dur: (end-start).max(0.0), avg_stddev }
    }
    pub fn duration(&self) -> f64 { (self.end - self.start).max(0.0) }
}

// ─── SceneChange ─────────────────────────────────────────────────────────────
// Comskip: schange_percent / schange_rate / validate_scenechange
// Individual hard-cut frame timestamps, aggregated into per-block rates.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneChange {
    /// PTS timestamp in seconds of the frame where a scene cut was detected.
    pub pts: f64,
}
impl SceneChange {
    pub fn new(pts: f64) -> Self { Self { pts } }
}

// ─── CutInterval ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CutInterval {
    pub start: f64,
    pub end: f64,
    pub kind: String,
    pub dur: f64,
    pub confidence: f64,
    pub signals: Vec<String>,
}
impl CutInterval {
    pub fn new(start: f64, end: f64, kind: &str) -> Self {
        Self { start, end, kind: kind.to_string(), dur: (end-start).max(0.0),
               confidence: 1.0, signals: Vec::new() }
    }
    pub fn duration(&self) -> f64 { (self.end - self.start).max(0.0) }
    pub fn add_signal(&mut self, s: &str) -> &mut Self { self.signals.push(s.to_string()); self }
    pub fn set_confidence(&mut self, c: f64) -> &mut Self { self.confidence = c.clamp(0.0, 1.0); self }
}

// ─── Plan ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    pub duration: f64,
    pub include_black: bool,
    pub edge_pad_pre: f64,
    pub edge_pad_post: f64,
    pub min_commercial: f64,
    pub max_commercial: f64,
    pub min_show_segment: f64,
    pub always_keep_first: f64,
    pub always_keep_last: f64,
    pub silence_noise_db: f64,
    pub silence_min_dur: f64,
    // ── new fields ────────────────────────────────────────────────────────────
    /// Comskip: non_uniformity — luma stddev ceiling for uniform frame detection.
    pub uniform_max_stddev: f64,
    /// Comskip: schange_threshold — scene change sensitivity (0.0–1.0).
    pub scene_threshold: f64,
    /// Comskip: remove_before — seconds to trim from the content side of each cut.
    pub remove_before: f64,
    /// Comskip: remove_after — seconds to trim from the ad side of each cut.
    pub remove_after: f64,
    /// Comskip: require_div5 — snap commercial boundaries to nearest 30s.
    pub require_div5: bool,
    /// Recording trim: seconds to remove from the start of the assembled show output.
    /// Applied after keep assembly — shrinks the first keep segment's start.
    pub trim_head: f64,
    /// Recording trim: seconds to remove from the end of the assembled show output.
    /// Applied after keep assembly — shrinks the last keep segment's end.
    pub trim_tail: f64,
    // ── detection outputs ─────────────────────────────────────────────────────
    pub blacks: Vec<BlackSegment>,
    pub silences: Vec<SilenceSegment>,
    pub uniforms: Vec<UniformSegment>,
    pub scene_changes: Vec<SceneChange>,
    // ── decision outputs ──────────────────────────────────────────────────────
    pub commercials: Vec<CutInterval>,
    pub keeps: Vec<CutInterval>,
}

// ─── RunMeta ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunMeta {
    pub run_id: String,
    pub source_file: String,
    pub source_duration_s: f64,
    pub ffmpeg_version: String,
    pub app_version: String,
    pub param_black_min_dur: f64,
    pub param_pix_th: f64,
    pub param_pic_th: f64,
    pub param_merge_gap: f64,
    pub param_edge_pad_pre: f64,
    pub param_edge_pad_post: f64,
    pub param_min_commercial: f64,
    pub param_max_commercial: f64,
    pub param_include_black: bool,
    pub param_silence_noise_db: f64,
    pub param_silence_min_dur: f64,
    pub param_min_show_segment: f64,
    pub param_always_keep_first: f64,
    pub param_always_keep_last: f64,
    // new
    pub param_uniform_max_stddev: f64,
    pub param_scene_threshold: f64,
    pub param_remove_before: f64,
    pub param_remove_after: f64,
    pub param_require_div5: bool,
    pub param_trim_head: f64,
    pub param_trim_tail: f64,
    // summary stats
    pub blacks_raw_count: usize,
    pub blacks_filtered_count: usize,
    pub silences_count: usize,
    pub uniforms_count: usize,
    pub scene_changes_count: usize,
    pub avg_scene_rate: f64,
    pub total_commercial_s: f64,
    pub total_keep_s: f64,
    pub commercial_ratio: f64,
    pub commercial_count: usize,
    pub keep_count: usize,
}

// ─── DatasetRecord ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetRecord {
    pub run_id: String,
    pub source_file: String,
    pub segment_index: usize,
    pub timeline_position: usize,
    pub start_s: f64,
    pub end_s: f64,
    pub dur_s: f64,
    pub start_norm: f64,
    pub end_norm: f64,
    pub dur_norm: f64,
    pub offset_from_start_s: f64,
    pub offset_from_end_s: f64,
    pub black_left_dur_s: f64,
    pub black_right_dur_s: f64,
    pub black_overlap_count: usize,
    pub black_overlap_total_s: f64,
    pub silence_overlap_count: usize,
    pub silence_overlap_total_s: f64,
    pub silence_coverage: f64,
    pub has_silence_overlap: f64,
    // new uniform
    pub uniform_overlap_count: usize,
    pub uniform_overlap_total_s: f64,
    pub uniform_coverage: f64,
    pub has_uniform_overlap: f64,
    // new scene change
    pub scene_change_count: usize,
    pub scene_change_rate: f64,
    pub scene_change_rate_vs_avg: f64,
    // signal indicators
    pub sig_black_boundary: f64,
    pub sig_within_commercial_range: f64,
    pub sig_silence_overlap: f64,
    pub sig_uniform_overlap: f64,
    pub sig_high_scene_rate: f64,
    pub sig_demoted_min_show_segment: f64,
    pub sig_always_keep_first: f64,
    pub sig_always_keep_last: f64,
    pub sig_content_between_commercials: f64,
    pub sig_content_after_last_commercial: f64,
    pub sig_div5_snapped: f64,
    pub sig_trim_head: f64,
    pub sig_trim_tail: f64,
    // classification
    pub label: String,
    pub label_int: u8,
    pub confidence: f64,
    // run context (denormalised)
    pub source_duration_s: f64,
    pub ffmpeg_version: String,
    pub app_version: String,
    pub param_black_min_dur: f64,
    pub param_pix_th: f64,
    pub param_pic_th: f64,
    pub param_merge_gap: f64,
    pub param_edge_pad_pre: f64,
    pub param_edge_pad_post: f64,
    pub param_min_commercial: f64,
    pub param_max_commercial: f64,
    pub param_include_black: bool,
    pub param_silence_noise_db: f64,
    pub param_silence_min_dur: f64,
    pub param_min_show_segment: f64,
    pub param_always_keep_first: f64,
    pub param_always_keep_last: f64,
    pub param_uniform_max_stddev: f64,
    pub param_scene_threshold: f64,
    pub param_remove_before: f64,
    pub param_remove_after: f64,
    pub param_require_div5: bool,
    pub param_trim_head: f64,
    pub param_trim_tail: f64,
    pub run_blacks_raw_count: usize,
    pub run_blacks_filtered_count: usize,
    pub run_silences_count: usize,
    pub run_uniforms_count: usize,
    pub run_scene_changes_count: usize,
    pub run_avg_scene_rate: f64,
    pub run_commercial_count: usize,
    pub run_keep_count: usize,
    pub run_total_commercial_s: f64,
    pub run_total_keep_s: f64,
    pub run_commercial_ratio: f64,
}

// ─── EncodeSettings ───────────────────────────────────────────────────────────
//
// Replaces the old binary `reencode: bool`.  The frontend sends this as a nested
// object; job.rs flattens it into ffmpeg arguments via build_encode_args().

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EncodeSettings {
    /// "copy" | "h264" | "h265" | "av1" | "prores"
    /// copy = stream copy (fast, keyframe-snap).  Others force re-encode.
    #[serde(default = "default_encode_mode")]
    pub mode: String,

    /// CRF quality value (lower = higher quality).
    /// h264 default: 18.  h265 default: 24.  av1 default: 30.
    #[serde(default = "default_video_crf")]
    pub video_crf: u8,

    /// ffmpeg preset string for x264/x265/av1.
    /// e.g. "veryfast", "medium", "slow"
    #[serde(default = "default_video_preset")]
    pub video_preset: String,

    /// "aac" | "ac3" | "copy" — audio codec for the output.
    #[serde(default = "default_audio_codec")]
    pub audio_codec: String,

    /// Audio bitrate in kbps (e.g. 192, 256, 320).  0 = let ffmpeg choose.
    #[serde(default)]
    pub audio_bitrate_kbps: u32,

    /// GPU hardware encoder to use.
    /// "none" | "nvenc" (NVIDIA) | "qsv" (Intel QuickSync) | "amf" (AMD)
    #[serde(default = "default_gpu_accel")]
    pub gpu_accel: String,

    /// Apply yadif deinterlace filter before encoding.
    #[serde(default)]
    pub deinterlace: bool,

    /// Scale output width in pixels (maintaining aspect ratio).
    /// 0 = no scaling.  e.g. 1920, 1280, 720.
    #[serde(default)]
    pub scale_width: u32,

    /// Apply EBU R128 loudnorm audio normalisation to the show output.
    /// Ignored for commercials.  Forces re-encode of audio even in copy mode.
    #[serde(default)]
    pub loudnorm: bool,
}

fn default_encode_mode()    -> String { "copy".to_string() }
fn default_video_crf()      -> u8     { 18 }
fn default_video_preset()   -> String { "veryfast".to_string() }
fn default_audio_codec()    -> String { "aac".to_string() }
fn default_gpu_accel()      -> String { "none".to_string() }

impl Default for EncodeSettings {
    fn default() -> Self {
        Self {
            mode:             default_encode_mode(),
            video_crf:        default_video_crf(),
            video_preset:     default_video_preset(),
            audio_codec:      default_audio_codec(),
            audio_bitrate_kbps: 0,
            gpu_accel:        default_gpu_accel(),
            deinterlace:      false,
            scale_width:      0,
            loudnorm:         false,
        }
    }
}
