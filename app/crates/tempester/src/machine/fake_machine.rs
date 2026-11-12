//! A scripted Tempest for demos and tests (`TEMPESTER_FAKE_MIDI=1`): answers the identity request, `40` with a
//! `60` sound, `41` with a `5F` beat, `42` with a whole project (`5E` + 16 x `5C`), all synthetic, and keeps the sound or beat it is sent, so that a restore
//! can be seen. With its remote panel (Tempester 1.6.0.0, `fake_panel`) it also answers keys and LED requests, reports
//! its Pad Function mode (n°40) and answers the version request (n°39) with 1.6.5.0, so that every control of the app is on
//! (it keeps the 32 pad channels of n°58 in its panel).
//! A project sent to it (`5E`, then `5C n`) is stored and answered by request `42` (beats not sent keep their content,
//! as on the machine); with the n°28 defect switched on it reads the Play Lists like the stock OS (`shifted_lists`).
//! `debug_send` plays the Debug-menu OS sends, one of them as a constant (as seen on the real machine).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use super::fake_panel::FakePanel;
use tempest_midi::message::Message;
use tempest_midi::{FakeTransport, StreamParser};
use tempest_proto::beat::{self, PADS};
use tempest_proto::beat_sequence::{synthetic_sequence_beat, BeatEvent};
use tempest_proto::identity::{TempesterVersion, IDENTITY_REQUEST, VERSION_REQUEST};
use tempest_proto::mixer_report::{report_bytes, MixerItem};
use tempest_proto::os_image::{self, OsId, BLOCK_SIZE};
use tempest_proto::pad_notes::PadNotes;
use tempest_proto::project::{self, Received, BEATS};
use tempest_proto::sound::Sound;
use tempest_proto::sound_message::{
    self, SoundMessage, REQUEST_BEAT, REQUEST_PROJECT, REQUEST_SOUND,
};

const IDENTITY_REPLY: [u8; 14] = [
    0xF0, 0x7E, 0x7F, 0x06, 0x02, 0x01, 0x28, 0x01, 0x00, 0x00, 0x15, 0x02, 0x7F, 0xF7,
];

/// What the fake announces to the version request (n°39).
const ANNOUNCED: TempesterVersion = TempesterVersion([1, 6, 5, 0]);

/// The project in the machine's RAM: its decoded `5E` data and the decoded data of each of the 16 beats.
struct StoredProject {
    header: Vec<u8>,
    beats: Vec<Vec<u8>>,
}

impl StoredProject {
    fn synthetic() -> Self {
        let mut stored = Self {
            header: Vec::new(),
            beats: vec![Vec::new(); BEATS],
        };
        for message in project_messages() {
            // The synthetic messages are built here and always decode: a failure is a bug of the fake itself.
            if let Ok(Some(received)) = project::decode_received(&message) {
                stored.keep(received, false);
            }
        }
        stored
    }

    fn keep(&mut self, received: Received, defect: bool) {
        match received {
            Received::Header(data) if defect => self.header = shifted_lists(&data),
            Received::Header(data) => self.header = data,
            Received::Beat { index, data } => self.beats[usize::from(index)] = data,
        }
    }

    /// What request `42` answers: the `5E`, then every beat, in the machine's own (flushed) layout.
    fn messages(&self) -> Vec<Vec<u8>> {
        let mut out = vec![project::header_data_message(&self.header)];
        out.extend(
            self.beats
                .iter()
                .enumerate()
                .map(|(index, data)| project::beat_message(index as u8, data)),
        );
        out
    }
}

const EMPTY_LIST_WORD: u16 = 0xFEFE;
/// Words of a header before the Play Lists: 44 parameters and the Play List version.
const WORDS_BEFORE_LISTS: usize = 45;
/// Words after the End word of a used list: 10 of name, 100 of steps.
const LIST_BODY: usize = 110;

/// The header data as the stock project receiver leaves it: after a used list it reads one word too many, so
/// every later list starts one word late, and a list read past the data holds zeros (`Docs/analyses/
/// bug-project-receive.md` 1). Only meaningful for parameters version 4 or later, the ones with Play Lists.
pub(crate) fn shifted_lists(data: &[u8]) -> Vec<u8> {
    let words: Vec<u16> = data
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_be_bytes(*pair))
        .collect();
    if words.first().is_none_or(|version| *version < 4) {
        return data.to_vec();
    }
    let word = |at: usize| words.get(at).copied().unwrap_or(0);
    let mut out = words[..WORDS_BEFORE_LISTS.min(words.len())].to_vec();
    let mut at = WORDS_BEFORE_LISTS;
    for _ in 0..project::PLAY_LISTS {
        if word(at) == EMPTY_LIST_WORD {
            out.push(EMPTY_LIST_WORD);
            at += 1;
        } else {
            out.extend((0..=LIST_BODY).map(|k| word(at + k)));
            at += LIST_BODY + 2;
        }
    }
    out.iter().flat_map(|w| w.to_be_bytes()).collect()
}

struct State {
    parser: StreamParser,
    sound: Sound,
    beat: Vec<u8>,
    project: StoredProject,
    /// The n°28 defect: Play Lists of a received project are read one word late after a used list.
    project_defect: Arc<AtomicBool>,
    /// `None`: a machine without the remote panel (original OS, or a Tempester before 1.6.0.0).
    panel: Option<FakePanel>,
}

fn initial_sound() -> Sound {
    let mut sound = Sound::with_defaults();
    // Values inside the table maxima, so a failure here is a bug in the fake itself.
    for (index, value) in [(34, 96), (35, 40), (73, 59), (6, 43)] {
        let _ = sound.set(index, value);
    }
    let _ = sound.set_name("Fake Kick");
    sound
}

/// A fake transport whose responder plays the machine; `remote_panel = false` plays one that never answers keys
/// or LED requests (original OS, or a Tempester before 1.6.0.0).
#[cfg(test)]
pub fn scripted(remote_panel: bool) -> Arc<FakeTransport> {
    scripted_with(remote_panel, Arc::default(), PadNotes::default())
}

/// `scripted` with the switch of the n°28 defect, which the caller may flip while the machine runs, and the
/// machine's own note of each pad.
pub fn scripted_with(
    remote_panel: bool,
    project_defect: Arc<AtomicBool>,
    pad_notes: PadNotes,
) -> Arc<FakeTransport> {
    let transport = Arc::new(FakeTransport::new());
    let state = Mutex::new(State {
        parser: StreamParser::new(),
        sound: initial_sound(),
        beat: fake_beat_dump(),
        project: StoredProject::synthetic(),
        project_defect,
        panel: remote_panel.then(|| FakePanel::with_pad_notes(pad_notes)),
    });
    transport.set_responder(Box::new(move |packet| {
        let mut state = state.lock().unwrap_or_else(PoisonError::into_inner);
        respond(&mut state, packet)
    }));
    transport
}

/// The fake's current beat, as the real machine sends a `5F`: all its sounds, the flushed last group. Its sequence is
/// real (four measures of 4/4 at 120 BPM, swing 3, 24 events on pads 1-4) so that it exports as a MIDI file.
fn fake_beat_dump() -> Vec<u8> {
    let sounds: [Sound; PADS] = std::array::from_fn(|pad| project_sound(0, pad));
    let assigns = [(0, 1), (1, 2), (2, 3), (3, 4)];
    let events: Vec<BeatEvent> = (0..24u16)
        .map(|k| BeatEvent {
            step: k * 8,
            pad: (k % 4) as u8,
            velocity: 100,
            duration: 3,
            time_shift: 7,
            half_step: false,
        })
        .collect();
    beat::machine_dump(&synthetic_sequence_beat(
        "Fake beat 01",
        &assigns,
        1200,
        3,
        4,
        &events,
        &sounds,
    ))
}

fn respond(state: &mut State, packet: &[u8]) -> Vec<Vec<u8>> {
    let messages = state.parser.push_bytes(packet);
    let mut answers = Vec::new();
    for message in messages {
        match message {
            Message::SysEx(sysex) => answers.extend(answer(state, &sysex)),
            Message::Channel(short) => {
                let reply = state
                    .panel
                    .as_mut()
                    .map(|panel| panel.on_channel(short.status, short.data()))
                    .filter(|reply| !reply.is_empty());
                answers.extend(reply);
            }
            _ => {}
        }
    }
    answers
}

fn answer(state: &mut State, sysex: &[u8]) -> Vec<Vec<u8>> {
    if sysex == IDENTITY_REQUEST {
        return vec![IDENTITY_REPLY.to_vec()];
    }
    if sysex == VERSION_REQUEST {
        // Only the Tempester with the remote panel knows the request; the others ignore it, as a machine without n°39.
        return state
            .panel
            .as_ref()
            .and_then(|_| ANNOUNCED.reply())
            .into_iter()
            .collect();
    }
    if sysex == REQUEST_SOUND {
        return vec![sound_message::encode_current(&state.sound)];
    }
    if sysex == REQUEST_BEAT {
        return vec![beat_with_probability(state)];
    }
    if sysex == REQUEST_PROJECT {
        return state.project.messages();
    }
    if let Ok(Some(received)) = project::decode_received(sysex) {
        let defect = state.project_defect.load(Ordering::SeqCst);
        state.project.keep(received, defect);
        return Vec::new();
    }
    if let Ok(SoundMessage::Current(sound)) = sound_message::decode(sysex) {
        state.sound = sound;
    } else if let Ok(summary) = beat::summarize(sysex) {
        state.beat = sysex.to_vec();
        if let Some(panel) = state.panel.as_mut() {
            panel.probability = summary.play_probability;
        }
    }
    Vec::new()
}

/// The current beat with the play probabilities CC 14 set (n°36 keeps them in the beat), in the machine's layout.
fn beat_with_probability(state: &State) -> Vec<u8> {
    let Some(panel) = &state.panel else {
        return state.beat.clone();
    };
    let message = &state.beat;
    beat::read_body(&message[4..message.len() - 1])
        .and_then(|data| beat::with_play_probability(&data, &panel.probability))
        .map(|data| beat::machine_dump(&data))
        .unwrap_or_else(|_| message.clone())
}

/// Moves of the machine's own controls, as Tempester 1.6.3.0 reports them on Remote Pad OUT (channel 2 here): FX ribbon
/// 1 touched at 96 then released (n°31), LP Freq beat-wide at 151 (n°29: MSB 75, LSB 0x40), then three mixer reports (n°38).
pub fn perform_moves(transport: &FakeTransport) {
    transport.push_incoming(&[
        0xB1, 0x6C, 96, 0xB1, 0x6E, 0, 0xB1, 0x16, 75, 0xB1, 0x36, 0x40,
    ]);
    // n°38: the selected pad's Level moved to 72, the tempo to 121.5 and the Distort knob to its upper half.
    for (item, value) in [
        (MixerItem::Level, 72),
        (MixerItem::Tempo, 1215),
        (MixerItem::Distort, 200),
    ] {
        if let Ok(bytes) = report_bytes(1, item, value) {
            transport.push_incoming(&bytes);
        }
    }
}

/// The sound of pad `pad` in beat `beat` of the synthetic project: eight kinds, three variants across the beats, so
/// that the project holds 24 different sounds out of 512 and a library import has duplicates to leave out.
fn project_sound(beat: usize, pad: usize) -> Sound {
    let mut sound = Sound::with_defaults();
    // Values inside the table maxima, so a failure here is a bug in the fake itself.
    let _ = sound.set(34, ((pad % 8) * 10 + beat % 3) as u16);
    let _ = sound.set_name(&format!("Fake {} v{}", pad % 8 + 1, beat % 3 + 1));
    sound
}

/// What the machine's menu Send Project sends: `5E`, then `5C 0` to `5C 15` (`refs/firmware-main.md` 9.6).
pub fn project_messages() -> Vec<Vec<u8>> {
    let mut messages = vec![project::header_message(1)];
    messages.extend((0..BEATS).map(|beat| {
        let sounds: [Sound; PADS] = std::array::from_fn(|pad| project_sound(beat, pad));
        let name = format!("Fake beat {:02}", beat + 1);
        let data = beat::synthetic_beat_data(&name, &[(0, 1), (1, 2), (2, 3), (3, 4)], 24, &sounds);
        project::beat_message(beat as u8, &data)
    }));
    messages
}

/// Beats (from 0) the fake machine leaves out of a partial project: a gap and a missing last beat.
pub const PARTIAL_ABSENT: [u8; 3] = [1, 4, 15];

/// Send Project as the machine sends it when some beats are not marked present: `project_messages` without the
/// beats of `absent`.
pub fn project_messages_without(absent: &[u8]) -> Vec<Vec<u8>> {
    project_messages()
        .into_iter()
        .enumerate()
        .filter(|(at, _)| *at == 0 || !absent.contains(&(*at as u8 - 1)))
        .map(|(_, message)| message)
        .collect()
}

/// Plays a Send Project with absent beats (`PARTIAL_ABSENT`) to whoever listens on the fake transport.
pub fn project_send_partial(transport: &FakeTransport) {
    for message in project_messages_without(&PARTIAL_ABSENT) {
        transport.push_incoming(&message);
    }
}

/// Plays the menu's Send Project to whoever listens on the fake transport.
pub fn project_send(transport: &FakeTransport) {
    for message in project_messages() {
        transport.push_incoming(&message);
    }
}

/// A patterned image (never constant) for the chip.
fn patterned_image(id: OsId) -> Vec<u8> {
    (0..id.blocks() * BLOCK_SIZE)
        .map(|i| (i.wrapping_mul(7) ^ (i / BLOCK_SIZE)) as u8)
        .collect()
}

/// The three OS messages of the Debug menu as the machine would send them: Main and Panel real-looking, Voice
/// constant 0xFF.
pub fn debug_send_messages() -> Vec<Vec<u8>> {
    let voice = vec![0xFF; OsId::Voice.blocks() * BLOCK_SIZE];
    [
        (OsId::Main, patterned_image(OsId::Main)),
        (OsId::Voice, voice),
        (OsId::Panel, patterned_image(OsId::Panel)),
    ]
    .iter()
    .filter_map(|(id, image)| os_image::encode(*id, image).ok())
    .collect()
}

/// Plays the Debug-menu sends to whoever listens on the fake transport.
pub fn debug_send(transport: &FakeTransport) {
    for message in debug_send_messages() {
        transport.push_incoming(&message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempest_midi::Transport;

    fn ask(transport: &FakeTransport, request: &[u8]) -> Vec<u8> {
        let got = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&got);
        transport.set_receiver(Some(Box::new(move |bytes| {
            sink.lock().unwrap().extend_from_slice(bytes);
        })));
        transport.send(request).unwrap();
        let answer = got.lock().unwrap().clone();
        answer
    }

    #[test]
    fn test_scripted_when_identity_requested_returns_a_tempester_identity() {
        let reply = ask(&scripted(true), &IDENTITY_REQUEST);

        let identity = tempest_proto::identity::parse(&reply).unwrap();
        assert_eq!(identity.version, 0x7F2);
    }

    #[test]
    fn test_scripted_when_version_requested_returns_1_6_5_0_only_with_the_remote_panel() {
        let reply = ask(&scripted(true), &VERSION_REQUEST);
        let silent = ask(&scripted(false), &VERSION_REQUEST);

        assert_eq!(
            tempest_proto::identity::parse_version(&reply),
            Some(TempesterVersion([1, 6, 5, 0]))
        );
        assert!(silent.is_empty());
    }

    #[test]
    fn test_scripted_when_sound_requested_returns_the_fake_kick() {
        let reply = ask(&scripted(true), &REQUEST_SOUND);

        match sound_message::decode(&reply).unwrap() {
            SoundMessage::Current(sound) => {
                assert_eq!(sound.name(), "Fake Kick");
                assert_eq!(sound.get(34).unwrap(), 96);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn test_scripted_when_beat_requested_returns_a_valid_beat_dump() {
        let reply = ask(&scripted(true), &REQUEST_BEAT);

        let summary = beat::summarize(&reply).unwrap();
        assert_eq!(
            (summary.name.as_str(), summary.events),
            ("Fake beat 01", 24)
        );
    }

    #[test]
    fn test_scripted_when_sound_sent_in_two_packets_then_requested_returns_the_new_sound() {
        let transport = scripted(true);
        let mut sound = Sound::with_defaults();
        sound.set(34, 7).unwrap();
        let message = sound_message::encode_current(&sound);

        transport.send(&message[..100]).unwrap();
        transport.send(&message[100..]).unwrap();
        let reply = ask(&transport, &REQUEST_SOUND);

        assert!(matches!(
            sound_message::decode(&reply),
            Ok(SoundMessage::Current(got)) if got.get(34).unwrap() == 7
        ));
    }

    #[test]
    fn test_scripted_when_project_requested_returns_a_complete_checked_project() {
        let transport = scripted(true);
        let got = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&got);
        transport.set_receiver(Some(Box::new(move |bytes| {
            sink.lock().unwrap().extend_from_slice(bytes);
        })));

        transport.send(&REQUEST_PROJECT).unwrap();

        let heard = got.lock().unwrap().clone();
        let mut parser = StreamParser::new();
        let mut assembler = project::ProjectAssembler::new();
        for message in parser.push_bytes(&heard) {
            if let Message::SysEx(sysex) = message {
                assembler.accept(&sysex).unwrap();
            }
        }
        let project = assembler.finish().unwrap();
        assert_eq!(project.beats[15].dump.summary.name, "Fake beat 16");
    }

    #[test]
    fn test_project_messages_when_built_return_24_different_sounds_over_512_pads() {
        let mut assembler = project::ProjectAssembler::new();
        for message in project_messages() {
            assembler.accept(&message).unwrap();
        }
        let project = assembler.finish().unwrap();

        let distinct: std::collections::HashSet<_> = project
            .beats
            .iter()
            .flat_map(|b| b.dump.sounds.iter().flatten().map(|s| *s.bytes()))
            .collect();

        assert_eq!(distinct.len(), 24);
    }

    #[test]
    fn test_scripted_when_unknown_request_returns_silence() {
        assert!(ask(&scripted(true), &[0xF0, 0x01, 0x28, 0x5D, 0xF7]).is_empty());
    }

    #[test]
    fn test_debug_send_messages_when_decoded_returns_main_voice_panel_with_voice_constant() {
        let messages = debug_send_messages();

        let images: Vec<_> = messages
            .iter()
            .map(|m| os_image::decode(m).unwrap())
            .collect();

        let ids: Vec<OsId> = images.iter().map(|i| i.id).collect();
        assert_eq!(ids, vec![OsId::Main, OsId::Voice, OsId::Panel]);
        assert!(images[1].image.iter().all(|b| *b == 0xFF));
        assert!(!images[0].image.iter().all(|b| *b == images[0].image[0]));
    }
}
