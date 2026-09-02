# Project Status (memory across sessions)

Native Linux (Wayland / CachyOS / KDE Plasma 6) port of the Where Winds Meet
MIDI player: parses `.mid`, maps notes onto the game's keyboard layout, injects
via `/dev/uinput`, and previews audio locally. See `DESIGN.md` (behavior) and
`INSTRUMENT_PREVIEW_SPEC.md` (audio preview) for the specs.

## Where things stand

All four phases are **complete and working**, plus the audio-preview feature.

### Crates (Cargo workspace, `wwm-midi-player-linux`)

| Crate | Role | Status |
|---|---|---|
| `engine` | MIDI parsing (midly) + 36-key note→key mapping | done |
| `input` | `/dev/uinput` virtual keyboard (evdev) | done |
| `hotkeys` | ashpd Wayland global shortcuts (Play/Pause, Stop) | done |
| `player` | transport core + timing thread + Go Live gating | done |
| `preview_synth` | rustysynth + rodio audio preview (5 instruments) | done |
| `gui` | Qt6/QML front-end (cxx-qt 0.10) | done |
| `cli` | `wwm` inspect/play/hotkeys subcommands | done |

### Build & run

- `cargo build` / `cargo test --workspace` (all green; ~30 tests).
- GUI: `cargo run -p wwm-gui` (or `./target/debug/wwm-gui`).
- CLI: `./target/debug/wwm` (subcommands: `inspect`, `play`, `hotkeys`).

### Key implementation notes

- **cxx-qt naming gotcha:** both QML *properties* and *invokables* are exposed as
  **snake_case** (e.g. `game_running`, `note_count`, `load_file`, `toggle_play_pause`).
  Do not use camelCase in QML.
- **Go Live gating:** button is disabled (gray) until a game process is detected;
  green when ready, red when live. Detection scans `/proc` every 2s, excluding our
  own `wwm-*` processes, matching `wwm.exe` (plus "where winds meet"/"winds meet")
  in cmdline.
- **Audio preview:** default-on (checkbox "Preview"), instrument dropdown for the
  five instruments. SoundFonts resolved from project `soundfonts/` first, then
  `~/.local/share/where-winds-meet-player/soundfonts/`. Preset selected via
  Bank Select (CC32) + Program Change.
- **Key layout (21/36):** the game's Free Play mode is 21 natural notes by default,
  toggled to 36 chromatic notes with F1. Both are supported: `KeyMode::TwentyOne`
  (6 key names × 3 octaves, no modifiers) and `KeyMode::ThirtySix` (12 semitones,
  Shift/Ctrl accidentals). The GUI has a "21-key/36-key" combo and the CLI a
  `--keys` flag; both default to 21-key.

### SoundFonts

- Project `soundfonts/` (binary, git-ignored): `FS_Erhu_v2.sf2` (erhu 8/110),
  `MFA_Pipa.sf2` (pipa 32/105), `OLPC_Guzheng.sf2` (guzheng 1/107),
  `DSK Asian DreamZ.SF2` (multi: erhu 0/4, pipa 0/0, guzheng 0/3),
  `ACCURATE_SF2_AiX_CTX800.SF2` (general GM library, unmapped so far).
- `~/.local/share/where-winds-meet-player/soundfonts/FluidR3_GM.sf2` + `FluidR3_GS.sf2`
  (installed from Arch `soundfont-fluid` package) = GM fallback.

### uinput udev rule (auto-setup)

- The GUI checks `/dev/uinput` write access at startup (exposed as the
  `uinput_ready` QML property). If it's not writable, it opens a `Dialog` from
  `main.qml` explaining the need and offering "Install…".
- "Install…" calls the `install_uinput_rule` invokable, which writes the rule to
  `/tmp`, then runs `pkexec sh -c "install -m 0644 ... && udevadm control --reload-rules
  && udevadm trigger"`. The rule (`input::UDEV_RULE_CONTENT`,
  `/etc/udev/rules.d/99-wwm-uinput.rules`) uses `KERNEL=="uinput",
  SUBSYSTEM=="misc", TAG+="uaccess"` so systemd-logind grants the active-seat user
  access with no group/re-login. The Go Live button also re-prompts if the user
  dismissed it at startup.
- Detection/install helpers live in the `input` crate (`uinput_accessible`,
  `udev_rule_installed`, `UDEV_RULE_PATH`/`UDEV_RULE_CONTENT`); the pkexec wiring
  is in the GUI bridge (`STAGED_RULE_PATH`, `install_uinput_rule_via_pkexec`).

### Confirmed mappings

| Instrument | Font | bank/patch |
|---|---|---|
| Guqin 古琴 | OLPC_Guzheng | 1/107 |
| Pipa 琵琶 | MFA_Pipa | 32/105 |
| Erhu 二胡 | FS_Erhu_v2 | 8/110 |
| Konghou 箜篌 | FluidR3 GM fallback | 0/46 (Orchestral Harp) |
| Fangxiang 方響 | FluidR3 GM fallback | 0/9 (Glockenspiel) |

## Open / deferred (future sessions)

1. **Authentic Konghou + Fangxiang** soundfonts. Best candidates are in
   `ACCURATE_SF2_AiX_CTX800.SF2`: Konghou → `008-107 Zheng 1` (plucked Chinese
   zither) or `032-046 Harp`; Fangxiang → `032-098 VibeBell` (resonant bright
   metallophone) or `001-009 Glocken.`. **Blocker:** the preview uses `rustysynth`
   1.3.6, which refuses to load ACCURATE_SF2 — (a) a nested `LIST` chunk in the
   INFO header (removable; stripped copy loads fine in fluidsynth), and (b) a
   strict `sanity_check` that rejects some sample-loop regions in the font
   (`end`/`end_loop` vs `wave_data.len()`, `start_loop`/`end_loop` ordering).
   Path forward: either vendor `rustysynth` via `[patch.crates-io]` with a lenient
   INFO/`sdta` parser + sanity check, or preprocess the font to clamp bad loop
   points. DSK Asian DreamZ and FluidR3 have no usable harp/metallophone presets.
2. **Octave collapse / tessitura centering** — **DONE**. The 21-key and 36-key
   layouts are both 3-octave grids (C3–B5), so wide-range pieces previously
   folded out-of-range octaves onto the boundary rows. Now `parse()` computes a
   static per-song octave fold (`Song::octave_shift`, a multiple of ±12) via
   `detect_octave_shift()` in `engine/src/midi.rs`: it scans whole-octave shifts
   and picks the one that centers the song's tessitura inside the window,
   minimizing how many notes collapse (ties prefer the smallest shift, so pieces
   that already fit are untouched). The fold is applied on top of the semitone
   transpose at map time in the player and the CLI's `inspect`; the GUI's
   transpose readout still shows the semitone value only. The audio preview keeps
   using the real MIDI pitches. Remaining: subjective "does it sound flat" check
   against real pieces.
3. ~~Game detection against the real Proton process~~ **DONE** — verified live on
   `oldalienware`: `Go Live` goes gray→green when the game launches (Steam appid
   3564740, folder "Where Winds Meet", exe `Engine/Binaries/Win64r/wwm.exe`, under
   Proton), green↔red toggling while live, back to gray on exit. Detection matches
   `wwm.exe` plus "where winds meet"/"winds meet".
4. **Live injection end-to-end with the game** (untested: `/dev/uinput` → Proton
   game receiving the melody).

## Test / deploy machine

- **`oldalienware`** — the gaming box with WWM installed. SSH alias
  `ssh oldalienware` (`HostName oldalienware.home.arpa`, `User maria`).
- x86_64, **CachyOS/KDE**, `qt6-base 6.11.2` (same as the dev box), **no Rust
  toolchain** — so build locally and copy binaries over.
- Deploy: `cargo build --release`, then
  `scp target/release/wwm-gui target/release/wwm oldalienware:~/` plus
  `scp -r soundfonts oldalienware:~/` for audio preview. Only **two** executables
  exist (`wwm-gui`, `wwm`); libraries are compiled in, and a stale `wwm-cli`
  sits in `target/debug/` (ignore it).

## Git

- Repo: `https://github.com/Alekisan/wwm-midi-player-linux` (branch `main`).
- Reference (original Svelte/Tauri Windows app by SnowiyQ, `SnowiyQ/Where-Winds-Meet-Midi-Player`): forked as
  `Alekisan/Where-Winds-Meet-Midi-Player`.
- MIT license, © Alexander D. Martinez (see `LICENSE`).
