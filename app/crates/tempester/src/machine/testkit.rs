//! Test fixtures for the worker: a machine on the scripted fake, a disposable data folder, and the events it emits.

use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tempest_midi::FakeTransport;

use super::connector::FakeConnector;
use super::{Machine, Worker};
use crate::command::Command;
use crate::event::Event;
use crate::storage::temp_storage;

const WAIT: Duration = Duration::from_secs(10);

/// The events a machine emits, from any of its threads.
pub struct Events(Receiver<Event>);

impl Events {
    /// Waits (up to 10 s) for an event that `last` accepts, and returns it with every event before it; fails the test
    /// on timeout.
    pub fn wait_until(&self, last: impl Fn(&Event) -> bool) -> Vec<Event> {
        let deadline = Instant::now() + WAIT;
        let mut seen = Vec::new();
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.0.recv_timeout(left) {
                Ok(event) => {
                    let done = last(&event);
                    seen.push(event);
                    if done {
                        return seen;
                    }
                }
                Err(_) => panic!("the awaited event did not come within {WAIT:?}"),
            }
        }
    }

    /// Waits (up to 10 s) for an event named `name` from another thread; fails the test on timeout.
    pub fn wait_for(&self, name: &str) -> Event {
        let deadline = Instant::now() + WAIT;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.0.recv_timeout(left) {
                Ok(event) if event.name == name => return event,
                Ok(_) => continue,
                Err(_) => panic!("no \"{name}\" event within {WAIT:?}"),
            }
        }
    }
}

pub struct TestRig {
    pub machine: Machine,
    events: Events,
}

impl TestRig {
    /// A disconnected machine on the fake connector.
    pub fn new(label: &str) -> Self {
        Self::with_connector(label, FakeConnector::new())
    }

    /// A disconnected machine on `connector` (a scripted machine set up by the test).
    pub fn with_connector(label: &str, connector: FakeConnector) -> Self {
        let (sender, events) = mpsc::channel();
        let emit = Arc::new(move |event| {
            // The receiver outlives every emitting thread of a test; a failure only means the test is over.
            let _ = sender.send(event);
        });
        let machine = Machine::new(Box::new(connector), temp_storage(label), emit);
        Self {
            machine,
            events: Events(events),
        }
    }

    /// A machine already connected to the fake port, with the connection event consumed.
    pub fn connected(label: &str) -> Self {
        let mut rig = Self::new(label);
        rig.machine.handle(Command::Connect(None));
        rig.events();
        rig
    }

    /// The scripted machine behind the connection.
    pub fn transport(&self) -> Arc<FakeTransport> {
        self.machine
            .connector
            .fake()
            .expect("connected to the fake machine")
    }

    /// Moves the machine onto the worker thread, as the app runs it: commands then go through `Worker::commands`.
    pub fn into_worker(self) -> (Worker, Events) {
        let worker = super::start(self.machine).expect("the worker thread starts");
        (worker, self.events)
    }

    /// Everything emitted so far.
    pub fn events(&mut self) -> Vec<Event> {
        self.events.0.try_iter().collect()
    }

    /// The first event named `name` among those emitted so far; fails the test when there is none.
    pub fn expect(&mut self, name: &str) -> Event {
        let all = self.events();
        let names: Vec<&str> = all.iter().map(|e| e.name).collect();
        all.iter()
            .find(|event| event.name == name)
            .cloned()
            .unwrap_or_else(|| panic!("no \"{name}\" event among {names:?}"))
    }

    /// Waits (up to 10 s) for an event that `last` accepts, and returns it with every event before it; fails the test
    /// on timeout.
    pub fn wait_until(&mut self, last: impl Fn(&Event) -> bool) -> Vec<Event> {
        self.events.wait_until(last)
    }

    /// Waits (up to 10 s) for an event named `name` from another thread; fails the test on timeout.
    pub fn wait_for(&mut self, name: &str) -> Event {
        self.events.wait_for(name)
    }
}
