//! Tests for `hold_stats`: the line format, the bound, and the summary.

use super::*;

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("antiknob-hold-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("hold-gaps.log")
}

#[test]
fn a_sample_round_trips_through_its_line() {
    for s in [
        HoldSample::Gap {
            slot: 3,
            gap_us: 41_977,
        },
        HoldSample::Expired,
    ] {
        assert_eq!(HoldSample::from_line(&s.to_line()), Some(s));
    }
}

#[test]
fn a_line_that_is_not_ours_is_ignored() {
    for junk in ["", "hello", "G 1", "G a b", "X 1", "G 1 2 3", "G -1 5"] {
        assert_eq!(HoldSample::from_line(junk), None, "{junk:?}");
    }
}

/// Nothing typed can reach the file: the only vocabulary a line can hold is
/// a letter, a slot ordinal and a number.
#[test]
fn a_line_carries_only_a_slot_ordinal_and_a_gap() {
    let line = HoldSample::Gap {
        slot: 4,
        gap_us: 812,
    }
    .to_line();
    assert_eq!(line, "G 4 812\n");
}

#[test]
fn the_summary_is_nearest_rank_over_the_gaps() {
    let mut text = String::new();
    for us in 1..=100u64 {
        text.push_str(
            &HoldSample::Gap {
                slot: 0,
                gap_us: us,
            }
            .to_line(),
        );
    }
    text.push_str("X\nX\nnoise\n");
    let s = summarize(&text);
    assert_eq!(
        s,
        Summary {
            count: 100,
            p50: Some(50),
            p95: Some(95),
            p99: Some(99),
            max: Some(100),
            expired: 2,
        }
    );
}

#[test]
fn an_empty_file_summarises_to_nothing_not_to_zero() {
    let s = summarize("");
    assert_eq!(s, Summary::default());
    assert_eq!(s.p50, None);
    assert!(render(&s).contains("p50             -"));
}

#[test]
fn a_single_gap_is_every_percentile() {
    let s = summarize("G 1 700\n");
    assert_eq!(
        (s.p50, s.p95, s.p99, s.max),
        (Some(700), Some(700), Some(700), Some(700))
    );
}

#[test]
fn render_names_the_hold_the_numbers_are_read_against() {
    let out = render(&summarize("G 0 1500\nX\n"));
    assert!(out.contains("slot chords       1"), "{out}");
    assert!(out.contains("1500 us"), "{out}");
    assert!(out.contains("expired holds     1"), "{out}");
    assert!(
        out.contains(&format!("{} ms", crate::policy::MOD_HOLD_MS)),
        "{out}"
    );
}

#[test]
fn append_creates_a_private_file_and_adds_lines() {
    use std::os::unix::fs::PermissionsExt as _;
    let path = scratch("create");
    let _ = std::fs::remove_file(&path);
    append(&path, HoldSample::Expired, 4096).unwrap();
    append(&path, HoldSample::Gap { slot: 1, gap_us: 9 }, 4096).unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "X\nG 1 9\n");
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}

/// The bound is the point: however many samples arrive, the file stays
/// under the cap, keeps the NEWEST records, and stays whole lines.
#[test]
fn the_file_never_passes_its_cap_and_keeps_the_newest_records() {
    let path = scratch("cap");
    let _ = std::fs::remove_file(&path);
    let cap = 200u64;
    for us in 0..500u64 {
        append(
            &path,
            HoldSample::Gap {
                slot: 0,
                gap_us: us,
            },
            cap,
        )
        .unwrap();
        assert!(std::fs::metadata(&path).unwrap().len() <= cap, "after {us}");
    }
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.ends_with("G 0 499\n"), "{text:?}");
    assert!(
        text.lines().all(|l| HoldSample::from_line(l).is_some()),
        "{text:?}"
    );
    assert!(summarize(&text).count > 5);
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}
