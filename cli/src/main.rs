//! CLI front-end for the Wayland MIDI player.
//!
//! Subcommands:
//! - `inspect` — parse a `.mid` file and print the translated timed key events.
//! - `play`   — parse and inject the events through `/dev/uinput` in real time.
//! - `hotkeys`— listen for Play/Pause and Stop global shortcuts via the portal.

use clap::{Parser, Subcommand, ValueEnum};
use futures_util::StreamExt;
use std::process::ExitCode;
use std::time::Duration;
use wwm_engine::mapping::{map_note, NoteMode};
use wwm_engine::midi::{load_file, NoteKind};
use wwm_hotkeys::TransportCommand;
use wwm_player::{Command as PlayerCommand, Player, PlayerEvent};

/// A minimal MIDI player core for native Linux.
#[derive(Debug, Parser)]
#[command(name = "wwm", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Parse a MIDI file and print the translated timed key events.
    Inspect {
        #[arg(value_name = "FILE")]
        file: String,
        #[command(flatten)]
        options: TranslateOptions,
        /// Only print note-on events (skip note-off).
        #[arg(long)]
        notes_only: bool,
        /// Max events to print (default: all).
        #[arg(short, long)]
        limit: Option<usize>,
    },
    /// Play a MIDI file. Acts as a plain MIDI player unless --live is set.
    Play {
        #[arg(value_name = "FILE")]
        file: String,
        #[command(flatten)]
        options: TranslateOptions,
        /// Playback speed multiplier (1.0 = normal, 2.0 = double speed).
        #[arg(long, default_value_t = 1.0)]
        speed: f64,
        /// Hold duration per key press, in milliseconds.
        #[arg(long, default_value_t = 0)]
        hold: u64,
        /// Go live: inject keystrokes into /dev/uinput. Off by default.
        #[arg(long)]
        live: bool,
        /// Register global hotkeys (Play/Pause, Stop) and keep running.
        #[arg(long)]
        hotkeys: bool,
        /// Log each emitted key to stdout.
        #[arg(short, long)]
        verbose: bool,
    },
    /// Listen for Play/Pause and Stop global shortcuts and print them.
    Hotkeys,
}

#[derive(Debug, clap::Args)]
struct TranslateOptions {
    /// Note-to-key mode.
    #[arg(short, long, value_enum, default_value_t = NoteModeArg::Closest)]
    mode: NoteModeArg,
    /// Manual transpose in semitones (overrides auto-detection).
    #[arg(short, long)]
    transpose: Option<i32>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum NoteModeArg {
    Closest,
    Raw,
}

impl From<NoteModeArg> for NoteMode {
    fn from(a: NoteModeArg) -> Self {
        match a {
            NoteModeArg::Closest => NoteMode::Closest,
            NoteModeArg::Raw => NoteMode::Raw,
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Inspect {
            file,
            options,
            notes_only,
            limit,
        } => cmd_inspect(&file, &options, notes_only, limit),
        Command::Play {
            file,
            options,
            speed,
            hold,
            live,
            hotkeys,
            verbose,
        } => cmd_play(&file, &options, speed, hold, live, hotkeys, verbose).await,
        Command::Hotkeys => cmd_hotkeys().await,
    }
}

fn cmd_inspect(
    file: &str,
    options: &TranslateOptions,
    notes_only: bool,
    limit: Option<usize>,
) -> ExitCode {
    let song = match load_file(file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    let transpose = options.transpose.unwrap_or(song.transpose);
    let mode: NoteMode = options.mode.into();

    eprintln!(
        "loaded {} ({} events, {:.2}s, {} BPM)",
        file,
        song.events.len(),
        song.duration_secs,
        60_000_000 / song.tempo_us_per_qn
    );
    eprintln!("transpose: {} semitones", transpose);

    for event in song
        .events
        .iter()
        .filter(|e| !notes_only || e.kind == NoteKind::NoteOn)
        .take(limit.unwrap_or(usize::MAX))
    {
        let key = map_note(mode, event.note, transpose);
        let verb = match event.kind {
            NoteKind::NoteOn => "press",
            NoteKind::NoteOff => "release",
        };
        println!(
            "{:>9.3}s  track={:<2}  note={:<3}  {:<8}  {}",
            event.time_ms as f64 / 1000.0,
            event.track_id,
            event.note,
            key,
            verb,
        );
    }

    ExitCode::SUCCESS
}

async fn cmd_play(
    file: &str,
    options: &TranslateOptions,
    speed: f64,
    hold: u64,
    live: bool,
    hotkeys: bool,
    verbose: bool,
) -> ExitCode {
    let song = match load_file(file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    eprintln!(
        "loaded {} ({} notes, {:.2}s, {} BPM, speed {:.2}x)",
        file,
        song.events
            .iter()
            .filter(|e| e.kind == NoteKind::NoteOn)
            .count(),
        song.duration_secs,
        60_000_000 / song.tempo_us_per_qn,
        speed
    );
    if !live {
        eprintln!("not live: playing as a MIDI player only (pass --live to inject input)");
    }

    let (player, events) = Player::spawn();
    player.send(PlayerCommand::SetSpeed(speed));
    player.send(PlayerCommand::SetHold(Duration::from_millis(hold)));
    player.send(PlayerCommand::SetMode(options.mode.into()));
    player.send(PlayerCommand::SetTranspose(options.transpose));
    player.load(song);
    if live {
        player.set_live(true);
    }

    if hotkeys {
        let commands = player.sender();
        tokio::spawn(async move {
            let shortcuts = match wwm_hotkeys::PlayerShortcuts::register().await {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("hotkeys unavailable: {e}");
                    return;
                }
            };
            let mut stream = match shortcuts.activated().await {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("hotkeys unavailable: {e}");
                    return;
                }
            };
            while let Some(event) = stream.next().await {
                let command = match wwm_hotkeys::command_for(event.shortcut_id()) {
                    Some(TransportCommand::PlayPause) => PlayerCommand::TogglePlayPause,
                    Some(TransportCommand::Stop) => PlayerCommand::Stop,
                    None => continue,
                };
                if commands.send(command).is_err() {
                    break;
                }
            }
        });
    }

    player.play();

    // Without hotkeys the CLI exits once the song ends; with hotkeys it stays
    // running so the transport controls remain usable.
    let stay_resident = hotkeys;
    let drain = tokio::task::spawn_blocking(move || {
        while let Ok(event) = events.recv() {
            match event {
                PlayerEvent::Note {
                    note,
                    chord,
                    time_ms,
                } => {
                    if verbose {
                        println!(
                            "{:>9.3}s  note={:<3}  {}",
                            time_ms as f64 / 1000.0,
                            note,
                            chord
                        );
                    }
                }
                PlayerEvent::Started => eprintln!("[player] started"),
                PlayerEvent::Paused => eprintln!("[player] paused"),
                PlayerEvent::Resumed => eprintln!("[player] resumed"),
                PlayerEvent::Stopped => eprintln!("[player] stopped"),
                PlayerEvent::Live(on) => {
                    eprintln!("[player] live: {}", if on { "ON" } else { "OFF" })
                }
                PlayerEvent::Error(e) => eprintln!("[player] error: {e}"),
                PlayerEvent::Finished => {
                    eprintln!("[player] finished");
                    if !stay_resident {
                        break;
                    }
                }
                PlayerEvent::Loaded { .. } | PlayerEvent::Position(_) => {}
            }
        }
    });

    let _ = drain.await;
    ExitCode::SUCCESS
}

async fn cmd_hotkeys() -> ExitCode {
    let player = match wwm_hotkeys::PlayerShortcuts::register().await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    let mut events = match player.activated().await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    eprintln!("listening for global shortcuts...");
    while let Some(evt) = events.next().await {
        if let Some(cmd) = wwm_hotkeys::command_for(evt.shortcut_id()) {
            println!("[{cmd}]");
        } else {
            println!("[hotkey: unknown id '{}']", evt.shortcut_id());
        }
    }

    ExitCode::SUCCESS
}
