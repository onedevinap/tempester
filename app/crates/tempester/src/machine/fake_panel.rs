//! The remote panel of the scripted machine (`TEMPESTER_FAKE_MIDI=1`), a deliberately simple model of Tempester
//! 1.6.0.0: a key press toggles the LED of that key, a note on a pad key lights its pad LED (once the pad LEDs were
//! enabled by a state request), and a state request sends every LED. Which note is which pad follows the machine's
//! own table (`pad_notes`: System > MIDI Note : Pad), never 36 + pad; a bank B pad lights the LED of its place. It also plays the 1.6.3.0 performance controls:
//! the 8 beat-wide positions (CC 20-27 / 52-59 kept, not echoed, sent after the LEDs on a state request, all neutral
//! after CC 19), the FX ribbons (CC 108-113 kept, not echoed) and the play probability (CC 14 for the pad last pressed
//! by a note, kept for the beat dump). The Pad Function keys set the mode (16 Sounds at start; one key at a time, no
//! 16 Tunings / 16 Levels combination): the four mode LEDs follow and the mode goes out as CC 106 (n°40), also after
//! the LEDs of a state request. It also keeps the 10 mixer, delay and main values of n°38 (`mixer_report`):
//! the n°23 controls (NRPN 222 / 226-229, encoders 208 / 209, Shift taken from the Shift LED) change them and the change is
//! reported as the machine does, CC 12 / 13 change Distort and Compress without a report, and a state request
//! sends all 10 after the beat-wide positions. Like the author's machine, it listens on one
//! channel and answers on the next one (Remote Pad IN = ch 1, OUT = ch 2), which exercises the app's choice of
//! accepting LED changes on any channel. The 32 MIDI channels of n°58 (CC 15 addresses a pad and is answered with CC 16 =
//! its value, CC 16 sets it, CC 17 = 127 saves them, which changes nothing here) follow the protocol of 1.6.5.0.

use tempest_proto::beat::PADS;
use tempest_proto::mixer_report::{report_bytes, MixerItem};
use tempest_proto::nrpn::{Nrpn, NrpnDecoder};
use tempest_proto::pad_channel::{CC_PAD_CHANNEL, CC_PAD_SELECT, CC_SYSTEM_SAVE, VALUE_MAX};
use tempest_proto::pad_notes::{PadNotes, BANK_PADS};
use tempest_proto::performance::{
    self, BeatWideReader, BEAT_WIDE_COUNT, BEAT_WIDE_NEUTRAL, CC_PLAY_PROBABILITY, PROBABILITY_MAX,
};
use tempest_proto::remote::{
    self, CC_KEY_PRESS, CC_LED_OFF, CC_LED_ON, CC_STATE_REQUEST, ENCODER_CENTER, FIRST_PAD_LED,
    LED_COUNT,
};

/// The Pad Function keys and the mode each sets alone (`refs/panel_keys.tsv`), and the LED lit in that mode.
const MODE_KEYS: [(u8, u8, u8); 4] = [(16, 0, 0x10), (17, 1, 0x11), (24, 2, 0x18), (25, 4, 0x19)];
const BOOT_MODE: u8 = 2;

/// The LEDs lit by each other key that has one (`refs/panel_keys.tsv`); the two Shift keys light both Shift LEDs.
const KEY_LEDS: [(u8, &[u8]); 22] = [
    (0, &[0x00]),
    (1, &[0x01]),
    (8, &[0x08]),
    (9, &[0x09]),
    (18, &[0x12]),
    (19, &[0x13]),
    (20, &[0x14]),
    (21, &[0x16]),
    (22, &[0x15]),
    (23, &[0x17]),
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
const CC_DISTORT: u8 = 12;
const CC_COMPRESS: u8 = 13;
/// The LED of both Shift keys (`refs/panel_leds.tsv`).
const SHIFT_LED: u8 = 0x26;
/// The Tempest ignores a distortion knob below this.
const DISTORT_DEAD_ZONE: u16 = 4;
/// Tempo units (tenths of a BPM) and Delay Send units one encoder detent moves.
const TEMPO_PER_DETENT: i32 = 5;
const DELAY_SEND_PER_DETENT: i32 = 3;

/// What the scripted machine shows at start, in the order of `MixerItem::ALL`.
const MIXER_START: [u16; 10] = [100, 64, 0, 3, 20, 1200, 3, 100, 0, 0];

#[derive(Debug)]
pub struct FakePanel {
    leds: [bool; LED_COUNT as usize],
    pads_enabled: bool,
    beat_wide: BeatWideReader,
    /// The pad a note pressed last: the selected pad CC 14 acts on.
    selected_pad: u8,
    /// Play probability of each pad of the current beat, in percent.
    pub probability: [u8; PADS],
    /// The MIDI channel value (0-32) of each pad (n°58) and the pad CC 15 addressed last.
    pub pad_channels: [u8; PADS],
    addressed_pad: u8,
    /// Where each FX ribbon is touched (`None`: released).
    pub ribbons: [Option<u8>; 2],
    /// The machine's note of each pad.
    pad_notes: PadNotes,
    nrpn_in: NrpnDecoder,
    /// The 10 values of n°38, in the order of `MixerItem::ALL`.
    pub mixer: [u16; 10],
    /// The Pad Function mode (0-5).
    pub mode: u8,
}

impl Default for FakePanel {
    fn default() -> Self {
        let mut beat_wide = BeatWideReader::default();
        beat_wide.revert();
        let mut leds = [false; LED_COUNT as usize];
        leds[usize::from(mode_led(BOOT_MODE))] = true;
        Self {
            leds,
            pads_enabled: false,
            beat_wide,
            selected_pad: 0,
            probability: [PROBABILITY_MAX; PADS],
            pad_channels: [0; PADS],
            addressed_pad: 0,
            ribbons: [None; 2],
            pad_notes: PadNotes::default(),
            nrpn_in: NrpnDecoder::new(),
            mixer: MIXER_START,
            mode: BOOT_MODE,
        }
    }
}

fn mode_led(mode: u8) -> u8 {
    MODE_KEYS
        .iter()
        .find(|(_, m, _)| *m == mode)
        .map_or(0x18, |(_, _, led)| *led)
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
            if let Some(change) = self.nrpn_in.feed_message(status, controller, value) {
                return self.apply_nrpn(out & 0x0F, change);
            }
            if let Some(reply) = self.pad_channel(out, controller, value) {
                return reply;
            }
            if self.performance(controller, value) {
                return Vec::new();
            }
        }
        let changed = match (status & 0xF0, data) {
            (0xB0, &[CC_STATE_REQUEST, value]) => {
                let mut reply = self.full_state(out, value & 1 == 1);
                reply.extend(self.mode_report(out));
                reply.extend(self.beat_wide_state(out));
                reply.extend(self.mixer_state(out & 0x0F));
                return reply;
            }
            (0xB0, &[CC_KEY_PRESS, key]) if MODE_KEYS.iter().any(|(k, ..)| *k == key) => {
                return self.set_mode(out, key);
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

    /// The pad channel controls of n°58: CC 15 is answered with the addressed pad's value, CC 16 sets it, CC 17 saves
    /// (nothing to keep here). `None` for any other controller.
    fn pad_channel(&mut self, out: u8, controller: u8, value: u8) -> Option<Vec<u8>> {
        match controller {
            CC_PAD_SELECT if usize::from(value) < PADS => {
                self.addressed_pad = value;
                Some(vec![
                    0xB0 | out & 0x0F,
                    CC_PAD_CHANNEL,
                    self.pad_channels[usize::from(value)],
                ])
            }
            CC_PAD_SELECT => Some(Vec::new()),
            CC_PAD_CHANNEL => {
                if value <= VALUE_MAX {
                    self.pad_channels[usize::from(self.addressed_pad)] = value;
                }
                Some(Vec::new())
            }
            CC_SYSTEM_SAVE => Some(Vec::new()),
            _ => None,
        }
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
            // The stock CC 12 / 13 set the raw value to the CC value, so the upper half of the knob is out of reach.
            CC_DISTORT | CC_COMPRESS => {
                let item = if controller == CC_DISTORT {
                    MixerItem::Distort
                } else {
                    MixerItem::Compress
                };
                self.mixer[item as usize] = u16::from(value & 0x7F);
                true
            }
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

    /// A remote-panel control (n°23) completed by the NRPN decoder: it moves a value of n°38, reported like the real
    /// machine does when the value changed. Any other number is ignored.
    fn apply_nrpn(&mut self, channel: u8, change: Nrpn) -> Vec<u8> {
        let shift = self.leds[usize::from(SHIFT_LED)];
        let knob = change.value;
        let byte = knob.min(255);
        let step = i32::from(knob) - i32::from(ENCODER_CENTER);
        let moved = match change.number {
            222 => Some((MixerItem::Compress, byte)),
            226 if shift => Some((MixerItem::DelayRepeats, (byte >> 1) * 126 / 127)),
            226 => Some((MixerItem::Pan, byte >> 1)),
            227 => Some((MixerItem::Level, byte >> 1)),
            228 => Some((MixerItem::Volume, byte >> 1)),
            229 => Some((MixerItem::Distort, dead_zone(byte))),
            208 if shift => Some((MixerItem::Swing, self.turned(MixerItem::Swing, step))),
            208 => Some((
                MixerItem::Tempo,
                self.turned(MixerItem::Tempo, step * TEMPO_PER_DETENT),
            )),
            209 if shift => Some((
                MixerItem::DelayTime,
                self.turned(MixerItem::DelayTime, step),
            )),
            209 => Some((
                MixerItem::DelaySend,
                self.turned(MixerItem::DelaySend, step * DELAY_SEND_PER_DETENT),
            )),
            _ => None,
        };
        let Some((item, value)) = moved else {
            return Vec::new();
        };
        if self.mixer[item as usize] == value {
            return Vec::new();
        }
        self.mixer[item as usize] = value;
        report_bytes(channel, item, value).map_or_else(|_| Vec::new(), |b| b.to_vec())
    }

    /// `item` moved by `delta` units, kept inside its range.
    fn turned(&self, item: MixerItem, delta: i32) -> u16 {
        let next = i32::from(self.mixer[item as usize]) + delta;
        let floor = if item == MixerItem::Tempo { 300 } else { 0 };
        next.clamp(floor, i32::from(item.max())) as u16
    }

    /// What n°38 adds to a state request: the 10 values.
    fn mixer_state(&self, channel: u8) -> Vec<u8> {
        MixerItem::ALL
            .into_iter()
            .filter_map(|item| report_bytes(channel, item, self.mixer[item as usize]).ok())
            .flatten()
            .collect()
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

    /// n°40's report: `CC 106 = mode` on Remote Pad OUT.
    fn mode_report(&self, out: u8) -> Vec<u8> {
        remote::pad_mode_report(out & 0x0F, self.mode)
            .map(|bytes| bytes.to_vec())
            .unwrap_or_default()
    }

    /// A Pad Function key: the mode LEDs that change, then the new mode; nothing when the mode stays.
    fn set_mode(&mut self, out: u8, key: u8) -> Vec<u8> {
        let Some(&(_, mode, lit)) = MODE_KEYS.iter().find(|(k, ..)| *k == key) else {
            return Vec::new();
        };
        if mode == self.mode {
            return Vec::new();
        }
        self.mode = mode;
        let changed: Vec<u8> = MODE_KEYS
            .iter()
            .map(|(_, _, led)| *led)
            .filter(|led| self.leds[usize::from(*led)] != (*led == lit))
            .collect();
        for led in &changed {
            self.leds[usize::from(*led)] = *led == lit;
        }
        let mut reply = led_messages(out, changed.into_iter().map(|i| (i, i == lit)));
        reply.extend(self.mode_report(out));
        reply
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

fn dead_zone(raw: u16) -> u16 {
    if raw < DISTORT_DEAD_ZONE {
        0
    } else {
        raw
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
    /// The mode report after the LEDs of a state reply (n°40).
    const MODE_REPORT: usize = 3;
    /// Bytes of the 10 mixer reports of n°38 closing a state reply: 12 each.
    const MIXER_REPORTS: usize = 10 * 12;

    fn press(key: u8) -> [u8; 2] {
        [CC_KEY_PRESS, key]
    }

    #[test]
    fn test_on_channel_when_state_requested_without_pads_returns_44_leds_on_the_next_channel() {
        let mut panel = FakePanel::default();

        let reply = panel.on_channel(0xB0, &[CC_STATE_REQUEST, 0]);

        // 44 LEDs, the mode (n°40), the 8 neutral beat-wide positions (MSB + LSB each, n°29), then the mixer (n°38).
        assert_eq!(
            reply.len(),
            44 * 3 + MODE_REPORT + NEUTRAL_POSITIONS + MIXER_REPORTS
        );
        assert_eq!(&reply[44 * 3..44 * 3 + 3], &[0xB1, 106, 2]);
        assert_eq!(&reply[..3], &[0xB1, CC_LED_OFF, 0]);
        assert!(reply[..44 * 3].chunks(3).all(|m| m[2] < FIRST_PAD_LED));
    }

    #[test]
    fn test_on_channel_when_state_requested_with_pads_returns_all_60_leds() {
        let mut panel = FakePanel::default();

        assert_eq!(
            panel.on_channel(0xB0, &[CC_STATE_REQUEST, 1]).len(),
            180 + MODE_REPORT + NEUTRAL_POSITIONS + MIXER_REPORTS
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
    fn test_on_channel_when_pad_function_key_pressed_moves_the_mode_leds_then_reports_the_mode() {
        let mut panel = FakePanel::default();

        let mutes = panel.on_channel(0xB0, &press(25));
        let again = panel.on_channel(0xB0, &press(25));
        let beats = panel.on_channel(0xB0, &press(16));

        assert_eq!(
            mutes,
            vec![0xB1, CC_LED_OFF, 0x18, 0xB1, CC_LED_ON, 0x19, 0xB1, 106, 4]
        );
        assert!(again.is_empty(), "the mode did not change");
        assert_eq!(
            beats,
            vec![0xB1, CC_LED_ON, 0x10, 0xB1, CC_LED_OFF, 0x19, 0xB1, 106, 0]
        );
        assert_eq!(panel.mode, 0);
    }

    #[test]
    fn test_on_channel_when_pad_channels_are_set_and_asked_returns_each_pad_s_own_value() {
        let mut panel = FakePanel::default();

        assert!(panel.on_channel(0xB0, &[15, 9]).eq(&[0xB1, 16, 0]));
        assert!(panel.on_channel(0xB0, &[16, 20]).is_empty());
        assert!(panel.on_channel(0xB0, &[15, 10]).eq(&[0xB1, 16, 0]));
        assert!(panel.on_channel(0xB0, &[15, 9]).eq(&[0xB1, 16, 20]));
        assert!(panel.on_channel(0xB0, &[17, 127]).is_empty());
        assert_eq!(panel.pad_channels[9], 20);
        panel.on_channel(0xB0, &[16, 33]);
        assert_eq!(panel.pad_channels[9], 20, "above 32 is ignored");
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

        let tail = &reply[44 * 3 + MODE_REPORT..];
        assert_eq!(&tail[..6], &[0xB1, 0x14, 0x3F, 0xB1, 0x34, 0x40]);
        assert!(tail.windows(3).any(|w| w == [0xB1, 0x16, 100]));
        assert_eq!(tail.len(), 7 * 6 + 3 + MIXER_REPORTS);
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

    /// The NRPN of an n°23 control as the app sends it.
    fn nrpn(panel: &mut FakePanel, number: u16, value: u16) -> Vec<u8> {
        let status = 0xB0;
        let mut reply = Vec::new();
        for (cc, v) in [
            (99, (number >> 7) as u8),
            (98, (number & 0x7F) as u8),
            (6, (value >> 7) as u8),
            (38, (value & 0x7F) as u8),
        ] {
            reply.extend(panel.on_channel(status, &[cc, v]));
        }
        reply
    }

    fn report(item: MixerItem, value: u16) -> Vec<u8> {
        report_bytes(1, item, value).unwrap().to_vec()
    }

    #[test]
    fn test_on_channel_when_state_requested_ends_with_the_10_mixer_values() {
        let mut panel = FakePanel::default();

        let reply = panel.on_channel(0xB0, &[CC_STATE_REQUEST, 0]);

        let expected: Vec<u8> = MixerItem::ALL
            .into_iter()
            .flat_map(|item| report(item, MIXER_START[item as usize]))
            .collect();
        assert_eq!(&reply[reply.len() - MIXER_REPORTS..], &expected[..]);
    }

    #[test]
    fn test_on_channel_when_absolute_controls_move_returns_the_report_of_the_changed_value() {
        let mut panel = FakePanel::default();

        assert_eq!(nrpn(&mut panel, 227, 145), report(MixerItem::Level, 72));
        assert_eq!(nrpn(&mut panel, 226, 20), report(MixerItem::Pan, 10));
        assert_eq!(nrpn(&mut panel, 228, 255), report(MixerItem::Volume, 127));
        assert_eq!(nrpn(&mut panel, 229, 200), report(MixerItem::Distort, 200));
        assert_eq!(nrpn(&mut panel, 222, 255), report(MixerItem::Compress, 255));
        assert_eq!(nrpn(&mut panel, 229, 3), report(MixerItem::Distort, 0));
        assert!(
            nrpn(&mut panel, 227, 144).is_empty(),
            "same 7-bit value: no report"
        );
    }

    #[test]
    fn test_on_channel_when_shift_is_lit_returns_delay_repeats_and_swing_and_delay_time() {
        let mut panel = FakePanel::default();
        panel.on_channel(0xB0, &press(41));

        assert_eq!(
            nrpn(&mut panel, 226, 254),
            report(MixerItem::DelayRepeats, 126)
        );
        assert_eq!(nrpn(&mut panel, 208, 8192 + 2), report(MixerItem::Swing, 5));
        assert_eq!(
            nrpn(&mut panel, 209, 8192 - 1),
            report(MixerItem::DelayTime, 2)
        );
        panel.on_channel(0xB0, &press(41));
        assert_eq!(
            nrpn(&mut panel, 208, 8192 + 2),
            report(MixerItem::Tempo, 1210)
        );
        assert_eq!(
            nrpn(&mut panel, 209, 8192 + 1),
            report(MixerItem::DelaySend, 3)
        );
    }

    #[test]
    fn test_on_channel_when_cc_12_and_13_arrive_changes_distort_and_compress_without_a_report() {
        let mut panel = FakePanel::default();

        assert!(panel.on_channel(0xB0, &[12, 90]).is_empty());
        assert!(panel.on_channel(0xB0, &[13, 127]).is_empty());

        assert_eq!(panel.mixer[MixerItem::Distort as usize], 90);
        assert_eq!(panel.mixer[MixerItem::Compress as usize], 127);
    }

    #[test]
    fn test_on_channel_when_a_sound_parameter_nrpn_arrives_reports_nothing() {
        let mut panel = FakePanel::default();

        assert!(nrpn(&mut panel, 34, 100).is_empty());
        assert!(nrpn(&mut panel, 230, 5).is_empty());
        assert_eq!(panel.mixer, MIXER_START);
    }
}
