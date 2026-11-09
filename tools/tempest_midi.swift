// MIDI dialogue with the Tempest through CoreMIDI (USB port "Tempest"), no dependency.
//
// usage: swift tools/tempest_midi.swift identity
//           universal identity request (F0 7E 7F 06 01 F7); prints the reply
//        swift tools/tempest_midi.swift send <file.syx> [bytes/s]
//           sends a SysEx file at the given rate (default 3125 B/s, that of a MIDI DIN cable), prints the
//           progress then everything the Tempest sends back for 10 s
//        swift tools/tempest_midi.swift listen <seconds>
//           prints everything the Tempest sends
//        swift tools/tempest_midi.swift request "<hex bytes>" <output.syx> [seconds]
//           sends a message (e.g. F0 01 28 40 F7: dump of the current sound, Main Tempester ≥ 1.5.2.0) and writes to the
//           file everything that comes back during the delay (default 5 s, clock and active sensing excluded)
//
// WARNING: "send" writes to the machine (a Main OS reflashes it). Use it only with the author's explicit
// agreement, for one given file. The Tempest listens to USB SysEx only if System > "MIDI: Sysex IN+OUT Cable"
// is USB. Limit: a single port, named exactly "Tempest".
import CoreMIDI
import Foundation

func displayName(_ object: MIDIObjectRef) -> String {
    var s: Unmanaged<CFString>?
    MIDIObjectGetStringProperty(object, kMIDIPropertyDisplayName, &s)
    return (s?.takeRetainedValue() as String?) ?? "?"
}

func hexString(_ bytes: [UInt8]) -> String { bytes.map { String(format: "%02X", $0) }.joined(separator: " ") }

let args = CommandLine.arguments
guard args.count >= 2 else { print("usage: identity | send <file.syx> [bytes/s] | listen <s> | request \"<hex>\" <output.syx> [s]"); exit(2) }
guard let source = (0..<MIDIGetNumberOfSources()).map(MIDIGetSource).first(where: { displayName($0) == "Tempest" }),
      let destination = (0..<MIDIGetNumberOfDestinations()).map(MIDIGetDestination).first(where: { displayName($0) == "Tempest" })
else { print("no \"Tempest\" port: is the machine plugged in and switched on?"); exit(1) }

var received: [UInt8] = []
let lock = NSLock()
var client = MIDIClientRef()
MIDIClientCreate("Tempester" as CFString, nil, nil, &client)
var input = MIDIPortRef()
MIDIInputPortCreateWithBlock(client, "input" as CFString, &input) { packetList, _ in
    var packet = packetList.pointee.packet
    for _ in 0..<packetList.pointee.numPackets {
        let bytes = withUnsafeBytes(of: packet.data) { Array($0.prefix(Int(packet.length))) }
        let useful = bytes.filter { $0 != 0xF8 && $0 != 0xFE }
        lock.lock(); received += useful; lock.unlock()
        if !useful.isEmpty && args.count >= 2 && args[1] != "request" { print("received:", hexString(useful)) }
        packet = MIDIPacketNext(&packet).pointee
    }
}
MIDIPortConnectSource(input, source, nil)
var output = MIDIPortRef()
MIDIOutputPortCreate(client, "output" as CFString, &output)

func send(_ bytes: [UInt8]) {
    var packetList = MIDIPacketList()
    let packet = MIDIPacketListInit(&packetList)
    _ = MIDIPacketListAdd(&packetList, MemoryLayout<MIDIPacketList>.size, packet, 0, bytes.count, bytes)
    MIDISend(output, destination, &packetList)
}

func wait(_ seconds: Double) { RunLoop.main.run(until: Date().addingTimeInterval(seconds)) }

switch args[1] {
case "identity":
    send([0xF0, 0x7E, 0x7F, 0x06, 0x01, 0xF7])
    print("sent: F0 7E 7F 06 01 F7")
    wait(2)
case "listen":
    wait(Double(args.count > 2 ? args[2] : "10") ?? 10)
case "send":
    guard args.count >= 3, let data = FileManager.default.contents(atPath: args[2]) else { print("unreadable file"); exit(1) }
    let bytes = [UInt8](data)
    guard bytes.first == 0xF0, bytes.last == 0xF7 else { print("refused: not a SysEx F0 … F7"); exit(1) }
    let rate = Double(args.count > 3 ? args[3] : "3125") ?? 3125
    let chunk = 128                                   // bytes per packet: one packet every 41 ms at 3125 B/s
    let start = Date()
    var sent = 0
    while sent < bytes.count {
        let end = min(sent + chunk, bytes.count)
        send(Array(bytes[sent..<end]))
        sent = end
        let lag = Double(sent) / rate - Date().timeIntervalSince(start)
        if lag > 0 { wait(lag) }
        if sent % (chunk * 160) == 0 || sent == bytes.count {
            print(String(format: "%6.1f %%  %d / %d bytes  %.0f s", 100.0 * Double(sent) / Double(bytes.count),
                         sent, bytes.count, Date().timeIntervalSince(start)))
        }
    }
    print("send finished; listening for 10 s")
    wait(10)
case "request":
    guard args.count >= 4 else { print("usage: request \"F0 … F7\" <output.syx> [seconds]"); exit(2) }
    let message = args[2].split(separator: " ").compactMap { UInt8($0, radix: 16) }
    send(message)
    print("sent:", hexString(message))
    wait(Double(args.count > 4 ? args[4] : "5") ?? 5)
    lock.lock(); let data = Data(received); lock.unlock()
    FileManager.default.createFile(atPath: args[3], contents: data)
    print("received \(data.count) bytes -> \(args[3])", data.count > 0 ? "(start: \(hexString(Array(data.prefix(8)))))" : "")
default:
    print("unknown command: \(args[1])"); exit(2)
}
