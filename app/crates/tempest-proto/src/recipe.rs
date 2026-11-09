//! Patch recipes: the edits of a patch as operations on the user's own DSI image, never DSI bytes
//! (`Docs/patch-recipe-format.md`, same semantics as `tools/recipe.py`). Our words are literal; a word derived from
//! DSI's code is a copy of the original image, possibly with one instruction field set; a j / jal is computed from its
//! target. Every read is from the ORIGINAL image, so recipes do not depend on each other or on their order.

/// Address of the first byte of the Main image.
pub const BASE: u32 = 0x9D00_0000;

/// One instruction field set by a copy: bits `lo .. lo + width` of the word take `value`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Field {
    pub lo: u8,
    pub width: u8,
    pub value: u32,
}

/// What an edit writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    /// Our own word (its value, stored little-endian), repeated `count` times.
    Word(u32),
    /// Words of the original image from `from` on; with fields to set, one word whose fields change.
    Copy { from: u32, set: &'static [Field] },
    /// `j target` (link false) or `jal target` (link true).
    Jump { target: u32, link: bool },
}

/// One edit of a recipe: `count` consecutive words from `address` on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Edit {
    pub address: u32,
    pub count: u32,
    pub op: Op,
}

/// Why an edit cannot be resolved on an image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditError {
    /// A word written or read outside the image (or not word aligned).
    OutOfImage(u32),
    /// A malformed edit: jump target out of the 256 MB region, field outside the word or value too wide,
    /// a count of zero or a count on a jump or a field copy.
    Malformed(u32),
}

impl EditError {
    /// The address the error is about.
    pub fn address(self) -> u32 {
        match self {
            Self::OutOfImage(a) | Self::Malformed(a) => a,
        }
    }
}

impl Edit {
    /// Every address this edit writes, without reading any image.
    pub fn addresses(&self) -> impl Iterator<Item = u32> + '_ {
        (0..self.count).map(move |i| self.address.wrapping_add(4 * i))
    }

    /// The `(address, new word)` pairs this edit writes, read from the original image where needed.
    pub fn words(&self, original: &[u8]) -> Result<Vec<(u32, u32)>, EditError> {
        let single = matches!(self.op, Op::Jump { .. } | Op::Copy { set: [_, ..], .. });
        if self.count == 0 || (single && self.count != 1) {
            return Err(EditError::Malformed(self.address));
        }
        let mut out = Vec::with_capacity(self.count as usize);
        for (i, at) in self.addresses().enumerate() {
            if offset(at, original.len()).is_none() {
                return Err(EditError::OutOfImage(at));
            }
            let value = match self.op {
                Op::Word(w) => w,
                Op::Jump { target, link } => jump(at, target, link)?,
                Op::Copy { from, set } => {
                    let source = from.wrapping_add(4 * i as u32);
                    let word = word_at(original, source).ok_or(EditError::OutOfImage(source))?;
                    set_fields(word, set).ok_or(EditError::Malformed(at))?
                }
            };
            out.push((at, value));
        }
        Ok(out)
    }
}

/// The byte offset of the word at `address` in an image of `len` bytes.
pub fn offset(address: u32, len: usize) -> Option<usize> {
    let at = address.checked_sub(BASE)? as usize;
    (address.is_multiple_of(4) && at + 4 <= len).then_some(at)
}

/// The original word at `address`, little-endian.
pub fn word_at(image: &[u8], address: u32) -> Option<u32> {
    let at = offset(address, image.len())?;
    let bytes: [u8; 4] = image[at..at + 4].try_into().ok()?;
    Some(u32::from_le_bytes(bytes))
}

fn jump(at: u32, target: u32, link: bool) -> Result<u32, EditError> {
    if !target.is_multiple_of(4) || target & 0xF000_0000 != at.wrapping_add(4) & 0xF000_0000 {
        return Err(EditError::Malformed(at));
    }
    Ok((if link { 3 } else { 2 }) << 26 | (target >> 2) & 0x03FF_FFFF)
}

fn set_fields(word: u32, set: &[Field]) -> Option<u32> {
    set.iter().try_fold(word, |w, f| {
        let (lo, width) = (u32::from(f.lo), u32::from(f.width));
        if width == 0 || lo + width > 32 || (width < 32 && f.value >> width != 0) {
            return None;
        }
        let mask = (u32::MAX >> (32 - width)) << lo;
        Some(w & !mask | f.value << lo)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image() -> Vec<u8> {
        let mut image = vec![0u8; 0x200];
        image[0x10..0x14].copy_from_slice(&0x8FBF_0014u32.to_le_bytes()); // lw ra,0x14(sp)
        image[0x14..0x18].copy_from_slice(&0x27BD_0018u32.to_le_bytes()); // addiu sp,sp,0x18
        image
    }

    fn edit(address: u32, count: u32, op: Op) -> Edit {
        Edit {
            address: BASE + address,
            count,
            op,
        }
    }

    #[test]
    fn test_words_when_literal_repeated_returns_each_word() {
        let e = edit(0x100, 3, Op::Word(0xFFFF_FFFF));

        let words = e.words(&image()).unwrap();

        assert_eq!(
            words,
            vec![
                (BASE + 0x100, u32::MAX),
                (BASE + 0x104, u32::MAX),
                (BASE + 0x108, u32::MAX)
            ]
        );
    }

    #[test]
    fn test_words_when_copy_of_two_words_returns_the_original_words() {
        let e = edit(
            0x100,
            2,
            Op::Copy {
                from: BASE + 0x10,
                set: &[],
            },
        );

        assert_eq!(
            e.words(&image()).unwrap(),
            vec![(BASE + 0x100, 0x8FBF_0014), (BASE + 0x104, 0x27BD_0018)]
        );
    }

    #[test]
    fn test_words_when_copy_sets_imm_returns_word_with_field_changed() {
        static IMM: [Field; 1] = [Field {
            lo: 0,
            width: 16,
            value: 0x1C,
        }];
        let e = edit(
            0x100,
            1,
            Op::Copy {
                from: BASE + 0x10,
                set: &IMM,
            },
        );

        assert_eq!(
            e.words(&image()).unwrap(),
            vec![(BASE + 0x100, 0x8FBF_001C)]
        );
    }

    #[test]
    fn test_words_when_jal_returns_encoded_call() {
        let e = edit(
            0x100,
            1,
            Op::Jump {
                target: 0x9D07_BA00,
                link: true,
            },
        );

        assert_eq!(
            e.words(&image()).unwrap(),
            vec![(BASE + 0x100, 0x0F41_EE80)]
        );
    }

    #[test]
    fn test_words_when_jump_leaves_the_region_returns_malformed() {
        let e = edit(
            0x100,
            1,
            Op::Jump {
                target: 0xBD00_0000,
                link: false,
            },
        );

        assert_eq!(e.words(&image()), Err(EditError::Malformed(BASE + 0x100)));
    }

    #[test]
    fn test_words_when_field_value_too_wide_returns_malformed() {
        static WIDE: [Field; 1] = [Field {
            lo: 16,
            width: 5,
            value: 32,
        }];
        let e = edit(
            0x100,
            1,
            Op::Copy {
                from: BASE + 0x10,
                set: &WIDE,
            },
        );

        assert_eq!(e.words(&image()), Err(EditError::Malformed(BASE + 0x100)));
    }

    #[test]
    fn test_words_when_copy_source_outside_image_returns_out_of_image() {
        let e = edit(
            0x100,
            1,
            Op::Copy {
                from: BASE + 0x1000,
                set: &[],
            },
        );

        assert_eq!(e.words(&image()), Err(EditError::OutOfImage(BASE + 0x1000)));
    }

    #[test]
    fn test_words_when_count_on_a_jump_or_zero_returns_malformed() {
        let jump = edit(
            0x100,
            2,
            Op::Jump {
                target: BASE,
                link: false,
            },
        );
        let empty = edit(0x100, 0, Op::Word(1));

        assert!(matches!(jump.words(&image()), Err(EditError::Malformed(_))));
        assert!(matches!(
            empty.words(&image()),
            Err(EditError::Malformed(_))
        ));
    }

    #[test]
    fn test_words_when_destination_outside_image_returns_out_of_image() {
        let e = edit(0x1FE, 1, Op::Word(1));

        assert_eq!(e.words(&image()), Err(EditError::OutOfImage(BASE + 0x1FE)));
    }
}
