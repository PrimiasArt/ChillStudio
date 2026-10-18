// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 PrimiasArt and ChillStudio contributors

//! Smart editing helpers for Chill Studio (NKI Edition):
//!
//! 1. **Word-by-Word / TikTok Viral Captions** ([`split_into_word_groups`],
//!    [`word_caption_commands`]).
//! 2. **Smart Cut 1-Click** — Auto Silence Removal ([`find_silent_spans`],
//!    [`remove_silence_commands`]) and Auto Beat Sync ([`find_beat_times`],
//!    [`beat_split_commands`]).
//! 3. **Pro Subtitle & Reframe Workflow** — `.srt` / `.vtt` subtitle export
//!    ([`export_srt`], [`export_vtt`]) and 1-Click Smart Vertical 9:16 Reframe
//!    ([`smart_reframe_commands`]).
//! 4. **NKI AI Copilot** — Natural-language Vietnamese & English prompt parser
//!    and command generator ([`CopilotAction`], [`parse_copilot_prompt`],
//!    [`copilot_timeline_commands`]).

use crate::commands::{ClipMove, Command, TrimEdge};
use crate::model::{ClipKind, Project, TextStyle, Timeline, VideoSettings, ranges};

// ============================================================================
// Group 1: Word-by-Word / Short-Phrase TikTok Captions
// ============================================================================

/// Splits a sentence or transcript segment into punchy short word groups
/// (e.g. 1–3 words per pop-in title) suitable for TikTok / Reels / Shorts.
pub fn split_into_word_groups(text: &str, max_words_per_group: usize) -> Vec<String> {
    let limit = max_words_per_group.clamp(1, 10);
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return Vec::new();
    }
    let mut groups = Vec::new();
    let mut current = Vec::with_capacity(limit);
    for word in words {
        current.push(word);
        let ends_clause = word.ends_with(['.', ',', '!', '?', ';', ':', '。', '！', '？']);
        if current.len() >= limit || ends_clause {
            groups.push(current.join(" "));
            current.clear();
        }
    }
    if !current.is_empty() {
        groups.push(current.join(" "));
    }
    groups
}

/// Default TikTok Viral pop-in caption style when no custom style is provided.
pub fn tiktok_caption_style(content: String, font_size: f64) -> TextStyle {
    TextStyle {
        content,
        font_family: "Hanken Grotesk".to_owned(),
        font_size,
        font_weight: 800.0,
        color: "#ffee00".to_owned(),
        stroke_width: 0.008,
        stroke_color: "#000000".to_owned(),
        shadow: true,
        ..TextStyle::default()
    }
}

/// Converts timed transcript segments `(start_sec, end_sec, text)` into
/// word-by-word or short-phrase `Command::AddTextClip` commands on a single
/// shared caption track, subdividing each segment's duration proportionally
/// across its word groups.
pub fn word_caption_commands(
    segments: &[(f64, f64, String)],
    clip_start: f64,
    clip_speed: f64,
    max_words_per_group: usize,
    offset_y: f64,
    font_size: f64,
    base_style: Option<&TextStyle>,
) -> Vec<Command> {
    let speed = clip_speed.max(ranges::MIN_SPEED);
    let mut commands = Vec::new();
    for (seg_start, seg_end, text) in segments {
        let groups = split_into_word_groups(text, max_words_per_group);
        if groups.is_empty() {
            continue;
        }
        let timeline_start = clip_start + (*seg_start / speed);
        let total_duration = ((*seg_end - *seg_start) / speed).max(0.2);
        let total_chars: usize = groups.iter().map(|g| g.chars().count().max(1)).sum();
        let mut cursor = timeline_start;
        for (idx, group) in groups.iter().enumerate() {
            let weight = group.chars().count().max(1) as f64 / total_chars.max(1) as f64;
            let dur = if idx + 1 == groups.len() {
                (timeline_start + total_duration - cursor).max(ranges::MIN_CLIP_DURATION)
            } else {
                (total_duration * weight).max(0.12)
            };
            let style = match base_style {
                Some(base) => TextStyle {
                    content: group.clone(),
                    font_size,
                    max_width: 0.0,
                    max_height: 0.0,
                    ..base.clone()
                },
                None => tiktok_caption_style(group.clone(), font_size),
            };
            commands.push(Command::AddTextClip {
                track_id: None,
                above: true,
                start: cursor,
                style: Some(style),
                duration: Some(dur),
                offset_y: Some(offset_y),
            });
            cursor += dur;
        }
    }
    commands
}

// ============================================================================
// Group 2: Smart Cut 1-Click — Auto Silence Removal & Auto Beat Sync
// ============================================================================

/// Scans interleaved `f32` PCM audio samples and returns silent intervals
/// `(start_sec, end_sec)` in source seconds where the RMS level stays below
/// `threshold_db` (e.g. `-38.0` dB) for at least `min_silence_secs`.
///
/// `padding_secs` is preserved inside both edges of each silent span so spoken
/// words are never clipped abruptly.
pub fn find_silent_spans(
    samples: &[f32],
    sample_rate: u32,
    channels: u32,
    threshold_db: f32,
    min_silence_secs: f64,
    padding_secs: f64,
) -> Vec<(f64, f64)> {
    if samples.is_empty() || sample_rate == 0 || channels == 0 {
        return Vec::new();
    }
    let ch = channels as usize;
    let total_frames = samples.len() / ch;
    if total_frames == 0 {
        return Vec::new();
    }
    // 25 ms analysis window.
    let window_frames = ((sample_rate as usize) / 40).max(1);
    let linear_threshold = 10.0_f32.powf(threshold_db / 20.0);
    let min_dur = min_silence_secs.max(0.1);
    let pad = padding_secs.clamp(0.0, min_dur * 0.45);

    let mut spans = Vec::new();
    let mut silent_start: Option<f64> = None;

    let mut frame = 0;
    while frame < total_frames {
        let end_frame = (frame + window_frames).min(total_frames);
        let count = (end_frame - frame) * ch;
        let slice = &samples[frame * ch..end_frame * ch];
        let sum_sq: f32 = slice.iter().map(|&s| s * s).sum();
        let rms = (sum_sq / count.max(1) as f32).sqrt();
        let t_start = frame as f64 / sample_rate as f64;
        let t_end = end_frame as f64 / sample_rate as f64;

        if rms < linear_threshold {
            if silent_start.is_none() {
                silent_start = Some(t_start);
            }
        } else if let Some(s0) = silent_start.take() {
            if t_start - s0 >= min_dur {
                let padded_start = s0 + pad;
                let padded_end = t_start - pad;
                if padded_end - padded_start >= ranges::MIN_CLIP_DURATION * 2.0 {
                    spans.push((padded_start, padded_end));
                }
            }
        }
        let _ = t_end;
        frame = end_frame;
    }

    if let Some(s0) = silent_start {
        let total_secs = total_frames as f64 / sample_rate as f64;
        if total_secs - s0 >= min_dur {
            let padded_start = s0 + pad;
            let padded_end = total_secs - pad;
            if padded_end - padded_start >= ranges::MIN_CLIP_DURATION * 2.0 {
                spans.push((padded_start, padded_end));
            }
        }
    }

    spans
}

/// Builds a reverse-order sequence of [`Command::SplitClips`] and ripple
/// [`Command::TrimClip`] commands that excise `silent_spans` (given in
/// clip-local source seconds relative to `0..clip_duration * clip_speed`) from
/// `clip_id` without changing its head ID before earlier cuts run.
///
/// Because `Command::SplitClips` preserves `clip_id` on the head piece and
/// mints a fresh ID for the tail piece, processing silent intervals from
/// **latest to earliest** guarantees that `clip_id` always covers every
/// remaining cut!
pub fn remove_silence_commands(
    clip_id: &str,
    clip_start: f64,
    clip_duration: f64,
    clip_speed: f64,
    silent_spans: &[(f64, f64)],
) -> Vec<Command> {
    let speed = clip_speed.max(ranges::MIN_SPEED);
    let clip_end = clip_start + clip_duration;
    let margin = ranges::MIN_CLIP_DURATION * 1.5;

    // Convert source-relative silent spans into timeline coordinates clamped
    // inside (clip_start + margin .. clip_end - margin).
    let mut timeline_spans: Vec<(f64, f64)> = silent_spans
        .iter()
        .filter_map(|&(s0, s1)| {
            let t0 = (clip_start + s0 / speed).max(clip_start + margin);
            let t1 = (clip_start + s1 / speed).min(clip_end - margin);
            (t1 - t0 >= margin).then_some((t0, t1))
        })
        .collect();

    timeline_spans.sort_by(|a, b| b.0.total_cmp(&a.0));

    let mut commands = Vec::new();
    for (t0, t1) in timeline_spans {
        // 1. Split at the end of the silent span (`t1`). `clip_id` remains the
        //    head (`clip_start..t1`), and the tail (`t1..`) gets a new ID.
        commands.push(Command::SplitClips {
            clip_ids: vec![clip_id.to_owned()],
            time: t1,
        });
        // 2. Ripple-trim the end of `clip_id` back from `t1` to `t0`, closing
        //    the silent gap and pulling the newly split tail left by `t1 - t0`.
        commands.push(Command::TrimClip {
            clip_id: clip_id.to_owned(),
            edge: TrimEdge::End,
            delta: -(t1 - t0),
            ripple: true,
        });
    }
    commands
}

/// Detects musical beat / percussive onset times (in source seconds) from
/// interleaved `f32` PCM audio samples using positive spectral/energy flux
/// peaks above a local moving average.
pub fn find_beat_times(
    samples: &[f32],
    sample_rate: u32,
    channels: u32,
    min_interval_secs: f64,
    sensitivity: f32,
) -> Vec<f64> {
    if samples.is_empty() || sample_rate == 0 || channels == 0 {
        return Vec::new();
    }
    let ch = channels as usize;
    let total_frames = samples.len() / ch;
    // 20 ms hop size.
    let hop_frames = ((sample_rate as usize) / 50).max(1);
    if total_frames < hop_frames * 4 {
        return Vec::new();
    }

    let mut energies = Vec::with_capacity(total_frames / hop_frames + 1);
    let mut frame = 0;
    while frame < total_frames {
        let end = (frame + hop_frames).min(total_frames);
        let slice = &samples[frame * ch..end * ch];
        let e: f32 = slice.iter().map(|&s| s * s).sum::<f32>() / (slice.len().max(1) as f32);
        energies.push(e.sqrt());
        frame = end;
    }

    // Positive first-order energy flux (onset strength).
    let mut flux = vec![0.0_f32; energies.len()];
    for i in 1..energies.len() {
        flux[i] = (energies[i] - energies[i - 1]).max(0.0);
    }

    let sens = sensitivity.clamp(0.5, 4.0);
    let min_gap = min_interval_secs.max(0.12);
    let local_radius = 12usize; // ~240ms each side
    let mut beats = Vec::new();
    let mut last_beat = -min_gap;

    for i in 1..flux.len().saturating_sub(1) {
        if flux[i] <= flux[i - 1] || flux[i] < flux[i + 1] {
            continue;
        }
        let lo = i.saturating_sub(local_radius);
        let hi = (i + local_radius + 1).min(flux.len());
        let mean: f32 = flux[lo..hi].iter().sum::<f32>() / ((hi - lo).max(1) as f32);
        // Require peak to stand well above local mean and above noise floor.
        if flux[i] > mean * (1.8 / sens) && flux[i] > 0.015 {
            let t = (i * hop_frames) as f64 / sample_rate as f64;
            if t - last_beat >= min_gap {
                beats.push(t);
                last_beat = t;
            }
        }
    }
    beats
}

/// Generates reverse-order [`Command::SplitClips`] commands at each beat
/// timestamp so a clip (or set of clips) is sliced on the beat in one undoable
/// batch.
pub fn beat_split_commands(
    clip_ids: &[String],
    clip_start: f64,
    clip_duration: f64,
    clip_speed: f64,
    beat_times_source: &[f64],
) -> Vec<Command> {
    if clip_ids.is_empty() {
        return Vec::new();
    }
    let speed = clip_speed.max(ranges::MIN_SPEED);
    let clip_end = clip_start + clip_duration;
    let margin = ranges::MIN_CLIP_DURATION * 1.5;

    let mut times: Vec<f64> = beat_times_source
        .iter()
        .map(|&b| clip_start + b / speed)
        .filter(|&t| t > clip_start + margin && t < clip_end - margin)
        .collect();
    times.sort_by(|a, b| b.total_cmp(a));

    times
        .into_iter()
        .map(|time| Command::SplitClips {
            clip_ids: clip_ids.to_vec(),
            time,
        })
        .collect()
}

// ============================================================================
// Group 4: Pro Workflow — SRT / VTT Subtitle Export & Smart 9:16 Reframe
// ============================================================================

/// Formats a timestamp in seconds as `HH:MM:SS,mmm` (for SubRip `.srt`) or
/// `HH:MM:SS.mmm` (for WebVTT `.vtt`).
fn format_subtitle_timestamp(seconds: f64, comma_separator: bool) -> String {
    let total_ms = (seconds.max(0.0) * 1000.0).round() as u64;
    let ms = total_ms % 1000;
    let total_secs = total_ms / 1000;
    let s = total_secs % 60;
    let m = (total_secs / 60) % 60;
    let h = total_secs / 3600;
    let sep = if comma_separator { ',' } else { '.' };
    format!("{h:02}:{m:02}:{s:02}{sep}{ms:03}")
}

/// Collects all visible text clips on `timeline` in chronological order.
fn ordered_subtitle_cues(timeline: &Timeline) -> Vec<(f64, f64, String)> {
    let hidden_tracks: std::collections::HashSet<&str> = timeline
        .tracks
        .iter()
        .filter(|t| !t.visible)
        .map(|t| t.id.as_str())
        .collect();

    let mut cues: Vec<(f64, f64, String)> = timeline
        .clips
        .iter()
        .filter(|clip| {
            clip.kind == ClipKind::Text && !hidden_tracks.contains(clip.track_id.as_str())
        })
        .filter_map(|clip| {
            let text = clip.text.as_ref()?.content.trim();
            (!text.is_empty()).then(|| (clip.start, clip.start + clip.duration, text.to_owned()))
        })
        .collect();

    cues.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.total_cmp(&b.1)));
    cues
}

/// Exports all visible text clips on `timeline` as a SubRip (`.srt`) document.
pub fn export_srt(timeline: &Timeline) -> String {
    let cues = ordered_subtitle_cues(timeline);
    let mut out = String::new();
    for (idx, (start, end, text)) in cues.into_iter().enumerate() {
        let index = idx + 1;
        let t0 = format_subtitle_timestamp(start, true);
        let t1 = format_subtitle_timestamp(end, true);
        out.push_str(&format!("{index}\n{t0} --> {t1}\n{text}\n\n"));
    }
    out
}

/// Exports all visible text clips on `timeline` as a WebVTT (`.vtt`) document.
pub fn export_vtt(timeline: &Timeline) -> String {
    let cues = ordered_subtitle_cues(timeline);
    let mut out = String::from("WEBVTT\n\n");
    for (idx, (start, end, text)) in cues.into_iter().enumerate() {
        let index = idx + 1;
        let t0 = format_subtitle_timestamp(start, false);
        let t1 = format_subtitle_timestamp(end, false);
        out.push_str(&format!("{index}\n{t0} --> {t1}\n{text}\n\n"));
    }
    out
}

/// Generates commands to reframe the active timeline to `(target_width, target_height)`
/// (e.g. `1080x1920` for 9:16 TikTok/Reels/Shorts, `1080x1080` for 1:1, or
/// `1920x1080` for 16:9 YouTube) and automatically scales visual clips so they
/// fill the new frame without letterbox/pillarbox black bars.
pub fn smart_reframe_commands(
    project: &Project,
    target_width: u32,
    target_height: u32,
) -> Vec<Command> {
    if target_width == 0 || target_height == 0 {
        return Vec::new();
    }
    let timeline = project.active();
    let mut commands = Vec::new();

    let new_video = VideoSettings {
        width: target_width,
        height: target_height,
        ..timeline.video
    };
    commands.push(Command::SetTimelineVideo {
        timeline_id: timeline.id.clone(),
        video: new_video,
    });

    let target_aspect = target_width as f64 / target_height as f64;
    for clip in &timeline.clips {
        if !clip.kind.is_visual() {
            continue;
        }
        // Determine source aspect ratio from media bin (fallback to 16:9 if unknown).
        let source_aspect = project
            .media_by_id(&clip.media_id)
            .and_then(|m| Some(m.width? as f64 / m.height?.max(1) as f64))
            .unwrap_or(16.0 / 9.0);

        // Because the engine fits a clip inside the frame at scale = 1.0, the
        // multiplier needed to cover ("fill") the target aspect without black
        // bars is max(source_aspect / target_aspect, target_aspect / source_aspect).
        let fill_scale = (source_aspect / target_aspect)
            .max(target_aspect / source_aspect)
            .clamp(ranges::MIN_SCALE, ranges::MAX_SCALE);

        commands.push(Command::SetClipTransform {
            clip_id: clip.id.clone(),
            scale: Some(fill_scale),
            offset_x: Some(0.0),
            offset_y: Some(0.0),
            rotation: None,
            stretch_x: None,
            stretch_y: None,
        });
    }

    commands
}

// ============================================================================
// Group 5: NKI AI Copilot — Vietnamese & English Natural Language Commands
// ============================================================================

/// High-level action parsed from a natural-language Vietnamese or English
/// NKI Copilot command.
#[derive(Clone, PartialEq, Debug)]
pub enum CopilotAction {
    /// Reframe the active timeline and auto-fill visual clips (e.g. 9:16, 1:1, 16:9).
    SmartReframe { width: u32, height: u32 },
    /// Change playback speed on all media clips in the active timeline.
    SetSpeedAll { speed: f64 },
    /// Close gaps between sequential clips on every track.
    RemoveGaps,
    /// Auto-cut silent segments from audio/video clips.
    RemoveSilence {
        threshold_db: f32,
        min_silence_secs: f64,
    },
    /// Split clips on detected musical beats.
    BeatSync { sensitivity: f32 },
    /// Add TikTok pop-in word-by-word captions starting at the playhead.
    AddWordCaptions { text: String, max_words: usize },
}

/// Parses a natural-language Vietnamese or English command (with or without a
/// leading `/`) into a structured [`CopilotAction`].
pub fn parse_copilot_prompt(prompt: &str) -> Option<CopilotAction> {
    let trimmed = prompt.trim().trim_start_matches('/').trim();
    if trimmed.is_empty() {
        return None;
    }
    let lower = trimmed.to_lowercase();

    // 1. Word-by-word / TikTok caption prefix:
    //    "[tiktok] ...", "[word] ...", "tiktok: ...", "phụ đề tiktok: ..."
    for prefix in [
        "[tiktok]",
        "[word]",
        "tiktok:",
        "phụ đề tiktok:",
        "phu de tiktok:",
        "caption tiktok:",
    ] {
        if lower.starts_with(prefix) {
            let content = trimmed[prefix.len()..].trim();
            if !content.is_empty() {
                return Some(CopilotAction::AddWordCaptions {
                    text: content.to_owned(),
                    max_words: 3,
                });
            }
        }
    }

    // 2. Smart Reframe (9:16 vertical, 1:1 square, 16:9 landscape):
    if lower.contains("9:16")
        || lower.contains("khung dọc")
        || lower.contains("video dọc")
        || lower.contains("chuyển sang dọc")
        || lower.contains("reframe vertical")
        || lower.contains("shorts")
        || lower.contains("reels")
    {
        return Some(CopilotAction::SmartReframe {
            width: 1080,
            height: 1920,
        });
    }
    if lower.contains("1:1") || lower.contains("khung vuông") || lower.contains("video vuông")
    {
        return Some(CopilotAction::SmartReframe {
            width: 1080,
            height: 1080,
        });
    }
    if lower.contains("16:9") || lower.contains("khung ngang") || lower.contains("video ngang")
    {
        return Some(CopilotAction::SmartReframe {
            width: 1920,
            height: 1080,
        });
    }

    // 3. Auto Silence Removal:
    if lower.contains("khoảng lặng")
        || lower.contains("khoang lang")
        || lower.contains("im lặng")
        || lower.contains("silence")
        || lower.contains("cắt đoạn im")
    {
        return Some(CopilotAction::RemoveSilence {
            threshold_db: -38.0,
            min_silence_secs: 0.4,
        });
    }

    // 4. Auto Beat Sync:
    if lower.contains("beat")
        || lower.contains("nhịp nhạc")
        || lower.contains("nhip nhac")
        || lower.contains("theo nhịp")
    {
        return Some(CopilotAction::BeatSync { sensitivity: 1.3 });
    }

    // 5. Remove timeline gaps:
    if lower.contains("khoảng trống")
        || lower.contains("khoang trong")
        || lower.contains("dồn clip")
        || lower.contains("close gap")
        || lower.contains("remove gap")
    {
        return Some(CopilotAction::RemoveGaps);
    }

    // 6. Speed changes ("tăng tốc 1.5x", "speed 2x", "chậm 0.5x"):
    if lower.contains("tăng tốc")
        || lower.contains("tang toc")
        || lower.contains("chậm")
        || lower.contains("tốc độ")
        || lower.contains("toc do")
        || lower.contains("speed")
    {
        let speed = extract_number(&lower).unwrap_or_else(|| {
            if lower.contains("chậm") || lower.contains("slow") {
                0.5
            } else {
                1.5
            }
        });
        return Some(CopilotAction::SetSpeedAll {
            speed: speed.clamp(ranges::MIN_SPEED, ranges::MAX_SPEED),
        });
    }

    None
}

fn extract_number(text: &str) -> Option<f64> {
    for token in text.split(|c: char| !c.is_ascii_digit() && c != '.') {
        let cleaned = token.trim_matches('.');
        if !cleaned.is_empty()
            && let Ok(val) = cleaned.parse::<f64>()
            && val > 0.0
        {
            return Some(val);
        }
    }
    None
}

/// Generates pure timeline commands for [`CopilotAction`] variants that do not
/// require decoding external media files (`SmartReframe`, `SetSpeedAll`,
/// `RemoveGaps`, `AddWordCaptions`).
pub fn copilot_timeline_commands(
    project: &Project,
    action: &CopilotAction,
    playhead: f64,
) -> Vec<Command> {
    let timeline = project.active();
    match action {
        CopilotAction::SmartReframe { width, height } => {
            smart_reframe_commands(project, *width, *height)
        }
        CopilotAction::SetSpeedAll { speed } => timeline
            .clips
            .iter()
            .filter(|c| matches!(c.kind, ClipKind::Video | ClipKind::Audio))
            .map(|c| Command::SetClipSpeed {
                clip_id: c.id.clone(),
                speed: *speed,
            })
            .collect(),
        CopilotAction::RemoveGaps => {
            let mut moves = Vec::new();
            for track in &timeline.tracks {
                let mut track_clips: Vec<_> = timeline
                    .clips
                    .iter()
                    .filter(|c| c.track_id == track.id)
                    .collect();
                track_clips.sort_by(|a, b| a.start.total_cmp(&b.start));
                let mut cursor = 0.0;
                for clip in track_clips {
                    if (clip.start - cursor).abs() > 1e-4 {
                        moves.push(ClipMove {
                            clip_id: clip.id.clone(),
                            track_id: track.id.clone(),
                            start: cursor,
                        });
                    }
                    cursor += clip.duration;
                }
            }
            if moves.is_empty() {
                Vec::new()
            } else {
                vec![Command::MoveClips { moves }]
            }
        }
        CopilotAction::AddWordCaptions { text, max_words } => {
            let groups = split_into_word_groups(text, *max_words);
            let duration_total = (text.chars().count() as f64 / 14.0).clamp(1.2, 30.0);
            let segs = vec![(0.0, duration_total, text.clone())];
            let _ = groups;
            word_caption_commands(&segs, playhead.max(0.0), 1.0, *max_words, 0.28, 0.065, None)
        }
        CopilotAction::RemoveSilence { .. } | CopilotAction::BeatSync { .. } => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::Editor;
    use crate::model::{Clip, Track};

    #[test]
    fn word_groups_split_cleanly_for_tiktok_captions() {
        let groups = split_into_word_groups(
            "Xin chào các bạn, chào mừng đến với Chill Studio NKI!",
            3,
        );
        assert_eq!(
            groups,
            vec![
                "Xin chào các",
                "bạn,",
                "chào mừng đến",
                "với Chill Studio",
                "NKI!",
            ]
        );
        let cmds = word_caption_commands(
            &[(0.0, 3.0, "Chào mừng bạn đến Chill Studio".to_owned())],
            1.0,
            1.0,
            2,
            0.3,
            0.06,
            None,
        );
        assert_eq!(cmds.len(), 3);
    }

    #[test]
    fn silence_detection_and_reverse_ripple_cut_preserve_head_id() {
        let sr = 1000;
        let mut pcm = vec![0.5_f32; 1000]; // 0.0..1.0s loud
        pcm.extend(vec![0.0_f32; 1000]); // 1.0..2.0s silent
        pcm.extend(vec![0.5_f32; 1000]); // 2.0..3.0s loud

        let spans = find_silent_spans(&pcm, sr, 1, -30.0, 0.4, 0.05);
        assert_eq!(spans.len(), 1);
        assert!(spans[0].0 >= 1.04 && spans[0].1 <= 1.96);

        let mut editor = Editor::new();
        let tl = editor.project_mut().active_mut();
        tl.clips.push(std::sync::Arc::new(Clip::blank(
            "c1",
            "T1",
            ClipKind::Video,
            "talk.mp4",
            0.0,
            3.0,
        )));

        let cmds = remove_silence_commands("c1", 0.0, 3.0, 1.0, &[(1.0, 2.0)]);
        let outcome = editor.apply(Command::Batch { commands: cmds }).unwrap();
        assert!(outcome.applied);
        let clips = &editor.project().active().clips;
        assert_eq!(clips.len(), 2);
        // Total duration shrunk from 3.0s to 2.0s with no gap between the two pieces!
        assert!((editor.project().active().duration() - 2.0).abs() < 1e-6);
    }

    #[test]
    fn beat_detection_finds_periodic_impulses() {
        let sr = 1000;
        let mut pcm = vec![0.001_f32; 3000];
        for beat_ms in [500usize, 1200, 1900, 2600] {
            for sample in &mut pcm[beat_ms..beat_ms + 25] {
                *sample = 0.9;
            }
        }
        let beats = find_beat_times(&pcm, sr, 1, 0.3, 1.5);
        assert_eq!(beats.len(), 4);
    }

    #[test]
    fn srt_and_vtt_export_format_text_clips() {
        let mut timeline = Timeline {
            id: "TL1".to_owned(),
            name: "Main".to_owned(),
            video: VideoSettings::default(),
            tracks: vec![Track {
                id: "T1".to_owned(),
                visible: true,
                muted: false,
                extra: Default::default(),
            }],
            clips: Vec::new(),
            extra: Default::default(),
        };
        let mut c1 = Clip::blank("c1", "T1", ClipKind::Text, "Xin chào", 1.5, 2.25);
        c1.text = Some(TextStyle {
            content: "Xin chào NKI".to_owned(),
            ..TextStyle::default()
        });
        timeline.clips.push(std::sync::Arc::new(c1));

        let srt = export_srt(&timeline);
        assert!(srt.contains("1\n00:00:01,500 --> 00:00:03,750\nXin chào NKI"));

        let vtt = export_vtt(&timeline);
        assert!(vtt.starts_with("WEBVTT\n\n"));
        assert!(vtt.contains("00:00:01.500 --> 00:00:03.750"));
    }

    #[test]
    fn copilot_parses_vietnamese_and_english_commands() {
        assert_eq!(
            parse_copilot_prompt("/chuyển sang khung dọc 9:16"),
            Some(CopilotAction::SmartReframe {
                width: 1080,
                height: 1920
            })
        );
        assert_eq!(
            parse_copilot_prompt("xóa khoảng lặng"),
            Some(CopilotAction::RemoveSilence {
                threshold_db: -38.0,
                min_silence_secs: 0.4
            })
        );
        assert_eq!(
            parse_copilot_prompt("tăng tốc 1.5x"),
            Some(CopilotAction::SetSpeedAll { speed: 1.5 })
        );
        assert_eq!(
            parse_copilot_prompt("[tiktok] Chill Studio đỉnh quá"),
            Some(CopilotAction::AddWordCaptions {
                text: "Chill Studio đỉnh quá".to_owned(),
                max_words: 3
            })
        );
    }
}
