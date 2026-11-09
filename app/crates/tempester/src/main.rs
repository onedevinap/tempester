//! Tempester: controller, librarian and updater for the DSI Tempest. MIDI only, no audio.
//!
//! `TEMPESTER_FAKE_MIDI=1` runs against a scripted machine in memory: nothing reaches a MIDI port.
//! `--version` prints the version and exits (the packaging scripts use it to check a build).

// A release build on Windows is a GUI program: no console window behind the app window.
#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

mod command;
mod edit;
mod error;
mod event;
mod hex;
mod json;
mod json_read;
mod library;
mod machine;
mod storage;
mod ui;

use std::process::ExitCode;

use machine::connector::{Connector, FakeConnector, RealConnector};
use storage::Storage;

const FAKE_MIDI_VAR: &str = "TEMPESTER_FAKE_MIDI";
/// With the fake machine, `TEMPESTER_FAKE_PANEL=0` plays a machine without the remote panel (keys stay greyed).
const FAKE_PANEL_VAR: &str = "TEMPESTER_FAKE_PANEL";

/// The line printed by `--version`.
fn version_line() -> String {
    format!("tempester {}", env!("CARGO_PKG_VERSION"))
}

/// Whether the command line asks for the version (`--version` or `-V`, as the only argument).
fn wants_version<I: IntoIterator<Item = String>>(args: I) -> bool {
    let mut args = args.into_iter();
    matches!(
        (args.next().as_deref(), args.next()),
        (Some("--version" | "-V"), None)
    )
}

fn main() -> ExitCode {
    if wants_version(std::env::args().skip(1)) {
        println!("{}", version_line());
        return ExitCode::SUCCESS;
    }
    let fake = std::env::var(FAKE_MIDI_VAR).is_ok_and(|value| value == "1");
    let storage = if fake {
        Storage::open_fake()
    } else {
        Storage::open()
    };
    let storage = match storage {
        Ok(storage) => storage,
        Err(e) => {
            eprintln!("tempester: {e}");
            return ExitCode::FAILURE;
        }
    };
    let connector: Box<dyn Connector> = if fake {
        if std::env::var(FAKE_PANEL_VAR).is_ok_and(|value| value == "0") {
            Box::new(FakeConnector::without_remote_panel())
        } else {
            Box::new(FakeConnector::new())
        }
    } else {
        Box::new(RealConnector)
    };
    match ui::run(connector, storage) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("tempester: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|a| a.to_string()).collect()
    }

    #[test]
    fn test_wants_version_when_version_flag_returns_true() {
        assert!(wants_version(args(&["--version"])));
        assert!(wants_version(args(&["-V"])));
    }

    #[test]
    fn test_wants_version_when_no_argument_or_other_returns_false() {
        assert!(!wants_version(args(&[])));
        assert!(!wants_version(args(&["--help"])));
        assert!(!wants_version(args(&["--version", "extra"])));
    }

    #[test]
    fn test_version_line_when_built_returns_workspace_version() {
        let from_manifest = include_str!("../../../Cargo.toml")
            .lines()
            .find_map(|l| l.strip_prefix("version = \""))
            .and_then(|v| v.strip_suffix('"'))
            .map(str::to_owned);

        assert_eq!(
            Some(version_line()),
            from_manifest.map(|v| format!("tempester {v}"))
        );
    }
}
