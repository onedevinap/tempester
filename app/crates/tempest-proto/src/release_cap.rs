//! The Release maxima (n°63, Tempester 1.6.6.0, and its 1.6.7.0 form). A Release of 124-127 never ends in DSI's Voice
//! OS, so Tempester's Main stops Pitch, Filter, Aux 1 and Aux 2 Release at 123, like Amp Release (which DSI's own table
//! already stops there). From 1.6.7.0 the Main lifts the cap to 127 for all five when the six Voice chips answered
//! `0xFE00` with exactly the Tempester Voice 1.5.1.0 word `0x16` at start-up (their release curve ends 124-127 too);
//! from 1.6.9.0 (n°65) with any Tempester Voice word, `0x16`-`0x79` (1.5.1.2 answers `0x17`).
//!
//! The generated table (`sound::params`) is extracted from DSI's 1.5.0.2: maxima 127 and, for Amp Release, 123. This is
//! the one function that says what the maximum of a parameter is for the machine in front of us (the Panel knobs, the
//! NRPN sends). A stored sound with 124-127 still loads: the machine plays and shows it as 123 under a cap, and so does
//! the app.

use std::ops::RangeInclusive;

use crate::identity::{VOICE_CHIPS, VOICE_WORD_TEMPESTER_1510};
use crate::sound::params;
use crate::versions::Version;

/// The five Release parameters: Pitch, Filter, Amp, Aux 1, Aux 2.
pub const RELEASES: [u8; 5] = [65, 70, 75, 83, 91];
/// Their maximum under a cap.
pub const RELEASE_MAX: u16 = 123;
/// Their maximum when the six Voice chips run Tempester Voice.
pub const RELEASE_MAX_TEMPESTER_VOICE: u16 = 127;
/// The start-up words Main 1.6.9.0 takes for Tempester Voice (n°65, patch/release_cap_1690.s): DSI's 1.5 answers
/// `0x15` and a silent chip `0x0000` / `0xFFFF`; main's own start-up loop accepts replies up to `0x79`.
pub const TEMPESTER_VOICE_WORDS: RangeInclusive<u16> = 0x0016..=0x0079;

const PATCH_ALWAYS: &str = "patch/release_cap.json";
const PATCH_WITH_VOICE: &str = "patch/release_cap_1670.json";
const PATCH_WITH_VOICE_RANGE: &str = "patch/release_cap_1690.json";

/// What the Main OS does with the Releases.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseCap {
    /// No n°63 (DSI's OS and Tempester up to 1.6.5.0): the maxima of the table, 127 and Amp Release 123.
    Stock,
    /// n°63 of 1.6.6.0: the five stop at 123, whatever the Voice chips run.
    Always,
    /// n°63 of 1.6.7.0 (and 1.6.8.0): 123, unless the six chips reported exactly the word `0x16`, then 127.
    WithTempesterVoice,
    /// n°63 of 1.6.9.0 (n°65): 123, unless the six chips reported a word of `TEMPESTER_VOICE_WORDS`, then 127.
    WithAnyTempesterVoice,
}

/// The cap of a Tempester Main version, read from its recipes.
pub fn cap_of(version: &Version) -> ReleaseCap {
    let has = |file: &str| version.patches.iter().any(|p| p.file == file);
    if has(PATCH_WITH_VOICE_RANGE) {
        ReleaseCap::WithAnyTempesterVoice
    } else if has(PATCH_WITH_VOICE) {
        ReleaseCap::WithTempesterVoice
    } else if has(PATCH_ALWAYS) {
        ReleaseCap::Always
    } else {
        ReleaseCap::Stock
    }
}

/// The maximum of parameter `index` on a Main with `cap`, given the version words of the six Voice chips as the
/// machine reported them (`None`: not reported). Parameters other than the five Releases keep the table's maximum.
pub fn release_max(index: u8, cap: ReleaseCap, voices: Option<&[u16; VOICE_CHIPS]>) -> u16 {
    let table = params()[usize::from(index)].max;
    if !RELEASES.contains(&index) {
        return table;
    }
    match cap {
        ReleaseCap::Stock => table,
        ReleaseCap::Always => RELEASE_MAX,
        ReleaseCap::WithTempesterVoice => {
            lifted_if(voices, |word| word == VOICE_WORD_TEMPESTER_1510)
        }
        ReleaseCap::WithAnyTempesterVoice => {
            lifted_if(voices, |word| TEMPESTER_VOICE_WORDS.contains(&word))
        }
    }
}

/// 127 when the six chips reported words that all pass `tempester`, 123 otherwise (not reported included).
fn lifted_if(voices: Option<&[u16; VOICE_CHIPS]>, tempester: impl Fn(u16) -> bool) -> u16 {
    if voices.is_some_and(|w| w.iter().all(|word| tempester(*word))) {
        RELEASE_MAX_TEMPESTER_VOICE
    } else {
        RELEASE_MAX
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{VOICE_WORD_DSI_15, VOICE_WORD_TEMPESTER_1512};
    use crate::versions::{find_version, versions};

    const TEMPESTER: [u16; VOICE_CHIPS] = [VOICE_WORD_TEMPESTER_1510; VOICE_CHIPS];
    const TEMPESTER_1512: [u16; VOICE_CHIPS] = [VOICE_WORD_TEMPESTER_1512; VOICE_CHIPS];
    const MIXED: [u16; VOICE_CHIPS] = [0x16, 0x17, 0x16, 0x17, 0x16, 0x17];
    const ALL_CAPS: [ReleaseCap; 4] = [
        ReleaseCap::Stock,
        ReleaseCap::Always,
        ReleaseCap::WithTempesterVoice,
        ReleaseCap::WithAnyTempesterVoice,
    ];

    #[test]
    fn test_releases_when_read_in_the_generated_table_are_the_five_with_dsi_s_maxima() {
        let releases: Vec<(u8, u16)> = params()
            .iter()
            .filter(|p| p.name == "Release")
            .map(|p| (p.index, p.max))
            .collect();

        assert_eq!(
            releases,
            vec![
                (65, 127),
                (70, 127),
                (75, RELEASE_MAX),
                (83, 127),
                (91, 127)
            ]
        );
        for index in RELEASES {
            assert_eq!(params()[usize::from(index)].name, "Release");
        }
    }

    #[test]
    fn test_release_max_when_stock_returns_the_table_maxima_whatever_the_voices_say() {
        for voices in [None, Some(&TEMPESTER)] {
            for index in RELEASES {
                assert_eq!(
                    release_max(index, ReleaseCap::Stock, voices),
                    params()[usize::from(index)].max,
                    "{index}"
                );
            }
        }
    }

    #[test]
    fn test_release_max_when_always_returns_123_for_the_five_even_with_tempester_voices() {
        for voices in [None, Some(&TEMPESTER)] {
            for index in RELEASES {
                assert_eq!(release_max(index, ReleaseCap::Always, voices), 123);
            }
        }
    }

    #[test]
    fn test_release_max_when_1_6_7_0_and_six_tempester_voices_returns_127_for_the_five() {
        for index in RELEASES {
            assert_eq!(
                release_max(index, ReleaseCap::WithTempesterVoice, Some(&TEMPESTER)),
                127,
                "{index}: Amp Release (75) included"
            );
        }
    }

    #[test]
    fn test_release_max_when_1_6_7_0_and_any_one_chip_differs_returns_123_for_each_chip_position() {
        for other in [VOICE_WORD_DSI_15, 0, 0x17, 0x8016, 0xFFFF] {
            for chip in 0..VOICE_CHIPS {
                let mut words = TEMPESTER;
                words[chip] = other;
                for index in RELEASES {
                    assert_eq!(
                        release_max(index, ReleaseCap::WithTempesterVoice, Some(&words)),
                        123,
                        "chip {chip} answered {other:#x}, parameter {index}"
                    );
                }
            }
        }
    }

    #[test]
    fn test_release_max_when_1_6_7_0_and_the_versions_are_not_reported_returns_123() {
        for index in RELEASES {
            assert_eq!(
                release_max(index, ReleaseCap::WithTempesterVoice, None),
                123
            );
        }
    }

    #[test]
    fn test_release_max_when_1_6_9_0_and_any_tempester_voice_word_returns_127_and_123_outside_the_range(
    ) {
        for words in [TEMPESTER, TEMPESTER_1512, MIXED, [0x79; VOICE_CHIPS]] {
            for index in RELEASES {
                assert_eq!(
                    release_max(index, ReleaseCap::WithAnyTempesterVoice, Some(&words)),
                    127,
                    "{words:x?}, {index}"
                );
            }
        }
        for other in [VOICE_WORD_DSI_15, 0, 0x7A, 0x0116, 0x8016, 0xFFFF] {
            for chip in 0..VOICE_CHIPS {
                let mut words = TEMPESTER_1512;
                words[chip] = other;
                assert_eq!(
                    release_max(65, ReleaseCap::WithAnyTempesterVoice, Some(&words)),
                    123,
                    "chip {chip} answered {other:#x}"
                );
            }
        }
        assert_eq!(
            release_max(65, ReleaseCap::WithAnyTempesterVoice, None),
            123
        );
    }

    #[test]
    fn test_release_max_when_1_6_8_0_and_a_1_5_1_2_chip_returns_123_the_reason_main_goes_first() {
        // Main 1.6.7.0 / 1.6.8.0 test the exact word 0x16: Voice 1.5.1.2 (0x17) loses the lift there.
        for words in [TEMPESTER_1512, MIXED] {
            assert_eq!(
                release_max(65, ReleaseCap::WithTempesterVoice, Some(&words)),
                123
            );
        }
    }

    #[test]
    fn test_release_max_when_not_a_release_returns_the_table_maximum_under_every_cap() {
        for cap in ALL_CAPS {
            for p in params().iter().filter(|p| !RELEASES.contains(&p.index)) {
                assert_eq!(release_max(p.index, cap, Some(&TEMPESTER)), p.max);
                assert_eq!(release_max(p.index, cap, None), p.max);
            }
        }
    }

    type MaxFn = fn(u8, ReleaseCap, Option<&[u16; VOICE_CHIPS]>) -> u16;

    /// The whole table of the rule, as one judge for any implementation: every cap, every Release, every way the six
    /// chips can answer (one chip different at each position, the bounds 0x16 / 0x79 and the words just outside, the
    /// versions not reported). `Err` names the first case an implementation gets wrong.
    fn judge(max: MaxFn) -> Result<(), String> {
        let mut cases: Vec<(ReleaseCap, Option<[u16; 6]>)> = Vec::new();
        for cap in ALL_CAPS {
            cases.push((cap, None));
            for all in [TEMPESTER, TEMPESTER_1512, MIXED] {
                cases.push((cap, Some(all)));
            }
            for chip in 0..VOICE_CHIPS {
                for other in [VOICE_WORD_DSI_15, 0, 0x17, 0x79, 0x7A, 0xFFFF] {
                    let mut words = TEMPESTER;
                    words[chip] = other;
                    cases.push((cap, Some(words)));
                }
            }
        }
        let in_range = |w: &[u16; 6]| w.iter().all(|x| (0x16..=0x79).contains(x));
        for (cap, voices) in cases {
            for index in RELEASES {
                let expected = match cap {
                    ReleaseCap::Stock => params()[usize::from(index)].max,
                    ReleaseCap::Always => 123,
                    ReleaseCap::WithTempesterVoice if voices == Some(TEMPESTER) => 127,
                    ReleaseCap::WithTempesterVoice => 123,
                    ReleaseCap::WithAnyTempesterVoice if voices.as_ref().is_some_and(in_range) => {
                        127
                    }
                    ReleaseCap::WithAnyTempesterVoice => 123,
                };
                let found = max(index, cap, voices.as_ref());
                if found != expected {
                    return Err(format!(
                        "{cap:?}, voices {voices:?}, parameter {index}: {found}, expected {expected}"
                    ));
                }
            }
        }
        Ok(())
    }

    #[test]
    fn test_judge_when_given_the_real_function_returns_ok() {
        assert_eq!(judge(release_max), Ok(()));
    }

    #[test]
    fn test_judge_when_given_a_deliberately_wrong_function_returns_the_case_it_gets_wrong() {
        // 127 as soon as any chip answered the Tempester word.
        let any_chip: MaxFn = |index, cap, voices| {
            let any = voices.is_some_and(|w| w.contains(&VOICE_WORD_TEMPESTER_1510));
            if cap == ReleaseCap::WithTempesterVoice && any && RELEASES.contains(&index) {
                127
            } else {
                release_max(index, cap, voices)
            }
        };
        // 127 when the versions are not reported (an optimistic default).
        let unreported: MaxFn = |index, cap, voices| {
            if cap == ReleaseCap::WithTempesterVoice
                && voices.is_none()
                && RELEASES.contains(&index)
            {
                127
            } else {
                release_max(index, cap, voices)
            }
        };
        // Only the first five chips are looked at.
        let five_chips: MaxFn = |index, cap, voices| {
            let words = voices.map(|w| {
                let mut w = *w;
                w[5] = VOICE_WORD_TEMPESTER_1510;
                w
            });
            release_max(index, cap, words.as_ref())
        };
        // Amp Release (75) left out of the lift.
        let no_amp: MaxFn = |index, cap, voices| {
            if index == 75 && cap == ReleaseCap::WithTempesterVoice {
                123
            } else {
                release_max(index, cap, voices)
            }
        };
        // 1.6.6.0 lifts too.
        let always_lifts: MaxFn = |index, cap, voices| {
            let cap = if cap == ReleaseCap::Always {
                ReleaseCap::WithTempesterVoice
            } else {
                cap
            };
            release_max(index, cap, voices)
        };
        // A cap of 123 also where there is none.
        let stock_capped: MaxFn = |index, cap, voices| {
            let cap = if cap == ReleaseCap::Stock {
                ReleaseCap::Always
            } else {
                cap
            };
            release_max(index, cap, voices)
        };

        // 1.6.9.0 kept on 1.6.8.0's exact word: Voice 1.5.1.2 (0x17) loses the lift.
        let exact_on_1690: MaxFn = |index, cap, voices| {
            let cap = if cap == ReleaseCap::WithAnyTempesterVoice {
                ReleaseCap::WithTempesterVoice
            } else {
                cap
            };
            release_max(index, cap, voices)
        };
        // 1.6.8.0 given the range: a 1.5.1.2 chip would be shown lifted where the machine caps it.
        let range_on_1680: MaxFn = |index, cap, voices| {
            let cap = if cap == ReleaseCap::WithTempesterVoice {
                ReleaseCap::WithAnyTempesterVoice
            } else {
                cap
            };
            release_max(index, cap, voices)
        };
        // A range that takes DSI's 0x15.
        let takes_dsi: MaxFn = |index, cap, voices| {
            let lifted = voices.is_some_and(|w| w.iter().all(|x| (0x15..=0x79).contains(x)));
            if cap == ReleaseCap::WithAnyTempesterVoice && RELEASES.contains(&index) {
                if lifted {
                    127
                } else {
                    123
                }
            } else {
                release_max(index, cap, voices)
            }
        };
        // A range with no upper bound (0x7A and 0xFFFF taken).
        let no_upper: MaxFn = |index, cap, voices| {
            let lifted = voices.is_some_and(|w| w.iter().all(|x| *x >= 0x16));
            if cap == ReleaseCap::WithAnyTempesterVoice && RELEASES.contains(&index) {
                if lifted {
                    127
                } else {
                    123
                }
            } else {
                release_max(index, cap, voices)
            }
        };

        for (name, wrong) in [
            ("any chip", any_chip),
            ("not reported", unreported),
            ("five chips", five_chips),
            ("no Amp Release", no_amp),
            ("1.6.6.0 lifts", always_lifts),
            ("stock capped", stock_capped),
            ("exact word on 1.6.9.0", exact_on_1690),
            ("range on 1.6.8.0", range_on_1680),
            ("range takes 0x15", takes_dsi),
            ("no upper bound", no_upper),
        ] {
            assert!(judge(wrong).is_err(), "{name} was accepted");
        }
    }

    #[test]
    fn test_cap_of_when_read_from_the_versions_table_follows_the_release_cap_patches() {
        let cap = |version: &str| cap_of(find_version(version).expect("in the table"));

        assert_eq!(cap("1.6.9.0"), ReleaseCap::WithAnyTempesterVoice);
        assert_eq!(cap("1.6.8.0"), ReleaseCap::WithTempesterVoice);
        assert_eq!(cap("1.6.7.0"), ReleaseCap::WithTempesterVoice);
        assert_eq!(cap("1.6.6.0"), ReleaseCap::Always);
        assert_eq!(cap("1.6.5.0"), ReleaseCap::Stock);
        assert_eq!(cap("1.5.2.0"), ReleaseCap::Stock);
        // Exactly the versions with a cap patch have a cap.
        for v in versions() {
            let patched = v.patches.iter().any(|p| p.file.contains("release_cap"));
            assert_eq!(cap_of(v) != ReleaseCap::Stock, patched, "{}", v.version);
        }
    }
}
