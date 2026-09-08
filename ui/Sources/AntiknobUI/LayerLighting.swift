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
// The six-row list with a description each is a row of glyphs, and the
// preview it used to carry moved onto the knob at the top of the pane --
// the same knob these gestures are bound on, which is the object the light
// is actually on. Two drawings of one knob, one of them lit and one of them
// clickable, was a picture of the same thing twice.

import SwiftUI

struct LayerLightingRow: View {
    @ObservedObject var store: ConfigStore
    let idx: Int

    private var selection: LedMode? {
        store[layer: idx]?.led.flatMap(LedMode.named)
    }

    var body: some View {
        glyphRow
            .onAppear { store.refreshFirmwareLedMode() }
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
            store[layer: idx]?.led = mode?.id
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
}
