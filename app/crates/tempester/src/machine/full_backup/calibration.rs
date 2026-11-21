//! The 35 calibration words of a Voice chip as the page, the manifest and the report show them, grouped by table. They
//! are shown for every chip whose message decoded, accepted, warned or refused: they are the user's own machine data
//! (measured by that machine's calibration), so a user can quote them in a bug report, where the Voice image itself
//! (DSI's Voice OS) must never go.

use tempest_proto::full_backup::{CalibrationWarning, CALIBRATION_WORDS};

use crate::json::{obj, Json};

/// The tables of the page, in order: label, first word, end.
const GROUPS: [(&str, usize, usize); 4] = [
    ("calibration A (osc 1)", 0, 10),
    ("calibration B (osc 2)", 10, 20),
    ("calibration C (linear)", 20, 32),
    ("scalars", 32, CALIBRATION_WORDS),
];

/// A word as the chip writes it holds `0xFF` in its upper byte, and only its 16-bit value is shown; any other word is
/// shown whole (6 digits), so a word that was not written by the calibration stands out.
fn word_hex(word: u32) -> String {
    if word >> 16 == 0xFF {
        format!("{:04x}", word & 0xFFFF)
    } else {
        format!("{word:06x}")
    }
}

fn group_words(raw: &[u32; CALIBRATION_WORDS], from: usize, to: usize) -> Vec<String> {
    raw[from..to].iter().map(|w| word_hex(*w)).collect()
}

/// The words for the page: `{"a": [...], "b": [...], "c": [...], "scalars": [...]}`.
pub(super) fn json(raw: &[u32; CALIBRATION_WORDS]) -> Json {
    let [a, b, c, s] = GROUPS.map(|(_, from, to)| Json::from(group_words(raw, from, to)));
    obj([("a", a), ("b", b), ("c", c), ("scalars", s)])
}

/// The words for the report, one table a line, as `tools/voice_compare.py --calibration` prints them.
pub(super) fn lines(raw: &[u32; CALIBRATION_WORDS]) -> String {
    GROUPS
        .iter()
        .map(|(label, from, to)| format!("  {label}: {}\n", group_words(raw, *from, *to).join(" ")))
        .collect()
}

/// The warning of a chip whose calibration has an unusual shape (empty when it has none).
pub(super) fn warning_text(warnings: &[CalibrationWarning]) -> Option<String> {
    if warnings.is_empty() {
        return None;
    }
    let found: Vec<String> = warnings.iter().map(ToString::to_string).collect();
    Some(format!(
        "WARNING: {}: an unusual calibration shape, saved exactly as read (the chip uses its page as it is); \
         you may report the calibration words, never the Voice .syx files",
        found.join(", ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw() -> [u32; CALIBRATION_WORDS] {
        std::array::from_fn(|i| 0xFF_0000 | (0x8000 + i as u32))
    }

    #[test]
    fn test_json_when_words_are_written_returns_the_16_bit_values_by_table() {
        let text = json(&raw()).render();

        assert!(text.starts_with(r#"{"a":["8000","8001","#), "{text}");
        assert!(text.contains(r#""b":["800a","#), "{text}");
        assert!(text.contains(r#""c":["8014","#), "{text}");
        assert!(
            text.ends_with(r#""scalars":["8020","8021","8022"]}"#),
            "{text}"
        );
    }

    #[test]
    fn test_lines_when_a_word_lacks_the_upper_byte_returns_it_whole() {
        let mut words = raw();
        words[33] = 0x00_1234;

        let text = lines(&words);

        assert!(text.contains("  scalars: 8020 001234 8022\n"), "{text}");
        assert!(
            text.starts_with("  calibration A (osc 1): 8000 8001 "),
            "{text}"
        );
        assert_eq!(text.lines().count(), 4);
    }

    #[test]
    fn test_warning_text_when_none_returns_nothing_and_otherwise_names_each_shape() {
        assert_eq!(warning_text(&[]), None);

        let text = warning_text(&[
            CalibrationWarning::NotIncreasing("A"),
            CalibrationWarning::NotLinear,
        ])
        .expect("a warning");

        assert!(text.starts_with("WARNING: table A does not increase, table C is not linear"));
        assert!(text.contains("saved exactly as read"));
        assert!(text.contains("never the Voice .syx files"));
    }
}
