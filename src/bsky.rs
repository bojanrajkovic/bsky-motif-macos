//! Minimal Bluesky (AT Protocol) XRPC client — just what the timeline needs.

use serde::{Deserialize, Serialize};
use std::io::Read;
use std::time::Duration;

pub const DEFAULT_SERVICE: &str = "https://bsky.social";
pub const PUBLIC_SERVICE: &str = "https://public.api.bsky.app";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub did: String,
    pub handle: String,
    pub access_jwt: String,
    pub refresh_jwt: String,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Author {
    pub did: String,
    pub handle: String,
    pub display_name: Option<String>,
    pub avatar: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Record {
    pub text: String,
    pub created_at: String,
    pub facets: Vec<Facet>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Facet {
    pub index: FacetIndex,
    pub features: Vec<serde_json::Value>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct FacetIndex {
    pub byte_start: usize,
    pub byte_end: usize,
}

impl Record {
    /// Expand truncated link display text to the full URI (Bluesky clients
    /// shorten links in `text`; the real URI lives in the facet feature) and
    /// collect the post's links in order of appearance.
    pub fn expanded_text_and_links(&self) -> (String, Vec<String>) {
        let mut spans: Vec<(usize, usize, String)> = Vec::new();
        for f in &self.facets {
            for feat in &f.features {
                if feat.get("$type").and_then(|t| t.as_str())
                    == Some("app.bsky.richtext.facet#link")
                {
                    if let Some(uri) = feat.get("uri").and_then(|u| u.as_str()) {
                        if f.index.byte_end > f.index.byte_start
                            && f.index.byte_end <= self.text.len()
                        {
                            spans.push((f.index.byte_start, f.index.byte_end, uri.to_owned()));
                        }
                    }
                }
            }
        }
        spans.sort_by_key(|s| s.0);
        let links: Vec<String> = spans.iter().map(|s| s.2.clone()).collect();
        let mut bytes = self.text.as_bytes().to_vec();
        for (start, end, uri) in spans.iter().rev() {
            bytes.splice(*start..*end, uri.bytes());
        }
        (String::from_utf8_lossy(&bytes).into_owned(), links)
    }
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Viewer {
    pub like: Option<String>,
    pub repost: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Post {
    pub uri: String,
    pub cid: String,
    pub author: Author,
    pub record: Record,
    pub like_count: Option<u64>,
    pub repost_count: Option<u64>,
    pub viewer: Option<Viewer>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Reason {
    #[serde(rename = "$type")]
    pub type_: String,
    pub by: Option<Author>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct FeedItem {
    pub post: Post,
    pub reason: Option<Reason>,
    pub reply: Option<serde_json::Value>,
}

impl FeedItem {
    pub fn reposted_by(&self) -> Option<&Author> {
        self.reason
            .as_ref()
            .filter(|r| r.type_.ends_with("reasonRepost"))
            .and_then(|r| r.by.as_ref())
    }

    /// The parent post this replies to, when present and visible (the union
    /// also carries NotFoundPost/BlockedPost, which deserialize to an empty
    /// handle and are treated as absent).
    pub fn reply_parent(&self) -> Option<Post> {
        let parent = self.reply.as_ref()?.get("parent")?;
        let post: Post = serde_json::from_value(parent.clone()).ok()?;
        if post.author.handle.is_empty() {
            return None;
        }
        Some(post)
    }
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
pub struct Timeline {
    pub cursor: Option<String>,
    pub feed: Vec<FeedItem>,
}

#[derive(Debug, Deserialize)]
struct XrpcError {
    error: String,
    #[serde(default)]
    message: String,
}

#[derive(Debug)]
pub enum Error {
    Http(String),
    Api { error: String, message: String },
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Http(e) => write!(f, "network error: {e}"),
            Error::Api { error, message } => {
                if message.is_empty() {
                    write!(f, "{error}")
                } else {
                    write!(f, "{message}")
                }
            }
        }
    }
}

fn to_error(e: ureq::Error) -> Error {
    match e {
        ureq::Error::Status(code, resp) => match resp.into_json::<XrpcError>() {
            Ok(body) => Error::Api {
                error: body.error,
                message: body.message,
            },
            Err(_) => Error::Api {
                error: format!("HTTP {code}"),
                message: String::new(),
            },
        },
        ureq::Error::Transport(t) => Error::Http(t.to_string()),
    }
}

pub struct Client {
    pub service: String,
    pub session: Option<Session>,
    agent: ureq::Agent,
}

impl Client {
    pub fn new(service: &str) -> Self {
        Client {
            service: service.trim_end_matches('/').to_string(),
            session: None,
            agent: ureq::AgentBuilder::new()
                .timeout(Duration::from_secs(20))
                .build(),
        }
    }

    fn url(&self, nsid: &str) -> String {
        format!("{}/xrpc/{}", self.service, nsid)
    }

    pub fn login(&mut self, identifier: &str, password: &str) -> Result<(), Error> {
        let session: Session = self
            .agent
            .post(&self.url("com.atproto.server.createSession"))
            .send_json(serde_json::json!({
                "identifier": identifier,
                "password": password,
            }))
            .map_err(to_error)?
            .into_json()
            .map_err(|e| Error::Http(e.to_string()))?;
        self.session = Some(session);
        Ok(())
    }

    pub fn resume(&mut self, session: Session) {
        self.session = Some(session);
    }

    pub fn refresh_session(&mut self) -> Result<(), Error> {
        let refresh_jwt = match &self.session {
            Some(s) => s.refresh_jwt.clone(),
            None => return Err(Error::Http("no session to refresh".into())),
        };
        let session: Session = self
            .agent
            .post(&self.url("com.atproto.server.refreshSession"))
            .set("Authorization", &format!("Bearer {refresh_jwt}"))
            .call()
            .map_err(to_error)?
            .into_json()
            .map_err(|e| Error::Http(e.to_string()))?;
        self.session = Some(session);
        Ok(())
    }

    /// Run an authenticated request, refreshing the session once on token expiry.
    fn with_auth_retry<T>(
        &mut self,
        run: impl Fn(&ureq::Agent, &str) -> Result<ureq::Response, ureq::Error>,
    ) -> Result<T, Error>
    where
        T: serde::de::DeserializeOwned,
    {
        let access = match &self.session {
            Some(s) => s.access_jwt.clone(),
            None => return Err(Error::Http("not logged in".into())),
        };
        let resp = match run(&self.agent, &access) {
            Ok(r) => r,
            Err(e) => {
                let err = to_error(e);
                let expired = matches!(&err, Error::Api { error, .. } if error == "ExpiredToken");
                if !expired {
                    return Err(err);
                }
                self.refresh_session()?;
                let access = self.session.as_ref().unwrap().access_jwt.clone();
                run(&self.agent, &access).map_err(to_error)?
            }
        };
        resp.into_json().map_err(|e| Error::Http(e.to_string()))
    }

    pub fn get_timeline(&mut self, cursor: Option<&str>, limit: u32) -> Result<Timeline, Error> {
        let url = self.url("app.bsky.feed.getTimeline");
        let cursor = cursor.map(str::to_owned);
        self.with_auth_retry(move |agent, access| {
            let mut req = agent
                .get(&url)
                .set("Authorization", &format!("Bearer {access}"))
                .query("limit", &limit.to_string());
            if let Some(c) = &cursor {
                req = req.query("cursor", c);
            }
            req.call()
        })
    }

    /// Unauthenticated feed from the public AppView — used for headless UI
    /// verification when no session is available.
    pub fn get_public_author_feed(
        &self,
        actor: &str,
        cursor: Option<&str>,
        limit: u32,
    ) -> Result<Timeline, Error> {
        let mut req = self
            .agent
            .get(&self.url("app.bsky.feed.getAuthorFeed"))
            .query("actor", actor)
            .query("filter", "posts_with_replies")
            .query("limit", &limit.to_string());
        if let Some(c) = cursor {
            req = req.query("cursor", c);
        }
        req.call()
            .map_err(to_error)?
            .into_json()
            .map_err(|e| Error::Http(e.to_string()))
    }

    fn create_record(&mut self, collection: &str, uri: &str, cid: &str) -> Result<String, Error> {
        let did = match &self.session {
            Some(s) => s.did.clone(),
            None => return Err(Error::Http("not logged in".into())),
        };
        let url = self.url("com.atproto.repo.createRecord");
        let body = serde_json::json!({
            "repo": did,
            "collection": collection,
            "record": {
                "$type": collection,
                "subject": { "uri": uri, "cid": cid },
                "createdAt": now_iso8601(),
            },
        });
        #[derive(Deserialize)]
        struct Created {
            uri: String,
        }
        let created: Created = self.with_auth_retry(move |agent, access| {
            agent
                .post(&url)
                .set("Authorization", &format!("Bearer {access}"))
                .send_json(body.clone())
        })?;
        Ok(created.uri)
    }

    /// Delete a like/repost record by its at:// URI (collection is parsed out).
    pub fn delete_record_uri(&mut self, record_uri: &str) -> Result<(), Error> {
        let did = match &self.session {
            Some(s) => s.did.clone(),
            None => return Err(Error::Http("not logged in".into())),
        };
        // at://did:plc:xxx/app.bsky.feed.like/rkey
        let mut parts = record_uri.trim_start_matches("at://").splitn(3, '/');
        let (_repo, collection, rkey) = (
            parts.next().unwrap_or_default(),
            parts.next().unwrap_or_default().to_owned(),
            parts.next().unwrap_or_default().to_owned(),
        );
        let url = self.url("com.atproto.repo.deleteRecord");
        let _: serde_json::Value = self.with_auth_retry(move |agent, access| {
            agent
                .post(&url)
                .set("Authorization", &format!("Bearer {access}"))
                .send_json(serde_json::json!({
                    "repo": did,
                    "collection": collection,
                    "rkey": rkey,
                }))
        })?;
        Ok(())
    }

    pub fn like(&mut self, uri: &str, cid: &str) -> Result<String, Error> {
        self.create_record("app.bsky.feed.like", uri, cid)
    }

    pub fn repost(&mut self, uri: &str, cid: &str) -> Result<String, Error> {
        self.create_record("app.bsky.feed.repost", uri, cid)
    }

}

/// Plain GET of an avatar image; standalone so avatar downloads never hold
/// the shared client lock. Capped so a bad URL can't balloon memory.
pub fn fetch_image(url: &str) -> Result<Vec<u8>, Error> {
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(20))
        .build();
    let resp = agent.get(url).call().map_err(to_error)?;
    let mut buf = Vec::new();
    resp.into_reader()
        .take(5 * 1024 * 1024)
        .read_to_end(&mut buf)
        .map_err(|e| Error::Http(e.to_string()))?;
    Ok(buf)
}

// ---- ISO 8601, hand-rolled (Howard Hinnant's civil-days algorithms) ----

/// Days since 1970-01-01 for a civil date. Correct across all leap years.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Parse "2026-08-09T12:34:56.789Z" (or ±HH:MM offset) to unix seconds.
/// Returns None on anything malformed — callers treat that as "no timestamp".
pub fn parse_iso8601(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if b.len() < 19 || b[4] != b'-' || b[7] != b'-' || b[10] != b'T' {
        return None;
    }
    let num = |r: std::ops::Range<usize>| s.get(r)?.parse::<i64>().ok();
    let (y, mo, d) = (num(0..4)?, num(5..7)?, num(8..10)?);
    let (h, mi, sec) = (num(11..13)?, num(14..16)?, num(17..19)?);
    if !(1..=12).contains(&mo) || !(1..=31).contains(&d) {
        return None;
    }
    let mut rest = &s[19..];
    if rest.starts_with('.') {
        let end = rest[1..]
            .find(|c: char| !c.is_ascii_digit())
            .map(|i| i + 1)
            .unwrap_or(rest.len());
        rest = &rest[end..];
    }
    let offset_secs = match rest.as_bytes() {
        [] | [b'Z'] | [b'z'] => 0,
        [sign @ (b'+' | b'-'), ..] if rest.len() == 6 && rest.as_bytes()[3] == b':' => {
            let oh = rest[1..3].parse::<i64>().ok()?;
            let om = rest[4..6].parse::<i64>().ok()?;
            let secs = oh * 3600 + om * 60;
            if *sign == b'+' {
                secs
            } else {
                -secs
            }
        }
        _ => return None,
    };
    Some(days_from_civil(y, mo, d) * 86400 + h * 3600 + mi * 60 + sec - offset_secs)
}

pub fn format_iso8601(unix_secs: i64) -> String {
    let days = unix_secs.div_euclid(86400);
    let rem = unix_secs.rem_euclid(86400);
    let (y, m, d) = civil_from_days(days);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        y,
        m,
        d,
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

pub fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn now_iso8601() -> String {
    format_iso8601(now_unix())
}

/// "now", "5m", "2h", "3d", or "Mar 4" style relative timestamps.
pub fn relative_time(created_unix: i64, now: i64) -> String {
    let dt = now - created_unix;
    match dt {
        i64::MIN..=59 => "now".into(),
        60..=3599 => format!("{}m", dt / 60),
        3600..=86399 => format!("{}h", dt / 3600),
        86400..=2591999 => format!("{}d", dt / 86400),
        _ => {
            let (_, m, d) = civil_from_days(created_unix.div_euclid(86400));
            const MON: [&str; 12] = [
                "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
            ];
            format!("{} {}", MON[(m - 1) as usize], d)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::net::TcpListener;

    #[test]
    fn iso8601_round_trip() {
        for (s, expect) in [
            ("1970-01-01T00:00:00Z", 0),
            ("2026-08-09T04:00:00Z", 1786248000),
            ("2000-02-29T12:00:00.5Z", 951825600),
            ("2026-08-09T04:00:00.123+00:00", 1786248000),
            ("2026-08-09T05:30:00+01:30", 1786248000),
        ] {
            assert_eq!(parse_iso8601(s), Some(expect), "parse {s}");
        }
        assert_eq!(format_iso8601(1786248000), "2026-08-09T04:00:00Z");
        assert_eq!(parse_iso8601("garbage"), None);
        assert_eq!(parse_iso8601("2026-13-09T04:00:00Z"), None);
        // round trip both directions across a spread of instants
        for t in (0..4102444800i64).step_by(86400 * 97 + 3601) {
            assert_eq!(parse_iso8601(&format_iso8601(t)), Some(t));
        }
    }

    #[test]
    fn relative_times() {
        let now = 1786248000;
        assert_eq!(relative_time(now - 30, now), "now");
        assert_eq!(relative_time(now - 300, now), "5m");
        assert_eq!(relative_time(now - 7200, now), "2h");
        assert_eq!(relative_time(now - 3 * 86400, now), "3d");
        assert_eq!(relative_time(parse_iso8601("2026-03-04T12:00:00Z").unwrap(), now), "Mar 4");
    }

    /// Tiny canned-response HTTP server: each connection gets the next body
    /// in the list. Records request lines + bodies for assertions.
    fn mock_server(
        responses: Vec<(u16, &'static str)>,
    ) -> (String, std::sync::mpsc::Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for (status, body) in responses {
                let (mut stream, _) = match listener.accept() {
                    Ok(s) => s,
                    Err(_) => return,
                };
                let mut buf = [0u8; 65536];
                let mut req = Vec::new();
                // read until end of headers, then content-length more
                loop {
                    let n = std::io::Read::read(&mut stream, &mut buf).unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    req.extend_from_slice(&buf[..n]);
                    if let Some(pos) = req.windows(4).position(|w| w == b"\r\n\r\n") {
                        let head = String::from_utf8_lossy(&req[..pos]).to_string();
                        let clen: usize = head
                            .lines()
                            .find_map(|l| {
                                l.to_ascii_lowercase()
                                    .strip_prefix("content-length:")
                                    .map(|v| v.trim().parse().unwrap_or(0))
                            })
                            .unwrap_or(0);
                        while req.len() < pos + 4 + clen {
                            let n = std::io::Read::read(&mut stream, &mut buf).unwrap_or(0);
                            if n == 0 {
                                break;
                            }
                            req.extend_from_slice(&buf[..n]);
                        }
                        break;
                    }
                }
                tx.send(String::from_utf8_lossy(&req).to_string()).ok();
                let resp = format!(
                    "HTTP/1.1 {} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    status,
                    body.len(),
                    body
                );
                stream.write_all(resp.as_bytes()).ok();
            }
        });
        (format!("http://{addr}"), rx)
    }

    const SESSION_JSON: &str = r#"{"did":"did:plc:abc","handle":"test.bsky.social","accessJwt":"access1","refreshJwt":"refresh1"}"#;

    #[test]
    fn login_parses_session() {
        let (base, rx) = mock_server(vec![(200, SESSION_JSON)]);
        let mut c = Client::new(&base);
        c.login("test.bsky.social", "app-pass").unwrap();
        let s = c.session.as_ref().unwrap();
        assert_eq!(s.did, "did:plc:abc");
        assert_eq!(s.access_jwt, "access1");
        let req = rx.recv().unwrap();
        assert!(req.starts_with("POST /xrpc/com.atproto.server.createSession"));
        assert!(req.contains(r#""identifier":"test.bsky.social""#));
    }

    #[test]
    fn login_bad_credentials_is_api_error() {
        let (base, _rx) = mock_server(vec![(
            401,
            r#"{"error":"AuthenticationRequired","message":"Invalid identifier or password"}"#,
        )]);
        let mut c = Client::new(&base);
        let err = c.login("x", "y").unwrap_err();
        assert!(matches!(err, Error::Api { ref error, .. } if error == "AuthenticationRequired"));
    }

    #[test]
    fn expired_token_refreshes_and_retries() {
        let timeline = r#"{"cursor":"c1","feed":[{"post":{"uri":"at://did:plc:abc/app.bsky.feed.post/1","cid":"cid1","author":{"did":"did:plc:abc","handle":"a.bsky.social","displayName":"A"},"record":{"text":"hello","createdAt":"2026-08-09T00:00:00Z"},"likeCount":3}}]}"#;
        let (base, rx) = mock_server(vec![
            (400, r#"{"error":"ExpiredToken","message":"Token has expired"}"#),
            (200, r#"{"did":"did:plc:abc","handle":"test.bsky.social","accessJwt":"access2","refreshJwt":"refresh2"}"#),
            (200, timeline),
        ]);
        let mut c = Client::new(&base);
        c.resume(serde_json::from_str(SESSION_JSON).unwrap());
        let tl = c.get_timeline(None, 30).unwrap();
        assert_eq!(tl.feed.len(), 1);
        assert_eq!(tl.feed[0].post.record.text, "hello");
        assert_eq!(tl.cursor.as_deref(), Some("c1"));
        assert_eq!(c.session.as_ref().unwrap().access_jwt, "access2");
        let first = rx.recv().unwrap();
        assert!(first.contains("Authorization: Bearer access1"));
        let refresh = rx.recv().unwrap();
        assert!(refresh.starts_with("POST /xrpc/com.atproto.server.refreshSession"));
        assert!(refresh.contains("Bearer refresh1"));
        let retry = rx.recv().unwrap();
        assert!(retry.contains("Bearer access2"));
    }

    #[test]
    fn like_posts_create_record_and_unlike_deletes() {
        let (base, rx) = mock_server(vec![
            (200, r#"{"uri":"at://did:plc:abc/app.bsky.feed.like/rkey9","cid":"lc"}"#),
            (200, r#"{}"#),
        ]);
        let mut c = Client::new(&base);
        c.resume(serde_json::from_str(SESSION_JSON).unwrap());
        let like_uri = c.like("at://did:plc:zzz/app.bsky.feed.post/7", "cid7").unwrap();
        assert_eq!(like_uri, "at://did:plc:abc/app.bsky.feed.like/rkey9");
        let req = rx.recv().unwrap();
        assert!(req.contains(r#""collection":"app.bsky.feed.like""#));
        assert!(req.contains(r#""uri":"at://did:plc:zzz/app.bsky.feed.post/7""#));
        assert!(req.contains(r#""$type":"app.bsky.feed.like""#));

        c.delete_record_uri(&like_uri).unwrap();
        let req = rx.recv().unwrap();
        assert!(req.contains(r#""rkey":"rkey9""#));
        assert!(req.contains(r#""collection":"app.bsky.feed.like""#));
    }

    #[test]
    fn real_world_feed_json_parses() {
        let json = include_str!("../tests/fixtures/public_author_feed.json");
        let tl: Timeline = serde_json::from_str(json).unwrap();
        assert!(!tl.feed.is_empty());
        for item in &tl.feed {
            assert!(!item.post.uri.is_empty());
            assert!(!item.post.author.handle.is_empty());
        }
        assert!(tl.feed[0].post.author.avatar.is_some());
    }

    #[test]
    fn timeline_parses_repost_reason_and_reply() {
        let json = r#"{"feed":[
            {"post":{"uri":"u1","cid":"c1","author":{"did":"d","handle":"h"},"record":{"text":"t","createdAt":"x"}},
             "reason":{"$type":"app.bsky.feed.defs#reasonRepost","by":{"did":"d2","handle":"reposter.bsky.social"}}},
            {"post":{"uri":"u2","cid":"c2","author":{"did":"d","handle":"h"},"record":{"text":"t2","createdAt":"x"}},
             "reply":{"parent":{"author":{"handle":"parent.bsky.social"}}}}
        ]}"#;
        let tl: Timeline = serde_json::from_str(json).unwrap();
        assert_eq!(tl.feed[0].reposted_by().unwrap().handle, "reposter.bsky.social");
        assert!(tl.feed[0].reply_parent().is_none());
        assert_eq!(
            tl.feed[1].reply_parent().unwrap().author.handle,
            "parent.bsky.social"
        );
        assert!(tl.feed[1].reposted_by().is_none());
    }

    #[test]
    fn link_facets_expand_with_multibyte_offsets() {
        // "héllo " is 7 bytes; link display "example.com..." spans bytes 7..21.
        let rec: Record = serde_json::from_str(
            r#"{"text":"héllo example.com... end","createdAt":"x","facets":[
                {"index":{"byteStart":7,"byteEnd":21},
                 "features":[{"$type":"app.bsky.richtext.facet#link",
                              "uri":"https://example.com/full/path"}]}]}"#,
        )
        .unwrap();
        let (text, links) = rec.expanded_text_and_links();
        assert_eq!(text, "héllo https://example.com/full/path end");
        assert_eq!(links, vec!["https://example.com/full/path"]);

        // No facets -> text unchanged, no links.
        let plain: Record =
            serde_json::from_str(r#"{"text":"hi","createdAt":"x"}"#).unwrap();
        let (text, links) = plain.expanded_text_and_links();
        assert_eq!(text, "hi");
        assert!(links.is_empty());
    }
}
