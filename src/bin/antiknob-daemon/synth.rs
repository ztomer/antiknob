//! Synthetic output for daemon active mode (slice 2b).
//!
//! macOS only: keys/scroll/brightness via CoreGraphics posts, media keys
//! via NX systemDefined events, launch/open/quit via NSWorkspace. The
//! pure op planning lives in `host::output`; this module only performs.
//! Untestable headless (every call touches the live session), so correctness
//! rests on code inspection against the vk01-anticater reference values.

use antiknob::host::output::OutOp;

#[cfg(target_os = "macos")]
pub fn synthesize(op: &OutOp) {
    match op {
        OutOp::Key { code, flags, down } => mac::synth_key(*code, *flags, *down),
        OutOp::SleepMs(ms) => std::thread::sleep(std::time::Duration::from_millis(*ms)),
        OutOp::Scroll { lines } => mac::synth_scroll(*lines),
        OutOp::MouseClick { button } => mac::synth_click(*button),
        OutOp::AuxNx { key } => mac::post_aux_nx(*key),
        OutOp::Launch { bundle_id } => {
            let ok = mac::ws_launch(bundle_id);
            println!("[slot] launch {}: {}", bundle_id, ok_as(ok));
        }
        OutOp::OpenUrl { url } => {
            let ok = mac::ws_open_url(url);
            println!("[slot] open url {}: {}", url, ok_as(ok));
        }
        OutOp::OpenPath { path } => {
            let ok = mac::ws_open_path(&shellexpand(path));
            println!("[slot] open path {}: {}", path, ok_as(ok));
        }
        OutOp::Quit { bundle_id, force } => {
            let n = mac::ws_quit(bundle_id, *force);
            println!(
                "[slot] quit {} (force={}): {} instance(s)",
                bundle_id, force, n
            );
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub fn synthesize(op: &OutOp) {
    println!("[slot] SKIP (non-macOS build): {:?}", op);
}

#[cfg(target_os = "macos")]
fn ok_as(ok: bool) -> &'static str {
    if ok {
        "ok"
    } else {
        "FAILED"
    }
}

/// `~` expansion for open-path actions (mirrors vk01-anticater).
#[cfg(target_os = "macos")]
fn shellexpand(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return format!("{}/{}", home, rest);
        }
    }
    path.to_string()
}

#[cfg(target_os = "macos")]
mod mac {
    use antiknob::host::MouseButton;
    use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation, ScrollEventUnit};
    use core_graphics::event::{CGEventType, CGMouseButton};
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
    use core_graphics::geometry::CGPoint;
    use objc2::rc::Retained;
    use objc2_app_kit::{NSEvent, NSEventModifierFlags, NSEventType, NSRunningApplication};
    use objc2_core_graphics::{CGEvent as CGEvent2, CGEventTapLocation as TapLoc};
    use objc2_foundation::{NSArray, NSInteger, NSPoint, NSString, NSTimeInterval, NSURL};
    use std::os::raw::c_void;

    fn cg_source() -> Option<CGEventSource> {
        CGEventSource::new(CGEventSourceStateID::HIDSystemState).ok()
    }

    pub fn synth_key(code: u16, flags: u64, down: bool) {
        if let Some(source) = cg_source() {
            if let Ok(ev) = CGEvent::new_keyboard_event(source, code, down) {
                ev.set_flags(CGEventFlags::from_bits_truncate(flags));
                ev.post(CGEventTapLocation::HID);
            }
        }
    }

    pub fn synth_scroll(lines: i32) {
        if let Some(source) = cg_source() {
            if let Ok(ev) = CGEvent::new_scroll_event(source, ScrollEventUnit::LINE, 1, lines, 0, 0)
            {
                ev.post(CGEventTapLocation::HID);
            }
        }
    }

    /// Mouse click at the current cursor location. Left/right go through
    /// the wrapper; middle needs a raw OtherMouse event (kCGEventOtherMouse
    /// Down/Up = 14/15, button 2), which the wrapper enum cannot express.
    pub fn synth_click(button: MouseButton) {
        let point = cg_source()
            .and_then(|src| CGEvent::new(src).ok())
            .map(|ev| ev.location())
            .unwrap_or(CGPoint { x: 0.0, y: 0.0 });
        match button {
            MouseButton::Left => click_wrapped(
                CGEventType::LeftMouseDown,
                CGEventType::LeftMouseUp,
                point,
                CGMouseButton::Left,
            ),
            MouseButton::Right => click_wrapped(
                CGEventType::RightMouseDown,
                CGEventType::RightMouseUp,
                point,
                CGMouseButton::Right,
            ),
            MouseButton::Middle => unsafe {
                for mouse_type in [14u32, 15u32] {
                    let ev = CGEventCreateMouseEvent(std::ptr::null_mut(), mouse_type, point, 2);
                    if !ev.is_null() {
                        CGEventPost(0, ev);
                        CFRelease(ev);
                    }
                }
            },
        }
    }

    fn click_wrapped(down: CGEventType, up: CGEventType, point: CGPoint, button: CGMouseButton) {
        for kind in [down, up] {
            if let Some(src) = cg_source() {
                if let Ok(ev) = CGEvent::new_mouse_event(src, kind, point, button) {
                    ev.post(CGEventTapLocation::HID);
                }
            }
        }
    }

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGEventCreateMouseEvent(
            source: *mut c_void,
            mouse_type: u32,
            mouse_cursor_position: CGPoint,
            mouse_button: u32,
        ) -> *mut c_void;
        fn CGEventPost(tap: u32, event: *mut c_void);
        fn CFRelease(cf: *mut c_void);
    }

    /// Real system media behavior (volume HUD etc.): down+up NX events.
    /// data1 = (keyCode << 16) | ((0x0A down / 0x0B up) << 8).
    /// NX subtype for auxiliary control buttons is 8.
    pub fn post_aux_nx(key: u8) {
        for down in [true, false] {
            let dir: NSInteger = if down { 0x0A } else { 0x0B };
            let data1: NSInteger = ((key as NSInteger) << 16) | (dir << 8);
            let event =
                NSEvent::otherEventWithType_location_modifierFlags_timestamp_windowNumber_context_subtype_data1_data2(
                    NSEventType::SystemDefined,
                    NSPoint { x: 0.0, y: 0.0 },
                    NSEventModifierFlags(0),
                    0.0 as NSTimeInterval,
                    0,
                    None,
                    8,
                    data1,
                    -1,
                );
            if let Some(event) = event {
                if let Some(cg) = event.CGEvent() {
                    CGEvent2::post(TapLoc::HIDEventTap, Some(&cg));
                }
            }
        }
    }

    /// Launch via Launch Services (`open -b`): identical behavior to the
    /// block-based NSWorkspace API without its completion-handler plumbing.
    pub fn ws_launch(bundle_id: &str) -> bool {
        std::process::Command::new("open")
            .args(["-b", bundle_id])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    pub fn ws_open_url(url: &str) -> bool {
        use objc2_app_kit::NSWorkspace;
        match NSURL::URLWithString(&NSString::from_str(url)) {
            Some(nsurl) => NSWorkspace::sharedWorkspace().openURL(&nsurl),
            None => false,
        }
    }

    pub fn ws_open_path(path: &str) -> bool {
        use objc2_app_kit::NSWorkspace;
        let nsurl = NSURL::fileURLWithPath(&NSString::from_str(path));
        NSWorkspace::sharedWorkspace().openURL(&nsurl)
    }

    /// Terminate every running instance with the bundle id. Returns the
    /// instance count so the log is honest about no-ops.
    pub fn ws_quit(bundle_id: &str, force: bool) -> usize {
        let apps: Retained<NSArray<NSRunningApplication>> =
            NSRunningApplication::runningApplicationsWithBundleIdentifier(&NSString::from_str(
                bundle_id,
            ));
        let mut n = 0usize;
        for i in 0..apps.count() {
            let app = apps.objectAtIndex(i);
            if force {
                app.forceTerminate();
            } else {
                app.terminate();
            }
            n += 1;
        }
        n
    }
}
