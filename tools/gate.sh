#!/usr/bin/env bash
# Per-repo gate entry point. Declares which toolchains this repo contains and
# delegates; it holds no gate logic of its own.
#   --staged : pre-commit scope (fast) — layer 1 only
#   --full   : pre-push scope — every layer
set -euo pipefail
GOH="${GOH_DIR:-${GOH:-$HOME/Projects/gates_of_heck}}"

# Informational flags never reach the gates below.
case "${1:-}" in
  -h|--help)
    echo "usage: gate.sh [--staged | --full | --doctor [repo] | --help]"
    exit 0 ;;
  --doctor)  exec bash "$GOH/gates/doctor.sh" "${2:-$PWD}" ;;
esac

# Cargo.toml's `goh-tui` path dependency names ../gates_of_heck, and Cargo reads
# no environment variable in a path. A checkout that has no such sibling -- the
# pre-push export (<export root>/antiknob-<hash>/antiknob) and CI (the workspace
# is the checkout, goh sits inside it) -- gets one here, as a link to $GOH, so
# the local gate, the push gate and CI resolve the crate the same way. Absent
# only; a real sibling is never touched.
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SIBLING="$(dirname "$ROOT")/gates_of_heck"
if [ ! -e "$SIBLING" ] && [ ! -L "$SIBLING" ]; then
  ln -s "$(cd "$GOH" && pwd)" "$SIBLING"
fi

"$GOH/gates/structural.sh" "$@"

case "${1:-}" in
  --full)
    # Add per-language layers for what this repo actually contains:
    #
    # The lockfile is an INPUT to the gate, never an OUTPUT. rust_gate.sh
    # (gates_of_heck >= v0.20.0) resolves `cargo metadata --locked` first,
    # passes --locked to every resolving cargo call and fails if any step
    # rewrote Cargo.lock; our own steps pass --locked themselves. That made
    # this repo's tools/lock_guard.sh redundant, and it was retired
    # 2026-10-06 after the upstream gate was shown refusing a lock-changing
    # manifest edit with the lockfile left untouched.
    "$GOH/gates/rust_gate.sh"  .
    #   "$GOH/gates/py_gate.sh"    .
    # The Swift half is its own SPM package under ui/, with its own .gatesrc.
    "$GOH/gates/swift_gate.sh" ./ui
    # Layer 3: genuinely local checks (cargo test + cargo audit).
    ./tools/repo_gates.sh
    ;;
esac
