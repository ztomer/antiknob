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
    ///
    /// These are the 514c:8850's own modes, watched one by one on real
    /// hardware. The previous list carried the 1189:884x names -- it offered
    /// "Shock (Breathe)" and "Press (Reactive)" for modes that are reactive
    /// and rainbow here, so picking one gave an effect the label did not
    /// describe. Mode 5 is deliberately absent: it crashes this firmware.
    static let all: [LedMode] = [
            LedMode(id: "static", name: "Static",
                    desc: "Steady illumination", icon: "lightbulb.fill"),
            LedMode(id: "reactive", name: "Reactive",
                    desc: "Lights up in response to input", icon: "hand.tap.fill"),
            LedMode(id: "ripple", name: "Ripple",
                    desc: "Ripple effect on input", icon: "waveform.path.ecg"),
            LedMode(id: "rainbow", name: "Rainbow",
                    desc: "Cycling multicolour — the effect the knob ships in",
                    icon: "rainbow"),
            LedMode(id: "off", name: "Off",
                    desc: "Disable LEDs to conserve power", icon: "power")
        ]

    /// Whether this build can promise the colour swatches do anything.
    ///
    /// On the 3-button knob they do not: mode 1 was set with blue, red and
    /// green in turn and stayed red every time. The 16-key device sharing
    /// this product id does honour them, so the controls stay -- but a UI
    /// that silently ignores a colour someone picked is the same defect as a
    /// layer view showing bindings that cannot fire.
    static let colourNotice =
        "This knob has a single fixed colour — only the effect can change. "
        + "Colour choices are sent and ignored by its firmware."
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
    @State private var selectedMode: String = "rainbow"
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
            PropertyGrid(horizontalSpacing: 14, verticalSpacing: 9) {
                ForEach(LedMode.all, id: \.id) { m in
                    modeRow(m)
                }
            }
        }
    }

    /// Four columns: glyph, name, description, selection mark.
    ///
    /// The description gets a column of its own between the name and the
    /// mark, so five explanations of very different lengths read down one
    /// edge. The name column expands, which keeps the mark pinned to the
    /// trailing edge where it was, and the mark keeps its space when the row
    /// is unselected so selecting one does not shift the text.
    private func modeRow(_ m: LedMode) -> some View {
        let isSelected = selectedMode == m.id
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

    private var colorSelectionSection: some View {
        Section("Color Swatches") {
            // Says plainly that these do nothing here rather than letting
            // someone pick a colour and wonder why the knob stays red.
            HStack(alignment: .top, spacing: 8) {
                Image(systemName: "info.circle")
                    .foregroundStyle(.secondary)
                Text(LedMode.colourNotice)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
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
