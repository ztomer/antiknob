#!/usr/bin/env python3
"""check_swift_rust_model_parity.py — the Swift app and the Rust daemon model
the SAME FILE, and nothing checked that they agree.

`host.json` has two implementations of one schema. `src/host/mod.rs` is the one
that matters: it is what the daemon reads, what the hardware ends up doing,
and what falls back to defaults when a file will not parse. `ui/Sources/
AntiknobUI/Core/Models.swift` is a second `Codable` model of the same enums,
written in another language, by another toolchain, never type-checked against
the first.

`tests/surface_parity.rs` closes the CLI/MCP version of this hole — both
surfaces are generated from one table — and says nothing about Swift. So the
Swift half could not disagree with the Rust half without any test noticing.

**They had drifted, and it was not cosmetic.** Measured on 2026-10-04, with a
throwaway probe feeding each spelling to the real `serde` deserializers:

| Swift wrote            | Rust reads        | what happened                                    |
|------------------------|-------------------|--------------------------------------------------|
| `{"type":"openURL"}`   | `openUrl`         | config UNDECODABLE → `load_json` returned DEFAULTS: every layer lost |
| `{"type":"launchApp","bundleId":…}` | `bundle_id` | same — the field has no `#[serde(default)]`, so the whole file died |
| `{"delayMs":40}`       | `delay_ms`        | parsed fine, delay read back as `None`: the pause was silently dropped |
| (nothing)              | `variants`        | a GUI save deleted every virtual layer the daemon had |

The first two are total loss with no error anywhere: `try_load_json` returns
`None`, `load_json` substitutes the defaults, and the user's configuration is
replaced by the two starter layers. The third is a value quietly discarded. The
fourth is the same class as divoom-control's `check_gui_is_a_client.py`: a
client that does not model a field is not a client, it is a lossy second
implementation.

## What is compared, and how the comparison gets its truth

Extraction is by DECLARATION, from both sources, on purpose. An earlier idea
was to have the Rust side emit a contract file and diff it; a generated file is
a second thing to keep current, and it is a snapshot of whatever ran last.

The Rust half is read from the serde attributes, because those ARE the wire
format — `rename_all = "camelCase"` on an enum renames its VARIANTS and not
its fields, which is exactly the fact that made `bundleId` wrong here, and a
regex that did not know it would have agreed with the bug.

**A declaration the extractor cannot find is a FAILURE, never a skip.** A gate
that quietly matches nothing when a refactor moves an enum reports compliance
over zero files, which is indistinguishable from a pass. Every extractor
returns `None` for "not found" and this gate turns that into an error naming
the file to read.

## The allowlist is a RATCHET

* a disagreement NOT on the list fails, so no new ones can be added; and
* an entry that no longer MATCHES anything also fails, so a fixed drift cannot
  leave a permanent hole for the next one to fall into.

The second property is the one that is easy to omit, and it is the one that
decides whether this is a ratchet or a rug. It is proven, not asserted:
`tests/swift_rust_model_parity.rs` runs this script against deliberately
mutated copies of both sources and against a seeded stale entry, and requires a
non-zero exit for each.
"""
from __future__ import annotations

import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from _parity_extract import (  # noqa: E402
    RENAME_ALL_FIELDS_SENTINEL,
    read,
    rust_simple_enum_values,
    rust_action_field_names,
    rust_action_tags,
    rust_gestures,
    rust_led_modes,
    rust_struct_wire_fields,
    rust_vendor_rainbow,
    swift_coding_key_names,
    swift_encoded_action_types,
    swift_encoded_keys,
    swift_encoded_keys_per_case,
    swift_enum_cases,
    swift_gestures,
    swift_led_modes,
    swift_unified_map,
    swift_vendor_rainbow,
)

REPO = Path(
    # A seam, and the only one: tests run this script against a COPY of the
    # model files with one thing deliberately broken, which is the only way to
    # show a gate can go red. Defaults to the repo this file lives in.
    os.environ.get("ANTIKNOB_PARITY_ROOT")
    or Path(__file__).resolve().parent.parent
)
GOH = Path(
    __import__("os").environ.get("GOH_DIR")
    or __import__("os").environ.get("GOH")
    or Path.home() / "Projects/gates_of_heck"
)
sys.path.insert(0, str(GOH / "tui"))
sys.path.insert(0, str(GOH / "checks"))

RUST_HOST = REPO / "src/host/mod.rs"
RUST_GESTURE = REPO / "src/host/gesture.rs"
RUST_VARIANT = REPO / "src/host/virtual_layer.rs"
RUST_FIRMWARE = REPO / "src/firmware.rs"
SWIFT_MODELS = REPO / "ui/Sources/AntiknobUI/Core/Models.swift"
SWIFT_LED = REPO / "ui/Sources/AntiknobUI/Core/LedModes.swift"
SWIFT_GESTURE = REPO / "ui/Sources/AntiknobUI/Core/Gesture.swift"
SWIFT_LAYER_VARIANT = REPO / "ui/Sources/AntiknobUI/Core/LayerVariant.swift"


def _err(msg: str) -> None:
    print(f"✗ [swift-rust-parity] {msg}", file=sys.stderr)


def _info(msg: str) -> None:
    print(f"→ [swift-rust-parity] {msg}", file=sys.stderr)


# ── the ratchet ──────────────────────────────────────────────────────────────
#
# (finding id, what it says, why it is still here / which change removes it).
# EMPTY is the completion criterion: the same command that reported four drifts
# reports none, and any new one fails the gate.


# ── the ratchet ──────────────────────────────────────────────────────────────
#
# (finding id) -> why it is still here / which change removes it.
# EMPTY is the completion criterion: the same command that reported four drifts
# reports none, and any new one fails the gate.
ALLOWLIST: dict[str, str] = {}


# ── the checks ───────────────────────────────────────────────────────────────


def main() -> int:
    sources = {
        "src/host/mod.rs": RUST_HOST,
        "src/host/gesture.rs": RUST_GESTURE,
        "src/host/virtual_layer.rs": RUST_VARIANT,
        "src/firmware.rs": RUST_FIRMWARE,
        "ui/Sources/AntiknobUI/Core/Models.swift": SWIFT_MODELS,
        "ui/Sources/AntiknobUI/Core/LayerVariant.swift": SWIFT_LAYER_VARIANT,
        "ui/Sources/AntiknobUI/Core/LedModes.swift": SWIFT_LED,
        "ui/Sources/AntiknobUI/Core/Gesture.swift": SWIFT_GESTURE,
    }
    texts: dict[str, str] = {}
    missing: list[str] = []
    for name, path in sources.items():
        body = read(path)
        if body is None:
            missing.append(name)
        else:
            texts[name] = body
    if missing:
        for name in missing:
            _err(f"cannot read {name} — a gate that matches nothing is not a pass")
        return 1

    host = texts["src/host/mod.rs"]
    gesture_rs = texts["src/host/gesture.rs"]
    variant_rs = texts["src/host/virtual_layer.rs"]
    firmware = texts["src/firmware.rs"]
    models = texts["ui/Sources/AntiknobUI/Core/Models.swift"]
    layer_variant = texts["ui/Sources/AntiknobUI/Core/LayerVariant.swift"]
    led = texts["ui/Sources/AntiknobUI/Core/LedModes.swift"]
    gesture_sw = texts["ui/Sources/AntiknobUI/Core/Gesture.swift"]

    findings: list[tuple[str, str]] = []

    def check(fid: str, ok: bool | None, detail: str) -> None:
        """Record a finding unless it passes. `ok is None` means EXTRACT FAILED."""
        if ok is None:
            findings.append((fid, detail + " — EXTRACTOR FOUND NOTHING, which is a gate failure"))
        elif not ok:
            findings.append((fid, detail))

    # 1. Action type tags. The drift that cost every layer.
    rust_tags = rust_action_tags(host)
    swift_tags = swift_encoded_action_types(models)
    check(
        "action-tags",
        None if rust_tags is None or swift_tags is None else rust_tags == swift_tags,
        f"action type tags differ: daemon-only={sorted((rust_tags or set()) - (swift_tags or set()))} "
        f"app-only={sorted((swift_tags or set()) - (rust_tags or set()))}",
    )

    # 2. HostAction field names, per variant. This is the bundle_id arm.
    rust_fields = rust_action_field_names(host)
    swift_fields = swift_encoded_keys_per_case(models, "Action") or {}
    # Swift `openURL` is one case whose tag is `openUrl`; the Rust variant is
    # `OpenUrl`. Map case name -> tag so the two compare per-variant.
    case_to_tag = {
        "none": "none", "scroll": "scroll", "keyChord": "keyChord",
        "sequence": "sequence", "aux": "aux", "mouseClick": "mouseClick",
        "launchApp": "launchApp", "openURL": "openUrl", "openPath": "openPath",
        "quitApp": "quitApp", "hotkeySwitch": "hotkeySwitch",
    }
    if RENAME_ALL_FIELDS_SENTINEL in rust_fields:
        findings.append((
            "action-fields",
            "HostAction now sets `rename_all_fields`, which this extractor does not model, "
            "so every field name it reports is wrong in the same direction. Re-read "
            "tools/_parity_extract.py before trusting this arm.",
        ))
    for case, keys in sorted(swift_fields.items()):
        if case == RENAME_ALL_FIELDS_SENTINEL:
            continue
        tag = case_to_tag.get(case)
        if tag is None:
            findings.append(
                ("action-fields",
                 f"Swift case `{case}` is not in case_to_tag, so this gate cannot compare it; "
                 "add it rather than letting the case go unchecked")
            )
            continue
        want = rust_fields.get(tag)
        if want is None:
            # A unit variant (`None`) has no fields; `type` is the tag itself
            # and is written for every variant, so it is never "extra".
            extra = keys - {"type"}
            if extra:
                findings.append(
                    ("action-fields",
                     f"the daemon's `HostAction::{tag}` carries no fields, but the app writes "
                     f"{sorted(extra)} for `{case}`")
                )
            continue
        extra = keys - want - {"type"}
        if extra:
            findings.append(
                ("action-fields",
                 f"`{tag}`: the app encodes field(s) {sorted(extra)} the daemon's variant does "
                 f"not have (it has {sorted(want)}); serde will refuse the file or drop them")
            )

    # 3. SeqStep: the delay arm.
    rust_step = rust_struct_wire_fields(host, "SeqStep")
    swift_step = swift_encoded_keys(models, "SeqStep")
    check(
        "seqstep-keys",
        None if rust_step is None or swift_step is None else bool(swift_step <= rust_step[0]),
        f"SeqStep: the app writes {sorted(swift_step or set())}, the daemon's field set is "
        f"{sorted((rust_step or (set(), set()))[0])}; anything outside that set is silently "
        f"dropped on read, which is what made every delay vanish",
    )

    # 4. Layer keys, both directions and with the two roles kept apart:
    #    every key the daemon WRITES must be writable here, or a GUI save
    #    deletes it (`variants` was exactly that); and every key the daemon
    #    ACCEPTS must be readable here, or an old config silently loses a
    #    binding.
    rust_layer = rust_struct_wire_fields(host, "HostLayer")
    swift_layer_declared: set[str] = set()
    for text in (models, layer_variant):
        keys = swift_coding_key_names(text, "LayerConfig") or set()
        keys |= swift_coding_key_names(text, "LayerVariant") or set()
        swift_layer_declared |= keys
    if rust_layer is None:
        check("layer-keys-written", None, "could not extract HostLayer from the daemon")
    else:
        check(
            "layer-keys-written",
            not (rust_layer[0] - swift_layer_declared),
            f"the daemon's HostLayer writes key(s) {sorted(rust_layer[0] - swift_layer_declared)} "
            f"the app's CodingKeys cannot express, so a GUI save deletes them",
        )
        check(
            "layer-keys-accepted",
            not (rust_layer[1] - swift_layer_declared),
            f"the daemon's HostLayer accepts key(s) {sorted(rust_layer[1] - swift_layer_declared)} "
            f"the app cannot decode",
        )
        # And the reverse: the app must not WRITE a key the daemon cannot read.
        # `variants` and a misspelt `boundDeviceLayer` are both invisible to
        # the arms above, because the arm above only asks what the app can
        # express -- which a wrong key trivially can.
        #
        # LayerConfig only. LayerVariant's keys are compared against
        # LayerVariant's own field set below; merging the two made `apps` --
        # a real field of LayerVariant -- look like a field of HostLayer that
        # the app had invented.
        swift_layer_emits = swift_encoded_keys(models, "LayerConfig") or set()
        check(
            "layer-keys-emitted",
            bool(swift_layer_emits <= rust_layer[1]),
            f"the app writes layer key(s) {sorted(swift_layer_emits - rust_layer[1])} the "
            f"daemon has no field for; the value is discarded on read",
        )

    # 5. LayerVariant: the daemon's alternative sets must round trip too.
    rust_variant = rust_struct_wire_fields(variant_rs, "LayerVariant")
    swift_variant_declared = swift_coding_key_names(layer_variant, "LayerVariant")
    if rust_variant is None:
        check("variant-keys", None, "could not extract LayerVariant from the daemon")
    else:
        check(
            "variant-keys-written",
            not (rust_variant[0] - (swift_variant_declared or set())),
            f"the daemon's LayerVariant writes key(s) "
            f"{sorted(rust_variant[0] - (swift_variant_declared or set()))} the app's "
            f"LayerVariant cannot carry",
        )
        check(
            "variant-keys-accepted",
            not (rust_variant[1] - (swift_variant_declared or set())),
            f"the daemon's LayerVariant accepts key(s) "
            f"{sorted(rust_variant[1] - (swift_variant_declared or set()))} the app cannot decode",
        )
        swift_variant_emits = swift_encoded_keys(layer_variant, "LayerVariant")
        check(
            "variant-keys-emitted",
            None if swift_variant_emits is None else bool(swift_variant_emits <= rust_variant[1]),
            f"the app writes LayerVariant key(s) "
            f"{sorted((swift_variant_emits or set()) - rust_variant[1])} the daemon has no field "
            f"for; the value is discarded on read",
        )

    # 6. HostConfig's own top-level keys. This arm was MISSING until the gate
    #    was calibrated: renaming `pub struct HostConfig` left it green,
    #    because nothing compared the top level at all. A `Config` CodingKeys
    #    rename would have dropped `doubleTapSwitch` or `boundDeviceLayers` in
    #    exactly the way `variants` was dropped — silently, on save, with no
    #    error anywhere.
    rust_config = rust_struct_wire_fields(host, "HostConfig")
    swift_config = swift_coding_key_names(models, "Config")
    if rust_config is None:
        check("config-keys", None, "could not extract HostConfig from the daemon")
    else:
        check(
            "config-keys-written",
            not (rust_config[0] - (swift_config or set())),
            f"the daemon's HostConfig writes key(s) "
            f"{sorted(rust_config[0] - (swift_config or set()))} the app's Config cannot "
            f"express, so a GUI save deletes them",
        )
        check(
            "config-keys-accepted",
            not (rust_config[1] - (swift_config or set())),
            f"the daemon's HostConfig accepts key(s) "
            f"{sorted(rust_config[1] - (swift_config or set()))} the app cannot decode",
        )
        swift_config_emits = swift_encoded_keys(models, "Config")
        check(
            "config-keys-emitted",
            None if rust_config is None or swift_config_emits is None
            else bool(swift_config_emits <= rust_config[1]),
            f"the app writes Config key(s) "
            f"{sorted((swift_config_emits or set()) - rust_config[1])} the daemon has no field "
            f"for; the value is discarded on read and the setting is lost with no error",
        )

    # 7. AuxKey: every value the app can ENCODE must exist on the daemon side.
    rust_aux = rust_simple_enum_values(host, "AuxKey")
    swift_aux_cases = swift_enum_cases(models, "AuxKey")
    unified = swift_unified_map(models, "AuxKey")
    if rust_aux is None or swift_aux_cases is None:
        check("aux-keys", None, "could not extract AuxKey from either side")
    else:
        # `unified` collapses the app's `*External` presentation aliases to the
        # value actually encoded, so the comparison is over the WIRE, not the
        # case list. An alias that did not collapse would be a real drift and
        # would show up here as a value the daemon has never heard of.
        encoded = {unified.get(c, c) for c in swift_aux_cases}
        check(
            "aux-keys",
            encoded <= rust_aux,
            f"the app encodes aux key(s) {sorted(encoded - rust_aux)} the daemon's AuxKey does "
            f"not have (it has {sorted(rust_aux)})",
        )

    # 8. MouseButton.
    rust_mouse = rust_simple_enum_values(host, "MouseButton")
    swift_mouse = swift_enum_cases(models, "MouseButton")
    check(
        "mouse-keys",
        None if rust_mouse is None or swift_mouse is None else set(swift_mouse or []) == rust_mouse,
        f"MouseButton differs: daemon-only={sorted((rust_mouse or set()) - set(swift_mouse or []))} "
        f"app-only={sorted(set(swift_mouse or []) - (rust_mouse or set()))}",
    )

    # 9. LED modes: name AND number, both directions. `LedMode.all` mirrors
    #    LED_MODE_NAMES; a mode added on one side only is a mode the other
    #    side cannot name or draw.
    rust_led = rust_led_modes(firmware)
    swift_led = swift_led_modes(led)
    if rust_led is None or swift_led is None:
        check("led-modes", None, "could not extract the LED mode table from either side")
    else:
        if list(swift_led) != rust_led:
            findings.append(
                ("led-modes",
                 f"LED mode ids differ in order: daemon={rust_led} app={list(swift_led)}")
            )
        for i, name in enumerate(rust_led):
            if swift_led.get(name) != i:
                findings.append(
                    ("led-modes",
                     f"LED mode `{name}` is number {i} on the daemon and "
                     f"{swift_led.get(name)} in the app")
                )

    # 10. The vendor rainbow, in the vendor's order. The preview renders it,
    #    so a reordered palette is a wrong picture, not a missing one.
    rust_rainbow = rust_vendor_rainbow(firmware)
    swift_rainbow = swift_vendor_rainbow(led)
    check(
        "vendor-rainbow",
        None if rust_rainbow is None or swift_rainbow is None else rust_rainbow == swift_rainbow,
        f"the vendor rainbow differs: daemon={rust_rainbow} app={swift_rainbow}",
    )

    # 11. Gestures: five, both sides. The daemon's ORDER is slot order, so it
    #     is compared as an ordered list too.
    rust_g = rust_gestures(gesture_rs)
    swift_g = swift_gestures(gesture_sw)
    # The daemon names them `TwistL`, the app `twistL`. Folded, because the
    # question is whether the same five gestures are modelled, not whether two
    # languages agree on capitalisation.
    check(
        "gestures-set",
        None if rust_g is None or swift_g is None
        else {n.lower() for n in rust_g} == {n.lower() for n in swift_g},
        f"gesture sets differ: daemon={sorted(rust_g or [])} app={sorted(swift_g or [])}",
    )
    # Swift's declaration order is the file's, not slot order, so compare
    # against the daemon's order rather than expecting the app to match it.
    check(
        "gestures-count",
        None if rust_g is None or swift_g is None else len(rust_g) == len(swift_g),
        f"the daemon drives {len(rust_g or [])} gestures, the app offers {len(swift_g or [])}",
    )

    # ── report ───────────────────────────────────────────────────────────────
    live = {fid: detail for fid, detail in findings}
    stale = sorted(set(ALLOWLIST) - set(live))
    if stale:
        _err(f"{len(stale)} stale allowlist entr(ies) — the code was fixed but the exemption stayed")
        for fid in stale:
            _info(f"`{fid}` no longer occurs ({ALLOWLIST[fid]}); delete it from ALLOWLIST")
        return 1

    unallowlisted = {fid: d for fid, d in live.items() if fid not in ALLOWLIST}
    if unallowlisted:
        _err(
            f"{len(unallowlisted)} disagreement(s) between the Swift model and the daemon's"
        )
        for fid, detail in sorted(unallowlisted.items()):
            _info(f"{fid}: {detail}")
        _info("")
        _info("The daemon is ground truth — it is what reads host.json and drives the")
        _info("hardware. Fix the SWIFT side (ui/Sources/AntiknobUI/Core/), then re-run.")
        _info("If a disagreement is genuinely intended, add it to ALLOWLIST with the")
        _info("reason; an entry that stops matching will then FAIL until you delete it.")
        return 1

    if ALLOWLIST:
        print(
            f"✓ [swift-rust-parity] OK — {len(ALLOWLIST)} allowlisted disagreement(s) "
            f"(ratchet in progress)",
            file=sys.stderr,
        )
    else:
        print(
            "✓ [swift-rust-parity] OK — Swift and the daemon agree on the whole "
            "host.json model, allowlist EMPTY",
            file=sys.stderr,
        )
    return 0


if __name__ == "__main__":
    sys.exit(main())
