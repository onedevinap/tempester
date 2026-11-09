//! Tempester Control: a CLAP and VST3 MIDI effect that drives the DSI Tempest from a music application.
//!
//! The host parameters are the Tempest's sound parameters; a change (automation, the DAW's own parameter UI, a learned
//! controller) goes out on the plugin's MIDI output as NRPN, beat-wide CCs or plain CCs, for the DAW to route to the
//! Tempest's MIDI port. The plugin never opens a MIDI port itself.
//!
//! - [`catalogue`]: the parameters and their bytes (from `tempest-proto`);
//! - [`mapping`]: MIDI learn table, saved in the plugin state;
//! - [`engine`]: the per-block logic (change detection, coalescing, rate budget, follow), host-free and tested;
//! - [`params`]: the host-facing parameter set;
//! - this file: the glue to nih-plug.
//!
//! Needs a Tempester Main that receives NRPN (1.5.3.0 or later; 1.6.0.0 for Volume; 1.6.1.4 for the beat-wide LSB):
//! the original DSI OS ignores NRPN.

pub mod catalogue;
pub mod engine;
pub mod mapping;
pub mod params;

use std::sync::Arc;

use nih_plug::prelude::*;

use catalogue::Catalogue;
use engine::{Block, CcIn, Core, InEvent, Out, MAX_EVENTS, OUT_CAPACITY};
use mapping::CcMap;
use params::TempesterParams;

pub struct TempesterControl {
    params: Arc<TempesterParams>,
    core: Core,
    /// Preallocated work buffers of `process`.
    host: Vec<i32>,
    raw: Vec<NoteEvent<()>>,
    events: Vec<InEvent>,
    out: Vec<Out>,
    sample_rate: f32,
}

impl Default for TempesterControl {
    fn default() -> Self {
        let catalogue = Arc::new(Catalogue::build());
        let map = Arc::new(CcMap::new());
        let params = Arc::new(TempesterParams::new(
            Arc::clone(&catalogue),
            Arc::clone(&map),
        ));
        Self {
            host: vec![0; catalogue.len()],
            core: Core::new(catalogue, map),
            params,
            raw: Vec::with_capacity(MAX_EVENTS),
            events: Vec::with_capacity(MAX_EVENTS),
            out: Vec::with_capacity(OUT_CAPACITY),
            sample_rate: 48_000.0,
        }
    }
}

/// The Control Change inside a host event, with the 0-1 value of nih-plug turned back into the MIDI byte.
fn cc_of(event: &NoteEvent<()>) -> Option<CcIn> {
    match *event {
        NoteEvent::MidiCC {
            channel, cc, value, ..
        } => Some(CcIn {
            channel,
            controller: cc,
            value: (value * 127.0).round().clamp(0.0, 127.0) as u8,
        }),
        _ => None,
    }
}

impl Plugin for TempesterControl {
    const NAME: &'static str = "Tempester Control";
    const VENDOR: &'static str = "Tempester";
    const URL: &'static str = "https://github.com/onedevinap/tempester";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    // A MIDI effect: no audio ports.
    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[];
    const MIDI_INPUT: MidiConfig = MidiConfig::MidiCCs;
    const MIDI_OUTPUT: MidiConfig = MidiConfig::MidiCCs;
    // The host cuts blocks at automation points, so a change is sent at its own sample.
    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        self.sample_rate = buffer_config.sample_rate;
        // The host has restored the saved values by now: they are the baseline, nothing is sent for them.
        self.core.restart();
        true
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        self.params.read_sounds(&mut self.host);
        self.raw.clear();
        self.events.clear();
        while let Some(event) = context.next_event() {
            if self.raw.len() < self.raw.capacity() {
                self.events.push(InEvent {
                    timing: event.timing(),
                    cc: cc_of(&event),
                });
                self.raw.push(event);
            } else {
                // Beyond the preallocated block capacity: forwarded as is.
                context.send_event(event);
            }
        }
        let block = Block {
            len: u32::try_from(buffer.samples()).unwrap_or(u32::MAX),
            sample_rate: self.sample_rate,
        };
        self.core.process_block(
            &self.host,
            &self.params.controls(),
            block,
            &self.events,
            &mut self.out,
        );
        for item in &self.out {
            match *item {
                Out::Pass(index) => {
                    if let Some(event) = self.raw.get(index) {
                        context.send_event(*event);
                    }
                }
                Out::Cc { timing, bytes } => context.send_event(NoteEvent::MidiCC {
                    timing,
                    channel: bytes[0] & 0x0F,
                    cc: bytes[1],
                    value: f32::from(bytes[2]) / 127.0,
                }),
            }
        }
        ProcessStatus::Normal
    }
}

impl ClapPlugin for TempesterControl {
    const CLAP_ID: &'static str = "io.github.onedevinap.tempester-control";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Drives the DSI Tempest sound parameters from the host (NRPN over MIDI).");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[ClapFeature::NoteEffect, ClapFeature::Utility];
}

impl Vst3Plugin for TempesterControl {
    const VST3_CLASS_ID: [u8; 16] = *b"TempesterCtrl001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Tools];
}

nih_export_clap!(TempesterControl);
// The warning comes from the `vst3-com` macros that this macro expands to (third-party code, not ours).
#[allow(semicolon_in_expressions_from_non_local_macros)]
mod vst3_export {
    use super::TempesterControl;
    use nih_plug::prelude::*;
    nih_export_vst3!(TempesterControl);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cc_of_when_midi_cc_event_returns_the_byte_and_non_cc_returns_none() {
        let cc = NoteEvent::<()>::MidiCC {
            timing: 3,
            channel: 2,
            cc: 74,
            value: 64.0 / 127.0,
        };
        let note = NoteEvent::<()>::NoteOn {
            timing: 0,
            voice_id: None,
            channel: 0,
            note: 60,
            velocity: 1.0,
        };

        assert_eq!(
            cc_of(&cc),
            Some(CcIn {
                channel: 2,
                controller: 74,
                value: 64
            })
        );
        assert_eq!(cc_of(&note), None);
    }

    #[test]
    fn test_cc_value_when_every_byte_goes_through_the_host_float_returns_the_same_byte() {
        for byte in 0..=127u8 {
            let host_value = f32::from(byte) / 127.0;
            let event = NoteEvent::<()>::MidiCC {
                timing: 0,
                channel: 0,
                cc: 1,
                value: host_value,
            };
            assert_eq!(cc_of(&event).map(|c| c.value), Some(byte));
        }
    }

    #[test]
    fn test_default_when_built_preallocates_the_work_buffers() {
        let plugin = TempesterControl::default();

        assert!(plugin.raw.capacity() >= MAX_EVENTS);
        assert!(plugin.out.capacity() >= OUT_CAPACITY);
        assert_eq!(plugin.host.len(), plugin.params.catalogue.len());
    }
}
