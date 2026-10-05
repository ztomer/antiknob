use serde_json::Value;

use super::decoder::hex;

// ── the capture fixture ─────────────────────────────────────────────────────

/// One entry of `tests/fixtures/vendor_capture.json`.
///
/// Read with `serde_json` rather than deserialized into a typed struct on
/// purpose: a struct would be a SECOND declaration of this schema, and the
/// fixture exists so there is one description of each capture. Every accessor
/// unwraps, because a renamed field must not read as an absent capture — the
/// silent way to make this oracle vacuous.
#[derive(Debug, Clone)]
pub struct Capture {
    pub id: String,
    /// `"frame"` for a whole report, `"entries"` for a captured run of
    /// entries with no header in front of them. The two are not the same
    /// shape and must not be decoded as each other.
    pub kind: String,
    pub doc_line: usize,
    pub hex: String,
    pub establishes: String,
    pub bytes: Vec<u8>,
}

/// The path to the committed capture.
pub const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/vendor_capture.json"
);

/// The document the capture was transcribed from.
pub const CAPTURE_LOG: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/VENDOR_UI_MAP.md");

/// Every captured frame.
pub fn fixture() -> Vec<Capture> {
    let text = std::fs::read_to_string(FIXTURE)
        .unwrap_or_else(|e| panic!("tests/fixtures/vendor_capture.json is missing: {e}"));
    let json: Value = serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("the capture fixture does not parse: {e}"));
    let list = json["captures"]
        .as_array()
        .unwrap_or_else(|| panic!("the fixture has no `captures` array"));
    assert!(
        list.len() >= 20,
        "the capture fixture holds {} frames; it is the oracle for every hardcoded byte \
         assertion in this crate and shrinking it is a regression",
        list.len()
    );
    list.iter()
        .map(|c| Capture {
            id: c["id"]
                .as_str()
                .unwrap_or_else(|| panic!("a capture has no id: {c}"))
                .to_string(),
            kind: c["kind"]
                .as_str()
                .unwrap_or_else(|| panic!("{} has no kind", c["id"]))
                .to_string(),
            doc_line: c["doc_line"].as_u64().expect("doc_line") as usize,
            hex: c["hex"].as_str().expect("hex").to_string(),
            establishes: c["establishes"].as_str().expect("establishes").to_string(),
            bytes: hex(c["hex"].as_str().expect("hex")),
        })
        .collect()
}

/// One capture by id.
pub fn by_id<'a>(all: &'a [Capture], id: &str) -> &'a Capture {
    all.iter()
        .find(|c| c.id == id)
        .unwrap_or_else(|| panic!("no capture with id {id:?}"))
}
