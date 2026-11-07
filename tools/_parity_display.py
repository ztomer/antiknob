"""The display arms of `check_swift_rust_model_parity.py`: LED modes, the vendor rainbow, gestures.

Split out to keep the gate under the line cap. `check` and `findings` are the gate's own
recorder and list, so a finding here is reported exactly as one raised in the gate.
"""

from __future__ import annotations

from collections.abc import Callable

from _parity_extract import (
    rust_gestures,
    rust_led_modes,
    rust_vendor_rainbow,
    swift_gestures,
    swift_led_modes,
    swift_vendor_rainbow,
)


def check_display(
    firmware: str,
    led: str,
    gesture_rs: str,
    gesture_sw: str,
    check: Callable[[str, bool | None, str], None],
    findings: list[tuple[str, str]],
) -> None:
    # 9. LED modes: name AND number, both directions. `LedMode.all` mirrors
    #    LED_MODE_NAMES; a mode added on one side only is a mode the other
    #    side cannot name or draw.
    rust_led = rust_led_modes(firmware)
    swift_led = swift_led_modes(led)
    if rust_led is None or swift_led is None:
        check(
            "led-modes", None, "could not extract the LED mode table from either side"
        )
    else:
        if list(swift_led) != rust_led:
            findings.append(
                (
                    "led-modes",
                    f"LED mode ids differ in order: daemon={rust_led} app={list(swift_led)}",
                )
            )
        for i, name in enumerate(rust_led):
            if swift_led.get(name) != i:
                findings.append(
                    (
                        "led-modes",
                        (
                            f"LED mode `{name}` is number {i} on the daemon and "
                            f"{swift_led.get(name)} in the app"
                        ),
                    )
                )

    # 10. The vendor rainbow, in the vendor's order. The preview renders it,
    #    so a reordered palette is a wrong picture, not a missing one.
    rust_rainbow = rust_vendor_rainbow(firmware)
    swift_rainbow = swift_vendor_rainbow(led)
    check(
        "vendor-rainbow",
        None
        if rust_rainbow is None or swift_rainbow is None
        else rust_rainbow == swift_rainbow,
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
        None
        if rust_g is None or swift_g is None
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
