# Design Notes

## Goal

Native Linux (Wayland) player for the *Where Winds Meet* MIDI player. Parses
`.mid` files, translates notes onto the game's keyboard layout, and injects
keystrokes through `/dev/uinput` so games running under Proton receive them as
ordinary hardware input.

Target environment: CachyOS / KDE Plasma 6.

## Architecture principles

- **Decoupled engine.** MIDI parsing, note-to-key mapping, and virtual-input are
  a standalone Rust core with no GUI dependency.
- **Wayland native.** No X11 tooling (`xdotool`, `XTest`, `WM_KEYDOWN`). All
  input is routed through `/dev/uinput` virtual hardware.
- **Errors, not panics.** Missing `/dev/uinput` permissions and similar failures
  surface as clear diagnostics.

## Program behavior

The app is first and foremost a MIDI player and **always runs**, regardless of
game state. Unlike the Windows version (which exited when it could not see the
game window), the Linux port keeps the UI open and usable at all times.

Game detection is a soft, background watcher. It never gates application
startup, UI rendering, or MIDI playback. It only informs whether input injection
is possible.

## Audio preview (local playback)

The player also offers a **local audio preview** so a song can be heard without
the game. Per `INSTRUMENT_PREVIEW_SPEC.md`, the user picks which of the five
in-game instruments the preview uses (Guqin, Pipa, Erhu, Konghou, Fangxiang) so
it matches what playback will sound like in the game. It is implemented in the
decoupled `preview_synth` crate using `rustysynth` (SoundFont synthesis) +
`rodio` (output), on a dedicated background thread, driven by `AudioCommand`
messages — fully independent of the `/dev/uinput` input driver. SoundFonts live
in `~/.local/share/where-winds-meet-player/soundfonts/` (instrument-specific
`.sf2` first, then a GM fallback); a missing SoundFont mutes the preview with a
status note instead of crashing.

### Input injection states

1. **Game not detected.** Fully functional: load MIDI, play, seek, visualize
   notes, adjust speed/octave/mapping. Input injection is idle and the player
   acts purely as a MIDI player.
2. **Game detected.** A prominent **"Go Live"** button appears and toggles input
   injection on/off. This is a manual, user-controlled switch so the user always
   knows whether they are sending input to the game. It defaults to **off**.

The "Go Live" toggle is the single source of truth for whether `/dev/uinput`
events reach the game — injection is never turned on automatically by focus or
detection alone.

## Phased plan

- **Phase 1 — CLI core:** parse `.mid`, 36-key mapping, timed events. *(done)*
- **Phase 2 — Virtual hardware:** `/dev/uinput` device, press/release. *(done)*
- **Phase 3 — Wayland portals:** global hotkeys (Play/Pause, Stop) via `ashpd`. *(done)*
- **Phase 4 — Qt6 front-end:** load files, playback controls, "Go Live" toggle.
