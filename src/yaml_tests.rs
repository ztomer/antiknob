//! Tests for `yaml`: the behaviours `serde_yaml_ng` had for this crate's
//! types, pinned so the parser swap cannot drift them.

use super::*;
use serde::Deserialize;

#[derive(Debug, Deserialize, PartialEq)]
struct Doc {
    #[serde(default)]
    names: Vec<String>,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    count: usize,
}

#[test]
fn a_scalar_read_as_a_string_is_its_text() {
    let d: Doc = from_str("names: [1, true, 2.50, null, yes, 0x1F]\nlabel: 0\n").unwrap();
    assert_eq!(d.names, ["1", "true", "2.50", "null", "yes", "0x1F"]);
    assert_eq!(d.label.as_deref(), Some("0"));
}

#[test]
fn a_scalar_read_as_a_number_resolves_and_a_quoted_one_does_not() {
    let d: Doc = from_str("count: 3\n").unwrap();
    assert_eq!(d.count, 3);
    let err = from_str::<Doc>("count: '3'\n").unwrap_err().to_string();
    assert!(err.contains("invalid type: string"), "{err}");
    assert!(err.contains("line 1 column 8"), "{err}");
}

#[test]
fn null_and_empty_are_none() {
    let d: Doc = from_str("label: null\n").unwrap();
    assert_eq!(d.label, None);
    let d: Doc = from_str("label:\n").unwrap();
    assert_eq!(d.label, None);
    let d: Doc = from_str("label: ''\n").unwrap();
    assert_eq!(d.label.as_deref(), Some(""));
}

#[test]
fn a_duplicate_key_is_refused() {
    let err = from_str::<Doc>("count: 1\ncount: 2\n")
        .unwrap_err()
        .to_string();
    assert!(err.contains("duplicated key"), "{err}");
}

#[test]
fn a_second_document_is_refused() {
    let err = from_str::<Doc>("count: 1\n---\ncount: 2\n")
        .unwrap_err()
        .to_string();
    assert!(err.contains("2 documents"), "{err}");
}

#[test]
fn malformed_yaml_names_its_line() {
    let err = from_str::<Doc>("names: [a, b\ncount: 1\n")
        .unwrap_err()
        .to_string();
    assert!(err.starts_with("YAML: "), "{err}");
    assert!(err.contains("line "), "{err}");
}

#[test]
fn aliases_resolve_and_a_merge_key_is_an_ordinary_key() {
    let d: Doc = from_str("base: &n [a, b]\nnames: *n\n").unwrap();
    assert_eq!(d.names, ["a", "b"]);
    // serde_yaml_ng's typed path never applied `<<`; nor does this.
    let d: Doc = from_str("b: &b {count: 3}\n<<: *b\n").unwrap();
    assert_eq!(d.count, 0);
}

#[test]
fn a_type_error_names_the_innermost_node() {
    let err = from_str::<Doc>("count: 1\nnames:\n  - a\n  - [b]\n")
        .unwrap_err()
        .to_string();
    assert!(err.contains("line 4 column 5"), "{err}");
}

#[derive(Debug, Deserialize, PartialEq)]
enum Mode {
    Plain,
    Sized(u8),
}

#[test]
fn an_enum_is_a_name_or_a_one_key_mapping() {
    assert_eq!(from_str::<Mode>("Plain").unwrap(), Mode::Plain);
    assert_eq!(from_str::<Mode>("Sized: 4").unwrap(), Mode::Sized(4));
    assert!(from_str::<Mode>("{Sized: 4, Plain: ~}").is_err());
}

#[test]
fn an_empty_document_is_null() {
    assert_eq!(from_str::<Option<Doc>>("").unwrap(), None);
}
