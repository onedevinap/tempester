//! Euclidean and probability sequencer for the Play page: each of the 16 pads has its own pattern, of `steps` 16th
//! notes with `pulses` hits spread as evenly as possible (Bjorklund), turned by `rotation` steps, each hit played with
//! a `probability`. Each pad plays the note the machine plays that pad with (pads A1-A16 of the app's pad note table,
//! `tempest_proto::pad_notes`) on the chosen channel, like the on-screen pads.
//!
//! It runs on the app clock thread (`play::run_clock`): the clock calls `Sequencer::on_clock` right after each MIDI
//! clock byte and sends what it returns, so the notes sit on the clock's own accumulating schedule and the UI thread
//! does no timing work. The settings are shared with the worker through a mutex held only to copy a few bytes.
//! A step is 6 clocks (a 16th note); a note lasts 3 clocks. Note-offs always go out: each sounding note is
//! remembered with its own channel, and `release_all` ends them all when the clock stops.
//!
//! The tempo follows the app clock only: the original OS does not start from an incoming clock on its own, and the
//! app has no incoming-clock follower (`refs/firmware-main.md` 6).

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use tempest_proto::pad_notes::PadNotes;

pub const PADS: usize = 16;
pub const MAX_STEPS: u8 = 32;
/// Clock bytes per step: 24 per quarter note, a step is a 16th.
const CLOCKS_PER_STEP: u64 = 6;
/// Clock bytes a note lasts: half a step, so it always ends before the pad's next step.
const GATE_CLOCKS: u64 = 3;

/// One pad's pattern, always valid (`new` is the only way in).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PadPattern {
    steps: u8,
    pulses: u8,
    rotation: u8,
    probability: u8,
}

impl PadPattern {
    /// `steps` 1-32, `pulses` at most `steps`, `rotation` below `steps`, `probability` 0-100 (percent).
    pub fn new(steps: u8, pulses: u8, rotation: u8, probability: u8) -> Result<Self, String> {
        if !(1..=MAX_STEPS).contains(&steps) {
            return Err(format!("steps must be 1 to {MAX_STEPS}, not {steps}"));
        }
        if pulses > steps {
            return Err(format!("pulses ({pulses}) cannot exceed steps ({steps})"));
        }
        if rotation >= steps {
            return Err(format!(
                "rotation ({rotation}) must be below steps ({steps})"
            ));
        }
        if probability > 100 {
            return Err(format!("probability must be 0 to 100, not {probability}"));
        }
        Ok(Self {
            steps,
            pulses,
            rotation,
            probability,
        })
    }

    /// A pad that plays nothing: 16 steps, no pulse.
    pub fn silent() -> Self {
        Self {
            steps: 16,
            pulses: 0,
            rotation: 0,
            probability: 100,
        }
    }

    pub fn steps(&self) -> u8 {
        self.steps
    }

    pub fn pulses(&self) -> u8 {
        self.pulses
    }

    pub fn rotation(&self) -> u8 {
        self.rotation
    }

    pub fn probability(&self) -> u8 {
        self.probability
    }

    /// The hits of one cycle: `euclid(steps, pulses)` delayed by `rotation` steps (step `i` of the result is step
    /// `i - rotation` of the plain pattern).
    pub fn cycle(&self) -> Vec<bool> {
        let plain = euclid(usize::from(self.steps), usize::from(self.pulses));
        let (n, r) = (plain.len(), usize::from(self.rotation));
        (0..n).map(|i| plain[(i + n - r) % n]).collect()
    }

    /// The cycle as text, `x` for a hit and `.` for a rest.
    pub fn text(&self) -> String {
        self.cycle()
            .iter()
            .map(|hit| if *hit { 'x' } else { '.' })
            .collect()
    }
}

/// The Euclidean rhythm of `pulses` hits over `steps` steps by Bjorklund's algorithm (Toussaint's form: it starts on
/// a hit, the rests are spread as evenly as possible). `pulses` above `steps` counts as `steps`.
pub fn euclid(steps: usize, pulses: usize) -> Vec<bool> {
    let pulses = pulses.min(steps);
    if pulses == 0 {
        // Nothing to pair the rests with: the loop below would never end.
        return vec![false; steps];
    }
    let mut head: Vec<Vec<bool>> = vec![vec![true]; pulses];
    let mut tail: Vec<Vec<bool>> = vec![vec![false]; steps - pulses];
    while tail.len() > 1 {
        let pairs = head.len().min(tail.len());
        let merged: Vec<Vec<bool>> = head
            .iter()
            .zip(&tail)
            .map(|(a, b)| a.iter().chain(b).copied().collect())
            .collect();
        let rest = if head.len() > tail.len() {
            head[pairs..].to_vec()
        } else {
            tail[pairs..].to_vec()
        };
        (head, tail) = (merged, rest);
    }
    head.into_iter().chain(tail).flatten().collect()
}

/// What the page sets; shared with the clock thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settings {
    pub enabled: bool,
    pub velocity: u8,
    /// 0-15, the channel of the notes.
    pub channel: u8,
    pub pads: [PadPattern; PADS],
    /// The note of each pad A1-A16 (the machine's table, as the app knows it).
    pub notes: [u8; PADS],
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: false,
            velocity: 100,
            channel: 0,
            pads: [PadPattern::silent(); PADS],
            notes: PadNotes::default().bank_a(),
        }
    }
}

/// The settings shared between the worker (which edits them) and the clock thread (which plays them).
#[derive(Debug, Clone, Default)]
pub struct Shared(Arc<Mutex<Settings>>);

impl Shared {
    fn lock(&self) -> MutexGuard<'_, Settings> {
        // Settings are plain values: a panic elsewhere cannot leave them half written.
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn get(&self) -> Settings {
        *self.lock()
    }

    pub fn update(&self, change: impl FnOnce(&mut Settings)) {
        change(&mut self.lock());
    }
}

/// xorshift32: small, seedable, no dependency; the quality needed is "not obviously patterned".
#[derive(Debug, Clone)]
struct Rng(u32);

impl Rng {
    fn new(seed: u32) -> Self {
        Self(if seed == 0 { 0x9E37_79B9 } else { seed })
    }

    fn next(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }

    /// 0-99.
    fn percent(&mut self) -> u8 {
        (self.next() % 100) as u8
    }
}

#[derive(Debug)]
struct Sounding {
    /// The Note Off status byte (`0x80 | channel`) of the channel the note went out on.
    off_status: u8,
    note: u8,
    off_at: u64,
}

/// A note message: status, note, velocity.
pub type Message = [u8; 3];

/// Plays the shared settings, one `on_clock` per MIDI clock byte sent.
pub struct Sequencer {
    shared: Shared,
    rng: Rng,
    clocks: u64,
    sounding: Vec<Sounding>,
    cycles: [Option<(PadPattern, Vec<bool>)>; PADS],
}

impl Sequencer {
    pub fn new(shared: Shared, seed: u32) -> Self {
        Self {
            shared,
            rng: Rng::new(seed),
            clocks: 0,
            sounding: Vec::new(),
            cycles: std::array::from_fn(|_| None),
        }
    }

    /// The messages to send right after clock byte number `clocks` since Start (the first is 0): the Note Offs that
    /// are due, then, on a step boundary, the Note Ons of the pads whose pattern hits and whose probability allows.
    pub fn on_clock(&mut self) -> Vec<Message> {
        let now = self.clocks;
        self.clocks += 1;
        let mut out = Vec::new();
        self.sounding.retain(|note| {
            let due = note.off_at <= now;
            if due {
                out.push([note.off_status, note.note, 0]);
            }
            !due
        });
        if now.is_multiple_of(CLOCKS_PER_STEP) {
            let settings = self.shared.get();
            if settings.enabled {
                self.play_step(&settings, now / CLOCKS_PER_STEP, now, &mut out);
            }
        }
        out
    }

    fn play_step(&mut self, settings: &Settings, step: u64, now: u64, out: &mut Vec<Message>) {
        for (pad, pattern) in settings.pads.iter().enumerate() {
            if pattern.pulses() == 0 || !self.hits(pad, pattern, step) {
                continue;
            }
            if pattern.probability() < 100 && self.rng.percent() >= pattern.probability() {
                continue;
            }
            let note = settings.notes[pad];
            // A note still sounding on this pad ends before it is struck again.
            self.sounding.retain(|s| {
                let same = s.note == note;
                if same {
                    out.push([s.off_status, s.note, 0]);
                }
                !same
            });
            out.push([0x90 | settings.channel, note, settings.velocity]);
            self.sounding.push(Sounding {
                off_status: 0x80 | settings.channel,
                note,
                off_at: now + GATE_CLOCKS,
            });
        }
    }

    fn hits(&mut self, pad: usize, pattern: &PadPattern, step: u64) -> bool {
        let cached = matches!(&self.cycles[pad], Some((known, _)) if known == pattern);
        if !cached {
            self.cycles[pad] = Some((*pattern, pattern.cycle()));
        }
        self.cycles[pad]
            .as_ref()
            .is_some_and(|(_, cycle)| cycle[(step % cycle.len() as u64) as usize])
    }

    /// The Note Offs of every note still sounding (the clock calls it when it stops).
    pub fn release_all(&mut self) -> Vec<Message> {
        self.sounding
            .drain(..)
            .map(|s| [s.off_status, s.note, 0])
            .collect()
    }
}

#[cfg(test)]
mod tests;
