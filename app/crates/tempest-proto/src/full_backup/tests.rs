use super::synthetic::{
    answer, answer_with, boot_flash_image, message, system_payload, voice_image, STOCK_ROUTE,
};
use super::*;
use crate::versions::Changes;

fn voice_message(k: u8, image: &[u8]) -> Vec<u8> {
    message(Chip::Voice(k), image)
}

fn decoded_answer() -> Vec<Backup> {
    answer()
        .iter()
        .map(|m| decode(m).expect("the synthetic answer decodes"))
        .collect()
}

#[test]
fn test_decode_when_healthy_answer_returns_the_eight_chips_in_order() {
    let items = decoded_answer();

    let chips: Vec<Chip> = items.iter().map(|i| i.chip).collect();

    assert_eq!(chips, Chip::all().to_vec());
    assert!(check_set(&items).is_empty());
}

#[test]
fn test_decode_when_voice_returns_its_own_35_calibration_words() {
    let items = decoded_answer();

    let Content::Voice(first) = &items[0].content else {
        panic!("voice 0 holds a calibration");
    };
    let Content::Voice(last) = &items[5].content else {
        panic!("voice 5 holds a calibration");
    };

    assert_eq!(first.words, synthetic::calibration_words(0));
    assert_ne!(first.words, last.words);
    assert_eq!(first.scalars().len(), 3);
}

#[test]
fn test_chip_when_id_round_trips_returns_the_same_chip_and_unknown_ids_none() {
    for chip in Chip::all() {
        assert_eq!(Chip::from_id(chip.id()), Some(chip));
    }
    for id in [0x71, 0x72, 0x73, 0x74, 0x77, 0x7E, 0x40] {
        assert_eq!(Chip::from_id(id), None, "{id:#04x}");
    }
}

#[test]
fn test_decode_when_boot_flash_returns_its_configuration_words() {
    let items = decoded_answer();

    let Content::MainBoot(boot) = &items[6].content else {
        panic!("boot flash");
    };

    assert_eq!(boot.devcfg[3], 0x7FFF_FFDF);
    assert_eq!(items[6].image.len(), 12 * BLOCK_SIZE);
}

/// Pad k's channel value k + 1 (1-32): every pad different, both ranges of n°58 used.
fn every_pad_channel() -> [u8; PAD_CHANNELS] {
    std::array::from_fn(|k| k as u8 + 1)
}

/// The message `76` built by hand from the contract (`midi-protocol.md` § Full backup), not by `synthetic`: groups of
/// 7 data bytes behind a head byte whose bit j is the MSB of byte j, the last group padded with zeros.
fn system_message_by_hand(payload: &[u8]) -> Vec<u8> {
    let mut out = vec![0xF0, 0x01, 0x28, 0x76];
    for group in payload.chunks(7) {
        let mut seven = [0u8; 7];
        seven[..group.len()].copy_from_slice(group);
        out.push(
            seven
                .iter()
                .enumerate()
                .fold(0, |head, (j, b)| head | (b >> 7) << j),
        );
        out.extend(seven.iter().map(|b| b & 0x7F));
    }
    out.push(0xF7);
    out
}

#[test]
fn test_answer_when_built_returns_the_sizes_and_order_of_the_contract() {
    let sizes: Vec<(u8, usize)> = answer().iter().map(|m| (m[3], m.len())).collect();

    assert_eq!(
        sizes,
        vec![
            (0x78, 74_951),
            (0x79, 74_951),
            (0x7A, 74_951),
            (0x7B, 74_951),
            (0x7C, 74_951),
            (0x7D, 74_951),
            (0x75, 14_059),
            (0x76, 277),
        ]
    );
    assert_eq!(
        sizes.iter().map(|(_, n)| n).sum::<usize>(),
        464_042,
        "the wire total the DIN timing is computed from"
    );
}

#[test]
fn test_decode_when_system_returns_the_version_the_202_settings_and_the_32_pad_channels() {
    let channels = every_pad_channel();
    let payload = system_payload(&channels);
    let wire = system_message_by_hand(&payload);

    let backup = decode(&wire).expect("the contract's 76 decodes");

    assert_eq!(
        wire,
        message(Chip::System, &payload),
        "synthetic = contract"
    );
    assert_eq!(wire.len(), 277);
    let Content::System(system) = &backup.content else {
        panic!("system");
    };
    assert_eq!(system.version, 7);
    assert_eq!(system.settings[..], payload[1..203]);
    assert_eq!(system.pad_channels, channels);
    assert_eq!(backup.image, payload);
}

#[test]
fn test_decode_when_system_lacks_the_pad_channels_or_a_byte_or_has_more_returns_a_system_error() {
    let payload = system_payload(&every_pad_channel());
    // The study's layout (version word + 202 settings), the version byte + 202 settings without the pad channels,
    // a group of pad channels missing, one byte too many. (A last channel missing is not seen: the zero padding of
    // the last group reads as channel 0, the wire is the same.)
    let mut study = vec![0, 7];
    study.extend(&payload[1..203]);
    let cases = [
        ("study layout", study),
        ("no pad channels (203 bytes)", payload[..203].to_vec()),
        ("24 pad channels, one group short", payload[..227].to_vec()),
        ("236 bytes", [&payload[..], &[5]].concat()),
    ];

    for (name, data) in cases {
        let result = decode(&system_message_by_hand(&data));
        assert!(
            matches!(result, Err(BackupError::System(_))),
            "{name}: {result:?}"
        );
    }
}

#[test]
fn test_decode_when_system_packing_is_not_standard_returns_a_system_error() {
    let payload = system_payload(&every_pad_channel());
    let good = system_message_by_hand(&payload);
    let mut dirty_padding = good.clone();
    let last = dirty_padding.len() - 2;
    dirty_padding[last] = 0x01;
    let mut high_bit = good.clone();
    high_bit[10] |= 0x80;
    let unpadded = [&good[..good.len() - 4], &[0xF7][..]].concat();

    for (name, wire) in [
        ("padding not zero", dirty_padding),
        ("a byte with bit 7", high_bit),
        ("last group not completed", unpadded),
    ] {
        assert!(
            matches!(decode(&wire), Err(BackupError::System(_))),
            "{name}"
        );
    }
}

#[test]
fn test_decode_when_system_version_or_a_pad_channel_is_out_of_range_returns_the_reason() {
    let mut wrong_version = system_payload(&STOCK_ROUTE);
    wrong_version[0] = 6;
    let mut channel_33 = system_payload(&STOCK_ROUTE);
    channel_33[203 + 9] = 33;

    let version = decode(&system_message_by_hand(&wrong_version)).unwrap_err();
    let channel = decode(&system_message_by_hand(&channel_33)).unwrap_err();

    assert!(
        version.to_string().contains("settings version 6"),
        "{version}"
    );
    assert!(channel.to_string().contains("pad 10"), "{channel}");
}

#[test]
fn test_machine_file_when_built_returns_203_words_with_the_documented_high_bytes() {
    let channels = every_pad_channel();
    let mut payload = system_payload(&channels);
    // Setting i is payload byte 1 + i.
    payload[1 + 47] = 0x90;
    payload[1 + 137] = 0x85;
    payload[1 + 138] = 0x80;
    payload[1 + 139] = 0x7F;
    payload[1 + 201] = 0xFF;
    let Content::System(system) = decode(&system_message_by_hand(&payload))
        .expect("decodes")
        .content
    else {
        panic!("system");
    };

    let file = system.machine_file();

    let word = |w: usize| u16::from_le_bytes([file[2 * w], file[2 * w + 1]]);
    assert_eq!(file.len(), 406);
    assert_eq!(word(0), 7);
    for i in 0..202 {
        let high = match i {
            48..=79 => u16::from(channels[i - 48]),
            138..=201 if payload[1 + i] >= 0x80 => 0xFF,
            _ => 0,
        };
        assert_eq!(
            word(1 + i),
            high << 8 | u16::from(payload[1 + i]),
            "setting {i}"
        );
    }
    assert_eq!(
        word(1 + 47),
        0x0090,
        "47 is not a pad note: no channel, no sign"
    );
    assert_eq!(
        word(1 + 48),
        0x0100 | u16::from(payload[49]),
        "pad 1's channel"
    );
    assert_eq!(
        word(1 + 79),
        0x2000 | u16::from(payload[80]),
        "pad 32's channel"
    );
    assert_eq!(word(1 + 137), 0x0085, "137 is not sign-extended");
    assert_eq!(word(1 + 138), 0xFF80);
    assert_eq!(word(1 + 139), 0x007F);
    assert_eq!(word(1 + 201), 0xFFFF);
}

#[test]
fn test_check_order_when_the_answer_is_in_order_returns_nothing() {
    assert!(check_order(&Chip::all()).is_empty());
    assert!(
        check_order(&Chip::all()[..3]).is_empty(),
        "a prefix is in order"
    );
}

#[test]
fn test_check_order_when_messages_are_swapped_doubled_or_extra_returns_each_misplaced_one() {
    let mut swapped = Chip::all().to_vec();
    swapped.swap(6, 7);
    let doubled = [Chip::Voice(0), Chip::Voice(0)];
    let mut extra = Chip::all().to_vec();
    extra.push(Chip::System);

    let swapped = check_order(&swapped);
    let doubled = check_order(&doubled);
    let extra = check_order(&extra);

    assert_eq!(swapped.len(), 2);
    assert_eq!(swapped[0].chip, Chip::System);
    assert_eq!(
        swapped[0].error,
        BackupError::OutOfOrder {
            position: 7,
            expected: Some(Chip::MainBoot)
        }
    );
    assert_eq!(doubled.len(), 1);
    assert!(doubled[0]
        .error
        .to_string()
        .contains("message 2 of the answer, where the machine sends Voice 1"));
    assert_eq!(
        extra[0].error,
        BackupError::OutOfOrder {
            position: 9,
            expected: None
        }
    );
}

#[test]
fn test_answer_with_when_pad_channels_are_given_returns_them_in_76() {
    let channels = every_pad_channel();

    let last = decode(&answer_with(&channels)[7]).expect("decodes");

    let Content::System(system) = last.content else {
        panic!("system");
    };
    assert_eq!(system.pad_channels, channels);
}

#[test]
fn test_decode_when_voice_is_shifted_by_one_byte_returns_misaligned() {
    let mut image = voice_image(1);
    image.rotate_left(1);

    let result = decode(&voice_message(1, &image));

    assert_eq!(result, Err(BackupError::Misaligned));
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("power the machine off and on"));
}

#[test]
fn test_decode_when_voice_message_is_the_empty_one_of_1_6_8_0_returns_not_read_with_the_power_cycle_advice(
) {
    for k in 0..6u8 {
        let message = [0xF0, 0x01, 0x28, 0x78 + k, 0xF7];

        let result = decode(&message);

        assert_eq!(result, Err(BackupError::NotRead(k)));
        let text = result.unwrap_err().to_string();
        assert!(text.contains(&format!("Voice {k} was not read")), "{text}");
        assert!(
            text.contains("DSI's Voice OS, or no answer"),
            "both causes named: {text}"
        );
        assert!(text.contains("power the machine off and on"), "{text}");
    }
}

#[test]
fn test_decode_body_when_given_the_empty_message_returns_an_error_without_the_power_cycle_advice() {
    // Counter-proof of the rule above: without the 1.6.8.0 case, the empty message falls to the OS-layout decoder,
    // whose refusal names no chip and gives no advice.
    let error = decode_body(&[0xF0, 0x01, 0x28, 0x78, 0xF7], |_| {
        Ok::<(), BackupError>(())
    })
    .map(|_| ())
    .expect_err("an empty OS-layout body is refused");

    assert!(
        !error.to_string().contains("power the machine off and on"),
        "{error}"
    );
    assert!(!matches!(error, BackupError::NotRead(_)));
}

#[test]
fn test_decode_when_the_boot_or_system_message_is_empty_returns_another_error_than_not_read() {
    for id in [0x75u8, 0x76] {
        let result = decode(&[0xF0, 0x01, 0x28, id, 0xF7]);

        assert!(result.is_err(), "{id:#x}");
        assert!(!matches!(result, Err(BackupError::NotRead(_))), "{id:#x}");
    }
}

#[test]
fn test_decode_when_voice_is_the_second_dump_without_boot_and_calibration_returns_misaligned_first()
{
    // The Voice's dump pointer is never reset: the second dump starts in the middle of the stream and has no boot or
    // calibration page (`feature-full-backup.md` 2).
    let mut image = voice_image(2);
    image.rotate_left(65_536 % 3);
    image[CALIBRATION_START..].fill(0);

    assert_eq!(
        decode(&voice_message(2, &image)),
        Err(BackupError::Misaligned)
    );
}

#[test]
fn test_decode_when_a_block_is_corrupt_returns_a_checksum_error() {
    let mut message = voice_message(0, &voice_image(0));
    message[4 + 1171 * 7 + 20] ^= 0x01;

    let result = decode(&message);

    assert!(matches!(
        result,
        Err(BackupError::Image(OsImageError::Checksum { .. }))
    ));
    assert!(result.unwrap_err().to_string().contains("checksum"));
}

#[test]
fn test_decode_when_a_block_is_missing_returns_the_block_count() {
    let image = voice_image(0);

    let result = decode(&voice_message(0, &image[..63 * BLOCK_SIZE]));

    assert_eq!(
        result,
        Err(BackupError::BlockCount {
            expected: 64,
            found: 63
        })
    );
}

#[test]
fn test_decode_when_the_calibration_page_is_blank_returns_the_calibration_error() {
    let mut image = voice_image(3);
    image[CALIBRATION_START..].fill(0xFF);

    let result = decode(&voice_message(3, &image));

    assert_eq!(
        result,
        Err(BackupError::Calibration(CalibrationError::Blank))
    );
    assert!(result.unwrap_err().to_string().contains("blank"));
}

#[test]
fn test_decode_when_the_calibration_is_half_written_or_not_increasing_returns_the_error() {
    let write = |image: &mut Vec<u8>, word: usize, value: u16| {
        let at = CALIBRATION_START + 3 * word;
        image[at] = (value & 0xFF) as u8;
        image[at + 1] = (value >> 8) as u8;
    };
    let mut half = voice_image(0);
    write(&mut half, 1, 0xFFFF);
    let mut backwards = voice_image(0);
    write(&mut backwards, 3, 0x8001);
    let mut crooked = voice_image(0);
    write(&mut crooked, 25, 0x1234);

    let error = |image: &[u8]| decode(&voice_message(0, image)).unwrap_err();

    assert_eq!(
        error(&half),
        BackupError::Calibration(CalibrationError::HalfWritten)
    );
    assert_eq!(
        error(&backwards),
        BackupError::Calibration(CalibrationError::NotIncreasing("A"))
    );
    assert_eq!(
        error(&crooked),
        BackupError::Calibration(CalibrationError::NotLinear)
    );
}

#[test]
fn test_decode_when_the_boot_page_is_missing_returns_boot_page_error() {
    let mut image = voice_image(0);
    image[BOOT_PAGE_START..CALIBRATION_START].fill(0);

    assert_eq!(
        decode(&voice_message(0, &image)),
        Err(BackupError::BootPageMissing)
    );
}

#[test]
fn test_decode_when_a_code_page_is_blank_returns_the_page_number() {
    let mut image = voice_image(0);
    image[5 * PAGE_BYTES..6 * PAGE_BYTES].fill(0xFF);

    assert_eq!(
        decode(&voice_message(0, &image)),
        Err(BackupError::CodePage(5))
    );
}

#[test]
fn test_decode_when_the_free_pages_are_blank_returns_the_backup() {
    let mut image = voice_image(0);
    image[30 * PAGE_BYTES..41 * PAGE_BYTES].fill(0xFF);

    assert!(decode(&voice_message(0, &image)).is_ok());
}

#[test]
fn test_decode_when_every_byte_is_the_same_returns_constant() {
    let result = decode(&voice_message(0, &vec![0xFF; VOICE_IMAGE_BYTES]));

    assert_eq!(result, Err(BackupError::Constant(0xFF)));
}

#[test]
fn test_decode_when_boot_flash_has_no_configuration_words_returns_the_error() {
    let mut image = boot_flash_image();
    let tail = image.len() - 16;
    image[tail..].fill(0xFF);

    assert_eq!(
        decode(&message(Chip::MainBoot, &image)),
        Err(BackupError::NoConfigBits)
    );
}

#[test]
fn test_decode_when_system_is_not_the_expected_layout_returns_a_system_error() {
    let mut short = system_payload(&STOCK_ROUTE);
    short.truncate(100);
    let mut wrong_version = system_payload(&STOCK_ROUTE);
    wrong_version[0] = 6;
    let mut stray = system_payload(&STOCK_ROUTE);
    stray.extend([0, 0, 0, 5]);

    for data in [short, wrong_version, stray] {
        assert!(matches!(
            decode(&message(Chip::System, &data)),
            Err(BackupError::System(_))
        ));
    }
}

#[test]
fn test_decode_when_not_a_backup_message_returns_not_backup_message() {
    for bytes in [
        vec![0xF0, 0x7E, 0x7F, 0x06, 0x01, 0xF7],
        vec![0xF0, 0x01, 0x28, 0x72, 0x00, 0xF7],
        vec![0xF0, 0x01, 0x28, 0x78],
    ] {
        assert_eq!(decode(&bytes), Err(BackupError::NotBackupMessage));
    }
}

#[test]
fn test_check_set_when_a_chip_is_missing_returns_it_named() {
    let mut items = decoded_answer();
    items.remove(3);

    let problems = check_set(&items);

    assert_eq!(problems.len(), 1);
    assert_eq!(problems[0].chip, Chip::Voice(3));
    assert_eq!(problems[0].error.to_string(), "Voice 3 did not come");
}

#[test]
fn test_check_set_when_two_voices_are_the_same_image_returns_the_second() {
    let mut items = decoded_answer();
    let first = items[0].clone();
    items[4] = Backup {
        chip: Chip::Voice(4),
        ..first
    };

    let problems = check_set(&items);

    assert_eq!(problems.len(), 1);
    assert_eq!(
        problems[0].error,
        BackupError::SameImage {
            first: 0,
            second: 4
        }
    );
}

#[test]
fn test_code_differences_when_a_chip_keeps_other_code_returns_the_count() {
    let items = decoded_answer();
    let mut other = items[1].clone();
    other.image[100] ^= 0x55;
    other.image[2000] ^= 0x55;

    assert_eq!(code_differences(&items[0], &items[1]), 0);
    assert_eq!(code_differences(&items[0], &other), 2);
}

#[test]
fn test_request_when_built_returns_the_exact_bytes_and_no_backup_id_is_an_os_id() {
    assert_eq!(REQUEST_FULL_BACKUP, [0xF0, 0x01, 0x28, 0x44, 0xF7]);
    for chip in Chip::all() {
        assert!(
            crate::os_image::OsId::from_byte(chip.id()).is_none(),
            "{chip:?} must never be an update file"
        );
    }
}

/// A synthetic Voice version whose pages 0-40 are those of `image` (the real images need the user's DSI file).
fn version_holding(image: &[u8]) -> &'static [VoiceVersion] {
    let hash: &'static str = Box::leak(sha256_hex(code_pages(image)).into_boxed_str());
    Box::leak(Box::new([VoiceVersion {
        version: "9.9.9.9",
        name: "synthetic",
        kind: voice_versions::VoiceKind::Test,
        date: "2026-11-14",
        summary: "",
        changes: Changes::default(),
        description: "",
        sha256_image: hash,
        sha256_image_written: hash,
        sha256_syx: hash,
        patches: &[],
    }]))
}

#[test]
fn test_code_pages_when_a_voice_image_returns_the_41_pages_an_update_sends() {
    let image = voice_image(0);

    assert_eq!(code_pages(&image).len(), voice_versions::SENT_LEN);
    assert_eq!(
        calibration_page(&image),
        &image[CALIBRATION_START..],
        "what the 64-block image holds of page 42"
    );
    assert_eq!(
        calibration_page(&image).len(),
        VOICE_IMAGE_BYTES - CALIBRATION_START
    );
    assert!(calibration_page(&image[..100]).is_empty());
}

#[test]
fn test_voice_build_when_the_pages_equal_a_known_version_returns_its_label() {
    let image = voice_image(0);
    let known = version_holding(&image);

    assert_eq!(
        voice_build_among(&image, known, None),
        VoiceBuild::Tempester("9.9.9.9")
    );
    assert_eq!(
        voice_build_among(&image, known, None).label(),
        "Tempester Voice 9.9.9.9"
    );
}

#[test]
fn test_voice_build_when_one_code_byte_differs_or_nothing_is_known_returns_unknown_build() {
    let image = voice_image(0);
    let known = version_holding(&image);
    let mut other_code = image.clone();
    other_code[5000] ^= 1;
    // The calibration page is the chip's own: it never makes a build unknown.
    let mut other_calibration = voice_image(3);
    other_calibration[..voice_versions::SENT_LEN].copy_from_slice(code_pages(&image));

    assert_eq!(
        voice_build_among(&other_code, known, None),
        VoiceBuild::Unknown
    );
    assert_eq!(voice_build_among(&image, &[], None), VoiceBuild::Unknown);
    assert_eq!(voice_build(&image, None), VoiceBuild::Unknown);
    assert_eq!(
        voice_build_among(&other_calibration, known, None),
        VoiceBuild::Tempester("9.9.9.9")
    );
    assert_eq!(VoiceBuild::Unknown.label(), "unknown build");
}

#[test]
fn test_voice_build_when_the_pages_equal_the_users_dsi_file_returns_dsi_15_only_if_the_app_saw_it()
{
    let image = voice_image(1);
    let dsi_hash = sha256_hex(code_pages(&image));

    assert_eq!(
        voice_build_among(&image, &[], Some(&dsi_hash)),
        VoiceBuild::Dsi15
    );
    assert_eq!(VoiceBuild::Dsi15.label(), "DSI Voice 1.5");
    assert_eq!(
        voice_build_among(&image, &[], Some(&"0".repeat(64))),
        VoiceBuild::Unknown
    );
    assert_eq!(voice_build_among(&image, &[], None), VoiceBuild::Unknown);
    // A Tempester version wins over the DSI hash when both match.
    let both = version_holding(&image);
    assert_eq!(
        voice_build_among(&image, both, Some(&dsi_hash)),
        VoiceBuild::Tempester("9.9.9.9")
    );
}
