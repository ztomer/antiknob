//! Asking macOS which TCC grants this process actually holds.
//!
//! Only the calls live here; `host::grants` turns their answer into the line
//! the user reads, and is unit-tested there. Untestable headless (every call
//! is about this live process's TCC state), so this file stays as small as
//! the question it asks.

use antiknob::host::grants::{grant_item, tap_failure_line, Grants};
use std::path::PathBuf;

/// Ask macOS what this process is allowed to do, then say it in one line.
pub fn tap_failure_reason() -> String {
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("antiknob-daemon"));
    tap_failure_line(current_grants(), &grant_item(&exe))
}

#[cfg(target_os = "macos")]
pub use mac::{current_grants, open_accessibility_pane, prompt_for_accessibility_once};

/// Non-macOS builds have no TCC, and no event tap for it to gate.
#[cfg(not(target_os = "macos"))]
pub fn current_grants() -> Grants {
    Grants {
        accessibility: true,
        input_monitoring: true,
    }
}

#[cfg(not(target_os = "macos"))]
pub fn prompt_for_accessibility_once() {}

#[cfg(not(target_os = "macos"))]
pub fn open_accessibility_pane() {}

#[cfg(target_os = "macos")]
mod mac {
    use super::Grants;
    use core_foundation::base::{Boolean, TCFType};
    use core_foundation::boolean::CFBoolean;
    use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
    use core_foundation::string::{CFString, CFStringRef};
    use std::sync::Once;

    // IOHIDRequestType (IOKit/hidsystem/IOHIDLib.h): PostEvent is 0 and
    // ListenEvent is 1 -- read off the SDK header, not remembered.
    const IOHID_REQUEST_LISTEN_EVENT: u32 = 1;
    // IOHIDAccessType: Granted = 0, Denied = 1, Unknown = 2.
    const IOHID_ACCESS_GRANTED: u32 = 0;

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXIsProcessTrusted() -> Boolean;
        fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> Boolean;
        static kAXTrustedCheckOptionPrompt: CFStringRef;
    }

    #[link(name = "IOKit", kind = "framework")]
    extern "C" {
        fn IOHIDCheckAccess(request_type: u32) -> u32;
    }

    /// Both calls only read TCC state; neither prompts.
    pub fn current_grants() -> Grants {
        Grants {
            accessibility: unsafe { AXIsProcessTrusted() != 0 },
            input_monitoring: unsafe { IOHIDCheckAccess(IOHID_REQUEST_LISTEN_EVENT) }
                == IOHID_ACCESS_GRANTED,
        }
    }

    /// Let macOS ask for the grant itself, at most once per daemon run.
    ///
    /// This is the difference between an instruction and a switch: the
    /// system alert adds the daemon to the Accessibility list and opens the
    /// pane on it, so the user has something to toggle rather than a path to
    /// go and find. Once, because the tap supervisor retries every three
    /// seconds and an alert per retry is not help.
    pub fn prompt_for_accessibility_once() {
        static ONCE: Once = Once::new();
        ONCE.call_once(|| {
            let key = unsafe { CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt) };
            let options = CFDictionary::from_CFType_pairs(&[(
                key.as_CFType(),
                CFBoolean::true_value().as_CFType(),
            )]);
            unsafe {
                AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef());
            }
        });
    }

    /// Open the Accessibility pane directly (the menu bar's way out).
    pub fn open_accessibility_pane() {
        let _ = std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
            .status();
    }
}
