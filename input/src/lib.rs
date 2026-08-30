//! Virtual keyboard backed by `/dev/uinput`.
//!
//! Phase 2 of the project: wrap the Linux uinput facility to create a virtual
//! input device that emits key press/release events. Games running under Proton
//! see this as ordinary hardware input, so no X11 tooling (`xdotool`, `XTest`,
//! `WM_KEYDOWN`) is involved — all input is routed through virtual hardware.

use evdev::uinput::VirtualDevice;
use evdev::{AttributeSet, EventType, InputEvent, KeyCode};
use std::collections::HashSet;
use std::time::Duration;
use thiserror::Error;
use wwm_engine::mapping::KeyChord;

/// Error type for virtual hardware input.
#[derive(Debug, Error)]
pub enum InputError {
    #[error("failed to open /dev/uinput ({0}); ensure you have write access (e.g. a udev rule or uinput group membership)")]
    Open(String),
    #[error("failed to emit input event: {0}")]
    Emit(String),
    #[error("no keyboard mapping for chord '{0}'")]
    UnknownKey(KeyChord),
}

const SYN_REPORT: u16 = 0;

/// A virtual keyboard device backed by `/dev/uinput`.
pub struct VirtualKeyboard {
    device: VirtualDevice,
    pressed: HashSet<KeyCode>,
    hold: Duration,
}

impl VirtualKeyboard {
    /// Create a virtual keyboard device with the given name.
    ///
    /// All keys the player can emit (letters, digits, punctuation, and the
    /// left/right modifier keys) are advertised as capabilities up front.
    pub fn create(name: &str) -> Result<Self, InputError> {
        let builder = VirtualDevice::builder().map_err(|e| InputError::Open(e.to_string()))?;
        let builder = builder.name(name);

        let mut keys = AttributeSet::<KeyCode>::new();
        for key in all_supported_keys() {
            keys.insert(key);
        }
        let builder = builder
            .with_keys(&keys)
            .map_err(|e| InputError::Open(e.to_string()))?;

        let device = builder
            .build()
            .map_err(|e| InputError::Open(e.to_string()))?;

        Ok(VirtualKeyboard {
            device,
            pressed: HashSet::new(),
            hold: Duration::ZERO,
        })
    }

    /// Set how long keys are held between press and release in [`Self::tap`].
    pub fn set_hold(&mut self, hold: Duration) {
        self.hold = hold;
    }

    /// Emit a list of input events, flushing with a `SYN_REPORT`.
    fn emit(&mut self, events: &[InputEvent]) -> Result<(), InputError> {
        self.device
            .emit(events)
            .map_err(|e| InputError::Emit(e.to_string()))
    }

    fn syn() -> InputEvent {
        InputEvent::new(EventType::SYNCHRONIZATION.0, SYN_REPORT, 0)
    }

    fn set_key_down(&mut self, key: KeyCode, down: bool) {
        if down {
            self.pressed.insert(key);
        } else {
            self.pressed.remove(&key);
        }
    }

    /// Press (and hold) a chord: modifier down, then base key down.
    pub fn press(&mut self, chord: KeyChord) -> Result<(), InputError> {
        let (modifier, base) = resolve(chord).ok_or(InputError::UnknownKey(chord))?;

        let mut events = Vec::with_capacity(4);
        if let Some(m) = modifier {
            events.push(InputEvent::new(EventType::KEY.0, m.code(), 1));
            self.set_key_down(m, true);
        }
        events.push(InputEvent::new(EventType::KEY.0, base.code(), 1));
        self.set_key_down(base, true);
        events.push(Self::syn());

        self.emit(&events)
    }

    /// Release a chord: base key up, then modifier up.
    pub fn release(&mut self, chord: KeyChord) -> Result<(), InputError> {
        let (modifier, base) = resolve(chord).ok_or(InputError::UnknownKey(chord))?;

        let mut events = Vec::with_capacity(4);
        events.push(InputEvent::new(EventType::KEY.0, base.code(), 0));
        self.set_key_down(base, false);
        if let Some(m) = modifier {
            events.push(InputEvent::new(EventType::KEY.0, m.code(), 0));
            self.set_key_down(m, false);
        }
        events.push(Self::syn());

        self.emit(&events)
    }

    /// Press then release a chord, holding for the configured [`Self::set_hold`]
    /// duration in between. This mirrors the reference player's "press-release
    /// per note" behavior.
    pub fn tap(&mut self, chord: KeyChord) -> Result<(), InputError> {
        self.press(chord)?;
        if !self.hold.is_zero() {
            std::thread::sleep(self.hold);
        }
        self.release(chord)
    }

    /// Release every key that is still pressed.
    pub fn release_all(&mut self) -> Result<(), InputError> {
        let keys: Vec<KeyCode> = self.pressed.iter().copied().collect();
        let mut events = Vec::with_capacity(keys.len() + 1);
        for key in keys {
            events.push(InputEvent::new(EventType::KEY.0, key.code(), 0));
            self.pressed.remove(&key);
        }
        if events.is_empty() {
            return Ok(());
        }
        events.push(Self::syn());
        self.emit(&events)
    }
}

impl Drop for VirtualKeyboard {
    fn drop(&mut self) {
        // Best-effort release of any held keys when the device is torn down.
        let _ = self.release_all();
    }
}

/// Map a character to its Linux keycode. Linux keycodes follow the physical
/// (scan-code) layout, not alphabetical order, so this must be an explicit table.
pub fn key_for_char(c: char) -> Option<KeyCode> {
    match c.to_ascii_lowercase() {
        'a' => Some(KeyCode::KEY_A),
        'b' => Some(KeyCode::KEY_B),
        'c' => Some(KeyCode::KEY_C),
        'd' => Some(KeyCode::KEY_D),
        'e' => Some(KeyCode::KEY_E),
        'f' => Some(KeyCode::KEY_F),
        'g' => Some(KeyCode::KEY_G),
        'h' => Some(KeyCode::KEY_H),
        'i' => Some(KeyCode::KEY_I),
        'j' => Some(KeyCode::KEY_J),
        'k' => Some(KeyCode::KEY_K),
        'l' => Some(KeyCode::KEY_L),
        'm' => Some(KeyCode::KEY_M),
        'n' => Some(KeyCode::KEY_N),
        'o' => Some(KeyCode::KEY_O),
        'p' => Some(KeyCode::KEY_P),
        'q' => Some(KeyCode::KEY_Q),
        'r' => Some(KeyCode::KEY_R),
        's' => Some(KeyCode::KEY_S),
        't' => Some(KeyCode::KEY_T),
        'u' => Some(KeyCode::KEY_U),
        'v' => Some(KeyCode::KEY_V),
        'w' => Some(KeyCode::KEY_W),
        'x' => Some(KeyCode::KEY_X),
        'y' => Some(KeyCode::KEY_Y),
        'z' => Some(KeyCode::KEY_Z),
        '0' => Some(KeyCode::KEY_0),
        '1' => Some(KeyCode::KEY_1),
        '2' => Some(KeyCode::KEY_2),
        '3' => Some(KeyCode::KEY_3),
        '4' => Some(KeyCode::KEY_4),
        '5' => Some(KeyCode::KEY_5),
        '6' => Some(KeyCode::KEY_6),
        '7' => Some(KeyCode::KEY_7),
        '8' => Some(KeyCode::KEY_8),
        '9' => Some(KeyCode::KEY_9),
        ';' => Some(KeyCode::KEY_SEMICOLON),
        ',' => Some(KeyCode::KEY_COMMA),
        '.' => Some(KeyCode::KEY_DOT),
        '/' => Some(KeyCode::KEY_SLASH),
        _ => None,
    }
}

/// Resolve a [`KeyChord`] into `(modifier, base)` keycodes.
///
/// `Shift` chords use `KEY_LEFTSHIFT`, `Ctrl` chords use `KEY_LEFTCTRL`.
pub fn resolve(chord: KeyChord) -> Option<(Option<KeyCode>, KeyCode)> {
    let base = match chord {
        KeyChord::Key(c) | KeyChord::Shift(c) | KeyChord::Ctrl(c) => key_for_char(c),
    }?;
    let modifier = match chord {
        KeyChord::Key(_) => None,
        KeyChord::Shift(_) => Some(KeyCode::KEY_LEFTSHIFT),
        KeyChord::Ctrl(_) => Some(KeyCode::KEY_LEFTCTRL),
    };
    Some((modifier, base))
}

/// Every keycode the device supports: all mappable characters plus modifiers.
fn all_supported_keys() -> Vec<KeyCode> {
    let mut keys: Vec<KeyCode> = "abcdefghijklmnopqrstuvwxyz0123456789;,./"
        .chars()
        .filter_map(key_for_char)
        .collect();
    keys.extend([
        KeyCode::KEY_LEFTSHIFT,
        KeyCode::KEY_RIGHTSHIFT,
        KeyCode::KEY_LEFTCTRL,
        KeyCode::KEY_RIGHTCTRL,
        KeyCode::KEY_LEFTALT,
        KeyCode::KEY_RIGHTALT,
    ]);
    keys
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_map_to_correct_linux_keycodes() {
        // Linux keycodes are scan-code based, not alphabetical.
        assert_eq!(key_for_char('a'), Some(KeyCode::KEY_A));
        assert_eq!(key_for_char('b'), Some(KeyCode::KEY_B));
        assert_eq!(key_for_char('z'), Some(KeyCode::KEY_Z));
        assert_eq!(key_for_char('q'), Some(KeyCode::KEY_Q));
    }

    #[test]
    fn letters_do_not_follow_alphabetical_order() {
        assert_ne!(KeyCode::KEY_B.code(), KeyCode::KEY_A.code() + 1);
        assert_eq!(KeyCode::KEY_S.code(), KeyCode::KEY_A.code() + 1); // A=30, S=31 on QWERTY
    }

    #[test]
    fn digits_map_to_keypad_row() {
        assert_eq!(key_for_char('0'), Some(KeyCode::KEY_0));
        assert_eq!(key_for_char('7'), Some(KeyCode::KEY_7));
    }

    #[test]
    fn resolve_applies_modifiers() {
        assert_eq!(resolve(KeyChord::Key('a')), Some((None, KeyCode::KEY_A)));
        assert_eq!(
            resolve(KeyChord::Shift('a')),
            Some((Some(KeyCode::KEY_LEFTSHIFT), KeyCode::KEY_A))
        );
        assert_eq!(
            resolve(KeyChord::Ctrl('j')),
            Some((Some(KeyCode::KEY_LEFTCTRL), KeyCode::KEY_J))
        );
    }

    #[test]
    fn unknown_key_resolves_to_none() {
        assert_eq!(key_for_char('!'), None);
        assert_eq!(resolve(KeyChord::Key('!')), None);
    }

    /// Integration check: creates a real virtual device via `/dev/uinput`.
    ///
    /// Ignored by default because it requires write access to `/dev/uinput`
    /// (root, a `uinput` group membership, or a per-user ACL). Run explicitly:
    ///
    /// ```sh
    /// cargo test -p wwm-input -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore]
    fn creates_virtual_device_via_uinput() {
        let device = VirtualKeyboard::create("wwm-test").expect("create virtual device");
        drop(device);
    }
}
