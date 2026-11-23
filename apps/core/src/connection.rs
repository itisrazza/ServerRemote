//! One open connection: runs a [`Connector`] ([`run`]), keeps what the
//! session reports, and turns the user's input into session input.
//!
//! The runner hands back the session's input and command senders
//! ([`Report::Ready`]), so input goes to the session directly. Front ends
//! render a [`Connection`] and feed it [`Event`]s; what they must do in turn
//! (pick a file, open a link) comes back as an [`Effect`].

use std::borrow::Cow;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use server_remote_core::{
    Action, ActionValue, Capabilities, CertFingerprint, ConnectError, Connector, Control,
    DummyConnector, InputEvent, PointerButton, PointerMode, SessionCommand, SessionEvent,
    SessionHandle, Status, Target, TlsPolicy, Traffic,
};
use server_remote_ilo::IloConnector;
use server_remote_spice::SpiceConnector;
use server_remote_vnc::VncConnector;
use tokio::sync::mpsc;

use crate::Protocol;
use crate::i18n::fl;

/// Where the connection is in its lifecycle.
#[derive(Debug, Clone, PartialEq)]
pub enum Phase {
    Connecting,
    Connected,
    Disconnected(Option<String>),
}

/// What the user signs in with. Only iLO uses the user name.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct Credentials {
    pub username: String,
    pub password: String,
}

/// The senders used to talk to a running session.
#[derive(Debug, Clone)]
pub struct Controls {
    input: mpsc::Sender<InputEvent>,
    commands: mpsc::Sender<SessionCommand>,
}

/// The last screen received, as RGBA.
#[derive(Debug, Clone)]
pub struct Screen {
    /// Counts up once per frame received, so a front end can tell whether
    /// its copy is stale.
    pub generation: u64,
    pub width: u32,
    pub height: u32,
    pub pixels: Arc<[u8]>,
}

/// A dialog shown over the connection.
#[derive(Debug, Clone)]
pub enum Dialog {
    /// Asks before invoking an action that has a
    /// [`Confirmation`](server_remote_core::Confirmation).
    Confirm(Action),
    /// Asks whether to trust a certificate that failed verification.
    TrustCertificate(CertFingerprint),
    Error {
        title: String,
        message: String,
    },
}

/// What the runner reports about the session.
#[derive(Debug, Clone)]
pub enum Report {
    /// The session is live; these are the channels to it.
    Ready(Controls),
    /// An event from the running session.
    Session(SessionEvent),
    /// The target's TLS certificate failed verification.
    UntrustedCertificate(CertFingerprint),
}

#[derive(Debug, Clone)]
pub enum Event {
    /// From the runner.
    Report(Report),
    /// Trust the certificate the dialog showed, and connect again. The
    /// application also saves it when `remember` is set.
    TrustCertificate {
        fingerprint: CertFingerprint,
        remember: bool,
    },
    /// The pointer moved, in framebuffer coordinates.
    Pointer { x: f32, y: f32 },
    Button {
        button: PointerButton,
        pressed: bool,
    },
    /// An X11 keysym, which is what every protocol here takes.
    Key { keysym: u32, pressed: bool },
    /// Invoke a declarative action from [`Capabilities`]: confirm it and
    /// collect its value first if it needs either.
    Invoke(Action),
    /// Go ahead with the action awaiting confirmation.
    Confirm,
    /// Close the open dialog without doing anything.
    CloseDialog,
    /// The file picker for an action closed; `path` is `None` if cancelled.
    FilePicked {
        id: Cow<'static, str>,
        path: Option<PathBuf>,
    },
    /// Send an action by id straight away, e.g. from a chip's close button.
    SendAction(Cow<'static, str>),
    /// A link from the connector couldn't be opened.
    OpenUrlFailed { url: String, error: String },
}

/// What the front end has to do for the connection.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// Ask for a file for action `id`, then send [`Event::FilePicked`].
    PickFile {
        id: Cow<'static, str>,
        extensions: Vec<Cow<'static, str>>,
    },
}

/// Turns the connector's cumulative byte counters into rates.
#[derive(Debug, Default)]
struct TrafficMeter {
    last: Option<(Instant, Traffic)>,
    rate: Option<Rate>,
}

/// Bytes per second in each direction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rate {
    pub received: f64,
    pub sent: f64,
}

impl TrafficMeter {
    /// Samples closer together than this would make the rate jumpy, so
    /// they're skipped until enough time has passed.
    const MIN_INTERVAL: Duration = Duration::from_millis(500);

    fn sample(&mut self, at: Instant, traffic: Traffic) {
        let Some((then, previous)) = self.last else {
            self.last = Some((at, traffic));
            return;
        };
        let elapsed = at.saturating_duration_since(then);
        if elapsed < Self::MIN_INTERVAL {
            return;
        }
        let per_second =
            |now: u64, before: u64| now.saturating_sub(before) as f64 / elapsed.as_secs_f64();
        self.rate = Some(Rate {
            received: per_second(traffic.received, previous.received),
            sent: per_second(traffic.sent, previous.sent),
        });
        self.last = Some((at, traffic));
    }
}

/// A rate in bits per second, the way network rates are usually written.
pub fn format_rate(bytes_per_second: f64) -> String {
    let bits = bytes_per_second * 8.0;
    if bits < 1_000_000.0 {
        format!("{:.0} kb/s", bits / 1_000.0)
    } else {
        format!("{:.1} Mb/s", bits / 1_000_000.0)
    }
}

pub struct Connection {
    id: u64,
    name: Arc<str>,
    protocol: Protocol,
    target: Target,
    tls: TlsPolicy,
    credentials: Credentials,
    phase: Phase,
    /// Whether the session has ever connected. A failure before then is
    /// reported where the connection was started, not in its tab.
    connected_once: bool,
    capabilities: Capabilities,
    screen: Option<Screen>,
    frames: u64,
    /// Remote framebuffer size in pixels, for mapping pointer coordinates.
    size: Option<(u32, u32)>,
    controls: Option<Controls>,
    /// The connector's latest status snapshot.
    status: Status,
    traffic: TrafficMeter,
    dialog: Option<Dialog>,
}

impl Connection {
    pub fn new(
        id: u64,
        name: Arc<str>,
        protocol: Protocol,
        target: Target,
        tls: TlsPolicy,
        credentials: Credentials,
    ) -> Self {
        Self {
            id,
            name,
            protocol,
            target,
            tls,
            credentials,
            phase: Phase::Connecting,
            connected_once: false,
            capabilities: Capabilities::default(),
            screen: None,
            frames: 0,
            size: None,
            controls: None,
            status: Status::default(),
            traffic: TrafficMeter::default(),
            dialog: None,
        }
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn name(&self) -> &Arc<str> {
        &self.name
    }

    pub fn protocol(&self) -> Protocol {
        self.protocol
    }

    pub fn target(&self) -> &Target {
        &self.target
    }

    pub fn phase(&self) -> &Phase {
        &self.phase
    }

    pub fn connected_once(&self) -> bool {
        self.connected_once
    }

    pub fn capabilities(&self) -> &Capabilities {
        &self.capabilities
    }

    pub fn screen(&self) -> Option<&Screen> {
        self.screen.as_ref()
    }

    /// How many frames have arrived.
    pub fn frames(&self) -> u64 {
        self.frames
    }

    pub fn size(&self) -> Option<(u32, u32)> {
        self.size
    }

    pub fn status(&self) -> &Status {
        &self.status
    }

    pub fn traffic(&self) -> Option<Rate> {
        self.traffic.rate
    }

    pub fn dialog(&self) -> Option<&Dialog> {
        self.dialog.as_ref()
    }

    /// What [`run`] needs to connect, or `None` once the connection has
    /// ended. A change means a new connection attempt (a newly trusted
    /// certificate, say).
    pub fn params(&self) -> Option<Params> {
        if let Phase::Disconnected(_) = self.phase {
            return None;
        }
        Some(Params {
            id: self.id,
            protocol: self.protocol,
            target: self.target.clone(),
            tls: self.tls.clone(),
            credentials: self.credentials.clone(),
        })
    }

    pub fn update(&mut self, event: Event) -> Option<Effect> {
        match event {
            Event::Report(report) => self.report(report),
            Event::TrustCertificate { fingerprint, .. } => {
                self.dialog = None;
                // A new policy changes `params`, so this starts a fresh
                // connection attempt.
                self.tls = TlsPolicy::AllowInsecure {
                    accepted: Some(fingerprint),
                };
                self.phase = Phase::Connecting;
            }
            Event::Pointer { x, y } => self.send_pointer(x, y),
            Event::Button { button, pressed } => {
                self.send_input(InputEvent::PointerButton { button, pressed });
            }
            Event::Key { keysym, pressed } => self.send_input(InputEvent::Key { keysym, pressed }),
            Event::Invoke(action) => return self.invoke(action),
            Event::Confirm => {
                if let Some(Dialog::Confirm(action)) = self.dialog.take() {
                    return self.collect_and_send(action);
                }
            }
            Event::CloseDialog => self.dialog = None,
            Event::FilePicked { id, path } => {
                if let Some(path) = path {
                    self.send_action(id, Some(ActionValue::File(path)));
                }
            }
            Event::SendAction(id) => self.send_action(id, None),
            Event::OpenUrlFailed { url, error } => {
                self.dialog = Some(Dialog::Error {
                    title: fl!("error-open-link"),
                    message: format!("{url}: {error}"),
                });
            }
        }
        None
    }

    fn report(&mut self, report: Report) {
        match report {
            Report::Ready(controls) => self.controls = Some(controls),
            Report::Session(event) => self.apply(event),
            Report::UntrustedCertificate(fingerprint) => {
                self.phase = Phase::Disconnected(Some(fl!("certificate-not-trusted")));
                self.dialog = Some(Dialog::TrustCertificate(fingerprint));
            }
        }
    }

    fn apply(&mut self, event: SessionEvent) {
        match event {
            SessionEvent::Connected { size, capabilities } => {
                self.connected_once = true;
                self.size = Some(size);
                self.capabilities = capabilities;
                self.phase = Phase::Connected;
            }
            SessionEvent::Frame(frame) => {
                self.size = Some((frame.width, frame.height));
                self.screen = Some(Screen {
                    generation: self.frames,
                    width: frame.width,
                    height: frame.height,
                    pixels: Arc::from(frame.pixels),
                });
                self.frames += 1;
            }
            SessionEvent::Resized { width, height } => self.size = Some((width, height)),
            SessionEvent::Capabilities(capabilities) => self.capabilities = capabilities,
            SessionEvent::Status(status) => {
                if let Some(traffic) = status.traffic {
                    self.traffic.sample(Instant::now(), traffic);
                }
                self.status = status;
            }
            SessionEvent::Disconnected { reason } => {
                self.phase = Phase::Disconnected(reason);
                self.controls = None;
            }
        }
    }

    /// Invoking an action: confirm it first if asked to, then collect its
    /// value and send it.
    fn invoke(&mut self, action: Action) -> Option<Effect> {
        if !action.enabled {
            return None;
        }
        if action.confirm.is_some() {
            self.dialog = Some(Dialog::Confirm(action));
            return None;
        }
        self.collect_and_send(action)
    }

    fn collect_and_send(&mut self, action: Action) -> Option<Effect> {
        match action.control {
            Control::Button => {
                self.send_action(action.id, None);
                None
            }
            Control::File { extensions } => Some(Effect::PickFile {
                id: action.id,
                extensions,
            }),
        }
    }

    fn send_action(&self, id: Cow<'static, str>, value: Option<ActionValue>) {
        if let Some(controls) = &self.controls {
            let _ = controls
                .commands
                .try_send(SessionCommand::Action { id, value });
        }
    }

    fn send_input(&self, event: InputEvent) {
        if let Some(controls) = &self.controls {
            let _ = controls.input.try_send(event);
        }
    }

    fn send_pointer(&self, x: f32, y: f32) {
        match self.capabilities.pointer {
            PointerMode::Absolute => self.send_input(InputEvent::PointerMoved { x, y }),
            // TODO: grab the pointer and send PointerMotion deltas.
            PointerMode::Relative => {}
        }
    }
}

/// Everything [`run`] needs, hashed by front ends to identify the session.
#[derive(Debug, Clone, Hash)]
pub struct Params {
    id: u64,
    protocol: Protocol,
    target: Target,
    tls: TlsPolicy,
    credentials: Credentials,
}

/// Connects and reports on the session until it ends or `output` closes.
/// Dropping the future tears the session down.
pub async fn run(params: Params, output: mpsc::Sender<Report>) {
    let Params {
        protocol,
        target,
        tls,
        credentials: Credentials { username, password },
        ..
    } = params;
    let password = (!password.is_empty()).then_some(password);
    let result = match protocol {
        Protocol::Dummy => DummyConnector.connect(&target, &tls).await,
        Protocol::Vnc => VncConnector::new(password).connect(&target, &tls).await,
        Protocol::Spice => SpiceConnector::new(password).connect(&target, &tls).await,
        Protocol::Ilo => {
            IloConnector::new(username, password.unwrap_or_default())
                .connect(&target, &tls)
                .await
        }
    };

    let handle = match result {
        Ok(handle) => handle,
        Err(ConnectError::UntrustedCertificate(fingerprint)) => {
            let _ = output.send(Report::UntrustedCertificate(fingerprint)).await;
            return;
        }
        Err(error) => {
            let _ = output
                .send(Report::Session(SessionEvent::Disconnected {
                    reason: Some(error.to_string()),
                }))
                .await;
            return;
        }
    };

    let SessionHandle {
        mut events,
        input,
        commands,
    } = handle;
    if output
        .send(Report::Ready(Controls { input, commands }))
        .await
        .is_err()
    {
        return;
    }
    while let Some(event) = events.recv().await {
        let ended = matches!(event, SessionEvent::Disconnected { .. });
        if output.send(Report::Session(event)).await.is_err() || ended {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use server_remote_core::Frame;

    fn connection() -> Connection {
        Connection::new(
            0,
            Arc::from("test"),
            Protocol::Dummy,
            Target::new("localhost", 0),
            TlsPolicy::default(),
            Credentials::default(),
        )
    }

    fn session(event: SessionEvent) -> Event {
        Event::Report(Report::Session(event))
    }

    fn wired_controls() -> (
        Controls,
        mpsc::Receiver<InputEvent>,
        mpsc::Receiver<SessionCommand>,
    ) {
        let (input, input_rx) = mpsc::channel(4);
        let (commands, command_rx) = mpsc::channel(4);
        (Controls { input, commands }, input_rx, command_rx)
    }

    /// A connection that's ready, with the receiving ends of its channels.
    fn wired() -> (
        Connection,
        mpsc::Receiver<InputEvent>,
        mpsc::Receiver<SessionCommand>,
    ) {
        let mut connection = connection();
        let (controls, input_rx, command_rx) = wired_controls();
        connection.update(Event::Report(Report::Ready(controls)));
        (connection, input_rx, command_rx)
    }

    #[test]
    fn connected_event_sets_size_capabilities_and_phase() {
        let mut connection = connection();
        let capabilities = Capabilities {
            pointer: PointerMode::Relative,
            ..Capabilities::default()
        };
        connection.update(session(SessionEvent::Connected {
            size: (800, 600),
            capabilities,
        }));

        assert_eq!(connection.size(), Some((800, 600)));
        assert_eq!(connection.capabilities().pointer, PointerMode::Relative);
        assert_eq!(connection.phase(), &Phase::Connected);
        assert!(connection.connected_once());
    }

    #[test]
    fn frame_event_updates_size_and_stores_the_frame() {
        let mut connection = connection();
        for _ in 0..2 {
            connection.update(session(SessionEvent::Frame(Frame {
                width: 4,
                height: 2,
                pixels: vec![0; 4 * 2 * 4],
            })));
        }

        assert_eq!(connection.size(), Some((4, 2)));
        assert_eq!(connection.screen().map(|screen| screen.generation), Some(1));
        assert_eq!(connection.frames(), 2);
    }

    #[test]
    fn resized_event_only_updates_size() {
        let mut connection = connection();
        connection.update(session(SessionEvent::Resized {
            width: 10,
            height: 20,
        }));
        assert_eq!(connection.size(), Some((10, 20)));
        assert!(connection.screen().is_none());
    }

    #[test]
    fn disconnecting_keeps_the_reason_drops_the_controls_and_stops_the_runner() {
        let (mut connection, mut input_rx, _command_rx) = wired();

        connection.update(session(SessionEvent::Disconnected {
            reason: Some("connection reset".to_owned()),
        }));
        connection.update(Event::Key {
            keysym: 0x61,
            pressed: true,
        });

        assert_eq!(
            connection.phase(),
            &Phase::Disconnected(Some("connection reset".to_owned()))
        );
        assert!(input_rx.try_recv().is_err());
        assert!(connection.params().is_none());
    }

    #[test]
    fn input_goes_to_the_session() {
        let cases = [
            (
                "button",
                Event::Button {
                    button: PointerButton::Right,
                    pressed: true,
                },
                InputEvent::PointerButton {
                    button: PointerButton::Right,
                    pressed: true,
                },
            ),
            (
                "key",
                Event::Key {
                    keysym: 0xFF1B,
                    pressed: true,
                },
                InputEvent::Key {
                    keysym: 0xFF1B,
                    pressed: true,
                },
            ),
            (
                "absolute pointer",
                Event::Pointer { x: 3.0, y: 4.0 },
                InputEvent::PointerMoved { x: 3.0, y: 4.0 },
            ),
        ];
        for (name, event, expected) in cases {
            let (mut connection, mut input_rx, _command_rx) = wired();
            connection.update(event);
            // `InputEvent` has no `PartialEq`, as it carries floats.
            let received = input_rx.try_recv().map(|event| format!("{event:?}"));
            assert_eq!(received, Ok(format!("{expected:?}")), "{name}");
        }
    }

    #[test]
    fn pointer_is_dropped_in_relative_mode() {
        let (mut connection, mut input_rx, _command_rx) = wired();
        connection.capabilities.pointer = PointerMode::Relative;

        connection.update(Event::Pointer { x: 3.0, y: 4.0 });

        assert!(input_rx.try_recv().is_err());
    }

    fn confirmed(action: Action) -> Action {
        action.confirm(server_remote_core::Confirmation {
            title: Cow::Borrowed("Sure?"),
            message: Cow::Borrowed("Really?"),
            confirm_label: Cow::Borrowed("Yes"),
            destructive: false,
        })
    }

    /// Actions sent to the session, as `(id, value)`.
    type Sent = Vec<(String, Option<ActionValue>)>;

    fn sent(command_rx: &mut mpsc::Receiver<SessionCommand>) -> Sent {
        std::iter::from_fn(|| command_rx.try_recv().ok())
            .map(|SessionCommand::Action { id, value }| (id.into_owned(), value))
            .collect()
    }

    #[test]
    fn action_flows() {
        let iso = PathBuf::from("/images/disk.iso");
        let cases: Vec<(&str, Vec<Event>, Sent)> = vec![
            (
                "button is sent straight away",
                vec![Event::Invoke(Action::button("cad", "Ctrl+Alt+Del"))],
                vec![("cad".into(), None)],
            ),
            (
                "disabled action is ignored",
                vec![Event::Invoke(
                    Action::button("cad", "Ctrl+Alt+Del").enabled(false),
                )],
                vec![],
            ),
            (
                "confirmation holds the action back",
                vec![Event::Invoke(confirmed(Action::button("off", "Power off")))],
                vec![],
            ),
            (
                "confirming sends it",
                vec![
                    Event::Invoke(confirmed(Action::button("off", "Power off"))),
                    Event::Confirm,
                ],
                vec![("off".into(), None)],
            ),
            (
                "cancelling drops it",
                vec![
                    Event::Invoke(confirmed(Action::button("off", "Power off"))),
                    Event::CloseDialog,
                    Event::Confirm,
                ],
                vec![],
            ),
            (
                "picked file is sent as the value",
                vec![Event::FilePicked {
                    id: Cow::Borrowed("mount"),
                    path: Some(iso.clone()),
                }],
                vec![("mount".into(), Some(ActionValue::File(iso.clone())))],
            ),
            (
                "cancelled file picker sends nothing",
                vec![Event::FilePicked {
                    id: Cow::Borrowed("mount"),
                    path: None,
                }],
                vec![],
            ),
            (
                "chip close sends its action",
                vec![Event::SendAction(Cow::Borrowed("eject"))],
                vec![("eject".into(), None)],
            ),
        ];

        for (name, events, expected) in cases {
            let (mut connection, _input_rx, mut command_rx) = wired();
            for event in events {
                connection.update(event);
            }
            assert_eq!(sent(&mut command_rx), expected, "{name}");
        }
    }

    #[test]
    fn a_file_action_asks_for_a_file() {
        let mut connection = connection();
        let action = Action {
            control: Control::File {
                extensions: vec![Cow::Borrowed("iso")],
            },
            ..Action::button("mount", "Mount")
        };
        assert_eq!(
            connection.update(Event::Invoke(action)),
            Some(Effect::PickFile {
                id: Cow::Borrowed("mount"),
                extensions: vec![Cow::Borrowed("iso")],
            })
        );
    }

    #[test]
    fn invoking_a_confirmed_action_opens_the_dialog() {
        let mut connection = connection();
        connection.update(Event::Invoke(confirmed(Action::button("off", "Power off"))));
        assert!(matches!(connection.dialog(), Some(Dialog::Confirm(_))));
    }

    #[test]
    fn an_untrusted_certificate_asks_and_trusting_it_reconnects() {
        let mut connection = connection();
        let fingerprint = CertFingerprint([7; 32]);
        connection.update(Event::Report(Report::UntrustedCertificate(fingerprint)));
        assert!(matches!(connection.phase(), Phase::Disconnected(Some(_))));
        assert!(
            matches!(connection.dialog(), Some(Dialog::TrustCertificate(f)) if *f == fingerprint)
        );

        connection.update(Event::TrustCertificate {
            fingerprint,
            remember: false,
        });
        assert!(connection.dialog().is_none());
        assert_eq!(connection.phase(), &Phase::Connecting);
        assert_eq!(
            connection.tls,
            TlsPolicy::AllowInsecure {
                accepted: Some(fingerprint)
            }
        );
    }

    #[test]
    fn a_link_that_wont_open_is_reported() {
        let mut connection = connection();
        connection.update(Event::OpenUrlFailed {
            url: "https://ilo.invalid".to_owned(),
            error: "no browser".to_owned(),
        });
        assert!(matches!(
            connection.dialog(),
            Some(Dialog::Error { message, .. }) if message == "https://ilo.invalid: no browser"
        ));
    }

    #[test]
    fn status_event_is_stored() {
        let mut connection = connection();
        connection.update(session(SessionEvent::Status(Status {
            encryption: Some(Cow::Borrowed("RC4")),
            ..Status::default()
        })));
        assert_eq!(connection.status().encryption.as_deref(), Some("RC4"));
    }

    #[test]
    fn traffic_rates() {
        let start = Instant::now();
        let at = |millis| start + Duration::from_millis(millis);
        let traffic = |received, sent| Traffic { received, sent };
        let cases = [
            ("one sample has no rate", vec![(0, traffic(0, 0))], None),
            (
                "rate over one second",
                vec![(0, traffic(0, 0)), (1_000, traffic(1_000, 500))],
                Some(Rate {
                    received: 1_000.0,
                    sent: 500.0,
                }),
            ),
            (
                "too-close samples are skipped",
                vec![(0, traffic(0, 0)), (100, traffic(900, 900))],
                None,
            ),
            (
                "rate measured from the last kept sample",
                vec![
                    (0, traffic(0, 0)),
                    (100, traffic(900, 900)),
                    (2_000, traffic(4_000, 2_000)),
                ],
                Some(Rate {
                    received: 2_000.0,
                    sent: 1_000.0,
                }),
            ),
        ];

        for (name, samples, expected) in cases {
            let mut meter = TrafficMeter::default();
            for (millis, traffic) in samples {
                meter.sample(at(millis), traffic);
            }
            assert_eq!(meter.rate, expected, "{name}");
        }
    }

    #[test]
    fn rate_formatting() {
        let cases = [
            (0.0, "0 kb/s"),
            (18_750.0, "150 kb/s"),
            (124_999.0, "1000 kb/s"),
            (125_000.0, "1.0 Mb/s"),
            (1_500_000.0, "12.0 Mb/s"),
        ];
        for (bytes_per_second, expected) in cases {
            assert_eq!(
                format_rate(bytes_per_second),
                expected,
                "{bytes_per_second}"
            );
        }
    }
}
