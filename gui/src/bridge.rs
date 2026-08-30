//! QML bridge exposing the decoupled player core to the Qt front-end.
//!
//! The GUI owns no playback logic: it forwards user actions to [`wwm_player`]
//! and mirrors the player's event stream into Qt properties and signals. QML
//! drives [`PlayerBridge::poll`] from a timer, which keeps all state handling on
//! the GUI thread without any cross-thread signal plumbing.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qstringlist.h");
        type QStringList = cxx_qt_lib::QStringList;
    }

    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(QString, file_name)]
        #[qproperty(QString, status)]
        #[qproperty(f64, duration)]
        #[qproperty(f64, position)]
        #[qproperty(bool, playing)]
        #[qproperty(bool, paused)]
        #[qproperty(bool, live)]
        #[qproperty(bool, game_running)]
        #[qproperty(QStringList, songs)]
        #[qproperty(QStringList, song_paths)]
        #[qproperty(i32, current_index)]
        #[qproperty(f64, speed)]
        #[qproperty(i32, note_count)]
        #[qproperty(i32, transpose)]
        #[qproperty(i32, bpm)]
        #[qproperty(bool, loaded)]
        type PlayerBridge = super::PlayerBridgeRust;

        /// Emitted for every note the player fires, for visualization.
        #[qsignal]
        fn note_fired(self: Pin<&mut PlayerBridge>, note: i32, chord: QString);
    }

    unsafe extern "RustQt" {
        #[qinvokable]
        fn load_file(self: Pin<&mut PlayerBridge>, path: &QString);
        #[qinvokable]
        fn play(self: Pin<&mut PlayerBridge>);
        #[qinvokable]
        fn pause(self: Pin<&mut PlayerBridge>);
        #[qinvokable]
        fn stop(self: Pin<&mut PlayerBridge>);
        #[qinvokable]
        fn toggle_play_pause(self: Pin<&mut PlayerBridge>);
        #[qinvokable]
        fn seek_to(self: Pin<&mut PlayerBridge>, secs: f64);
        #[qinvokable]
        fn go_live(self: Pin<&mut PlayerBridge>, on: bool);
        #[qinvokable]
        fn apply_speed(self: Pin<&mut PlayerBridge>, value: f64);
        #[qinvokable]
        fn select_song(self: Pin<&mut PlayerBridge>, index: i32);
        #[qinvokable]
        fn remove_song(self: Pin<&mut PlayerBridge>, index: i32);
        #[qinvokable]
        fn add_folder(self: Pin<&mut PlayerBridge>, path: &QString);
        #[qinvokable]
        fn poll(self: Pin<&mut PlayerBridge>);
    }
}

use core::pin::Pin;
use cxx_qt_lib::{QString, QStringList};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::Duration;
use wwm_engine::midi::{load_file, NoteKind};
use wwm_player::{Command, Player, PlayerEvent};

/// Directories scanned for MIDI files, in addition to any folders the user adds.
fn default_scan_dirs() -> Vec<std::path::PathBuf> {
    let mut dirs = vec![
        std::path::PathBuf::from(expand_home("~/Projects/test-midi-files")),
        std::path::PathBuf::from(expand_home("~/Music")),
    ];
    // De-duplicate nonexistent dirs cheaply; scanning ignores missing paths.
    dirs.dedup();
    dirs
}

fn expand_home(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return format!("{}/{rest}", home.to_string_lossy());
        }
    }
    path.to_string()
}

pub struct PlayerBridgeRust {
    file_name: QString,
    status: QString,
    duration: f64,
    position: f64,
    playing: bool,
    paused: bool,
    live: bool,
    game_running: bool,
    game_detected_flag: Arc<AtomicBool>,
    songs: QStringList,
    song_paths: QStringList,
    current_index: i32,
    speed: f64,
    note_count: i32,
    transpose: i32,
    bpm: i32,
    loaded: bool,
    player: Player,
    events: Receiver<PlayerEvent>,
}

impl Default for PlayerBridgeRust {
    fn default() -> Self {
        let (player, events) = Player::spawn();
        let (songs, song_paths) = scan_dirs(&default_scan_dirs());

        // Watch for the game process in the background so the Go Live button
        // can be enabled/disabled without blocking the GUI.
        let game_detected_flag = Arc::new(AtomicBool::new(false));
        spawn_game_watcher(Arc::clone(&game_detected_flag));

        Self {
            file_name: QString::from(""),
            status: QString::from("No file loaded"),
            duration: 0.0,
            position: 0.0,
            playing: false,
            paused: false,
            live: false,
            game_running: false,
            game_detected_flag,
            songs,
            song_paths,
            current_index: -1,
            speed: 1.0,
            note_count: 0,
            transpose: 0,
            bpm: 0,
            loaded: false,
            player,
            events,
        }
    }
}

/// Game detection looks for the game's process. Needles must not match this
/// app's own binary or directory names (which contain "wwm").
const GAME_KEYWORDS: &[&str] = &["where winds meet", "winds meet"];

/// The Go Live button's three visual states.
#[cfg_attr(test, derive(Debug, PartialEq, Eq))]
pub enum LiveColor {
    Gray,
    Green,
    Red,
}

/// Color of the Go Live button given game-detection and live state.
pub fn live_button_color(game_running: bool, live: bool) -> LiveColor {
    if !game_running {
        LiveColor::Gray
    } else if live {
        LiveColor::Red
    } else {
        LiveColor::Green
    }
}

/// The Go Live button is only interactive while the game is running.
pub fn live_button_enabled(game_running: bool) -> bool {
    game_running
}

/// Remove a leading `file://` URL scheme, returning a plain path.
pub fn strip_file_url(path: &str) -> &str {
    path.strip_prefix("file://").unwrap_or(path)
}

/// True when a cmdline belongs to one of our own binaries.
fn process_is_ours(cmdline: &str) -> bool {
    cmdline.contains("wwm-midi-player")
        || cmdline.contains("wwm-gui")
        || cmdline.contains("wwm-cli")
}

fn detect_game() -> bool {
    detect_game_at(Path::new("/proc"))
}

fn detect_game_at(proc: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(proc) else {
        return false;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if !name.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        let cmdline_path = entry.path().join("cmdline");
        let Ok(bytes) = std::fs::read(&cmdline_path) else {
            continue;
        };
        let cmdline = String::from_utf8_lossy(&bytes).to_lowercase();
        if process_is_ours(&cmdline) {
            continue;
        }
        for keyword in GAME_KEYWORDS {
            if cmdline.contains(keyword) {
                return true;
            }
        }
    }
    false
}

fn spawn_game_watcher(detected: Arc<AtomicBool>) {
    std::thread::Builder::new()
        .name("wwm-game-watch".into())
        .spawn(move || loop {
            detected.store(detect_game(), Ordering::Relaxed);
            std::thread::sleep(Duration::from_secs(2));
        })
        .expect("failed to spawn game watcher");
}

/// Scan directories (one level deep) for `.mid` / `.midi` files.
fn scan_dirs(dirs: &[std::path::PathBuf]) -> (QStringList, QStringList) {
    let mut names: Vec<(String, String)> = Vec::new();
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let is_midi = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| matches!(e.to_ascii_lowercase().as_str(), "mid" | "midi"))
                .unwrap_or(false);
            if !is_midi {
                continue;
            }
            let name = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            names.push((name, path.to_string_lossy().to_string()));
        }
    }
    names.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));

    let names_list: Vec<QString> = names
        .iter()
        .map(|(n, _)| QString::from(n.as_str()))
        .collect();
    let paths_list: Vec<QString> = names
        .iter()
        .map(|(_, p)| QString::from(p.as_str()))
        .collect();
    (
        QStringList::from_iter(names_list.iter()),
        QStringList::from_iter(paths_list.iter()),
    )
}

impl qobject::PlayerBridge {
    /// Load a `.mid` file. Accepts either a plain path or a `file://` URL.
    pub fn load_file(mut self: Pin<&mut Self>, path: &QString) {
        let raw = path.to_string();
        let path_str = raw.strip_prefix("file://").unwrap_or(&raw).to_string();

        match load_file(&path_str) {
            Ok(song) => {
                let notes = song
                    .events
                    .iter()
                    .filter(|e| e.kind == NoteKind::NoteOn)
                    .count() as i32;
                let bpm = (60_000_000 / song.tempo_us_per_qn) as i32;
                let transpose = song.transpose;
                let duration = song.duration_secs;
                let name = std::path::Path::new(&path_str)
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| path_str.clone());

                self.player.load(song);

                self.as_mut().set_file_name(QString::from(name.as_str()));
                self.as_mut().set_note_count(notes);
                self.as_mut().set_bpm(bpm);
                self.as_mut().set_transpose(transpose);
                self.as_mut().set_duration(duration);
                self.as_mut().set_position(0.0);
                self.as_mut().set_loaded(true);
                self.as_mut().set_status(QString::from("Ready"));
            }
            Err(e) => {
                self.as_mut().set_loaded(false);
                self.as_mut()
                    .set_status(QString::from(format!("Error: {e}").as_str()));
            }
        }
    }

    /// Load the song at `index` in the playlist.
    pub fn select_song(mut self: Pin<&mut Self>, index: i32) {
        let Some(path) = self.song_paths.get(index as isize) else {
            return;
        };
        let path = path.to_string();
        self.as_mut().set_current_index(index);
        self.as_mut().load_file(&QString::from(path.as_str()));
    }

    /// Remove the song at `index` from the playlist.
    pub fn remove_song(mut self: Pin<&mut Self>, index: i32) {
        let mut names: Vec<QString> = self.songs.iter().map(QString::clone).collect();
        let mut paths: Vec<QString> = self.song_paths.iter().map(QString::clone).collect();
        let i = index as usize;
        if i < names.len() {
            names.remove(i);
            paths.remove(i);
        }
        self.as_mut().set_songs(QStringList::from_iter(names.iter()));
        self.as_mut().set_song_paths(QStringList::from_iter(paths.iter()));

        // Keep the "now playing" highlight correct after removal.
        let current = self.current_index;
        if i == current as usize {
            self.as_mut().set_current_index(-1);
        } else if i < current as usize {
            self.as_mut().set_current_index(current - 1);
        }
    }

    /// Add a folder to the playlist (one level deep, .mid/.midi files).
    pub fn add_folder(mut self: Pin<&mut Self>, path: &QString) {
        let raw = path.to_string();
        let path_str = raw.strip_prefix("file://").unwrap_or(&raw).to_string();
        let (songs, song_paths) = scan_dirs(&[std::path::PathBuf::from(path_str)]);
        self.as_mut().set_songs(songs);
        self.as_mut().set_song_paths(song_paths);
        self.as_mut().set_current_index(-1);
    }

    pub fn play(self: Pin<&mut Self>) {
        self.player.play();
    }

    pub fn pause(self: Pin<&mut Self>) {
        self.player.pause();
    }

    pub fn stop(self: Pin<&mut Self>) {
        self.player.stop();
    }

    pub fn toggle_play_pause(self: Pin<&mut Self>) {
        self.player.toggle_play_pause();
    }

    pub fn seek_to(self: Pin<&mut Self>, secs: f64) {
        self.player.seek(secs);
    }

    /// Toggle input injection into the game.
    pub fn go_live(self: Pin<&mut Self>, on: bool) {
        self.player.set_live(on);
    }

    pub fn apply_speed(mut self: Pin<&mut Self>, value: f64) {
        self.player.send(Command::SetSpeed(value));
        self.as_mut().set_speed(value);
    }

    /// Drain the player's event stream and mirror state into Qt properties.
    /// Called from a QML timer.
    pub fn poll(mut self: Pin<&mut Self>) {
        let mut drained = Vec::new();
        while let Ok(event) = self.events.try_recv() {
            drained.push(event);
        }

        for event in drained {
            match event {
                PlayerEvent::Note { note, chord, .. } => {
                    let chord = QString::from(chord.to_string().as_str());
                    self.as_mut().note_fired(note as i32, chord);
                }
                PlayerEvent::Started | PlayerEvent::Resumed => {
                    self.as_mut().set_status(QString::from("Playing"));
                }
                PlayerEvent::Paused => self.as_mut().set_status(QString::from("Paused")),
                PlayerEvent::Stopped => self.as_mut().set_status(QString::from("Stopped")),
                PlayerEvent::Finished => self.as_mut().set_status(QString::from("Finished")),
                PlayerEvent::Live(on) => {
                    let text = if on {
                        "LIVE - sending input to the game"
                    } else {
                        "Not live - MIDI player only"
                    };
                    self.as_mut().set_status(QString::from(text));
                }
                PlayerEvent::Error(e) => {
                    self.as_mut()
                        .set_status(QString::from(format!("Error: {e}").as_str()));
                }
                PlayerEvent::Loaded { .. } | PlayerEvent::Position(_) => {}
            }
        }

        // Mirror the lock-free state snapshot.
        let state = self.player.state();
        let position = state.position_secs();
        let playing = state.is_playing();
        let paused = state.is_paused();
        let live = state.is_live();

        if (self.position - position).abs() > f64::EPSILON {
            self.as_mut().set_position(position);
        }
        if self.playing != playing {
            self.as_mut().set_playing(playing);
        }
        if self.paused != paused {
            self.as_mut().set_paused(paused);
        }
        if self.live != live {
            self.as_mut().set_live(live);
        }

        // Mirror game detection.
        let detected = self.game_detected_flag.load(Ordering::Relaxed);
        if self.game_running != detected {
            self.as_mut().set_game_running(detected);
        }

        // Can't inject into a game that isn't running — drop live if the game
        // went away while we were live.
        if !detected && live {
            self.player.set_live(false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{live_button_color, live_button_enabled, strip_file_url, LiveColor};

    #[test]
    fn live_button_color_is_gray_without_game() {
        assert_eq!(live_button_color(false, false), LiveColor::Gray);
        assert_eq!(live_button_color(false, true), LiveColor::Gray);
    }

    #[test]
    fn live_button_color_reflects_state_when_game_running() {
        assert_eq!(live_button_color(true, false), LiveColor::Green);
        assert_eq!(live_button_color(true, true), LiveColor::Red);
    }

    #[test]
    fn live_button_enabled_only_when_game_running() {
        assert!(!live_button_enabled(false));
        assert!(live_button_enabled(true));
    }

    #[test]
    fn strips_file_url_scheme() {
        assert_eq!(strip_file_url("file:///home/x/a.mid"), "/home/x/a.mid");
        assert_eq!(strip_file_url("/home/x/a.mid"), "/home/x/a.mid");
    }

    #[test]
    fn our_own_processes_are_excluded_from_game_detection() {
        use super::process_is_ours;
        assert!(process_is_ours(
            "/home/x/wwm-midi-player-linux/target/debug/wwm-gui"
        ));
        assert!(process_is_ours("wwm-cli play song.mid"));
        assert!(!process_is_ours(
            "/games/steamapps/common/Where Winds Meet/wwm.exe"
        ));
    }

    #[test]
    fn fake_proc_detects_game_and_ignores_ours() {
        use super::detect_game_at;
        let dir = std::env::temp_dir().join(format!("wwm-fake-proc-{}", std::process::id()));
        // Our own GUI process must not count as the game.
        let ours = dir.join("100");
        std::fs::create_dir_all(&ours).unwrap();
        std::fs::write(
            ours.join("cmdline"),
            "/home/x/wwm-midi-player-linux/target/debug/wwm-gui",
        )
        .unwrap();
        assert!(!detect_game_at(&dir));
        // A real game process is detected.
        let game = dir.join("200");
        std::fs::create_dir_all(&game).unwrap();
        std::fs::write(game.join("cmdline"), "Z:\\Where Winds Meet\\wwm.exe").unwrap();
        assert!(detect_game_at(&dir));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
