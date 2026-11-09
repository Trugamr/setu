//! What the window shows for the receiver, worked out from its [`Status`] without any UI toolkit,
//! so the wording and the meter can be tested.

use crate::output::OutputState;
use crate::playout::PlayoutState;
use crate::{Connection, Status};

/// The meter's floor: peaks at or below this show no lit dots.
const METER_FLOOR_DBFS: f32 = -60.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// No receiver running.
    Stopped,
    /// Saying HELLO; nothing is streaming yet.
    Waiting,
    Buffering,
    Playing,
    /// The headset streams a format this receiver cannot play.
    Unsupported,
    /// The output device failed and is being reopened.
    OutputLost,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Summary {
    pub phase: Phase,
    pub title: String,
    pub detail: String,
    /// The peak level on a 0 to 1 scale for the meter, 0 unless playing.
    pub level: f32,
    /// Buffer and rate figures while streaming, for the footer.
    pub stats: Option<String>,
}

/// Summarises a receiver's status, or the absence of a receiver.
pub fn summarize(status: Option<&Status>) -> Summary {
    let Some(status) = status else {
        return summary(
            Phase::Stopped,
            "Not connected",
            "Enter the headset's address and connect.",
        );
    };
    if let OutputState::Reopening { error } = &status.output {
        return summary(
            Phase::OutputLost,
            "Output unavailable",
            &format!("{error}. Retrying."),
        );
    }
    let format = match status.connection {
        Connection::Waiting => {
            return summary(
                Phase::Waiting,
                "Waiting for the headset",
                "Start the capture server on the headset. Setu connects as soon as it runs.",
            );
        }
        Connection::Unsupported(format) => {
            return summary(
                Phase::Unsupported,
                "Unsupported stream",
                &format!(
                    "The headset sends {}-bit audio, which this version cannot play.",
                    format.bytes_per_sample * 8
                ),
            );
        }
        Connection::Streaming(format) => format,
    };
    let p = &status.playout;
    let stats = format!(
        "buffer {:.0}–{:.0} ms · block {:.0} ms · margin {:.1} ms · rate {:+.2}% · underruns {}",
        p.buffer_min_ms,
        p.buffer_max_ms,
        format.block_ms(),
        p.margin_ms,
        p.rate_adjust * 100.0,
        p.underruns,
    );
    if p.state != PlayoutState::Playing {
        return Summary {
            stats: Some(stats),
            ..summary(
                Phase::Buffering,
                "Buffering",
                "Filling the buffer before playing.",
            )
        };
    }
    let detail = match status.latency_ms {
        Some(ms) => format!("About {ms:.0} ms behind the headset."),
        None => "Streaming from the headset.".into(),
    };
    Summary {
        phase: Phase::Playing,
        title: "Playing".into(),
        detail,
        level: level(p.peak),
        stats: Some(stats),
    }
}

/// Maps a peak sample (0 to 1) to the meter's 0 to 1 scale, linear in decibels.
pub fn level(peak: f32) -> f32 {
    if peak <= 0.0 {
        return 0.0;
    }
    let dbfs = 20.0 * peak.log10();
    (1.0 - dbfs / METER_FLOOR_DBFS).clamp(0.0, 1.0)
}

fn summary(phase: Phase, title: &str, detail: &str) -> Summary {
    Summary {
        phase,
        title: title.into(),
        detail: detail.into(),
        level: 0.0,
        stats: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::{OutputFormat, OutputInfo};
    use crate::playout::PlayoutStats;
    use crate::protocol::Format;

    const FORMAT: Format = Format {
        sample_rate: 48_000,
        channels: 2,
        bytes_per_sample: 2,
        block_frames: 480,
    };

    fn playing_output() -> OutputState {
        OutputState::Playing(OutputInfo {
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
        })
    }

    fn status(connection: Connection, state: PlayoutState) -> Status {
        Status {
            connection,
            output: playing_output(),
            playout: PlayoutStats {
                state,
                buffer_min_ms: 12.0,
                buffer_max_ms: 18.0,
                margin_ms: 4.0,
                rate_adjust: 0.0001,
                peak: 0.5,
                underruns: 2,
                lost_ms: 0.0,
                skipped_ms: 0.0,
            },
            latency_ms: Some(31.6),
        }
    }

    #[test]
    fn no_receiver_is_stopped() {
        assert_eq!(summarize(None).phase, Phase::Stopped);
    }

    #[test]
    fn waiting_has_no_stats_or_level() {
        let s = summarize(Some(&status(Connection::Waiting, PlayoutState::Idle)));
        assert_eq!(s.phase, Phase::Waiting);
        assert_eq!(s.stats, None);
        assert_eq!(s.level, 0.0);
    }

    #[test]
    fn playing_shows_latency_level_and_stats() {
        let s = summarize(Some(&status(
            Connection::Streaming(FORMAT),
            PlayoutState::Playing,
        )));
        assert_eq!(s.phase, Phase::Playing);
        assert_eq!(s.detail, "About 32 ms behind the headset.");
        assert!(s.level > 0.8 && s.level < 1.0, "level {}", s.level);
        assert_eq!(
            s.stats.as_deref(),
            Some("buffer 12–18 ms · block 10 ms · margin 4.0 ms · rate +0.01% · underruns 2")
        );
    }

    #[test]
    fn buffering_keeps_the_meter_dark() {
        let s = summarize(Some(&status(
            Connection::Streaming(FORMAT),
            PlayoutState::Buffering,
        )));
        assert_eq!(s.phase, Phase::Buffering);
        assert_eq!(s.level, 0.0);
        assert!(s.stats.is_some());
    }

    #[test]
    fn a_lost_output_wins_over_the_stream() {
        let mut st = status(Connection::Streaming(FORMAT), PlayoutState::Playing);
        st.output = OutputState::Reopening {
            error: "device unplugged".into(),
        };
        let s = summarize(Some(&st));
        assert_eq!(s.phase, Phase::OutputLost);
        assert_eq!(s.detail, "device unplugged. Retrying.");
    }

    #[test]
    fn unsupported_names_the_sample_size() {
        let format = Format {
            bytes_per_sample: 4,
            ..FORMAT
        };
        let s = summarize(Some(&status(
            Connection::Unsupported(format),
            PlayoutState::Idle,
        )));
        assert_eq!(s.phase, Phase::Unsupported);
        assert!(s.detail.contains("32-bit"), "{}", s.detail);
    }

    #[test]
    fn level_is_linear_in_decibels() {
        assert_eq!(level(0.0), 0.0);
        assert_eq!(level(1.0), 1.0);
        assert!((level(0.001) - 0.0).abs() < 1e-6); // -60 dBFS
        assert!((level(10f32.powf(-30.0 / 20.0)) - 0.5).abs() < 1e-4); // -30 dBFS
    }
}
