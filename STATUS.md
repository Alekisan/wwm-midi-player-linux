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
| `playlist` | persistent editable playlist (JSON store, `~/.config/where-winds-meet-player/playlist.json`) | done |
| `input` | `/dev/uinput` virtual keyboard (evdev) | done |
| `hotkeys` | ashpd Wayland global shortcuts (Play/Pause, Stop) | done |
| `player` | transport core + timing thread + Go Live gating | done |
| `preview_synth` | rustysynth + rodio audio preview (5 instruments) | done |
| `gui` | Qt6/QML front-end (cxx-qt 0.10) | done |
| `cli` | headless dev/diagnostic CLI (`wwm`: inspect/play/hotkeys); not shipped — see `CLI.md` | done |

### Build & run

- `cargo test --workspace` (all green; ~30 tests).
- **Release ships one binary:** `cargo build --release -p wwm-gui` →
  `./target/release/wwm-gui`. (`cargo build --release` builds the whole workspace
  and also produces `wwm`.)
- GUI: `cargo run -p wwm-gui` (or `./target/debug/wwm-gui`).
- CLI (dev/diagnostic only, **not shipped**): `./target/debug/wwm` — `inspect`,
  `play`, `hotkeys`. Global hotkeys are implemented only here (the GUI has none,
  by design). Full reference: `CLI.md`.

### Key implementation notes

- **Persistent playlist (queue playback + loop):** the playlist is a decoupled
  `playlist` crate that serializes an ordered list of absolute `.mid` paths to
  JSON at `~/.config/where-winds-meet-player/playlist.json` (`$XDG_CONFIG_HOME`
  honored). The GUI bridge owns a `Playlist` instance (in a `RefCell`, since the
  cxx-qt QObject only `Deref`s — not `DerefMut` — to the Rust struct) and projects
  it into the parallel `songs`/`song_paths` `QStringList`s the `ListView` binds to.
  Tracks are added via "Add files…" (multi-select `FileDialog`, `add_files`)
  or "Add folder…" (now *appends* rather than replaces), removed, and reordered
  (per-row up/down → `move_song`). Each edit persists + refreshes the views.
  **Selection** is a `QStringList` of selected paths (path-keyed, so stable across
  reorder); checkboxes drive `set_selected`. **"Play selected"** snapshots the
  selected tracks (in playlist order) into an internal queue and plays them in
  sequence: on `PlayerEvent::Finished` the bridge's `advance_queue` loads the next
  queued path and auto-plays, wrapping to the start when `loop_queue` is on
  ("Loop: On/Off" button). `current_index` (the highlight) is relocated by path
  so it survives reorder/removal. A manual single-file load (`load_file` /
  `select_song`) clears the queue, so auto-advance only applies to explicitly
  queued playback.
- **cxx-qt naming gotcha:** both QML *properties* and *invokables* are exposed as
  **snake_case** (e.g. `game_running`, `note_count`, `load_file`, `toggle_play_pause`).
  Do not use camelCase in QML.
- **Go Live gating:** button is disabled (gray) until a game process is detected;
  green when ready, red when live. Detection scans `/proc` every 2s, excluding our
  own `wwm-*` processes, matching `wwm.exe` (plus "where winds meet"/"winds meet")
  in cmdline.
- **Audio preview:** implicit, no UI toggle. It is on whenever the player is *not*
  live and off while live (the Go Live button drives it: `go_live` / the
  game-gone auto-drop both call `player.set_preview(!live)`). The instrument
  dropdown (Guqin/Pipa/Erhu/Konghou/Fangxiang) stays enabled while not live and
  grays out while live. SoundFonts resolved from project `soundfonts/` first, then
  `~/.local/share/where-winds-meet-player/soundfonts/`. Preset selected via
  Bank Select (CC32) + Program Change. (The old "Preview" checkbox and the
  `preview` QML property / `toggle_preview` invokable were removed.)
- **Key layout (21/36):** the game's Free Play mode is 21 natural notes by default,
  toggled to 36 chromatic notes with F1. Both are supported: `KeyMode::TwentyOne`
  (6 key names × 3 octaves, no modifiers) and `KeyMode::ThirtySix` (12 semitones,
  Shift/Ctrl accidentals). The GUI has a "21-key/36-key" combo and the CLI a
  `--keys` flag; both default to 21-key.

### SoundFonts

- **Shipped** in `soundfonts/` (tracked; redistribution-friendly — see
  `soundfonts/README.md`): `FS_Erhu_v2.sf2` (erhu 8/110), `MFA_Pipa.sf2`
  (pipa 32/105), `OLPC_Guzheng.sf2` (guzheng 1/107), `ConcertHarp.sf2`
  (Konghou, FreePats CC0, bank 0/patch 0), `Xylophone.sf2` (Fangxiang, FreePats
  CC0, bank 0/patch 0).
- **Not redistributed** (git-ignored, keep local): `ACCURATE_SF2_AiX_CTX800.SF2`
  (all rights reserved) and `DSK Asian DreamZ.SF2` (DSK freeware). The CC0
  FreePats fonts replace ACCURATE for Konghou/Fangxiang; DSK was only an optional
  Guqin/Pipa/Erhu fallback.
- `~/.local/share/where-winds-meet-player/soundfonts/FluidR3_GM.sf2` + `FluidR3_GS.sf2`
  (Arch `soundfont-fluid` package) = GM fallback if the user provides one. No GM
  bank is bundled.

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
| Konghou 箜篌 | FreePats Concert Harp | 0/0 (CC0) |
| Fangxiang 方響 | FreePats Xylophone | 0/0 (CC0) |

## Open / deferred (future sessions)

1. **Authentic Konghou + Fangxiang** soundfonts — **DONE (redistribution-safe).**
   The original `ACCURATE_SF2_AiX_CTX800.SF2` mapping (Konghou → `032-046 Harp`,
   Fangxiang → `032-098 VibeBell`) could not be redistributed (all rights
   reserved), so it was replaced with **CC0 FreePats** banks assembled from the
   CC0 Versilian Community Sample Library: Konghou → FreePats Concert Harp,
   Fangxiang → FreePats Xylophone (each a single preset at bank 0/patch 0; added
   to `SPECIFIC_FONTS` in `preview_synth/src/lib.rs`). Verified by
   `specific_fonts_load_konghou_and_fangxiang` (loads the font, selects the
   preset, renders an audible C4). These are approximations (a concert harp for
   箜篌, an orchestral xylophone for 方響); `SPECIFIC_FONTS` is the place to
   retune. The vendored `rustysynth` lenient parser under `vendor/rustysynth`
   (swapped in via `[patch.crates-io]`) is **retained but no longer required** for
   the shipped fonts: (a) it skips unknown INFO sub-chunks instead of erroring on
   `ListContainsUnknownId`; (b) `sanity_check` normalizes degenerate loop points
   whose `start_loop >= end_loop` to `NoLoop`.
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
4. ~~Live injection end-to-end with the game~~ **DONE** — tested live on
   `oldalienware` with the game running under Proton. `wwm play test-scale.mid
   --live --verbose` injected keystrokes that the game picked up, in **both** the
   21-key and 36-key layouts (a C-major scale+melody, 19 notes, that maps to
   obvious keys `a s d f g h j q …`). Notably `/dev/uinput` was already writable by
   `maria` there (logind `uaccess` ACL), so no udev rule install was needed.

## Release

- Version is **1.0.0** (`[workspace.package] version` in `Cargo.toml`); changes are
  tracked in `CHANGELOG.md`.
- CI: `.github/workflows/ci.yml` runs `cargo fmt --all --check`,
  `cargo test --workspace`, and `cargo clippy --workspace --all-targets -- -D warnings`
  on push to `main` and on PRs (Qt installed via `jurplel/install-qt-action`).
- Release: pushing a `v*` tag triggers `.github/workflows/release.yml`, which builds
  `-p wwm-gui` and publishes `wwm-midi-player-linux-<ver>-x86_64.tar.gz` (binary +
  `soundfonts/` + `LICENSE`/`README.md`/`CLI.md`) plus a `.sha256`.
- Cutting a release: bump the version, update `CHANGELOG.md`, commit, then
  `git tag vX.Y.Z && git push origin vX.Y.Z`.

## Test / deploy machine

- **`oldalienware`** — the gaming box with WWM installed. SSH alias
  `ssh oldalienware` (`HostName oldalienware.home.arpa`, `User maria`).
- x86_64, **CachyOS/KDE**, `qt6-base 6.11.2` (same as the dev box), **no Rust
  toolchain** — so build locally and copy binaries over.
- Deploy: `cargo build --release -p wwm-gui` (add `-p wwm-cli` if you also want the
  diagnostic CLI there), then `scp target/release/wwm-gui oldalienware:~/`
  (+ `wwm` if built) plus `scp -r soundfonts oldalienware:~/` for audio preview.
  Libraries are compiled in.
- As of the live-injection test, `oldalienware:~/` already has `wwm-gui`, `wwm`,
  `soundfonts/`, and a `test-scale.mid` smoke-test fixture. **Those binaries
  predate the preview-checkbox removal and the CC0 soundfont swap**, so rebuild +
  re-copy `wwm-gui` (and `soundfonts/`) before relying on the latest behavior
  there.

## Git

- Repo: `https://github.com/Alekisan/wwm-midi-player-linux` (branch `main`).
- Reference (original Svelte/Tauri Windows app by SnowiyQ, `SnowiyQ/Where-Winds-Meet-Midi-Player`): forked as
  `Alekisan/Where-Winds-Meet-Midi-Player`.
- MIT license, © Alexander D. Martinez (see `LICENSE`).
