//! The application as a whole: which page is shown, the open connections,
//! saved sessions and settings. Front ends feed it [`Event`]s and carry out
//! the [`Effect`]s it returns; everything they show comes from here, apart
//! from their own view state (open menus, half-typed forms and so on).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use server_remote_core::{CertFingerprint, SessionEvent, Target};

use crate::Protocol;
use crate::connect::{Request, parse_target, saved_session_from_request};
use crate::connection::{self, Connection, Credentials, Report};
use crate::i18n::{self, fl};
use crate::persistence::sessions::{self, Sessions, thumbnail};
use crate::persistence::settings::{self, Change, KnownCertificate, Settings};
use crate::update;

/// What fills the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Page {
    Manager,
    Connect,
    Connection(u64),
    Settings,
}

/// A dialog over the whole app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dialog {
    Error {
        title: String,
        message: String,
    },
    /// A connection failed before it ever connected. `details` is for the
    /// clipboard, for bug reports.
    ConnectionFailed {
        message: String,
        details: String,
    },
}

#[derive(Debug, Clone)]
pub enum Event {
    ShowPage(Page),
    /// Open a connection from the connect form.
    Connect(Request),
    /// Save the connect form as a new session, and show the manager.
    SaveNew(Request),
    /// Open a connection to a saved session.
    OpenSaved(u64),
    /// Save a session from the manager: a new one, or the one being edited.
    SaveSession {
        editing: Option<u64>,
        request: Request,
    },
    SetGroup {
        id: u64,
        group: String,
    },
    DeleteSession(u64),
    ExportSession(u64),
    ImportSessions,
    /// The contents of the file picked for [`Effect::ImportSessions`], or why
    /// it couldn't be read.
    Imported(Result<String, String>),
    /// How [`Effect::ExportSession`] went.
    Exported(Result<(), String>),
    /// A saved session's new thumbnail, as a PNG; `None` if it couldn't be
    /// made, which keeps the old one.
    ThumbnailSaved {
        session: u64,
        png: Option<Vec<u8>>,
    },
    Connection(u64, connection::Event),
    /// Disconnect and close a connection.
    CloseConnection(u64),
    /// Refuse the certificate a connection asked about, and close it.
    BlockCertificate(u64, CertFingerprint),
    ChangeSetting(Change),
    CheckForUpdates,
    UpdateChecked(Result<update::Outcome, update::Error>),
    /// A link couldn't be opened.
    OpenUrlFailed {
        url: String,
        error: String,
    },
    CloseDialog,
}

/// What the front end has to do, reporting back with the event named.
#[derive(Debug)]
pub enum Effect {
    /// Ask for a file for a connection's action, then send
    /// [`connection::Event::FilePicked`] to it.
    PickFile {
        connection: u64,
        action: std::borrow::Cow<'static, str>,
        extensions: Vec<std::borrow::Cow<'static, str>>,
    },
    /// Ask where to save `contents`, suggesting `file_name`, and write it
    /// there; then send [`Event::Exported`] (unless cancelled).
    ExportSession { file_name: String, contents: String },
    /// Ask for a file of exported sessions and read it; then send
    /// [`Event::Imported`] (unless cancelled).
    ImportSessions,
    /// Run [`update::check`]; then send [`Event::UpdateChecked`].
    CheckForUpdates,
    /// Run [`ThumbnailSource::save`] off the UI thread; then send
    /// [`Event::ThumbnailSaved`].
    SaveThumbnail(ThumbnailSource),
    /// A saved session's thumbnail changed: show this PNG, or none.
    Thumbnail { session: u64, png: Option<Vec<u8>> },
}

/// A connection's last screen, to be saved as its session's thumbnail.
#[derive(Debug)]
pub struct ThumbnailSource {
    dir: PathBuf,
    session: u64,
    width: u32,
    height: u32,
    pixels: Arc<[u8]>,
}

impl ThumbnailSource {
    pub fn session(&self) -> u64 {
        self.session
    }

    /// Encodes and writes the thumbnail, returning the PNG. Slow enough to
    /// keep off the UI thread.
    pub fn save(self) -> Option<Vec<u8>> {
        let png = thumbnail::encode(self.width, self.height, &self.pixels)?;
        match thumbnail::save(&self.dir, self.session, &png) {
            Ok(()) => Some(png),
            Err(error) => {
                eprintln!("failed to save thumbnail: {error}");
                None
            }
        }
    }
}

pub struct App {
    connections: Vec<Connection>,
    /// The page each connection was opened from, returned to when it closes.
    origins: HashMap<u64, Page>,
    /// The saved session each connection was opened from, if any, whose
    /// thumbnail it updates when it closes.
    saved_origins: HashMap<u64, u64>,
    next_id: u64,
    page: Page,
    sessions: Sessions,
    settings: Settings,
    /// Where settings and sessions are saved; `None` keeps them in memory.
    config_dir: Option<PathBuf>,
    update: update::Status,
    dialog: Option<Dialog>,
}

impl App {
    /// Starts the app with `settings`, loading saved sessions from
    /// `config_dir`. Tests pass `None` so nothing touches the user's files.
    pub fn new(config_dir: Option<PathBuf>, settings: Settings) -> (Self, Vec<Effect>) {
        i18n::select(settings.general.language);
        let check_on_launch = update::ENABLED && settings.updates.check_on_launch;
        let mut app = Self {
            connections: Vec::new(),
            origins: HashMap::new(),
            saved_origins: HashMap::new(),
            next_id: 0,
            page: Page::Connect,
            sessions: Sessions::load(config_dir.as_deref()),
            settings,
            config_dir,
            update: update::Status::NotChecked,
            dialog: None,
        };
        let effects = if check_on_launch {
            app.check_for_updates()
        } else {
            Vec::new()
        };
        (app, effects)
    }

    pub fn page(&self) -> Page {
        self.page
    }

    pub fn connections(&self) -> &[Connection] {
        &self.connections
    }

    pub fn connection(&self, id: u64) -> Option<&Connection> {
        self.connections
            .iter()
            .find(|connection| connection.id() == id)
    }

    fn connection_mut(&mut self, id: u64) -> Option<&mut Connection> {
        self.connections
            .iter_mut()
            .find(|connection| connection.id() == id)
    }

    pub fn sessions(&self) -> &Sessions {
        &self.sessions
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    pub fn config_dir(&self) -> Option<&Path> {
        self.config_dir.as_deref()
    }

    pub fn update_status(&self) -> &update::Status {
        &self.update
    }

    pub fn dialog(&self) -> Option<&Dialog> {
        self.dialog.as_ref()
    }

    pub fn update(&mut self, event: Event) -> Vec<Effect> {
        match event {
            Event::ShowPage(page) => self.page = page,
            Event::Connect(request) => self.open(request, false),
            Event::SaveNew(request) => {
                self.sessions.add(saved_session_from_request(request));
                self.save_sessions();
                self.page = Page::Manager;
            }
            Event::OpenSaved(id) => self.open_saved(id),
            Event::SaveSession { editing, request } => {
                return self.save_session(editing, request);
            }
            Event::SetGroup { id, group } => {
                if let Some(session) = self.sessions.saved.iter_mut().find(|s| s.id == id) {
                    session.group = group;
                    self.save_sessions();
                }
            }
            Event::DeleteSession(id) => {
                if self.sessions.remove(id).is_some() {
                    self.save_sessions();
                    return self.forget_thumbnail(id);
                }
            }
            Event::ExportSession(id) => return self.export_session(id),
            Event::ImportSessions => return vec![Effect::ImportSessions],
            Event::Imported(result) => self.import_sessions(result),
            Event::Exported(Ok(())) => {}
            Event::Exported(Err(message)) => {
                self.dialog = Some(Dialog::Error {
                    title: fl!("manager-export-failed"),
                    message,
                });
            }
            Event::ThumbnailSaved { session, png } => {
                if self.sessions.get(session).is_none() {
                    // Deleted while the thumbnail was being written.
                    if let Some(dir) = &self.config_dir {
                        thumbnail::remove(dir, session);
                    }
                } else if png.is_some() {
                    return vec![Effect::Thumbnail { session, png }];
                }
            }
            Event::Connection(id, event) => return self.connection_event(id, event),
            Event::CloseConnection(id) => return self.close_connection(id),
            Event::BlockCertificate(id, fingerprint) => {
                if let Some(target) = self.connection(id).map(|c| c.target().clone()) {
                    self.change_setting(Change::BlockCertificate(KnownCertificate::new(
                        &target,
                        fingerprint,
                    )));
                }
                return self.close_connection(id);
            }
            Event::ChangeSetting(change) => self.change_setting(change),
            Event::CheckForUpdates => return self.check_for_updates(),
            Event::UpdateChecked(result) => self.update = update::Status::Checked(result),
            Event::OpenUrlFailed { url, error } => {
                self.dialog = Some(Dialog::Error {
                    title: fl!("error-open-link"),
                    message: format!("{url}: {error}"),
                });
            }
            Event::CloseDialog => self.dialog = None,
        }
        Vec::new()
    }

    /// Saves every open connection's thumbnail straight away, for when the
    /// app is about to quit and there's no time left for effects.
    pub fn save_thumbnails_now(&self) {
        self.connections
            .iter()
            .filter_map(|connection| self.thumbnail_source(connection.id()))
            .for_each(|source| {
                source.save();
            });
    }

    fn check_for_updates(&mut self) -> Vec<Effect> {
        self.update = update::Status::Checking;
        vec![Effect::CheckForUpdates]
    }

    fn change_setting(&mut self, change: Change) {
        if let Change::Language(language) = change {
            i18n::select(language);
        }
        settings::apply(&mut self.settings, change);
        self.settings.save(self.config_dir.as_deref());
    }

    fn save_sessions(&self) {
        self.sessions.save(self.config_dir.as_deref());
    }

    fn save_session(&mut self, editing: Option<u64>, request: Request) -> Vec<Effect> {
        let session = saved_session_from_request(request);
        let Some(id) = editing else {
            self.sessions.add(session);
            self.save_sessions();
            return Vec::new();
        };
        // A different machine's last screen would mislead.
        let moved = self.sessions.get(id).is_some_and(|old| {
            old.protocol != session.protocol || old.hostname != session.hostname
        });
        self.sessions.replace(id, session);
        self.save_sessions();
        if moved {
            self.forget_thumbnail(id)
        } else {
            Vec::new()
        }
    }

    fn forget_thumbnail(&self, id: u64) -> Vec<Effect> {
        if let Some(dir) = &self.config_dir {
            thumbnail::remove(dir, id);
        }
        vec![Effect::Thumbnail {
            session: id,
            png: None,
        }]
    }

    fn export_session(&mut self, id: u64) -> Vec<Effect> {
        let Some(session) = self.sessions.get(id) else {
            return Vec::new();
        };
        match sessions::export(session) {
            Ok(contents) => vec![Effect::ExportSession {
                file_name: sessions::export_file_name(&session.name),
                contents,
            }],
            Err(error) => {
                self.dialog = Some(Dialog::Error {
                    title: fl!("manager-export-failed"),
                    message: error.to_string(),
                });
                Vec::new()
            }
        }
    }

    fn import_sessions(&mut self, contents: Result<String, String>) {
        let imported = contents
            .and_then(|contents| sessions::import(&contents).map_err(|error| error.to_string()));
        match imported {
            Ok(imported) => {
                for session in imported {
                    self.sessions.add(session);
                }
                self.save_sessions();
            }
            Err(message) => {
                self.dialog = Some(Dialog::Error {
                    title: fl!("manager-import-failed"),
                    message,
                });
            }
        }
    }

    /// Opens a connection to a saved session, named after it.
    fn open_saved(&mut self, session_id: u64) {
        let Some(session) = self.sessions.get(session_id) else {
            return;
        };
        let request = Request::from(session);
        let id = self.next_id;
        self.open(request, true);
        self.saved_origins.insert(id, session_id);
    }

    /// Opens a connection and shows it. It's named after the request when
    /// `named` (a saved session), otherwise after its hostname.
    fn open(&mut self, request: Request, named: bool) {
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

        self.connections.push(Connection::new(
            id,
            name,
            request.protocol,
            target,
            tls,
            Credentials {
                username: request.username,
                password: request.password,
            },
        ));
        let origin = match self.page {
            Page::Manager => Page::Manager,
            _ => Page::Connect,
        };
        self.origins.insert(id, origin);
        self.page = Page::Connection(id);
    }

    fn connection_event(&mut self, id: u64, event: connection::Event) -> Vec<Effect> {
        let Some(target) = self.connection(id).map(|c| c.target().clone()) else {
            return Vec::new();
        };
        let event = self.settle_certificate(&target, event);
        if let connection::Event::Report(Report::Session(SessionEvent::Disconnected { reason })) =
            &event
            && self.never_connected(id)
        {
            self.fail_connection(id, reason.clone());
            return Vec::new();
        }
        let Some(connection) = self.connection_mut(id) else {
            return Vec::new();
        };
        connection
            .update(event)
            .map(
                |connection::Effect::PickFile {
                     id: action,
                     extensions,
                 }| Effect::PickFile {
                    connection: id,
                    action,
                    extensions,
                },
            )
            .into_iter()
            .collect()
    }

    /// Applies the security settings to a connection's certificate events:
    /// refuses a certificate they rule out, and saves a certificate the user
    /// chose to always trust. Returns what the connection should handle.
    fn settle_certificate(
        &mut self,
        target: &Target,
        event: connection::Event,
    ) -> connection::Event {
        match &event {
            connection::Event::Report(Report::UntrustedCertificate(fingerprint)) => {
                match self.settings.security.refusal(target, *fingerprint) {
                    Some(refusal) => {
                        connection::Event::Report(Report::Session(SessionEvent::Disconnected {
                            reason: Some(refusal.reason()),
                        }))
                    }
                    None => event,
                }
            }
            connection::Event::TrustCertificate {
                fingerprint,
                remember: true,
            } => {
                self.change_setting(Change::TrustCertificate(KnownCertificate::new(
                    target,
                    *fingerprint,
                )));
                event
            }
            _ => event,
        }
    }

    fn never_connected(&self, id: u64) -> bool {
        self.connection(id)
            .is_some_and(|connection| !connection.connected_once())
    }

    /// Closes a connection that failed before connecting, reporting the
    /// failure on the page it was opened from.
    fn fail_connection(&mut self, id: u64, reason: Option<String>) {
        let Some(connection) = self.connection(id) else {
            return;
        };
        let message = reason.unwrap_or_else(|| fl!("connection-failed-unknown"));
        let details = failure_details(connection.protocol(), connection.target(), &message);
        // It never connected, so there's no screen to keep.
        let _ = self.close_connection(id);
        self.dialog = Some(Dialog::ConnectionFailed { message, details });
    }

    /// Closes a connection, saving its last screen as its saved session's
    /// thumbnail.
    fn close_connection(&mut self, id: u64) -> Vec<Effect> {
        let thumbnail = self.thumbnail_source(id).map(Effect::SaveThumbnail);
        self.connections.retain(|connection| connection.id() != id);
        self.saved_origins.remove(&id);
        let origin = self.origins.remove(&id).unwrap_or(Page::Connect);
        if self.page == Page::Connection(id) {
            self.page = origin;
        }
        thumbnail.into_iter().collect()
    }

    /// Where a connection's thumbnail goes, for which session, and the frame
    /// to make it from; `None` if it has no saved session (still) or no
    /// frame yet.
    fn thumbnail_source(&self, id: u64) -> Option<ThumbnailSource> {
        let dir = self.config_dir.clone()?;
        let session = *self.saved_origins.get(&id)?;
        self.sessions.get(session)?;
        let screen = self.connection(id)?.screen()?;
        Some(ThumbnailSource {
            dir,
            session,
            width: screen.width,
            height: screen.height,
            pixels: screen.pixels.clone(),
        })
    }
}

/// What a failed connection's copy button puts on the clipboard. Kept in
/// English whatever the UI language, as it's meant for bug reports.
fn failure_details(protocol: Protocol, target: &Target, error: &str) -> String {
    format!(
        "ServerRemote {}\nProtocol: {protocol}\nAddress: {}:{}\nError: {error}",
        env!("CARGO_PKG_VERSION"),
        target.host,
        target.port,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::Phase;
    use crate::persistence::settings::{CertificateChecking, Refusal};
    use server_remote_core::Capabilities;

    fn app() -> App {
        App::new(None, Settings::default()).0
    }

    fn request(hostname: &str) -> Request {
        Request {
            protocol: Protocol::Dummy,
            name: String::new(),
            group: String::new(),
            hostname: hostname.to_owned(),
            username: String::new(),
            password: String::new(),
        }
    }

    fn session_event(id: u64, event: SessionEvent) -> Event {
        Event::Connection(id, connection::Event::Report(Report::Session(event)))
    }

    fn ids(app: &App) -> Vec<u64> {
        app.connections().iter().map(Connection::id).collect()
    }

    #[test]
    fn connecting_assigns_increasing_ids_and_shows_the_new_connection() {
        let mut app = app();

        app.update(Event::Connect(request("first")));
        app.update(Event::Connect(request("second")));

        assert_eq!(ids(&app), vec![0, 1]);
        assert_eq!(app.page(), Page::Connection(1));
    }

    #[test]
    fn a_connection_without_a_hostname_is_named_after_its_protocol_and_id() {
        let mut app = app();
        app.update(Event::Connect(request("")));
        assert_eq!(app.connections()[0].name().as_ref(), "dummy #0");
    }

    #[test]
    fn closing_a_connection_returns_to_where_it_was_opened() {
        for origin in [Page::Connect, Page::Manager, Page::Settings] {
            let mut app = app();
            app.update(Event::ShowPage(origin));
            app.update(Event::Connect(request("only")));
            assert_eq!(app.page(), Page::Connection(0));

            app.update(Event::CloseConnection(0));

            assert!(app.connections().is_empty());
            let expected = match origin {
                Page::Manager => Page::Manager,
                _ => Page::Connect,
            };
            assert_eq!(app.page(), expected, "{origin:?}");
        }
    }

    #[test]
    fn closing_a_connection_leaves_an_unrelated_page_alone() {
        let mut app = app();
        app.update(Event::Connect(request("first")));
        app.update(Event::Connect(request("second")));
        app.update(Event::ShowPage(Page::Manager));

        app.update(Event::CloseConnection(0));

        assert_eq!(ids(&app), vec![1]);
        assert_eq!(app.page(), Page::Manager);
    }

    #[test]
    fn a_failure_before_connecting_is_reported_where_it_started() {
        for origin in [Page::Connect, Page::Manager] {
            let mut app = app();
            app.update(Event::ShowPage(origin));
            app.update(Event::Connect(request("ilo:443")));
            app.update(session_event(
                0,
                SessionEvent::Disconnected {
                    reason: Some("busy".to_owned()),
                },
            ));

            assert!(app.connections().is_empty(), "{origin:?}");
            assert_eq!(app.page(), origin);
            match app.dialog() {
                Some(Dialog::ConnectionFailed { message, details }) => {
                    assert_eq!(message, "busy");
                    assert!(details.contains("Address: ilo:443"), "{details}");
                    assert!(details.ends_with("Error: busy"), "{details}");
                }
                other => panic!("{origin:?}: {other:?}"),
            }
        }
    }

    #[test]
    fn a_failure_after_connecting_stays_on_its_page() {
        let mut app = app();
        app.update(Event::Connect(request("ilo:443")));
        app.update(session_event(
            0,
            SessionEvent::Connected {
                size: (1, 1),
                capabilities: Capabilities::default(),
            },
        ));
        app.update(session_event(
            0,
            SessionEvent::Disconnected {
                reason: Some("lost".to_owned()),
            },
        ));

        assert_eq!(ids(&app), vec![0]);
        assert_eq!(app.page(), Page::Connection(0));
        assert!(app.dialog().is_none());
        assert_eq!(
            app.connections()[0].phase(),
            &Phase::Disconnected(Some("lost".to_owned()))
        );
    }

    #[test]
    fn cancelling_or_blocking_a_certificate_closes_the_connection_quietly() {
        let fingerprint = CertFingerprint([4; 32]);
        for event in [
            Event::CloseConnection(0),
            Event::BlockCertificate(0, fingerprint),
        ] {
            let mut app = app();
            app.update(Event::ShowPage(Page::Manager));
            app.update(Event::Connect(request("ilo:443")));
            app.update(Event::Connection(
                0,
                connection::Event::Report(Report::UntrustedCertificate(fingerprint)),
            ));
            app.update(event.clone());

            assert!(app.connections().is_empty(), "{event:?}");
            assert_eq!(app.page(), Page::Manager, "{event:?}");
            assert!(app.dialog().is_none(), "{event:?}");
        }
    }

    #[test]
    fn certificate_choices_follow_and_update_the_security_settings() {
        let mut app = app();
        app.update(Event::Connect(request("ilo:443")));
        let target = app.connections()[0].target().clone();
        let trusted = CertFingerprint([1; 32]);
        let blocked = CertFingerprint([2; 32]);
        let trust = |remember| {
            Event::Connection(
                0,
                connection::Event::TrustCertificate {
                    fingerprint: trusted,
                    remember,
                },
            )
        };

        app.update(trust(false));
        assert!(app.settings().security.trusted.is_empty(), "connect once");
        app.update(trust(true));
        assert_eq!(
            app.settings().security.trusted,
            vec![KnownCertificate::new(&target, trusted)]
        );

        let untrusted =
            |fingerprint| connection::Event::Report(Report::UntrustedCertificate(fingerprint));
        let refused_with = |event| match event {
            connection::Event::Report(Report::Session(SessionEvent::Disconnected { reason })) => {
                reason
            }
            _ => None,
        };
        app.settings.security.blocked = vec![KnownCertificate::new(&target, blocked)];
        let asked = app.settle_certificate(&target, untrusted(CertFingerprint([3; 32])));
        assert!(
            matches!(
                asked,
                connection::Event::Report(Report::UntrustedCertificate(_))
            ),
            "ask: prompts"
        );
        let refused = app.settle_certificate(&target, untrusted(blocked));
        assert_eq!(refused_with(refused), Some(Refusal::Blocked.reason()));

        app.settings.security.certificate_checking = CertificateChecking::Strict;
        let refused = app.settle_certificate(&target, untrusted(CertFingerprint([3; 32])));
        assert_eq!(refused_with(refused), Some(Refusal::Strict.reason()));
    }

    #[test]
    fn blocking_a_certificate_saves_it() {
        let mut app = app();
        app.update(Event::Connect(request("ilo:443")));
        let target = app.connections()[0].target().clone();
        let blocked = CertFingerprint([2; 32]);

        app.update(Event::BlockCertificate(0, blocked));

        assert_eq!(
            app.settings().security.blocked,
            vec![KnownCertificate::new(&target, blocked)]
        );
    }

    #[test]
    fn saving_from_the_connect_form_shows_the_manager() {
        let mut app = app();
        app.update(Event::SaveNew(request("nas.local")));
        assert_eq!(app.page(), Page::Manager);
        assert_eq!(app.sessions().saved[0].name, "nas.local");
    }

    #[test]
    fn a_saved_session_opens_under_its_name() {
        let mut app = app();
        let mut named = request("nas.local");
        named.name = "Home NAS".to_owned();
        app.update(Event::SaveSession {
            editing: None,
            request: named,
        });
        let id = app.sessions().saved[0].id;

        app.update(Event::OpenSaved(id));

        assert_eq!(app.connections()[0].name().as_ref(), "Home NAS");
        assert_eq!(app.saved_origins.get(&0), Some(&id));
    }

    #[test]
    fn moving_a_session_to_another_machine_drops_its_thumbnail() {
        let cases = [
            ("renamed", "nas.local", false),
            ("new host", "other.local", true),
        ];
        for (name, hostname, dropped) in cases {
            let mut app = app();
            app.update(Event::SaveSession {
                editing: None,
                request: request("nas.local"),
            });
            let id = app.sessions().saved[0].id;
            let mut edited = request(hostname);
            edited.name = "Renamed".to_owned();

            let effects = app.update(Event::SaveSession {
                editing: Some(id),
                request: edited,
            });

            let cleared = matches!(
                effects.as_slice(),
                [Effect::Thumbnail { session, png: None }] if *session == id
            );
            assert_eq!(cleared, dropped, "{name}");
            assert_eq!(app.sessions().saved[0].name, "Renamed", "{name}");
        }
    }

    #[test]
    fn importing_adds_sessions_and_reports_bad_files() {
        let mut app = app();
        let exported = sessions::export(&saved_session_from_request(request("nas.local")))
            .expect("a session exports");

        app.update(Event::Imported(Ok(exported)));
        assert_eq!(app.sessions().saved.len(), 1);
        assert!(app.dialog().is_none());

        app.update(Event::Imported(Ok("not = [toml".to_owned())));
        assert_eq!(app.sessions().saved.len(), 1);
        assert!(matches!(app.dialog(), Some(Dialog::Error { .. })));
    }

    #[test]
    fn exporting_suggests_a_file_name() {
        let mut app = app();
        app.update(Event::SaveSession {
            editing: None,
            request: request("CON"),
        });
        let id = app.sessions().saved[0].id;

        let effects = app.update(Event::ExportSession(id));

        assert!(matches!(
            effects.as_slice(),
            [Effect::ExportSession { file_name, .. }] if file_name == "CON-connection.toml"
        ));
    }

    #[test]
    fn checking_for_updates_asks_the_front_end() {
        let mut app = app();
        let effects = app.update(Event::CheckForUpdates);
        assert!(matches!(effects.as_slice(), [Effect::CheckForUpdates]));
        assert_eq!(app.update_status(), &update::Status::Checking);
    }

    #[test]
    fn a_connections_file_picker_is_handed_to_the_front_end_with_its_id() {
        let mut app = app();
        app.update(Event::Connect(request("ilo")));
        let action = server_remote_core::Action {
            control: server_remote_core::Control::File {
                extensions: vec!["iso".into()],
            },
            ..server_remote_core::Action::button("mount", "Mount")
        };

        let effects = app.update(Event::Connection(0, connection::Event::Invoke(action)));

        assert!(matches!(
            effects.as_slice(),
            [Effect::PickFile { connection: 0, action, .. }] if action == "mount"
        ));
    }
}
