//! BskyMotif: a Bluesky client in OSF/Motif, driven from Rust.
//!
//! Threading model: Xt owns the main thread. Network work happens on plain
//! std::threads which hand results back via `ui_post` — a queue drained by an
//! XtAppAddInput callback watching a self-pipe. All widget calls stay on the
//! main thread.

mod bsky;
mod keychain;
mod motif;
mod text;
mod ui;

use bsky::Client;
use motif::*;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::ffi::CStr;
use std::os::raw::{c_char, c_int, c_long};
use std::ptr;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Mutex};

const PAGE: u32 = 30;
const REFRESH_CHECK_MS: u64 = 60_000;

const FALLBACK_RESOURCES: &[&CStr] = &[
    // Fonts come from Xft render tables (see Fonts), not fontList — Xft gives
    // full Unicode + emoji, which core X fonts can't.
    c"*background: #d4d0c8",
    c"*XmTextField.background: #ffffff",
    c"*metaLabel.foreground: #5a5a5a",
    c"*contextLabel.foreground: #5a5a5a",
    c"*statusLabel.foreground: #3a3a3a",
    c"*fieldLabel.foreground: #3a3a3a",
    c"*errorLabel.foreground: #a00000",
    c"*parentText.foreground: #4a4a4a",
    c"*linkButton.foreground: #1a3a8a",
    c"*highlightThickness: 1",
];

pub struct App {
    app_ctx: XtAppContext,
    shell: Widget,
    gfx: ui::Gfx,
    client: Arc<Mutex<Client>>,
    public_actor: Option<String>,
    login: Option<ui::LoginUi>,
    timeline: Option<ui::TimelineUi>,
    posts: Vec<ui::PostUi>,
    avatars: ui::AvatarCache,
    placeholder: Pixmap,
    cursor: Option<String>,
    newest_key: Option<String>,
    identifier: String,
    password: String,
    timer_armed: bool,
    fonts: ui::Fonts,
    text: text::TextRenderer,
    /// URLs behind link buttons; button closures carry an index into this.
    links: Vec<String>,
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

fn with_app(f: impl FnOnce(&mut App)) {
    APP.with(|cell| {
        if let Some(app) = cell.borrow_mut().as_mut() {
            f(app);
        }
    });
}

// ---- main-thread dispatch from worker threads ----

type UiFn = Box<dyn FnOnce(&mut App) + Send>;
static UI_QUEUE: Mutex<VecDeque<UiFn>> = Mutex::new(VecDeque::new());
static PIPE_W: AtomicI32 = AtomicI32::new(-1);

pub fn ui_post(f: impl FnOnce(&mut App) + Send + 'static) {
    UI_QUEUE.lock().unwrap().push_back(Box::new(f));
    let fd = PIPE_W.load(Ordering::SeqCst);
    if fd >= 0 {
        unsafe { libc::write(fd, b"x".as_ptr() as *const _, 1) };
    }
}

extern "C" fn pipe_cb(_closure: XtPointer, fd: *mut c_int, _id: *mut XtInputId) {
    let mut buf = [0u8; 256];
    unsafe { libc::read(*fd, buf.as_mut_ptr() as *mut _, buf.len()) };
    loop {
        let job = UI_QUEUE.lock().unwrap().pop_front();
        match job {
            Some(job) => with_app(|app| job(app)),
            None => break,
        }
    }
}

// ---- UI state transitions ----

fn show_timeline(app: &mut App) {
    if let Some(login) = app.login.take() {
        unsafe { XtDestroyWidget(login.form) };
    }
    if app.timeline.is_none() {
        let interactive = app.public_actor.is_none();
        app.timeline = Some(unsafe { ui::build_timeline(app.shell, interactive) });
        if !app.timer_armed {
            unsafe { XtAppAddTimeOut(app.app_ctx, REFRESH_CHECK_MS, timer_cb, ptr::null_mut()) };
            app.timer_armed = true;
        }
    }
}

fn show_login_with_error(app: &mut App, error: &str) {
    if let Some(tl) = app.timeline.take() {
        unsafe {
            XtDestroyWidget(tl.form);
            for pm in tl.text_pixmaps {
                XFreePixmap(app.gfx.display, pm);
            }
        }
    }
    app.posts.clear();
    app.avatars.waiting.clear();
    app.cursor = None;
    app.newest_key = None;
    if app.login.is_none() {
        let prefill = app.identifier.clone();
        app.login = Some(unsafe { ui::build_login(app.shell, &prefill, &app.fonts) });
    }
    if let Some(login) = app.login.as_ref() {
        unsafe { set_label(login.error_label, error) };
    }
}

fn feed_key(item: &bsky::FeedItem) -> String {
    match item.reposted_by() {
        Some(by) => format!("{}#r{}", item.post.uri, by.did),
        None => item.post.uri.clone(),
    }
}

fn is_auth_error(code: &str) -> bool {
    code.contains("Token") || code.starts_with("Auth")
}

// ---- network orchestration ----

fn fetch_timeline_blocking(
    client: &Arc<Mutex<Client>>,
    public_actor: Option<&str>,
    cursor: Option<&str>,
    identifier: &str,
    password: &str,
) -> Result<bsky::Timeline, bsky::Error> {
    let mut c = client.lock().unwrap();
    match public_actor {
        Some(actor) => c.get_public_author_feed(actor, cursor, PAGE),
        None => match c.get_timeline(cursor, PAGE) {
            // Refresh token dead too? Fall back to a fresh app-password login.
            Err(bsky::Error::Api { ref error, .. })
                if is_auth_error(error) && !identifier.is_empty() =>
            {
                c.login(identifier, password)?;
                c.get_timeline(cursor, PAGE)
            }
            other => other,
        },
    }
}

fn spawn_fetch(app: &mut App, cursor: Option<String>, replace: bool) {
    if let Some(tl) = app.timeline.as_ref() {
        unsafe {
            set_label(tl.status_label, "Loading...");
            XtSetSensitive(tl.refresh_btn, FALSE);
        }
    }
    let client = app.client.clone();
    let public = app.public_actor.clone();
    let identifier = app.identifier.clone();
    let password = app.password.clone();
    std::thread::spawn(move || {
        let result = fetch_timeline_blocking(
            &client,
            public.as_deref(),
            cursor.as_deref(),
            &identifier,
            &password,
        );
        ui_post(move |app| apply_fetch(app, result, replace));
    });
}

fn apply_fetch(
    app: &mut App,
    result: Result<bsky::Timeline, bsky::Error>,
    replace: bool,
) {
    if let Some(tl) = app.timeline.as_ref() {
        unsafe { XtSetSensitive(tl.refresh_btn, TRUE) };
    }
    let timeline = match result {
        Err(e) => {
            let auth_failed = matches!(&e, bsky::Error::Api { error, .. } if is_auth_error(error));
            if auth_failed && app.public_actor.is_none() {
                show_login_with_error(app, &e.to_string());
            } else if let Some(tl) = app.timeline.as_ref() {
                unsafe { set_label(tl.status_label, &format!("Error: {e}")) };
            }
            return;
        }
        Ok(mut t) => {
            // Hidden verification hook: prepend a synthetic post with a known
            // Unicode/emoji string so rendering can be checked deterministically.
            if replace {
                if let Ok(text) = std::env::var("BSKY_TEST_TEXT") {
                    let mut item = bsky::FeedItem::default();
                    item.post.uri = "at://test/post/0".into();
                    item.post.author.handle = "test.bsky.social".into();
                    item.post.author.display_name = Some("Unicode Test 🦋 日本語".into());
                    item.post.record.text = text;
                    item.post.record.created_at = bsky::format_iso8601(bsky::now_unix());
                    t.feed.insert(0, item);
                }
            }
            t
        }
    };

    let dpy = app.gfx.display;
    let Some(tl) = app.timeline.as_mut() else { return };
    if replace {
        for w in tl.rows.drain(..) {
            unsafe { XtDestroyWidget(w) };
        }
        for pm in tl.text_pixmaps.drain(..) {
            unsafe { XFreePixmap(dpy, pm) };
        }
        tl.last_child = None;
        app.posts.clear();
        app.avatars.waiting.clear();
        app.links.clear();
    }

    let now = bsky::now_unix();
    let mut fetch_urls: Vec<String> = Vec::new();
    for item in &timeline.feed {
        let idx = app.posts.len();
        let cached = item
            .post
            .author
            .avatar
            .as_ref()
            .and_then(|u| app.avatars.ready.get(u).copied());
        let (post, avatar_want) = unsafe {
            ui::build_post_row(
                tl,
                item,
                idx,
                cached,
                app.placeholder,
                now,
                &mut app.links,
                &app.text,
            )
        };
        app.posts.push(post);
        if let Some((url, label)) = avatar_want {
            let waiters = app.avatars.waiting.entry(url.clone()).or_default();
            waiters.push(label);
            if waiters.len() == 1 {
                fetch_urls.push(url);
            }
        }
    }

    if replace {
        app.newest_key = timeline.feed.first().map(feed_key);
        if let Some(first) = tl.rows.first().copied() {
            unsafe { XmScrollVisible(tl.sw, first, 8, 8) };
        }
    }
    if timeline.cursor.is_some() {
        app.cursor = timeline.cursor.clone();
    }
    unsafe {
        set_label(tl.status_label, &format!("{} posts", app.posts.len()));
        XtSetSensitive(
            tl.load_more_btn,
            if timeline.cursor.is_some() { TRUE } else { FALSE },
        );
        XmUpdateDisplay(tl.form);
    }

    // Persist rotated tokens so the next launch resumes without a login.
    if app.public_actor.is_none() && !app.identifier.is_empty() {
        if let Some(session) = app.client.lock().unwrap().session.clone() {
            keychain::save(&keychain::StoredCreds {
                identifier: app.identifier.clone(),
                password: app.password.clone(),
                session,
            });
        }
    }

    for url in fetch_urls {
        spawn_avatar_fetch(url);
    }
}

fn spawn_avatar_fetch(url: String) {
    std::thread::spawn(move || {
        let rgba = bsky::fetch_image(&url)
            .ok()
            .and_then(|bytes| ui::decode_avatar(&bytes));
        ui_post(move |app| {
            let Some(rgba) = rgba else {
                app.avatars.waiting.remove(&url);
                return;
            };
            let pm = unsafe { ui::rgba_to_pixmap(&app.gfx, &rgba, ui::AVATAR_PX, ui::AVATAR_PX) };
            app.avatars.ready.insert(url.clone(), pm);
            for label in app.avatars.waiting.remove(&url).unwrap_or_default() {
                unsafe { set_label_pixmap(label, pm) };
            }
            unsafe { XFlush(app.gfx.display) };
        });
    });
}

// ---- Xt callbacks ----

pub extern "C" fn password_verify_cb(_w: Widget, _closure: XtPointer, call_data: XtPointer) {
    with_app(|app| {
        let Some(login) = app.login.as_mut() else { return };
        let cbs = unsafe { &mut *(call_data as *mut XmTextVerifyCallbackStruct) };
        if cbs.text.is_null() {
            return;
        }
        let block = unsafe { &mut *cbs.text };
        let len = login.shadow.len() as c_long;
        let start = cbs.start_pos.clamp(0, len) as usize;
        let end = cbs.end_pos.clamp(cbs.start_pos.clamp(0, len), len) as usize;
        let inserted: Vec<u8> = if block.ptr.is_null() || block.length <= 0 {
            Vec::new()
        } else {
            unsafe { std::slice::from_raw_parts(block.ptr as *const u8, block.length as usize) }
                .to_vec()
        };
        login.shadow.splice(start..end, inserted);
        // The widget's copy shows asterisks; `shadow` keeps the real bytes.
        if !block.ptr.is_null() {
            for i in 0..block.length.max(0) as usize {
                unsafe { *block.ptr.add(i) = b'*' as c_char };
            }
        }
    });
}

pub extern "C" fn login_clicked_cb(_w: Widget, _closure: XtPointer, _cd: XtPointer) {
    with_app(|app| {
        let Some(login) = app.login.as_ref() else { return };
        let identifier = unsafe { text_field_string(login.handle_field) }
            .trim()
            .trim_start_matches('@')
            .to_string();
        let password = String::from_utf8_lossy(&login.shadow).into_owned();
        if identifier.is_empty() || password.is_empty() {
            unsafe { set_label(login.error_label, "Enter a handle and app password") };
            return;
        }
        unsafe {
            XtSetSensitive(login.button, FALSE);
            set_label(login.error_label, "Logging in...");
        }
        app.identifier = identifier.clone();
        app.password = password.clone();
        let client = app.client.clone();
        std::thread::spawn(move || {
            let result = client.lock().unwrap().login(&identifier, &password);
            ui_post(move |app| match result {
                Ok(()) => {
                    if let Some(session) = app.client.lock().unwrap().session.clone() {
                        keychain::save(&keychain::StoredCreds {
                            identifier: app.identifier.clone(),
                            password: app.password.clone(),
                            session,
                        });
                    }
                    show_timeline(app);
                    spawn_fetch(app, None, true);
                }
                Err(e) => {
                    if let Some(login) = app.login.as_ref() {
                        unsafe {
                            XtSetSensitive(login.button, TRUE);
                            set_label(login.error_label, &e.to_string());
                        }
                    }
                }
            });
        });
    });
}

pub extern "C" fn refresh_cb(_w: Widget, _closure: XtPointer, _cd: XtPointer) {
    with_app(|app| spawn_fetch(app, None, true));
}

pub extern "C" fn load_more_cb(_w: Widget, _closure: XtPointer, _cd: XtPointer) {
    with_app(|app| {
        if let Some(cursor) = app.cursor.clone() {
            spawn_fetch(app, Some(cursor), false);
        }
    });
}

fn toggle_post_action(idx: usize, is_like: bool) {
    with_app(|app| {
        if app.public_actor.is_some() {
            return;
        }
        let Some(post) = app.posts.get_mut(idx) else { return };
        if post.busy {
            return;
        }
        post.busy = true;
        let btn = if is_like { post.like_btn } else { post.repost_btn };
        unsafe { XtSetSensitive(btn, FALSE) };
        let client = app.client.clone();
        let uri = post.uri.clone();
        let cid = post.cid.clone();
        let existing = if is_like {
            post.like_uri.clone()
        } else {
            post.repost_uri.clone()
        };
        std::thread::spawn(move || {
            let result = {
                let mut c = client.lock().unwrap();
                match (&existing, is_like) {
                    (Some(record), _) => c.delete_record_uri(record).map(|_| None),
                    (None, true) => c.like(&uri, &cid).map(Some),
                    (None, false) => c.repost(&uri, &cid).map(Some),
                }
            };
            ui_post(move |app| {
                // The row set may have been rebuilt while we were in flight;
                // only apply if this index still refers to the same post.
                let Some(post) = app.posts.get_mut(idx) else { return };
                if post.uri != uri {
                    return;
                }
                post.busy = false;
                let btn = if is_like { post.like_btn } else { post.repost_btn };
                unsafe { XtSetSensitive(btn, TRUE) };
                match result {
                    Ok(new_uri) => {
                        let (count, label) = if is_like {
                            post.like_count = if new_uri.is_some() {
                                post.like_count + 1
                            } else {
                                post.like_count.saturating_sub(1)
                            };
                            post.like_uri = new_uri;
                            (post.like_count, ui::label_for_like(post.like_count, post.like_uri.is_some()))
                        } else {
                            post.repost_count = if new_uri.is_some() {
                                post.repost_count + 1
                            } else {
                                post.repost_count.saturating_sub(1)
                            };
                            post.repost_uri = new_uri;
                            (
                                post.repost_count,
                                ui::label_for_repost(post.repost_count, post.repost_uri.is_some()),
                            )
                        };
                        let _ = count;
                        unsafe { set_label(btn, &label) };
                    }
                    Err(e) => {
                        if let Some(tl) = app.timeline.as_ref() {
                            unsafe {
                                set_label(tl.status_label, &format!("Action failed: {e}"))
                            };
                        }
                    }
                }
            });
        });
    });
}

pub extern "C" fn open_link_cb(_w: Widget, closure: XtPointer, _cd: XtPointer) {
    let idx = closure as usize;
    with_app(|app| {
        if let Some(url) = app.links.get(idx) {
            let _ = std::process::Command::new(open_command()).arg(url).spawn();
        }
    });
}

pub extern "C" fn like_cb(_w: Widget, closure: XtPointer, _cd: XtPointer) {
    toggle_post_action(closure as usize, true);
}

pub extern "C" fn repost_cb(_w: Widget, closure: XtPointer, _cd: XtPointer) {
    toggle_post_action(closure as usize, false);
}

pub extern "C" fn logout_cb(_w: Widget, _closure: XtPointer, _cd: XtPointer) {
    with_app(|app| {
        keychain::clear();
        app.client.lock().unwrap().session = None;
        app.identifier.clear();
        app.password.clear();
        if let Some(tl) = app.timeline.take() {
            unsafe {
                XtDestroyWidget(tl.form);
                for pm in tl.text_pixmaps {
                    XFreePixmap(app.gfx.display, pm);
                }
            }
        }
        app.posts.clear();
        app.avatars.waiting.clear();
        app.links.clear();
        app.cursor = None;
        app.newest_key = None;
        app.login = Some(unsafe { ui::build_login(app.shell, "", &app.fonts) });
    });
}

extern "C" fn timer_cb(_closure: XtPointer, _id: *mut XtIntervalId) {
    with_app(|app| {
        unsafe { XtAppAddTimeOut(app.app_ctx, REFRESH_CHECK_MS, timer_cb, ptr::null_mut()) };
        if app.timeline.is_none() || app.newest_key.is_none() {
            return;
        }
        let client = app.client.clone();
        let public = app.public_actor.clone();
        let newest = app.newest_key.clone();
        std::thread::spawn(move || {
            let head = {
                let mut c = client.lock().unwrap();
                match public.as_deref() {
                    Some(actor) => c.get_public_author_feed(actor, None, 5),
                    None => c.get_timeline(None, 5),
                }
            };
            if let Ok(tl) = head {
                if let Some(first) = tl.feed.first() {
                    if Some(feed_key(first)) != newest {
                        ui_post(|app| {
                            if let Some(tl) = app.timeline.as_ref() {
                                unsafe {
                                    set_label(
                                        tl.status_label,
                                        "New posts available - hit Refresh",
                                    )
                                };
                            }
                        });
                    }
                }
            }
        });
    });
}

extern "C" fn shell_destroy_cb(_w: Widget, _closure: XtPointer, _cd: XtPointer) {
    std::process::exit(0);
}

/// Log X protocol errors instead of dying on them (the Xlib default).
extern "C" fn x_error_cb(dpy: *mut Display, ev: *mut XErrorEvent) -> c_int {
    unsafe {
        let e = &*ev;
        let mut buf = [0 as c_char; 256]; // c_char is i8 on macOS, u8 on Linux
        XGetErrorText(dpy, e.error_code as c_int, buf.as_mut_ptr(), 256);
        eprintln!(
            "X error: {} (request {}.{})",
            CStr::from_ptr(buf.as_ptr()).to_string_lossy(),
            e.request_code,
            e.minor_code
        );
    }
    0
}

// ---- entry ----

/// Browser opener: overridable, else the platform default.
fn open_command() -> String {
    std::env::var("BSKY_OPEN_CMD").unwrap_or_else(|_| {
        if cfg!(target_os = "macos") {
            "/usr/bin/open".into()
        } else {
            "xdg-open".into()
        }
    })
}

/// A resource inside our own .app bundle (`Contents/Resources/<rel>`), if we're
/// running from one. `current_exe` is `.../Contents/MacOS/BskyMotif`.
fn bundle_resource(rel: &str) -> Option<std::path::PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let p = exe.parent()?.join("..").join("Resources").join(rel);
    p.canonicalize().ok().filter(|p| p.exists())
}

fn display_reachable(name: &CStr) -> bool {
    unsafe {
        let d = XOpenDisplay(name.as_ptr());
        if d.is_null() {
            false
        } else {
            XCloseDisplay(d);
            true
        }
    }
}

/// Self-bootstrap the X environment so double-clicking the .app "just works":
/// point fontconfig at the bundled config, and make sure an X server is
/// answering — cold-starting XQuartz on :0 if nothing is. Folding this into the
/// binary (instead of a launcher script) is also what lets the app carry a
/// hardened runtime and be notarized.
#[cfg(target_os = "macos")]
fn ensure_x_display() {
    if std::env::var_os("FONTCONFIG_FILE").is_none() {
        if let Some(cfg) = bundle_resource("fontconfig/fonts.conf") {
            std::env::set_var("FONTCONFIG_FILE", cfg);
        }
    }

    let requested = std::env::var("DISPLAY").unwrap_or_default();
    let target = if requested.is_empty() { ":0" } else { &requested }.to_string();
    let ctarget = std::ffi::CString::new(target.clone()).unwrap_or_default();

    if display_reachable(&ctarget) {
        std::env::set_var("DISPLAY", &target);
        return;
    }

    // Only auto-start XQuartz for the default local display.
    if target == ":0" && std::path::Path::new("/opt/X11/bin/Xquartz").exists() {
        let _ = std::process::Command::new("open")
            .args(["-a", "XQuartz"])
            .status();
        for _ in 0..60 {
            std::thread::sleep(std::time::Duration::from_millis(500));
            if display_reachable(&ctarget) {
                std::env::set_var("DISPLAY", ":0");
                return;
            }
        }
    }

    if target == ":0" {
        let msg = "XQuartz is required and could not be started. Install it from \
                   https://www.xquartz.org (or run: brew install --cask xquartz), \
                   then reopen BskyMotif.";
        let _ = std::process::Command::new("osascript")
            .args(["-e", &format!(r#"display alert "BskyMotif" message "{msg}""#)])
            .status();
        eprintln!("{msg}");
        std::process::exit(1);
    }
    // A non-default display was requested but is unreachable — let Xt report it.
    std::env::set_var("DISPLAY", &target);
}

#[cfg(not(target_os = "macos"))]
fn ensure_x_display() {
    // Native X11 elsewhere; DISPLAY is already set by the session.
}

fn main() {
    ensure_x_display();

    let public_actor = std::env::var("BSKY_PUBLIC_FEED")
        .ok()
        .filter(|s| !s.is_empty());
    let service = std::env::var("BSKY_SERVICE").unwrap_or_else(|_| bsky::DEFAULT_SERVICE.into());

    // Xft needs a UTF-8 locale to keep XmString bytes UTF-8. Default one in if
    // the environment didn't set a UTF-8 locale (macOS terminals often don't).
    if !std::env::var("LC_ALL").is_ok_and(|v| v.contains("UTF-8"))
        && !std::env::var("LC_CTYPE").is_ok_and(|v| v.contains("UTF-8"))
        && !std::env::var("LANG").is_ok_and(|v| v.contains("UTF-8"))
    {
        std::env::set_var("LC_ALL", "en_US.UTF-8");
    }

    unsafe {
        // Install Xt's default language proc (setlocale + X locale) before init.
        XtSetLanguageProc(ptr::null_mut(), ptr::null_mut(), ptr::null_mut());
        let mut app_ctx: XtAppContext = ptr::null_mut();
        let mut argc: c_int = 0;
        let fallback: Vec<*const c_char> = FALLBACK_RESOURCES
            .iter()
            .map(|s| s.as_ptr())
            .chain(std::iter::once(ptr::null()))
            .collect();
        let shell = XtAppInitialize(
            &mut app_ctx,
            c"BskyMotif".as_ptr(),
            ptr::null(),
            0,
            &mut argc,
            ptr::null_mut(),
            fallback.as_ptr(),
            ptr::null(),
            0,
        );
        set_values(
            shell,
            &[
                arg(XmNtitle, c"Bluesky".as_ptr() as XtArgVal),
                arg(XmNallowShellResize, TRUE as XtArgVal),
            ],
        );
        XtAddCallback(
            shell,
            c"destroyCallback".as_ptr(),
            shell_destroy_cb,
            ptr::null_mut(),
        );

        XSetErrorHandler(x_error_cb);

        // Xft render tables (Unicode/emoji). Set the normal one on the shell so
        // every widget inherits it; bold/title are applied per-widget in ui.rs.
        let fonts = ui::Fonts {
            normal: make_render_table(shell, c"Arial Unicode MS:size=12"),
            title: make_render_table(shell, c"Arial Unicode MS:bold:size=18"),
        };
        set_values(shell, &[arg(XmNrenderTable, fonts.normal as XtArgVal)]);

        let gfx = ui::init_gfx(XtDisplay(shell));
        let text = text::TextRenderer::new(&gfx);
        let placeholder = ui::solid_pixmap(&gfx, ui::AVATAR_PX, ui::AVATAR_PX, (0xb0, 0xb4, 0xbc));

        let mut fds: [c_int; 2] = [0; 2];
        libc::pipe(fds.as_mut_ptr());
        PIPE_W.store(fds[1], Ordering::SeqCst);
        XtAppAddInput(app_ctx, fds[0], XtInputReadMask, pipe_cb, ptr::null_mut());

        let stored = if public_actor.is_none() {
            keychain::load()
        } else {
            None
        };
        let client = Client::new(if public_actor.is_some() {
            bsky::PUBLIC_SERVICE
        } else {
            &service
        });

        let mut app = App {
            app_ctx,
            shell,
            gfx,
            client: Arc::new(Mutex::new(client)),
            public_actor,
            login: None,
            timeline: None,
            posts: Vec::new(),
            avatars: ui::AvatarCache::default(),
            placeholder,
            cursor: None,
            newest_key: None,
            identifier: String::new(),
            password: String::new(),
            timer_armed: false,
            fonts,
            text,
            links: Vec::new(),
        };

        match (app.public_actor.is_some(), stored) {
            (true, _) => {
                show_timeline(&mut app);
                spawn_fetch(&mut app, None, true);
            }
            (false, Some(creds)) => {
                app.identifier = creds.identifier;
                app.password = creds.password;
                app.client.lock().unwrap().resume(creds.session);
                show_timeline(&mut app);
                spawn_fetch(&mut app, None, true);
            }
            (false, None) => {
                app.login = Some(ui::build_login(app.shell, "", &app.fonts));
            }
        }

        APP.with(|cell| *cell.borrow_mut() = Some(app));
        XtRealizeWidget(shell);
        XtAppMainLoop(app_ctx);
    }
}
