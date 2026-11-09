//! The parameters the plugin exposes to the host, and the MIDI bytes each one becomes.
//!
//! Built once from the `tempest-proto` parameter table (the same table as the app), plus the beat-wide positions,
//! Distort / Compress and Volume. Nothing here runs on the audio thread except the read-only lookups.

use std::collections::HashMap;

use tempest_proto::{nrpn, remote, sound};

/// First CC of the 8 beat-wide positions (CC 20-27); the optional LSB is `CC 52-59`
/// (`Docs/analyses/feature-beat-wide-emission.md` section 4).
pub const BEAT_WIDE_MSB_CC: u8 = 20;
pub const BEAT_WIDE_LSB_CC: u8 = 52;
pub const BEAT_WIDE_COUNT: u8 = 8;
/// Beat-wide positions are 0-254 (neutral 127).
pub const BEAT_WIDE_MAX: i32 = 254;
const BEAT_WIDE_NAMES: [&str; 8] = [
    "All Osc Freq",
    "Feedback",
    "LP Freq",
    "Resonance",
    "Audio Mod",
    "HP Freq",
    "Attack",
    "Decay",
];
pub const CC_DISTORT: u8 = 12;
pub const CC_COMPRESS: u8 = 13;
/// NRPN numbers whose application makes the machine wait for the SAM chip with interrupts off (parameters 12-15,
/// 18-21 and the 14-bit samples 156-157, `Docs/analyses/feature-nrpn.md` section 4): they get a longer minimum spacing.
const SAM_NRPN: [u16; 10] = [12, 13, 14, 15, 18, 19, 20, 21, 156, 157];
const SAM_MIN_INTERVAL_MS: u32 = 50;
/// Osc 3 and Osc 4 sample, 0-463 (a 14-bit pair of the table's fields 14/15 and 20/21).
const SAMPLE_MAX: i32 = 463;
const SAMPLE_NAMES: [(u16, &str); 2] = [(156, "Osc 3"), (157, "Osc 4")];

/// What a slot sends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// NRPN `number` (sound parameter, 0-126 or 156-157), the value as is.
    Nrpn(u16),
    /// Beat-wide position `0-7`: `CC 20+i` = p >> 1, then `CC 52+i` = 0x40 when p is odd.
    BeatWide(u8),
    /// A plain 7-bit Control Change (Distort 12, Compress 13).
    Cc(u8),
    /// Remote-panel absolute control 8 (NRPN 228); the knob's 0-127 is sent doubled.
    Volume,
}

/// One host parameter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slot {
    /// Stable id: it names the parameter in the host's saved projects and in the stored CC mapping.
    pub id: String,
    pub name: String,
    pub group: String,
    pub kind: Kind,
    pub min: i32,
    pub max: i32,
    pub default: i32,
    /// Minimum time between two messages for this slot.
    pub min_interval_ms: u32,
}

/// At most four Control Changes (an NRPN), without allocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Wire {
    messages: [[u8; 3]; 4],
    len: usize,
}

impl Wire {
    fn from_slice(source: &[u8]) -> Self {
        let mut wire = Self {
            messages: [[0; 3]; 4],
            len: 0,
        };
        for chunk in source.as_chunks::<3>().0.iter().take(4) {
            wire.messages[wire.len] = *chunk;
            wire.len += 1;
        }
        wire
    }

    pub fn messages(&self) -> &[[u8; 3]] {
        &self.messages[..self.len]
    }
}

impl Slot {
    /// The bytes that set this slot to `value` on `channel` (0-15). `value` is clamped to the slot's range.
    /// `None` only when the protocol refuses the combination, which a catalogue slot never does.
    pub fn wire(&self, channel: u8, value: i32) -> Option<Wire> {
        let value = value.clamp(self.min, self.max);
        let unsigned = u16::try_from(value).ok()?;
        match self.kind {
            Kind::Nrpn(number) => nrpn::encode(channel, number, unsigned)
                .ok()
                .map(|bytes| Wire::from_slice(&bytes)),
            Kind::Volume => remote::control(channel, remote::VOLUME_CONTROL, unsigned * 2)
                .ok()
                .map(|bytes| Wire::from_slice(&bytes)),
            Kind::Cc(controller) => {
                let byte = u8::try_from(value).ok()?;
                Some(Wire::from_slice(&[
                    0xB0 | (channel & 0x0F),
                    controller,
                    byte,
                ]))
            }
            Kind::BeatWide(index) => {
                let status = 0xB0 | (channel & 0x0F);
                let msb = u8::try_from(value >> 1).ok()?;
                let mut bytes = [
                    status,
                    BEAT_WIDE_MSB_CC + index,
                    msb,
                    status,
                    BEAT_WIDE_LSB_CC + index,
                    0x40,
                ];
                let used = if value & 1 == 1 { 6 } else { 3 };
                bytes[used..].fill(0);
                Some(Wire::from_slice(&bytes[..used]))
            }
        }
    }

    /// The value a CC byte (0-127) stands for, spread over the slot's range with rounding; 0 gives `min`, 127 `max`.
    pub fn scale_cc(&self, byte: u8) -> i32 {
        let range = i64::from(self.max - self.min);
        let scaled = (i64::from(byte.min(127)) * range * 2 + 127) / 254;
        self.min + i32::try_from(scaled).unwrap_or(0)
    }
}

/// English label of a group name of the table (the table keeps the French names of the firmware screens).
fn group_label(group: &str) -> &str {
    match group {
        "Oscillateurs" => "Oscillators",
        "Mixeur" => "Mixer",
        "Filtre passe-bas" => "Low-pass filter",
        "Filtre passe-haut" => "High-pass filter",
        "Enveloppes" => "Envelopes",
        "Matrice" => "Mod matrix",
        "Divers" => "Misc",
        other => other,
    }
}

fn nrpn_slot(def: &sound::ParamDef) -> Slot {
    let number = u16::from(def.index);
    let name = if def.name.is_empty() {
        "Unnamed".to_string()
    } else {
        def.name.replace("montant", "amount")
    };
    Slot {
        id: format!("n{number}"),
        name: format!("{} {} #{}", group_label(def.group), name, number),
        group: group_label(def.group).to_string(),
        kind: Kind::Nrpn(number),
        min: 0,
        max: i32::from(def.max),
        default: i32::from(def.default),
        min_interval_ms: min_interval(number),
    }
}

fn min_interval(number: u16) -> u32 {
    if SAM_NRPN.contains(&number) {
        SAM_MIN_INTERVAL_MS
    } else {
        0
    }
}

fn extra_slots() -> Vec<Slot> {
    let mut slots = Vec::new();
    for (number, osc) in SAMPLE_NAMES {
        slots.push(Slot {
            id: format!("n{number}"),
            name: format!("{osc} Sample/Noise 14-bit #{number}"),
            group: group_label(if number == 156 { "Osc 3" } else { "Osc 4" }).to_string(),
            kind: Kind::Nrpn(number),
            min: 0,
            max: SAMPLE_MAX,
            default: 0,
            min_interval_ms: min_interval(number),
        });
    }
    for (index, label) in (0u8..).zip(BEAT_WIDE_NAMES) {
        slots.push(Slot {
            id: format!("bw{index}"),
            name: format!("Beat-wide {label}"),
            group: "Beat-wide".to_string(),
            kind: Kind::BeatWide(index),
            min: 0,
            max: BEAT_WIDE_MAX,
            default: 127,
            min_interval_ms: 0,
        });
    }
    for (controller, label) in [(CC_DISTORT, "Distort"), (CC_COMPRESS, "Compress")] {
        slots.push(Slot {
            id: format!("cc{controller}"),
            name: format!("{label} (CC {controller})"),
            group: "Master".to_string(),
            kind: Kind::Cc(controller),
            min: 0,
            max: 127,
            default: 0,
            min_interval_ms: 0,
        });
    }
    slots.push(Slot {
        id: "vol".to_string(),
        name: "Volume (NRPN 228)".to_string(),
        group: "Master".to_string(),
        kind: Kind::Volume,
        min: 0,
        max: 127,
        default: 127,
        min_interval_ms: 0,
    });
    slots
}

/// Every slot, in display order, with the lookups the receiving side needs.
#[derive(Debug)]
pub struct Catalogue {
    slots: Vec<Slot>,
    by_id: HashMap<String, usize>,
    by_nrpn: HashMap<u16, usize>,
}

impl Catalogue {
    pub fn build() -> Self {
        let mut slots: Vec<Slot> = sound::params().iter().map(nrpn_slot).collect();
        slots.extend(extra_slots());
        let by_id = slots
            .iter()
            .enumerate()
            .map(|(i, s)| (s.id.clone(), i))
            .collect();
        let by_nrpn = slots
            .iter()
            .enumerate()
            .filter_map(|(i, s)| match s.kind {
                Kind::Nrpn(number) => Some((number, i)),
                _ => None,
            })
            .collect();
        Self {
            slots,
            by_id,
            by_nrpn,
        }
    }

    pub fn len(&self) -> usize {
        self.slots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    pub fn slots(&self) -> &[Slot] {
        &self.slots
    }

    pub fn slot(&self, index: usize) -> Option<&Slot> {
        self.slots.get(index)
    }

    pub fn index_of_id(&self, id: &str) -> Option<usize> {
        self.by_id.get(id).copied()
    }

    pub fn index_of_nrpn(&self, number: u16) -> Option<usize> {
        self.by_nrpn.get(&number).copied()
    }

    /// The slot of beat-wide position `index` (0-7).
    pub fn index_of_beat_wide(&self, index: u8) -> Option<usize> {
        self.index_of_id(&format!("bw{index}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slot_by_id<'a>(catalogue: &'a Catalogue, id: &str) -> &'a Slot {
        let index = catalogue.index_of_id(id);
        index
            .and_then(|i| catalogue.slot(i))
            .unwrap_or_else(|| panic!("slot {id} missing"))
    }

    #[test]
    fn test_catalogue_when_built_has_every_table_parameter_plus_extras() {
        let catalogue = Catalogue::build();

        // 127 table parameters + 2 samples + 8 beat-wide + Distort + Compress + Volume
        assert_eq!(catalogue.len(), 127 + 2 + 8 + 2 + 1);
        assert!(
            catalogue.len() < 255,
            "mapping cells store index + 1 in a byte"
        );
        for def in sound::params() {
            let slot = slot_by_id(&catalogue, &format!("n{}", def.index));
            assert_eq!(
                (slot.max, slot.default),
                (i32::from(def.max), i32::from(def.default))
            );
        }
    }

    #[test]
    fn test_catalogue_when_built_has_unique_ids_and_names() {
        let catalogue = Catalogue::build();
        let ids: std::collections::HashSet<_> = catalogue.slots().iter().map(|s| &s.id).collect();
        let names: std::collections::HashSet<_> =
            catalogue.slots().iter().map(|s| &s.name).collect();

        assert_eq!(ids.len(), catalogue.len());
        assert_eq!(names.len(), catalogue.len());
    }

    #[test]
    fn test_wire_when_nrpn_returns_the_proto_bytes() {
        let catalogue = Catalogue::build();
        let slot = slot_by_id(&catalogue, "n34");

        let wire = slot.wire(0, 100);

        assert_eq!(
            wire.as_ref().map(Wire::messages),
            Some(&[[0xB0, 99, 0], [0xB0, 98, 34], [0xB0, 6, 0], [0xB0, 38, 100]][..])
        );
    }

    #[test]
    fn test_wire_when_sample_157_returns_msb_1_lsb_29_and_14_bit_value() {
        let catalogue = Catalogue::build();
        let slot = slot_by_id(&catalogue, "n157");

        let wire = slot.wire(4, 463);

        assert_eq!(
            wire.as_ref().map(Wire::messages),
            Some(&[[0xB4, 99, 1], [0xB4, 98, 29], [0xB4, 6, 3], [0xB4, 38, 79]][..])
        );
    }

    #[test]
    fn test_wire_when_beat_wide_even_sends_msb_only() {
        let catalogue = Catalogue::build();
        let slot = slot_by_id(&catalogue, "bw2");

        let wire = slot.wire(1, 100);

        assert_eq!(
            wire.as_ref().map(Wire::messages),
            Some(&[[0xB1, 22, 50]][..])
        );
    }

    #[test]
    fn test_wire_when_beat_wide_odd_adds_lsb_40_and_neutral_is_3f_40() {
        let catalogue = Catalogue::build();
        let slot = slot_by_id(&catalogue, "bw0");

        let neutral = slot.wire(0, 127);
        let top = slot.wire(0, 254);

        assert_eq!(
            neutral.as_ref().map(Wire::messages),
            Some(&[[0xB0, 20, 0x3F], [0xB0, 52, 0x40]][..])
        );
        assert_eq!(
            top.as_ref().map(Wire::messages),
            Some(&[[0xB0, 20, 127]][..])
        );
    }

    #[test]
    fn test_wire_when_distort_and_compress_send_cc_12_and_13() {
        let catalogue = Catalogue::build();

        let distort = slot_by_id(&catalogue, "cc12").wire(2, 99);
        let compress = slot_by_id(&catalogue, "cc13").wire(2, 5);

        assert_eq!(
            distort.as_ref().map(Wire::messages),
            Some(&[[0xB2, 12, 99]][..])
        );
        assert_eq!(
            compress.as_ref().map(Wire::messages),
            Some(&[[0xB2, 13, 5]][..])
        );
    }

    #[test]
    fn test_wire_when_volume_sends_nrpn_228_doubled() {
        let catalogue = Catalogue::build();

        let wire = slot_by_id(&catalogue, "vol").wire(0, 100);

        // 228 = 1 * 128 + 100, value 200 = 1 * 128 + 72
        assert_eq!(
            wire.as_ref().map(Wire::messages),
            Some(&[[0xB0, 99, 1], [0xB0, 98, 100], [0xB0, 6, 1], [0xB0, 38, 72]][..])
        );
    }

    #[test]
    fn test_wire_when_value_out_of_range_is_clamped_to_the_slot_max() {
        let catalogue = Catalogue::build();
        let slot = slot_by_id(&catalogue, "n1");

        let clamped = slot.wire(0, 9999);
        let at_max = slot.wire(0, slot.max);

        assert_eq!(clamped, at_max);
    }

    #[test]
    fn test_wire_when_every_slot_at_min_and_max_is_valid_midi() {
        let catalogue = Catalogue::build();

        for slot in catalogue.slots() {
            for value in [slot.min, slot.default, slot.max] {
                let wire = slot.wire(15, value);
                let wire = wire.unwrap_or_else(|| panic!("{} refused {value}", slot.id));
                for message in wire.messages() {
                    assert_eq!(message[0], 0xBF, "{}", slot.id);
                    assert!(
                        message[1] < 128 && message[2] < 128,
                        "{} {message:?}",
                        slot.id
                    );
                }
            }
        }
    }

    #[test]
    fn test_scale_cc_when_endpoints_returns_min_and_max_and_is_monotone() {
        let catalogue = Catalogue::build();
        let slot = slot_by_id(&catalogue, "n37");

        assert_eq!((slot.scale_cc(0), slot.scale_cc(127)), (slot.min, slot.max));
        let values: Vec<i32> = (0..=127).map(|b| slot.scale_cc(b)).collect();
        assert!(values.windows(2).all(|w| w[0] <= w[1]));
    }

    #[test]
    fn test_lookup_when_nrpn_number_returns_slot_and_unknown_returns_none() {
        let catalogue = Catalogue::build();

        assert!(catalogue.index_of_nrpn(34).is_some());
        assert!(catalogue.index_of_nrpn(156).is_some());
        assert_eq!(catalogue.index_of_nrpn(127), None);
        assert_eq!(catalogue.index_of_nrpn(228), None);
    }
}
