# `wwm` — command-line interface (development tool)

`wwm` is a headless front-end for the engine and player. It is a **testing and
diagnostics tool, not part of releases** — end users get the GUI (`wwm-gui`).
It exists so the core can be exercised without Qt: verifying note→key
translation, live-injection smoke tests, and global hotkeys.

Build it with:

```sh
cargo build --release -p wwm-cli   # -> target/release/wwm
```

(A bare `cargo build --release` builds the whole workspace and also produces it.)

## Why it exists

- **Headless testing** of `engine` / `player` without a display or Qt.
- **`inspect`** — dump the translated key events for a `.mid`, to check the
  mapping (mode, 21/36-key layout, transpose, octave fold) without the game.
- **`play --live`** — the lightweight path used to verify `/dev/uinput`
  injection end to end.
- **Global hotkeys** — the CLI is the *only* front-end that implements them; the
  GUI has none by design.

## Usage

```
Usage: wwm <COMMAND>

Commands:
  inspect  Parse a MIDI file and print the translated timed key events
  play     Play a MIDI file. Acts as a plain MIDI player unless --live is set
  hotkeys  Listen for Play/Pause and Stop global shortcuts and print them
  help     Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
```

### Shared translation options (`inspect`, `play`)

| Option | Default | Meaning |
|---|---|---|
| `-m, --mode <closest\|raw>` | `closest` | Note-to-key mapping mode |
| `--keys <twenty-one\|thirty-six>` | `twenty-one` | 21 natural notes, or 36 chromatic (game's F1 toggle) |
| `-t, --transpose <N>` | auto | Manual transpose in semitones (overrides per-song detection) |

When `--transpose` is omitted, the per-song auto transpose and octave fold
(`Song::octave_shift`) are applied.

### `wwm inspect [OPTIONS] <FILE>`

Print the translated, timed key events for a MIDI file. Prints a summary to
stderr (event count, duration, BPM, transpose, octave fold, key mode) and one
line per event to stdout: `time  track  note  key  press|release`.

| Extra option | Meaning |
|---|---|
| `--notes-only` | Only print note-on events |
| `-l, --limit <N>` | Stop after `N` events |

```sh
./target/release/wwm inspect song.mid --keys thirty-six --notes-only --limit 40
```

### `wwm play [OPTIONS] <FILE>`

Drive the transport. Without `--live` it plays locally (audio preview) only and
exits when the song ends.

| Option | Default | Meaning |
|---|---|---|
| `--speed <F64>` | `1.0` | Playback speed multiplier |
| `--hold <MS>` | `0` | Hold duration per key press |
| `--live` | off | Inject keystrokes into the game via `/dev/uinput` |
| `--hotkeys` | off | Register global hotkeys and stay resident |
| `-v, --verbose` | off | Log each emitted key to stdout |

```sh
./target/release/wwm play song.mid                 # local preview only
./target/release/wwm play song.mid --live --verbose
./target/release/wwm play song.mid --live --hotkeys
```

`--live` needs write access to `/dev/uinput`; see the uinput section in
[`README.md`](README.md).

### `wwm hotkeys`

Register the Play/Pause and Stop global shortcuts (Wayland portal, via `ashpd`)
and print the ones that fire. Purely a diagnostic for the hotkey path.

## Hotkeys and the GUI

Global hotkeys are implemented **only** in this CLI. The GUI deliberately has
none, so hotkey support is not something to expect from `wwm-gui`.
