use super::*;
use crate::beat::{synthetic_beat_data, PADS};
use crate::pack7::unpack;
use crate::sound::Sound;
use crate::sound_message::encode_current;

fn sound_for(beat: usize, pad: usize) -> Sound {
    let mut sound = Sound::with_defaults();
    // Values inside the table maxima: a failure here is a bug of the test itself.
    let _ = sound.set(34, (beat * 3 + pad) as u16 % 50);
    let _ = sound.set_name(&format!("S{beat}-{pad}"));
    sound
}

fn beat_data(beat: usize) -> Vec<u8> {
    let sounds: [Sound; PADS] = std::array::from_fn(|pad| sound_for(beat, pad));
    synthetic_beat_data(
        &format!("Beat {beat}"),
        &[(0, 1), (5, 3)],
        4 + beat,
        &sounds,
    )
}

fn messages(beats: usize) -> Vec<Vec<u8>> {
    let mut all = vec![header_message(1)];
    all.extend((0..beats).map(|n| beat_message(n as u8, &beat_data(n))));
    all
}

fn assemble(all: &[Vec<u8>]) -> Result<ProjectAssembler, ProjectError> {
    let mut assembler = ProjectAssembler::new();
    for message in all {
        assembler.accept(message)?;
    }
    Ok(assembler)
}

#[test]
fn test_finish_when_header_and_16_beats_return_project_with_names_events_and_raw_bytes() {
    let all = messages(16);

    let project = assemble(&all).unwrap().finish().unwrap();

    assert_eq!(project.header.params_version, 5);
    assert_eq!(project.header.play_lists_used, 1);
    assert_eq!(project.beats.len(), 16);
    assert_eq!(project.beats[3].dump.summary.name, "Beat 3");
    assert_eq!(project.beats[3].dump.summary.events, 7);
    assert_eq!(project.raw(), all.concat());
}

#[test]
fn test_beat_dump_when_read_returns_each_pad_sound_unchanged() {
    let project = assemble(&messages(16)).unwrap().finish().unwrap();

    for (b, beat) in project.beats.iter().enumerate() {
        for (pad, sound) in beat.dump.sounds.iter().enumerate() {
            assert_eq!(
                sound.as_ref(),
                Some(&sound_for(b, pad)),
                "beat {b} pad {pad}"
            );
        }
    }
}

#[test]
fn test_beat_sound_when_encoded_as_current_returns_the_same_message_as_the_sound_alone() {
    // The format proof: the 128 bytes inside a beat are the payload of the 60 message.
    let project = assemble(&messages(16)).unwrap().finish().unwrap();

    let from_beat = project.beats[2].dump.sounds[7].as_ref().unwrap();

    assert_eq!(encode_current(from_beat), encode_current(&sound_for(2, 7)));
    let message = encode_current(&sound_for(2, 7));
    assert_eq!(
        &unpack(&message[4..message.len() - 1])[..128],
        sound_for(2, 7).bytes()
    );
}

#[test]
fn test_beat_file_when_read_as_a_5f_beat_returns_the_same_beat_with_a_5f_header() {
    let project = assemble(&messages(16)).unwrap().finish().unwrap();
    let beat = &project.beats[5];

    let file = beat.beat_file();

    assert_eq!(&file[..4], &[0xF0, 0x01, 0x28, 0x5F]);
    let summary = crate::beat::summarize(&file).unwrap();
    assert_eq!(summary, beat.dump.summary);
    // The file is in the standard layout: the machine's receiver rule reads the data back exactly.
    let body = &file[4..file.len() - 1];
    assert!(crate::pack7::is_standard_for(body, &beat.data));
    assert_eq!(
        crate::beat::read_unpacked(&unpack(body)[..beat.data.len()]).unwrap(),
        beat.dump
    );
}

#[test]
fn test_accept_when_beat_cut_short_returns_beat_error_naming_the_pad() {
    let data = beat_data(0);
    let cut = &data[..data.len() - 128 * 3];
    let mut assembler = assemble(&messages(0)).unwrap();

    let result = assembler.accept(&beat_message(0, cut));

    assert_eq!(
        result,
        Err(ProjectError::Beat {
            index: 0,
            error: BeatError::TruncatedSounds { pad: 29 }
        })
    );
    assert!(result.unwrap_err().to_string().contains("pad 30"));
}

#[test]
fn test_accept_when_beat_number_skips_some_returns_accepted_and_names_the_absent_beats() {
    let mut assembler = assemble(&messages(2)).unwrap();

    assert_eq!(
        assembler.accept(&beat_message(5, &beat_data(5))),
        Ok(Accepted::Beat(5))
    );
    assert_eq!(
        assembler.accept(&beat_message(15, &beat_data(15))),
        Ok(Accepted::Beat(15))
    );

    let project = assembler.finish().unwrap();
    assert_eq!(
        project.absent_beats(),
        vec![2, 3, 4, 6, 7, 8, 9, 10, 11, 12, 13, 14]
    );
    assert!(!project.is_complete());
    let kept: Vec<u8> = project.beats.iter().map(|b| b.index).collect();
    assert_eq!(kept, vec![0, 1, 5, 15]);
    assert_eq!(project.beats[2].dump.summary.name, "Beat 5");
}

#[test]
fn test_accept_when_first_beat_is_not_zero_returns_accepted() {
    let mut assembler = assemble(&messages(0)).unwrap();

    assert_eq!(
        assembler.accept(&beat_message(7, &beat_data(7))),
        Ok(Accepted::Beat(7))
    );
    assert_eq!(assembler.last_beat(), Some(7));
}

#[test]
fn test_accept_when_beat_repeats_returns_out_of_order_error() {
    let mut assembler = assemble(&messages(2)).unwrap();

    let result = assembler.accept(&beat_message(1, &beat_data(1)));

    assert_eq!(
        result,
        Err(ProjectError::BeatOutOfOrder { got: 1, after: 1 })
    );
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("beat 2 arrived after beat 2"));
    assert_eq!(assembler.beats_received(), 2);
}

#[test]
fn test_accept_when_beat_number_decreases_returns_out_of_order_error() {
    let mut assembler = assemble(&messages(0)).unwrap();
    assembler.accept(&beat_message(9, &beat_data(9))).unwrap();

    let result = assembler.accept(&beat_message(4, &beat_data(4)));

    assert_eq!(
        result,
        Err(ProjectError::BeatOutOfOrder { got: 4, after: 9 })
    );
    assert_eq!(assembler.beats_received(), 1);
}

#[test]
fn test_accept_when_beat_number_is_16_or_more_returns_beat_number_error() {
    let mut assembler = assemble(&messages(0)).unwrap();

    assert_eq!(
        assembler.accept(&beat_message(16, &beat_data(0))),
        Err(ProjectError::BeatNumber(16))
    );
}

#[test]
fn test_accept_when_beat_comes_before_the_header_returns_error_and_keeps_nothing() {
    let mut assembler = ProjectAssembler::new();

    let result = assembler.accept(&beat_message(0, &beat_data(0)));

    assert_eq!(result, Err(ProjectError::BeatBeforeHeader { beat: 0 }));
    assert_eq!(assembler.beats_received(), 0);
}

#[test]
fn test_accept_when_header_arrives_twice_returns_error() {
    let mut assembler = assemble(&messages(0)).unwrap();

    assert_eq!(
        assembler.accept(&header_message(0)),
        Err(ProjectError::HeaderTwice)
    );
}

#[test]
fn test_accept_when_packed_byte_has_bit_7_returns_packing_error() {
    let mut message = beat_message(0, &beat_data(0));
    message[20] |= 0x80;
    let mut assembler = assemble(&messages(0)).unwrap();

    assert!(matches!(
        assembler.accept(&message),
        Err(ProjectError::Packing(PackError::HighBit { at: 15 }))
    ));
}

#[test]
fn test_accept_when_last_group_is_not_completed_returns_packing_error() {
    let mut message = beat_message(0, &beat_data(0));
    message.remove(message.len() - 2);
    let mut assembler = assemble(&messages(0)).unwrap();

    assert!(matches!(
        assembler.accept(&message),
        Err(ProjectError::Packing(PackError::PartialGroup { .. }))
    ));
}

#[test]
fn test_accept_when_bytes_follow_the_last_sound_returns_trailing_data_error() {
    let mut data = beat_data(0);
    data.extend([0, 0, 0, 0, 0, 0, 0, 1]);
    let mut assembler = assemble(&messages(0)).unwrap();

    assert!(matches!(
        assembler.accept(&beat_message(0, &data)),
        Err(ProjectError::Beat {
            error: BeatError::TrailingData(_),
            ..
        })
    ));
}

#[test]
fn test_accept_when_header_has_unknown_parameters_version_returns_error() {
    let mut words = [0u16; 44];
    words[0] = 9;
    let data: Vec<u8> = words.iter().flat_map(|w| w.to_be_bytes()).collect();

    assert_eq!(
        ProjectAssembler::new().accept(&frame(CMD_PROJECT_HEADER, &[], &data)),
        Err(ProjectError::HeaderVersion(9))
    );
}

#[test]
fn test_accept_when_header_stops_inside_the_play_lists_returns_truncated_error() {
    let whole = header_message(2);
    let data = unpack(&whole[4..whole.len() - 1]);

    let result = ProjectAssembler::new().accept(&frame(CMD_PROJECT_HEADER, &[], &data[..200]));

    assert_eq!(result, Err(ProjectError::HeaderTruncated));
}

#[test]
fn test_accept_when_play_list_version_is_unknown_returns_error() {
    let whole = header_message(0);
    let mut data = unpack(&whole[4..whole.len() - 1]);
    data[2 * 44 + 1] = 7;

    assert_eq!(
        ProjectAssembler::new().accept(&frame(CMD_PROJECT_HEADER, &[], &data)),
        Err(ProjectError::PlayListVersion(7))
    );
}

#[test]
fn test_accept_when_beat_format_version_is_unknown_returns_unsupported_version() {
    let mut data = beat_data(0);
    data[1] = 11;
    let mut assembler = assemble(&messages(0)).unwrap();

    assert!(matches!(
        assembler.accept(&beat_message(0, &data)),
        Err(ProjectError::Beat {
            error: BeatError::UnsupportedVersion(11),
            ..
        })
    ));
}

#[test]
fn test_accept_when_event_chunk_is_zero_words_returns_bad_chunk_error() {
    let mut data = beat_data(0);
    data[538 * 2] = 0;
    data[538 * 2 + 1] = 0;
    let mut assembler = assemble(&messages(0)).unwrap();

    assert!(matches!(
        assembler.accept(&beat_message(0, &data)),
        Err(ProjectError::Beat {
            error: BeatError::BadChunk { count: 0, .. },
            ..
        })
    ));
}

#[test]
fn test_accept_when_other_messages_arrive_returns_ignored_and_keeps_nothing() {
    let mut assembler = ProjectAssembler::new();

    for other in [
        vec![0xF0, 0x01, 0x28, 0x60, 0x00, 0xF7],
        vec![0xF0, 0x7E, 0x7F, 0x06, 0x02, 0xF7],
        vec![0xF0, 0xF7],
    ] {
        assert_eq!(assembler.accept(&other), Ok(Accepted::Ignored));
    }
    assert!(!assembler.has_header());
}

#[test]
fn test_accept_when_header_has_no_end_byte_returns_malformed() {
    let mut message = header_message(0);
    message.pop();

    assert_eq!(
        ProjectAssembler::new().accept(&message),
        Err(ProjectError::Malformed)
    );
}

#[test]
fn test_finish_when_a_beat_is_missing_returns_a_partial_project_naming_it() {
    let project = assemble(&messages(15)).unwrap().finish().unwrap();

    assert_eq!(project.beats.len(), 15);
    assert_eq!(project.absent_beats(), vec![15]);
    assert!(!project.is_complete());
}

#[test]
fn test_finish_when_header_only_returns_a_project_with_no_beat_and_all_absent() {
    let project = assemble(&messages(0)).unwrap().finish().unwrap();

    assert!(project.beats.is_empty());
    assert_eq!(project.absent_beats(), (0..16).collect::<Vec<u8>>());
}

#[test]
fn test_finish_when_all_16_beats_came_returns_a_complete_project_with_none_absent() {
    let project = assemble(&messages(16)).unwrap().finish().unwrap();

    assert!(project.absent_beats().is_empty());
    assert!(project.is_complete());
}

#[test]
fn test_accept_when_beat_15_arrives_after_gaps_returns_it_as_the_last_beat() {
    let mut assembler = assemble(&messages(0)).unwrap();
    for n in [0u8, 2, 5, 15] {
        assembler
            .accept(&beat_message(n, &beat_data(n as usize)))
            .unwrap();
    }

    assert_eq!(assembler.last_beat(), Some(15));
    assert_eq!(assembler.finish().unwrap().absent_beats().len(), 12);
}

#[test]
fn test_finish_when_nothing_arrived_returns_no_header_error() {
    let err = ProjectAssembler::new().finish().unwrap_err();

    assert_eq!(err, ProjectError::NoHeader);
    assert!(err.to_string().contains("no project header"));
}

#[test]
fn test_read_unpacked_when_old_version_returns_16_sounds_and_none_for_the_other_pads() {
    let mut data = beat_data(0);
    data[1] = 6;
    data.truncate(data.len() - 16 * 128);

    let dump = beat::read_unpacked(&data).unwrap();

    assert_eq!(dump.sounds.iter().flatten().count(), 16);
    assert!(dump.sounds[16].is_none());
}

#[test]
fn test_read_unpacked_when_old_version_but_32_sounds_follow_returns_trailing_data_error() {
    let mut data = beat_data(0);
    data[1] = 6;

    assert!(matches!(
        beat::read_unpacked(&data),
        Err(BeatError::TrailingData(_))
    ));
}

#[test]
fn test_read_unpacked_when_several_event_chunks_returns_events_of_all_and_sounds_after() {
    let mut data = unpack(&beat_message(0, &beat_data(0))[5..]);
    data.truncate(538 * 2);
    let mut words = vec![2001u16];
    words.extend((0..2000).map(|i| i as u16));
    words.push(6);
    words.extend((0..5).map(|i| i as u16));
    data.extend(words.iter().flat_map(|w| w.to_be_bytes()));
    for pad in 0..PADS {
        data.extend_from_slice(sound_for(9, pad).bytes());
    }

    let dump = beat::read_unpacked(&data).unwrap();

    assert_eq!(dump.summary.events, 401);
    assert_eq!(dump.sounds[31].as_ref(), Some(&sound_for(9, 31)));
}

#[test]
fn test_read_header_when_non_empty_lists_returns_how_many_and_their_size() {
    let whole = header_message(3);
    let size = 2 * (44 + 1 + 3 * 111 + 1);
    let data = crate::pack7::unpack_flushed(&whole[4..whole.len() - 1], size).unwrap();

    let (header, used) = read_header(&data).unwrap();

    assert_eq!(header.play_lists_used, 3);
    assert_eq!(used, size);
}

#[test]
fn test_accept_when_beat_ends_in_every_partial_group_length_with_msbs_set_returns_the_same_sounds()
{
    // Events are 10 bytes each: 0 to 6 events cover every length modulo 7, so every partial last group of the
    // firmware's flush (1 to 6 bytes) and the full one. The last sound's final bytes carry MSBs, which a reader that
    // takes the standard head layout loses.
    for events in 0..=7 {
        let sounds: [Sound; PADS] = std::array::from_fn(|pad| {
            let mut bytes = *sound_for(1, pad).bytes();
            bytes[120..].fill(0xFF);
            Sound::from_bytes(bytes)
        });
        let data = synthetic_beat_data("Tail", &[], events, &sounds);
        let mut assembler = assemble(&messages(0)).unwrap();

        assembler.accept(&beat_message(0, &data)).unwrap();

        let project = assembler.finish().unwrap();
        let read = project.beats[0].dump.sounds[PADS - 1].as_ref().unwrap();
        assert_eq!(read.bytes(), sounds[PADS - 1].bytes(), "{events} events");
    }
}

#[test]
fn test_accept_when_header_is_flushed_with_lists_returns_header_with_list_names() {
    let mut assembler = ProjectAssembler::new();

    assembler.accept(&header_message(2)).unwrap();

    let header = assembler.finish().unwrap().header;
    assert_eq!(header.play_list_version, Some(2));
    assert_eq!(header.play_lists_used, 2);
    assert_eq!(
        header.play_list_names,
        [Some(String::new()), Some(String::new()), None, None]
    );
}
/// A project whose beats end in every length of partial group (0 to 6 bytes in the last group), as the machine sends it.
fn tail_messages() -> Vec<Vec<u8>> {
    let mut all = vec![header_message(1)];
    all.extend((0..7).map(|n| beat_message(n as u8, &crate::beat::synthetic_tail_beat(n))));
    all
}

/// Splits concatenated SysEx messages.
fn split(bytes: &[u8]) -> Vec<&[u8]> {
    bytes.split_inclusive(|byte| *byte == 0xF7).collect()
}

#[test]
fn test_standard_when_beats_end_in_every_partial_group_returns_messages_the_receiver_reads_exactly()
{
    let project = assemble(&tail_messages()).unwrap().finish().unwrap();

    let standard = project.standard();

    let messages = split(&standard);
    assert_eq!(messages.len(), 8);
    let header = messages[0];
    assert_eq!(header[3], CMD_PROJECT_HEADER);
    assert!(crate::pack7::is_standard_for(
        &header[4..header.len() - 1],
        &project.header_data
    ));
    for (n, message) in messages[1..].iter().enumerate() {
        let body = &message[5..message.len() - 1];
        let data = crate::beat::synthetic_tail_beat(n);
        assert_eq!(message[3], CMD_PROJECT_BEAT, "tail {n}");
        assert_eq!(message[4], n as u8, "tail {n}");
        assert!(crate::pack7::is_standard_for(body, &data), "tail {n}");
        // The receiver's rule, MSBs of the last bytes included.
        assert_eq!(unpack(body)[..data.len()], data[..], "tail {n}");
        assert_eq!(project.beats[n].data, data, "tail {n}");
    }
}

#[test]
fn test_standard_when_decoded_returns_the_same_header_and_beats_as_the_received_project() {
    let project = assemble(&tail_messages()).unwrap().finish().unwrap();
    let standard = project.standard();
    let messages = split(&standard);

    // The assembler reads the machine's layout only; a standard file is read by the tolerant reader.
    let header = &messages[0][4..messages[0].len() - 1];
    let header_data =
        crate::pack7::unpack_tolerant_with(header, |data| read_header_exact(data).map(|_| ()))
            .unwrap();
    assert_eq!(header_data, project.header_data);
    for (message, beat) in messages[1..].iter().zip(&project.beats) {
        let data = crate::beat::read_body(&message[5..message.len() - 1]).unwrap();
        assert_eq!(data, beat.data);
    }
}

#[test]
fn test_raw_when_the_machine_flushed_tails_are_kept_returns_bytes_the_receiver_misreads() {
    // Counter-proof: keeping the received bytes (what raw() returns) fails the standard check for tails of 1 to 6.
    let project = assemble(&tail_messages()).unwrap().finish().unwrap();
    let raw = project.raw();

    for (n, message) in split(&raw)[1..].iter().enumerate() {
        let body = &message[5..message.len() - 1];
        let data = crate::beat::synthetic_tail_beat(n);
        assert_eq!(
            crate::pack7::is_standard_for(body, &data),
            n == 0,
            "tail {n}: only a full last group is the same in both layouts"
        );
    }
    assert_ne!(raw, project.standard());
}

#[test]
fn test_beat_file_when_the_beat_ends_in_every_partial_group_returns_a_standard_5f() {
    let project = assemble(&tail_messages()).unwrap().finish().unwrap();

    for (n, beat) in project.beats.iter().enumerate() {
        let file = beat.beat_file();
        let data = crate::beat::synthetic_tail_beat(n);

        assert!(
            crate::pack7::is_standard_for(&file[4..file.len() - 1], &data),
            "tail {n}"
        );
        assert_eq!(
            crate::beat::standard_message(&file).unwrap(),
            file,
            "tail {n}"
        );
    }
}
fn project_of(lists_used: usize, beats: usize) -> Project {
    let mut all = vec![header_message(lists_used)];
    all.extend((0..beats).map(|n| beat_message(n as u8, &beat_data(n))));
    assemble(&all).unwrap().finish().unwrap()
}

/// The project as a machine holding `data` as its header would send it back.
fn with_header(project: &Project, data: &[u8]) -> Project {
    let mut all = vec![header_data_message(data)];
    all.extend(
        project
            .beats
            .iter()
            .map(|b| beat_message(b.index, b.data())),
    );
    assemble(&all).unwrap().finish().unwrap()
}

#[test]
fn test_send_messages_when_project_has_beats_returns_5e_then_5c_in_order_in_the_standard_layout() {
    let project = project_of(1, 16);

    let sent = project.send_messages();

    assert_eq!(sent[0][3], CMD_PROJECT_HEADER);
    let numbers: Vec<u8> = sent[1..].iter().map(|m| m[4]).collect();
    assert_eq!(numbers, (0..16).collect::<Vec<u8>>());
    assert!(sent[1..].iter().all(|m| m[3] == CMD_PROJECT_BEAT));
    assert_eq!(sent.concat(), project.standard());
    let again = assemble(&sent).unwrap().finish().unwrap();
    assert_eq!(again.header_data(), project.header_data());
    assert_eq!(again.beats, project.beats);
}

#[test]
fn test_send_messages_when_project_came_in_the_flushed_layout_returns_exact_padded_bytes() {
    // Counter-proof of the layout: the raw capture differs from what is sent, the decoded data does not.
    let project = project_of(1, 3);

    assert_ne!(project.raw(), project.standard());
    for (beat, message) in project.beats.iter().zip(&project.send_messages()[1..]) {
        assert_eq!(
            &message[5..message.len() - 1],
            &pack_padded(beat.data())[..]
        );
    }
}

#[test]
fn test_with_empty_play_lists_when_lists_used_returns_four_empty_lists_and_same_beats() {
    let project = project_of(3, 16);

    let emptied = project.with_empty_play_lists();

    assert_eq!(emptied.header.lists_used(), [false; 4]);
    assert_eq!(emptied.beats, project.beats);
    assert_eq!(emptied.header_data()[..90], project.header_data()[..90]);
    assert_eq!(&emptied.header_data()[90..], &[0xFE; 8][..]);
    let back = assemble(&emptied.send_messages())
        .unwrap()
        .finish()
        .unwrap();
    assert_eq!(back.header.lists_used(), [false; 4]);
}

#[test]
fn test_lists_used_when_lists_one_and_two_are_used_returns_them_flagged() {
    assert_eq!(
        project_of(2, 0).header.lists_used(),
        [true, true, false, false]
    );
    assert_eq!(project_of(0, 0).header.lists_used(), [false; 4]);
}

#[test]
fn test_compare_when_machine_returns_what_was_sent_returns_no_difference() {
    let project = project_of(2, 16);
    let back = with_header(&project, project.header_data());

    let result = compare(&project, &back);

    assert!(result.matches(), "{:?}", result.differences);
    assert_eq!(result.beats_compared, 16);
    assert!(result.notes.is_empty());
}

#[test]
fn test_compare_when_a_list_step_differs_returns_the_list_and_the_step() {
    // Counter-proof: a list that came back wrong (the n°28 defect) must be named, list by list.
    let project = project_of(2, 2);
    let mut data = project.header_data().to_vec();
    let second_list = 2 * (PARAM_WORDS + 1 + 1 + LIST_BODY_WORDS);
    let step_4 = second_list + 2 * (1 + LIST_NAME_WORDS + 3);
    data[step_4 + 1] = 0x09;
    let back = with_header(&project, &data);

    let result = compare(&project, &back);

    assert_eq!(
        result.differences,
        vec![Difference::PlayListContent {
            list: 2,
            end: false,
            name: false,
            steps: vec![4]
        }]
    );
    assert!(result.differences[0].to_string().contains("Play List 2"));
}

#[test]
fn test_compare_when_a_list_is_used_on_one_side_only_returns_the_use_difference() {
    let project = project_of(2, 1);
    let mut data = project.header_data().to_vec();
    data.truncate(2 * (PARAM_WORDS + 1 + 1 + LIST_BODY_WORDS));
    data.extend([0xFE; 6]);
    let back = with_header(&project, &data);

    let result = compare(&project, &back);

    assert_eq!(
        result.differences,
        vec![Difference::PlayListUse {
            list: 2,
            sent_used: true
        }]
    );
}

#[test]
fn test_compare_when_list_one_was_empty_and_came_back_used_returns_a_note_not_a_difference() {
    let sent = project_of(0, 1);
    let used = project_of(1, 1);
    let back = with_header(&sent, used.header_data());

    let result = compare(&sent, &back);

    assert!(result.matches());
    assert!(result.notes[0].contains("Play List 1"));
}

#[test]
fn test_compare_when_list_two_came_back_used_but_was_sent_empty_returns_a_difference() {
    let sent = project_of(0, 1);
    let back = with_header(&sent, project_of(2, 1).header_data());

    let result = compare(&sent, &back);

    assert_eq!(
        result.differences,
        vec![Difference::PlayListUse {
            list: 2,
            sent_used: false
        }]
    );
}

#[test]
fn test_compare_when_beat_name_events_and_sounds_differ_returns_each_precisely() {
    let project = project_of(1, 3);
    let other_beat = crate::beat::synthetic_beat_data(
        "Other",
        &[(0, 1)],
        9,
        &std::array::from_fn(|pad| sound_for(7, pad)),
    );
    let mut all = vec![header_data_message(project.header_data())];
    all.push(beat_message(0, &other_beat));
    all.push(beat_message(2, &beat_data(2)));
    let back = assemble(&all).unwrap().finish().unwrap();

    let result = compare(&project, &back);

    let texts: Vec<String> = result.differences.iter().map(ToString::to_string).collect();
    assert!(texts
        .iter()
        .any(|t| t.contains("beat 1: name \"Beat 0\" sent, \"Other\"")));
    assert!(texts.iter().any(|t| t.contains("beat 1: 4 events sent, 9")));
    assert!(texts.iter().any(|t| t.starts_with("beat 1: sound differs")));
    assert!(texts.contains(&"beat 2: the machine did not send it back".to_string()));
    assert_eq!(result.beats_compared, 3);
}

#[test]
fn test_compare_when_the_sent_project_has_fewer_beats_returns_a_note_and_ignores_the_others() {
    let sent = project_of(1, 4);
    let back = with_header(&project_of(1, 16), sent.header_data());

    let result = compare(&sent, &back);

    assert!(result.matches(), "{:?}", result.differences);
    assert_eq!(result.beats_compared, 4);
    assert!(result.notes[0].contains("5, 6, 7"));
}

#[test]
fn test_compare_when_parameter_words_differ_returns_their_indexes() {
    let project = project_of(1, 1);
    let mut data = project.header_data().to_vec();
    data[2 * 7 + 1] = 3;
    data[2 * 30 + 1] = 1;
    let back = with_header(&project, &data);

    let result = compare(&project, &back);

    assert_eq!(
        result.differences,
        vec![Difference::Parameters { words: vec![7, 30] }]
    );
}

#[test]
fn test_decode_received_when_messages_come_in_either_layout_returns_the_same_decoded_data() {
    let project = project_of(1, 1);

    let standard = decode_received(&project.send_messages()[0]).unwrap();
    let machine = decode_received(&header_message(1)).unwrap();

    assert_eq!(standard, machine);
    assert_eq!(
        decode_received(&[0xF0, 0x01, 0x28, 0x5F, 0x00, 0xF7]),
        Ok(None)
    );
}

/// The data of a `5E` header (no Play List used) with the given words 4 and 18.
fn header_with_quantize(word4: u16, word18: u16) -> Vec<u8> {
    let mut words = vec![0u16; PARAM_WORDS];
    words[0] = LAST_PARAMS_VERSION;
    words[QUANTIZE_WORD] = word4;
    words[FINE_WORD] = word18;
    words.push(LIST_VERSION);
    words.extend([EMPTY_LIST; LISTS]);
    words.iter().flat_map(|w| w.to_be_bytes()).collect()
}

#[test]
fn test_read_header_when_quantize_words_are_set_returns_the_fine_or_official_value() {
    let read = |w4, w18| {
        read_header(&header_with_quantize(w4, w18))
            .unwrap()
            .0
            .quantize
    };

    assert_eq!(read(7 << 8, 0x5107), Some(Quantize::Fine(7)));
    assert_eq!(read(7 << 8, 0x5101).map(Quantize::label), Some("8 Dot"));
    assert_eq!(read(3 << 8, 0xFFFF), Some(Quantize::Official(3)));
    assert_eq!(read(7 << 8, 0x5100), Some(Quantize::Official(7)));
    assert_eq!(read(9 << 8, 0), None);
}
