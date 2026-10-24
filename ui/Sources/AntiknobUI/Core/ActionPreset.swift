// ActionPreset.swift — which entry of the action picker an Action is, and
// what the picker calls it.
//
// The layer pane kept TWO parallel switches over `Action`: one that named the
// current choice for the dropdown's title and one that mapped it to the
// picker's selection. They had to agree case for case, nothing checked that
// they did, and each sat over the cyclomatic cap on its own. One mapping
// from Action to preset, and one from preset to title, with the title
// derived from the preset, makes the disagreement unrepresentable -- and
// both are pure, so they have tests, which the view-bound copies never did.

import Foundation

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

extension Action {
    /// The picker entry for this action.
    ///
    /// `defaultScrollLines` decides the direction of a scroll that names no
    /// count of its own. A pending keystroke (a gesture the user just set to
    /// "Keystroke" and has not recorded yet) is the view's state, not an
    /// action, and is resolved by the caller before this is asked.
    func preset(defaultScrollLines: Int32) -> ActionPreset {
        switch self {
        case .scroll(let lines):
            return (lines ?? defaultScrollLines) >= 0 ? .scrollUp : .scrollDown
        case .aux(let key):
            return .aux(key.unified)
        default:
            return plainPreset
        }
    }

    /// The presets that depend on nothing but the case. `scroll` and `aux`
    /// are answered by `preset(defaultScrollLines:)` and never reach here;
    /// they are listed so a new `Action` case is a compile error, not a
    /// silent `.none`.
    private var plainPreset: ActionPreset {
        switch self {
        case .none, .mouseClick, .scroll, .aux: return .none
        case .keyChord: return .keystroke
        case .sequence: return .sequence
        case .launchApp: return .openApp
        case .openURL: return .openURL
        case .openPath: return .openPath
        case .quitApp: return .quitApp
        case .hotkeySwitch: return .hotkeySwitch
        }
    }
}

extension ActionPreset {
    /// What the dropdown reads while this entry is selected.
    var title: String {
        if case .aux(let key) = self { return auxTitles[key] ?? "Media" }
        return Self.fixedTitles[self] ?? "None"
    }

    /// Every preset that is not parameterised. A preset missing here would
    /// read "None"; `ActionPresetTests` walks every case to keep that from
    /// happening.
    static let fixedTitles: [ActionPreset: String] = [
        .none: "None",
        .scrollUp: "Scroll Up",
        .scrollDown: "Scroll Down",
        .keystroke: "Keystroke",
        .openApp: "Open App",
        .sequence: "Sequence",
        .openURL: "Website",
        .openPath: "Open File",
        .quitApp: "Quit App",
        .hotkeySwitch: "Hotkey Switch"
    ]
}
