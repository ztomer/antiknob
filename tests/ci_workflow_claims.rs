//! AK.CIDOC: tracked text about CI must not claim the workflow never ran.
#[test]
fn workflow_header_does_not_claim_it_never_ran() {
    let text = include_str!("../.github/workflows/tests.yml").to_lowercase();
    for stale in [
        "never run",
        "never ran",
        "has not been observed",
        "unverified",
    ] {
        assert!(!text.contains(stale), "tests.yml still claims {stale:?}");
    }
}
