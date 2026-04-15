use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Quarter {
    Q1,
    Q2,
    Q3,
    Q4,
}

impl fmt::Display for Quarter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Quarter::Q1 => write!(f, "Q1"),
            Quarter::Q2 => write!(f, "Q2"),
            Quarter::Q3 => write!(f, "Q3"),
            Quarter::Q4 => write!(f, "Q4"),
        }
    }
}

impl FromStr for Quarter {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "Q1" => Ok(Quarter::Q1),
            "Q2" => Ok(Quarter::Q2),
            "Q3" => Ok(Quarter::Q3),
            "Q4" => Ok(Quarter::Q4),
            _ => Err(format!("Invalid quarter: {}", s)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptSegment {
    pub start_time: f32,
    pub end_time: f32,
    pub text: String,
    pub speaker_id: String,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub words: Vec<WordTiming>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WordTiming {
    pub start_time: f32,
    pub end_time: f32,
    pub text: String,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DialogueTurn {
    pub speaker: String,
    pub start_time: f32,
    pub end_time: f32,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transcript {
    pub segments: Vec<TranscriptSegment>,
}

impl Transcript {
    /// Plain text with speaker labels, one segment per line.
    pub fn full_text(&self) -> String {
        self.segments
            .iter()
            .filter(|s| !s.text.is_empty())
            .map(|s| {
                if s.speaker_id.is_empty() {
                    s.text.clone()
                } else {
                    format!("{}: {}", s.speaker_id, s.text)
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Get the set of unique speaker IDs.
    pub fn unique_speakers(&self) -> Vec<String> {
        let mut speakers: Vec<String> = self
            .segments
            .iter()
            .map(|s| s.speaker_id.clone())
            .filter(|s| !s.is_empty())
            .collect();
        speakers.sort();
        speakers.dedup();
        speakers
    }

    /// Group consecutive segments by the same speaker into dialogue turns.
    pub fn as_dialogue(&self) -> Vec<DialogueTurn> {
        let mut turns: Vec<DialogueTurn> = Vec::new();

        for seg in &self.segments {
            let text = seg.text.trim();
            if text.is_empty() {
                continue;
            }

            match turns.last_mut() {
                Some(last) if last.speaker == seg.speaker_id => {
                    last.text.push(' ');
                    last.text.push_str(text);
                    last.end_time = seg.end_time;
                }
                _ => {
                    turns.push(DialogueTurn {
                        speaker: seg.speaker_id.clone(),
                        start_time: seg.start_time,
                        end_time: seg.end_time,
                        text: text.to_string(),
                    });
                }
            }
        }

        turns
    }

    /// Total duration of the transcript in seconds.
    pub fn duration(&self) -> f32 {
        self.segments.last().map(|s| s.end_time).unwrap_or(0.0)
    }

    /// True if any segment has a speaker label.
    pub fn has_speakers(&self) -> bool {
        self.segments.iter().any(|s| !s.speaker_id.is_empty())
    }
}

impl fmt::Display for Transcript {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for turn in self.as_dialogue() {
            if turn.speaker.is_empty() {
                writeln!(
                    f,
                    "[{:.1}s → {:.1}s] {}",
                    turn.start_time, turn.end_time, turn.text
                )?;
            } else {
                writeln!(
                    f,
                    "[{:.1}s → {:.1}s] {}: {}",
                    turn.start_time, turn.end_time, turn.speaker, turn.text
                )?;
            }
        }
        Ok(())
    }
}
