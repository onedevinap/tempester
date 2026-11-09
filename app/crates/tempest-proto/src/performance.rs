//! The performance controls Tempester adds over MIDI control changes, both ways (n = Remote Pad IN − 1 to the
//! machine, m = Remote Pad OUT − 1 from it; the app does not know Remote Pad OUT, so reports are read on any channel:
//! the stock OS never sends a control change).
//!
//! - **Beat-wide positions** (n°29, Tempester 1.6.2.0, `Docs/analyses/feature-beat-wide-emission.md` §4): the 8
//!   beat-wide parameters have a position 0-254 (127 = neutral). `B (14+i) (p>>1)`, then `B (34+i) 40` only when p is
//!   odd. A new MSB clears the low bit; an LSB alone sets the low bit of the current position. Same bytes both ways;
//!   what the app sends is never echoed back.
//! - **FX ribbons** (n°31, Tempester 1.6.3.0, `Docs/analyses/feature-fx-pedal.md` §2): ribbon r (0, 1) touched at
//!   p 0-127 = `B (6C+r) p`, released = `B (6E+r) xx`, pressure v = `B (70+r) v` (to the machine only). The machine
//!   reports its physical ribbons as `Bm (6C+r) p` and `Bm (6E+r) 00`; remote moves are not echoed.
//! - **Play probability** (n°36, Tempester 1.6.3.0, `Docs/analyses/feature-step-probability.md` §1): `Bn 0E p`, p in
//!   percent, sets the probability of the selected pad of the edited beat (to the machine only; read back from the
//!   beat dump, `beat::BeatSummary::play_probability`).

use std::fmt;

/// CC of the MSB of beat-wide parameter 0 (All Osc Freq); parameters 0-7 are CC 20-27.
pub const CC_BEAT_WIDE: u8 = 20;
/// CC of the LSB of beat-wide parameter 0; parameters 0-7 are CC 52-59.
pub const CC_BEAT_WIDE_LSB: u8 = 52;
pub const BEAT_WIDE_COUNT: u8 = 8;
/// The highest position (the firmware clamps 255 to 254).
pub const BEAT_WIDE_MAX: u8 = 254;
/// The position of a parameter nobody moved, and of all 8 after a Revert (CC 19 or the Revert key).
pub const BEAT_WIDE_NEUTRAL: u8 = 127;
/// The LSB value that sets the low bit of a position.
const LSB_ODD: u8 = 0x40;

/// CC of ribbon 1 touched (ribbon 2 is the next one).
pub const CC_RIBBON_TOUCH: u8 = 108;
/// CC of ribbon 1 released.
pub const CC_RIBBON_RELEASE: u8 = 110;
/// CC of the pressure on ribbon 1.
pub const CC_RIBBON_PRESSURE: u8 = 112;
pub const RIBBON_COUNT: u8 = 2;
/// Positions and pressures are 7-bit inside the firmware.
pub const RIBBON_MAX: u8 = 127;

/// CC of the play probability of the selected pad.
pub const CC_PLAY_PROBABILITY: u8 = 14;
pub const PROBABILITY_MAX: u8 = 100;

/// Why a performance message could not be built.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PerformanceError {
    BadChannel(u8),
    BadBeatWide(u8),
    BadPosition(u8),
    BadRibbon(u8),
    BadRibbonValue(u8),
    BadProbability(u8),
}

impl fmt::Display for PerformanceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadChannel(c) => write!(f, "MIDI channel {c} is above 15"),
            Self::BadBeatWide(i) => {
                write!(
                    f,
                    "beat-wide parameter {i} is not 0-{}",
                    BEAT_WIDE_COUNT - 1
                )
            }
            Self::BadPosition(p) => write!(f, "beat-wide position {p} is above {BEAT_WIDE_MAX}"),
            Self::BadRibbon(r) => write!(f, "ribbon {r} is not 0-{}", RIBBON_COUNT - 1),
            Self::BadRibbonValue(v) => write!(f, "ribbon value {v} is above {RIBBON_MAX}"),
            Self::BadProbability(p) => write!(f, "probability {p} is above {PROBABILITY_MAX} %"),
        }
    }
}

impl std::error::Error for PerformanceError {}

fn status(channel: u8) -> Result<u8, PerformanceError> {
    if channel > 15 {
        return Err(PerformanceError::BadChannel(channel));
    }
    Ok(0xB0 | channel)
}

/// Beat-wide parameter `index` (0-7) set to `position` (0-254): 3 bytes, or 6 for an odd position.
pub fn beat_wide(channel: u8, index: u8, position: u8) -> Result<Vec<u8>, PerformanceError> {
    let status = status(channel)?;
    if index >= BEAT_WIDE_COUNT {
        return Err(PerformanceError::BadBeatWide(index));
    }
    if position > BEAT_WIDE_MAX {
        return Err(PerformanceError::BadPosition(position));
    }
    let mut bytes = vec![status, CC_BEAT_WIDE + index, position >> 1];
    if position & 1 == 1 {
        bytes.extend([status, CC_BEAT_WIDE_LSB + index, LSB_ODD]);
    }
    Ok(bytes)
}

/// The 8 beat-wide positions as the machine reports them, decoded from its control changes. A position is unknown
/// until its MSB arrives (an LSB alone cannot be placed). What the app sends is not echoed, so the app records it
/// with `set`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BeatWideReader {
    positions: [Option<u8>; BEAT_WIDE_COUNT as usize],
}

impl BeatWideReader {
    /// Reads one control change; returns the parameter and its new position when it is a beat-wide report that
    /// gives a known position.
    pub fn feed(&mut self, controller: u8, value: u8) -> Option<(u8, u8)> {
        let msb = CC_BEAT_WIDE..CC_BEAT_WIDE + BEAT_WIDE_COUNT;
        let lsb = CC_BEAT_WIDE_LSB..CC_BEAT_WIDE_LSB + BEAT_WIDE_COUNT;
        let (index, position) = if msb.contains(&controller) {
            let index = controller - CC_BEAT_WIDE;
            (index, (value & 0x7F) << 1)
        } else if lsb.contains(&controller) {
            let index = controller - CC_BEAT_WIDE_LSB;
            let known = self.positions[usize::from(index)]?;
            // The firmware clamps 255 to 254 in its beat-wide CC handler too.
            (index, ((known & !1) | (value >> 6 & 1)).min(BEAT_WIDE_MAX))
        } else {
            return None;
        };
        self.positions[usize::from(index)] = Some(position);
        Some((index, position))
    }

    /// A position the app has just sent.
    pub fn set(&mut self, index: u8, position: u8) {
        if let Some(slot) = self.positions.get_mut(usize::from(index)) {
            *slot = Some(position.min(BEAT_WIDE_MAX));
        }
    }

    /// After a Revert (CC 19 sent by the app, not reported by the machine): every position is neutral.
    pub fn revert(&mut self) {
        self.positions = [Some(BEAT_WIDE_NEUTRAL); BEAT_WIDE_COUNT as usize];
    }

    /// The port is gone: nothing is known any more.
    pub fn forget(&mut self) {
        *self = Self::default();
    }

    pub fn position(&self, index: u8) -> Option<u8> {
        self.positions.get(usize::from(index)).copied().flatten()
    }
}

fn ribbon_check(ribbon: u8, value: u8) -> Result<(), PerformanceError> {
    if ribbon >= RIBBON_COUNT {
        return Err(PerformanceError::BadRibbon(ribbon));
    }
    if value > RIBBON_MAX {
        return Err(PerformanceError::BadRibbonValue(value));
    }
    Ok(())
}

/// Ribbon `ribbon` (0, 1) touched at `position` (0-127).
pub fn ribbon_touch(channel: u8, ribbon: u8, position: u8) -> Result<[u8; 3], PerformanceError> {
    ribbon_check(ribbon, position)?;
    Ok([status(channel)?, CC_RIBBON_TOUCH + ribbon, position])
}

/// Ribbon `ribbon` released (the machine ignores the value; 0 is what it sends itself).
pub fn ribbon_release(channel: u8, ribbon: u8) -> Result<[u8; 3], PerformanceError> {
    ribbon_check(ribbon, 0)?;
    Ok([status(channel)?, CC_RIBBON_RELEASE + ribbon, 0])
}

/// Pressure `pressure` (0-127) on ribbon `ribbon`.
pub fn ribbon_pressure(channel: u8, ribbon: u8, pressure: u8) -> Result<[u8; 3], PerformanceError> {
    ribbon_check(ribbon, pressure)?;
    Ok([status(channel)?, CC_RIBBON_PRESSURE + ribbon, pressure])
}

/// A physical ribbon of the machine: touched at a position, or released (`None`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RibbonReport {
    pub ribbon: u8,
    pub position: Option<u8>,
}

/// The ribbon report carried by a channel message, if it is one (any channel, see the module notes).
pub fn ribbon_report(status: u8, controller: u8, value: u8) -> Option<RibbonReport> {
    if status & 0xF0 != 0xB0 {
        return None;
    }
    let touch = CC_RIBBON_TOUCH..CC_RIBBON_TOUCH + RIBBON_COUNT;
    let release = CC_RIBBON_RELEASE..CC_RIBBON_RELEASE + RIBBON_COUNT;
    if touch.contains(&controller) {
        Some(RibbonReport {
            ribbon: controller - CC_RIBBON_TOUCH,
            position: Some(value & RIBBON_MAX),
        })
    } else if release.contains(&controller) {
        Some(RibbonReport {
            ribbon: controller - CC_RIBBON_RELEASE,
            position: None,
        })
    } else {
        None
    }
}

/// Play probability `percent` (0-100) for the selected pad of the edited beat.
pub fn play_probability(channel: u8, percent: u8) -> Result<[u8; 3], PerformanceError> {
    if percent > PROBABILITY_MAX {
        return Err(PerformanceError::BadProbability(percent));
    }
    Ok([status(channel)?, CC_PLAY_PROBABILITY, percent])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_beat_wide_when_even_or_odd_returns_the_bytes_of_the_analysis() {
        // §4: 127 = neutral = `Bm 14+i 3F`, `Bm 34+i 40`; LP Freq is i = 2 (CC 22 / 54).
        assert_eq!(
            beat_wide(0, 0, 127),
            Ok(vec![0xB0, 0x14, 0x3F, 0xB0, 0x34, 0x40])
        );
        assert_eq!(beat_wide(1, 2, 200), Ok(vec![0xB1, 0x16, 100]));
        assert_eq!(beat_wide(0, 7, 254), Ok(vec![0xB0, 0x1B, 127]));
        assert_eq!(beat_wide(0, 7, 0), Ok(vec![0xB0, 0x1B, 0]));
    }

    #[test]
    fn test_beat_wide_when_out_of_range_returns_error() {
        assert_eq!(beat_wide(0, 8, 0), Err(PerformanceError::BadBeatWide(8)));
        assert_eq!(
            beat_wide(0, 0, 255),
            Err(PerformanceError::BadPosition(255))
        );
        assert_eq!(beat_wide(16, 0, 0), Err(PerformanceError::BadChannel(16)));
    }

    #[test]
    fn test_reader_when_every_position_is_sent_returns_it_back() {
        for index in 0..BEAT_WIDE_COUNT {
            for position in 0..=BEAT_WIDE_MAX {
                let mut reader = BeatWideReader::default();
                let bytes = beat_wide(0, index, position).unwrap();
                let mut last = None;
                for message in bytes.chunks(3) {
                    last = reader.feed(message[1], message[2]).or(last);
                }
                assert_eq!(last, Some((index, position)), "{index} {position}");
            }
        }
    }

    #[test]
    fn test_reader_when_msb_follows_an_odd_position_clears_the_low_bit() {
        let mut reader = BeatWideReader::default();
        reader.feed(0x16, 0x3F);
        reader.feed(0x36, 0x40);

        assert_eq!(reader.feed(0x16, 0x3F), Some((2, 126)));
    }

    #[test]
    fn test_reader_when_lsb_comes_alone_sets_only_the_low_bit_or_waits_for_an_msb() {
        let mut reader = BeatWideReader::default();
        assert_eq!(reader.feed(0x34, 0x40), None);

        reader.set(0, 100);
        assert_eq!(reader.feed(0x34, 0x40), Some((0, 101)));
        reader.set(0, 254);
        assert_eq!(reader.feed(0x34, 0x40), Some((0, 254)));
        assert_eq!(reader.feed(0x34, 0x00), Some((0, 254)));
    }

    #[test]
    fn test_reader_when_other_controllers_or_revert_returns_nothing_or_neutral() {
        let mut reader = BeatWideReader::default();
        for controller in [12, 13, 14, 19, 28, 51, 60, 104, 108] {
            assert_eq!(reader.feed(controller, 64), None, "CC {controller}");
        }
        reader.revert();
        assert!((0..BEAT_WIDE_COUNT).all(|i| reader.position(i) == Some(BEAT_WIDE_NEUTRAL)));
        reader.forget();
        assert_eq!(reader.position(3), None);
    }

    #[test]
    fn test_ribbon_when_touched_released_and_pressed_returns_cc_108_to_113() {
        assert_eq!(ribbon_touch(0, 0, 64), Ok([0xB0, 0x6C, 64]));
        assert_eq!(ribbon_touch(2, 1, 127), Ok([0xB2, 0x6D, 127]));
        assert_eq!(ribbon_release(0, 1), Ok([0xB0, 0x6F, 0]));
        assert_eq!(ribbon_pressure(0, 0, 90), Ok([0xB0, 0x70, 90]));
        assert_eq!(ribbon_pressure(0, 1, 0), Ok([0xB0, 0x71, 0]));
    }

    #[test]
    fn test_ribbon_when_out_of_range_returns_error() {
        assert_eq!(ribbon_touch(0, 2, 0), Err(PerformanceError::BadRibbon(2)));
        assert_eq!(
            ribbon_touch(0, 0, 128),
            Err(PerformanceError::BadRibbonValue(128))
        );
        assert_eq!(ribbon_release(16, 0), Err(PerformanceError::BadChannel(16)));
    }

    #[test]
    fn test_ribbon_report_when_machine_reports_returns_position_or_release_on_any_channel() {
        assert_eq!(
            ribbon_report(0xB1, 0x6C, 33),
            Some(RibbonReport {
                ribbon: 0,
                position: Some(33)
            })
        );
        assert_eq!(
            ribbon_report(0xBF, 0x6F, 0),
            Some(RibbonReport {
                ribbon: 1,
                position: None
            })
        );
        assert_eq!(ribbon_report(0xB0, 0x70, 5), None);
        assert_eq!(ribbon_report(0x90, 0x6C, 5), None);
        assert_eq!(ribbon_report(0xB0, 0x6B, 5), None);
    }

    #[test]
    fn test_play_probability_when_0_to_100_returns_cc_14_and_refuses_more() {
        assert_eq!(play_probability(0, 0), Ok([0xB0, 0x0E, 0]));
        assert_eq!(play_probability(3, 100), Ok([0xB3, 0x0E, 100]));
        assert_eq!(
            play_probability(0, 101),
            Err(PerformanceError::BadProbability(101))
        );
    }
}
