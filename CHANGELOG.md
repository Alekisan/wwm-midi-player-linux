# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.0] - 2026-09-24

First stable release: a native Linux (Wayland) MIDI auto-player for
*Where Winds Meet*.

### Added

- **MIDI engine** — parses `.mid`/`.midi` (midly) and maps notes onto the game's
  keyboard layout, with a per-song octave fold that centers the tessitura in the
  playable window.
- **Input injection** — a virtual keyboard via `/dev/uinput`, so games running
  under Proton see ordinary hardware input (no X11 hacks). Includes a one-click
  udev-rule installer (polkit) so no group membership or re-login is needed.
- **Qt 6 / QML GUI** (`wwm-gui`) — the only shipped binary.
- **Persistent playlist** — an editable list saved to
  `~/.config/where-winds-meet-player/playlist.json`; add files/folders, reorder,
  remove, multi-select tracks to queue sequential playback with optional loop.
- **Local audio preview** — rustysynth + rodio, with five instruments (Guqin,
  Pipa, Erhu, Konghou, Fangxiang). Preview is implicit: on whenever the player is
  not live.
- **21-key and 36-key layouts** — the game's default 21 natural notes, or the
  F1-toggled 36 chromatic notes.
- **Game detection** — a background watcher gates the "Go Live" button.
- **Global hotkeys** — Play/Pause and Stop via the Wayland portal (ashpd).
  Available only in the development CLI, not the GUI.
- **Development CLI** (`wwm`) — a headless `inspect` / `play` / `hotkeys` tool for
  testing and diagnostics; not part of releases. See [CLI.md](CLI.md).
- **Redistribution-friendly soundfonts** — CC0 (FreePats Concert Harp, FreePats
  Xylophone) and CC BY 3.0 (OLPC Guzheng, MFA Pipa, FS Erhu v2). See
  [soundfonts/README.md](soundfonts/README.md).

### Notes

- Requires the Qt 6 runtime; the released binary is built for Linux x86_64.
- The Konghou and Fangxiang previews are close approximations (a concert harp and
  an orchestral xylophone).
- Third-party tools can carry a ban risk in online games; use at your own risk.

[1.0.0]: https://github.com/Alekisan/wwm-midi-player-linux/releases/tag/v1.0.0
