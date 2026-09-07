#!/usr/bin/env bash
# Create the local "Antiknob Dev" code-signing identity, once per machine.
#
# Why a certificate at all, when ad-hoc signing is free: TCC remembers an
# Accessibility grant against the binary's DESIGNATED REQUIREMENT, and an
# ad-hoc signature's requirement is `cdhash H"..."` -- the exact bits. Every
# rebuild is then a program macOS has never seen, so a reinstall silently
# voids a switch that still reads as on, and the daemon goes deaf with no
# way for the user to tell that anything changed. A certificate moves the
# requirement to `identifier "..." and certificate leaf = H"..."`, which two
# different builds share.
#
# The certificate is self-signed and deliberately NOT added to the trust
# store: `codesign` signs with it regardless, and the requirement above
# pins the leaf's own hash, so trusting it as a root would buy nothing and
# cost a change to the user's trust settings.
set -euo pipefail

GOH="${GOH_DIR:-${GOH:-$HOME/Projects/gates_of_heck}}"
# shellcheck source=/dev/null
. "$GOH/tui/lib.sh"

CERT_NAME="${ANTIKNOB_CERT_NAME:-Antiknob Dev}"
KEYCHAIN="${HOME}/Library/Keychains/login.keychain-db"
DAYS=3650

section "signing identity"

if security find-identity -p codesigning | grep -qF "\"${CERT_NAME}\""; then
    ok "${CERT_NAME} already exists"
    security find-identity -p codesigning | grep -F "\"${CERT_NAME}\""
    exit 0
fi

WORK="$(mktemp -d)"
trap 'rm -rf "${WORK}"' EXIT

info "generating a self-signed code-signing certificate"
cat > "${WORK}/req.cnf" <<EOF
[ req ]
distinguished_name = dn
x509_extensions    = v3
prompt             = no
default_md         = sha256

[ dn ]
CN = ${CERT_NAME}

[ v3 ]
basicConstraints     = critical,CA:FALSE
keyUsage             = critical,digitalSignature
extendedKeyUsage     = critical,codeSigning
subjectKeyIdentifier = hash
EOF

openssl req -x509 -newkey rsa:2048 -sha256 -days "${DAYS}" -nodes \
    -keyout "${WORK}/key.pem" -out "${WORK}/cert.pem" \
    -config "${WORK}/req.cnf" >/dev/null 2>&1

# -legacy plus the SHA-1/3DES algorithms: OpenSSL 3 defaults to AES-256 and
# a SHA-256 MAC, which Apple's importer rejects with "MAC verification
# failed (wrong password?)" -- a message about the wrong thing entirely.
PASS="$(openssl rand -hex 16)"
openssl pkcs12 -export -legacy \
    -inkey "${WORK}/key.pem" -in "${WORK}/cert.pem" -name "${CERT_NAME}" \
    -certpbe PBE-SHA1-3DES -keypbe PBE-SHA1-3DES -macalg sha1 \
    -out "${WORK}/bundle.p12" -passout "pass:${PASS}" >/dev/null 2>&1

info "importing into the login keychain"
# -A lets codesign use the key unattended. The key signs local development
# builds and nothing else, so the alternative -- a GUI prompt on every
# build -- costs more than it protects.
security import "${WORK}/bundle.p12" -k "${KEYCHAIN}" -P "${PASS}" \
    -T /usr/bin/codesign -A >/dev/null

if ! security find-identity -p codesigning | grep -qF "\"${CERT_NAME}\""; then
    die "import reported success but ${CERT_NAME} is not in the keychain"
fi

ok "created ${CERT_NAME}"
security find-identity -p codesigning | grep -F "\"${CERT_NAME}\""
info "./install.sh now signs with it; nothing else to do"
