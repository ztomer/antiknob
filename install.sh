#!/usr/bin/env bash
# Install Antiknob CLI, GUI app, and menu-bar daemon to /Applications/Antiknob
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${SCRIPT_DIR}"

DEST_DIR="${1:-/Applications/Antiknob}"
TARGET_DIR="$(cargo metadata --format-version 1 | python3 -c 'import json, sys; print(json.load(sys.stdin)["target_directory"])')"
PKG_VERSION="$(cargo metadata --format-version 1 | python3 -c 'import json, sys; print([p["version"] for p in json.load(sys.stdin)["packages"] if p["name"] == "antiknob"][0])')"
PKG_BUILD="$(git rev-list --count HEAD)"

echo "[ ==> ] Building release binaries (arm64)..."
cargo build --release

echo "[ ==> ] Installing to ${DEST_DIR}..."
mkdir -p "${DEST_DIR}/bin"
cp "${TARGET_DIR}/release/antiknob" "${DEST_DIR}/bin/antiknob"
cp "${TARGET_DIR}/release/antiknob-gui" "${DEST_DIR}/bin/antiknob-gui"
cp "${TARGET_DIR}/release/antiknob-daemon" "${DEST_DIR}/bin/antiknob-daemon"
# Never overwrite a live config: install the starter only when missing.
if [[ ! -f "${DEST_DIR}/config.yaml" ]]; then
    cp config.yaml "${DEST_DIR}/config.yaml"
else
    echo "[ --- ] Keeping existing ${DEST_DIR}/config.yaml (repo copy differs; diff to review)."
fi

# Assemble macOS Application Bundle (clean rebuild: stale nested content
# breaks codesigning with "unsealed contents present in the bundle root")
APP_BUNDLE="${DEST_DIR}/Antiknob.app"
echo "[ ==> ] Creating macOS App Bundle at ${APP_BUNDLE}..."
rm -rf "${APP_BUNDLE}"
mkdir -p "${APP_BUNDLE}/Contents/MacOS"
mkdir -p "${APP_BUNDLE}/Contents/Resources"

cp "${DEST_DIR}/bin/antiknob-gui" "${APP_BUNDLE}/Contents/MacOS/Antiknob"
chmod +x "${APP_BUNDLE}/Contents/MacOS/Antiknob"

# Original cartoon AK-12 app icon (assets/Antiknob.icns, generated from
# assets/ak12-1024.png — original artwork, no vendor assets).
if [[ -f "assets/Antiknob.icns" ]]; then
    cp "assets/Antiknob.icns" "${APP_BUNDLE}/Contents/Resources/Antiknob.icns"
fi

cat << PLIST > "${APP_BUNDLE}/Contents/Info.plist"
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
	<string>${PKG_VERSION}</string>
	<key>CFBundleVersion</key>
	<string>${PKG_BUILD}</string>
	<key>LSMinimumSystemVersion</key>
	<string>12.0</string>
	<key>NSHighResolutionCapable</key>
	<true/>
</dict>
</plist>
PLIST

echo "[ ==> ] Ad-hoc codesigning App Bundle..."
codesign -s - --force --deep "${APP_BUNDLE}"

# Assemble menu-bar daemon bundle (LSUIElement: tray icon, no dock icon)
DAEMON_BUNDLE="${DEST_DIR}/AntiknobDaemon.app"
echo "[ ==> ] Creating daemon App Bundle at ${DAEMON_BUNDLE}..."
rm -rf "${DAEMON_BUNDLE}"
mkdir -p "${DAEMON_BUNDLE}/Contents/MacOS"
mkdir -p "${DAEMON_BUNDLE}/Contents/Resources"

cp "${DEST_DIR}/bin/antiknob-daemon" "${DAEMON_BUNDLE}/Contents/MacOS/AntiknobDaemon"
chmod +x "${DAEMON_BUNDLE}/Contents/MacOS/AntiknobDaemon"

if [[ -f "assets/Antiknob.icns" ]]; then
    cp "assets/Antiknob.icns" "${DAEMON_BUNDLE}/Contents/Resources/Antiknob.icns"
fi

cat << PLIST > "${DAEMON_BUNDLE}/Contents/Info.plist"
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleExecutable</key>
	<string>AntiknobDaemon</string>
	<key>CFBundleIconFile</key>
	<string>Antiknob.icns</string>
	<key>CFBundleIdentifier</key>
	<string>com.antiknob.daemon</string>
	<key>CFBundleName</key>
	<string>AntiknobDaemon</string>
	<key>CFBundlePackageType</key>
	<string>APPL</string>
	<key>CFBundleShortVersionString</key>
	<string>${PKG_VERSION}</string>
	<key>CFBundleVersion</key>
	<string>${PKG_BUILD}</string>
	<key>LSMinimumSystemVersion</key>
	<string>12.0</string>
	<key>LSUIElement</key>
	<true/>
	<key>NSHighResolutionCapable</key>
	<true/>
</dict>
</plist>
PLIST

echo "[ ==> ] Ad-hoc codesigning Daemon Bundle..."
codesign -s - --force --deep "${DAEMON_BUNDLE}"

# Convenient symlinks in /Applications for Spotlight / Launchpad.
# rm first: ln follows a stale symlinked dir and would nest the link
# inside the bundle (breaking codesign with "unsealed contents").
if [[ "${DEST_DIR}" == "/Applications/Antiknob" ]]; then
    rm -f "/Applications/Antiknob.app" "/Applications/AntiknobDaemon.app"
    ln -s "${APP_BUNDLE}" "/Applications/Antiknob.app"
    ln -s "${DAEMON_BUNDLE}" "/Applications/AntiknobDaemon.app"
fi

# Link CLIs if user has ~/.local/bin in PATH
if [[ -d "${HOME}/.local/bin" ]]; then
    ln -sf "${DEST_DIR}/bin/antiknob" "${HOME}/.local/bin/antiknob"
    ln -sf "${DEST_DIR}/bin/antiknob-daemon" "${HOME}/.local/bin/antiknob-daemon"
    echo "[ Ok  ] Symlinked CLIs to ${HOME}/.local/bin/antiknob{,-daemon}"
fi

echo "[ Ok  ] Antiknob installed successfully to ${DEST_DIR}!"
echo "        GUI App: ${APP_BUNDLE}"
echo "        Daemon App: ${DAEMON_BUNDLE} (menu bar, no dock icon)"
echo "        CLI tool: ${DEST_DIR}/bin/antiknob"
echo "        Start at login with: ${DEST_DIR}/bin/antiknob-daemon --install-login-item"
