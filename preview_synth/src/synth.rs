//! Background audio renderer: runs a rustysynth synthesizer inside a rodio
//! [`Source`] on a dedicated thread, driven by [`AudioCommand`] messages.

use std::fs::File;
use std::io::BufReader;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rustysynth::{SoundFont, Synthesizer, SynthesizerSettings};

use crate::{resolve_soundfont, AudioCommand, Instrument, PreviewError};

/// The MIDI channel all preview notes play on.
const CHANNEL: i32 = 0;
/// MIDI program-change command byte.
const PROGRAM_CHANGE: i32 = 0xC0;
/// Samples rendered per block inside the synthesizer.
const BLOCK_SIZE: usize = 64;
/// How often the renderer wakes to drain pending commands.
const DRAIN_INTERVAL: Duration = Duration::from_millis(5);

/// Shared handle to the currently active synthesizer, swapped between the
/// command thread and the rodio audio callback.
type SharedSynth = Arc<Mutex<Option<Synthesizer>>>;

/// Handle to the running preview system. Dropping it shuts the renderer down.
pub struct Preview {
    commands: Sender<AudioCommand>,
    volume: Arc<std::sync::atomic::AtomicU32>, // f32 bits
}

impl Preview {
    /// Spawn the renderer. Returns the handle and a receiver for non-fatal
    /// status/error messages (e.g. a missing SoundFont).
    pub fn spawn() -> (Preview, Receiver<String>) {
        let (cmd_tx, cmd_rx) = channel::<AudioCommand>();
        let (status_tx, status_rx) = channel::<String>();
        let volume = Arc::new(std::sync::atomic::AtomicU32::new(f32::to_bits(1.0)));
        let volume_thread = Arc::clone(&volume);

        std::thread::Builder::new()
            .name("wwm-preview".into())
            .spawn(move || run(cmd_rx, status_tx, volume_thread))
            .expect("failed to spawn preview thread");

        (
            Preview {
                commands: cmd_tx,
                volume,
            },
            status_rx,
        )
    }

    /// Send a command to the renderer. Never blocks the caller.
    pub fn send(&self, command: AudioCommand) {
        let _ = self.commands.send(command);
    }

    pub fn note_on(&self, pitch: u8, velocity: u8) {
        self.send(AudioCommand::PlayNote { pitch, velocity });
    }

    pub fn note_off(&self, pitch: u8) {
        self.send(AudioCommand::StopNote { pitch });
    }

    pub fn set_instrument(&self, instrument: Instrument) {
        self.send(AudioCommand::SetInstrument(instrument));
    }

    pub fn set_volume(&self, volume: f32) {
        self.volume
            .store(f32::to_bits(volume.clamp(0.0, 1.0)), Ordering::Relaxed);
        self.send(AudioCommand::SetVolume(volume));
    }

    pub fn all_notes_off(&self) {
        self.send(AudioCommand::AllNotesOff);
    }

    pub fn shutdown(&self) {
        self.send(AudioCommand::Shutdown);
    }

    pub fn volume(&self) -> f32 {
        f32::from_bits(self.volume.load(Ordering::Relaxed))
    }
}

impl Drop for Preview {
    fn drop(&mut self) {
        let _ = self.commands.send(AudioCommand::Shutdown);
    }
}

/// The rodio `Source` that pulls rendered samples from the shared synthesizer.
struct SynthSource {
    synth: SharedSynth,
    volume: Arc<std::sync::atomic::AtomicU32>,
    // Reused render buffers; the mixer calls `next()` one sample at a time.
    left: Vec<f32>,
    right: Vec<f32>,
    interleaved: Vec<f32>,
    pos: usize,
}

impl SynthSource {
    fn new(synth: SharedSynth, volume: Arc<std::sync::atomic::AtomicU32>) -> Self {
        Self {
            synth,
            volume,
            left: vec![0.0; BLOCK_SIZE],
            right: vec![0.0; BLOCK_SIZE],
            interleaved: Vec::with_capacity(BLOCK_SIZE * 2),
            pos: 0,
        }
    }
}

impl Iterator for SynthSource {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        if self.pos >= self.interleaved.len() {
            // Render a fresh block.
            let volume = f32::from_bits(self.volume.load(Ordering::Relaxed));
            self.left.fill(0.0);
            self.right.fill(0.0);

            if let Ok(mut guard) = self.synth.lock() {
                if let Some(synth) = guard.as_mut() {
                    synth.render(&mut self.left, &mut self.right);
                }
            }

            self.interleaved.clear();
            for i in 0..BLOCK_SIZE {
                self.interleaved.push(self.left[i] * volume);
                self.interleaved.push(self.right[i] * volume);
            }
            self.pos = 0;
        }

        let sample = self.interleaved[self.pos];
        self.pos += 1;
        Some(sample)
    }
}

impl rodio::Source for SynthSource {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> rodio::ChannelCount {
        rodio::nz!(2)
    }

    fn sample_rate(&self) -> rodio::SampleRate {
        rodio::nz!(44100)
    }

    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

fn run(
    commands: Receiver<AudioCommand>,
    status: Sender<String>,
    volume: Arc<std::sync::atomic::AtomicU32>,
) {
    // Open the default output. If there is none, report and stay idle (but keep
    // servicing commands so Shutdown works).
    let device = match rodio::DeviceSinkBuilder::open_default_sink() {
        Ok(d) => Some(d),
        Err(e) => {
            let _ = status.send(format!("audio output unavailable: {e}; preview muted"));
            None
        }
    };

    let synth: SharedSynth = Arc::new(Mutex::new(None));
    let mut instrument = Instrument::Guqin;

    // Only attach a source if we actually opened a device.
    if let Some(dev) = device.as_ref() {
        let source = SynthSource::new(Arc::clone(&synth), volume);
        dev.mixer().add(source);
        // The `MixerDeviceSink` in `device` keeps the output stream alive.
    }

    // Load the default instrument so notes produce sound immediately.
    load_instrument(&synth, instrument, &status);

    let running = AtomicBool::new(true);
    loop {
        // Drain pending commands without blocking the audio thread.
        match commands.recv_timeout(DRAIN_INTERVAL) {
            Ok(cmd) => {
                if !handle_command(cmd, &synth, &mut instrument, &status, &running) {
                    break;
                }
            }
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => break,
        }
        if !running.load(Ordering::Relaxed) {
            break;
        }
    }
}

/// Apply a command; returns `false` when the renderer should shut down.
fn handle_command(
    command: AudioCommand,
    synth: &SharedSynth,
    instrument: &mut Instrument,
    status: &Sender<String>,
    running: &AtomicBool,
) -> bool {
    match command {
        AudioCommand::PlayNote { pitch, velocity } => {
            if let Ok(mut guard) = synth.lock() {
                if let Some(s) = guard.as_mut() {
                    s.note_on(CHANNEL, pitch as i32, velocity as i32);
                }
            }
        }
        AudioCommand::StopNote { pitch } => {
            if let Ok(mut guard) = synth.lock() {
                if let Some(s) = guard.as_mut() {
                    s.note_off(CHANNEL, pitch as i32);
                }
            }
        }
        AudioCommand::SetInstrument(inst) => {
            *instrument = inst;
            load_instrument(synth, *instrument, status);
        }
        AudioCommand::SetVolume(v) => {
            if let Ok(mut guard) = synth.lock() {
                if let Some(s) = guard.as_mut() {
                    s.set_master_volume(v.clamp(0.0, 1.0));
                }
            }
        }
        AudioCommand::AllNotesOff => {
            if let Ok(mut guard) = synth.lock() {
                if let Some(s) = guard.as_mut() {
                    s.note_off_all(true);
                }
            }
        }
        AudioCommand::Shutdown => {
            running.store(false, Ordering::Relaxed);
            return false;
        }
    }
    true
}

/// Load (or reload) the SoundFont for `instrument` and select its preset.
fn load_instrument(synth: &SharedSynth, instrument: Instrument, status: &Sender<String>) {
    let choice = match resolve_soundfont(instrument) {
        Ok(c) => c,
        Err(e) => {
            let _ = status.send(e.to_string());
            return;
        }
    };
    let path = choice.path.clone();

    let new_synth = (|| -> Result<Synthesizer, PreviewError> {
        let file = File::open(&path).map_err(|e| PreviewError::SoundFontLoad {
            path: path.display().to_string(),
            detail: e.to_string(),
        })?;
        let mut reader = BufReader::new(file);
        let font = SoundFont::new(&mut reader).map_err(|e| PreviewError::SoundFontLoad {
            path: path.display().to_string(),
            detail: format!("{e:?}"),
        })?;
        let font = Arc::new(font);

        let mut settings = SynthesizerSettings::new(44100);
        settings.block_size = BLOCK_SIZE;
        settings.maximum_polyphony = 64;
        settings.enable_reverb_and_chorus = true;

        let mut synth =
            Synthesizer::new(&font, &settings).map_err(|e| PreviewError::SoundFontLoad {
                path: path.display().to_string(),
                detail: format!("{e:?}"),
            })?;

        // Select the resolved preset (bank select CC32, then program change).
        synth.process_midi_message(CHANNEL, 0xB0, 32, choice.bank); // Bank Select LSB
        synth.process_midi_message(CHANNEL, PROGRAM_CHANGE, choice.patch, 0);
        Ok(synth)
    })();

    match new_synth {
        Ok(s) => {
            if let Ok(mut guard) = synth.lock() {
                *guard = Some(s);
            }
            let _ = status.send(format!(
                "preview: {} ({:?} bank {} patch {})",
                instrument.display_name(),
                choice.path.file_name().map(|f| f.to_string_lossy()),
                choice.bank,
                choice.patch
            ));
        }
        Err(e) => {
            let _ = status.send(e.to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::BufReader;

    fn load_font() -> Option<Arc<SoundFont>> {
        let p = std::env::var_os("HOME")?
            .into_string()
            .ok()?
            .parse::<std::path::PathBuf>()
            .ok()?
            .join(".local/share/where-winds-meet-player/soundfonts/FluidR3_GM.sf2");
        let file = File::open(p).ok()?;
        SoundFont::new(&mut BufReader::new(file)).ok().map(Arc::new)
    }

    fn peak(synth: &mut Synthesizer, blocks: usize) -> f32 {
        let mut l = vec![0.0; BLOCK_SIZE];
        let mut r = vec![0.0; BLOCK_SIZE];
        let mut peak = 0.0f32;
        for _ in 0..blocks {
            synth.render(&mut l, &mut r);
            for (&a, &b) in l.iter().zip(r.iter()) {
                peak = peak.max(a.abs()).max(b.abs());
            }
        }
        peak
    }

    #[test]
    fn synth_renders_audible_audio() {
        let Some(font) = load_font() else {
            eprintln!("skipping: no FluidR3_GM.sf2 installed");
            return;
        };
        let mut settings = SynthesizerSettings::new(44100);
        settings.block_size = BLOCK_SIZE;
        settings.maximum_polyphony = 64;
        let mut synth = Synthesizer::new(&font, &settings).unwrap();
        synth.note_on(0, 60, 100);
        let p = peak(&mut synth, 10);
        assert!(p > 0.001, "expected audible sample, peak={p}");
        synth.note_off_all(true);
    }
}
