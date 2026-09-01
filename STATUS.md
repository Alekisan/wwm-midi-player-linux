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
  own `wwm-*` processes, matching "where winds meet"/"winds meet" in cmdline.
- **Audio preview:** default-on (checkbox "Preview"), instrument dropdown for the
  five instruments. SoundFonts resolved from project `soundfonts/` first, then
  `~/.local/share/where-winds-meet-player/soundfonts/`. Preset selected via
  Bank Select (CC32) + Program Change.

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

1. **Authentic Konghou + Fangxiang** soundfonts. Either map a preset from
   `DSK Asian DreamZ`/`ACCURATE_SF2` or source dedicated `konghou.sf2`/`fangxiang.sf2`.
2. **36-key octave collapse.** `octave_of()` clamps everything below MIDI 60 into
   the low row, so wide-range pieces collapse many pitches onto the same game key
   (notes 36/48/60 all → `n`). Faithful port of the reference, but loses octave
   detail. Tune if preview sounds flat.
3. **Game detection against the real Proton process.** Verified against the
   installed game (Steam appid 3564740, name "Where Winds Meet", folder
   `Where Winds Meet`, exe `Engine/Binaries/Win64r/wwm.exe`, running under
   Proton). Detection now matches `wwm.exe` plus `where winds meet`/`winds meet`,
   so the `Go Live` button resolves reliably whether the process cmdline carries
   the full install path or just the exe name. Still needs a live on-box check
   that the button actually lights up while the game is running here.
4. **Live injection end-to-end with the game** (untested: `/dev/uinput` → Proton
   game receiving the melody).

## Git

- Repo: `https://github.com/Alekisan/wwm-midi-player-linux` (branch `main`).
- Reference (original Svelte/Tauri Windows app): forked as
  `Alekisan/Where-Winds-Meet-Midi-Player`.
