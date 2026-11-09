//! Variations of a sound: a group of parameters each moved at random by up to `amount` % of its range, kept inside
//! its legal range (the parameter table, which is the NRPN table: NRPN n = sound parameter n). The random numbers come
//! from a seed (SplitMix64, no dependency), so the same sound, group, amount and seed always give the same variation.
//!
//! Only continuous parameters are in the groups: a shape, a sample, a modulation source or destination, a switch
//! moved "a little" is not a small change, so they are never touched.

use super::history::{max_of, Values};

/// Smallest and largest amount, in percent of each parameter's range.
pub const AMOUNT_MIN: u8 = 1;
pub const AMOUNT_MAX: u8 = 100;

/// A group of parameters to vary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    /// Oscillator frequencies, fine tunings and glides; the pitch envelope's amounts.
    Pitch,
    /// Delay, attack, decay, sustain and release of the five envelopes.
    Env,
    /// Low-pass and high-pass filters (not the 2/4-pole switch).
    Filter,
    /// LFO rates and amounts, the auxiliary envelopes' amounts, the 8 modulation-matrix amounts.
    Mod,
    /// The four groups together.
    All,
}

const PITCH: [u8; 14] = [0, 1, 3, 6, 7, 9, 12, 13, 16, 18, 19, 22, 59, 60];
const ENV: [u8; 25] = [
    61, 62, 63, 64, 65, 66, 67, 68, 69, 70, 71, 72, 73, 74, 75, 79, 80, 81, 82, 83, 87, 88, 89, 90,
    91,
];
const FILTER: [u8; 8] = [34, 35, 36, 37, 38, 39, 41, 42];
const MOD: [u8; 16] = [
    47, 49, 52, 54, 77, 78, 85, 86, 93, 96, 99, 102, 105, 108, 111, 114,
];

impl Group {
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "pitch" => Some(Self::Pitch),
            "env" => Some(Self::Env),
            "filter" => Some(Self::Filter),
            "mod" => Some(Self::Mod),
            "all" => Some(Self::All),
            _ => None,
        }
    }

    /// The parameters of the group, in index order.
    pub fn params(self) -> Vec<u8> {
        let mut out: Vec<u8> = match self {
            Self::Pitch => PITCH.to_vec(),
            Self::Env => ENV.to_vec(),
            Self::Filter => FILTER.to_vec(),
            Self::Mod => MOD.to_vec(),
            Self::All => [&PITCH[..], &ENV[..], &FILTER[..], &MOD[..]].concat(),
        };
        out.sort_unstable();
        out.dedup();
        out
    }
}

/// SplitMix64: a tiny, well-mixed generator; enough for musical randomness, not for anything secret.
struct SplitMix(u64);

impl SplitMix {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A whole number from `-span` to `span`, every value equally likely.
    fn spread(&mut self, span: u32) -> i64 {
        let width = u64::from(span) * 2 + 1;
        i64::try_from(self.next() % width).unwrap_or(0) - i64::from(span)
    }
}

/// `base` with every parameter of `group` moved by a random step of at most `amount` % of its range (rounded to the
/// nearest whole value), clamped to 0..=max. Same inputs, same result.
pub fn vary(base: &Values, group: Group, amount: u8, seed: u32) -> Values {
    let amount = u32::from(amount.clamp(AMOUNT_MIN, AMOUNT_MAX));
    let mut rng = SplitMix(u64::from(seed));
    let mut out = *base;
    for index in group.params() {
        let max = max_of(index);
        let span = (u32::from(max) * amount + 50) / 100;
        let step = rng.spread(span);
        let slot = &mut out[usize::from(index)];
        let moved = (i64::from(*slot) + step).clamp(0, i64::from(max));
        *slot = u16::try_from(moved).unwrap_or(max);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempest_proto::sound::{params, Sound};

    fn base() -> Values {
        Sound::with_defaults().values()
    }

    #[test]
    fn test_vary_when_same_seed_returns_the_same_sound_and_another_seed_a_different_one() {
        let first = vary(&base(), Group::All, 40, 1234);

        assert_eq!(vary(&base(), Group::All, 40, 1234), first);
        // counter-proof: the seed matters
        assert_ne!(vary(&base(), Group::All, 40, 1235), first);
    }

    #[test]
    fn test_vary_when_any_group_returns_only_its_parameters_changed() {
        for group in [Group::Pitch, Group::Env, Group::Filter, Group::Mod] {
            let inside = group.params();
            let varied = vary(&base(), group, 100, 99);

            for (index, (a, b)) in base().iter().zip(varied.iter()).enumerate() {
                if !inside.contains(&(index as u8)) {
                    assert_eq!(a, b, "{group:?} moved parameter {index}");
                }
            }
            assert_ne!(varied, base(), "{group:?} moved nothing");
        }
    }

    #[test]
    fn test_vary_when_amount_is_full_on_extremes_returns_every_value_in_its_range() {
        let mut top = base();
        for p in params() {
            top[usize::from(p.index)] = p.max;
        }
        let bottom = [0u16; tempest_proto::sound::PARAM_COUNT];

        for seed in 0..200 {
            for start in [&top, &bottom] {
                let varied = vary(start, Group::All, 100, seed);
                for p in params() {
                    assert!(
                        varied[usize::from(p.index)] <= p.max,
                        "seed {seed} {}",
                        p.index
                    );
                }
            }
        }
    }

    #[test]
    fn test_vary_when_amount_is_small_returns_steps_within_that_share_of_the_range() {
        for seed in 0..100 {
            let varied = vary(&base(), Group::All, 10, seed);
            for index in Group::All.params() {
                let max = i32::from(max_of(index));
                let step = (i32::from(varied[usize::from(index)])
                    - i32::from(base()[usize::from(index)]))
                .abs();
                assert!(
                    step <= (max * 10 + 50) / 100,
                    "seed {seed} param {index}: {step}"
                );
            }
        }
    }

    #[test]
    fn test_groups_when_listed_returns_only_continuous_parameters() {
        // A switch (max 1), a shape, a sample or a modulation source/destination is never varied.
        let excluded = [
            2u8, 4, 5, 8, 14, 15, 20, 21, 24, 25, 40, 48, 50, 53, 55, 57, 58, 76, 84, 92, 94, 123,
        ];
        for index in Group::All.params() {
            assert!(!excluded.contains(&index), "{index}");
            assert!(max_of(index) > 1, "{index}");
        }
        assert_eq!(Group::parse("filter"), Some(Group::Filter));
        assert_eq!(Group::parse("amp"), None);
    }
}
