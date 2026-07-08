//! What a live session supports.
//!
//! Reported when a session connects and whenever it changes, [`Capabilities`]
//! drives the connection toolbar and how input is handled. Common capabilities
//! are typed fields; anything protocol-specific is expressed declaratively as an
//! [`Action`] so the front end can render it without knowing the protocol.
//!
//! When the declarative vocabulary can't express something a protocol needs,
//! the intent is to add an opt-in trait for plugins to provide their own UI —
//! but that stays out of here so protocols that don't need it pull no UI
//! dependency.

use std::borrow::Cow;

/// How the server tracks pointer position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum PointerMode {
    /// The server tracks an absolute cursor position; the UI sends
    /// [`InputEvent::PointerMoved`](crate::InputEvent::PointerMoved).
    #[default]
    Absolute,
    /// The server expects relative motion; the UI grabs the pointer, hides the
    /// local cursor, and sends
    /// [`InputEvent::PointerMotion`](crate::InputEvent::PointerMotion) deltas.
    Relative,
}

/// What a live session supports.
#[derive(Debug, Clone, Default)]
pub struct Capabilities {
    pub pointer: PointerMode,
    /// Protocol-specific actions to surface in the connection toolbar.
    pub actions: Vec<Action>,
}

/// A protocol-defined action, described declaratively so the front end can
/// render it and report back which one the user invoked, without knowing what
/// the protocol does with it.
#[derive(Debug, Clone)]
pub struct Action {
    /// Stable identifier the plugin recognises when the command returns.
    pub id: Cow<'static, str>,
    /// Human-readable label.
    pub label: Cow<'static, str>,
    /// How the front end renders and collects input for the action.
    pub control: Control,
}

/// The vocabulary of controls a declarative [`Action`] may use.
///
/// Intentionally small. It grows (`Toggle`, `File`, `Choose`, ...) as protocols
/// that need richer controls — Spice/iLO media mounting — are implemented.
#[derive(Debug, Clone)]
pub enum Control {
    /// A push button; invoking it emits the action with no value.
    Button,
}
