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

## Open items (need hands on hardware or the owner)

1. Twist-to-action proof: twist/press the knob under `--active`, confirm
   FIRED lines + real effects. (Self-injection via osascript does NOT
   reach taps; cannot automate headless.)
2. Slot bindings flashed live 2026-09-07 (9 packets, all layers); verify
   with `antiknob listen` while twisting.
