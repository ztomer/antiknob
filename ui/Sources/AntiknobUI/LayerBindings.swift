// LayerBindings.swift — the binding and action plumbing behind LayerDetail.
//
// Split out of LayerDetail.swift, which crossed the 500-line file cap once
// the gesture rows became a grid. The division is view code there, state
// plumbing here: nothing in this file draws anything.

import AppKit
import SwiftUI

// Members are module-internal rather than private for the same reason the
// state above is: `private` does not cross a file boundary, even within one
// type. Nothing outside AntiknobUI calls any of this.
extension LayerDetail {
    /// This layer's action for one gesture, tolerant of a stale index.
    ///
    /// Every binding helper below reached `self[g]`
    /// directly -- thirty-three subscripts, each of which traps if the
    /// layer is deleted while its view is still on screen. Reading and
    /// writing both go through here, so the tolerance is in one place
    /// rather than thirty-three guards someone has to remember.
    subscript(g: Gesture) -> Action? {
        get { store[layer: idx]?[g] }
        nonmutating set {
            guard store.cfg.layers.indices.contains(idx) else { return }
            // The one place that reaches the array directly. `self[g] = ...`
            // here is this setter calling itself.
            store.cfg.layers[idx][g] = newValue
        }
    }
}

extension LayerDetail {

    func currentTitle(_ g: Gesture) -> String {
        switch self[g] {
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

    func presetBinding(_ g: Gesture) -> Binding<ActionPreset> {
        Binding(
            get: {
                switch self[g] {
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
                    self[g] = nil
                case .scrollUp:
                    self[g] = .scroll(lines: abs(currentLines(g)))
                case .scrollDown:
                    self[g] = .scroll(lines: -abs(currentLines(g)))
                case .aux(let k):
                    self[g] = .aux(key: k)
                case .keystroke:
                    if case .keyChord? = self[g] { break }
                    self[g] = nil
                    pendingKeystroke.insert(g)
                case .openApp:
                    chooseApp(g)
                case .openURL:
                    if case .openURL? = self[g] { break }
                    self[g] = .openURL(url: "https://")
                case .openPath:
                    choosePath(g)
                case .quitApp:
                    chooseApp(g) { .quitApp(bundleId: $0, force: nil) }
                case .hotkeySwitch:
                    if case .hotkeySwitch? = self[g] { break }
                    self[g] = .hotkeySwitch(first: nil, second: nil)
                case .sequence:
                    if case .sequence? = self[g] {} else {
                        self[g] = .sequence(steps: [])
                    }
                    editingSequence = g
                }
            }
        )
    }

    func setChord(_ g: Gesture, key: UInt16, label: String) {
        pendingKeystroke.remove(g)
        self[g] = .keyChord(key: key, mods: ["cmd"], label: label)
    }

    func setLaunch(_ g: Gesture, _ bundleId: String) {
        pendingKeystroke.remove(g)
        self[g] = .launchApp(bundleId: bundleId)
    }

    func setAction(_ g: Gesture, _ a: Action) {
        pendingKeystroke.remove(g)
        self[g] = a
    }

    func urlBinding(_ g: Gesture) -> Binding<String> {
        Binding(
            get: {
                if case .openURL(let u)? = self[g] { return u }
                return ""
            },
            set: { self[g] = .openURL(url: $0) }
        )
    }

    func currentLines(_ g: Gesture) -> Int32 {
        if case .scroll(let l)? = self[g] {
            return l ?? store.cfg.defaultScrollLines
        }
        return store.cfg.defaultScrollLines
    }

    func linesBinding(_ g: Gesture) -> Binding<Int> {
        Binding(
            get: { Int(abs(currentLines(g))) },
            set: { v in
                let sign: Int32 = currentLines(g) >= 0 ? 1 : -1
                self[g] = .scroll(lines: sign * Int32(v))
            }
        )
    }

    func chordBinding(_ g: Gesture) -> Binding<KeyChordSpec?> {
        Binding(
            get: {
                if case .keyChord(let k, let m, let l)? = self[g] {
                    return KeyChordSpec(key: k, mods: m, label: l)
                }
                return nil
            },
            set: { new in
                guard let n = new else { return }
                self[g] = .keyChord(key: n.key, mods: n.mods, label: n.label)
                pendingKeystroke.remove(g)
            }
        )
    }

    func stepsBinding(_ g: Gesture) -> Binding<[SeqStep]> {
        Binding(
            get: {
                guard case .sequence(let s)? = self[g] else { return [] }
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
            set: { self[g] = .sequence(steps: $0) }
        )
    }

    func forceBinding(_ g: Gesture) -> Binding<Bool> {
        Binding(
            get: {
                if case .quitApp(_, let f)? = self[g] { return f ?? false }
                return false
            },
            set: { v in
                if case .quitApp(let b, _)? = self[g] {
                    self[g] = .quitApp(bundleId: b, force: v)
                }
            }
        )
    }

    func switchChordBinding(_ g: Gesture, second: Bool) -> Binding<KeyChordSpec?> {
        Binding(
            get: {
                if case .hotkeySwitch(let f, let s)? = self[g] {
                    return second ? s : f
                }
                return nil
            },
            set: { new in
                guard let n = new,
                      case .hotkeySwitch(let f, let s)? = self[g] else { return }
                self[g] = .hotkeySwitch(
                    first: second ? f : n,
                    second: second ? n : s
                )
            }
        )
    }

    func appName(_ bundleId: String) -> String {
        if let url = NSWorkspace.shared.urlForApplication(withBundleIdentifier: bundleId) {
            return FileManager.default.displayName(atPath: url.path)
        }
        return bundleId
    }

    func pathName(_ path: String) -> String {
        FileManager.default.displayName(atPath: (path as NSString).expandingTildeInPath)
    }

    func chooseApp(_ g: Gesture, make: (String) -> Action = { .launchApp(bundleId: $0) }) {
        let panel = NSOpenPanel()
        panel.allowedContentTypes = [.application]
        panel.directoryURL = URL(fileURLWithPath: "/Applications")
        panel.canChooseDirectories = false
        panel.allowsMultipleSelection = false
        if panel.runModal() == .OK, let url = panel.url,
           let bundleId = Bundle(url: url)?.bundleIdentifier {
            pendingKeystroke.remove(g)
            self[g] = make(bundleId)
        }
    }

    func choosePath(_ g: Gesture) {
        let panel = NSOpenPanel()
        panel.canChooseFiles = true
        panel.canChooseDirectories = true
        panel.allowsMultipleSelection = false
        if panel.runModal() == .OK, let url = panel.url {
            pendingKeystroke.remove(g)
            self[g] = .openPath(path: url.path)
        }
    }
}
