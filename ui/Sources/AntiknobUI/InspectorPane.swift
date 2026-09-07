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

/// The two slot-table banks the firmware exposes.
enum SlotMemoryGroup: UInt8, CaseIterable, Identifiable {
    case primary = 0x0F
    case secondary = 0x19

    var id: UInt8 { rawValue }

    var label: String {
        switch self {
        case .primary: return "Group 0x0F (Primary)"
        case .secondary: return "Group 0x19 (Secondary)"
        }
    }
}

struct InspectorPane: View {
    @ObservedObject var store: ConfigStore
    // Module-internal, not private: InspectorActions.swift is an extension on
    // this type in another file, and `private` does not reach across files.
    @State var isSnooping: Bool = false
    @State var snoopProcess: Process?
    @State var logs: [PacketLogItem] = []

    @State var isReadingSlots: Bool = false
    @State var slotGroup: UInt8 = 0x0F
    @State var slotRecords: [SlotDumpRecord] = []
    @State var slotError: String?

    @State var rawPacketInput: String = "FD FE FF"
    @State var rawPacketStatus: String?

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

                PropertyGrid {
                    GridRow {
                        Text("Memory Group")
                            .foregroundStyle(.secondary)
                            .frame(width: Layout.controlLabel, alignment: .leading)
                            .gridColumnAlignment(.leading)
                        Dropdown(title: SlotMemoryGroup(rawValue: slotGroup)?.label ?? "") {
                            Picker("", selection: $slotGroup) {
                                ForEach(SlotMemoryGroup.allCases) { g in
                                    Text(g.label).tag(g.rawValue)
                                }
                            }
                            .pickerStyle(.inline).labelsHidden()
                        }
                        .gridColumnAlignment(.leading)

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
                        .gridColumnAlignment(.leading)
                    }
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
}
