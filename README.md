# Antiknob

A modern native macOS Apple Silicon (`arm64`) configurator and utility for the **Anticater VK01 Knob** mechanical keyboard.

Antiknob is written in 100% pure Rust and native SwiftUI, permissively licensed (**MIT OR Apache-2.0**), fully commercializable with zero copyleft/dual-license traps, and operates completely unprivileged (**no `sudo` required**) via Apple's native `IOHIDManager`.

---

## Highlights

* **Native macOS SwiftUI Configurator (`Antiknob.app`)**:
  * **System Settings Aesthetic**: Native `.formStyle(.grouped)` layout with Liquid Glass materials and SF Symbols.
  * **System-Managed Tab Bar**: A native `TabView` renders the tab strip into the window titlebar, alongside the traffic lights. Four tabs — General, Layers, Hardware, Services. It used to be one tab per LAYER plus five more, which grew a strip that has a fixed budget: two layers fitted, five would have pushed Hardware and Services off the end. Layers live behind one tab now, chosen by a control that does not care how many there are. Lighting stopped being a tab because it is not a peer of the layers — it is part of one.
  * **Interactive Knob Centerpiece**: Rendered knob header with clickable gesture zones (Twist Left/Right, Hold + Twist Left/Right, Press) that highlight and select the corresponding gesture row.
  * **Layer Management**: Each layer's pane carries its name, its position (`Move Left` / `Move Right`, with a `n of m` readout) and a confirmed `Delete Layer`; `+` in the toolbar adds one.
  * **Column Layout**: Property rows, status rows and record lists are laid out on shared column edges (`PropertyGrid` / `StatusRow` / `PropertyRow`), so labels, values, state lights and controls each read down one straight edge instead of ragging against the trailing margin. HID endpoints are a four-column table; state lights sit in their own column right of the text.
  * **System Settings Capsule Chord Recorder**: One-click shortcut capture displaying macOS native glyphs (`⌃`, `⌥`, `⇧`, `⌘`).
  * **Macro Sequence Editor**: Sheet modal supporting multi-step macros, millisecond wait steps, and drag-to-reorder.
  * **Per-Layer Lighting**: Each host layer carries a backlight mode (Off, Red, Green, Ripple, Rainbow, RGB) and the daemon writes it to the knob on every layer switch, so the light says which layer you are on. The firmware stores modes per DEVICE layer and knows nothing about host layers, so this is something the daemon does rather than something the hardware offers — see `host::led_sync`. Picked from a row of glyphs, beside a knob whose ring animates the selected mode the way the hardware renders it. The glyphs carry WITHOUT their colour — `r.circle.fill` and `g.circle.fill` rather than one filled circle tinted two ways — because a row with no labels and the name on hover made the two fixed-colour modes identical to anyone who cannot separate red from green; a test asserts no two modes share a glyph. Modes 1 and 2 are fixed COLOURS on the device rather than effects, so the KNOB tells layers apart by colour — which is what the colour bytes never achieved. The pane reads the firmware's current mode rather than showing the last thing clicked. Note what that read-back does and does not prove: it confirms the device STORED a mode, never that the light rendered it. This repo spent a session treating the two as one thing while a knob glowing red reported green.
  * **Bottom Status Bar**: Connection, transport and power source read as SF Symbol glyphs in the lower-right corner (words in the tooltip), next to a manual refresh and the transient autosave badge.
  * **One Mapping Per Question**: Transport is a `Transport` enum, not a string compared at each call site, so every switch over it is exhaustive and adding a link is a compile error at each place that must render it. Power source, transport glyph and LED mode name each have exactly one definition.
* **Single Source of Truth (`src/api/registry.rs`)**:
  * Every command declared once: name, description, and each parameter with its kind, whether it is required, and which surfaces it appears on. The CLI's clap tree, the MCP tool list and the settings app's capability list are all GENERATED from it — the app reads the table over the socket with `list_commands` rather than keeping a copy, so a command cannot exist on one surface and not the other unless the table says why. A parameter can differ per surface where it must — `upload` takes a file path on the command line and a YAML string over MCP, because an agent has no filesystem the daemon can see.
* **Unix Domain Socket Interface (`/tmp/antiknob.sock`)**:
  * Fast JSON-RPC 2.0 communication between UI, CLI, and Daemon.
  * Dynamically hot-reloads the daemon tap engine in memory without restarting.
* **Model Context Protocol (MCP) Server**:
  * Built-in standard MCP server (protocol version `2024-11-05`) over stdio via `antiknob mcp` or `antiknob-daemon --mcp`.
  * Allows AI agents (Claude Desktop, Antigravity, etc.) to inspect status, read/write host configs, switch layers, reprogram LED lighting, and trigger hardware actions.
* **Direct `CGEventTap` Keyboard Tap**:
  * The daemon drives a `CGEventTap` itself rather than through an input crate, so the dependency tree carries no unmaintained Objective-C shims. Modifiers arrive as `FlagsChanged` with no up/down bit of their own; the decoder derives it from the device-dependent flag bits, which distinguishes left from right — releasing one Control while the other is held is reported for the key that actually moved.
  * Keyboard events only. A tap that also saw the mouse could swallow it.
* **Host-Side Translation ("bind once")**:
  * One-time firmware slot binding (`ctrl-alt-shift-F16..F20`, one chord per gesture, hold+twist included) plus a macOS daemon that swallows those chords and runs unlimited layered actions (scroll, keystrokes, sequences, media, brightness, launch/open/quit, mouse) with double-tap and hotkey layer switching.
* **Unprivileged USB HID (`no sudo`)**:
  * Targets vendor usage page (`0xFF00`), avoiding macOS kernel driver collisions and running cleanly as a regular user.
  * All hidapi work is marshalled onto one dedicated, event-loop-free thread (`device::with_hid` / `device::with_device`). hidapi's macOS backend binds its IOHIDManager sources to the CFRunLoop of the thread that first initialised it, so a call from any other thread traps inside CoreFoundation. The single-thread rule makes that unrepresentable and is enforced by `tests/hid_thread_affinity.rs`.
  * That thread also RE-ENUMERATES the bus before every job. `HidApi::new` takes one snapshot, so a daemon that never refreshes keeps answering from it: unplug the knob and plug it back in and macOS issues a new device path, every open fails against the old one, and `get_status` still reports the stale list as connected — a green dot beside hardware nothing can touch. Refreshing on the thread rather than at the call sites means no command can forget to.

---

## Hardware Support

Tested and verified with the following hardware:
* **Vendor ID**: `0x514C` (LQKJ) / `0x1189` (CH57x)
* **Product ID**: `0x8850`, `0x8840`, `0x8842`, `0x8851`, `0x8890`
* **Report Interface**: Report ID `0x03`, 64-byte payload.

---

## Installation to macOS `/Applications`

Run the included installer to build and install `Antiknob.app`, the daemon, and CLI tools:

```bash
./install.sh
```

This will:
1. Build native Apple Silicon release binaries (`antiknob`, `antiknob-daemon`).
2. Build and codesign native SwiftUI `Antiknob.app` via `ui/build.sh`.
3. Assemble and codesign `AntiknobDaemon.app` (menu-bar only, `LSUIElement`).
4. Install to `/Applications/Antiknob` with symlinks in `/Applications` and `~/.local/bin`.

### Letting the daemon read the knob

The daemon translates knob gestures through a global keyboard tap, and macOS
refuses that tap until it is granted. Switch on **AntiknobDaemon** in
**System Settings > Privacy & Security > Accessibility** (if it is not listed,
click **+** and add `/Applications/Antiknob/AntiknobDaemon.app`). Everything
else — flashing the hardware, LED control, the settings app — works without
it. The menu-bar icon says `knob gestures off` while the grant is missing and
its menu opens the pane for you.

The grant lands on a code signature, not a path. With the default ad-hoc
signature every rebuild is a program macOS has not seen before, so a
reinstall silently voids a switch that still looks on — toggle it off and on
again. To keep one identity across rebuilds, make a self-signed code-signing
certificate named `Antiknob Dev` (Keychain Access > Certificate Assistant >
Create a Certificate, type "Code Signing") and `install.sh` will find it, or
name any identity yourself:

```bash
ANTIKNOB_SIGN_ID="Antiknob Dev" ./install.sh
```

---

## Quick Start

### Native SwiftUI Settings App
```bash
# Launch via terminal or Spotlight:
open -a Antiknob

# Or run the installed binaries directly:
/Applications/Antiknob.app/Contents/MacOS/Antiknob
/Applications/Antiknob/bin/antiknob-daemon --active
```

### Model Context Protocol (MCP) Server
```bash
# Launch the MCP server over stdio for LLM tools:
antiknob mcp

# Or via the daemon binary:
antiknob-daemon --mcp
```

### Command-Line Interface

Every command below is declared once in [`src/api/registry.rs`](src/api/registry.rs);
the CLI and the MCP tool list are both generated from it, so they cannot
describe different devices.

```bash
# Probe the connected device
antiknob status
antiknob status --json

# What the device understands. Ask rather than guess -- an unknown action
# name is refused, and the vocabulary is not macOS key names.
antiknob show-keys

# Flash a layout
antiknob validate config.yaml
antiknob upload config.yaml
antiknob upload config.yaml --layer 0        # one layer only

# Bind ONE gesture to a sequence, with a wait between steps.
# Key ids: 2=twist CCW, 3=press, 4=twist CW, 5=hold+twist L, 6=hold+twist R
antiknob bind-seq --key 5 cmd-c cmd-v
antiknob bind-seq --key 5 --delay-ms 120 cmd-a cmd-c
antiknob bind-seq --key 5 --dry-run h e l l o     # see the packet first

# LED. Modes 1 and 2 are fixed COLOURS on this device, not effects.
antiknob led 0 red
antiknob led 0 green
antiknob led 0 rainbow          # cycling multicolour, the shipped effect
antiknob led 0 rgb              # a second multicolour effect
antiknob led 0 off
antiknob led-read 0

# Read the device back. --full uses the vendor's burst read: three queries
# for every key on every layer, instead of eighteen for the first six.
antiknob read-slots --full

# Which key does each gesture actually drive? Arms keys 1-6 with distinct
# markers, asks for all five gestures, restores everything afterwards.
antiknob probe-gestures --map

# Verify what the knob sends
antiknob listen --timeout-secs 10

# Host-translate slot binding, and the daemon's presets
antiknob bind-slots --dry-run
antiknob import-presets --out ~/Library/Application\ Support/antiknob/host.json
antiknob list-apps
```

#### Gestures and sequences

A knob is **five** gestures, and a gesture can run a **sequence** with a wait
between steps -- that is the vendor's `0xFD` command. Single keyboard and
media actions go out as `0xFD` too: the `0xFE` record this tool used for them
declared no entry count, so the device stored it, read it back verbatim and
executed nothing. In a layout:

```yaml
knobs:
  - ccw: "volumedown"                 # one action
    press: "mute"
    cw: "volumeup"
    hold_twist_l: ["cmd-c", "cmd-v"]  # a sequence
    hold_twist_r:                     # a sequence that waits between steps
      steps: ["cmd-a", "cmd-c"]
      delay_ms: 120
```

A slot holds 19 entries; a chord costs one entry per modifier plus one for
the key, so `cmd-c` is two. Only KEYBOARD sequences chain -- the firmware
stores one media action per slot, so a media list is refused when the config
loads rather than silently truncated when it flashes.

That is enough to type a string or drive an application: the vendor's own
Procreate menu is nothing but named chords, and its Undo encodes as
`F4 1D` -- Cmd+Z. The wire format is in [VENDOR_UI_MAP.md](VENDOR_UI_MAP.md).

### Host Translation Daemon
```bash
# Observe what slot chords would do (nothing swallowed, nothing synthesized)
antiknob-daemon --timeout-secs 10

# Go live: swallow slot chords, synthesize layered actions, show menu-bar icon
antiknob-daemon --active

# Start at login (per-user LaunchAgent) / remove it
antiknob-daemon --install-login-item
antiknob-daemon --uninstall-login-item
```

Run `--install-login-item` from `AntiknobDaemon.app/Contents/MacOS/AntiknobDaemon`
(what `install.sh` prints) rather than the loose `bin/antiknob-daemon`: only the
app bundle can be granted Accessibility, and the login item starts whichever one
installed it.

The agent restarts the daemon if it crashes, so `kill` alone will not stop it —
that is launchd, not a hang. Use **Quit** in the menu bar, or:

```bash
launchctl bootout gui/$(id -u)/com.antiknob.daemon
```

---

## Automated Test Suite

```bash
cargo test --all-targets --all-features
```

The Swift half is an SPM package and has its own suite:

```bash
swift test --package-path ui
```

Quality gates cover both halves, each with a measured coverage floor:

```bash
./tools/gate.sh --full
```

| Layer | What it runs |
| --- | --- |
| structural | emoji, conflict markers, file length, shell lint, secrets |
| rust | `fmt`, `clippy -D warnings`, cargo manifest lints, no-`#[allow]`, coverage floor |
| swift | `swiftlint --strict` against a shrink-only baseline, a **cold** warnings-as-errors build, `swift test`, coverage floor |
| repo | the Rust test suite, `cargo audit` |

`--staged` is the fast pre-commit scope; `--full` is the pre-push scope and is
what CI runs.

Two ratchets keep the debt shrink-only. [.cargo/audit.toml](.cargo/audit.toml)
holds the dependency-advisory ignores -- every entry states why it may stand,
and one whose advisory stops firing is deleted rather than left to rot.
[ui/.swiftlint-baseline.json](ui/.swiftlint-baseline.json) holds today's lint
debt: a new violation fails, and so does a listed one that grows, because the
match key includes the measured count. Neither is re-recorded to silence a
failure. Swift gate config lives in [ui/.gatesrc](ui/.gatesrc), which explains
what its 4% coverage floor does and does not mean.

---

## Repository Map

| Path | What it is |
| --- | --- |
| [config.yaml](config.yaml) | Full three-layer starter config, installed to `/Applications/Antiknob/config.yaml` on first install (never overwritten afterwards). |
| [PLAN.md](PLAN.md) | Forward-looking backlog. Shipped work lives in git history, not here. |
| [VENDOR_UI_MAP.md](VENDOR_UI_MAP.md) | The wire protocol, measured by driving the vendor app under an HID interposer. The `0xFD` record format, the gesture-to-key map, the delay encoding, the LED modes. Measurement, not plan — it does not get pruned. |
| [FINDINGS.md](FINDINGS.md) | The original 2026 porting analysis of `/Applications/ANTICATER.app`. Historical: its device IDs are the `1189:884x` family, not this `514c:8850` knob. |
| [REFERENCES.md](REFERENCES.md) | Where each fact came from, and what each source is NOT good for. |
| [BATTERY_RESEARCH.md](BATTERY_RESEARCH.md) | Research notes on radio-mode drain and power reporting. No code depends on it. |
| [.cargo/audit.toml](.cargo/audit.toml) | `cargo audit` deny list and the advisory-ignore ratchet. |
| [ui/Package.swift](ui/Package.swift) | The SwiftUI app as an SPM package, so the house Swift gate can reach it. |
| [ui/.swiftlint.yml](ui/.swiftlint.yml) | Lint rules, with the reason for each deviation from the defaults. |
| [ui/.swiftlint-baseline.json](ui/.swiftlint-baseline.json) | Shrink-only lint ratchet. Re-recorded only when the debt shrinks, never to silence a failure. |

---

## License & Commercialization

Antiknob is dual-licensed under either:
* **MIT License** ([LICENSE-MIT](LICENSE-MIT))
* **Apache License, Version 2.0** ([LICENSE-APACHE](LICENSE-APACHE))

at your option. All crate dependencies are strictly permissive (MIT / Apache-2.0 / BSD) with zero copyleft licenses.
