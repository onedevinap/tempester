//! MIDI learn: which incoming Control Change drives which slot.
//!
//! The table is read on the audio thread and written by it (learn) and by the host's state load, so every cell is an
//! atomic: no lock. It is saved in the plugin state as text (`"20=n34,21=bw0"`, slot ids, not indexes, so a later
//! catalogue with a different order still restores the same targets).

use std::sync::atomic::{AtomicU8, Ordering};

use crate::catalogue::Catalogue;

/// Controllers that can never be learned: bank select, the NRPN / RPN family (the Tempest's own protocol, and the
/// machine-to-plugin direction) and the channel-mode messages.
pub fn is_learnable(controller: u8) -> bool {
    !matches!(controller, 0 | 6 | 32 | 38 | 98..=101) && controller < 120
}

/// `controller -> slot`, 0 = none, otherwise slot index + 1.
#[derive(Debug)]
pub struct CcMap {
    cells: [AtomicU8; 128],
}

impl Default for CcMap {
    fn default() -> Self {
        Self::new()
    }
}

impl CcMap {
    pub fn new() -> Self {
        Self {
            cells: std::array::from_fn(|_| AtomicU8::new(0)),
        }
    }

    pub fn get(&self, controller: u8) -> Option<usize> {
        let cell = self.cells.get(usize::from(controller))?;
        usize::from(cell.load(Ordering::Relaxed)).checked_sub(1)
    }

    /// Maps `controller` to `slot`; false (nothing stored) when the controller cannot be learned or the slot index
    /// does not fit a cell.
    pub fn set(&self, controller: u8, slot: usize) -> bool {
        let Some(cell) = self.cells.get(usize::from(controller)) else {
            return false;
        };
        let Ok(stored) = u8::try_from(slot + 1) else {
            return false;
        };
        if !is_learnable(controller) {
            return false;
        }
        cell.store(stored, Ordering::Relaxed);
        true
    }

    pub fn clear(&self) {
        for cell in &self.cells {
            cell.store(0, Ordering::Relaxed);
        }
    }

    pub fn count(&self) -> usize {
        self.cells
            .iter()
            .filter(|c| c.load(Ordering::Relaxed) != 0)
            .count()
    }

    /// The saved form: `controller=slot id`, comma separated, in controller order.
    pub fn to_text(&self, catalogue: &Catalogue) -> String {
        let mut entries = Vec::new();
        for controller in 0..=127u8 {
            let id = self
                .get(controller)
                .and_then(|slot| catalogue.slot(slot))
                .map(|slot| slot.id.as_str());
            if let Some(id) = id {
                entries.push(format!("{controller}={id}"));
            }
        }
        entries.join(",")
    }

    /// Replaces the table with a saved one. Entries that are malformed, name an unknown slot or an unlearnable
    /// controller are skipped (a project saved by another version must still load). Returns the entries kept.
    pub fn load_text(&self, text: &str, catalogue: &Catalogue) -> usize {
        self.clear();
        let mut kept = 0;
        for entry in text.split(',') {
            let Some((controller, id)) = entry.split_once('=') else {
                continue;
            };
            let controller = controller.trim().parse::<u8>().ok();
            let slot = catalogue.index_of_id(id.trim());
            if let (Some(controller), Some(slot)) = (controller, slot) {
                if self.set(controller, slot) {
                    kept += 1;
                }
            }
        }
        kept
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_when_learnable_controller_maps_and_get_returns_slot() {
        let map = CcMap::new();

        assert!(map.set(74, 12));

        assert_eq!(map.get(74), Some(12));
        assert_eq!(map.get(75), None);
        assert_eq!(map.count(), 1);
    }

    #[test]
    fn test_set_when_reserved_controller_refuses_and_stores_nothing() {
        let map = CcMap::new();

        for controller in [0, 6, 32, 38, 98, 99, 100, 101, 120, 127] {
            assert!(!map.set(controller, 3), "CC {controller}");
        }
        assert_eq!(map.count(), 0);
        // counter-proof: the neighbours of the reserved ones are accepted
        assert!(map.set(1, 3) && map.set(97, 3) && map.set(102, 3) && map.set(119, 3));
    }

    #[test]
    fn test_set_when_slot_does_not_fit_a_cell_refuses() {
        let map = CcMap::new();

        assert!(!map.set(10, 255));
        assert!(map.set(10, 254));
    }

    #[test]
    fn test_text_when_round_trip_restores_the_same_mapping() {
        let catalogue = Catalogue::build();
        let map = CcMap::new();
        map.set(74, catalogue.index_of_id("n34").unwrap_or(0));
        map.set(1, catalogue.index_of_id("bw3").unwrap_or(0));
        map.set(11, catalogue.index_of_id("vol").unwrap_or(0));
        let text = map.to_text(&catalogue);

        let restored = CcMap::new();
        let kept = restored.load_text(&text, &catalogue);

        assert_eq!(kept, 3);
        assert_eq!(restored.to_text(&catalogue), text);
        assert_eq!(restored.get(74), catalogue.index_of_id("n34"));
        assert_eq!(text, "1=bw3,11=vol,74=n34");
    }

    #[test]
    fn test_text_when_a_wrong_copy_is_loaded_the_round_trip_differs() {
        // counter-proof of the round trip: a mapping loaded from altered text is not the saved one
        let catalogue = Catalogue::build();
        let map = CcMap::new();
        map.set(74, catalogue.index_of_id("n34").unwrap_or(0));
        let text = map.to_text(&catalogue);

        let altered = CcMap::new();
        altered.load_text(&text.replace("n34", "n35"), &catalogue);

        assert_ne!(altered.to_text(&catalogue), text);
    }

    #[test]
    fn test_load_text_when_garbage_skips_bad_entries_and_keeps_good_ones() {
        let catalogue = Catalogue::build();
        let map = CcMap::new();
        map.set(5, 1);

        let kept = map.load_text(
            "74=n34,,x=n1,300=n1,20=nope,6=n1,=,21 = bw0, 22=",
            &catalogue,
        );

        assert_eq!(kept, 2);
        assert_eq!(map.get(5), None, "load replaces the previous table");
        assert_eq!(map.to_text(&catalogue), "21=bw0,74=n34");
    }

    #[test]
    fn test_load_text_when_empty_clears_the_table() {
        let catalogue = Catalogue::build();
        let map = CcMap::new();
        map.set(5, 1);

        assert_eq!(map.load_text("", &catalogue), 0);

        assert_eq!(map.count(), 0);
    }
}
