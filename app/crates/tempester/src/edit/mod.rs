//! The Panel page's editing tools, as pure logic (no MIDI, no file): the edit history of the current sound with A/B
//! snapshots (`history`), seeded variations (`variation`), the MIDI learn table (`learn`) and their commands
//! (`command`). `machine::editor` wires them to the worker.

pub mod command;
pub mod history;
pub mod learn;
pub mod variation;
