//! The Euclidean patterns against known tables, the probability with a fixed seed, and the note-offs.

use super::*;

fn text(hits: &[bool]) -> String {
    hits.iter().map(|h| if *h { 'x' } else { '.' }).collect()
}

fn pattern(steps: u8, pulses: u8, rotation: u8, probability: u8) -> PadPattern {
    PadPattern::new(steps, pulses, rotation, probability).expect("valid pattern")
}

/// A sequencer on `pads`, enabled, on channel `channel`, with `seed`.
fn sequencer(pads: &[(usize, PadPattern)], channel: u8, seed: u32) -> (Sequencer, Shared) {
    let shared = Shared::default();
    shared.update(|settings| {
        settings.enabled = true;
        settings.channel = channel;
        for (pad, pattern) in pads {
            settings.pads[*pad] = *pattern;
        }
    });
    (Sequencer::new(shared.clone(), seed), shared)
}

/// Every message of `clocks` clock bytes, as `(clock index, message)`.
fn run(sequencer: &mut Sequencer, clocks: u64) -> Vec<(u64, Message)> {
    (0..clocks)
        .flat_map(|at| {
            sequencer
                .on_clock()
                .into_iter()
                .map(move |message| (at, message))
        })
        .collect()
}

fn ons(messages: &[(u64, Message)]) -> Vec<(u64, u8)> {
    messages
        .iter()
        .filter(|(_, m)| m[0] & 0xF0 == 0x90)
        .map(|(at, m)| (*at, m[1]))
        .collect()
}

fn offs(messages: &[(u64, Message)]) -> Vec<(u64, u8)> {
    messages
        .iter()
        .filter(|(_, m)| m[0] & 0xF0 == 0x80)
        .map(|(at, m)| (*at, m[1]))
        .collect()
}

#[test]
fn test_euclid_when_known_tables_return_toussaints_rhythms() {
    // The rhythms of Toussaint, "The Euclidean Algorithm Generates Traditional Musical Rhythms" (2005).
    let table = [
        (3, 8, "x..x..x."),
        (5, 8, "x.xx.xx."),
        (2, 5, "x.x.."),
        (4, 7, "x.x.x.x"),
        (7, 12, "x.xx.x.xx.x."),
        (5, 12, "x..x.x..x.x."),
        (7, 16, "x..x.x.x..x.x.x."),
        (9, 16, "x.xx.x.x.xx.x.x."),
        (4, 16, "x...x...x...x..."),
    ];
    for (pulses, steps, expected) in table {
        assert_eq!(
            text(&euclid(steps, pulses)),
            expected,
            "E({pulses},{steps})"
        );
    }
}

#[test]
fn test_euclid_when_extremes_return_all_rests_or_all_hits() {
    assert_eq!(text(&euclid(8, 0)), "........");
    assert_eq!(text(&euclid(5, 5)), "xxxxx");
    assert_eq!(
        text(&euclid(5, 9)),
        "xxxxx",
        "more pulses than steps count as steps"
    );
    assert_eq!(text(&euclid(6, 1)), "x.....");
}

#[test]
fn test_euclid_when_any_size_returns_exactly_the_pulses_spread_within_one_step_of_even() {
    for steps in 1..=usize::from(MAX_STEPS) {
        for pulses in 0..=steps {
            let hits = euclid(steps, pulses);
            assert_eq!(hits.len(), steps);
            assert_eq!(hits.iter().filter(|h| **h).count(), pulses);
            if pulses < 2 {
                continue;
            }
            let at: Vec<usize> = (0..steps).filter(|i| hits[*i]).collect();
            let gaps: Vec<usize> = (0..pulses)
                .map(|i| (at[(i + 1) % pulses] + steps - at[i]) % steps)
                .map(|gap| if gap == 0 { steps } else { gap })
                .collect();
            let (min, max) = (gaps.iter().min(), gaps.iter().max());
            assert!(
                max.zip(min).is_some_and(|(max, min)| max - min <= 1),
                "E({pulses},{steps}) gaps {gaps:?}"
            );
        }
    }
}

#[test]
fn test_cycle_when_rotated_returns_the_pattern_delayed_by_that_many_steps() {
    assert_eq!(pattern(8, 3, 0, 100).text(), "x..x..x.");
    assert_eq!(pattern(8, 3, 1, 100).text(), ".x..x..x");
    assert_eq!(pattern(8, 3, 3, 100).text(), ".x.x..x.");
    assert_ne!(pattern(8, 3, 1, 100).text(), pattern(8, 3, 0, 100).text());
}

#[test]
fn test_new_when_a_field_is_out_of_range_returns_an_error_naming_it() {
    assert!(PadPattern::new(0, 0, 0, 100).is_err_and(|e| e.contains("steps")));
    assert!(PadPattern::new(33, 0, 0, 100).is_err_and(|e| e.contains("steps")));
    assert!(PadPattern::new(8, 9, 0, 100).is_err_and(|e| e.contains("pulses")));
    assert!(PadPattern::new(8, 3, 8, 100).is_err_and(|e| e.contains("rotation")));
    assert!(PadPattern::new(8, 3, 0, 101).is_err_and(|e| e.contains("probability")));
    assert!(PadPattern::new(32, 32, 31, 0).is_ok());
}

#[test]
fn test_on_clock_when_one_pad_plays_e_4_16_returns_four_notes_on_the_quarters_each_ending_three_clocks_later(
) {
    let (mut sequencer, _) = sequencer(&[(0, pattern(16, 4, 0, 100))], 9, 1);

    let messages = run(&mut sequencer, 96);

    assert_eq!(ons(&messages), [(0, 36), (24, 36), (48, 36), (72, 36)]);
    assert_eq!(offs(&messages), [(3, 36), (27, 36), (51, 36), (75, 36)]);
    assert_eq!(messages[0].1, [0x99, 36, 100], "channel 10, velocity 100");
}

#[test]
fn test_on_clock_when_two_pads_have_different_lengths_returns_each_cycling_on_its_own() {
    let (mut sequencer, _) = sequencer(
        &[(0, pattern(3, 1, 0, 100)), (5, pattern(4, 1, 0, 100))],
        0,
        1,
    );

    let messages = run(&mut sequencer, 6 * 12);

    let steps_of = |note: u8| -> Vec<u64> {
        ons(&messages)
            .into_iter()
            .filter(|(_, n)| *n == note)
            .map(|(at, _)| at / 6)
            .collect()
    };
    assert_eq!(steps_of(36), [0, 3, 6, 9]);
    assert_eq!(
        steps_of(45),
        [0, 4, 8],
        "pad A6 is note 45 on the author's machine"
    );
}

#[test]
fn test_on_clock_when_a_pad_hits_every_step_returns_the_note_off_before_the_next_note_on() {
    let (mut sequencer, _) = sequencer(&[(2, pattern(1, 1, 0, 100))], 0, 1);

    let messages = run(&mut sequencer, 13);

    assert_eq!(
        ons(&messages),
        [(0, 40), (6, 40), (12, 40)],
        "pad A3 is note 40"
    );
    assert_eq!(offs(&messages), [(3, 40), (9, 40)]);
}

#[test]
fn test_on_clock_when_probability_is_0_or_100_returns_no_note_or_every_note() {
    let all = |probability| {
        let (mut sequencer, _) = sequencer(&[(0, pattern(1, 1, 0, probability))], 0, 7);
        ons(&run(&mut sequencer, 6 * 200)).len()
    };

    assert_eq!(all(0), 0);
    assert_eq!(all(100), 200);
}

#[test]
fn test_on_clock_when_probability_is_50_with_a_fixed_seed_returns_the_same_notes_every_time() {
    let steps_played = |seed| {
        let (mut sequencer, _) = sequencer(&[(0, pattern(1, 1, 0, 50))], 0, seed);
        ons(&run(&mut sequencer, 6 * 1000))
            .into_iter()
            .map(|(at, _)| at / 6)
            .collect::<Vec<_>>()
    };

    let first = steps_played(12345);

    assert_eq!(first, steps_played(12345), "same seed, same notes");
    assert_eq!(
        first.len(),
        530,
        "count for seed 12345 over 1000 steps, from an independent xorshift32 in Python"
    );
    assert_eq!(
        &first[..8],
        &[0, 1, 2, 3, 4, 11, 12, 13],
        "first steps, same source"
    );
    // Counter-proof: another seed gives other notes.
    assert_ne!(first, steps_played(54321));
}

#[test]
fn test_on_clock_when_probability_is_25_returns_about_a_quarter_of_the_hits() {
    let (mut sequencer, _) = sequencer(&[(0, pattern(1, 1, 0, 25))], 0, 99);

    let played = ons(&run(&mut sequencer, 6 * 4000)).len();

    assert!((850..=1150).contains(&played), "{played} of 4000");
}

#[test]
fn test_release_all_when_stopped_inside_a_gate_returns_the_note_off_of_every_sounding_note() {
    let (mut sequencer, _) = sequencer(
        &[(0, pattern(1, 1, 0, 100)), (3, pattern(1, 1, 0, 100))],
        4,
        1,
    );
    let heard = run(&mut sequencer, 2);

    let released = sequencer.release_all();

    assert_eq!(ons(&heard).len(), 2);
    assert_eq!(released, vec![[0x84, 36, 0], [0x84, 41, 0]]);
    assert!(
        sequencer.release_all().is_empty(),
        "nothing is released twice"
    );
}

#[test]
fn test_on_clock_when_not_released_at_the_stop_returns_notes_on_without_their_off() {
    // Counter-proof of the note-off guarantee: this is what a stop without `release_all` would leave behind.
    let (mut sequencer, _) = sequencer(&[(0, pattern(1, 1, 0, 100))], 0, 1);

    let heard = run(&mut sequencer, 2);

    assert_eq!((ons(&heard).len(), offs(&heard).len()), (1, 0));
}

#[test]
fn test_on_clock_when_disabled_inside_a_gate_returns_the_pending_note_off_and_no_new_note() {
    let (mut sequencer, shared) = sequencer(&[(0, pattern(1, 1, 0, 100))], 0, 1);
    let first = run(&mut sequencer, 2);
    shared.update(|settings| settings.enabled = false);

    let later: Vec<_> = (2..20)
        .flat_map(|at| sequencer.on_clock().into_iter().map(move |m| (at, m)))
        .collect();

    assert_eq!(ons(&first).len(), 1);
    assert_eq!(ons(&later).len(), 0);
    assert_eq!(offs(&later), [(3, 36)]);
}

#[test]
fn test_on_clock_when_the_channel_changes_inside_a_gate_returns_the_off_on_the_channel_of_the_on() {
    let (mut sequencer, shared) = sequencer(&[(0, pattern(1, 1, 0, 100))], 2, 1);
    let first = run(&mut sequencer, 1);
    shared.update(|settings| settings.channel = 9);

    let rest = run(&mut sequencer, 4);

    assert_eq!(first[0].1[0], 0x92);
    assert_eq!(
        rest.iter()
            .find(|(_, m)| m[0] & 0xF0 == 0x80)
            .map(|(_, m)| m[0]),
        Some(0x82)
    );
}

#[test]
fn test_on_clock_when_the_pattern_changes_while_playing_returns_the_new_pattern_from_the_next_step()
{
    let (mut sequencer, shared) = sequencer(&[(0, pattern(4, 1, 0, 100))], 0, 1);
    let first = run(&mut sequencer, 6 * 4);
    shared.update(|settings| settings.pads[0] = pattern(4, 4, 0, 100));

    let second: Vec<_> = (0..6 * 4).flat_map(|_| sequencer.on_clock()).collect();

    assert_eq!(ons(&first).len(), 1);
    assert_eq!(second.iter().filter(|m| m[0] & 0xF0 == 0x90).count(), 4);
}

#[test]
fn test_on_clock_when_pad_notes_are_set_returns_each_pad_on_its_note_not_36_plus_pad() {
    let (mut sequencer, shared) = sequencer(&[(2, pattern(1, 1, 0, 100))], 0, 1);
    let mut notes = PadNotes::default();
    notes.learn(2, 99);
    shared.update(|settings| settings.notes = notes.bank_a());

    let messages = run(&mut sequencer, 6);

    assert_eq!(ons(&messages), [(0, 99)]);
    assert_eq!(offs(&messages), [(3, 99)]);
}
