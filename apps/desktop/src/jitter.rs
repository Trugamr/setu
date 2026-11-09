//! Jitter: how late each datagram arrives, measured against the stream's own clock.
//!
//! The headset sends each capture block as soon as it has it, so on a perfect network the
//! datagram whose first frame is at stream time `s` would arrive a fixed delay after `s`. Its
//! *transit*, arrival time minus `s`, is that fixed delay plus however late this datagram was.
//! The lowest transit of the last [`FLOOR_SLOTS`] × [`FLOOR_SLOT_S`] seconds stands in for the
//! fixed delay; a datagram's *lateness* is its transit above that floor.
//!
//! The floor's span is short so that drift between the headset's and the PC's clocks barely
//! inflates lateness: by at most the drift over two seconds, 0.6 ms at 300 ppm. Lateness sits
//! on the safe side of that error, as the floor can only be too low.
//!
//! A datagram more than [`OUTAGE_S`] late is not jitter a buffer should cover. It ends an outage
//! or marks a step in the stream's timing, such as the headset dropping captured frames or its
//! capture restarting from position 0, either of which raises every later transit. The floor
//! restarts from it rather than counting it. A step the other way lowers the floor at once.
//!
//! [`Jitter::peak`] is the worst lateness over the last [`PEAK_SLOTS`] × [`PEAK_SLOT_S`] seconds
//! of the stream: how much buffer would have ridden out every delay that recent.

use std::time::Instant;

const FLOOR_SLOT_S: f64 = 0.25;
const FLOOR_SLOTS: usize = 8;
const PEAK_SLOT_S: f64 = 1.0;
const PEAK_SLOTS: usize = 120;
const OUTAGE_S: f64 = 0.1;

pub struct Jitter {
    /// Arrival times are measured from here.
    origin: Option<Instant>,
    /// Negated transits, so their maximum is the floor.
    floor: RollingMax<FLOOR_SLOTS>,
    peak: RollingMax<PEAK_SLOTS>,
}

impl Jitter {
    pub fn new() -> Self {
        Jitter {
            origin: None,
            floor: RollingMax::new(FLOOR_SLOT_S),
            peak: RollingMax::new(PEAK_SLOT_S),
        }
    }

    /// Records a datagram that arrived `at`, whose first frame is `stream_s` seconds into the
    /// stream. Returns its lateness in seconds: 0 for one more than [`OUTAGE_S`] late, which
    /// the floor restarts from.
    pub fn arrive(&mut self, at: Instant, stream_s: f64) -> f64 {
        let origin = *self.origin.get_or_insert(at);
        let now = at.saturating_duration_since(origin).as_secs_f64();
        let transit = now - stream_s;
        self.floor.add(now, -transit);
        let mut late = transit + self.floor.max();
        if late > OUTAGE_S {
            self.floor.clear();
            self.floor.add(now, -transit);
            late = 0.0;
        }
        self.peak.add(now, late);
        late
    }

    /// The worst lateness over the last two minutes of the stream, in seconds; 0 before any
    /// datagram.
    pub fn peak(&self) -> f64 {
        self.peak.max().max(0.0)
    }
}

impl Default for Jitter {
    fn default() -> Self {
        Self::new()
    }
}

/// The largest value added over the last `N` slots of time.
struct RollingMax<const N: usize> {
    slot_s: f64,
    values: [f64; N],
    /// The slot the newest value went into, counted from time 0.
    newest: Option<u64>,
}

impl<const N: usize> RollingMax<N> {
    fn new(slot_s: f64) -> Self {
        RollingMax {
            slot_s,
            values: [f64::NEG_INFINITY; N],
            newest: None,
        }
    }

    /// Adds `value` at `now` seconds, which never goes backwards.
    fn add(&mut self, now: f64, value: f64) {
        let slot = (now / self.slot_s) as u64;
        let slot = match self.newest {
            Some(newest) if slot > newest => {
                // Empty the slots time has moved past, at most the whole ring.
                let first = (newest + 1).max(slot.saturating_sub(N as u64 - 1));
                for s in first..=slot {
                    self.values[(s % N as u64) as usize] = f64::NEG_INFINITY;
                }
                slot
            }
            Some(newest) => newest,
            None => slot,
        };
        self.newest = Some(slot);
        let v = &mut self.values[(slot % N as u64) as usize];
        *v = v.max(value);
    }

    fn max(&self) -> f64 {
        self.values
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max)
    }

    fn clear(&mut self) {
        self.values = [f64::NEG_INFINITY; N];
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    const BLOCK_S: f64 = 0.002;

    /// Feeds `blocks` 2 ms blocks from `first`, each arriving `delay(i)` seconds after it was
    /// captured on a headset clock that runs `ppm` slow, and returns each one's lateness.
    fn feed(
        jitter: &mut Jitter,
        start: Instant,
        first: u64,
        blocks: u64,
        ppm: f64,
        delay: impl Fn(u64) -> f64,
    ) -> Vec<f64> {
        (first..first + blocks)
            .map(|i| {
                let stream_s = i as f64 * BLOCK_S;
                let sent = stream_s * (1.0 + ppm * 1e-6);
                let at = start + Duration::from_secs_f64(sent + 0.003 + delay(i));
                jitter.arrive(at, stream_s)
            })
            .collect()
    }

    #[test]
    fn a_steady_stream_is_never_late() {
        let mut j = Jitter::new();
        let late = feed(&mut j, Instant::now(), 0, 1000, 0.0, |_| 0.0);
        assert!(late.iter().all(|&l| l.abs() < 1e-6));
        assert!(j.peak() < 1e-6);
    }

    #[test]
    fn a_late_datagram_counts_against_the_on_time_ones() {
        let mut j = Jitter::new();
        let late = feed(&mut j, Instant::now(), 0, 1000, 0.0, |i| match i {
            500 => 0.012,
            501 => 0.010,
            _ => 0.0,
        });
        assert!((late[500] - 0.012).abs() < 1e-6, "{}", late[500]);
        assert!((late[501] - 0.010).abs() < 1e-6);
        assert!(late[502].abs() < 1e-6);
        assert!((j.peak() - 0.012).abs() < 1e-6);
    }

    #[test]
    fn the_first_datagram_being_late_does_not_count_later_ones_early() {
        let mut j = Jitter::new();
        let late = feed(&mut j, Instant::now(), 0, 100, 0.0, |i| {
            if i == 0 { 0.020 } else { 0.0 }
        });
        assert!(late.iter().all(|&l| l >= 0.0));
        assert!(late[1..].iter().all(|&l| l.abs() < 1e-6));
    }

    #[test]
    fn the_peak_is_forgotten_after_two_minutes() {
        let mut j = Jitter::new();
        let start = Instant::now();
        feed(&mut j, start, 0, 1, 0.0, |_| 0.0);
        feed(&mut j, start, 1, 1, 0.0, |_| 0.015);
        let blocks_per_s = (1.0 / BLOCK_S) as u64;
        feed(&mut j, start, 2, 100 * blocks_per_s, 0.0, |_| 0.0);
        assert!((j.peak() - 0.015).abs() < 1e-6, "still within two minutes");
        feed(
            &mut j,
            start,
            2 + 100 * blocks_per_s,
            25 * blocks_per_s,
            0.0,
            |_| 0.0,
        );
        assert!(j.peak() < 1e-6, "{}", j.peak());
    }

    #[test]
    fn clock_drift_adds_at_most_two_seconds_of_it() {
        for ppm in [-300.0, 300.0] {
            let mut j = Jitter::new();
            let blocks_per_s = (1.0 / BLOCK_S) as u64;
            feed(&mut j, Instant::now(), 0, 60 * blocks_per_s, ppm, |_| 0.0);
            let bound = ppm.max(0.0) * 1e-6 * FLOOR_SLOTS as f64 * FLOOR_SLOT_S;
            assert!(j.peak() <= bound + 1e-6, "{ppm} ppm: peak {}", j.peak());
        }
    }

    #[test]
    fn an_outage_is_not_jitter() {
        let mut j = Jitter::new();
        // 300 ms without datagrams, then the backlog in one burst.
        let late = feed(&mut j, Instant::now(), 0, 1000, 0.0, |i| match i {
            500..650 => (650 - i) as f64 * BLOCK_S,
            _ => 0.0,
        });
        assert_eq!(late[500], 0.0);
        assert!(late.iter().all(|&l| l <= OUTAGE_S));
        assert!(j.peak() <= OUTAGE_S);
    }

    #[test]
    fn a_step_in_the_stream_clock_is_adopted_rather_than_counted() {
        let mut j = Jitter::new();
        // The headset drops 0.5 s of captured frames: every later transit is 0.5 s higher.
        let late = feed(&mut j, Instant::now(), 0, 1000, 0.0, |i| {
            if i >= 500 { 0.5 } else { 0.0 }
        });
        assert!(late.iter().all(|&l| l.abs() < 1e-6));
        assert!(j.peak() < 1e-6);
    }

    #[test]
    fn a_restarted_stream_is_not_late() {
        let mut j = Jitter::new();
        let start = Instant::now();
        feed(&mut j, start, 0, 1000, 0.0, |_| 0.0);
        // A new capture numbers from 0 again, two seconds of stream time later.
        let at = |s: f64| start + Duration::from_secs_f64(1000.0 * BLOCK_S + 0.003 + s);
        assert_eq!(j.arrive(at(0.0), 0.0), 0.0);
        assert!(j.arrive(at(BLOCK_S), BLOCK_S).abs() < 1e-6);
        assert!(j.peak() < 1e-6);
    }

    #[test]
    fn a_stream_that_jumps_ahead_is_not_late() {
        let mut j = Jitter::new();
        let start = Instant::now();
        feed(&mut j, start, 0, 1000, 0.0, |_| 0.0);
        let late = feed(
            &mut j,
            start + Duration::from_secs(1),
            1000 + 500 * 60,
            100,
            0.0,
            |_| 0.0,
        );
        assert!(late.iter().all(|&l| l.abs() < 1e-6));
    }
}
