//! MIDI file parsing: reads an SMF (Standard MIDI File), resolves tempo changes,
//! and produces a flat, time-ordered stream of note events in milliseconds.

use midly::{MidiMessage, Smf, Timing, TrackEventKind};
use thiserror::Error;

/// Errors that can occur while loading or parsing a MIDI file.
#[derive(Debug, Error)]
pub enum MidiError {
    #[error("failed to read file: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to parse MIDI: {0}")]
    Parse(String),
}

/// Whether a [`NoteEvent`] begins or ends a note.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteKind {
    NoteOn,
    NoteOff,
}

/// A single note event resolved to wall-clock milliseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoteEvent {
    /// Absolute time relative to the song start, in milliseconds.
    pub time_ms: u64,
    pub kind: NoteKind,
    /// MIDI note number (0..127).
    pub note: u8,
    /// Index of the MIDI track this event came from (for band/ensemble modes).
    pub track_id: usize,
}

/// A parsed song, ready for playback.
#[derive(Debug, Clone)]
pub struct Song {
    /// Sorted note events, ascending by `time_ms`.
    pub events: Vec<NoteEvent>,
    /// Approximate duration in seconds.
    pub duration_secs: f64,
    /// Detected best transpose, in semitones.
    pub transpose: i32,
    /// Initial tempo in microseconds per quarter note.
    pub tempo_us_per_qn: u32,
    /// Ticks per quarter note (resolution of the delta-times).
    pub ticks_per_quarter: u16,
}

/// A tempo change at an absolute tick position (μs per quarter note).
#[derive(Debug, Clone, Copy)]
struct TempoChange {
    tick: u64,
    us_per_qn: u32,
}

/// Converts absolute ticks to milliseconds, applying tempo changes.
#[derive(Debug, Clone)]
struct TempoMap {
    ticks_per_quarter: f64,
    changes: Vec<TempoChange>,
}

impl TempoMap {
    fn new(ticks_per_quarter: f64, mut changes: Vec<TempoChange>) -> Self {
        changes.sort_by_key(|c| c.tick);
        TempoMap {
            ticks_per_quarter,
            changes,
        }
    }

    /// Convert an absolute tick position to milliseconds.
    fn to_ms(&self, ticks: u64) -> u64 {
        const DEFAULT_TEMPO: u32 = 500_000; // 120 BPM

        let mut result = 0.0_f64;
        let mut last_tick = 0_u64;
        let mut current_tempo = DEFAULT_TEMPO as f64;

        for change in &self.changes {
            if change.tick >= ticks {
                break;
            }
            let delta = (change.tick - last_tick) as f64;
            result += delta / self.ticks_per_quarter * current_tempo / 1000.0;
            last_tick = change.tick;
            current_tempo = change.us_per_qn as f64;
        }

        let delta = (ticks - last_tick) as f64;
        result += delta / self.ticks_per_quarter * current_tempo / 1000.0;
        result as u64
    }
}

/// Parse raw MIDI bytes into a [`Song`].
pub fn parse(data: &[u8]) -> Result<Song, MidiError> {
    let smf = Smf::parse(data).map_err(|e| MidiError::Parse(e.to_string()))?;

    let ticks_per_quarter = match smf.header.timing {
        Timing::Metrical(tpq) => tpq.as_int() as f64,
        // SMTPE timing; fall back to a sensible default resolution.
        _ => 480.0,
    };

    // First pass: collect tempo changes from every track.
    let mut tempo_changes: Vec<TempoChange> = Vec::new();
    for track in &smf.tracks {
        let mut tick = 0_u64;
        for event in track {
            tick += event.delta.as_int() as u64;
            if let TrackEventKind::Meta(midly::MetaMessage::Tempo(t)) = event.kind {
                tempo_changes.push(TempoChange {
                    tick,
                    us_per_qn: t.as_int(),
                });
            }
        }
    }
    let tempo_map = TempoMap::new(ticks_per_quarter, tempo_changes);

    // Second pass: collect note events with absolute timing.
    let mut events: Vec<NoteEvent> = Vec::new();
    for (track_idx, track) in smf.tracks.iter().enumerate() {
        let mut tick = 0_u64;
        for event in track {
            tick += event.delta.as_int() as u64;
            if let TrackEventKind::Midi { message, .. } = event.kind {
                match message {
                    MidiMessage::NoteOn { key, vel } => {
                        let kind = if vel.as_int() > 0 {
                            NoteKind::NoteOn
                        } else {
                            NoteKind::NoteOff
                        };
                        events.push(NoteEvent {
                            time_ms: tempo_map.to_ms(tick),
                            kind,
                            note: key.as_int(),
                            track_id: track_idx,
                        });
                    }
                    MidiMessage::NoteOff { key, .. } => {
                        events.push(NoteEvent {
                            time_ms: tempo_map.to_ms(tick),
                            kind: NoteKind::NoteOff,
                            note: key.as_int(),
                            track_id: track_idx,
                        });
                    }
                    _ => {}
                }
            }
        }
    }

    events.sort_by_key(|e| e.time_ms);
    events.dedup_by_key(|e| (e.time_ms, e.kind, e.note, e.track_id));

    let duration_secs = events.last().map_or(0.0, |e| e.time_ms as f64 / 1000.0);
    let transpose = detect_best_transpose(&events);

    // Initial tempo: the earliest tempo change, else default.
    let tempo_us_per_qn = tempo_map
        .changes
        .first()
        .map_or(500_000, |c| c.us_per_qn);

    Ok(Song {
        events,
        duration_secs,
        transpose,
        tempo_us_per_qn,
        ticks_per_quarter: ticks_per_quarter as u16,
    })
}

/// Load a MIDI file from disk.
pub fn load_file(path: &str) -> Result<Song, MidiError> {
    let data = std::fs::read(path)?;
    parse(&data)
}

// The instrument's playable scale, from the reference implementation.
// C3 (48) .. B5 (83): seven note names across three octaves.
const INSTRUMENT_NOTES: [i32; 21] = [
    48, 50, 52, 53, 55, 57, 59, // Low  (C3-B3)
    60, 62, 64, 65, 67, 69, 71, // Mid  (C4-B4)
    72, 74, 76, 77, 79, 81, 83, // High (C5-B5)
];

/// Shift a note into the instrument's playable range [C3, B5] by octaves.
fn normalize_into_range(note: i32) -> i32 {
    let lo = INSTRUMENT_NOTES[0];
    let hi = INSTRUMENT_NOTES[20];
    let mut result = note;
    while result < lo {
        result += 12;
    }
    while result > hi {
        result -= 12;
    }
    result
}

/// Heuristic: pick the transpose (-12..=12) that best aligns the song's notes
/// with the instrument scale, minimizing total distance to the nearest scale note.
pub fn detect_best_transpose(events: &[NoteEvent]) -> i32 {
    let mut best_transpose: i32 = 0;
    let mut best_score = i32::MAX;

    for transpose in -12..=12 {
        let mut score = 0_i32;
        for event in events {
            if event.kind != NoteKind::NoteOn {
                continue;
            }
            let normalized = normalize_into_range(event.note as i32 + transpose);
            let min_distance = INSTRUMENT_NOTES
                .iter()
                .map(|&n| (n - normalized).abs())
                .min()
                .unwrap_or(i32::MAX);
            score += min_distance;
        }
        // Prefer a lower score; on ties prefer the transpose closest to zero.
        if score < best_score || (score == best_score && transpose.abs() < best_transpose.abs()) {
            best_score = score;
            best_transpose = transpose;
        }
    }

    best_transpose
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note_on(time_ms: u64, note: u8) -> NoteEvent {
        NoteEvent {
            time_ms,
            kind: NoteKind::NoteOn,
            note,
            track_id: 0,
        }
    }

    #[test]
    fn transpose_of_empty_events_is_zero() {
        assert_eq!(detect_best_transpose(&[]), 0);
    }

    #[test]
    fn transpose_aligns_major_scale() {
        // A C-major scale already sits perfectly on the instrument notes.
        let events: Vec<NoteEvent> = [60, 62, 64, 65, 67, 69, 71]
            .into_iter()
            .enumerate()
            .map(|(i, n)| note_on(i as u64, n))
            .collect();
        assert_eq!(detect_best_transpose(&events), 0);
    }

    #[test]
    fn normalize_into_range_wraps_octaves() {
        assert_eq!(normalize_into_range(36), 48); // C2 -> C3
        assert_eq!(normalize_into_range(95), 83); // B6 -> B5
        assert_eq!(normalize_into_range(60), 60); // C4 unchanged
    }
}
