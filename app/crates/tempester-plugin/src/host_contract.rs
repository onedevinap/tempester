//! The plugin as a host sees it, read through nih-plug's own VST3 wrapper and class info (the code a host's calls
//! reach) rather than through our constants alone: categories, audio and event buses, bus arrangement, audio
//! pass-through, announced version.
//!
//! Public issue #5: Ableton Live refused the VST3 with « plugin has an effect category, but no valid audio input
//! bus » and « plugin did not follow bus configuration protocol (error: invalid argument) ». No DAW runs here: what a
//! given host then does with the plugin is not proved by these tests.

use std::ffi::c_void;
use std::mem;

use nih_plug::prelude::*;
use nih_plug::wrapper::vst3::vst3_sys;
use nih_plug::wrapper::vst3::{PluginInfo, Wrapper};
use vst3_sys::base::kResultOk;
use vst3_sys::vst::{
    kStereo, AudioBusBuffers, BusDirections, BusInfo, BusTypes, IAudioProcessor, IComponent,
    MediaTypes, ProcessData, ProcessModes, ProcessSetup, SpeakerArrangement, SymbolicSampleSizes,
};

use crate::engine::realtime::count_allocations;
use crate::TempesterControl;

const AUDIO: i32 = MediaTypes::kAudio as i32;
const EVENT: i32 = MediaTypes::kEvent as i32;
const INPUT: i32 = BusDirections::kInput as i32;
const OUTPUT: i32 = BusDirections::kOutput as i32;
const BLOCK: usize = 64;

/// `[workspace.package] version` of `app/Cargo.toml`, read from the file itself: the release version the plugin must
/// announce, whatever path the plugin takes to get it.
fn workspace_version() -> String {
    let manifest = concat!(env!("CARGO_MANIFEST_DIR"), "/../../Cargo.toml");
    let text = std::fs::read_to_string(manifest).expect("app/Cargo.toml is readable");
    let section = text
        .split("[workspace.package]")
        .nth(1)
        .expect("app/Cargo.toml has a [workspace.package] section");
    section
        .lines()
        .take_while(|line| !line.starts_with('['))
        .find_map(|line| line.strip_prefix("version = \""))
        .and_then(|rest| rest.strip_suffix('"'))
        .expect("[workspace.package] names a version")
        .to_string()
}

fn wrapper() -> Box<Wrapper<TempesterControl>> {
    Wrapper::<TempesterControl>::new()
}

fn bus_info(wrapper: &Wrapper<TempesterControl>, media: i32, direction: i32) -> Option<BusInfo> {
    // SAFETY: an all-zero `BusInfo` is valid (integers and a UTF-16 array); `info` outlives the call.
    let mut info: BusInfo = unsafe { mem::zeroed() };
    // SAFETY: a live wrapper, a valid pointer to `info`.
    let result = unsafe { wrapper.get_bus_info(media, direction, 0, &mut info) };
    (result == kResultOk).then_some(info)
}

fn set_stereo(wrapper: &Wrapper<TempesterControl>) -> i32 {
    let mut input: SpeakerArrangement = kStereo;
    let mut output: SpeakerArrangement = kStereo;
    // SAFETY: one arrangement on each side, both alive for the call.
    unsafe { wrapper.set_bus_arrangements(&mut input, 1, &mut output, 1) }
}

/// The main bus of `direction` as Live checks it for an effect: present, main, with at least one channel.
fn has_main_audio_bus(wrapper: &Wrapper<TempesterControl>, direction: i32) -> bool {
    // SAFETY: a live wrapper.
    let count = unsafe { wrapper.get_bus_count(AUDIO, direction) };
    let main = bus_info(wrapper, AUDIO, direction);
    count >= 1
        && main.is_some_and(|bus| bus.bus_type == BusTypes::kMain as i32 && bus.channel_count > 0)
}

#[test]
fn test_vst3_class_info_when_read_by_a_host_announces_the_app_workspace_version() {
    let info = PluginInfo::for_plugin::<TempesterControl>();

    assert_eq!(info.version, workspace_version());
}

#[test]
fn test_workspace_version_when_read_from_app_cargo_toml_returns_a_release_number() {
    // The reference of the test above is a real version, not an empty or default string that would match a plugin
    // without one.
    let version = workspace_version();
    let parts: Vec<&str> = version.split('.').collect();

    assert_eq!(parts.len(), 3, "{version}");
    assert!(parts.iter().all(|p| p.parse::<u32>().is_ok()), "{version}");
    assert_ne!(version, "0.0.0");
}

#[test]
fn test_vst3_effect_category_when_declared_comes_with_main_audio_input_and_output_buses() {
    let info = PluginInfo::for_plugin::<TempesterControl>();
    let wrapper = wrapper();

    let is_effect = info.subcategories.split('|').any(|c| c == "Fx");

    assert!(
        !is_effect || has_main_audio_bus(&wrapper, INPUT),
        "VST3 subcategories {:?} but no main audio input bus: Ableton Live refuses such a plugin",
        info.subcategories
    );
    assert!(
        !is_effect || has_main_audio_bus(&wrapper, OUTPUT),
        "VST3 subcategories {:?} but no main audio output bus",
        info.subcategories
    );
}

#[test]
fn test_clap_audio_effect_feature_when_declared_comes_with_main_audio_input_and_output_in_every_layout(
) {
    let is_effect = TempesterControl::CLAP_FEATURES.contains(&ClapFeature::AudioEffect);
    let layouts = TempesterControl::AUDIO_IO_LAYOUTS;

    assert!(
        !is_effect
            || (!layouts.is_empty()
                && layouts.iter().all(|layout| {
                    layout.main_input_channels.is_some() && layout.main_output_channels.is_some()
                }))
    );
}

#[test]
fn test_vst3_bus_arrangement_when_host_asks_for_stereo_in_and_out_is_accepted_and_kept() {
    let wrapper = wrapper();

    let result = set_stereo(&wrapper);

    assert_eq!(result, kResultOk, "setBusArrangements(stereo, stereo)");
    for direction in [INPUT, OUTPUT] {
        let mut arrangement: SpeakerArrangement = 0;
        // SAFETY: a live wrapper, a valid pointer to `arrangement`.
        let result = unsafe { wrapper.get_bus_arrangement(direction, 0, &mut arrangement) };
        assert_eq!(result, kResultOk, "getBusArrangement({direction}, 0)");
        assert_eq!(arrangement, kStereo, "direction {direction}");
    }
}

#[test]
fn test_vst3_event_buses_when_queried_keep_one_midi_input_and_one_midi_output() {
    // The audio buses must not cost the MIDI output, which is the whole point of the plugin.
    let wrapper = wrapper();

    // SAFETY: a live wrapper.
    let counts = unsafe {
        (
            wrapper.get_bus_count(EVENT, INPUT),
            wrapper.get_bus_count(EVENT, OUTPUT),
        )
    };

    assert_eq!(counts, (1, 1));
    assert!(bus_info(&wrapper, EVENT, OUTPUT).is_some());
}

/// Out-of-place stereo buffers of one block: the input a ramp, the output a value no ramp sample has.
struct Buffers {
    input: [Vec<f32>; 2],
    output: [Vec<f32>; 2],
}

impl Buffers {
    fn new() -> Self {
        let ramp = |channel: usize| -> Vec<f32> {
            (0..BLOCK)
                .map(|i| (i as f32 + 1.0) / BLOCK as f32 * if channel == 0 { 0.5 } else { -0.25 })
                .collect()
        };
        Self {
            input: [ramp(0), ramp(1)],
            output: [vec![9.0; BLOCK], vec![9.0; BLOCK]],
        }
    }
}

/// An active, processing wrapper in stereo, as a host leaves it before its first `process` call.
fn processing_wrapper() -> Box<Wrapper<TempesterControl>> {
    let wrapper = wrapper();
    assert_eq!(set_stereo(&wrapper), kResultOk);
    let setup = ProcessSetup {
        process_mode: ProcessModes::kRealtime as i32,
        symbolic_sample_size: SymbolicSampleSizes::kSample32 as i32,
        max_samples_per_block: BLOCK as i32,
        sample_rate: 48_000.0,
    };
    // SAFETY: a live wrapper; `setup` outlives the call; the order is the host's (setup, activate, process).
    unsafe {
        assert_eq!(wrapper.setup_processing(&setup), kResultOk);
        assert_eq!(wrapper.set_active(1), kResultOk);
        assert_eq!(wrapper.set_processing(1), kResultOk);
    }
    wrapper
}

/// One VST3 `process` call over `buffers`, without events or parameter changes.
fn process(wrapper: &Wrapper<TempesterControl>, buffers: &mut Buffers) -> i32 {
    let mut input_pointers: [*mut c_void; 2] = [
        buffers.input[0].as_mut_ptr().cast(),
        buffers.input[1].as_mut_ptr().cast(),
    ];
    let mut output_pointers: [*mut c_void; 2] = [
        buffers.output[0].as_mut_ptr().cast(),
        buffers.output[1].as_mut_ptr().cast(),
    ];
    let mut input = AudioBusBuffers {
        num_channels: 2,
        silence_flags: 0,
        buffers: input_pointers.as_mut_ptr(),
    };
    let mut output = AudioBusBuffers {
        num_channels: 2,
        silence_flags: 0,
        buffers: output_pointers.as_mut_ptr(),
    };
    // SAFETY: all-zero is valid for `ProcessData`: integers and raw pointers, null meaning « none » (no parameter
    // changes, no events, no context).
    let mut data: ProcessData = unsafe { mem::zeroed() };
    data.process_mode = ProcessModes::kRealtime as i32;
    data.symbolic_sample_size = SymbolicSampleSizes::kSample32 as i32;
    data.num_samples = BLOCK as i32;
    data.num_inputs = 1;
    data.num_outputs = 1;
    data.inputs = &mut input;
    data.outputs = &mut output;
    // SAFETY: a processing wrapper; every pointer in `data` points to `BLOCK` samples alive for the call.
    unsafe { wrapper.process(&mut data) }
}

#[test]
fn test_vst3_process_when_stereo_audio_comes_in_passes_it_out_untouched() {
    let wrapper = processing_wrapper();
    let mut buffers = Buffers::new();
    let expected = buffers.input.clone();

    let result = process(&wrapper, &mut buffers);

    assert_eq!(result, kResultOk);
    assert_eq!(buffers.output, expected);
    assert_eq!(buffers.input, expected, "the input is left as it came");
}

#[test]
fn test_vst3_process_when_audio_passes_through_never_allocates() {
    let wrapper = processing_wrapper();
    let mut buffers = Buffers::new();
    // The first call settles what the wrapper prepares lazily; the audio thread's steady state is what counts.
    assert_eq!(process(&wrapper, &mut buffers), kResultOk);

    let allocations = count_allocations(|| {
        for _ in 0..100 {
            process(&wrapper, &mut buffers);
        }
    });

    assert_eq!(allocations, 0);
    assert_eq!(buffers.output, buffers.input);
}
