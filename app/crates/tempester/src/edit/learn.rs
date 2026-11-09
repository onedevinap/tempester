//! MIDI learn: which Control Change of an external controller drives which control of the Panel page. The idea of
//! the plugin's table (`tempester-plugin/src/mapping.rs`): controllers that can never be learned, a text form saved
//! by target id (not by position), bad entries skipped on load. Here one target has one controller: learning a new
//! one for it replaces the old one.
//!
//! A learned value (0-127) is scaled to the target's range; the page then moves the control and sends it exactly as
//! if the user had turned it (same NRPN, same rate limit).

use std::collections::BTreeMap;

use super::history::max_of;

/// Settings key of the saved table (`"21=n34,22=distort"`).
pub const KEY_LEARN_MAP: &str = "learn_map";
/// Settings key of the controller's input port.
pub const KEY_LEARN_PORT: &str = "learn_port";

/// Controllers that can never be learned: bank select, the NRPN / RPN family (the Tempest's own protocol) and the
/// channel-mode messages (120-127). The same rule as the plugin.
pub fn is_learnable(controller: u8) -> bool {
    !matches!(controller, 0 | 6 | 32 | 38 | 98..=101) && controller < 120
}

/// A control of the Panel page that a controller can drive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Target {
    /// Sound parameter 0-126 (sent as the NRPN of the same number).
    Param(u8),
    /// A knob that is not a sound parameter.
    Local(Local),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Local {
    Distort,
    Compress,
    Volume,
    Bpm,
}

/// The BPM knob's range (the app's clock).
const BPM_MIN: u16 = 20;
const BPM_MAX: u16 = 300;

impl Target {
    /// `n34`, `distort`, `compress`, `volume`, `bpm`.
    pub fn parse(id: &str) -> Option<Self> {
        match id {
            "distort" => Some(Self::Local(Local::Distort)),
            "compress" => Some(Self::Local(Local::Compress)),
            "volume" => Some(Self::Local(Local::Volume)),
            "bpm" => Some(Self::Local(Local::Bpm)),
            _ => {
                let index = id.strip_prefix('n')?.parse::<u8>().ok()?;
                (usize::from(index) < tempest_proto::sound::PARAM_COUNT)
                    .then_some(Self::Param(index))
            }
        }
    }

    pub fn id(self) -> String {
        match self {
            Self::Param(index) => format!("n{index}"),
            Self::Local(Local::Distort) => "distort".into(),
            Self::Local(Local::Compress) => "compress".into(),
            Self::Local(Local::Volume) => "volume".into(),
            Self::Local(Local::Bpm) => "bpm".into(),
        }
    }

    /// A controller value 0-127 brought to the target's range, ends on ends (0 -> min, 127 -> max), rounded.
    pub fn scale(self, value: u8) -> u16 {
        let value = u32::from(value.min(127));
        let (min, max) = match self {
            Self::Param(index) => (0, u32::from(max_of(index))),
            Self::Local(Local::Bpm) => (u32::from(BPM_MIN), u32::from(BPM_MAX)),
            Self::Local(_) => (0, 127),
        };
        let scaled = min + ((max - min) * value + 63) / 127;
        u16::try_from(scaled).unwrap_or(u16::MAX)
    }
}

/// `controller -> target`, at most one controller per target.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LearnMap {
    map: BTreeMap<u8, Target>,
}

impl LearnMap {
    pub fn get(&self, controller: u8) -> Option<Target> {
        self.map.get(&controller).copied()
    }

    /// Maps `controller` to `target`, forgetting the target's previous controller. False (nothing changed) when the
    /// controller cannot be learned.
    pub fn set(&mut self, controller: u8, target: Target) -> bool {
        if !is_learnable(controller) {
            return false;
        }
        self.map.retain(|_, t| *t != target);
        self.map.insert(controller, target);
        true
    }

    /// Forgets the controller of `target`; false when it had none.
    pub fn clear_target(&mut self, target: Target) -> bool {
        let before = self.map.len();
        self.map.retain(|_, t| *t != target);
        self.map.len() != before
    }

    pub fn clear(&mut self) {
        self.map.clear();
    }

    /// `(controller, target id)` in controller order.
    pub fn entries(&self) -> Vec<(u8, String)> {
        self.map.iter().map(|(c, t)| (*c, t.id())).collect()
    }

    /// The saved form: `controller=target id`, comma separated, in controller order.
    pub fn render(&self) -> String {
        self.entries()
            .iter()
            .map(|(c, id)| format!("{c}={id}"))
            .collect::<Vec<_>>()
            .join(",")
    }

    /// A table read from its saved form. Malformed entries, unknown targets and unlearnable controllers are skipped
    /// (a settings file written by another version must still load).
    pub fn parse(text: &str) -> Self {
        let mut map = Self::default();
        for entry in text.split(',') {
            let Some((controller, id)) = entry.split_once('=') else {
                continue;
            };
            let controller = controller.trim().parse::<u8>().ok();
            if let (Some(controller), Some(target)) = (controller, Target::parse(id.trim())) {
                map.set(controller, target);
            }
        }
        map
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_when_learnable_maps_and_a_second_controller_for_the_target_replaces_the_first() {
        let mut map = LearnMap::default();

        assert!(map.set(21, Target::Param(34)));
        assert!(map.set(22, Target::Param(34)));

        assert_eq!(map.get(21), None);
        assert_eq!(map.get(22), Some(Target::Param(34)));
        assert_eq!(map.entries().len(), 1);
    }

    #[test]
    fn test_set_when_reserved_controller_refuses_and_stores_nothing() {
        let mut map = LearnMap::default();

        for controller in [0, 6, 32, 38, 98, 99, 100, 101, 120, 127] {
            assert!(!map.set(controller, Target::Param(1)), "CC {controller}");
        }
        assert_eq!(map.entries().len(), 0);
        // counter-proof: the neighbours of the reserved ones are accepted
        assert!(map.set(1, Target::Param(1)) && map.set(97, Target::Param(2)));
        assert!(map.set(102, Target::Param(3)) && map.set(119, Target::Param(4)));
    }

    #[test]
    fn test_render_then_parse_when_round_trip_returns_the_same_table() {
        let mut map = LearnMap::default();
        map.set(74, Target::Param(34));
        map.set(1, Target::Local(Local::Distort));
        map.set(11, Target::Local(Local::Bpm));

        let text = map.render();

        assert_eq!(text, "1=distort,11=bpm,74=n34");
        assert_eq!(LearnMap::parse(&text), map);
        // counter-proof: an altered text gives another table
        assert_ne!(LearnMap::parse(&text.replace("n34", "n35")), map);
    }

    #[test]
    fn test_parse_when_garbage_skips_bad_entries_and_keeps_good_ones() {
        let map =
            LearnMap::parse("74=n34,,x=n1,300=n1,20=nope,6=n1,=,21 = volume, 22=,23=n127,24=n126");

        assert_eq!(map.render(), "21=volume,24=n126,74=n34");
    }

    #[test]
    fn test_clear_target_when_mapped_returns_true_and_forgets_only_it() {
        let mut map = LearnMap::parse("21=n34,22=n35");

        assert!(map.clear_target(Target::Param(34)));
        assert!(!map.clear_target(Target::Param(34)));

        assert_eq!(map.render(), "22=n35");
        map.clear();
        assert_eq!(map.entries().len(), 0);
    }

    #[test]
    fn test_scale_when_ends_and_middle_returns_the_target_range() {
        let lp_freq = Target::Param(34); // 0-164
        let switch = Target::Param(40); // 0-1
        let bpm = Target::Local(Local::Bpm);

        assert_eq!((lp_freq.scale(0), lp_freq.scale(127)), (0, 164));
        assert_eq!(lp_freq.scale(64), 83);
        assert_eq!((switch.scale(63), switch.scale(64)), (0, 1));
        assert_eq!((bpm.scale(0), bpm.scale(127)), (20, 300));
        assert_eq!(Target::Local(Local::Volume).scale(100), 100);
        // a value above 127 is read as 127
        assert_eq!(lp_freq.scale(200), 164);
    }

    #[test]
    fn test_target_id_when_round_trip_returns_the_same_target() {
        for id in ["n0", "n126", "distort", "compress", "volume", "bpm"] {
            assert_eq!(Target::parse(id).map(Target::id).as_deref(), Some(id));
        }
        for bad in ["n127", "n", "x34", "", "Distort"] {
            assert_eq!(Target::parse(bad), None, "{bad}");
        }
    }
}
