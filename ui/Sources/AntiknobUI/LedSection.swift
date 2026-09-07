// LedSection.swift — Dynamic hardware LED lighting controls.
// Sends live set_led commands to antiknob-daemon over the Unix domain socket.

import SwiftUI

/// One selectable LED mode. A named type rather than a 4-tuple: every use
/// site read positionally before, and `id`/`name`/`desc` are all Strings.
struct LedMode: Identifiable, Hashable {
    let id: String
    let name: String
    let desc: String
    let icon: String

    /// The selectable modes, in display order.
    static let all: [LedMode] = [
            LedMode(id: "backlight", name: "Backlight",
                    desc: "Steady solid illumination", icon: "lightbulb.fill"),
            LedMode(id: "shock", name: "Shock (Breathe)",
                    desc: "Gentle pulsing breath", icon: "waveform.path"),
            LedMode(id: "shock2", name: "Shock 2 (Rapid)",
                    desc: "Faster pulse cycle", icon: "waveform.path.ecg"),
            LedMode(id: "press", name: "Press (Reactive)",
                    desc: "Lights up on knob press", icon: "hand.tap.fill"),
            LedMode(id: "off", name: "Off",
                    desc: "Disable LEDs to conserve power", icon: "power")
        ]
}

/// One swatch in the colour row. `hex` is the wire name sent to `set_led`.
struct LedColorPreset: Hashable {
    let name: String
    let hex: String
    let color: Color

    /// The swatch row, in display order.
    static let all: [LedColorPreset] = [
            LedColorPreset(name: "White", hex: "white", color: .white),
            LedColorPreset(name: "Red", hex: "red", color: .red),
            LedColorPreset(name: "Orange", hex: "orange", color: .orange),
            LedColorPreset(name: "Yellow", hex: "yellow", color: .yellow),
            LedColorPreset(name: "Green", hex: "green", color: .green),
            LedColorPreset(name: "Cyan", hex: "cyan", color: Color(red: 0, green: 0.9, blue: 0.9)),
            LedColorPreset(name: "Blue", hex: "blue", color: .blue),
            LedColorPreset(name: "Purple", hex: "purple", color: .purple)
        ]
}

struct LedSection: View {
    @ObservedObject var store: ConfigStore

    @State private var selectedLayer: Int = 0
    @State private var selectedMode: String = "backlight"
    @State private var selectedColorHex: String = "white"
    @State private var customColor: Color = .white
    @State private var liveApply: Bool = true

    @State private var beadColors: [Color] = Array(repeating: .white, count: 16)
    @State private var hardwareReadMode: String?
    @State private var isReadingMode: Bool = false

    var body: some View {
        Form {
            layerPickerSection
            ringVisualizerSection
            modeSelectionSection
            if selectedMode != "off" {
                colorSelectionSection
            }
            actionSection
        }
        .formStyle(.grouped)
    }

    private var layerPickerSection: some View {
        Section("Hardware Device Layer") {
            HStack {
                Picker("Apply to Layer", selection: $selectedLayer) {
                    ForEach(0..<3) { i in
                        Text("Device Layer \(i + 1)").tag(i)
                    }
                }
                .pickerStyle(.segmented)

                Button {
                    queryDeviceState()
                } label: {
                    if isReadingMode {
                        ProgressView().controlSize(.small)
                    } else {
                        Label("Query Firmware", systemImage: "arrow.clockwise")
                    }
                }
                .disabled(isReadingMode || !store.hardwareConnected)
            }

            if let readMode = hardwareReadMode {
                PropertyGrid {
                    PropertyRow(label: "Firmware Reported Mode") {
                        Text(readMode)
                            .font(.caption)
                            .foregroundStyle(Color.accentColor)
                    }
                }
            }
        }
    }

    private var ringVisualizerSection: some View {
        Section("16-LED RGB Ring Visualizer") {
            VStack(spacing: 12) {
                ZStack {
                    // Center knob icon
                    Circle()
                        .fill(Color(nsColor: .controlBackgroundColor))
                        .frame(width: 56, height: 56)
                        .overlay(Circle().strokeBorder(Color.secondary.opacity(0.4), lineWidth: 1))
                        .shadow(radius: 2)

                    Image(systemName: "dial.low.fill")
                        .font(.title2)
                        .foregroundStyle(.secondary)

                    // 16 Circular LED beads
                    ForEach(0..<16, id: \.self) { i in
                        let angle = Double(i) * (2.0 * .pi / 16.0) - (.pi / 2.0)
                        let radius: Double = 48.0
                        let x = cos(angle) * radius
                        let y = sin(angle) * radius
                        let col = selectedMode == "off" ? Color.secondary.opacity(0.3) : beadColors[i]

                        Circle()
                            .fill(col)
                            .frame(width: 12, height: 12)
                            .overlay(Circle().strokeBorder(Color.black.opacity(0.2), lineWidth: 0.5))
                            .shadow(color: col.opacity(selectedMode == "off" ? 0 : 0.8), radius: 3)
                            .offset(x: x, y: y)
                    }
                }
                .frame(width: 130, height: 130)
                .padding(.vertical, 4)

                HStack(spacing: 12) {
                    Button("Solid Fill") {
                        applySolidToRing()
                    }
                    .buttonStyle(.borderless)
                    .font(.caption)

                    Divider().frame(height: 12)

                    Button("Spectrum Gradient") {
                        applySpectrumToRing()
                    }
                    .buttonStyle(.borderless)
                    .font(.caption)

                    Divider().frame(height: 12)

                    Button("Clear (Off)") {
                        selectedMode = "off"
                        if liveApply { sendLedUpdate() }
                    }
                    .buttonStyle(.borderless)
                    .font(.caption)
                }
            }
            .frame(maxWidth: .infinity)
            .padding(.vertical, 6)
        }
    }

    /// Icon column width. SF Symbols differ in width (`power` is narrow,
    /// `waveform.path.ecg` wide), so without a fixed frame each row's text
    /// would start at a different x.
    private static let iconColumn: CGFloat = 20

    private var modeSelectionSection: some View {
        Section("Lighting Mode") {
            ForEach(LedMode.all, id: \.id) { m in
                modeRow(m)
                    .contentShape(Rectangle())
                    .onTapGesture {
                        selectedMode = m.id
                        if liveApply { sendLedUpdate() }
                    }
                    .padding(.vertical, 2)
            }
        }
    }

    /// Name over description in one left-aligned column.
    ///
    /// The description used to be pushed to the trailing edge by a `Spacer`,
    /// which left five lines of prose with five different left edges and no
    /// column for the eye to follow. The checkmark keeps its space when the
    /// row is unselected so selecting one does not shift the text.
    private func modeRow(_ m: LedMode) -> some View {
        let isSelected = selectedMode == m.id
        return HStack(spacing: 10) {
            Image(systemName: m.icon)
                .frame(width: Self.iconColumn)
                .foregroundStyle(isSelected ? Color.accentColor : Color.secondary)

            VStack(alignment: .leading, spacing: 1) {
                Text(m.name)
                    .foregroundStyle(isSelected ? Color.accentColor : Color.primary)
                Text(m.desc)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            Spacer(minLength: 8)

            Image(systemName: "checkmark")
                .foregroundStyle(Color.accentColor)
                .fontWeight(.semibold)
                .opacity(isSelected ? 1 : 0)
                .accessibilityHidden(!isSelected)
        }
    }

    private var colorSelectionSection: some View {
        Section("Color Swatches") {
            HStack(spacing: 12) {
                ForEach(LedColorPreset.all, id: \.hex) { preset in
                    Button {
                        selectedColorHex = preset.hex
                        updateBeadsColor(preset.color)
                        if liveApply { sendLedUpdate() }
                    } label: {
                        ZStack {
                            Circle()
                                .fill(preset.color)
                                .frame(width: 28, height: 28)
                                .overlay(Circle().strokeBorder(.separator, lineWidth: 1))
                                .shadow(radius: selectedColorHex == preset.hex ? 3 : 0)

                            if selectedColorHex == preset.hex {
                                Image(systemName: "checkmark")
                                    .font(.caption2)
                                    .fontWeight(.bold)
                                    .foregroundStyle(preset.hex == "white" || preset.hex == "yellow" ? .black : .white)
                            }
                        }
                    }
                    .buttonStyle(.plain)
                    .help(preset.name)
                }
            }
            .padding(.vertical, 6)
        }
    }

    private var actionSection: some View {
        Section {
            Toggle("Live update knob on selection", isOn: $liveApply)

            HStack {
                Button {
                    sendLedUpdate()
                } label: {
                    Label("Send to Hardware Now", systemImage: "bolt.fill")
                }
                .buttonStyle(.borderedProminent)

                Spacer()

                if let msg = store.statusMessage {
                    Text(msg)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
        } footer: {
            Text("""
                Anticater VK01 supports steady backlight, breath cycles, and \
                press-reactive lighting per hardware layer.
                """)
        }
    }

    private func sendLedUpdate() {
        store.setLed(
            layer: selectedLayer,
            mode: selectedMode,
            color: selectedMode == "off" ? nil : selectedColorHex
        )
    }

    private func updateBeadsColor(_ col: Color) {
        beadColors = Array(repeating: col, count: 16)
    }

    private func applySolidToRing() {
        if let preset = LedColorPreset.all.first(where: { $0.hex == selectedColorHex }) {
            updateBeadsColor(preset.color)
        } else {
            updateBeadsColor(.white)
        }
        if liveApply { sendLedUpdate() }
    }

    private func applySpectrumToRing() {
        for i in 0..<16 {
            let hue = Double(i) / 16.0
            beadColors[i] = Color(hue: hue, saturation: 1.0, brightness: 1.0)
        }
    }

    private func queryDeviceState() {
        isReadingMode = true
        store.getHardwareLedMode(layer: selectedLayer) { mode in
            isReadingMode = false
            if let m = mode {
                let name = ledModeNames[Int(m)] ?? "Unknown"
                hardwareReadMode = "Mode \(m): \(name)"
            } else {
                hardwareReadMode = "Could not read mode"
            }
        }
    }
}
