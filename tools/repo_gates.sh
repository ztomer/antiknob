#!/usr/bin/env bash
# Layer 3: the checks that are genuinely local to this repo.
#
#   1. cargo test -- the house rust gate runs fmt/clippy/lints but NOT the
#      test suite. That hole is why an intermittent SIGTRAP in the daemon's
#      socket path (hidapi off its thread) survived several green pushes:
#      `./tools/gate.sh --full` reported success while `cargo test` crashed
#      about one run in four. Tests belong in the gate, not in a habit.
#   2. cargo audit -- house Rust rule. `audit.toml` carries the deny list
#      that makes it fail on unmaintained/unsound, plus the ignore ratchet.
#
# BOTH cargo steps are pinned, differently, because they are different kinds
# of command. `cargo test` gets `--locked`, so cargo fails with its own
# message naming `Cargo.lock` rather than re-resolving and rewriting it --
# which is how a manifest-only dependency edit reaches a green gate.
#
# `cargo audit` gets NO such flag, and that is measured rather than assumed:
# `cargo audit --locked` exits non-zero with "error: unexpected argument
# '--locked' found", and its only path option is `-f/--file`, whose default
# is already `Cargo.lock`. It reads a lockfile and never resolves one, so
# there is nothing for it to launder. `tools/gate.sh` wraps this whole script
# in `lock_guard.sh`, which hashes the lockfile either way -- so even a step
# that omitted the flag cannot leave a rewritten `Cargo.lock` behind.
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

info "test suite (all targets, all features, --locked)"
if cargo test --all-targets --all-features --locked --quiet; then
    ok "test suite"
else
    die "test suite failed"
fi

info "dependency audit"
if ! command -v cargo-audit >/dev/null 2>&1; then
    die "cargo-audit not installed: cargo install cargo-audit"
fi
if cargo audit; then
    ok "dependency audit"
else
    die "dependency audit failed (see audit.toml for the ignore ratchet)"
fi

ok "all repo gates passed"
