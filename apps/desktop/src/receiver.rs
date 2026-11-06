//! Receiver: plays the headset's audio on the PC, behind one handle.
//!
//! [`Receiver::start`] opens the output device, then links to the headset's capture server and
//! feeds what arrives into the playout. It keeps going on its own: a server that is not running
//! yet, goes quiet or restarts is picked up again, and so is an output device that comes back.
//! [`Receiver::status`] is everything a UI shows; dropping the receiver stops it.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use crate::link::{Event, Link};
use crate::output::{self, Output, OutputFormat, OutputOptions, OutputState, Render};
use crate::playout::{Playout, PlayoutStats};
use crate::protocol::Format;

/// Default safety margin over Wi-Fi, beyond one capture block and one output period.
pub const DEFAULT_MARGIN_MS: f64 = 4.0;

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    /// The capture server on the headset.
    pub headset: SocketAddr,
    /// How this PC introduces itself to the headset.
    pub name: String,
    pub output: OutputOptions,
    pub margin_ms: f64,
}

impl Config {
    pub fn new(headset: SocketAddr) -> Self {
        Config {
            headset,
            name: default_name(),
            output: OutputOptions::default(),
            margin_ms: DEFAULT_MARGIN_MS,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Status {
    pub connection: Connection,
    pub output: OutputState,
    pub playout: PlayoutStats,
    /// Estimated delay from the headset playing a sound to this PC playing it, while streaming.
    pub latency_ms: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Connection {
    /// Saying HELLO; the server is not streaming to this PC.
    Waiting,
    Streaming(Format),
    /// The server streams a format this receiver cannot play.
    Unsupported(Format),
}

#[derive(Debug)]
pub enum Error {
    Output(output::Error),
    Link(std::io::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Output(e) => write!(f, "could not open the output: {e}"),
            Error::Link(e) => write!(f, "could not open the network socket: {e}"),
        }
    }
}

impl std::error::Error for Error {}

pub struct Receiver {
    shared: Arc<Shared>,
    // Declared before `output` so the network stops feeding first.
    _link: Link,
    output: Output,
}

struct Shared {
    playout: Mutex<Playout>,
    connection: Mutex<Connection>,
}

impl Receiver {
    pub fn start(config: Config) -> Result<Receiver, Error> {
        let shared = Arc::new(Shared {
            playout: Mutex::new(Playout::new(config.margin_ms)),
            connection: Mutex::new(Connection::Waiting),
        });
        let output =
            Output::start(config.output, PlayoutRender(shared.clone())).map_err(Error::Output)?;
        let link = Link::start(config.headset, &config.name, {
            let shared = shared.clone();
            move |event| shared.on_link_event(event)
        })
        .map_err(Error::Link)?;
        Ok(Receiver {
            shared,
            _link: link,
            output,
        })
    }

    pub fn status(&self) -> Status {
        let connection = *lock(&self.shared.connection);
        let playout = lock(&self.shared.playout).stats();
        let output = self.output.state();
        let latency_ms = match (connection, &output) {
            (Connection::Streaming(format), OutputState::Playing(info)) => Some(
                headset_capture_ms(format)
                    + (playout.buffer_min_ms + playout.buffer_max_ms) / 2.0
                    + info.latency_ms(),
            ),
            _ => None,
        };
        Status {
            connection,
            output,
            playout,
            latency_ms,
        }
    }
}

impl Shared {
    fn on_link_event(&self, event: Event<'_>) {
        match event {
            Event::Format(format) => {
                let accepted = lock(&self.playout).set_format(format).is_ok();
                *lock(&self.connection) = if accepted {
                    Connection::Streaming(format)
                } else {
                    Connection::Unsupported(format)
                };
            }
            Event::Audio(audio) => lock(&self.playout).push(audio.frame_position, audio.pcm),
            Event::Quiet => *lock(&self.connection) = Connection::Waiting,
        }
    }
}

/// Plays the shared playout on the output thread.
struct PlayoutRender(Arc<Shared>);

impl Render for PlayoutRender {
    fn open(&mut self, format: OutputFormat) {
        lock(&self.0.playout).set_output(format);
    }

    fn render(&mut self, out: &mut [f32]) {
        lock(&self.0.playout).pull(out);
    }
}

/// Headset delay from a sound reaching its output to the capture server reading it, fitted to
/// `probe` on Quest 3: 6.3 ms at 96-frame blocks, 7.0 at 192, 8.0 at 384, 24 at 1920.
fn headset_capture_ms(format: Format) -> f64 {
    5.0 + f64::from(format.block_frames) * 1000.0 / f64::from(format.sample_rate) / 2.0
}

/// The computer's name, as the headset's status shows it.
fn default_name() -> String {
    ["COMPUTERNAME", "HOSTNAME"]
        .into_iter()
        .find_map(|var| std::env::var(var).ok().filter(|name| !name.is_empty()))
        .unwrap_or_else(|| "setu-desktop".into())
}

/// Locks shared state even if a holder panicked. The state is plain numbers and samples, so the
/// worst a poisoned lock leaves behind is a glitch, which beats stopping the audio.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
