//! Widget construction and pixel pushing: login form, timeline rows, avatars.
//!
//! The timeline is a ScrolledWindow whose work area is a plain Form; post rows
//! chain top-to-bottom via ATTACH_WIDGET. That sidesteps RowColumn's geometry
//! renegotiation entirely — the work form has a fixed width and grows downward.

use crate::bsky::{self, FeedItem};
use crate::motif::*;
use crate::text::{TextRenderer, Weight};
use std::collections::HashMap;
use std::ffi::CStr;
use std::os::raw::{c_char, c_int, c_uint};
use std::ptr;

const FG_DARK: (u8, u8, u8) = (0x1a, 0x1a, 0x1a);
const FG_GRAY: (u8, u8, u8) = (0x4a, 0x4a, 0x4a);

pub const CONTENT_W: XtArgVal = 560;
pub const AVATAR_PX: u32 = 48;

/// Xft render tables for the ASCII chrome (buttons, handles, meta). `normal`
/// is set on the shell and inherited; `title` styles the login heading.
/// Unicode-bearing text is drawn separately by TextRenderer.
pub struct Fonts {
    pub normal: XmRenderTable,
    pub title: XmRenderTable,
}

unsafe fn set_font(w: Widget, rt: XmRenderTable) {
    set_values(w, &[arg(XmNrenderTable, rt as XtArgVal)]);
}
// ponytail: char-count wrap against a proportional font; 66 cols keeps worst
// lines inside CONTENT_W. Upgrade path: measure with XmStringWidth.
const WRAP_COLS: usize = 66;

pub struct LoginUi {
    pub form: Widget,
    pub handle_field: Widget,
    pub button: Widget,
    pub error_label: Widget,
    /// Real password bytes; the widget shows only '*'s (see password_verify_cb).
    pub shadow: Vec<u8>,
}

pub struct TimelineUi {
    pub form: Widget,
    pub sw: Widget,
    pub work: Widget,
    pub refresh_btn: Widget,
    pub status_label: Widget,
    pub load_more_btn: Widget,
    /// Post rows + separators in order, for teardown and scroll-to-top.
    pub rows: Vec<Widget>,
    /// Text pixmaps behind the Unicode labels; freed when rows are cleared.
    pub text_pixmaps: Vec<Pixmap>,
    /// Bottom-most child, for chaining the next row's top attachment.
    pub last_child: Option<Widget>,
    pub interactive: bool,
}

pub struct PostUi {
    pub uri: String,
    pub cid: String,
    pub like_uri: Option<String>,
    pub repost_uri: Option<String>,
    pub like_count: u64,
    pub repost_count: u64,
    pub like_btn: Widget,
    pub repost_btn: Widget,
    pub busy: bool,
}

// ---- small helpers ----

unsafe fn mk(
    ctor: unsafe extern "C" fn(Widget, *const c_char, *const Arg, Cardinal) -> Widget,
    parent: Widget,
    name: &CStr,
    args: &[Arg],
) -> Widget {
    let w = ctor(parent, name.as_ptr(), args.as_ptr(), args.len() as Cardinal);
    XtManageChild(w);
    w
}

/// A label whose content is Unicode text drawn to a pixmap by Xft (openmotif
/// can't render UTF-8 itself). The pixmap is pushed to `sink` so the row's
/// teardown can free it.
unsafe fn text_label(
    parent: Widget,
    name: &CStr,
    attach: &[Arg],
    tr: &TextRenderer,
    lines: &[String],
    weight: Weight,
    color: (u8, u8, u8),
    sink: &mut Vec<Pixmap>,
) -> Widget {
    let (pm, _, _) = tr.render(lines, weight, color);
    sink.push(pm);
    let mut a = attach.to_vec();
    a.push(arg(XmNlabelType, XmPIXMAP));
    a.push(arg(XmNlabelPixmap, pm as XtArgVal));
    a.push(arg(XmNmarginWidth, 0));
    a.push(arg(XmNmarginHeight, 1));
    mk(XmCreateLabel, parent, name, &a)
}

fn wrapped_lines(text: &str, cols: usize) -> Vec<String> {
    let cleaned = clean_text(text);
    let body = wrap_text(&cleaned, cols);
    if body.is_empty() {
        vec!["(no text)".into()]
    } else {
        body.split('\n').map(str::to_owned).collect()
    }
}

/// Greedy word wrap by char count. Long tokens (URLs) are hard-broken.
pub fn wrap_text(text: &str, cols: usize) -> String {
    let mut out = String::new();
    for (i, line) in text.split('\n').enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let mut col = 0usize;
        for word in line.split_whitespace() {
            let wlen = word.chars().count();
            if col > 0 && col + 1 + wlen > cols {
                out.push('\n');
                col = 0;
            } else if col > 0 {
                out.push(' ');
                col += 1;
            }
            if wlen <= cols {
                out.push_str(word);
                col += wlen;
            } else {
                for ch in word.chars() {
                    if col >= cols {
                        out.push('\n');
                        col = 0;
                    }
                    out.push(ch);
                    col += 1;
                }
            }
        }
    }
    out
}

pub fn label_for_like(count: u64, liked: bool) -> String {
    format!("{} {}", if liked { "Liked" } else { "Like" }, count)
}

pub fn label_for_repost(count: u64, reposted: bool) -> String {
    format!("{} {}", if reposted { "Reposted" } else { "Repost" }, count)
}

// ---- login ----

pub unsafe fn build_login(shell: Widget, prefill_handle: &str, fonts: &Fonts) -> LoginUi {
    let form = XmCreateForm(
        shell,
        c"loginForm".as_ptr(),
        [arg(XmNwidth, 360)].as_ptr(),
        1,
    );

    let title = mk(
        XmCreateLabel,
        form,
        c"loginTitle",
        &[
            arg(XmNtopAttachment, XmATTACH_FORM),
            arg(XmNleftAttachment, XmATTACH_FORM),
            arg(XmNrightAttachment, XmATTACH_FORM),
            arg(XmNtopOffset, 18),
        ],
    );
    set_label(title, "Bluesky");
    set_font(title, fonts.title);

    let subtitle = mk(
        XmCreateLabel,
        form,
        c"metaLabel",
        &[
            arg(XmNtopAttachment, XmATTACH_WIDGET),
            arg_widget(XmNtopWidget, title),
            arg(XmNleftAttachment, XmATTACH_FORM),
            arg(XmNrightAttachment, XmATTACH_FORM),
            arg(XmNtopOffset, 4),
        ],
    );
    set_label(subtitle, "Sign in with a handle and app password");

    let handle_label = mk(
        XmCreateLabel,
        form,
        c"fieldLabel",
        &[
            arg(XmNtopAttachment, XmATTACH_WIDGET),
            arg_widget(XmNtopWidget, subtitle),
            arg(XmNleftAttachment, XmATTACH_FORM),
            arg(XmNtopOffset, 18),
            arg(XmNleftOffset, 20),
            arg(XmNalignment, XmALIGNMENT_BEGINNING),
        ],
    );
    set_label(handle_label, "Handle");

    let handle_field = mk(
        XmCreateTextField,
        form,
        c"handleField",
        &[
            arg(XmNtopAttachment, XmATTACH_WIDGET),
            arg_widget(XmNtopWidget, handle_label),
            arg(XmNleftAttachment, XmATTACH_FORM),
            arg(XmNrightAttachment, XmATTACH_FORM),
            arg(XmNtopOffset, 2),
            arg(XmNleftOffset, 20),
            arg(XmNrightOffset, 20),
            arg(XmNcolumns, 26),
        ],
    );
    if !prefill_handle.is_empty() {
        if let Ok(c) = std::ffi::CString::new(prefill_handle) {
            XmTextFieldSetString(handle_field, c.as_ptr());
        }
    }

    let pw_label = mk(
        XmCreateLabel,
        form,
        c"fieldLabel",
        &[
            arg(XmNtopAttachment, XmATTACH_WIDGET),
            arg_widget(XmNtopWidget, handle_field),
            arg(XmNleftAttachment, XmATTACH_FORM),
            arg(XmNtopOffset, 10),
            arg(XmNleftOffset, 20),
            arg(XmNalignment, XmALIGNMENT_BEGINNING),
        ],
    );
    set_label(pw_label, "App password");

    let password_field = mk(
        XmCreateTextField,
        form,
        c"passwordField",
        &[
            arg(XmNtopAttachment, XmATTACH_WIDGET),
            arg_widget(XmNtopWidget, pw_label),
            arg(XmNleftAttachment, XmATTACH_FORM),
            arg(XmNrightAttachment, XmATTACH_FORM),
            arg(XmNtopOffset, 2),
            arg(XmNleftOffset, 20),
            arg(XmNrightOffset, 20),
            arg(XmNcolumns, 26),
            arg(XmNmaxLength, 128),
        ],
    );
    XtAddCallback(
        password_field,
        XmNmodifyVerifyCallback.as_ptr(),
        crate::password_verify_cb,
        ptr::null_mut(),
    );
    XtAddCallback(
        password_field,
        XmNactivateCallback.as_ptr(),
        crate::login_clicked_cb,
        ptr::null_mut(),
    );

    let error_label = mk(
        XmCreateLabel,
        form,
        c"errorLabel",
        &[
            arg(XmNtopAttachment, XmATTACH_WIDGET),
            arg_widget(XmNtopWidget, password_field),
            arg(XmNleftAttachment, XmATTACH_FORM),
            arg(XmNrightAttachment, XmATTACH_FORM),
            arg(XmNtopOffset, 8),
            arg(XmNleftOffset, 20),
            arg(XmNrightOffset, 20),
        ],
    );
    set_label(error_label, " ");

    let button = mk(
        XmCreatePushButton,
        form,
        c"loginButton",
        &[
            arg(XmNtopAttachment, XmATTACH_WIDGET),
            arg_widget(XmNtopWidget, error_label),
            arg(XmNleftAttachment, XmATTACH_FORM),
            arg(XmNrightAttachment, XmATTACH_FORM),
            arg(XmNbottomAttachment, XmATTACH_FORM),
            arg(XmNtopOffset, 8),
            arg(XmNleftOffset, 120),
            arg(XmNrightOffset, 120),
            arg(XmNbottomOffset, 20),
        ],
    );
    set_label(button, "Log in");
    XtAddCallback(
        button,
        XmNactivateCallback.as_ptr(),
        crate::login_clicked_cb,
        ptr::null_mut(),
    );

    XtManageChild(form);
    LoginUi {
        form,
        handle_field,
        button,
        error_label,
        shadow: Vec::new(),
    }
}

// ---- timeline ----

pub unsafe fn build_timeline(shell: Widget, interactive: bool) -> TimelineUi {
    let form = XmCreateForm(shell, c"timelineForm".as_ptr(), ptr::null(), 0);

    let refresh_btn = mk(
        XmCreatePushButton,
        form,
        c"refreshButton",
        &[
            arg(XmNtopAttachment, XmATTACH_FORM),
            arg(XmNleftAttachment, XmATTACH_FORM),
            arg(XmNtopOffset, 6),
            arg(XmNleftOffset, 8),
        ],
    );
    set_label(refresh_btn, "Refresh");
    XtAddCallback(
        refresh_btn,
        XmNactivateCallback.as_ptr(),
        crate::refresh_cb,
        ptr::null_mut(),
    );

    let logout_btn = mk(
        XmCreatePushButton,
        form,
        c"logoutButton",
        &[
            arg(XmNtopAttachment, XmATTACH_FORM),
            arg(XmNrightAttachment, XmATTACH_FORM),
            arg(XmNtopOffset, 6),
            arg(XmNrightOffset, 8),
        ],
    );
    set_label(logout_btn, "Log out");
    XtAddCallback(
        logout_btn,
        XmNactivateCallback.as_ptr(),
        crate::logout_cb,
        ptr::null_mut(),
    );
    if !interactive {
        XtSetSensitive(logout_btn, FALSE);
    }

    let status_label = mk(
        XmCreateLabel,
        form,
        c"statusLabel",
        &[
            arg(XmNtopAttachment, XmATTACH_FORM),
            arg(XmNleftAttachment, XmATTACH_WIDGET),
            arg_widget(XmNleftWidget, refresh_btn),
            arg(XmNrightAttachment, XmATTACH_WIDGET),
            arg_widget(XmNrightWidget, logout_btn),
            arg(XmNtopOffset, 10),
            arg(XmNleftOffset, 10),
            arg(XmNalignment, XmALIGNMENT_BEGINNING),
        ],
    );
    set_label(status_label, "Loading...");

    let load_more_btn = mk(
        XmCreatePushButton,
        form,
        c"loadMoreButton",
        &[
            arg(XmNbottomAttachment, XmATTACH_FORM),
            arg(XmNleftAttachment, XmATTACH_FORM),
            arg(XmNrightAttachment, XmATTACH_FORM),
            arg(XmNbottomOffset, 6),
            arg(XmNleftOffset, 8),
            arg(XmNrightOffset, 8),
        ],
    );
    set_label(load_more_btn, "Load older posts");
    XtAddCallback(
        load_more_btn,
        XmNactivateCallback.as_ptr(),
        crate::load_more_cb,
        ptr::null_mut(),
    );

    let sw = mk(
        XmCreateScrolledWindow,
        form,
        c"timelineScroll",
        &[
            arg(XmNtopAttachment, XmATTACH_WIDGET),
            arg_widget(XmNtopWidget, refresh_btn),
            arg(XmNleftAttachment, XmATTACH_FORM),
            arg(XmNrightAttachment, XmATTACH_FORM),
            arg(XmNbottomAttachment, XmATTACH_WIDGET),
            arg_widget(XmNbottomWidget, load_more_btn),
            arg(XmNtopOffset, 6),
            arg(XmNbottomOffset, 6),
            arg(XmNscrollingPolicy, XmAUTOMATIC),
            arg(XmNwidth, CONTENT_W + 40),
            arg(XmNheight, 700),
        ],
    );

    let work = mk(
        XmCreateForm,
        sw,
        c"timelineWork",
        &[arg(XmNwidth, CONTENT_W + 16)],
    );

    XtManageChild(form);
    TimelineUi {
        form,
        sw,
        work,
        refresh_btn,
        status_label,
        load_more_btn,
        rows: Vec::new(),
        text_pixmaps: Vec::new(),
        last_child: None,
        interactive,
    }
}

fn clean_text(text: &str) -> String {
    text.chars()
        .filter(|c| *c == '\n' || !c.is_control())
        .collect()
}

/// Short display label for a link button: no scheme, capped length.
fn link_label(url: &str) -> String {
    let short = url
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    let mut label: String = short.chars().take(44).collect();
    if short.chars().count() > 44 {
        label.push_str("...");
    }
    label
}

/// Build one post row (+ trailing separator) chained below the previous one.
/// `cached` is the avatar pixmap if already available; otherwise the row shows
/// `placeholder` and the returned (URL, avatar label) should be fetched/updated.
/// Link URLs are appended to `links`; buttons carry the global link index.
pub unsafe fn build_post_row(
    tl: &mut TimelineUi,
    item: &FeedItem,
    post_index: usize,
    cached: Option<Pixmap>,
    placeholder: Pixmap,
    now: i64,
    links: &mut Vec<String>,
    tr: &TextRenderer,
) -> (PostUi, Option<(String, Widget)>) {
    let mut row_args = vec![
        arg(XmNleftAttachment, XmATTACH_FORM),
        arg(XmNrightAttachment, XmATTACH_FORM),
    ];
    match tl.last_child {
        Some(prev) => {
            row_args.push(arg(XmNtopAttachment, XmATTACH_WIDGET));
            row_args.push(arg_widget(XmNtopWidget, prev));
        }
        None => row_args.push(arg(XmNtopAttachment, XmATTACH_FORM)),
    }
    let row = mk(XmCreateForm, tl.work, c"postRow", &row_args);

    // Context stack above the main post: repost line, then quoted reply parent.
    // `anchor` is the widget the next block hangs below (None = top of row).
    let mut anchor: Option<Widget> = None;
    let top_of = |anchor: Option<Widget>, off: XtArgVal| -> Vec<Arg> {
        match anchor {
            Some(a) => vec![
                arg(XmNtopAttachment, XmATTACH_WIDGET),
                arg_widget(XmNtopWidget, a),
                arg(XmNtopOffset, off),
            ],
            None => vec![arg(XmNtopAttachment, XmATTACH_FORM), arg(XmNtopOffset, off)],
        }
    };

    if let Some(by) = item.reposted_by() {
        let mut a = top_of(anchor, 4);
        a.extend([
            arg(XmNleftAttachment, XmATTACH_FORM),
            arg(XmNleftOffset, 62),
            arg(XmNalignment, XmALIGNMENT_BEGINNING),
        ]);
        let l = mk(XmCreateLabel, row, c"contextLabel", &a);
        set_label(l, &format!("reposted by @{}", by.handle));
        anchor = Some(l);
    }

    if let Some(parent) = item.reply_parent() {
        let mut a = top_of(anchor, 4);
        a.extend([
            arg(XmNleftAttachment, XmATTACH_FORM),
            arg(XmNleftOffset, 62),
            arg(XmNalignment, XmALIGNMENT_BEGINNING),
        ]);
        let ctx = mk(XmCreateLabel, row, c"contextLabel", &a);
        set_label(ctx, "in reply to:");

        let mut a = top_of(Some(ctx), 2);
        a.extend([
            arg(XmNleftAttachment, XmATTACH_FORM),
            arg(XmNleftOffset, 74),
            arg(XmNalignment, XmALIGNMENT_BEGINNING),
        ]);
        let pdisplay = parent
            .author
            .display_name
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(&parent.author.handle);
        let pname = text_label(
            row,
            c"authorLabel",
            &a,
            tr,
            &[clean_text(pdisplay)],
            Weight::Bold,
            FG_DARK,
            &mut tl.text_pixmaps,
        );

        let mut a = top_of(Some(ctx), 2);
        a.extend([
            arg(XmNleftAttachment, XmATTACH_WIDGET),
            arg_widget(XmNleftWidget, pname),
            arg(XmNleftOffset, 8),
            arg(XmNalignment, XmALIGNMENT_BEGINNING),
        ]);
        let pmeta = mk(XmCreateLabel, row, c"metaLabel", &a);
        let pwhen = bsky::parse_iso8601(&parent.record.created_at)
            .map(|t| bsky::relative_time(t, now))
            .unwrap_or_default();
        set_label(pmeta, &format!("@{} - {}", parent.author.handle, pwhen));

        let (ptext_raw, _plinks) = parent.record.expanded_text_and_links();
        let mut a = top_of(Some(pname), 2);
        a.extend([
            arg(XmNleftAttachment, XmATTACH_FORM),
            arg(XmNleftOffset, 74),
            arg(XmNalignment, XmALIGNMENT_BEGINNING),
        ]);
        let ptext = text_label(
            row,
            c"parentText",
            &a,
            tr,
            &wrapped_lines(&ptext_raw, WRAP_COLS - 6),
            Weight::Normal,
            FG_GRAY,
            &mut tl.text_pixmaps,
        );
        anchor = Some(ptext);
    }

    let mut a = top_of(anchor, 6);
    a.extend([
        arg(XmNleftAttachment, XmATTACH_FORM),
        arg(XmNleftOffset, 6),
        arg(XmNlabelType, XmPIXMAP),
        arg(XmNlabelPixmap, cached.unwrap_or(placeholder) as XtArgVal),
    ]);
    let avatar = mk(XmCreateLabel, row, c"avatarLabel", &a);

    let author = &item.post.author;
    let mut a = top_of(anchor, 6);
    a.extend([
        arg(XmNleftAttachment, XmATTACH_WIDGET),
        arg_widget(XmNleftWidget, avatar),
        arg(XmNleftOffset, 8),
        arg(XmNalignment, XmALIGNMENT_BEGINNING),
    ]);
    let display_name = author
        .display_name
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(&author.handle);
    let name = text_label(
        row,
        c"authorLabel",
        &a,
        tr,
        &[clean_text(display_name)],
        Weight::Bold,
        FG_DARK,
        &mut tl.text_pixmaps,
    );

    let mut a = top_of(anchor, 6);
    a.extend([
        arg(XmNleftAttachment, XmATTACH_WIDGET),
        arg_widget(XmNleftWidget, name),
        arg(XmNleftOffset, 8),
        arg(XmNalignment, XmALIGNMENT_BEGINNING),
    ]);
    let meta = mk(XmCreateLabel, row, c"metaLabel", &a);
    let when = bsky::parse_iso8601(&item.post.record.created_at)
        .map(|t| bsky::relative_time(t, now))
        .unwrap_or_default();
    set_label(meta, &format!("@{} - {}", author.handle, when));

    let (text_raw, post_links) = item.post.record.expanded_text_and_links();
    let text = text_label(
        row,
        c"postText",
        &[
            arg(XmNtopAttachment, XmATTACH_WIDGET),
            arg_widget(XmNtopWidget, name),
            arg(XmNleftAttachment, XmATTACH_WIDGET),
            arg_widget(XmNleftWidget, avatar),
            arg(XmNtopOffset, 4),
            arg(XmNleftOffset, 8),
            arg(XmNalignment, XmALIGNMENT_BEGINNING),
        ],
        tr,
        &wrapped_lines(&text_raw, WRAP_COLS),
        Weight::Normal,
        FG_DARK,
        &mut tl.text_pixmaps,
    );

    // One button per link, stacked under the text.
    let mut above: Widget = text;
    for url in post_links {
        let btn = mk(
            XmCreatePushButton,
            row,
            c"linkButton",
            &[
                arg(XmNtopAttachment, XmATTACH_WIDGET),
                arg_widget(XmNtopWidget, above),
                arg(XmNleftAttachment, XmATTACH_WIDGET),
                arg_widget(XmNleftWidget, avatar),
                arg(XmNtopOffset, 3),
                arg(XmNleftOffset, 8),
            ],
        );
        set_label(btn, &link_label(&url));
        XtAddCallback(
            btn,
            XmNactivateCallback.as_ptr(),
            crate::open_link_cb,
            links.len() as XtPointer,
        );
        links.push(url);
        above = btn;
    }

    let viewer = item.post.viewer.as_ref();
    let like_uri = viewer.and_then(|v| v.like.clone());
    let repost_uri = viewer.and_then(|v| v.repost.clone());
    let like_count = item.post.like_count.unwrap_or(0);
    let repost_count = item.post.repost_count.unwrap_or(0);

    let like_btn = mk(
        XmCreatePushButton,
        row,
        c"actionButton",
        &[
            arg(XmNtopAttachment, XmATTACH_WIDGET),
            arg_widget(XmNtopWidget, above),
            arg(XmNleftAttachment, XmATTACH_WIDGET),
            arg_widget(XmNleftWidget, avatar),
            arg(XmNbottomAttachment, XmATTACH_FORM),
            arg(XmNtopOffset, 4),
            arg(XmNleftOffset, 8),
            arg(XmNbottomOffset, 6),
        ],
    );
    set_label(like_btn, &label_for_like(like_count, like_uri.is_some()));
    XtAddCallback(
        like_btn,
        XmNactivateCallback.as_ptr(),
        crate::like_cb,
        post_index as XtPointer,
    );

    let repost_btn = mk(
        XmCreatePushButton,
        row,
        c"actionButton",
        &[
            arg(XmNtopAttachment, XmATTACH_WIDGET),
            arg_widget(XmNtopWidget, above),
            arg(XmNleftAttachment, XmATTACH_WIDGET),
            arg_widget(XmNleftWidget, like_btn),
            arg(XmNbottomAttachment, XmATTACH_FORM),
            arg(XmNtopOffset, 4),
            arg(XmNleftOffset, 6),
            arg(XmNbottomOffset, 6),
        ],
    );
    set_label(
        repost_btn,
        &label_for_repost(repost_count, repost_uri.is_some()),
    );
    XtAddCallback(
        repost_btn,
        XmNactivateCallback.as_ptr(),
        crate::repost_cb,
        post_index as XtPointer,
    );

    if !tl.interactive {
        XtSetSensitive(like_btn, FALSE);
        XtSetSensitive(repost_btn, FALSE);
    }

    let sep = mk(
        XmCreateSeparator,
        tl.work,
        c"postSep",
        &[
            arg(XmNtopAttachment, XmATTACH_WIDGET),
            arg_widget(XmNtopWidget, row),
            arg(XmNleftAttachment, XmATTACH_FORM),
            arg(XmNrightAttachment, XmATTACH_FORM),
        ],
    );

    tl.rows.push(row);
    tl.rows.push(sep);
    tl.last_child = Some(sep);

    let avatar_want = if cached.is_none() {
        author.avatar.clone().map(|u| (u, avatar))
    } else {
        None
    };
    (
        PostUi {
            uri: item.post.uri.clone(),
            cid: item.post.cid.clone(),
            like_uri,
            repost_uri,
            like_count,
            repost_count,
            like_btn,
            repost_btn,
            busy: false,
        },
        avatar_want,
    )
}

// ---- pixmaps ----

pub struct Gfx {
    pub display: *mut Display,
    pub root: XWindow,
    pub gc: GC,
    pub depth: c_uint,
    pub visual: *mut Visual,
    pub colormap: Colormap,
    pub screen: c_int,
    rmask: u64,
    gmask: u64,
    bmask: u64,
}

pub unsafe fn init_gfx(display: *mut Display) -> Gfx {
    let screen = XDefaultScreen(display);
    let root = XDefaultRootWindow(display);
    let visual = XDefaultVisual(display, screen);
    let v = &*(visual as *const VisualLayout);
    Gfx {
        display,
        root,
        gc: XCreateGC(display, root, 0, ptr::null_mut()),
        depth: XDefaultDepth(display, screen) as c_uint,
        visual,
        colormap: XDefaultColormap(display, screen),
        screen,
        rmask: v.red_mask as u64,
        gmask: v.green_mask as u64,
        bmask: v.blue_mask as u64,
    }
}

impl Gfx {
    /// Pack an RGB triple into a pixel for this visual.
    pub fn pixel(&self, rgb: (u8, u8, u8)) -> Pixel {
        (place(self.rmask, rgb.0) | place(self.gmask, rgb.1) | place(self.bmask, rgb.2)) as Pixel
    }
}

fn place(mask: u64, channel: u8) -> u64 {
    if mask == 0 {
        return 0;
    }
    ((channel as u64) << mask.trailing_zeros()) & mask
}

/// RGBA bytes -> server-side pixmap, composited over the Motif background
/// gray so transparent avatar corners look right.
pub unsafe fn rgba_to_pixmap(gfx: &Gfx, rgba: &[u8], w: u32, h: u32) -> Pixmap {
    const BG: (u16, u16, u16) = (0xd4, 0xd0, 0xc8);
    let mut buf: Vec<u32> = Vec::with_capacity((w * h) as usize);
    for px in rgba.chunks_exact(4) {
        let a = px[3] as u16;
        let blend = |c: u8, bg: u16| (((c as u16) * a + bg * (255 - a)) / 255) as u8;
        let (r, g, b) = (blend(px[0], BG.0), blend(px[1], BG.1), blend(px[2], BG.2));
        buf.push((place(gfx.rmask, r) | place(gfx.gmask, g) | place(gfx.bmask, b)) as u32);
    }
    let pixmap = XCreatePixmap(gfx.display, gfx.root, w, h, gfx.depth);
    let image = XCreateImage(
        gfx.display,
        gfx.visual,
        gfx.depth,
        ZPixmap,
        0,
        buf.as_mut_ptr() as *mut c_char,
        w,
        h,
        32,
        0,
    );
    XPutImage(gfx.display, pixmap, gfx.gc, image, 0, 0, 0, 0, w, h);
    XFree(image); // frees the XImage struct only; buf is ours and dropped here
    pixmap
}

pub unsafe fn solid_pixmap(gfx: &Gfx, w: u32, h: u32, rgb: (u8, u8, u8)) -> Pixmap {
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for _ in 0..w * h {
        rgba.extend_from_slice(&[rgb.0, rgb.1, rgb.2, 255]);
    }
    rgba_to_pixmap(gfx, &rgba, w, h)
}

/// Decode + scale an avatar into RGBA bytes (done on a worker thread).
pub fn decode_avatar(bytes: &[u8]) -> Option<Vec<u8>> {
    let img = image::load_from_memory(bytes).ok()?;
    let scaled = img.resize_exact(AVATAR_PX, AVATAR_PX, image::imageops::FilterType::Triangle);
    Some(scaled.to_rgba8().into_raw())
}

/// Cache of avatar URL -> pixmap, plus labels waiting on in-flight fetches.
#[derive(Default)]
pub struct AvatarCache {
    pub ready: HashMap<String, Pixmap>,
    pub waiting: HashMap<String, Vec<Widget>>,
}

#[cfg(test)]
mod tests {
    use super::wrap_text;

    #[test]
    fn wraps_and_hard_breaks() {
        assert_eq!(wrap_text("aaa bbb ccc", 7), "aaa bbb\nccc");
        let long = wrap_text("https://example.com/averylongpathsegment", 10);
        assert!(long.lines().all(|l| l.chars().count() <= 10));
        assert_eq!(wrap_text("keep\n\nblank", 20), "keep\n\nblank");
    }
}
