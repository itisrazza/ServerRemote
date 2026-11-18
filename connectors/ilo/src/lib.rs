//! HPE iLO 4 Integrated Remote Console connector.
//!
//! Implements the v1 protocol described in `docs/protocols/ilo4.md`: log in
//! over HTTPS, fetch the console's keys and port (`rc_info`), negotiate the
//! KVM and command connections, then decode the video stream and send HID
//! keyboard and mouse reports. The vendor client is Windows-only and the
//! target hardware predates the HTML5 console, which is a key motivation for
//! the project.
//!
//! Not yet supported: virtual media, seizing or sharing a console another
//! user holds, and the newer v2 protocol.

mod cipher;
mod command;
mod dvc;
mod http;
mod input;
mod negotiate;
mod web;

use std::borrow::Cow;
use std::time::Duration;

use server_remote_core::{
    Action, Capabilities, Confirmation, ConnectError, Connector, Detail, Frame, IndicatorState,
    InputEvent, MenuItem, PointerMode, SessionCommand, SessionEvent, SessionHandle, Status,
    StatusItem, Target, TlsPolicy, ToolbarItem, Traffic,
};
use server_remote_icons as icons;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::sync::mpsc;
use tokio::time::{Instant, MissedTickBehavior};

use cipher::{CipherKind, Keystream};
use command::{Health, Notice, Packet};
use dvc::{Decoder, Event};
use input::{Keyboard, Mouse};
use negotiate::Channel;
use web::RcInfo;

const CAD_ACTION: &str = "ilo.ctrl-alt-del";
const REFRESH_ACTION: &str = "ilo.refresh";
const POWER_PRESS_ACTION: &str = "ilo.power.press";
const POWER_HOLD_ACTION: &str = "ilo.power.hold";
const POWER_CYCLE_ACTION: &str = "ilo.power.cycle";
const POWER_RESET_ACTION: &str = "ilo.power.reset";

/// KVM messages the session sends.
const REFRESH: [u8; 2] = [0x05, 0x00];
const ACK: [u8; 2] = [0x0C, 0x00];
const CLOSING: [u8; 2] = [0x0B, 0x00];

/// The vendor client gives up on a KVM connection silent for this long.
const KVM_SILENCE: Duration = Duration::from_secs(45);
/// How often traffic counters are reported.
const STATUS_INTERVAL: Duration = Duration::from_secs(1);

/// Connects to an iLO 4's remote console with an iLO user name and password.
pub struct IloConnector {
    user: String,
    password: String,
}

impl IloConnector {
    pub fn new(user: String, password: String) -> Self {
        Self { user, password }
    }
}

impl Connector for IloConnector {
    fn name(&self) -> &'static str {
        "iLO"
    }

    async fn connect(
        &self,
        target: &Target,
        policy: &TlsPolicy,
    ) -> Result<SessionHandle, ConnectError> {
        if self.user.is_empty() {
            return Err(ConnectError::Auth("iLO needs a user name".into()));
        }
        let web = web::Session::login(target, policy, &self.user, &self.password).await?;
        match open(target, policy, &web).await {
            Ok(opened) => Ok(start(opened, target.clone(), policy.clone(), web)),
            Err(error) => {
                let _ = web.logout(target, policy).await;
                Err(error)
            }
        }
    }
}

/// The console connections, negotiated and ready.
struct Opened {
    rc: RcInfo,
    kvm: TcpStream,
    command: TcpStream,
    command_encrypted: bool,
}

async fn open(
    target: &Target,
    policy: &TlsPolicy,
    web: &web::Session,
) -> Result<Opened, ConnectError> {
    if !web.privileges.remote_console {
        return Err(ConnectError::Auth(
            "this iLO user may not use the remote console".into(),
        ));
    }
    let rc = web.rc_info(target, policy).await?;

    // The KVM connection is negotiated first, then the command connection.
    let mut kvm = console_connection(&target.host, rc.rc_port).await?;
    negotiate::negotiate(&mut kvm, Channel::Kvm, &web.key, &rc).await?;
    let mut command = console_connection(&target.host, rc.rc_port).await?;
    negotiate::negotiate(&mut command, Channel::Command, &web.key, &rc).await?;

    let command_encrypted = rc.features.encrypt_cmd;
    if command_encrypted {
        command.write_all(&command::START_ENCRYPTION).await?;
    }
    Ok(Opened {
        rc,
        kvm,
        command,
        command_encrypted,
    })
}

async fn console_connection(host: &str, port: u16) -> Result<TcpStream, ConnectError> {
    let stream = TcpStream::connect((host, port)).await?;
    stream.set_nodelay(true)?;
    Ok(stream)
}

fn start(opened: Opened, target: Target, policy: TlsPolicy, web: web::Session) -> SessionHandle {
    let (event_tx, events) = mpsc::channel(16);
    let (input, input_rx) = mpsc::channel(256);
    let (commands, command_rx) = mpsc::channel(16);

    let Opened {
        rc,
        kvm,
        command,
        command_encrypted,
    } = opened;
    let (kvm_read, kvm_write) = kvm.into_split();
    let (command_read, command_write) = command.into_split();
    let command_cipher = || {
        if command_encrypted {
            Keystream::new(CipherKind::Aes128, &rc.cmd_enc_key)
        } else {
            Keystream::None
        }
    };

    let (kvm_tx, kvm_rx) = mpsc::channel(64);
    let kvm_reader = tokio::spawn(read_kvm(kvm_read, kvm_tx));
    let (packet_tx, packet_rx) = mpsc::channel(16);
    let command_reader = tokio::spawn(read_commands(command_read, command_cipher(), packet_tx));

    let session = Session {
        event_tx,
        decoder: Decoder::new(rc.enc_key),
        kvm_write,
        kvm_cipher: Keystream::None,
        command_write,
        command_cipher: command_cipher(),
        command_encrypted,
        ready: false,
        connected: false,
        keyboard: Keyboard::default(),
        mouse: Mouse::default(),
        console: Console {
            reset_allowed: web.privileges.reset,
            web_url: web_url(&target),
            ..Console::default()
        },
        server_name: rc.server_name.clone(),
        enc_key: rc.enc_key,
        traffic: Traffic::default(),
    };
    tokio::spawn(async move {
        session.run(kvm_rx, packet_rx, input_rx, command_rx).await;
        kvm_reader.abort();
        command_reader.abort();
        let _ = web.logout(&target, &policy).await;
    });

    SessionHandle {
        events,
        input,
        commands,
    }
}

fn web_url(target: &Target) -> String {
    let host = if target.host.contains(':') {
        format!("[{}]", target.host)
    } else {
        target.host.clone()
    };
    if target.port == 443 {
        format!("https://{host}/")
    } else {
        format!("https://{host}:{}/", target.port)
    }
}

/// Forwards raw KVM bytes. The decoder copes with any chunking.
async fn read_kvm(mut read: OwnedReadHalf, tx: mpsc::Sender<Result<Vec<u8>, String>>) {
    let mut buf = vec![0u8; 16 * 1024];
    loop {
        let message = match read.read(&mut buf).await {
            Ok(0) => Err("the iLO closed the console connection".to_owned()),
            Ok(n) => Ok(buf[..n].to_vec()),
            Err(error) => Err(error.to_string()),
        };
        let end = message.is_err();
        if tx.send(message).await.is_err() || end {
            return;
        }
    }
}

/// Reads whole command packets, so a packet is never split across `select!`
/// branches.
async fn read_commands(
    mut read: OwnedReadHalf,
    mut cipher: Keystream,
    tx: mpsc::Sender<Result<Packet, String>>,
) {
    loop {
        let message = command::read_packet(&mut read, &mut cipher)
            .await
            .map_err(|error| error.to_string());
        let end = message.is_err();
        if tx.send(message).await.is_err() || end {
            return;
        }
    }
}

/// What the session knows about the server, for capabilities and status.
#[derive(Debug, Default)]
struct Console {
    power: Option<bool>,
    health: Option<Health>,
    post_code: Option<u16>,
    message: Option<String>,
    cipher: Option<CipherKind>,
    reset_allowed: bool,
    web_url: String,
}

struct Session {
    event_tx: mpsc::Sender<SessionEvent>,
    decoder: Decoder,
    kvm_write: OwnedWriteHalf,
    /// The client→server KVM keystream; restarts whenever the server picks a
    /// cipher.
    kvm_cipher: Keystream,
    command_write: OwnedWriteHalf,
    command_cipher: Keystream,
    command_encrypted: bool,
    /// The session header has arrived, so the client may send.
    ready: bool,
    /// `Connected` has been reported.
    connected: bool,
    keyboard: Keyboard,
    mouse: Mouse,
    console: Console,
    server_name: Option<String>,
    enc_key: [u8; 16],
    traffic: Traffic,
}

/// Why the session loop stopped.
enum Stop {
    /// The UI went away; close quietly.
    Closed,
    /// Report this reason to the UI.
    Ended(Option<String>),
}

impl Session {
    async fn run(
        mut self,
        mut kvm_rx: mpsc::Receiver<Result<Vec<u8>, String>>,
        mut packet_rx: mpsc::Receiver<Result<Packet, String>>,
        mut input_rx: mpsc::Receiver<InputEvent>,
        mut command_rx: mpsc::Receiver<SessionCommand>,
    ) {
        let mut status_tick = tokio::time::interval(STATUS_INTERVAL);
        status_tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
        let mut last_video = Instant::now();
        let mut commands_open = true;
        let mut packets_open = true;

        let stop = loop {
            let step = tokio::select! {
                chunk = kvm_rx.recv() => {
                    last_video = Instant::now();
                    self.on_video(chunk, &mut kvm_rx).await
                }
                packet = packet_rx.recv(), if packets_open => match packet {
                    Some(Ok(packet)) => self.on_packet(packet).await,
                    // The video can carry on without the command channel.
                    Some(Err(_)) | None => {
                        packets_open = false;
                        Ok(())
                    }
                },
                input = input_rx.recv() => match input {
                    Some(event) => self.on_input(event).await,
                    None => Err(Stop::Closed),
                },
                command = command_rx.recv(), if commands_open => match command {
                    Some(SessionCommand::Action { id, .. }) => self.on_action(&id).await,
                    None => {
                        commands_open = false;
                        Ok(())
                    }
                },
                _ = status_tick.tick() => self.report_status().await,
                _ = tokio::time::sleep_until(last_video + KVM_SILENCE) => Err(Stop::Ended(Some(
                    "the iLO stopped sending video".into(),
                ))),
            };
            if let Err(stop) = step {
                break stop;
            }
        };

        if self.ready {
            let _ = self.send_kvm(&CLOSING).await;
        }
        let _ = self.kvm_write.shutdown().await;
        let _ = self.command_write.shutdown().await;
        if let Stop::Ended(reason) = stop {
            let _ = self
                .event_tx
                .send(SessionEvent::Disconnected { reason })
                .await;
        }
    }

    /// Decodes a chunk of video, and any others already queued, then sends
    /// one frame for all of them.
    async fn on_video(
        &mut self,
        chunk: Option<Result<Vec<u8>, String>>,
        kvm_rx: &mut mpsc::Receiver<Result<Vec<u8>, String>>,
    ) -> Result<(), Stop> {
        let mut events = Vec::new();
        let mut next = chunk;
        loop {
            match next {
                Some(Ok(bytes)) => {
                    self.traffic.received += bytes.len() as u64;
                    self.decoder.feed(&bytes, &mut events);
                }
                Some(Err(reason)) => return Err(Stop::Ended(Some(reason))),
                None => return Err(Stop::Ended(None)),
            }
            next = match kvm_rx.try_recv() {
                Ok(message) => Some(message),
                Err(_) => break,
            };
        }

        let mut frame = false;
        for event in events {
            match event {
                Event::Frame => frame = true,
                other => self.on_decoder_event(other).await?,
            }
        }
        if frame {
            self.send_frame().await?;
        }
        Ok(())
    }

    async fn on_decoder_event(&mut self, event: Event) -> Result<(), Stop> {
        match event {
            Event::Cipher(kind) => {
                self.kvm_cipher = Keystream::new(kind, &self.enc_key);
                self.console.cipher = Some(kind);
                self.report_status().await?;
            }
            Event::Header { .. } => {
                self.ready = true;
                if !self.connected {
                    self.connected = true;
                    self.emit(SessionEvent::Connected {
                        size: self.decoder.size(),
                        capabilities: self.capabilities(),
                    })
                    .await?;
                }
                let release = self.keyboard.release_all();
                self.send_kvm(&release).await?;
                self.send_kvm(&REFRESH).await?;
            }
            Event::Mode { width, height } => {
                self.emit(SessionEvent::Resized { width, height }).await?;
            }
            Event::Print { text, .. } => {
                self.console.message = (!text.is_empty()).then_some(text);
                self.report_status().await?;
            }
            Event::PowerOn => self.set_power(true).await?,
            Event::PowerOff => self.set_power(false).await?,
            Event::AckRequested => self.send_kvm(&ACK).await?,
            Event::RefreshNeeded => self.send_kvm(&REFRESH).await?,
            Event::Exit => {
                return Err(Stop::Ended(Some(
                    "the iLO ended the console session".into(),
                )));
            }
            Event::Frame => self.send_frame().await?,
        }
        Ok(())
    }

    async fn on_packet(&mut self, packet: Packet) -> Result<(), Stop> {
        self.traffic.received += 12 + packet.data.len() as u64;
        match packet.notice() {
            Notice::Power(on) => self.set_power(on).await?,
            Notice::Health(health) => {
                if self.console.health != Some(health) {
                    self.console.health = Some(health);
                    self.report_status().await?;
                }
            }
            Notice::PostCode(code) => {
                self.console.post_code = Some(code);
                self.report_status().await?;
            }
            // Like the vendor client's default: let the other user in.
            Notice::Seize { user } => {
                let _ = self.send_command(&command::YES).await;
                return Err(Stop::Ended(Some(format!(
                    "{user} took over the remote console"
                ))));
            }
            // Sharing needs this client to relay video; decline.
            Notice::Share { .. } => self.send_command(&command::NO).await?,
            Notice::Ended(reason) => return Err(Stop::Ended(Some(reason.into()))),
            Notice::Ignored => {}
        }
        Ok(())
    }

    async fn on_input(&mut self, event: InputEvent) -> Result<(), Stop> {
        if !self.ready {
            return Ok(());
        }
        let size = self.decoder.size();
        let message = match event {
            InputEvent::PointerMoved { x, y } => Some(self.mouse.moved(x, y, size)),
            InputEvent::PointerButton { button, pressed } => {
                Some(self.mouse.button(button, pressed, size))
            }
            // The iLO takes absolute positions; relative motion isn't sent.
            InputEvent::PointerMotion { .. } => None,
            InputEvent::Key { keysym, pressed } => self.keyboard.key(keysym, pressed),
        };
        match message {
            Some(message) => self.send_kvm(&message).await,
            None => Ok(()),
        }
    }

    async fn on_action(&mut self, id: &str) -> Result<(), Stop> {
        if !self.ready {
            return Ok(());
        }
        let power = |option: u8| [0x00, 0x00, option, 0x00];
        match id {
            CAD_ACTION => {
                let press = input::report(input::MODIFIERS_CTRL_ALT, &[input::HID_DELETE]);
                self.send_kvm(&press).await?;
                tokio::time::sleep(Duration::from_millis(250)).await;
                self.send_kvm(&input::report(input::MODIFIERS_CTRL_ALT, &[]))
                    .await?;
                let release = self.keyboard.release_all();
                self.send_kvm(&release).await
            }
            REFRESH_ACTION => self.send_kvm(&REFRESH).await,
            POWER_PRESS_ACTION => self.send_kvm(&power(0)).await,
            POWER_HOLD_ACTION => self.send_kvm(&power(1)).await,
            POWER_CYCLE_ACTION => self.send_kvm(&power(2)).await,
            POWER_RESET_ACTION => self.send_kvm(&power(3)).await,
            _ => Ok(()),
        }
    }

    async fn set_power(&mut self, on: bool) -> Result<(), Stop> {
        if self.console.power == Some(on) {
            return Ok(());
        }
        self.console.power = Some(on);
        if !on {
            self.console.post_code = None;
        }
        if self.connected {
            self.emit(SessionEvent::Capabilities(self.capabilities()))
                .await?;
        }
        self.report_status().await
    }

    async fn send_frame(&mut self) -> Result<(), Stop> {
        let (width, height) = self.decoder.size();
        if width == 0 || height == 0 {
            return Ok(());
        }
        self.emit(SessionEvent::Frame(Frame {
            width,
            height,
            pixels: self.decoder.framebuffer().to_vec(),
        }))
        .await
    }

    async fn send_kvm(&mut self, message: &[u8]) -> Result<(), Stop> {
        let mut bytes = message.to_vec();
        self.kvm_cipher.apply(&mut bytes);
        self.traffic.sent += bytes.len() as u64;
        self.kvm_write
            .write_all(&bytes)
            .await
            .map_err(|error| Stop::Ended(Some(error.to_string())))
    }

    async fn send_command(&mut self, message: &[u8]) -> Result<(), Stop> {
        let mut bytes = message.to_vec();
        self.command_cipher.apply(&mut bytes);
        self.traffic.sent += bytes.len() as u64;
        self.command_write
            .write_all(&bytes)
            .await
            .map_err(|error| Stop::Ended(Some(error.to_string())))
    }

    async fn emit(&self, event: SessionEvent) -> Result<(), Stop> {
        self.event_tx.send(event).await.map_err(|_| Stop::Closed)
    }

    async fn report_status(&self) -> Result<(), Stop> {
        if !self.connected {
            return Ok(());
        }
        self.emit(SessionEvent::Status(self.status())).await
    }

    fn capabilities(&self) -> Capabilities {
        capabilities(&self.console)
    }

    fn status(&self) -> Status {
        let mut status = status(&self.console, self.command_encrypted, self.traffic);
        if let Some(name) = &self.server_name {
            status.details.insert(
                0,
                Detail {
                    label: "Server".into(),
                    value: name.clone().into(),
                },
            );
        }
        status
    }
}

fn capabilities(console: &Console) -> Capabilities {
    let mut toolbar = Vec::new();
    if console.reset_allowed {
        toolbar.push(power_menu(console.power));
    }
    toolbar.push(ToolbarItem::Link {
        icon: icons::LAUNCH.into(),
        label: "Open iLO web interface".into(),
        url: console.web_url.clone().into(),
    });
    Capabilities {
        pointer: PointerMode::Absolute,
        input_actions: vec![
            Action::button(CAD_ACTION, "Ctrl+Alt+Del"),
            Action::button(REFRESH_ACTION, "Refresh screen"),
        ],
        toolbar,
    }
}

fn power_menu(power: Option<bool>) -> ToolbarItem {
    let on = power != Some(false);
    let action = |id: &'static str,
                  label: &'static str,
                  icon: server_remote_icons::Icon,
                  title: &'static str,
                  message: &'static str,
                  confirm_label: &'static str,
                  destructive: bool| {
        Action::button(id, label).icon(icon).confirm(Confirmation {
            title: title.into(),
            message: message.into(),
            confirm_label: confirm_label.into(),
            destructive,
        })
    };
    let press = if on {
        action(
            POWER_PRESS_ACTION,
            "Press power button…",
            icons::POWER,
            "Press the power button?",
            "Presses the server's power button briefly. Most operating systems shut down gracefully.",
            "Press",
            false,
        )
    } else {
        action(
            POWER_PRESS_ACTION,
            "Power on…",
            icons::POWER,
            "Power on?",
            "Presses the server's power button to turn it on.",
            "Power on",
            false,
        )
    };
    ToolbarItem::Menu {
        id: Cow::Borrowed("ilo.power"),
        icon: icons::POWER.into(),
        label: "Power".into(),
        items: vec![
            MenuItem::Action(press),
            MenuItem::Separator,
            MenuItem::Action(
                action(
                    POWER_HOLD_ACTION,
                    "Press and hold…",
                    icons::POWER,
                    "Hold the power button?",
                    "Holds the server's power button until it turns off. Unsaved work will be lost.",
                    "Hold",
                    true,
                )
                .enabled(on),
            ),
            MenuItem::Action(
                action(
                    POWER_RESET_ACTION,
                    "Reset…",
                    icons::RESTART,
                    "Reset the server?",
                    "Restarts the server immediately. Unsaved work will be lost.",
                    "Reset",
                    true,
                )
                .enabled(on),
            ),
            MenuItem::Action(
                action(
                    POWER_CYCLE_ACTION,
                    "Cold boot…",
                    icons::RESTART,
                    "Cold boot?",
                    "Cuts power and starts the server again. Unsaved work will be lost.",
                    "Cold boot",
                    true,
                )
                .enabled(on),
            ),
        ],
    }
}

fn status(console: &Console, command_encrypted: bool, traffic: Traffic) -> Status {
    let mut items = Vec::new();
    if let Some(on) = console.power {
        items.push(StatusItem::Indicator {
            label: if on { "Powered on" } else { "Powered off" }.into(),
            state: if on {
                IndicatorState::Ok
            } else {
                IndicatorState::Inactive
            },
        });
    }
    if let Some(health) = console.health {
        items.push(StatusItem::Indicator {
            label: "Health".into(),
            state: match health {
                Health::Normal => IndicatorState::Ok,
                Health::Caution => IndicatorState::Warning,
                Health::Critical => IndicatorState::Error,
                Health::Inactive => IndicatorState::Inactive,
            },
        });
    }
    if let Some(code) = console.post_code.filter(|&code| code != 0) {
        items.push(StatusItem::Text {
            icon: None,
            text: format!("POST: {code:04X}").into(),
        });
    }
    if let Some(message) = &console.message {
        items.push(StatusItem::Text {
            icon: None,
            text: message.clone().into(),
        });
    }

    Status {
        encryption: console
            .cipher
            .and_then(CipherKind::label)
            .map(Cow::Borrowed),
        traffic: Some(traffic),
        items,
        chips: Vec::new(),
        details: vec![Detail {
            label: "Command channel".into(),
            value: if command_encrypted {
                "AES"
            } else {
                "Unencrypted"
            }
            .into(),
        }],
    }
}

#[cfg(test)]
mod tests;
