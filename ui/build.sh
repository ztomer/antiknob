#!/usr/bin/env bash
# Build Antiknob.app — the native macOS SwiftUI configurator for Anticater VK01 Knob.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
APP_DIR="${ROOT_DIR}/Antiknob.app"
MACOS_DIR="${APP_DIR}/Contents/MacOS"
RESOURCES_DIR="${APP_DIR}/Contents/Resources"

mkdir -p "${MACOS_DIR}" "${RESOURCES_DIR}"

cat > "${APP_DIR}/Contents/Info.plist" <<'EOF'
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
    <string>0.3.0</string>
    <key>CFBundleIconFile</key>
    <string>Antiknob.icns</string>
    <key>LSMinimumSystemVersion</key>
    <string>14.0</string>
    <key>NSHighResolutionCapable</key>
    <true/>
</dict>
</plist>
EOF

if [[ -f "${ROOT_DIR}/assets/Antiknob.icns" ]]; then
    cp "${ROOT_DIR}/assets/Antiknob.icns" "${RESOURCES_DIR}/Antiknob.icns"
fi

echo "[ ==> ] Compiling Swift sources into Antiknob.app..."
SWIFT_FILES=(
    "${SCRIPT_DIR}/Models.swift"
    "${SCRIPT_DIR}/SocketClient.swift"
    "${SCRIPT_DIR}/ConfigStore.swift"
    "${SCRIPT_DIR}/KnobHeader.swift"
    "${SCRIPT_DIR}/ChordRecorder.swift"
    "${SCRIPT_DIR}/SequenceEditor.swift"
    "${SCRIPT_DIR}/LayerDetail.swift"
    "${SCRIPT_DIR}/GeneralPane.swift"
    "${SCRIPT_DIR}/LedSection.swift"
    "${SCRIPT_DIR}/SettingsRoot.swift"
    "${SCRIPT_DIR}/App.swift"
)

swiftc -O -parse-as-library "${SWIFT_FILES[@]}" -o "${MACOS_DIR}/Antiknob"

echo "[ ==> ] Codesigning Antiknob.app..."
IDENTITY=$(security find-identity -v -p codesigning 2>/dev/null | awk -F'"' '/Apple Development/ && !/CSSMERR/ {print $2; exit}' || true)
codesign --force --sign "${IDENTITY:--}" "${APP_DIR}"
echo "[ Ok  ] Signed as: ${IDENTITY:-ad-hoc}"
echo "[ Ok  ] Built ${APP_DIR} successfully."
