//! Mapping of MIDI pitches onto the game's keyboard layout.
//!
//! The game's Free Play mode exposes a tiered QWERTY grid: three octave rows
//! (bass/alto/treble = low/mid/high), each holding the seven natural note
//! positions (Do–Ti). The **21-key** layout covers just those natural notes; the
//! **36-key** layout (toggled with F1 in-game) additionally unlocks the accidentals
//! of each octave as Shift/Ctrl modifier chords.

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

/// Which physical keyboard layout the game is using (F1 toggles this in-game).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum KeyMode {
    /// 21 natural notes (7 per octave × 3 octaves), no modifiers.
    #[default]
    TwentyOne,
    /// Full 36-note chromatic layout: accidentals via Shift/Ctrl chords.
    ThirtySix,
}

/// Note-to-key calculation modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NoteMode {
    /// Snap to the nearest available note.
    #[default]
    Closest,
    /// Direct 1:1 (`note % layout_size`) mapping, no transpose.
    Raw,
}

/// Natural keys for each octave row (low, mid, high), indexed by scale degree
/// 0..6 (Do, Re, Mi, Fa, Sol, La, Ti).
const NATURAL_KEYS: [[char; 7]; 3] = [
    ['z', 'x', 'c', 'v', 'b', 'n', 'm'],
    ['a', 's', 'd', 'f', 'g', 'h', 'j'],
    ['q', 'w', 'e', 'r', 't', 'y', 'u'],
];

/// The 21 natural notes of the instrument's playable range, C3–B5, indexed as
/// `octave * 7 + degree`: index 0–6 is C3–B3 (low), 7–13 is C4–B4 (mid), and
/// 14–20 is C5–B5 (high).
pub const DIATONIC_NOTES: [i32; 21] = [
    48, 50, 52, 53, 55, 57, 59, // Low  (C3–B3)
    60, 62, 64, 65, 67, 69, 71, // Mid  (C4–B4)
    72, 74, 76, 77, 79, 81, 83, // High (C5–B5)
];

/// Fold a pitch into the instrument range [C3, B5] by whole octaves, preserving
/// its pitch class.
pub fn normalize_into_range(note: i32) -> i32 {
    let lo = DIATONIC_NOTES[0];
    let hi = DIATONIC_NOTES[20];
    let mut result = note;
    while result < lo {
        result += 12;
    }
    while result > hi {
        result -= 12;
    }
    result
}

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

/// Map a diatonic index (0..20 = `octave * 7 + degree`) to its natural key.
fn diatonic_index_to_key(index: usize) -> KeyChord {
    let octave = index / 7;
    let degree = index % 7;
    KeyChord::Key(NATURAL_KEYS[octave][degree])
}

/// Index (0..20) of the natural note closest to `target`.
fn nearest_diatonic_index(target: i32) -> usize {
    let mut best = 0_usize;
    let mut best_dist = (DIATONIC_NOTES[0] - target).abs();
    for (i, &n) in DIATONIC_NOTES.iter().enumerate() {
        let dist = (n - target).abs();
        if dist < best_dist {
            best_dist = dist;
            best = i;
        }
    }
    best
}

/// 21-key "closest": fold into range, then snap to the nearest natural note.
pub fn note_to_key_21(note: i32, transpose: i32) -> KeyChord {
    let target = normalize_into_range(note + transpose);
    diatonic_index_to_key(nearest_diatonic_index(target))
}

/// 21-key "raw": `note % 21` directly indexes the natural-note grid.
pub fn note_to_key_21_raw(note: i32) -> KeyChord {
    diatonic_index_to_key(note.rem_euclid(21) as usize)
}

/// 36-key "closest": resolve to the nearest chromatic pitch class.
pub fn note_to_key_36(note: i32, transpose: i32) -> KeyChord {
    let target = note + transpose;
    semitone_to_key(target.rem_euclid(12), octave_of(target))
}

/// 36-key "raw": direct 1:1 mapping across all 36 chromatic slots, no transpose.
pub fn note_to_key_36_raw(note: i32) -> KeyChord {
    let key_idx = note.rem_euclid(36);
    let octave = (key_idx / 12) as usize;
    let semitone = key_idx % 12;
    semitone_to_key(semitone, octave)
}

/// Dispatch a MIDI note through the requested key layout and note mode.
pub fn map_note(key_mode: KeyMode, note_mode: NoteMode, note: u8, transpose: i32) -> KeyChord {
    match (key_mode, note_mode) {
        (KeyMode::TwentyOne, NoteMode::Closest) => note_to_key_21(note as i32, transpose),
        (KeyMode::TwentyOne, NoteMode::Raw) => note_to_key_21_raw(note as i32),
        (KeyMode::ThirtySix, NoteMode::Closest) => note_to_key_36(note as i32, transpose),
        (KeyMode::ThirtySix, NoteMode::Raw) => note_to_key_36_raw(note as i32),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn naturals_map_to_plain_keys() {
        // Middle row, C4 = a (mid octave).
        assert_eq!(note_to_key_36(60, 0), KeyChord::Key('a'));
        assert_eq!(note_to_key_36(62, 0), KeyChord::Key('s'));
        assert_eq!(note_to_key_36(64, 0), KeyChord::Key('d'));
        assert_eq!(note_to_key_36(65, 0), KeyChord::Key('f'));
        assert_eq!(note_to_key_36(67, 0), KeyChord::Key('g'));
        assert_eq!(note_to_key_36(69, 0), KeyChord::Key('h'));
        assert_eq!(note_to_key_36(71, 0), KeyChord::Key('j'));
    }

    #[test]
    fn accidentals_map_to_shift_and_ctrl() {
        assert_eq!(note_to_key_36(61, 0), KeyChord::Shift('a')); // C#4
        assert_eq!(note_to_key_36(63, 0), KeyChord::Ctrl('d')); // D#4
        assert_eq!(note_to_key_36(66, 0), KeyChord::Shift('f')); // F#4
        assert_eq!(note_to_key_36(70, 0), KeyChord::Ctrl('j')); // A#4
    }

    #[test]
    fn octave_rows_use_expected_keys() {
        assert_eq!(note_to_key_36(48, 0), KeyChord::Key('z')); // C3 low = z
        assert_eq!(note_to_key_36(72, 0), KeyChord::Key('q')); // C5 high = q
        assert_eq!(note_to_key_36(83, 0), KeyChord::Key('u')); // B5 high = u
    }

    #[test]
    fn raw_mode_is_deterministic() {
        assert_eq!(note_to_key_36_raw(60), KeyChord::Key('q'));
        assert_eq!(note_to_key_36_raw(0), KeyChord::Key('z'));
        assert_eq!(note_to_key_36_raw(36), KeyChord::Key('z'));
    }

    #[test]
    fn twent_one_key_maps_natural_notes_across_rows() {
        // C3/C4/C5 land on low/mid/high C keys.
        assert_eq!(note_to_key_21(48, 0), KeyChord::Key('z'));
        assert_eq!(note_to_key_21(60, 0), KeyChord::Key('a'));
        assert_eq!(note_to_key_21(72, 0), KeyChord::Key('q'));
        // Ti (B) = 7th degree.
        assert_eq!(note_to_key_21(71, 0), KeyChord::Key('j'));
    }

    #[test]
    fn twenty_one_key_quantizes_accidentals_to_nearest_natural() {
        // C#4 (61) is equidistant from C4/D4; ties resolve to the lower natural.
        assert_eq!(note_to_key_21(61, 0), KeyChord::Key('a')); // C#4 -> C4
        // A#4 (70) is nearer A4 (69) than B4 (71).
        assert_eq!(note_to_key_21(70, 0), KeyChord::Key('h')); // A#4 -> A4
    }

    #[test]
    fn twenty_one_key_never_uses_modifiers() {
        for note in 0..=127 {
            let key = note_to_key_21(note, 0);
            assert!(matches!(key, KeyChord::Key(_)), "note {note} produced {key}");
        }
    }

    #[test]
    fn twenty_one_key_raw_wraps_modulo_21() {
        assert_eq!(note_to_key_21_raw(0), KeyChord::Key('z'));
        assert_eq!(note_to_key_21_raw(21), KeyChord::Key('z'));
        assert_eq!(note_to_key_21_raw(7), KeyChord::Key('a')); // second octave Do
    }

    #[test]
    fn normalize_into_range_wraps_octaves() {
        assert_eq!(normalize_into_range(36), 48); // C2 -> C3
        assert_eq!(normalize_into_range(95), 83); // B6 -> B5
        assert_eq!(normalize_into_range(60), 60); // C4 unchanged
    }

    #[test]
    fn display_formats_chords() {
        assert_eq!(KeyChord::Key('z').to_string(), "z");
        assert_eq!(KeyChord::Shift('a').to_string(), "shift+a");
        assert_eq!(KeyChord::Ctrl('c').to_string(), "ctrl+c");
    }
}
