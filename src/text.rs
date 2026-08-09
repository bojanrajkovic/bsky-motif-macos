//! Self-rendered Unicode/emoji text → X pixmaps, via Xft directly.
//!
//! openmotif 2.3.8's XmString rendering draws UTF-8 bytes one-glyph-per-byte
//! (mojibake) no matter how the string is tagged, so any text that can carry
//! Unicode — post bodies, display names — is drawn here instead: measured and
//! painted with Xft, with a two-font fallback (a broad Unicode text face, then
//! a color-emoji face) so CJK, symbols, accents, and emoji all render.

use crate::ui::Gfx;
use crate::motif::*;
use std::os::raw::c_int;

const BG: (u8, u8, u8) = (0xd4, 0xd0, 0xc8);
const PAD: i32 = 1;

#[derive(Clone, Copy, PartialEq)]
pub enum Weight {
    Normal,
    Bold,
}

/// A text face: a primary Unicode font plus an emoji fallback, at one size.
struct Face {
    primary: XftFontPtr,
    emoji: XftFontPtr,
}

pub struct TextRenderer {
    dpy: *mut Display,
    visual: *mut Visual,
    colormap: Colormap,
    depth: u32,
    root: XWindow,
    gc: GC,
    bg_pixel: Pixel,
    normal: Face,
    bold: Face,
}

unsafe fn open(dpy: *mut Display, screen: c_int, pattern: &std::ffi::CStr) -> XftFontPtr {
    XftFontOpenName(dpy, screen, pattern.as_ptr())
}

impl TextRenderer {
    pub unsafe fn new(gfx: &Gfx) -> TextRenderer {
        let s = gfx.screen;
        let d = gfx.display;
        // Arial Unicode MS: one face covering Latin/CJK/Greek/Cyrillic/symbols.
        // Apple Color Emoji: the emoji fallback (color, via libXft FT_Color).
        let face = |primary: &std::ffi::CStr, emoji: &std::ffi::CStr| Face {
            primary: open(d, s, primary),
            emoji: open(d, s, emoji),
        };
        TextRenderer {
            dpy: d,
            visual: gfx.visual,
            colormap: gfx.colormap,
            depth: gfx.depth,
            root: gfx.root,
            gc: gfx.gc,
            bg_pixel: gfx.pixel(BG),
            // Comma family lists so fontconfig picks the best available per OS:
            // macOS lands on Arial Unicode MS + Apple Color Emoji; Linux falls
            // through to Noto/DejaVu.
            normal: face(
                c"Arial Unicode MS,Noto Sans CJK SC,DejaVu Sans,sans-serif:pixelsize=13",
                c"Apple Color Emoji,Noto Color Emoji,emoji:pixelsize=13",
            ),
            bold: face(
                c"Arial Unicode MS,Noto Sans CJK SC,DejaVu Sans,sans-serif:bold:pixelsize=13",
                c"Apple Color Emoji,Noto Color Emoji,emoji:pixelsize=13",
            ),
        }
    }

    fn face(&self, weight: Weight) -> &Face {
        match weight {
            Weight::Normal => &self.normal,
            Weight::Bold => &self.bold,
        }
    }

    /// Pick the font that has a glyph for `ch`: primary, else emoji, else
    /// primary (which draws its notdef box — honest about the gap).
    unsafe fn font_for(&self, face: &Face, ch: u32) -> XftFontPtr {
        if XftCharExists(self.dpy, face.primary, ch) != 0 {
            face.primary
        } else if !face.emoji.is_null() && XftCharExists(self.dpy, face.emoji, ch) != 0 {
            face.emoji
        } else {
            face.primary
        }
    }

    unsafe fn run_width(&self, font: XftFontPtr, cps: &[u32]) -> i32 {
        if cps.is_empty() {
            return 0;
        }
        let mut gi: XGlyphInfo = std::mem::zeroed();
        XftTextExtents32(self.dpy, font, cps.as_ptr(), cps.len() as c_int, &mut gi);
        gi.x_off as i32
    }

    /// Split a line into (font, codepoints) runs by glyph coverage.
    unsafe fn runs(&self, face: &Face, line: &str) -> Vec<(XftFontPtr, Vec<u32>)> {
        let mut runs: Vec<(XftFontPtr, Vec<u32>)> = Vec::new();
        for ch in line.chars() {
            let cp = ch as u32;
            let font = self.font_for(face, cp);
            match runs.last_mut() {
                Some((f, cps)) if *f == font => cps.push(cp),
                _ => runs.push((font, vec![cp])),
            }
        }
        runs
    }

    /// Render pre-wrapped `lines` into a pixmap. Returns (pixmap, w, h); the
    /// caller owns the pixmap and must XFreePixmap it. `(0,_,_)` if empty.
    pub unsafe fn render(
        &self,
        lines: &[String],
        weight: Weight,
        color: (u8, u8, u8),
    ) -> (Pixmap, u32, u32) {
        let face = self.face(weight);
        let ascent = (*face.primary).ascent;
        let descent = (*face.primary).descent;
        let line_h = ascent + descent + 3;

        // Measure + cache runs.
        let mut laid: Vec<Vec<(XftFontPtr, Vec<u32>)>> = Vec::with_capacity(lines.len());
        let mut max_w = 1i32;
        for line in lines {
            let runs = self.runs(face, line);
            let w: i32 = runs.iter().map(|(f, cps)| self.run_width(*f, cps)).sum();
            max_w = max_w.max(w);
            laid.push(runs);
        }
        let width = (max_w + PAD * 2).max(1) as u32;
        let height = (line_h * laid.len().max(1) as i32 + PAD * 2).max(1) as u32;

        let pixmap = XCreatePixmap(self.dpy, self.root, width, height, self.depth);
        XSetForeground(self.dpy, self.gc, self.bg_pixel);
        XFillRectangle(self.dpy, pixmap, self.gc, 0, 0, width, height);

        let draw = XftDrawCreate(self.dpy, pixmap, self.visual, self.colormap);
        let mut xcolor: XftColor = std::mem::zeroed();
        let render_color = XRenderColor {
            red: (color.0 as u16) << 8 | color.0 as u16,
            green: (color.1 as u16) << 8 | color.1 as u16,
            blue: (color.2 as u16) << 8 | color.2 as u16,
            alpha: 0xffff,
        };
        XftColorAllocValue(self.dpy, self.visual, self.colormap, &render_color, &mut xcolor);

        for (row, runs) in laid.iter().enumerate() {
            let baseline = PAD + row as i32 * line_h + ascent;
            let mut x = PAD;
            for (font, cps) in runs {
                XftDrawString32(draw, &xcolor, *font, x, baseline, cps.as_ptr(), cps.len() as c_int);
                x += self.run_width(*font, cps);
            }
        }

        XftColorFree(self.dpy, self.visual, self.colormap, &mut xcolor);
        XftDrawDestroy(draw);
        (pixmap, width, height)
    }
}
