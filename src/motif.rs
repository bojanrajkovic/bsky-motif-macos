//! Handwritten FFI over Xt / Motif (Xm) / Xlib — just the surface this app uses.
//!
//! Constants below are transcribed from the installed openmotif 2.3.8 headers
//! (/opt/homebrew/include/Xm/Xm.h, X11/Intrinsic.h), not from memory.
//! Resource names are matched by content (XrmStringToQuark interns strings),
//! so plain &CStr constants are equivalent to the XmN* header macros.

#![allow(non_snake_case, non_upper_case_globals, dead_code)]

use std::ffi::CStr;
use std::os::raw::{c_char, c_int, c_long, c_short, c_uchar, c_uint, c_ulong, c_ushort, c_void};

pub type Widget = *mut c_void;
pub type WidgetClass = *mut c_void;
pub type XtAppContext = *mut c_void;
pub type XmString = *mut c_void;
pub type XtPointer = *mut c_void;
pub type XtArgVal = c_long;
pub type Boolean = c_uchar;
pub type Cardinal = c_uint;
pub type Dimension = c_ushort;
pub type XtInputId = c_ulong;
pub type XtIntervalId = c_ulong;
pub type Display = c_void;
pub type XWindow = c_ulong;
pub type Drawable = c_ulong;
pub type Pixmap = c_ulong;
pub type GC = *mut c_void;
pub type Visual = c_void;
pub type Pixel = c_ulong;
pub type XmRendition = *mut c_void;
pub type XmRenderTable = *mut c_void;

pub const FALSE: Boolean = 0;
pub const TRUE: Boolean = 1;

// enum{ XmFONT_IS_FONT, XmFONT_IS_FONTSET, XmFONT_IS_XFT }
pub const XmFONT_IS_XFT: XtArgVal = 2;
// enum{ XmUNSPECIFIED_LOAD_MODEL, XmLOAD_DEFERRED, XmLOAD_IMMEDIATE }
pub const XmLOAD_IMMEDIATE: XtArgVal = 2;
// enum XmMergeMode{ XmSKIP, XmMERGE_REPLACE, XmMERGE_OLD, XmMERGE_NEW, XmDUPLICATE }
pub const XmMERGE_REPLACE: c_int = 1;
// enum XmTextType{ XmCHARSET_TEXT, XmMULTIBYTE_TEXT, XmWIDECHAR_TEXT, XmNO_TEXT }
pub const XmCHARSET_TEXT: c_int = 0;

// enum{ XmATTACH_NONE, XmATTACH_FORM, XmATTACH_OPPOSITE_FORM, XmATTACH_WIDGET,
//       XmATTACH_OPPOSITE_WIDGET, XmATTACH_POSITION, XmATTACH_SELF }
pub const XmATTACH_NONE: XtArgVal = 0;
pub const XmATTACH_FORM: XtArgVal = 1;
pub const XmATTACH_WIDGET: XtArgVal = 3;
pub const XmATTACH_OPPOSITE_WIDGET: XtArgVal = 4;
pub const XmATTACH_POSITION: XtArgVal = 5;

// enum{ XmPIXMAP = 1, XmSTRING, XmPIXMAP_AND_STRING }
pub const XmPIXMAP: XtArgVal = 1;
pub const XmSTRING: XtArgVal = 2;

// enum{ XmNO_ORIENTATION, XmVERTICAL, XmHORIZONTAL }
pub const XmVERTICAL: XtArgVal = 1;
pub const XmHORIZONTAL: XtArgVal = 2;

// enum{ XmAUTOMATIC, XmAPPLICATION_DEFINED }
pub const XmAUTOMATIC: XtArgVal = 0;
// enum{ /* XmSTATIC */ XmAS_NEEDED = 1 }
pub const XmAS_NEEDED: XtArgVal = 1;

// enum{ XmALIGNMENT_BEGINNING, XmALIGNMENT_CENTER, XmALIGNMENT_END }
pub const XmALIGNMENT_BEGINNING: XtArgVal = 0;
pub const XmALIGNMENT_CENTER: XtArgVal = 1;

pub const XtInputReadMask: XtPointer = 1 as XtPointer; // (1L<<0)
pub const ZPixmap: c_int = 2;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Arg {
    pub name: *const c_char,
    pub value: XtArgVal,
}

// Layout from Xm/Xm.h (openmotif 2.3.8); `format` is trailing and never read.
#[repr(C)]
pub struct XmTextBlockRec {
    pub ptr: *mut c_char,
    pub length: c_int,
    pub format: c_int,
}

#[repr(C)]
pub struct XmTextVerifyCallbackStruct {
    pub reason: c_int,
    pub event: *mut c_void,
    pub doit: Boolean,
    pub curr_insert: c_long,
    pub new_insert: c_long,
    pub start_pos: c_long,
    pub end_pos: c_long,
    pub text: *mut XmTextBlockRec,
}

pub type XftFontPtr = *mut XftFont;
pub type XftDraw = *mut c_void;
pub type Colormap = c_ulong;
pub type FcChar32 = u32;

// First fields of _XftFont (X11/Xft/Xft.h); trailing charset/pattern ignored.
#[repr(C)]
pub struct XftFont {
    pub ascent: c_int,
    pub descent: c_int,
    pub height: c_int,
    pub max_advance_width: c_int,
    pub charset: *mut c_void,
    pub pattern: *mut c_void,
}

#[repr(C)]
pub struct XRenderColor {
    pub red: c_ushort,
    pub green: c_ushort,
    pub blue: c_ushort,
    pub alpha: c_ushort,
}

#[repr(C)]
pub struct XftColor {
    pub pixel: c_ulong,
    pub color: XRenderColor,
}

#[repr(C)]
pub struct XGlyphInfo {
    pub width: c_ushort,
    pub height: c_ushort,
    pub x: c_short,
    pub y: c_short,
    pub x_off: c_short,
    pub y_off: c_short,
}

// Visual layout from X11/Xlib.h — read-only, for the channel masks.
#[repr(C)]
pub struct VisualLayout {
    pub ext_data: *mut c_void,
    pub visualid: c_ulong,
    pub class: c_int,
    pub red_mask: c_ulong,
    pub green_mask: c_ulong,
    pub blue_mask: c_ulong,
    pub bits_per_rgb: c_int,
    pub map_entries: c_int,
}

pub type XtCallbackProc = extern "C" fn(Widget, XtPointer, XtPointer);
pub type XtTimerCallbackProc = extern "C" fn(XtPointer, *mut XtIntervalId);
pub type XtInputCallbackProc = extern "C" fn(XtPointer, *mut c_int, *mut XtInputId);

extern "C" {
    // ---- Xt ----
    pub fn XtAppInitialize(
        app: *mut XtAppContext,
        application_class: *const c_char,
        options: *const c_void,
        num_options: Cardinal,
        argc: *mut c_int,
        argv: *mut *mut c_char,
        fallback_resources: *const *const c_char,
        args: *const Arg,
        num_args: Cardinal,
    ) -> Widget;
    /// Passing all-null installs Xt's default language proc: setlocale(LC_ALL,"")
    /// + X locale setup. Must run before XtAppInitialize for UTF-8 to take.
    pub fn XtSetLanguageProc(
        app: XtAppContext,
        proc_: *mut c_void,
        closure: XtPointer,
    ) -> *mut c_void;
    pub fn XtSetValues(w: Widget, args: *const Arg, num_args: Cardinal);
    pub fn XtGetValues(w: Widget, args: *const Arg, num_args: Cardinal);
    pub fn XtManageChild(w: Widget);
    pub fn XtUnmanageChild(w: Widget);
    pub fn XtDestroyWidget(w: Widget);
    pub fn XtRealizeWidget(w: Widget);
    pub fn XtAddCallback(
        w: Widget,
        callback_name: *const c_char,
        callback: XtCallbackProc,
        closure: XtPointer,
    );
    pub fn XtAppMainLoop(app: XtAppContext);
    pub fn XtAppAddTimeOut(
        app: XtAppContext,
        interval_ms: c_ulong,
        proc_: XtTimerCallbackProc,
        closure: XtPointer,
    ) -> XtIntervalId;
    pub fn XtAppAddInput(
        app: XtAppContext,
        source: c_int,
        condition: XtPointer,
        proc_: XtInputCallbackProc,
        closure: XtPointer,
    ) -> XtInputId;
    pub fn XtDisplay(w: Widget) -> *mut Display;
    pub fn XtWindow(w: Widget) -> XWindow;
    pub fn XOpenDisplay(name: *const c_char) -> *mut Display;
    pub fn XCloseDisplay(dpy: *mut Display) -> c_int;
    pub fn XtSetSensitive(w: Widget, sensitive: Boolean);
    pub fn XtFree(ptr: *mut c_char);
    pub fn XtParent(w: Widget) -> Widget;

    // ---- Xm widget creation: (parent, name, args, num_args) -> unmanaged widget ----
    pub fn XmCreateForm(p: Widget, n: *const c_char, a: *const Arg, c: Cardinal) -> Widget;
    pub fn XmCreateFrame(p: Widget, n: *const c_char, a: *const Arg, c: Cardinal) -> Widget;
    pub fn XmCreateLabel(p: Widget, n: *const c_char, a: *const Arg, c: Cardinal) -> Widget;
    pub fn XmCreatePushButton(p: Widget, n: *const c_char, a: *const Arg, c: Cardinal) -> Widget;
    pub fn XmCreateRowColumn(p: Widget, n: *const c_char, a: *const Arg, c: Cardinal) -> Widget;
    pub fn XmCreateScrolledWindow(p: Widget, n: *const c_char, a: *const Arg, c: Cardinal)
        -> Widget;
    pub fn XmCreateSeparator(p: Widget, n: *const c_char, a: *const Arg, c: Cardinal) -> Widget;
    pub fn XmCreateTextField(p: Widget, n: *const c_char, a: *const Arg, c: Cardinal) -> Widget;

    // ---- Xft render tables (Unicode/emoji text) ----
    pub fn XmRenditionCreate(
        w: Widget,
        tag: *mut c_char,
        args: *const Arg,
        n: Cardinal,
    ) -> XmRendition;
    pub fn XmRenderTableAddRenditions(
        old: XmRenderTable,
        renditions: *const XmRendition,
        n: Cardinal,
        mode: c_int,
    ) -> XmRenderTable;

    // ---- XmString ----
    pub fn XmStringCreateLocalized(text: *const c_char) -> XmString;
    /// XmMULTIBYTE_TEXT decodes `text` as a locale multibyte string (UTF-8 here)
    /// to wide chars, so Xft draws real Unicode instead of byte-per-glyph.
    pub fn XmStringGenerate(
        text: XtPointer,
        tag: *mut c_char,
        text_type: c_int,
        rendition: *mut c_char,
    ) -> XmString;
    pub fn XmStringSeparatorCreate() -> XmString;
    pub fn XmStringConcatAndFree(a: XmString, b: XmString) -> XmString;
    pub fn XmStringFree(s: XmString);

    // ---- XmTextField ----
    pub fn XmTextFieldGetString(w: Widget) -> *mut c_char;
    pub fn XmTextFieldSetString(w: Widget, s: *const c_char);

    // ---- XmScrolledWindow ----
    pub fn XmScrollVisible(sw: Widget, w: Widget, hor_margin: Dimension, ver_margin: Dimension);

    // ---- misc Xm ----
    pub fn XmUpdateDisplay(w: Widget);

    // ---- Xlib (avatars / pixmaps) ----
    pub fn XDefaultScreen(dpy: *mut Display) -> c_int;
    pub fn XDefaultDepth(dpy: *mut Display, screen: c_int) -> c_int;
    pub fn XDefaultVisual(dpy: *mut Display, screen: c_int) -> *mut Visual;
    pub fn XDefaultRootWindow(dpy: *mut Display) -> XWindow;
    pub fn XCreatePixmap(
        dpy: *mut Display,
        d: Drawable,
        width: c_uint,
        height: c_uint,
        depth: c_uint,
    ) -> Pixmap;
    pub fn XFreePixmap(dpy: *mut Display, p: Pixmap);
    pub fn XCreateGC(
        dpy: *mut Display,
        d: Drawable,
        valuemask: c_ulong,
        values: *mut c_void,
    ) -> GC;
    pub fn XFreeGC(dpy: *mut Display, gc: GC);
    pub fn XCreateImage(
        dpy: *mut Display,
        visual: *mut Visual,
        depth: c_uint,
        format: c_int,
        offset: c_int,
        data: *mut c_char,
        width: c_uint,
        height: c_uint,
        bitmap_pad: c_int,
        bytes_per_line: c_int,
    ) -> *mut c_void;
    pub fn XPutImage(
        dpy: *mut Display,
        d: Drawable,
        gc: GC,
        image: *mut c_void,
        src_x: c_int,
        src_y: c_int,
        dest_x: c_int,
        dest_y: c_int,
        width: c_uint,
        height: c_uint,
    ) -> c_int;
    pub fn XFree(ptr: *mut c_void) -> c_int;
    pub fn XFlush(dpy: *mut Display) -> c_int;
    pub fn XDefaultColormap(dpy: *mut Display, screen: c_int) -> Colormap;
    pub fn XSetForeground(dpy: *mut Display, gc: GC, pixel: Pixel);
    pub fn XFillRectangle(
        dpy: *mut Display,
        d: Drawable,
        gc: GC,
        x: c_int,
        y: c_int,
        width: c_uint,
        height: c_uint,
    ) -> c_int;

    // ---- Xft (self-rendered Unicode/emoji text; openmotif's XmString path
    //      can't draw UTF-8, so text pixmaps are drawn here directly) ----
    pub fn XftFontOpenName(dpy: *mut Display, screen: c_int, name: *const c_char) -> XftFontPtr;
    pub fn XftFontClose(dpy: *mut Display, font: XftFontPtr);
    pub fn XftCharExists(dpy: *mut Display, font: XftFontPtr, ucs4: FcChar32) -> c_int;
    pub fn XftTextExtents32(
        dpy: *mut Display,
        font: XftFontPtr,
        string: *const FcChar32,
        len: c_int,
        extents: *mut XGlyphInfo,
    );
    pub fn XftDrawCreate(
        dpy: *mut Display,
        drawable: Drawable,
        visual: *mut Visual,
        colormap: Colormap,
    ) -> XftDraw;
    pub fn XftDrawDestroy(draw: XftDraw);
    pub fn XftDrawString32(
        draw: XftDraw,
        color: *const XftColor,
        font: XftFontPtr,
        x: c_int,
        y: c_int,
        string: *const FcChar32,
        len: c_int,
    );
    pub fn XftColorAllocValue(
        dpy: *mut Display,
        visual: *mut Visual,
        colormap: Colormap,
        color: *const XRenderColor,
        result: *mut XftColor,
    ) -> c_int;
    pub fn XftColorFree(
        dpy: *mut Display,
        visual: *mut Visual,
        colormap: Colormap,
        color: *mut XftColor,
    );
    pub fn XClearArea(
        dpy: *mut Display,
        w: XWindow,
        x: c_int,
        y: c_int,
        width: c_uint,
        height: c_uint,
        exposures: c_int,
    ) -> c_int;
    pub fn XSetErrorHandler(
        handler: extern "C" fn(*mut Display, *mut XErrorEvent) -> c_int,
    ) -> *mut c_void;
    pub fn XGetErrorText(
        dpy: *mut Display,
        code: c_int,
        buffer: *mut c_char,
        length: c_int,
    ) -> c_int;
}

// Layout from X11/Xlib.h.
#[repr(C)]
pub struct XErrorEvent {
    pub type_: c_int,
    pub display: *mut Display,
    pub resourceid: c_ulong,
    pub serial: c_ulong,
    pub error_code: c_uchar,
    pub request_code: c_uchar,
    pub minor_code: c_uchar,
}

// ---- resource names (content-matched; see module docs) ----
pub const XmNactivateCallback: &CStr = c"activateCallback";
pub const XmNalignment: &CStr = c"alignment";
pub const XmNallowShellResize: &CStr = c"allowShellResize";
pub const XmNbottomAttachment: &CStr = c"bottomAttachment";
pub const XmNbottomOffset: &CStr = c"bottomOffset";
pub const XmNbottomWidget: &CStr = c"bottomWidget";
pub const XmNcolumns: &CStr = c"columns";
pub const XmNheight: &CStr = c"height";
pub const XmNlabelPixmap: &CStr = c"labelPixmap";
pub const XmNlabelString: &CStr = c"labelString";
pub const XmNlabelType: &CStr = c"labelType";
pub const XmNleftAttachment: &CStr = c"leftAttachment";
pub const XmNleftOffset: &CStr = c"leftOffset";
pub const XmNleftWidget: &CStr = c"leftWidget";
pub const XmNmarginHeight: &CStr = c"marginHeight";
pub const XmNmarginWidth: &CStr = c"marginWidth";
pub const XmNmaxLength: &CStr = c"maxLength";
pub const XmNmodifyVerifyCallback: &CStr = c"modifyVerifyCallback";
pub const XmNorientation: &CStr = c"orientation";
pub const XmNrecomputeSize: &CStr = c"recomputeSize";
pub const XmNresizable: &CStr = c"resizable";
pub const XmNfontName: &CStr = c"fontName";
pub const XmNfontType: &CStr = c"fontType";
pub const XmNloadModel: &CStr = c"loadModel";
pub const XmNrenderTable: &CStr = c"renderTable";
pub const XmNrightAttachment: &CStr = c"rightAttachment";
pub const XmNrightOffset: &CStr = c"rightOffset";
pub const XmNrightWidget: &CStr = c"rightWidget";
pub const XmNscrollBarDisplayPolicy: &CStr = c"scrollBarDisplayPolicy";
pub const XmNscrollingPolicy: &CStr = c"scrollingPolicy";
pub const XmNspacing: &CStr = c"spacing";
pub const XmNtitle: &CStr = c"title";
pub const XmNtopAttachment: &CStr = c"topAttachment";
pub const XmNtopOffset: &CStr = c"topOffset";
pub const XmNtopWidget: &CStr = c"topWidget";
pub const XmNwidth: &CStr = c"width";

// ---- helpers ----

pub fn arg(name: &CStr, value: XtArgVal) -> Arg {
    Arg {
        name: name.as_ptr(),
        value,
    }
}

pub fn arg_ptr<T>(name: &CStr, value: *mut T) -> Arg {
    arg(name, value as XtArgVal)
}

pub fn arg_widget(name: &CStr, w: Widget) -> Arg {
    arg(name, w as XtArgVal)
}

pub unsafe fn set_values(w: Widget, args: &[Arg]) {
    XtSetValues(w, args.as_ptr(), args.len() as Cardinal);
}

/// The charset tag used for both renditions and string segments. Naming it
/// "UTF-8" is what flips openmotif's Xft draw onto XftDrawStringUtf8.
pub const UTF8_TAG: &CStr = c"UTF-8";

/// Build an Xft-backed render table for a fontconfig pattern (e.g.
/// "Arial Unicode MS:size=12"). Xft + fontconfig cover the whole Unicode BMP
/// and, with libXft's color support, emoji.
pub unsafe fn make_render_table(w: Widget, font_pattern: &CStr) -> XmRenderTable {
    let args = [
        arg(XmNfontName, font_pattern.as_ptr() as XtArgVal),
        arg(XmNfontType, XmFONT_IS_XFT),
        arg(XmNloadModel, XmLOAD_IMMEDIATE),
    ];
    let rendition = XmRenditionCreate(
        w,
        UTF8_TAG.as_ptr() as *mut c_char,
        args.as_ptr(),
        args.len() as Cardinal,
    );
    XmRenderTableAddRenditions(std::ptr::null_mut(), &rendition, 1, XmMERGE_REPLACE)
}

/// UTF-8 with NULs/controls dropped; Motif can't hold a NUL and controls draw
/// as boxes. Newlines are already split into separate segments.
fn clean_line(line: &str) -> std::ffi::CString {
    let s: String = line
        .chars()
        .filter(|c| *c != '\0' && !c.is_control())
        .collect();
    std::ffi::CString::new(s).unwrap_or_default()
}

/// Multiline-aware XmString: newlines become XmString separators, so labels
/// render pre-wrapped text without needing an XmText widget. Segments carry the
/// UTF-8 charset tag so openmotif's Xft path draws them as UTF-8.
pub fn xm_string(text: &str) -> XmString {
    let tag = UTF8_TAG.as_ptr() as *mut c_char;
    let mut out: XmString = std::ptr::null_mut();
    for (i, line) in text.split('\n').enumerate() {
        let cline = clean_line(line);
        unsafe {
            let seg = XmStringGenerate(cline.as_ptr() as XtPointer, tag, XmCHARSET_TEXT, tag);
            out = if i == 0 {
                seg
            } else {
                XmStringConcatAndFree(XmStringConcatAndFree(out, XmStringSeparatorCreate()), seg)
            };
        }
    }
    if out.is_null() {
        out = unsafe { XmStringCreateLocalized(c"".as_ptr()) };
    }
    out
}

/// Set a label/button's labelString, freeing the XmString after Motif copies it.
pub unsafe fn set_label(w: Widget, text: &str) {
    let s = xm_string(text);
    set_values(w, &[arg_ptr(XmNlabelString, s)]);
    XmStringFree(s);
}

/// Swap a label's pixmap. openmotif 2.3.8's XmLabel stores the new pixmap but
/// skips the repaint, so force a full-window Expose to make it visible.
pub unsafe fn set_label_pixmap(w: Widget, pixmap: Pixmap) {
    set_values(w, &[arg(XmNlabelPixmap, pixmap as XtArgVal)]);
    let win = XtWindow(w);
    if win != 0 {
        XClearArea(XtDisplay(w), win, 0, 0, 0, 0, 1);
    }
}

/// Read a TextField's contents as an owned Rust string.
pub unsafe fn text_field_string(w: Widget) -> String {
    let p = XmTextFieldGetString(w);
    if p.is_null() {
        return String::new();
    }
    let s = CStr::from_ptr(p).to_string_lossy().into_owned();
    XtFree(p);
    s
}
