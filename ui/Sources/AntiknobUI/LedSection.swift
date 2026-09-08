// LedSection.swift — the knob's backlight, and nothing this device cannot do.
//
// What used to be here and is not any more, with the reason:
//
//   Colour swatches. Eight of them, above a notice saying the firmware
//   ignores every one. A control the app itself documents as inert is not a
//   control; the mode carries its own colour on this device and that is the
//   whole of the colour story. (`set_led`'s colour argument still exists on
//   the wire for the 16-key variant that honours it -- that is a CLI matter,
//   not something to offer here as though it did something.)
//
//   The 16-LED ring, Solid Fill and Spectrum Gradient. Sixteen beads this
//   knob does not have, driven by two buttons that recoloured the picture
//   and not the light. Spectrum Gradient never even opened the socket.
//
//   A footer promising "steady backlight, breath cycles, and press-reactive
//   lighting", which named three things while the list above it named six
//   different ones.
//
// What is here instead: the six real modes, a preview that animates each one
// the way the knob renders it, and a read-back so the pane can say what the
// firmware actually holds rather than what was last clicked.

import SwiftUI

struct LedSection: View {
    @ObservedObject var store: ConfigStore

    @State private var selectedLayer: Int = 0
    @State private var selectedMode: String = "rainbow"
    @State private var liveApply: Bool = true

    /// What the firmware reported for `selectedLayer`, and whether that
    /// answer is current. `nil` is "not read yet", which is a third state
    /// and must not render as either a mode or a failure.
    @State private var firmwareMode: Int?
    @State private var readFailed: Bool = false
    @State private var isReading: Bool = false

    /// The daemon's own words about the last write. Not a locally invented
    /// "OK": `set_led` now reads the mode back and reports what it found,
    /// and this shows that.
    @State private var lastWrite: String?
    @State private var writeFailed: Bool = false

    private var mode: LedMode {
        LedMode.named(selectedMode) ?? LedMode.all[0]
    }

    /// True when the selection is what the firmware says it is holding.
    private var selectionIsLive: Bool {
        firmwareMode == mode.number
    }

    var body: some View {
        Form {
            layerSection
            previewSection
            modeSection
            actionSection
        }
        .formStyle(.grouped)
        .onAppear { readFirmwareMode() }
        .onChange(of: selectedLayer) { _, _ in
            lastWrite = nil
            readFirmwareMode()
        }
    }

    // MARK: - Device layer

    private var layerSection: some View {
        Section {
            Picker("Apply to", selection: $selectedLayer) {
                ForEach(0..<3, id: \.self) { i in
                    Text("Layer \(i + 1)").tag(i)
                }
            }
            .pickerStyle(.segmented)

            PropertyGrid {
                GridRow {
                    Text("On the knob")
                        .foregroundStyle(.secondary)
                        .gridColumnAlignment(.leading)
                    firmwareReadout
                        .gridColumnAlignment(.leading)
                    Button {
                        readFirmwareMode()
                    } label: {
                        if isReading {
                            ProgressView().controlSize(.small)
                        } else {
                            Image(systemName: "arrow.clockwise")
                        }
                    }
                    .buttonStyle(.borderless)
                    .disabled(isReading || !store.hardwareConnected)
                    .help("Re-read this layer's mode from the firmware")
                    .gridColumnAlignment(.leading)
                }
            }
        } header: {
            Text("Hardware Layer")
        } footer: {
            Text("""
                One mode per layer, stored on the knob. These are the firmware's own \
                three layers, not the host layers in the tabs above.
                """)
        }
    }

    @ViewBuilder
    private var firmwareReadout: some View {
        if !store.hardwareConnected {
            Text("No device").foregroundStyle(.secondary)
        } else if isReading {
            Text("Reading…").foregroundStyle(.secondary)
        } else if let number = firmwareMode {
            Text("\(LedMode.describe(number: number)) (mode \(number))")
        } else if readFailed {
            Label("Could not read", systemImage: "exclamationmark.triangle")
                .foregroundStyle(.orange)
        } else {
            Text("Not read yet").foregroundStyle(.secondary)
        }
    }

    // MARK: - Preview

    private var previewSection: some View {
        Section("Preview") {
            VStack(spacing: 10) {
                LedPreview(mode: mode, isLive: selectionIsLive)
                LedPreviewCaption(mode: mode, isLive: selectionIsLive)
            }
            .frame(maxWidth: .infinity)
            .padding(.vertical, 6)
        }
    }

    // MARK: - Modes

    /// Icon column width. SF Symbols differ in width, so without a fixed
    /// frame each row's text would start at a different x.
    private static let iconColumn: CGFloat = 20

    private var modeSection: some View {
        Section("Mode") {
            PropertyGrid(horizontalSpacing: 14, verticalSpacing: 9) {
                ForEach(LedMode.all) { m in
                    modeRow(m)
                }
            }
        }
    }

    /// Five columns: glyph, name, description, "on the knob" marker,
    /// selection mark.
    ///
    /// The two marks answer different questions and so get separate columns:
    /// one says what you have selected, the other says what the firmware is
    /// holding. Collapsing them would put the app back to presenting a
    /// selection as a fact about the hardware.
    private func modeRow(_ m: LedMode) -> some View {
        let isSelected = selectedMode == m.id
        let isOnDevice = firmwareMode == m.number
        return GridRow {
            Image(systemName: m.icon)
                .foregroundStyle(isSelected ? Color.accentColor : Color.secondary)
                .frame(width: Self.iconColumn, alignment: .leading)
                .gridColumnAlignment(.leading)

            Text(m.name)
                .foregroundStyle(isSelected ? Color.accentColor : Color.primary)
                .gridColumnAlignment(.leading)

            Text(m.desc)
                .font(.caption)
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, alignment: .leading)
                .gridColumnAlignment(.leading)

            Text("on the knob")
                .font(.caption2)
                .foregroundStyle(.secondary)
                .opacity(isOnDevice ? 1 : 0)
                .accessibilityHidden(!isOnDevice)
                .gridColumnAlignment(.leading)

            Image(systemName: "checkmark")
                .foregroundStyle(Color.accentColor)
                .fontWeight(.semibold)
                .opacity(isSelected ? 1 : 0)
                .accessibilityHidden(!isSelected)
                .gridColumnAlignment(.leading)
        }
        .contentShape(Rectangle())
        .onTapGesture {
            selectedMode = m.id
            if liveApply { sendLedUpdate() }
        }
    }

    // MARK: - Send

    private var actionSection: some View {
        Section {
            Toggle("Send on selection", isOn: $liveApply)

            HStack {
                Button {
                    sendLedUpdate()
                } label: {
                    Label("Send to Knob", systemImage: "bolt.fill")
                }
                .buttonStyle(.borderedProminent)
                .disabled(!store.hardwareConnected)

                Spacer()

                if let msg = lastWrite {
                    Label(msg, systemImage: writeFailed
                          ? "exclamationmark.triangle.fill" : "checkmark.circle.fill")
                        .font(.caption)
                        .foregroundStyle(writeFailed ? Color.red : Color.secondary)
                }
            }
        } footer: {
            if !store.hardwareConnected {
                Text("No knob detected, so nothing can be sent.")
            }
        }
    }

    private func sendLedUpdate() {
        lastWrite = nil
        // The colour argument is deliberately not sent. This knob's modes
        // carry their own colours; passing one would make the reply's `spec`
        // name a colour the firmware never applied.
        store.setLed(layer: selectedLayer, mode: selectedMode) { result in
            switch result {
            case .success(let applied):
                writeFailed = false
                firmwareMode = applied.mode
                lastWrite = applied.mode == mode.number
                    ? "Knob is now \(applied.name)"
                    : "Knob is still \(applied.name) — the write did not take"
                if applied.mode != mode.number {
                    writeFailed = true
                }
            case .failure(let error):
                writeFailed = true
                lastWrite = error.localizedDescription
            }
        }
    }

    private func readFirmwareMode() {
        guard store.hardwareConnected else {
            firmwareMode = nil
            readFailed = false
            return
        }
        isReading = true
        readFailed = false
        store.getHardwareLedMode(layer: selectedLayer) { number in
            isReading = false
            firmwareMode = number
            readFailed = (number == nil)
        }
    }
}
