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
        fn poll(self: Pin<&mut PlayerBridge>);
    }
}

use core::pin::Pin;
use cxx_qt_lib::QString;
use std::sync::mpsc::Receiver;
use wwm_engine::midi::{load_file, NoteKind};
use wwm_player::{Command, Player, PlayerEvent};

pub struct PlayerBridgeRust {
    file_name: QString,
    status: QString,
    duration: f64,
    position: f64,
    playing: bool,
    paused: bool,
    live: bool,
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
        Self {
            file_name: QString::from(""),
            status: QString::from("No file loaded"),
            duration: 0.0,
            position: 0.0,
            playing: false,
            paused: false,
            live: false,
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
    }
}
