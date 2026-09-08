// LayerDetail.swift — Detail pane for configuring gestures in a single layer.
// Uses macOS grouped form styling, categorized action menus, and native controls.

import AppKit
import SwiftUI

enum ActionPreset: Hashable {
    case none
    case scrollUp
    case scrollDown
    case aux(AuxKey)
    case keystroke
    case openApp
    case sequence
    case openURL
    case openPath
    case quitApp
    case hotkeySwitch
}

struct LayerDetail: View {
    @ObservedObject var store: ConfigStore
    let idx: Int
    /// Which layer the pane is showing. Owned by `LayersPane`; move and
    /// delete write it so the pane lands on a layer that still exists.
    @Binding var selectedLayer: Int

    @State private var selected: Gesture?

    // Module-internal rather than private: the binding plumbing in
    // LayerBindings.swift is an extension on this type, and an extension in
    // another file cannot see `private` members.
    @State var pendingKeystroke: Set<Gesture> = []
    @State var editingSequence: Gesture?

    var body: some View {
        if store.cfg.layers.indices.contains(idx) {
            Form {
                // Global, so it stands apart from the layer below it.
                LayerSwitchingSection(store: store)

                // Everything else is ONE section, because everything else is
                // one layer: which layer, its name and place, the knob it
                // lights, the gestures it binds, and whether the knob is
                // sending them here yet. Splitting those across four panes
                // made a single subject look like four subjects.
                Section {
                    LayerTabBar(store: store, selected: $selectedLayer, idx: idx)
                    LayerEditRow(store: store, selectedLayer: $selectedLayer, idx: idx)

                    KnobHeader(selected: $selected, mode: lightingMode)
                        .frame(maxWidth: .infinity)

                    PropertyGrid(horizontalSpacing: 12, verticalSpacing: 2) {
                        // First, directly under the knob it lights, and on
                        // the same column edges as the gestures below it.
                        LayerLightingRow(store: store, idx: idx)
                        ForEach(Gesture.allCases) { g in
                            gestureRow(g)
                        }
                    }

                    PropertyGrid {
                        LayerFlashRow(store: store)
                    }
                } header: {
                    Text("Layers")
                }
            }
            .formStyle(.grouped)
            .sheet(item: $editingSequence) { g in
                SequenceEditor(
                    title: "\(gestureTitles[g] ?? g.rawValue) Sequence",
                    steps: stepsBinding(g)
                )
            }
        }
    }

    /// The mode this layer sets, if it sets one. Drawn as the knob's ring.
    private var lightingMode: LedMode? {
        store[layer: idx]?.led.flatMap(LedMode.named)
    }

    /// Three columns: gesture, its parameter control, its action menu.
    ///
    /// These were a `LabeledContent` with both controls crammed into one
    /// trailing `HStack`, so a stepper on one row and a chord pill on the
    /// next started at different x and the action menus stepped in and out
    /// with the parameter's width. Every column is now natural width and
    /// packed left, so the three read down three straight edges.
    private func gestureRow(_ g: Gesture) -> some View {
        GridRow {
            Text(gestureTitles[g] ?? g.rawValue)
                .fixedSize(horizontal: true, vertical: false)
                .gridColumnAlignment(.leading)
            // The action and whatever it needs, in ONE cell. The parameter
            // used to come BEFORE the menu that decides whether there is
            // one -- a row read "3 lines" and then, to its right, what those
            // lines were for. Putting it in a column of its own fixed the
            // order and introduced a new problem: the last column takes the
            // grid's slack, which left the stepper at the pane's edge.
            HStack(spacing: 10) {
                actionMenu(g)
                params(g)
                Spacer(minLength: 0)
            }
            .gridColumnAlignment(.leading)
        }
        .padding(.vertical, 3)
        .contentShape(Rectangle())
        .onTapGesture { selected = g }
    }

    // MARK: - Categorized Action Menu

    private func actionMenu(_ g: Gesture) -> some View {
        Dropdown(title: currentTitle(g)) {
            actionMenuItems(g)
        }
    }

    /// The menu's contents, split from `actionMenu` only because the two
    /// together crossed the function-body cap.
    @ViewBuilder
    private func actionMenuItems(_ g: Gesture) -> some View {
        Group {
            builtInActions(g)
            appAndCustomActions(g)
        }
    }

    /// Actions the knob can perform on its own: nothing, scrolling, media
    /// transport, display brightness.
    @ViewBuilder
    private func builtInActions(_ g: Gesture) -> some View {
        Picker("", selection: presetBinding(g)) {
            Text("None").tag(ActionPreset.none)
        }
        .pickerStyle(.inline).labelsHidden()

        Menu("Scroll") {
            Picker("", selection: presetBinding(g)) {
                Text("Scroll Up").tag(ActionPreset.scrollUp)
                Text("Scroll Down").tag(ActionPreset.scrollDown)
            }
            .pickerStyle(.inline).labelsHidden()
        }

        Menu("Media") {
            Picker("", selection: presetBinding(g)) {
                Text("Volume Up").tag(ActionPreset.aux(.volumeUp))
                Text("Volume Down").tag(ActionPreset.aux(.volumeDown))
                Text("Mute").tag(ActionPreset.aux(.mute))
                Text("Play / Pause").tag(ActionPreset.aux(.playPause))
                Text("Next Track").tag(ActionPreset.aux(.next))
                Text("Previous Track").tag(ActionPreset.aux(.previous))
            }
            .pickerStyle(.inline).labelsHidden()
        }

        Menu("Display") {
            Picker("", selection: presetBinding(g)) {
                Text("Brightness Up").tag(ActionPreset.aux(.brightnessUp))
                Text("Brightness Down").tag(ActionPreset.aux(.brightnessDown))
            }
            .pickerStyle(.inline).labelsHidden()
        }
    }

    /// Actions that reach outside the knob: the browser, other apps, and the
    /// custom keystroke / hotkey / sequence editors.
    @ViewBuilder
    private func appAndCustomActions(_ g: Gesture) -> some View {
        Menu("Web") {
            Button("Back  ⌘[") { setChord(g, key: 33, label: "[") }
            Button("Forward  ⌘]") { setChord(g, key: 30, label: "]") }
            Button("Refresh  ⌘R") { setChord(g, key: 15, label: "R") }
            Divider()
            Button("Open Website…") { setAction(g, .openURL(url: "https://")) }
        }

        Menu("Open") {
            Button("Calculator") { setLaunch(g, "com.apple.calculator") }
            Button("Mail") { setLaunch(g, "com.apple.mail") }
            Button("Finder") { setLaunch(g, "com.apple.finder") }
            Divider()
            Button("Other App…") { chooseApp(g) }
            Button("File or Folder…") { choosePath(g) }
            Divider()
            Button("Quit App…") { chooseApp(g) { .quitApp(bundleId: $0, force: nil) } }
        }

        Menu("Custom") {
            Picker("", selection: presetBinding(g)) {
                Text("Keystroke…").tag(ActionPreset.keystroke)
                Text("Hotkey Switch…").tag(ActionPreset.hotkeySwitch)
                Text("Sequence…").tag(ActionPreset.sequence)
            }
            .pickerStyle(.inline).labelsHidden()
        }
    }

    // MARK: - Parameter Controls

    @ViewBuilder
    /// Exactly one grid cell, whatever the action type needs inside it.
    ///
    /// `paramControls` returns one, two or three views depending on the
    /// action. As direct children of a `GridRow` those become separate cells,
    /// which gave rows different column counts and split the scroll Stepper's
    /// label away from its control. The HStack collapses them to one cell.
    private func params(_ g: Gesture) -> some View {
        HStack(spacing: 8) {
            paramControls(g)
        }
        .fixedSize()
    }

    @ViewBuilder
    private func paramControls(_ g: Gesture) -> some View {
        switch self[g] {
        case .scroll?:
            // The label is drawn here rather than passed to `Stepper`, whose
            // built-in label splits to the leading edge of whatever width it
            // is given -- inside a grid cell that stretched the whole column
            // and left "3 lines" a column away from its own arrows.
            Text("\(Int(abs(currentLines(g)))) lines")
            Stepper("", value: linesBinding(g), in: 1...30)
                .labelsHidden()
                .fixedSize()
        case .keyChord?:
            ChordRecorder(chord: chordBinding(g))
        case .launchApp(let bundleId)?:
            Text(appName(bundleId)).foregroundStyle(.secondary)
        case .openURL?:
            TextField("https://…", text: urlBinding(g))
                .textFieldStyle(.roundedBorder)
                .frame(width: 180)
        case .openPath(let path)?:
            Text(pathName(path)).foregroundStyle(.secondary).help(path)
        case .quitApp(let bundleId, _)?:
            Text(appName(bundleId)).foregroundStyle(.secondary)
            Toggle("Force", isOn: forceBinding(g))
                .toggleStyle(.checkbox)
        case .hotkeySwitch?:
            ChordRecorder(chord: switchChordBinding(g, second: false))
            Text("⇄").foregroundStyle(.secondary)
            ChordRecorder(chord: switchChordBinding(g, second: true))
        case .sequence(let steps)?:
            Button("\(steps.count) step\(steps.count == 1 ? "" : "s")…") {
                editingSequence = g
            }
            .buttonStyle(.link)
        default:
            if pendingKeystroke.contains(g) {
                ChordRecorder(chord: chordBinding(g))
            }
        }
    }
}
