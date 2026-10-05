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

"$GOH/gates/structural.sh" "$@"

# The lockfile guard, resolved before any cargo step runs.
LOCK_GUARD="$PWD/tools/lock_guard.sh"

case "${1:-}" in
  --full)
    # Add per-language layers for what this repo actually contains:
    #
    # Every cargo step below runs INSIDE the lock guard, including the two
    # that live in gates_of_heck and are therefore not this repo's to edit.
    # `--locked` on our own steps (tools/repo_gates.sh, install.sh) makes
    # cargo name the lockfile itself; the guard catches the ones we cannot
    # pass it to, so a re-resolve anywhere in the gate is a red gate rather
    # than a lockfile diff nobody reads.
    "$LOCK_GUARD" "$GOH/gates/rust_gate.sh"  .
    #   "$GOH/gates/py_gate.sh"    .
    # The Swift half is its own SPM package under ui/, with its own .gatesrc.
    "$GOH/gates/swift_gate.sh" ./ui
    # Layer 3: genuinely local checks (cargo test + cargo audit).
    "$LOCK_GUARD" ./tools/repo_gates.sh
    ;;
esac
