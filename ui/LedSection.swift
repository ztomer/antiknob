// LedSection.swift — Dynamic hardware LED lighting controls.
// Sends live set_led commands to antiknob-daemon over the Unix domain socket.

import SwiftUI

struct LedSection: View {
    @ObservedObject var store: ConfigStore

    @State private var selectedLayer: Int = 0
    @State private var selectedMode: String = "backlight"
    @State private var selectedColorHex: String = "white"
    @State private var customColor: Color = .white
    @State private var liveApply: Bool = true

    let modes: [(id: String, name: String, desc: String, icon: String)] = [
        ("backlight", "Backlight", "Steady solid illumination", "lightbulb.fill"),
        ("shock", "Shock (Breathe)", "Gentle pulsing breath", "waveform.path"),
        ("shock2", "Shock 2 (Rapid)", "Faster pulse cycle", "waveform.path.ecg"),
        ("press", "Press (Reactive)", "Lights up on knob press", "hand.tap.fill"),
        ("off", "Off", "Disable LEDs to conserve power", "power"),
    ]

    let colorPresets: [(name: String, hex: String, color: Color)] = [
        ("White", "white", .white),
        ("Red", "red", .red),
        ("Orange", "orange", .orange),
        ("Yellow", "yellow", .yellow),
        ("Green", "green", .green),
        ("Cyan", "cyan", Color(red: 0, green: 0.9, blue: 0.9)),
        ("Blue", "blue", .blue),
        ("Purple", "purple", .purple),
    ]

    @State private var beadColors: [Color] = Array(repeating: .white, count: 16)
    @State private var hardwareReadMode: String? = nil
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
                LabeledContent("Firmware Reported Mode") {
                    Text(readMode)
                        .font(.caption)
                        .foregroundStyle(Color.accentColor)
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

    private var modeSelectionSection: some View {
        Section("Lighting Mode") {
            ForEach(modes, id: \.id) { m in
                HStack {
                    Label(m.name, systemImage: m.icon)
                        .foregroundStyle(selectedMode == m.id ? Color.accentColor : Color.primary)
                    Spacer()
                    Text(m.desc)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    if selectedMode == m.id {
                        Image(systemName: "checkmark")
                            .foregroundStyle(Color.accentColor)
                            .fontWeight(.semibold)
                    }
                }
                .contentShape(Rectangle())
                .onTapGesture {
                    selectedMode = m.id
                    if liveApply { sendLedUpdate() }
                }
                .padding(.vertical, 2)
            }
        }
    }

    private var colorSelectionSection: some View {
        Section("Color Swatches") {
            HStack(spacing: 12) {
                ForEach(colorPresets, id: \.hex) { preset in
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
            Text("Anticater VK01 supports steady backlight, breath cycles, and press-reactive lighting per hardware layer.")
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
        if let preset = colorPresets.first(where: { $0.hex == selectedColorHex }) {
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
                let name = (m == 0 ? "Off" : (m == 1 ? "Backlight" : (m == 2 ? "Shock (Breathe)" : (m == 3 ? "Shock2" : "Press"))))
                hardwareReadMode = "Mode \(m): \(name)"
            } else {
                hardwareReadMode = "Could not read mode"
            }
        }
    }
}
