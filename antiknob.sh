#!/usr/bin/env bash
# ==============================================================================
# antiknob.sh - Native ARM64 CLI tool for Anticater VK01 Knob on macOS
# ==============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN_PATH="${SCRIPT_DIR}/bin/ch57x-keyboard-tool"

if [[ ! -x "${BIN_PATH}" ]]; then
    if command -v ch57x-keyboard-tool >/dev/null 2>&1; then
        BIN_PATH="$(command -v ch57x-keyboard-tool)"
    else
        echo "[ Err ] Native binary not found at ${BIN_PATH}." >&2
        echo "[ ==> ] Please run 'cargo install --root \"${SCRIPT_DIR}\" ch57x-keyboard-tool' to compile." >&2
        exit 1
    fi
fi

usage() {
    cat << EOF
antiknob - Native macOS ARM64 configurator for Anticater VK01 Knob

Usage:
  ./antiknob.sh validate [config.yaml]    Validate mapping YAML syntax
  ./antiknob.sh upload [config.yaml]      Flash mappings to keyboard over USB
  ./antiknob.sh led <layer> <mode...>     Configure LED backlight mode
  ./antiknob.sh show-keys                 List all recognized keycodes and modifiers
  ./antiknob.sh info                      Show detected hardware architecture and binary info

Examples:
  ./antiknob.sh validate config.yaml
  sudo ./antiknob.sh upload config.yaml
  sudo ./antiknob.sh led 0 backlight white
  sudo ./antiknob.sh led 0 shock blue
EOF
}

cmd="${1:-help}"
shift || true

case "${cmd}" in
    validate)
        CONFIG_FILE="${1:-${SCRIPT_DIR}/config.yaml}"
        if [[ ! -f "${CONFIG_FILE}" ]]; then
            echo "[ Err ] Config file '${CONFIG_FILE}' not found." >&2
            exit 1
        fi
        echo "[ ==> ] Validating configuration '${CONFIG_FILE}'..."
        "${BIN_PATH}" validate < "${CONFIG_FILE}"
        echo "[ Ok  ] Config '${CONFIG_FILE}' is valid."
        ;;
    upload)
        CONFIG_FILE="${1:-${SCRIPT_DIR}/config.yaml}"
        if [[ ! -f "${CONFIG_FILE}" ]]; then
            echo "[ Err ] Config file '${CONFIG_FILE}' not found." >&2
            exit 1
        fi
        echo "[ ==> ] Validating '${CONFIG_FILE}' prior to flashing..."
        "${BIN_PATH}" validate < "${CONFIG_FILE}"
        echo "[ ==> ] Uploading to Anticater VK01 over USB..."
        "${BIN_PATH}" upload < "${CONFIG_FILE}"
        echo "[ Ok  ] Configuration successfully written to device!"
        ;;
    led)
        if [[ $# -lt 2 ]]; then
            echo "[ Err ] Missing arguments. Usage: ./antiknob.sh led <layer> <mode...>" >&2
            echo "        Example: ./antiknob.sh led 0 backlight white" >&2
            exit 1
        fi
        echo "[ ==> ] Setting LED mode for layer $1: ${*:2}..."
        "${BIN_PATH}" led "$@"
        echo "[ Ok  ] LED mode updated."
        ;;
    show-keys)
        "${BIN_PATH}" show-keys
        ;;
    info)
        echo "[ ==> ] Binary: ${BIN_PATH}"
        file "${BIN_PATH}"
        echo "[ ==> ] System Architecture: $(uname -m)"
        ;;
    help|-h|--help)
        usage
        ;;
    *)
        echo "[ Err ] Unknown command '${cmd}'." >&2
        usage
        exit 1
        ;;
esac
