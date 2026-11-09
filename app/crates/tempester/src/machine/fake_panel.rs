//! The remote panel of the scripted machine (`TEMPESTER_FAKE_MIDI=1`), a deliberately simple model of Tempester
//! 1.6.0.0: a key press toggles the LED of that key, a note on a pad key lights its pad LED (once the pad LEDs were
//! enabled by a state request), and a state request sends every LED. Which note is which pad follows the machine's
//! own table (`pad_notes`: System > MIDI Note : Pad), never 36 + pad; a bank B pad lights the LED of its place. It also plays the 1.6.3.0 performance controls:
//! the 8 beat-wide positions (CC 20-27 / 52-59 kept, not echoed, sent after the LEDs on a state request, all neutral
//! after CC 19), the FX ribbons (CC 108-113 kept, not echoed) and the play probability (CC 14 for the pad last pressed
//! by a note, kept for the beat dump). Like the author's machine, it listens on one
//! channel and answers on the next one (Remote Pad IN = ch 1, OUT = ch 2), which exercises the app's choice of
//! accepting LED changes on any channel.

use tempest_proto::beat::PADS;
use tempest_proto::pad_notes::{PadNotes, BANK_PADS};
use tempest_proto::performance::{
    self, BeatWideReader, BEAT_WIDE_COUNT, BEAT_WIDE_NEUTRAL, CC_PLAY_PROBABILITY, PROBABILITY_MAX,
};
use tempest_proto::remote::{
    CC_KEY_PRESS, CC_LED_OFF, CC_LED_ON, CC_STATE_REQUEST, FIRST_PAD_LED, LED_COUNT,
};

/// The LEDs lit by each key that has one (`refs/panel_keys.tsv`); the two Shift keys light both Shift LEDs.
const KEY_LEDS: [(u8, &[u8]); 26] = [
    (0, &[0x00]),
    (1, &[0x01]),
    (8, &[0x08]),
    (9, &[0x09]),
    (16, &[0x10]),
    (17, &[0x11]),
    (18, &[0x12]),
    (19, &[0x13]),
    (20, &[0x14]),
    (21, &[0x16]),
    (22, &[0x15]),
    (23, &[0x17]),
    (24, &[0x18]),
    (25, &[0x19]),
    (26, &[0x1A]),
    (27, &[0x1B]),
    (28, &[0x1C]),
    (29, &[0x1D]),
    (30, &[0x1E]),
    (31, &[0x1F]),
    (32, &[0x20]),
    (38, &[0x23]),
    (40, &[0x25]),
    (41, &[0x26, 0x2B]),
    (46, &[0x24]),
    (47, &[0x26, 0x2B]),
];

const CC_REVERT: u8 = 19;

#[derive(Debug)]
pub struct FakePanel {
    leds: [bool; LED_COUNT as usize],
    pads_enabled: bool,
    beat_wide: BeatWideReader,
    /// The pad a note pressed last: the selected pad CC 14 acts on.
    selected_pad: u8,
    /// Play probability of each pad of the current beat, in percent.
    pub probability: [u8; PADS],
    /// Where each FX ribbon is touched (`None`: released).
    pub ribbons: [Option<u8>; 2],
    /// The machine's note of each pad.
    pad_notes: PadNotes,
}

impl Default for FakePanel {
    fn default() -> Self {
        let mut beat_wide = BeatWideReader::default();
        beat_wide.revert();
        Self {
            leds: [false; LED_COUNT as usize],
            pads_enabled: false,
            beat_wide,
            selected_pad: 0,
            probability: [PROBABILITY_MAX; PADS],
            ribbons: [None; 2],
            pad_notes: PadNotes::default(),
        }
    }
}

impl FakePanel {
    pub fn with_pad_notes(pad_notes: PadNotes) -> Self {
        Self {
            pad_notes,
            ..Self::default()
        }
    }

    /// What the machine sends back for one channel message received on `status`.
    pub fn on_channel(&mut self, status: u8, data: &[u8]) -> Vec<u8> {
        let out = 0xB0 | ((status & 0x0F).wrapping_add(1) & 0x0F);
        if let (0xB0, &[controller, value]) = (status & 0xF0, data) {
            if self.performance(controller, value) {
                return Vec::new();
            }
        }
        let changed = match (status & 0xF0, data) {
            (0xB0, &[CC_STATE_REQUEST, value]) => {
                let mut reply = self.full_state(out, value & 1 == 1);
                reply.extend(self.beat_wide_state(out));
                return reply;
            }
            (0xB0, &[CC_KEY_PRESS, key]) => self.toggle_key(key),
            (0x90, &[note, velocity]) => self.pad(note, velocity > 0),
            (0x80, &[note, _]) => self.pad(note, false),
            _ => Vec::new(),
        };
        led_messages(
            out,
            changed.into_iter().map(|i| (i, self.leds[usize::from(i)])),
        )
    }

    /// The performance control changes, none answered (the firmware never echoes them). `true` when it was one.
    fn performance(&mut self, controller: u8, value: u8) -> bool {
        if let Some(report) = performance::ribbon_report(0xB0, controller, value) {
            self.ribbons[usize::from(report.ribbon)] = report.position;
            return true;
        }
        if (performance::CC_RIBBON_PRESSURE..performance::CC_RIBBON_PRESSURE + 2)
            .contains(&controller)
        {
            return true;
        }
        match controller {
            CC_PLAY_PROBABILITY => {
                self.probability[usize::from(self.selected_pad)] = value.min(PROBABILITY_MAX);
                true
            }
            CC_REVERT => {
                if value != 0 {
                    self.beat_wide.revert();
                }
                true
            }
            _ => self.beat_wide.feed(controller, value).is_some(),
        }
    }

    /// What n°29 adds to a state request: the 8 positions, MSB then the LSB of an odd one.
    fn beat_wide_state(&self, out: u8) -> Vec<u8> {
        (0..BEAT_WIDE_COUNT)
            .filter_map(|index| {
                let position = self.beat_wide.position(index).unwrap_or(BEAT_WIDE_NEUTRAL);
                performance::beat_wide(out & 0x0F, index, position).ok()
            })
            .flatten()
            .collect()
    }

    #[cfg(test)]
    pub fn beat_wide_position(&self, index: u8) -> Option<u8> {
        self.beat_wide.position(index)
    }

    fn full_state(&mut self, out: u8, with_pads: bool) -> Vec<u8> {
        self.pads_enabled = with_pads;
        let shown = (0..LED_COUNT).filter(|i| with_pads || *i < FIRST_PAD_LED);
        led_messages(out, shown.map(|i| (i, self.leds[usize::from(i)])))
    }

    fn toggle_key(&mut self, key: u8) -> Vec<u8> {
        let Some((_, leds)) = KEY_LEDS.iter().find(|(code, _)| *code == key) else {
            return Vec::new();
        };
        for led in *leds {
            let lit = &mut self.leds[usize::from(*led)];
            *lit = !*lit;
        }
        leds.to_vec()
    }

    fn pad(&mut self, note: u8, on: bool) -> Vec<u8> {
        let Some(pad) = self.pad_notes.pad_of(note) else {
            return Vec::new();
        };
        if on {
            self.selected_pad = pad as u8;
        }
        if !self.pads_enabled {
            return Vec::new();
        }
        let led = FIRST_PAD_LED + (pad % BANK_PADS) as u8;
        self.leds[usize::from(led)] = on;
        vec![led]
    }
}

fn led_messages(status: u8, leds: impl Iterator<Item = (u8, bool)>) -> Vec<u8> {
    leds.flat_map(|(index, on)| [status, if on { CC_LED_ON } else { CC_LED_OFF }, index])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bytes of the 8 beat-wide positions at 127 in a state reply: 3 for the MSB, 3 for the LSB of an odd position.
    const NEUTRAL_POSITIONS: usize = 8 * 6;

    fn press(key: u8) -> [u8; 2] {
        [CC_KEY_PRESS, key]
    }

    #[test]
    fn test_on_channel_when_state_requested_without_pads_returns_44_leds_on_the_next_channel() {
        let mut panel = FakePanel::default();

        let reply = panel.on_channel(0xB0, &[CC_STATE_REQUEST, 0]);

        // 44 LEDs, then the 8 neutral beat-wide positions (MSB + LSB each, n°29).
        assert_eq!(reply.len(), 44 * 3 + NEUTRAL_POSITIONS);
        assert_eq!(&reply[..3], &[0xB1, CC_LED_OFF, 0]);
        assert!(reply[..44 * 3].chunks(3).all(|m| m[2] < FIRST_PAD_LED));
    }

    #[test]
    fn test_on_channel_when_state_requested_with_pads_returns_all_60_leds() {
        let mut panel = FakePanel::default();

        assert_eq!(
            panel.on_channel(0xB0, &[CC_STATE_REQUEST, 1]).len(),
            180 + NEUTRAL_POSITIONS
        );
    }

    #[test]
    fn test_on_channel_when_key_pressed_twice_toggles_its_led_on_then_off() {
        let mut panel = FakePanel::default();

        let first = panel.on_channel(0xB0, &press(46));
        let second = panel.on_channel(0xB0, &press(46));

        assert_eq!(first, vec![0xB1, CC_LED_ON, 0x24]);
        assert_eq!(second, vec![0xB1, CC_LED_OFF, 0x24]);
    }

    #[test]
    fn test_on_channel_when_shift_pressed_lights_both_shift_leds() {
        let mut panel = FakePanel::default();

        let reply = panel.on_channel(0xB0, &press(41));

        assert_eq!(reply, vec![0xB1, CC_LED_ON, 0x26, 0xB1, CC_LED_ON, 0x2B]);
    }

    #[test]
    fn test_on_channel_when_key_has_no_led_or_is_released_returns_nothing() {
        let mut panel = FakePanel::default();

        assert!(panel.on_channel(0xB0, &press(34)).is_empty());
        assert!(panel.on_channel(0xB0, &[103, 46]).is_empty());
    }

    #[test]
    fn test_on_channel_when_state_requested_returns_the_leds_then_the_8_beat_wide_positions() {
        let mut panel = FakePanel::default();
        panel.on_channel(0xB0, &[0x16, 100]);

        let reply = panel.on_channel(0xB0, &[CC_STATE_REQUEST, 0]);

        let tail = &reply[44 * 3..];
        assert_eq!(&tail[..6], &[0xB1, 0x14, 0x3F, 0xB1, 0x34, 0x40]);
        assert!(tail.windows(3).any(|w| w == [0xB1, 0x16, 100]));
        assert_eq!(tail.len(), 7 * 6 + 3);
    }

    #[test]
    fn test_on_channel_when_performance_ccs_arrive_keeps_them_and_echoes_nothing() {
        let mut panel = FakePanel::default();

        let replies = [
            panel.on_channel(0xB0, &[0x17, 10]),
            panel.on_channel(0xB0, &[0x37, 0x40]),
            panel.on_channel(0xB0, &[0x6D, 70]),
            panel.on_channel(0xB0, &[0x71, 20]),
            panel.on_channel(0x90, &[45, 100]),
            panel.on_channel(0xB0, &[0x0E, 120]),
        ];

        assert!(replies.iter().all(Vec::is_empty));
        assert_eq!(panel.beat_wide_position(3), Some(21));
        assert_eq!(panel.ribbons, [None, Some(70)]);
        assert_eq!(panel.probability[5], 100);
        panel.on_channel(0xB0, &[0x0E, 33]);
        assert_eq!(panel.probability[5], 33);
        panel.on_channel(0xB0, &[0x6F, 0]);
        panel.on_channel(0xB0, &[19, 127]);
        assert_eq!(panel.ribbons, [None, None]);
        assert_eq!(panel.beat_wide_position(3), Some(127));
    }

    #[test]
    fn test_on_channel_when_pad_note_and_pads_not_enabled_returns_nothing() {
        let mut panel = FakePanel::default();

        assert!(panel.on_channel(0x90, &[36, 100]).is_empty());
    }

    #[test]
    fn test_on_channel_when_pad_note_after_pads_enabled_lights_and_clears_its_led() {
        let mut panel = FakePanel::default();
        panel.on_channel(0xB0, &[CC_STATE_REQUEST, 1]);

        let on = panel.on_channel(0x90, &[38, 100]);
        let off = panel.on_channel(0x80, &[38, 0]);
        let not_a_pad = panel.on_channel(0x90, &[37, 100]);

        assert_eq!(on, vec![0xB1, CC_LED_ON, 0x2D]);
        assert_eq!(off, vec![0xB1, CC_LED_OFF, 0x2D]);
        assert!(not_a_pad.is_empty(), "37 is no pad of the default table");
    }

    #[test]
    fn test_on_channel_when_pad_notes_given_lights_the_pad_of_that_table() {
        let mut notes = PadNotes::default();
        notes.learn(4, 37);
        notes.learn(20, 99);
        let mut panel = FakePanel::with_pad_notes(notes);
        panel.on_channel(0xB0, &[CC_STATE_REQUEST, 1]);

        let a5 = panel.on_channel(0x90, &[37, 100]);
        let b5 = panel.on_channel(0x90, &[99, 100]);

        assert_eq!(a5, vec![0xB1, CC_LED_ON, FIRST_PAD_LED + 4]);
        assert_eq!(b5, vec![0xB1, CC_LED_ON, FIRST_PAD_LED + 4]);
        panel.on_channel(0xB0, &[0x0E, 40]);
        assert_eq!(panel.probability[20], 40, "CC 14 acts on pad B5");
    }
}
