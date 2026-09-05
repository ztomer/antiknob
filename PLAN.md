# Antiknob: Pure Rust Native Plan for Anticater VK01 Knob

## Strategic Context & Hardware Discovery

* **Hardware Identified & Verified On USB**:
  * Connected device: **VID `0x514C` (LQKJ), PID `0x8850`**
  * Product: `USB Composite Device`, Serial: `EB60121120051103`
  * Vendor Usage Page: `0xFF00`, Usage `0x0001`, Report ID `0x03` (64 bytes).
  * **Zero Sudo Requirement**: Dynamic testing confirmed that macOS `IOHIDManager` allows opening the `0xFF00` configuration interface without root/sudo privileges.
* **Licensing & Commercialization**:
  * **100% Permissive Open Source**: Dual-licensed under **MIT OR Apache-2.0**.
  * **No Dual-Licensing / No Copyleft**: Clean commercialization with zero GPL, AGPL, or SSPL encumbrance.
  * All crate dependencies (`hidapi`, `serde`, `clap`, `anyhow`) are MIT / Apache-2.0.
* **Rosetta 2 Deprecation (macOS 27, September 2026)**:
  * Pure native `aarch64-apple-darwin` binary built with Rust.

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
|                       antiknob                        |
|        (Pure Rust Native Binary: aarch64-apple-darwin)|
|              Dual-licensed: MIT / Apache-2.0          |
+-------------------------------------------------------+
                           |
                           v
+-------------------------------------------------------+
|                    hidapi (MIT)                       |
|          Apple IOHIDManager (macOS System Framework)  |
|            Usage Page 0xFF00 - NO SUDO NEEDED         |
+-------------------------------------------------------+
                           |
                           v
+-------------------------------------------------------+
|             Anticater VK01 Knob Keyboard              |
|        (CH57x Microcontroller: 0x514C:0x8850)         |
+-------------------------------------------------------+
```

---

## Implementation Workstreams

1. **Rust Crate Setup**:
   * Create `Cargo.toml` with `hidapi`, `serde`, `serde_yaml`, `clap`, `anyhow`.
   * Add `LICENSE-MIT` and `LICENSE-APACHE`.
2. **Device & Protocol Layer**:
   * `src/device.rs`: Device enumeration and handle management targeting `UsagePage == 0xFF00`.
   * `src/protocol.rs`: CH57x packet serialization (Report ID 3, 0xFE commands, knob & layer mappings).
3. **Configuration & CLI**:
   * `src/config.rs`: YAML model definition.
   * `src/main.rs`: CLI commands (`status`, `validate`, `upload`, `led`, `show-keys`).
4. **Verification**:
   * Live hardware probe with `antiknob status` confirming unprivileged access to the connected knob.
