//! What a live session supports, and the UI a connector contributes for it.
//!
//! Reported when a session connects and whenever it changes, [`Capabilities`]
//! drives the connection toolbar and how input is handled. Common capabilities
//! are typed fields; anything protocol-specific is expressed declaratively —
//! [`Action`]s, [`ToolbarItem`]s — so the front end can render it without
//! knowing the protocol, and report back which action the user invoked.
//!
//! Values that change while connected (power state, health, traffic) are
//! reported separately as a [`Status`](crate::Status).

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
    /// Input-related actions (e.g. Ctrl+Alt+Del), listed in the Input menu.
    pub input_actions: Vec<Action>,
    /// Entries the connector adds to the connection toolbar, in order.
    pub toolbar: Vec<ToolbarItem>,
}

/// An icon for connector-contributed UI: one of the app's own icons, or image
/// data the connector supplies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Icon {
    Builtin(server_remote_icons::Icon),
    /// An SVG document. Like the built-in icons, it's tinted to the text
    /// colour, so draw it in a single colour.
    Svg(Cow<'static, [u8]>),
    /// An encoded raster image (PNG, JPEG, ...), drawn as-is.
    Raster(Cow<'static, [u8]>),
}

impl From<server_remote_icons::Icon> for Icon {
    fn from(icon: server_remote_icons::Icon) -> Self {
        Icon::Builtin(icon)
    }
}

/// A toolbar entry contributed by a connector.
#[derive(Debug, Clone)]
pub enum ToolbarItem {
    /// A dropdown menu button, e.g. Power or Virtual media.
    Menu {
        /// Stable identifier, so the UI can keep the menu open across updates.
        id: Cow<'static, str>,
        icon: Icon,
        label: Cow<'static, str>,
        items: Vec<MenuItem>,
    },
    /// A button the front end handles itself by opening `url` in the browser,
    /// e.g. a management processor's web interface.
    Link {
        icon: Icon,
        label: Cow<'static, str>,
        url: Cow<'static, str>,
    },
}

#[derive(Debug, Clone)]
pub enum MenuItem {
    Action(Action),
    Submenu {
        label: Cow<'static, str>,
        items: Vec<MenuItem>,
    },
    Separator,
}

/// A protocol-defined action, described declaratively so the front end can
/// render it and report back which one the user invoked, without knowing what
/// the protocol does with it.
#[derive(Debug, Clone)]
pub struct Action {
    /// Stable identifier the connector recognises when the command returns.
    pub id: Cow<'static, str>,
    /// Human-readable label.
    pub label: Cow<'static, str>,
    pub icon: Option<Icon>,
    /// Shows a tick beside the label, for options that are currently on.
    pub checked: bool,
    /// Disabled actions are shown but can't be invoked.
    pub enabled: bool,
    /// How the front end collects input for the action.
    pub control: Control,
    /// When set, the front end asks the user before invoking the action.
    pub confirm: Option<Confirmation>,
}

impl Action {
    /// An enabled push button with no icon or confirmation.
    pub fn button(id: impl Into<Cow<'static, str>>, label: impl Into<Cow<'static, str>>) -> Self {
        Action {
            id: id.into(),
            label: label.into(),
            icon: None,
            checked: false,
            enabled: true,
            control: Control::Button,
            confirm: None,
        }
    }

    pub fn icon(self, icon: impl Into<Icon>) -> Self {
        Action {
            icon: Some(icon.into()),
            ..self
        }
    }

    pub fn checked(self, checked: bool) -> Self {
        Action { checked, ..self }
    }

    pub fn enabled(self, enabled: bool) -> Self {
        Action { enabled, ..self }
    }

    pub fn control(self, control: Control) -> Self {
        Action { control, ..self }
    }

    pub fn confirm(self, confirmation: Confirmation) -> Self {
        Action {
            confirm: Some(confirmation),
            ..self
        }
    }
}

/// The vocabulary of controls a declarative [`Action`] may use.
///
/// Intentionally small; it grows as protocols need richer controls.
#[derive(Debug, Clone)]
pub enum Control {
    /// A push button; invoking it emits the action with no value.
    Button,
    /// Asks the user to pick a local file, which is sent as
    /// [`ActionValue::File`](crate::ActionValue::File). `extensions` filters
    /// the picker (without dots, e.g. `"iso"`); empty allows any file.
    File { extensions: Vec<Cow<'static, str>> },
}

/// The question asked before invoking an [`Action`].
#[derive(Debug, Clone)]
pub struct Confirmation {
    pub title: Cow<'static, str>,
    pub message: Cow<'static, str>,
    /// Label of the button that goes ahead, e.g. "Shut down".
    pub confirm_label: Cow<'static, str>,
    /// Styles the confirming button as dangerous, for actions that can lose
    /// work (e.g. cutting power).
    pub destructive: bool,
}
