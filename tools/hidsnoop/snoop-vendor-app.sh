#!/usr/bin/env bash
# Capture the vendor app's HID traffic.
#
# The only instrument that reliably answered a protocol question in this
# repo's history. A disassembly shows what the code COULD send; a capture
# shows what it DID send, in order, including the packets nobody thought to
# look for -- the LED init this firmware requires was sitting in the first
# capture, logged as an unremarkable "handshake", hours before anyone
# realised writes were being ignored without it.
#
# Works on a COPY. The vendor app ships with hardened runtime, which blocks
# both lldb and DYLD injection, so this re-signs a copy with the entitlements
# that permit them. Your installed app is never touched.
#
#   tools/hidsnoop/snoop-vendor-app.sh [/Applications/ANTICATER.app]
#
# Then drive the app; every hid_write and hid_read_timeout is logged.
set -euo pipefail

GOH="${GOH_DIR:-${GOH:-$HOME/Projects/gates_of_heck}}"
# shellcheck source=/dev/null
. "$GOH/tui/lib.sh"

APP="${1:-/Applications/ANTICATER.app}"
WORK="${TMPDIR:-/tmp}/hidsnoop.$$"
LOG="${HIDSNOOP_LOG:-${TMPDIR:-/tmp}/hidsnoop.txt}"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

[[ -d "$APP" ]] || die "no app bundle at $APP"
command -v clang >/dev/null || die "clang is required to build the interposer"

section "hid snoop"
mkdir -p "$WORK"
trap 'rm -rf "$WORK"' EXIT

info "copying $APP"
COPY="$WORK/app.app"
# ditto fails on the two libhidapi symlinks; recreate them afterwards.
ditto "$APP" "$COPY" 2>/dev/null || true
LIB="$(find "$COPY/Contents/Frameworks" -name 'libhidapi.*.dylib' -type f | head -1)"
[[ -n "$LIB" ]] || die "no bundled libhidapi found; is hidapi statically linked?"
( cd "$(dirname "$LIB")" && ln -sf "$(basename "$LIB")" libhidapi.0.dylib \
                          && ln -sf "$(basename "$LIB")" libhidapi.dylib )

info "building the x86_64 interposer"
clang -arch x86_64 -dynamiclib -o "$WORK/hidlog.dylib" "$HERE/hidlog.c" "$LIB"

info "re-signing the copy so injection is permitted"
cat > "$WORK/ent.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>com.apple.security.get-task-allow</key><true/>
  <key>com.apple.security.cs.disable-library-validation</key><true/>
</dict></plist>
PLIST
codesign -s - -f --entitlements "$WORK/ent.plist" "$WORK/hidlog.dylib" >/dev/null 2>&1
codesign -s - -f --deep --entitlements "$WORK/ent.plist" "$COPY" >/dev/null 2>&1

: > "$LOG"
ok "logging to $LOG"
info "drive the app now; ctrl-C here when done"
DYLD_INSERT_LIBRARIES="$WORK/hidlog.dylib" HIDSNOOP_LOG="$LOG" HIDLOG="$LOG" \
    "$COPY/Contents/MacOS/$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$COPY/Contents/Info.plist")"
