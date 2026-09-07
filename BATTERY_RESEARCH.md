# VK-01 Knob Battery Life: Radio-Mode Drain, USB Use, Alternative Firmware

Status: research only. No code or device changes were made for this file.
Date: 2026-09-07. Device context: Anticater VK-01 knob, currently connected over USB.

## TL;DR

- Yes, there is a simple solution for desk use: run the knob in USB-C wired
  mode permanently. Wired mode draws host power (no battery drain), charges
  the cell, and is the only mode in which the vendor enables lighting
  (reseller spec sheets: "lighting only available when connected via wired
  mode to conserve battery life"). Switch the device to wired mode and, to be
  safe, unplug the 2.4 GHz dongle so there is no ambiguity about which link
  is active.
- Poor battery in radio mode, especially 2.4 GHz mode, is expected by design:
  2.4 GHz keeps a high-rate link to the dongle (typically ~1000 Hz / ~1 ms),
  while Bluetooth LE uses aggressive sleep between bursts. On the WCH CH57x
  series SoC used in this hardware family, the radio draws milliamps when
  transmitting/receiving versus microamps in sleep, so any firmware behavior
  that keeps the radio awake (high poll rate, no/fast sleep, retransmits
  under interference, LEDs) dominates battery life.
- There is no safe drop-in alternative firmware. QMK supports Atmel AVR and
  Arm USB MCUs; the VK-01 family is built on the WCH CH57x BLE SoC
  (RISC-V / Cortex-M0 + 2.4 GHz radio), which QMK does not target. ZMK targets
  Nordic nRF parts, same verdict. Community tooling
  (kriomant/ch57x-keyboard-tool, achushu/CH57x-keyboard-mapper, and this repo)
  configures the stock firmware over USB instead of replacing it. The only
  sanctioned firmware is the vendor 1.1.0 release on anticater.com.

## 1. Device and radio modes

- Product: Anticater VK-01 desktop volume knob, CNC aluminium body
  (40 mm x 25 mm, 65 g), built-in battery plus lighting.
- Variants: dual-mode (USB-C wired + Bluetooth) and tri-mode
  (USB-C wired + Bluetooth + 2.4 GHz via dongle).
- Relevant USB IDs seen on this hardware family: VID 0x1189 / 0x514C with
  PIDs including 0x8840, 0x8842, 0x8850, 0x8851, 0x8890. The community
  ch57x-keyboard-tool explicitly supports 1189:8890, 1189:8840, 1189:8842.
- Third-party manual claims "approximately 30 days on a full charge". Treat
  as a best case (light use, lighting off); vendor publishes no cell
  capacity, and the 65 g / 40x25 mm envelope leaves room for only a small
  cell.
- Vendor FAQ notes the back cover keeps an acrylic RF window because a fully
  metal back would block the signal: the radio link is inherently
  constrained by the metal enclosure, which matters for interference and
  retransmit-driven drain (section 2).

## 2. Why radio mode drains the battery (and why 2.4 GHz is worst)

1. Polling rate. 2.4 GHz links commonly poll at ~1000 Hz (report every
   ~1 ms, ~1-5 ms latency) versus Bluetooth at ~125 Hz with ~20-40 ms+
   latency. Power scales with wake-and-transmit frequency, so 2.4 GHz draws
   more under active use.
2. Idle/sleep behavior. Bluetooth LE transmits in short bursts and sleeps
   aggressively between them. Proprietary 2.4 GHz maintains a tighter,
   continuous link to the dongle for responsiveness, so idle drain is higher
   and depends heavily on firmware sleep tuning.
3. SoC currents (WCH CH573 / CH579 datasheets, 25 C reference values):
   TX ~6 mA at 0 dBm, RX ~3.5-7.5 mA, idle ~1.1-1.5 mA, halt ~320-420 uA,
   sleep ~0.3-6 uA depending on retained blocks. Radio-on versus radio-asleep
   differs by roughly three orders of magnitude: firmware that delays sleep
   or never reaches it is the battery killer, not the encoder itself.
4. Lighting. LEDs are the largest sustained load after the radio, which is
   why the vendor gates lighting to wired mode. Any third-party setting that
   keeps backlight on in wireless mode will swamp radio savings.
5. Interference and retransmits. The 2.4 GHz band is shared with Wi-Fi and
   Bluetooth; a congested environment forces retransmits at full TX power.
   The metal body with a small RF window makes placement matter: blocked or
   distant dongle placement increases TX power and retries.
6. Knob traffic pattern. A knob rotated continuously streams far more
   reports than occasional key presses, keeping the link out of sleep. Heavy
   volume-scrubbing sessions over 2.4 GHz are the worst case for this
   device class.

 Net: Bluetooth generally wins on battery, 2.4 GHz wins on latency and
 consistency. "Extremely poor" life in 2.4 GHz mode with default settings is
 consistent with all of the above and does not by itself indicate a faulty
 cell, though a degraded cell cannot be ruled out without measurement
 (section 6).

## 3. USB-connected use: recommended setup (the practical fix)

1. Switch the knob itself to wired mode (mode switch, not just plugging the
   cable in). Verify the OS sees the wired HID path: `antiknob status`
   should show the device over USB.
2. Unplug the 2.4 GHz dongle while docked. This removes any doubt about
   which link carries input and frees a USB port.
3. Use a USB-C data cable (charge-only cables can leave the device on
   radio while appearing "plugged in"). Prefer a direct host port over an
   unpowered hub for stable charging current.
4. Lighting is safe to use while wired (vendor design intent). If you go
   back to battery, turn lighting off first: `antiknob led 0 off`
   (per-layer backlight/shock modes are the wireless drain to avoid).
5. Set the shortest auto-sleep in the vendor app's DelaySetting tab if you
   ever run wireless; this repo does not currently expose that setting
   (open item, section 6).
6. If you must stay wireless, prefer Bluetooth over 2.4 GHz for battery,
   keep the dongle (if used) close with line of sight, and move away from
   congested Wi-Fi channels.
7. Battery care per the manual: recharge from a standard USB source, avoid
   frequent full drains and prolonged overcharging to prolong cell life.

## 4. Alternative firmware: verdict is no

- QMK: supports Atmel AVR and Arm USB MCU families (3000+ boards). The
  VK-01 family runs a WCH CH57x SoC (CH573: Qingke RISC-V3A + BLE 4.2;
  CH579: Cortex-M0 + BLE/Ethernet). No QMK MCU target, no board port, no
  documented bootloader or pinout for this board. A port would be a new
  architecture effort plus a from-scratch board definition with bricking
  risk. Not recommended.
- ZMK (BLE): targets Nordic nRF MCUs only. Same verdict.
- What does exist: stock-firmware configurators used over a USB cable:
  kriomant/ch57x-keyboard-tool (MIT, ~1.2k stars, exact VID/PID match),
  achushu/CH57x-keyboard-mapper (MIT), and this repo (antiknob, same
  lineage: HID report 0x03, 64-byte payload, YAML profiles, LED control).
  These write key bindings into the vendor firmware and exit; none of them
  replaces the firmware, and none can change radio poll rates or sleep
  tables the vendor firmware does not expose.
- Vendor firmware 1.1.0 for Windows/macOS is published on anticater.com
  (Download page). That is the only sanctioned update path; check it before
  assuming a battery bug, since sleep/RF behavior is entirely in vendor
  firmware.
- Flashing anything else risks: bricking (no recovery docs), losing 2.4 GHz
  dongle pairing, and voiding seller support. Do not attempt unless you
  accept a dead device and have SWD/pinout data, which is not published.

## 5. If long wireless battery is a hard requirement

The honest alternative is different hardware, not different firmware on this
board. Categories worth comparing (no specific endorsement):

- QMK/VIA-compatible wired macropads with a knob: zero battery concern,
  full remapping, same desk role as a USB-docked VK-01.
- ZMK (nRF-based) BLE macropads/knobs: BLE-first power design with
  documented sleep tuning, at the cost of DIY or enthusiast pricing.
- Plain Bluetooth (non-2.4 GHz) single-purpose dials: best battery-per-cost
  if latency does not matter.

Decision rule: if the knob lives at one desk (your current USB setup), keep
the VK-01 wired. Only shop for new hardware if multi-day wireless life is
the actual requirement.

## 6. Open questions (measurable, not blocking)

- Actual VK-01 sleep current and sleep delay defaults in stock firmware
  (vendor does not publish; would need a USB power meter or battery-logging
  run in each mode).
- Whether wired mode fully powers down the RF section or merely prefers USB
  (behavioral test: wired in, dongle out, confirm input still flows).
- Whether the vendor app's DelaySetting (auto-sleep) persists and whether it
  is worth exposing in antiknob's CLI/GUI.
- Cell health on this specific unit: if wired-or-BT life is also collapsing,
  suspect an aged cell rather than radio behavior.

## 7. Sources

- Anticater VK-01 product page (modes, battery+light, dimensions, FAQ on RF
  window and power-on design):
  https://anticater.com/products/anticater-vk-01-desktop-volume-control-knob
- VK-01 user manual via manuals.plus (2.4G/BT setup, ~30-day claim, battery
  care): https://manuals.plus/ae/1005009577840607
- Reseller spec sheets noting lighting is wired-only (Rebult Keyboards,
  ilumkb, same vendor text):
  https://www.rebultkeyboards.com/products/anticater-vk01-knob
  https://ilumkb.com/products/anticater-vk-01-desktop-volume-control-knob
- Bluetooth vs 2.4 GHz comparison (polling rates, latency, sleep behavior):
  https://shop.rapoo.com/blogs/product-comparisons/bluetooth-vs-2-4ghz-wireless-keyboard
  https://store.angrymiao.com/blogs/insider-stories/wireless-vs-bluetooth-mouse
- Power deep-dive (BLE sleep vs 2.4 GHz continuous link):
  https://smart.dhgate.com/bluetooth-vs-2-4-ghz-wireless-mouse-keyboard-which-kills-the-battery-faster/
- WCH CH573 datasheet page (RISC-V + BLE, sleep/shutdown currents):
  https://www.wch-ic.com/downloads/CH573DS1_PDF.html
- WCH CH579 datasheet (Cortex-M0 + BLE, TX/RX/idle/halt/sleep figures):
  https://5.imimg.com/data5/SELLER/Doc/2024/3/397610618/UI/YR/GN/21854127/wch-microcontroller-ch579f-ic.pdf
- QMK Firmware (supported MCU families): https://qmk.fm/
- kriomant/ch57x-keyboard-tool README (supported VID/PIDs, USB-cable
  programming, LED modes): https://github.com/kriomant/ch57x-keyboard-tool
- achushu/CH57x-keyboard-mapper (stock-firmware mapper, MIT):
  https://github.com/achushu/CH57x-keyboard-mapper
