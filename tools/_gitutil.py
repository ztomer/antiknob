"""Shared git plumbing for the checkers. Two contracts live here:

1. File listing: tracked files (full runs) vs staged files (pre-commit scope).
2. Content truth: a STAGED check measures THE INDEX — what will actually be
   committed — never the worktree, which may hold unrelated scratch edits.
   Full-tree runs measure the worktree, because that is what exists now.

The class this fixes: every checker used to open() working-tree paths even in
--staged mode, so a file staged clean then dirtied in the editor could block
an innocent commit, and a file staged dirty then cleaned in the editor could
slip one through. One implementation, all checkers.
"""
import subprocess


def repo_root() -> str:
    out = subprocess.run(
        ["git", "rev-parse", "--show-toplevel"], capture_output=True, text=True
    )
    return out.stdout.strip()


def listed_files(root: str, staged: bool, pathspec: str = "*") -> list[str]:
    """Repo-root-relative file list. Staged mode lists Added/Copied/Modified
    index entries; full mode lists everything tracked.

    `-z` + NUL splitting is load-bearing: without it git QUOTES paths holding
    non-ASCII or control characters ("caf\\303\\251.md", "we\\nird.md"), names
    no consumer could resolve — those files were silently unpoliced. decode
    with 'replace' keeps a hostile byte sequence from killing the gate."""
    cmd = (
        ["git", "-C", root, "diff", "--cached", "--name-only", "-z",
         "--diff-filter=ACM"]
        if staged
        else ["git", "-C", root, "ls-files", "-z"]
    )
    out = subprocess.run(cmd, capture_output=True)
    if out.returncode != 0:
        # A silent [] here is the worst possible answer: every consumer reports "0 files clean"
        # and exits 0, so a corrupt index or an unreadable object database reads exactly like a
        # spotless repo. Callers guard the "not a git repo" case themselves via repo_root(); this
        # is git ANSWERING and failing, which nothing was distinguishing from an empty tree.
        raise RuntimeError(
            "git %s failed (exit %d): %s"
            % (cmd[3], out.returncode, out.stderr.decode("utf-8", "replace").strip()[:200])
        )
    return [
        n.decode("utf-8", "replace") for n in out.stdout.split(b"\0") if n
    ]


def content_bytes(root: str, rel: str, staged: bool):
    """The bytes this check should police: the index blob when staged, the
    worktree otherwise. None means 'nothing to police' (deleted/binary-unreadable
    is the caller's call — this returns raw bytes either way when present).

    No PEP 604 unions in annotations here on purpose: hooks resolve whatever
    `python3` is on PATH (often the OS-bundled 3.9), and `bytes | None`
    evaluates at def time there."""
    if staged:
        out = subprocess.run(
            ["git", "-C", root, "show", f":{rel}"],
            capture_output=True,
        )
        if out.returncode == 0:
            return out.stdout
        # Not resolvable from the index (race with a re-staged delete):
        # fall through to whatever the worktree has.
    try:
        with open(f"{root}/{rel}", "rb") as fh:
            return fh.read()
    except OSError:
        return None
