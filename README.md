# Antiknob

A modern native macOS Apple Silicon (`arm64`) configurator and utility for the **Anticater VK01 Knob** mechanical keyboard.

Antiknob is written in 100% pure Rust and native SwiftUI, permissively licensed (**MIT OR Apache-2.0**), fully commercializable with zero copyleft/dual-license traps, and operates completely unprivileged (**no `sudo` required**) via Apple's native `IOHIDManager`.

---

## Highlights

* **Native macOS SwiftUI Configurator (`Antiknob.app`)**:
  * **System Settings Aesthetic**: Native `.formStyle(.grouped)` layout with Liquid Glass materials and SF Symbols.
  * **Interactive Knob Centerpiece**: Rendered knob header with clickable gesture zones (Twist Left/Right, Hold + Twist Left/Right, Press) that highlight and select the corresponding gesture row.
  * **Horizontal Layer Tabs**: Drag-and-drop layer reordering, context menus (Move Left/Right, Delete Layer), and `+` button to add layers.
  * **System Settings Capsule Chord Recorder**: One-click shortcut capture displaying macOS native glyphs (`⌃`, `⌥`, `⇧`, `⌘`).
  * **Macro Sequence Editor**: Sheet modal supporting multi-step macros, millisecond wait steps, and drag-to-reorder.
  * **Dynamic Hardware Lighting**: Real-time LED mode controls (Off, Backlight, Shock/Breathe, Shock 2, Press Reactive, Custom) with color swatches sending instant updates to hardware.
  * **Transient Autosave Badge**: Seamless background saving with instant apply over the daemon socket.
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

Quality gates:
```bash
./tools/gate.sh --full
```

---

## License & Commercialization

Antiknob is dual-licensed under either:
* **MIT License** ([LICENSE-MIT](LICENSE-MIT))
* **Apache License, Version 2.0** ([LICENSE-APACHE](LICENSE-APACHE))

at your option. All crate dependencies are strictly permissive (MIT / Apache-2.0 / BSD) with zero copyleft licenses.
