//! Bindings to the shared application core ([`server_remote_app::App`]) for
//! the iPadOS app, generated with UniFFI. Android will use the same ones.
//!
//! [`Core`] owns the app and runs its connections on a tokio runtime of its
//! own, much as the desktop front end does with iced subscriptions. The
//! front end reads [`AppState`] whenever [`Shell::changed`] fires, and
//! handles the few things only it can do ([`ShellRequest`]).

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, Weak};

use server_remote_app::app::{self, Effect, Event};
use server_remote_app::connect::Request;
use server_remote_app::connection::{self, Connection};
use server_remote_app::persistence::sessions::thumbnail;
use server_remote_app::persistence::settings::Settings;
use server_remote_app::{App, i18n, keys, update};
use server_remote_core::{Action, MenuItem, PointerButton, ToolbarItem};
use tokio::runtime::Runtime;
use tokio::sync::mpsc;
use tokio::task::AbortHandle;

mod look;
mod records;

pub use look::*;
pub use records::*;

uniffi::setup_scaffolding!();

/// What the front end provides.
#[uniffi::export(foreign)]
pub trait Shell: Send + Sync {
    /// The state changed; read [`Core::state`] again. Called from any thread.
    fn changed(&self);
    /// Something only the front end can do. Called from any thread.
    fn perform(&self, request: ShellRequest);
}

#[derive(Debug, Clone, uniffi::Enum)]
pub enum ShellRequest {
    /// Ask for a file for a connection's action, then call
    /// [`Core::file_picked`].
    PickFile {
        connection: u64,
        action: String,
        extensions: Vec<String>,
    },
    /// Ask where to save `contents`, then call [`Core::exported`].
    ExportSession { file_name: String, contents: String },
    /// Ask for a file of exported sessions, then call [`Core::imported`].
    ImportSessions,
}

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum CoreError {
    #[error("couldn't start the background runtime: {0}")]
    Runtime(String),
}

#[derive(uniffi::Object)]
pub struct Core {
    shared: Arc<Shared>,
}

struct Shared {
    app: Mutex<App>,
    /// Saved sessions' thumbnails, as PNGs.
    thumbnails: Mutex<HashMap<u64, Vec<u8>>>,
    /// The running sessions, by connection id, with a hash of what they
    /// were started with.
    runners: Mutex<HashMap<u64, (u64, AbortHandle)>>,
    runtime: Runtime,
    shell: Arc<dyn Shell>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .expect("nothing panics while holding the core's locks")
}

#[uniffi::export]
impl Core {
    /// Starts the app, keeping settings and sessions in `config_dir` (or
    /// only in memory without one).
    #[uniffi::constructor]
    pub fn new(config_dir: Option<String>, shell: Arc<dyn Shell>) -> Result<Arc<Self>, CoreError> {
        let config_dir = config_dir.map(PathBuf::from);
        let settings = Settings::load(config_dir.as_deref());
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|error| CoreError::Runtime(error.to_string()))?;
        let (app, effects) = App::new(config_dir, settings);
        let thumbnails = app
            .config_dir()
            .map(|dir| {
                app.sessions()
                    .saved
                    .iter()
                    .filter_map(|session| {
                        thumbnail::load(dir, session.id).map(|png| (session.id, png))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let shared = Arc::new(Shared {
            app: Mutex::new(app),
            thumbnails: Mutex::new(thumbnails),
            runners: Mutex::new(HashMap::new()),
            runtime,
            shell,
        });
        Shared::perform(&shared, effects);
        Ok(Arc::new(Self { shared }))
    }

    /// Everything the front end shows, apart from screens and thumbnails.
    pub fn state(&self) -> AppState {
        AppState::from(&*lock(&self.shared.app))
    }

    /// A connection's last screen, if it has one yet.
    pub fn screen(&self, connection: u64) -> Option<Screen> {
        let app = lock(&self.shared.app);
        let screen = app.connection(connection)?.screen()?;
        Some(Screen {
            generation: screen.generation,
            width: screen.width,
            height: screen.height,
            rgba: screen.pixels.to_vec(),
        })
    }

    /// One of a connection's icons (by the id in its records), `pixels`
    /// across; `None` if it no longer has it.
    pub fn connection_icon(
        &self,
        connection: u64,
        icon: u64,
        pixels: u32,
    ) -> Result<Option<IconImage>, IconError> {
        let found = {
            let app = lock(&self.shared.app);
            app.connection(connection)
                .and_then(|connection| find_icon(connection, icon))
        };
        found.map(|found| look::render(&found, pixels)).transpose()
    }

    /// A saved session's thumbnail, as a PNG.
    pub fn thumbnail(&self, session: u64) -> Option<Vec<u8>> {
        lock(&self.shared.thumbnails).get(&session).cloned()
    }

    /// A translated message, for the front end's own text.
    pub fn localize(&self, id: String) -> String {
        i18n::LOADER.get(&id)
    }

    /// A translated message with `name = value` arguments.
    pub fn localize_args(&self, id: String, args: HashMap<String, String>) -> String {
        let args: HashMap<&str, String> = args
            .iter()
            .map(|(name, value)| (name.as_str(), value.clone()))
            .collect();
        i18n::LOADER.get_args(&id, args)
    }

    pub fn show_page(&self, page: Page) {
        self.send(Event::ShowPage(page.into()));
    }

    pub fn connect(&self, request: ConnectRequest) {
        self.send(Event::Connect(request.into()));
    }

    /// Saves the connect form as a new session, and shows the manager.
    pub fn save_new(&self, request: ConnectRequest) {
        self.send(Event::SaveNew(request.into()));
    }

    pub fn open_saved(&self, session: u64) {
        self.send(Event::OpenSaved(session));
    }

    /// Saves a new session (`editing` unset) or replaces one.
    pub fn save_session(&self, editing: Option<u64>, request: ConnectRequest) {
        self.send(Event::SaveSession {
            editing,
            request: request.into(),
        });
    }

    /// Moves a session into a group; an empty one for none.
    pub fn set_group(&self, session: u64, group: String) {
        self.send(Event::SetGroup { id: session, group });
    }

    pub fn delete_session(&self, session: u64) {
        self.send(Event::DeleteSession(session));
    }

    pub fn export_session(&self, session: u64) {
        self.send(Event::ExportSession(session));
    }

    pub fn import_sessions(&self) {
        self.send(Event::ImportSessions);
    }

    /// The picked file's contents, or why it couldn't be read.
    pub fn imported(&self, contents: Option<String>, error: Option<String>) {
        let result = match (contents, error) {
            (_, Some(error)) => Err(error),
            (Some(contents), None) => Ok(contents),
            (None, None) => return,
        };
        self.send(Event::Imported(result));
    }

    /// How an export went: `None` if it was written.
    pub fn exported(&self, error: Option<String>) {
        self.send(Event::Exported(error.map_or(Ok(()), Err)));
    }

    pub fn close_dialog(&self) {
        self.send(Event::CloseDialog);
    }

    pub fn close_connection(&self, connection: u64) {
        self.send(Event::CloseConnection(connection));
    }

    /// Trusts the certificate a connection is asking about, and connects
    /// again; for good if `remember`.
    pub fn trust_certificate(&self, connection: u64, remember: bool) {
        let Some(fingerprint) = self.asked_certificate(connection) else {
            return;
        };
        self.send_to(
            connection,
            connection::Event::TrustCertificate {
                fingerprint,
                remember,
            },
        );
    }

    /// Refuses the certificate a connection is asking about, for good, and
    /// closes it.
    pub fn block_certificate(&self, connection: u64) {
        if let Some(fingerprint) = self.asked_certificate(connection) {
            self.send(Event::BlockCertificate(connection, fingerprint));
        }
    }

    /// Invokes one of a connection's actions (from [`ConnectionState::menus`]).
    pub fn invoke(&self, connection: u64, action: String) {
        let found = {
            let app = lock(&self.shared.app);
            app.connection(connection)
                .and_then(|connection| find_action(connection, &action))
        };
        if let Some(action) = found {
            self.send_to(connection, connection::Event::Invoke(action));
        }
    }

    /// Sends an action by id straight away, as when closing a chip.
    pub fn send_action(&self, connection: u64, action: String) {
        self.send_to(connection, connection::Event::SendAction(action.into()));
    }

    /// Goes ahead with the action the connection asked to confirm.
    pub fn confirm(&self, connection: u64) {
        self.send_to(connection, connection::Event::Confirm);
    }

    pub fn close_connection_dialog(&self, connection: u64) {
        self.send_to(connection, connection::Event::CloseDialog);
    }

    /// The file picked for a connection's action; `None` if cancelled.
    pub fn file_picked(&self, connection: u64, action: String, path: Option<String>) {
        self.send_to(
            connection,
            connection::Event::FilePicked {
                id: action.into(),
                path: path.map(PathBuf::from),
            },
        );
    }

    /// The pointer moved, in screen pixels.
    pub fn pointer(&self, connection: u64, x: f32, y: f32) {
        self.send_to(connection, connection::Event::Pointer { x, y });
    }

    pub fn button(&self, connection: u64, button: MouseButton, pressed: bool) {
        let button = match button {
            MouseButton::Left => PointerButton::Left,
            MouseButton::Middle => PointerButton::Middle,
            MouseButton::Right => PointerButton::Right,
        };
        self.send_to(connection, connection::Event::Button { button, pressed });
    }

    /// A hardware key, as a USB HID usage. Returns whether it was sent.
    pub fn hid_key(&self, connection: u64, usage: u32, pressed: bool) -> bool {
        let Some(keysym) = keys::hid_keysym(usage) else {
            return false;
        };
        self.send_to(connection, connection::Event::Key { keysym, pressed });
        true
    }

    /// Typed text, from the on-screen keyboard: each character is pressed
    /// and released.
    pub fn type_text(&self, connection: u64, text: String) {
        for character in text.chars() {
            let keysym = match character {
                '\n' => 0xFF0D,
                '\u{8}' => 0xFF08,
                character => keys::character_keysym(character),
            };
            for pressed in [true, false] {
                self.send_to(connection, connection::Event::Key { keysym, pressed });
            }
        }
    }

    /// Whether this build checks for updates at all; store builds don't.
    pub fn checks_for_updates(&self) -> bool {
        update::ENABLED
    }
}

impl Core {
    fn send(&self, event: Event) {
        Shared::send(&self.shared, event);
    }

    fn send_to(&self, connection: u64, event: connection::Event) {
        self.send(Event::Connection(connection, event));
    }

    fn asked_certificate(&self, connection: u64) -> Option<server_remote_core::CertFingerprint> {
        let app = lock(&self.shared.app);
        match app.connection(connection)?.dialog()? {
            connection::Dialog::TrustCertificate(fingerprint) => Some(*fingerprint),
            _ => None,
        }
    }
}

impl Drop for Core {
    fn drop(&mut self) {
        lock(&self.shared.app).save_thumbnails_now();
        lock(&self.shared.runners)
            .drain()
            .for_each(|(_, (_, runner))| runner.abort());
    }
}

impl Shared {
    /// Hands `event` to the app, keeps sessions running to match, and
    /// carries out what the app asks for.
    fn send(shared: &Arc<Self>, event: Event) {
        let effects = {
            let mut app = lock(&shared.app);
            let effects = app.update(event);
            Self::sync_runners(shared, &app);
            effects
        };
        Self::perform(shared, effects);
        shared.shell.changed();
    }

    /// Starts a session for each live connection, restarting one whose
    /// parameters changed (a newly trusted certificate) and stopping those
    /// that ended or closed.
    fn sync_runners(shared: &Arc<Self>, app: &App) {
        let wanted: HashMap<u64, connection::Params> = app
            .connections()
            .iter()
            .filter_map(|connection| Some((connection.id(), connection.params()?)))
            .collect();
        let mut runners = lock(&shared.runners);
        runners.retain(|id, (hash, runner)| {
            let keep = wanted
                .get(id)
                .is_some_and(|params| hash_of(params) == *hash);
            if !keep {
                runner.abort();
            }
            keep
        });
        for (id, params) in wanted {
            if runners.contains_key(&id) {
                continue;
            }
            let hash = hash_of(&params);
            let runner = shared
                .runtime
                .spawn(Self::run(Arc::downgrade(shared), id, params));
            runners.insert(id, (hash, runner.abort_handle()));
        }
    }

    async fn run(shared: Weak<Self>, id: u64, params: connection::Params) {
        let (reports, mut received) = mpsc::channel(32);
        let forward = async move {
            while let Some(report) = received.recv().await {
                let Some(shared) = shared.upgrade() else {
                    break;
                };
                Self::send(
                    &shared,
                    Event::Connection(id, connection::Event::Report(report)),
                );
            }
        };
        tokio::join!(connection::run(params, reports), forward);
    }

    fn perform(shared: &Arc<Self>, effects: Vec<Effect>) {
        for effect in effects {
            match effect {
                Effect::PickFile {
                    connection,
                    action,
                    extensions,
                } => shared.shell.perform(ShellRequest::PickFile {
                    connection,
                    action: action.into_owned(),
                    extensions: extensions.into_iter().map(|e| e.into_owned()).collect(),
                }),
                Effect::ExportSession {
                    file_name,
                    contents,
                } => shared.shell.perform(ShellRequest::ExportSession {
                    file_name,
                    contents,
                }),
                Effect::ImportSessions => shared.shell.perform(ShellRequest::ImportSessions),
                Effect::CheckForUpdates => {
                    let weak = Arc::downgrade(shared);
                    shared.runtime.spawn(async move {
                        let result = update::check().await;
                        if let Some(shared) = weak.upgrade() {
                            Self::send(&shared, Event::UpdateChecked(result));
                        }
                    });
                }
                Effect::SaveThumbnail(source) => {
                    let weak = Arc::downgrade(shared);
                    let session = source.session();
                    shared.runtime.spawn_blocking(move || {
                        let png = source.save();
                        if let Some(shared) = weak.upgrade() {
                            Self::send(&shared, Event::ThumbnailSaved { session, png });
                        }
                    });
                }
                Effect::Thumbnail { session, png } => {
                    let mut thumbnails = lock(&shared.thumbnails);
                    match png {
                        Some(png) => thumbnails.insert(session, png),
                        None => thumbnails.remove(&session),
                    };
                }
            }
        }
    }
}

fn hash_of(params: &connection::Params) -> u64 {
    let mut hasher = DefaultHasher::new();
    params.hash(&mut hasher);
    hasher.finish()
}

/// The icon with `id` (see [`look::icon_id`]) among a connection's
/// capabilities and status.
fn find_icon(connection: &Connection, id: u64) -> Option<server_remote_core::Icon> {
    fn in_menu(items: &[MenuItem]) -> Vec<&server_remote_core::Icon> {
        items
            .iter()
            .flat_map(|item| match item {
                MenuItem::Action(action) => action.icon.iter().collect(),
                MenuItem::Submenu { items, .. } => in_menu(items),
                MenuItem::Separator => Vec::new(),
            })
            .collect()
    }
    let capabilities = connection.capabilities();
    let status = connection.status();
    let actions = capabilities
        .input_actions
        .iter()
        .filter_map(|action| action.icon.as_ref());
    let toolbar = capabilities.toolbar.iter().flat_map(|item| match item {
        ToolbarItem::Menu { icon, items, .. } => {
            std::iter::once(icon).chain(in_menu(items)).collect()
        }
        ToolbarItem::Link { icon, .. } => vec![icon],
    });
    let chips = status.chips.iter().filter_map(|chip| chip.icon.as_ref());
    let items = status.items.iter().filter_map(|item| match item {
        server_remote_core::StatusItem::Text { icon, .. } => icon.as_ref(),
        server_remote_core::StatusItem::Indicator { .. } => None,
    });
    actions
        .chain(toolbar)
        .chain(chips)
        .chain(items)
        .find(|icon| look::icon_id(icon) == id)
        .cloned()
}

/// The action with `id` among a connection's input actions and menus.
fn find_action(connection: &Connection, id: &str) -> Option<Action> {
    fn in_menu(items: &[MenuItem], id: &str) -> Option<Action> {
        items.iter().find_map(|item| match item {
            MenuItem::Action(action) if action.id == id => Some(action.clone()),
            MenuItem::Submenu { items, .. } => in_menu(items, id),
            _ => None,
        })
    }
    let capabilities = connection.capabilities();
    capabilities
        .input_actions
        .iter()
        .find(|action| action.id == id)
        .cloned()
        .or_else(|| {
            capabilities.toolbar.iter().find_map(|item| match item {
                ToolbarItem::Menu { items, .. } => in_menu(items, id),
                ToolbarItem::Link { .. } => None,
            })
        })
}

impl From<Page> for app::Page {
    fn from(page: Page) -> Self {
        match page {
            Page::Manager => app::Page::Manager,
            Page::Connect => app::Page::Connect,
            Page::Connection { id } => app::Page::Connection(id),
            Page::Settings => app::Page::Settings,
        }
    }
}

impl From<ConnectRequest> for Request {
    fn from(request: ConnectRequest) -> Self {
        Request {
            protocol: request.protocol.into(),
            name: request.name,
            group: request.group,
            hostname: request.hostname,
            username: request.username,
            password: request.password,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    #[derive(Default)]
    struct TestShell {
        changes: AtomicUsize,
        requests: Mutex<Vec<ShellRequest>>,
    }

    impl Shell for TestShell {
        fn changed(&self) {
            self.changes.fetch_add(1, Ordering::SeqCst);
        }

        fn perform(&self, request: ShellRequest) {
            lock(&self.requests).push(request);
        }
    }

    fn core() -> (Arc<Core>, Arc<TestShell>) {
        let shell = Arc::new(TestShell::default());
        let core = Core::new(None, shell.clone()).expect("the core starts");
        (core, shell)
    }

    fn dummy() -> ConnectRequest {
        ConnectRequest {
            protocol: ConnectionProtocol::Dummy,
            name: String::new(),
            group: String::new(),
            hostname: String::new(),
            username: String::new(),
            password: String::new(),
        }
    }

    /// Waits up to a few seconds for `done`.
    fn eventually(mut done: impl FnMut() -> bool) -> bool {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if done() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        false
    }

    #[test]
    fn a_dummy_connection_runs_until_closed() {
        let (core, shell) = core();

        core.connect(dummy());
        assert_eq!(core.state().page, Page::Connection { id: 0 });
        assert!(
            eventually(|| core.screen(0).is_some()),
            "the dummy session sends a screen"
        );
        let state = core.state();
        assert_eq!(state.connections[0].phase, ConnectionPhase::Connected);
        let screen = core.screen(0).expect("a screen arrived");
        assert_eq!(
            screen.rgba.len(),
            (screen.width * screen.height * 4) as usize
        );
        assert!(shell.changes.load(Ordering::SeqCst) > 1);

        core.close_connection(0);
        assert!(core.state().connections.is_empty());
        assert!(lock(&core.shared.runners).is_empty());
    }

    #[test]
    fn hid_keys_without_a_keysym_are_not_sent() {
        let (core, _) = core();
        core.connect(dummy());
        assert!(core.hid_key(0, 0x04, true));
        assert!(!core.hid_key(0, 0x32, true));
    }

    #[test]
    fn importing_asks_the_shell_for_a_file() {
        let (core, shell) = core();
        core.import_sessions();
        assert!(matches!(
            lock(&shell.requests).as_slice(),
            [ShellRequest::ImportSessions]
        ));
    }

    #[test]
    fn saved_sessions_appear_in_the_state() {
        let (core, _) = core();
        core.save_session(
            None,
            ConnectRequest {
                name: "Home".to_owned(),
                hostname: "nas.local".to_owned(),
                ..dummy()
            },
        );
        let sessions = core.state().sessions;
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].name, "Home");
    }
}
