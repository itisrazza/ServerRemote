//! A self-contained connector that streams a moving gradient.
//!
//! It talks to nothing, so it's useful for exercising the UI and the session
//! plumbing without a real server. It also demonstrates every kind of UI a
//! connector can contribute: a power menu with confirmations, virtual media
//! mounted through a file picker, a web link, and status bar indicators.

use std::borrow::Cow;
use std::path::PathBuf;
use std::time::Duration;

use server_remote_icons as icons;
use tokio::sync::mpsc;

use crate::{
    Action, ActionValue, Capabilities, Chip, Confirmation, ConnectError, Connector, Control,
    Detail, Frame, IndicatorState, InputEvent, MenuItem, PointerMode, SessionCommand, SessionEvent,
    SessionHandle, Status, StatusItem, Target, TlsPolicy, ToolbarItem, Traffic,
};

const WIDTH: u32 = 320;
const HEIGHT: u32 = 240;
const FRAME_INTERVAL: Duration = Duration::from_millis(50);
const STATUS_INTERVAL: Duration = Duration::from_secs(1);
const PROJECT_URL: &str = "https://github.com/itisrazza/ServerRemote";

const RESET_ACTION: &str = "reset";
const SHUT_DOWN_ACTION: &str = "power.shut-down";
const RESTART_ACTION: &str = "power.restart";
const POWER_OFF_ACTION: &str = "power.off";
const COLD_BOOT_ACTION: &str = "power.cold-boot";
const MOUNT_ACTION: &str = "media.mount";
const EJECT_ACTION: &str = "media.eject";

pub struct DummyConnector;

impl Connector for DummyConnector {
    fn name(&self) -> &'static str {
        "Dummy"
    }

    async fn connect(
        &self,
        _target: &Target,
        _policy: &TlsPolicy,
    ) -> Result<SessionHandle, ConnectError> {
        let (event_tx, events) = mpsc::channel(16);
        let (input, input_rx) = mpsc::channel(64);
        let (commands, command_rx) = mpsc::channel::<SessionCommand>(16);

        tokio::spawn(run(event_tx, input_rx, command_rx));

        Ok(SessionHandle {
            events,
            input,
            commands,
        })
    }
}

/// The pretend machine behind the gradient.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Demo {
    powered_on: bool,
    mounted: Option<PathBuf>,
    frame_no: u32,
    traffic: Traffic,
}

impl Default for Demo {
    fn default() -> Self {
        Demo {
            powered_on: true,
            mounted: None,
            frame_no: 0,
            traffic: Traffic::default(),
        }
    }
}

/// What handling a command changed, so only the affected snapshots are
/// resent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct Changed {
    capabilities: bool,
    status: bool,
}

async fn run(
    event_tx: mpsc::Sender<SessionEvent>,
    mut input_rx: mpsc::Receiver<InputEvent>,
    mut command_rx: mpsc::Receiver<SessionCommand>,
) {
    let mut demo = Demo::default();
    let connected = SessionEvent::Connected {
        size: (WIDTH, HEIGHT),
        capabilities: capabilities(&demo),
    };
    if event_tx.send(connected).await.is_err()
        || event_tx
            .send(SessionEvent::Status(status(&demo)))
            .await
            .is_err()
    {
        return;
    }

    let mut frame_tick = tokio::time::interval(FRAME_INTERVAL);
    let mut status_tick = tokio::time::interval(STATUS_INTERVAL);
    let mut input_open = true;
    let mut commands_open = true;
    while input_open || commands_open {
        let events = tokio::select! {
            _ = frame_tick.tick() => vec![SessionEvent::Frame(next_frame(&mut demo))],
            _ = status_tick.tick() => vec![SessionEvent::Status(status(&demo))],
            incoming = input_rx.recv(), if input_open => {
                match incoming {
                    // Pretend each input event costs a few bytes on the wire.
                    Some(_) => demo.traffic.sent += 8,
                    None => input_open = false,
                }
                Vec::new()
            }
            command = command_rx.recv(), if commands_open => match command {
                Some(command) => {
                    let changed = apply(&mut demo, command);
                    changed_events(&demo, changed)
                }
                None => {
                    commands_open = false;
                    Vec::new()
                }
            },
        };

        for event in events {
            if event_tx.send(event).await.is_err() {
                return;
            }
        }
    }
}

fn changed_events(demo: &Demo, changed: Changed) -> Vec<SessionEvent> {
    let capabilities = changed
        .capabilities
        .then(|| SessionEvent::Capabilities(capabilities(demo)));
    let status = changed.status.then(|| SessionEvent::Status(status(demo)));
    capabilities.into_iter().chain(status).collect()
}

fn apply(demo: &mut Demo, command: SessionCommand) -> Changed {
    let SessionCommand::Action { id, value } = command;
    match (id.as_ref(), value) {
        (RESET_ACTION, _) => {
            demo.frame_no = 0;
            Changed::default()
        }
        (SHUT_DOWN_ACTION | POWER_OFF_ACTION, _) => {
            demo.powered_on = false;
            Changed {
                capabilities: true,
                status: true,
            }
        }
        (RESTART_ACTION | COLD_BOOT_ACTION, _) => {
            demo.powered_on = true;
            demo.frame_no = 0;
            Changed {
                capabilities: true,
                status: true,
            }
        }
        (MOUNT_ACTION, Some(ActionValue::File(path))) => {
            demo.mounted = Some(path);
            Changed {
                capabilities: true,
                status: true,
            }
        }
        (EJECT_ACTION, _) => {
            demo.mounted = None;
            Changed {
                capabilities: true,
                status: true,
            }
        }
        _ => Changed::default(),
    }
}

fn capabilities(demo: &Demo) -> Capabilities {
    Capabilities {
        pointer: PointerMode::Absolute,
        input_actions: vec![Action::button(RESET_ACTION, "Reset gradient")],
        toolbar: vec![power_menu(demo), media_menu(demo), web_link()],
    }
}

fn power_menu(demo: &Demo) -> ToolbarItem {
    let action = |id: &'static str, label, icon, title, message, confirm_label, destructive| {
        MenuItem::Action(Action::button(id, label).icon(icon).confirm(Confirmation {
            title: Cow::Borrowed(title),
            message: Cow::Borrowed(message),
            confirm_label: Cow::Borrowed(confirm_label),
            destructive,
        }))
    };
    let on = demo.powered_on;

    ToolbarItem::Menu {
        id: Cow::Borrowed("power"),
        icon: icons::POWER.into(),
        label: Cow::Borrowed("Power"),
        items: vec![
            enabled_if(
                on,
                action(
                    SHUT_DOWN_ACTION,
                    "Shut down…",
                    icons::POWER,
                    "Shut down?",
                    "Asks the operating system to shut down gracefully.",
                    "Shut down",
                    false,
                ),
            ),
            enabled_if(
                on,
                action(
                    RESTART_ACTION,
                    "Restart…",
                    icons::RESTART,
                    "Restart?",
                    "Asks the operating system to restart gracefully.",
                    "Restart",
                    false,
                ),
            ),
            MenuItem::Separator,
            enabled_if(
                on,
                action(
                    POWER_OFF_ACTION,
                    "Power off…",
                    icons::POWER,
                    "Power off?",
                    "Cuts power immediately. Unsaved work will be lost.",
                    "Power off",
                    true,
                ),
            ),
            action(
                COLD_BOOT_ACTION,
                "Cold boot…",
                icons::RESTART,
                "Cold boot?",
                "Cuts power and starts the machine again. Unsaved work will be lost.",
                "Cold boot",
                true,
            ),
        ],
    }
}

fn media_menu(demo: &Demo) -> ToolbarItem {
    let mounted = demo.mounted.is_some();
    ToolbarItem::Menu {
        id: Cow::Borrowed("media"),
        icon: icons::VMDK_DISK.into(),
        label: Cow::Borrowed("Virtual media"),
        items: vec![
            MenuItem::Action(
                Action::button(MOUNT_ACTION, "Mount image…")
                    .icon(icons::FOLDER)
                    .control(Control::File {
                        extensions: vec![Cow::Borrowed("iso"), Cow::Borrowed("img")],
                    })
                    .enabled(!mounted),
            ),
            MenuItem::Action(Action::button(EJECT_ACTION, "Eject").enabled(mounted)),
        ],
    }
}

fn web_link() -> ToolbarItem {
    ToolbarItem::Link {
        icon: icons::LAUNCH.into(),
        label: Cow::Borrowed("Open project page"),
        url: Cow::Borrowed(PROJECT_URL),
    }
}

fn enabled_if(enabled: bool, item: MenuItem) -> MenuItem {
    match item {
        MenuItem::Action(action) => MenuItem::Action(action.enabled(enabled)),
        other => other,
    }
}

fn status(demo: &Demo) -> Status {
    let power = if demo.powered_on {
        StatusItem::Indicator {
            label: Cow::Borrowed("Powered on"),
            state: IndicatorState::Ok,
        }
    } else {
        StatusItem::Indicator {
            label: Cow::Borrowed("Powered off"),
            state: IndicatorState::Inactive,
        }
    };
    let chips = demo
        .mounted
        .iter()
        .map(|path| Chip {
            icon: Some(icons::VMDK_DISK.into()),
            label: path
                .file_name()
                .map_or_else(|| path.to_string_lossy(), |name| name.to_string_lossy())
                .into_owned()
                .into(),
            close_action: Some(Cow::Borrowed(EJECT_ACTION)),
        })
        .collect();

    Status {
        encryption: None,
        traffic: Some(demo.traffic),
        items: vec![
            power,
            StatusItem::Text {
                icon: None,
                text: format!("Frame: {}", demo.frame_no).into(),
            },
        ],
        chips,
        details: vec![Detail {
            label: Cow::Borrowed("Generator"),
            value: Cow::Borrowed("Test pattern"),
        }],
    }
}

/// The next frame: the gradient while powered on, black while off.
fn next_frame(demo: &mut Demo) -> Frame {
    let frame = if demo.powered_on {
        gradient_frame(demo.frame_no)
    } else {
        blank_frame()
    };
    demo.frame_no = demo.frame_no.wrapping_add(1);
    demo.traffic.received += frame.pixels.len() as u64;
    frame
}

fn blank_frame() -> Frame {
    let pixels = [0, 0, 0, 255].repeat((WIDTH * HEIGHT) as usize);
    Frame {
        width: WIDTH,
        height: HEIGHT,
        pixels,
    }
}

fn gradient_frame(frame_no: u32) -> Frame {
    let mut pixels = vec![0u8; (WIDTH * HEIGHT * 4) as usize];
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let i = ((y * WIDTH + x) * 4) as usize;
            pixels[i] = x.wrapping_add(frame_no) as u8; // R
            pixels[i + 1] = y as u8; // G
            pixels[i + 2] = frame_no as u8; // B
            pixels[i + 3] = 255; // A
        }
    }
    Frame {
        width: WIDTH,
        height: HEIGHT,
        pixels,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn connect() -> SessionHandle {
        DummyConnector
            .connect(&Target::new("localhost", 0), &TlsPolicy::default())
            .await
            .expect("dummy connect never fails")
    }

    fn action(id: &'static str, value: Option<ActionValue>) -> SessionCommand {
        SessionCommand::Action {
            id: Cow::Borrowed(id),
            value,
        }
    }

    /// R channel of the top-left pixel, which encodes `frame_no % 256`.
    fn first_frame_no(frame: &Frame) -> u8 {
        frame.pixels[0]
    }

    fn menu_action<'a>(capabilities: &'a Capabilities, id: &str) -> &'a Action {
        capabilities
            .toolbar
            .iter()
            .filter_map(|item| match item {
                ToolbarItem::Menu { items, .. } => Some(items),
                ToolbarItem::Link { .. } => None,
            })
            .flatten()
            .find_map(|item| match item {
                MenuItem::Action(action) if action.id == id => Some(action),
                _ => None,
            })
            .expect("action is in a toolbar menu")
    }

    #[test]
    fn commands_update_the_demo_state() {
        let mounted = PathBuf::from("/images/installer.iso");
        let cases = [
            (
                "shut down powers off",
                Demo::default(),
                action(SHUT_DOWN_ACTION, None),
                Demo {
                    powered_on: false,
                    ..Demo::default()
                },
            ),
            (
                "cold boot powers on from off",
                Demo {
                    powered_on: false,
                    frame_no: 7,
                    ..Demo::default()
                },
                action(COLD_BOOT_ACTION, None),
                Demo::default(),
            ),
            (
                "mount stores the picked file",
                Demo::default(),
                action(MOUNT_ACTION, Some(ActionValue::File(mounted.clone()))),
                Demo {
                    mounted: Some(mounted.clone()),
                    ..Demo::default()
                },
            ),
            (
                "mount without a file is ignored",
                Demo::default(),
                action(MOUNT_ACTION, None),
                Demo::default(),
            ),
            (
                "eject clears the mount",
                Demo {
                    mounted: Some(mounted.clone()),
                    ..Demo::default()
                },
                action(EJECT_ACTION, None),
                Demo::default(),
            ),
            (
                "unknown actions are ignored",
                Demo::default(),
                action("nonsense", None),
                Demo::default(),
            ),
        ];

        for (name, mut demo, command, expected) in cases {
            apply(&mut demo, command);
            assert_eq!(demo, expected, "{name}");
        }
    }

    #[test]
    fn capabilities_follow_power_and_media_state() {
        let off_and_mounted = Demo {
            powered_on: false,
            mounted: Some(PathBuf::from("disk.img")),
            ..Demo::default()
        };
        let cases = [
            (
                "on, nothing mounted",
                Demo::default(),
                [
                    (SHUT_DOWN_ACTION, true),
                    (COLD_BOOT_ACTION, true),
                    (MOUNT_ACTION, true),
                    (EJECT_ACTION, false),
                ],
            ),
            (
                "off, image mounted",
                off_and_mounted,
                [
                    (SHUT_DOWN_ACTION, false),
                    (COLD_BOOT_ACTION, true),
                    (MOUNT_ACTION, false),
                    (EJECT_ACTION, true),
                ],
            ),
        ];

        for (name, demo, expected) in cases {
            let capabilities = capabilities(&demo);
            for (id, enabled) in expected {
                assert_eq!(
                    menu_action(&capabilities, id).enabled,
                    enabled,
                    "{name}: {id}"
                );
            }
        }
    }

    #[test]
    fn power_actions_ask_for_confirmation() {
        let capabilities = capabilities(&Demo::default());
        for id in [
            SHUT_DOWN_ACTION,
            RESTART_ACTION,
            POWER_OFF_ACTION,
            COLD_BOOT_ACTION,
        ] {
            assert!(menu_action(&capabilities, id).confirm.is_some(), "{id}");
        }
    }

    #[test]
    fn mounted_image_shows_as_a_dismissable_chip() {
        let status = status(&Demo {
            mounted: Some(PathBuf::from("/images/installer.iso")),
            ..Demo::default()
        });
        assert_eq!(
            status.chips,
            vec![Chip {
                icon: Some(icons::VMDK_DISK.into()),
                label: Cow::Borrowed("installer.iso"),
                close_action: Some(Cow::Borrowed(EJECT_ACTION)),
            }]
        );
    }

    #[tokio::test]
    async fn streams_connected_status_then_frames() {
        let mut handle = connect().await;

        match handle.events.recv().await {
            Some(SessionEvent::Connected { size, capabilities }) => {
                assert_eq!(size, (WIDTH, HEIGHT));
                assert_eq!(capabilities.pointer, PointerMode::Absolute);
                assert_eq!(capabilities.input_actions.len(), 1);
                assert_eq!(capabilities.input_actions[0].id, RESET_ACTION);
            }
            other => panic!("expected Connected, got {other:?}"),
        }

        assert!(matches!(
            handle.events.recv().await,
            Some(SessionEvent::Status(_))
        ));

        let frame = loop {
            if let Some(SessionEvent::Frame(frame)) = handle.events.recv().await {
                break frame;
            }
        };
        assert_eq!(frame.width, WIDTH);
        assert_eq!(frame.height, HEIGHT);
        assert_eq!(frame.pixels.len(), (WIDTH * HEIGHT * 4) as usize);
    }

    #[tokio::test]
    async fn reset_action_restarts_the_gradient() {
        let mut handle = connect().await;

        // Advance a few frames so the counter is clearly non-zero.
        let mut advanced = false;
        for _ in 0..8 {
            if let Some(SessionEvent::Frame(frame)) = handle.events.recv().await
                && first_frame_no(&frame) > 0
            {
                advanced = true;
                break;
            }
        }
        assert!(advanced, "gradient never advanced");

        handle
            .commands
            .send(action(RESET_ACTION, None))
            .await
            .unwrap();

        // A post-reset frame restarts at frame_no 0.
        let mut reset_seen = false;
        for _ in 0..4 {
            if let Some(SessionEvent::Frame(frame)) = handle.events.recv().await
                && first_frame_no(&frame) == 0
            {
                reset_seen = true;
                break;
            }
        }
        assert!(reset_seen, "gradient did not reset");
    }

    #[tokio::test]
    async fn power_off_resends_capabilities_and_status() {
        let mut handle = connect().await;
        handle
            .commands
            .send(action(POWER_OFF_ACTION, None))
            .await
            .unwrap();

        let mut capabilities_seen = false;
        let mut powered_off_seen = false;
        for _ in 0..16 {
            match handle.events.recv().await {
                Some(SessionEvent::Capabilities(capabilities)) => {
                    capabilities_seen = !menu_action(&capabilities, SHUT_DOWN_ACTION).enabled;
                }
                Some(SessionEvent::Status(status)) => {
                    powered_off_seen = status.items.contains(&StatusItem::Indicator {
                        label: Cow::Borrowed("Powered off"),
                        state: IndicatorState::Inactive,
                    });
                }
                _ => {}
            }
            if capabilities_seen && powered_off_seen {
                break;
            }
        }
        assert!(capabilities_seen, "capabilities not resent");
        assert!(powered_off_seen, "status not resent");
    }

    #[tokio::test]
    async fn shuts_down_when_handle_dropped() {
        let handle = connect().await;
        let input = handle.input.clone();
        drop(handle);

        // Once every request channel is dropped, the task exits and closes its
        // receivers, so the surviving sender clone reports the channel closed.
        input.closed().await;
    }
}
