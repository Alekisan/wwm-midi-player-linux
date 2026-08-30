//! Decoupled MIDI parsing and note-to-key mapping engine.
//!
//! This crate has no knowledge of Qt, Wayland, `/dev/uinput`, or any GUI. It is
//! the standalone core described in the project directives: it parses `.mid`
//! files, converts tick-based event timing to wall-clock milliseconds, and
//! translates MIDI pitches onto the game's keyboard layout.

pub mod mapping;
pub mod midi;

pub use mapping::{KeyChord, NoteMode};
pub use midi::{MidiError, NoteEvent, NoteKind, Song};
