#!/usr/bin/env bash
# Install Antiknob CLI and macOS GUI App to /Applications/Antiknob
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${SCRIPT_DIR}"

DEST_DIR="${1:-/Applications/Antiknob}"
TARGET_DIR="$(cargo metadata --format-version 1 | python3 -c 'import json, sys; print(json.load(sys.stdin)["target_directory"])')"

echo "[ ==> ] Building release binaries (arm64)..."
cargo build --release

echo "[ ==> ] Installing to ${DEST_DIR}..."
mkdir -p "${DEST_DIR}/bin"
cp "${TARGET_DIR}/release/antiknob" "${DEST_DIR}/bin/antiknob"
cp "${TARGET_DIR}/release/antiknob-gui" "${DEST_DIR}/bin/antiknob-gui"
cp config.yaml "${DEST_DIR}/config.yaml"

# Assemble macOS Application Bundle
APP_BUNDLE="${DEST_DIR}/Antiknob.app"
echo "[ ==> ] Creating macOS App Bundle at ${APP_BUNDLE}..."
mkdir -p "${APP_BUNDLE}/Contents/MacOS"
mkdir -p "${APP_BUNDLE}/Contents/Resources"

cp "${DEST_DIR}/bin/antiknob-gui" "${APP_BUNDLE}/Contents/MacOS/Antiknob"
chmod +x "${APP_BUNDLE}/Contents/MacOS/Antiknob"

if [[ -f "extracted_assets/Antiknob.icns" ]]; then
    cp "extracted_assets/Antiknob.icns" "${APP_BUNDLE}/Contents/Resources/Antiknob.icns"
fi

cat << 'PLIST' > "${APP_BUNDLE}/Contents/Info.plist"
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleExecutable</key>
	<string>Antiknob</string>
	<key>CFBundleIconFile</key>
	<string>Antiknob.icns</string>
	<key>CFBundleIdentifier</key>
	<string>com.antiknob.app</string>
	<key>CFBundleName</key>
	<string>Antiknob</string>
	<key>CFBundlePackageType</key>
	<string>APPL</string>
	<key>CFBundleShortVersionString</key>
	<string>0.1.0</string>
	<key>CFBundleVersion</key>
	<string>1</string>
	<key>LSMinimumSystemVersion</key>
	<string>12.0</string>
	<key>NSHighResolutionCapable</key>
	<true/>
</dict>
</plist>
PLIST

echo "[ ==> ] Ad-hoc codesigning App Bundle..."
codesign -s - --force --deep "${APP_BUNDLE}"

# Create convenient symlink in /Applications for Spotlight / Launchpad
if [[ "${DEST_DIR}" == "/Applications/Antiknob" ]]; then
    ln -sf "${APP_BUNDLE}" "/Applications/Antiknob.app"
fi

# Link CLI if user has ~/.local/bin in PATH
if [[ -d "${HOME}/.local/bin" ]]; then
    ln -sf "${DEST_DIR}/bin/antiknob" "${HOME}/.local/bin/antiknob"
    echo "[ Ok  ] Symlinked CLI to ${HOME}/.local/bin/antiknob"
fi

echo "[ Ok  ] Antiknob installed successfully to ${DEST_DIR}!"
echo "        GUI App: ${APP_BUNDLE}"
echo "        CLI tool: ${DEST_DIR}/bin/antiknob"
