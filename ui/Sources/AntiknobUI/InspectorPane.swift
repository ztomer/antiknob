// InspectorPane.swift — Live HID Traffic Snooper, Slot Memory Dump, and Raw Packet Console.
// Provides real-time packet inspection, reverse-engineering diagnostics, and wire testing.

import AppKit
import SwiftUI

struct PacketLogItem: Identifiable {
    let id = UUID()
    let timestamp: Date
    let iface: String
    let bytesHex: String
    let decode: String
}

struct SlotDumpRecord: Identifiable {
    let id = UUID()
    let group: UInt8
    let counter: UInt8
    let hex: String
}

struct InspectorPane: View {
    @ObservedObject var store: ConfigStore
    @State private var isSnooping: Bool = false
    @State private var snoopProcess: Process?
    @State private var logs: [PacketLogItem] = []

    @State private var isReadingSlots: Bool = false
    @State private var slotGroup: UInt8 = 0x0F
    @State private var slotRecords: [SlotDumpRecord] = []
    @State private var slotError: String?

    @State private var rawPacketInput: String = "FD FE FF"
    @State private var rawPacketStatus: String?

    var body: some View {
        Form {
            trafficSnooperSection
            slotMemorySection
            rawConsoleSection
        }
        .formStyle(.grouped)
    }

    private var trafficSnooperSection: some View {
        Section {
            VStack(alignment: .leading, spacing: 8) {
                Text("""
                    Non-exclusive packet monitor. Snoops keyboard, mouse, and vendor \
                    endpoints while the OS continues receiving inputs.
                    """)
                    .font(.caption)
                    .foregroundStyle(.secondary)

                HStack {
                    Button {
                        if isSnooping {
                            stopSnooping()
                        } else {
                            startSnooping()
                        }
                    } label: {
                        HStack(spacing: 6) {
                            Circle()
                                .fill(isSnooping ? Color.green : Color.secondary)
                                .frame(width: 8, height: 8)
                            Text(isSnooping ? "Stop Live Snoop" : "Start Live Traffic Snoop")
                        }
                    }

                    Button("Clear Traffic Log") {
                        logs.removeAll()
                    }
                    .disabled(logs.isEmpty)

                    Spacer()
                }

                if logs.isEmpty {
                    Text("No packets captured yet. Start snooping and twist or press the knob.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .padding(.vertical, 12)
                } else {
                    ScrollView {
                        LazyVStack(alignment: .leading, spacing: 4) {
                            ForEach(logs) { item in
                                VStack(alignment: .leading, spacing: 2) {
                                    HStack {
                                        Text(timeStr(item.timestamp))
                                            .font(.caption2)
                                            .foregroundStyle(.secondary)
                                        Text(item.iface)
                                            .font(.caption2)
                                            .padding(.horizontal, 4)
                                            .padding(.vertical, 1)
                                            .background(Capsule().fill(.quaternary))
                                        Spacer()
                                        if !item.decode.isEmpty {
                                            Text(item.decode)
                                                .font(.caption2)
                                                .fontWeight(.medium)
                                                .foregroundStyle(Color.accentColor)
                                        }
                                    }
                                    Text(item.bytesHex)
                                        .font(.system(.caption, design: .monospaced))
                                        .foregroundStyle(.secondary)
                                }
                                .padding(.vertical, 2)
                                Divider()
                            }
                        }
                    }
                    .frame(maxHeight: 180)
                }
            }
            .padding(.vertical, 4)
        } header: {
            Text("Live HID Wire Snoop")
        } footer: {
            Text("Opens all device interfaces non-exclusively (vendor 0xFF00, keyboard 0x01:0x06, mouse 0x01:0x02).")
        }
    }

    private var slotMemorySection: some View {
        Section {
            VStack(alignment: .leading, spacing: 8) {
                Text("""
                    Inspect on-chip slot table records. Reads raw 64-byte responses via \
                    vendor query [0xFA group 0x00 counter].
                    """)
                    .font(.caption)
                    .foregroundStyle(.secondary)

                HStack {
                    Picker("Memory Group", selection: $slotGroup) {
                        Text("Group 0x0F (Primary)").tag(UInt8(0x0F))
                        Text("Group 0x19 (Secondary)").tag(UInt8(0x19))
                    }
                    .frame(maxWidth: 220)

                    Spacer()

                    Button {
                        readSlotTable()
                    } label: {
                        if isReadingSlots {
                            HStack(spacing: 6) {
                                ProgressView().controlSize(.small)
                                Text("Reading memory…")
                            }
                        } else {
                            Label("Dump Slot Memory", systemImage: "memorychip")
                        }
                    }
                    .disabled(isReadingSlots || !store.hardwareConnected)
                }

                if let err = slotError {
                    Text(err)
                        .font(.caption2)
                        .foregroundStyle(.red)
                }

                if !slotRecords.isEmpty {
                    VStack(alignment: .leading, spacing: 6) {
                        ForEach(slotRecords) { rec in
                            VStack(alignment: .leading, spacing: 2) {
                                Text("Group 0x\(String(format: "%02X", rec.group)) · Slot Entry \(rec.counter)")
                                    .font(.caption2)
                                    .fontWeight(.semibold)
                                Text(rec.hex)
                                    .font(.system(.caption2, design: .monospaced))
                                    .foregroundStyle(.secondary)
                            }
                            .padding(6)
                            .background(RoundedRectangle(cornerRadius: 4).fill(Color.secondary.opacity(0.1)))
                        }
                    }
                }
            }
            .padding(.vertical, 4)
        } header: {
            Text("Slot Table Memory Dump")
        }
    }

    private var rawConsoleSection: some View {
        Section {
            VStack(alignment: .leading, spacing: 8) {
                Text("Send custom 64-byte raw HID report payloads (Report ID 0x03) directly to the device.")
                    .font(.caption)
                    .foregroundStyle(.secondary)

                HStack {
                    TextField("Hex Bytes (e.g. FD FE FF)", text: $rawPacketInput)
                        .font(.system(.body, design: .monospaced))

                    Button("Send") {
                        sendRaw()
                    }
                    .disabled(!store.hardwareConnected)
                }

                HStack(spacing: 8) {
                    Text("Presets:")
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                    Button("Commit (FD FE FF)") { rawPacketInput = "FD FE FF" }
                        .buttonStyle(.link)
                        .font(.caption2)
                    Button("Query LED (FA B0 00)") { rawPacketInput = "FA B0 00" }
                        .buttonStyle(.link)
                        .font(.caption2)
                    Button("Query Slot (FA 0F 00 01)") { rawPacketInput = "FA 0F 00 01" }
                        .buttonStyle(.link)
                        .font(.caption2)
                }

                if let status = rawPacketStatus {
                    Text(status)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
            .padding(.vertical, 4)
        } header: {
            Text("Raw HID Packet Console")
        }
    }

    // MARK: - Snoop Execution

    private func startSnooping() {
        isSnooping = true
        let task = Process()
        let pipe = Pipe()

        var possiblePaths: [String] = [
            "/Applications/Antiknob/bin/antiknob",
            "\(FileManager.default.homeDirectoryForCurrentUser.path)/.local/bin/antiknob",
            "/usr/local/bin/antiknob"
        ]
        if let res = Bundle.main.resourcePath {
            possiblePaths.append("\(res)/antiknob")
        }

        guard let exec = possiblePaths.first(where: { FileManager.default.isExecutableFile(atPath: $0) }) else {
            isSnooping = false
            return
        }

        task.executableURL = URL(fileURLWithPath: exec)
        task.arguments = ["listen", "--timeout-secs", "30"]
        task.standardOutput = pipe
        task.standardError = Pipe()

        pipe.fileHandleForReading.readabilityHandler = { handle in
            let data = handle.availableData
            guard !data.isEmpty, let text = String(data: data, encoding: .utf8) else { return }
            let lines = text.components(separatedBy: .newlines).filter { !$0.isEmpty }
            DispatchQueue.main.async {
                for line in lines {
                    if line.contains("[") && line.contains("B:") {
                        parseSnoopLine(line)
                    }
                }
            }
        }

        task.terminationHandler = { _ in
            DispatchQueue.main.async {
                self.isSnooping = false
            }
        }

        do {
            try task.run()
            self.snoopProcess = task
        } catch {
            isSnooping = false
        }
    }

    private func stopSnooping() {
        snoopProcess?.terminate()
        snoopProcess = nil
        isSnooping = false
    }

    private func parseSnoopLine(_ line: String) {
        let parts = line.components(separatedBy: "   ")
        let head = parts.first ?? line
        let decode = parts.count > 1 ? parts[1] : ""
        let item = PacketLogItem(
            timestamp: Date(),
            iface: "HID",
            bytesHex: head.trimmingCharacters(in: .whitespaces),
            decode: decode.trimmingCharacters(in: .whitespaces)
        )
        logs.insert(item, at: 0)
        if logs.count > 100 { logs.removeLast() }
    }

    private func readSlotTable() {
        isReadingSlots = true
        slotError = nil
        slotRecords = []

        store.readSlots(group: slotGroup, counters: [1, 2, 3]) { result in
            isReadingSlots = false
            switch result {
            case .success(let slots):
                var recs: [SlotDumpRecord] = []
                for s in slots {
                    let g = (s["group"] as? UInt8) ?? slotGroup
                    let c = (s["counter"] as? UInt8) ?? 0
                    let h = (s["hex"] as? String) ?? (s["error"] as? String ?? "")
                    recs.append(SlotDumpRecord(group: g, counter: c, hex: h))
                }
                slotRecords = recs
            case .failure(let err):
                slotError = err.localizedDescription
            }
        }
    }

    private func sendRaw() {
        rawPacketStatus = "Sending..."
        store.sendRawPacket(hexString: rawPacketInput) { result in
            switch result {
            case .success(let msg):
                rawPacketStatus = "Success: \(msg)"
            case .failure(let err):
                rawPacketStatus = "Error: \(err.localizedDescription)"
            }
        }
    }

    private func timeStr(_ date: Date) -> String {
        let f = DateFormatter()
        f.dateFormat = "HH:mm:ss.SSS"
        return f.string(from: date)
    }
}
