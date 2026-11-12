//! The Quantize of a project, as the parameter block of a `5E` message holds it (`public-src/docs/midi-protocol.md`,
//! "Fine Quantize value").
//!
//! Word 4's high byte is the official Quantize, 0-7 (1/4, 1/8, 1/8T, 1/16, 1/16T, 1/32, 1/32T, Off). From Tempester
//! 1.6.5.0 (n°55) one of eight more values is kept in word 18, `0x5100 | v`, with word 4 saying Off; the official OS
//! never reads words 18-33, and loads such a project with Off. A reader accepts word 18 only with the high byte
//! `0x51`, `v` 1-8 and word 4 at Off: anything else is plain Off.

use std::fmt;

/// Index of the official Quantize word in the parameter block (high byte).
pub const QUANTIZE_WORD: usize = 4;
/// Index of the word that holds the fine value.
pub const FINE_WORD: usize = 18;
const OFF: u8 = 7;
const FINE_TAG: u16 = 0x5100;
const OFFICIAL_LABELS: [&str; 8] = [
    "1/4", "1/8", "1/8T", "1/16", "1/16T", "1/32", "1/32T", "Off",
];
const FINE_LABELS: [&str; 8] = [
    "8 Dot", "16 Dot", "32 Dot", "64th", "64T", "8 Quint", "16 Quint", "16 Sept",
];

/// The Quantize of a project.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quantize {
    /// The official value, 0-7 (7 is Off).
    Official(u8),
    /// A Tempester 1.6.5.0 value, 1-8 (`FINE_LABELS` in order).
    Fine(u8),
}

impl Quantize {
    /// The value the parameter words 4 and 18 stand for; `None` when word 4's high byte is not 0-7.
    pub fn from_words(word4: u16, word18: u16) -> Option<Self> {
        let official = u8::try_from(word4 >> 8).ok().filter(|v| *v <= OFF)?;
        let fine = (word18 & 0xFF00 == FINE_TAG)
            .then_some((word18 & 0xFF) as u8)
            .filter(|v| (1..=8).contains(v));
        Some(match fine {
            Some(v) if official == OFF => Self::Fine(v),
            _ => Self::Official(official),
        })
    }

    /// True for a value only Tempester 1.6.5.0 or later keeps: another OS loads the project with Off.
    pub fn needs_tempester(self) -> bool {
        matches!(self, Self::Fine(_))
    }

    /// The name the machine's screen shows.
    pub fn label(self) -> &'static str {
        match self {
            Self::Official(v) => OFFICIAL_LABELS[usize::from(v.min(OFF))],
            Self::Fine(v) => FINE_LABELS[usize::from(v.clamp(1, 8)) - 1],
        }
    }
}

impl fmt::Display for Quantize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_words_when_official_value_returns_its_label() {
        assert_eq!(Quantize::from_words(3 << 8, 0), Some(Quantize::Official(3)));
        assert_eq!(
            Quantize::from_words(3 << 8, 0).map(Quantize::label),
            Some("1/16")
        );
        assert_eq!(
            Quantize::from_words(OFF as u16 * 256, 0xFFFF),
            Some(Quantize::Official(7))
        );
    }

    #[test]
    fn test_from_words_when_off_and_fine_tag_returns_each_fine_value() {
        let labels: Vec<_> = (1..=8u16)
            .filter_map(|v| Quantize::from_words(7 << 8, 0x5100 | v))
            .map(Quantize::label)
            .collect();
        assert_eq!(
            labels,
            ["8 Dot", "16 Dot", "32 Dot", "64th", "64T", "8 Quint", "16 Quint", "16 Sept"]
        );
    }

    #[test]
    fn test_from_words_when_fine_word_is_not_valid_returns_plain_off() {
        let off = Some(Quantize::Official(7));
        assert_eq!(Quantize::from_words(7 << 8, 0x5100), off, "v 0");
        assert_eq!(Quantize::from_words(7 << 8, 0x5109), off, "v 9");
        assert_eq!(Quantize::from_words(7 << 8, 0x5207), off, "wrong tag");
        assert_eq!(Quantize::from_words(7 << 8, 0xFFFF), off);
    }

    #[test]
    fn test_from_words_when_official_value_is_not_off_ignores_the_fine_word() {
        assert_eq!(
            Quantize::from_words(3 << 8, 0x5107),
            Some(Quantize::Official(3))
        );
    }

    #[test]
    fn test_from_words_when_word_4_is_out_of_range_returns_none() {
        assert_eq!(Quantize::from_words(8 << 8, 0), None);
    }

    #[test]
    fn test_needs_tempester_when_fine_returns_true_only_for_fine() {
        assert!(Quantize::Fine(7).needs_tempester());
        assert!(!Quantize::Official(7).needs_tempester());
    }
}
