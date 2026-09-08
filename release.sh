#!/usr/bin/env bash
# Cut an Antiknob release: build, package the dmg, publish it, and bump the
# Homebrew cask — one command so the tap can never go stale again.
#
# Usage: ./release.sh v0.13.0
#
# The version bump itself stays a separate commit made beforehand ("release:
# 0.13.0" or similar): Cargo.toml must already carry the tag's version and
# the tree must be clean, so the tag points at exactly what was tested.
#
# What it does:
#   1. preconditions (clean tree, on main, main pushed, version match,
#      gh + hdiutil + rsync present)
#   2. repo gate (tools/gate.sh --full)
#   3. ./install.sh — rebuilds and refreshes the live /Applications install,
#      which is also what gets packaged, so release bits == live bits.
#      (Packaging the live install avoids a staging dir whose side effects —
#      repointed ~/.local/bin symlinks, a restarted daemon — would otherwise
#      need undoing.)
#   4. stage /Applications/Antiknob (minus .DS_Store) and build the dmg.
#   5. tag, push the tag, create the GitHub release, upload the dmg.
#   6. bump Casks/antiknob.rb in ztomer/homebrew-tap via the gh API.
#   7. prove it: brew update the tap and fetch the cask.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${SCRIPT_DIR}"

TAG="${1:?Usage: ./release.sh vX.Y.Z}"
case "${TAG}" in
  v[0-9]*.[0-9]*.[0-9]*) ;;
  *) echo "error: tag must look like vMAJOR.MINOR.PATCH (got '${TAG}')" >&2; exit 1 ;;
esac
VER="${TAG#v}"

REPO="ztomer/antiknob"
TAP="ztomer/homebrew-tap"
CASK_PATH="Casks/antiknob.rb"
APP_SRC="/Applications/Antiknob"
DMG_NAME="Antiknob-v${VER}-aarch64.dmg"

die() { echo "error: $*" >&2; exit 1; }
info() { echo "→ $*"; }
ok() { echo "✓ $*"; }

command -v gh >/dev/null || die "gh CLI required (brew install gh)"
command -v hdiutil >/dev/null || die "hdiutil required (macOS only)"
command -v rsync >/dev/null || die "rsync required"

git diff --quiet || die "working tree is dirty — commit or stash first"
git diff --cached --quiet || die "staged changes present — commit first"
[ "$(git branch --show-current)" = "main" ] || die "not on main"
[ -z "$(git log --oneline origin/main..HEAD 2>/dev/null)" ] || die "main has unpushed commits — push first"
git rev-parse "${TAG}" >/dev/null 2>&1 && die "tag ${TAG} already exists"

CARGO_V="$(grep -m1 '^version = ' Cargo.toml | cut -d'"' -f2)"
[ "${CARGO_V}" = "${VER}" ] || die "Cargo.toml is ${CARGO_V}, tag wants ${VER} — bump it in a commit first"

info "running the repo gate ..."
./tools/gate.sh --full >/dev/null || die "gate failed — nothing tagged, nothing pushed"
ok "gate green"

info "rebuilding and refreshing the live install ..."
./install.sh

for f in "${APP_SRC}/Antiknob.app" "${APP_SRC}/AntiknobDaemon.app" \
         "${APP_SRC}/bin/antiknob" "${APP_SRC}/bin/antiknob-daemon"; do
  [ -e "${f}" ] || die "expected ${f} after install.sh — aborting"
done

STAGE="$(mktemp -d)"
trap 'rm -rf "${STAGE}"' EXIT
rsync -a --exclude=.DS_Store "${APP_SRC}/" "${STAGE}/"

DMG="${STAGE}/${DMG_NAME}"
info "building ${DMG_NAME} ..."
hdiutil create -volname "Antiknob" -srcfolder "${STAGE}" -ov -format UDZO "${DMG}" >/dev/null
SHA="$(shasum -a 256 "${DMG}" | cut -d' ' -f1)"
info "dmg sha256: ${SHA}"

info "tagging ${TAG} ..."
git tag -a "${TAG}" -m "Release ${TAG}"
git push origin "${TAG}"
ok "pushed ${TAG}"

info "creating the GitHub release ..."
gh release create "${TAG}" "${DMG}" --repo "${REPO}" --title "${TAG}" \
  --notes "Antiknob ${TAG} — native macOS configurator and menu-bar daemon for the Anticater VK01 knob. Install via Homebrew: brew install --cask ztomer/tap/antiknob"
ok "release published"

info "bumping ${TAP}/${CASK_PATH} → ${VER} ..."
CUR_SHA="$(gh api "repos/${TAP}/contents/${CASK_PATH}" --jq .sha)"
CUR_CASK="$(gh api "repos/${TAP}/contents/${CASK_PATH}" --jq .content | base64 --decode)"
NEW_CASK="$(printf '%s' "${CUR_CASK}" | VER="${VER}" SHA="${SHA}" python3 -c "
import os, re, sys
s = sys.stdin.read()
s = re.sub(r'version \"[^\"]+\"', 'version \"%s\"' % os.environ['VER'], s, count=1)
s = re.sub(r'sha256 \"[0-9a-f]+\"', 'sha256 \"%s\"' % os.environ['SHA'], s, count=1)
sys.stdout.write(s)
")"
if [ "${NEW_CASK}" = "${CUR_CASK}" ]; then
  die "tap transform produced no change — refusing to push an empty bump"
fi
printf '%s' "${NEW_CASK}" | ruby -c >/dev/null || die "transformed cask failed ruby -c"
gh api -X PUT "repos/${TAP}/contents/${CASK_PATH}" \
  -f message="antiknob ${VER}" \
  -f content="$(printf '%s' "${NEW_CASK}" | base64 | tr -d '\n')" \
  -f sha="${CUR_SHA}" >/dev/null
ok "cask updated"

info "proving it: brew update + fetch ..."
brew update >/dev/null 2>&1
brew fetch --cask "ztomer/tap/antiknob" --force >/dev/null 2>&1 || die "brew fetch failed after tap bump"
ok "released ${TAG}: ${DMG_NAME} verified through Homebrew"
