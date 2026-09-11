#!/usr/bin/env bash
# Install Antiknob CLI, GUI app, and menu-bar daemon to /Applications/Antiknob
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${SCRIPT_DIR}"

DEST_DIR="${1:-/Applications/Antiknob}"
TARGET_DIR="$(cargo metadata --format-version 1 | python3 -c 'import json, sys; print(json.load(sys.stdin)["target_directory"])')"
PKG_VERSION="$(cargo metadata --format-version 1 | python3 -c 'import json, sys; print([p["version"] for p in json.load(sys.stdin)["packages"] if p["name"] == "antiknob"][0])')"
PKG_BUILD="$(git rev-list --count HEAD)"

# TCC remembers an Accessibility grant against the binary's designated
# requirement. Ad-hoc signing makes that requirement `cdhash H"..."` -- the
# exact bits -- so every reinstall is a program macOS has never seen and the
# grant the user already gave is silently void while the switch still reads
# as on. A certificate makes it `identifier "..." and certificate leaf =
# H"..."`, which survives a rebuild. `tools/make_signing_cert.sh` creates one.
#
# Resolved by SHA-1 hash, and WITHOUT `find-identity -v`: the certificate is
# self-signed and untrusted on purpose (codesign does not care, and the
# requirement pins the leaf itself), so `-v` filters out the very identity
# this is looking for.
SIGN_ID="${ANTIKNOB_SIGN_ID:-}"
if [[ -z "${SIGN_ID}" ]]; then
    SIGN_ID="$(security find-identity -p codesigning 2>/dev/null \
        | awk '/"Antiknob Dev"/ {print $2; exit}')"
fi
SIGN_ID="${SIGN_ID:--}"

echo "[ ==> ] Building release binaries (arm64)..."
cargo build --release

echo "[ ==> ] Installing to ${DEST_DIR}..."
mkdir -p "${DEST_DIR}/bin"
# The egui GUI was removed in 0.5.0; sweep it out of an earlier install.
rm -f "${DEST_DIR}/bin/antiknob-gui"
cp "${TARGET_DIR}/release/antiknob" "${DEST_DIR}/bin/antiknob"
cp "${TARGET_DIR}/release/antiknob-daemon" "${DEST_DIR}/bin/antiknob-daemon"
codesign -s "${SIGN_ID}" -i com.antiknob.cli --force "${DEST_DIR}/bin/antiknob"
codesign -s "${SIGN_ID}" -i com.antiknob.daemon.cli --force "${DEST_DIR}/bin/antiknob-daemon"
# The layout lives in Application Support beside host.json and the socket,
# and the binary seeds it from a compiled-in copy on first use. A copy under
# ${DEST_DIR} was never a config location: nothing resolved it, so it only
# worked if the user happened to cd there first. Sweep an old one out, after
# migrating it if the real location is still empty.
LAYOUT_DIR="${HOME}/Library/Application Support/antiknob"
if [[ -f "${DEST_DIR}/config.yaml" ]]; then
    if [[ ! -f "${LAYOUT_DIR}/config.yaml" ]]; then
        mkdir -p "${LAYOUT_DIR}"
        mv "${DEST_DIR}/config.yaml" "${LAYOUT_DIR}/config.yaml"
        echo "[ ==> ] Moved your layout to ${LAYOUT_DIR}/config.yaml"
    else
        rm -f "${DEST_DIR}/config.yaml"
        echo "[ --- ] Removed the unused ${DEST_DIR}/config.yaml (yours is in ${LAYOUT_DIR})."
    fi
fi

# Build native macOS SwiftUI Application Bundle
echo "[ ==> ] Building native SwiftUI Antiknob.app..."
ANTIKNOB_SIGN_ID="${SIGN_ID}" "${SCRIPT_DIR}/ui/build.sh"
APP_BUNDLE="${DEST_DIR}/Antiknob.app"
rm -rf "${APP_BUNDLE}"
cp -R "${SCRIPT_DIR}/Antiknob.app" "${APP_BUNDLE}"

# Assemble menu-bar daemon bundle (LSUIElement: tray icon, no dock icon)
DAEMON_BUNDLE="${DEST_DIR}/AntiknobDaemon.app"
DAEMON_EXE="${DAEMON_BUNDLE}/Contents/MacOS/AntiknobDaemon"
echo "[ ==> ] Creating daemon App Bundle at ${DAEMON_BUNDLE}..."
rm -rf "${DAEMON_BUNDLE}"
mkdir -p "${DAEMON_BUNDLE}/Contents/MacOS"
mkdir -p "${DAEMON_BUNDLE}/Contents/Resources"

cp "${DEST_DIR}/bin/antiknob-daemon" "${DAEMON_BUNDLE}/Contents/MacOS/AntiknobDaemon"
chmod +x "${DAEMON_BUNDLE}/Contents/MacOS/AntiknobDaemon"

if [[ -f "assets/Antiknob.icns" ]]; then
    cp "assets/Antiknob.icns" "${DAEMON_BUNDLE}/Contents/Resources/Antiknob.icns"
fi
if [[ -f "assets/antiknob-1024.png" ]]; then
    cp "assets/antiknob-1024.png" "${DAEMON_BUNDLE}/Contents/Resources/antiknob-1024.png"
    cp "assets/antiknob-1024.png" "${DAEMON_BUNDLE}/Contents/Resources/ak12-1024.png"
elif [[ -f "assets/ak12-1024.png" ]]; then
    cp "assets/ak12-1024.png" "${DAEMON_BUNDLE}/Contents/Resources/ak12-1024.png"
fi
if [[ -f "assets/knob-tray.png" ]]; then
    cp "assets/knob-tray.png" "${DAEMON_BUNDLE}/Contents/Resources/knob-tray.png"
    cp "assets/knob-tray.png" "${DAEMON_BUNDLE}/Contents/Resources/ak12-tray.png"
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

echo "[ ==> ] Codesigning Daemon Bundle (identity: ${SIGN_ID})..."
codesign -s "${SIGN_ID}" --force --deep "${DAEMON_BUNDLE}"

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
echo "        Start at login with: ${DAEMON_EXE} --install-login-item"

# The binary was just replaced under a daemon that is still running the old
# one, so restart it. Without this the user keeps the previous build until
# they log out, which makes a fix look like it did not take.
#
# Reinstall the agent rather than kickstarting it: through 0.8.1 it started
# ${DEST_DIR}/bin/antiknob-daemon, a loose executable that the Accessibility
# pane will not let anyone add, so the grant had nowhere to land. The agent
# now starts the bundle, which the pane lists by name.
LOGIN_ITEM="com.antiknob.daemon"
if launchctl print "gui/$(id -u)/${LOGIN_ITEM}" >/dev/null 2>&1; then
    echo "[ ==> ] Restarting the login-item daemon on the new build..."
    "${DAEMON_EXE}" --install-login-item >/dev/null
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
    "")
        # No answer at all is a different problem from a refused tap, and
        # telling the user to go and flip a switch would send them to fix
        # something that is not broken.
        echo
        echo "[ --- ] The daemon is not running. Start it at login with:"
        echo "          ${DAEMON_EXE} --install-login-item"
        ;;
    *)
        echo
        echo "[ --- ] Knob gestures stay off until macOS lets AntiknobDaemon read the keyboard:"
        echo "        System Settings > Privacy & Security > Accessibility → switch on AntiknobDaemon"
        echo "        Not listed? Click +, then add ${DAEMON_BUNDLE}"
        if [[ "${SIGN_ID}" == "-" ]]; then
            echo "        (Ad-hoc signed: macOS reads each reinstall as a new program, so an"
            echo "        already-granted switch must be toggled off and on again. Run"
            echo "        ./tools/make_signing_cert.sh once to stop that.)"
        fi
        ;;
esac
