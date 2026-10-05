"""Declaration extractors for `check_swift_rust_model_parity.py`.

Both halves of that gate read FACTS OUT OF SOURCE — the serde attributes on the
Rust side, the `CodingKeys` and `encode(to:)` on the Swift side — and this
module is where those reads live, so the gate file is the checks and the
reporting and nothing else.

**Every extractor returns `None` for "I could not find that".** None of them
ever returns an empty set to mean failure. That distinction is the whole
contract: a rename that moves a declaration would otherwise turn a regex into
a silent pass, reporting compliance over zero files, which is indistinguishable
from agreement. The gate turns `None` into a named failure.

A seam, not a vendored copy: the sibling module in `tools/`, imported by path
exactly as `divoom-control`'s gates import `tools/_tui.py`.
"""

import re
from pathlib import Path

#: Sentinel key `rust_action_field_names` adds when `HostAction` grows a
#: `rename_all_fields` this module does not model. The gate turns it into a
#: named failure; it is never compared as a field name.
RENAME_ALL_FIELDS_SENTINEL = "__RENAME_ALL_FIELDS_PRESENT__"


def _rust_block(src: str, header: str) -> str | None:
    """The body of a Rust item, from `header` to the next top-level item."""
    i = src.find(header)
    if i < 0:
        return None
    rest = src[i + len(header):]
    m = re.search(r"^\}", rest, re.M)
    return rest[: m.start()] if m else None


def _strip_comments(src: str) -> str:
    """Drop `//` comments, so a name quoted in prose is not read as code."""
    return re.sub(r"//[^\n]*", "", src)


def _swift_type_body(src: str, decl: str) -> str | None:
    """The body of a Swift type declaration, braces balanced.

    Brace counting rather than a regex to the next `\n}`: several of these
    types contain nested `enum CodingKeys { ... }` and closures, and a
    non-greedy match to the first closing brace at any indent silently returns
    half a declaration.
    """
    m = re.search(rf"\b{decl}\b[^{{]*\{{", src)
    if not m:
        return None
    i = m.end() - 1
    depth = 0
    while i < len(src):
        if src[i] == "{":
            depth += 1
        elif src[i] == "}":
            depth -= 1
            if depth == 0:
                return src[m.end(): i]
        i += 1
    return None


def read(path: Path) -> str | None:
    """A source file's text, or None. Used for the unreadable-source arm."""
    if not path.is_file():
        return None
    return path.read_text(encoding="utf-8")


def rust_action_tags(src: str) -> set[str] | None:
    """Wire tags `HostAction` serialises, from its serde attributes.

    `#[serde(tag = "type", rename_all = "camelCase")]` renames the VARIANTS;
    `None` (the default variant) needs no rename. So each variant becomes
    lowerCamelCase of its Rust name.
    """
    attrs = re.search(
        r'#\[serde\(tag\s*=\s*"type"\s*,\s*rename_all\s*=\s*"(\w+)"\)\]\s*'
        r"pub enum HostAction",
        src,
    )
    if not attrs:
        return None
    rename = attrs.group(1)
    body = _rust_block(src, "pub enum HostAction")
    if body is None:
        return None
    # `#[default]` decorates a variant; strip attributes before reading names.
    body = re.sub(r"^\s*#\[[^\]]*\]\s*$", "", body, flags=re.M)
    names = re.findall(r"^\s{4}([A-Z][A-Za-z0-9]*)\s*\{", body, re.M)
    names += re.findall(r"^\s{4}([A-Z][A-Za-z0-9]*)\s*,\s*$", body, re.M)
    if not names:
        return None
    if rename == "camelCase":
        return {n[0].lower() + n[1:] for n in names}
    if rename == "snake_case":
        return {re.sub(r"(?<!^)(?=[A-Z])", "_", n).lower() for n in names}
    if rename == "lowercase":
        return {n.lower() for n in names}
    return None


def rust_simple_enum_values(src: str, name: str) -> set[str] | None:
    """Wire values of `pub enum <name>`, honouring `rename_all` on it."""
    m = re.search(
        rf'((?:#\[serde\([^\)]*\)\]\s*)?)pub enum {name}\b', src, re.M
    )
    if not m:
        return None
    rename = re.search(r'rename_all\s*=\s*"(\w+)"', m.group(1) or "")
    body = _rust_block(src, f"pub enum {name}")
    if body is None:
        return None
    names = re.findall(r"^\s{4}([A-Z][A-Za-z0-9]*)\s*,\s*$", body, re.M)
    if not names:
        return None
    mode = rename.group(1) if rename else None
    if mode == "camelCase":
        return {n[0].lower() + n[1:] for n in names}
    if mode == "snake_case":
        return {re.sub(r"(?<!^)(?=[A-Z])", "_", n).lower() for n in names}
    if mode == "lowercase":
        return {n.lower() for n in names}
    return set(names)


def rust_action_field_names(src: str) -> dict[str, set[str]]:
    """Per-`HostAction`-variant field wire names, as serde computes them.

    A variant's own struct fields are renamed ONLY by `rename_all_fields`,
    which `HostAction` does not have — the container's `rename_all` applies to
    variant names. That single fact is the `bundleId` bug, so the extractor has
    to model it rather than assume the container attribute applies twice.
    """
    # Scoped to the attribute block ON HostAction. Searching the whole file
    # would let an unrelated struct's `rename_all_fields` change what this
    # extractor believes about HostAction's wire, with nothing to connect them.
    attrs = re.search(r'((?:#\[serde\([^\)]*\)\]\s*)?)pub enum HostAction', src)
    has_rename_all_fields = bool(attrs and "rename_all_fields" in (attrs.group(1) or ""))
    body = _rust_block(src, "pub enum HostAction")
    if body is None:
        return {}
    out: dict[str, set[str]] = {}
    for variant, inner in re.findall(
        r"^\s{4}([A-Z][A-Za-z0-9]*)\s*\{(.*?)^\s{4}\}", body, re.M | re.S
    ):
        names = set()
        for fname in re.findall(r"^\s{8}([a-z_][a-z0-9_]*)\s*:", inner, re.M):
            if fname == "serde":
                continue
            names.add(fname)
        tag = variant[0].lower() + variant[1:]
        out[tag] = names
    if has_rename_all_fields:
        # A future edit adding `rename_all_fields` would change what this
        # extractor's model of the wire IS, silently — every field name it
        # reports would become wrong in the same direction. Surfaced as a
        # sentinel so the gate goes red rather than trusting a stale model.
        out[RENAME_ALL_FIELDS_SENTINEL] = set()
    return out


def rust_enum_variant_count(src: str, name: str) -> int | None:
    body = _rust_block(src, f"pub enum {name}")
    if body is None:
        return None
    body = re.sub(r"^\s*#\[[^\]]*\]\s*$", "", body, flags=re.M)
    return len(
        re.findall(r"^\s{4}([A-Z][A-Za-z0-9]*)\s*(?:,|\{)", body, re.M)
    )


def rust_struct_wire_fields(src: str, name: str) -> tuple[set[str], set[str]] | None:
    """`(written, accepted)` wire field names of a plain `pub struct`.

    Two sets because serde's two jobs are different and conflating them is how
    this gate would demand the impossible. `rename_all` decides what is
    SERIALISED; `alias` names are ACCEPTED ON INPUT ONLY and never written. So
    `HostLayer` writes `twistL` and additionally tolerates `twist_l`, and
    requiring the Swift model to be able to WRITE `twist_l` would be wrong —
    requiring it to be able to READ it is not.
    """
    m = re.search(
        rf'((?:#\[serde\([^\)]*\)\]\s*)?)pub struct {name}\b', src, re.M
    )
    if not m:
        return None
    attrs = m.group(1) or ""
    rename = re.search(r'rename_all\s*=\s*"(\w+)"', attrs)
    body = _rust_block(src, f"pub struct {name}")
    if body is None:
        return None
    written: set[str] = set()
    # Per-field `#[serde(..., alias = "...")]` sits on the FIELD, so the whole
    # body has to be scanned for aliases rather than just the container attrs.
    aliases: set[str] = set(re.findall(r'alias\s*=\s*"([^"]+)"', attrs + "\n" + body))
    for fname in re.findall(r"^\s{4}pub ([a-z_][a-z0-9_]*)\s*:", body, re.M):
        if rename and rename.group(1) == "camelCase":
            parts = fname.split("_")
            written.add(parts[0] + "".join(p.title() for p in parts[1:]))
        else:
            written.add(fname)
    if not written:
        return None
    return written, written | aliases


def rust_led_modes(src: str) -> list[str] | None:
    m = re.search(r'LED_MODE_NAMES:\s*\[&str;\s*\d+\]\s*=\s*\[(.*?)\]', src, re.S)
    if not m:
        return None
    return re.findall(r'"([^"]+)"', m.group(1))


def rust_vendor_rainbow(src: str) -> list[str] | None:
    """The vendor palette as decimal `r,g,b`, so it compares to Swift's floats."""
    m = re.search(
        r"VENDOR_RAINBOW:\s*\[\(u8,\s*u8,\s*u8\);\s*\d+\]\s*=\s*\[(.*?)\n\]", src, re.S
    )
    if not m:
        return None
    triples = re.findall(
        r"\(\s*(0x[0-9A-Fa-f]+|\d+)\s*,\s*(0x[0-9A-Fa-f]+|\d+)\s*,\s*(0x[0-9A-Fa-f]+|\d+)\s*\)",
        m.group(1),
    )
    if not triples:
        return None
    return [",".join(str(int(v, 0)) for v in t) for t in triples]


def rust_gestures(src: str) -> list[str] | None:
    """`Gesture::ALL` — slot order, which is what makes it worth comparing."""
    m = re.search(r"pub const ALL: \[Gesture;\s*\d+\]\s*=\s*\[(.*?)\];", src, re.S)
    if not m:
        return None
    names = re.findall(r"Gesture::([A-Za-z]+)", m.group(1))
    return names or None


# ── Swift side ───────────────────────────────────────────────────────────────


def swift_enum_cases(src: str, name: str) -> list[str] | None:
    body = _swift_type_body(src, rf"(?:struct|enum)\s+{name}\b")
    if body is None:
        return None
    cases = re.findall(r"^\s*case\s+([A-Za-z][A-Za-z0-9]*)", _strip_comments(body), re.M)
    return cases or None


def swift_unified_map(src: str, name: str) -> dict[str, str]:
    """`AuxKey.unified`: which wire value each case actually encodes as."""
    body = _swift_type_body(src, rf"enum\s+{name}\b")
    if body is None:
        return {}
    return {
        case: ret
        for case, ret in re.findall(
            r"case\s+\.([A-Za-z][A-Za-z0-9]*)\s*:\s*return\s+\.([A-Za-z][A-Za-z0-9]*)",
            body,
        )
    }


def _swift_encode_body(src: str, type_name: str) -> str | None:
    """The `encode(to:)` belonging to one named type, comments stripped.

    Matches `struct` as well as `enum`: `Action` is an enum, `SeqStep`,
    `LayerConfig` and `LayerVariant` are structs, and a helper that only
    understood the first reported every struct's encoder as absent.
    """
    body = _swift_type_body(src, rf"(?:struct|enum)\s+{type_name}\b")
    if body is None:
        return None
    m = re.search(r"func encode\(to encoder: Encoder\) throws\s*\{", body)
    if not m:
        return None
    rest = body[m.end() - 1:]
    depth = 0
    for i, ch in enumerate(rest):
        if ch == "{":
            depth += 1
        elif ch == "}":
            depth -= 1
            if depth == 0:
                return _strip_comments(rest[: i + 1])
    return None


def swift_encoded_action_types(src: str) -> set[str] | None:
    """The `type` strings the Swift encoder can WRITE.

    Read from `Action.encode(to:)`, not from the enum's case list: `openURL`
    and `openUrl` are one Swift case with one payload, and only one of them is
    what goes on the wire. A gate reading the case list would have called the
    two spellings agreement — which is exactly how `openURL` survived.
    """
    body = _swift_encode_body(src, "Action")
    if body is None:
        return None
    tags = set(re.findall(r'encode\("([^"]+)",\s*forKey:\s*\.type\)', body))
    return tags or None


def swift_encoded_keys_per_case(src: str, type_name: str) -> dict[str, set[str]] | None:
    """Per-`case`, the CodingKeys the encoder writes for it."""
    body = _swift_encode_body(src, type_name)
    if body is None:
        return None
    out: dict[str, set[str]] = {}
    for case, inner in re.findall(
        r"case\s+\.([a-zA-Z][A-Za-z0-9]*)[^:\n]*:\s*(.*?)(?=\n\s{8}case\s+\.|\s*$)",
        body,
        re.S,
    ):
        keys = set(re.findall(r"forKey:\s*\.([A-Za-z_][A-Za-z0-9_]*)", inner))
        if keys:
            out[case] = keys
    return out or None


def swift_encoded_keys(src: str, type_name: str) -> set[str] | None:
    """Every CodingKey a type's encoder writes, flattened.

    Handles both encoder shapes. `Action` switches per case; `SeqStep` and the
    structs write straight-line `try c.encode…(forKey:)` with no `case` at all,
    and an extractor that only understood the first shape reported SeqStep's
    encoder as empty — a finding phrased as drift when it was blindness.
    """
    body = _swift_encode_body(src, type_name)
    if body is None:
        return None
    keys = set(re.findall(r"forKey:\s*\.([A-Za-z_][A-Za-z0-9_]*)", body))
    return keys or None


def swift_coding_key_names(src: str, type_name: str) -> set[str] | None:
    """Every key a type's `CodingKeys` DECLARES, encode and decode alike.

    The acceptance half of the contract: what this app can READ. A key
    declared but never written is exactly the `delayMs` case, and it is not
    dead code — it is how a file written by an older build still loads.

    `case name, apps` declares TWO keys, and reading only the first is how
    `apps` came to look absent from the app's model.
    """
    body = _swift_type_body(src, rf"(?:struct|enum)\s+{type_name}\b")
    if body is None:
        return None
    keys: set[str] = set()
    for block in re.findall(
        r"enum CodingKeys: String, CodingKey \{(.*?)\n\s{4}\}", body, re.S
    ):
        for group in re.findall(r"\bcase\s+([^\n]+)", _strip_comments(block)):
            for name in group.split(","):
                name = name.strip()
                if re.fullmatch(r"[a-zA-Z_][A-Za-z0-9_]*", name):
                    keys.add(name)
    return keys or None


def swift_led_modes(src: str) -> dict[str, int] | None:
    m = re.search(r"static let all: \[LedMode\] = \[(.*?)\n    \]", src, re.S)
    if not m:
        return None
    ids = re.findall(r'id:\s*"([^"]+)"', m.group(1))
    numbers = [int(n) for n in re.findall(r"number:\s*(\d+)", m.group(1))]
    if len(ids) != len(numbers) or not ids:
        return None
    return dict(zip(ids, numbers, strict=True))


def swift_vendor_rainbow(src: str) -> list[str] | None:
    m = re.search(r"static let vendorRainbow: \[RGB\] = \[(.*?)\n    \]", src, re.S)
    if not m:
        return None
    triples = re.findall(r"RGB\(\s*([\d.]+)\s*,\s*([\d.]+)\s*,\s*([\d.]+)\s*\)", m.group(1))
    if not triples:
        return None
    out = []
    for r, g, b in triples:
        out.append(",".join(str(round(float(v) * 255)) for v in (r, g, b)))
    return out


def swift_gestures(src: str) -> list[str] | None:
    m = re.search(r"\benum Gesture\b[^{]*\{(.*?)\n\}", src, re.S)
    if not m:
        return None
    return re.findall(r"^\s*case\s+([a-zA-Z][A-Za-z0-9]*)", m.group(1), re.M) or None
