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
    /// The last connect failed; no receiver running.
    Failed,
    /// Saying HELLO; nothing is streaming yet.
    Waiting,
    Buffering,
    Playing,
    /// The headset streams a format this receiver cannot play.
    Unsupported,
    /// The output device failed and is being reopened.
    OutputLost,
}

impl Phase {
    /// Whether something is wrong that the user may need to act on.
    pub fn is_problem(self) -> bool {
        matches!(self, Phase::Failed | Phase::Unsupported | Phase::OutputLost)
    }

    pub fn look(self) -> Look {
        match self {
            Phase::Playing => Look::Flowing,
            Phase::Waiting | Phase::Buffering => Look::Waiting,
            _ => Look::Plain,
        }
    }
}

/// How the status card looks, as on the headset. See design/brand/README.md.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Look {
    /// Ink fill: not connected, or a problem.
    Plain,
    /// Ink fill and a Cream outline: connected, but no sound yet.
    Waiting,
    /// Cream fill: sound is playing.
    Flowing,
}

/// A figure on the card while streaming, such as "12–18 ms" over "buffer".
#[derive(Debug, Clone, PartialEq)]
pub struct Fact {
    pub value: String,
    pub label: &'static str,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Summary {
    pub phase: Phase,
    pub title: String,
    pub detail: String,
    /// The peak level on a 0 to 1 scale for the meter, 0 unless playing.
    pub level: f32,
    /// Every playout figure on one line while streaming, for `setu-cli status`.
    pub stats: Option<String>,
    /// The figures worth a glance while streaming: the buffer and underruns.
    pub facts: Vec<Fact>,
    /// The rest of the figures while streaming, for the window's footer.
    pub footer: Option<String>,
}

/// Summarises a receiver's status, or without a receiver, why the last connect failed.
pub fn summarize(status: Option<&Status>, error: Option<&str>) -> Summary {
    let Some(status) = status else {
        return match error {
            Some(error) => summary(Phase::Failed, "Couldn't connect", &sentence(error)),
            None => summary(
                Phase::Stopped,
                "Not connected",
                "Enter the headset's address and connect.",
            ),
        };
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
                "Waiting",
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
        "buffer {:.0}–{:.0} ms · block {:.0} ms · margin {:.1} ms · jitter {:.1} ms, peak {:.1} ms · rate {:+.2}% · underruns {}",
        p.buffer_min_ms,
        p.buffer_max_ms,
        format.block_ms(),
        p.margin_ms,
        p.jitter_ms,
        p.jitter_peak_ms,
        p.rate_adjust * 100.0,
        p.underruns,
    );
    let facts = vec![
        Fact {
            value: format!("{:.0}–{:.0} ms", p.buffer_min_ms, p.buffer_max_ms),
            label: "buffer",
        },
        Fact {
            value: p.underruns.to_string(),
            label: "underruns",
        },
    ];
    let footer = format!(
        "Block {:.0} ms, margin {:.1} ms, rate {:+.2}%",
        format.block_ms(),
        p.margin_ms,
        p.rate_adjust * 100.0,
    );
    if p.state != PlayoutState::Playing {
        return Summary {
            stats: Some(stats),
            facts,
            footer: Some(footer),
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
        facts,
        footer: Some(footer),
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

/// An error message as a sentence: capitalised, ending in a full stop.
fn sentence(message: &str) -> String {
    let message = message.trim();
    let mut chars = message.chars();
    let mut text: String = match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => return String::new(),
    };
    if !text.ends_with(['.', '!', '?']) {
        text.push('.');
    }
    text
}

fn summary(phase: Phase, title: &str, detail: &str) -> Summary {
    Summary {
        phase,
        title: title.into(),
        detail: detail.into(),
        level: 0.0,
        stats: None,
        facts: Vec::new(),
        footer: None,
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
                jitter_ms: 3.0,
                jitter_peak_ms: 9.5,
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
        assert_eq!(summarize(None, None).phase, Phase::Stopped);
    }

    #[test]
    fn a_failed_connect_says_why_as_a_sentence() {
        let s = summarize(None, Some("1.2.3.4:x: invalid port value"));
        assert_eq!(s.phase, Phase::Failed);
        assert_eq!(s.title, "Couldn't connect");
        assert_eq!(s.detail, "1.2.3.4:x: invalid port value.");
        let s = summarize(None, Some("enter the headset's address"));
        assert_eq!(s.detail, "Enter the headset's address.");
    }

    #[test]
    fn waiting_has_no_stats_or_level() {
        let s = summarize(Some(&status(Connection::Waiting, PlayoutState::Idle)), None);
        assert_eq!(s.phase, Phase::Waiting);
        assert_eq!(s.phase.look(), Look::Waiting);
        assert_eq!(s.stats, None);
        assert!(s.facts.is_empty());
        assert_eq!(s.level, 0.0);
    }

    #[test]
    fn playing_shows_latency_level_and_stats() {
        let s = summarize(
            Some(&status(
                Connection::Streaming(FORMAT),
                PlayoutState::Playing,
            )),
            None,
        );
        assert_eq!(s.phase, Phase::Playing);
        assert_eq!(s.detail, "About 32 ms behind the headset.");
        assert!(s.level > 0.8 && s.level < 1.0, "level {}", s.level);
        assert_eq!(
            s.stats.as_deref(),
            Some(
                "buffer 12–18 ms · block 10 ms · margin 4.0 ms · jitter 3.0 ms, peak 9.5 ms · rate +0.01% · underruns 2"
            )
        );
        assert_eq!(
            s.facts,
            vec![
                Fact {
                    value: "12–18 ms".into(),
                    label: "buffer"
                },
                Fact {
                    value: "2".into(),
                    label: "underruns"
                },
            ]
        );
        assert_eq!(
            s.footer.as_deref(),
            Some("Block 10 ms, margin 4.0 ms, rate +0.01%")
        );
        assert_eq!(s.phase.look(), Look::Flowing);
    }

    #[test]
    fn buffering_keeps_the_meter_dark() {
        let s = summarize(
            Some(&status(
                Connection::Streaming(FORMAT),
                PlayoutState::Buffering,
            )),
            None,
        );
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
        let s = summarize(Some(&st), None);
        assert_eq!(s.phase, Phase::OutputLost);
        assert_eq!(s.detail, "device unplugged. Retrying.");
    }

    #[test]
    fn unsupported_names_the_sample_size() {
        let format = Format {
            bytes_per_sample: 4,
            ..FORMAT
        };
        let s = summarize(
            Some(&status(Connection::Unsupported(format), PlayoutState::Idle)),
            None,
        );
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
