# Antiknob Plan (forward-looking)

## Context

* Hardware: Anticater VK01 knob, VID `0x514C` (LQKJ), PID `0x8850`
  (serial `EB60121120051103`); vendor usage page `0xFF00`, report `0x03`.
* Zero-sudo IOHIDManager access. MIT OR Apache-2.0, native arm64.
* Architecture: GUI + CLI stay TCC-free (device flashing, presets, LED,
  YAML). The daemon alone needs Accessibility / Input Monitoring
  (host-side translation). `host.json` is the contract between them.

## Shipped in 0.2.0

* Crash fixes: worker-thread SIGTRAP closed by moving HID to the main
  thread; then a launch-time SIGABRT (hid_enumerate pumping the running
  event loop reentrantly) closed architecturally — the GUI process never
  calls hidapi and shells out to the CLI (`gui::clihid`: resolution,
  `status --json`, `upload --layer`, `led`, `bind-slots`). Pinned by a
  structural test asserting zero in-process HID under `src/gui`.
* Host engine (`src/host/`): JSON layers, rolodex, double-tap window,
  hotkey-switch alternation, chord matching, output plans, preset
  migration, app discovery, login-item plist, tap glue.
* Daemon (`antiknob-daemon`, `AntiknobDaemon.app`): observe/active modes,
  full synthesis (keys, scroll, fn-brightness, NX media, mouse incl.
  middle, launch/open/quit), tray with layer title + menu, login item,
  instant-apply with last-good rule.
* CLI: `status validate upload led show-keys bind-slots listen
  import-presets list-apps`.
* GUI: presets, recorder, palettes, Slots tab, Host Layers tab with full
  gesture editor (None/Scroll/Media/Key/Sequence/launch/quit/open/mouse),
  preset export, layer-switch LED sync.

## Shipped in 0.2.1

* LED lighting & hardware protocol fully verified:
  - Reverse-engineered `Widget::HID_write` and `Widget::Read_RgbLed_DataDsp` from
    vendor binary (`/Applications/ANTICATER.app`).
  - Resolved double report ID bug in `device::send_report`: stripped redundant
    `0x03` prefix so command bytes land at wire index 1.
  - Added 16-color RGB palette population (`fill_palette`) in `protocol::build_led_packet`
    so single-color modes illuminate all 16 ring LEDs.
  - Live hardware verification: modes 0 (off), 1 (backlight), 2 (shock/reactive),
    3 (shock2/ripple), 4 (press) successfully written, committed (`[FD FE FF]`),
    persisted, and verified via `antiknob led-read`.
  - Added CLI `led-read` command with human-readable mode decoding and `--raw` dump.
  - Added CLI `read-slots` command and `raw` packet dispatch for diagnostics.
* Codebase architecture & gates:
  - Refactored `src/bin/antiknob.rs` into `src/bin/antiknob/main.rs` and `cmds.rs`.
  - Strictly enforced `.gatesrc` file length gate (`GOH_MAX_LINES=500` across all files).
  - 100% test pass rate across all 79 unit, integration, and E2E tests.
  - Fully passing `./tools/gate.sh --full` (structural, fmt, clippy, cargo lints).

## Shipped in 0.3.0

* Native macOS SwiftUI Configurator (`Antiknob.app` in `ui/`):
  - Native `.formStyle(.grouped)` layout with Liquid Glass and system vibrancy.
  - Interactive rotary knob centerpiece with clickable gesture zones (twist, hold+twist, press) highlighting matching form rows.
  - Horizontal layer tab strip with drag-to-reorder, right-click context menus, and `+` to add layers.
  - System Settings capsule shortcut recorder capturing macOS native modifier glyphs.
  - Sequence editor sheet modal for multi-step macros with delay steps and reordering.
  - Dynamic hardware lighting controls (`LedSection.swift`) with mode selection and color swatches sending live `set_led` commands to hardware.
  - Transient autosave badge providing visual confirmation without manual save buttons.
* Single Source of Truth (`src/api/`):
  - Unified tool schemas and command dispatch (`get_status`, `get_config`, `set_config`, `set_layer`, `set_led`, `get_led`, `bind_slots`, `upload_keymap`, `list_apps`).
* Unix Domain Socket Interface (`/tmp/antiknob.sock`):
  - Non-blocking JSON-RPC 2.0 socket listener in `antiknob-daemon`.
  - Dynamic in-memory tap engine hot-reloading.
* Model Context Protocol (MCP) Server:
  - Standard stdio MCP server (`2024-11-05`) in `antiknob-daemon --mcp` and `antiknob mcp`.
* Automated E2E & Quality Verification:
  - Added `tests/api_e2e.rs` testing socket server round-trip and MCP protocol flows.
  - 100% test pass rate (85 tests) and strict `./tools/gate.sh --full` compliance.

