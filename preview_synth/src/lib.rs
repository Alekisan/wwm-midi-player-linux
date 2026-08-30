//! In-app audio preview via SoundFont synthesis.
//!
//! This crate is fully decoupled from the `/dev/uinput` input driver: it turns
//! MIDI note events into audible playback using `rustysynth` (pure-Rust
//! SoundFont 2 synthesis) mixed into a `rodio` output stream on a dedicated
//! background thread. The UI/playback thread only ever sends lightweight
//! [`AudioCommand`] messages — it never blocks on rendering or disk I/O.

pub mod synth;

use std::path::PathBuf;

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

/// The SoundFont file and `(bank, patch)` preset to use for an instrument.
#[derive(Debug, Clone)]
pub struct SoundFontChoice {
    pub path: PathBuf,
    pub bank: i32,
    pub patch: i32,
    /// True when this came from an instrument-specific font (not the GM fallback).
    pub specific: bool,
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

/// Directories searched for SoundFonts, in priority order. The project-local
/// `soundfonts/` folder ships instrument-specific fonts; the user dir holds the
/// GM fallback.
fn soundfont_search_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        dirs.push(cwd.join("soundfonts"));
    }
    if let Ok(exe) = std::env::current_exe() {
        for ancestor in exe.ancestors().skip(1) {
            let candidate = ancestor.join("soundfonts");
            if candidate.is_dir() {
                dirs.push(candidate);
            }
        }
    }
    dirs.push(soundfonts_dir());
    dirs
}

/// The directory SoundFonts are loaded from (per the spec, for the GM fallback).
pub fn soundfonts_dir() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    home.join(".local/share/where-winds-meet-player/soundfonts")
}

/// Name-based entries: (file name substring, bank, patch). Matched
/// case-insensitively against files found in the search dirs.
const SPECIFIC_FONTS: &[(Instrument, &str, i32, i32)] = &[
    (Instrument::Erhu, "erhu", 8, 110),        // FS_Erhu_v2.sf2
    (Instrument::Pipa, "pipa", 32, 105),       // MFA_Pipa.sf2
    (Instrument::Guqin, "guzheng", 1, 107),    // OLPC_Guzheng.sf2
    (Instrument::Guqin, "guqin", 0, 0),
    (Instrument::Erhu, "asian dreamz", 0, 4),  // DSK Asian DreamZ (ERHU)
    (Instrument::Pipa, "asian dreamz", 0, 0),  // DSK Asian DreamZ (PIPA)
    (Instrument::Guqin, "asian dreamz", 0, 3), // DSK Asian DreamZ (GUZHEN)
];

/// Resolve the SoundFont + preset for an instrument:
/// instrument-specific font (by name), else a GM fallback with the mapped program.
pub fn resolve_soundfont(instrument: Instrument) -> Result<SoundFontChoice, PreviewError> {
    let dirs = soundfont_search_dirs();

    // Instrument-specific: scan each dir for a file whose name matches.
    for dir in &dirs {
        let Ok(entries) = std::fs::read_dir(dir) else { continue };
        for entry in entries.flatten() {
            let fname = entry.file_name().to_string_lossy().to_lowercase();
            if !fname.ends_with(".sf2") {
                continue;
            }
            for (inst, needle, bank, patch) in SPECIFIC_FONTS {
                if *inst == instrument && fname.contains(needle) {
                    return Ok(SoundFontChoice {
                        path: entry.path(),
                        bank: *bank,
                        patch: *patch,
                        specific: true,
                    });
                }
            }
        }
    }

    // GM fallback.
    for dir in &dirs {
        for fallback in ["FluidR3_GM.sf2", "FluidR3_GS.sf2"] {
            let candidate = dir.join(fallback);
            if candidate.is_file() {
                return Ok(SoundFontChoice {
                    path: candidate,
                    bank: 0,
                    patch: instrument.gm_program(),
                    specific: false,
                });
            }
        }
    }

    Err(PreviewError::NoSoundFont {
        dir: dirs
            .first()
            .map(|d| d.to_string_lossy().into_owned())
            .unwrap_or_default(),
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
    fn specific_fonts_have_valid_patch_numbers() {
        for (inst, needle, bank, patch) in SPECIFIC_FONTS {
            assert!((0..128).contains(patch), "{needle} patch out of range");
            assert!(*bank >= 0, "{needle} bank negative");
            let _ = inst;
        }
    }

    #[test]
    fn gm_programs_are_in_gm_range() {
        for inst in Instrument::ALL {
            let p = inst.gm_program();
            assert!((0..128).contains(&p));
        }
    }
}
