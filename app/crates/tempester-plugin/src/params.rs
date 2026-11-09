//! The parameters shown by the host: one per catalogue slot, plus the plugin's own controls.

use std::collections::BTreeMap;
use std::sync::Arc;

use nih_plug::prelude::*;

use crate::catalogue::Catalogue;
use crate::engine::Controls;
use crate::mapping::CcMap;

/// Key of the learned mapping in the plugin state.
const MAPPING_KEY: &str = "cc-map";

pub struct TempesterParams {
    /// Output MIDI channel, 1-16: the Tempest's Remote Pad IN channel.
    pub channel: IntParam,
    /// While on, touch a parameter in the host, then move a controller: the controller is mapped to it.
    pub learn: BoolParam,
    /// Follow the machine's own NRPN and beat-wide CCs arriving on the input.
    pub follow: BoolParam,
    /// Send every parameter now (acts when switched on).
    pub send_all: BoolParam,
    /// Forget every learned controller (acts when switched on).
    pub clear_map: BoolParam,
    /// Send every parameter when the plugin is activated.
    pub send_on_load: BoolParam,
    /// One per catalogue slot, same order.
    pub sounds: Vec<IntParam>,
    pub catalogue: Arc<Catalogue>,
    pub map: Arc<CcMap>,
}

impl TempesterParams {
    pub fn new(catalogue: Arc<Catalogue>, map: Arc<CcMap>) -> Self {
        let sounds = catalogue
            .slots()
            .iter()
            .map(|slot| {
                IntParam::new(
                    slot.name.clone(),
                    slot.default,
                    IntRange::Linear {
                        min: slot.min,
                        max: slot.max,
                    },
                )
            })
            .collect();
        Self {
            channel: IntParam::new("MIDI Channel", 1, IntRange::Linear { min: 1, max: 16 }),
            learn: BoolParam::new("MIDI Learn", false),
            follow: BoolParam::new("Follow Machine", false),
            send_all: BoolParam::new("Send All", false),
            clear_map: BoolParam::new("Clear Mappings", false),
            send_on_load: BoolParam::new("Send All On Load", false),
            sounds,
            catalogue,
            map,
        }
    }

    /// The non-sound parameters as the core reads them.
    pub fn controls(&self) -> Controls {
        Controls {
            channel: u8::try_from(self.channel.value() - 1).unwrap_or(0).min(15),
            learn: self.learn.value(),
            follow: self.follow.value(),
            send_all: self.send_all.value(),
            clear_map: self.clear_map.value(),
            send_on_load: self.send_on_load.value(),
        }
    }

    /// Copies the sound values into `host` (same length as the catalogue); no allocation.
    pub fn read_sounds(&self, host: &mut [i32]) {
        for (value, param) in host.iter_mut().zip(&self.sounds) {
            *value = param.value();
        }
    }
}

// SAFETY: every `ParamPtr` returned by `param_map` points into a field of `self` (or into its `sounds` vector, which
// is never resized after construction). The wrapper keeps the `Arc<dyn Params>` it got from `Plugin::params` alive
// for as long as it uses the pointers, and `self` is never moved out of that `Arc`.
unsafe impl Params for TempesterParams {
    fn param_map(&self) -> Vec<(String, ParamPtr, String)> {
        let mut map = Vec::with_capacity(self.sounds.len() + 6);
        let control = |id: &str, ptr: ParamPtr| (id.to_string(), ptr, "Control".to_string());
        map.push(control("chan", self.channel.as_ptr()));
        map.push(control("learn", self.learn.as_ptr()));
        map.push(control("follow", self.follow.as_ptr()));
        map.push(control("sendall", self.send_all.as_ptr()));
        map.push(control("clrmap", self.clear_map.as_ptr()));
        map.push(control("onload", self.send_on_load.as_ptr()));
        for (slot, param) in self.catalogue.slots().iter().zip(&self.sounds) {
            map.push((slot.id.clone(), param.as_ptr(), slot.group.clone()));
        }
        map
    }

    fn serialize_fields(&self) -> BTreeMap<String, String> {
        BTreeMap::from([(MAPPING_KEY.to_string(), self.map.to_text(&self.catalogue))])
    }

    fn deserialize_fields(&self, serialized: &BTreeMap<String, String>) {
        if let Some(text) = serialized.get(MAPPING_KEY) {
            self.map.load_text(text, &self.catalogue);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params() -> TempesterParams {
        TempesterParams::new(Arc::new(Catalogue::build()), Arc::new(CcMap::new()))
    }

    #[test]
    fn test_param_map_when_built_exposes_every_slot_with_unique_ids() {
        let params = params();

        let map = params.param_map();

        let ids: std::collections::HashSet<_> = map.iter().map(|(id, _, _)| id.clone()).collect();
        assert_eq!(map.len(), params.catalogue.len() + 6);
        assert_eq!(ids.len(), map.len());
    }

    #[test]
    fn test_sounds_when_built_carry_table_names_ranges_and_defaults() {
        let params = params();

        let lp_freq = params.catalogue.index_of_id("n34").unwrap_or(0);
        let param = &params.sounds[lp_freq];

        assert_eq!(param.name(), "Low-pass filter LP Freq #34");
        assert_eq!(param.value(), 164);
        assert_eq!(param.default_plain_value(), 164);
        assert_eq!(params.sounds.len(), params.catalogue.len());
    }

    #[test]
    fn test_read_sounds_when_defaults_equal_the_catalogue_defaults() {
        let params = params();
        let mut host = vec![-1; params.catalogue.len()];

        params.read_sounds(&mut host);

        let defaults: Vec<i32> = params.catalogue.slots().iter().map(|s| s.default).collect();
        assert_eq!(host, defaults);
    }

    #[test]
    fn test_controls_when_channel_param_is_1_to_16_returns_zero_based_channel() {
        let params = params();
        let first = params.controls().channel;
        let setter = params.channel.value();

        assert_eq!((first, setter), (0, 1));
    }

    #[test]
    fn test_state_when_saved_and_restored_into_a_new_instance_the_mapping_is_identical() {
        let params = params();
        let slot = params.catalogue.index_of_id("n34").unwrap_or(0);
        params.map.set(74, slot);
        params
            .map
            .set(1, params.catalogue.index_of_id("vol").unwrap_or(0));
        let saved = params.serialize_fields();

        let restored = self::params();
        restored.deserialize_fields(&saved);

        assert_eq!(restored.map.get(74), Some(slot));
        assert_eq!(restored.serialize_fields(), saved);
        // counter-proof: an instance that did not load the state has no mapping
        assert_eq!(self::params().map.count(), 0);
        assert_ne!(self::params().serialize_fields(), saved);
    }

    #[test]
    fn test_state_when_the_saved_key_is_missing_the_mapping_is_left_alone() {
        let params = params();
        params.map.set(74, 3);

        params.deserialize_fields(&BTreeMap::new());

        assert_eq!(params.map.get(74), Some(3));
    }
}
