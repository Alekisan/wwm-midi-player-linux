//! Mapping of MIDI pitches onto the game's keyboard layout.
//!
//! The instrument exposes 36 keys: three octaves (low / mid / high) of the
//! twelve chromatic pitch classes. Natural notes map to a plain key; accidentals
//! map to a `Shift` or `Ctrl` chord on the corresponding natural key. This is the
//! same scheme as the reference player's "36-key" mode.

use std::fmt;

/// A keyboard chord: an optional modifier plus a base key character.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyChord {
    /// Plain key, e.g. `z`.
    Key(char),
    /// `Shift` + key, e.g. `Shift+z`.
    Shift(char),
    /// `Ctrl` + key, e.g. `Ctrl+c`.
    Ctrl(char),
}

impl fmt::Display for KeyChord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KeyChord::Key(k) => write!(f, "{k}"),
            KeyChord::Shift(k) => write!(f, "shift+{k}"),
            KeyChord::Ctrl(k) => write!(f, "ctrl+{k}"),
        }
    }
}

/// Note-to-key calculation modes. For Phase 1 only the two most-used modes are
/// implemented; the remaining modes follow in later phases.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NoteMode {
    /// Find the closest chromatic pitch class (may produce accidentals).
    #[default]
    Closest,
    /// Raw 1:1 mapping with no transpose, `note % 36`.
    Raw,
}

/// Natural keys for each octave row (low, mid, high), indexed by scale degree
/// 0..6 (C, D, E, F, G, A, B).
const NATURAL_KEYS: [[char; 7]; 3] = [
    ['z', 'x', 'c', 'v', 'b', 'n', 'm'],
    ['a', 's', 'd', 'f', 'g', 'h', 'j'],
    ['q', 'w', 'e', 'r', 't', 'y', 'u'],
];

/// Map a pitch class (0..11) and octave row (0..2) to a chord.
///
/// Accidentals follow the reference layout: `C#`/`F#`/`G#` use `Shift`, and
/// `D#`/`A#` use `Ctrl`, on their respective natural keys.
fn semitone_to_key(semitone: i32, octave: usize) -> KeyChord {
    let n = NATURAL_KEYS[octave];
    match semitone {
        0 => KeyChord::Key(n[0]),   // C
        1 => KeyChord::Shift(n[0]), // C#
        2 => KeyChord::Key(n[1]),   // D
        3 => KeyChord::Ctrl(n[2]),  // D#/Eb
        4 => KeyChord::Key(n[2]),   // E
        5 => KeyChord::Key(n[3]),   // F
        6 => KeyChord::Shift(n[3]), // F#
        7 => KeyChord::Key(n[4]),   // G
        8 => KeyChord::Shift(n[4]), // G#
        9 => KeyChord::Key(n[5]),   // A
        10 => KeyChord::Ctrl(n[6]), // A#/Bb
        11 => KeyChord::Key(n[6]),  // B
        _ => KeyChord::Key(n[0]),   // unreachable fallback (tonic)
    }
}

/// Determine the octave row for an absolute (post-transpose) pitch.
/// C4 (60) marks the start of the mid row; C5 (72) the high row.
fn octave_of(target: i32) -> usize {
    if target < 60 {
        0
    } else if target < 72 {
        1
    } else {
        2
    }
}

/// Closest mode: resolve the pitch to its nearest chromatic pitch class.
pub fn note_to_key_closest(note: i32, transpose: i32) -> KeyChord {
    let target = note + transpose;
    semitone_to_key(target.rem_euclid(12), octave_of(target))
}

/// Raw mode: direct 1:1 mapping across all 36 keys, no transpose.
pub fn note_to_key_raw(note: i32) -> KeyChord {
    let key_idx = note.rem_euclid(36);
    let octave = (key_idx / 12) as usize;
    let semitone = key_idx % 12;
    semitone_to_key(semitone, octave)
}

/// Dispatch a MIDI note through the requested [`NoteMode`].
pub fn map_note(mode: NoteMode, note: u8, transpose: i32) -> KeyChord {
    match mode {
        NoteMode::Closest => note_to_key_closest(note as i32, transpose),
        NoteMode::Raw => note_to_key_raw(note as i32),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn naturals_map_to_plain_keys() {
        // Middle row, C4 = a (mid octave).
        assert_eq!(note_to_key_closest(60, 0), KeyChord::Key('a'));
        assert_eq!(note_to_key_closest(62, 0), KeyChord::Key('s'));
        assert_eq!(note_to_key_closest(64, 0), KeyChord::Key('d'));
        assert_eq!(note_to_key_closest(65, 0), KeyChord::Key('f'));
        assert_eq!(note_to_key_closest(67, 0), KeyChord::Key('g'));
        assert_eq!(note_to_key_closest(69, 0), KeyChord::Key('h'));
        assert_eq!(note_to_key_closest(71, 0), KeyChord::Key('j'));
    }

    #[test]
    fn accidentals_map_to_shift_and_ctrl() {
        assert_eq!(note_to_key_closest(61, 0), KeyChord::Shift('a')); // C#4
        assert_eq!(note_to_key_closest(63, 0), KeyChord::Ctrl('d')); // D#4
        assert_eq!(note_to_key_closest(66, 0), KeyChord::Shift('f')); // F#4
        assert_eq!(note_to_key_closest(70, 0), KeyChord::Ctrl('j')); // A#4
    }

    #[test]
    fn octave_rows_use_expected_keys() {
        assert_eq!(note_to_key_closest(48, 0), KeyChord::Key('z')); // C3 low = z
        assert_eq!(note_to_key_closest(72, 0), KeyChord::Key('q')); // C5 high = q
        assert_eq!(note_to_key_closest(83, 0), KeyChord::Key('u')); // B5 high = u
    }

    #[test]
    fn raw_mode_is_deterministic() {
        assert_eq!(note_to_key_raw(60), KeyChord::Key('q'));
        assert_eq!(note_to_key_raw(0), KeyChord::Key('z'));
        assert_eq!(note_to_key_raw(36), KeyChord::Key('z'));
    }

    #[test]
    fn display_formats_chords() {
        assert_eq!(KeyChord::Key('z').to_string(), "z");
        assert_eq!(KeyChord::Shift('a').to_string(), "shift+a");
        assert_eq!(KeyChord::Ctrl('c').to_string(), "ctrl+c");
    }
}
