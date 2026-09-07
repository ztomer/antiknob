// SequenceEditor.swift — Sheet modal for editing multi-step macro sequences.
// Supports drag-and-drop reordering, keystroke steps, and millisecond delay steps.

import SwiftUI

struct SequenceEditor: View {
    let title: String
    @Binding var steps: [SeqStep]
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        VStack(spacing: 0) {
            header
            stepList
            Divider()
            footerToolbar
        }
        .frame(width: 460, height: 360)
    }

    private var header: some View {
        VStack(spacing: 4) {
            Text(title)
                .font(.headline)
                .padding(.top, 16)
            Text("Steps execute top to bottom — drag rows to reorder.")
                .font(.caption)
                .foregroundStyle(.secondary)
        }
        .padding(.bottom, 8)
    }

    private var stepList: some View {
        List {
            ForEach(steps.indices, id: \.self) { i in
                stepRow(i)
            }
            .onMove { indices, newOffset in
                steps.move(fromOffsets: indices, toOffset: newOffset)
            }
        }
        .overlay {
            if steps.isEmpty {
                VStack(spacing: 8) {
                    Image(systemName: "list.bullet.rectangle")
                        .font(.largeTitle)
                        .foregroundStyle(.tertiary)
                    Text("No steps configured.")
                        .foregroundStyle(.secondary)
                    Text("Add a keystroke or delay below.")
                        .font(.caption)
                        .foregroundStyle(.tertiary)
                }
            }
        }
    }

    @ViewBuilder
    private func stepRow(_ i: Int) -> some View {
        HStack(spacing: 10) {
            if steps[i].key == nil && steps[i].delayMs != nil {
                Image(systemName: "clock")
                    .foregroundStyle(.secondary)
                Text("Wait")
                TextField("ms", value: delayBinding(i), format: .number)
                    .textFieldStyle(.roundedBorder)
                    .multilineTextAlignment(.trailing)
                    .frame(width: 70)
                Text("ms")
                    .foregroundStyle(.secondary)
            } else {
                Image(systemName: "keyboard")
                    .foregroundStyle(.secondary)
                ChordRecorder(chord: chordBinding(i), requireModifiers: false, clearable: false)
            }

            Spacer()

            Button {
                if steps.indices.contains(i) {
                    steps.remove(at: i)
                }
            } label: {
                Image(systemName: "minus.circle.fill")
            }
            .buttonStyle(.plain)
            .foregroundStyle(.tertiary)
            .help("Delete step")
        }
        .padding(.vertical, 3)
    }

    private var footerToolbar: some View {
        HStack {
            Button {
                steps.append(SeqStep())
            } label: {
                Label("Add Keystroke", systemImage: "keyboard")
            }

            Button {
                steps.append(SeqStep(delayMs: 100))
            } label: {
                Label("Add Wait", systemImage: "clock")
            }

            Spacer()

            Button("Done") {
                dismiss()
            }
            .keyboardShortcut(.defaultAction)
        }
        .padding(12)
    }

    private func chordBinding(_ i: Int) -> Binding<KeyChordSpec?> {
        Binding(
            get: {
                guard steps.indices.contains(i), let k = steps[i].key else { return nil }
                return KeyChordSpec(key: k, mods: steps[i].mods ?? [], label: steps[i].label)
            },
            set: { newChord in
                guard steps.indices.contains(i), let n = newChord else { return }
                steps[i].key = n.key
                steps[i].mods = n.mods
                steps[i].label = n.label
            }
        )
    }

    private func delayBinding(_ i: Int) -> Binding<Int> {
        Binding(
            get: { steps.indices.contains(i) ? (steps[i].delayMs ?? 0) : 0 },
            set: { val in
                if steps.indices.contains(i) {
                    steps[i].delayMs = max(0, val)
                }
            }
        )
    }
}
