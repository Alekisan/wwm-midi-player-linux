//! Persistent MIDI playlist storage.
//!
//! A playlist is an ordered list of absolute MIDI file paths. It is wholly
//! decoupled from playback and the GUI: the front-end edits it and this crate
//! hands it back as plain strings, persisting to disk as JSON. Keeping it a
//! standalone crate means the model is unit-testable without Qt and can later be
//! shared by a CLI without pulling in any UI dependencies.
//!
//! Storage lives under the XDG config directory
//! (`$XDG_CONFIG_HOME/where-winds-meet-player/playlist.json`, defaulting to
//! `~/.config/...`), since a playlist is user-editable state rather than
//! installed data (which stays under `~/.local/share/...` alongside the
//! SoundFonts).

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// An ordered, editable list of MIDI file paths.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Playlist {
    /// Absolute paths, in playlist order.
    pub entries: Vec<String>,
}

impl Playlist {
    /// Absolute path of the on-disk playlist file.
    pub fn default_path() -> PathBuf {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
            .unwrap_or_else(|| PathBuf::from("."));
        base.join("where-winds-meet-player").join("playlist.json")
    }

    /// Load a playlist from `path`, yielding an empty playlist on any failure
    /// (missing file, unreadable, or malformed JSON). Loading must never panic.
    pub fn load(path: &Path) -> Self {
        let Ok(text) = std::fs::read_to_string(path) else {
            return Playlist::default();
        };
        serde_json::from_str(&text).unwrap_or_default()
    }

    /// Write the playlist to `path`, creating parent directories as needed.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text =
            serde_json::to_string_pretty(self).unwrap_or_else(|_| String::from("{\"entries\":[]}"));
        std::fs::write(path, text)
    }

    /// Append a path, ignoring duplicates.
    pub fn add(&mut self, path: String) {
        if !path.is_empty() && !self.entries.contains(&path) {
            self.entries.push(path);
        }
    }

    /// Remove the entry at `index` (no-op if out of range).
    pub fn remove(&mut self, index: usize) -> Option<String> {
        if index < self.entries.len() {
            Some(self.entries.remove(index))
        } else {
            None
        }
    }

    /// Move the entry at `from` to `to`, shifting the others along.
    pub fn move_entry(&mut self, from: usize, to: usize) {
        if from >= self.entries.len() || to >= self.entries.len() || from == to {
            return;
        }
        let item = self.entries.remove(from);
        self.entries.insert(to, item);
    }

    /// Display name for a path: the file stem, or the whole path as a fallback.
    pub fn name_of(path: &str) -> String {
        Path::new(path)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string())
    }

    /// Display names, in playlist order.
    pub fn names(&self) -> Vec<String> {
        self.entries.iter().map(|p| Playlist::name_of(p)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir() -> PathBuf {
        std::env::temp_dir().join(format!("wwm-playlist-test-{}", std::process::id()))
    }

    #[test]
    fn roundtrips_through_json() {
        let dir = tmp_dir();
        let path = dir.join("playlist.json");
        let mut p = Playlist::default();
        p.add("/music/a.mid".into());
        p.add("/music/b.mid".into());
        p.add("/music/a.mid".into()); // duplicate ignored
        p.save(&path).unwrap();

        let loaded = Playlist::load(&path);
        assert_eq!(loaded.entries, vec!["/music/a.mid", "/music/b.mid"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_or_malformed_file_yields_empty() {
        let dir = tmp_dir();
        let missing = dir.join("nope.json");
        assert!(Playlist::load(&missing).entries.is_empty());

        let malformed = dir.join("bad.json");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&malformed, "{ not json !").unwrap();
        assert!(Playlist::load(&malformed).entries.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn move_entry_reorders() {
        let mut p = Playlist::default();
        for s in ["a.mid", "b.mid", "c.mid"] {
            p.add(format!("/m/{s}"));
        }
        p.move_entry(0, 2);
        assert_eq!(p.entries, vec!["/m/b.mid", "/m/c.mid", "/m/a.mid"]);
        p.move_entry(2, 1);
        assert_eq!(p.entries, vec!["/m/b.mid", "/m/a.mid", "/m/c.mid"]);
    }

    #[test]
    fn remove_is_optional_and_bounded() {
        let mut p = Playlist::default();
        p.add("/m/a.mid".into());
        assert_eq!(p.remove(5), None);
        assert_eq!(p.remove(0), Some("/m/a.mid".into()));
        assert!(p.entries.is_empty());
    }

    #[test]
    fn name_is_the_file_stem() {
        assert_eq!(Playlist::name_of("/music/foo.mid"), "foo");
        assert_eq!(Playlist::name_of("/weird"), "weird");
    }

    #[test]
    fn default_path_honors_xdg_config_home() {
        // Safe-guard: only assert the trailing portion, independent of env.
        let p = Playlist::default_path();
        assert!(p.ends_with(Path::new("where-winds-meet-player/playlist.json")));
    }
}
