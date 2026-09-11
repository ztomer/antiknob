#!/usr/bin/env bash
# Build Antiknob.app — the native macOS SwiftUI configurator for Anticater VK01 Knob.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
APP_DIR="${ROOT_DIR}/Antiknob.app"
MACOS_DIR="${APP_DIR}/Contents/MacOS"
RESOURCES_DIR="${APP_DIR}/Contents/Resources"

mkdir -p "${MACOS_DIR}" "${RESOURCES_DIR}"

# Cargo.toml is the single source of truth for the version. This script used
# to carry its own copy, which drifted three releases behind the tag.
VERSION="$(awk -F\" '/^version = /{print $2; exit}' "${ROOT_DIR}/Cargo.toml")"
[[ -n "${VERSION}" ]] || { echo "[ Err ] Cannot read version from Cargo.toml" >&2; exit 1; }
BUILD="$(git -C "${ROOT_DIR}" rev-list --count HEAD 2>/dev/null || echo 0)"

cat > "${APP_DIR}/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleIdentifier</key>
    <string>com.antiknob.app</string>
    <key>CFBundleName</key>
    <string>Antiknob</string>
    <key>CFBundleDisplayName</key>
    <string>Antiknob</string>
    <key>CFBundleExecutable</key>
    <string>Antiknob</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>${VERSION}</string>
    <key>CFBundleVersion</key>
    <string>${BUILD}</string>
    <key>CFBundleIconFile</key>
    <string>Antiknob.icns</string>
    <key>LSMinimumSystemVersion</key>
    <string>26.0</string>
    <key>NSHighResolutionCapable</key>
    <true/>
</dict>
</plist>
EOF

if [[ -f "${ROOT_DIR}/assets/Antiknob.icns" ]]; then
    cp "${ROOT_DIR}/assets/Antiknob.icns" "${RESOURCES_DIR}/Antiknob.icns"
fi
if [[ -f "${ROOT_DIR}/assets/antiknob-1024.png" ]]; then
    cp "${ROOT_DIR}/assets/antiknob-1024.png" "${RESOURCES_DIR}/antiknob-1024.png"
    cp "${ROOT_DIR}/assets/antiknob-1024.png" "${RESOURCES_DIR}/ak12-1024.png"
elif [[ -f "${ROOT_DIR}/assets/ak12-1024.png" ]]; then
    cp "${ROOT_DIR}/assets/ak12-1024.png" "${RESOURCES_DIR}/ak12-1024.png"
fi
if [[ -f "${ROOT_DIR}/assets/knob-tray.png" ]]; then
    cp "${ROOT_DIR}/assets/knob-tray.png" "${RESOURCES_DIR}/knob-tray.png"
    cp "${ROOT_DIR}/assets/knob-tray.png" "${RESOURCES_DIR}/ak12-tray.png"
fi
if [[ -f "${ROOT_DIR}/assets/knob-tray@2x.png" ]]; then
    cp "${ROOT_DIR}/assets/knob-tray@2x.png" "${RESOURCES_DIR}/knob-tray@2x.png"
    cp "${ROOT_DIR}/assets/knob-tray@2x.png" "${RESOURCES_DIR}/ak12-tray@2x.png"
fi

echo "[ ==> ] Compiling Swift sources into Antiknob.app (v${VERSION} build ${BUILD})..."

# Built through SwiftPM rather than a hand-rolled swiftc line, so this script
# and `./tools/gate.sh --full` compile exactly the same thing. The package
# pins the Swift 6 language mode and the macOS 26 platform (see ui/Package.swift);
# only the flags SwiftPM has no manifest setting for are passed here.
swift build --package-path "${SCRIPT_DIR}" -c release \
    --product Antiknob \
    -Xswiftc -warnings-as-errors

BIN_PATH="$(swift build --package-path "${SCRIPT_DIR}" -c release \
    --product Antiknob --show-bin-path)"
install -m 755 "${BIN_PATH}/Antiknob" "${MACOS_DIR}/Antiknob"

echo "[ ==> ] Codesigning Antiknob.app..."
# install.sh passes the identity it resolved, so both bundles carry one
# signer; standalone runs still fall back to an Apple Development cert.
IDENTITY="${ANTIKNOB_SIGN_ID:-}"
if [[ -z "${IDENTITY}" || "${IDENTITY}" == "-" ]]; then
    IDENTITY=$(security find-identity -v -p codesigning 2>/dev/null | awk -F'"' '/Apple Development/ && !/CSSMERR/ {print $2; exit}' || true)
fi
codesign --force --sign "${IDENTITY:--}" "${APP_DIR}"
echo "[ Ok  ] Signed as: ${IDENTITY:-ad-hoc}"
echo "[ Ok  ] Built ${APP_DIR} successfully."
