//! The protocols the app can connect with.

use server_remote_icons as icons;

/// The protocol a connection speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    Dummy,
    Vnc,
    Spice,
    Ilo,
}

impl Protocol {
    pub const ALL: [Protocol; 4] = [
        Protocol::Dummy,
        Protocol::Vnc,
        Protocol::Spice,
        Protocol::Ilo,
    ];

    fn label(self) -> &'static str {
        match self {
            Protocol::Dummy => "dummy",
            Protocol::Vnc => "vnc",
            Protocol::Spice => "spice",
            Protocol::Ilo => "ilo",
        }
    }

    /// Human-facing name, as shown in the status bar.
    pub fn name(self) -> &'static str {
        match self {
            Protocol::Dummy => "Dummy",
            Protocol::Vnc => "VNC",
            Protocol::Spice => "SPICE",
            Protocol::Ilo => "iLO",
        }
    }

    /// The name spelt out, as on a connection manager card.
    pub fn full_name(self) -> &'static str {
        match self {
            Protocol::Ilo => "HPE iLO",
            other => other.name(),
        }
    }

    pub fn icon(self) -> icons::Icon {
        match self {
            Protocol::Dummy => icons::DIAMOND_OUTLINE,
            Protocol::Vnc => icons::PROTOCOL_VNC,
            Protocol::Spice => icons::PROTOCOL_SPICE,
            Protocol::Ilo => icons::PROTOCOL_ILO,
        }
    }

    /// Default port when the hostname doesn't specify one.
    pub fn default_port(self) -> u16 {
        match self {
            Protocol::Dummy => 0,
            Protocol::Vnc | Protocol::Spice => 5900,
            Protocol::Ilo => 443,
        }
    }
}

impl std::fmt::Display for Protocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_ports_match_each_protocol() {
        assert_eq!(Protocol::Dummy.default_port(), 0);
        assert_eq!(Protocol::Vnc.default_port(), 5900);
        assert_eq!(Protocol::Spice.default_port(), 5900);
        assert_eq!(Protocol::Ilo.default_port(), 443);
    }
}
