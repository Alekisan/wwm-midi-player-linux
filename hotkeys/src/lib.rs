//! Wayland global hotkeys via the XDG Desktop Portal `GlobalShortcuts` interface
//! (the `ashpd` crate).
//!
//! Phase 3 of the project: register the player's transport controls
//! (Play/Pause and Stop) as system-wide shortcuts through the portal, so they
//! work regardless of which window is focused. The shortcuts themselves are
//! supplied as *preferred triggers*; the portal backend (KDE Plasma 6) is free
//! to let the user remap them.

use ashpd::desktop::global_shortcuts::{Activated, GlobalShortcuts, NewShortcut};
use ashpd::desktop::Session;
use futures_util::Stream;
use std::fmt;
use thiserror::Error;

/// Application-provided shortcut IDs, stable identifiers for the portal.
pub const PLAY_PAUSE_ID: &str = "play_pause";
pub const STOP_ID: &str = "stop";

/// Preferred trigger for the Play/Pause shortcut (XKB keysym notation).
pub const PLAY_PAUSE_TRIGGER: &str = "CTRL+ALT+P";
/// Preferred trigger for the Stop shortcut (XKB keysym notation).
pub const STOP_TRIGGER: &str = "CTRL+ALT+S";

/// A transport control produced by a global shortcut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportCommand {
    PlayPause,
    Stop,
}

impl fmt::Display for TransportCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransportCommand::PlayPause => write!(f, "play/pause"),
            TransportCommand::Stop => write!(f, "stop"),
        }
    }
}

/// Errors from the global-shortcut layer.
#[derive(Debug, Error)]
pub enum HotkeyError {
    #[error("xdg-desktop-portal not available: {0}")]
    Portal(String),
    #[error("failed to bind shortcuts: {0}")]
    Bind(String),
    #[error("failed to receive shortcut signals: {0}")]
    Signals(String),
}

/// A live global-shortcuts session with the player's shortcuts bound.
pub struct PlayerShortcuts {
    portal: GlobalShortcuts,
    // The session must be kept alive for the shortcuts to remain active.
    _session: Session<GlobalShortcuts>,
}

impl PlayerShortcuts {
    /// Create a portal session and bind the Play/Pause and Stop shortcuts.
    ///
    /// On first run the portal backend may prompt the user to approve the
    /// requested bindings.
    pub async fn register() -> Result<Self, HotkeyError> {
        let portal = GlobalShortcuts::new()
            .await
            .map_err(|e| HotkeyError::Portal(e.to_string()))?;

        let session = portal
            .create_session(Default::default())
            .await
            .map_err(|e| HotkeyError::Portal(e.to_string()))?;

        let shortcuts = [
            NewShortcut::new(PLAY_PAUSE_ID, "Play / Pause")
                .preferred_trigger(Some(PLAY_PAUSE_TRIGGER)),
            NewShortcut::new(STOP_ID, "Stop").preferred_trigger(Some(STOP_TRIGGER)),
        ];

        let request = portal
            .bind_shortcuts(&session, &shortcuts, None, Default::default())
            .await
            .map_err(|e| HotkeyError::Bind(e.to_string()))?;

        // Best-effort logging of the bindings the portal actually assigned.
        if let Ok(response) = request.response() {
            for shortcut in response.shortcuts() {
                eprintln!(
                    "[hotkeys] bound '{}' ({}) to '{}'",
                    shortcut.id(),
                    shortcut.description(),
                    shortcut.trigger_description()
                );
            }
        }

        Ok(PlayerShortcuts {
            portal,
            _session: session,
        })
    }

    /// Stream of shortcut activations for this session.
    pub async fn activated(&self) -> Result<impl Stream<Item = Activated> + '_, HotkeyError> {
        self.portal
            .receive_activated()
            .await
            .map_err(|e| HotkeyError::Signals(e.to_string()))
    }
}

/// Map a shortcut ID to its transport command.
pub fn command_for(id: &str) -> Option<TransportCommand> {
    match id {
        PLAY_PAUSE_ID => Some(TransportCommand::PlayPause),
        STOP_ID => Some(TransportCommand::Stop),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_known_shortcut_ids() {
        assert_eq!(
            command_for(PLAY_PAUSE_ID),
            Some(TransportCommand::PlayPause)
        );
        assert_eq!(command_for(STOP_ID), Some(TransportCommand::Stop));
    }

    #[test]
    fn unknown_id_maps_to_none() {
        assert_eq!(command_for("next_track"), None);
        assert_eq!(command_for(""), None);
    }

    #[test]
    fn commands_display_readably() {
        assert_eq!(TransportCommand::PlayPause.to_string(), "play/pause");
        assert_eq!(TransportCommand::Stop.to_string(), "stop");
    }
}
