#!/usr/bin/env python3
"""Fail if any git-tracked Rust source contains an `#[allow(...)]` attribute.

Policy (user, 2026-08-02): clippy runs at `-D warnings` everywhere, and warnings
are FIXED, not silenced. An `#[allow(...)]` suppresses the lint so it never
fires, which defeats the gate — so any `#[allow]`/`#![allow]` in source is a
failure. A file is exempt only if it is machine-generated and carries the
standard `@generated` marker (see below); hand-written code never is.

    python3 tools/check_no_allow.py            # all tracked Rust files (CI gate)
    python3 tools/check_no_allow.py --staged   # only staged files (pre-commit)

Exemption: a Rust file is treated as generated iff it contains `@generated`
within its first 40 lines of comments (the de-facto marker used by prost/tonic,
bindgen and friends). Nothing else exempts a file. This is deterministic and
auditable, the same way check_no_emoji.py's codepoint ranges are.

The checker greps for the literal tokens `#[allow(` and `#![allow(` — never a
looser regex — so `#[expect(...)]` (which ERRORS if its lint never fires, the
"allow that cannot rot") stays permitted.

Conservative comment-awareness (2026-08-25, depth-counter rewrite 2026-08-26):
a doc-comment MENTIONING #[allow] is prose about the policy, not a
suppression, and a gate that flags its own documentation trains people to
ignore it. Whole-line // comments (covering /// doc comments and //! inner
docs), // tails, and /* */ spans are stripped by a left-to-right depth
counter that tracks OPENERS and CLOSERS per line and carries nesting state
across lines — so a second `/*` on one line re-enters comment state (no false
positive on comment-only mentions) and a real attribute after a same-line
`*/` close is still searched (no blind spot). Unterminated blocks run safe to
EOF: everything after stays comment.
"""
import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from _gitutil import content_bytes, listed_files, repo_root  # noqa: E402

_GENERATED_MARKER = "@generated"
_ALLOW_PATTERN = re.compile(r"#!?\[allow\(")


def _code_portions(text: str):
    """Yield (lineno, code-only line): /* */ spans (nestable, left-to-right,
    state carried across lines) and // tails removed."""
    depth = 0
    for lineno, line in enumerate(text.split("\n"), 1):
        kept = []
        i = 0
        while i < len(line):
            if depth == 0 and line.startswith("//", i):
                break  # line comment: rest of the line is prose
            if line.startswith("/*", i):
                depth += 1
                i += 2
                continue
            if depth > 0 and line.startswith("*/", i):
                depth -= 1
                i += 2
                continue
            if depth == 0:
                kept.append(line[i])
            i += 1
        yield lineno, "".join(kept)


def _is_compiled_src(rel: str) -> bool:
    # Gate is scoped to compiled non-test code: any crate's src/, benches/,
    # and build.rs. Tests may legitimately unwrap and are excluded here
    # (clippy -D warnings still runs over them via --all-targets).
    if rel == "build.rs" or rel.endswith("/build.rs"):
        return True
    return "/src/" in rel or rel.startswith("src/") or "/benches/" in rel or rel.startswith("benches/")


def _files(root: str, staged: bool):
    return [
        f
        for f in listed_files(root, staged=staged)
        if f.endswith(".rs") and _is_compiled_src(f)
    ]


def _is_generated(root: str, rel: str, staged: bool) -> bool:
    """The documented policy: `@generated` among the FIRST 40 LINES exempts
    the file. (An earlier implementation read the first 2000 characters
    instead — same intent, different window; the docstring wins.)"""
    blob = content_bytes(root, rel, staged=staged)
    if blob is None:
        return False
    head_lines = blob.decode("utf-8", errors="replace").splitlines()[:40]
    return _GENERATED_MARKER in "\n".join(head_lines)


def _scan(root: str, paths, staged: bool):
    hits = []
    for rel in paths:
        blob = content_bytes(root, rel, staged=staged)
        if blob is None or _is_generated(root, rel, staged):
            continue
        text = blob.decode("utf-8", errors="replace")
        for lineno, code in _code_portions(text):
            if _ALLOW_PATTERN.search(code):
                hits.append(f"{rel}:{lineno}: {code.strip()}")
    return hits


def _has_rust(root: str, staged: bool) -> bool:
    """Does this repo contain Rust at all? A tracked Cargo.toml is the manifest that says so."""
    return any(
        f == "Cargo.toml" or f.endswith("/Cargo.toml")
        for f in listed_files(root, staged=staged)
    )


def main():
    staged = "--staged" in sys.argv
    root = repo_root()
    files = _files(root, staged)
    hits = _scan(root, files, staged)
    if hits:
        scope = "staged" if staged else "tracked"
        print(f"✗ [{'no_allow'}] {len(hits)} #[allow] in {scope} Rust source:")
        for h in hits[:40]:
            print(f"    {h}")
        print("fix the finding properly; do not add #[allow]. Generated files must carry @generated.")
        sys.exit(1)
    # A repo with no Rust at all genuinely has nothing to police, and failing there would make
    # this gate unadoptable by every non-Rust repo that runs the shared layer. But a repo that
    # HAS Rust and whose scan found none of it is blind: the crate moved, or the src/ + benches/
    # + build.rs scope stopped matching its layout, and "0 files clean" reads identically to a
    # spotless crate. Conditioned on the manifest, so the two cases stay distinguishable.
    if not files and _has_rust(root, staged):
        print("✗ [no_allow] this repo has Rust (a Cargo.toml is tracked) and the scan matched "
              "NO compiled source. That is not a clean run -- the crate layout moved out from "
              "under src/, benches/ and build.rs.", file=sys.stderr)
        sys.exit(1)
    print(f"✓ [no_allow] OK — {len(files)} files clean")


if __name__ == "__main__":
    main()
