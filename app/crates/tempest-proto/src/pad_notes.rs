//! Which MIDI note plays which pad. The Tempest keeps its own table (System > « MIDI Note : Pad A1-B16 », manual
//! p. 63, 4-35): one note per pad of banks A and B, each note on one pad only. The app must use that table, not a
//! formula: on the author's machine pad k is NOT note 36 + k.
//!
//! The default is the table of the author's machine on 09/11/2026, as its screen shows it and as its pads play it
//! (pads tapped in 16 Sounds, notes read on Remote Pad OUT, then checked by the author on the System page): A1-A15 the
//! white keys from C2 (36-60), A16 61, B1-B16 62-77. Whether a factory default exists is not known (no such table
//! was found in the Main flash).
//!
//! A note the machine sends is not always a pad's table entry: tapping B1 sent note 60 (A15's note) while the
//! machine's screen shows B1 = 62 [P measured twice]. B1 was the Sequencer Sound with Sequencer Out on channel 1, so it
//! also played its pitch there (`Docs/analyses/bugs-open-2026-11.md` §1, confirmed on the machine: Off, then 62 on
//! Remote Pad OUT); the app now watches only the LEDs' channel (`machine/pad_map.rs`). So learning never takes a note
//! that another pad already has: the table stays one note per pad and a wrong report does not move other pads.
//!
//! Pads are numbered 0-31: 0-15 are A1-A16, 16-31 are B1-B16 (the Sound Bank B key adds 16 to a pad,
//! `refs/panel_keys.tsv`).

use std::fmt;

/// Pads of the two banks.
pub const PAD_COUNT: usize = 32;
/// Pads of one bank.
pub const BANK_PADS: usize = 16;

/// The author's machine (see the module notes): A1-A15 white keys from C2, A16 61, B1-B16 62-77.
const DEFAULT_NOTES: [u8; PAD_COUNT] = [
    36, 38, 40, 41, 43, 45, 47, 48, 50, 52, 53, 55, 57, 59, 60, 61, //
    62, 63, 64, 65, 66, 67, 68, 69, 70, 71, 72, 73, 74, 75, 76, 77,
];

/// Why a table or an entry was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PadNotesError {
    BadPad(usize),
    BadNote(u8),
    /// The note is already the note of another pad.
    Taken {
        note: u8,
        pad: usize,
    },
    /// A table that is not 32 notes 0-127 separated by commas.
    BadText,
}

impl fmt::Display for PadNotesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadPad(pad) => write!(f, "pad {pad} is not 0-{}", PAD_COUNT - 1),
            Self::BadNote(note) => write!(f, "note {note} is above 127"),
            Self::Taken { note, pad } => write!(
                f,
                "note {} is already the note of pad {} (the machine plays each note on one pad only)",
                note_name(*note),
                pad_name(*pad)
            ),
            Self::BadText => write!(
                f,
                "a pad note table is {PAD_COUNT} different notes 0-127 separated by commas"
            ),
        }
    }
}

/// The note of every pad, always valid: 32 different notes 0-127.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PadNotes([u8; PAD_COUNT]);

impl Default for PadNotes {
    fn default() -> Self {
        Self(DEFAULT_NOTES)
    }
}

impl PadNotes {
    /// A table from 32 notes; refused when a note is above 127 or appears twice.
    pub fn new(notes: [u8; PAD_COUNT]) -> Result<Self, PadNotesError> {
        for (pad, &note) in notes.iter().enumerate() {
            if note > 127 {
                return Err(PadNotesError::BadNote(note));
            }
            if let Some(first) = notes[..pad].iter().position(|n| *n == note) {
                return Err(PadNotesError::Taken { note, pad: first });
            }
        }
        Ok(Self(notes))
    }

    /// The note of pad 0-31.
    pub fn note(&self, pad: usize) -> Option<u8> {
        self.0.get(pad).copied()
    }

    /// The pad 0-31 that `note` plays, if any.
    pub fn pad_of(&self, note: u8) -> Option<usize> {
        self.0.iter().position(|n| *n == note)
    }

    pub fn notes(&self) -> [u8; PAD_COUNT] {
        self.0
    }

    /// The notes of the 16 pads of bank A.
    pub fn bank_a(&self) -> [u8; BANK_PADS] {
        std::array::from_fn(|pad| self.0[pad])
    }

    /// Gives pad `pad` the note `note`; refused when another pad has it (the machine plays each note on one pad).
    pub fn set(&mut self, pad: usize, note: u8) -> Result<(), PadNotesError> {
        if pad >= PAD_COUNT {
            return Err(PadNotesError::BadPad(pad));
        }
        if note > 127 {
            return Err(PadNotesError::BadNote(note));
        }
        match self.pad_of(note) {
            Some(other) if other != pad => Err(PadNotesError::Taken { note, pad: other }),
            _ => {
                self.0[pad] = note;
                Ok(())
            }
        }
    }

    /// What the machine showed: `note` plays pad `pad`. Taken only when no other pad has this note: the machine can
    /// report a note that is not the pad's (module notes, B1), and swapping would then move a right note to a wrong
    /// pad. A real change of two notes is typed on the Machine page. `true` when the table changed.
    pub fn learn(&mut self, pad: usize, note: u8) -> bool {
        if pad >= PAD_COUNT || note > 127 || self.0[pad] == note || self.pad_of(note).is_some() {
            return false;
        }
        self.0[pad] = note;
        true
    }

    /// The table as kept in the settings file: 32 notes separated by commas.
    pub fn render(&self) -> String {
        self.0
            .iter()
            .map(u8::to_string)
            .collect::<Vec<_>>()
            .join(",")
    }

    /// Reads `render`'s text.
    pub fn parse(text: &str) -> Result<Self, PadNotesError> {
        let notes: Vec<u8> = text
            .split(',')
            .map(|part| part.trim().parse::<u8>())
            .collect::<Result<_, _>>()
            .map_err(|_| PadNotesError::BadText)?;
        let notes: [u8; PAD_COUNT] = notes.try_into().map_err(|_| PadNotesError::BadText)?;
        Self::new(notes)
    }
}

/// `C2` for 36 (the Tempest's and the monitor's octave numbering: 60 is C4).
pub fn note_name(note: u8) -> String {
    const NAMES: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    let octave = i32::from(note / 12) - 1;
    format!("{}{octave}", NAMES[usize::from(note % 12)])
}

/// `A1` for pad 0, `B16` for pad 31.
pub fn pad_name(pad: usize) -> String {
    let bank = if pad < BANK_PADS { 'A' } else { 'B' };
    format!("{bank}{}", pad % BANK_PADS + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_when_read_returns_the_table_of_the_author_machine() {
        let notes = PadNotes::default();

        let screen = [
            36, 38, 40, 41, 43, 45, 47, 48, 50, 52, 53, 55, 57, 59, 60, 61, //
            62, 63, 64, 65, 66, 67, 68, 69, 70, 71, 72, 73, 74, 75, 76, 77,
        ];
        for (pad, note) in screen.into_iter().enumerate() {
            assert_eq!(notes.note(pad), Some(note), "pad {}", pad_name(pad));
        }
        assert_eq!(notes.pad_of(50), Some(8), "note 50 is pad A9, not A15");
        assert!(PadNotes::new(notes.notes()).is_ok(), "32 different notes");
    }

    #[test]
    fn test_set_when_note_taken_by_another_pad_returns_error_and_keeps_the_table() {
        let mut notes = PadNotes::default();

        let refused = notes.set(0, 38);

        assert_eq!(refused, Err(PadNotesError::Taken { note: 38, pad: 1 }));
        assert_eq!(notes, PadNotes::default());
        assert!(notes.set(0, 37).is_ok());
        assert_eq!(notes.note(0), Some(37));
        assert!(notes.set(0, 37).is_ok(), "its own note again");
        assert_eq!(notes.set(32, 1), Err(PadNotesError::BadPad(32)));
        assert_eq!(notes.set(0, 128), Err(PadNotesError::BadNote(128)));
    }

    #[test]
    fn test_learn_when_note_belongs_to_another_pad_returns_false_and_keeps_the_table() {
        // What the author's machine did: B1 (pad 16) reported note 60, which is A15's (pad 14).
        let mut notes = PadNotes::default();

        assert!(!notes.learn(16, 60), "60 is A15's: refused");

        assert_eq!(notes, PadNotes::default(), "neither A15 nor B1 moved");
        assert!(!notes.learn(14, 60), "nothing new");
        assert!(notes.learn(3, 99), "a note no pad had");
        assert_eq!(notes.note(3), Some(99));
        assert_eq!(notes.pad_of(41), None);
        assert!(PadNotes::new(notes.notes()).is_ok());
    }

    #[test]
    fn test_parse_when_rendered_table_returns_it_and_refuses_bad_texts() {
        let mut notes = PadNotes::default();
        notes.learn(0, 100);

        assert_eq!(PadNotes::parse(&notes.render()), Ok(notes));
        let duplicate = PadNotes::default().render().replacen("38", "36", 1);
        for bad in [
            "",
            "36,38",
            "x",
            &duplicate,
            &format!("{},200", "1,".repeat(31)),
        ] {
            assert!(PadNotes::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn test_names_when_formatted_returns_tempest_octaves_and_banks() {
        assert_eq!(note_name(36), "C2");
        assert_eq!(note_name(50), "D3");
        assert_eq!(note_name(61), "C#4");
        assert_eq!(note_name(0), "C-1");
        assert_eq!(pad_name(0), "A1");
        assert_eq!(pad_name(31), "B16");
    }
}
