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

    var body: some View {
        Form {
            layerPickerSection
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
            Picker("Apply to Layer", selection: $selectedLayer) {
                ForEach(0..<3) { i in
                    Text("Device Layer \(i + 1)").tag(i)
                }
            }
            .pickerStyle(.segmented)
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
}
