#!/bin/bash
# Render a simple app icon (blue rounded square + white butterfly-ish glyph)
# into AppIcon.icns using only stock macOS tools. Idempotent.
set -euo pipefail
OUT="$1" # target .icns path
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

ICONSET="$TMP/AppIcon.iconset"
mkdir -p "$ICONSET"

# Base 1024px PNG via an inline Swift/CoreGraphics snippet — no dependencies.
BASE="$TMP/base.png"
cat > "$TMP/icon.swift" <<'SWIFT'
import AppKit

let size = 1024
let img = NSImage(size: NSSize(width: size, height: size))
img.lockFocus()
let ctx = NSGraphicsContext.current!.cgContext

// Background: Bluesky blue rounded rect.
let rect = CGRect(x: 0, y: 0, width: size, height: size)
let path = CGPath(roundedRect: rect.insetBy(dx: 40, dy: 40),
                  cornerWidth: 180, cornerHeight: 180, transform: nil)
ctx.addPath(path)
ctx.setFillColor(CGColor(red: 0.04, green: 0.51, blue: 0.98, alpha: 1))
ctx.fillPath()

// A simple white butterfly glyph via two mirrored bezier lobes.
func lobe(_ mirror: Bool) -> CGPath {
    let p = CGMutablePath()
    let cx: CGFloat = 512
    let s: CGFloat = mirror ? -1 : 1
    p.move(to: CGPoint(x: cx, y: 640))
    p.addCurve(to: CGPoint(x: cx + s*70, y: 430),
               control1: CGPoint(x: cx + s*20, y: 560),
               control2: CGPoint(x: cx + s*70, y: 520))
    p.addCurve(to: CGPoint(x: cx + s*250, y: 560),
               control1: CGPoint(x: cx + s*70, y: 350),
               control2: CGPoint(x: cx + s*200, y: 470))
    p.addCurve(to: CGPoint(x: cx, y: 640),
               control1: CGPoint(x: cx + s*300, y: 630),
               control2: CGPoint(x: cx + s*140, y: 610))
    p.closeSubpath()
    return p
}
ctx.setFillColor(CGColor(red: 1, green: 1, blue: 1, alpha: 1))
ctx.addPath(lobe(false)); ctx.fillPath()
ctx.addPath(lobe(true)); ctx.fillPath()

img.unlockFocus()

let tiff = img.tiffRepresentation!
let bmp = NSBitmapImageRep(data: tiff)!
let png = bmp.representation(using: .png, properties: [:])!
try! png.write(to: URL(fileURLWithPath: CommandLine.arguments[1]))
SWIFT
swift "$TMP/icon.swift" "$BASE"

for sz in 16 32 64 128 256 512; do
    sips -z $sz $sz "$BASE" --out "$ICONSET/icon_${sz}x${sz}.png" >/dev/null
    d=$((sz*2))
    sips -z $d $d "$BASE" --out "$ICONSET/icon_${sz}x${sz}@2x.png" >/dev/null
done
sips -z 1024 1024 "$BASE" --out "$ICONSET/icon_512x512@2x.png" >/dev/null

iconutil -c icns "$ICONSET" -o "$OUT"
echo "wrote $OUT"
