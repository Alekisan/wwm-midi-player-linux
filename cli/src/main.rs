//! Phase 1 CLI: parse a `.mid` file, translate pitches to 36-key mappings, and
//! print the resulting timed events. Playback/injection (Phase 2+) is not wired
//! up yet; this command validates the core engine end-to-end.

use clap::Parser;
use std::process::ExitCode;
use wwm_engine::mapping::{map_note, NoteMode};
use wwm_engine::midi::NoteKind;

/// A minimal MIDI player core for native Linux (Phase 1: parse + translate).
#[derive(Debug, Parser)]
#[command(name = "wwm", version, about)]
struct Args {
    /// Path to the `.mid` file to parse.
    #[arg(value_name = "FILE")]
    file: String,

    /// Note-to-key mode.
    #[arg(short, long, value_enum, default_value_t = NoteModeArg::Closest)]
    mode: NoteModeArg,

    /// Manual transpose in semitones (overrides auto-detection).
    #[arg(short, long)]
    transpose: Option<i32>,

    /// Only print note-on events (skip note-off).
    #[arg(long)]
    notes_only: bool,

    /// Max events to print (default: all).
    #[arg(short, long)]
    limit: Option<usize>,
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
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
    let args = Args::parse();

    let song = match wwm_engine::midi::load_file(&args.file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    let transpose = args
        .transpose
        .unwrap_or(song.transpose);

    eprintln!(
        "loaded {} ({} events, {:.2}s, {} BPM)",
        args.file,
        song.events.len(),
        song.duration_secs,
        60_000_000 / song.tempo_us_per_qn
    );
    eprintln!("transpose: {} semitones", transpose);

    let mode: NoteMode = args.mode.into();

    let events: Vec<_> = song
        .events
        .iter()
        .filter(|e| !args.notes_only || e.kind == NoteKind::NoteOn)
        .collect();

    for event in events.iter().take(args.limit.unwrap_or(usize::MAX)) {
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
