//! What's going on in a live session right now.
//!
//! A connector reports a whole [`Status`] snapshot whenever something in it
//! changes (and periodically, if it tracks traffic), and the front end
//! re-renders the status bar, toolbar chips and info popout from it.

use std::borrow::Cow;

use crate::Icon;

#[derive(Debug, Clone, Default)]
pub struct Status {
    /// The transport's encryption, e.g. `"TLS 1.3"` or `"RC4"`. `None` means
    /// unencrypted or unknown.
    pub encryption: Option<Cow<'static, str>>,
    /// Cumulative bytes moved since connecting. The front end derives rates
    /// from consecutive snapshots.
    pub traffic: Option<Traffic>,
    /// Protocol-specific status bar entries, in order.
    pub items: Vec<StatusItem>,
    /// Ongoing things shown as chips in the toolbar, e.g. a mounted image.
    pub chips: Vec<Chip>,
    /// Extra rows for the connection info popout.
    pub details: Vec<Detail>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Traffic {
    pub received: u64,
    pub sent: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatusItem {
    /// A coloured dot and label, e.g. "Powered on" or "Health".
    Indicator {
        label: Cow<'static, str>,
        state: IndicatorState,
    },
    /// Plain text with an optional icon, e.g. "POST: 5000".
    Text {
        icon: Option<Icon>,
        text: Cow<'static, str>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndicatorState {
    Ok,
    Warning,
    Error,
    /// Neither good nor bad, e.g. a POST code or an idle activity light.
    Inactive,
}

/// An ongoing thing shown in the toolbar, optionally dismissable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chip {
    pub icon: Option<Icon>,
    pub label: Cow<'static, str>,
    /// The [`Action`](crate::Action) id sent when the chip's close button is
    /// pressed; `None` hides the button.
    pub close_action: Option<Cow<'static, str>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Detail {
    pub label: Cow<'static, str>,
    pub value: Cow<'static, str>,
}
