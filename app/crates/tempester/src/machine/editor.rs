//! The Panel page's editing tools on the worker (`crate::edit` holds their logic):
//!
//! - **History**: every knob of the Full view (and every learned controller driving one) arrives as an NRPN command;
//!   it is recorded here (a drag = one step), and undo / redo send back only the parameters of the step.
//! - **A/B** and **variations** move the whole sound as one undoable step and need a sound fetched first (Sync): the
//!   model is then what the machine holds. A SysEx sent to the machine (sound, beat, kit, library item) may replace
//!   that sound, so the model is dropped until the next Sync.
//! - How it sends: the NRPN of each parameter that differs, the same message as a knob turned on screen, one
//!   parameter per packet, paced at the MIDI cable rate. Chosen over the whole sound (`60`) because NRPN is the path
//!   proven on the machine (Tempester 1.5.3.0 and later, tests T0-T15), it changes only what differs, it acts on the
//!   same pad as the knobs (the selected one) and only in RAM, and it shows no receive screen; the `60` send has not
//!   been tried on the machine yet. Nothing here writes the machine's flash.
//! - **MIDI learn**: an input port alone (`InputPort`: nothing can be sent to the controller, so nothing echoes back
//!   to it). A learned Control Change becomes a `learned` event; the page moves the control and sends it through its
//!   own knob path (same NRPN, same 30 ms rate limit). The table is saved in the settings file, read again and merged
//!   before each write.

use std::collections::BTreeMap;
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tempest_midi::message::Message;
use tempest_midi::{InputPort, StreamParser};
use tempest_proto::sound::Sound;

use super::Machine;
use crate::command::Command;
use crate::edit::command::{EditCommand, Slot};
use crate::edit::history::{Change, History, StepKind, Values};
use crate::edit::learn::{is_learnable, LearnMap, Target, KEY_LEARN_MAP, KEY_LEARN_PORT};
use crate::edit::variation::vary;
use crate::error::AppError;
use crate::event::Event;
use crate::storage::Storage;

/// Bytes of one NRPN (four Control Changes with their status).
const NRPN_BYTES: u64 = 12;

/// The open controller port and the messages it delivered.
struct Controller {
    port: String,
    _input: Arc<dyn InputPort>,
    queue: Receiver<Message>,
}

/// What the editing tools keep between commands.
pub(super) struct EditState {
    history: History,
    /// True once a sound was fetched, until a SysEx send may have replaced it.
    synced: bool,
    slots: [Option<Values>; 2],
    active: Option<Slot>,
    learn: LearnMap,
    armed: Option<Target>,
    controller: Option<Controller>,
    /// A controller port saved by an earlier session, opened again when the page first asks for the state.
    saved_port: Option<String>,
}

impl EditState {
    pub(super) fn load(storage: &Storage) -> Self {
        let settings = storage.load_settings();
        Self {
            history: History::new(&Sound::with_defaults().values()),
            synced: false,
            slots: [None, None],
            active: None,
            learn: LearnMap::parse(settings.get(KEY_LEARN_MAP).unwrap_or_default()),
            armed: None,
            controller: None,
            saved_port: settings
                .get(KEY_LEARN_PORT)
                .filter(|p| !p.is_empty())
                .map(str::to_string),
        }
    }
}

fn slot_index(slot: Slot) -> usize {
    match slot {
        Slot::A => 0,
        Slot::B => 1,
    }
}

impl Machine {
    pub(super) fn handle_edit(&mut self, command: Command) -> Option<Command> {
        match command {
            Command::Edit(edit) => self.guard("edit", |m| m.run_edit(edit)),
            other => return Some(other),
        }
        None
    }

    fn run_edit(&mut self, command: EditCommand) -> Result<(), AppError> {
        match command {
            EditCommand::State => {
                self.reopen_saved_port();
                self.publish_edit(false);
                self.publish_learn(None);
                Ok(())
            }
            EditCommand::Undo => self.step_back(true),
            EditCommand::Redo => self.step_back(false),
            EditCommand::Store(slot) => self.store(slot),
            EditCommand::Recall(slot) => self.recall(slot),
            EditCommand::Vary {
                group,
                amount,
                seed,
            } => {
                let target = vary(self.synced_values()?, group, amount, seed);
                self.jump(&target, StepKind::Variation(seed))
            }
            EditCommand::LearnPorts => {
                let ports = self.connector.input_ports()?;
                self.emit(Event::new("learn_ports").with("ports", ports));
                Ok(())
            }
            EditCommand::LearnOpen(port) => self.open_controller(port),
            EditCommand::LearnArm(target) => {
                self.edit.armed = Some(target);
                self.publish_learn(None);
                Ok(())
            }
            EditCommand::LearnCancel => {
                self.edit.armed = None;
                self.publish_learn(None);
                Ok(())
            }
            EditCommand::LearnClear(target) => self.change_map(|map| {
                map.clear_target(target);
            }),
            EditCommand::LearnClearAll => self.change_map(LearnMap::clear),
            EditCommand::LearnFakeCc { controller, value } => {
                let fake = self
                    .connector
                    .fake_controller()
                    .filter(|_| self.edit.controller.is_some())
                    .ok_or(AppError::Precondition(
                        "only with the fake controller, once opened",
                    ))?;
                fake.push_incoming(&[0xB0, controller, value]);
                Ok(())
            }
        }
    }

    /// Hook of the knob path (`handle_panel`): a sound parameter set from the page joins the history.
    pub(super) fn edit_knob(&mut self, number: u16, value: u16) {
        let Ok(index) = u8::try_from(number) else {
            return;
        };
        let before = (self.edit.history.depth(), self.edit.history.can_redo());
        self.edit.history.knob(index, value, Instant::now());
        if before != (self.edit.history.depth(), self.edit.history.can_redo()) {
            self.publish_edit(false);
        }
    }

    /// Hook of the machine's reports (`on_channel`): the model follows a knob turned on the machine.
    pub(super) fn edit_observe(&mut self, number: u16, value: u16) {
        if let Ok(index) = u8::try_from(number) {
            self.edit.history.observe(index, value);
        }
    }

    /// Hook of `fetch_sound`: the machine's sound is the new start of the history.
    pub(super) fn edit_synced(&mut self, sound: &Sound) {
        self.edit.history = History::new(&sound.values());
        self.edit.synced = true;
        self.publish_edit(false);
    }

    /// Hook of every SysEx send: the machine's sound may have been replaced, the model is no longer known.
    pub(super) fn edit_forget(&mut self) {
        if self.edit.synced || self.edit.history.can_undo() || self.edit.history.can_redo() {
            self.edit.history = History::new(self.edit.history.values());
            self.edit.synced = false;
            self.publish_edit(false);
        }
    }

    fn synced_values(&self) -> Result<&Values, AppError> {
        if self.edit.synced {
            Ok(self.edit.history.values())
        } else {
            Err(AppError::Precondition("fetch the sound first (Sync)"))
        }
    }

    fn step_back(&mut self, undo: bool) -> Result<(), AppError> {
        self.idle_session()?;
        let changes = if undo {
            self.edit
                .history
                .undo()
                .ok_or(AppError::Precondition("nothing to undo"))?
        } else {
            self.edit
                .history
                .redo()
                .ok_or(AppError::Precondition("nothing to redo"))?
        };
        self.send_changes(&changes)?;
        self.publish_edit(true);
        Ok(())
    }

    fn store(&mut self, slot: Slot) -> Result<(), AppError> {
        let values = *self.synced_values()?;
        self.edit.slots[slot_index(slot)] = Some(values);
        self.edit.active = Some(slot);
        self.publish_edit(false);
        Ok(())
    }

    fn recall(&mut self, slot: Slot) -> Result<(), AppError> {
        self.synced_values()?;
        let target = self.edit.slots[slot_index(slot)]
            .ok_or(AppError::Precondition("that slot is empty"))?;
        self.jump(&target, StepKind::Recall(slot.letter()))?;
        self.edit.active = Some(slot);
        self.publish_edit(true);
        Ok(())
    }

    fn jump(&mut self, target: &Values, kind: StepKind) -> Result<(), AppError> {
        self.idle_session()?;
        let changes = self.edit.history.jump(target, kind, Instant::now());
        self.send_changes(&changes)?;
        self.publish_edit(true);
        Ok(())
    }

    /// One NRPN per changed parameter, each in its own packet, paced at the cable rate.
    fn send_changes(&mut self, changes: &[Change]) -> Result<(), AppError> {
        let pause = Duration::from_micros(NRPN_BYTES * 1_000_000 / u64::from(self.rate.max(1)));
        for (k, &(index, value)) in changes.iter().enumerate() {
            if k > 0 {
                std::thread::sleep(pause);
            }
            self.send_nrpn(u16::from(index), value)?;
        }
        Ok(())
    }

    fn publish_edit(&self, with_values: bool) {
        let history = &self.edit.history;
        let mut event = Event::new("edit")
            .with("synced", self.edit.synced)
            .with("undo", history.last_label())
            .with("redo", history.next_label())
            .with("depth", history.depth())
            .with("a", self.edit.slots[0].is_some())
            .with("b", self.edit.slots[1].is_some())
            .with("active", self.edit.active.map(|s| s.letter().to_string()));
        if with_values {
            event = event.with("values", history.values().to_vec());
        }
        self.emit(event);
    }

    fn publish_learn(&self, learned: Option<(u8, Target)>) {
        let map: Vec<crate::json::Json> = self
            .edit
            .learn
            .entries()
            .into_iter()
            .map(|(controller, id)| vec![crate::json::Json::from(controller), id.into()].into())
            .collect();
        let mut event = Event::new("learn")
            .with(
                "port",
                self.edit.controller.as_ref().map(|c| c.port.clone()),
            )
            .with("saved_port", self.edit.saved_port.clone())
            .with("armed", self.edit.armed.map(Target::id))
            .with("map", map)
            .with("fake", self.connector.is_fake());
        if let Some((controller, target)) = learned {
            event = event
                .with("learned_cc", controller)
                .with("learned_target", target.id());
        }
        self.emit(event);
    }

    /// Reads the saved table, changes it, writes it, keeps it: another window's change is not lost.
    fn change_map(&mut self, change: impl FnOnce(&mut LearnMap)) -> Result<(), AppError> {
        let mut kept = LearnMap::default();
        self.storage.update_settings(|settings| {
            let mut map = LearnMap::parse(settings.get(KEY_LEARN_MAP).unwrap_or_default());
            change(&mut map);
            settings.set(KEY_LEARN_MAP, &map.render());
            kept = map;
        })?;
        self.edit.learn = kept;
        self.publish_learn(None);
        Ok(())
    }

    fn open_controller(&mut self, port: Option<String>) -> Result<(), AppError> {
        self.edit.controller = None;
        self.edit.armed = None;
        let Some(name) = port else {
            self.edit.saved_port = None;
            self.storage.update_settings(|s| s.remove(KEY_LEARN_PORT))?;
            self.publish_learn(None);
            return Ok(());
        };
        if self.session.as_ref().is_some_and(|s| s.port == name) {
            return Err(AppError::Precondition(
                "the Tempest's own port cannot be the controller",
            ));
        }
        let result = self.attach_controller(&name);
        self.publish_learn(None);
        result?;
        self.edit.saved_port = Some(name.clone());
        self.storage
            .update_settings(|s| s.set(KEY_LEARN_PORT, &name))?;
        self.publish_learn(None);
        Ok(())
    }

    fn attach_controller(&mut self, name: &str) -> Result<(), AppError> {
        let input = self.connector.open_input(name)?;
        let (sender, queue) = mpsc::channel();
        let mut parser = StreamParser::new();
        input.listen(Some(Box::new(move |bytes| {
            for message in parser.push_bytes(bytes) {
                // A closed queue means the controller was closed: nobody to tell.
                let _ = sender.send(message);
            }
        })));
        self.edit.controller = Some(Controller {
            port: name.to_string(),
            _input: input,
            queue,
        });
        Ok(())
    }

    /// The port saved by an earlier session, quietly: a controller left unplugged is not an error.
    fn reopen_saved_port(&mut self) {
        if self.edit.controller.is_none() {
            if let Some(name) = self.edit.saved_port.clone() {
                let _ = self.attach_controller(&name);
            }
        }
    }

    /// What the controller sent since the last call: a move while a control waits is learned (and consumed); a
    /// learned controller's last value of the batch becomes a `learned` event for the page.
    pub(super) fn poll_controller(&mut self) {
        let Some(controller) = &self.edit.controller else {
            return;
        };
        let moves: Vec<(u8, u8)> = controller
            .queue
            .try_iter()
            .filter_map(|message| match message {
                Message::Channel(short) if short.status & 0xF0 == 0xB0 => match short.data() {
                    &[cc, value] => Some((cc, value)),
                    _ => None,
                },
                _ => None,
            })
            .collect();
        let mut last: BTreeMap<u8, u8> = BTreeMap::new();
        let mut consumed = None;
        for (cc, value) in moves {
            if let Some(target) = self.edit.armed.filter(|_| is_learnable(cc)) {
                self.edit.armed = None;
                consumed = Some(cc);
                if let Err(error) = self.change_map(|map| {
                    map.set(cc, target);
                }) {
                    self.emit(Event::error("edit", &error));
                }
                self.publish_learn(Some((cc, target)));
            } else if consumed != Some(cc) && self.edit.learn.get(cc).is_some() {
                last.insert(cc, value);
            }
        }
        for (cc, value) in last {
            if let Some(target) = self.edit.learn.get(cc) {
                self.emit(
                    Event::new("learned")
                        .with("controller", cc)
                        .with("target", target.id())
                        .with("value", target.scale(value)),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests;
