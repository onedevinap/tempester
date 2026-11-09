//! The sequence of a beat: its tempo, swing, time signature and events, read out of the unpacked beat data (the bytes
//! of a `5F`, a `5C` without its number, or a beat inside a project file).
//!
//! Everything here is established from the firmware:
//! - header word 1-2: tempo x 10 as a 32-bit number (the header serializer multiplies the float by 10);
//! - header word 3: high byte = swing in ticks (0-12, the limit of its descriptor), low byte = quantization;
//! - header word 4: high byte = beats per measure, low byte = unit index 0-5 (whole, half, quarter, eighth, sixteenth
//!   notes, and one step) read through two tables of the firmware (48, 24, 12, 6, 3, 1 steps);
//! - header word 11, low byte: measures - 1; length in ticks = beats x unit steps x measures x 8;
//! - an event is `[step, w0, w1, w2, w3]`; its tick is `step x 8 + Time Shift - 7` (half step: +4 on steps 1 mod 6,
//!   -4 on steps 5 mod 6), plus the swing on the off 16th (`step x 8 mod 48 >= 24`), wrapped in the beat
//!   (the firmware's event tick computation). A tick is 1/96 of a quarter note.

use crate::beat::{
    header_words_of, read_unpacked, synthetic_beat_data, walk_events, BeatError, PADS,
};
use crate::sound::{Sound, SOUND_BYTES};

const HEADER_BYTES: usize = 538 * 2;
const TEMPO_WORD: usize = 2;
const SWING_WORD: usize = 3;
const SIGNATURE_WORD: usize = 4;
const MEASURES_WORD: usize = 11;
/// Steps in one unit of the time signature, by unit index (two tables of the firmware, one indexing the other).
const UNIT_STEPS: [u8; 6] = [48, 24, 12, 6, 3, 1];
const TICKS_PER_STEP: u32 = 8;
const SWING_PERIOD: u32 = 48;
const SWING_FROM: u32 = 24;
/// Time Shift 7 is "on the grid".
const TIME_SHIFT_CENTRE: i32 = 7;
const HALF_STEP_TICKS: i32 = 4;

/// One event of a beat, fields as in `refs/firmware-main.md` 9.4.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BeatEvent {
    pub step: u16,
    /// The sound (pad) 0-31.
    pub pad: u8,
    /// 0-127; 0 is never played by the machine.
    pub velocity: u8,
    /// In 24 ppq ticks (4 ticks of position each); 0 is a free slot.
    pub duration: u8,
    pub time_shift: u8,
    pub half_step: bool,
}

impl BeatEvent {
    fn from_words(words: [u16; 5]) -> Self {
        let [step, w0, w1, w2, _] = words;
        Self {
            step,
            pad: (w0 & 0x1F) as u8,
            velocity: ((w1 >> 8) & 0x7F) as u8,
            duration: (w1 & 0xFF) as u8,
            time_shift: (w0 >> 12) as u8,
            half_step: w2 & 0x4000 != 0,
        }
    }

    /// The record as the format stores it: `[step, w0, w1, w2, w3]` (tuning, FX and masks are left at 0).
    pub fn words(&self) -> [u16; 5] {
        [
            self.step,
            u16::from(self.time_shift) << 12 | u16::from(self.pad),
            u16::from(self.velocity) << 8 | u16::from(self.duration),
            u16::from(self.half_step) << 14,
            0,
        ]
    }

    /// True when the machine plays it: a free slot (duration 0) and velocity 0 are skipped by the firmware's note
    /// reader.
    pub fn is_played(&self) -> bool {
        self.velocity > 0 && self.duration > 0
    }
}

/// A beat's tempo, time signature, swing and events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BeatSequence {
    pub name: String,
    /// Beats per minute x 10.
    pub tempo_tenths: u32,
    /// Ticks the off 16ths are delayed by, 0-12.
    pub swing: u8,
    pub beats_per_measure: u8,
    pub unit_index: u8,
    pub measures: u16,
    pub events: Vec<BeatEvent>,
    /// The name of the sound of each pad.
    pub sound_names: [Option<String>; PADS],
}

impl BeatSequence {
    /// Steps in one unit of the time signature (a quarter note is 12).
    pub fn unit_steps(&self) -> u32 {
        u32::from(UNIT_STEPS[usize::from(self.unit_index)])
    }

    /// The beat's length in ticks of 1/96 quarter note.
    pub fn length_ticks(&self) -> u32 {
        u32::from(self.beats_per_measure)
            * self.unit_steps()
            * u32::from(self.measures)
            * TICKS_PER_STEP
    }

    /// The time signature's denominator as the power of two a Standard MIDI File stores (0 whole note ... 4
    /// sixteenth); `None` for the one-step unit, which is no note value.
    pub fn denominator_power(&self) -> Option<u8> {
        (self.unit_index < 5).then_some(self.unit_index)
    }

    /// Where the machine plays `event` in the loop, in ticks from the beat's start, or `None` for an event the
    /// machine never reaches (its step is past the end of the beat).
    pub fn event_tick(&self, event: &BeatEvent) -> Option<u32> {
        let length = i64::from(self.length_ticks());
        let base = u32::from(event.step) * TICKS_PER_STEP;
        if length == 0 || i64::from(base) >= length {
            return None;
        }
        let swing = if base % SWING_PERIOD >= SWING_FROM {
            i64::from(self.swing)
        } else {
            0
        };
        let offset = time_offset(event);
        Some(((i64::from(base) + swing + offset).rem_euclid(length)) as u32)
    }
}

/// Time Shift and half step as ticks (as the firmware's event tick computation).
fn time_offset(event: &BeatEvent) -> i64 {
    let shift = i32::from(event.time_shift) - TIME_SHIFT_CENTRE;
    let half = match (event.half_step, event.step % 6) {
        (false, _) => 0,
        (true, 1) => HALF_STEP_TICKS,
        (true, 5) => -HALF_STEP_TICKS,
        (true, _) => 0,
    };
    i64::from(shift + half)
}

/// Reads the sequence of a beat; the data must be exactly one beat (`beat::read_unpacked`).
pub fn read_sequence(data: &[u8]) -> Result<BeatSequence, BeatError> {
    let dump = read_unpacked(data)?;
    let version = u16::from_be_bytes([data[0], data[1]]);
    let header_bytes = header_words_of(version)? * 2;
    let word = |at: usize| u16::from_be_bytes([data[2 * at], data[2 * at + 1]]);
    let unit_index = (word(SIGNATURE_WORD) & 0xFF) as u8;
    if usize::from(unit_index) >= UNIT_STEPS.len() {
        return Err(BeatError::UnsupportedTimeUnit(unit_index));
    }
    let mut events = Vec::with_capacity(dump.summary.events);
    walk_events(data, header_bytes, |words| {
        events.push(BeatEvent::from_words(words));
    })?;
    Ok(BeatSequence {
        name: dump.summary.name,
        tempo_tenths: u32::from(word(1)) << 16 | u32::from(word(TEMPO_WORD)),
        swing: (word(SWING_WORD) >> 8) as u8,
        beats_per_measure: (word(SIGNATURE_WORD) >> 8) as u8,
        unit_index,
        measures: (word(MEASURES_WORD) & 0xFF) + 1,
        events,
        sound_names: dump.sounds.map(|sound| sound.map(|s| s.name())),
    })
}

/// The unpacked bytes of a complete beat (format 10) with a real sequence header: `name`, Voice Assign `assigns`, `tempo_tenths`, `swing`,
/// 4/4 over `measures` measures (1-256), one chunk of `events` and the 32 `sounds`. For tests and the fake machine.
pub fn synthetic_sequence_beat(
    name: &str,
    assigns: &[(usize, u8)],
    tempo_tenths: u32,
    swing: u8,
    measures: u16,
    events: &[BeatEvent],
    sounds: &[Sound; PADS],
) -> Vec<u8> {
    let mut data = synthetic_beat_data(name, assigns, 0, sounds);
    let tail = data.split_off(data.len() - PADS * SOUND_BYTES);
    data.truncate(HEADER_BYTES);
    let mut put = |word: usize, value: u16| {
        data[2 * word..2 * word + 2].copy_from_slice(&value.to_be_bytes());
    };
    put(1, (tempo_tenths >> 16) as u16);
    put(TEMPO_WORD, tempo_tenths as u16);
    put(SWING_WORD, u16::from(swing) << 8 | 2);
    put(SIGNATURE_WORD, 4 << 8 | 2);
    put(MEASURES_WORD, 0x7F00 | (measures.saturating_sub(1) & 0xFF));
    data.extend_from_slice(&(1 + 5 * events.len() as u16).to_be_bytes());
    for word in events.iter().flat_map(BeatEvent::words) {
        data.extend_from_slice(&word.to_be_bytes());
    }
    data.extend(tail);
    data
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sequence(swing: u8) -> BeatSequence {
        BeatSequence {
            name: "t".into(),
            tempo_tenths: 1200,
            swing,
            beats_per_measure: 4,
            unit_index: 2,
            measures: 2,
            events: Vec::new(),
            sound_names: std::array::from_fn(|_| None),
        }
    }

    fn event(step: u16, time_shift: u8, half_step: bool) -> BeatEvent {
        BeatEvent {
            step,
            pad: 0,
            velocity: 100,
            duration: 3,
            time_shift,
            half_step,
        }
    }

    #[test]
    fn test_length_ticks_when_two_measures_of_four_quarters_returns_768() {
        assert_eq!(sequence(0).length_ticks(), 768);
        assert_eq!(sequence(0).denominator_power(), Some(2));
    }

    #[test]
    fn test_event_tick_when_on_grid_returns_eight_ticks_per_step() {
        assert_eq!(sequence(0).event_tick(&event(12, 7, false)), Some(96));
    }

    #[test]
    fn test_event_tick_when_off_sixteenth_returns_swing_added() {
        // Step 3 = tick 24 = the off 16th of the first 8th; step 2 (tick 16) is on the first half.
        assert_eq!(sequence(5).event_tick(&event(3, 7, false)), Some(29));
        assert_eq!(sequence(5).event_tick(&event(2, 7, false)), Some(16));
    }

    #[test]
    fn test_event_tick_when_time_shift_before_the_start_returns_wrapped_into_the_beat() {
        assert_eq!(sequence(0).event_tick(&event(0, 4, false)), Some(765));
        assert_eq!(sequence(0).event_tick(&event(1, 11, false)), Some(12));
    }

    #[test]
    fn test_event_tick_when_half_step_returns_plus_four_on_1_and_minus_four_on_5_mod_6_only() {
        let at = |step| sequence(0).event_tick(&event(step, 7, true));
        assert_eq!(at(1), Some(12));
        assert_eq!(at(5), Some(36));
        assert_eq!(at(2), Some(16));
        assert_eq!(at(7), Some(60));
    }

    #[test]
    fn test_event_tick_when_step_past_the_end_returns_none() {
        assert_eq!(sequence(0).event_tick(&event(96, 7, false)), None);
    }

    #[test]
    fn test_is_played_when_velocity_or_duration_is_zero_returns_false() {
        let mut silent = event(0, 7, false);
        silent.velocity = 0;
        let mut free = event(0, 7, false);
        free.duration = 0;
        assert!(!silent.is_played() && !free.is_played() && event(0, 7, false).is_played());
    }

    /// A 2-measure 4/4 beat at 109.0 BPM with swing 9, the given events and 32 sounds named after their pad.
    fn beat_bytes(events: &[BeatEvent]) -> Vec<u8> {
        let sounds: [Sound; PADS] = std::array::from_fn(|pad| {
            let mut sound = Sound::with_defaults();
            let _ = sound.set_name(&format!("S{pad}"));
            sound
        });
        synthetic_sequence_beat("Seq", &[], 1090, 9, 2, events, &sounds)
    }

    #[test]
    fn test_read_sequence_when_header_and_events_are_set_returns_each_field() {
        let events = [
            BeatEvent {
                step: 3,
                pad: 5,
                velocity: 100,
                duration: 6,
                time_shift: 11,
                half_step: true,
            },
            BeatEvent {
                step: 95,
                pad: 31,
                velocity: 1,
                duration: 255,
                time_shift: 7,
                half_step: false,
            },
        ];

        let sequence = read_sequence(&beat_bytes(&events)).unwrap();

        assert_eq!(sequence.tempo_tenths, 1090);
        assert_eq!((sequence.swing, sequence.beats_per_measure), (9, 4));
        assert_eq!((sequence.unit_index, sequence.measures), (2, 2));
        assert_eq!(sequence.length_ticks(), 768);
        assert_eq!(sequence.sound_names[7].as_deref(), Some("S7"));
        assert_eq!(sequence.events, events.to_vec());
    }

    #[test]
    fn test_read_sequence_when_time_unit_is_outside_the_table_returns_error() {
        let mut data = beat_bytes(&[]);
        data[9] = 6;

        assert_eq!(read_sequence(&data), Err(BeatError::UnsupportedTimeUnit(6)));
    }
}
