// HardwarePane.swift — the device, its endpoints, and the two ways to flash it.
//
// What was removed here, and why:
//
//   "Auxiliary Hardware Keypad Buttons". A static block describing three
//   mechanical buttons at key IDs 0x01/0x02/0x03. This knob has at most ONE
//   button: `probe-gestures --map` put a distinct marker on keys 1-6, ran
//   all five gestures, and found keys 2..6 are the gestures themselves. So
//   the section described key IDs that belong to the knob, offered no
//   control of any kind, and pointed at a "buttons array" in an editor whose
//   templates all declared `buttons: []`.
//
//   Every keymap template's colour. `led: backlight green`, `backlight cyan`,
//   `backlight red`, `backlight blue`, `backlight white` -- five templates
//   naming five colours, and `backlight` is an alias for mode 1, which is
//   RED on this device whatever colour follows it. All five produced red.
//
//   Every keymap template's layout. `buttons: []` with no rows or columns
//   declares zero buttons, which puts the knob's gestures at key IDs 1/2/3.
//   On this hardware key 1 is driven by nothing and the gestures start at 2,
//   so flashing any template wrote `ccw` to a dead slot, `press` to twist,
//   and `cw` to press. The device accepts all of it and reports no error.
//
// The chord text was wrong in the other direction: it said ⌃⌥F16..F20 while
// `bind-slots` flashed three chords. It flashes five now, so the text is
// true and the two hold+twist gestures can reach the daemon.

import AppKit
import SwiftUI

/// Which device layer a slot-binding flash targets.
///
/// An enum rather than bare `-1 / 0 / 1 / 2` tags so the labels can be
/// enumerated -- `LayoutTests` renders every one and fails if it would not
/// fit `Layout.dropdown`.
enum SlotTarget: Int, CaseIterable, Identifiable {
    case allLayers = -1
    case layer1 = 0
    case layer2 = 1
    case layer3 = 2

    var id: Int { rawValue }

    var label: String {
        switch self {
        case .allLayers: return "All Layers (0..2)"
        case .layer1: return "Layer 1 (0)"
        case .layer2: return "Layer 2 (1)"
        case .layer3: return "Layer 3 (2)"
        }
    }
}

struct HardwarePane: View {
    @ObservedObject var store: ConfigStore
    @State private var selectedSlotLayer: Int = -1 // -1 = All layers

    // Module-internal rather than private: the standalone-flash half of this
    // pane is an extension in HardwareKeymap.swift, and an extension in
    // another file cannot see `private` members. Same arrangement as
    // LayerDetail / LayerBindings and InspectorSections / InspectorActions.
    @State var selectedTemplate: KeymapTemplate = .media
    @State var yamlText: String = KeymapTemplate.media.defaultYaml
    @State var isFlashingKeymap: Bool = false
    @State var flashStatus: String?
    @State var flashSuccess: Bool = true

    var body: some View {
        Form {
            endpointSection
            slotBindingSection
            standaloneKeymapSection
            // The Inspector's three sections. They read the same device this
            // pane flashes, and were a tab away from it.
            InspectorSections(store: store)
        }
        .formStyle(.grouped)
        .onAppear { store.refreshKnobMode() }
    }

    /// Endpoints as a five-column table: index, transport tag, device name,
    /// device path, and whether this is the one being driven.
    private var endpointSection: some View {
        Section("HID Endpoints") {
            if store.devices.isEmpty {
                Text("No active HID endpoints")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            } else {
                PropertyGrid(horizontalSpacing: 14, verticalSpacing: 6) {
                    GridRow {
                        Text("#").gridColumnAlignment(.leading)
                        Text("Transport").gridColumnAlignment(.leading)
                        Text("Device").gridColumnAlignment(.leading)
                        Text("Path").gridColumnAlignment(.leading)
                        Text("").gridColumnAlignment(.leading)
                    }
                    .font(.caption2.weight(.semibold))
                    .foregroundStyle(.secondary)
                    .textCase(.uppercase)

                    ForEach(Array(store.devices.enumerated()), id: \.offset) { idx, dev in
                        endpointRow(index: idx, device: dev)
                    }
                }
            }
        }
    }

    /// The vendor configuration endpoint, which is the one every command
    /// goes to. Nineteen near-identical rows with nothing marking the one
    /// that matters is a list, not information.
    private static let vendorUsagePage = 0xFF00

    private func endpointRow(index: Int, device dev: [String: Any]) -> some View {
        let isTarget = (dev["usage_page"] as? Int) == Self.vendorUsagePage
        return GridRow {
            Text("\(index + 1)")
                .font(.caption.monospacedDigit())
                .foregroundStyle(.secondary)
            TagPill(text: dev["transport"] as? String ?? "")
            Text(dev["name"] as? String ?? "Unknown Device")
                .font(.caption.weight(isTarget ? .semibold : .medium))
            Text(dev["path"] as? String ?? "")
                .font(.system(size: 10, design: .monospaced))
                .foregroundStyle(.secondary)
            Text(isTarget ? "in use" : "")
                .font(.caption2)
                .foregroundStyle(Color.accentColor)
        }
    }

    private var slotBindingSection: some View {
        Section {
            VStack(alignment: .leading, spacing: 8) {
                PropertyGrid {
                    GridRow {
                        Text("Target Layer")
                            .foregroundStyle(.secondary)
                            .frame(width: Layout.controlLabel, alignment: .leading)
                            .gridColumnAlignment(.leading)
                        Dropdown(title: SlotTarget(rawValue: selectedSlotLayer)?.label ?? "") {
                            Picker("", selection: $selectedSlotLayer) {
                                ForEach(SlotTarget.allCases) { t in
                                    Text(t.label).tag(t.rawValue)
                                }
                            }
                            .pickerStyle(.inline).labelsHidden()
                        }
                        .gridColumnAlignment(.leading)

                        Button {
                            let l = selectedSlotLayer >= 0 ? selectedSlotLayer : nil
                            store.bindSlots(layer: l)
                        } label: {
                            if store.isBindingSlots {
                                HStack(spacing: 6) {
                                    ProgressView().controlSize(.small)
                                    Text("Flashing slots…")
                                }
                            } else {
                                Label("Flash Slot Bindings", systemImage: "bolt.fill")
                            }
                        }
                        .disabled(store.isBindingSlots || !store.canFlashHardware)
                        .help(!store.hardwareConnected
                              ? "No knob detected"
                              : (!store.canFlashHardware
                                 ? "Connect knob via USB-C cable to flash hardware"
                                 : "Flash host translation chords to the knob"))
                        .gridColumnAlignment(.leading)
                    }
                }
                .padding(.top, 4)

                slotLayoutReadout
            }
            .padding(.vertical, 4)
        } header: {
            Text("Host Translation Chords")
        } footer: {
            Text("""
                Twist Left=⌃⌥F16, Press=⌃⌥F17, Twist Right=⌃⌥F18, \
                Hold+Twist Left=⌃⌥F19, Hold+Twist Right=⌃⌥F20.
                """)
        }
    }

    /// Which key IDs a flash will write, read from the daemon.
    ///
    /// The declared button count places the knob's slots, so a layout that
    /// declares more buttons than the device has moves every gesture along
    /// by that many -- and both the write and the device's acceptance of it
    /// are indistinguishable from a correct flash. Naming the slots is the
    /// only place that becomes visible before someone wonders why a freshly
    /// flashed knob does nothing.
    @ViewBuilder
    private var slotLayoutReadout: some View {
        if let keys = store.knobKeyIds, let buttons = store.knobButtons {
            Text("Writes key IDs \(keys.map(String.init).joined(separator: ", ")) "
               + "— the layout declares \(buttons) button\(buttons == 1 ? "" : "s") "
               + "before the knob.")
                .font(.caption2)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
        } else {
            Text("Slot layout not read yet.")
                .font(.caption2)
                .foregroundStyle(.secondary)
        }
    }
}
