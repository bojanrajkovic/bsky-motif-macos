#!/bin/bash
# Assemble BskyMotif.app from a release build. Run from the repo root.
# Ad-hoc signs by default (fast, no prompts); run sign-app.sh for a Developer
# ID signed + notarizable build.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="$ROOT/dist/BskyMotif.app"
CONTENTS="$APP/Contents"

echo ">> cargo build --release"
cargo build --release --manifest-path "$ROOT/Cargo.toml"

echo ">> assembling $APP"
rm -rf "$APP"
mkdir -p "$CONTENTS/MacOS" "$CONTENTS/Resources"

cp "$ROOT/packaging/Info.plist" "$CONTENTS/Info.plist"
# The Rust binary IS the main executable — it self-bootstraps XQuartz. No
# launcher script (a script can't carry a hardened runtime for notarization).
cp "$ROOT/target/release/bsky-motif" "$CONTENTS/MacOS/BskyMotif"
chmod +x "$CONTENTS/MacOS/BskyMotif"

echo ">> rendering icon"
bash "$ROOT/packaging/make-icon.sh" "$CONTENTS/Resources/AppIcon.icns"

echo ">> vendoring dylibs"
bash "$ROOT/packaging/bundle-libs.sh" "$APP"

# Ad-hoc sign LAST — install_name_tool invalidates signatures. For distribution
# run: bash packaging/sign-app.sh "$APP"
echo ">> ad-hoc signing"
codesign --force --sign - "$CONTENTS/Frameworks"/*.dylib 2>/dev/null || true
codesign --force --sign - "$APP" 2>/dev/null || echo "(codesign skipped)"

echo ">> done: $APP"
