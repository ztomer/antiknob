// Models.swift — Core data models and serialization for Antiknob.
// Matches the host config schema and vk01-anticater vocabulary.

import AppKit
import Foundation

// MARK: - Enums & Gestures

enum AuxKey: String, Codable, CaseIterable, Hashable, Sendable {
    case volumeUp
    case volumeDown
    case mute
    case playPause
    case next
    case previous
    case brightnessUp
    case brightnessDown
    case brightnessUpExternal
    case brightnessDownExternal

    var unified: AuxKey {
        switch self {
        case .brightnessUpExternal: return .brightnessUp
        case .brightnessDownExternal: return .brightnessDown
        default: return self
        }
    }
}

let auxTitles: [AuxKey: String] = [
    .volumeUp: "Volume Up",
    .volumeDown: "Volume Down",
    .mute: "Mute",
    .playPause: "Play / Pause",
    .next: "Next Track",
    .previous: "Previous Track",
    .brightnessUp: "Brightness Up",
    .brightnessDown: "Brightness Down"
]

enum MouseButton: String, Codable, CaseIterable, Hashable, Sendable {
    case left
    case right
    case middle
}

// MARK: - Key Chords & Sequences

/// Hardware LED modes, as the firmware reports them back through `get_led`.
///
/// Mirrors `led_mode_name` on the Rust side; the Inspector prints the number
/// beside the name so a drift between the two is visible rather than silent.
/// The previous inline ternary chain fell through to "Press" for anything
/// above 3, which mislabelled mode 5 (custom).
let ledModeNames: [Int: String] = [
    0: "Off",
    1: "Backlight",
    2: "Shock (Breathe)",
    3: "Shock 2 (Rapid)",
    4: "Press (Reactive)",
    5: "Custom"
]

/// The link the knob is on.
///
/// Raw values are the daemon's `transport` strings. Parsing once, here, is
/// what lets every mapping below switch EXHAUSTIVELY: adding a transport
/// becomes a compile error at each site that has to render it, instead of a
/// silent `default:` fall-through to "Disconnected". The colour mapping lives
/// on `ConfigStore` (it needs SwiftUI) and is switched over this same enum,
/// so the two cannot drift apart the way two string switches did.
enum Transport: String, CaseIterable, Sendable {
    case usb
    case wireless24GHz = "wireless_2_4g"
    case bluetooth

    var displayName: String {
        switch self {
        case .usb: return "USB (Wired)"
        case .wireless24GHz: return "2.4GHz Wireless"
        case .bluetooth: return "Bluetooth Wireless"
        }
    }

    var icon: String {
        switch self {
        case .usb: return "cable.connector"
        case .wireless24GHz: return "antenna.radiowaves.left.and.right"
        case .bluetooth: return "wave.3.right"
        }
    }
}

/// The status bar's read of the device, as a pure value.
///
/// Transport and power render as SF Symbol glyphs with the words in the
/// tooltip, so glyph and tooltip must come from one place or they drift.
/// `powerIcon` is derived from the daemon's own `powerDescription` for
/// exactly that reason -- it cannot disagree with the text beside it.
struct StatusPresentation: Equatable, Sendable {
    let transport: String
    let powerDescription: String
    let connected: Bool

    /// nil when the daemon reports a link this build does not know (including
    /// its "disconnected" sentinel).
    var link: Transport? { Transport(rawValue: transport) }

    var transportDisplay: String { link?.displayName ?? "Disconnected" }
    var transportIcon: String { link?.icon ?? "circle.slash" }

    var powerIcon: String {
        guard connected else { return "powerplug.slash" }
        return powerDescription.localizedCaseInsensitiveContains("battery")
            ? "battery.100"
            : "powerplug.fill"
    }
}

struct KeyChordSpec: Codable, Equatable, Hashable, Sendable {
    var key: UInt16
    var mods: [String]
    var label: String?

    var display: String {
        var s = ""
        if mods.contains("ctrl") { s += "⌃" }
        if mods.contains("opt") || mods.contains("alt") { s += "⌥" }
        if mods.contains("shift") { s += "⇧" }
        if mods.contains("cmd") { s += "⌘" }
        let k = label ?? specialKeyNames[key] ?? fallbackKeyNames[key] ?? "key \(key)"
        return s + k
    }
}

struct SeqStep: Codable, Equatable, Hashable, Sendable {
    var key: UInt16?
    var mods: [String]?
    var label: String?
    var delayMs: Int?

    enum CodingKeys: String, CodingKey {
        case key, mods, label
        case delayMs
        case delay_ms
    }

    init(key: UInt16? = nil, mods: [String]? = nil, label: String? = nil, delayMs: Int? = nil) {
        self.key = key
        self.mods = mods
        self.label = label
        self.delayMs = delayMs
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        key = try c.decodeIfPresent(UInt16.self, forKey: .key)
        mods = try c.decodeIfPresent([String].self, forKey: .mods)
        label = try c.decodeIfPresent(String.self, forKey: .label)
        if let d = try c.decodeIfPresent(Int.self, forKey: .delayMs) {
            delayMs = d
        } else {
            delayMs = try c.decodeIfPresent(Int.self, forKey: .delay_ms)
        }
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encodeIfPresent(key, forKey: .key)
        try c.encodeIfPresent(mods, forKey: .mods)
        try c.encodeIfPresent(label, forKey: .label)
        try c.encodeIfPresent(delayMs, forKey: .delayMs)
    }
}

// MARK: - Actions

enum Action: Codable, Equatable, Hashable, Sendable {
    case none
    case scroll(lines: Int32?)
    case keyChord(key: UInt16, mods: [String], label: String?)
    case sequence(steps: [SeqStep])
    case aux(key: AuxKey)
    case mouseClick(button: MouseButton)
    case launchApp(bundleId: String)
    case openURL(url: String)
    case openPath(path: String)
    case quitApp(bundleId: String, force: Bool?)
    case hotkeySwitch(first: KeyChordSpec?, second: KeyChordSpec?)

    enum CodingKeys: String, CodingKey {
        case type, lines, key, mods, label, steps, button, bundleId, bundle_id
        case url, path, force, first, second
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        let t = (try? c.decode(String.self, forKey: .type)) ?? "none"
        switch t {
        case "scroll":
            let l = try? c.decodeIfPresent(Int32.self, forKey: .lines)
            self = .scroll(lines: l ?? nil)
        case "keyChord":
            let k = try c.decode(UInt16.self, forKey: .key)
            let m = (try? c.decode([String].self, forKey: .mods)) ?? []
            let lbl = try? c.decodeIfPresent(String.self, forKey: .label)
            self = .keyChord(key: k, mods: m, label: lbl)
        case "sequence":
            let s = (try? c.decode([SeqStep].self, forKey: .steps)) ?? []
            self = .sequence(steps: s)
        case "aux":
            let k = try c.decode(AuxKey.self, forKey: .key)
            self = .aux(key: k.unified)
        case "mouseClick":
            let b = try c.decode(MouseButton.self, forKey: .button)
            self = .mouseClick(button: b)
        case "launchApp":
            let b = (try? c.decode(String.self, forKey: .bundleId))
                ?? (try? c.decode(String.self, forKey: .bundle_id)) ?? ""
            self = .launchApp(bundleId: b)
        case "openURL", "openUrl":
            let u = try c.decode(String.self, forKey: .url)
            self = .openURL(url: u)
        case "openPath":
            let p = try c.decode(String.self, forKey: .path)
            self = .openPath(path: p)
        case "quitApp":
            let b = (try? c.decode(String.self, forKey: .bundleId))
                ?? (try? c.decode(String.self, forKey: .bundle_id)) ?? ""
            let f = try? c.decodeIfPresent(Bool.self, forKey: .force)
            self = .quitApp(bundleId: b, force: f ?? nil)
        case "hotkeySwitch":
            let f = try? c.decodeIfPresent(KeyChordSpec.self, forKey: .first)
            let s = try? c.decodeIfPresent(KeyChordSpec.self, forKey: .second)
            self = .hotkeySwitch(first: f ?? nil, second: s ?? nil)
        default:
            self = .none
        }
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .none:
            try c.encode("none", forKey: .type)
        case .scroll(let lines):
            try c.encode("scroll", forKey: .type)
            try c.encodeIfPresent(lines, forKey: .lines)
        case .keyChord(let key, let mods, let label):
            try c.encode("keyChord", forKey: .type)
            try c.encode(key, forKey: .key)
            try c.encode(mods, forKey: .mods)
            try c.encodeIfPresent(label, forKey: .label)
        case .sequence(let steps):
            try c.encode("sequence", forKey: .type)
            try c.encode(steps, forKey: .steps)
        case .aux(let key):
            try c.encode("aux", forKey: .type)
            try c.encode(key.unified, forKey: .key)
        case .mouseClick(let button):
            try c.encode("mouseClick", forKey: .type)
            try c.encode(button, forKey: .button)
        case .launchApp(let bundleId):
            try c.encode("launchApp", forKey: .type)
            try c.encode(bundleId, forKey: .bundleId)
        case .openURL(let url):
            try c.encode("openURL", forKey: .type)
            try c.encode(url, forKey: .url)
        case .openPath(let path):
            try c.encode("openPath", forKey: .type)
            try c.encode(path, forKey: .path)
        case .quitApp(let bundleId, let force):
            try c.encode("quitApp", forKey: .type)
            try c.encode(bundleId, forKey: .bundleId)
            try c.encodeIfPresent(force, forKey: .force)
        case .hotkeySwitch(let first, let second):
            try c.encode("hotkeySwitch", forKey: .type)
            try c.encodeIfPresent(first, forKey: .first)
            try c.encodeIfPresent(second, forKey: .second)
        }
    }
}

// MARK: - Layer and Config

struct LayerConfig: Codable, Equatable, Hashable, Sendable {
    var name: String
    var twistL: Action?
    var twistR: Action?
    var holdTwistL: Action?
    var holdTwistR: Action?
    var press: Action?

    enum CodingKeys: String, CodingKey {
        case name
        case twistL, twist_l
        case twistR, twist_r
        case holdTwistL, hold_twist_l
        case holdTwistR, hold_twist_r
        case press
    }

    init(name: String, twistL: Action? = nil, twistR: Action? = nil,
         holdTwistL: Action? = nil, holdTwistR: Action? = nil, press: Action? = nil) {
        self.name = name
        self.twistL = twistL
        self.twistR = twistR
        self.holdTwistL = holdTwistL
        self.holdTwistR = holdTwistR
        self.press = press
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        name = try c.decode(String.self, forKey: .name)
        twistL = (try? c.decodeIfPresent(Action.self, forKey: .twistL))
            ?? (try? c.decodeIfPresent(Action.self, forKey: .twist_l)) ?? nil
        twistR = (try? c.decodeIfPresent(Action.self, forKey: .twistR))
            ?? (try? c.decodeIfPresent(Action.self, forKey: .twist_r)) ?? nil
        holdTwistL = (try? c.decodeIfPresent(Action.self, forKey: .holdTwistL))
            ?? (try? c.decodeIfPresent(Action.self, forKey: .hold_twist_l)) ?? nil
        holdTwistR = (try? c.decodeIfPresent(Action.self, forKey: .holdTwistR))
            ?? (try? c.decodeIfPresent(Action.self, forKey: .hold_twist_r)) ?? nil
        press = try? c.decodeIfPresent(Action.self, forKey: .press)
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(name, forKey: .name)
        try c.encode(twistL ?? .none, forKey: .twistL)
        try c.encode(twistR ?? .none, forKey: .twistR)
        try c.encode(holdTwistL ?? .none, forKey: .holdTwistL)
        try c.encode(holdTwistR ?? .none, forKey: .holdTwistR)
        try c.encode(press ?? .none, forKey: .press)
    }

    subscript(g: Gesture) -> Action? {
        get {
            switch g {
            case .twistL: return twistL
            case .twistR: return twistR
            case .holdTwistL: return holdTwistL
            case .holdTwistR: return holdTwistR
            case .press: return press
            }
        }
        set {
            switch g {
            case .twistL: twistL = newValue
            case .twistR: twistR = newValue
            case .holdTwistL: holdTwistL = newValue
            case .holdTwistR: holdTwistR = newValue
            case .press: press = newValue
            }
        }
    }
}

struct Config: Codable, Equatable, Sendable {
    var layers: [LayerConfig]
    var doubleTapSwitch: Bool
    var doubleTapWindow: Double
    var layerHotkey: KeyChordSpec?
    var layerHotkeyBack: KeyChordSpec?
    var scrollLinesPerDetent: Int32

    var doubleTapEnabled: Bool {
        get { doubleTapSwitch }
        set { doubleTapSwitch = newValue }
    }

    var tapWindow: Double {
        get { doubleTapWindow }
        set { doubleTapWindow = newValue }
    }

    var defaultScrollLines: Int32 { scrollLinesPerDetent }

    enum CodingKeys: String, CodingKey {
        case layers
        case doubleTapSwitch, double_tap_switch
        case doubleTapWindow, double_tap_window
        case layerHotkey, layer_hotkey
        case layerHotkeyBack, layer_hotkey_back
        case scrollLinesPerDetent, scroll_lines_per_detent
    }

    init(layers: [LayerConfig] = [], doubleTapSwitch: Bool = true, doubleTapWindow: Double = 0.25,
         layerHotkey: KeyChordSpec? = nil, layerHotkeyBack: KeyChordSpec? = nil,
         scrollLinesPerDetent: Int32 = 3) {
        self.layers = layers
        self.doubleTapSwitch = doubleTapSwitch
        self.doubleTapWindow = doubleTapWindow
        self.layerHotkey = layerHotkey
        self.layerHotkeyBack = layerHotkeyBack
        self.scrollLinesPerDetent = scrollLinesPerDetent
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        layers = (try? c.decode([LayerConfig].self, forKey: .layers)) ?? []
        doubleTapSwitch = (try? c.decode(Bool.self, forKey: .doubleTapSwitch))
            ?? (try? c.decode(Bool.self, forKey: .double_tap_switch)) ?? true
        doubleTapWindow = (try? c.decode(Double.self, forKey: .doubleTapWindow))
            ?? (try? c.decode(Double.self, forKey: .double_tap_window)) ?? 0.25
        layerHotkey = (try? c.decodeIfPresent(KeyChordSpec.self, forKey: .layerHotkey))
            ?? (try? c.decodeIfPresent(KeyChordSpec.self, forKey: .layer_hotkey)) ?? nil
        layerHotkeyBack = (try? c.decodeIfPresent(KeyChordSpec.self, forKey: .layerHotkeyBack))
            ?? (try? c.decodeIfPresent(KeyChordSpec.self, forKey: .layer_hotkey_back)) ?? nil
        scrollLinesPerDetent = (try? c.decode(Int32.self, forKey: .scrollLinesPerDetent))
            ?? (try? c.decode(Int32.self, forKey: .scroll_lines_per_detent)) ?? 3
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(layers, forKey: .layers)
        try c.encode(doubleTapSwitch, forKey: .doubleTapSwitch)
        try c.encode(doubleTapWindow, forKey: .doubleTapWindow)
        try c.encodeIfPresent(layerHotkey, forKey: .layerHotkey)
        try c.encodeIfPresent(layerHotkeyBack, forKey: .layerHotkeyBack)
        try c.encode(scrollLinesPerDetent, forKey: .scrollLinesPerDetent)
    }

    static var defaultPOC: Config {
        Config(layers: [
            LayerConfig(name: "Navigate",
                        twistL: .scroll(lines: -3),
                        twistR: .scroll(lines: 3),
                        holdTwistL: .keyChord(key: 43, mods: ["cmd"], label: ","),
                        holdTwistR: .keyChord(key: 24, mods: ["cmd"], label: "="),
                        press: .keyChord(key: 126, mods: ["cmd"], label: "Up")),
            LayerConfig(name: "Media",
                        twistL: .aux(key: .volumeDown),
                        twistR: .aux(key: .volumeUp),
                        holdTwistL: .aux(key: .brightnessDown),
                        holdTwistR: .aux(key: .brightnessUp),
                        press: .aux(key: .mute))
        ])
    }
}

// MARK: - Key Labels

let knobKeys: Set<Int64> = [106, 64, 79, 80, 90]

let specialKeyNames: [UInt16: String] = [
    36: "⏎", 48: "Tab", 49: "Space", 51: "Delete", 53: "⎋", 76: "Enter",
    115: "Home", 116: "PageUp", 117: "Del", 119: "End", 121: "PageDown",
    123: "←", 124: "→", 125: "↓", 126: "↑",
    122: "F1", 120: "F2", 99: "F3", 118: "F4", 96: "F5", 97: "F6", 98: "F7",
    100: "F8", 101: "F9", 109: "F10", 103: "F11", 111: "F12", 105: "F13",
    107: "F14", 113: "F15", 106: "F16", 64: "F17", 79: "F18", 80: "F19", 90: "F20"
]

let fallbackKeyNames: [UInt16: String] = [
    29: "0", 18: "1", 19: "2", 20: "3", 21: "4",
    23: "5", 22: "6", 26: "7", 28: "8", 25: "9"
]

func keyLabel(_ e: NSEvent) -> String {
    if let s = specialKeyNames[e.keyCode] { return s }
    if let c = e.charactersIgnoringModifiers, !c.isEmpty { return c.uppercased() }
    return "key \(e.keyCode)"
}
