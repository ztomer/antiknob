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
set -euo pipefail

GOH="${GOH_DIR:-${GOH:-$HOME/Projects/gates_of_heck}}"
# shellcheck source=/dev/null
. "$GOH/tui/lib.sh"

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

section "repo gates"

info "test suite (all targets, all features)"
if cargo test --all-targets --all-features --quiet; then
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
