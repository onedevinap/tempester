//! Commands the page sends to Rust: one line, `name<TAB>arg<TAB>arg...`. Binary data (files) travels as hex.
//! The page strips tabs and line breaks out of the text it sends, so a tab is always a separator.

use std::fmt;

use tempest_proto::pad_notes::PAD_COUNT;
use tempest_proto::{performance, remote};

use crate::edit::command::EditCommand;
use crate::hex;
use crate::library::command::LibCommand;
use crate::machine::sequencer::{PadPattern, MAX_STEPS, PADS};
use crate::storage::settings::{PANEL_ZOOM_MAX, PANEL_ZOOM_MIN};

/// A library folder of `.syx` files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Sounds,
    Beats,
    Projects,
}

/// A folder the user may ask to see in the file manager (never an arbitrary path).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    Data,
    Sounds,
    Beats,
    Projects,
    Backups,
    Os,
    Exports,
}

/// Everything the page can ask for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    State,
    Ports,
    /// `None` = the port named "Tempest".
    Connect(Option<String>),
    Disconnect,
    Identity,
    /// Asks a Tempester for its exact version (n°39, `F0 01 28 43 F7`); the answer comes back as incoming SysEx.
    TempesterVersion,
    /// MIDI channel 1-16 used for NRPN, notes and clock.
    SetChannel(u8),
    Nrpn {
        number: u16,
        value: u16,
    },
    /// A control change of the allow-list (`CC_ALLOWED`) on the chosen channel.
    Cc {
        controller: u8,
        value: u8,
    },
    /// A key of the remote panel (Tempester 1.6.0.0): press (`down`) or release, key code 0-48.
    Key {
        down: bool,
        code: u8,
    },
    /// Absolute control 0-9 of the remote panel (NRPN 220-229), value 0-255.
    Control {
        index: u8,
        value: u16,
    },
    /// Encoder 0-13 of the remote panel turned by `step` detents (NRPN 200-213, relative, -16 to 15).
    Encoder {
        index: u8,
        step: i8,
    },
    /// Beat-wide parameter 0-7 set to position 0-254 (n°29, Tempester 1.6.2.0: CC 20-27, LSB 52-59).
    BeatWide {
        index: u8,
        position: u8,
    },
    /// FX ribbon 0-1 touched at `position` (0-127, with an optional pressure 0-127), or released (`None`) (n°31,
    /// Tempester 1.6.3.0: CC 108-113).
    Ribbon {
        ribbon: u8,
        position: Option<u8>,
        pressure: Option<u8>,
    },
    /// Play probability 0-100 % of pad 0-15 (n°36, Tempester 1.6.3.0): the pad is pressed (its note) so that the
    /// machine selects it, then CC 14 sets the probability of the selected pad.
    PadProbability {
        pad: u8,
        percent: u8,
    },
    /// Fake MIDI only: the scripted machine plays moves of its own controls (a ribbon touched then released, a
    /// beat-wide knob), as reports of n°29 and n°31.
    FakePerform,
    /// The view of the Panel page to open on next time: `true` = Full, `false` = Machine. Kept in the settings file.
    SetPanelView {
        full: bool,
    },
    /// The zoom of the Panel page in percent of the fit-to-width scale (`PANEL_ZOOM_MIN..=PANEL_ZOOM_MAX`). Kept in the
    /// settings file with the view.
    SetPanelZoom {
        percent: u16,
    },
    /// Asks the machine for the state of every LED (pad LEDs included or not); also probes the remote panel.
    PanelState {
        pads: bool,
    },
    /// MIDI Start (`true`) or Stop (`false`) for a Tempest set to slave its clock.
    Transport {
        start: bool,
    },
    FetchSound,
    SaveSound(String),
    /// A script error of the page (message, file, line), reported for diagnosis: written to stderr, never sent on.
    PageError(String),
    OpenSound {
        name: String,
        data: Vec<u8>,
    },
    SendSound,
    FetchBeat,
    SaveBeat(String),
    OpenBeat {
        name: String,
        data: Vec<u8>,
    },
    SendBeat,
    Library(Kind),
    OpenLibrary {
        kind: Kind,
        name: String,
    },
    /// A library manager command (`lib_*`): see `library::command`.
    Lib(LibCommand),
    /// An editing tool of the Panel page (`edit_*`, `learn_*`): see `edit::command`.
    Edit(EditCommand),
    BackupStart,
    BackupStop,
    BackupSave,
    OsVersions,
    OsBuild {
        version: String,
        data: Vec<u8>,
    },
    OsValidate {
        name: String,
        data: Vec<u8>,
    },
    OsOriginal {
        data: Vec<u8>,
    },
    /// Sends the ready file. Refused when the file is not a known build (see `OsSendUnknown`).
    OsSend,
    /// Sends the ready file after the user's extra confirmation for a build that matches no known version.
    OsSendUnknown,
    OsCancel,
    /// Listens for the Debug-menu "Send Main OS" dump and compares its image with the last installed OS.
    OsVerifyStart,
    OsVerifyStop,
    Note {
        on: bool,
        note: u8,
        velocity: u8,
    },
    /// Pad 0-31 (A1-A16, B1-B16) pressed or released: its note in the machine's pad note table (`pad_map`).
    Pad {
        on: bool,
        pad: u8,
        velocity: u8,
    },
    /// The page asks for the pad note table (`pad_notes` event).
    PadNotes,
    /// Pad 0-31 gets note 0-127 (refused when another pad has it), kept in the settings file.
    SetPadNote {
        pad: u8,
        note: u8,
    },
    /// The pad note table back to its default (the author's machine).
    ResetPadNotes,
    ClockStart(u16),
    ClockBpm(u16),
    ClockStop,
    /// The Euclidean sequencer's pattern for pad 0-15 (validated: see `PadPattern::new`).
    SeqPad {
        pad: u8,
        pattern: PadPattern,
    },
    /// Sequencer on or off, and the velocity of its notes (1-127).
    SeqRun {
        enabled: bool,
        velocity: u8,
    },
    /// Asks for the sequencer's whole state (the page does when it is built).
    SeqState,
    Reveal(Place),
    /// Fake MIDI only: the scripted machine plays its Debug-menu OS sends.
    FakeDebugSend,
    /// Listens for the menu's Send Project (`5E` + 16 x `5C`) and saves it.
    ReceiveProject,
    /// Asks the machine for the project (`42`, Tempester 1.6.1.0 or later), then receives it like `ReceiveProject`.
    FetchProject,
    /// Stops listening for a project.
    ProjectCancel,
    /// Fake MIDI only: the scripted machine plays its menu's Send Project.
    FakeProjectSend,
    /// Fake MIDI only: the same with some beats not sent (as when their presence flags are clear).
    FakeProjectSendPartial,
    /// Fake MIDI only: the scripted machine reads a received project's Play Lists as the stock OS does (n°28 defect).
    FakeProjectDefect(bool),
    /// The MIDI monitor of the Machine page: collect and report every message (`true`) or nothing at all.
    Monitor(bool),
    /// Whether the monitor lists the clock and active-sensing bytes (they are always counted).
    MonitorClock(bool),
    /// Sent by the window when it closes; never parsed from the page.
    Shutdown,
}

/// The only control changes the Tempest OS reads (`refs/firmware-main.md` §6.2): Distortion, Compression, revert and
/// the beat-wide parameters. CC 7 (Volume) is stored but never read; everything else is ignored by the machine.
pub const CC_ALLOWED: [u8; 11] = [12, 13, 19, 20, 21, 22, 23, 24, 25, 26, 27];

/// Why a line is not a command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandError {
    Empty,
    Unknown(String),
    Arguments { command: String, expected: usize },
    BadArgument { command: String, why: String },
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "empty command"),
            Self::Unknown(name) => write!(f, "unknown command \"{name}\""),
            Self::Arguments { command, expected } => {
                write!(f, "command \"{command}\" takes {expected} argument(s)")
            }
            Self::BadArgument { command, why } => write!(f, "command \"{command}\": {why}"),
        }
    }
}

impl std::error::Error for CommandError {}

/// Parses one line sent by the page.
pub fn parse(line: &str) -> Result<Command, CommandError> {
    let mut parts = line.split('\t');
    let name = parts.next().unwrap_or_default();
    if name.is_empty() {
        return Err(CommandError::Empty);
    }
    let args: Vec<&str> = parts.collect();
    let command = Args { name, args: &args };
    command.build()
}

struct Args<'a> {
    name: &'a str,
    args: &'a [&'a str],
}

impl Args<'_> {
    fn build(&self) -> Result<Command, CommandError> {
        match self.name {
            "state"
            | "ports"
            | "disconnect"
            | "identity"
            | "tempester_version"
            | "fetch_sound"
            | "send_sound"
            | "fetch_beat"
            | "send_beat"
            | "backup_start"
            | "backup_stop"
            | "backup_save"
            | "os_versions"
            | "os_send"
            | "os_send_unknown"
            | "os_cancel"
            | "os_verify_start"
            | "os_verify_stop"
            | "clock_stop"
            | "seq_state"
            | "pad_notes"
            | "pad_notes_reset"
            | "fake_debug_send"
            | "receive_project"
            | "fetch_project"
            | "project_cancel"
            | "fake_project_send"
            | "fake_project_send_partial"
            | "fake_perform" => self.simple(),
            "connect"
            | "set_channel"
            | "nrpn"
            | "cc"
            | "note"
            | "pad"
            | "pad_note"
            | "clock_start"
            | "clock_bpm"
            | "seq_pad"
            | "seq_run"
            | "key"
            | "control"
            | "encoder"
            | "panel_zoom"
            | "panel_state"
            | "beat_wide"
            | "ribbon"
            | "pad_probability"
            | "fake_project_defect" => self.with_numbers(),
            name if name.starts_with("lib_") => {
                crate::library::command::parse(name, self.args).map(Command::Lib)
            }
            name if name.starts_with("edit_") || name.starts_with("learn_") => {
                crate::edit::command::parse(name, self.args).map(Command::Edit)
            }
            _ => self.with_text(),
        }
    }

    fn simple(&self) -> Result<Command, CommandError> {
        self.expect(0)?;
        Ok(match self.name {
            "state" => Command::State,
            "ports" => Command::Ports,
            "disconnect" => Command::Disconnect,
            "identity" => Command::Identity,
            "tempester_version" => Command::TempesterVersion,
            "fetch_sound" => Command::FetchSound,
            "send_sound" => Command::SendSound,
            "fetch_beat" => Command::FetchBeat,
            "send_beat" => Command::SendBeat,
            "backup_start" => Command::BackupStart,
            "backup_stop" => Command::BackupStop,
            "backup_save" => Command::BackupSave,
            "os_versions" => Command::OsVersions,
            "os_send" => Command::OsSend,
            "os_send_unknown" => Command::OsSendUnknown,
            "os_cancel" => Command::OsCancel,
            "os_verify_start" => Command::OsVerifyStart,
            "os_verify_stop" => Command::OsVerifyStop,
            "clock_stop" => Command::ClockStop,
            "seq_state" => Command::SeqState,
            "pad_notes" => Command::PadNotes,
            "pad_notes_reset" => Command::ResetPadNotes,
            "fake_debug_send" => Command::FakeDebugSend,
            "receive_project" => Command::ReceiveProject,
            "fetch_project" => Command::FetchProject,
            "project_cancel" => Command::ProjectCancel,
            "fake_project_send" => Command::FakeProjectSend,
            "fake_project_send_partial" => Command::FakeProjectSendPartial,
            "fake_perform" => Command::FakePerform,
            _ => return Err(self.unknown()),
        })
    }

    fn with_numbers(&self) -> Result<Command, CommandError> {
        match self.name {
            "connect" => self.connect(),
            "set_channel" => {
                self.expect(1)?;
                let channel = self.number(0, 1, 16)?;
                Ok(Command::SetChannel(channel as u8))
            }
            "nrpn" => {
                self.expect(2)?;
                let number = self.number(0, 0, 16383)? as u16;
                Ok(Command::Nrpn {
                    number,
                    value: self.number(1, 0, 16383)? as u16,
                })
            }
            "fake_project_defect" => {
                self.expect(1)?;
                Ok(Command::FakeProjectDefect(self.number(0, 0, 1)? == 1))
            }
            "cc" => self.cc(),
            "key" => self.key(),
            "control" => self.control(),
            "encoder" => self.encoder(),
            "panel_zoom" => self.panel_zoom(),
            "panel_state" => self.panel_state(),
            "beat_wide" => self.beat_wide(),
            "ribbon" => self.ribbon(),
            "pad_probability" => self.pad_probability(),
            "note" => self.note(),
            "pad" => self.pad(),
            "pad_note" => {
                self.expect(2)?;
                Ok(Command::SetPadNote {
                    pad: self.number(0, 0, PAD_COUNT as u32 - 1)? as u8,
                    note: self.number(1, 0, 127)? as u8,
                })
            }
            "clock_start" | "clock_bpm" => self.clock(),
            "seq_pad" => self.seq_pad(),
            "seq_run" => self.seq_run(),
            _ => Err(self.unknown()),
        }
    }

    fn connect(&self) -> Result<Command, CommandError> {
        self.expect(1)?;
        let port = self.args[0];
        Ok(Command::Connect(
            (!port.is_empty()).then(|| port.to_string()),
        ))
    }

    fn cc(&self) -> Result<Command, CommandError> {
        self.expect(2)?;
        let controller = self.number(0, 0, 127)? as u8;
        if !CC_ALLOWED.contains(&controller) {
            return Err(self.bad(format!(
                "CC {controller} is not read by the Tempest (allowed: 12, 13, 19, 20-27)"
            )));
        }
        Ok(Command::Cc {
            controller,
            value: self.number(1, 0, 127)? as u8,
        })
    }

    fn key(&self) -> Result<Command, CommandError> {
        self.expect(2)?;
        let down = match self.args[0] {
            "press" => true,
            "release" => false,
            other => return Err(self.bad(format!("\"{other}\" is neither press nor release"))),
        };
        let last = u32::from(remote::KEY_COUNT - 1);
        Ok(Command::Key {
            down,
            code: self.number(1, 0, last)? as u8,
        })
    }

    fn control(&self) -> Result<Command, CommandError> {
        self.expect(2)?;
        let last = u32::from(remote::CONTROL_COUNT - 1);
        Ok(Command::Control {
            index: self.number(0, 0, last)? as u8,
            value: self.number(1, 0, u32::from(remote::CONTROL_MAX_VALUE))? as u16,
        })
    }

    fn encoder(&self) -> Result<Command, CommandError> {
        self.expect(2)?;
        let text = self.args[1];
        let step = text
            .parse::<i8>()
            .ok()
            .filter(|step| (remote::ENCODER_MIN_STEP..=remote::ENCODER_MAX_STEP).contains(step))
            .ok_or_else(|| {
                self.bad(format!(
                    "\"{text}\" is not a step from {} to {}",
                    remote::ENCODER_MIN_STEP,
                    remote::ENCODER_MAX_STEP
                ))
            })?;
        let last = u32::from(remote::ENCODER_COUNT - 1);
        Ok(Command::Encoder {
            index: self.number(0, 0, last)? as u8,
            step,
        })
    }

    fn beat_wide(&self) -> Result<Command, CommandError> {
        self.expect(2)?;
        Ok(Command::BeatWide {
            index: self.number(0, 0, u32::from(performance::BEAT_WIDE_COUNT) - 1)? as u8,
            position: self.number(1, 0, u32::from(performance::BEAT_WIDE_MAX))? as u8,
        })
    }

    /// `ribbon r release`, `ribbon r touch p` or `ribbon r touch p pressure`.
    fn ribbon(&self) -> Result<Command, CommandError> {
        let last = u32::from(performance::RIBBON_COUNT) - 1;
        let max = u32::from(performance::RIBBON_MAX);
        match self.args.get(1).copied() {
            Some("release") => {
                self.expect(2)?;
                Ok(Command::Ribbon {
                    ribbon: self.number(0, 0, last)? as u8,
                    position: None,
                    pressure: None,
                })
            }
            Some("touch") => {
                if !(3..=4).contains(&self.args.len()) {
                    return Err(self.bad("touch takes a position and an optional pressure".into()));
                }
                let pressure = if self.args.len() == 4 {
                    Some(self.number(3, 0, max)? as u8)
                } else {
                    None
                };
                Ok(Command::Ribbon {
                    ribbon: self.number(0, 0, last)? as u8,
                    position: Some(self.number(2, 0, max)? as u8),
                    pressure,
                })
            }
            other => Err(self.bad(format!(
                "\"{}\" is neither touch nor release",
                other.unwrap_or_default()
            ))),
        }
    }

    fn pad_probability(&self) -> Result<Command, CommandError> {
        self.expect(2)?;
        Ok(Command::PadProbability {
            pad: self.number(0, 0, 15)? as u8,
            percent: self.number(1, 0, u32::from(performance::PROBABILITY_MAX))? as u8,
        })
    }

    fn panel_zoom(&self) -> Result<Command, CommandError> {
        self.expect(1)?;
        Ok(Command::SetPanelZoom {
            percent: self.number(0, PANEL_ZOOM_MIN, PANEL_ZOOM_MAX)? as u16,
        })
    }

    fn panel_view(&self) -> Result<Command, CommandError> {
        self.expect(1)?;
        match self.args[0] {
            "full" => Ok(Command::SetPanelView { full: true }),
            "machine" => Ok(Command::SetPanelView { full: false }),
            other => Err(self.bad(format!("\"{other}\" is neither full nor machine"))),
        }
    }

    fn panel_state(&self) -> Result<Command, CommandError> {
        self.expect(1)?;
        Ok(Command::PanelState {
            pads: self.number(0, 0, 1)? == 1,
        })
    }

    fn transport(&self) -> Result<Command, CommandError> {
        self.expect(1)?;
        match self.args[0] {
            "start" => Ok(Command::Transport { start: true }),
            "stop" => Ok(Command::Transport { start: false }),
            other => Err(self.bad(format!("\"{other}\" is neither start nor stop"))),
        }
    }

    fn switch(&self) -> Result<Command, CommandError> {
        self.expect(1)?;
        let on = match self.args[0] {
            "on" => true,
            "off" => false,
            other => return Err(self.bad(format!("\"{other}\" is neither on nor off"))),
        };
        Ok(if self.name == "monitor" {
            Command::Monitor(on)
        } else {
            Command::MonitorClock(on)
        })
    }

    fn note(&self) -> Result<Command, CommandError> {
        self.expect(3)?;
        Ok(Command::Note {
            on: self.on_off(0)?,
            note: self.number(1, 0, 127)? as u8,
            velocity: self.number(2, 0, 127)? as u8,
        })
    }

    fn pad(&self) -> Result<Command, CommandError> {
        self.expect(3)?;
        Ok(Command::Pad {
            on: self.on_off(0)?,
            pad: self.number(1, 0, PAD_COUNT as u32 - 1)? as u8,
            velocity: self.number(2, 0, 127)? as u8,
        })
    }

    fn on_off(&self, at: usize) -> Result<bool, CommandError> {
        match self.args[at] {
            "on" => Ok(true),
            "off" => Ok(false),
            other => Err(self.bad(format!("\"{other}\" is neither on nor off"))),
        }
    }

    fn seq_pad(&self) -> Result<Command, CommandError> {
        self.expect(5)?;
        let pad = self.number(0, 0, PADS as u32 - 1)? as u8;
        let pattern = PadPattern::new(
            self.number(1, 1, u32::from(MAX_STEPS))? as u8,
            self.number(2, 0, u32::from(MAX_STEPS))? as u8,
            self.number(3, 0, u32::from(MAX_STEPS) - 1)? as u8,
            self.number(4, 0, 100)? as u8,
        )
        .map_err(|why| self.bad(why))?;
        Ok(Command::SeqPad { pad, pattern })
    }

    fn seq_run(&self) -> Result<Command, CommandError> {
        self.expect(2)?;
        Ok(Command::SeqRun {
            enabled: self.number(0, 0, 1)? == 1,
            velocity: self.number(1, 1, 127)? as u8,
        })
    }

    fn clock(&self) -> Result<Command, CommandError> {
        self.expect(1)?;
        let bpm = self.number(0, 20, 300)? as u16;
        Ok(if self.name == "clock_start" {
            Command::ClockStart(bpm)
        } else {
            Command::ClockBpm(bpm)
        })
    }

    fn with_text(&self) -> Result<Command, CommandError> {
        match self.name {
            "save_sound" => self.one_text().map(Command::SaveSound),
            "page_error" => self.one_text().map(Command::PageError),
            "save_beat" => self.one_text().map(Command::SaveBeat),
            "library" => self.kind(0).map(Command::Library),
            "open_library" => self.open_library(),
            "reveal" => self.reveal(),
            "transport" => self.transport(),
            "panel_view" => self.panel_view(),
            "monitor" | "monitor_clock" => self.switch(),
            "open_sound" => self
                .named_data()
                .map(|(name, data)| Command::OpenSound { name, data }),
            "open_beat" => self
                .named_data()
                .map(|(name, data)| Command::OpenBeat { name, data }),
            "os_validate" => self
                .named_data()
                .map(|(name, data)| Command::OsValidate { name, data }),
            "os_build" => self
                .named_data()
                .map(|(version, data)| Command::OsBuild { version, data }),
            "os_original" => self.data_only().map(|data| Command::OsOriginal { data }),
            _ => Err(self.unknown()),
        }
    }

    fn one_text(&self) -> Result<String, CommandError> {
        self.expect(1)?;
        Ok(self.args[0].to_string())
    }

    fn named_data(&self) -> Result<(String, Vec<u8>), CommandError> {
        self.expect(2)?;
        let data = hex::decode(self.args[1]).map_err(|e| self.bad(e.to_string()))?;
        Ok((self.args[0].to_string(), data))
    }

    fn data_only(&self) -> Result<Vec<u8>, CommandError> {
        self.expect(1)?;
        hex::decode(self.args[0]).map_err(|e| self.bad(e.to_string()))
    }

    fn open_library(&self) -> Result<Command, CommandError> {
        self.expect(2)?;
        Ok(Command::OpenLibrary {
            kind: self.kind(0)?,
            name: self.args[1].to_string(),
        })
    }

    fn reveal(&self) -> Result<Command, CommandError> {
        self.expect(1)?;
        let place = match self.args[0] {
            "data" => Place::Data,
            "sounds" => Place::Sounds,
            "beats" => Place::Beats,
            "projects" => Place::Projects,
            "backups" => Place::Backups,
            "os" => Place::Os,
            "exports" => Place::Exports,
            other => return Err(self.bad(format!("unknown folder \"{other}\""))),
        };
        Ok(Command::Reveal(place))
    }

    fn kind(&self, at: usize) -> Result<Kind, CommandError> {
        match self.args.get(at).copied() {
            Some("sounds") => Ok(Kind::Sounds),
            Some("beats") => Ok(Kind::Beats),
            Some("projects") => Ok(Kind::Projects),
            _ => Err(self.bad("expected sounds, beats or projects".into())),
        }
    }

    fn expect(&self, count: usize) -> Result<(), CommandError> {
        if self.args.len() == count {
            Ok(())
        } else {
            Err(CommandError::Arguments {
                command: self.name.to_string(),
                expected: count,
            })
        }
    }

    fn number(&self, at: usize, min: u32, max: u32) -> Result<u32, CommandError> {
        let text = self.args[at];
        match text.parse::<u32>() {
            Ok(n) if (min..=max).contains(&n) => Ok(n),
            _ => Err(self.bad(format!("\"{text}\" is not a number from {min} to {max}"))),
        }
    }

    fn bad(&self, why: String) -> CommandError {
        CommandError::BadArgument {
            command: self.name.to_string(),
            why,
        }
    }

    fn unknown(&self) -> CommandError {
        CommandError::Unknown(self.name.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_when_monitor_switches_returns_on_and_off_for_both_commands() {
        assert_eq!(parse("monitor\ton"), Ok(Command::Monitor(true)));
        assert_eq!(parse("monitor\toff"), Ok(Command::Monitor(false)));
        assert_eq!(parse("monitor_clock\ton"), Ok(Command::MonitorClock(true)));
        assert_eq!(
            parse("monitor_clock\toff"),
            Ok(Command::MonitorClock(false))
        );
    }

    #[test]
    fn test_parse_when_monitor_argument_is_wrong_returns_an_error() {
        for line in [
            "monitor",
            "monitor\t",
            "monitor\t1",
            "monitor\tON",
            "monitor\ton\toff",
            "monitor_clock\ttrue",
        ] {
            assert!(parse(line).is_err(), "{line}");
        }
    }

    #[test]
    fn test_parse_when_fake_project_defect_returns_the_switch_or_an_error() {
        assert_eq!(
            parse("fake_project_defect\t1"),
            Ok(Command::FakeProjectDefect(true))
        );
        assert_eq!(
            parse("fake_project_defect\t0"),
            Ok(Command::FakeProjectDefect(false))
        );
        assert!(parse("fake_project_defect\t2").is_err());
        assert!(parse("fake_project_defect").is_err());
    }

    #[test]
    fn test_parse_when_simple_command_returns_it() {
        assert_eq!(parse("fetch_sound"), Ok(Command::FetchSound));
        assert_eq!(parse("os_cancel"), Ok(Command::OsCancel));
        assert_eq!(parse("tempester_version"), Ok(Command::TempesterVersion));
        assert!(parse("tempester_version\t1").is_err());
    }

    #[test]
    fn test_parse_when_os_send_variants_and_verify_return_distinct_commands() {
        assert_eq!(parse("os_send"), Ok(Command::OsSend));
        assert_eq!(parse("os_send_unknown"), Ok(Command::OsSendUnknown));
        assert_eq!(parse("os_verify_start"), Ok(Command::OsVerifyStart));
        assert_eq!(parse("os_verify_stop"), Ok(Command::OsVerifyStop));
        assert!(parse("os_send_unknown\tyes").is_err());
    }

    #[test]
    fn test_parse_when_simple_command_has_argument_returns_arguments_error() {
        assert!(matches!(
            parse("fetch_sound\tx"),
            Err(CommandError::Arguments { expected: 0, .. })
        ));
    }

    #[test]
    fn test_parse_when_connect_with_and_without_port_returns_option() {
        assert_eq!(parse("connect\t"), Ok(Command::Connect(None)));
        assert_eq!(
            parse("connect\tIAC Driver Bus 1"),
            Ok(Command::Connect(Some("IAC Driver Bus 1".into())))
        );
    }

    #[test]
    fn test_parse_when_nrpn_in_range_returns_numbers() {
        assert_eq!(
            parse("nrpn\t34\t96"),
            Ok(Command::Nrpn {
                number: 34,
                value: 96
            })
        );
    }

    #[test]
    fn test_parse_when_nrpn_out_of_range_or_not_a_number_returns_bad_argument() {
        for line in [
            "nrpn\t16384\t0",
            "nrpn\t1\t16384",
            "nrpn\t-1\t0",
            "nrpn\tx\t0",
            "nrpn\t1",
        ] {
            assert!(parse(line).is_err(), "{line}");
        }
    }

    #[test]
    fn test_parse_when_channel_outside_1_to_16_returns_error() {
        assert_eq!(parse("set_channel\t16"), Ok(Command::SetChannel(16)));
        assert!(parse("set_channel\t0").is_err());
        assert!(parse("set_channel\t17").is_err());
    }

    #[test]
    fn test_parse_when_note_returns_on_off_note_velocity() {
        assert_eq!(
            parse("note\ton\t36\t100"),
            Ok(Command::Note {
                on: true,
                note: 36,
                velocity: 100
            })
        );
        assert!(parse("note\tmaybe\t36\t100").is_err());
        assert!(parse("note\ton\t128\t100").is_err());
    }

    #[test]
    fn test_parse_when_pad_commands_returns_them_and_refuses_bad_values() {
        assert_eq!(
            parse("pad\ton\t31\t100"),
            Ok(Command::Pad {
                on: true,
                pad: 31,
                velocity: 100
            })
        );
        assert_eq!(
            parse("pad_note\t0\t50"),
            Ok(Command::SetPadNote { pad: 0, note: 50 })
        );
        assert_eq!(parse("pad_notes"), Ok(Command::PadNotes));
        assert_eq!(parse("pad_notes_reset"), Ok(Command::ResetPadNotes));
        for line in [
            "pad\ton\t32\t100",
            "pad\tmaybe\t0\t100",
            "pad\ton\t0",
            "pad_note\t32\t50",
            "pad_note\t0\t128",
        ] {
            assert!(parse(line).is_err(), "{line}");
        }
    }

    #[test]
    fn test_parse_when_cc_is_in_the_allow_list_returns_controller_and_value() {
        assert_eq!(
            parse("cc\t12\t127"),
            Ok(Command::Cc {
                controller: 12,
                value: 127
            })
        );
        for controller in CC_ALLOWED {
            assert!(parse(&format!("cc\t{controller}\t0")).is_ok());
        }
    }

    #[test]
    fn test_parse_when_performance_commands_returns_them_and_refuses_bad_values() {
        assert_eq!(
            parse("beat_wide\t7\t254"),
            Ok(Command::BeatWide {
                index: 7,
                position: 254
            })
        );
        assert_eq!(
            parse("ribbon\t1\ttouch\t127"),
            Ok(Command::Ribbon {
                ribbon: 1,
                position: Some(127),
                pressure: None
            })
        );
        assert_eq!(
            parse("ribbon\t0\ttouch\t5\t90"),
            Ok(Command::Ribbon {
                ribbon: 0,
                position: Some(5),
                pressure: Some(90)
            })
        );
        assert_eq!(
            parse("ribbon\t0\trelease"),
            Ok(Command::Ribbon {
                ribbon: 0,
                position: None,
                pressure: None
            })
        );
        assert_eq!(
            parse("pad_probability\t15\t100"),
            Ok(Command::PadProbability {
                pad: 15,
                percent: 100
            })
        );
        assert_eq!(parse("fake_perform"), Ok(Command::FakePerform));
        for line in [
            "beat_wide\t8\t0",
            "beat_wide\t0\t255",
            "ribbon\t2\trelease",
            "ribbon\t0\ttouch\t128",
            "ribbon\t0\ttouch",
            "ribbon\t0\ttouch\t1\t128",
            "ribbon\t0\trelease\t3",
            "ribbon\t0\tslide\t3",
            "pad_probability\t16\t50",
            "pad_probability\t0\t101",
        ] {
            assert!(parse(line).is_err(), "{line}");
        }
    }

    #[test]
    fn test_parse_when_cc_is_not_read_by_the_tempest_or_out_of_range_returns_error() {
        for line in [
            "cc\t7\t100",
            "cc\t64\t1",
            "cc\t11\t1",
            "cc\t28\t1",
            "cc\t12\t128",
            "cc\t12\t-1",
            "cc\t128\t0",
            "cc\t12",
            "cc\t12\t1\t2",
        ] {
            assert!(parse(line).is_err(), "{line}");
        }
    }

    #[test]
    fn test_parse_when_key_returns_press_or_release_with_the_code() {
        assert_eq!(
            parse("key\tpress\t0"),
            Ok(Command::Key {
                down: true,
                code: 0
            })
        );
        assert_eq!(
            parse("key\trelease\t48"),
            Ok(Command::Key {
                down: false,
                code: 48
            })
        );
    }

    #[test]
    fn test_parse_when_key_code_out_of_range_or_malformed_returns_error() {
        for line in [
            "key\tpress\t49",
            "key\trelease\t128",
            "key\tpress\t-1",
            "key\tpress\tx",
            "key\thold\t3",
            "key\tpress",
            "key\tpress\t3\t4",
        ] {
            assert!(parse(line).is_err(), "{line}");
        }
    }

    #[test]
    fn test_parse_when_control_in_range_returns_index_and_value() {
        assert_eq!(
            parse("control\t8\t255"),
            Ok(Command::Control {
                index: 8,
                value: 255
            })
        );
        for line in [
            "control\t10\t0",
            "control\t8\t256",
            "control\t8",
            "control\t-1\t0",
        ] {
            assert!(parse(line).is_err(), "{line}");
        }
    }

    #[test]
    fn test_parse_when_encoder_in_range_returns_index_and_signed_step() {
        assert_eq!(
            parse("encoder\t13\t-16"),
            Ok(Command::Encoder {
                index: 13,
                step: -16
            })
        );
        assert_eq!(
            parse("encoder\t0\t15"),
            Ok(Command::Encoder { index: 0, step: 15 })
        );
        for line in [
            "encoder\t14\t1",
            "encoder\t0\t16",
            "encoder\t0\t-17",
            "encoder\t0\tx",
            "encoder\t-1\t1",
            "encoder\t0",
        ] {
            assert!(parse(line).is_err(), "{line}");
        }
    }

    #[test]
    fn test_parse_when_panel_zoom_in_range_returns_percent_and_refuses_the_rest() {
        assert_eq!(
            parse("panel_zoom\t60"),
            Ok(Command::SetPanelZoom { percent: 60 })
        );
        assert_eq!(
            parse("panel_zoom\t200"),
            Ok(Command::SetPanelZoom { percent: 200 })
        );
        for line in [
            "panel_zoom\t59",
            "panel_zoom\t201",
            "panel_zoom\tx",
            "panel_zoom",
        ] {
            assert!(parse(line).is_err(), "{line}");
        }
    }

    #[test]
    fn test_parse_when_panel_view_returns_the_view_and_refuses_other_words() {
        assert_eq!(
            parse("panel_view\tfull"),
            Ok(Command::SetPanelView { full: true })
        );
        assert_eq!(
            parse("panel_view\tmachine"),
            Ok(Command::SetPanelView { full: false })
        );
        assert!(parse("panel_view\tbig").is_err());
        assert!(parse("panel_view").is_err());
    }

    #[test]
    fn test_parse_when_panel_state_returns_pads_flag_and_refuses_other_values() {
        assert_eq!(
            parse("panel_state\t1"),
            Ok(Command::PanelState { pads: true })
        );
        assert_eq!(
            parse("panel_state\t0"),
            Ok(Command::PanelState { pads: false })
        );
        assert!(parse("panel_state\t2").is_err());
        assert!(parse("panel_state").is_err());
    }

    #[test]
    fn test_parse_when_transport_returns_start_or_stop_and_refuses_the_rest() {
        assert_eq!(
            parse("transport\tstart"),
            Ok(Command::Transport { start: true })
        );
        assert_eq!(
            parse("transport\tstop"),
            Ok(Command::Transport { start: false })
        );
        assert!(parse("transport\tpause").is_err());
        assert!(parse("transport").is_err());
    }

    #[test]
    fn test_parse_when_clock_bpm_outside_20_to_300_returns_error() {
        assert_eq!(parse("clock_start\t120"), Ok(Command::ClockStart(120)));
        assert!(parse("clock_start\t19").is_err());
        assert!(parse("clock_bpm\t301").is_err());
    }

    #[test]
    fn test_parse_when_file_command_returns_name_and_decoded_bytes() {
        assert_eq!(
            parse("open_sound\tkick.syx\tf001f7"),
            Ok(Command::OpenSound {
                name: "kick.syx".into(),
                data: vec![0xF0, 0x01, 0xF7]
            })
        );
    }

    #[test]
    fn test_parse_when_file_data_is_not_hex_returns_bad_argument() {
        assert!(matches!(
            parse("os_validate\tx.syx\tzz"),
            Err(CommandError::BadArgument { .. })
        ));
    }

    #[test]
    fn test_parse_when_reveal_unknown_place_returns_error() {
        assert_eq!(parse("reveal\tos"), Ok(Command::Reveal(Place::Os)));
        assert!(parse("reveal\t/etc").is_err());
    }

    #[test]
    fn test_parse_when_project_commands_return_them_and_refuse_arguments() {
        assert_eq!(parse("receive_project"), Ok(Command::ReceiveProject));
        assert_eq!(parse("fetch_project"), Ok(Command::FetchProject));
        assert_eq!(parse("project_cancel"), Ok(Command::ProjectCancel));
        assert_eq!(parse("fake_project_send"), Ok(Command::FakeProjectSend));
        assert_eq!(
            parse("fake_project_send_partial"),
            Ok(Command::FakeProjectSendPartial)
        );
        assert_eq!(
            parse("reveal\tprojects"),
            Ok(Command::Reveal(Place::Projects))
        );
        assert!(parse("receive_project\tx").is_err());
    }

    #[test]
    fn test_parse_when_library_kind_unknown_returns_error() {
        assert_eq!(parse("library\tbeats"), Ok(Command::Library(Kind::Beats)));
        assert_eq!(
            parse("library\tprojects"),
            Ok(Command::Library(Kind::Projects))
        );
        assert!(parse("library\tbackups").is_err());
        assert!(parse("library\tos").is_err());
    }

    #[test]
    fn test_parse_when_empty_or_unknown_returns_error() {
        assert_eq!(parse(""), Err(CommandError::Empty));
        assert_eq!(parse("\targ"), Err(CommandError::Empty));
        assert_eq!(
            parse("format_disk"),
            Err(CommandError::Unknown("format_disk".into()))
        );
    }

    #[test]
    fn test_parse_when_page_sends_shutdown_returns_unknown_command() {
        assert_eq!(
            parse("shutdown"),
            Err(CommandError::Unknown("shutdown".into()))
        );
    }

    #[test]
    fn test_parse_when_too_many_arguments_for_save_returns_arguments_error() {
        assert!(matches!(
            parse("save_sound\ta\tb"),
            Err(CommandError::Arguments { expected: 1, .. })
        ));
    }

    #[test]
    fn test_parse_when_page_error_returns_its_message() {
        assert_eq!(
            parse("page_error	boom (panel.js:3:1)"),
            Ok(Command::PageError("boom (panel.js:3:1)".into()))
        );
    }

    #[test]
    fn test_parse_when_page_error_has_no_message_returns_arguments_error() {
        assert!(matches!(
            parse("page_error"),
            Err(CommandError::Arguments { expected: 1, .. })
        ));
    }

    #[test]
    fn test_parse_when_seq_pad_is_valid_returns_the_pattern() {
        let expected = PadPattern::new(8, 3, 1, 80).expect("valid");

        assert_eq!(
            parse("seq_pad\t4\t8\t3\t1\t80"),
            Ok(Command::SeqPad {
                pad: 4,
                pattern: expected
            })
        );
        assert_eq!(parse("seq_state"), Ok(Command::SeqState));
        assert_eq!(
            parse("seq_run\t1\t90"),
            Ok(Command::SeqRun {
                enabled: true,
                velocity: 90
            })
        );
    }

    #[test]
    fn test_parse_when_seq_pad_is_out_of_range_returns_an_error() {
        for line in [
            "seq_pad\t16\t8\t3\t0\t100",
            "seq_pad\t0\t0\t0\t0\t100",
            "seq_pad\t0\t33\t3\t0\t100",
            "seq_pad\t0\t8\t9\t0\t100",
            "seq_pad\t0\t8\t3\t8\t100",
            "seq_pad\t0\t8\t3\t0\t101",
            "seq_pad\t0\t8\t3\t0",
            "seq_pad\t0\t8\t-1\t0\t100",
            "seq_run\t1\t0",
            "seq_run\t2\t100",
        ] {
            assert!(parse(line).is_err(), "{line}");
        }
    }
}
