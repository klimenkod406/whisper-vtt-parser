//! Parse the output of `whisper.cpp` / `whisper-cli` into timed segments.
//!
//! `whisper.cpp` emits cues in two different shapes depending on how it is
//! invoked, and a parser that only understands one of them silently produces
//! **zero** segments — the classic "it ran but nothing happened" bug:
//!
//! 1. **The `.vtt` file** written by `-ovtt`:
//!    ```text
//!    00:00:00.000 --> 00:00:02.100
//!    the cue text lives here
//!    ```
//!    The timestamp is alone on its line; the text follows on the next lines.
//!
//! 2. **The stdout progress format** printed by `whisper-cli`:
//!    ```text
//!    [00:00:00.000 --> 00:00:02.100]   the cue text lives here
//!    ```
//!    The text is inline, after the closing bracket.
//!
//! This parser handles **both** and produces identical segments from either,
//! so the caller does not have to care which one it captured.
//!
//! Everything here is pure and unit-tested.

use serde::{Deserialize, Serialize};

/// One timed piece of transcribed speech.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TranscriptionSegment {
    /// Start offset in seconds, relative to the beginning of the audio.
    pub start: f32,
    /// End offset in seconds.
    pub end: f32,
    /// The recognised text for this segment.
    pub text: String,
    /// Speaker label, when diarization is available.
    pub speaker: Option<String>,
    /// Confidence reported by the backend, `0.0..=1.0`.
    pub confidence: f32,
}

/// Parse a WebVTT body into timed segments.
///
/// Accepts both the `.vtt` file format and the stdout progress format.
/// Unparsable timestamp lines are skipped rather than aborting the parse —
/// a partial transcript is more useful than none.
pub fn parse_vtt(output: &str) -> Result<Vec<TranscriptionSegment>, ParseError> {
    let mut segments = Vec::new();
    let mut start: Option<f32> = None;
    let mut end: Option<f32> = None;
    let mut text_lines: Vec<String> = Vec::new();

    for raw in output.lines() {
        let line = raw.trim_end();
        if let Some(idx) = line.find("-->") {
            // Flush the previous cue (timestamp alone on a line, text on the
            // following lines) before starting a new one.
            push_vtt_segment(&mut segments, start.take(), end.take(), &mut text_lines);

            let left = line[..idx].trim();
            let right = line[idx + 3..].trim();

            // The stdout progress format carries the cue text after the
            // closing `]` on the timestamp line itself: `[t1 --> t2]  text`.
            let left = left.strip_prefix('[').unwrap_or(left);
            let (right, inline_text) = match right.find(']') {
                Some(close) => (right[..close].trim(), right[close + 1..].trim()),
                None => (right, ""),
            };

            match (parse_vtt_timestamp(left), parse_vtt_timestamp(right)) {
                (Some(s), Some(e)) if !inline_text.is_empty() => {
                    segments.push(TranscriptionSegment {
                        start: s,
                        end: e,
                        text: inline_text.to_string(),
                        speaker: None,
                        confidence: 1.0,
                    });
                }
                (Some(s), Some(e)) => {
                    start = Some(s);
                    end = Some(e);
                }
                _ => {} // malformed timestamp line — ignore
            }
        } else if line.is_empty()
            || line.starts_with("WEBVTT")
            || line.starts_with("NOTE")
            || line.starts_with("Kind:")
            || line.starts_with("Language:")
        {
            continue;
        } else if start.is_some() {
            text_lines.push(line.trim().to_string());
        }
    }

    push_vtt_segment(&mut segments, start, end, &mut text_lines);
    Ok(segments)
}

fn push_vtt_segment(
    segments: &mut Vec<TranscriptionSegment>,
    start: Option<f32>,
    end: Option<f32>,
    text_lines: &mut Vec<String>,
) {
    let text = text_lines.join(" ").trim().to_string();
    text_lines.clear();
    if let (Some(s), Some(e)) = (start, end) {
        if !text.is_empty() {
            segments.push(TranscriptionSegment {
                start: s,
                end: e,
                text,
                speaker: None,
                confidence: 1.0,
            });
        }
    }
}

/// Parse a `HH:MM:SS.mmm` or `MM:SS.mmm` timestamp into seconds.
pub fn parse_vtt_timestamp(s: &str) -> Option<f32> {
    let s = s.trim();
    let parts: Vec<&str> = s.split(':').collect();
    if parts.is_empty() {
        return None;
    }
    let (h, m, sec) = match parts.len() {
        3 => (
            parts[0].parse::<f32>().ok()?,
            parts[1].parse::<f32>().ok()?,
            parts[2].parse::<f32>().ok()?,
        ),
        2 => (
            0.0,
            parts[0].parse::<f32>().ok()?,
            parts[1].parse::<f32>().ok()?,
        ),
        _ => return None,
    };
    Some(h * 3600.0 + m * 60.0 + sec)
}

/// A very small heuristic language tag.
///
/// CJK-heavy text is reported as `zh`; anything else is `auto`, meaning "let
/// the backend decide". This is deliberately conservative — it exists to pick
/// a model, not to be a language detector.
pub fn detect_language(text: &str) -> String {
    let cjk = text
        .chars()
        .filter(|c| ('\u{4E00}'..='\u{9FFF}').contains(c))
        .count();
    let total = text.chars().filter(|c| c.is_alphabetic()).count();
    if total > 0 && cjk as f32 / total as f32 > 0.3 {
        "zh".to_string()
    } else {
        "auto".to_string()
    }
}

/// Error type reserved for future strict-mode parsing.
///
/// The current implementation never fails: malformed cues are skipped so that
/// a partial transcript is still usable. The type exists so that adding a
/// strict mode later does not break the public API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    /// Human-readable description of what went wrong.
    pub message: String,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ParseError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_vtt_timestamps() {
        assert!((parse_vtt_timestamp("00:00:01.500").unwrap() - 1.5).abs() < 0.001);
        assert!((parse_vtt_timestamp("00:01:00.000").unwrap() - 60.0).abs() < 0.001);
        assert!((parse_vtt_timestamp("01:00:00.000").unwrap() - 3600.0).abs() < 0.001);
    }

    #[test]
    fn parses_two_component_timestamps() {
        assert!((parse_vtt_timestamp("01:30.000").unwrap() - 90.0).abs() < 0.001);
    }

    #[test]
    fn rejects_malformed_timestamps() {
        assert!(parse_vtt_timestamp("not-a-time").is_none());
        assert!(parse_vtt_timestamp("").is_none());
        assert!(parse_vtt_timestamp("1:2:3:4").is_none());
    }

    #[test]
    fn parses_vtt_body_into_segments() {
        let vtt = "WEBVTT\n\n00:00:00.000 --> 00:00:02.100\nhello world\n\n\
00:00:02.500 --> 00:00:04.000\nsecond segment\n";
        let segs = parse_vtt(vtt).unwrap();
        assert_eq!(segs.len(), 2);
        assert_eq!(segs[0].text, "hello world");
        assert!((segs[0].end - 2.1).abs() < 0.001);
        assert_eq!(segs[1].text, "second segment");
    }

    #[test]
    fn parses_whisper_stdout_progress_format() {
        // `whisper-cli` prints cues to stdout as `[t1 --> t2]  text` — the
        // same timestamps it writes into the .vtt file, but inline. Both must
        // produce the same segments (this was the "nothing happens" bug: the
        // parser only understood the file format and dropped every cue).
        let out = "[00:00:00.000 --> 00:00:02.000]   Привет!\n\
[00:00:02.000 --> 00:00:04.000]   мир\n";
        let segs = parse_vtt(out).unwrap();
        assert_eq!(segs.len(), 2);
        assert_eq!(segs[0].text, "Привет!");
        assert!((segs[0].start - 0.0).abs() < 0.001);
        assert!((segs[0].end - 2.0).abs() < 0.001);
        assert_eq!(segs[1].text, "мир");
        assert!((segs[1].start - 2.0).abs() < 0.001);
        assert!((segs[1].end - 4.0).abs() < 0.001);
    }

    #[test]
    fn both_formats_yield_the_same_segments() {
        let file = "00:00:00.000 --> 00:00:02.000\nhello\n";
        let stdout = "[00:00:00.000 --> 00:00:02.000]   hello\n";
        assert_eq!(parse_vtt(file).unwrap(), parse_vtt(stdout).unwrap());
    }

    #[test]
    fn skips_headers_and_notes() {
        let vtt = "WEBVTT\nKind: captions\nLanguage: en\nNOTE this is a note\n\n\
00:00:00.000 --> 00:00:01.000\ntext\n";
        let segs = parse_vtt(vtt).unwrap();
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].text, "text");
    }

    #[test]
    fn joins_multiline_cue_text() {
        let vtt = "00:00:00.000 --> 00:00:02.000\nline one\nline two\n";
        let segs = parse_vtt(vtt).unwrap();
        assert_eq!(segs[0].text, "line one line two");
    }

    #[test]
    fn drops_cues_without_text() {
        let vtt = "00:00:00.000 --> 00:00:02.000\n\n00:00:02.000 --> 00:00:03.000\nkept\n";
        let segs = parse_vtt(vtt).unwrap();
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].text, "kept");
    }

    #[test]
    fn ignores_malformed_timestamp_lines() {
        let vtt = "garbage line\n00:00:00.000 --> 00:00:01.000\nkept\n";
        let segs = parse_vtt(vtt).unwrap();
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].text, "kept");
    }

    #[test]
    fn detects_chinese() {
        assert_eq!(detect_language("你好世界"), "zh");
        assert_eq!(detect_language("hello world"), "auto");
    }

    #[test]
    fn detects_chinese_mixed() {
        // CJK-dominant mixed text still reads as `zh`.
        assert_eq!(detect_language("你好世界 hello"), "zh");
    }

    #[test]
    fn latin_dominant_mixed_text_stays_auto() {
        // 4 CJK ideographs out of 14 alphabetic characters is 0.29, below the
        // 0.3 threshold, so the backend is left to decide.
        assert_eq!(detect_language("你好 hello 世界 world"), "auto");
    }
}
