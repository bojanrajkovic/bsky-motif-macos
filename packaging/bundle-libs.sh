#!/bin/bash
# Vendor the Homebrew dylib closure into BskyMotif.app so it runs on any Mac
# with XQuartz — no Homebrew needed. Rewrites every /opt/homebrew load command
# to @executable_path/../Frameworks. (XQuartz's X *server* is still required;
# these are just the client libs.)
set -euo pipefail

APP="$1"
BIN="$APP/Contents/MacOS/BskyMotif"
FRAMEWORKS="$APP/Contents/Frameworks"
mkdir -p "$FRAMEWORKS"

# BFS the closure: every /opt/homebrew dylib reachable from the binary.
# (macOS ships Bash 3.2 — no associative arrays; a space-padded string is the
# membership set.)
seen=" "
libs=()
queue=("$BIN")
while [ ${#queue[@]} -gt 0 ]; do
    cur="${queue[0]}"; queue=("${queue[@]:1}")
    while read -r dep; do
        [ -z "$dep" ] && continue
        case "$dep" in /opt/homebrew/*) ;; *) continue ;; esac
        base="$(basename "$dep")"
        case "$seen" in *" $base "*) continue ;; esac
        seen="$seen$base "
        real="$(python3 -c 'import os,sys;print(os.path.realpath(sys.argv[1]))' "$dep")"
        libs+=("$base")
        cp -f "$real" "$FRAMEWORKS/$base"
        chmod u+w "$FRAMEWORKS/$base"
        queue+=("$real")
    done < <(otool -L "$cur" | tail -n +2 | awk '{print $1}')
done
echo ">> vendored ${#libs[@]} dylibs"

# Rewrite install names. For each Mach-O (binary + every vendored lib), point
# every /opt/homebrew dependency at the bundled copy; give libs a bundle id.
rewrite() {
    local file="$1" is_lib="$2" base
    if [ "$is_lib" = lib ]; then
        base="$(basename "$file")"
        install_name_tool -id "@executable_path/../Frameworks/$base" "$file"
    fi
    while read -r dep; do
        case "$dep" in /opt/homebrew/*) ;; *) continue ;; esac
        install_name_tool -change "$dep" \
            "@executable_path/../Frameworks/$(basename "$dep")" "$file"
    done < <(otool -L "$file" | tail -n +2 | awk '{print $1}')
}

rewrite "$BIN" bin
for base in "${libs[@]}"; do
    rewrite "$FRAMEWORKS/$base" lib
done

# Self-contained fontconfig config so the vendored libfontconfig finds fonts
# without Homebrew's compiled-in /opt/homebrew/etc/fonts path.
mkdir -p "$APP/Contents/Resources/fontconfig"
cat > "$APP/Contents/Resources/fontconfig/fonts.conf" <<'CONF'
<?xml version="1.0"?>
<!DOCTYPE fontconfig SYSTEM "urn:fontconfig:fonts.dtd">
<fontconfig>
  <dir>/System/Library/Fonts</dir>
  <dir>/System/Library/Fonts/Supplemental</dir>
  <dir>/Library/Fonts</dir>
  <dir>~/Library/Fonts</dir>
  <cachedir>~/.cache/bsky-motif/fontconfig</cachedir>
  <config></config>
</fontconfig>
CONF

echo ">> install names rewritten, fonts.conf written"
