//! CLI front-end for the Wayland MIDI player.
//!
//! Subcommands:
//! - `inspect` — parse a `.mid` file and print the translated timed key events.
//! - `play`   — parse and inject the events through `/dev/uinput` in real time.

use clap::{Parser, Subcommand, ValueEnum};
use std::process::ExitCode;
use std::time::{Duration, Instant};
use wwm_engine::mapping::{map_note, NoteMode};
use wwm_engine::midi::{load_file, NoteKind};

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
    /// Inject a MIDI file through a virtual /dev/uinput keyboard in real time.
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
        /// Log each emitted key to stdout.
        #[arg(short, long)]
        verbose: bool,
    },
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

fn main() -> ExitCode {
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
            verbose,
        } => cmd_play(&file, &options, speed, hold, verbose),
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

fn cmd_play(
    file: &str,
    options: &TranslateOptions,
    speed: f64,
    hold: u64,
    verbose: bool,
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

    let mut keyboard = match wwm_input::VirtualKeyboard::create("wwm-midi-player") {
        Ok(k) => k,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    keyboard.set_hold(Duration::from_millis(hold));

    eprintln!(
        "playing {} ({} notes, {:.2}s, {} BPM, speed {:.2}x)",
        file,
        song.events
            .iter()
            .filter(|e| e.kind == NoteKind::NoteOn)
            .count(),
        song.duration_secs,
        60_000_000 / song.tempo_us_per_qn,
        speed
    );

    let speed = speed.clamp(0.05, 16.0);
    let start = Instant::now();

    for event in &song.events {
        if event.kind != NoteKind::NoteOn {
            continue;
        }

        let target = Duration::from_secs_f64(event.time_ms as f64 / 1000.0 / speed);
        if let Some(remaining) = target.checked_sub(start.elapsed()) {
            std::thread::sleep(remaining);
        }

        let chord = map_note(mode, event.note, transpose);
        if let Err(e) = keyboard.tap(chord) {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }

        if verbose {
            println!(
                "{:>9.3}s  note={:<3}  {}",
                event.time_ms as f64 / 1000.0,
                event.note,
                chord
            );
        }
    }

    eprintln!("done");
    ExitCode::SUCCESS
}
