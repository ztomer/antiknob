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
# The egui GUI was removed in 0.5.0; sweep it out of an earlier install.
rm -f "${DEST_DIR}/bin/antiknob-gui"
cp "${TARGET_DIR}/release/antiknob" "${DEST_DIR}/bin/antiknob"
cp "${TARGET_DIR}/release/antiknob-daemon" "${DEST_DIR}/bin/antiknob-daemon"
codesign -s - --force "${DEST_DIR}/bin/antiknob" "${DEST_DIR}/bin/antiknob-daemon"
# Never overwrite a live config: install the starter only when missing.
if [[ ! -f "${DEST_DIR}/config.yaml" ]]; then
    cp config.yaml "${DEST_DIR}/config.yaml"
else
    echo "[ --- ] Keeping existing ${DEST_DIR}/config.yaml (repo copy differs; diff to review)."
fi

# Build native macOS SwiftUI Application Bundle
echo "[ ==> ] Building native SwiftUI Antiknob.app..."
"${SCRIPT_DIR}/ui/build.sh"
APP_BUNDLE="${DEST_DIR}/Antiknob.app"
rm -rf "${APP_BUNDLE}"
cp -R "${SCRIPT_DIR}/Antiknob.app" "${APP_BUNDLE}"

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
if [[ -f "assets/ak12-1024.png" ]]; then
    cp "assets/ak12-1024.png" "${DAEMON_BUNDLE}/Contents/Resources/ak12-1024.png"
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
	<string>26.0</string>
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

# The binary was just replaced under a daemon that is still running the old
# one, so restart it. Without this the user keeps the previous build until
# they log out, which makes a fix look like it did not take.
LOGIN_ITEM="com.antiknob.daemon"
if launchctl print "gui/$(id -u)/${LOGIN_ITEM}" >/dev/null 2>&1; then
    echo "[ ==> ] Restarting the login-item daemon on the new binary..."
    launchctl kickstart -k "gui/$(id -u)/${LOGIN_ITEM}" >/dev/null 2>&1 || true
    sleep 2
fi

# Then ASK THE DAEMON whether its keyboard tap came up, rather than testing
# from this shell. Those are different questions: TCC grants an event tap by
# the launching context, so a check run from a granted Terminal reports
# success while the launchd-launched daemon still cannot tap -- silent in
# exactly the case the warning exists for. The daemon's own status is the
# only answer that is about the daemon.
TAP_STATE="$(printf '{"jsonrpc":"2.0","id":1,"method":"get_status","params":{}}\n' \
    | nc -U /tmp/antiknob.sock 2>/dev/null | head -1 || true)"
case "${TAP_STATE}" in
    *'"tap_active":true'*) ;;   # tapping fine, say nothing
    *)
        echo
        echo "[ --- ] The daemon cannot translate knob gestures yet: its keyboard"
        echo "        tap needs Accessibility. System Settings > Privacy & Security"
        echo "        > Accessibility, then add:"
        echo "          ${DEST_DIR}/bin/antiknob-daemon"
        echo "        (Hardware flashing and LED control work without it.)"
        ;;
esac
