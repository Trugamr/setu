//! Command-line front end. It controls the Setu running on this PC (the window, or another
//! `setu-cli play`) over the control port, and can play the headset's audio itself, without a
//! window, printing the receiver's status once a second.

use std::process::ExitCode;
use std::thread;
use std::time::{Duration, Instant};

use clap::{Parser, Subcommand, ValueEnum};
use setu::control::{self, Request};
use setu::output::{self, OutputState};
use setu::playout::PlayoutState;
use setu::receiver::DEFAULT_MARGIN_MS;
use setu::settings::Settings;
use setu::{Command, Connection, Receiver, Session, Status};

#[derive(Parser)]
#[command(
    version,
    about = "Plays a Meta Quest's audio on this PC, and controls the Setu app"
)]
struct Args {
    #[command(subcommand)]
    command: Cli,
}

#[derive(Subcommand)]
enum Cli {
    /// Print what the running app is doing, as `key=value` lines.
    Status,
    /// Connect the running app to a headset (the saved one when omitted).
    Connect {
        /// The headset's address, as `host` or `host:port`.
        headset: Option<String>,
    },
    /// Disconnect the running app.
    Disconnect,
    /// Choose the running app's output device.
    Device {
        /// Part of the device's name. Follows the system default when omitted.
        name: Option<String>,
    },
    /// Turn the running app's exclusive mode on or off.
    Exclusive { mode: Switch },
    /// List this PC's output devices; `*` marks the default.
    Devices,
    /// Play here without a window, printing the status once a second. Takes control requests
    /// unless another Setu is running. Leaves the window's saved settings alone.
    Play(Play),
}

#[derive(Clone, Copy, ValueEnum)]
enum Switch {
    On,
    Off,
}

#[derive(clap::Args)]
struct Play {
    /// The headset's address, as `host` or `host:port`.
    headset: String,
    /// Output device: part of its name. Follows the system default when omitted.
    #[arg(long)]
    device: Option<String>,
    /// Bypass the system mixer for the lowest latency (the device must be idle).
    #[arg(long)]
    exclusive: bool,
    /// Safety margin in ms beyond the measured jitter, one capture block and one output
    /// period. It grows on an underrun and decays back.
    #[arg(long, default_value_t = DEFAULT_MARGIN_MS)]
    margin: f64,
    /// Shared mode: ms queued in the device beyond one period.
    #[arg(long, default_value_t = output::OutputOptions::default().queue_ms)]
    queue: f64,
    /// How this PC appears in the headset's status.
    #[arg(long)]
    name: Option<String>,
}

fn main() -> ExitCode {
    let result = match Args::parse().command {
        Cli::Status => control(Request::Status),
        Cli::Connect { headset } => control(Request::Do(Command::Connect(headset))),
        Cli::Disconnect => control(Request::Do(Command::Disconnect)),
        Cli::Device { name } => control(Request::Do(Command::Device(name))),
        Cli::Exclusive { mode } => {
            control(Request::Do(Command::Exclusive(matches!(mode, Switch::On))))
        }
        Cli::Devices => list_devices(),
        Cli::Play(play) => run_play(play),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Sends a request to the running app and prints the snapshot it replies with.
fn control(request: Request) -> Result<(), String> {
    let reply = control::send(&request).map_err(|e| {
        format!(
            "could not reach Setu on port {} ({e}); open the window or run `setu-cli play`",
            control::CONTROL_PORT
        )
    })?;
    for (key, value) in &reply.fields {
        println!("{key}={value}");
    }
    match reply.error {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

fn list_devices() -> Result<(), String> {
    let devices = output::devices().map_err(|e| e.to_string())?;
    for name in &devices.names {
        let marker = if Some(name) == devices.default.as_ref() {
            "*"
        } else {
            " "
        };
        println!("{marker} {name}");
    }
    Ok(())
}

fn run_play(play: Play) -> Result<(), String> {
    let settings = Settings {
        headset: play.headset,
        device: play.device,
        exclusive: play.exclusive,
    };
    let (margin, queue, name) = (play.margin, play.queue, play.name);
    let mut session = Session::with_start(settings, None, move |mut config| {
        config.margin_ms = margin;
        config.output.queue_ms = queue;
        if let Some(name) = &name {
            config.name = name.clone();
        }
        Receiver::start(config).map_err(|e| e.to_string())
    });
    let server = control::Server::bind()
        .inspect_err(|e| eprintln!("not taking control requests: {e}"))
        .ok();

    session.apply(Command::Connect(None))?;
    eprintln!(
        "receiving from {}; Ctrl+C stops",
        session.snapshot().settings.headset
    );
    let mut last_output = None;
    let mut next_print = Instant::now();
    loop {
        thread::sleep(Duration::from_millis(100));
        if let Some(server) = &server {
            server.answer(&mut session);
        }
        if Instant::now() < next_print {
            continue;
        }
        next_print += Duration::from_secs(1);
        let Some(status) = session.status() else {
            eprintln!("not connected");
            continue;
        };
        if last_output.as_ref() != Some(&status.output) {
            eprintln!("{}", describe_output(&status.output));
            last_output = Some(status.output.clone());
        }
        eprintln!("{}", describe(&status));
    }
}

fn describe_output(state: &OutputState) -> String {
    match state {
        OutputState::Playing(info) => format!(
            "output: {} [{}, {} Hz, {} ch, {}] period {:.2} ms, buffer {:.2} ms, queued {:.2} ms, engine latency {:.2} ms",
            info.device,
            if info.exclusive {
                "exclusive"
            } else {
                "shared"
            },
            info.format.sample_rate,
            info.format.channels,
            info.encoding,
            info.format.period_ms(),
            info.buffer_ms,
            info.queued_ms,
            info.engine_latency_ms,
        ),
        OutputState::Reopening { error } => format!("output: {error}; reopening"),
    }
}

fn describe(status: &Status) -> String {
    let format = match status.connection {
        Connection::Waiting => return "waiting for the headset...".into(),
        Connection::Unsupported(format) => {
            return format!("the headset streams a format this receiver cannot play: {format:?}");
        }
        Connection::Streaming(format) => format,
    };
    let p = &status.playout;
    if p.state != PlayoutState::Playing {
        return format!("buffering... (underruns {})", p.underruns);
    }
    let level = if p.peak > 0.0 {
        format!("{:3.0} dBFS", 20.0 * f64::from(p.peak).log10())
    } else {
        "silent".into()
    };
    let block_ms = format.block_ms();
    let latency = status
        .latency_ms
        .map(|ms| format!("  | est. {ms:3.0} ms behind headset"))
        .unwrap_or_default();
    format!(
        "peak {level:<9} block {block_ms:.1} ms  buffer {:5.1}..{:5.1} ms  target {:4.1} ms  jitter {:4.1} ms (peak {:4.1})  rate {:+.2}%  underruns {}  lost {:.1} ms  skipped {:.1} ms{latency}",
        p.buffer_min_ms,
        p.buffer_max_ms,
        p.target_ms,
        p.jitter_ms,
        p.jitter_peak_ms,
        p.rate_adjust * 100.0,
        p.underruns,
        p.lost_ms,
        p.skipped_ms,
    )
}
