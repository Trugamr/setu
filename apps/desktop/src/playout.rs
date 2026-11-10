//! Playout: the jitter buffer between the network and the audio device.
//!
//! The headset sends audio one capture block at a time, so the buffer level is a sawtooth and
//! latency is set by its bottom. `Jitter` measures how late each datagram arrives, and as each
//! one arrives the playout notes how much it would have found buffered ahead of it had it come
//! on time: the level plus its lateness. That figure does not move with jitter, so the
//! controller can steer it without chasing every late datagram. It nudges the playback rate
//! (±0.5 %, linearly interpolated) until the lowest such figure of each half-second window
//! settles at the target: the margin, plus an allowance for the worst lateness of the last two
//! minutes. Whatever the block size, that keeps the margin to spare when the worst recent delay
//! comes again, and it absorbs the clock drift between headset and PC. A lost datagram becomes
//! silence of the same length, so timing survives loss.
//!
//! Running dry, restarting, skipping and losing a datagram never cut the audio abruptly.
//! Playback fades out over [`FADE_OUT_MS`] before the buffer runs dry, from a reserve kept for
//! it, and fades in over [`FADE_IN_MS`] when it starts again. A skip fades out, jumps and fades
//! back in. Where the stream itself breaks, at a lost datagram's silence or a position jump too
//! large to conceal, the audio either side fades too. A new format or output restarts playback
//! at once. Playback restarts at one block, one output period and the target, dropping
//! whatever piled up beyond that while it was stopped, so a stall costs latency only while it
//! lasts.
//!
//! `Playout` is plain single-threaded state with no I/O. The receiver shares it between the
//! network and device threads behind a mutex, held only to copy a datagram in or a device
//! period out. Inside, `Ring` stores and interpolates the audio and `Control` decides when
//! and how fast to play it.

use std::fmt;
use std::time::Instant;

use crate::jitter::Jitter;
use crate::output::OutputFormat;
use crate::protocol::Format;

/// Ring capacity: 1.3 s at 48 kHz, far beyond any latency worth keeping.
const CAPACITY_FRAMES: usize = 1 << 16;
/// Playback rate stays within ±0.5 %, inaudible as pitch.
const MAX_RATE_ADJUST: f64 = 0.005;
/// An excess beyond this is skipped at once instead of played out faster.
const SKIP_THRESHOLD_MS: f64 = 20.0;
/// The jitter allowance covers lateness up to this much, and the target as a whole stays
/// within this beyond the deadband: past it an occasional dropout costs less than the constant
/// delay.
const MAX_COVER_MS: f64 = 40.0;
/// Once a delay is forgotten the allowance falls at half the fastest rate the controller sheds
/// at, so it is shed by rate alone, without a skip, unless clock drift already takes more than
/// half the rate range.
const ALLOWANCE_DECAY_MS_PER_S: f64 = MAX_RATE_ADJUST * 1000.0 / 2.0;
/// Each underrun grows the margin by this much, up to the [`MAX_COVER_MS`] ceiling...
const UNDERRUN_BACKOFF_MS: f64 = 2.0;
/// ...and it shrinks back by a step after this long without one.
const MARGIN_DECAY_AFTER_S: f64 = 15.0;
const MARGIN_DECAY_STEP_MS: f64 = 0.5;
/// The controller acts once per window of output.
const WINDOW_S: f64 = 0.5;
/// Playback fades to silence over this long before the buffer runs dry or the position jumps,
/// and the buffer keeps that much in reserve for it.
const FADE_OUT_MS: f64 = 1.0;
/// Playback fades in over this long when it starts or after a jump.
const FADE_IN_MS: f64 = 2.0;
/// A position gap up to this long is concealed with silence; a larger jump (such as a capture
/// restarting from position 0) resynchronizes instead.
const MAX_CONCEAL_S: f64 = 0.1;

pub struct Playout {
    margin_ms: f64,
    format: Option<Format>,
    output: Option<OutputFormat>,
    ring: Ring,
    /// Stream position expected in the next datagram.
    next_position: Option<u64>,
    /// Exists once both the format and the output are known.
    control: Option<Control>,
    /// A fade-in over ring frames `[from, to)` that the frames written so far do not reach the
    /// end of.
    fade_in: Option<(u64, u64)>,
    jitter: Jitter,
    /// Buffer level, signal peak and lateness, for [`Playout::stats`].
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
    /// What the controller keeps buffered ahead of an on-time datagram: the safety margin plus
    /// the allowance for jitter.
    pub target_ms: f64,
    /// Playback rate offset, e.g. 0.002 plays 0.2 % fast.
    pub rate_adjust: f64,
    /// The most any datagram arrived late over the last half second, measured against the
    /// stream's own clock.
    pub jitter_ms: f64,
    /// The same over the last two minutes of the stream.
    pub jitter_peak_ms: f64,
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
    /// Filling to one block, one output period and the target before playing.
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
struct Window {
    min_level: u64,
    max_level: u64,
    peak: u16,
    /// The most a datagram arrived late, in seconds.
    late_s: f64,
}

impl Window {
    fn new(level: u64) -> Self {
        Window {
            min_level: level,
            max_level: level,
            peak: 0,
            late_s: 0.0,
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
            ring: Ring::new(0),
            next_position: None,
            control: None,
            fade_in: None,
            jitter: Jitter::new(MAX_COVER_MS / 1000.0),
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
        self.ring = Ring::new(usize::from(format.channels));
        self.next_position = None;
        self.fade_in = None;
        self.retune();
        Ok(())
    }

    /// Plays into a device of this format, e.g. after the output (re)opened. Buffered audio is
    /// kept; playback restarts once one block, one output period and the target are buffered.
    pub fn set_output(&mut self, output: OutputFormat) {
        self.output = Some(output);
        self.retune();
    }

    /// Adds one datagram's PCM, whose first frame is `frame_position` in the stream, and which
    /// arrived at `arrived`.
    pub fn push(&mut self, arrived: Instant, frame_position: u64, pcm: &[u8]) {
        let Some(format) = self.format else {
            return;
        };
        let frame_bytes = format.frame_bytes();
        let frames = (pcm.len() / frame_bytes) as u64;
        let max_conceal = (f64::from(format.sample_rate) * MAX_CONCEAL_S) as u64;
        let fade = |ms: f64| (ms * f64::from(format.sample_rate) / 1000.0) as u64;
        let mut skip = 0;
        let mut gap = 0;
        let mut joined = true;
        match self.next_position {
            Some(next) if frame_position > next && frame_position - next <= max_conceal => {
                gap = frame_position - next;
            }
            Some(next) if frame_position < next && next - frame_position <= max_conceal => {
                // Late or duplicate: keep only frames not yet written.
                skip = next - frame_position;
            }
            // A jump too large to conceal, such as a capture restarting from position 0.
            Some(next) if frame_position != next => joined = false,
            // The first datagram, or the next in order.
            _ => {}
        }
        let broken = gap > 0 || !joined;
        if broken {
            self.ring.fade_out_tail(fade(FADE_OUT_MS));
            self.ring.push_silence(gap);
            self.totals.lost_frames += gap;
        }
        // A late datagram counts even when its frames were concealed already.
        let stream_s = frame_position as f64 / f64::from(format.sample_rate);
        let late_s = self.jitter.arrive(arrived, stream_s);
        self.window.late_s = self.window.late_s.max(late_s);
        if let Some(control) = &mut self.control {
            let ahead = self.ring.level() as i64 - skip as i64;
            let late = (late_s * control.tuning.in_rate).round() as i64;
            control.arrived(ahead + late);
        }
        if skip > 0 && skip >= frames {
            return;
        }
        let start = self.ring.write;
        let peak = self
            .ring
            .push_pcm(&pcm[skip as usize * frame_bytes..frames as usize * frame_bytes]);
        if broken {
            self.fade_in = Some((start, start + fade(FADE_IN_MS)));
        }
        if let Some((from, to)) = self.fade_in {
            // Only the frames just written, as earlier ones had their share already.
            self.ring.ramp(from.max(start), from, to, true);
            if self.ring.write >= to {
                self.fade_in = None;
            }
        }
        self.window.peak = self.window.peak.max(peak);
        self.window.max_level = self.window.max_level.max(self.ring.level());
        self.next_position = Some(frame_position + frames);
    }

    /// Fills `out` with interleaved frames in the output's format. Silent until a stream and
    /// an output are both known, and while buffering.
    pub fn pull(&mut self, out: &mut [f32]) {
        out.fill(0.0);
        let Some(control) = &mut self.control else {
            return;
        };
        let channels = control.tuning.out_channels;
        let cover = (self.jitter.cover() * control.tuning.in_rate).round() as u64;
        // A new delay counts before playback restarts, so a stall's own burst is kept for it.
        control.cover(cover);
        if control.should_play(self.ring.level()) {
            let step = control.step();
            let frames = out.len() / channels;
            for (i, frame) in out.chunks_exact_mut(channels).enumerate() {
                if control.gain <= 0.0 && control.pending_skip > 0 {
                    // Silent at this point, so the jump is inaudible. Enough stays for the rest
                    // of this pull and the reserve.
                    let rest = ((frames - i) as f64 * step).ceil() as u64;
                    let keep = rest + control.tuning.reserve;
                    let skip = control
                        .pending_skip
                        .min(self.ring.level().saturating_sub(keep));
                    self.ring.skip(skip);
                    self.totals.skipped_frames += skip;
                    control.pending_skip = 0;
                }
                let fading = control.pending_skip > 0 || self.ring.level() < control.tuning.reserve;
                if (fading && control.gain <= 0.0) || !self.ring.play(frame, step) {
                    // The reserve is spent; this frame and the rest stay silent.
                    control.underrun();
                    self.totals.underruns += 1;
                    break;
                }
                control.gain = if fading {
                    (control.gain - control.tuning.fade_out_step).max(0.0)
                } else {
                    (control.gain + control.tuning.fade_in_step).min(1.0)
                };
                for sample in frame {
                    *sample *= control.gain;
                }
            }
        }
        let level = self.ring.level();
        self.window.min_level = self.window.min_level.min(level);
        if control.after_pull((out.len() / channels) as u64, cover) {
            self.last_window = self.window;
            self.window = Window::new(level);
        }
    }

    pub fn stats(&self) -> PlayoutStats {
        let to_ms = |frames: u64| match self.format {
            Some(f) => frames as f64 * 1000.0 / f64::from(f.sample_rate),
            None => 0.0,
        };
        let window = &self.last_window;
        PlayoutStats {
            state: match &self.control {
                None => PlayoutState::Idle,
                Some(c) if c.playing => PlayoutState::Playing,
                Some(_) => PlayoutState::Buffering,
            },
            buffer_min_ms: to_ms(window.min_level),
            buffer_max_ms: to_ms(window.max_level),
            target_ms: to_ms(self.control.as_ref().map_or(0, Control::target)),
            rate_adjust: self.control.as_ref().map_or(0.0, |c| c.rate_adjust),
            jitter_ms: window.late_s * 1000.0,
            jitter_peak_ms: self.jitter.peak() * 1000.0,
            peak: f32::from(window.peak) / 32768.0,
            underruns: self.totals.underruns,
            lost_ms: to_ms(self.totals.lost_frames),
            skipped_ms: to_ms(self.totals.skipped_frames),
        }
    }

    /// Restarts control for the current format and output, which also restarts buffering.
    fn retune(&mut self) {
        self.control = match (self.format, self.output) {
            (Some(format), Some(output)) => {
                Some(Control::new(Tuning::new(format, output, self.margin_ms)))
            }
            _ => None,
        };
        self.window = Window::new(self.ring.level());
    }
}

/// The buffered stream: `CAPACITY_FRAMES` frames in the stream's channel layout, with a
/// fractional read position so playback can run slightly fast or slow.
struct Ring {
    samples: Vec<i16>,
    channels: usize,
    /// Absolute index of the next frame to play.
    read: u64,
    /// Absolute index of the next frame to write.
    write: u64,
    /// Interpolation position between frames `read` and `read + 1`.
    frac: f64,
}

impl Ring {
    const MASK: usize = CAPACITY_FRAMES - 1;

    fn new(channels: usize) -> Self {
        Ring {
            samples: vec![0; CAPACITY_FRAMES * channels],
            channels,
            read: 0,
            write: 0,
            frac: 0.0,
        }
    }

    fn level(&self) -> u64 {
        self.write - self.read
    }

    /// Appends little-endian 16-bit frames and returns their largest absolute sample.
    fn push_pcm(&mut self, pcm: &[u8]) -> u16 {
        let mut peak = 0;
        for frame in pcm.chunks_exact(self.channels * 2) {
            let base = (self.write as usize & Self::MASK) * self.channels;
            for (slot, bytes) in self.samples[base..].iter_mut().zip(frame.chunks_exact(2)) {
                *slot = i16::from_le_bytes([bytes[0], bytes[1]]);
                peak = peak.max(slot.unsigned_abs());
            }
            self.write += 1;
        }
        self.drop_overflow();
        peak
    }

    fn push_silence(&mut self, frames: u64) {
        for _ in 0..frames.min(CAPACITY_FRAMES as u64) {
            let base = (self.write as usize & Self::MASK) * self.channels;
            self.samples[base..base + self.channels].fill(0);
            self.write += 1;
        }
        self.drop_overflow();
    }

    /// A full ring overwrites the oldest audio.
    fn drop_overflow(&mut self) {
        let capacity = CAPACITY_FRAMES as u64 - 1;
        if self.level() > capacity {
            self.read = self.write - capacity;
        }
    }

    fn skip(&mut self, frames: u64) {
        self.read += frames.min(self.level());
    }

    /// Fades the last `frames` written out to silence, or as many of them as have not started
    /// playing, so the fade never starts with a jump.
    fn fade_out_tail(&mut self, frames: u64) {
        let to = self.write;
        let from = to.saturating_sub(frames).max(self.read + 2);
        if from < to {
            self.ramp(from, from, to, false);
        }
    }

    /// Scales frames from `first` on, of a fade over `[from, to)` that rises from silence when
    /// `rising`, else falls to it. Frames already playing, and frames not yet written, are left
    /// as they are.
    fn ramp(&mut self, first: u64, from: u64, to: u64, rising: bool) {
        let span = (to - from) as f32 + 1.0;
        for k in first.max(from).max(self.read + 2)..to.min(self.write) {
            let gain = if rising {
                (k - from + 1) as f32 / span
            } else {
                (to - k) as f32 / span
            };
            let base = (k as usize & Self::MASK) * self.channels;
            for sample in &mut self.samples[base..base + self.channels] {
                *sample = (f32::from(*sample) * gain).round() as i16;
            }
        }
    }

    /// Plays one output frame into `out`, interpolated between frames `read` and `read + 1`,
    /// then advances by `step` input frames. Output channels beyond the stream's stay as they
    /// are. Returns false, playing nothing, when fewer than two frames are buffered.
    fn play(&mut self, out: &mut [f32], step: f64) -> bool {
        if self.level() < 2 {
            return false;
        }
        let a = (self.read as usize & Self::MASK) * self.channels;
        let b = ((self.read + 1) as usize & Self::MASK) * self.channels;
        let f = self.frac as f32;
        let frames = self.samples[a..a + self.channels]
            .iter()
            .zip(&self.samples[b..b + self.channels]);
        for (sample, (&x, &y)) in out.iter_mut().zip(frames) {
            *sample = (f32::from(x) * (1.0 - f) + f32::from(y) * f) / 32768.0;
        }
        self.frac += step;
        let whole = self.frac as u64;
        self.read += whole;
        self.frac -= whole as f64;
        true
    }
}

/// The control law: when to play, and how fast.
struct Control {
    tuning: Tuning,
    playing: bool,
    /// Safety margin in input frames: the configured margin and the deadband, grown by
    /// underruns the allowance did not foresee.
    margin: u64,
    /// Input frames kept for datagrams arriving late, from the worst recent lateness worth
    /// covering.
    allowance: u64,
    /// Output frames played since the last underrun or margin decay.
    calm_frames: u64,
    rate_adjust: f64,
    /// The least an arriving datagram would have found buffered ahead of it in the current
    /// window, had it come on time.
    window_min: Option<i64>,
    /// Output frames pulled in the current window.
    window_pulled: u64,
    /// Playback gain, 0 to 1, which moves by at most a fade step per output frame.
    gain: f32,
    /// Input frames to drop once playback has faded to silence.
    pending_skip: u64,
}

impl Control {
    fn new(tuning: Tuning) -> Self {
        Control {
            margin: tuning.min_margin,
            allowance: 0,
            tuning,
            playing: false,
            calm_frames: 0,
            rate_adjust: 0.0,
            window_min: None,
            window_pulled: 0,
            gain: 0.0,
            pending_skip: 0,
        }
    }

    /// What the controller keeps buffered ahead of an on-time datagram, in input frames.
    fn target(&self) -> u64 {
        (self.margin + self.allowance).min(self.tuning.max_target)
    }

    /// Raises the allowance at once to `cover`, the worst recent lateness worth covering, in
    /// input frames.
    fn cover(&mut self, cover: u64) {
        self.allowance = self.allowance.max(cover);
    }

    /// A datagram arrived that would have found `on_time_ahead` input frames buffered ahead of
    /// it had it come on time.
    fn arrived(&mut self, on_time_ahead: i64) {
        if self.playing {
            self.window_min = Some(
                self.window_min
                    .map_or(on_time_ahead, |m| m.min(on_time_ahead)),
            );
        }
    }

    /// Whether to play this period. Once stopped, playback restarts when `level` reaches one
    /// block, one output period and the target, and drops anything beyond that first: what
    /// piled up while stopped would only add latency. The target is steered as the level a
    /// datagram finds, which a pull has just lowered, so before one it needs a period more.
    fn should_play(&mut self, level: u64) -> bool {
        let start = self.tuning.block + self.tuning.period + self.target();
        if !self.playing && level >= start {
            self.playing = true;
            self.pending_skip = level - start;
            self.window_min = None;
            self.window_pulled = 0;
        }
        self.playing
    }

    /// Input frames per output frame.
    fn step(&self) -> f64 {
        self.tuning.step * (1.0 + self.rate_adjust)
    }

    /// The buffer ran dry: a block arrived later than the target covers, so back off.
    fn underrun(&mut self) {
        let t = &self.tuning;
        self.playing = false;
        self.gain = 0.0;
        self.pending_skip = 0;
        self.calm_frames = 0;
        self.margin = (self.margin + t.backoff).min(t.max_target);
    }

    /// Counts `frames` output frames pulled. At the end of each window, lets the allowance fall
    /// toward `cover`, the worst recent lateness worth covering in input frames, steers, and
    /// returns true.
    fn after_pull(&mut self, frames: u64, cover: u64) -> bool {
        let t = &self.tuning;
        self.window_pulled += frames;
        if self.playing {
            // A transient spike grows the margin on underrun; win it back once playback has
            // been clean for a while.
            self.calm_frames += frames;
            if self.calm_frames >= t.calm_after && self.margin > t.min_margin {
                self.margin = self.margin.saturating_sub(t.decay_step).max(t.min_margin);
                self.calm_frames = 0;
            }
        }
        if self.window_pulled < t.window_frames {
            return false;
        }
        // Up at once in `cover`, down gently here.
        self.allowance = cover.max(self.allowance.saturating_sub(t.allowance_decay));
        match (self.playing, self.window_min) {
            (true, Some(min)) => self.pending_skip = self.steer(min),
            // No arrivals to steer by: play at the nominal rate and let underruns deal with
            // the silence.
            (true, None) => self.rate_adjust = 0.0,
            (false, _) => {}
        }
        self.window_min = None;
        self.window_pulled = 0;
        true
    }

    /// Sets the playback rate from the window's minimum on-time figure and returns frames to
    /// skip. Within [target, target + quantum] the minimum moves with arrival phase, not with
    /// the true level, so that band is on target.
    fn steer(&mut self, window_min: i64) -> u64 {
        let t = &self.tuning;
        let excess = window_min - self.target() as i64;
        let quantum = t.quantum as i64;
        let mut skip = 0;
        self.rate_adjust = if excess > t.skip_threshold as i64 + quantum {
            skip = (excess - quantum / 2) as u64;
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
        skip
    }
}

/// Constants of the control law for one format and output, in input frames unless noted.
#[derive(Debug, Clone, Copy)]
struct Tuning {
    in_rate: f64,
    /// Input frames per output frame at nominal rate.
    step: f64,
    out_channels: usize,
    block: u64,
    /// One output period.
    period: u64,
    /// How far the measured minimum moves with arrival phase alone: min(block, output
    /// period). It is part of the margin and the controller's deadband.
    quantum: u64,
    min_margin: u64,
    /// The most the target, and the margin within it, may grow to: [`MAX_COVER_MS`] beyond the
    /// deadband, or the configured margin if that is more.
    max_target: u64,
    /// How far the allowance may fall per window.
    allowance_decay: u64,
    backoff: u64,
    decay_step: u64,
    skip_threshold: u64,
    /// In output frames.
    window_frames: u64,
    /// In output frames.
    calm_after: u64,
    /// Gain change per output frame while fading out, and while fading in.
    fade_out_step: f32,
    fade_in_step: f32,
    /// Below this level playback fades out: enough for the whole fade at the fastest rate,
    /// plus the two frames interpolation reads.
    reserve: u64,
}

impl Tuning {
    fn new(format: Format, output: OutputFormat, margin_ms: f64) -> Self {
        let in_rate = f64::from(format.sample_rate);
        let out_rate = f64::from(output.sample_rate);
        let frames = |ms: f64| (ms * in_rate / 1000.0) as u64;
        let quantum_ms = format.block_ms().min(output.period_ms());
        let step = in_rate / out_rate;
        let out_frames = |ms: f64| (ms * out_rate / 1000.0).max(1.0);
        let reserve = (out_frames(FADE_OUT_MS) * step * (1.0 + MAX_RATE_ADJUST)).ceil() as u64 + 2;
        let min_margin = frames(margin_ms + quantum_ms).max(reserve);
        Tuning {
            in_rate,
            step,
            out_channels: usize::from(output.channels.max(1)),
            block: u64::from(format.block_frames),
            period: (f64::from(output.period_frames) * step).ceil() as u64,
            quantum: frames(quantum_ms),
            min_margin,
            max_target: frames(MAX_COVER_MS + quantum_ms).max(min_margin),
            allowance_decay: frames(ALLOWANCE_DECAY_MS_PER_S * WINDOW_S),
            backoff: frames(UNDERRUN_BACKOFF_MS),
            decay_step: frames(MARGIN_DECAY_STEP_MS),
            skip_threshold: frames(SKIP_THRESHOLD_MS),
            window_frames: (out_rate * WINDOW_S) as u64,
            calm_after: (out_rate * MARGIN_DECAY_AFTER_S) as u64,
            fade_out_step: 1.0 / out_frames(FADE_OUT_MS) as f32,
            fade_in_step: 1.0 / out_frames(FADE_IN_MS) as f32,
            reserve,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

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

    const MARGIN_MS: f64 = 1.5;

    fn playout(block_frames: u32, period_frames: u32) -> Playout {
        let mut p = Playout::new(MARGIN_MS);
        p.set_format(format(block_frames)).unwrap();
        p.set_output(output(48000, 2, period_frames));
        p
    }

    #[test]
    fn silent_until_format_and_output_are_known() {
        let mut p = Playout::new(MARGIN_MS);
        p.push(Instant::now(), 0, &pcm(960, 1000));
        let mut out = [1.0; 64];
        p.pull(&mut out);
        assert!(out.iter().all(|&s| s == 0.0));
        assert_eq!(p.stats().state, PlayoutState::Idle);
    }

    #[test]
    fn buffers_a_block_a_period_and_the_margin_then_plays_the_stream() {
        let mut p = playout(96, 96);
        // Margin = 1.5 ms + min(block, period) = 3.5 ms = 168 frames; prime to 96 + 96 + 168.
        p.push(Instant::now(), 0, &pcm(300, 16384));
        let mut out = vec![1.0; 2 * 32];
        p.pull(&mut out);
        assert!(out.iter().all(|&s| s == 0.0));
        assert_eq!(p.stats().state, PlayoutState::Buffering);

        p.push(Instant::now(), 300, &pcm(100, 16384));
        p.pull(&mut out);
        // Fading in.
        assert!(out[0] > 0.0 && out[0] < out[out.len() - 1] && out[out.len() - 1] <= 0.5);
        assert_eq!(p.stats().state, PlayoutState::Playing);
    }

    #[test]
    fn extra_output_channels_are_silent() {
        let mut p = Playout::new(MARGIN_MS);
        p.set_format(format(96)).unwrap();
        p.set_output(output(48000, 4, 96));
        p.push(Instant::now(), 0, &pcm(400, 16384));
        let mut out = vec![1.0; 4 * 8];
        p.pull(&mut out);
        for frame in out.chunks_exact(4) {
            assert!(frame[0] > 0.0);
            assert_eq!(frame[0], frame[1]);
            assert_eq!(frame[2..], [0.0, 0.0]);
        }
    }

    /// Stereo frames from `first` of a triangle wave at half scale, so any jump in position
    /// shows as a step between samples.
    fn triangle(first: u64, frames: usize) -> Vec<u8> {
        const PERIOD: u64 = 997;
        (first..first + frames as u64)
            .flat_map(|i| {
                let phase = (i % PERIOD) as f64 / PERIOD as f64;
                let v = (16384.0 * (1.0 - 4.0 * (phase - 0.5).abs())) as i16;
                [v, v]
            })
            .flat_map(i16::to_le_bytes)
            .collect()
    }

    /// The largest change between consecutive output samples of the same channel.
    fn largest_step(out: &[f32]) -> f32 {
        out.chunks_exact(2)
            .zip(out.chunks_exact(2).skip(1))
            .map(|(a, b)| (a[0] - b[0]).abs().max((a[1] - b[1]).abs()))
            .fold(0.0, f32::max)
    }

    /// The most a sample may move when fading: a 1 ms fade at half scale moves it by 0.5 / 48,
    /// on top of the triangle wave's own slope.
    const FADE_STEP: f32 = 0.5 / 48.0 + 4.0 * 0.5 / 997.0 + 1e-4;

    #[test]
    fn fades_in_when_playback_starts() {
        let mut p = playout(96, 96);
        p.push(Instant::now(), 0, &pcm(400, 16384));
        let mut out = vec![0.0; 2 * 200];
        p.pull(&mut out);
        assert!(out[0] > 0.0 && out[0] < 0.02, "{}", out[0]);
        assert_eq!(out[out.len() - 1], 0.5);
        assert!(largest_step(&out) <= FADE_STEP);
    }

    #[test]
    fn fades_out_before_running_dry() {
        let mut p = playout(96, 96);
        p.push(Instant::now(), 0, &pcm(400, 16384));
        let mut played = Vec::new();
        let mut out = vec![0.0; 2 * 32];
        while p.stats().underruns == 0 {
            p.pull(&mut out);
            played.extend_from_slice(&out);
        }
        assert!(played.contains(&0.5));
        assert!(largest_step(&played) <= FADE_STEP);
        assert_eq!(p.stats().state, PlayoutState::Buffering);
    }

    #[test]
    fn resumes_after_a_stall_at_the_target_instead_of_behind() {
        let mut p = playout(96, 96);
        p.push(Instant::now(), 0, &pcm(400, 16384));
        let mut out = vec![0.0; 2 * 32];
        while p.stats().underruns == 0 {
            p.pull(&mut out);
        }
        // The stall's datagrams arrive together: 30 ms.
        p.push(Instant::now(), 400, &pcm(1440, 16384));
        p.pull(&mut out);
        let control = p.control.as_ref().unwrap();
        let target = control.tuning.block + control.tuning.period + control.target();
        assert!(p.ring.level() < target, "{} buffered", p.ring.level());
        assert!(p.stats().skipped_ms > 20.0);
    }

    #[test]
    fn a_lost_datagram_fades_around_its_gap() {
        let mut p = playout(96, 96);
        p.push(Instant::now(), 0, &triangle(0, 400));
        let mut out = vec![0.0; 2 * 32];
        p.pull(&mut out);
        let mut played = out.clone();
        // Frames 400..740 are lost.
        p.push(Instant::now(), 740, &triangle(740, 340));
        while p.stats().underruns == 0 {
            p.pull(&mut out);
            played.extend_from_slice(&out);
        }
        assert!(largest_step(&played) <= FADE_STEP);
        let silent = played.windows(2 * 300).any(|w| w.iter().all(|&s| s == 0.0));
        assert!(silent, "the gap plays as silence");
    }

    #[test]
    fn the_fade_after_a_gap_carries_on_into_the_next_datagram() {
        let mut p = playout(96, 96);
        // 400-frame blocks go out as 340 + 60 frames.
        p.push(Instant::now(), 0, &triangle(0, 400));
        let mut out = vec![0.0; 2 * 32];
        p.pull(&mut out);
        let mut played = out.clone();
        // Frames 400..740 are lost; the 60 after them are shorter than the fade-in.
        for (first, frames) in [(740, 60), (800, 340), (1140, 60)] {
            p.push(Instant::now(), first, &triangle(first, frames));
        }
        while p.stats().underruns == 0 {
            p.pull(&mut out);
            played.extend_from_slice(&out);
        }
        assert!(largest_step(&played) <= FADE_STEP);
    }

    #[test]
    fn a_restarted_capture_fades_across_the_join() {
        let mut p = playout(96, 96);
        p.push(Instant::now(), 1_000_000, &triangle(1_000_000, 400));
        let mut out = vec![0.0; 2 * 32];
        p.pull(&mut out);
        let mut played = out.clone();
        p.push(Instant::now(), 0, &triangle(0, 400));
        while p.stats().underruns == 0 {
            p.pull(&mut out);
            played.extend_from_slice(&out);
        }
        assert!(largest_step(&played) <= FADE_STEP);
    }

    /// Blocks arriving exactly on time into a device whose period is longer than a block plus
    /// the margin never run dry, from the very start.
    #[test]
    fn starts_without_underruns_when_a_period_outlasts_a_block() {
        for (block, period) in [(96, 480), (96, 336), (192, 1024)] {
            let mut p = playout(block, period);
            let input = pcm(block as usize, 1000);
            let mut out = vec![0.0; period as usize * 2];
            let (block_us, period_us) = (f64::from(block) / 0.048, f64::from(period) / 0.048);
            let (mut blocks, mut pulls) = (0u64, 0u64);
            while (pulls as f64) * period_us < 10e6 {
                if blocks as f64 * block_us <= pulls as f64 * period_us {
                    p.push(Instant::now(), blocks * u64::from(block), &input);
                    blocks += 1;
                } else {
                    p.pull(&mut out);
                    pulls += 1;
                }
            }
            assert_eq!(p.stats().underruns, 0, "block {block}, period {period}");
        }
    }

    #[test]
    fn a_skip_dips_instead_of_jumping() {
        let mut p = playout(96, 96);
        let mut position = 0;
        let mut played = Vec::new();
        let mut out = vec![0.0; 2 * 96];
        for tick in 0..1000 {
            // 50 ms arrives at once, more than the controller sheds by rate.
            let frames = if tick == 300 { 96 + 2400 } else { 96 };
            p.push(Instant::now(), position, &triangle(position, frames));
            position += frames as u64;
            p.pull(&mut out);
            played.extend_from_slice(&out);
        }
        assert!(p.stats().skipped_ms > 20.0);
        assert_eq!(p.stats().underruns, 0);
        assert!(largest_step(&played) <= FADE_STEP);
    }

    #[test]
    fn a_lost_datagram_becomes_silence_of_its_length() {
        let mut p = playout(96, 96);
        p.push(Instant::now(), 0, &pcm(340, 1000));
        p.push(Instant::now(), 680, &pcm(340, 1000));
        assert_eq!(p.ring.level(), 1020);
        assert_eq!(p.stats().lost_ms, 340.0 * 1000.0 / 48000.0);
    }

    #[test]
    fn late_and_duplicate_datagrams_add_only_unwritten_frames() {
        let mut p = playout(96, 96);
        p.push(Instant::now(), 0, &pcm(96, 1000));
        p.push(Instant::now(), 96, &pcm(96, 1000));
        p.push(Instant::now(), 0, &pcm(96, 1000));
        assert_eq!(p.ring.level(), 192);
        p.push(Instant::now(), 144, &pcm(96, 1000));
        assert_eq!(p.ring.level(), 240);
        assert_eq!(p.stats().lost_ms, 0.0);
    }

    #[test]
    fn a_restarted_capture_resynchronizes_without_silence_or_jitter() {
        let mut p = playout(96, 96);
        let start = Instant::now();
        let at = |ms: f64| start + Duration::from_secs_f64(ms / 1000.0);
        p.push(at(0.0), 1_000_000, &pcm(96, 1000));
        p.push(at(2.0), 1_000_096, &pcm(96, 1000));
        p.push(at(4.0), 0, &pcm(96, 1000));
        p.push(at(6.0), 96, &pcm(96, 1000));
        assert_eq!(p.ring.level(), 384);
        assert_eq!(p.stats().lost_ms, 0.0);
        assert!(p.stats().jitter_peak_ms < 1e-3);
    }

    #[test]
    fn a_reordered_datagram_counts_as_late() {
        let mut p = playout(96, 96);
        let start = Instant::now();
        let at = |ms: f64| start + Duration::from_secs_f64(ms / 1000.0);
        p.push(at(0.0), 0, &pcm(96, 1000));
        p.push(at(4.0), 192, &pcm(96, 1000));
        // Block 1 was due at 2 ms and its frames are concealed already.
        p.push(at(5.0), 96, &pcm(96, 1000));
        assert_eq!(p.ring.level(), 288);
        assert!((p.stats().jitter_peak_ms - 3.0).abs() < 1e-6);
    }

    #[test]
    fn a_late_datagram_shows_as_jitter() {
        let mut p = playout(96, 96);
        let start = Instant::now();
        let at = |ms: f64| start + Duration::from_secs_f64(ms / 1000.0);
        for i in 0..10 {
            p.push(at(2.0 * i as f64), i * 96, &pcm(96, 1000));
        }
        // Block 10 was due at 20 ms.
        p.push(at(27.0), 960, &pcm(96, 1000));
        assert!((p.stats().jitter_peak_ms - 7.0).abs() < 1e-6);
        // The stats window shows it once the window closes.
        let mut out = vec![0.0; 2 * 96];
        for _ in 0..(48000.0 * WINDOW_S / 96.0).ceil() as usize {
            p.pull(&mut out);
        }
        assert!(
            (p.stats().jitter_ms - 7.0).abs() < 1e-6,
            "{}",
            p.stats().jitter_ms
        );
    }

    #[test]
    fn repeated_format_keeps_the_buffer_and_a_new_one_clears_it() {
        let mut p = playout(96, 96);
        p.push(Instant::now(), 0, &pcm(96, 1000));
        p.set_format(format(96)).unwrap();
        assert_eq!(p.ring.level(), 96);
        p.set_format(format(192)).unwrap();
        assert_eq!(p.ring.level(), 0);
    }

    #[test]
    fn only_16_bit_pcm_is_supported() {
        let mut p = Playout::new(MARGIN_MS);
        let wide = Format {
            bytes_per_sample: 3,
            ..format(96)
        };
        assert_eq!(p.set_format(wide), Err(UnsupportedFormat(wide)));
    }

    /// Streams blocks of `p`'s format into its output, which pulls on a clock 100 ppm fast,
    /// for `seconds`. Block `i` arrives `late(i)` µs after it was captured, and never before
    /// the block ahead of it. Calls `each_second` with the playout every second.
    fn stream(
        p: &mut Playout,
        seconds: u64,
        mut late: impl FnMut(u64) -> f64,
        mut each_second: impl FnMut(u64, &Playout),
    ) {
        let (block, output) = (p.format.unwrap().block_frames, p.output.unwrap());
        let block_us = f64::from(block) * 1e6 / 48000.0;
        let period_us = f64::from(output.period_frames) * 1e6 / f64::from(output.sample_rate);
        let period_us = period_us / 1.0001;
        let input = pcm(block as usize, 1000);
        let mut out = vec![0.0; output.period_frames as usize * usize::from(output.channels)];
        let start = Instant::now();
        let (mut blocks, mut next_block, mut next_pull, mut second) = (0u64, 0.0f64, 0.0, 1);
        while next_pull < seconds as f64 * 1e6 {
            if next_block <= next_pull {
                let at = start + Duration::from_secs_f64(next_block / 1e6);
                p.push(at, blocks * u64::from(block), &input);
                blocks += 1;
                next_block = next_block.max(blocks as f64 * block_us + late(blocks));
            } else {
                p.pull(&mut out);
                next_pull += period_us;
                if next_pull >= second as f64 * 1e6 {
                    each_second(second, p);
                    second += 1;
                }
            }
        }
    }

    /// A xorshift generator of uniform numbers in [0, 1).
    fn uniform() -> impl FnMut() -> f64 {
        let mut x = 0x2545_f491_4f6c_dd1d_u64;
        move || {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            (x >> 11) as f64 / (1u64 << 53) as f64
        }
    }

    #[test]
    fn rides_out_recurring_stalls_once_it_has_seen_one() {
        let mut p = playout(96, 128);
        let mut rng = uniform();
        // Up to 3 ms of jitter, and a stall every 10 s: 35 ms first, then 15-35 ms.
        let late = |i: u64| {
            let stall = match i {
                2500 => 35e3,
                _ if i % 5000 == 2500 => 15e3 + 20e3 * rng(),
                _ => 0.0,
            };
            3e3 * rng() + stall
        };
        stream(&mut p, 180, late, |_, _| {});
        let stats = p.stats();
        assert_eq!(stats.underruns, 1, "only the first stall: {stats:?}");
        assert!(stats.buffer_min_ms < 45.0, "{stats:?}");
    }

    #[test]
    fn covers_a_second_stall_soon_after_the_first() {
        let mut p = playout(96, 128);
        let mut rng = uniform();
        // 30 ms stalls at 10 s and 12 s.
        let late = |i: u64| 1e3 * rng() + if i == 5000 || i == 6000 { 30e3 } else { 0.0 };
        stream(&mut p, 20, late, |_, _| {});
        assert_eq!(p.stats().underruns, 1);
    }

    #[test]
    fn does_not_buffer_for_stalls_too_long_to_cover() {
        let mut p = playout(96, 128);
        let mut rng = uniform();
        // A 60 ms stall every 10 s.
        let late = |i: u64| 1e3 * rng() + if i % 5000 == 2500 { 60e3 } else { 0.0 };
        let mut most = 0;
        stream(&mut p, 120, late, |_, p| {
            most = most.max(p.control.as_ref().unwrap().allowance);
        });
        let most_ms = most as f64 / 48.0;
        assert!(most_ms < 1.5, "allowance reached {most_ms} ms");
        assert!(p.stats().jitter_peak_ms > 50.0, "the stalls are measured");
    }

    #[test]
    fn sheds_a_forgotten_stall_without_skipping() {
        let mut p = playout(96, 128);
        let mut rng = uniform();
        let late = |i: u64| 1e3 * rng() + if i == 2500 { 30e3 } else { 0.0 };
        let (mut at_10_s, mut at_60_s) = (None, None);
        stream(&mut p, 240, late, |second, p| match second {
            10 => at_10_s = Some(p.stats()),
            60 => at_60_s = Some(p.stats()),
            _ => {}
        });
        let (at_10_s, at_60_s, end) = (at_10_s.unwrap(), at_60_s.unwrap(), p.stats());
        assert!(at_60_s.target_ms > 30.0, "covers the stall: {at_60_s:?}");
        // Back to the margin, the 2 ms deadband and the everyday jitter.
        let settled = MARGIN_MS + 2.0 + end.jitter_peak_ms;
        assert!(end.target_ms < settled + 0.1, "forgets it: {end:?}");
        assert_eq!(end.skipped_ms, at_10_s.skipped_ms);
        assert_eq!(end.underruns, at_10_s.underruns);
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
        let mut p = Playout::new(MARGIN_MS);
        p.set_format(format(block)).unwrap();
        p.set_output(output(out_rate, 2, period));
        let block_us = f64::from(block) * 1e6 / 48000.0;
        let period_us = f64::from(period) * 1e6 / f64::from(out_rate);
        let quantum_ms = block_us.min(period_us) / 1000.0;
        let ceiling_ms = MARGIN_MS + 2.0 * quantum_ms + jitter_us / 1000.0 + 3.0;
        let mut rng = uniform();
        let (mut settled_underruns, mut late_windows) = (None, 0);
        stream(
            &mut p,
            60,
            |_| rng() * jitter_us,
            |second, p| {
                let stats = p.stats();
                if second == 5 {
                    settled_underruns = Some(stats.underruns);
                }
                if second > 5 && stats.buffer_min_ms > ceiling_ms {
                    late_windows += 1;
                }
            },
        );
        let stats = p.stats();
        let underruns = stats.underruns - settled_underruns.unwrap();
        assert_eq!(underruns, 0, "{name}: underruns after settling");
        assert_eq!(
            late_windows, 0,
            "{name}: buffer minimum stayed above {ceiling_ms:.1} ms"
        );
        assert_eq!(stats.state, PlayoutState::Playing, "{name}");
        let jitter_ms = jitter_us / 1000.0;
        assert!(
            stats.jitter_peak_ms > jitter_ms * 0.8 && stats.jitter_peak_ms < jitter_ms + 0.5,
            "{name}: jitter peak {:.2} ms for {jitter_ms} ms of jitter",
            stats.jitter_peak_ms
        );
    }
}
