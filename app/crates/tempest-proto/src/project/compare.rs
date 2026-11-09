//! Comparing the project a machine holds (read back with the request `42`) with the one that was sent to it: the
//! decoded content, parameter word by parameter word, Play List by Play List, beat by beat. Packing layouts never
//! matter, only the decoded data does. A difference is reported precisely (which word, which list, which pad), so
//! that a mismatch can be told from a transfer error.
//!
//! Two things a read-back is NOT expected to repeat, and which are said as notes rather than differences:
//! - beats the sent project did not hold: the machine keeps their old content, so they are not compared;
//! - an empty Play List 1: DSI's own rule fills it with the selected beat as step 1 once the project is loaded
//!   (`refs/firmware-main.md` 9.6, `Docs/analyses/bug-project-receive.md` 4), so a list 1 that came back used while
//!   the sent one was empty is the machine's doing.

use std::fmt;

use super::{
    Project, ProjectBeat, EMPTY_LIST, FIRST_VERSION_WITH_LISTS, LISTS, LIST_BODY_WORDS,
    LIST_NAME_WORDS, PARAM_WORDS,
};

/// One thing that does not match, numbered from 1 as the machine shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Difference {
    /// Parameter words (0-based indexes in the 44-word block) that differ.
    Parameters {
        words: Vec<usize>,
    },
    PlayListVersion {
        sent: Option<u16>,
        back: Option<u16>,
    },
    /// A Play List used on one side and empty on the other.
    PlayListUse {
        list: usize,
        sent_used: bool,
    },
    /// A used Play List whose End word, name or steps (1-based step numbers) differ.
    PlayListContent {
        list: usize,
        end: bool,
        name: bool,
        steps: Vec<usize>,
    },
    /// A beat the machine did not send back.
    BeatMissing {
        beat: u8,
    },
    BeatName {
        beat: u8,
        sent: String,
        back: String,
    },
    BeatEvents {
        beat: u8,
        sent: usize,
        back: usize,
    },
    /// Pads (1-based) whose sound differs.
    BeatSounds {
        beat: u8,
        pads: Vec<usize>,
    },
    /// Anything else in the beat's data: the offset of the first byte that differs (or the shorter length).
    BeatBytes {
        beat: u8,
        offset: usize,
    },
}

impl fmt::Display for Difference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parameters { words } => {
                write!(f, "project parameters differ at word(s) {}", join(words))
            }
            Self::PlayListVersion { sent, back } => write!(
                f,
                "Play List version: sent {}, machine {}",
                shown(*sent),
                shown(*back)
            ),
            Self::PlayListUse { list, sent_used } => write!(
                f,
                "Play List {list}: {} in the sent project, {} on the machine",
                if *sent_used { "used" } else { "empty" },
                if *sent_used { "empty" } else { "used" }
            ),
            Self::PlayListContent {
                list,
                end,
                name,
                steps,
            } => {
                let mut parts = Vec::new();
                if *end {
                    parts.push("its End word".to_string());
                }
                if *name {
                    parts.push("its name".to_string());
                }
                if !steps.is_empty() {
                    parts.push(format!("step(s) {}", join(steps)));
                }
                write!(f, "Play List {list} differs: {}", parts.join(", "))
            }
            Self::BeatMissing { beat } => {
                write!(f, "beat {}: the machine did not send it back", beat + 1)
            }
            Self::BeatName { beat, sent, back } => write!(
                f,
                "beat {}: name \"{sent}\" sent, \"{back}\" on the machine",
                beat + 1
            ),
            Self::BeatEvents { beat, sent, back } => write!(
                f,
                "beat {}: {sent} events sent, {back} on the machine",
                beat + 1
            ),
            Self::BeatSounds { beat, pads } => {
                write!(
                    f,
                    "beat {}: sound differs on pad(s) {}",
                    beat + 1,
                    join(pads)
                )
            }
            Self::BeatBytes { beat, offset } => write!(
                f,
                "beat {}: data differs from byte {offset} (not its name, events or sounds)",
                beat + 1
            ),
        }
    }
}

fn join(numbers: &[usize]) -> String {
    numbers
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

fn shown(version: Option<u16>) -> String {
    version.map_or_else(|| "none".to_string(), |v| v.to_string())
}

/// What the comparison found.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Comparison {
    pub differences: Vec<Difference>,
    pub notes: Vec<String>,
    /// Beats that were compared (the ones the sent project held).
    pub beats_compared: usize,
}

impl Comparison {
    /// True when the machine holds exactly what was sent.
    pub fn matches(&self) -> bool {
        self.differences.is_empty()
    }
}

/// A Play List as the header data holds it: its 111 words (End, 10 name words, 100 step words), `None` when empty.
type ListWords = Option<Vec<u16>>;

fn words_of(data: &[u8]) -> Vec<u16> {
    data.as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_be_bytes(*pair))
        .collect()
}

/// The lists of a header data block (none when its parameters version has no lists), plus the Play List version.
fn lists_of(data: &[u8]) -> (Option<u16>, [ListWords; LISTS]) {
    let words = words_of(data);
    let mut lists: [ListWords; LISTS] = Default::default();
    let has_lists = words
        .first()
        .is_some_and(|v| *v >= FIRST_VERSION_WITH_LISTS);
    if !has_lists {
        return (None, lists);
    }
    let mut at = PARAM_WORDS + 1;
    for list in &mut lists {
        match words.get(at) {
            Some(&EMPTY_LIST) => at += 1,
            Some(_) => {
                *list = words.get(at..at + 1 + LIST_BODY_WORDS).map(<[u16]>::to_vec);
                at += 1 + LIST_BODY_WORDS;
            }
            None => break,
        }
    }
    (words.get(PARAM_WORDS).copied(), lists)
}

fn differing(a: &[u16], b: &[u16]) -> Vec<usize> {
    (0..a.len().max(b.len()))
        .filter(|at| a.get(*at) != b.get(*at))
        .collect()
}

fn compare_lists(sent: &Project, back: &Project, out: &mut Comparison) {
    let (sent_version, sent_lists) = lists_of(&sent.header_data);
    let (back_version, back_lists) = lists_of(&back.header_data);
    if sent_version != back_version {
        out.differences.push(Difference::PlayListVersion {
            sent: sent_version,
            back: back_version,
        });
    }
    for (at, (sent, back)) in sent_lists.iter().zip(&back_lists).enumerate() {
        let list = at + 1;
        match (sent, back) {
            (None, None) => {}
            (None, Some(_)) if at == 0 => out.notes.push(
                "Play List 1 was empty in the sent project: the machine fills an empty Play List 1 with the selected beat as step 1 (DSI's own rule), so it is not compared."
                    .to_string(),
            ),
            (Some(_), None) | (None, Some(_)) => out.differences.push(Difference::PlayListUse {
                list,
                sent_used: sent.is_some(),
            }),
            (Some(sent), Some(back)) => {
                let end = sent[0] != back[0];
                let name = sent[1..=LIST_NAME_WORDS] != back[1..=LIST_NAME_WORDS];
                let steps: Vec<usize> = differing(&sent[1 + LIST_NAME_WORDS..], &back[1 + LIST_NAME_WORDS..])
                    .into_iter()
                    .map(|step| step + 1)
                    .collect();
                if end || name || !steps.is_empty() {
                    out.differences.push(Difference::PlayListContent {
                        list,
                        end,
                        name,
                        steps,
                    });
                }
            }
        }
    }
}

fn compare_beat(sent: &ProjectBeat, back: &ProjectBeat, differences: &mut Vec<Difference>) {
    if sent.data == back.data {
        return;
    }
    let beat = sent.index;
    let (a, b) = (&sent.dump, &back.dump);
    let before = differences.len();
    if a.summary.name != b.summary.name {
        differences.push(Difference::BeatName {
            beat,
            sent: a.summary.name.clone(),
            back: b.summary.name.clone(),
        });
    }
    if a.summary.events != b.summary.events {
        differences.push(Difference::BeatEvents {
            beat,
            sent: a.summary.events,
            back: b.summary.events,
        });
    }
    let pads: Vec<usize> = (0..a.sounds.len())
        .filter(|pad| {
            a.sounds[*pad].as_ref().map(|s| *s.bytes())
                != b.sounds[*pad].as_ref().map(|s| *s.bytes())
        })
        .map(|pad| pad + 1)
        .collect();
    if !pads.is_empty() {
        differences.push(Difference::BeatSounds { beat, pads });
    }
    if differences.len() == before {
        let offset = (0..sent.data.len().max(back.data.len()))
            .find(|at| sent.data.get(*at) != back.data.get(*at))
            .unwrap_or(0);
        differences.push(Difference::BeatBytes { beat, offset });
    }
}

/// Compares what the machine sent back (`back`) with what was sent to it (`sent`).
pub fn compare(sent: &Project, back: &Project) -> Comparison {
    let mut out = Comparison::default();
    let params = differing(
        &words_of(&sent.header_data)[..PARAM_WORDS.min(sent.header_data.len() / 2)],
        &words_of(&back.header_data)[..PARAM_WORDS.min(back.header_data.len() / 2)],
    );
    if !params.is_empty() {
        out.differences
            .push(Difference::Parameters { words: params });
    }
    compare_lists(sent, back, &mut out);
    for beat in &sent.beats {
        out.beats_compared += 1;
        match back.beats.iter().find(|b| b.index == beat.index) {
            Some(returned) => compare_beat(beat, returned, &mut out.differences),
            None => out
                .differences
                .push(Difference::BeatMissing { beat: beat.index }),
        }
    }
    let kept = sent.absent_beats();
    if !kept.is_empty() {
        let numbers: Vec<String> = kept.iter().map(|n| (n + 1).to_string()).collect();
        out.notes.push(format!(
            "Beat(s) {} were not in the sent project: the machine keeps their previous content, so they are not compared.",
            numbers.join(", ")
        ));
    }
    out
}
