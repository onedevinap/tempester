//! The settings file: plain `key=value` lines (blank lines and `#` comments allowed). Unknown keys are kept.

use std::collections::BTreeMap;

use tempest_proto::pad_notes::PadNotes;

/// Known keys.
pub const KEY_PORT: &str = "port";
pub const KEY_CHANNEL: &str = "channel";
pub const KEY_PANEL_VIEW: &str = "panel_view";
pub const KEY_PANEL_ZOOM: &str = "panel_zoom";
/// The machine's note of each pad A1-B16 (`tempest_proto::pad_notes`), learned or edited.
pub const KEY_PAD_NOTES: &str = "pad_notes";
/// The Panel page zoom, in percent of the fit-to-width scale.
pub const PANEL_ZOOM_MIN: u32 = 60;
pub const PANEL_ZOOM_MAX: u32 = 200;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Settings {
    values: BTreeMap<String, String>,
}

impl Settings {
    /// Reads the text of a settings file; lines without `=` are ignored.
    pub fn parse(text: &str) -> Self {
        let values = text
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .filter_map(|line| line.split_once('='))
            .map(|(key, value)| (key.trim().to_string(), value.trim().to_string()))
            .filter(|(key, _)| !key.is_empty())
            .collect();
        Self { values }
    }

    /// The text to write back.
    pub fn render(&self) -> String {
        let mut out = String::from("# Tempester settings\n");
        for (key, value) in &self.values {
            out.push_str(&format!("{key}={value}\n"));
        }
        out
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }

    /// Sets a value; line breaks are dropped so that a value always stays on its line.
    pub fn set(&mut self, key: &str, value: &str) {
        let clean: String = value
            .chars()
            .filter(|c| !matches!(c, '\n' | '\r'))
            .collect();
        self.values
            .insert(key.to_string(), clean.trim().to_string());
    }

    /// Forgets a key (a missing key reads as absent).
    pub fn remove(&mut self, key: &str) {
        self.values.remove(key);
    }

    /// True for the Full view of the Panel page; the Machine view only when the file says so.
    pub fn panel_view_full(&self) -> bool {
        self.get(KEY_PANEL_VIEW) != Some("machine")
    }

    /// The Panel page zoom in percent: 100 (fit to width) when absent or out of range.
    pub fn panel_zoom(&self) -> u16 {
        self.get(KEY_PANEL_ZOOM)
            .and_then(|text| text.parse::<u32>().ok())
            .filter(|zoom| (PANEL_ZOOM_MIN..=PANEL_ZOOM_MAX).contains(zoom))
            .map_or(100, |zoom| zoom as u16)
    }

    /// The machine's pad note table: the default (the author's machine) when absent or invalid.
    pub fn pad_notes(&self) -> PadNotes {
        self.get(KEY_PAD_NOTES)
            .and_then(|text| PadNotes::parse(text).ok())
            .unwrap_or_default()
    }

    /// The MIDI channel 1-16, or 1 when absent or invalid.
    pub fn channel(&self) -> u8 {
        self.get(KEY_CHANNEL)
            .and_then(|text| text.parse::<u8>().ok())
            .filter(|channel| (1..=16).contains(channel))
            .unwrap_or(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_when_lines_have_comments_and_blanks_returns_only_pairs() {
        let settings = Settings::parse("# c\n\nport = Tempest \nchannel=3\nnonsense\n=x\n");

        assert_eq!(settings.get(KEY_PORT), Some("Tempest"));
        assert_eq!(settings.get(KEY_CHANNEL), Some("3"));
        assert_eq!(settings.get("nonsense"), None);
    }

    #[test]
    fn test_panel_view_full_when_absent_or_unknown_returns_true_and_machine_returns_false() {
        assert!(Settings::default().panel_view_full());
        assert!(Settings::parse("panel_view=other").panel_view_full());
        assert!(!Settings::parse("panel_view=machine").panel_view_full());
    }

    #[test]
    fn test_panel_zoom_when_absent_or_out_of_range_returns_100_else_the_value() {
        assert_eq!(Settings::default().panel_zoom(), 100);
        assert_eq!(Settings::parse("panel_zoom=500").panel_zoom(), 100);
        assert_eq!(Settings::parse("panel_zoom=x").panel_zoom(), 100);
        assert_eq!(Settings::parse("panel_zoom=140").panel_zoom(), 140);
    }

    #[test]
    fn test_render_then_parse_when_values_set_returns_same_settings() {
        let mut settings = Settings::default();
        settings.set(KEY_PORT, "IAC Driver Bus 1");
        settings.set("other", "x=y");

        assert_eq!(Settings::parse(&settings.render()), settings);
    }

    #[test]
    fn test_set_when_value_has_line_break_returns_single_line_value() {
        let mut settings = Settings::default();

        settings.set(KEY_PORT, "a\nchannel=9");

        assert_eq!(settings.get(KEY_PORT), Some("achannel=9"));
        assert_eq!(Settings::parse(&settings.render()).get(KEY_CHANNEL), None);
    }

    #[test]
    fn test_pad_notes_when_absent_or_invalid_returns_default_else_the_table() {
        let mut learned = PadNotes::default();
        learned.learn(0, 99);
        let mut settings = Settings::default();
        settings.set(KEY_PAD_NOTES, &learned.render());

        assert_eq!(Settings::default().pad_notes(), PadNotes::default());
        assert_eq!(
            Settings::parse("pad_notes=1,2").pad_notes(),
            PadNotes::default()
        );
        assert_eq!(Settings::parse(&settings.render()).pad_notes(), learned);
    }

    #[test]
    fn test_channel_when_missing_or_out_of_range_returns_one() {
        assert_eq!(Settings::default().channel(), 1);
        assert_eq!(Settings::parse("channel=0").channel(), 1);
        assert_eq!(Settings::parse("channel=17").channel(), 1);
        assert_eq!(Settings::parse("channel=abc").channel(), 1);
        assert_eq!(Settings::parse("channel=16").channel(), 16);
    }
}
