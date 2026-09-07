# Antiknob

A modern native macOS Apple Silicon (`arm64`) configurator and GUI utility for the **Anticater VK01 Knob** mechanical keyboard.

Antiknob is written in 100% pure Rust, permissively licensed (**MIT OR Apache-2.0**), fully commercializable with zero copyleft/dual-license traps, and operates completely unprivileged (**no `sudo` required**) via Apple's native `IOHIDManager`.

---

## Features

* **Apple Silicon Native (`arm64`)**: Prepared for macOS 27+ with zero reliance on Rosetta 2.
* **Modern Desktop GUI (`antiknob-gui` / `Antiknob.app`)**:
  * **Interactive Radial Dial**: Visual rotary dial with 3 functional sectors (<- CCW, CW ->, Press). Press+twist sectors render but are firmware stubs; use hold+twist host gestures instead.
  * **Direct Keyboard Shortcut Recorder**: Click any knob action or key and press the keys on your keyboard ("Press to Assign").
  * **Curated Workflow Presets**: 1-click profiles for Video Scrubbing (Final Cut / Premiere), Media Master, Digital Art (Photoshop / Procreate), Spaces & Window Tiling, Developer, and Web Reading.
  * **Live Animated RGB LED Ring Simulator**: Real-time breathing, shock, and backlight pulse waveforms matching active device lighting.
  * **Profile Management**: Human-readable YAML profiles with 1-click Import / Export and layer sync.
* **Unprivileged USB HID (`no sudo`)**: Targets vendor usage page (`0xFF00`), avoiding macOS kernel driver collisions and running cleanly as a regular user.
* **Crash-proof HID isolation**: hidapi's macOS backend pumps the calling thread's runloop, which aborts inside GUI event loops. The GUI process therefore never calls HID in-process — every device operation shells out to the `antiknob` CLI (`status --json`, `upload --layer`, `led`, `bind-slots`). Only the CLI touches hardware, on a plain main thread.
* **Triple Binaries**: CLI tool (`antiknob`) for headless automation, full macOS GUI (`antiknob-gui`), and host-side translation daemon (`antiknob-daemon` / `AntiknobDaemon.app`, menu-bar only).
* **Host-Side Translation ("bind once")**: One-time firmware slot binding (`ctrl-alt-F16..F18`) plus a macOS daemon that swallows those chords and runs unlimited layered actions (scroll, keystrokes, sequences, media, brightness, launch/open/quit, mouse) with double-tap and hotkey layer switching. Only the daemon needs Accessibility / Input Monitoring; GUI and CLI stay grant-free.
* **100% Permissive Open Source**: Dual-licensed under MIT OR Apache-2.0 with an audited dependency tree (0% copyleft/GPL/AGPL/LGPL).

---

## Hardware Support

Tested and verified with the following hardware:
* **Vendor ID**: `0x514C` (LQKJ) / `0x1189` (CH57x)
* **Product ID**: `0x8850`, `0x8840`, `0x8842`, `0x8851`, `0x8890`
* **Report Interface**: Report ID `0x03`, 64-byte payload.

---

## Installation to macOS `/Applications`

Run the included installer to build and install `Antiknob.app` and CLI tools:

```bash
./install.sh
```

This will:
1. Build native Apple Silicon release binaries.
2. Install to `/Applications/Antiknob`.
3. Create and ad-hoc codesign `Antiknob.app` (plus a Spotlight symlink at `/Applications/Antiknob.app`).
4. Create and ad-hoc codesign `AntiknobDaemon.app` (menu-bar only, `LSUIElement`, no dock icon).
5. Symlink the CLIs to `~/.local/bin/antiknob{,-daemon}`.

---

## Quick Start

### Graphical User Interface
```bash
# Launch via terminal or Spotlight:
open -a Antiknob

# Or run the installed binaries directly:
/Applications/Antiknob/bin/antiknob-gui
/Applications/Antiknob/bin/antiknob-daemon --active
```

### Command-Line Interface
```bash
# 1. Probe connected USB device
antiknob status

# 2. List supported keys, media keys, and mouse actions
antiknob show-keys

# 3. Validate configuration file
antiknob validate config.yaml

# 4. Flash configuration to hardware (No sudo required)
antiknob upload config.yaml
antiknob upload config.yaml --layer 0   # flash one layer only

# 4b. Machine-readable device list (used by the GUI subprocess bridge)
antiknob status --json

# 5. Set LED lighting
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

The GUI's Host Layers tab views and edits the same `host.json`
(layers, gestures, sequences, app pickers); the running daemon applies
edits within a tick via instant-apply.

---

## Automated Test Suite

Antiknob includes unit tests, end-to-end integration tests, and headless GUI state machine tests:

```bash
cargo test --all-targets --all-features
```

---

## Quality Assurance (`gates_of_heck`)

Antiknob is wired into the local quality gate system [`gates_of_heck`](https://github.com/ztomer/gates_of_heck) via `.githooks/`:

* **`pre-commit`**: Automatically runs structural gates over staged files (file length <= 500 lines, emoji policy, shell lint, no committed secrets).
* **`pre-push` / Full Gates**:
  ```bash
  ./tools/gate.sh --full
  ```
  Enforces strict formatting (`cargo fmt --check`), Clippy (`-D warnings`), manifest linting, and `no #[allow]` policy.

---

## License & Commercialization

Antiknob is dual-licensed under either:
* **MIT License** ([LICENSE-MIT](LICENSE-MIT))
* **Apache License, Version 2.0** ([LICENSE-APACHE](LICENSE-APACHE))

at your option. All crate dependencies are strictly permissive (MIT / Apache-2.0 / BSD) with zero copyleft licenses.
