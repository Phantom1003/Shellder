#!/bin/bash
# Create a local self-signed code-signing certificate ("shellder local signing")
# in the login keychain. build.sh signs with it when present, so the app keeps
# the same identity across rebuilds and the keychain's "Always Allow" sticks.
set -euo pipefail
NAME="shellder local signing"
KC="$HOME/Library/Keychains/login.keychain-db"
if security find-certificate -c "$NAME" "$KC" >/dev/null 2>&1; then
    echo "'$NAME' already exists in the login keychain"; exit 0
fi
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
PASS=$(head -c 16 /dev/urandom | xxd -p)
openssl req -x509 -newkey rsa:2048 -sha256 -days 3650 -nodes \
    -keyout "$TMP/k.pem" -out "$TMP/c.pem" -subj "/CN=$NAME" \
    -addext "keyUsage=critical,digitalSignature" -addext "extendedKeyUsage=codeSigning" \
    -addext "basicConstraints=critical,CA:false" 2>/dev/null
openssl pkcs12 -export -inkey "$TMP/k.pem" -in "$TMP/c.pem" -out "$TMP/id.p12" -passout "pass:$PASS" -name "$NAME"
security import "$TMP/id.p12" -k "$KC" -P "$PASS" -T /usr/bin/codesign -T /usr/bin/security
echo "created '$NAME'; rebuild with ./build.sh"
