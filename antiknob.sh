#!/usr/bin/env bash
# ==============================================================================
# antiknob.sh - Native macOS Apple Silicon CLI for Anticater VK01 Knob
# Pure Rust, Open Source (MIT OR Apache-2.0), Sudo-Free
# ==============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN_PATH="${SCRIPT_DIR}/bin/antiknob"

if [[ ! -x "${BIN_PATH}" ]]; then
    if command -v antiknob >/dev/null 2>&1; then
        BIN_PATH="$(command -v antiknob)"
    else
        echo "[ ==> ] Compiling native antiknob binary with cargo..."
        cargo install --path "${SCRIPT_DIR}" --root "${SCRIPT_DIR}"
    fi
fi

exec "${BIN_PATH}" "$@"
