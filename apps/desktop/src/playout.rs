//! Playout: the jitter buffer between the network and the audio device.
//!
//! The headset sends audio one capture block at a time, so the buffer level is a sawtooth and
//! latency is set by its bottom. The controller watches the lowest level of each half-second
//! window and nudges the playback rate (±0.5 %, linearly interpolated) until that minimum
//! settles at the margin. That keeps one block plus the margin buffered whatever the block
//! size, and absorbs the clock drift between headset and PC. A lost datagram becomes silence
//! of the same length, so timing survives loss.
//!
//! `Playout` is plain single-threaded state with no I/O. The receiver shares it between the
//! network and device threads behind a mutex, held only to copy a datagram in or a device
//! period out.

use std::fmt;

use crate::output::OutputFormat;
use crate::protocol::Format;

/// Ring capacity: 1.3 s at 48 kHz, far beyond any latency worth keeping.
const CAPACITY_FRAMES: usize = 1 << 16;
/// Playback rate stays within ±0.5 %, inaudible as pitch.
const MAX_RATE_ADJUST: f64 = 0.005;
/// An excess beyond this is skipped at once instead of played out faster.
const SKIP_THRESHOLD_MS: f64 = 20.0;
/// Each underrun grows the margin by this much...
const UNDERRUN_BACKOFF_MS: f64 = 2.0;
/// ...up to this much beyond the deadband...
const MAX_MARGIN_MS: f64 = 40.0;
/// ...and it shrinks back by a step after this long without one.
const MARGIN_DECAY_AFTER_S: f64 = 15.0;
const MARGIN_DECAY_STEP_MS: f64 = 0.5;
/// The controller acts once per window of output.
const WINDOW_S: f64 = 0.5;
/// A position gap up to this long is concealed with silence; a larger jump (such as a capture
/// restarting from position 0) resynchronizes instead.
const MAX_CONCEAL_S: f64 = 0.1;

pub struct Playout {
    margin_ms: f64,
    format: Option<Format>,
    output: Option<OutputFormat>,
    /// Derived from the format and the output once both are known.
    tuning: Option<Tuning>,

    /// Interleaved samples of `CAPACITY_FRAMES` frames in the stream's channel layout.
    ring: Vec<i16>,
    /// Absolute index of the next frame to play.
    read: u64,
    /// Absolute index of the next frame to write.
    write: u64,
    /// Interpolation position between frames `read` and `read + 1`.
    frac: f64,
    /// Stream position expected in the next datagram.
    next_position: Option<u64>,

    playing: bool,
    /// Target buffer minimum, in input frames.
    margin: u64,
    /// Level to reach before playback (re)starts.
    prime_to: u64,
    /// Output frames played since the last underrun or margin decay.
    calm_frames: u64,
    rate_adjust: f64,

    window: Window,
    last_window: Window,
    totals: Totals,
}

/// A snapshot of the playout for status displays.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayoutStats {
    pub state: PlayoutState,
    /// Lowest and highest buffer level over the last half second.
    pub buffer_min_ms: f64,
    pub buffer_max_ms: f64,
    /// The buffer minimum the controller steers to.
    pub margin_ms: f64,
    /// Playback rate offset, e.g. 0.002 plays 0.2 % fast.
    pub rate_adjust: f64,
    /// Largest sample over the last half second, 0 to 1.
    pub peak: f32,
    /// Totals since the receiver started.
    pub underruns: u64,
    pub lost_ms: f64,
    pub skipped_ms: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayoutState {
    /// Waiting for the stream format or the output device.
    Idle,
    /// Filling to one block plus the margin before playing.
    Buffering,
    Playing,
}

/// The stream's samples are not 16-bit PCM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnsupportedFormat(pub Format);

impl fmt::Display for UnsupportedFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Format {
            channels,
            bytes_per_sample,
            ..
        } = self.0;
        write!(
            f,
            "unsupported stream format: {channels} channels of {bytes_per_sample}-byte samples"
        )
    }
}

impl std::error::Error for UnsupportedFormat {}

#[derive(Debug, Clone, Copy)]
struct Tuning {
    in_rate: f64,
    /// Input frames per output frame at nominal rate.
    step: f64,
    in_channels: usize,
    out_channels: usize,
    block: u64,
    /// How far the measured minimum moves with arrival phase alone: min(block, output
    /// period). It is part of the margin and the controller's deadband.
    quantum: u64,
    min_margin: u64,
    max_margin: u64,
    backoff: u64,
    decay_step: u64,
    skip_threshold: u64,
    /// In output frames.
    window_frames: u64,
    calm_after: u64,
}

#[derive(Debug, Clone, Copy)]
struct Window {
    /// Output frames pulled in this window.
    pulled: u64,
    min_level: u64,
    max_level: u64,
    peak: u16,
}

impl Window {
    fn new(level: u64) -> Self {
        Window {
            pulled: 0,
            min_level: u64::MAX,
            max_level: level,
            peak: 0,
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
struct Totals {
    underruns: u64,
    lost_frames: u64,
    skipped_frames: u64,
}

impl Playout {
    /// `margin_ms` is the safety margin beyond one capture block and one output period.
    pub fn new(margin_ms: f64) -> Self {
        Playout {
            margin_ms,
            format: None,
            output: None,
            tuning: None,
            ring: Vec::new(),
            read: 0,
            write: 0,
            frac: 0.0,
            next_position: None,
            playing: false,
            margin: 0,
            prime_to: 0,
            calm_frames: 0,
            rate_adjust: 0.0,
            window: Window::new(0),
            last_window: Window::new(0),
            totals: Totals::default(),
        }
    }

    /// Plays a stream in `format`. The same format again changes nothing, as the server repeats
    /// FORMAT on every HELLO; a different one discards the buffered audio.
    pub fn set_format(&mut self, format: Format) -> Result<(), UnsupportedFormat> {
        if format.bytes_per_sample != 2 || format.channels == 0 || format.sample_rate == 0 {
            return Err(UnsupportedFormat(format));
        }
        if self.format == Some(format) {
            return Ok(());
        }
        self.format = Some(format);
        self.ring = vec![0; CAPACITY_FRAMES * usize::from(format.channels)];
        self.read = 0;
        self.write = 0;
        self.frac = 0.0;
        self.next_position = None;
        self.retune();
        Ok(())
    }

    /// Plays into a device of this format, e.g. after the output (re)opened. Buffered audio is
    /// kept; playback restarts once one block plus the margin is buffered.
    pub fn set_output(&mut self, output: OutputFormat) {
        self.output = Some(output);
        self.retune();
    }

    /// Adds one datagram's PCM, whose first frame is `frame_position` in the stream.
    pub fn push(&mut self, frame_position: u64, pcm: &[u8]) {
        let Some(format) = self.format else {
            return;
        };
        let frame_bytes = format.frame_bytes();
        let frames = (pcm.len() / frame_bytes) as u64;
        let max_conceal = (f64::from(format.sample_rate) * MAX_CONCEAL_S) as u64;
        let mut skip = 0;
        match self.next_position {
            Some(next) if frame_position > next && frame_position - next <= max_conceal => {
                let gap = frame_position - next;
                self.write_silence(gap);
                self.totals.lost_frames += gap;
            }
            Some(next) if frame_position < next && next - frame_position <= max_conceal => {
                // Late or duplicate: keep only frames not yet written.
                skip = next - frame_position;
                if skip >= frames {
                    return;
                }
            }
            // The first datagram, the next in order, or a jump too large to conceal.
            _ => {}
        }
        let start = skip as usize * frame_bytes;
        let end = frames as usize * frame_bytes;
        self.write_pcm(&pcm[start..end]);
        self.next_position = Some(frame_position + frames);
    }

    /// Fills `out` with interleaved frames in the output's format. Silent until a stream and
    /// an output are both known, and while buffering.
    pub fn pull(&mut self, out: &mut [f32]) {
        out.fill(0.0);
        let Some(t) = self.tuning else {
            return;
        };
        let frames = out.len() / t.out_channels;
        if !self.playing {
            if self.level() < self.prime_to {
                self.after_pull(frames as u64, &t);
                return;
            }
            self.playing = true;
            self.window = Window::new(self.level());
        }

        let step = t.step * (1.0 + self.rate_adjust);
        let mask = CAPACITY_FRAMES - 1;
        let channels = t.in_channels.min(t.out_channels);
        for frame in out.chunks_exact_mut(t.out_channels) {
            // Interpolation needs frames `read` and `read + 1`.
            if self.level() < 2 {
                self.underrun(&t);
                break;
            }
            let a = (self.read as usize & mask) * t.in_channels;
            let b = ((self.read + 1) as usize & mask) * t.in_channels;
            let f = self.frac as f32;
            for (c, sample) in frame[..channels].iter_mut().enumerate() {
                let s = f32::from(self.ring[a + c]) * (1.0 - f) + f32::from(self.ring[b + c]) * f;
                *sample = s / 32768.0;
            }
            self.frac += step;
            let whole = self.frac as u64;
            self.read += whole;
            self.frac -= whole as f64;
        }
        self.after_pull(frames as u64, &t);
    }

    pub fn stats(&self) -> PlayoutStats {
        let to_ms = |frames: u64| match self.format {
            Some(f) => frames as f64 * 1000.0 / f64::from(f.sample_rate),
            None => 0.0,
        };
        let window = &self.last_window;
        let min_level = if window.min_level == u64::MAX {
            window.max_level
        } else {
            window.min_level
        };
        PlayoutStats {
            state: match (self.tuning, self.playing) {
                (None, _) => PlayoutState::Idle,
                (Some(_), false) => PlayoutState::Buffering,
                (Some(_), true) => PlayoutState::Playing,
            },
            buffer_min_ms: to_ms(min_level),
            buffer_max_ms: to_ms(window.max_level.max(min_level)),
            margin_ms: to_ms(self.margin),
            rate_adjust: self.rate_adjust,
            peak: f32::from(window.peak) / 32768.0,
            underruns: self.totals.underruns,
            lost_ms: to_ms(self.totals.lost_frames),
            skipped_ms: to_ms(self.totals.skipped_frames),
        }
    }

    fn level(&self) -> u64 {
        self.write - self.read
    }

    /// Recomputes the controller for the current format and output, and restarts buffering.
    fn retune(&mut self) {
        self.tuning = match (self.format, self.output) {
            (Some(format), Some(output)) => Some(Tuning::new(format, output, self.margin_ms)),
            _ => None,
        };
        if let Some(t) = self.tuning {
            self.margin = t.min_margin;
            self.prime_to = t.block + self.margin;
        }
        self.playing = false;
        self.rate_adjust = 0.0;
        self.calm_frames = 0;
        self.window = Window::new(self.level());
    }

    fn write_pcm(&mut self, pcm: &[u8]) {
        let channels = self.ring.len() / CAPACITY_FRAMES;
        let mask = CAPACITY_FRAMES - 1;
        let mut peak = self.window.peak;
        for frame in pcm.chunks_exact(channels * 2) {
            let base = (self.write as usize & mask) * channels;
            for (c, bytes) in frame.chunks_exact(2).enumerate() {
                let sample = i16::from_le_bytes([bytes[0], bytes[1]]);
                self.ring[base + c] = sample;
                peak = peak.max(sample.unsigned_abs());
            }
            self.write += 1;
        }
        self.window.peak = peak;
        self.after_write();
    }

    fn write_silence(&mut self, frames: u64) {
        let channels = self.ring.len() / CAPACITY_FRAMES;
        let mask = CAPACITY_FRAMES - 1;
        for _ in 0..frames.min(CAPACITY_FRAMES as u64) {
            let base = (self.write as usize & mask) * channels;
            self.ring[base..base + channels].fill(0);
            self.write += 1;
        }
        self.after_write();
    }

    fn after_write(&mut self) {
        // A full ring overwrites the oldest audio.
        let capacity = CAPACITY_FRAMES as u64 - 1;
        if self.level() > capacity {
            self.read = self.write - capacity;
        }
        self.window.max_level = self.window.max_level.max(self.level());
    }

    fn underrun(&mut self, t: &Tuning) {
        self.totals.underruns += 1;
        self.playing = false;
        self.calm_frames = 0;
        // Blocks arrive later than the margin covers: back off.
        self.margin = (self.margin + t.backoff).min(t.max_margin);
        self.prime_to = t.block + self.margin;
    }

    /// Records the post-pull level (the bottom of the sawtooth) and runs the controller once
    /// per window.
    fn after_pull(&mut self, frames: u64, t: &Tuning) {
        let level = self.level();
        self.window.min_level = self.window.min_level.min(level);
        self.window.pulled += frames;
        if self.playing {
            // A transient spike grows the margin on underrun; win it back once playback has
            // been clean for a while.
            self.calm_frames += frames;
            if self.calm_frames >= t.calm_after && self.margin > t.min_margin {
                self.margin = self.margin.saturating_sub(t.decay_step).max(t.min_margin);
                self.prime_to = t.block + self.margin;
                self.calm_frames = 0;
            }
        }
        if self.window.pulled < t.window_frames {
            return;
        }
        if self.playing {
            self.steer(self.window.min_level, t);
        }
        self.last_window = self.window;
        self.window = Window::new(self.level());
    }

    /// Sets the playback rate from the window's minimum level. Within [margin, margin +
    /// quantum] the minimum moves with arrival phase, not with the true level, so that band is
    /// on target.
    fn steer(&mut self, min_level: u64, t: &Tuning) {
        let excess = min_level as i64 - self.margin as i64;
        let quantum = t.quantum as i64;
        self.rate_adjust = if excess > t.skip_threshold as i64 + quantum {
            let skip = (excess - quantum / 2) as u64;
            self.read += skip;
            self.totals.skipped_frames += skip;
            0.0
        } else if excess > quantum {
            // Shed the excess over about two seconds.
            ((excess - quantum) as f64 / (2.0 * t.in_rate)).min(MAX_RATE_ADJUST)
        } else if excess < 0 {
            // Refill the deficit over about one second.
            (excess as f64 / t.in_rate).max(-MAX_RATE_ADJUST)
        } else {
            0.0
        };
    }
}

impl Tuning {
    fn new(format: Format, output: OutputFormat, margin_ms: f64) -> Self {
        let in_rate = f64::from(format.sample_rate);
        let out_rate = f64::from(output.sample_rate);
        let frames = |ms: f64| (ms * in_rate / 1000.0) as u64;
        let block_ms = f64::from(format.block_frames) * 1000.0 / in_rate;
        let period_ms = f64::from(output.period_frames) * 1000.0 / out_rate;
        let quantum_ms = block_ms.min(period_ms);
        Tuning {
            in_rate,
            step: in_rate / out_rate,
            in_channels: usize::from(format.channels),
            out_channels: usize::from(output.channels.max(1)),
            block: u64::from(format.block_frames),
            quantum: frames(quantum_ms),
            min_margin: frames(margin_ms + quantum_ms),
            max_margin: frames(MAX_MARGIN_MS + quantum_ms),
            backoff: frames(UNDERRUN_BACKOFF_MS),
            decay_step: frames(MARGIN_DECAY_STEP_MS),
            skip_threshold: frames(SKIP_THRESHOLD_MS),
            window_frames: (out_rate * WINDOW_S) as u64,
            calm_after: (out_rate * MARGIN_DECAY_AFTER_S) as u64,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn format(block_frames: u32) -> Format {
        Format {
            sample_rate: 48000,
            channels: 2,
            bytes_per_sample: 2,
            block_frames,
        }
    }

    fn output(sample_rate: u32, channels: u16, period_frames: u32) -> OutputFormat {
        OutputFormat {
            sample_rate,
            channels,
            period_frames,
        }
    }

    /// `frames` stereo frames with every sample `value`.
    fn pcm(frames: usize, value: i16) -> Vec<u8> {
        value.to_le_bytes().repeat(frames * 2)
    }

    fn playout(block_frames: u32, period_frames: u32) -> Playout {
        let mut p = Playout::new(1.5);
        p.set_format(format(block_frames)).unwrap();
        p.set_output(output(48000, 2, period_frames));
        p
    }

    #[test]
    fn silent_until_format_and_output_are_known() {
        let mut p = Playout::new(1.5);
        p.push(0, &pcm(960, 1000));
        let mut out = [1.0; 64];
        p.pull(&mut out);
        assert!(out.iter().all(|&s| s == 0.0));
        assert_eq!(p.stats().state, PlayoutState::Idle);
    }

    #[test]
    fn buffers_one_block_plus_margin_then_plays_the_stream() {
        let mut p = playout(96, 96);
        // Margin = 1.5 ms + min(block, period) = 3.5 ms = 168 frames; prime to 96 + 168.
        p.push(0, &pcm(200, 16384));
        let mut out = vec![1.0; 2 * 32];
        p.pull(&mut out);
        assert!(out.iter().all(|&s| s == 0.0));
        assert_eq!(p.stats().state, PlayoutState::Buffering);

        p.push(200, &pcm(100, 16384));
        p.pull(&mut out);
        assert!(out.iter().all(|&s| s == 0.5));
        assert_eq!(p.stats().state, PlayoutState::Playing);
    }

    #[test]
    fn extra_output_channels_are_silent() {
        let mut p = Playout::new(1.5);
        p.set_format(format(96)).unwrap();
        p.set_output(output(48000, 4, 96));
        p.push(0, &pcm(400, 16384));
        let mut out = vec![1.0; 4 * 8];
        p.pull(&mut out);
        for frame in out.chunks_exact(4) {
            assert_eq!(frame, [0.5, 0.5, 0.0, 0.0]);
        }
    }

    #[test]
    fn a_lost_datagram_becomes_silence_of_its_length() {
        let mut p = playout(96, 96);
        p.push(0, &pcm(340, 1000));
        p.push(680, &pcm(340, 1000));
        assert_eq!(p.level(), 1020);
        assert_eq!(p.stats().lost_ms, 340.0 * 1000.0 / 48000.0);
    }

    #[test]
    fn late_and_duplicate_datagrams_add_only_unwritten_frames() {
        let mut p = playout(96, 96);
        p.push(0, &pcm(96, 1000));
        p.push(96, &pcm(96, 1000));
        p.push(0, &pcm(96, 1000));
        assert_eq!(p.level(), 192);
        p.push(144, &pcm(96, 1000));
        assert_eq!(p.level(), 240);
        assert_eq!(p.stats().lost_ms, 0.0);
    }

    #[test]
    fn a_restarted_capture_resynchronizes_without_silence() {
        let mut p = playout(96, 96);
        p.push(1_000_000, &pcm(96, 1000));
        p.push(0, &pcm(96, 1000));
        assert_eq!(p.level(), 192);
        assert_eq!(p.stats().lost_ms, 0.0);
    }

    #[test]
    fn repeated_format_keeps_the_buffer_and_a_new_one_clears_it() {
        let mut p = playout(96, 96);
        p.push(0, &pcm(96, 1000));
        p.set_format(format(96)).unwrap();
        assert_eq!(p.level(), 96);
        p.set_format(format(192)).unwrap();
        assert_eq!(p.level(), 0);
    }

    #[test]
    fn only_16_bit_pcm_is_supported() {
        let mut p = Playout::new(1.5);
        let wide = Format {
            bytes_per_sample: 3,
            ..format(96)
        };
        assert_eq!(p.set_format(wide), Err(UnsupportedFormat(wide)));
    }

    /// Simulates capture blocks arriving with jitter into a device pulling fixed periods on a
    /// slightly faster clock, and checks the buffer settles near one block plus the margin
    /// without underruns.
    #[test]
    fn settles_on_one_block_plus_margin() {
        let cases = [
            ("40 ms blocks, 10 ms pulls", 1920, 48000, 480, 3000.0),
            ("2 ms blocks, 7 ms pulls", 96, 48000, 336, 2000.0),
            ("2 ms blocks, 2.67 ms pulls", 96, 48000, 128, 2000.0),
            (
                "2 ms blocks, 10 ms pulls at 44.1 kHz",
                96,
                44100,
                441,
                2000.0,
            ),
        ];
        for (name, block, out_rate, period, jitter_us) in cases {
            simulate(name, block, out_rate, period, jitter_us);
        }
    }

    fn simulate(name: &str, block: u32, out_rate: u32, period: u32, jitter_us: f64) {
        const MARGIN_MS: f64 = 1.5;
        let mut p = Playout::new(MARGIN_MS);
        p.set_format(format(block)).unwrap();
        p.set_output(output(out_rate, 2, period));
        let block_us = f64::from(block) * 1e6 / 48000.0;
        let period_us = f64::from(period) * 1e6 / f64::from(out_rate);
        let input = pcm(block as usize, 1000);
        let mut out = vec![0.0; period as usize * 2];
        let quantum_ms = block_us.min(period_us) / 1000.0;
        let ceiling_ms = MARGIN_MS + 2.0 * quantum_ms + jitter_us / 1000.0 + 3.0;

        // Event-driven timeline in microseconds. Blocks leave on the headset's clock and each
        // arrives late by up to `jitter_us`, never before the previous one. The PC's clock runs
        // 100 ppm fast.
        let mut rng = 0x2545_f491_4f6c_dd1d_u64;
        let mut uniform = move || {
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            (rng >> 11) as f64 / (1u64 << 53) as f64
        };
        let mut blocks = 0u64;
        let mut next_block = 0.0f64;
        let mut next_pull = 0.0f64;
        let pull_step = period_us / 1.0001;
        let mut settled_underruns = None;
        let mut next_check = 6e6;
        let mut late_windows = 0;
        let mut now = 0.0;
        while now < 60e6 {
            if next_block <= next_pull {
                now = next_block;
                p.push(blocks * u64::from(block), &input);
                blocks += 1;
                next_block = next_block.max(blocks as f64 * block_us + uniform() * jitter_us);
            } else {
                now = next_pull;
                p.pull(&mut out);
                next_pull += pull_step;
            }
            if settled_underruns.is_none() && now > 5e6 {
                settled_underruns = Some(p.stats().underruns);
            }
            if now >= next_check {
                next_check += 1e6;
                if p.stats().buffer_min_ms > ceiling_ms {
                    late_windows += 1;
                }
            }
        }
        let stats = p.stats();
        let underruns = stats.underruns - settled_underruns.unwrap();
        assert_eq!(underruns, 0, "{name}: underruns after settling");
        assert_eq!(
            late_windows, 0,
            "{name}: buffer minimum stayed above {ceiling_ms:.1} ms"
        );
        assert_eq!(stats.state, PlayoutState::Playing, "{name}");
    }
}
