# Antiknob / ANTICATER Porting & Architecture Findings

This document details the reverse-engineering and architecture analysis of `/Applications/ANTICATER.app` (the configuration utility for the Anticater VK01 Knob mechanical keyboard) and outlines the feasibility and requirements for porting it natively to Apple Silicon (ARM64).

---

## 1. Executive Summary

* **Current Operational Status**: **Functional today on Apple Silicon via Rosetta 2, BUT deprecated this month.**  
  While `/Applications/ANTICATER.app` currently launches on macOS via Rosetta 2, **Rosetta 2 is being removed in macOS 27 (September 2026)**. Because the vendor bundle is strictly an `x86_64` thin binary, it will permanently fail to execute once macOS 27 is installed. A native ARM64 solution is urgent and mandatory.
* **Native Solution Identified ([`ch57x-keyboard-tool`](https://github.com/kriomant/ch57x-keyboard-tool))**:  
  An open-source Rust implementation already exists and explicitly supports the CH57x USB protocol for model `1189:8840` / `1189:8842` (the exact VID/PID of the Anticater VK01 Knob). There is no need to write a protocol layer from scratch.
* **No Qt Requirement**:  
  Qt 5.12 was purely the vendor's GUI framework. Qt is not a binding requirement. We can utilize `ch57x-keyboard-tool` directly as a native `aarch64` CLI tool driven by declarative YAML configuration files.
* **Full Plan**: See [PLAN.md](PLAN.md) and [`implementation_plan.md`](file:///Users/ztomer/.gemini/antigravity/brain/d29140a2-39b9-40cc-8217-bc0487535c5d/implementation_plan.md) for the native implementation steps.

---

## 2. Application Architecture & Inspection Details

### 2.1 Bundle & Execution Info
* **App Path**: `/Applications/ANTICATER.app`
* **Original Package**: `/Users/ztomer/drive/dotfiles/devices/anticater_vk01_knob_mac_en.zip` -> `mac.EN/Mac.pkg`
* **Bundle Identifier**: `COM.LQKJ.KEYBOARD.ANTICATER`
* **Executable**: `Contents/MacOS/ANTICATER` (Mach-O 64-bit executable `x86_64`)
* **Code Signing**:
  * Authority: `Developer ID Application: dehui li (4Z759TG5T3)`
  * Team ID: `4Z759TG5T3`
  * Hardened Runtime: Enabled (`Runtime Version=15.2.0`)
  * Gatekeeper: Accepted (`spctl -a -vv` passes)

### 2.2 Frameworks and Dependencies
The application was built using **Qt 5.12.9** and **libhidapi 0.12.0**:
* **Bundled Frameworks** (`Contents/Frameworks/`):
  * `QtCore.framework` (5.12.9, `x86_64`)
  * `QtGui.framework` (5.12.9, `x86_64`)
  * `QtWidgets.framework` (5.12.9, `x86_64`)
  * `QtNetwork.framework`, `QtSvg.framework`, `QtQml.framework`, `QtQuick.framework`, `QtVirtualKeyboard.framework`, `QtDBus.framework`, `QtPrintSupport.framework`
  * `libhidapi.0.12.0.dylib` (symlinked as `libhidapi.0.dylib`)
* **Bundled Plugins** (`Contents/PlugIns/`):
  * `platforms/libqcocoa.dylib` (`x86_64`)
  * Image formats, virtual keyboard, styles, etc.
* **System Frameworks Linked**:
  * `IOKit.framework`, `DiskArbitration.framework`, `OpenGL.framework`, `AGL.framework`, `libc++.1.dylib`, `libSystem.B.dylib`

### 2.3 Hardware Identifiers Extracted from Binary
Analysis of the `__DATA` segment and symbol table revealed the USB HID identifiers and memory structures used by the application:
* **Vendor ID (`_VID`)**: `0x1189`
* **Primary Product ID (`_PID`)**: `0x8840`
* **Supported Product ID Group (`_PID_GRU`)**:
  * `0x8842`
  * `0x8840`
  * `0x8830`
  * `0x8831`
  * `0x8832`
  * `0x8833`
  * `0x8850`
  * `0x8851`
* **Firmware/Protocol Version Info (`_KD_Ver_Infor`)**: `0x0002`

### 2.4 Internal Symbols & State Structures
Extracted global variables and methods in `ANTICATER`:
* **Key Configuration**:
  * `_Cur_KeyBoard_KeyNum`
  * `_PHY_KEY_Value` / `_PHY2_KEY_Value`
  * `_Select_PHY_Key` / `_Select_PHY_Key_Layer` / `_Select_PHY_Key_Mode`
  * `_KeyVale_ModifyFlag`
  * `Widget::InitBasicEn()`
  * `Widget::SetBasicKey(int)`
* **RGB LED Controls**:
  * `_RGB_LED_Md`
  * `_RgbLED_Change_Flag`
  * `_KeyBoard_KeyLed`
* **Knob Parameters**:
  * `_KN1_Height`, `_KN2_Height`, `_KN3_Height`, `_KN4_Height`
* **HID Communication Functions**:
  * Calls standard HIDAPI primitives (`hid_init`, `hid_open`, `hid_read_timeout`, `hid_write`, `hid_close`).

---

## 3. Requirements for Porting to ARM64

### Option A: Official / Source-Based Port (Vendor Path)
To compile a native `arm64` or Universal 2 binary from source code:
1. **Upgrade Qt**:
   * Qt 5.12.9 does not have Apple Silicon support (Qt was first compiled for Apple Silicon in Qt 5.15 commercial patches and fully in Qt 6.2+).
   * Upgrade the project configuration (`anticater.pro` / CMake) to build against Qt 5.15.x (arm64) or Qt 6.
2. **Build Dependencies for ARM64**:
   * Build `hidapi` (`libhidapi.dylib`) for `arm64` or create a universal binary via `lipo`:
     ```bash
     lipo -create -output libhidapi.dylib libhidapi_x86_64.dylib libhidapi_arm64.dylib
     ```
3. **Configure Multi-Arch Build**:
   * In `qmake`:
     ```qmake
     QMAKE_APPLE_DEVICE_ARCHS = arm64 x86_64
     ```
   * Or in CMake:
     ```cmake
     set(CMAKE_OSX_ARCHITECTURES "arm64;x86_64")
     ```
4. **Bundle & Sign**:
   * Run the ARM-native `macdeployqt` to copy arm64 Qt frameworks and plugins into the `.app` bundle.
   * Sign and notarize with Apple Developer ID.

### Option B: Binary Conversion Without Source Code (Not Viable)
* Static re-translation of a complex C++ GUI application linking against dynamic C++ frameworks, Objective-C cocoa runtime hooks, and plugins is technically intractable.
* Rosetta 2 already serves as Apple's runtime binary translator. Until Apple removes Rosetta 2 from macOS, running the existing binary under Rosetta 2 is the intended way to run x86_64 software on Apple Silicon.

### Option C: Clean-Room Native ARM64 Driver / Configurator
If the goal is to have an open-source, native Apple Silicon solution that does not rely on Rosetta or vendor binaries:
1. The hardware uses standard USB HID commands via `libhidapi` (VID: `0x1189`, PID: `0x8840`).
2. A lightweight Python CLI / GUI tool (using `hidapi`) or a browser WebHID app can directly send and receive feature reports.
3. USB packets can be captured and mapped using Wireshark (with USBPcap or macOS `xpcproxy`/PacketLogger) while interacting with the official ANTICATER app under Rosetta.

---

## 4. USB HID Quick-Start / Device Enumeration Script

Below is a minimal, native Python 3 script using `hidapi` to detect the Anticater device natively on Apple Silicon without Rosetta:

```python
#!/usr/bin/env python3
"""
Anticater VK01 USB HID Device Enumeration Script
"""
import hid

VENDOR_ID = 0x1189
PRODUCT_IDS = [0x8840, 0x8842, 0x8830, 0x8831, 0x8832, 0x8833, 0x8850, 0x8851]

def print_info(message):
    print(f"[ ==> ] {message}")

def print_wrn(message):
    print(f"[ Wrn ] {message}")

def print_err(message):
    print(f"[ Err ] {message}")

def print_ok(message):
    print(f"[ Ok  ] {message}")

def find_anticater():
    print_info("Enumerating USB HID devices for Anticater hardware...")
    devices = hid.enumerate(VENDOR_ID)
    found = []
    for d in devices:
        if d['product_id'] in PRODUCT_IDS:
            found.append(d)
            print_ok(f"Found Anticater device: PID=0x{d['product_id']:04x}, Path={d['path']}, Product={d.get('product_string', '')}")
    if not found:
        print_wrn("No Anticater devices found. Ensure the device is connected.")
    return found

if __name__ == "__main__":
    find_anticater()
```
