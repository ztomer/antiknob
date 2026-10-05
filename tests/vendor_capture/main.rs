//! The byte goldens, checked against a capture instead of against a comment.
//!
//! One test crate, four modules, because the two halves share a decoder and a
//! fixture loader: as separate integration-test files each would compile its
//! own copy, and every item only one of them used would be dead code in the
//! other. One crate, one copy, no suppressions.
//!
//! * `decoder` — reads a captured frame per the documented wire layout. The
//!   crate could BUILD every frame it sends but could not READ one back, so
//!   the capture log had no code interpreting it.
//! * `fixture` — the committed capture, `tests/fixtures/vendor_capture.json`.
//! * `oracle` — the capture is real, is still in the document, and this repo's
//!   own producers re-derive it byte for byte.
//! * `claims` — each frame DECODES, and what it decodes to is what the
//!   document claims about it.
//!
//! ## What this replaces
//!
//! This repo's hardcoded byte assertions are real, and most of them carry real
//! provenance — a VK01 dump, a vendor-app capture, an upstream issue, the USB
//! HID Usage Tables. That provenance lived in COMMENTS and in
//! `VENDOR_UI_MAP.md`, and **no test read either**. So the suite was
//! indistinguishable from a snapshot while looking oracle-backed: a reviewer
//! reading `assert_eq!(p[4], 5)` could not tell a measured value from a
//! remembered one, and neither could the test.
//!
//! ## What could not be done, stated plainly
//!
//! **No new capture was taken, and none was invented.** The knob
//! (`514c:8850`) was not attached when this was written: `ioreg` and
//! `system_profiler` list only the 2.4G receiver (`25a7:fa11`), and
//! `antiknob raw` refuses outright in that state —
//!
//! ```text
//! Error: Device is connected wirelessly. Hardware configuration (flashing
//! slot bindings and LED modes) requires a direct USB-C wired connection.
//! ```
//!
//! `tools/hidsnoop/` needs the same absent device (it wraps the vendor app
//! while that app drives the knob), so it could not produce a fixture either.
//! Every frame is transcribed from the capture log already committed in
//! `VENDOR_UI_MAP.md`, which is a weaker oracle than a fresh capture and is
//! described as such in the fixture's own header.
//!
//! **What still rests on a comment**, after this change — the full list is in
//! `oracle.rs`:
//!
//! * `VENDOR_UI_MAP.md:137`'s elided frame, written with `...` in the source,
//!   so there is no complete frame to decode.
//! * Gesture-to-key-id for the **0x1189 CH57x** devices: the EXPERIMENT 1
//!   capture is a VK01, and the mapping is applied across every VID/PID in
//!   `SUPPORTED_DEVICES`.
//! * **Byte 5 of a chained record** — open in the document, unresolved here.
//! * **Mode 3's behaviour** (`ripple`), taken from an upstream issue rather
//!   than watched on this hardware.
//! * **A frame added to the document but not to the fixture.**

mod claims;
mod decoder;
mod fixture;
mod oracle;
