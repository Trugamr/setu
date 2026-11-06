//! Output: plays audio on one of the PC's devices.
//!
//! [`Output::start`] runs a device on its own real-time thread and asks a [`Render`] for every
//! period. When the device goes away, or the system default changes while following it, the
//! output reopens on its own and tells the renderer the new format. Only a failure to open the
//! first time is returned to the caller; later failures show in [`Output::state`].
//!
//! The device API lives in a platform adapter: WASAPI on Windows. Other platforms compile, but
//! [`Output::start`] fails there.

use std::fmt;
use std::sync::mpsc;
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, JoinHandle};

#[cfg(not(windows))]
mod unsupported;
#[cfg(windows)]
mod wasapi;

#[cfg(not(windows))]
use unsupported as platform;
#[cfg(windows)]
use wasapi as platform;

/// What a device plays: interleaved `f32` frames, `period_frames` at a time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutputFormat {
    pub sample_rate: u32,
    pub channels: u16,
    /// Frames the device asks for in one go.
    pub period_frames: u32,
}

impl OutputFormat {
    pub fn period_ms(&self) -> f64 {
        f64::from(self.period_frames) * 1000.0 / f64::from(self.sample_rate)
    }
}

/// Produces the audio a device plays. Both methods run on the output's real-time thread, so
/// they must not block for long.
pub trait Render: Send + 'static {
    /// The device (re)opened in `format`. Called before the first `render` after each open.
    fn open(&mut self, format: OutputFormat);
    /// Fills `out` with interleaved samples in the format of the last `open`, nominally -1 to 1.
    fn render(&mut self, out: &mut [f32]);
}

#[derive(Debug, Clone, PartialEq)]
pub struct OutputOptions {
    /// A case-insensitive part of the device name; `None` follows the system default.
    pub device: Option<String>,
    /// Bypass the system mixer for the lowest latency. The device must be idle.
    pub exclusive: bool,
    /// Shared mode: audio kept queued in the device beyond one period, in milliseconds.
    pub queue_ms: f64,
}

impl Default for OutputOptions {
    fn default() -> Self {
        OutputOptions {
            device: None,
            exclusive: false,
            queue_ms: 1.0,
        }
    }
}

/// An opened device.
#[derive(Debug, Clone, PartialEq)]
pub struct OutputInfo {
    pub device: String,
    pub exclusive: bool,
    pub format: OutputFormat,
    /// The device's own sample encoding, e.g. "float32".
    pub encoding: String,
    pub buffer_ms: f64,
    /// Audio this output keeps queued in the device buffer.
    pub queued_ms: f64,
    /// Latency the audio engine reports for the stream.
    pub engine_latency_ms: f64,
}

impl OutputInfo {
    /// Delay from a frame leaving [`Render::render`] to it reaching the device.
    pub fn latency_ms(&self) -> f64 {
        self.queued_ms + self.engine_latency_ms
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum OutputState {
    Playing(OutputInfo),
    /// The device failed; the output retries until it opens again.
    Reopening {
        error: String,
    },
}

/// The platform adapter's report on a device it opened or lost.
enum Event {
    Opened(OutputInfo),
    Lost(Error),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(String);

impl Error {
    fn new(message: impl Into<String>) -> Self {
        Error(message.into())
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

/// Output devices by name, and which one is the system default.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Devices {
    pub names: Vec<String>,
    pub default: Option<String>,
}

pub fn devices() -> Result<Devices, Error> {
    platform::devices()
}

/// A running output. Dropping it stops the device.
pub struct Output {
    state: Arc<Mutex<OutputState>>,
    stop: Arc<platform::Stop>,
    thread: Option<JoinHandle<()>>,
}

impl Output {
    /// Opens the device and starts rendering. Returns once the device plays, or with the reason
    /// it could not open.
    pub fn start(options: OutputOptions, render: impl Render) -> Result<Output, Error> {
        let stop = Arc::new(platform::Stop::new()?);
        let (opened_tx, opened_rx) = mpsc::channel();
        // Replaced by the first open, before `start` returns.
        let state = Arc::new(Mutex::new(OutputState::Reopening {
            error: "opening".into(),
        }));
        let thread = thread::Builder::new()
            .name("setu-output".into())
            .spawn({
                let stop = stop.clone();
                let state = state.clone();
                let mut render = render;
                move || {
                    let mut first = Some(opened_tx);
                    let mut report = |event: Event| {
                        let next = match event {
                            Event::Opened(info) => {
                                if let Some(tx) = first.take() {
                                    let _ = tx.send(Ok(()));
                                }
                                OutputState::Playing(info)
                            }
                            Event::Lost(error) => OutputState::Reopening {
                                error: error.to_string(),
                            },
                        };
                        *state.lock().unwrap_or_else(PoisonError::into_inner) = next;
                    };
                    let result = platform::run(&options, &mut render, &mut report, &stop);
                    if let (Err(e), Some(tx)) = (result, first.take()) {
                        let _ = tx.send(Err(e));
                    }
                }
            })
            .map_err(|e| Error::new(format!("could not start the output thread: {e}")))?;
        let output = Output {
            state,
            stop,
            thread: Some(thread),
        };
        match opened_rx.recv() {
            Ok(Ok(())) => Ok(output),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(Error::new("the output thread stopped unexpectedly")),
        }
    }

    pub fn state(&self) -> OutputState {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl Drop for Output {
    fn drop(&mut self) {
        self.stop.signal();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
