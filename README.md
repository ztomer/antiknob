# Antiknob

A modern native macOS Apple Silicon (`arm64`) configurator and utility for the **Anticater VK01 Knob** mechanical keyboard.

Antiknob is written in 100% pure Rust and native SwiftUI, permissively licensed (**MIT OR Apache-2.0**), fully commercializable with zero copyleft/dual-license traps, and operates completely unprivileged (**no `sudo` required**) via Apple's native `IOHIDManager`.

---

## Highlights

* **Native macOS SwiftUI Configurator (`Antiknob.app`)**:
  * **System Settings Aesthetic**: Native `.formStyle(.grouped)` layout with Liquid Glass materials and SF Symbols.
  * **System-Managed Tab Bar**: A native `TabView` renders the tab strip into the window titlebar, alongside the traffic lights. One tab per layer, plus Switching, Lighting, Hardware, Inspector and Services.
  * **Interactive Knob Centerpiece**: Rendered knob header with clickable gesture zones (Twist Left/Right, Hold + Twist Left/Right, Press) that highlight and select the corresponding gesture row.
  * **Layer Management**: Each layer's pane carries its name, its position (`Move Left` / `Move Right`, with a `n of m` readout) and a confirmed `Delete Layer`; `+` in the toolbar adds one.
  * **Column Layout**: Property rows, status rows and record lists are laid out on shared column edges (`PropertyGrid` / `StatusRow` / `PropertyRow`), so labels, values, state lights and controls each read down one straight edge instead of ragging against the trailing margin. HID endpoints are a four-column table; state lights sit in their own column right of the text.
  * **System Settings Capsule Chord Recorder**: One-click shortcut capture displaying macOS native glyphs (`⌃`, `⌥`, `⇧`, `⌘`).
  * **Macro Sequence Editor**: Sheet modal supporting multi-step macros, millisecond wait steps, and drag-to-reorder.
  * **Dynamic Hardware Lighting**: Real-time LED mode controls (Off, Backlight, Shock/Breathe, Shock 2, Press Reactive, Custom) with color swatches sending instant updates to hardware.
  * **Bottom Status Bar**: Connection, transport and power source read as SF Symbol glyphs in the lower-right corner (words in the tooltip), next to a manual refresh and the transient autosave badge.
  * **One Mapping Per Question**: Transport is a `Transport` enum, not a string compared at each call site, so every switch over it is exhaustive and adding a link is a compile error at each place that must render it. Power source, transport glyph and LED mode name each have exactly one definition.
* **Single Source of Truth (`src/api/`)**:
  * Unified schema and tool definitions shared across the Unix socket interface and the MCP server.
* **Unix Domain Socket Interface (`/tmp/antiknob.sock`)**:
  * Fast JSON-RPC 2.0 communication between UI, CLI, and Daemon.
  * Dynamically hot-reloads the daemon tap engine in memory without restarting.
* **Model Context Protocol (MCP) Server**:
  * Built-in standard MCP server (protocol version `2024-11-05`) over stdio via `antiknob mcp` or `antiknob-daemon --mcp`.
  * Allows AI agents (Claude Desktop, Antigravity, etc.) to inspect status, read/write host configs, switch layers, reprogram LED lighting, and trigger hardware actions.
* **Host-Side Translation ("bind once")**:
  * One-time firmware slot binding (`ctrl-alt-F16..F18`) plus a macOS daemon that swallows those chords and runs unlimited layered actions (scroll, keystrokes, sequences, media, brightness, launch/open/quit, mouse) with double-tap and hotkey layer switching.
* **Unprivileged USB HID (`no sudo`)**:
  * Targets vendor usage page (`0xFF00`), avoiding macOS kernel driver collisions and running cleanly as a regular user.
  * All hidapi work is marshalled onto one dedicated, event-loop-free thread (`device::with_hid` / `device::with_device`). hidapi's macOS backend binds its IOHIDManager sources to the CFRunLoop of the thread that first initialised it, so a call from any other thread traps inside CoreFoundation. The single-thread rule makes that unrepresentable and is enforced by `tests/hid_thread_affinity.rs`.

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
```bash
# 1. Probe connected USB device
antiknob status
antiknob status --json

# 2. List supported keys, media keys, and mouse actions
antiknob show-keys

# 3. Validate configuration file
antiknob validate config.yaml

# 4. Flash configuration to hardware (No sudo required)
antiknob upload config.yaml
antiknob upload config.yaml --layer 0   # flash one layer only

# 5. Set LED lighting dynamically
antiknob led 0 backlight white
antiknob led 0 shock blue
antiknob led 0 off

# 6. One-time host-translate slot binding (flash once, translate forever)
antiknob bind-slots --dry-run   # inspect the 9-packet plan first
antiknob bind-slots             # CCW=ctrl-alt-F16, Press=F17, CW=F18

# 7. Verify what the knob actually sends
antiknob listen --timeout-secs 10   # twist/press the knob, watch reports

# 8. Migrate presets to daemon host layers
antiknob import-presets --out ~/Library/Application\ Support/antiknob/host.json

# 9. List installed apps (bundle IDs for launch/quit actions)
antiknob list-apps
```

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
| [config_knob_only.yaml](config_knob_only.yaml) | Minimal template for a knob with no keypad -- a starting point for `antiknob upload`. |
| [PLAN.md](PLAN.md) | Forward-looking backlog. Shipped work lives in git history, not here. |
| [FINDINGS.md](FINDINGS.md) | Reverse-engineering record for `/Applications/ANTICATER.app` and the VK01 wire protocol. |
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
