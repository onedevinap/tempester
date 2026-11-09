//! The editing tools' commands from the page (`edit_*`, `learn_*`), parsed here so that `command.rs` only routes
//! them.

use super::learn::{is_learnable, Target};
use super::variation::{Group, AMOUNT_MAX, AMOUNT_MIN};
use crate::command::CommandError;

/// A or B.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    A,
    B,
}

impl Slot {
    pub fn letter(self) -> char {
        match self {
            Self::A => 'A',
            Self::B => 'B',
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditCommand {
    /// Publishes the editing state (history, A/B, learn table).
    State,
    Undo,
    Redo,
    /// Keeps the current sound in slot A or B.
    Store(Slot),
    /// Sends slot A or B to the machine (only the parameters that differ), as one undoable step.
    Recall(Slot),
    /// Generates a variation of the current sound and sends it, as one undoable step.
    Vary {
        group: Group,
        amount: u8,
        seed: u32,
    },
    /// Lists the MIDI input ports a controller can be on.
    LearnPorts,
    /// Opens the controller's input port; `None` closes it.
    LearnOpen(Option<String>),
    /// Waits for a controller's move to map it to `target`.
    LearnArm(Target),
    LearnCancel,
    LearnClear(Target),
    LearnClearAll,
    /// Fake MIDI only: the simulated controller sends Control Change `controller` = `value`.
    LearnFakeCc {
        controller: u8,
        value: u8,
    },
}

/// Largest seed the page may give (it shows it; five digits are easy to note down).
pub const SEED_MAX: u32 = 99_999;

pub fn parse(name: &str, args: &[&str]) -> Result<EditCommand, CommandError> {
    let bad = |why: String| CommandError::BadArgument {
        command: name.to_string(),
        why,
    };
    let expect = |count: usize| {
        if args.len() == count {
            Ok(())
        } else {
            Err(CommandError::Arguments {
                command: name.to_string(),
                expected: count,
            })
        }
    };
    let number = |at: usize, min: u32, max: u32| {
        let text = args[at];
        text.parse::<u32>()
            .ok()
            .filter(|n| (min..=max).contains(n))
            .ok_or_else(|| bad(format!("\"{text}\" is not a number from {min} to {max}")))
    };
    let slot = |at: usize| match args[at] {
        "a" => Ok(Slot::A),
        "b" => Ok(Slot::B),
        other => Err(bad(format!("\"{other}\" is neither a nor b"))),
    };
    let target = |at: usize| {
        Target::parse(args[at])
            .ok_or_else(|| bad(format!("\"{}\" is not a control of the panel", args[at])))
    };
    let command = match name {
        "edit_state" | "edit_undo" | "edit_redo" | "learn_ports" | "learn_cancel"
        | "learn_clear_all" => {
            expect(0)?;
            match name {
                "edit_state" => EditCommand::State,
                "edit_undo" => EditCommand::Undo,
                "edit_redo" => EditCommand::Redo,
                "learn_ports" => EditCommand::LearnPorts,
                "learn_cancel" => EditCommand::LearnCancel,
                _ => EditCommand::LearnClearAll,
            }
        }
        "edit_store" | "edit_recall" => {
            expect(1)?;
            if name == "edit_store" {
                EditCommand::Store(slot(0)?)
            } else {
                EditCommand::Recall(slot(0)?)
            }
        }
        "edit_vary" => {
            expect(3)?;
            let group = Group::parse(args[0]).ok_or_else(|| {
                bad(format!(
                    "\"{}\" is not pitch, env, filter, mod or all",
                    args[0]
                ))
            })?;
            EditCommand::Vary {
                group,
                amount: number(1, u32::from(AMOUNT_MIN), u32::from(AMOUNT_MAX))? as u8,
                seed: number(2, 0, SEED_MAX)?,
            }
        }
        "learn_open" => {
            expect(1)?;
            EditCommand::LearnOpen((!args[0].is_empty()).then(|| args[0].to_string()))
        }
        "learn_arm" | "learn_clear" => {
            expect(1)?;
            if name == "learn_arm" {
                EditCommand::LearnArm(target(0)?)
            } else {
                EditCommand::LearnClear(target(0)?)
            }
        }
        "learn_fake_cc" => {
            expect(2)?;
            let controller = number(0, 0, 127)? as u8;
            if !is_learnable(controller) {
                return Err(bad(format!("CC {controller} cannot be learned")));
            }
            EditCommand::LearnFakeCc {
                controller,
                value: number(1, 0, 127)? as u8,
            }
        }
        _ => return Err(CommandError::Unknown(name.to_string())),
    };
    Ok(command)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::{parse as parse_line, Command};

    fn edit(line: &str) -> Result<EditCommand, CommandError> {
        match parse_line(line)? {
            Command::Edit(command) => Ok(command),
            other => panic!("not an edit command: {other:?}"),
        }
    }

    #[test]
    fn test_parse_when_valid_lines_returns_each_command() {
        assert_eq!(edit("edit_undo"), Ok(EditCommand::Undo));
        assert_eq!(edit("edit_store\tb"), Ok(EditCommand::Store(Slot::B)));
        assert_eq!(edit("edit_recall\ta"), Ok(EditCommand::Recall(Slot::A)));
        assert_eq!(
            edit("edit_vary\tfilter\t25\t4242"),
            Ok(EditCommand::Vary {
                group: Group::Filter,
                amount: 25,
                seed: 4242
            })
        );
        assert_eq!(edit("learn_open\t"), Ok(EditCommand::LearnOpen(None)));
        assert_eq!(
            edit("learn_open\tMy Knobs"),
            Ok(EditCommand::LearnOpen(Some("My Knobs".into())))
        );
        assert_eq!(
            edit("learn_arm\tn34"),
            Ok(EditCommand::LearnArm(Target::Param(34)))
        );
        assert_eq!(
            edit("learn_fake_cc\t21\t64"),
            Ok(EditCommand::LearnFakeCc {
                controller: 21,
                value: 64
            })
        );
    }

    #[test]
    fn test_parse_when_arguments_are_wrong_returns_an_error() {
        for line in [
            "edit_undo\tx",
            "edit_store\tc",
            "edit_vary\tamp\t25\t1",
            "edit_vary\tall\t0\t1",
            "edit_vary\tall\t101\t1",
            "edit_vary\tall\t50\t100000",
            "edit_vary\tall\t50",
            "learn_arm\tn127",
            "learn_clear\t",
            "learn_fake_cc\t6\t1",
            "learn_fake_cc\t21\t128",
            "edit_nothing",
        ] {
            assert!(parse_line(line).is_err(), "{line}");
        }
    }
}
