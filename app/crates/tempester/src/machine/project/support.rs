//! Test helpers for the project and library tests: a project now waits for the user's choice before anything is
//! written, so the tests that look at the files play that choice (the defaults: write the new, skip the identical,
//! update the modified).

use super::*;
use crate::library::command::LibCommand;
use crate::library::plan::Policy;
use crate::machine::testkit::TestRig;

/// Applies the choice the review dialog offers by default, if a review is waiting.
pub(in crate::machine) fn accept_review(rig: &mut TestRig) {
    if rig.machine.plan.is_some() {
        rig.machine
            .handle(Command::Lib(LibCommand::Apply(Policy::default())));
    }
}

/// Plans `project` and applies the default choice, as clicking « Apply » on the review would.
pub(in crate::machine) fn save_project_now(rig: &mut TestRig, project: &Project, stamp: &str) {
    rig.machine
        .plan_project(project, stamp)
        .expect("the project is planned");
    accept_review(rig);
}

/// A rig that has received and saved the fake machine's project, a sound and a beat.
pub(in crate::machine) fn rig_with_everything(label: &str) -> TestRig {
    let mut rig = TestRig::connected(label);
    rig.machine.handle(Command::FetchSound);
    rig.machine.handle(Command::SaveSound("Kick".into()));
    rig.machine.handle(Command::FetchBeat);
    rig.machine.handle(Command::SaveBeat("Groove".into()));
    rig.machine.handle(Command::ReceiveProject);
    for message in fake_machine::project_messages() {
        rig.transport().push_incoming(&message);
    }
    rig.machine.poll_incoming();
    accept_review(&mut rig);
    rig.events();
    rig
}
