//! A failure before or while the window opens. A release build on Windows has no console (`windows_subsystem`), so
//! the message printed on stderr reaches nobody: the window vanishes and the user has nothing to report. The message
//! is also written to a file the user can find and send.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::error::AppError;
use crate::storage::stamp::now_stamp;
use crate::version_line;

const FILE_NAME: &str = "startup-error.txt";

/// The file a start-up failure goes to, and its text: in the data folder when it could be opened, else in the
/// temporary folder (the data folder itself may be what failed).
pub fn report(
    data_root: Option<&Path>,
    temp_dir: &Path,
    stamp: &str,
    message: &str,
) -> (PathBuf, String) {
    let folder = data_root.unwrap_or(temp_dir);
    let text = format!("{}\n{stamp}\n{message}\n", version_line());
    (folder.join(FILE_NAME), text)
}

/// Prints the failure on stderr, writes it to the report file and returns the failure exit code.
pub fn fail(data_root: Option<&Path>, error: &AppError) -> ExitCode {
    let message = error.to_string();
    eprintln!("tempester: {message}");
    let (path, text) = report(data_root, &std::env::temp_dir(), &now_stamp(), &message);
    match fs::write(&path, text) {
        Ok(()) => eprintln!("tempester: written to {}", path.display()),
        Err(e) => eprintln!("tempester: cannot write {}: {e}", path.display()),
    }
    ExitCode::FAILURE
}

#[cfg(test)]
mod tests {
    use super::*;

    const STAMP: &str = "2026-11-20_101500Z";

    #[test]
    fn test_report_when_data_folder_known_returns_a_file_in_it() {
        let (path, _) = report(
            Some(Path::new("/data/Tempester")),
            Path::new("/tmp"),
            STAMP,
            "boom",
        );

        assert_eq!(path, Path::new("/data/Tempester").join("startup-error.txt"));
    }

    #[test]
    fn test_report_when_data_folder_unavailable_returns_a_file_in_the_temp_folder() {
        let (path, _) = report(None, Path::new("/tmp"), STAMP, "boom");

        assert_eq!(path, Path::new("/tmp").join("startup-error.txt"));
    }

    #[test]
    fn test_report_when_built_returns_version_stamp_and_message_on_their_own_lines() {
        let message = "file error: WebView2 environment failed (0x80070005)";

        let (_, text) = report(None, Path::new("/tmp"), STAMP, message);

        assert_eq!(text, format!("{}\n{STAMP}\n{message}\n", version_line()));
    }
}
