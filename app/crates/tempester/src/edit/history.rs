//! Edit history of the current sound: what the app believes the machine's selected pad holds, the steps that led
//! there, undo and redo. A step keeps, for each parameter it changed, the value before and after, so that undoing
//! it restores only those parameters (a knob the machine reported in between is left alone).
//!
//! A knob drag is one step: values of the same parameter arriving less than `GESTURE_GAP` apart are merged into the
//! step they continue. The history keeps at most `HISTORY_LIMIT` steps (the oldest go first).

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use tempest_proto::sound::{params, PARAM_COUNT};

/// The 127 values of a sound, in parameter order.
pub type Values = [u16; PARAM_COUNT];
/// One parameter set to one value.
pub type Change = (u8, u16);

/// Steps kept for undo.
pub const HISTORY_LIMIT: usize = 100;
/// The longest pause inside one knob gesture: the page sends a dragged knob's values at least every 30 ms.
pub const GESTURE_GAP: Duration = Duration::from_millis(600);

/// What made a step (its label for the page).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepKind {
    /// A knob of the page, or a learned controller driving it.
    Knob(u8),
    /// A recalled A or B snapshot.
    Recall(char),
    /// A generated variation, with its seed.
    Variation(u32),
}

impl StepKind {
    /// A short id the page turns into words: `n34` (the knob of parameter 34, named by the page's labels),
    /// `recall A`, `variation 4242`.
    pub fn label(self) -> String {
        match self {
            Self::Knob(index) => format!("n{index}"),
            Self::Recall(slot) => format!("recall {slot}"),
            Self::Variation(seed) => format!("variation {seed}"),
        }
    }
}

#[derive(Debug, Clone)]
struct Step {
    kind: StepKind,
    /// (index, before, after) of each parameter the step changed.
    changes: Vec<(u8, u16, u16)>,
    at: Instant,
    /// A knob gesture that may still continue (closed by a report of the machine or by a redo).
    open: bool,
}

/// The current sound and its steps.
#[derive(Debug, Clone)]
pub struct History {
    values: Values,
    undo: VecDeque<Step>,
    redo: Vec<Step>,
}

/// The largest legal value of parameter `index` (the NRPN table: NRPN n = sound parameter n).
pub fn max_of(index: u8) -> u16 {
    params().get(usize::from(index)).map_or(0, |p| p.max)
}

/// `value` brought into the legal range of parameter `index`.
pub fn clamp(index: u8, value: u16) -> u16 {
    value.min(max_of(index))
}

impl History {
    /// A history starting from `values` (each clamped to its range), with no step.
    pub fn new(values: &Values) -> Self {
        let mut clean = *values;
        for (index, value) in (0u8..).zip(clean.iter_mut()) {
            *value = clamp(index, *value);
        }
        Self {
            values: clean,
            undo: VecDeque::new(),
            redo: Vec::new(),
        }
    }

    pub fn values(&self) -> &Values {
        &self.values
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Steps that can be undone.
    pub fn depth(&self) -> usize {
        self.undo.len()
    }

    /// The label of the step the next undo would revert.
    pub fn last_label(&self) -> Option<String> {
        self.undo.back().map(|step| step.kind.label())
    }

    /// The label of the step the next redo would apply again.
    pub fn next_label(&self) -> Option<String> {
        self.redo.last().map(|step| step.kind.label())
    }

    /// A knob of the page moved parameter `index` to `value` (clamped). Continues the last step when it is the same
    /// knob's gesture, otherwise starts a step. Returns false when nothing changed (unknown parameter or same value).
    pub fn knob(&mut self, index: u8, value: u16, now: Instant) -> bool {
        if usize::from(index) >= PARAM_COUNT {
            return false;
        }
        let value = clamp(index, value);
        let before = self.values[usize::from(index)];
        if value == before {
            return false;
        }
        self.values[usize::from(index)] = value;
        self.redo.clear();
        if let Some(step) = self.undo.back_mut().filter(|step| {
            step.open
                && step.kind == StepKind::Knob(index)
                && now.saturating_duration_since(step.at) <= GESTURE_GAP
        }) {
            step.at = now;
            if let Some(change) = step.changes.first_mut() {
                change.2 = value;
            }
            if step
                .changes
                .iter()
                .all(|(_, before, after)| before == after)
            {
                self.undo.pop_back();
            }
            return true;
        }
        self.push(Step {
            kind: StepKind::Knob(index),
            changes: vec![(index, before, value)],
            at: now,
            open: true,
        });
        true
    }

    /// The machine reported parameter `index` (a knob turned on it): the model follows, no step is made, and a knob
    /// gesture of the page on that parameter ends there.
    pub fn observe(&mut self, index: u8, value: u16) {
        if let Some(slot) = self.values.get_mut(usize::from(index)) {
            *slot = clamp(index, value);
            if let Some(step) = self.undo.back_mut() {
                if step.kind == StepKind::Knob(index) {
                    step.open = false;
                }
            }
        }
    }

    /// Moves the whole sound to `target` (clamped) as one step. Returns the changes to send (only the parameters that
    /// differ); empty, and no step, when the sound is already there.
    pub fn jump(&mut self, target: &Values, kind: StepKind, now: Instant) -> Vec<Change> {
        let changes: Vec<(u8, u16, u16)> = (0u8..)
            .zip(target.iter())
            .map(|(index, &value)| (index, self.values[usize::from(index)], clamp(index, value)))
            .filter(|(_, before, after)| before != after)
            .collect();
        if changes.is_empty() {
            return Vec::new();
        }
        let out = changes
            .iter()
            .map(|&(index, _, after)| (index, after))
            .collect();
        for &(index, _, after) in &changes {
            self.values[usize::from(index)] = after;
        }
        self.redo.clear();
        self.push(Step {
            kind,
            changes,
            at: now,
            open: false,
        });
        out
    }

    /// Reverts the last step; returns the changes to send, or `None` when there is nothing to undo.
    pub fn undo(&mut self) -> Option<Vec<Change>> {
        let step = self.undo.pop_back()?;
        let out: Vec<Change> = step
            .changes
            .iter()
            .map(|&(index, before, _)| (index, before))
            .collect();
        self.apply(&out);
        self.redo.push(step);
        Some(out)
    }

    /// Applies again the last undone step; returns the changes to send, or `None` when there is nothing to redo.
    pub fn redo(&mut self) -> Option<Vec<Change>> {
        let mut step = self.redo.pop()?;
        let out: Vec<Change> = step
            .changes
            .iter()
            .map(|&(index, _, after)| (index, after))
            .collect();
        self.apply(&out);
        // A redone knob step must not swallow the next drag of that knob.
        step.open = false;
        self.undo.push_back(step);
        Some(out)
    }

    fn apply(&mut self, changes: &[Change]) {
        for &(index, value) in changes {
            self.values[usize::from(index)] = value;
        }
    }

    fn push(&mut self, step: Step) {
        self.undo.push_back(step);
        while self.undo.len() > HISTORY_LIMIT {
            self.undo.pop_front();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempest_proto::sound::Sound;

    const LP_FREQ: u8 = 34;
    const RESONANCE: u8 = 35;

    fn defaults() -> Values {
        Sound::with_defaults().values()
    }

    fn ms(start: Instant, millis: u64) -> Instant {
        start + Duration::from_millis(millis)
    }

    #[test]
    fn test_knob_when_a_drag_sends_many_values_returns_one_step_undone_at_once() {
        let mut history = History::new(&defaults());
        let start = Instant::now();
        let first = history.values()[34];

        for (k, value) in [100u16, 110, 120, 130, 140].into_iter().enumerate() {
            history.knob(LP_FREQ, value, ms(start, 30 * k as u64));
        }

        assert_eq!(history.depth(), 1, "one drag = one step");
        assert_eq!(history.undo(), Some(vec![(LP_FREQ, first)]));
        assert_eq!(history.values()[34], first);
        assert!(!history.can_undo());
    }

    #[test]
    fn test_knob_when_a_pause_or_another_knob_comes_between_returns_separate_steps() {
        // counter-proof of the coalescing: a pause longer than the gap, or another knob, starts a new step
        let mut history = History::new(&defaults());
        let start = Instant::now();

        history.knob(LP_FREQ, 100, start);
        history.knob(LP_FREQ, 110, start + GESTURE_GAP + Duration::from_millis(1));
        history.knob(
            RESONANCE,
            20,
            start + GESTURE_GAP + Duration::from_millis(2),
        );
        history.knob(LP_FREQ, 90, start + GESTURE_GAP + Duration::from_millis(3));

        assert_eq!(history.depth(), 4);
        assert_eq!(history.undo(), Some(vec![(LP_FREQ, 110)]));
        assert_eq!(history.undo(), Some(vec![(RESONANCE, 0)]));
        assert_eq!(history.undo(), Some(vec![(LP_FREQ, 100)]));
    }

    #[test]
    fn test_knob_when_a_drag_comes_back_to_where_it_started_leaves_no_step() {
        let mut history = History::new(&defaults());
        let start = Instant::now();
        let first = history.values()[34];

        history.knob(LP_FREQ, 100, start);
        history.knob(LP_FREQ, first, ms(start, 30));

        assert!(!history.can_undo());
    }

    #[test]
    fn test_undo_then_redo_when_steps_exist_returns_the_values_of_each_side() {
        let mut history = History::new(&defaults());
        let start = Instant::now();
        history.knob(LP_FREQ, 100, start);
        history.knob(RESONANCE, 50, ms(start, 1000));

        assert_eq!(history.undo(), Some(vec![(RESONANCE, 0)]));
        assert!(history.can_redo());
        assert_eq!(history.redo(), Some(vec![(RESONANCE, 50)]));
        assert_eq!(history.values()[35], 50);
        assert_eq!(history.redo(), None);
    }

    #[test]
    fn test_knob_after_an_undo_returns_redo_forgotten() {
        let mut history = History::new(&defaults());
        let start = Instant::now();
        history.knob(LP_FREQ, 100, start);
        history.undo();

        history.knob(RESONANCE, 9, ms(start, 2000));

        assert!(!history.can_redo());
    }

    #[test]
    fn test_redo_then_drag_of_the_same_knob_returns_a_new_step() {
        let mut history = History::new(&defaults());
        let start = Instant::now();
        history.knob(LP_FREQ, 100, start);
        history.undo();
        history.redo();

        history.knob(LP_FREQ, 120, ms(start, 10));

        assert_eq!(history.depth(), 2);
    }

    #[test]
    fn test_push_when_more_than_the_limit_returns_the_oldest_dropped() {
        let mut history = History::new(&defaults());
        let start = Instant::now();

        for k in 0..(HISTORY_LIMIT as u64 + 20) {
            history.knob(RESONANCE, (k % 100) as u16 + 1, ms(start, k * 1000));
        }

        assert_eq!(history.depth(), HISTORY_LIMIT);
        let mut undone = 0;
        while history.undo().is_some() {
            undone += 1;
        }
        assert_eq!(undone, HISTORY_LIMIT);
        assert_ne!(
            history.values()[35],
            0,
            "the oldest steps are gone, so is the way back to 0"
        );
    }

    #[test]
    fn test_knob_and_jump_when_values_above_the_range_returns_them_clamped() {
        let mut start = defaults();
        start[34] = 100;
        start[40] = 0;
        let mut history = History::new(&start);
        let mut target = start;
        target[34] = 9999;
        target[40] = 7;

        let sent = history.jump(&target, StepKind::Recall('A'), Instant::now());
        history.knob(RESONANCE, 500, Instant::now());

        assert_eq!(sent, vec![(34, 164), (40, 1)]);
        assert_eq!(history.values()[35], 127);
        assert_eq!(clamp(34, 165), 164);
        // counter-proof: a legal value is left as it is
        assert_eq!(clamp(34, 164), 164);
    }

    #[test]
    fn test_jump_when_target_differs_returns_only_the_changed_parameters_and_one_step() {
        let mut history = History::new(&defaults());
        let mut target = defaults();
        target[0] = 50;
        target[72] = 99;

        let sent = history.jump(&target, StepKind::Variation(7), Instant::now());

        assert_eq!(sent, vec![(0, 50), (72, 99)]);
        assert_eq!(history.last_label().as_deref(), Some("variation 7"));
        assert_eq!(history.undo().map(|c| c.len()), Some(2));
        assert_eq!(history.values(), &defaults());
        assert!(history
            .jump(&defaults(), StepKind::Recall('B'), Instant::now())
            .is_empty());
    }

    #[test]
    fn test_observe_when_machine_reports_returns_model_updated_without_a_step() {
        let mut history = History::new(&defaults());
        let start = Instant::now();
        history.knob(LP_FREQ, 100, start);

        history.observe(LP_FREQ, 60);
        history.knob(LP_FREQ, 70, ms(start, 40));

        assert_eq!(
            history.depth(),
            2,
            "the machine's report ends the page's gesture"
        );
        assert_eq!(history.undo(), Some(vec![(LP_FREQ, 60)]));
    }
}
