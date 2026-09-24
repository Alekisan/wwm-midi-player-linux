# wwm-midi-player-linux

[![CI](https://github.com/Alekisan/wwm-midi-player-linux/actions/workflows/ci.yml/badge.svg)](https://github.com/Alekisan/wwm-midi-player-linux/actions/workflows/ci.yml)

A native Linux MIDI music player for **[Where Winds Meet](https://store.steampowered.com/app/3564740)**.
It parses `.mid` files, translates the notes onto the game's on-screen keyboard
layout, and plays them into the game by injecting keystrokes through a virtual
input device (`/dev/uinput`) — no X11 hacks, so games running under Proton see it
as ordinary hardware input.

> **Inspired by** the Windows original **[WWM Overlay](https://github.com/SnowiyQ/Where-Winds-Meet-Midi-Player)**
> by **[SnowiyQ](https://github.com/SnowiyQ)**. This is an independent Rust/Qt port
> targeting Linux (Wayland / KDE Plasma), not a fork of its codebase.

> ⚠️ **Use at your own risk.** Third-party tools can carry a ban risk in online games.
> MIDI players have not been widely reported as triggering bans, but the risk exists.

## Features

- **MIDI playback** — loads `.mid` / `.midi`, seeks, loops, and plays at adjustable speed.
- **Persistent playlist** — an editable list of tracks (add files/folders, remove, reorder)
  saved to disk and reloaded on startup; multi-select tracks to queue them for
  sequential playback, with an optional loop.
- **36-key mapping** — maps notes onto the game's keyboard layout (closest/raw note modes, auto or manual transpose).
- **Live input injection** — a "Go Live" toggle sends keystrokes to the game via `/dev/uinput`.
- **Local audio preview** — hear the song locally (Guqin, Pipa, Erhu, Konghou, Fangxiang) without the game running.
- **Game detection** — a background watcher spots the running game and gates the "Go Live" button.
- **Global hotkeys** — Play/Pause and Stop via the Wayland portal (ashpd). Provided
  by the development CLI only ([`CLI.md`](CLI.md)); the GUI has none by design.
- **One-click setup** — if `/dev/uinput` isn't writable, the GUI offers to install a udev rule for you (via polkit).

## Architecture

A Cargo workspace of decoupled crates (see [`DESIGN.md`](DESIGN.md) and [`STATUS.md`](STATUS.md)):

| Crate | Role |
|---|---|
| `engine` | MIDI parsing (midly) + 36-key note → key mapping |
| `playlist` | persistent, editable playlist model (JSON under `~/.config`) |
| `input` | `/dev/uinput` virtual keyboard (evdev) |
| `hotkeys` | ashpd Wayland global shortcuts |
| `player` | transport core + timing thread + Go Live gating |
| `preview_synth` | rustysynth + rodio audio preview |
| `gui` | Qt6/QML front-end (cxx-qt) |
| `cli` | headless dev/diagnostic CLI (`wwm`: inspect/play/hotkeys); not shipped — see [`CLI.md`](CLI.md) |

## Requirements

- Linux with **Qt 6** runtime (tested on CachyOS / KDE Plasma 6).
- Rust (stable) with `cargo`, plus Qt6 development headers to build.
- For input injection: write access to `/dev/uinput` (see [uinput setup](#uinput-setup)).

## Install

### From a release (Linux x86_64)

Download `wwm-midi-player-linux-<version>-x86_64.tar.gz` from the
[releases page](https://github.com/Alekisan/wwm-midi-player-linux/releases),
extract it, and run `./wwm-gui`. The tarball bundles `soundfonts/` — keep it
next to the binary. The Qt 6 runtime is required (Arch/CachyOS: `qt6-base
qt6-declarative`; Debian/Ubuntu: the `libqt6*` runtime packages).

### From source

See [Build](#build) below.

## Build

The app is a single binary, `wwm-gui`:

```sh
cargo build --release -p wwm-gui
```

`cargo build --release` builds the whole workspace, which also produces `wwm` — a
headless development/diagnostic CLI that is **not** shipped (see [`CLI.md`](CLI.md)).

```sh
cargo test --workspace
```

## Usage

### GUI

```sh
cargo run -p wwm-gui
# or
./target/release/wwm-gui
```

### CLI (development tool)

`wwm` is a headless front-end for testing and diagnostics — **not part of
releases**. It provides `inspect`, `play`, and the global `hotkeys`. See
[`CLI.md`](CLI.md) for the full reference.

## uinput setup

Input injection needs write access to `/dev/uinput`. The GUI checks this on startup
and, if access is missing, shows a dialog that installs a udev rule automatically
(it will ask for your password via polkit). The rule:

```
KERNEL=="uinput", SUBSYSTEM=="misc", TAG+="uaccess"
```

grants the logged-in user access with no group membership or re-login required.

To install it manually:

```sh
echo 'KERNEL=="uinput", SUBSYSTEM=="misc", TAG+="uaccess"' \
  | sudo tee /etc/udev/rules.d/99-wwm-uinput.rules
sudo udevadm control --reload-rules && sudo udevadm trigger
```

## SoundFonts

Audio preview uses one instrument-specific SoundFont per instrument, shipped in
`soundfonts/` (all under redistribution-friendly licenses — see
[`soundfonts/README.md`](soundfonts/README.md) for sources and attribution):

| Instrument | Font | License |
|---|---|---|
| Guqin 古琴 | OLPC Guzheng | CC BY 3.0 |
| Pipa 琵琶 | MFA Pipa | CC BY 3.0 |
| Erhu 二胡 | FS Erhu v2 | CC BY 3.0 |
| Konghou 箜篌 | FreePats Concert Harp | CC0 1.0 |
| Fangxiang 方響 | FreePats Xylophone | CC0 1.0 |

If an instrument-specific font is missing, the app falls back to a General MIDI
bank (`FluidR3_GM.sf2` / `FluidR3_GS.sf2`, MIT) placed under
`~/.local/share/where-winds-meet-player/soundfonts/`. No GM bank is bundled.

## License

[MIT](LICENSE)
