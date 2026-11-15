//! Voice patch recipes (`"cpu": "dspic33f"`, same semantics as the Voice part of `tools/recipe.py`): the edits address
//! the 24-bit instruction words of the Voice image by their program counter (PC, 2 per word, 3 bytes each in the DSI
//! file). A literal word is our own; a word derived from DSI's code is a copy of the original image, possibly with one
//! instruction field set; goto / call (two words) and bra / rcall (PC-relative) are computed from their target. Every
//! read is from the ORIGINAL image. No DSI byte is embedded.
//!
//! Protected zones, never written by a recipe (the Voice's equivalent of the Main's flasher page): the reset
//! `goto 0x15F0` (PC 0-2), the update entry (PC 0x650E-0x651C: `0xF700` after `0xF6A5` calls the boot page) and
//! everything at or above PC 0xA400 (boot page and calibration, which an update never writes).

use crate::recipe::{set_fields, EditError, Field};

/// First PC an update never writes (boot page 41 and calibration page 42).
pub const PROTECTED_FROM: u32 = 0xA400;
/// The PC ranges kept as DSI has them: the reset `goto` and the update entry.
pub const KEPT: [(u32, u32, &str); 2] = [
    (0x0000, 0x0004, "the reset goto 0x15F0 (PC 0-2)"),
    (
        0x650E,
        0x651E,
        "the update entry 0xF6A5 / 0xF700 (PC 0x650E-0x651C)",
    ),
];

/// A jump a recipe computes from its target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JumpKind {
    /// Two words, absolute.
    Goto,
    /// Two words, absolute.
    Call,
    /// One word, relative to the next instruction.
    Bra,
    /// One word, relative to the next instruction.
    Rcall,
}

impl JumpKind {
    fn opcode(self) -> u32 {
        match self {
            Self::Goto => 0x04,
            Self::Call => 0x02,
            Self::Bra => 0x37,
            Self::Rcall => 0x07,
        }
    }

    fn absolute(self) -> bool {
        matches!(self, Self::Goto | Self::Call)
    }
}

/// What a Voice edit writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceOp {
    /// Our own 24-bit word, repeated `count` times.
    Word(u32),
    /// Words of the original image from PC `from` on; with fields to set, one word whose fields change.
    Copy { from: u32, set: &'static [Field] },
    /// goto / call / bra / rcall to `target`.
    Jump { kind: JumpKind, target: u32 },
}

/// One edit: `count` consecutive words from PC `address` on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VoiceEdit {
    pub address: u32,
    pub count: u32,
    pub op: VoiceOp,
}

/// The 24-bit word at `pc` of a Voice image (3 bytes per instruction, little-endian).
pub fn word_at(image: &[u8], pc: u32) -> Option<u32> {
    if !pc.is_multiple_of(2) {
        return None;
    }
    let at = (pc / 2) as usize * 3;
    let bytes = image.get(at..at + 3)?;
    Some(u32::from(bytes[0]) | u32::from(bytes[1]) << 8 | u32::from(bytes[2]) << 16)
}

/// Writes the 24-bit `value` at `pc`; `false` when the PC is outside the image or odd.
pub fn put_word(image: &mut [u8], pc: u32, value: u32) -> bool {
    let at = (pc / 2) as usize * 3;
    match image.get_mut(at..at + 3) {
        Some(slot) if pc.is_multiple_of(2) => {
            slot.copy_from_slice(&value.to_le_bytes()[..3]);
            true
        }
        _ => false,
    }
}

/// Why a PC may not be written, `None` when it may.
pub fn protected(pc: u32) -> Option<String> {
    if pc >= PROTECTED_FROM {
        return Some(format!(
            "PC {pc:#x} is at or above {PROTECTED_FROM:#x} (boot page, calibration: never written by an update)"
        ));
    }
    KEPT.iter()
        .find(|(low, high, _)| (*low..*high).contains(&pc))
        .map(|(_, _, what)| format!("PC {pc:#x} is in {what}"))
}

/// The two words of a goto / call, or the one of a bra / rcall.
fn jump_words(kind: JumpKind, at: u32, target: u32) -> Result<Vec<u32>, EditError> {
    if !target.is_multiple_of(2) || target >= 0x80_0000 {
        return Err(EditError::Malformed(at));
    }
    if kind.absolute() {
        return Ok(vec![kind.opcode() << 16 | target & 0xFFFE, target >> 16]);
    }
    let offset = (i64::from(target) - i64::from(at) - 2) / 2;
    if !(-0x8000..0x8000).contains(&offset) {
        return Err(EditError::Malformed(at));
    }
    Ok(vec![kind.opcode() << 16 | (offset as u32 & 0xFFFF)])
}

impl VoiceEdit {
    /// The PCs this edit owns, written or not: a goto / call owns its second word even when that word does not
    /// change, so that no other edit may write it.
    pub fn owned(&self) -> impl Iterator<Item = u32> + '_ {
        let span = match self.op {
            VoiceOp::Jump { kind, .. } if kind.absolute() => 2,
            VoiceOp::Jump { .. } => 1,
            _ => self.count,
        };
        (0..span).map(move |i| self.address.wrapping_add(2 * i))
    }

    /// The `(PC, new word)` pairs this edit writes, read from the original image where needed. The second word of a
    /// goto / call (the target's bits 22-16, zero in a 64 KB chip) is written only when it differs from the original.
    pub fn words(&self, original: &[u8]) -> Result<Vec<(u32, u32)>, EditError> {
        let single = matches!(
            self.op,
            VoiceOp::Jump { .. } | VoiceOp::Copy { set: [_, ..], .. }
        );
        if self.count == 0 || (single && self.count != 1) {
            return Err(EditError::Malformed(self.address));
        }
        for at in self.owned() {
            word_at(original, at).ok_or(EditError::OutOfImage(at))?;
        }
        match self.op {
            VoiceOp::Word(word) => Ok(self.owned().map(|at| (at, word)).collect()),
            VoiceOp::Copy { from, set } => self
                .owned()
                .enumerate()
                .map(|(i, at)| {
                    let source = from.wrapping_add(2 * i as u32);
                    let word = word_at(original, source).ok_or(EditError::OutOfImage(source))?;
                    set_fields(word, set)
                        .map(|value| (at, value))
                        .ok_or(EditError::Malformed(at))
                })
                .collect(),
            VoiceOp::Jump { kind, target } => {
                let words = jump_words(kind, self.address, target)?;
                let mut out = Vec::with_capacity(words.len());
                for (at, word) in self.owned().zip(words) {
                    let first = at == self.address;
                    if first || word_at(original, at) != Some(word) {
                        out.push((at, word));
                    }
                }
                Ok(out)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A Voice image of `pcs` instructions whose word at PC `2 i` is `i + 1`.
    fn image(pcs: usize) -> Vec<u8> {
        (0..pcs)
            .flat_map(|i| (i as u32 + 1).to_le_bytes()[..3].to_vec())
            .collect()
    }

    fn edit(address: u32, count: u32, op: VoiceOp) -> VoiceEdit {
        VoiceEdit { address, count, op }
    }

    #[test]
    fn test_word_at_when_in_the_image_returns_the_three_bytes_little_endian() {
        let image = image(8);

        assert_eq!(word_at(&image, 0), Some(1));
        assert_eq!(word_at(&image, 6), Some(4));
        assert_eq!(word_at(&image, 1), None, "odd PC");
        assert_eq!(word_at(&image, 16), None, "past the image");
    }

    #[test]
    fn test_put_word_when_valid_returns_true_and_the_word_reads_back() {
        let mut image = image(8);

        assert!(put_word(&mut image, 4, 0x12_3456));
        assert_eq!(word_at(&image, 4), Some(0x12_3456));
        assert!(!put_word(&mut image, 5, 1));
        assert!(!put_word(&mut image, 100, 1));
    }

    #[test]
    fn test_words_when_literal_repeated_returns_each_pc() {
        let e = edit(2, 3, VoiceOp::Word(0xAB_CDEF));

        assert_eq!(
            e.words(&image(8)),
            Ok(vec![(2, 0xAB_CDEF), (4, 0xAB_CDEF), (6, 0xAB_CDEF)])
        );
    }

    #[test]
    fn test_words_when_copy_sets_a_field_returns_the_word_with_it_changed() {
        static K10: [Field; 1] = [Field {
            lo: 4,
            width: 10,
            value: 22,
        }];
        let e = edit(8, 1, VoiceOp::Copy { from: 2, set: &K10 });

        // Word at PC 2 is 2: k10 (bits 4-13) becomes 22.
        assert_eq!(e.words(&image(8)), Ok(vec![(8, 2 & !0x3FF0 | 22 << 4)]));
    }

    #[test]
    fn test_words_when_goto_returns_two_words_only_when_the_second_changes() {
        let original = image(0x10);
        let e = edit(
            2,
            1,
            VoiceOp::Jump {
                kind: JumpKind::Goto,
                target: 0x15F0,
            },
        );

        // The second word at PC 4 is 3, not 0: it is written too.
        assert_eq!(e.words(&original), Ok(vec![(2, 0x04_15F0), (4, 0)]));
        let mut zero = original;
        put_word(&mut zero, 4, 0);
        assert_eq!(e.words(&zero), Ok(vec![(2, 0x04_15F0)]));
        assert_eq!(e.owned().collect::<Vec<_>>(), vec![2, 4], "still owns it");
    }

    #[test]
    fn test_words_when_bra_returns_the_offset_from_the_next_instruction() {
        let forward = edit(
            0,
            1,
            VoiceOp::Jump {
                kind: JumpKind::Bra,
                target: 0x10,
            },
        );
        let backward = edit(
            0x10,
            1,
            VoiceOp::Jump {
                kind: JumpKind::Rcall,
                target: 0,
            },
        );

        assert_eq!(forward.words(&image(0x20)), Ok(vec![(0, 0x37_0007)]));
        assert_eq!(
            backward.words(&image(0x20)),
            Ok(vec![(0x10, 0x07_FFF7)]),
            "(0 - 0x10 - 2) / 2 = -9 on 16 bits"
        );
    }

    #[test]
    fn test_words_when_target_odd_or_out_of_reach_or_count_wrong_returns_malformed() {
        let jump = |kind, target| edit(0, 1, VoiceOp::Jump { kind, target });
        let big = vec![0u8; 0x10000 * 3];

        assert_eq!(
            jump(JumpKind::Goto, 3).words(&big),
            Err(EditError::Malformed(0))
        );
        assert_eq!(
            jump(JumpKind::Bra, 0x20000).words(&big),
            Err(EditError::Malformed(0))
        );
        assert_eq!(
            edit(0, 0, VoiceOp::Word(1)).words(&big),
            Err(EditError::Malformed(0))
        );
        assert_eq!(
            edit(
                0,
                2,
                VoiceOp::Jump {
                    kind: JumpKind::Call,
                    target: 8
                }
            )
            .words(&big),
            Err(EditError::Malformed(0))
        );
    }

    #[test]
    fn test_words_when_outside_the_image_returns_out_of_image() {
        let e = edit(0x20, 1, VoiceOp::Word(1));
        let copy = edit(
            0,
            1,
            VoiceOp::Copy {
                from: 0x40,
                set: &[],
            },
        );

        assert_eq!(e.words(&image(8)), Err(EditError::OutOfImage(0x20)));
        assert_eq!(copy.words(&image(8)), Err(EditError::OutOfImage(0x40)));
    }

    #[test]
    fn test_protected_when_reset_update_entry_or_boot_page_returns_a_reason() {
        for pc in [0, 2, 0x650E, 0x651C, 0xA400, 0xFC00] {
            assert!(protected(pc).is_some(), "{pc:#x}");
        }
        for pc in [4, 0x650C, 0x651E, 0xA3FE] {
            assert_eq!(protected(pc), None, "{pc:#x}");
        }
    }
}
