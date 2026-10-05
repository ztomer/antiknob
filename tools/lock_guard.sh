#!/usr/bin/env bash
# lock_guard.sh — the lockfile is an INPUT to the gate, never an OUTPUT.
#
# Without `--locked`, cargo re-resolves and REWRITES `Cargo.lock` when a
# manifest and the lockfile disagree, then builds and tests happily. A gate
# that lets that happen is laundering a dependency change: the manifest-only
# edit passes green, and the lockfile diff is the only trace of what actually
# moved. This is not hypothetical — in the sibling repo `divoom-control`,
# `reqwest` sat pinned for MONTHS because 0.13.2 deleted an optional
# dependency behind a feature the repo declared. `cargo update` could not
# move it, printed `Unchanged reqwest v0.13.1 (available: v0.13.5)` in a
# parenthetical, and exited 0. No gate printed that line.
#
# So the gate is pinned from both ends:
#
#   1. `--locked` on every cargo invocation this repo owns, so cargo fails
#      with its OWN message naming the lockfile instead of quietly repairing
#      it. (`cargo audit` is the exception and takes no such flag — it reads
#      a lockfile path and never re-resolves — so it is pinned by path.)
#
#   2. This guard, which wraps the cargo steps it does NOT own, including
#      the shared `gates/rust_gate.sh` in gates_of_heck. That script is a
#      different repo and out of this repo's hands; step 1 alone would leave
#      `cargo fmt`/`clippy` free to re-resolve behind it. Hashing the
#      lockfile before and after closes the class rather than the instance:
#      no cargo invocation anywhere in the gate can leave a rewritten
#      `Cargo.lock` behind, whether or not it passes `--locked`.
#
# Usage:
#   tools/lock_guard.sh                       # verify only
#   tools/lock_guard.sh <command> [args...]   # verify, run the command, verify again
#
# Exit codes: 0 clean, 1 the lockfile was stale or was rewritten, 2 misuse.
set -euo pipefail

GOH="${GOH_DIR:-${GOH:-$HOME/Projects/gates_of_heck}}"
# shellcheck source=/dev/null
. "$GOH/tui/lib.sh"

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

LOCKFILE="Cargo.lock"

if [ ! -f "$LOCKFILE" ]; then
    die "$LOCKFILE is missing; a Rust gate cannot run without it"
fi

# Content hash, not mtime: a gate cares whether the DEPENDENCY GRAPH moved,
# and a rewrite that produced identical bytes did not move it. Missing
# `shasum` is a hard failure rather than a fallback — a guard that quietly
# degrades to "probably fine" is the defect it exists to catch.
command -v shasum >/dev/null 2>&1 || die "shasum not found; cannot fingerprint $LOCKFILE"

fingerprint() {
    shasum -a 256 "$LOCKFILE" | cut -d' ' -f1
}

# The lockfile must already satisfy every manifest, checked WITHOUT writing.
# `cargo metadata --locked` is the cheapest command that reads the whole
# dependency graph, so it fails on a stale lockfile that `cargo build` would
# otherwise repair on the way past.
#
# Cargo's own stderr is printed, not swallowed: it names the lockfile and the
# package that disagrees, which is the message that makes this actionable. A
# guard that reports "the lockfile is stale" and hides why is the same
# uninformative pass/fail it replaced.
assert_in_sync() {
    local log
    log="$(mktemp -t antiknob-lock.XXXXXX)"
    if cargo metadata --locked --format-version 1 --all-features >/dev/null 2>"$log"; then
        rm -f "$log"
        return 0
    fi
    err "[lock] $LOCKFILE does not satisfy the manifests."
    info "cargo says:"
    while IFS= read -r line; do
        info "  $line"
    done <"$log"
    rm -f "$log"
    info "A dependency change is a DELIBERATE act: run"
    info "  cargo update -p <package>"
    info "or 'cargo update' for a full refresh, then commit $LOCKFILE WITH the"
    info "manifest edit. A gate must never make that decision for you."
    return 1
}

assert_unchanged() {
    local before="$1" after
    after="$(fingerprint)"
    if [ "$after" != "$before" ]; then
        err "[lock] $LOCKFILE was REWRITTEN by a cargo step during this gate."
        info "before: $before"
        info "after : $after"
        info "  git diff -- $LOCKFILE"
        info "That step ran cargo without --locked. It is now a dependency change"
        info "smuggled past a green gate; restore the file and fix the step."
        return 1
    fi
    return 0
}

section "lockfile"

before="$(fingerprint)"
info "$LOCKFILE $(printf '%s' "$before" | cut -c1-12)"

assert_in_sync || die "$LOCKFILE is stale relative to the manifests"

if [ "$#" -gt 0 ]; then
    info "guarded: $*"
    "$@"
    assert_unchanged "$before" || die "$LOCKFILE was rewritten during this gate"
    # A command that repaired the lockfile and then exited 0 has already
    # rewritten it, so the hash catches it — but re-checking sync catches the
    # case where cargo rewrote the file back to its original contents, which
    # is still a gate that decided a dependency question on its own.
    assert_in_sync || die "$LOCKFILE is no longer in sync after: $*"
fi

ok "$LOCKFILE is in sync and unchanged"
