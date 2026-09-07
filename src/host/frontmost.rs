//! Which application is in front, behind a seam.
//!
//! One call, one trait, one fake. The point is not that the OS call is
//! complicated -- it is four lines of AppKit -- but that everything which
//! *depends* on it becomes untestable the moment it is called directly.
//! `virtual_layer` decides which bindings are live from the frontmost app;
//! that decision has to be exercised against a browser, an editor and an
//! unknown app without any of them being open.

use std::sync::{Arc, Mutex};

/// Anything that can say what is in front.
pub trait FrontmostApp: Send + Sync {
    /// Bundle id of the frontmost application, or `None` when it cannot be
    /// read. `None` is a real answer, not an error: no app is frontmost
    /// during login, and the variant resolver treats it as "no match".
    fn bundle_id(&self) -> Option<String>;
}

/// A fixed answer, for tests and for builds with no window server.
pub struct FixedApp(pub Option<String>);

impl FrontmostApp for FixedApp {
    fn bundle_id(&self) -> Option<String> {
        self.0.clone()
    }
}

/// A settable answer, so a test can switch apps mid-scenario.
#[derive(Default)]
pub struct FakeApp(Mutex<Option<String>>);

impl FakeApp {
    pub fn new(bundle_id: Option<&str>) -> Self {
        Self(Mutex::new(bundle_id.map(str::to_string)))
    }

    /// Switch to another app, as the user would.
    pub fn switch_to(&self, bundle_id: Option<&str>) {
        if let Ok(mut g) = self.0.lock() {
            *g = bundle_id.map(str::to_string);
        }
    }
}

impl FrontmostApp for FakeApp {
    fn bundle_id(&self) -> Option<String> {
        self.0.lock().ok().and_then(|g| g.clone())
    }
}

/// The real source for this platform.
pub fn system_source() -> Arc<dyn FrontmostApp> {
    #[cfg(target_os = "macos")]
    {
        Arc::new(mac::WorkspaceApp)
    }
    #[cfg(not(target_os = "macos"))]
    {
        Arc::new(FixedApp(None))
    }
}

#[cfg(target_os = "macos")]
mod mac {
    use super::FrontmostApp;
    use objc2_app_kit::NSWorkspace;

    pub struct WorkspaceApp;

    impl FrontmostApp for WorkspaceApp {
        /// Read at the moment it is needed, never cached. The frontmost app
        /// when a gesture completes is not necessarily the one the user was
        /// looking at when they turned the knob, and a stale answer would
        /// silently run the wrong bindings.
        fn bundle_id(&self) -> Option<String> {
            let app = NSWorkspace::sharedWorkspace().frontmostApplication()?;
            app.bundleIdentifier().map(|s| s.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fixed_source_answers_what_it_was_given() {
        assert_eq!(
            FixedApp(Some("com.apple.Safari".into()))
                .bundle_id()
                .as_deref(),
            Some("com.apple.Safari")
        );
        assert_eq!(FixedApp(None).bundle_id(), None);
    }

    #[test]
    fn a_fake_source_can_switch_apps_mid_scenario() {
        let fake = FakeApp::new(Some("com.apple.Safari"));
        assert_eq!(fake.bundle_id().as_deref(), Some("com.apple.Safari"));
        fake.switch_to(Some("com.microsoft.VSCode"));
        assert_eq!(fake.bundle_id().as_deref(), Some("com.microsoft.VSCode"));
        // "Nothing is frontmost" is a state the resolver has to handle.
        fake.switch_to(None);
        assert_eq!(fake.bundle_id(), None);
    }

    /// The seam has to be usable as a trait object from the daemon, which
    /// holds it behind an Arc across threads.
    #[test]
    fn the_seam_is_object_safe_and_shareable() {
        let src: Arc<dyn FrontmostApp> = Arc::new(FakeApp::new(Some("com.apple.Finder")));
        let clone = Arc::clone(&src);
        std::thread::spawn(move || clone.bundle_id())
            .join()
            .expect("a source must be usable from another thread");
        assert_eq!(src.bundle_id().as_deref(), Some("com.apple.Finder"));
    }
}
