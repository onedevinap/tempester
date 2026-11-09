//! Channel messages: notes, control changes and the NRPN reassembled from them.
//!
//! The state per channel is sticky like the machine's: after an NRPN completes, a lone CC 38 sets a new value of the
//! same number (`Docs/analyses/feature-remote-panel.md` §5), shown as such.

use super::{tables, Describer, Kind, Line};
use crate::nrpn::{CC_DATA_LSB, CC_DATA_MSB, CC_NRPN_LSB, CC_NRPN_MSB};
use crate::pad_notes::{self, PadNotes};
use crate::remote::{
    CC_KEY_PRESS, CC_KEY_RELEASE, CC_LED_OFF, CC_LED_ON, CC_STATE_REQUEST, FIRST_PAD_LED,
};
use crate::sound::params;

const CC_RPN_LSB: u8 = 100;
const CC_RPN_MSB: u8 = 101;
const NULL_NUMBER: u16 = 127 << 7 | 127;

/// The NRPN selection of one channel and the parts still waiting for their CC 38.
#[derive(Debug, Default)]
pub(super) struct ChannelState {
    msb: Option<u8>,
    lsb: Option<u8>,
    data_msb: Option<u8>,
    held: Vec<[u8; 3]>,
    held_since: u64,
}

impl ChannelState {
    pub(super) fn waiting_since(&self) -> Option<u64> {
        (!self.held.is_empty()).then_some(self.held_since)
    }

    /// The waiting parts as plain control changes.
    pub(super) fn flush(&mut self) -> Vec<Line> {
        self.held
            .drain(..)
            .map(|m| {
                let note = "NRPN part, no CC 38 followed";
                cc_line(&m, Some(note))
            })
            .collect()
    }

    fn hold(&mut self, now: u64, message: &[u8]) -> Vec<Line> {
        let controller = message[1];
        let repeated = self.held.iter().any(|m| m[1] == controller);
        let out = if repeated { self.flush() } else { Vec::new() };
        match controller {
            CC_NRPN_MSB => self.msb = Some(message[2]),
            CC_NRPN_LSB => self.lsb = Some(message[2]),
            _ => self.data_msb = Some(message[2]),
        }
        if self.held.is_empty() {
            self.held_since = now;
        }
        self.held.push([message[0], controller, message[2]]);
        out
    }

    fn complete(&mut self, message: &[u8]) -> Vec<Line> {
        let channel = message[0] & 0x0F;
        let Some((msb, lsb)) = self.msb.zip(self.lsb) else {
            return self.refuse(message, "no NRPN number selected on this channel");
        };
        let number = u16::from(msb) << 7 | u16::from(lsb);
        if number == NULL_NUMBER {
            return self.refuse(message, "NRPN deselected (127/127)");
        }
        let value = u16::from(self.data_msb.unwrap_or(0)) << 7 | u16::from(message[2]);
        let alone = self.held.is_empty();
        let mut raw: Vec<u8> = self.held.drain(..).flatten().collect();
        raw.extend_from_slice(message);
        let mut label = format!(
            "Ch {} NRPN {number} {} = {value}",
            channel + 1,
            nrpn_name(number)
        );
        if alone {
            label.push_str(" (CC 38 alone: number and MSB from before)");
        }
        vec![Line::new(Kind::Nrpn, label, &raw)]
    }

    fn refuse(&mut self, message: &[u8], why: &str) -> Vec<Line> {
        let mut out = self.flush();
        out.push(cc_line(message, Some(why)));
        out
    }
}

impl Describer {
    pub(super) fn channel_message(&mut self, now: u64, message: &[u8]) -> Vec<Line> {
        let status = message[0];
        let expected = if matches!(status & 0xF0, 0xC0 | 0xD0) {
            2
        } else {
            3
        };
        if message.len() != expected {
            let label = format!(
                "Ch {} message of {} bytes, {expected} expected",
                (status & 0x0F) + 1,
                message.len()
            );
            return vec![Line::new(Kind::Other, label, message)];
        }
        if status & 0xF0 == 0xB0 {
            return self.control_change(now, message);
        }
        vec![other_channel_line(message, &self.pad_notes)]
    }

    fn control_change(&mut self, now: u64, message: &[u8]) -> Vec<Line> {
        let state = &mut self.channels[usize::from(message[0] & 0x0F)];
        match message[1] {
            CC_NRPN_MSB | CC_NRPN_LSB | CC_DATA_MSB => state.hold(now, message),
            CC_DATA_LSB => state.complete(message),
            controller => {
                let mut out = state.flush();
                if matches!(controller, CC_RPN_LSB | CC_RPN_MSB) {
                    (state.msb, state.lsb, state.data_msb) = (None, None, None);
                }
                out.push(cc_line(message, None));
                out
            }
        }
    }
}

fn other_channel_line(message: &[u8], pads: &PadNotes) -> Line {
    let status = message[0];
    let channel = (status & 0x0F) + 1;
    let (kind, text) = match status & 0xF0 {
        0x80 => (
            Kind::Note,
            note_text("Note off", message[1], message[2], pads),
        ),
        0x90 if message[2] == 0 => (
            Kind::Note,
            note_text("Note off (velocity 0)", message[1], 0, pads),
        ),
        0x90 => (
            Kind::Note,
            note_text("Note on", message[1], message[2], pads),
        ),
        0xA0 => (
            Kind::Note,
            format!(
                "Key pressure {} = {}",
                pad_notes::note_name(message[1]),
                message[2]
            ),
        ),
        0xC0 => (Kind::Other, format!("Program change {}", message[1])),
        0xD0 => (Kind::Other, format!("Channel pressure {}", message[1])),
        _ => (
            Kind::Other,
            format!(
                "Pitch bend {}",
                i32::from(message[2]) << 7 | i32::from(message[1])
            ),
        ),
    };
    Line::new(kind, format!("Ch {channel} {text}"), message)
}

/// The pad is the one of the machine's own note table (`pad_notes`), as the app knows it.
fn note_text(what: &str, note: u8, velocity: u8, pads: &PadNotes) -> String {
    let pad = pads
        .pad_of(note)
        .map(|k| format!(", pad {}", pad_notes::pad_name(k)))
        .unwrap_or_default();
    format!(
        "{what} {} ({note}) velocity {velocity}{pad}",
        pad_notes::note_name(note)
    )
}

fn cc_line(message: &[u8], note: Option<&str>) -> Line {
    let (channel, controller, value) = ((message[0] & 0x0F) + 1, message[1], message[2]);
    let mut label = format!(
        "Ch {channel} CC {controller} {}",
        cc_text(controller, value)
    );
    if let Some(note) = note {
        label.push_str(&format!(" - {note}"));
    }
    Line::new(Kind::Control, label, message)
}

/// The name of a controller and what its value means: `Distort = 64`, `Key press: Play (code 22)`.
fn cc_text(controller: u8, value: u8) -> String {
    match controller {
        CC_KEY_PRESS | CC_KEY_RELEASE => {
            let verb = if controller == CC_KEY_PRESS {
                "pressed"
            } else {
                "released"
            };
            let name = tables::key_name(value).unwrap_or("unknown key");
            format!("Remote key {verb}: {name} (code {value})")
        }
        CC_LED_ON => format!("LED on: {}", led_text(value)),
        CC_LED_OFF => format!("LED off: {}", led_text(value)),
        CC_STATE_REQUEST => format!(
            "LED state request ({})",
            if value & 1 == 1 {
                "pad LEDs included"
            } else {
                "without pad LEDs"
            }
        ),
        _ => match cc_name(controller) {
            Some(name) => format!("{name} = {value}"),
            None => format!("= {value}"),
        },
    }
}

fn led_text(index: u8) -> String {
    if index >= FIRST_PAD_LED {
        format!("{index} (pad LED)")
    } else {
        index.to_string()
    }
}

fn cc_name(controller: u8) -> Option<&'static str> {
    Some(match controller {
        1 => "Mod wheel",
        2 => "Breath",
        4 => "Foot",
        6 => "Data entry MSB",
        7 => "Volume",
        11 => "Expression",
        12 => "Distort",
        13 => "Compress",
        19 => "Revert beat-wide parameters",
        20 => "All Osc Freq",
        21 => "Feedback",
        22 => "LP Freq",
        23 => "LP Reson",
        24 => "Audio Mod",
        25 => "HP Freq",
        26 => "Env Attack",
        27 => "Env Decay",
        32 => "Bank select LSB",
        38 => "Data entry LSB",
        64 => "Sustain",
        98 => "NRPN LSB",
        99 => "NRPN MSB",
        CC_RPN_LSB => "RPN LSB",
        CC_RPN_MSB => "RPN MSB",
        121 => "Reset controllers",
        123 => "All notes off",
        _ => return None,
    })
}

/// The parameter an NRPN number stands for.
fn nrpn_name(number: u16) -> String {
    if let Some(param) = params().iter().find(|p| u16::from(p.index) == number) {
        return if param.group.is_empty() {
            param.name.to_string()
        } else {
            format!("{} {}", param.group, param.name)
        };
    }
    match number {
        156 => "Osc 3 Sample".into(),
        157 => "Osc 4 Sample".into(),
        _ => tables::remote_control(number)
            .map(|name| format!("remote {name}"))
            .unwrap_or_else(|| "(not a Tempest parameter)".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nrpn::frame;

    fn feed_all(d: &mut Describer, now: u64, bytes: &[u8]) -> Vec<Line> {
        bytes
            .chunks(3)
            .flat_map(|m| d.feed(now, m))
            .collect::<Vec<_>>()
    }

    fn label_of(lines: &[Line]) -> String {
        assert_eq!(lines.len(), 1, "{lines:?}");
        lines[0].label.clone()
    }

    #[test]
    fn test_feed_when_note_on_and_off_returns_name_velocity_and_pad() {
        let mut d = Describer::new();

        let on = d.feed(0, &[0x99, 36, 100]);
        let off = d.feed(0, &[0x89, 50, 0]);
        let not_a_pad = d.feed(0, &[0x89, 51, 0]);
        let zero = d.feed(0, &[0x90, 79, 0]);
        let plain = d.feed(0, &[0x90, 79, 5]);

        assert_eq!(on[0].kind, Kind::Note);
        assert_eq!(label_of(&on), "Ch 10 Note on C2 (36) velocity 100, pad A1");
        assert_eq!(label_of(&off), "Ch 10 Note off D3 (50) velocity 0, pad A9");
        assert_eq!(label_of(&not_a_pad), "Ch 10 Note off D#3 (51) velocity 0");
        assert_eq!(
            label_of(&zero),
            "Ch 1 Note off (velocity 0) G5 (79) velocity 0"
        );
        assert_eq!(label_of(&plain), "Ch 1 Note on G5 (79) velocity 5");
    }

    #[test]
    fn test_feed_when_pad_notes_set_returns_the_pad_of_that_table_after_a_reset_too() {
        let mut d = Describer::new();
        let mut notes = PadNotes::default();
        notes.learn(16, 51);
        d.set_pad_notes(notes);
        d.reset();

        let line = d.feed(0, &[0x91, 51, 90]);

        assert_eq!(label_of(&line), "Ch 2 Note on D#3 (51) velocity 90, pad B1");
    }

    #[test]
    fn test_feed_when_note_outside_the_pads_returns_no_pad_name() {
        let mut d = Describer::new();

        assert!(!label_of(&d.feed(0, &[0x90, 35, 1])).contains("pad"));
        assert!(!label_of(&d.feed(0, &[0x90, 51, 1])).contains("pad"));
        assert!(!label_of(&d.feed(0, &[0x90, 90, 1])).contains("pad"));
    }

    #[test]
    fn test_feed_when_tempest_controllers_returns_their_names() {
        let mut d = Describer::new();

        assert_eq!(
            label_of(&d.feed(0, &[0xB0, 12, 64])),
            "Ch 1 CC 12 Distort = 64"
        );
        assert_eq!(
            label_of(&d.feed(0, &[0xB1, 13, 5])),
            "Ch 2 CC 13 Compress = 5"
        );
        assert_eq!(label_of(&d.feed(0, &[0xB0, 77, 5])), "Ch 1 CC 77 = 5");
    }

    #[test]
    fn test_feed_when_remote_panel_controllers_returns_key_and_led_wording() {
        let mut d = Describer::new();

        let press = label_of(&d.feed(0, &[0xB0, 102, 0]));
        let release = label_of(&d.feed(0, &[0xB0, 103, 0]));
        let unknown = label_of(&d.feed(0, &[0xB0, 102, 99]));

        assert_eq!(
            press,
            "Ch 1 CC 102 Remote key pressed: Playback (FX1) (code 0)"
        );
        assert!(
            release.contains("Remote key released: Playback (FX1)"),
            "{release}"
        );
        assert!(unknown.contains("unknown key (code 99)"), "{unknown}");
        assert_eq!(
            label_of(&d.feed(0, &[0xB0, 104, 5])),
            "Ch 1 CC 104 LED on: 5"
        );
        assert_eq!(
            label_of(&d.feed(0, &[0xB0, 105, 0x2C])),
            "Ch 1 CC 105 LED off: 44 (pad LED)"
        );
        assert_eq!(
            label_of(&d.feed(0, &[0xB0, 107, 1])),
            "Ch 1 CC 107 LED state request (pad LEDs included)"
        );
        assert!(label_of(&d.feed(0, &[0xB0, 107, 0])).contains("without pad LEDs"));
    }

    #[test]
    fn test_feed_when_full_nrpn_returns_one_line_with_number_name_value_and_channel() {
        let mut d = Describer::new();
        let bytes = frame(0, 34, 100);

        let lines = feed_all(&mut d, 0, &bytes);

        assert_eq!(lines.len(), 1, "{lines:?}");
        assert_eq!(lines[0].kind, Kind::Nrpn);
        assert_eq!(
            lines[0].label,
            "Ch 1 NRPN 34 Filtre passe-bas LP Freq = 100"
        );
        assert_eq!(lines[0].raw, "B0 63 00 B0 62 22 B0 06 00 B0 26 64");
    }

    #[test]
    fn test_feed_when_nrpn_value_is_14_bits_returns_the_whole_value() {
        let mut d = Describer::new();

        let lines = feed_all(&mut d, 0, &frame(4, 157, 463));

        assert_eq!(label_of(&lines), "Ch 5 NRPN 157 Osc 4 Sample = 463");
    }

    #[test]
    fn test_feed_when_remote_control_nrpn_returns_the_control_name() {
        let mut d = Describer::new();

        let lines = feed_all(&mut d, 0, &frame(0, 228, 200));

        assert_eq!(
            label_of(&lines),
            "Ch 1 NRPN 228 remote absolute 8: Volume = 200"
        );
    }

    #[test]
    fn test_feed_when_cc_38_alone_after_a_complete_nrpn_returns_the_same_number_flagged() {
        let mut d = Describer::new();
        feed_all(&mut d, 0, &frame(0, 34, 100));

        let lines = d.feed(10, &[0xB0, 38, 101]);

        assert_eq!(
            label_of(&lines),
            "Ch 1 NRPN 34 Filtre passe-bas LP Freq = 101 (CC 38 alone: number and MSB from before)"
        );
    }

    #[test]
    fn test_feed_when_cc_38_comes_without_any_number_returns_a_plain_cc_naming_the_problem() {
        let mut d = Describer::new();

        let lines = d.feed(0, &[0xB0, 38, 5]);

        assert_eq!(lines[0].kind, Kind::Control);
        assert_eq!(
            label_of(&lines),
            "Ch 1 CC 38 Data entry LSB = 5 - no NRPN number selected on this channel"
        );
    }

    #[test]
    fn test_feed_when_data_lsb_comes_before_the_number_returns_no_nrpn_ever() {
        let mut d = Describer::new();

        // Wrong order: the CC 38 first, then the number and the data MSB.
        let first = d.feed(0, &[0xB0, 38, 7]);
        let rest = feed_all(&mut d, 1, &[0xB0, 99, 0, 0xB0, 98, 34, 0xB0, 6, 0]);
        let late = d.flush_stale(1000);

        assert!(
            first[0].label.contains("no NRPN number selected"),
            "{first:?}"
        );
        assert!(rest.is_empty());
        assert_eq!(late.len(), 3, "{late:?}");
        assert!(late.iter().all(|l| l.kind == Kind::Control));
    }

    #[test]
    fn test_feed_when_number_msb_missing_returns_the_plain_cc_38() {
        let mut d = Describer::new();
        d.feed(0, &[0xB0, 6, 0]);

        let lines = d.feed(1, &[0xB0, 38, 9]);

        assert_eq!(lines.len(), 2, "{lines:?}");
        assert!(lines[0].label.contains("CC 6 Data entry MSB"));
        assert!(lines[1].label.contains("no NRPN number selected"));
    }

    #[test]
    fn test_feed_when_null_number_returns_deselected_note() {
        let mut d = Describer::new();
        feed_all(&mut d, 0, &[0xB0, 99, 127, 0xB0, 98, 127, 0xB0, 6, 1]);

        let lines = d.feed(1, &[0xB0, 38, 1]);

        assert!(
            lines.last().unwrap().label.contains("deselected"),
            "{lines:?}"
        );
        assert!(lines.iter().all(|l| l.kind == Kind::Control));
    }

    #[test]
    fn test_flush_stale_when_parts_wait_long_enough_returns_them_as_plain_cc() {
        let mut d = Describer::new();
        assert!(feed_all(&mut d, 100, &[0xB0, 99, 0, 0xB0, 98, 34, 0xB0, 6, 0]).is_empty());

        assert!(d.flush_stale(100 + super::super::STALE_MS - 1).is_empty());
        let lines = d.flush_stale(100 + super::super::STALE_MS);

        assert_eq!(lines.len(), 3, "{lines:?}");
        assert!(lines.iter().all(|l| l.label.contains("no CC 38 followed")));
        assert!(d.flush_stale(10_000).is_empty());
    }

    #[test]
    fn test_feed_when_other_cc_on_the_channel_interrupts_returns_parts_then_that_cc_in_order() {
        let mut d = Describer::new();
        d.feed(0, &[0xB0, 99, 0]);

        let lines = d.feed(1, &[0xB0, 12, 3]);

        assert_eq!(lines.len(), 2);
        assert!(lines[0].label.contains("CC 99"));
        assert!(lines[1].label.contains("Distort"));
    }

    #[test]
    fn test_feed_when_two_channels_interleave_returns_each_nrpn_on_its_own_channel() {
        let mut d = Describer::new();
        let (a, b) = (frame(0, 34, 1), frame(1, 35, 2));
        let mut lines = Vec::new();
        for k in 0..4 {
            lines.extend(d.feed(0, &a[k * 3..k * 3 + 3]));
            lines.extend(d.feed(0, &b[k * 3..k * 3 + 3]));
        }

        assert_eq!(lines.len(), 2, "{lines:?}");
        assert!(lines[0].label.starts_with("Ch 1 NRPN 34"));
        assert!(lines[1].label.starts_with("Ch 2 NRPN 35"));
    }

    #[test]
    fn test_feed_when_message_has_the_wrong_length_returns_a_malformed_note() {
        let mut d = Describer::new();

        let lines = d.feed(0, &[0xB0, 12]);

        assert_eq!(lines[0].kind, Kind::Other);
        assert_eq!(lines[0].label, "Ch 1 message of 2 bytes, 3 expected");
    }

    #[test]
    fn test_feed_when_program_pressure_and_bend_returns_plain_labels() {
        let mut d = Describer::new();

        assert_eq!(label_of(&d.feed(0, &[0xC2, 7])), "Ch 3 Program change 7");
        assert_eq!(label_of(&d.feed(0, &[0xD0, 7])), "Ch 1 Channel pressure 7");
        assert_eq!(label_of(&d.feed(0, &[0xE0, 0, 64])), "Ch 1 Pitch bend 8192");
    }
}
