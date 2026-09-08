// LayerLighting.swift — one layer's backlight, as glyphs and a preview.
//
// This was a tab of its own, and it addressed the wrong thing. Its picker
// chose between the firmware's three DEVICE layers -- a fixed hardware axis
// that has nothing to do with the host layers in the tabs beside it. Only
// ONE device layer is host-translated, so while the daemon is driving, the
// knob sits on that one and wears its one colour no matter which host layer
// is active. Choosing a colour "per layer" there changed a value the layers
// did not read.
//
// It is per host layer now, for real: the layer carries a mode and the
// daemon writes it on every switch (`host::led_sync`). So the control lives
// in the layer it describes.
//
// The six-row list with a description each is a row of glyphs. The preview
// under it shows what the mode does, which is what the descriptions were
// for -- "Cycling colours" beside an animation of cycling colours is a
// caption for a picture of itself.

import SwiftUI

struct LayerLighting: View {
    @ObservedObject var store: ConfigStore
    let idx: Int

    /// What the firmware holds on the bound device layer right now, so the
    /// section can say whether this layer's choice is the one on the knob.
    /// `nil` until read; failure and "not read yet" are different states and
    /// neither is a mode.
    @State private var firmwareMode: Int?
    @State private var isReading = false

    private var selection: LedMode? {
        store.cfg.layers[idx].led.flatMap(LedMode.named)
    }

    /// True when this layer's mode is what the knob is currently wearing.
    private var isLive: Bool {
        guard let selection, let firmwareMode else { return false }
        return selection.number == firmwareMode && store.activeLayerIdx == idx
    }

    var body: some View {
        Section {
            glyphRow
            if let selection {
                VStack(spacing: 8) {
                    LedPreview(mode: selection, isLive: isLive)
                    LedPreviewCaption(mode: selection, isLive: isLive)
                }
                .frame(maxWidth: .infinity)
                .padding(.vertical, 4)
            }
        } header: {
            Text("Lighting")
        } footer: {
            footer
        }
        .onAppear(perform: readFirmware)
    }

    // MARK: - Glyphs

    /// "Leave it alone" is a choice, and the first one. A layer with no mode
    /// is the state every config written before this field loads in, and
    /// there has to be a way back to it.
    private var glyphRow: some View {
        HStack(spacing: 6) {
            glyph(nil)
            Divider().frame(height: 22)
            ForEach(LedMode.all) { m in
                glyph(m)
            }
            Text(selection?.name ?? "No change")
                .font(.caption)
                .foregroundStyle(.secondary)
                .padding(.leading, 4)
            Spacer(minLength: 0)
        }
        .padding(.vertical, 2)
    }

    @ViewBuilder
    private func glyph(_ mode: LedMode?) -> some View {
        let isSelected = selection?.id == mode?.id
        Button {
            store.cfg.layers[idx].led = mode?.id
        } label: {
            Image(systemName: mode?.icon ?? "minus")
                .frame(width: 28, height: 24)
                // Tinted by the mode's own colour when it has one, because
                // red and green share a glyph and the colour IS the
                // difference. Selected rows go white on the accent fill,
                // where a tint would be unreadable.
                .foregroundStyle(
                    isSelected
                        ? Color.white
                        : (mode?.swatch?.color ?? Color.secondary)
                )
                .background(
                    RoundedRectangle(cornerRadius: 5, style: .continuous)
                        .fill(isSelected ? Color.accentColor : Color.secondary.opacity(0.12))
                )
                .contentShape(RoundedRectangle(cornerRadius: 5, style: .continuous))
        }
        .buttonStyle(.plain)
        // The description the rows used to carry, on hover. It is worth
        // having and not worth six lines of the pane.
        .help(mode.map { "\($0.name) — \($0.desc)" } ?? "Leave the backlight as it is")
        .accessibilityLabel(mode?.name ?? "No change")
        .accessibilityAddTraits(isSelected ? [.isSelected] : [])
    }

    // MARK: - Honesty about whether it can happen

    @ViewBuilder
    private var footer: some View {
        if selection == nil {
            Text("This layer leaves the knob's backlight as it is.")
        } else if !store.modePresentation.hostLayersCanFire {
            // The daemon writes this on a layer switch, and it only switches
            // layers when it can hear the knob. Saying so here beats the
            // colour silently never appearing.
            Text("Set when this layer becomes active — once the knob's slot bindings "
               + "are flashed. It is not bound yet, so this colour will not appear.")
        } else {
            Text("The daemon puts this mode on the knob whenever this layer is active.")
        }
    }

    private func readFirmware() {
        guard store.hardwareConnected, let bound = store.boundDeviceLayer else {
            firmwareMode = nil
            return
        }
        isReading = true
        store.getHardwareLedMode(layer: bound) { mode in
            isReading = false
            firmwareMode = mode
        }
    }
}
