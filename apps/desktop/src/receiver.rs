//! Receiver: plays the headset's audio on the PC, behind one handle.
//!
//! [`Receiver::start`] opens the output device, then links to the headset's capture server and
//! feeds what arrives into the playout. It keeps going on its own: a server that is not running
//! yet, goes quiet or restarts is picked up again, and so is an output device that comes back.
//! [`Receiver::status`] is everything a UI shows; dropping the receiver stops it.

use std::net::{SocketAddr, ToSocketAddrs};
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
    state: Arc<Mutex<State>>,
    // Declared before `output` so the network stops feeding first.
    _link: Link,
    output: Output,
}

impl Receiver {
    pub fn start(config: Config) -> Result<Receiver, Error> {
        let state = Arc::new(Mutex::new(State::new(config.margin_ms)));
        let output =
            Output::start(config.output, PlayoutRender(state.clone())).map_err(Error::Output)?;
        let link = Link::start(config.headset, &config.name, {
            let state = state.clone();
            move |event| lock(&state).on_link_event(event)
        })
        .map_err(Error::Link)?;
        Ok(Receiver {
            state,
            _link: link,
            output,
        })
    }

    pub fn status(&self) -> Status {
        let output = self.output.state();
        lock(&self.state).status(output)
    }
}

/// What the link and output threads share. One lock covers it all, so a status never mixes
/// two moments.
struct State {
    playout: Playout,
    connection: Connection,
}

impl State {
    fn new(margin_ms: f64) -> Self {
        State {
            playout: Playout::new(margin_ms),
            connection: Connection::Waiting,
        }
    }

    fn on_link_event(&mut self, event: Event<'_>) {
        match event {
            Event::Format(format) => {
                self.connection = match self.playout.set_format(format) {
                    Ok(()) => Connection::Streaming(format),
                    Err(_) => Connection::Unsupported(format),
                };
            }
            Event::Audio { audio, arrived } => {
                self.playout.push(arrived, audio.frame_position, audio.pcm)
            }
            Event::Quiet => self.connection = Connection::Waiting,
        }
    }

    fn status(&self, output: OutputState) -> Status {
        let playout = self.playout.stats();
        let latency_ms = match (self.connection, &output) {
            (Connection::Streaming(format), OutputState::Playing(info)) => Some(
                headset_capture_ms(format)
                    + (playout.buffer_min_ms + playout.buffer_max_ms) / 2.0
                    + info.latency_ms(),
            ),
            _ => None,
        };
        Status {
            connection: self.connection,
            output,
            playout,
            latency_ms,
        }
    }
}

/// Plays the shared playout on the output thread.
struct PlayoutRender(Arc<Mutex<State>>);

impl Render for PlayoutRender {
    fn open(&mut self, format: OutputFormat) {
        lock(&self.0).playout.set_output(format);
    }

    fn render(&mut self, out: &mut [f32]) {
        lock(&self.0).playout.pull(out);
    }
}

/// Headset delay from a sound reaching its output to the capture server reading it, fitted to
/// `probe` on Quest 3: 6.3 ms at 96-frame blocks, 7.0 at 192, 8.0 at 384, 24 at 1920.
fn headset_capture_ms(format: Format) -> f64 {
    5.0 + format.block_ms() / 2.0
}

/// The computer's name, as the headset's status shows it.
/// Resolves a headset typed as `host` or `host:port`; the port defaults to the protocol's.
pub fn resolve_headset(headset: &str) -> Result<SocketAddr, String> {
    let headset = headset.trim();
    if headset.is_empty() {
        return Err("enter the headset's address".into());
    }
    let with_port = if headset.contains(':') {
        headset.to_string()
    } else {
        format!("{headset}:{}", crate::protocol::UDP_PORT)
    };
    with_port
        .to_socket_addrs()
        .map_err(|e| format!("{headset}: {e}"))?
        .next()
        .ok_or_else(|| format!("{headset}: no address"))
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::OutputInfo;
    use crate::playout::PlayoutState;
    use crate::protocol::Audio;

    const FORMAT: Format = Format {
        sample_rate: 48000,
        channels: 2,
        bytes_per_sample: 2,
        block_frames: 96,
    };

    const DEVICE: OutputFormat = OutputFormat {
        sample_rate: 48000,
        channels: 2,
        period_frames: 128,
    };

    fn playing() -> OutputState {
        OutputState::Playing(OutputInfo {
            device: "Speakers".into(),
            exclusive: false,
            format: DEVICE,
            encoding: "float32".into(),
            buffer_ms: 5.0,
            queued_ms: 3.0,
            engine_latency_ms: 1.0,
        })
    }

    fn audio(frame_position: u64, pcm: &[u8]) -> Event<'_> {
        Event::Audio {
            audio: Audio {
                sequence: 0,
                frame_position,
                pcm,
            },
            arrived: std::time::Instant::now(),
        }
    }

    #[test]
    fn waits_until_the_server_sends_its_format() {
        let state = State::new(4.0);
        let status = state.status(playing());
        assert_eq!(status.connection, Connection::Waiting);
        assert_eq!(status.latency_ms, None);
    }

    #[test]
    fn format_starts_streaming_and_quiet_ends_it() {
        let mut state = State::new(4.0);
        state.on_link_event(Event::Format(FORMAT));
        assert_eq!(
            state.status(playing()).connection,
            Connection::Streaming(FORMAT)
        );
        state.on_link_event(Event::Quiet);
        assert_eq!(state.status(playing()).connection, Connection::Waiting);
    }

    #[test]
    fn a_format_it_cannot_play_is_reported() {
        let mut state = State::new(4.0);
        let wide = Format {
            bytes_per_sample: 3,
            ..FORMAT
        };
        state.on_link_event(Event::Format(wide));
        assert_eq!(
            state.status(playing()).connection,
            Connection::Unsupported(wide)
        );
    }

    #[test]
    fn audio_from_the_link_reaches_the_device() {
        let state = Arc::new(Mutex::new(State::new(4.0)));
        let mut render = PlayoutRender(state.clone());
        render.open(DEVICE);
        let pcm = 8192i16.to_le_bytes().repeat(2 * 1000);
        lock(&state).on_link_event(Event::Format(FORMAT));
        lock(&state).on_link_event(audio(0, &pcm));
        let mut out = vec![0.0; 2 * 128];
        render.render(&mut out);
        assert!(out.iter().all(|&s| s == 0.25));
        assert_eq!(
            lock(&state).status(playing()).playout.state,
            PlayoutState::Playing
        );
    }

    #[test]
    fn latency_adds_headset_capture_buffer_and_output() {
        let mut state = State::new(4.0);
        state.on_link_event(Event::Format(FORMAT));
        let status = state.status(playing());
        let buffer_ms = (status.playout.buffer_min_ms + status.playout.buffer_max_ms) / 2.0;
        // 5 ms + half a 2 ms block on the headset, 3 + 1 ms in the output.
        assert_eq!(status.latency_ms, Some(6.0 + buffer_ms + 4.0));
        let lost = OutputState::Reopening {
            error: "unplugged".into(),
        };
        assert_eq!(state.status(lost).latency_ms, None);
    }
}
