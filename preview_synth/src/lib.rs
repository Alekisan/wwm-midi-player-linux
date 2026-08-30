//! In-app audio preview via SoundFont synthesis.
//!
//! This crate is fully decoupled from the `/dev/uinput` input driver: it turns
//! MIDI note events into audible playback using `rustysynth` (pure-Rust
//! SoundFont 2 synthesis) mixed into a `rodio` output stream on a dedicated
//! background thread. The UI/playback thread only ever sends lightweight
//! [`AudioCommand`] messages — it never blocks on rendering or disk I/O.

pub mod synth;

use std::path::{Path, PathBuf};

pub use synth::Preview;

/// The five instruments available in Where Winds Meet, per the preview spec.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Instrument {
    Guqin,
    Pipa,
    Erhu,
    Konghou,
    Fangxiang,
}

impl Instrument {
    /// All instruments in a stable UI order.
    pub const ALL: [Instrument; 5] = [
        Instrument::Guqin,
        Instrument::Pipa,
        Instrument::Erhu,
        Instrument::Konghou,
        Instrument::Fangxiang,
    ];

    /// Display name (with Chinese characters).
    pub fn display_name(self) -> &'static str {
        match self {
            Instrument::Guqin => "Guqin (古琴)",
            Instrument::Pipa => "Pipa (琵琶)",
            Instrument::Erhu => "Erhu (二胡)",
            Instrument::Konghou => "Konghou (箜篌)",
            Instrument::Fangxiang => "Fangxiang (方響)",
        }
    }

    /// The instrument-specific SoundFont file name to try first, per the spec's
    /// asset-resolution strategy.
    pub fn sf2_name(self) -> &'static str {
        match self {
            Instrument::Guqin => "guqin.sf2",
            Instrument::Pipa => "pipa.sf2",
            Instrument::Erhu => "erhu.sf2",
            Instrument::Konghou => "konghou.sf2",
            Instrument::Fangxiang => "fangxiang.sf2",
        }
    }

    /// General MIDI program number used when falling back to a GM SoundFont
    /// (e.g. `FluidR3_GM.sf2`), chosen to approximate each instrument.
    ///
    /// | Instrument | GM fallback |
    /// | Guqin      | 107 Koto    |
    /// | Pipa       | 105 Sitar   |
    /// | Erhu       | 40  Violin  |
    /// | Konghou    | 46  Orchestral Harp |
    /// | Fangxiang  | 9   Glockenspiel |
    pub fn gm_program(self) -> i32 {
        match self {
            Instrument::Guqin => 107,
            Instrument::Pipa => 105,
            Instrument::Erhu => 40,
            Instrument::Konghou => 46,
            Instrument::Fangxiang => 9,
        }
    }
}

/// Commands sent from the UI/playback thread to the audio renderer thread.
#[derive(Debug, Clone)]
pub enum AudioCommand {
    /// Begin a note.
    PlayNote { pitch: u8, velocity: u8 },
    /// Release a note.
    StopNote { pitch: u8 },
    /// Hot-swap the active instrument (reloads the preset, not the stream).
    SetInstrument(Instrument),
    /// Master volume, 0.0..=1.0.
    SetVolume(f32),
    /// Stop all sounding notes immediately.
    AllNotesOff,
    /// Shut the renderer thread down.
    Shutdown,
}

/// Errors surfaced by the preview system.
#[derive(Debug, thiserror::Error)]
pub enum PreviewError {
    #[error("no audio output device available: {0}")]
    NoOutput(String),
    #[error("no SoundFont found for instrument (looked in {dir}); preview is muted")]
    NoSoundFont { dir: String },
    #[error("failed to load SoundFont {path}: {detail}")]
    SoundFontLoad { path: String, detail: String },
}

/// The directory SoundFonts are loaded from.
pub fn soundfonts_dir() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    home.join(".local/share/where-winds-meet-player/soundfonts")
}

/// Resolve the SoundFont file for an instrument, per the spec's hierarchy:
/// instrument-specific `.sf2` first, then a general GM SoundFont.
pub fn resolve_soundfont(instrument: Instrument) -> Result<PathBuf, PreviewError> {
    resolve_soundfont_in(instrument, &soundfonts_dir())
}

fn resolve_soundfont_in(instrument: Instrument, dir: &Path) -> Result<PathBuf, PreviewError> {
    let specific = dir.join(instrument.sf2_name());
    if specific.is_file() {
        return Ok(specific);
    }
    for fallback in ["FluidR3_GM.sf2", "FluidR3_GS.sf2"] {
        let candidate = dir.join(fallback);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err(PreviewError::NoSoundFont {
        dir: dir.to_string_lossy().into_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_instrument_has_unique_display_name_and_sf2() {
        let mut names = std::collections::HashSet::new();
        for inst in Instrument::ALL {
            assert!(names.insert(inst.display_name()));
            assert!(inst.sf2_name().ends_with(".sf2"));
        }
    }

    #[test]
    fn resolve_prefers_instrument_specific() {
        let dir = std::env::temp_dir().join(format!("wwm-sf-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("FluidR3_GM.sf2"), b"fake").unwrap();
        std::fs::write(dir.join("guqin.sf2"), b"fake").unwrap();

        // Specific file wins.
        assert_eq!(
            resolve_soundfont_in(Instrument::Guqin, &dir).unwrap(),
            dir.join("guqin.sf2")
        );
        // Others fall back to GM.
        assert_eq!(
            resolve_soundfont_in(Instrument::Pipa, &dir).unwrap(),
            dir.join("FluidR3_GM.sf2")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolve_errors_when_nothing_present() {
        let dir = std::env::temp_dir().join("wwm-sf-empty");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(resolve_soundfont_in(Instrument::Erhu, &dir).is_err());
    }

    #[test]
    fn gm_programs_are_in_gm_range() {
        for inst in Instrument::ALL {
            let p = inst.gm_program();
            assert!((0..128).contains(&p));
        }
    }
}
