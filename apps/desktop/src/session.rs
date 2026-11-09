//! Session: the desktop app behind one handle, whichever front end hosts it.
//!
//! A session holds the [`Settings`], the [`Receiver`] while connected, and why the last connect
//! failed. Front ends change it only through [`Command`]s and read it only through a
//! [`Snapshot`], so the window, `setu-cli play` and requests over [`crate::control`] all behave
//! the same way.
//!
//! - Every command that changes a setting saves it to the session's [`Store`], when it has one.
//! - Changing the output while connected reopens the receiver with it, since the device is
//!   chosen when the receiver starts. The old receiver stops first, so an exclusive device is
//!   free again.
//! - A failed connect leaves the session disconnected, with the reason in the snapshot until
//!   the next connect or disconnect.

use crate::output::OutputOptions;
use crate::settings::{Settings, Store};
use crate::view::{self, Summary};
use crate::{Config, Receiver, Status, resolve_headset};

/// A running receiver, as a session sees it.
pub trait Running {
    fn status(&self) -> Status;
}

impl Running for Receiver {
    fn status(&self) -> Status {
        Receiver::status(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Connect to this headset (`host` or `host:port`), or to the saved one when `None`.
    /// Reconnects when already connected.
    Connect(Option<String>),
    Disconnect,
    /// Play on the device whose name contains this, ignoring case; `None` follows the system
    /// default.
    Device(Option<String>),
    /// Bypass the system mixer for the lowest latency. The device must be idle.
    Exclusive(bool),
}

/// Everything a front end shows about a session.
#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    pub settings: Settings,
    pub connected: bool,
    /// Why the last connect failed.
    pub error: Option<String>,
    pub summary: Summary,
}

type Start<R> = Box<dyn FnMut(Config) -> Result<R, String>>;

pub struct Session<R = Receiver> {
    settings: Settings,
    store: Option<Store>,
    start: Start<R>,
    receiver: Option<R>,
    error: Option<String>,
}

impl Session<Receiver> {
    /// A session that plays through [`Receiver`]s and saves its settings to `store`.
    pub fn new(settings: Settings, store: Option<Store>) -> Self {
        Session::with_start(settings, store, |config| {
            Receiver::start(config).map_err(|e| e.to_string())
        })
    }
}

impl<R: Running> Session<R> {
    /// A session that starts its receivers with `start`, which gets the [`Config`] made from the
    /// settings and may adjust it.
    pub fn with_start(
        settings: Settings,
        store: Option<Store>,
        start: impl FnMut(Config) -> Result<R, String> + 'static,
    ) -> Self {
        Session {
            settings,
            store,
            start: Box::new(start),
            receiver: None,
            error: None,
        }
    }

    pub fn apply(&mut self, command: Command) -> Result<(), String> {
        match command {
            Command::Connect(headset) => {
                if let Some(headset) = headset {
                    self.settings.headset = headset.trim().into();
                    self.save();
                }
                self.receiver = None;
                self.connect()
            }
            Command::Disconnect => {
                self.receiver = None;
                self.error = None;
                Ok(())
            }
            Command::Device(device) => {
                self.settings.device = device.filter(|name| !name.trim().is_empty());
                self.output_changed()
            }
            Command::Exclusive(exclusive) => {
                self.settings.exclusive = exclusive;
                self.output_changed()
            }
        }
    }

    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            settings: self.settings.clone(),
            connected: self.receiver.is_some(),
            error: self.error.clone(),
            summary: view::summarize(self.status().as_ref(), self.error.as_deref()),
        }
    }

    /// The receiver's full status while connected, for front ends that show more than the
    /// summary.
    pub fn status(&self) -> Option<Status> {
        self.receiver.as_ref().map(Running::status)
    }

    fn output_changed(&mut self) -> Result<(), String> {
        self.save();
        let was_connected = self.receiver.take().is_some();
        if was_connected {
            self.connect()
        } else {
            Ok(())
        }
    }

    fn connect(&mut self) -> Result<(), String> {
        let started = resolve_headset(&self.settings.headset).and_then(|headset| {
            let mut config = Config::new(headset);
            config.output = OutputOptions {
                device: self.settings.device.clone(),
                exclusive: self.settings.exclusive,
                ..OutputOptions::default()
            };
            (self.start)(config)
        });
        match started {
            Ok(receiver) => {
                self.receiver = Some(receiver);
                self.error = None;
                Ok(())
            }
            Err(e) => {
                self.error = Some(e.clone());
                Err(e)
            }
        }
    }

    fn save(&self) {
        if let Some(store) = &self.store
            && let Err(e) = store.save(&self.settings)
        {
            eprintln!("could not save settings: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;
    use crate::Connection;
    use crate::output::{OutputFormat, OutputInfo, OutputState};
    use crate::playout::{PlayoutState, PlayoutStats};
    use crate::settings::TempDir;
    use crate::view::Phase;

    /// A receiver that is always waiting for the headset.
    struct Fake;

    impl Running for Fake {
        fn status(&self) -> Status {
            Status {
                connection: Connection::Waiting,
                output: OutputState::Playing(OutputInfo {
                    device: "Speakers".into(),
                    exclusive: false,
                    format: OutputFormat {
                        sample_rate: 48_000,
                        channels: 2,
                        period_frames: 480,
                    },
                    encoding: "float32".into(),
                    buffer_ms: 20.0,
                    queued_ms: 10.0,
                    engine_latency_ms: 0.0,
                }),
                playout: PlayoutStats {
                    state: PlayoutState::Idle,
                    buffer_min_ms: 0.0,
                    buffer_max_ms: 0.0,
                    margin_ms: 0.0,
                    rate_adjust: 0.0,
                    peak: 0.0,
                    underruns: 0,
                    lost_ms: 0.0,
                    skipped_ms: 0.0,
                },
                latency_ms: None,
            }
        }
    }

    /// A session of fakes, and the configs it started them with.
    fn session(
        settings: Settings,
        store: Option<Store>,
    ) -> (Session<Fake>, Rc<RefCell<Vec<Config>>>) {
        let started = Rc::new(RefCell::new(Vec::new()));
        let session = Session::with_start(settings, store, {
            let started = started.clone();
            move |config| {
                started.borrow_mut().push(config);
                Ok(Fake)
            }
        });
        (session, started)
    }

    fn saved_headset() -> Settings {
        Settings {
            headset: "127.0.0.1".into(),
            ..Settings::default()
        }
    }

    #[test]
    fn starts_disconnected() {
        let (session, _) = session(saved_headset(), None);
        let snapshot = session.snapshot();
        assert!(!snapshot.connected);
        assert_eq!(snapshot.summary.phase, Phase::Stopped);
        assert_eq!(session.status(), None);
    }

    #[test]
    fn connects_to_the_saved_headset_on_the_default_port() {
        let (mut session, started) = session(saved_headset(), None);
        session.apply(Command::Connect(None)).unwrap();
        assert_eq!(
            started.borrow()[0].headset,
            "127.0.0.1:27184".parse().unwrap()
        );
        let snapshot = session.snapshot();
        assert!(snapshot.connected);
        assert_eq!(snapshot.summary.phase, Phase::Waiting);
    }

    #[test]
    fn connecting_to_a_new_headset_remembers_it() {
        let dir = TempDir::new("session-headset");
        let store = Store::at(dir.0.join("settings.txt"));
        let (mut session, started) = session(Settings::default(), Some(store.clone()));
        session
            .apply(Command::Connect(Some(" 127.0.0.2:9000 ".into())))
            .unwrap();
        assert_eq!(
            started.borrow()[0].headset,
            "127.0.0.2:9000".parse().unwrap()
        );
        assert_eq!(store.load().headset, "127.0.0.2:9000");
    }

    #[test]
    fn a_bad_address_leaves_it_disconnected_with_the_reason() {
        let (mut session, started) = session(Settings::default(), None);
        let error = session.apply(Command::Connect(None)).unwrap_err();
        assert_eq!(error, "enter the headset's address");
        let snapshot = session.snapshot();
        assert!(!snapshot.connected);
        assert_eq!(
            snapshot.error.as_deref(),
            Some("enter the headset's address")
        );
        assert_eq!(snapshot.summary.phase, Phase::Failed);
        assert!(started.borrow().is_empty());
    }

    #[test]
    fn a_failed_start_is_reported() {
        let mut session: Session<Fake> =
            Session::with_start(saved_headset(), None, |_| Err("device busy".into()));
        assert_eq!(
            session.apply(Command::Connect(None)),
            Err("device busy".into())
        );
        assert_eq!(session.snapshot().error.as_deref(), Some("device busy"));
    }

    #[test]
    fn disconnecting_clears_the_error() {
        let (mut session, _) = session(Settings::default(), None);
        let _ = session.apply(Command::Connect(None));
        session.apply(Command::Disconnect).unwrap();
        assert_eq!(session.snapshot().error, None);
    }

    #[test]
    fn changing_the_output_while_connected_reopens_with_it() {
        let (mut session, started) = session(saved_headset(), None);
        session.apply(Command::Connect(None)).unwrap();
        session
            .apply(Command::Device(Some("Speakers".into())))
            .unwrap();
        session.apply(Command::Exclusive(true)).unwrap();
        let started = started.borrow();
        assert_eq!(started.len(), 3);
        assert_eq!(started[2].output.device.as_deref(), Some("Speakers"));
        assert!(started[2].output.exclusive);
        assert!(session.snapshot().connected);
    }

    #[test]
    fn changing_the_output_while_disconnected_only_remembers_it() {
        let dir = TempDir::new("session-output");
        let store = Store::at(dir.0.join("settings.txt"));
        let (mut session, started) = session(saved_headset(), Some(store.clone()));
        session
            .apply(Command::Device(Some("Speakers".into())))
            .unwrap();
        session.apply(Command::Exclusive(true)).unwrap();
        assert!(started.borrow().is_empty());
        let saved = store.load();
        assert_eq!(saved.device.as_deref(), Some("Speakers"));
        assert!(saved.exclusive);
    }

    #[test]
    fn a_blank_device_follows_the_default() {
        let (mut session, _) = session(saved_headset(), None);
        session.apply(Command::Device(Some("  ".into()))).unwrap();
        assert_eq!(session.snapshot().settings.device, None);
    }
}
