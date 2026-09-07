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
    @Binding var sel: TabSelection

    @State private var selected: Gesture?
    @State private var pendingKeystroke: Set<Gesture> = []
    @State private var editingSequence: Gesture?
    @State private var confirmingDelete = false

    var body: some View {
        if idx < store.cfg.layers.count {
            Form {
                Section {
                    KnobHeader(selected: $selected)
                        .frame(maxWidth: .infinity)
                        .listRowBackground(Color.clear)
                }

                layerSection

                Section("Knob Gestures") {
                    ForEach(Gesture.allCases) { g in
                        gestureRow(g)
                    }
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

    // MARK: - Layer Name, Order, and Removal

    private var layerSection: some View {
        Section("Layer") {
            TextField("Name", text: $store.cfg.layers[idx].name)

            LabeledContent("Order") {
                HStack(spacing: 6) {
                    Button { move(by: -1) } label: {
                        Image(systemName: "chevron.left")
                    }
                    .disabled(idx == 0)
                    .help("Move this layer left")

                    Button { move(by: 1) } label: {
                        Image(systemName: "chevron.right")
                    }
                    .disabled(idx >= store.cfg.layers.count - 1)
                    .help("Move this layer right")

                    Text("\(idx + 1) of \(store.cfg.layers.count)")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .padding(.leading, 4)
                }
            }

            Button("Delete Layer…", role: .destructive) { confirmingDelete = true }
                .disabled(store.cfg.layers.count <= 1)
                .help(store.cfg.layers.count <= 1
                      ? "The last layer cannot be deleted"
                      : "Delete this layer and its gesture bindings")
                .confirmationDialog(
                    "Delete \(layerLabel)?",
                    isPresented: $confirmingDelete
                ) {
                    Button("Delete Layer", role: .destructive) { delete() }
                    Button("Cancel", role: .cancel) {}
                } message: {
                    Text("Its gesture bindings are removed. This cannot be undone.")
                }
        }
    }

    private var layerLabel: String {
        let name = store.cfg.layers[idx].name
        return name.isEmpty ? "Layer \(idx + 1)" : name
    }

    /// Swaps this layer with its neighbour and keeps the tab selection on it.
    private func move(by delta: Int) {
        let dest = idx + delta
        guard store.cfg.layers.indices.contains(dest) else { return }
        withAnimation {
            store.cfg.layers.swapAt(idx, dest)
            sel = .layer(dest)
        }
    }

    /// Removes this layer and lands the selection on a still-valid tab.
    private func delete() {
        guard store.cfg.layers.count > 1,
              store.cfg.layers.indices.contains(idx) else { return }
        withAnimation {
            store.cfg.layers.remove(at: idx)
            sel = .layer(min(idx, store.cfg.layers.count - 1))
        }
    }

    private func gestureRow(_ g: Gesture) -> some View {
        LabeledContent(gestureTitles[g] ?? g.rawValue) {
            HStack(spacing: 10) {
                params(g)
                actionMenu(g)
            }
        }
        .contentShape(Rectangle())
        .onTapGesture { selected = g }
        .listRowBackground(selected == g ? Color.accentColor.opacity(0.12) : nil)
    }

    // MARK: - Categorized Action Menu

    private func actionMenu(_ g: Gesture) -> some View {
        Menu {
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
        } label: {
            Text(currentTitle(g))
        }
        .fixedSize()
    }

    // MARK: - Parameter Controls

    @ViewBuilder
    private func params(_ g: Gesture) -> some View {
        switch store.cfg.layers[idx][g] {
        case .scroll?:
            Stepper("\(Int(abs(currentLines(g)))) lines", value: linesBinding(g), in: 1...30)
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

    // MARK: - Helper Bindings & Actions

    private func currentTitle(_ g: Gesture) -> String {
        switch store.cfg.layers[idx][g] {
        case nil, .some(.none):
            return pendingKeystroke.contains(g) ? "Keystroke" : "None"
        case .some(.scroll(let lines)):
            return (lines ?? store.cfg.defaultScrollLines) >= 0 ? "Scroll Up" : "Scroll Down"
        case .some(.keyChord): return "Keystroke"
        case .some(.sequence): return "Sequence"
        case .some(.aux(let k)): return auxTitles[k] ?? "Media"
        case .some(.launchApp): return "Open App"
        case .some(.openURL): return "Website"
        case .some(.openPath): return "Open File"
        case .some(.quitApp): return "Quit App"
        case .some(.hotkeySwitch): return "Hotkey Switch"
        case .some(.mouseClick(let b)): return "Click \(b.rawValue)"
        }
    }

    private func presetBinding(_ g: Gesture) -> Binding<ActionPreset> {
        Binding(
            get: {
                switch store.cfg.layers[idx][g] {
                case nil, .some(.none):
                    return pendingKeystroke.contains(g) ? .keystroke : .none
                case .some(.scroll(let lines)):
                    return (lines ?? store.cfg.defaultScrollLines) >= 0 ? .scrollUp : .scrollDown
                case .some(.keyChord): return .keystroke
                case .some(.sequence): return .sequence
                case .some(.aux(let k)): return .aux(k.unified)
                case .some(.launchApp): return .openApp
                case .some(.openURL): return .openURL
                case .some(.openPath): return .openPath
                case .some(.quitApp): return .quitApp
                case .some(.hotkeySwitch): return .hotkeySwitch
                case .some(.mouseClick): return .none
                }
            },
            set: { p in
                pendingKeystroke.remove(g)
                switch p {
                case .none:
                    store.cfg.layers[idx][g] = nil
                case .scrollUp:
                    store.cfg.layers[idx][g] = .scroll(lines: abs(currentLines(g)))
                case .scrollDown:
                    store.cfg.layers[idx][g] = .scroll(lines: -abs(currentLines(g)))
                case .aux(let k):
                    store.cfg.layers[idx][g] = .aux(key: k)
                case .keystroke:
                    if case .keyChord? = store.cfg.layers[idx][g] { break }
                    store.cfg.layers[idx][g] = nil
                    pendingKeystroke.insert(g)
                case .openApp:
                    chooseApp(g)
                case .openURL:
                    if case .openURL? = store.cfg.layers[idx][g] { break }
                    store.cfg.layers[idx][g] = .openURL(url: "https://")
                case .openPath:
                    choosePath(g)
                case .quitApp:
                    chooseApp(g) { .quitApp(bundleId: $0, force: nil) }
                case .hotkeySwitch:
                    if case .hotkeySwitch? = store.cfg.layers[idx][g] { break }
                    store.cfg.layers[idx][g] = .hotkeySwitch(first: nil, second: nil)
                case .sequence:
                    if case .sequence? = store.cfg.layers[idx][g] {} else {
                        store.cfg.layers[idx][g] = .sequence(steps: [])
                    }
                    editingSequence = g
                }
            }
        )
    }

    private func setChord(_ g: Gesture, key: UInt16, label: String) {
        pendingKeystroke.remove(g)
        store.cfg.layers[idx][g] = .keyChord(key: key, mods: ["cmd"], label: label)
    }

    private func setLaunch(_ g: Gesture, _ bundleId: String) {
        pendingKeystroke.remove(g)
        store.cfg.layers[idx][g] = .launchApp(bundleId: bundleId)
    }

    private func setAction(_ g: Gesture, _ a: Action) {
        pendingKeystroke.remove(g)
        store.cfg.layers[idx][g] = a
    }

    private func urlBinding(_ g: Gesture) -> Binding<String> {
        Binding(
            get: {
                if case .openURL(let u)? = store.cfg.layers[idx][g] { return u }
                return ""
            },
            set: { store.cfg.layers[idx][g] = .openURL(url: $0) }
        )
    }

    private func currentLines(_ g: Gesture) -> Int32 {
        if case .scroll(let l)? = store.cfg.layers[idx][g] {
            return l ?? store.cfg.defaultScrollLines
        }
        return store.cfg.defaultScrollLines
    }

    private func linesBinding(_ g: Gesture) -> Binding<Int> {
        Binding(
            get: { Int(abs(currentLines(g))) },
            set: { v in
                let sign: Int32 = currentLines(g) >= 0 ? 1 : -1
                store.cfg.layers[idx][g] = .scroll(lines: sign * Int32(v))
            }
        )
    }

    private func chordBinding(_ g: Gesture) -> Binding<KeyChordSpec?> {
        Binding(
            get: {
                if case .keyChord(let k, let m, let l)? = store.cfg.layers[idx][g] {
                    return KeyChordSpec(key: k, mods: m, label: l)
                }
                return nil
            },
            set: { new in
                guard let n = new else { return }
                store.cfg.layers[idx][g] = .keyChord(key: n.key, mods: n.mods, label: n.label)
                pendingKeystroke.remove(g)
            }
        )
    }

    private func stepsBinding(_ g: Gesture) -> Binding<[SeqStep]> {
        Binding(
            get: {
                guard case .sequence(let s)? = store.cfg.layers[idx][g] else { return [] }
                var rows: [SeqStep] = []
                for st in s {
                    if st.key != nil || st.delayMs == nil {
                        rows.append(SeqStep(key: st.key, mods: st.mods, label: st.label))
                    }
                    if let ms = st.delayMs {
                        rows.append(SeqStep(delayMs: ms))
                    }
                }
                return rows
            },
            set: { store.cfg.layers[idx][g] = .sequence(steps: $0) }
        )
    }

    private func forceBinding(_ g: Gesture) -> Binding<Bool> {
        Binding(
            get: {
                if case .quitApp(_, let f)? = store.cfg.layers[idx][g] { return f ?? false }
                return false
            },
            set: { v in
                if case .quitApp(let b, _)? = store.cfg.layers[idx][g] {
                    store.cfg.layers[idx][g] = .quitApp(bundleId: b, force: v)
                }
            }
        )
    }

    private func switchChordBinding(_ g: Gesture, second: Bool) -> Binding<KeyChordSpec?> {
        Binding(
            get: {
                if case .hotkeySwitch(let f, let s)? = store.cfg.layers[idx][g] {
                    return second ? s : f
                }
                return nil
            },
            set: { new in
                guard let n = new,
                      case .hotkeySwitch(let f, let s)? = store.cfg.layers[idx][g] else { return }
                store.cfg.layers[idx][g] = .hotkeySwitch(
                    first: second ? f : n,
                    second: second ? n : s
                )
            }
        )
    }

    private func appName(_ bundleId: String) -> String {
        if let url = NSWorkspace.shared.urlForApplication(withBundleIdentifier: bundleId) {
            return FileManager.default.displayName(atPath: url.path)
        }
        return bundleId
    }

    private func pathName(_ path: String) -> String {
        FileManager.default.displayName(atPath: (path as NSString).expandingTildeInPath)
    }

    private func chooseApp(_ g: Gesture, make: (String) -> Action = { .launchApp(bundleId: $0) }) {
        let panel = NSOpenPanel()
        panel.allowedContentTypes = [.application]
        panel.directoryURL = URL(fileURLWithPath: "/Applications")
        panel.canChooseDirectories = false
        panel.allowsMultipleSelection = false
        if panel.runModal() == .OK, let url = panel.url,
           let bundleId = Bundle(url: url)?.bundleIdentifier {
            pendingKeystroke.remove(g)
            store.cfg.layers[idx][g] = make(bundleId)
        }
    }

    private func choosePath(_ g: Gesture) {
        let panel = NSOpenPanel()
        panel.canChooseFiles = true
        panel.canChooseDirectories = true
        panel.allowsMultipleSelection = false
        if panel.runModal() == .OK, let url = panel.url {
            pendingKeystroke.remove(g)
            store.cfg.layers[idx][g] = .openPath(path: url.path)
        }
    }
}
