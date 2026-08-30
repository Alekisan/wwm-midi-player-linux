# Feature Specification: Native Audio Preview & Instrument Synthesis Engine

## 1. Executive Summary & Goals
This specification outlines the architecture for integrating an in-app audio preview system into the native Rust/Qt6 MIDI player. The feature allows users to preview MIDI playback directly within the application prior to transmitting input commands to *Where Winds Meet* via `/dev/uinput`.

The player must support audio previewing across five distinct historical Chinese instruments available in the game:
1. **Guqin (古琴)** – 7-string plucked zither
2. **Pipa (琵琶)** – 4-string plucked lute
3. **Erhu (二胡)** – 2-string bowed vertical fiddle
4. **Konghou (箜篌)** – Plucked harp / vertical zither
5. **Fangxiang (方響)** – Tuned metal slab metallophone

---

## 2. Technical Stack & Rust Dependencies

To maintain zero runtime C dependencies and avoid requiring external daemons (e.g., FluidSynth, TiMidity++), the preview system relies strictly on pure-Rust audio crates compatible with Linux / PipeWire:

- **Synthesizer Engine:** [`rustysynth`](https://crates.io/crates/rustysynth)
  - Pure-Rust SoundFont 2 (`.sf2`) real-time softsynth parser and renderer.
  - Renders raw PCM float audio samples in memory.
- **Audio Output Driver:** [`rodio`](https://crates.io/crates/rodio) (built on [`cpal`](https://crates.io/crates/cpal))
  - Output stream manager providing native cross-platform audio playback on PipeWire/ALSA.
- **Concurrency & Messaging:** [`tokio::sync::mpsc`](https://docs.rs/tokio/latest/tokio/sync/mpsc/index.html)
  - Non-blocking channel communication between the UI/Playback thread and the background synth renderer.

---

## 3. Instrument Asset Sourcing & Mapping Strategy

### 3.1 Instrument Sound Font Mapping
| Instrument | Primary Sound Class | Target SF2 Preset / Fallback Mapping |
| :--- | :--- | :--- |
| **Guqin (古琴)** | Plucked Zither | Dedicated Guqin SF2 OR Low-tuned Guzheng preset |
| **Pipa (琵琶)** | Plucked Lute | Dedicated Pipa SF2 OR Lute / Shamisen preset |
| **Erhu (二胡)** | Bowed String | Dedicated Erhu SF2 OR Kokyu / Violin legato preset |
| **Konghou (箜篌)** | Plucked Harp | Classical Harp / Concert Harp SF2 preset |
| **Fangxiang (方響)** | Metallophone | Glockenspiel / Carillon / Steel Chime SF2 preset |

### 3.2 Asset Storage Directory
SoundFont assets MUST be kept outside of the application binary to ensure small binary size and modular user overrides:
- **Default Path:** `~/.local/share/where-winds-meet-player/soundfonts/`
- **Fallback Hierarchy:**
  1. Load instrument-specific `.sf2` (e.g., `guqin.sf2`, `pipa.sf2`)
  2. Fall back to a bundled general GM SoundFont (e.g., `FluidR3_GM.sf2` or mini equivalent)
  3. Mute preview gracefully if no SoundFont is present while logging an informative UI toast.

### 3.3 Optional Game Asset Extractor (Future Enhancement)
To provide 100% authentic in-game audio without shipping copyrighted files in the repository:
- Include an optional scanner utility that checks `~/.local/share/Steam/steamapps/common/WhereWindsMeet/` (Proton prefix).
- Extract raw note audio clips directly from game resource archives when available.

---

## 4. Software Architecture & Data Structures

### 4.1 Data Types
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Instrument {
    Guqin,
    Pipa,
    Erhu,
    Konghou,
    Fangxiang,
}

#[derive(Debug)]
pub enum AudioCommand {
    PlayNote { pitch: u8, velocity: u8 },
    StopNote { pitch: u8 },
    SetInstrument(Instrument),
    SetVolume(f32),
    AllNotesOff,
}
```

### 4.2 Engine Thread Isolation
```
┌────────────────────────┐      Tokio Channel       ┌────────────────────────┐
│  UI / Playback Loop    ├─────────────────────────►│  Background Audio Thread│
│ (Qt6 / Rust Engine)    │  (AudioCommand events)   │ (rustysynth + rodio)   │
└────────────────────────┘                          └────────────────────────┘
```
- **Rule 1:** The audio synthesis engine MUST run on a dedicated background thread.
- **Rule 2:** The MIDI tick-timer must emit `AudioCommand::PlayNote` events concurrently with `/dev/uinput` keypress events when preview mode is active.
- **Rule 3:** Hot-swapping instruments sends a `SetInstrument` command to reload the active SoundFont preset without re-initializing the `rodio` audio output stream.

---

## 5. Agent Implementation Instructions (For OpenCode)

When building this module, adhere to the following directives:

1. **Isolation:** Keep the audio preview system in a separate module/crate (`crates/preview_synth`). It must compile even if `/dev/uinput` features are disabled (e.g., during unit tests).
2. **Error Recovery:** Never panic if an audio device is unplugged or an `.sf2` file is corrupted. Fail silently or send an error state to the UI bus.
3. **Low Latency:** Pre-load SoundFont presets into memory (`Arc<SoundFont>`) during application startup or instrument selection. Never perform blocking disk I/O inside the note-rendering loop.
