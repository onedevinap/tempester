//! What the app saves must hold everything the firmware stores (`Docs/analyses/app-save-coverage.md`): data goes
//! machine dump -> app file -> machine byte for byte, and a word the app does not know is kept, not zeroed. Every
//! test here builds data with words the app has no meaning for (the project parameters 19-33 and 41-43, the spare
//! tail of the beat header, the 128 bytes of a sound beyond what a parameter table names), runs it through the app's
//! readers and writers, and compares the bytes.
#![allow(clippy::expect_used)] // integration tests: a panic with a message is the failure report

use tempest_proto::beat::{self, PADS};
use tempest_proto::beat_sequence::{synthetic_sequence_beat, BeatEvent};
use tempest_proto::project::{
    beat_message, decode_received, header_data_message, ProjectAssembler, Received, BEATS,
};
use tempest_proto::quantize::Quantize;
use tempest_proto::sound::{Sound, SOUND_BYTES};
use tempest_proto::sound_message::{decode, encode_current, encode_file, SoundMessage};

const PARAM_WORDS: usize = 44;
const EMPTY_LIST: u16 = 0xFEFE;
/// Word of the beat header after the 32 pad blocks of 15 words that start at word 26.
const BEAT_TAIL_WORD: usize = 26 + 32 * 15;

fn words_to_bytes(words: &[u16]) -> Vec<u8> {
    words.iter().flat_map(|w| w.to_be_bytes()).collect()
}

/// A `5E` header (parameters version 5) with Quantize "16 Quint" in word 18 (n°55) and a value in every other word,
/// four empty Play Lists.
fn header_with_unknown_words() -> Vec<u8> {
    let mut words: Vec<u16> = (0..PARAM_WORDS as u16).map(|w| 0xA000 + w).collect();
    words[0] = 5;
    words[4] = 7 << 8;
    words[18] = 0x5107;
    words.push(2);
    words.extend([EMPTY_LIST; 4]);
    words_to_bytes(&words)
}

/// A beat with a long note (bit 15 of the second word, n°53), play probabilities (n°36) and a non-zero value in every
/// spare word at the end of its header.
fn beat_with_unknown_words(n: usize) -> Vec<u8> {
    let sounds: [Sound; PADS] = std::array::from_fn(|pad| {
        let mut bytes = [0u8; SOUND_BYTES];
        for (at, byte) in bytes.iter_mut().enumerate() {
            *byte = (at * 7 + pad * 3 + n) as u8;
        }
        Sound::from_bytes(bytes)
    });
    let events = [BeatEvent {
        step: 0,
        pad: 0,
        velocity: 100,
        duration: 768,
        time_shift: 0,
        half_step: false,
    }];
    let data = synthetic_sequence_beat(
        &format!("Beat {n}"),
        &[(0, 1)],
        1200,
        3,
        4,
        &events,
        &sounds,
    );
    let mut probability = [100u8; PADS];
    probability[0] = 35;
    let mut data = beat::with_play_probability(&data, &probability).expect("a header");
    for word in BEAT_TAIL_WORD..538 {
        let at = word * 2;
        data[at..at + 2].copy_from_slice(&(0xB000 + word as u16).to_be_bytes());
    }
    data
}

fn assembled() -> (ProjectAssembler, Vec<Vec<u8>>, Vec<Vec<u8>>) {
    let header = header_with_unknown_words();
    let beats: Vec<Vec<u8>> = (0..BEATS).map(beat_with_unknown_words).collect();
    let mut messages = vec![header_data_message(&header)];
    messages.extend(
        beats
            .iter()
            .enumerate()
            .map(|(n, data)| beat_message(n as u8, data)),
    );
    let mut assembler = ProjectAssembler::new();
    for message in &messages {
        assembler.accept(message).expect("the message is accepted");
    }
    (assembler, messages, beats)
}

#[test]
fn test_project_when_header_has_fine_quantize_and_unknown_words_returns_every_word_in_the_file() {
    let (assembler, messages, beats) = assembled();
    let header = header_with_unknown_words();

    let project = assembler.finish().expect("a project");

    assert_eq!(project.header.quantize, Some(Quantize::Fine(7)));
    assert_eq!(project.header_data(), header.as_slice());
    assert_eq!(
        project.raw(),
        messages.concat(),
        "as received, byte for byte"
    );
    for (sent, decoded) in project
        .send_messages()
        .iter()
        .zip(std::iter::once(&header).chain(&beats))
    {
        let kept = match decode_received(sent)
            .expect("decodes")
            .expect("a project message")
        {
            Received::Header(data) | Received::Beat { data, .. } => data,
        };
        assert_eq!(&kept, decoded, "the standard file keeps every byte");
    }
}

#[test]
fn test_project_when_play_lists_are_left_out_returns_the_44_parameter_words_untouched() {
    let (assembler, _, _) = assembled();
    let project = assembler.finish().expect("a project");

    let without = project.with_empty_play_lists();

    assert_eq!(
        without.header_data()[..2 * PARAM_WORDS],
        header_with_unknown_words()[..2 * PARAM_WORDS]
    );
    assert_eq!(without.header.quantize, Some(Quantize::Fine(7)));
}

#[test]
fn test_beat_file_when_beat_has_long_notes_probability_and_spare_words_returns_the_same_data() {
    let (assembler, _, beats) = assembled();
    let project = assembler.finish().expect("a project");

    for (beat, data) in project.beats.iter().zip(&beats) {
        let file = beat.beat_file();
        let read = beat::read_body(&file[4..file.len() - 1]).expect("a beat body");
        assert_eq!(&read, data, "beat {}", beat.index + 1);
    }
    assert_eq!(project.beats[0].dump.summary.play_probability[0], 35);
}

#[test]
fn test_beat_standard_message_when_a_machine_dump_is_repaired_returns_the_same_data() {
    let data = beat_with_unknown_words(3);
    for dump in [beat::machine_dump(&data), beat::standard_dump(&data)] {
        let repaired = beat::standard_message(&dump).expect("a beat");
        let read = beat::read_body(&repaired[4..repaired.len() - 1]).expect("a body");
        assert_eq!(read, data);
    }
}

#[test]
fn test_sound_when_every_byte_has_a_value_returns_the_128_bytes_in_the_file_and_in_the_message() {
    let mut bytes = [0u8; SOUND_BYTES];
    for (at, byte) in bytes.iter_mut().enumerate() {
        *byte = (at as u8).wrapping_mul(37) ^ 0x80;
    }
    let sound = Sound::from_bytes(bytes);

    let file = encode_file("S/Test/Everything", &sound).expect("a file");
    let current = encode_current(&sound);

    assert!(
        matches!(decode(&file), Ok(SoundMessage::File { sound: s, .. }) if s.bytes() == &bytes)
    );
    assert!(matches!(decode(&current), Ok(SoundMessage::Current(s)) if s.bytes() == &bytes));
}

#[test]
fn test_sound_when_one_parameter_is_edited_returns_every_other_byte_unchanged() {
    let mut bytes = [0u8; SOUND_BYTES];
    for (at, byte) in bytes.iter_mut().enumerate() {
        *byte = (at as u8).wrapping_mul(11) ^ 0x5A;
    }
    let mut sound = Sound::from_bytes(bytes);

    sound.set(34, 3).expect("in range");

    let changed: Vec<usize> = (0..SOUND_BYTES)
        .filter(|at| sound.bytes()[*at] != bytes[*at])
        .collect();
    assert!(
        changed.len() <= 2,
        "one parameter touches at most two bytes: {changed:?}"
    );
}
