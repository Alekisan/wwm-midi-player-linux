//! Decoupled playback engine.
//!
//! Owns a dedicated timing thread that walks a [`Song`]'s note events in real
//! time, maps each note to a key chord, and (when "live") injects it through the
//! virtual keyboard. It has no GUI dependency: the front-end drives it with
//! [`Command`]s and observes [`PlayerEvent`]s.
//!
//! Per the project's behavior rules, the player is always usable as a MIDI
//! player. Input injection is gated behind an explicit "Go Live" toggle, and the
//! `/dev/uinput` device is only created the first time the user goes live — a
//! missing device never prevents playback or startup.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use wwm_engine::mapping::{map_note, KeyChord, NoteMode};
use wwm_engine::midi::{NoteKind, Song};
use wwm_input::VirtualKeyboard;

pub use wwm_preview_synth::Instrument;

/// How long the timing thread may sleep before re-checking for commands.
/// Bounds worst-case note lateness.
const MAX_SLEEP: Duration = Duration::from_micros(2_000);
/// How often position updates are emitted to the front-end.
const POSITION_INTERVAL: Duration = Duration::from_millis(50);

/// Shared, lock-free snapshot of the player's state, safe to poll from a UI.
#[derive(Debug, Default)]
pub struct PlayerState {
    playing: AtomicBool,
    paused: AtomicBool,
    live: AtomicBool,
    preview: AtomicBool,
    position_ms: AtomicU64,
    duration_ms: AtomicU64,
}

impl PlayerState {
    /// Playback is active (may still be paused).
    pub fn is_playing(&self) -> bool {
        self.playing.load(Ordering::SeqCst)
    }

    /// Playback is paused.
    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst)
    }

    /// Input injection to the game is enabled.
    pub fn is_live(&self) -> bool {
        self.live.load(Ordering::SeqCst)
    }

    /// Local audio preview is enabled.
    pub fn is_preview(&self) -> bool {
        self.preview.load(Ordering::SeqCst)
    }

    /// Current playback position, in seconds.
    pub fn position_secs(&self) -> f64 {
        self.position_ms.load(Ordering::SeqCst) as f64 / 1000.0
    }

    /// Duration of the loaded song, in seconds.
    pub fn duration_secs(&self) -> f64 {
        self.duration_ms.load(Ordering::SeqCst) as f64 / 1000.0
    }
}

/// Commands accepted by the playback thread.
#[derive(Debug)]
pub enum Command {
    /// Replace the loaded song (stops any current playback).
    Load(Box<Song>),
    Play,
    Pause,
    Resume,
    /// Play if stopped, pause if playing, resume if paused.
    TogglePlayPause,
    Stop,
    /// Seek to an absolute position, in seconds.
    Seek(f64),
    /// Playback speed multiplier (1.0 = normal).
    SetSpeed(f64),
    SetMode(NoteMode),
    /// Override the auto-detected transpose; `None` restores auto.
    SetTranspose(Option<i32>),
    /// Key hold duration per note.
    SetHold(Duration),
    /// Enable/disable input injection ("Go Live").
    SetLive(bool),
    /// Enable/disable local audio preview.
    SetPreview(bool),
    /// Switch the preview instrument.
    SetInstrument(Instrument),
    Shutdown,
}

/// Notifications emitted by the playback thread.
#[derive(Debug, Clone)]
pub enum PlayerEvent {
    Loaded {
        duration_secs: f64,
        notes: usize,
    },
    Started,
    Paused,
    Resumed,
    Stopped,
    Finished,
    /// A note fired; carries the mapped chord for visualization.
    Note {
        note: u8,
        chord: KeyChord,
        time_ms: u64,
    },
    /// Playback position, in seconds.
    Position(f64),
    /// "Go Live" state changed.
    Live(bool),
    /// Audio preview state changed.
    Preview(bool),
    /// Preview instrument changed.
    Instrument(Instrument),
    /// Non-fatal error (e.g. uinput unavailable when going live).
    Error(String),
}

/// Handle to a running playback thread.
pub struct Player {
    commands: Sender<Command>,
    state: Arc<PlayerState>,
    thread: Option<JoinHandle<()>>,
}

impl Player {
    /// Start the playback thread, returning the handle and the event stream.
    pub fn spawn() -> (Player, Receiver<PlayerEvent>) {
        let (cmd_tx, cmd_rx) = channel();
        let (evt_tx, evt_rx) = channel();
        let state = Arc::new(PlayerState::default());

        // Spawn the preview synth and forward its status messages as player events.
        let (preview, preview_status) = wwm_preview_synth::Preview::spawn();
        let status_tx = evt_tx.clone();
        thread::Builder::new()
            .name("wwm-preview-status".into())
            .spawn(move || {
                for message in preview_status {
                    let _ = status_tx.send(PlayerEvent::Error(message));
                }
            })
            .expect("failed to spawn preview status thread");

        let worker_state = Arc::clone(&state);
        let thread = thread::Builder::new()
            .name("wwm-playback".into())
            .spawn(move || {
                Worker {
                    song: None,
                    keyboard: None,
                    preview,
                    state: worker_state,
                    events: evt_tx,
                    commands: cmd_rx,
                    index: 0,
                    anchor: Instant::now(),
                    anchor_song_ms: 0.0,
                    speed: 1.0,
                    mode: NoteMode::Closest,
                    transpose_override: None,
                    hold: Duration::ZERO,
                    instrument: Instrument::Guqin,
                    last_position_emit: Instant::now(),
                }
                .run();
            })
            .expect("failed to spawn playback thread");

        (
            Player {
                commands: cmd_tx,
                state,
                thread: Some(thread),
            },
            evt_rx,
        )
    }

    /// Shared state snapshot, cheap to poll.
    pub fn state(&self) -> Arc<PlayerState> {
        Arc::clone(&self.state)
    }

    /// A cloneable command sender, e.g. for the global-hotkey listener.
    pub fn sender(&self) -> Sender<Command> {
        self.commands.clone()
    }

    /// Send a command to the playback thread.
    pub fn send(&self, command: Command) {
        let _ = self.commands.send(command);
    }

    pub fn load(&self, song: Song) {
        self.send(Command::Load(Box::new(song)));
    }
    pub fn play(&self) {
        self.send(Command::Play);
    }
    pub fn pause(&self) {
        self.send(Command::Pause);
    }
    pub fn stop(&self) {
        self.send(Command::Stop);
    }
    pub fn toggle_play_pause(&self) {
        self.send(Command::TogglePlayPause);
    }
    pub fn seek(&self, secs: f64) {
        self.send(Command::Seek(secs));
    }
    pub fn set_live(&self, live: bool) {
        self.send(Command::SetLive(live));
    }
    pub fn set_preview(&self, preview: bool) {
        self.send(Command::SetPreview(preview));
    }
    pub fn set_instrument(&self, instrument: Instrument) {
        self.send(Command::SetInstrument(instrument));
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct Worker {
    song: Option<Song>,
    keyboard: Option<VirtualKeyboard>,
    preview: wwm_preview_synth::Preview,
    state: Arc<PlayerState>,
    events: Sender<PlayerEvent>,
    commands: Receiver<Command>,
    index: usize,
    anchor: Instant,
    anchor_song_ms: f64,
    speed: f64,
    mode: NoteMode,
    transpose_override: Option<i32>,
    hold: Duration,
    instrument: Instrument,
    last_position_emit: Instant,
}

impl Worker {
    fn run(mut self) {
        loop {
            let active = self.state.is_playing() && !self.state.is_paused();

            if !active {
                // Idle: block until the next command arrives.
                match self.commands.recv() {
                    Ok(command) => {
                        if !self.handle(command) {
                            break;
                        }
                    }
                    Err(_) => break,
                }
                continue;
            }

            // Playing: drain pending commands without blocking, then advance.
            loop {
                match self.commands.try_recv() {
                    Ok(command) => {
                        if !self.handle(command) {
                            self.release_keys();
                            return;
                        }
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        self.release_keys();
                        return;
                    }
                }
            }

            // A command may have paused or stopped us; re-check before
            // advancing, otherwise `tick` would overwrite the new position.
            if !self.state.is_playing() || self.state.is_paused() {
                continue;
            }
            self.tick();
        }
        self.release_keys();
    }

    /// Returns `false` when the thread should shut down.
    fn handle(&mut self, command: Command) -> bool {
        match command {
            Command::Load(song) => {
                self.stop_playback(false);
                let notes = song
                    .events
                    .iter()
                    .filter(|e| e.kind == NoteKind::NoteOn)
                    .count();
                let duration_secs = song.duration_secs;
                self.state
                    .duration_ms
                    .store((duration_secs * 1000.0) as u64, Ordering::SeqCst);
                self.state.position_ms.store(0, Ordering::SeqCst);
                self.song = Some(*song);
                self.index = 0;
                self.emit(PlayerEvent::Loaded {
                    duration_secs,
                    notes,
                });
            }
            Command::Play => self.start(),
            Command::Pause => self.pause(),
            Command::Resume => self.resume(),
            Command::TogglePlayPause => {
                if !self.state.is_playing() {
                    self.start();
                } else if self.state.is_paused() {
                    self.resume();
                } else {
                    self.pause();
                }
            }
            Command::Stop => self.stop_playback(true),
            Command::Seek(secs) => self.seek(secs),
            Command::SetSpeed(speed) => {
                // Re-anchor so the change applies from the current position.
                self.anchor_song_ms = self.current_song_ms();
                self.anchor = Instant::now();
                self.speed = speed.clamp(0.05, 16.0);
            }
            Command::SetMode(mode) => self.mode = mode,
            Command::SetTranspose(transpose) => self.transpose_override = transpose,
            Command::SetHold(hold) => {
                self.hold = hold;
                if let Some(keyboard) = self.keyboard.as_mut() {
                    keyboard.set_hold(hold);
                }
            }
            Command::SetLive(live) => self.set_live(live),
            Command::SetPreview(preview) => {
                self.state.preview.store(preview, Ordering::SeqCst);
                if !preview {
                    self.preview.all_notes_off();
                }
                self.emit(PlayerEvent::Preview(preview));
            }
            Command::SetInstrument(instrument) => {
                self.instrument = instrument;
                self.preview.set_instrument(instrument);
                self.emit(PlayerEvent::Instrument(instrument));
            }
            Command::Shutdown => return false,
        }
        true
    }

    fn set_live(&mut self, live: bool) {
        if !live {
            self.state.live.store(false, Ordering::SeqCst);
            self.release_keys();
            self.emit(PlayerEvent::Live(false));
            return;
        }

        // Lazily create the virtual keyboard on the first "Go Live".
        if self.keyboard.is_none() {
            match VirtualKeyboard::create("wwm-midi-player") {
                Ok(mut keyboard) => {
                    keyboard.set_hold(self.hold);
                    self.keyboard = Some(keyboard);
                }
                Err(e) => {
                    self.state.live.store(false, Ordering::SeqCst);
                    self.emit(PlayerEvent::Error(e.to_string()));
                    self.emit(PlayerEvent::Live(false));
                    return;
                }
            }
        }

        self.state.live.store(true, Ordering::SeqCst);
        self.emit(PlayerEvent::Live(true));
    }

    fn start(&mut self) {
        if self.song.is_none() {
            self.emit(PlayerEvent::Error("no song loaded".into()));
            return;
        }
        // Restart from the beginning if we're sitting at the end.
        if self.index >= self.song.as_ref().map_or(0, |s| s.events.len()) {
            self.index = 0;
            self.state.position_ms.store(0, Ordering::SeqCst);
        }
        self.anchor_song_ms = self.state.position_ms.load(Ordering::SeqCst) as f64;
        self.anchor = Instant::now();
        self.state.playing.store(true, Ordering::SeqCst);
        self.state.paused.store(false, Ordering::SeqCst);
        self.emit(PlayerEvent::Started);
    }

    fn pause(&mut self) {
        if !self.state.is_playing() || self.state.is_paused() {
            return;
        }
        let position = self.current_song_ms();
        self.state
            .position_ms
            .store(position.max(0.0) as u64, Ordering::SeqCst);
        self.state.paused.store(true, Ordering::SeqCst);
        self.release_keys();
        self.preview.all_notes_off();
        self.emit(PlayerEvent::Paused);
    }

    fn resume(&mut self) {
        if !self.state.is_playing() || !self.state.is_paused() {
            return;
        }
        self.anchor_song_ms = self.state.position_ms.load(Ordering::SeqCst) as f64;
        self.anchor = Instant::now();
        self.state.paused.store(false, Ordering::SeqCst);
        self.emit(PlayerEvent::Resumed);
    }

    fn stop_playback(&mut self, notify: bool) {
        let was_playing = self.state.is_playing();
        self.state.playing.store(false, Ordering::SeqCst);
        self.state.paused.store(false, Ordering::SeqCst);
        self.state.position_ms.store(0, Ordering::SeqCst);
        self.index = 0;
        self.release_keys();
        self.preview.all_notes_off();
        if notify && was_playing {
            self.emit(PlayerEvent::Stopped);
        }
    }

    fn seek(&mut self, secs: f64) {
        let target_ms = (secs.max(0.0) * 1000.0) as u64;
        self.index = match self.song.as_ref() {
            Some(song) => song.events.partition_point(|e| e.time_ms < target_ms),
            None => 0,
        };
        self.state.position_ms.store(target_ms, Ordering::SeqCst);
        self.anchor_song_ms = target_ms as f64;
        self.anchor = Instant::now();
        self.release_keys();
        self.preview.all_notes_off();
    }

    /// Song-time position implied by the wall clock since the last anchor.
    fn current_song_ms(&self) -> f64 {
        if self.state.is_paused() || !self.state.is_playing() {
            return self.state.position_ms.load(Ordering::SeqCst) as f64;
        }
        self.anchor_song_ms + self.anchor.elapsed().as_secs_f64() * 1000.0 * self.speed
    }

    fn tick(&mut self) {
        let now_song_ms =
            self.anchor_song_ms + self.anchor.elapsed().as_secs_f64() * 1000.0 * self.speed;

        let mut errors: Vec<String> = Vec::new();
        let finished;
        let next_time_ms;

        {
            let Some(song) = self.song.as_ref() else {
                self.state.playing.store(false, Ordering::SeqCst);
                return;
            };

            let transpose = self.transpose_override.unwrap_or(song.transpose);
            let mode = self.mode;
            let live = self.state.live.load(Ordering::SeqCst);
            let preview = self.state.preview.load(Ordering::SeqCst);

            while self.index < song.events.len() {
                let event = song.events[self.index];
                if (event.time_ms as f64) > now_song_ms {
                    break;
                }
                self.index += 1;

                // Audio preview is a simple note-on/note-off following the MIDI
                // velocity so it sounds like the piece, independent of the
                // game's "tap on note-on" key behavior.
                if preview {
                    match event.kind {
                        NoteKind::NoteOn => self.preview.note_on(event.note, 100),
                        NoteKind::NoteOff => self.preview.note_off(event.note),
                    }
                }

                if event.kind != NoteKind::NoteOn {
                    continue;
                }

                let chord = map_note(mode, event.note, transpose);
                if live {
                    if let Some(keyboard) = self.keyboard.as_mut() {
                        if let Err(e) = keyboard.tap(chord) {
                            errors.push(e.to_string());
                        }
                    }
                }
                let _ = self.events.send(PlayerEvent::Note {
                    note: event.note,
                    chord,
                    time_ms: event.time_ms,
                });
            }

            finished = self.index >= song.events.len();
            next_time_ms = if finished {
                0.0
            } else {
                song.events[self.index].time_ms as f64
            };
        }

        self.state
            .position_ms
            .store(now_song_ms.max(0.0) as u64, Ordering::SeqCst);

        for error in errors {
            self.emit(PlayerEvent::Error(error));
        }

        if self.last_position_emit.elapsed() >= POSITION_INTERVAL {
            self.last_position_emit = Instant::now();
            self.emit(PlayerEvent::Position(now_song_ms / 1000.0));
        }

        if finished {
            self.state.playing.store(false, Ordering::SeqCst);
            self.state.paused.store(false, Ordering::SeqCst);
            self.index = 0;
            self.release_keys();
            self.preview.all_notes_off();
            self.emit(PlayerEvent::Finished);
            return;
        }

        let wait_song_ms = (next_time_ms - now_song_ms).max(0.0);
        let wait = Duration::from_secs_f64(wait_song_ms / self.speed / 1000.0);
        thread::sleep(wait.min(MAX_SLEEP));
    }

    fn release_keys(&mut self) {
        if let Some(keyboard) = self.keyboard.as_mut() {
            let _ = keyboard.release_all();
        }
    }

    fn emit(&self, event: PlayerEvent) {
        let _ = self.events.send(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wwm_engine::midi::{NoteEvent, NoteKind};

    fn test_song(note_times: &[u64]) -> Song {
        Song {
            events: note_times
                .iter()
                .map(|&t| NoteEvent {
                    time_ms: t,
                    kind: NoteKind::NoteOn,
                    note: 60,
                    track_id: 0,
                })
                .collect(),
            duration_secs: note_times.last().copied().unwrap_or(0) as f64 / 1000.0,
            transpose: 0,
            tempo_us_per_qn: 500_000,
            ticks_per_quarter: 480,
        }
    }

    fn drain(rx: &Receiver<PlayerEvent>, timeout: Duration) -> Vec<PlayerEvent> {
        let deadline = Instant::now() + timeout;
        let mut out = Vec::new();
        while Instant::now() < deadline {
            match rx.recv_timeout(Duration::from_millis(20)) {
                Ok(e) => out.push(e),
                Err(_) => continue,
            }
        }
        out
    }

    #[test]
    fn loading_reports_duration_and_note_count() {
        let (player, events) = Player::spawn();
        player.load(test_song(&[0, 100, 200]));

        let received = drain(&events, Duration::from_millis(200));
        let loaded = received.iter().find_map(|e| match e {
            PlayerEvent::Loaded {
                duration_secs,
                notes,
            } => Some((*duration_secs, *notes)),
            _ => None,
        });
        assert_eq!(loaded, Some((0.2, 3)));
    }

    #[test]
    fn playback_fires_notes_and_finishes() {
        let (player, events) = Player::spawn();
        player.load(test_song(&[0, 50, 100]));
        player.play();

        let received = drain(&events, Duration::from_millis(500));
        let notes = received
            .iter()
            .filter(|e| matches!(e, PlayerEvent::Note { .. }))
            .count();
        assert_eq!(notes, 3);
        assert!(received.iter().any(|e| matches!(e, PlayerEvent::Finished)));
        assert!(!player.state().is_playing());
    }

    #[test]
    fn not_live_by_default() {
        let (player, _events) = Player::spawn();
        assert!(!player.state().is_live());
    }

    #[test]
    fn stop_resets_position() {
        let (player, events) = Player::spawn();
        player.load(test_song(&[0, 5_000]));
        player.play();
        thread::sleep(Duration::from_millis(80));
        player.stop();
        thread::sleep(Duration::from_millis(80));

        let _ = drain(&events, Duration::from_millis(100));
        assert!(!player.state().is_playing());
        assert_eq!(player.state().position_secs(), 0.0);
    }

    #[test]
    fn pause_halts_progress() {
        let (player, _events) = Player::spawn();
        player.load(test_song(&[0, 10_000]));
        player.play();
        thread::sleep(Duration::from_millis(100));
        player.pause();
        thread::sleep(Duration::from_millis(50));

        let first = player.state().position_secs();
        thread::sleep(Duration::from_millis(150));
        let second = player.state().position_secs();

        assert!(player.state().is_paused());
        assert_eq!(first, second, "position advanced while paused");
    }
}
