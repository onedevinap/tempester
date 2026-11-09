// Drives a Tempest set as clock slave over USB (port "Tempest"): FX recording tests: 3 s lead-in, MIDI Start, clock at 120 BPM (24 ppqn) for 16 s
// with pad notes 36/38/40/42 every 250 ms on channel 1, then MIDI Stop. usage: swiftc -O -o tempest_clock tools/tempest_clock.swift && ./tempest_clock [notes|no_notes]. Sends MIDI only (no SysEx).
import CoreMIDI
import Foundation
func name(_ e: MIDIObjectRef) -> String { var s: Unmanaged<CFString>?; MIDIObjectGetStringProperty(e, kMIDIPropertyDisplayName, &s); return (s?.takeRetainedValue() as String?) ?? "?" }
guard let dst = (0..<MIDIGetNumberOfDestinations()).map(MIDIGetDestination).first(where: { name($0) == "Tempest" }) else { print("no Tempest port"); exit(1) }
var c = MIDIClientRef(); MIDIClientCreate("tempest_clock" as CFString, nil, nil, &c)
var o = MIDIPortRef(); MIDIOutputPortCreate(c, "o" as CFString, &o)
func send(_ b: [UInt8]) { var l = MIDIPacketList(); let p = MIDIPacketListInit(&l); _ = MIDIPacketListAdd(&l, 1024, p, 0, b.count, b); MIDISend(o, dst, &l) }
let withNotes = CommandLine.arguments.count < 2 || CommandLine.arguments[1] == "notes"
let tick = 60.0 / 120.0 / 24.0
let notes: [UInt8] = [36, 38, 40, 42]
print("start in 3 s"); RunLoop.main.run(until: Date().addingTimeInterval(3))
send([0xFA])
let start = Date()
var current: UInt8 = 0
for t in 0..<(24 * 2 * 16) {
    send([0xF8])
    if withNotes && t % 12 == 0 { current = notes[(t / 12) % 4]; send([0x90, current, 100]) }
    if withNotes && t % 12 == 6 { send([0x80, current, 0]) }
    RunLoop.main.run(until: start.addingTimeInterval(Double(t + 1) * tick))
}
send([0xFC])
print("stop sent")
