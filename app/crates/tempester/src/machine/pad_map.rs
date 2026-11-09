//! The machine's note of each pad (`tempest_proto::pad_notes`): what the app plays a pad with, kept in the settings
//! file (`pad_notes`), edited on the Machine page and learned from the machine.
//!
//! Learning: a pad tapped on the machine sends its note on Remote Pad OUT, and the remote panel (Tempester 1.6.0.0)
//! then reports the pad LED of the selected pad (`CC 104 0x2C + n`). When exactly one note-on arrives within
//! `PAIRING_WINDOW` of a pad LED turning on, before or after it, that note is pad n (bank B when the Sound Bank B
//! LED 0x12 was last reported on, else bank A). Two notes in the window (a beat playing, a chord) prove nothing and
//! are ignored, so a pairing is only decided once the window after the LED has passed (`settle_pad_learn`).

use std::time::{Duration, Instant};

use tempest_proto::pad_notes::{PadNotes, BANK_PADS, PAD_COUNT};
use tempest_proto::remote::{LedChange, FIRST_PAD_LED};

use super::Machine;
use crate::command::Command;
use crate::error::AppError;
use crate::event::Event;
use crate::storage::settings::KEY_PAD_NOTES;

/// How close a note and a pad LED must be to belong to the same tap.
pub(super) const PAIRING_WINDOW: Duration = Duration::from_millis(50);
/// The LED of the Sound Bank B key (`refs/panel_leds.tsv`): lit, the pads are B1-B16.
const SOUND_BANK_B_LED: u8 = 0x12;

/// A pad LED that lit, waiting for the end of its window.
#[derive(Debug, Clone, Copy)]
struct LitPad {
    pad: usize,
    at: Instant,
}

/// What learning needs to remember between two passes of the worker.
#[derive(Debug, Default)]
pub(super) struct PadLearn {
    /// Note-ons from the machine, newest last, none older than the window.
    notes: Vec<(u8, Instant)>,
    lit: Option<LitPad>,
    /// The Sound Bank B LED as last reported; unknown counts as bank A.
    bank_b: bool,
}

impl PadLearn {
    fn forget_before(&mut self, now: Instant) {
        self.notes
            .retain(|(_, at)| now.saturating_duration_since(*at) <= PAIRING_WINDOW);
    }

    /// The one note within the window around `lit`, if there is exactly one.
    fn single_note_around(&self, lit: Instant) -> Option<u8> {
        let mut near = self.notes.iter().filter(|(_, at)| {
            let gap = if *at >= lit { *at - lit } else { lit - *at };
            gap <= PAIRING_WINDOW
        });
        let (note, _) = near.next()?;
        near.next().is_none().then_some(*note)
    }
}

/// The table for the page: sent with `state`, on request and after every change.
pub(super) fn pad_notes_event(notes: &PadNotes) -> Event {
    Event::new("pad_notes").with("notes", notes.notes().to_vec())
}

impl Machine {
    pub(super) fn handle_pad_map(&mut self, command: Command) -> Option<Command> {
        match command {
            Command::Pad { on, pad, velocity } => {
                self.guard("pad", |m| m.play_pad(on, pad, velocity));
            }
            Command::PadNotes => self.emit(pad_notes_event(&self.pad_notes)),
            Command::SetPadNote { pad, note } => {
                self.guard("pad_note", |m| m.set_pad_note(pad, note));
            }
            Command::ResetPadNotes => {
                self.guard("pad_note", |m| m.use_pad_notes(PadNotes::default()))
            }
            other => return Some(other),
        }
        None
    }

    /// The note of pad 0-31 in the app's table.
    pub(super) fn pad_note(&self, pad: u8) -> Result<u8, AppError> {
        self.pad_notes
            .note(usize::from(pad))
            .ok_or_else(|| AppError::Invalid(format!("pad {pad} is not 0-{}", PAD_COUNT - 1)))
    }

    fn play_pad(&mut self, on: bool, pad: u8, velocity: u8) -> Result<(), AppError> {
        let note = self.pad_note(pad)?;
        self.send_note(on, note, velocity)
    }

    fn set_pad_note(&mut self, pad: u8, note: u8) -> Result<(), AppError> {
        let mut notes = self.pad_notes;
        notes
            .set(usize::from(pad), note)
            .map_err(|e| AppError::Invalid(e.to_string()))?;
        self.use_pad_notes(notes)
    }

    /// Keeps `notes` (settings file, read before written), and gives them to the sequencer and the monitor.
    fn use_pad_notes(&mut self, notes: PadNotes) -> Result<(), AppError> {
        self.storage
            .update_settings(|s| s.set(KEY_PAD_NOTES, &notes.render()))?;
        self.apply_pad_notes(notes);
        self.emit(pad_notes_event(&notes));
        Ok(())
    }

    /// Every part of the worker that turns a pad into a note uses `notes` from now on.
    pub(super) fn apply_pad_notes(&mut self, notes: PadNotes) {
        self.pad_notes = notes;
        self.sequencer
            .update(|settings| settings.notes = notes.bank_a());
        self.monitor.set_pad_notes(notes);
    }

    /// A channel message from the machine: a note-on is remembered for pairing.
    pub(super) fn observe_note(&mut self, now: Instant, status: u8, data: &[u8]) {
        if let (0x90, &[note, velocity]) = (status & 0xF0, data) {
            if velocity > 0 {
                self.pad_learn.notes.push((note, now));
            }
        }
    }

    /// A LED change from the machine: the Sound Bank B LED sets the bank, a pad LED turning on starts a pairing.
    pub(super) fn observe_led(&mut self, now: Instant, change: LedChange) {
        if change.index == SOUND_BANK_B_LED {
            self.pad_learn.bank_b = change.on;
            return;
        }
        let Some(place) = change.index.checked_sub(FIRST_PAD_LED) else {
            return;
        };
        if !change.on || usize::from(place) >= BANK_PADS {
            return;
        }
        self.settle_pad_learn(now, true);
        let bank = if self.pad_learn.bank_b { BANK_PADS } else { 0 };
        self.pad_learn.lit = Some(LitPad {
            pad: usize::from(place) + bank,
            at: now,
        });
    }

    /// Decides the pairing of the last lit pad once its window has passed (`force`: a new pad lit, decide now),
    /// then forgets the notes too old to pair with anything.
    pub(super) fn settle_pad_learn(&mut self, now: Instant, force: bool) {
        if let Some(lit) = self.pad_learn.lit {
            if force || now.saturating_duration_since(lit.at) > PAIRING_WINDOW {
                self.pad_learn.lit = None;
                if let Some(note) = self.pad_learn.single_note_around(lit.at) {
                    self.learn_pad(lit.pad, note);
                }
            }
        }
        let keep_from = self.pad_learn.lit.map_or(now, |lit| lit.at);
        self.pad_learn.forget_before(keep_from);
    }

    fn learn_pad(&mut self, pad: usize, note: u8) {
        self.emit(
            Event::new("pad_learned")
                .with("pad", pad)
                .with("note", note),
        );
        let mut notes = self.pad_notes;
        if notes.learn(pad, note) {
            self.guard("pad_note", |m| m.use_pad_notes(notes));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json::Json;
    use crate::machine::testkit::TestRig;
    use tempest_proto::remote::CC_LED_ON;

    fn led(index: u8, on: bool) -> LedChange {
        LedChange { index, on }
    }

    fn notes_of(event: &Event) -> Vec<i64> {
        match event.field("notes") {
            Some(Json::Arr(items)) => items
                .iter()
                .map(|item| match item {
                    Json::Int(n) => *n,
                    other => panic!("{other:?}"),
                })
                .collect(),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn test_pad_when_pressed_returns_the_note_of_the_table_not_36_plus_pad() {
        let mut rig = TestRig::connected("pad-note");
        let before = rig.transport().sent().len();

        rig.machine.handle(Command::Pad {
            on: true,
            pad: 14,
            velocity: 100,
        });
        rig.machine.handle(Command::Pad {
            on: false,
            pad: 14,
            velocity: 0,
        });

        assert_eq!(
            rig.transport().sent()[before..],
            vec![vec![0x90, 60, 100], vec![0x80, 60, 0]],
            "pad A15 is note 60 on the author's machine, not 50"
        );
    }

    #[test]
    fn test_set_pad_note_when_note_taken_returns_error_and_when_free_saves_and_plays_it() {
        let mut rig = TestRig::connected("pad-note-set");

        rig.machine.handle(Command::SetPadNote { pad: 0, note: 38 });
        let refused = rig.expect("error");
        rig.machine.handle(Command::SetPadNote { pad: 0, note: 37 });
        let saved = rig.expect("pad_notes");
        let before = rig.transport().sent().len();
        rig.machine.handle(Command::Pad {
            on: true,
            pad: 0,
            velocity: 9,
        });

        assert_eq!(refused.field("during"), Some(&Json::from("pad_note")));
        assert_eq!(notes_of(&saved)[0], 37);
        assert_eq!(
            rig.machine.storage.load_settings().pad_notes().note(0),
            Some(37)
        );
        assert_eq!(rig.transport().sent()[before..], vec![vec![0x90, 37, 9]]);
    }

    #[test]
    fn test_reset_pad_notes_when_learned_returns_the_default_table() {
        let mut rig = TestRig::connected("pad-note-reset");
        rig.machine
            .handle(Command::SetPadNote { pad: 3, note: 100 });

        rig.machine.handle(Command::ResetPadNotes);

        assert_eq!(
            rig.machine.storage.load_settings().pad_notes(),
            PadNotes::default()
        );
        assert_eq!(rig.machine.pad_notes, PadNotes::default());
    }

    #[test]
    fn test_learn_when_one_note_then_its_pad_led_returns_the_pairing_saved_and_used() {
        let mut rig = TestRig::connected("pad-learn");
        let start = Instant::now();

        rig.machine.observe_note(start, 0x91, &[99, 100]);
        rig.machine.observe_led(
            start + Duration::from_millis(10),
            led(FIRST_PAD_LED + 14, true),
        );
        rig.machine
            .settle_pad_learn(start + Duration::from_millis(80), false);

        let learned = rig.expect("pad_learned");
        assert_eq!(learned.field("pad"), Some(&Json::from(14usize)));
        assert_eq!(learned.field("note"), Some(&Json::from(99u8)));
        assert_eq!(rig.machine.pad_notes.note(14), Some(99));
        assert_eq!(
            rig.machine.storage.load_settings().pad_notes().note(14),
            Some(99)
        );
        assert_eq!(rig.machine.sequencer.get().notes[14], 99);
    }

    /// The author's machine reported B1 with A15's note (60): a note another pad has is not taken, nothing moves.
    #[test]
    fn test_learn_when_note_is_another_pads_keeps_the_table_and_the_settings() {
        let mut rig = TestRig::connected("pad-learn-taken");
        let start = Instant::now();

        rig.machine.observe_led(start, led(SOUND_BANK_B_LED, true));
        rig.machine.observe_note(start, 0x91, &[60, 100]);
        rig.machine
            .observe_led(start + Duration::from_millis(10), led(FIRST_PAD_LED, true));
        rig.machine
            .settle_pad_learn(start + Duration::from_millis(80), false);

        assert_eq!(rig.machine.pad_notes, PadNotes::default());
        assert_eq!(
            rig.machine.storage.load_settings().pad_notes(),
            PadNotes::default()
        );
        assert_eq!(rig.machine.pad_notes.note(14), Some(60), "A15 keeps 60");
        assert_eq!(rig.machine.pad_notes.note(16), Some(62), "B1 keeps 62");
    }

    #[test]
    fn test_learn_when_sound_bank_b_lit_returns_a_bank_b_pad() {
        let mut rig = TestRig::connected("pad-learn-b");
        let start = Instant::now();

        rig.machine.observe_led(start, led(SOUND_BANK_B_LED, true));
        rig.machine.observe_note(start, 0x91, &[30, 100]);
        rig.machine.observe_led(start, led(FIRST_PAD_LED + 1, true));
        rig.machine
            .settle_pad_learn(start + Duration::from_millis(60), false);

        assert_eq!(rig.machine.pad_notes.note(17), Some(30));
    }

    #[test]
    fn test_learn_when_ambiguous_late_or_not_a_pad_returns_no_change() {
        let mut rig = TestRig::connected("pad-learn-none");
        let start = Instant::now();
        let ms = |n| start + Duration::from_millis(n);

        // Two notes around one LED: a beat playing.
        rig.machine.observe_note(ms(0), 0x91, &[50, 100]);
        rig.machine.observe_led(ms(5), led(FIRST_PAD_LED + 2, true));
        rig.machine.observe_note(ms(30), 0x91, &[52, 100]);
        rig.machine.settle_pad_learn(ms(100), false);
        // A note too long before its LED, a note-on of velocity 0, a LED turning off, a key LED.
        rig.machine.observe_note(ms(200), 0x91, &[70, 100]);
        rig.machine.observe_note(ms(290), 0x91, &[71, 0]);
        rig.machine
            .observe_led(ms(300), led(FIRST_PAD_LED + 3, true));
        rig.machine
            .observe_led(ms(300), led(FIRST_PAD_LED + 4, false));
        rig.machine.observe_led(ms(300), led(0x10, true));
        rig.machine.settle_pad_learn(ms(400), false);

        assert_eq!(rig.machine.pad_notes, PadNotes::default());
        assert!(rig.events().iter().all(|e| e.name != "pad_learned"));
    }

    #[test]
    fn test_learn_when_window_not_over_waits_for_a_second_note() {
        let mut rig = TestRig::connected("pad-learn-wait");
        let start = Instant::now();
        let ms = |n| start + Duration::from_millis(n);

        rig.machine.observe_note(ms(0), 0x91, &[90, 100]);
        rig.machine.observe_led(ms(1), led(FIRST_PAD_LED, true));
        rig.machine.settle_pad_learn(ms(20), false);
        rig.machine.observe_note(ms(40), 0x91, &[91, 100]);
        rig.machine.settle_pad_learn(ms(100), false);

        assert_eq!(rig.machine.pad_notes, PadNotes::default());
    }

    /// The whole path: bytes from the machine through `poll_incoming` (the window then ends without a wait), then the
    /// app plays the learned note.
    #[test]
    fn test_poll_incoming_when_machine_sends_a_tap_learns_it_and_the_pad_plays_the_new_note() {
        let mut rig = TestRig::connected("pad-learn-poll");

        rig.transport()
            .push_incoming(&[0x91, 99, 100, 0xB1, CC_LED_ON, FIRST_PAD_LED + 5]);
        rig.machine.poll_incoming();
        rig.machine
            .settle_pad_learn(Instant::now() + PAIRING_WINDOW * 2, false);
        let before = rig.transport().sent().len();
        rig.machine.handle(Command::Pad {
            on: true,
            pad: 5,
            velocity: 100,
        });

        assert_eq!(rig.transport().sent()[before..], vec![vec![0x90, 99, 100]]);
    }

    /// A table far from both 36 + pad and the white keys.
    fn scrambled() -> PadNotes {
        PadNotes::new(std::array::from_fn(|k| ((k * 7 + 20) % 128) as u8))
            .expect("32 different notes")
    }

    /// The scripted machine with the `scrambled` table, and the app given `app` as its table.
    fn rig_on_scrambled_machine(label: &str, app: PadNotes) -> TestRig {
        let connector = crate::machine::connector::FakeConnector::with_pad_notes(scrambled());
        let mut rig = TestRig::with_connector(label, connector);
        rig.machine.handle(Command::Connect(None));
        rig.machine.guard("test", |m| m.use_pad_notes(app));
        rig.machine.handle(Command::PanelState { pads: true });
        rig.machine.poll_incoming();
        rig.events();
        rig
    }

    fn lit_pads(rig: &mut TestRig) -> Vec<String> {
        rig.machine.poll_incoming();
        rig.events()
            .iter()
            .filter(|e| e.name == "leds")
            .map(Event::render)
            .collect()
    }

    #[test]
    fn test_pad_when_app_and_machine_share_a_table_lights_the_pressed_pad_on_the_fake() {
        let mut rig = rig_on_scrambled_machine("pad-fake", scrambled());

        rig.machine.handle(Command::Pad {
            on: true,
            pad: 9,
            velocity: 100,
        });

        let lit = lit_pads(&mut rig);
        assert_eq!(
            lit,
            vec![r#"{"event":"leds","changes":[[53,1]]}"#.to_string()],
            "LED 0x2C + 9"
        );
    }

    /// Counter-proof: an app that plays its default table on a machine with another table lights another pad (or
    /// none), exactly the bug seen on the author's machine.
    #[test]
    fn test_pad_when_app_table_differs_from_the_machine_misses_the_pressed_pad() {
        let mut rig = rig_on_scrambled_machine("pad-fake-wrong", PadNotes::default());

        rig.machine.handle(Command::Pad {
            on: true,
            pad: 9,
            velocity: 100,
        });

        let lit = lit_pads(&mut rig);
        assert!(!lit.iter().any(|e| e.contains("[53,1]")), "{lit:?}");
    }

    #[test]
    fn test_probability_when_app_and_machine_share_a_table_lands_on_that_pad_of_the_fake_beat() {
        let mut rig = rig_on_scrambled_machine("prob-fake-map", scrambled());

        rig.machine.handle(Command::PadProbability {
            pad: 6,
            percent: 30,
        });
        rig.machine.handle(Command::FetchBeat);

        let beat = rig.expect("beat");
        let Some(Json::Arr(values)) = beat.field("play_probability") else {
            panic!("no play_probability in {}", beat.render());
        };
        assert_eq!(values[6], 30u8.into());
    }
}
