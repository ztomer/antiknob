# Antiknob

A native macOS Apple Silicon (`arm64`) configurator for the **Anticater VK01 Knob** mechanical keyboard.

Antiknob is written in 100% pure Rust, permissively licensed (**MIT OR Apache-2.0**), fully commercializable with zero copyleft/dual-license traps, and operates completely unprivileged (**no `sudo` required**) via Apple's native `IOHIDManager`.

---

## Features

* **Apple Silicon Native (`arm64`)**: Fully prepared for macOS 27+ (with zero reliance on Rosetta 2).
* **Unprivileged USB HID (`no sudo`)**: Targets the dedicated vendor usage page (`0xFF00`), avoiding macOS kernel driver collisions and running cleanly as a regular user.
* **100% Permissive Open Source**: Dual-licensed under MIT OR Apache-2.0 with an audited dependency tree (0% copyleft/GPL/AGPL/LGPL).
* **Declarative YAML Mapping**: Easily configure buttons, knobs, and multiple layers in simple YAML files.
* **Multi-Action Knob Support**: Program clockwise rotation (`cw`), counter-clockwise rotation (`ccw`), and center press (`press`).
* **RGB LED Management**: Control backlighting, reactive shock effects, keypress lighting, and colors.

---

## Hardware Support

Tested and verified with the following hardware:
* **Vendor ID**: `0x514C` (LQKJ) / `0x1189` (CH57x)
* **Product ID**: `0x8850`, `0x8840`, `0x8842`, `0x8851`, `0x8890`
* **Report Interface**: Report ID `0x03`, 64-byte payload.

---

## Installation & Build

Requires Rust toolchain (`cargo`):

```bash
cd ~/Projects/antiknob
cargo build --release
cargo install --path . --root .
```

The native binary is installed to `bin/antiknob`.

---

## Usage

You can use either `./bin/antiknob` or the convenience wrapper `./antiknob.sh`:

### 1. Check Connected Device Status (No Sudo)
```bash
./antiknob.sh status
```

Example Output:
```text
[ ==> ] Scanning for Anticater / CH57x USB devices...
[ Ok  ] Found: Anticater / LQKJ VK01 (0x514c:0x8850) (VID: 0x514c, PID: 0x8850, UsagePage: 0xff00)
        Serial Number: EB60121120051103
[ Ok  ] Unprivileged access verified: Device can be configured WITHOUT sudo!
```

### 2. Validate Configuration File
```bash
./antiknob.sh validate config.yaml
./antiknob.sh validate config_knob_only.yaml
```

### 3. Flash Keymap to Keyboard Over USB (No Sudo)
```bash
./antiknob.sh upload config.yaml
```

### 4. Adjust RGB LED Lighting
```bash
# Set layer 0 to white steady backlight
./antiknob.sh led 0 backlight white

# Set layer 0 to blue reactive shock effect
./antiknob.sh led 0 shock blue

# Turn off LEDs
./antiknob.sh led 0 off
```

### 5. View Supported Keycodes
```bash
./antiknob.sh show-keys
```

---

## Configuration Example (`config.yaml`)

```yaml
model: ch57x-1
orientation: normal
rows: 1
columns: 3
knobs: 1

layers:
  # Layer 0: Media & Audio Controls
  - buttons:
      - ["play", "prev", "next"]
    knobs:
      - ccw: "volumedown"
        press: "mute"
        cw: "volumeup"

  # Layer 1: Navigation & Productivity Shortcuts
  - buttons:
      - ["cmd-c", "cmd-v", "cmd-z"]
    knobs:
      - ccw: "wheelup"
        press: "click"
        cw: "wheeldown"

  # Layer 2: Workspace & Zoom Controls
  - buttons:
      - ["ctrl-left", "ctrl-up", "ctrl-right"]
    knobs:
      - ccw: "cmd-minus"
        press: "cmd-0"
        cw: "cmd-equal"
```

---

## Quality Assurance & Local Gates (`gates_of_heck`)

Antiknob is wired into the local quality gate system [`gates_of_heck`](file:///Users/ztomer/Projects/gates_of_heck) via `.githooks/`:

* **`pre-commit`**: Automatically runs structural gates over staged files (file length <= 500 lines, emoji policy, shell lint, no committed secrets, no conflict markers).
* **`pre-push` / Full Gates**:
  ```bash
  tools/gate.sh --full
  ```
  Runs full structural checks, Rust formatting (`cargo fmt --check`), strict Clippy (`-D warnings`, all targets, all features), manifest linting, and `no #[allow]` policy enforcement.

---

## License & Commercialization

Antiknob is dual-licensed under either:
* **MIT License** ([LICENSE-MIT](LICENSE-MIT))
* **Apache License, Version 2.0** ([LICENSE-APACHE](LICENSE-APACHE))

at your option. All crate dependencies are exclusively permissive (MIT / Apache-2.0 / BSD) with zero copyleft licenses.
