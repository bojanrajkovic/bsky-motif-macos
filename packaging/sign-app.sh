#!/bin/bash
# Sign BskyMotif.app with a Developer ID for distribution (hardened runtime +
# secure timestamp), inside-out. Optionally notarize + staple.
#
#   bash packaging/sign-app.sh [path/to/BskyMotif.app]
#
# Identity: $SIGN_IDENTITY, else the first "Developer ID Application" in the
#   keychain. First run may prompt to allow codesign to use the signing key —
#   click "Always Allow" so future runs are non-interactive.
# Notarize: set NOTARY_PROFILE to a `notarytool store-credentials` profile name.
set -euo pipefail

APP="${1:-dist/BskyMotif.app}"
FRAMEWORKS="$APP/Contents/Frameworks"

IDENTITY="${SIGN_IDENTITY:-}"
if [ -z "$IDENTITY" ]; then
    IDENTITY="$(security find-identity -v -p codesigning \
        | grep "Developer ID Application" | head -1 \
        | sed -E 's/.*"(.*)"/\1/')"
fi
if [ -z "$IDENTITY" ]; then
    echo "No 'Developer ID Application' identity found. Set SIGN_IDENTITY." >&2
    exit 1
fi
echo ">> identity: $IDENTITY"

sign() { codesign --force --options runtime --timestamp --sign "$IDENTITY" "$@"; }

# Inside-out: nested dylibs first (all our own team → library validation passes,
# no entitlements needed), then the bundle, which seals the main executable.
if [ -d "$FRAMEWORKS" ]; then
    find "$FRAMEWORKS" -type f -name "*.dylib" -print0 \
        | while IFS= read -r -d '' lib; do sign "$lib"; done
fi
sign "$APP"

echo ">> verifying"
codesign --verify --deep --strict --verbose=2 "$APP"
codesign -dv --verbose=4 "$APP" 2>&1 | grep -E "Authority=|TeamIdentifier=|Timestamp=|flags=" || true
echo ">> Gatekeeper assessment:"
spctl -a -vvv "$APP" 2>&1 || echo "(not notarized yet — Gatekeeper will reject until notarization)"

if [ -n "${NOTARY_PROFILE:-}" ]; then
    echo ">> notarizing via profile: $NOTARY_PROFILE"
    ZIP="${APP%.app}-notarize.zip"
    ditto -c -k --keepParent "$APP" "$ZIP"
    xcrun notarytool submit "$ZIP" --keychain-profile "$NOTARY_PROFILE" --wait
    xcrun stapler staple "$APP"
    rm -f "$ZIP"
    echo ">> stapled; re-checking Gatekeeper:"
    spctl -a -vvv "$APP"
fi

echo ">> done"
