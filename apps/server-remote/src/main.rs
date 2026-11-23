use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use iced::{
    Alignment, Element, Length, Subscription, Task, padding,
    widget::{Column, Row, Scrollable, Space, button, container, mouse_area, stack, text, tooltip},
    window,
};
use server_remote_core::{SessionEvent, Target};
use server_remote_icons as icons;

use crate::i18n::fl;
use crate::persistence::settings::{Appearance, GraphicsBackend, KnownCertificate, Settings};
use crate::tooltip_box::tooltip_box;

mod app_icon;
mod contributors;
mod i18n;
mod persistence;
mod tooltip_box;
mod ui;
mod update;

/// Split a `host[:port]` string, falling back to the protocol's default port.
fn parse_target(hostname: &str, protocol: ui::connection::Protocol) -> Target {
    let default_port = protocol.default_port();
    if hostname.is_empty() {
        return Target::new("localhost", default_port);
    }
    match hostname.rsplit_once(':') {
        Some((host, port)) => match port.parse::<u16>() {
            Ok(port) => Target::new(host, port),
            Err(_) => Target::new(hostname, default_port),
        },
        None => Target::new(hostname, default_port),
    }
}

/// Builds a [`persistence::sessions::SavedSession`] from a connect-tab
/// request, falling back to the hostname and then the protocol label when the
/// user left the name field blank.
fn saved_session_from_request(
    request: ui::connect::Request,
) -> persistence::sessions::SavedSession {
    let name = if !request.name.trim().is_empty() {
        request.name.trim().to_owned()
    } else if !request.hostname.is_empty() {
        request.hostname.clone()
    } else {
        request.protocol.to_string()
    };

    persistence::sessions::SavedSession {
        id: 0,
        name,
        group: request.group,
        protocol: request.protocol,
        hostname: request.hostname,
        username: request.username,
        password: request.password,
    }
}

/// A file name to suggest when exporting the session called `name`, safe
/// on every platform: characters some file systems refuse become dashes,
/// trailing dots and spaces (which Windows drops) go, and Windows' reserved
/// device names get a suffix.
fn export_file_name(name: &str) -> String {
    const RESERVED: [&str; 22] = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];

    let cleaned: String = name
        .chars()
        .map(|c| {
            if r#"/\:*?"<>|"#.contains(c) || c.is_control() {
                '-'
            } else {
                c
            }
        })
        .collect();
    let mut stem = cleaned.trim().trim_end_matches(['.', ' ']).to_owned();
    if stem.is_empty() {
        stem = "connection".to_owned();
    }
    // Windows reserves these even with an extension, as in `CON.toml` or
    // `con.backup.toml`.
    let base = stem.split('.').next().unwrap_or_default().trim_end();
    if RESERVED
        .iter()
        .any(|reserved| base.eq_ignore_ascii_case(reserved))
    {
        stem.push_str("-connection");
    }
    format!("{stem}.toml")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_file_names_are_safe_on_windows() {
        assert_eq!(export_file_name("Aur"), "Aur.toml");
        assert_eq!(export_file_name("a/b: c?"), "a-b- c-.toml");
        assert_eq!(export_file_name("Server. "), "Server.toml");
        assert_eq!(export_file_name("..."), "connection.toml");
        assert_eq!(export_file_name("CON"), "CON-connection.toml");
        assert_eq!(export_file_name("nul.old"), "nul.old-connection.toml");
        assert_eq!(export_file_name("Console"), "Console.toml");
    }

    #[test]
    fn a_blank_name_saves_as_the_hostname() {
        let request = ui::connect::Request {
            protocol: ui::connection::Protocol::Vnc,
            name: "   ".to_owned(),
            group: String::new(),
            hostname: "nas.local".to_owned(),
            username: String::new(),
            password: String::new(),
        };
        assert_eq!(saved_session_from_request(request).name, "nas.local");
    }

    #[test]
    fn empty_hostname_falls_back_to_localhost_and_default_port() {
        let target = parse_target("", ui::connection::Protocol::Vnc);
        assert_eq!(target, Target::new("localhost", 5900));
    }

    #[test]
    fn host_without_port_uses_the_protocol_default() {
        let target = parse_target("example.com", ui::connection::Protocol::Ilo);
        assert_eq!(target, Target::new("example.com", 443));
    }

    #[test]
    fn host_with_valid_port_overrides_the_default() {
        let target = parse_target("example.com:2222", ui::connection::Protocol::Vnc);
        assert_eq!(target, Target::new("example.com", 2222));
    }

    #[test]
    fn host_with_unparsable_port_keeps_the_whole_string_as_host() {
        let target = parse_target("example.com:not-a-port", ui::connection::Protocol::Vnc);
        assert_eq!(target, Target::new("example.com:not-a-port", 5900));
    }

    #[test]
    fn host_with_out_of_range_port_keeps_the_whole_string_as_host() {
        let target = parse_target("example.com:99999", ui::connection::Protocol::Vnc);
        assert_eq!(target, Target::new("example.com:99999", 5900));
    }

    fn request(hostname: &str) -> ui::connect::Request {
        ui::connect::Request {
            protocol: ui::connection::Protocol::Dummy,
            name: String::new(),
            group: String::new(),
            hostname: hostname.to_owned(),
            username: String::new(),
            password: String::new(),
        }
    }

    #[test]
    fn open_connection_assigns_increasing_ids_and_switches_to_the_new_tab() {
        let (mut app, _) = Application::new(None, Settings::default());

        let _ = app.open_connection(request("first"));
        let _ = app.open_connection(request("second"));

        assert_eq!(app.connections.len(), 2);
        assert_eq!(app.connections[0].id(), 0);
        assert_eq!(app.connections[1].id(), 1);
        assert_eq!(app.tab, Tab::Connection(1));
    }

    #[test]
    fn open_connection_names_an_empty_hostname_after_the_protocol_and_id() {
        let (mut app, _) = Application::new(None, Settings::default());

        let _ = app.open_connection(request(""));

        assert_eq!(app.connections[0].name().as_ref(), "dummy #0");
    }

    #[test]
    fn close_connection_removes_it_and_returns_to_the_connect_tab() {
        let (mut app, _) = Application::new(None, Settings::default());
        let _ = app.open_connection(request("only"));
        assert_eq!(app.tab, Tab::Connection(0));

        let _ = app.close_connection(0);

        assert!(app.connections.is_empty());
        assert_eq!(app.tab, Tab::Connect);
    }

    #[test]
    fn close_connection_leaves_an_unrelated_active_tab_alone() {
        let (mut app, _) = Application::new(None, Settings::default());
        let _ = app.open_connection(request("first"));
        let _ = app.open_connection(request("second"));
        app.tab = Tab::Manager;

        let _ = app.close_connection(0);

        assert_eq!(app.connections.len(), 1);
        assert_eq!(app.tab, Tab::Manager);
    }

    #[test]
    fn saved_session_uses_the_provided_name_when_set() {
        let mut req = request("example.com");
        req.name = "Home NAS".to_owned();

        let saved = saved_session_from_request(req);

        assert_eq!(saved.name, "Home NAS");
        assert_eq!(saved.hostname, "example.com");
    }

    #[test]
    fn saved_session_falls_back_to_hostname_when_name_is_blank() {
        let saved = saved_session_from_request(request("example.com"));
        assert_eq!(saved.name, "example.com");
    }

    /// Renders the window chrome to PNGs for comparing against the design.
    /// Run with `SNAPSHOT_DIR=<dir> cargo test -p server-remote -- --ignored`.
    /// Capabilities shaped like the iLO mockups, for review snapshots.
    fn ilo_like_capabilities() -> server_remote_core::Capabilities {
        use server_remote_core::{
            Action, Capabilities, Confirmation, Control, MenuItem, ToolbarItem,
        };
        use server_remote_icons as icons;

        let power = |id: &'static str, label: &'static str, icon, verb: &'static str, hard| {
            MenuItem::Action(Action::button(id, label).icon(icon).confirm(Confirmation {
                title: format!("{verb}?").into(),
                message: if hard {
                    "Cuts power immediately. Unsaved work will be lost.".into()
                } else {
                    "Asks the operating system to do this gracefully.".into()
                },
                confirm_label: verb.into(),
                destructive: hard,
            }))
        };
        Capabilities {
            input_actions: vec![Action::button("cad", "Ctrl+Alt+Delete").icon(icons::RESTART)],
            toolbar: vec![
                ToolbarItem::Menu {
                    id: "power".into(),
                    icon: icons::POWER.into(),
                    label: "Power".into(),
                    items: vec![
                        power(
                            "power.shut-down",
                            "Shut down…",
                            icons::POWER,
                            "Shut down",
                            false,
                        ),
                        power(
                            "power.restart",
                            "Restart…",
                            icons::RESTART,
                            "Restart",
                            false,
                        ),
                        MenuItem::Separator,
                        power("power.off", "Power off…", icons::POWER, "Power off", true),
                        power(
                            "power.cold-boot",
                            "Cold boot…",
                            icons::RESTART,
                            "Cold boot",
                            true,
                        ),
                    ],
                },
                ToolbarItem::Menu {
                    id: "media".into(),
                    icon: icons::VMDK_DISK.into(),
                    label: "Virtual media".into(),
                    items: vec![MenuItem::Action(
                        Action::button("media.mount", "Mount image…").control(Control::File {
                            extensions: vec!["iso".into()],
                        }),
                    )],
                },
                ToolbarItem::Link {
                    icon: icons::LAUNCH.into(),
                    label: "Open iLO in web browser".into(),
                    url: "https://ilo.invalid".into(),
                },
            ],
            ..Capabilities::default()
        }
    }

    /// Connects the first tab to an iLO-like session, with two status
    /// samples far enough apart for traffic rates to show.
    fn feed_ilo_like_session(app: &mut Application) {
        use server_remote_core::{Chip, IndicatorState, SessionEvent, Status, StatusItem, Traffic};
        use server_remote_icons as icons;

        let status = |received, sent| Status {
            encryption: Some("RC4".into()),
            traffic: Some(Traffic { received, sent }),
            items: vec![
                StatusItem::Indicator {
                    label: "Powered on".into(),
                    state: IndicatorState::Ok,
                },
                StatusItem::Indicator {
                    label: "POST: 5000".into(),
                    state: IndicatorState::Inactive,
                },
                StatusItem::Indicator {
                    label: "ACT".into(),
                    state: IndicatorState::Ok,
                },
                StatusItem::Indicator {
                    label: "Health".into(),
                    state: IndicatorState::Warning,
                },
            ],
            chips: vec![Chip {
                icon: Some(icons::PROGRESS_BAR_ROUND.into()),
                label: "proxmox-ve_9.2-1.iso".into(),
                close_action: Some("media.eject".into()),
            }],
            details: Vec::new(),
        };
        let session = |event| Message::Connection(0, ui::connection::Message::Session(event));

        let _ = app.update(session(SessionEvent::Connected {
            size: (1024, 768),
            capabilities: ilo_like_capabilities(),
        }));
        let _ = app.update(session(SessionEvent::Status(status(0, 0))));
        // 150 kb/s down and 95 kb/s up over the 0.6 s between samples.
        std::thread::sleep(std::time::Duration::from_millis(600));
        let _ = app.update(session(SessionEvent::Status(status(11_250, 7_125))));
    }

    /// Saved sessions like the connection manager mockup's, with fixed ids
    /// (1–6) so states can refer to them, and two with thumbnails.
    fn add_saved_sessions(app: &mut Application) {
        use persistence::sessions::{SavedSession, thumbnail};
        use ui::connection::Protocol;

        let sessions = [
            ("Aur", "Homelab", Protocol::Ilo, "10.10.5.120"),
            ("Argint", "Homelab", Protocol::Ilo, "10.10.5.121"),
            (
                "Windows XP",
                "Old PCs",
                Protocol::Vnc,
                "winxp.dhcp.internal",
            ),
            ("Windows 7", "Old PCs", Protocol::Vnc, "win7.dhcp.internal"),
            (
                "Mac OS X Snow Leopard",
                "Old PCs",
                Protocol::Vnc,
                "snowleopard.dhcp.internal",
            ),
            (
                "Debian Dev VM",
                "Proxmox VMs",
                Protocol::Spice,
                "debdev.vms.cloud",
            ),
        ];
        for (index, (name, group, protocol, hostname)) in sessions.into_iter().enumerate() {
            app.sessions.saved.push(SavedSession {
                id: index as u64 + 1,
                name: name.to_owned(),
                group: group.to_owned(),
                protocol,
                hostname: hostname.to_owned(),
                username: String::new(),
                password: String::new(),
            });
        }
        let desktop = containerdesk_frame();
        let png = thumbnail::encode(desktop.width, desktop.height, &desktop.pixels);
        for id in [2, 5] {
            app.manager.set_thumbnail(id, png.clone());
        }
    }

    /// The remote desktop from the containerdesk mock's README capture, cut
    /// out of the window around it and scaled to the session's 1024×768.
    fn containerdesk_frame() -> server_remote_core::Frame {
        use image::imageops;

        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../mock/containerdesk/screenshot.png"
        );
        let capture = image::open(path)
            .expect("reads the containerdesk capture")
            .into_rgba8();
        let desktop = imageops::crop_imm(&capture, 205, 272, 1862, 1396).to_image();
        let desktop = imageops::resize(&desktop, 1024, 768, imageops::FilterType::Lanczos3);
        server_remote_core::Frame {
            width: desktop.width(),
            height: desktop.height(),
            pixels: desktop.into_raw(),
        }
    }

    #[test]
    #[ignore = "writes review snapshots; needs SNAPSHOT_DIR"]
    fn render_review_snapshots() {
        let dir =
            std::path::PathBuf::from(std::env::var("SNAPSHOT_DIR").expect("SNAPSHOT_DIR is set"));
        let settings = ui::style::settings();

        use ui::connection::{Message as Conn, ToolbarMenu};

        let power_off = ilo_like_capabilities()
            .toolbar
            .into_iter()
            .find_map(|item| match item {
                server_remote_core::ToolbarItem::Menu { items, .. } => {
                    items.into_iter().find_map(|item| match item {
                        server_remote_core::MenuItem::Action(action)
                            if action.id == "power.off" =>
                        {
                            Some(action)
                        }
                        _ => None,
                    })
                }
                server_remote_core::ToolbarItem::Link { .. } => None,
            })
            .expect("the power menu has a power-off action");

        let conn = |message| Message::Connection(0, message);
        let settings_page = |section, select| {
            vec![
                Message::SelectTab(Tab::Settings),
                Message::Settings(ui::settings::Message::Section(section)),
                Message::Settings(ui::settings::Message::Toggle(select)),
            ]
        };

        // (name, whether a connector session is fed in first, messages)
        let mut states: Vec<(&str, bool, Vec<Message>)> = vec![
            ("window", false, vec![]),
            ("connector", true, vec![]),
            (
                "power-menu",
                true,
                vec![conn(Conn::ToggleMenu(ToolbarMenu::Contributed(
                    "power".into(),
                )))],
            ),
            ("confirm", true, vec![conn(Conn::Invoke(power_off))]),
            (
                "display-menu",
                false,
                vec![conn(Conn::ToggleMenu(ToolbarMenu::Display))],
            ),
            (
                "input-menu",
                false,
                vec![
                    conn(Conn::ToggleMenu(ToolbarMenu::Input)),
                    conn(Conn::OpenSubmenu(ui::connection::Submenu::KeyboardStyle)),
                ],
            ),
            (
                "keyboard",
                false,
                vec![conn(Conn::SetKeyboardSize(
                    ui::connection::KeyboardSize::Large,
                ))],
            ),
            (
                "recording-menu",
                false,
                vec![
                    conn(Conn::ToggleMenu(ToolbarMenu::Recording)),
                    conn(Conn::OpenSubmenu(ui::connection::Submenu::RecordingFormat)),
                ],
            ),
            (
                "settings-input",
                false,
                settings_page(
                    ui::settings::Section::Input,
                    ui::settings::Select::KeyboardLayout,
                ),
            ),
            (
                "settings-recording",
                false,
                settings_page(
                    ui::settings::Section::Recording,
                    ui::settings::Select::RecordingFolder,
                ),
            ),
            (
                "quit",
                false,
                vec![Message::RequestClose(window::Id::unique())],
            ),
            (
                // The Settings tab is shorter than the window; the prompt's
                // backdrop must still cover all of it.
                "quit-over-settings",
                false,
                vec![
                    Message::SelectTab(Tab::Settings),
                    Message::RequestClose(window::Id::unique()),
                ],
            ),
        ];

        let about = vec![
            Message::SelectTab(Tab::Settings),
            Message::Settings(ui::settings::Message::Section(ui::settings::Section::About)),
        ];
        states.push(("settings-about", false, about.clone()));
        states.push(("settings-about-mockup", false, about.clone()));
        states.push((
            "settings-about-licences",
            false,
            about
                .iter()
                .cloned()
                .chain([Message::Settings(
                    ui::settings::Message::ShowThirdPartyLicences,
                )])
                .collect(),
        ));
        states.push((
            "settings-about-update",
            false,
            about
                .iter()
                .cloned()
                .chain([Message::UpdateChecked(Ok(update::Outcome::Available {
                    version: semver::Version::new(0, 2, 0),
                    url: "https://example.invalid/release".to_owned(),
                }))])
                .collect(),
        ));

        // A few in the light theme too.
        let light = |messages: Vec<Message>| -> Vec<Message> {
            std::iter::once(Message::Settings(ui::settings::Message::Change(
                ui::settings::Change::Appearance(Appearance::Light),
            )))
            .chain(messages)
            .collect()
        };
        states.push(("connector-light", true, light(vec![])));
        // The website's screenshots: a live desktop rather than the
        // "waiting for first frame" placeholder.
        let desktop = conn(Conn::Session(server_remote_core::SessionEvent::Frame(
            containerdesk_frame(),
        )));
        states.push(("website", true, vec![desktop.clone()]));
        states.push(("website-light", true, light(vec![desktop])));
        states.push((
            "power-menu-light",
            true,
            light(vec![conn(Conn::ToggleMenu(ToolbarMenu::Contributed(
                "power".into(),
            )))]),
        ));
        states.push((
            "keyboard-light",
            false,
            light(vec![conn(Conn::SetKeyboardSize(
                ui::connection::KeyboardSize::Large,
            ))]),
        ));
        states.push(("settings-about-light", false, light(about.clone())));
        states.push((
            "settings-general-light",
            false,
            light(settings_page(
                ui::settings::Section::General,
                ui::settings::Select::Appearance,
            )),
        ));
        states.push((
            "quit-light",
            false,
            light(vec![Message::RequestClose(window::Id::unique())]),
        ));

        // And in Romanian, whose longer words test the layouts.
        let romanian = |messages: Vec<Message>| -> Vec<Message> {
            std::iter::once(Message::Settings(ui::settings::Message::Change(
                ui::settings::Change::Language(persistence::settings::Language::Romanian),
            )))
            .chain(messages)
            .collect()
        };
        states.push((
            "display-menu-ro",
            false,
            romanian(vec![conn(Conn::ToggleMenu(ToolbarMenu::Display))]),
        ));
        states.push(("connector-ro", true, romanian(vec![])));
        states.push(("settings-about-ro", false, romanian(about.clone())));
        states.push((
            "settings-general-ro",
            false,
            romanian(settings_page(
                ui::settings::Section::General,
                ui::settings::Select::Language,
            )),
        ));
        states.push((
            "settings-recording-ro",
            false,
            romanian(settings_page(
                ui::settings::Section::Recording,
                ui::settings::Select::RecordingQuality,
            )),
        ));
        states.push((
            "quit-ro",
            false,
            romanian(vec![Message::RequestClose(window::Id::unique())]),
        ));

        // The iLO connect form, and the certificate prompt in each theme and
        // language.
        let ilo_form = vec![
            Message::SelectTab(Tab::Connect),
            Message::ConnectTab(ui::connect::Message::Internal(
                ui::connect::IMessage::Protocol(ui::connection::Protocol::Ilo),
            )),
        ];
        let trust = vec![conn(Conn::UntrustedCertificate(
            server_remote_core::CertFingerprint(core::array::from_fn(|i| i as u8 * 7)),
        ))];
        let form =
            |messages: Vec<ui::connect::IMessage>| -> Vec<Message> {
                std::iter::once(Message::SelectTab(Tab::Connect))
                    .chain(messages.into_iter().map(|message| {
                        Message::ConnectTab(ui::connect::Message::Internal(message))
                    }))
                    .collect()
            };
        let vnc = || ui::connect::IMessage::Protocol(ui::connection::Protocol::Vnc);
        states.push(("connect-mockup", false, form(vec![vnc()])));
        states.push((
            "connect-protocols",
            false,
            form(vec![vnc(), ui::connect::IMessage::ToggleProtocols]),
        ));
        states.push(("connect-light", false, light(form(vec![vnc()]))));
        states.push((
            "connect-save",
            false,
            form(vec![
                vnc(),
                ui::connect::IMessage::Hostname("nas.local".to_owned()),
                ui::connect::IMessage::AskName,
            ]),
        ));
        states.push(("connect-ilo", false, ilo_form.clone()));
        states.push(("connect-ilo-ro", false, romanian(ilo_form)));
        states.push(("trust-certificate", false, trust.clone()));
        states.push(("trust-certificate-light", false, light(trust.clone())));
        states.push(("trust-certificate-ro", false, romanian(trust)));

        // A connection that fails before connecting, back on the Connect page.
        let failed = vec![conn(Conn::Session(SessionEvent::Disconnected {
            reason: Some("There is another user using the remote console.".to_owned()),
        }))];
        states.push(("connection-failed", false, failed.clone()));
        states.push(("connection-failed-light", false, light(failed.clone())));
        states.push(("connection-failed-ro", false, romanian(failed)));

        // Bands of the four VGA greys, to check the display shows each
        // value unchanged (no stray gamma).
        let greys = server_remote_core::Frame {
            width: 256,
            height: 64,
            pixels: (0..256 * 64)
                .flat_map(|index: usize| {
                    let grey = [0, 85, 170, 255][index % 256 / 64];
                    [grey, grey, grey, 255]
                })
                .collect(),
        };
        states.push((
            "display-greys",
            false,
            vec![
                conn(Conn::Session(SessionEvent::Connected {
                    size: (256, 64),
                    capabilities: server_remote_core::Capabilities::default(),
                })),
                conn(Conn::Session(SessionEvent::Frame(greys))),
            ],
        ));

        // The Security section with an entry in each list.
        let known = |byte: u8| {
            KnownCertificate::new(
                &Target::new("ilo.example", 443),
                server_remote_core::CertFingerprint(core::array::from_fn(|i| i as u8 ^ byte)),
            )
        };
        let security = vec![
            Message::SelectTab(Tab::Settings),
            Message::Settings(ui::settings::Message::Section(
                ui::settings::Section::Security,
            )),
            Message::Settings(ui::settings::Message::Change(
                ui::settings::Change::TrustCertificate(known(0x5A)),
            )),
            Message::Settings(ui::settings::Message::Change(
                ui::settings::Change::BlockCertificate(known(0xC3)),
            )),
        ];
        let security_menu = security
            .iter()
            .cloned()
            .chain([Message::Settings(ui::settings::Message::Toggle(
                ui::settings::Select::CertificateChecking,
            ))])
            .collect::<Vec<_>>();
        states.push(("settings-security", false, security.clone()));
        states.push(("settings-security-menu", false, security_menu));
        states.push(("settings-security-light", false, light(security.clone())));
        states.push(("settings-security-ro", false, romanian(security.clone())));
        states.push(("settings-security-narrow", false, security));

        // Wide by default; these are also rendered at a narrow width.
        const NARROW: [&str; 2] = ["settings-display-narrow", "settings-security-narrow"];
        states.push((
            "settings-display-narrow",
            false,
            vec![
                Message::SelectTab(Tab::Settings),
                Message::Settings(ui::settings::Message::Section(
                    ui::settings::Section::Display,
                )),
            ],
        ));

        // The connection manager, filled in by `add_saved_sessions`, except
        // in `manager-empty`.
        let manager = |messages: Vec<ui::manager::IMessage>| -> Vec<Message> {
            std::iter::once(Message::SelectTab(Tab::Manager))
                .chain(
                    messages
                        .into_iter()
                        .map(|message| Message::Manager(ui::manager::Message::Internal(message))),
                )
                .collect()
        };
        {
            use ui::manager::IMessage as M;
            states.push(("manager", false, manager(vec![])));
            states.push(("manager-mockup", false, manager(vec![])));
            states.push(("manager-light", false, light(manager(vec![]))));
            states.push(("manager-ro", false, romanian(manager(vec![]))));
            states.push(("manager-narrow", false, manager(vec![])));
            states.push(("manager-empty", false, manager(vec![])));
            states.push((
                "manager-group",
                false,
                manager(vec![M::SelectGroup(Some("Old PCs".to_owned()))]),
            ));
            states.push(("manager-menu", false, manager(vec![M::OpenMenu(2)])));
            states.push((
                "manager-menu-ro",
                false,
                romanian(manager(vec![M::OpenMenu(2)])),
            ));
            states.push((
                "manager-group-menu",
                false,
                manager(vec![M::OpenMenu(1), M::OpenGroupMenu]),
            ));
            states.push((
                "manager-group-menu-ro",
                false,
                romanian(manager(vec![M::OpenMenu(1), M::OpenGroupMenu])),
            ));
            states.push((
                "manager-new-group",
                false,
                manager(vec![M::AskNewGroup(1), M::GroupName("Lab rack".to_owned())]),
            ));
            states.push(("manager-edit", false, manager(vec![M::Edit(1)])));
            states.push((
                "manager-edit-light",
                false,
                light(manager(vec![M::Edit(1)])),
            ));
            states.push(("manager-delete", false, manager(vec![M::AskDelete(3)])));
            states.push((
                "manager-delete-ro",
                false,
                romanian(manager(vec![M::AskDelete(3)])),
            ));
        }

        for (name, with_session, messages) in states {
            let size = match name {
                // The Figma mockups' frames.
                "settings-about-mockup" => (800.0, 292.0),
                "manager-mockup" => (800.0, 611.0),
                "connect-mockup" => (800.0, 640.0),
                name if NARROW.contains(&name) || name == "manager-narrow" => (640.0, 680.0),
                _ => (1024.0, 768.0),
            };
            let (mut app, _) = Application::new(None, Settings::default());
            let _ = app.open_connection(request("Connection #1"));
            let _ = app.open_connection(request("Connection #2"));
            app.tab = Tab::Connection(0);
            if name.starts_with("manager") && name != "manager-empty" {
                add_saved_sessions(&mut app);
            }
            // Snapshots render at 2x. `SNAPSHOT_SNAP_SCALE=1` snaps the layout
            // for 1x displays instead: anything then on an odd pixel would sit
            // between two at 1x.
            let snap_scale = std::env::var("SNAPSHOT_SNAP_SCALE")
                .ok()
                .and_then(|scale| scale.parse().ok())
                .unwrap_or(2.0);
            let _ = app.update(Message::Rescaled(snap_scale));
            if with_session {
                feed_ilo_like_session(&mut app);
            }
            for message in messages {
                let _ = app.update(message);
            }
            let mut simulator = iced_test::Simulator::with_size(settings.clone(), size, app.view());
            // Raster images decode on a worker thread and are missing from
            // the first frame, so render once, give decoding a moment, and
            // keep the second frame.
            let _ = simulator.snapshot(&app.theme()).expect("renders");
            std::thread::sleep(std::time::Duration::from_millis(250));
            let snapshot = simulator.snapshot(&app.theme()).expect("renders");
            // `matches_image` only writes when no snapshot exists yet, and
            // names the file after the renderer too, so clear old ones first.
            let prefix = format!("{name}-");
            for entry in std::fs::read_dir(&dir).expect("SNAPSHOT_DIR is readable") {
                let path = entry.expect("directory entry").path();
                let stale = path
                    .file_name()
                    .is_some_and(|file| file.to_string_lossy().starts_with(&prefix));
                if stale {
                    std::fs::remove_file(path).expect("removes stale snapshot");
                }
            }
            assert!(
                snapshot
                    .matches_image(dir.join(format!("{name}.png")))
                    .expect("writes png")
            );
        }
    }

    #[test]
    fn a_failure_before_connecting_is_reported_where_it_started() {
        use ui::connection::Message as Conn;
        for origin in [Tab::Connect, Tab::Manager] {
            let (mut app, _) = Application::new(None, Settings::default());
            app.tab = origin.clone();
            let _ = app.open_connection(request("ilo:443"));
            let _ = app.update(Message::Connection(
                0,
                Conn::Session(SessionEvent::Disconnected {
                    reason: Some("busy".to_owned()),
                }),
            ));

            assert!(app.connections.is_empty(), "{origin:?}");
            assert_eq!(app.tab, origin);
            match &app.modal {
                Some(AppModal::ConnectionFailed { message, details }) => {
                    assert_eq!(message, "busy");
                    assert!(details.contains("Address: ilo:443"), "{details}");
                    assert!(details.ends_with("Error: busy"), "{details}");
                }
                other => panic!("{origin:?}: {other:?}"),
            }
        }
    }

    #[test]
    fn a_failure_after_connecting_stays_in_its_tab() {
        use ui::connection::Message as Conn;
        let (mut app, _) = Application::new(None, Settings::default());
        let _ = app.open_connection(request("ilo:443"));
        for event in [
            SessionEvent::Connected {
                size: (1, 1),
                capabilities: server_remote_core::Capabilities::default(),
            },
            SessionEvent::Disconnected {
                reason: Some("lost".to_owned()),
            },
        ] {
            let _ = app.update(Message::Connection(0, Conn::Session(event)));
        }

        assert_eq!(app.connections.len(), 1);
        assert_eq!(app.tab, Tab::Connection(0));
        assert!(app.modal.is_none());
    }

    #[test]
    fn cancelling_or_blocking_a_certificate_closes_the_tab_quietly() {
        use ui::connection::Message as Conn;
        let fingerprint = server_remote_core::CertFingerprint([4; 32]);
        for message in [Conn::Close, Conn::BlockCertificate(fingerprint)] {
            let (mut app, _) = Application::new(None, Settings::default());
            app.tab = Tab::Manager;
            let _ = app.open_connection(request("ilo:443"));
            let _ = app.update(Message::Connection(
                0,
                Conn::UntrustedCertificate(fingerprint),
            ));
            let _ = app.update(Message::Connection(0, message.clone()));

            assert!(app.connections.is_empty(), "{message:?}");
            assert_eq!(app.tab, Tab::Manager, "{message:?}");
            assert!(app.modal.is_none(), "{message:?}");
        }
    }

    #[test]
    fn certificate_choices_follow_and_update_the_security_settings() {
        use crate::persistence::settings::{CertificateChecking, Refusal};
        use server_remote_core::CertFingerprint;
        use ui::connection::Message as Conn;

        let (mut app, _) = Application::new(None, Settings::default());
        let _ = app.open_connection(request("ilo:443"));
        let target = app.connections[0].target().clone();
        let trusted = CertFingerprint([1; 32]);
        let blocked = CertFingerprint([2; 32]);

        let _ = app.settle_certificate(
            0,
            Conn::TrustCertificate {
                fingerprint: trusted,
                remember: false,
            },
        );
        assert!(app.settings.security.trusted.is_empty(), "connect once");
        let _ = app.settle_certificate(
            0,
            Conn::TrustCertificate {
                fingerprint: trusted,
                remember: true,
            },
        );
        assert_eq!(
            app.settings.security.trusted,
            vec![KnownCertificate::new(&target, trusted)]
        );

        let _ = app.settle_certificate(0, Conn::BlockCertificate(blocked));
        assert_eq!(
            app.settings.security.blocked,
            vec![KnownCertificate::new(&target, blocked)]
        );

        let refused_with = |message: Conn| match message {
            Conn::Session(SessionEvent::Disconnected { reason }) => reason,
            _ => None,
        };
        let asked = app.settle_certificate(0, Conn::UntrustedCertificate(CertFingerprint([3; 32])));
        assert!(
            matches!(asked, Conn::UntrustedCertificate(_)),
            "ask: prompts"
        );
        let refused = app.settle_certificate(0, Conn::UntrustedCertificate(blocked));
        assert_eq!(refused_with(refused), Some(Refusal::Blocked.reason()));

        app.settings.security.certificate_checking = CertificateChecking::Strict;
        let refused =
            app.settle_certificate(0, Conn::UntrustedCertificate(CertFingerprint([3; 32])));
        assert_eq!(refused_with(refused), Some(Refusal::Strict.reason()));
    }

    #[test]
    fn saved_session_falls_back_to_the_protocol_label_when_name_and_hostname_are_blank() {
        let saved = saved_session_from_request(request(""));
        assert_eq!(saved.name, "dummy");
    }
}

/// Whether the app draws its own window controls and resize edges. macOS keeps
/// its native traffic lights over a transparent titlebar instead.
const CUSTOM_DECORATIONS: bool = !cfg!(target_os = "macos");
/// Room left for the macOS traffic lights at the start of the title bar.
const TRAFFIC_LIGHTS_WIDTH: f32 = 72.0;
/// Thickness of the invisible resize handles around an undecorated window.
const RESIZE_BORDER: f32 = 4.0;
const TAB_WIDTH: f32 = 150.0;
/// Title bar height: the tab buttons plus an equal inset above and below.
const TITLE_BAR_HEIGHT: f32 = ui::style::CONTROL_HEIGHT + 2.0 * ui::style::SPACING;

fn main() -> iced::Result {
    let config_dir = persistence::settings::config_dir();
    let settings = Settings::load(config_dir.as_deref());
    apply_graphics_backend(settings.general.graphics_backend);

    let boot = move || Application::new(config_dir.clone(), settings.clone());
    iced::application::application(boot, Application::update, Application::view)
        .title(Application::title)
        .window(window::Settings {
            icon: Some(app_icon::window_icon()),
            exit_on_close_request: false,
            decorations: !CUSTOM_DECORATIONS,
            platform_specific: platform_specific(),
            ..Default::default()
        })
        .settings(ui::style::settings())
        .subscription(Application::subscription)
        .theme(Application::theme)
        .run()
}

/// Tells wgpu which API to draw with on Windows, as chosen in Settings
/// (DX12 by default: its swapchain keeps up with live resizing far better
/// than Vulkan's). iced only takes the backend from the environment, so set
/// `WGPU_BACKEND` — unless the user already did, which wins.
fn apply_graphics_backend(backend: GraphicsBackend) {
    if !cfg!(windows) || std::env::var_os("WGPU_BACKEND").is_some() {
        return;
    }
    // SAFETY: runs at the start of `main`, before any other thread exists.
    unsafe { std::env::set_var("WGPU_BACKEND", backend.wgpu_name()) };
}

/// What a failed connection's copy button puts on the clipboard. Kept in
/// English whatever the UI language, as it's meant for bug reports.
fn failure_details(protocol: ui::connection::Protocol, target: &Target, error: &str) -> String {
    format!(
        "ServerRemote {}\nProtocol: {protocol}\nAddress: {}:{}\nError: {error}",
        env!("CARGO_PKG_VERSION"),
        target.host,
        target.port,
    )
}

fn third_party_licences<'a>() -> Element<'a, Message> {
    let link = |label: &str, url: &str| ui::dialog::Button {
        label: label.to_owned(),
        on_press: Message::Settings(ui::settings::Message::OpenUrl(url.to_owned())),
        kind: ui::dialog::Kind::Secondary,
        icon: None,
        enabled: true,
    };
    ui::dialog::view(
        fl!("licences-title"),
        fl!("licences-message"),
        vec![
            link(
                "Carbon",
                "https://carbondesignsystem.com/elements/icons/library/",
            ),
            link("Nunito Sans", "https://github.com/googlefonts/NunitoSans"),
            ui::dialog::Button {
                label: fl!("button-close"),
                on_press: Message::CloseModal,
                kind: ui::dialog::Kind::Primary,
                icon: None,
                enabled: true,
            },
        ],
    )
}

/// Asks for a file of exported sessions and reads them from it.
fn import_sessions() -> Task<Message> {
    let dialog = rfd::AsyncFileDialog::new()
        .set_title(fl!("manager-import-dialog-title"))
        .add_filter(fl!("manager-file-type"), &["toml"]);
    Task::perform(
        async move {
            let Some(file) = dialog.pick_file().await else {
                return Ok(Vec::new());
            };
            let contents = std::fs::read_to_string(file.path()).map_err(|e| e.to_string())?;
            persistence::sessions::import(&contents).map_err(|e| e.to_string())
        },
        Message::Imported,
    )
}

fn pick_recording_folder() -> Task<Message> {
    Task::perform(
        async {
            rfd::AsyncFileDialog::new()
                .set_title(fl!("recording-folder-dialog-title"))
                .pick_folder()
                .await
                .map(|folder| folder.path().to_path_buf())
        },
        Message::RecordingFolderPicked,
    )
}

#[cfg(target_os = "macos")]
fn platform_specific() -> window::settings::PlatformSpecific {
    window::settings::PlatformSpecific {
        title_hidden: true,
        titlebar_transparent: true,
        fullsize_content_view: true,
    }
}

/// Keeps the DWM frame on the undecorated window so Windows 11 still draws
/// its rounded corners, accent border and drop shadow.
#[cfg(target_os = "windows")]
fn platform_specific() -> window::settings::PlatformSpecific {
    window::settings::PlatformSpecific {
        undecorated_shadow: true,
        corner_preference: window::settings::platform::CornerPreference::Round,
        ..Default::default()
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn platform_specific() -> window::settings::PlatformSpecific {
    window::settings::PlatformSpecific::default()
}

/// A connection's last screen, to be saved as its session's thumbnail.
struct ThumbnailSource {
    dir: PathBuf,
    session_id: u64,
    width: u32,
    height: u32,
    pixels: Arc<[u8]>,
}

impl ThumbnailSource {
    /// Encodes and writes the thumbnail, returning the PNG.
    fn save(self) -> Option<Vec<u8>> {
        use persistence::sessions::thumbnail;

        let png = thumbnail::encode(self.width, self.height, &self.pixels)?;
        match thumbnail::save(&self.dir, self.session_id, &png) {
            Ok(()) => Some(png),
            Err(error) => {
                eprintln!("failed to save thumbnail: {error}");
                None
            }
        }
    }
}

struct Application {
    connections: Vec<ui::connection::State>,
    /// The page each connection was opened from, returned to when it closes.
    origins: HashMap<u64, Tab>,
    /// The saved session each connection was opened from, if any, whose
    /// thumbnail it updates when it closes.
    saved_origins: HashMap<u64, u64>,
    next_id: u64,
    tab: Tab,

    connect_tab: ui::connect::State,

    sessions: persistence::sessions::Sessions,
    manager: ui::manager::State,

    settings: Settings,
    settings_tab: ui::settings::State,
    /// Where settings are saved; `None` keeps them in memory only.
    config_dir: Option<PathBuf>,

    /// Tracked so the title bar shows restore rather than maximise.
    maximized: bool,
    /// Physical pixels per logical pixel, so the layout can snap to them.
    scale_factor: f32,
    /// The operating system's light or dark preference.
    system_theme: iced::theme::Mode,
    update: update::Status,
    /// A dialog over the whole window, if any.
    modal: Option<AppModal>,
}

#[derive(Debug, Clone)]
enum AppModal {
    /// "Quit?", for the window that asked to close.
    ConfirmQuit(window::Id),
    Error {
        title: String,
        message: String,
    },
    ThirdPartyLicences,
    /// A connection failed before it ever connected. `details` is what the
    /// copy button puts on the clipboard.
    ConnectionFailed {
        message: String,
        details: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Tab {
    Manager,
    Connect,
    Connection(u64),
    Settings,
}

#[derive(Clone)]
enum Message {
    SelectTab(Tab),

    ConnectTab(ui::connect::Message),
    Manager(ui::manager::Message),
    Connection(u64, ui::connection::Message),
    Settings(ui::settings::Message),
    RecordingFolderPicked(Option<PathBuf>),
    /// A saved session's new thumbnail, as a PNG; `None` if it couldn't be
    /// made, which keeps the old one.
    ThumbnailSaved(u64, Option<Vec<u8>>),
    /// Sessions read from a file the user picked (none if they cancelled).
    Imported(Result<Vec<persistence::sessions::SavedSession>, String>),
    Exported(Result<(), String>),
    UpdateChecked(Result<update::Outcome, update::Error>),

    Window(WindowAction),
    Resized(iced::window::Id),
    Maximized(bool),
    Rescaled(f32),
    SystemThemeChanged(iced::theme::Mode),
    RequestClose(iced::window::Id),
    Quit(iced::window::Id),
    CloseModal,
    CopyToClipboard(String),
}

/// Window management triggered from the custom title bar.
#[derive(Debug, Clone, Copy)]
enum WindowAction {
    Drag,
    ToggleMaximize,
    Minimize,
    Close,
    Resize(window::Direction),
}

impl Application {
    fn new(config_dir: Option<PathBuf>, settings: Settings) -> (Self, Task<Message>) {
        let check_on_launch = settings.updates.check_on_launch;
        i18n::select(settings.general.language);
        let sessions = persistence::sessions::Sessions::load(config_dir.as_deref());
        let mut manager = ui::manager::State::default();
        manager.load_thumbnails(config_dir.as_deref(), &sessions);
        let mut app = Self {
            connections: Vec::new(),
            origins: HashMap::new(),
            saved_origins: HashMap::new(),
            next_id: 0,
            tab: Tab::Connect,
            connect_tab: ui::connect::State::new(),
            sessions,
            manager,
            settings,
            settings_tab: ui::settings::State::default(),
            config_dir,
            maximized: false,
            scale_factor: 1.0,
            system_theme: iced::theme::Mode::None,
            update: update::Status::NotChecked,
            modal: None,
        };
        let update_check = if check_on_launch {
            app.check_for_updates()
        } else {
            Task::none()
        };
        // iced runs this once the window is open. Opening doesn't report a
        // scale factor, so it's asked for here; later changes (moving to
        // another display) arrive as `Rescaled` events.
        let scale_factor = window::latest()
            .and_then(window::scale_factor)
            .map(Message::Rescaled);
        let system_theme = iced::system::theme().map(Message::SystemThemeChanged);
        (app, Task::batch([update_check, scale_factor, system_theme]))
    }

    fn check_for_updates(&mut self) -> Task<Message> {
        self.update = update::Status::Checking;
        Task::perform(update::check(), Message::UpdateChecked)
    }

    fn open_url(&mut self, url: &str) {
        if let Err(error) = open::that_detached(url) {
            self.modal = Some(AppModal::Error {
                title: fl!("error-open-link"),
                message: format!("{url}: {error}"),
            });
        }
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::SelectTab(tab) => {
                self.tab = tab;
                Task::none()
            }
            Message::ConnectTab(message) => match message {
                ui::connect::Message::Internal(message) => {
                    self.connect_tab.update(message).map(Message::ConnectTab)
                }
                ui::connect::Message::Connect(request) => self.open_connection(request),
                ui::connect::Message::Save(request) => {
                    let _ = self.connect_tab.update(ui::connect::IMessage::CloseNaming);
                    self.save_session(request);
                    Task::none()
                }
            },
            Message::Manager(message) => self.update_manager(message),
            Message::ThumbnailSaved(id, png) => {
                if self.sessions.get(id).is_none() {
                    // Deleted while the thumbnail was being written.
                    if let Some(dir) = &self.config_dir {
                        persistence::sessions::thumbnail::remove(dir, id);
                    }
                } else if png.is_some() {
                    self.manager.set_thumbnail(id, png);
                }
                Task::none()
            }
            Message::Imported(Ok(imported)) => {
                for session in imported {
                    self.sessions.add(session);
                }
                self.sessions.save(self.config_dir.as_deref());
                Task::none()
            }
            Message::Imported(Err(message)) => {
                self.modal = Some(AppModal::Error {
                    title: fl!("manager-import-failed"),
                    message,
                });
                Task::none()
            }
            Message::Exported(result) => {
                if let Err(message) = result {
                    self.modal = Some(AppModal::Error {
                        title: fl!("manager-export-failed"),
                        message,
                    });
                }
                Task::none()
            }
            Message::Connection(id, message) => {
                use ui::connection::Message as Conn;
                // Messages about app-wide state are the application's to handle.
                match message {
                    Conn::Close => return self.close_connection(id),
                    Conn::OpenSettings(section) => {
                        self.close_menu(id);
                        self.settings_tab.open_section(section);
                        self.tab = Tab::Settings;
                        return Task::none();
                    }
                    Conn::ChangeSetting(change) => {
                        self.close_menu(id);
                        self.change_setting(change);
                        return Task::none();
                    }
                    Conn::ChooseRecordingFolder => {
                        self.close_menu(id);
                        return pick_recording_folder();
                    }
                    _ => {}
                }
                let message = self.settle_certificate(id, message);
                match message {
                    Conn::BlockCertificate(_) => return self.close_connection(id),
                    Conn::Session(SessionEvent::Disconnected { reason })
                        if self.never_connected(id) =>
                    {
                        self.fail_connection(id, reason);
                        return Task::none();
                    }
                    _ => {}
                }
                let Some(connection) = self.connections.iter_mut().find(|c| c.id() == id) else {
                    return Task::none();
                };
                connection
                    .update(message)
                    .map(move |message| Message::Connection(id, message))
            }
            Message::Window(action) => window::latest().and_then(move |id| match action {
                WindowAction::Drag => window::drag(id),
                WindowAction::ToggleMaximize => window::toggle_maximize(id),
                WindowAction::Minimize => window::minimize(id, true),
                WindowAction::Close => Task::done(Message::RequestClose(id)),
                WindowAction::Resize(direction) => window::drag_resize(id, direction),
            }),
            Message::Resized(id) => window::is_maximized(id).map(Message::Maximized),
            Message::Maximized(maximized) => {
                self.maximized = maximized;
                Task::none()
            }
            Message::SystemThemeChanged(mode) => {
                self.system_theme = mode;
                Task::none()
            }
            Message::Rescaled(scale_factor) => {
                self.scale_factor = scale_factor;
                Task::none()
            }
            Message::Settings(message) => {
                self.settings_tab.update(&message);
                match message {
                    ui::settings::Message::Change(change) => {
                        self.change_setting(change);
                        Task::none()
                    }
                    ui::settings::Message::ChooseRecordingFolder => pick_recording_folder(),
                    ui::settings::Message::CheckForUpdates => self.check_for_updates(),
                    ui::settings::Message::OpenUrl(url) => {
                        self.open_url(&url);
                        Task::none()
                    }
                    ui::settings::Message::ShowThirdPartyLicences => {
                        self.modal = Some(AppModal::ThirdPartyLicences);
                        Task::none()
                    }
                    ui::settings::Message::Section(_)
                    | ui::settings::Message::Toggle(_)
                    | ui::settings::Message::Dismiss => Task::none(),
                }
            }
            Message::UpdateChecked(result) => {
                self.update = update::Status::Checked(result);
                Task::none()
            }
            Message::RecordingFolderPicked(folder) => {
                if let Some(folder) = folder {
                    self.change_setting(ui::settings::Change::RecordingFolder(folder));
                }
                Task::none()
            }
            Message::RequestClose(id) => {
                if self.settings.general.confirm_quit && !self.connections.is_empty() {
                    self.modal = Some(AppModal::ConfirmQuit(id));
                    return Task::none();
                }
                self.save_thumbnails_now();
                window::close(id)
            }
            Message::Quit(id) => {
                self.save_thumbnails_now();
                window::close(id)
            }
            Message::CopyToClipboard(contents) => iced::clipboard::write(contents),
            Message::CloseModal => {
                self.modal = None;
                Task::none()
            }
        }
    }

    fn change_setting(&mut self, change: ui::settings::Change) {
        if let ui::settings::Change::Language(language) = change {
            i18n::select(language);
        }
        ui::settings::apply(&mut self.settings, change);
        self.settings.save(self.config_dir.as_deref());
    }

    /// Applies the security settings to a connection's certificate messages:
    /// refuses a certificate they rule out, and saves the user's choice in
    /// the prompt. Returns what the connection itself should handle.
    fn settle_certificate(
        &mut self,
        id: u64,
        message: ui::connection::Message,
    ) -> ui::connection::Message {
        use ui::connection::Message as Conn;
        use ui::settings::Change;
        let Some(target) = self
            .connections
            .iter()
            .find(|connection| connection.id() == id)
            .map(|connection| connection.target().clone())
        else {
            return message;
        };
        match &message {
            Conn::UntrustedCertificate(fingerprint) => {
                match self.settings.security.refusal(&target, *fingerprint) {
                    Some(refusal) => Conn::Session(SessionEvent::Disconnected {
                        reason: Some(refusal.reason()),
                    }),
                    None => message,
                }
            }
            Conn::TrustCertificate {
                fingerprint,
                remember: true,
            } => {
                self.change_setting(Change::TrustCertificate(KnownCertificate::new(
                    &target,
                    *fingerprint,
                )));
                message
            }
            Conn::BlockCertificate(fingerprint) => {
                self.change_setting(Change::BlockCertificate(KnownCertificate::new(
                    &target,
                    *fingerprint,
                )));
                message
            }
            _ => message,
        }
    }

    fn close_menu(&mut self, id: u64) {
        if let Some(connection) = self.connections.iter_mut().find(|c| c.id() == id) {
            let _ = connection.update(ui::connection::Message::CloseMenu);
        }
    }

    /// Persists the connect tab's current fields as a saved session and
    /// switches to the Manager tab so the result is immediately visible.
    fn save_session(&mut self, request: ui::connect::Request) {
        self.sessions.add(saved_session_from_request(request));
        self.sessions.save(self.config_dir.as_deref());
        self.tab = Tab::Manager;
    }

    fn update_manager(&mut self, message: ui::manager::Message) -> Task<Message> {
        use persistence::sessions::thumbnail;
        use ui::manager::Message as Manager;

        if let Manager::Internal(message) = message {
            self.manager.update(message, &self.sessions);
            return Task::none();
        }
        self.manager.dismiss();
        match message {
            Manager::Internal(_) => Task::none(),
            Manager::Connect(id) => self.open_saved(id),
            Manager::Save { editing, request } => {
                let session = saved_session_from_request(request);
                match editing {
                    Some(id) => {
                        // A different machine's last screen would mislead.
                        let moved = self.sessions.get(id).is_some_and(|old| {
                            old.protocol != session.protocol || old.hostname != session.hostname
                        });
                        if moved {
                            if let Some(dir) = &self.config_dir {
                                thumbnail::remove(dir, id);
                            }
                            self.manager.set_thumbnail(id, None);
                        }
                        self.sessions.replace(id, session);
                    }
                    None => {
                        self.sessions.add(session);
                    }
                }
                self.sessions.save(self.config_dir.as_deref());
                Task::none()
            }
            Manager::SetGroup { id, group } => {
                if let Some(session) = self.sessions.saved.iter_mut().find(|s| s.id == id) {
                    session.group = group;
                    self.sessions.save(self.config_dir.as_deref());
                }
                Task::none()
            }
            Manager::Delete(id) => {
                if self.sessions.remove(id).is_some() {
                    self.sessions.save(self.config_dir.as_deref());
                    if let Some(dir) = &self.config_dir {
                        thumbnail::remove(dir, id);
                    }
                    self.manager.set_thumbnail(id, None);
                }
                Task::none()
            }
            Manager::Export(id) => self.export_session(id),
            Manager::Import => import_sessions(),
        }
    }

    /// Asks where to save the session and writes it there, without its
    /// password.
    fn export_session(&mut self, id: u64) -> Task<Message> {
        let Some(session) = self.sessions.get(id) else {
            return Task::none();
        };
        let contents = match persistence::sessions::export(session) {
            Ok(contents) => contents,
            Err(error) => {
                self.modal = Some(AppModal::Error {
                    title: fl!("manager-export-failed"),
                    message: error.to_string(),
                });
                return Task::none();
            }
        };
        let dialog = rfd::AsyncFileDialog::new()
            .set_title(fl!("manager-export-dialog-title"))
            .add_filter(fl!("manager-file-type"), &["toml"])
            .set_file_name(export_file_name(&session.name));
        Task::perform(
            async move {
                let Some(file) = dialog.save_file().await else {
                    return Ok(());
                };
                std::fs::write(file.path(), contents).map_err(|error| error.to_string())
            },
            Message::Exported,
        )
    }

    /// Opens a connection to a saved session, named after it.
    fn open_saved(&mut self, session_id: u64) -> Task<Message> {
        let Some(session) = self.sessions.get(session_id) else {
            return Task::none();
        };
        let request = ui::connect::Request::from(session);
        let task = self.open(request, true);
        self.saved_origins.insert(self.next_id - 1, session_id);
        task
    }

    /// Saves the last screen of a connection to a saved session as that
    /// session's thumbnail, off the UI thread.
    fn save_thumbnail(&self, id: u64) -> Task<Message> {
        let Some(source) = self.thumbnail_source(id) else {
            return Task::none();
        };
        let session_id = source.session_id;
        Task::perform(
            async move {
                let encode = move || source.save();
                tokio::task::spawn_blocking(encode).await.ok().flatten()
            },
            move |png| Message::ThumbnailSaved(session_id, png),
        )
    }

    /// Saves every open connection's thumbnail before the app quits, when
    /// there's no time left for tasks.
    fn save_thumbnails_now(&self) {
        for connection in &self.connections {
            if let Some(source) = self.thumbnail_source(connection.id()) {
                source.save();
            }
        }
    }

    /// Where a connection's thumbnail goes, for which session, and the
    /// frame to make it from; `None` if it has no saved session (still) or
    /// no frame yet.
    fn thumbnail_source(&self, id: u64) -> Option<ThumbnailSource> {
        let dir = self.config_dir.clone()?;
        let session_id = *self.saved_origins.get(&id)?;
        self.sessions.get(session_id)?;
        let (width, height, pixels) = self
            .connections
            .iter()
            .find(|connection| connection.id() == id)?
            .last_frame()?;
        Some(ThumbnailSource {
            dir,
            session_id,
            width,
            height,
            pixels,
        })
    }

    fn open_connection(&mut self, request: ui::connect::Request) -> Task<Message> {
        self.open(request, false)
    }

    /// Opens a connection tab. The tab is named after the request when
    /// `named` (a saved session), otherwise after its hostname.
    fn open(&mut self, request: ui::connect::Request, named: bool) -> Task<Message> {
        let id = self.next_id;
        self.next_id += 1;

        let name: Arc<str> = if named && !request.name.is_empty() {
            Arc::from(request.name.as_str())
        } else if request.hostname.is_empty() {
            format!("{} #{id}", request.protocol).into()
        } else {
            Arc::from(request.hostname.as_str())
        };
        let target = parse_target(&request.hostname, request.protocol);
        let tls = self.settings.security.policy_for(&target);

        self.connections.push(ui::connection::State::new(
            id,
            name,
            request.protocol,
            target,
            tls,
            ui::connection::Credentials {
                username: request.username,
                password: request.password,
            },
            &self.settings.input,
        ));
        let origin = match self.tab {
            Tab::Manager => Tab::Manager,
            _ => Tab::Connect,
        };
        self.origins.insert(id, origin);
        self.tab = Tab::Connection(id);
        Task::none()
    }

    fn never_connected(&self, id: u64) -> bool {
        self.connections
            .iter()
            .find(|connection| connection.id() == id)
            .is_some_and(|connection| !connection.connected_once())
    }

    /// Closes a connection that failed before connecting, reporting the
    /// failure on the page it was opened from.
    fn fail_connection(&mut self, id: u64, reason: Option<String>) {
        let Some(connection) = self.connections.iter().find(|c| c.id() == id) else {
            return;
        };
        let message = reason.unwrap_or_else(|| fl!("connection-failed-unknown"));
        let details = failure_details(connection.protocol(), connection.target(), &message);
        // It never connected, so there's no screen to keep.
        let _ = self.close_connection(id);
        self.modal = Some(AppModal::ConnectionFailed { message, details });
    }

    /// Closes a connection tab, saving its last screen as its saved
    /// session's thumbnail.
    fn close_connection(&mut self, id: u64) -> Task<Message> {
        let thumbnail = self.save_thumbnail(id);
        self.connections.retain(|connection| connection.id() != id);
        self.saved_origins.remove(&id);
        let origin = self.origins.remove(&id).unwrap_or(Tab::Connect);
        if self.tab == Tab::Connection(id) {
            self.tab = origin;
        }
        thumbnail
    }

    fn view(&self) -> Element<'_, Message> {
        // The header carries its own insets so the window controls can sit
        // flush in the corner; only the body gets the outer margin.
        // Fills the window whatever the tab holds: layers stacked over it
        // (the resize handles, the quit prompt) take their size from it.
        let content = Column::new()
            .width(Length::Fill)
            .height(Length::Fill)
            .push(self.header())
            .push(
                container(self.body())
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .padding(
                        padding::left(ui::style::SPACING)
                            .right(ui::style::SPACING)
                            .bottom(ui::style::SPACING),
                    ),
            );

        // A maximised window can't be resized, and the handles would steal
        // the screen corner from the close button.
        let window: Element<'_, Message> = if CUSTOM_DECORATIONS && !self.maximized {
            stack![content, resize_handles()].into()
        } else {
            content.into()
        };
        let tab_modal = match self.tab {
            Tab::Manager => {
                ui::manager::modal(&self.manager, &self.sessions).map(|m| m.map(Message::Manager))
            }
            Tab::Connect => self.connect_tab.modal().map(|m| m.map(Message::ConnectTab)),
            _ => None,
        };
        let window = match tab_modal {
            Some(modal) => stack![window, modal].into(),
            None => window,
        };
        let window = match &self.modal {
            Some(modal) => stack![window, self.modal_view(modal)].into(),
            None => window,
        };
        ui::pixel_snap(ui::arrow_cursor(window), self.scale_factor)
    }

    fn modal_view<'a>(&'a self, modal: &'a AppModal) -> Element<'a, Message> {
        let id = match modal {
            AppModal::ConfirmQuit(id) => *id,
            AppModal::Error { title, message } => {
                return ui::dialog::view(
                    title.clone(),
                    message.clone(),
                    vec![ui::dialog::Button {
                        label: fl!("button-ok"),
                        on_press: Message::CloseModal,
                        kind: ui::dialog::Kind::Primary,
                        icon: None,
                        enabled: true,
                    }],
                );
            }
            AppModal::ThirdPartyLicences => return third_party_licences(),
            AppModal::ConnectionFailed { message, details } => {
                return ui::dialog::view(
                    fl!("connection-failed-title"),
                    message.clone(),
                    vec![
                        ui::dialog::Button {
                            label: fl!("copy-details"),
                            on_press: Message::CopyToClipboard(details.clone()),
                            kind: ui::dialog::Kind::Secondary,
                            icon: Some(icons::COPY_TO_CLIPBOARD),
                            enabled: true,
                        },
                        ui::dialog::Button {
                            label: fl!("button-close"),
                            on_press: Message::CloseModal,
                            kind: ui::dialog::Kind::Secondary,
                            icon: None,
                            enabled: true,
                        },
                    ],
                );
            }
        };
        ui::dialog::view(
            fl!("quit-title"),
            fl!("quit-message", count = self.connections.len()),
            vec![
                ui::dialog::Button {
                    label: fl!("button-cancel"),
                    on_press: Message::CloseModal,
                    kind: ui::dialog::Kind::Secondary,
                    icon: None,
                    enabled: true,
                },
                ui::dialog::Button {
                    label: fl!("quit-confirm"),
                    on_press: Message::Quit(id),
                    kind: ui::dialog::Kind::Destructive,
                    icon: None,
                    enabled: true,
                },
            ],
        )
    }

    fn body(&self) -> Element<'_, Message> {
        match &self.tab {
            Tab::Manager => ui::manager::view(&self.manager, &self.sessions).map(Message::Manager),
            Tab::Connect => self.connect_tab.view().map(Message::ConnectTab),
            Tab::Connection(id) => match self.connections.iter().find(|c| c.id() == *id) {
                Some(connection) => {
                    let id = *id;
                    connection
                        .view(&self.settings)
                        .map(move |message| Message::Connection(id, message))
                }
                None => text(fl!("connection-closed")).into(),
            },
            Tab::Settings => ui::settings::view(&self.settings_tab, &self.settings, &self.update)
                .map(Message::Settings),
        }
    }

    fn theme(&self) -> iced::Theme {
        let light = match self.settings.general.appearance {
            Appearance::System => self.system_theme == iced::theme::Mode::Light,
            Appearance::Light => true,
            Appearance::Dark => false,
        };
        if light {
            ui::style::light_theme()
        } else {
            ui::style::dark_theme()
        }
    }

    fn title(&self) -> String {
        // TODO: show connection
        "Server Remote".into()
    }

    fn subscription(&self) -> Subscription<Message> {
        let close_requests = window::close_requests().map(Message::RequestClose);
        // `Subscription::map` requires a non-capturing closure, so the id rides
        // along via `with` and is unpacked from the tuple instead of captured.
        let connections = self.connections.iter().map(|connection| {
            connection
                .subscription()
                .with(connection.id())
                .map(|(id, message)| Message::Connection(id, message))
        });

        // Keyboard input is captured directly by the focused `kvm_display`
        // widget on the active connection tab, not routed here.
        let resizes = window::resize_events().map(|(id, _size)| Message::Resized(id));
        let rescales = window::events().filter_map(|(_id, event)| match event {
            window::Event::Rescaled(scale_factor) => Some(Message::Rescaled(scale_factor)),
            _ => None,
        });
        let system_theme = iced::system::theme_changes().map(Message::SystemThemeChanged);
        Subscription::batch(
            [close_requests, resizes, rescales, system_theme]
                .into_iter()
                .chain(connections),
        )
    }

    fn header(&self) -> Element<'_, Message> {
        let tabs = Row::with_children(
            self.connections
                .iter()
                .map(|connection| self.connection_tab(connection)),
        )
        .push(self.icon_tab(Tab::Connect, icons::ADD_LARGE, fl!("tab-connect")))
        .spacing(ui::style::SPACING);

        // Empty space in the title bar moves the window, as a native one would.
        let drag_region = mouse_area(
            Row::new()
                .push((!CUSTOM_DECORATIONS).then(|| Space::new().width(TRAFFIC_LIGHTS_WIDTH)))
                .push(self.icon_tab(Tab::Manager, icons::VIRTUAL_DESKTOP, fl!("tab-manager")))
                .push(Scrollable::new(tabs).horizontal().width(Length::Fill))
                .spacing(ui::style::SPACING),
        )
        .on_press(Message::Window(WindowAction::Drag))
        .on_double_click(Message::Window(WindowAction::ToggleMaximize));

        Row::new()
            .push(drag_region)
            .push(self.icon_tab(Tab::Settings, icons::SETTINGS, fl!("tab-settings")))
            .push(CUSTOM_DECORATIONS.then(|| {
                ui::window_controls::view(
                    self.maximized,
                    Message::Window(WindowAction::Minimize),
                    Message::Window(WindowAction::ToggleMaximize),
                    Message::Window(WindowAction::Close),
                )
            }))
            .spacing(ui::style::SPACING)
            .align_y(Alignment::Center)
            // Tall enough for an even inset above, below and left of the
            // tabs; the window controls stretch to fill it, flush with the
            // top and right edges so the screen corner hits close.
            .height(TITLE_BAR_HEIGHT)
            .padding(
                padding::left(ui::style::SPACING).right(if CUSTOM_DECORATIONS {
                    0.0
                } else {
                    ui::style::SPACING
                }),
            )
            .into()
    }

    fn icon_tab(&self, tab: Tab, icon: icons::Icon, label: String) -> Element<'_, Message> {
        let active = self.tab == tab;
        tooltip(
            button(container(themed(icon)).center(Length::Fill))
                .width(ui::style::CONTROL_HEIGHT)
                .height(ui::style::CONTROL_HEIGHT)
                .padding(0)
                .style(move |theme, status| ui::style::bordered_button(theme, status, active))
                .on_press(Message::SelectTab(tab)),
            tooltip_box(text(label)),
            tooltip::Position::Bottom,
        )
        .into()
    }

    fn connection_tab<'a>(&self, connection: &'a ui::connection::State) -> Element<'a, Message> {
        let id = connection.id();
        let active = self.tab == Tab::Connection(id);

        let close = button(container(themed(icons::CLOSE)).center(Length::Fill))
            .width(18)
            .height(18)
            .padding(0)
            .style(move |theme, status| ui::style::tab_close_button(theme, status, active))
            .on_press(Message::Connection(id, ui::connection::Message::Close));

        let font = if active {
            ui::style::BOLD
        } else {
            ui::style::FONT
        };

        button(
            Row::new()
                .push(themed(connection.protocol().icon()))
                .push(
                    text(connection.name().as_ref())
                        .font(font)
                        .width(Length::Fill)
                        .wrapping(text::Wrapping::None),
                )
                .push(close)
                .spacing(4)
                .height(Length::Fill)
                .align_y(Alignment::Center),
        )
        .width(TAB_WIDTH)
        .height(ui::style::CONTROL_HEIGHT)
        .padding(padding::left(5).right(5))
        .style(move |theme, status| ui::style::bordered_button(theme, status, active))
        .on_press(Message::SelectTab(Tab::Connection(id)))
        .into()
    }
}

fn themed<'a>(icon: icons::Icon) -> iced::widget::svg::Svg<'a> {
    ui::themed_icon(icon.element(), |palette| palette.background.base.text)
}

/// Invisible handles along the window edges that resize an undecorated
/// window, since the OS no longer provides them. Layered over the content so
/// they sit in its outer margin rather than adding to it.
fn resize_handles<'a>() -> Element<'a, Message> {
    use iced::mouse::Interaction;
    use window::Direction;

    let handle = |direction: Direction, width: Length, height: Length| {
        let interaction = match direction {
            Direction::North | Direction::South => Interaction::ResizingVertically,
            Direction::East | Direction::West => Interaction::ResizingHorizontally,
            Direction::NorthWest | Direction::SouthEast => Interaction::ResizingDiagonallyDown,
            Direction::NorthEast | Direction::SouthWest => Interaction::ResizingDiagonallyUp,
        };
        mouse_area(Space::new().width(width).height(height))
            .interaction(interaction)
            .on_press(Message::Window(WindowAction::Resize(direction)))
    };
    let border = Length::Fixed(RESIZE_BORDER);

    Column::new()
        .push(
            Row::new()
                .push(handle(Direction::NorthWest, border, border))
                .push(handle(Direction::North, Length::Fill, border))
                .push(handle(Direction::NorthEast, border, border)),
        )
        .push(
            Row::new()
                .push(handle(Direction::West, border, Length::Fill))
                .push(Space::new().width(Length::Fill))
                .push(handle(Direction::East, border, Length::Fill))
                .height(Length::Fill),
        )
        .push(
            Row::new()
                .push(handle(Direction::SouthWest, border, border))
                .push(handle(Direction::South, Length::Fill, border))
                .push(handle(Direction::SouthEast, border, border)),
        )
        .into()
}
