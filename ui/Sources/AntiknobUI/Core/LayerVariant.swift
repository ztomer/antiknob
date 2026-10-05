// LayerVariant.swift — the daemon's alternative binding sets for a virtual
// layer.
//
// Split from Models.swift for the file-length gate, joining Gesture.swift and
// LedModes.swift as the other two extractions from it.

import Foundation

/// One named alternative binding set for a virtual layer.
///
/// Carried so it SURVIVES a round trip, and nothing more. The daemon owns
/// virtual layers (`host::virtual_layer`), it chooses between variants from
/// the frontmost app at the moment a gesture fires, and this app has no
/// control that edits them.
///
/// That is exactly why the field has to exist. A `Codable` type that does not
/// know a key does not preserve it -- it discards it on the way out -- and
/// this store writes the whole file on every edit. So before it existed, the
/// first unrelated change made in this app deleted every variant the daemon
/// had, and the virtual layers went inert with nothing to search for.
///
/// The alternative, a raw-JSON passthrough, was rejected: it would preserve
/// bytes this app cannot interpret and would then be free to write them back
/// wrong. Re-decoding into the same types the daemon uses means an action
/// this app cannot represent shows up as `.none` in the copy, which is
/// visible, rather than as a silently mangled file.
struct LayerVariant: Codable, Equatable, Hashable, Sendable {
    var name: String
    var apps: [String] = []
    var twistL: Action = .none
    var twistR: Action = .none
    var holdTwistL: Action = .none
    var holdTwistR: Action = .none
    var press: Action = .none

    enum CodingKeys: String, CodingKey {
        case name, apps
        case twistL, twist_l
        case twistR, twist_r
        case holdTwistL, hold_twist_l
        case holdTwistR, hold_twist_r
        case press
    }

    init(name: String, apps: [String] = [], twistL: Action = .none, twistR: Action = .none,
         holdTwistL: Action = .none, holdTwistR: Action = .none, press: Action = .none) {
        self.name = name
        self.apps = apps
        self.twistL = twistL
        self.twistR = twistR
        self.holdTwistL = holdTwistL
        self.holdTwistR = holdTwistR
        self.press = press
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        name = try c.decode(String.self, forKey: .name)
        apps = (try? c.decodeIfPresent([String].self, forKey: .apps)) ?? []
        twistL = (try? c.decodeIfPresent(Action.self, forKey: .twistL))
            ?? (try? c.decodeIfPresent(Action.self, forKey: .twist_l)) ?? .none
        twistR = (try? c.decodeIfPresent(Action.self, forKey: .twistR))
            ?? (try? c.decodeIfPresent(Action.self, forKey: .twist_r)) ?? .none
        holdTwistL = (try? c.decodeIfPresent(Action.self, forKey: .holdTwistL))
            ?? (try? c.decodeIfPresent(Action.self, forKey: .hold_twist_l)) ?? .none
        holdTwistR = (try? c.decodeIfPresent(Action.self, forKey: .holdTwistR))
            ?? (try? c.decodeIfPresent(Action.self, forKey: .hold_twist_r)) ?? .none
        press = (try? c.decodeIfPresent(Action.self, forKey: .press)) ?? .none
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(name, forKey: .name)
        try c.encode(apps, forKey: .apps)
        try c.encode(twistL, forKey: .twistL)
        try c.encode(twistR, forKey: .twistR)
        try c.encode(holdTwistL, forKey: .holdTwistL)
        try c.encode(holdTwistR, forKey: .holdTwistR)
        try c.encode(press, forKey: .press)
    }
}
