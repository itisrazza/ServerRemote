use std::collections::HashMap;
use std::path::PathBuf;

use iced::{
    Alignment, Element, Length, Subscription, Task, padding,
    widget::{Column, Row, Scrollable, Space, button, container, mouse_area, stack, text, tooltip},
    window,
};
use server_remote_app::App;
use server_remote_app::app::{Dialog, Effect, Event, Page};
use server_remote_app::connection::Connection;
use server_remote_app::persistence::settings::Change;
use server_remote_icons as icons;

use crate::i18n::fl;
use crate::persistence::settings::{Appearance, GraphicsBackend, Settings};
use crate::tooltip_box::tooltip_box;
use server_remote_app::{persistence, update};

mod app_icon;
mod i18n;
mod tooltip_box;
mod ui;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::settings::KnownCertificate;
    use server_remote_app::connection::Report;
    use server_remote_core::{SessionEvent, Target};

    fn request(hostname: &str) -> server_remote_app::connect::Request {
        server_remote_app::connect::Request {
            protocol: server_remote_app::Protocol::Dummy,
            name: String::new(),
            group: String::new(),
            hostname: hostname.to_owned(),
            username: String::new(),
            password: String::new(),
        }
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
        let session =
            |event| Message::Connection(0, ui::connection::Message::Report(Report::Session(event)));

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
    /// A config folder holding the sample saved sessions, with ids 1 to 6.
    fn saved_sessions_dir() -> tempfile::TempDir {
        use persistence::sessions::{SavedSession, Sessions};
        use server_remote_app::Protocol;

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
        let saved = sessions
            .into_iter()
            .enumerate()
            .map(|(index, (name, group, protocol, hostname))| SavedSession {
                id: index as u64 + 1,
                name: name.to_owned(),
                group: group.to_owned(),
                protocol,
                hostname: hostname.to_owned(),
                username: String::new(),
                password: String::new(),
            })
            .collect();
        let dir = tempfile::tempdir().expect("creates a temporary config folder");
        Sessions { saved }.save(Some(dir.path()));
        dir
    }

    /// Gives two of the sample sessions a thumbnail.
    fn add_thumbnails(app: &mut Application) {
        let desktop = containerdesk_frame();
        let png = persistence::sessions::thumbnail::encode(
            desktop.width,
            desktop.height,
            &desktop.pixels,
        );
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
                Message::SelectTab(Page::Settings),
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
                    persistence::settings::KeyboardSize::Large,
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
                    Message::SelectTab(Page::Settings),
                    Message::RequestClose(window::Id::unique()),
                ],
            ),
        ];

        let about = vec![
            Message::SelectTab(Page::Settings),
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
                .chain([Message::App(Event::UpdateChecked(Ok(
                    update::Outcome::Available {
                        version: semver::Version::new(0, 2, 0),
                        url: "https://example.invalid/release".to_owned(),
                    },
                )))])
                .collect(),
        ));

        // A few in the light theme too.
        let light = |messages: Vec<Message>| -> Vec<Message> {
            std::iter::once(Message::Settings(ui::settings::Message::Change(
                persistence::settings::Change::Appearance(Appearance::Light),
            )))
            .chain(messages)
            .collect()
        };
        states.push(("connector-light", true, light(vec![])));
        // The website's screenshots: a live desktop rather than the
        // "waiting for first frame" placeholder.
        let desktop = conn(Conn::Report(Report::Session(
            server_remote_core::SessionEvent::Frame(containerdesk_frame()),
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
                persistence::settings::KeyboardSize::Large,
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
                persistence::settings::Change::Language(persistence::settings::Language::Romanian),
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
            Message::SelectTab(Page::Connect),
            Message::ConnectTab(ui::connect::Message::Internal(
                ui::connect::IMessage::Protocol(server_remote_app::Protocol::Ilo),
            )),
        ];
        let trust = vec![conn(Conn::Report(Report::UntrustedCertificate(
            server_remote_core::CertFingerprint(core::array::from_fn(|i| i as u8 * 7)),
        )))];
        let form =
            |messages: Vec<ui::connect::IMessage>| -> Vec<Message> {
                std::iter::once(Message::SelectTab(Page::Connect))
                    .chain(messages.into_iter().map(|message| {
                        Message::ConnectTab(ui::connect::Message::Internal(message))
                    }))
                    .collect()
            };
        let vnc = || ui::connect::IMessage::Protocol(server_remote_app::Protocol::Vnc);
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
        let failed = vec![conn(Conn::Report(Report::Session(
            SessionEvent::Disconnected {
                reason: Some("There is another user using the remote console.".to_owned()),
            },
        )))];
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
                conn(Conn::Report(Report::Session(SessionEvent::Connected {
                    size: (256, 64),
                    capabilities: server_remote_core::Capabilities::default(),
                }))),
                conn(Conn::Report(Report::Session(SessionEvent::Frame(greys)))),
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
            Message::SelectTab(Page::Settings),
            Message::Settings(ui::settings::Message::Section(
                ui::settings::Section::Security,
            )),
            Message::Settings(ui::settings::Message::Change(
                persistence::settings::Change::TrustCertificate(known(0x5A)),
            )),
            Message::Settings(ui::settings::Message::Change(
                persistence::settings::Change::BlockCertificate(known(0xC3)),
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
                Message::SelectTab(Page::Settings),
                Message::Settings(ui::settings::Message::Section(
                    ui::settings::Section::Display,
                )),
            ],
        ));

        // The connection manager, filled in by `saved_sessions_dir`, except
        // in `manager-empty`.
        let manager = |messages: Vec<ui::manager::IMessage>| -> Vec<Message> {
            std::iter::once(Message::SelectTab(Page::Manager))
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
            let sessions_dir =
                (name.starts_with("manager") && name != "manager-empty").then(saved_sessions_dir);
            let config_dir = sessions_dir.as_ref().map(|dir| dir.path().to_path_buf());
            let (mut app, _) = Application::new(config_dir, Settings::default());
            for name in ["Connection #1", "Connection #2"] {
                let _ = app.update(Message::App(Event::Connect(request(name))));
            }
            let _ = app.update(Message::SelectTab(Page::Connection(0)));
            if sessions_dir.is_some() {
                add_thumbnails(&mut app);
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
        format!("{}\n\n{}", fl!("licences-icons"), fl!("licences-font")),
        vec![
            link("Carbon", server_remote_app::about::CARBON_URL),
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

/// Asks for a file of exported sessions and reads it; nothing if cancelled.
fn import_sessions() -> Task<Message> {
    let dialog = rfd::AsyncFileDialog::new()
        .set_title(fl!("manager-import-dialog-title"))
        .add_filter(fl!("manager-file-type"), &["toml"]);
    Task::perform(
        async move {
            let file = dialog.pick_file().await?;
            let contents = std::fs::read_to_string(file.path()).map_err(|e| e.to_string());
            Some(Message::App(Event::Imported(contents)))
        },
        std::convert::identity,
    )
    .and_then(Task::done)
}

/// Asks where to save an exported session and writes it there.
fn export_session(file_name: String, contents: String) -> Task<Message> {
    let dialog = rfd::AsyncFileDialog::new()
        .set_title(fl!("manager-export-dialog-title"))
        .add_filter(fl!("manager-file-type"), &["toml"])
        .set_file_name(file_name);
    Task::perform(
        async move {
            let file = dialog.save_file().await?;
            let written = std::fs::write(file.path(), contents).map_err(|e| e.to_string());
            Some(Message::App(Event::Exported(written)))
        },
        std::convert::identity,
    )
    .and_then(Task::done)
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

/// The desktop front end over [`App`]: the window, and what each page needs
/// to remember that the app itself doesn't.
struct Application {
    app: App,
    /// Each open connection's tab, by connection id.
    tabs: HashMap<u64, ui::connection::State>,

    connect_tab: ui::connect::State,
    manager: ui::manager::State,
    settings_tab: ui::settings::State,

    /// Tracked so the title bar shows restore rather than maximise.
    maximized: bool,
    /// Physical pixels per logical pixel, so the layout can snap to them.
    scale_factor: f32,
    /// The operating system's light or dark preference.
    system_theme: iced::theme::Mode,
    /// A dialog only the desktop has, over the whole window. The app's own
    /// come from [`App::dialog`].
    modal: Option<AppModal>,
}

#[derive(Debug, Clone)]
enum AppModal {
    /// "Quit?", for the window that asked to close.
    ConfirmQuit(window::Id),
    ThirdPartyLicences,
}

#[derive(Clone)]
enum Message {
    SelectTab(Page),

    ConnectTab(ui::connect::Message),
    Manager(ui::manager::Message),
    Connection(u64, ui::connection::Message),
    Settings(ui::settings::Message),
    RecordingFolderPicked(Option<PathBuf>),
    /// Straight to the app, such as the result of an effect.
    App(Event),

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
        let (app, effects) = App::new(config_dir, settings);
        let mut manager = ui::manager::State::default();
        manager.load_thumbnails(app.config_dir(), app.sessions());
        let mut application = Self {
            app,
            tabs: HashMap::new(),
            connect_tab: ui::connect::State::new(),
            manager,
            settings_tab: ui::settings::State::default(),
            maximized: false,
            scale_factor: 1.0,
            system_theme: iced::theme::Mode::None,
            modal: None,
        };
        let effects = application.perform(effects);
        // iced runs this once the window is open. Opening doesn't report a
        // scale factor, so it's asked for here; later changes (moving to
        // another display) arrive as `Rescaled` events.
        let scale_factor = window::latest()
            .and_then(window::scale_factor)
            .map(Message::Rescaled);
        let system_theme = iced::system::theme().map(Message::SystemThemeChanged);
        (
            application,
            Task::batch([effects, scale_factor, system_theme]),
        )
    }

    /// Hands `event` to the app and carries out what it asks for.
    fn send(&mut self, event: Event) -> Task<Message> {
        let effects = self.app.update(event);
        self.sync_tabs();
        self.perform(effects)
    }

    /// Gives new connections a tab and forgets the tabs of closed ones.
    fn sync_tabs(&mut self) {
        let input = &self.app.settings().input;
        for connection in self.app.connections() {
            self.tabs
                .entry(connection.id())
                .or_insert_with(|| ui::connection::State::new(input));
        }
        let app = &self.app;
        self.tabs.retain(|id, _| app.connection(*id).is_some());
    }

    fn perform(&mut self, effects: Vec<Effect>) -> Task<Message> {
        Task::batch(effects.into_iter().map(|effect| match effect {
            Effect::PickFile {
                connection,
                action,
                extensions,
            } => Task::perform(ui::connection::pick_file(extensions), move |path| {
                Message::Connection(
                    connection,
                    ui::connection::Message::FilePicked { id: action, path },
                )
            }),
            Effect::ExportSession {
                file_name,
                contents,
            } => export_session(file_name, contents),
            Effect::ImportSessions => import_sessions(),
            Effect::CheckForUpdates => Task::perform(update::check(), |result| {
                Message::App(Event::UpdateChecked(result))
            }),
            Effect::SaveThumbnail(source) => {
                let session = source.session();
                Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || source.save())
                            .await
                            .ok()
                            .flatten()
                    },
                    move |png| Message::App(Event::ThumbnailSaved { session, png }),
                )
            }
            Effect::Thumbnail { session, png } => {
                self.manager.set_thumbnail(session, png);
                Task::none()
            }
        }))
    }

    fn open_url(&mut self, url: &str) -> Task<Message> {
        match open::that_detached(url) {
            Ok(()) => Task::none(),
            Err(error) => self.send(Event::OpenUrlFailed {
                url: url.to_owned(),
                error: error.to_string(),
            }),
        }
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::SelectTab(page) => self.send(Event::ShowPage(page)),
            Message::ConnectTab(message) => match message {
                ui::connect::Message::Internal(message) => {
                    self.connect_tab.update(message).map(Message::ConnectTab)
                }
                ui::connect::Message::Connect(request) => self.send(Event::Connect(request)),
                ui::connect::Message::Save(request) => {
                    let _ = self.connect_tab.update(ui::connect::IMessage::CloseNaming);
                    self.send(Event::SaveNew(request))
                }
            },
            Message::Manager(message) => self.update_manager(message),
            Message::App(event) => self.send(event),
            Message::Connection(id, message) => self.update_connection(id, message),
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
                        self.send(Event::ChangeSetting(change))
                    }
                    ui::settings::Message::ChooseRecordingFolder => pick_recording_folder(),
                    ui::settings::Message::CheckForUpdates => self.send(Event::CheckForUpdates),
                    ui::settings::Message::OpenUrl(url) => self.open_url(&url),
                    ui::settings::Message::ShowThirdPartyLicences => {
                        self.modal = Some(AppModal::ThirdPartyLicences);
                        Task::none()
                    }
                    ui::settings::Message::Section(_)
                    | ui::settings::Message::Toggle(_)
                    | ui::settings::Message::Dismiss => Task::none(),
                }
            }
            Message::RecordingFolderPicked(folder) => match folder {
                Some(folder) => self.send(Event::ChangeSetting(Change::RecordingFolder(folder))),
                None => Task::none(),
            },
            Message::RequestClose(id) => {
                if self.app.settings().general.confirm_quit && !self.app.connections().is_empty() {
                    self.modal = Some(AppModal::ConfirmQuit(id));
                    return Task::none();
                }
                self.app.save_thumbnails_now();
                window::close(id)
            }
            Message::Quit(id) => {
                self.app.save_thumbnails_now();
                window::close(id)
            }
            Message::CopyToClipboard(contents) => iced::clipboard::write(contents),
            Message::CloseModal => {
                if self.modal.take().is_some() {
                    return Task::none();
                }
                self.send(Event::CloseDialog)
            }
        }
    }

    fn update_connection(&mut self, id: u64, message: ui::connection::Message) -> Task<Message> {
        use ui::connection::Message as Conn;
        // Messages about app-wide state are the application's to handle.
        match message {
            Conn::Close => return self.send(Event::CloseConnection(id)),
            Conn::BlockCertificate(fingerprint) => {
                return self.send(Event::BlockCertificate(id, fingerprint));
            }
            Conn::OpenSettings(section) => {
                self.close_menu(id);
                self.settings_tab.open_section(section);
                return self.send(Event::ShowPage(Page::Settings));
            }
            Conn::ChangeSetting(change) => {
                self.close_menu(id);
                return self.send(Event::ChangeSetting(change));
            }
            Conn::ChooseRecordingFolder => {
                self.close_menu(id);
                return pick_recording_folder();
            }
            _ => {}
        }
        let (Some(tab), Some(connection)) = (self.tabs.get_mut(&id), self.app.connection(id))
        else {
            return Task::none();
        };
        let events = tab.update(message, connection);
        let sent: Vec<_> = events
            .into_iter()
            .map(|event| self.send(Event::Connection(id, event)))
            .collect();
        let settled = match (self.tabs.get_mut(&id), self.app.connection(id)) {
            (Some(tab), Some(connection)) => tab
                .settle(connection)
                .map(move |message| Message::Connection(id, message)),
            _ => Task::none(),
        };
        Task::batch(sent.into_iter().chain([settled]))
    }

    fn close_menu(&mut self, id: u64) {
        if let Some(tab) = self.tabs.get_mut(&id) {
            tab.close_menu();
        }
    }

    fn update_manager(&mut self, message: ui::manager::Message) -> Task<Message> {
        use ui::manager::Message as Manager;

        let event = match message {
            Manager::Internal(message) => {
                self.manager.update(message, self.app.sessions());
                return Task::none();
            }
            Manager::Connect(id) => Event::OpenSaved(id),
            Manager::Save { editing, request } => Event::SaveSession { editing, request },
            Manager::SetGroup { id, group } => Event::SetGroup { id, group },
            Manager::Delete(id) => Event::DeleteSession(id),
            Manager::Export(id) => Event::ExportSession(id),
            Manager::Import => Event::ImportSessions,
        };
        self.manager.dismiss();
        self.send(event)
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
        let tab_modal = match self.app.page() {
            Page::Manager => ui::manager::modal(&self.manager, self.app.sessions())
                .map(|m| m.map(Message::Manager)),
            Page::Connect => self.connect_tab.modal().map(|m| m.map(Message::ConnectTab)),
            _ => None,
        };
        let window = match tab_modal {
            Some(modal) => stack![window, modal].into(),
            None => window,
        };
        let modal = match (&self.modal, self.app.dialog()) {
            (Some(modal), _) => Some(self.modal_view(modal)),
            (None, Some(dialog)) => Some(dialog_view(dialog)),
            (None, None) => None,
        };
        let window = match modal {
            Some(modal) => stack![window, modal].into(),
            None => window,
        };
        ui::pixel_snap(ui::arrow_cursor(window), self.scale_factor)
    }

    fn modal_view<'a>(&'a self, modal: &'a AppModal) -> Element<'a, Message> {
        let id = match modal {
            AppModal::ConfirmQuit(id) => *id,
            AppModal::ThirdPartyLicences => return third_party_licences(),
        };
        ui::dialog::view(
            fl!("quit-title"),
            fl!("quit-message", count = self.app.connections().len()),
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
        match self.app.page() {
            Page::Manager => {
                ui::manager::view(&self.manager, self.app.sessions()).map(Message::Manager)
            }
            Page::Connect => self.connect_tab.view().map(Message::ConnectTab),
            Page::Connection(id) => match (self.tabs.get(&id), self.app.connection(id)) {
                (Some(tab), Some(connection)) => tab
                    .view(connection, self.app.settings())
                    .map(move |message| Message::Connection(id, message)),
                _ => text(fl!("connection-closed")).into(),
            },
            Page::Settings => ui::settings::view(
                &self.settings_tab,
                self.app.settings(),
                self.app.update_status(),
            )
            .map(Message::Settings),
        }
    }

    fn theme(&self) -> iced::Theme {
        let light = match self.app.settings().general.appearance {
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
        let connections = self.app.connections().iter().map(|connection| {
            ui::connection::subscription(connection)
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
            self.app
                .connections()
                .iter()
                .map(|connection| self.connection_tab(connection)),
        )
        .push(self.icon_tab(Page::Connect, icons::ADD_LARGE, fl!("tab-connect")))
        .spacing(ui::style::SPACING);

        // Empty space in the title bar moves the window, as a native one would.
        let drag_region = mouse_area(
            Row::new()
                .push((!CUSTOM_DECORATIONS).then(|| Space::new().width(TRAFFIC_LIGHTS_WIDTH)))
                .push(self.icon_tab(Page::Manager, icons::VIRTUAL_DESKTOP, fl!("tab-manager")))
                .push(Scrollable::new(tabs).horizontal().width(Length::Fill))
                .spacing(ui::style::SPACING),
        )
        .on_press(Message::Window(WindowAction::Drag))
        .on_double_click(Message::Window(WindowAction::ToggleMaximize));

        Row::new()
            .push(drag_region)
            .push(self.icon_tab(Page::Settings, icons::SETTINGS, fl!("tab-settings")))
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

    fn icon_tab(&self, tab: Page, icon: icons::Icon, label: String) -> Element<'_, Message> {
        let active = self.app.page() == tab;
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

    fn connection_tab<'a>(&self, connection: &'a Connection) -> Element<'a, Message> {
        let id = connection.id();
        let active = self.app.page() == Page::Connection(id);

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
        .on_press(Message::SelectTab(Page::Connection(id)))
        .into()
    }
}

/// A dialog the app put up.
fn dialog_view<'a>(dialog: &'a Dialog) -> Element<'a, Message> {
    match dialog {
        Dialog::Error { title, message } => ui::dialog::view(
            title.clone(),
            message.clone(),
            vec![ui::dialog::Button {
                label: fl!("button-ok"),
                on_press: Message::CloseModal,
                kind: ui::dialog::Kind::Primary,
                icon: None,
                enabled: true,
            }],
        ),
        Dialog::ConnectionFailed { message, details } => ui::dialog::view(
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
        ),
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
