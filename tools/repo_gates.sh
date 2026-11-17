#!/usr/bin/env bash
# Layer 3: the checks that are genuinely local to this repo.
#
#   1. cargo test -- the house rust gate runs fmt/clippy/lints but NOT the
#      test suite. That hole is why an intermittent SIGTRAP in the daemon's
#      socket path (hidapi off its thread) survived several green pushes:
#      `./tools/gate.sh --full` reported success while `cargo test` crashed
#      about one run in four. Tests belong in the gate, not in a habit.
#
# NO `cargo audit` here (GOH.AUDITONCE, 2026-11-17): goh's rust_gate.sh
# (layer 1, tools/gate.sh) already audits this repo's Cargo.lock through
# goh's advisory-db wrapper, with the deny list on its command line, this
# repo's `.cargo/audit.toml` ignore ratchet, and goh's shared advisory
# rulings. A second, raw audit here judged the same lock without the shared
# rulings and refused the v0.130.0 pin push on RUSTSEC-2026-0543 (chrono via
# rmcp), which the gate's own audit had passed.
#
# `cargo test` gets `--locked`, so cargo fails with its own message naming
# `Cargo.lock` rather than re-resolving and rewriting it -- which is how a
# manifest-only dependency edit reaches a green gate.
set -euo pipefail

GOH="${GOH_DIR:-${GOH:-$HOME/Projects/gates_of_heck}}"
# shellcheck source=/dev/null
. "$GOH/tui/lib.sh"

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

section "repo gates"

info "Swift and the daemon agree on the host.json model"
# A PYTHON gate, and the first one this repo had. The class is real and
# measured: the app's `Action`/`LayerConfig` and the daemon's
# `HostAction`/`HostLayer` are two implementations of one JSON schema in two
# languages, and they had drifted in four places — two of which made a
# GUI-written host.json undecodable, so `load_json` silently replaced the
# user's configuration with the defaults.
#
# It lives in `tools/` and runs from here rather than from the shared
# `structural.sh` because it knows about THIS repo's two model files. Its own
# calibration is `tests/swift_rust_parity_gate_calibration.rs`, which
# re-introduces each drift and requires this script to go red and say why.
if python3 tools/check_swift_rust_model_parity.py; then
    ok "Swift/Rust model parity"
else
    die "the Swift app and the daemon disagree about host.json (the daemon is ground truth)"
fi

info "status lines print through the house TUI"
# No seed: antiknob's 113 hand-rolled markers were all converted (S.1), so a
# reintroduced `[ ==> ]` / `[ Ok  ]` or a raw colour escape is a finding, not a
# ratchet entry. The scan judges its own floor (GOH_STATUS_MARKERS_MIN_FILES).
if "$GOH/gates/goh.sh" status-markers; then
    ok "no hand-rolled status markers"
else
    die "a status line bypasses the house TUI: print through tui/lib.sh or goh_tui (Rust)"
fi

info "test suite (all targets, all features, --locked)"
if cargo test --all-targets --all-features --locked --quiet; then
    ok "test suite"
else
    die "test suite failed"
fi

ok "all repo gates passed"
