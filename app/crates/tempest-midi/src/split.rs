//! One `send` of the app may hold several MIDI messages (the four Control Changes of an NRPN, a note then a clock).
//! CoreMIDI takes such a packet as it is. midir 0.11's WinMM and ALSA backends take one message per call: WinMM
//! refuses a packet over 3 bytes that does not start with `F0` (`backend/winmm/mod.rs`, `MidiOutputConnection::send`),
//! ALSA sends the first event of the packet and drops the rest (`backend/alsa/mod.rs`, same function). There the packet
//! is cut into its messages, in order.

const SYSEX_START: u8 = 0xF0;
const SYSEX_END: u8 = 0xF7;

/// The packets to hand to the MIDI system for one `send` of `bytes`: `bytes` itself where the system takes several
/// messages per call, else each message in order (a whole `F0 … F7` SysEx is one message).
///
/// From the first byte that does not start a whole message (a data byte without its status, as running status
/// would leave; a message cut short or holding a status byte among its data; a SysEx without its `F7`; the
/// undefined `F4` / `F5`; a lone `F7`), the rest of the packet goes unchanged as one last packet: the backend's own
/// handling or error then applies, and nothing is dropped or invented. An empty packet is passed as it is.
pub(crate) fn packets_for(bytes: &[u8], one_message_per_call: bool) -> Vec<&[u8]> {
    if !one_message_per_call || bytes.is_empty() {
        return vec![bytes];
    }
    let mut packets = Vec::new();
    let mut rest = bytes;
    while !rest.is_empty() {
        let Some(len) = message_len(rest) else {
            packets.push(rest);
            break;
        };
        let (message, after) = rest.split_at(len);
        packets.push(message);
        rest = after;
    }
    packets
}

/// Length of the whole message at the start of `bytes`, or `None` when `bytes` does not start one.
fn message_len(bytes: &[u8]) -> Option<usize> {
    let len = match *bytes.first()? {
        SYSEX_START => return sysex_len(bytes),
        0x80..=0xBF | 0xE0..=0xEF | 0xF2 => 3,
        0xC0..=0xDF | 0xF1 | 0xF3 => 2,
        0xF6 | 0xF8..=0xFF => 1,
        _ => return None,
    };
    let data = bytes.get(1..len)?;
    data.iter().all(|&byte| byte < 0x80).then_some(len)
}

/// Length of the SysEx at the start of `bytes` when its first status byte after `F0` is its `F7`.
fn sysex_len(bytes: &[u8]) -> Option<usize> {
    let end = 1 + bytes.get(1..)?.iter().position(|&byte| byte >= 0x80)?;
    (bytes[end] == SYSEX_END).then_some(end + 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::MidiError;
    use crate::fake::FakeTransport;
    use crate::transport::Transport;

    /// The four Control Changes of an NRPN on channel 1, as `tempest_proto::nrpn::frame` lays them out: number
    /// 99/98, then value 6/38.
    const NRPN: [u8; 12] = [
        0xB0, 99, 0x01, 0xB0, 98, 0x05, 0xB0, 6, 0x00, 0xB0, 38, 0x40,
    ];

    fn split(bytes: &[u8]) -> Vec<Vec<u8>> {
        packets_for(bytes, true)
            .into_iter()
            .map(<[u8]>::to_vec)
            .collect()
    }

    #[test]
    fn test_packets_for_when_several_messages_per_call_returns_the_packet_untouched() {
        assert_eq!(packets_for(&NRPN, false), vec![&NRPN[..]]);
    }

    #[test]
    fn test_packets_for_when_nrpn_frame_returns_four_control_changes() {
        assert_eq!(
            split(&NRPN),
            vec![
                vec![0xB0, 99, 0x01],
                vec![0xB0, 98, 0x05],
                vec![0xB0, 6, 0x00],
                vec![0xB0, 38, 0x40],
            ]
        );
    }

    #[test]
    fn test_packets_for_when_mixed_packet_returns_each_message_in_order() {
        let packet = [
            0x90, 36, 100, 0xF8, 0xF0, 0x01, 0x28, 0x40, 0xF7, 0xC0, 5, 0xE0, 0, 64,
        ];

        assert_eq!(
            split(&packet),
            vec![
                vec![0x90, 36, 100],
                vec![0xF8],
                vec![0xF0, 0x01, 0x28, 0x40, 0xF7],
                vec![0xC0, 5],
                vec![0xE0, 0, 64],
            ]
        );
    }

    #[test]
    fn test_packets_for_when_system_common_messages_returns_their_lengths() {
        let packet = [0xF1, 0x10, 0xF2, 0x01, 0x02, 0xF3, 0x03, 0xF6, 0xD0, 0x7F];

        assert_eq!(
            split(&packet),
            vec![
                vec![0xF1, 0x10],
                vec![0xF2, 0x01, 0x02],
                vec![0xF3, 0x03],
                vec![0xF6],
                vec![0xD0, 0x7F],
            ]
        );
    }

    /// `send_paced` on a whole-message transport hands over whole SysEx messages: each must stay one packet.
    #[test]
    fn test_packets_for_when_one_large_sysex_returns_it_as_one_packet() {
        let mut sysex = vec![0x55; 600_000];
        sysex[0] = 0xF0;
        sysex[599_999] = 0xF7;

        assert_eq!(packets_for(&sysex, true), vec![&sysex[..]]);
    }

    #[test]
    fn test_packets_for_when_empty_returns_the_empty_packet_as_is() {
        assert_eq!(packets_for(&[], true), vec![&[][..]]);
    }

    #[test]
    fn test_packets_for_when_malformed_returns_the_rest_unchanged_as_one_last_packet() {
        let cases: [(&[u8], usize); 8] = [
            (&[0x90, 36, 100, 37, 100], 3),          // running status
            (&[0xB0, 99], 0),                        // cut short
            (&[0x90, 36, 0xF8, 100], 0),             // status among the data
            (&[0xB0, 99, 1, 0xF0, 0x01, 0x02], 3),   // SysEx without F7
            (&[0xF0, 0x01, 0x90, 36, 100, 0xF7], 0), // SysEx broken by a status
            (&[0xF4, 0x90, 36, 100], 0),             // undefined system common
            (&[0xF7, 0x90, 36, 100], 0),             // lone F7
            (&[0x40, 0x41, 0x42, 0x43], 0),          // data bytes only
        ];

        for (packet, whole) in cases {
            let mut expected: Vec<Vec<u8>> =
                packet[..whole].chunks(3).map(<[u8]>::to_vec).collect();
            expected.push(packet[whole..].to_vec());

            assert_eq!(split(packet), expected, "{packet:02X?}");
            assert_eq!(split(packet).concat(), packet, "nothing dropped or added");
        }
    }

    /// The reported defect on a port like WinMM: the NRPN in one packet is refused; cut into its messages, it goes.
    #[test]
    fn test_send_when_fragments_refused_returns_error_for_the_whole_frame_and_ok_for_its_messages()
    {
        let fake = FakeTransport::new();
        fake.refuse_sysex_fragments();

        let whole = fake.send(&NRPN);
        let split = packets_for(&NRPN, true)
            .into_iter()
            .try_for_each(|packet| fake.send(packet));

        assert!(matches!(whole, Err(MidiError::Send(_))));
        assert_eq!(split, Ok(()));
        assert_eq!(fake.sent().len(), 4);
        assert_eq!(fake.sent_bytes(), NRPN);
    }
}
