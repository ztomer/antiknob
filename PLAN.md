# Antiknob: Native ARM64 Plan for Anticater VK01 Knob

## Strategic Context & Deadline

* **Urgency**: **Rosetta 2 deprecation in macOS 27 (September 2026)**.
  The vendor application (`/Applications/ANTICATER.app`) is strictly `x86_64` and will cease functioning when macOS 27 releases. A native Apple Silicon (`arm64`) solution is mandatory.
* **No Qt Requirement**:
  Qt 5.12 was an artifact of the vendor's legacy stack. There is no requirement to retain Qt.
* **Foundational Project Found**:
  [`kriomant/ch57x-keyboard-tool`](https://github.com/kriomant/ch57x-keyboard-tool) already provides an open-source Rust implementation of the CH57x USB protocol, with explicit built-in support for model `ch57x-1` (`VID 0x1189`, `PID 0x8840` / `0x8842` / `0x8850` / `0x514C:0x8851`), matching the exact USB identifiers extracted from `/Applications/ANTICATER.app`.

---

## Technical Architecture

```
+-------------------------------------------------------+
|                     User Config                       |
|           (config.yaml: keys, layers, knobs)          |
+-------------------------------------------------------+
                           |
                           v
+-------------------------------------------------------+
|               ch57x-keyboard-tool                     |
|           (Native Rust binary: aarch64)               |
+-------------------------------------------------------+
                           |
                           v
+-------------------------------------------------------+
|                    rusb / libusb                      |
|          (macOS IOKit / IOUSBHost backend)            |
+-------------------------------------------------------+
                           |
                           v
+-------------------------------------------------------+
|             Anticater VK01 Knob Keyboard              |
|        (CH57x Microcontroller: 0x1189:0x8840)         |
+-------------------------------------------------------+
```

---

## Capabilities of `ch57x-keyboard-tool` for Anticater VK01

1. **Knob Support**:
   * Counter-Clockwise (`ccw`)
   * Clockwise (`cw`)
   * Press (`press`)
2. **Layer Support**:
   * Up to 16 layers (typically 3 layers configured in hardware).
3. **Action Types**:
   * Standard keys (`a`–`z`, `0`–`9`, `F1`–`F24`, symbols).
   * Modifiers: `cmd` / `win`, `opt` / `alt`, `ctrl`, `shift`, right-hand modifiers.
   * Media controls: `volumeup`, `volumedown`, `mute`, `play`, `next`, `prev`.
   * Mouse events: `click`, `rclick`, `mclick`, `move(x, y)`, `drag`, `wheelup`, `wheeldown`.
   * Sequences & Macros: Multi-key chords and timed delays (`<100ms>`).
4. **LED Controls (`0x8840` / `0x8842`)**:
   * Modes: `off`, `backlight <color>`, `shock <color>`, `shock2 <color>`, `press <color>`.
   * Colors: `white` (backlight only), `red`, `orange`, `yellow`, `green`, `cyan`, `blue`, `purple`.

---

## Implementation Workstreams

1. **Tool Compilation**:
   * Build `ch57x-keyboard-tool` locally using the system's native `cargo` (`aarch64-apple-darwin`).
2. **Hardware Mapping Profile**:
   * Create `config.yaml` tailored to the physical button and knob count of the Anticater VK01.
3. **Workflow Automation**:
   * Provide a lightweight wrapper CLI (`antiknob`) for convenient validation, flashing, and LED adjustments.
