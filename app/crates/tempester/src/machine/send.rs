//! The two ways the worker talks to the machine besides the single packets of NRPN and notes: a request that waits
//! for its answer, and a paced SysEx send with progress. Both refuse to run during an OS update.

use std::time::Duration;

use tempest_midi::{dsi_command, request, send_paced, CancelToken};

use super::Machine;
use crate::error::AppError;
use crate::event::Event;

impl Machine {
    /// Sends `request_bytes` and returns the DSI message whose command byte is `answer`.
    pub(super) fn ask(
        &mut self,
        request_bytes: &[u8],
        answer: u8,
        timeout: Duration,
    ) -> Result<Vec<u8>, AppError> {
        let session = self.idle_session()?;
        let reply = request(
            session.transport.as_ref(),
            &session.listener,
            request_bytes,
            timeout,
            &dsi_command(answer),
        );
        reply.map_err(|e| self.failed(e.into()))
    }

    /// Sends `bytes` at the cable rate, reporting `progress` events (about 100 per send) tagged with `kind`.
    pub(super) fn send_sysex(&mut self, kind: &'static str, bytes: &[u8]) -> Result<(), AppError> {
        // A sound, a beat or a library item replaces what the editing tools believe the machine holds.
        self.edit_forget();
        let session = self.idle_session()?;
        let emit = self.emit.clone();
        let mut last_percent = usize::MAX;
        let result = send_paced(
            session.transport.as_ref(),
            bytes,
            self.rate,
            &CancelToken::new(),
            &mut |progress| {
                let percent = progress.sent * 100 / progress.total.max(1);
                if percent != last_percent {
                    last_percent = percent;
                    emit(
                        Event::new("progress")
                            .with("kind", kind)
                            .with("sent", progress.sent)
                            .with("total", progress.total),
                    );
                }
            },
        );
        result.map_err(|e| self.failed(e.into()))
    }

    /// Notes a failure on the connection state and hands the error back.
    pub(super) fn failed(&mut self, error: AppError) -> AppError {
        self.note_failure(&error);
        error
    }
}
