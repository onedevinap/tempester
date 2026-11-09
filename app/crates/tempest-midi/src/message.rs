//! Splits a raw MIDI byte stream into messages: complete SysEx (even when the system delivers it in pieces),
//! channel messages with running status expanded, system common messages, and real-time bytes that may fall
//! anywhere, even inside a SysEx, without disturbing it.

/// A short message: status byte and 0-2 data bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShortMessage {
    pub status: u8,
    data: [u8; 2],
    len: u8,
}

impl ShortMessage {
    fn new(status: u8, data: &[u8]) -> Self {
        let mut bytes = [0u8; 2];
        bytes[..data.len()].copy_from_slice(data);
        Self {
            status,
            data: bytes,
            len: data.len() as u8,
        }
    }

    /// The data bytes (without the status).
    pub fn data(&self) -> &[u8] {
        &self.data[..usize::from(self.len)]
    }
}

/// One message of the stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    /// `F0 … F7`, complete.
    SysEx(Vec<u8>),
    /// Note, Control Change, etc. (status 0x80-0xEF, always explicit).
    Channel(ShortMessage),
    /// Time code, song position, song select, tune request (0xF1-0xF6).
    SystemCommon(ShortMessage),
    /// 0xF8-0xFF: clock, start, continue, stop, active sensing, reset.
    RealTime(u8),
}

impl Message {
    /// Clock (`F8`) and active sensing (`FE`), which a request ignores.
    pub fn is_noise(&self) -> bool {
        matches!(self, Message::RealTime(0xF8 | 0xFE))
    }
}

/// Stateful splitter; feed it the bytes in arrival order.
#[derive(Debug, Default)]
pub struct StreamParser {
    sysex: Option<Vec<u8>>,
    /// Status waiting for data bytes (channel running status stays, system common is consumed).
    status: Option<u8>,
    data: Vec<u8>,
}

impl StreamParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feeds a chunk, returns the messages it completed.
    pub fn push_bytes(&mut self, bytes: &[u8]) -> Vec<Message> {
        let mut out = Vec::new();
        for &byte in bytes {
            self.push(byte, &mut out);
        }
        out
    }

    fn push(&mut self, byte: u8, out: &mut Vec<Message>) {
        match byte {
            0xF8..=0xFF => out.push(Message::RealTime(byte)),
            0xF0 => self.start_sysex(),
            0xF7 => self.end_sysex(out),
            0x80..=0xFF => self.start_short(byte, out),
            _ => self.push_data(byte, out),
        }
    }

    fn start_sysex(&mut self) {
        self.status = None;
        self.data.clear();
        self.sysex = Some(vec![0xF0]);
    }

    fn end_sysex(&mut self, out: &mut Vec<Message>) {
        if let Some(mut message) = self.sysex.take() {
            message.push(0xF7);
            out.push(Message::SysEx(message));
        }
    }

    fn start_short(&mut self, status: u8, out: &mut Vec<Message>) {
        self.sysex = None; // any status byte aborts an unfinished SysEx
        self.data.clear();
        match status {
            0xF4 | 0xF5 => self.status = None, // undefined: no data, no message
            0xF6 => {
                self.status = None;
                out.push(Message::SystemCommon(ShortMessage::new(status, &[])));
            }
            _ => self.status = Some(status),
        }
    }

    fn push_data(&mut self, byte: u8, out: &mut Vec<Message>) {
        if let Some(sysex) = self.sysex.as_mut() {
            sysex.push(byte);
            return;
        }
        let Some(status) = self.status else { return };
        self.data.push(byte);
        if self.data.len() < data_len(status) {
            return;
        }
        let short = ShortMessage::new(status, &self.data);
        self.data.clear();
        if status >= 0xF0 {
            self.status = None;
            out.push(Message::SystemCommon(short));
        } else {
            out.push(Message::Channel(short));
        }
    }
}

fn data_len(status: u8) -> usize {
    match status {
        0xC0..=0xDF | 0xF1 | 0xF3 => 1,
        _ => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(bytes: &[u8]) -> Vec<Message> {
        StreamParser::new().push_bytes(bytes)
    }

    fn channel(status: u8, data: &[u8]) -> Message {
        Message::Channel(ShortMessage::new(status, data))
    }

    #[test]
    fn test_push_bytes_when_complete_sysex_returns_one_sysex_message() {
        let found = parse(&[0xF0, 0x01, 0x28, 0x60, 0x7F, 0xF7]);

        assert_eq!(
            found,
            vec![Message::SysEx(vec![0xF0, 1, 0x28, 0x60, 0x7F, 0xF7])]
        );
    }

    #[test]
    fn test_push_bytes_when_sysex_split_in_chunks_returns_one_message_at_the_end() {
        let mut parser = StreamParser::new();

        let first = parser.push_bytes(&[0xF0, 0x01, 0x28]);
        let second = parser.push_bytes(&[0x60, 0x11]);
        let third = parser.push_bytes(&[0x22, 0xF7]);

        assert!(first.is_empty() && second.is_empty());
        assert_eq!(
            third,
            vec![Message::SysEx(vec![0xF0, 1, 0x28, 0x60, 0x11, 0x22, 0xF7])]
        );
    }

    #[test]
    fn test_push_bytes_when_clock_inside_sysex_returns_sysex_without_it_and_the_clock() {
        let found = parse(&[0xF0, 0x01, 0xF8, 0x02, 0xFE, 0xF7]);

        assert_eq!(
            found,
            vec![
                Message::RealTime(0xF8),
                Message::RealTime(0xFE),
                Message::SysEx(vec![0xF0, 1, 2, 0xF7]),
            ]
        );
    }

    #[test]
    fn test_push_bytes_when_running_status_returns_explicit_status_each_time() {
        let found = parse(&[0x90, 60, 100, 62, 90, 0xB0, 99, 1, 98, 2]);

        assert_eq!(
            found,
            vec![
                channel(0x90, &[60, 100]),
                channel(0x90, &[62, 90]),
                channel(0xB0, &[99, 1]),
                channel(0xB0, &[98, 2]),
            ]
        );
    }

    #[test]
    fn test_push_bytes_when_clock_between_data_bytes_returns_message_unbroken() {
        let found = parse(&[0x90, 60, 0xF8, 100]);

        assert_eq!(
            found,
            vec![Message::RealTime(0xF8), channel(0x90, &[60, 100])]
        );
    }

    #[test]
    fn test_push_bytes_when_program_change_returns_one_data_byte() {
        let found = parse(&[0xC2, 5, 6]);

        assert_eq!(found, vec![channel(0xC2, &[5]), channel(0xC2, &[6])]);
    }

    #[test]
    fn test_push_bytes_when_status_aborts_sysex_returns_no_sysex() {
        let found = parse(&[0xF0, 1, 2, 0x90, 60, 100, 0xF7]);

        assert_eq!(found, vec![channel(0x90, &[60, 100])]);
    }

    #[test]
    fn test_push_bytes_when_sysex_clears_running_status_returns_stray_data_dropped() {
        let found = parse(&[0x90, 60, 100, 0xF0, 1, 0xF7, 62, 90]);

        assert_eq!(
            found,
            vec![
                channel(0x90, &[60, 100]),
                Message::SysEx(vec![0xF0, 1, 0xF7])
            ]
        );
    }

    #[test]
    fn test_push_bytes_when_system_common_returns_message_and_no_running_status() {
        let found = parse(&[0xF2, 1, 2, 3, 4, 0xF6, 0xF1, 7]);

        assert_eq!(
            found,
            vec![
                Message::SystemCommon(ShortMessage::new(0xF2, &[1, 2])),
                Message::SystemCommon(ShortMessage::new(0xF6, &[])),
                Message::SystemCommon(ShortMessage::new(0xF1, &[7])),
            ]
        );
    }

    #[test]
    fn test_push_bytes_when_stray_eox_or_data_returns_nothing() {
        assert!(parse(&[0xF7, 5, 6]).is_empty());
    }

    #[test]
    fn test_push_bytes_when_two_sysex_back_to_back_returns_both() {
        let found = parse(&[0xF0, 1, 0xF7, 0xF0, 2, 0xF7]);

        assert_eq!(found.len(), 2);
    }

    #[test]
    fn test_is_noise_when_clock_or_active_sensing_returns_true_only_for_them() {
        assert!(Message::RealTime(0xF8).is_noise());
        assert!(Message::RealTime(0xFE).is_noise());
        assert!(!Message::RealTime(0xFA).is_noise());
        assert!(!channel(0x90, &[1, 2]).is_noise());
    }
}
