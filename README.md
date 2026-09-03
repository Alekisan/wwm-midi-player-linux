# wwm-midi-player-linux

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
- **Global hotkeys** — Play/Pause and Stop via the Wayland portal (ashpd).
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
| `cli` | `wwm` inspect/play/hotkeys subcommands |

## Requirements

- Linux with **Qt 6** runtime (tested on CachyOS / KDE Plasma 6).
- Rust (stable) with `cargo`, plus Qt6 development headers to build.
- For input injection: write access to `/dev/uinput` (see [uinput setup](#uinput-setup)).

## Build

```sh
cargo build --release
```

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

### CLI

```sh
./target/release/wwm inspect song.mid          # print translated key events
./target/release/wwm play song.mid             # play as a plain MIDI player
./target/release/wwm play song.mid --live      # inject keystrokes into the game
./target/release/wwm hotkeys                   # listen for global Play/Pause & Stop
```

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

Audio preview uses instrument-specific SoundFonts from `soundfonts/` (git-ignored),
falling back to a general MIDI library under
`~/.local/share/where-winds-meet-player/soundfonts/`. See `STATUS.md` for the
confirmed instrument → SoundFont (bank/patch) mappings.

## License

[MIT](LICENSE)
