//! Headless CLAP host: loads the built `.clap` bundle and drives it like a DAW would, through the CLAP ABI only.
//!
//! No dependency (the few CLAP structures needed are declared here, `dlopen` comes from libc). It opens no MIDI port
//! and no window, and writes nothing to disk. Checks, each with its expected bytes:
//!
//! 1. the factory, the descriptor and the parameter list (names, ranges, count);
//! 2. a block without events sends nothing (the baseline);
//! 3. an automated parameter change goes out as the 4 NRPN Control Changes at the sample of the change;
//!    an unmapped CC passes through;
//! 4. MIDI learn: touch a parameter, move a CC, then the CC drives the parameter and is not passed through;
//! 5. the saved state (real `state` extension) holds the mapping, and a second instance that loads it behaves the same.
//!
//! `cargo run --release --example host_clap -- "target/bundled/Tempester Control.clap"` (macOS and Linux; on Windows
//! the example only says so).

#![allow(unsafe_code)] // the CLAP ABI: every call goes through a function pointer of the plugin

#[cfg(unix)]
mod unix_host {
    use std::ffi::{c_char, c_void, CStr, CString};
    use std::path::{Path, PathBuf};

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Version {
        major: u32,
        minor: u32,
        revision: u32,
    }
    const CLAP_VERSION: Version = Version {
        major: 1,
        minor: 2,
        revision: 0,
    };

    #[repr(C)]
    struct Entry {
        version: Version,
        init: unsafe extern "C" fn(*const c_char) -> bool,
        deinit: unsafe extern "C" fn(),
        get_factory: unsafe extern "C" fn(*const c_char) -> *const c_void,
    }

    #[repr(C)]
    struct Factory {
        get_plugin_count: unsafe extern "C" fn(*const Factory) -> u32,
        get_plugin_descriptor: unsafe extern "C" fn(*const Factory, u32) -> *const Descriptor,
        create_plugin:
            unsafe extern "C" fn(*const Factory, *const Host, *const c_char) -> *const Plugin,
    }

    #[repr(C)]
    struct Descriptor {
        version: Version,
        id: *const c_char,
        name: *const c_char,
        vendor: *const c_char,
        url: *const c_char,
        manual_url: *const c_char,
        support_url: *const c_char,
        version_text: *const c_char,
        description: *const c_char,
        features: *const *const c_char,
    }

    #[repr(C)]
    struct Host {
        version: Version,
        host_data: *mut c_void,
        name: *const c_char,
        vendor: *const c_char,
        url: *const c_char,
        version_text: *const c_char,
        get_extension: unsafe extern "C" fn(*const Host, *const c_char) -> *const c_void,
        request_restart: unsafe extern "C" fn(*const Host),
        request_process: unsafe extern "C" fn(*const Host),
        request_callback: unsafe extern "C" fn(*const Host),
    }

    #[repr(C)]
    struct Plugin {
        desc: *const Descriptor,
        plugin_data: *mut c_void,
        init: unsafe extern "C" fn(*const Plugin) -> bool,
        destroy: unsafe extern "C" fn(*const Plugin),
        activate: unsafe extern "C" fn(*const Plugin, f64, u32, u32) -> bool,
        deactivate: unsafe extern "C" fn(*const Plugin),
        start_processing: unsafe extern "C" fn(*const Plugin) -> bool,
        stop_processing: unsafe extern "C" fn(*const Plugin),
        reset: unsafe extern "C" fn(*const Plugin),
        process: unsafe extern "C" fn(*const Plugin, *const Process) -> i32,
        get_extension: unsafe extern "C" fn(*const Plugin, *const c_char) -> *const c_void,
        on_main_thread: unsafe extern "C" fn(*const Plugin),
    }

    #[repr(C)]
    struct ParamInfo {
        id: u32,
        flags: u32,
        cookie: *mut c_void,
        name: [c_char; 256],
        module: [c_char; 1024],
        min: f64,
        max: f64,
        default: f64,
    }

    #[repr(C)]
    struct ParamsExt {
        count: unsafe extern "C" fn(*const Plugin) -> u32,
        get_info: unsafe extern "C" fn(*const Plugin, u32, *mut ParamInfo) -> bool,
    }

    #[repr(C)]
    struct OutStream {
        ctx: *mut c_void,
        write: unsafe extern "C" fn(*const OutStream, *const c_void, u64) -> i64,
    }

    #[repr(C)]
    struct InStream {
        ctx: *mut c_void,
        read: unsafe extern "C" fn(*const InStream, *mut c_void, u64) -> i64,
    }

    #[repr(C)]
    struct StateExt {
        save: unsafe extern "C" fn(*const Plugin, *const OutStream) -> bool,
        load: unsafe extern "C" fn(*const Plugin, *const InStream) -> bool,
    }

    #[repr(C)]
    struct InEvents {
        ctx: *mut c_void,
        size: unsafe extern "C" fn(*const InEvents) -> u32,
        get: unsafe extern "C" fn(*const InEvents, u32) -> *const c_void,
    }

    #[repr(C)]
    struct OutEvents {
        ctx: *mut c_void,
        try_push: unsafe extern "C" fn(*const OutEvents, *const c_void) -> bool,
    }

    #[repr(C)]
    struct Process {
        steady_time: i64,
        frames_count: u32,
        transport: *const c_void,
        audio_inputs: *const c_void,
        audio_outputs: *mut c_void,
        audio_inputs_count: u32,
        audio_outputs_count: u32,
        in_events: *const InEvents,
        out_events: *const OutEvents,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct EventHeader {
        size: u32,
        time: u32,
        space_id: u16,
        kind: u16,
        flags: u32,
    }
    const EVENT_PARAM_VALUE: u16 = 5;
    const EVENT_MIDI: u16 = 10;

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct ParamValueEvent {
        header: EventHeader,
        param_id: u32,
        cookie: *mut c_void,
        note_id: i32,
        port_index: i16,
        channel: i16,
        key: i16,
        value: f64,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct MidiEvent {
        header: EventHeader,
        port_index: u16,
        data: [u8; 3],
    }

    extern "C" {
        fn dlopen(path: *const c_char, mode: i32) -> *mut c_void;
        fn dlsym(module: *mut c_void, name: *const c_char) -> *mut c_void;
    }
    const RTLD_NOW: i32 = 2;

    unsafe extern "C" fn host_extension(_h: *const Host, _id: *const c_char) -> *const c_void {
        std::ptr::null()
    }
    unsafe extern "C" fn host_nothing(_h: *const Host) {}

    unsafe extern "C" fn events_size(list: *const InEvents) -> u32 {
        // SAFETY: `ctx` is the `Vec<*const c_void>` that `Block::run` keeps alive for the whole `process` call.
        let events = unsafe { &*((*list).ctx as *const Vec<*const c_void>) };
        u32::try_from(events.len()).unwrap_or(0)
    }
    unsafe extern "C" fn events_get(list: *const InEvents, index: u32) -> *const c_void {
        // SAFETY: as in `events_size`.
        let events = unsafe { &*((*list).ctx as *const Vec<*const c_void>) };
        events
            .get(index as usize)
            .copied()
            .unwrap_or(std::ptr::null())
    }
    unsafe extern "C" fn events_push(list: *const OutEvents, event: *const c_void) -> bool {
        // SAFETY: `ctx` is the `Vec<Sent>` of `Block::run`; `event` points to a CLAP event whose header says its kind
        // and size, and a MIDI event is at least as long as `MidiEvent`.
        unsafe {
            let sent = &mut *((*list).ctx as *mut Vec<Sent>);
            let header = *(event as *const EventHeader);
            if header.kind == EVENT_MIDI {
                let midi = *(event as *const MidiEvent);
                sent.push(Sent {
                    time: header.time,
                    bytes: midi.data,
                });
            }
        }
        true
    }
    unsafe extern "C" fn stream_write(
        stream: *const OutStream,
        data: *const c_void,
        size: u64,
    ) -> i64 {
        // SAFETY: `ctx` is the `Vec<u8>` of `save_state`; `data` holds `size` readable bytes (CLAP stream contract).
        unsafe {
            let buffer = &mut *((*stream).ctx as *mut Vec<u8>);
            buffer.extend_from_slice(std::slice::from_raw_parts(data as *const u8, size as usize));
        }
        i64::try_from(size).unwrap_or(-1)
    }
    unsafe extern "C" fn stream_read(stream: *const InStream, data: *mut c_void, size: u64) -> i64 {
        // SAFETY: `ctx` is the `(Vec<u8>, usize)` of `load_state`; `data` has room for `size` bytes (CLAP contract).
        unsafe {
            let (bytes, position) = &mut *((*stream).ctx as *mut (Vec<u8>, usize));
            let count = bytes.len().saturating_sub(*position).min(size as usize);
            std::ptr::copy_nonoverlapping(bytes.as_ptr().add(*position), data as *mut u8, count);
            *position += count;
            i64::try_from(count).unwrap_or(-1)
        }
    }

    /// One MIDI message the plugin sent.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct Sent {
        time: u32,
        bytes: [u8; 3],
    }

    /// What the host sends in a block.
    enum Input {
        Param {
            id: u32,
            value: f64,
            time: u32,
        },
        Cc {
            controller: u8,
            value: u8,
            time: u32,
        },
    }

    struct Instance {
        plugin: *const Plugin,
        host: Box<Host>,
        params: Vec<(u32, String, f64, f64, f64)>,
    }

    fn text(pointer: *const c_char) -> String {
        if pointer.is_null() {
            return String::new();
        }
        // SAFETY: the plugin returns NUL-terminated strings that outlive this call.
        unsafe { CStr::from_ptr(pointer) }
            .to_string_lossy()
            .into_owned()
    }

    fn check(condition: bool, message: impl Into<String>) -> Result<(), String> {
        if condition {
            Ok(())
        } else {
            Err(message.into())
        }
    }

    impl Instance {
        /// # Safety
        /// `factory` must be a valid CLAP plugin factory.
        unsafe fn create(factory: *const Factory, id: &CStr) -> Result<Self, String> {
            let host = Box::new(Host {
                version: CLAP_VERSION,
                host_data: std::ptr::null_mut(),
                name: c"host_clap (Tempester test host)".as_ptr(),
                vendor: c"Tempester".as_ptr(),
                url: c"".as_ptr(),
                version_text: c"0.1".as_ptr(),
                get_extension: host_extension,
                request_restart: host_nothing,
                request_process: host_nothing,
                request_callback: host_nothing,
            });
            // SAFETY: the caller guarantees `factory`; `host` lives as long as the instance.
            let plugin = unsafe { ((*factory).create_plugin)(factory, &*host, id.as_ptr()) };
            check(!plugin.is_null(), "create_plugin returned null")?;
            // SAFETY: `plugin` was just created.
            check(unsafe { ((*plugin).init)(plugin) }, "init failed")?;
            let mut instance = Self {
                plugin,
                host,
                params: Vec::new(),
            };
            // SAFETY: the plugin is initialised.
            unsafe { instance.read_params()? };
            // SAFETY: the plugin is initialised and inactive.
            check(
                unsafe { ((*plugin).activate)(plugin, 48_000.0, 1, 512) },
                "activate failed",
            )?;
            // SAFETY: the plugin is active.
            check(
                unsafe { ((*plugin).start_processing)(plugin) },
                "start_processing failed",
            )?;
            let _ = &instance.host;
            Ok(instance)
        }

        unsafe fn extension<T>(&self, id: &CStr) -> Result<*const T, String> {
            // SAFETY: the plugin outlives `self`.
            let pointer = unsafe { ((*self.plugin).get_extension)(self.plugin, id.as_ptr()) };
            check(!pointer.is_null(), format!("no extension {id:?}"))?;
            Ok(pointer as *const T)
        }

        unsafe fn read_params(&mut self) -> Result<(), String> {
            // SAFETY: extension lookups and calls on a live plugin.
            unsafe {
                let ext = self.extension::<ParamsExt>(c"clap.params")?;
                let count = ((*ext).count)(self.plugin);
                for index in 0..count {
                    let mut info: ParamInfo = std::mem::zeroed();
                    check(
                        ((*ext).get_info)(self.plugin, index, &mut info),
                        "get_info failed",
                    )?;
                    self.params.push((
                        info.id,
                        text(info.name.as_ptr()),
                        info.min,
                        info.max,
                        info.default,
                    ));
                }
            }
            Ok(())
        }

        fn param(&self, name: &str) -> Result<(u32, f64, f64, f64), String> {
            self.params
                .iter()
                .find(|p| p.1 == name)
                .map(|p| (p.0, p.2, p.3, p.4))
                .ok_or_else(|| format!("no parameter named {name:?}"))
        }

        /// Runs one block of 256 samples and returns what the plugin sent.
        fn run(&self, inputs: &[Input]) -> Result<Vec<Sent>, String> {
            let header = |time: u32, size: usize, kind: u16| EventHeader {
                size: u32::try_from(size).unwrap_or(0),
                time,
                space_id: 0,
                kind,
                flags: 0,
            };
            let mut params = Vec::new();
            let mut midis = Vec::new();
            for input in inputs {
                match *input {
                    Input::Param { id, value, time } => params.push(Box::new(ParamValueEvent {
                        header: header(time, size_of::<ParamValueEvent>(), EVENT_PARAM_VALUE),
                        param_id: id,
                        cookie: std::ptr::null_mut(),
                        note_id: -1,
                        port_index: -1,
                        channel: -1,
                        key: -1,
                        value,
                    })),
                    Input::Cc {
                        controller,
                        value,
                        time,
                    } => midis.push(Box::new(MidiEvent {
                        header: header(time, size_of::<MidiEvent>(), EVENT_MIDI),
                        port_index: 0,
                        data: [0xB0, controller, value],
                    })),
                }
            }
            // Events in time order, as a host sends them.
            let mut ordered: Vec<(u32, *const c_void)> = params
                .iter()
                .map(|e| {
                    (
                        e.header.time,
                        &**e as *const ParamValueEvent as *const c_void,
                    )
                })
                .chain(
                    midis
                        .iter()
                        .map(|e| (e.header.time, &**e as *const MidiEvent as *const c_void)),
                )
                .collect();
            ordered.sort_by_key(|e| e.0);
            let mut events: Vec<*const c_void> = ordered.into_iter().map(|e| e.1).collect();
            let mut sent: Vec<Sent> = Vec::new();
            let in_events = InEvents {
                ctx: &mut events as *mut Vec<*const c_void> as *mut c_void,
                size: events_size,
                get: events_get,
            };
            let out_events = OutEvents {
                ctx: &mut sent as *mut Vec<Sent> as *mut c_void,
                try_push: events_push,
            };
            let process = Process {
                steady_time: -1,
                frames_count: 256,
                transport: std::ptr::null(),
                audio_inputs: std::ptr::null(),
                audio_outputs: std::ptr::null_mut(),
                audio_inputs_count: 0,
                audio_outputs_count: 0,
                in_events: &in_events,
                out_events: &out_events,
            };
            // SAFETY: the plugin is processing; every pointer in `process` outlives the call.
            let status = unsafe { ((*self.plugin).process)(self.plugin, &process) };
            check(
                status != 0,
                format!("process returned the error status {status}"),
            )?;
            Ok(sent)
        }

        fn save_state(&self) -> Result<Vec<u8>, String> {
            let mut bytes: Vec<u8> = Vec::new();
            let stream = OutStream {
                ctx: &mut bytes as *mut Vec<u8> as *mut c_void,
                write: stream_write,
            };
            // SAFETY: state extension of a live plugin, stream valid for the call.
            unsafe {
                let ext = self.extension::<StateExt>(c"clap.state")?;
                check(((*ext).save)(self.plugin, &stream), "state save failed")?;
            }
            Ok(bytes)
        }

        fn load_state(&self, bytes: Vec<u8>) -> Result<(), String> {
            let mut source = (bytes, 0usize);
            let stream = InStream {
                ctx: &mut source as *mut (Vec<u8>, usize) as *mut c_void,
                read: stream_read,
            };
            // SAFETY: as in `save_state`.
            unsafe {
                let ext = self.extension::<StateExt>(c"clap.state")?;
                check(((*ext).load)(self.plugin, &stream), "state load failed")?;
            }
            Ok(())
        }
    }

    impl Drop for Instance {
        fn drop(&mut self) {
            // SAFETY: the plugin is processing and was created by `create`; it is not used afterwards.
            unsafe {
                ((*self.plugin).stop_processing)(self.plugin);
                ((*self.plugin).deactivate)(self.plugin);
                ((*self.plugin).destroy)(self.plugin);
            }
        }
    }

    fn library_path(argument: &Path) -> PathBuf {
        if argument.is_dir() {
            let stem = argument
                .file_stem()
                .map(|s| s.to_os_string())
                .unwrap_or_default();
            argument.join("Contents").join("MacOS").join(stem)
        } else {
            argument.to_path_buf()
        }
    }

    fn nrpn_bytes(channel: u8, number: u16, value: u16) -> [[u8; 3]; 4] {
        let status = 0xB0 | channel;
        [
            [status, 99, (number >> 7) as u8],
            [status, 98, (number & 0x7F) as u8],
            [status, 6, (value >> 7) as u8],
            [status, 38, (value & 0x7F) as u8],
        ]
    }

    fn run() -> Result<(), String> {
        let argument = std::env::args()
            .nth(1)
            .ok_or("usage: host_clap <bundle.clap>")?;
        let path = library_path(Path::new(&argument));
        let c_path = CString::new(path.to_string_lossy().as_bytes()).map_err(|e| e.to_string())?;
        // SAFETY: loading the plugin library is the point of this host; the entry point and factory follow the CLAP ABI.
        let (entry, factory) = unsafe {
            let module = dlopen(c_path.as_ptr(), RTLD_NOW);
            check(!module.is_null(), format!("dlopen failed for {path:?}"))?;
            let entry = dlsym(module, c"clap_entry".as_ptr()) as *const Entry;
            check(!entry.is_null(), "no clap_entry symbol")?;
            check(((*entry).init)(c_path.as_ptr()), "entry init failed")?;
            let factory = ((*entry).get_factory)(c"clap.plugin-factory".as_ptr()) as *const Factory;
            check(!factory.is_null(), "no plugin factory")?;
            (entry, factory)
        };
        // SAFETY: `factory` was just checked.
        let (id, name) = unsafe {
            check(
                ((*factory).get_plugin_count)(factory) == 1,
                "expected one plugin",
            )?;
            let descriptor = ((*factory).get_plugin_descriptor)(factory, 0);
            check(!descriptor.is_null(), "no descriptor")?;
            (text((*descriptor).id), text((*descriptor).name))
        };
        println!("plugin {name:?} ({id})");
        let c_id = CString::new(id).map_err(|e| e.to_string())?;

        // SAFETY: `factory` is valid for the whole run.
        let a = unsafe { Instance::create(factory, &c_id)? };
        println!("  parameters: {}", a.params.len());
        check(a.params.len() == 140 + 6, "unexpected parameter count")?;
        let (lp, lp_min, lp_max, lp_default) = a.param("Low-pass filter LP Freq #34")?;
        check(
            (lp_min, lp_max, lp_default) == (0.0, 164.0, 164.0),
            "LP Freq range or default",
        )?;

        check(
            a.run(&[])?.is_empty(),
            "a block without events must send nothing",
        )?;
        println!("  baseline block: nothing sent");

        // 3. an automated change at sample 100 and an unmapped CC at sample 10
        let sent = a.run(&[
            Input::Cc {
                controller: 1,
                value: 9,
                time: 10,
            },
            Input::Param {
                id: lp,
                value: 100.0,
                time: 100,
            },
        ])?;
        let mut expected = vec![Sent {
            time: 10,
            bytes: [0xB0, 1, 9],
        }];
        expected.extend(nrpn_bytes(0, 34, 100).map(|bytes| Sent { time: 100, bytes }));
        check(
            sent == expected,
            format!("automation: expected {expected:?}, got {sent:?}"),
        )?;
        println!(
            "  automation at sample 100 -> NRPN 34 = 100 at sample 100, CC 1 passed through at 10"
        );
        check(a.run(&[])?.is_empty(), "no repeat on the next block")?;

        // 4. learn: switch learn on, touch Resonance (n35), move CC 74, then drive it
        let (learn, ..) = a.param("MIDI Learn")?;
        let (resonance, ..) = a.param("Low-pass filter Resonance #35")?;
        a.run(&[Input::Param {
            id: learn,
            value: 1.0,
            time: 0,
        }])?;
        let touch = a.run(&[Input::Param {
            id: resonance,
            value: 10.0,
            time: 0,
        }])?;
        check(touch.len() == 4, "the touched parameter is sent once")?;
        let learned = a.run(&[Input::Cc {
            controller: 74,
            value: 64,
            time: 0,
        }])?;
        check(
            learned.is_empty(),
            format!("the learning CC is consumed, got {learned:?}"),
        )?;
        let driven = a.run(&[Input::Cc {
            controller: 74,
            value: 127,
            time: 20,
        }])?;
        let expected: Vec<Sent> = nrpn_bytes(0, 35, 127)
            .map(|bytes| Sent { time: 20, bytes })
            .to_vec();
        check(
            driven == expected,
            format!("learned CC: expected {expected:?}, got {driven:?}"),
        )?;
        println!("  learn: CC 74 -> NRPN 35, 127 at sample 20 (CC consumed)");

        // 5. state through the real extension, then a second instance
        let state = a.save_state()?;
        let readable = String::from_utf8_lossy(&state);
        check(
            readable.contains("74=n35"),
            "the saved state holds the mapping",
        )?;
        println!("  state: {} bytes, mapping present", state.len());
        // SAFETY: as for `a`.
        let b = unsafe { Instance::create(factory, &c_id)? };
        b.load_state(state)?;
        // deactivate / reactivate so the restored values become the baseline, as hosts do after a project load
        // SAFETY: valid plugin; the host calls these from the main thread, here the only thread.
        unsafe {
            ((*b.plugin).stop_processing)(b.plugin);
            ((*b.plugin).deactivate)(b.plugin);
            check(
                ((*b.plugin).activate)(b.plugin, 48_000.0, 1, 512),
                "reactivate failed",
            )?;
            check(((*b.plugin).start_processing)(b.plugin), "restart failed")?;
        }
        check(
            b.run(&[])?.is_empty(),
            "restored values are the baseline, nothing is sent",
        )?;
        let again = b.run(&[Input::Cc {
            controller: 74,
            value: 127,
            time: 20,
        }])?;
        check(
            again == expected,
            format!("restored mapping: expected {expected:?}, got {again:?}"),
        )?;
        println!("  second instance restored from the state: CC 74 -> NRPN 35 again");

        // counter-proof: a fresh instance without the state has no mapping, so CC 74 is passed through
        let fresh = unsafe { Instance::create(factory, &c_id)? };
        fresh.run(&[])?;
        let passed = fresh.run(&[Input::Cc {
            controller: 74,
            value: 127,
            time: 20,
        }])?;
        check(
            passed
                == vec![Sent {
                    time: 20,
                    bytes: [0xB0, 74, 127],
                }],
            format!("fresh instance: CC 74 must pass through, got {passed:?}"),
        )?;
        println!("  counter-proof: a fresh instance passes CC 74 through unchanged");

        drop(a);
        drop(b);
        drop(fresh);
        // SAFETY: every instance is destroyed.
        unsafe { ((*entry).deinit)() };
        println!("host_clap: all checks passed");
        Ok(())
    }

    pub fn main() {
        if let Err(message) = run() {
            eprintln!("host_clap FAILED: {message}");
            std::process::exit(1);
        }
    }
}

#[cfg(unix)]
fn main() {
    unix_host::main();
}

#[cfg(not(unix))]
fn main() {
    eprintln!("host_clap needs dlopen: macOS and Linux only");
}
