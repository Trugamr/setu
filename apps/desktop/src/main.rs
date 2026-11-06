//! Command-line front end until the desktop UI lands: plays the headset's audio and prints the
//! receiver's status once a second.

use std::net::{SocketAddr, ToSocketAddrs};
use std::process::ExitCode;
use std::thread;
use std::time::Duration;

use clap::Parser;
use setu::output::{self, OutputOptions, OutputState};
use setu::playout::PlayoutState;
use setu::protocol::UDP_PORT;
use setu::receiver::DEFAULT_MARGIN_MS;
use setu::{Config, Connection, Receiver, Status};

#[derive(Parser)]
#[command(version, about = "Plays a Meta Quest's audio on this PC")]
struct Args {
    /// The headset's address, as `host` or `host:port`.
    #[arg(required_unless_present = "list_devices")]
    headset: Option<String>,
    /// Output device: part of its name. Follows the system default when omitted.
    #[arg(long)]
    device: Option<String>,
    /// Bypass the system mixer for the lowest latency (the device must be idle).
    #[arg(long)]
    exclusive: bool,
    /// Safety margin in ms beyond one capture block and one output period. It grows on an
    /// underrun and decays back.
    #[arg(long, default_value_t = DEFAULT_MARGIN_MS)]
    margin: f64,
    /// Shared mode: ms queued in the device beyond one period.
    #[arg(long, default_value_t = OutputOptions::default().queue_ms)]
    queue: f64,
    /// How this PC appears in the headset's status.
    #[arg(long)]
    name: Option<String>,
    /// List output devices and exit.
    #[arg(long)]
    list_devices: bool,
}

fn main() -> ExitCode {
    let args = Args::parse();
    match run(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    if args.list_devices {
        let devices = output::devices()?;
        for name in &devices.names {
            let marker = if Some(name) == devices.default.as_ref() {
                "*"
            } else {
                " "
            };
            println!("{marker} {name}");
        }
        return Ok(());
    }

    let headset = resolve(args.headset.as_deref().unwrap_or_default())?;
    let mut config = Config::new(headset);
    config.output = OutputOptions {
        device: args.device,
        exclusive: args.exclusive,
        queue_ms: args.queue,
    };
    config.margin_ms = args.margin;
    if let Some(name) = args.name {
        config.name = name;
    }

    let receiver = Receiver::start(config)?;
    eprintln!("receiving from {headset}; Ctrl+C stops");
    let mut last_output = None;
    loop {
        thread::sleep(Duration::from_secs(1));
        let status = receiver.status();
        if last_output.as_ref() != Some(&status.output) {
            eprintln!("{}", describe_output(&status.output));
            last_output = Some(status.output.clone());
        }
        eprintln!("{}", describe(&status));
    }
}

fn resolve(headset: &str) -> Result<SocketAddr, String> {
    let with_port = if headset.contains(':') {
        headset.to_string()
    } else {
        format!("{headset}:{UDP_PORT}")
    };
    with_port
        .to_socket_addrs()
        .map_err(|e| format!("{headset}: {e}"))?
        .next()
        .ok_or_else(|| format!("{headset}: no address"))
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
            f64::from(info.format.period_frames) * 1000.0 / f64::from(info.format.sample_rate),
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
    let block_ms = f64::from(format.block_frames) * 1000.0 / f64::from(format.sample_rate);
    let latency = status
        .latency_ms
        .map(|ms| format!("  | est. {ms:3.0} ms behind headset"))
        .unwrap_or_default();
    format!(
        "peak {level:<9} block {block_ms:.1} ms  buffer {:5.1}..{:5.1} ms  margin {:4.1} ms  rate {:+.2}%  underruns {}  lost {:.1} ms  skipped {:.1} ms{latency}",
        p.buffer_min_ms,
        p.buffer_max_ms,
        p.margin_ms,
        p.rate_adjust * 100.0,
        p.underruns,
        p.lost_ms,
        p.skipped_ms,
    )
}
