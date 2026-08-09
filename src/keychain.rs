//! Session persistence via the `keyring` crate: the macOS Keychain, or the
//! freedesktop Secret Service (GNOME Keyring / KWallet over D-Bus) on Linux.
//! If no store is available the calls fail quietly — the app just won't resume
//! silently on the next launch.

use crate::bsky::Session;
use keyring::Entry;
use serde::{Deserialize, Serialize};

const SERVICE: &str = "io.github.bojanrajkovic.bsky-motif";
const ACCOUNT: &str = "session";

/// Everything needed to resume silently: the session tokens for normal use,
/// and the identifier + app password to re-login if the refresh token dies.
#[derive(Serialize, Deserialize)]
pub struct StoredCreds {
    pub identifier: String,
    pub password: String,
    pub session: Session,
}

/// Reading a Keychain item from a *different* binary than the one that stored
/// it triggers a macOS authorization prompt, so dev iterations set
/// BSKY_NO_KEYCHAIN=1 to skip persistence entirely.
fn disabled() -> bool {
    std::env::var_os("BSKY_NO_KEYCHAIN").is_some()
}

fn entry() -> keyring::Result<Entry> {
    Entry::new(SERVICE, ACCOUNT)
}

pub fn save(creds: &StoredCreds) {
    if disabled() {
        return;
    }
    let json = serde_json::to_string(creds).expect("session serializes");
    if let Err(e) = entry().and_then(|e| e.set_password(&json)) {
        eprintln!("session save failed: {e}");
    }
}

pub fn load() -> Option<StoredCreds> {
    if disabled() {
        return None;
    }
    let json = match entry().and_then(|e| e.get_password()) {
        Ok(json) => json,
        Err(keyring::Error::NoEntry) => return None,
        Err(e) => {
            eprintln!("session load failed: {e}");
            return None;
        }
    };
    serde_json::from_str(&json).ok()
}

pub fn clear() {
    let result = entry().and_then(|e| e.delete_credential());
    if let Err(e) = result {
        if !matches!(e, keyring::Error::NoEntry) {
            eprintln!("session clear failed: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Touches the real store (Keychain / Secret Service) — needs a running
    /// secret service on Linux. Run explicitly with --ignored.
    #[test]
    #[ignore]
    fn round_trip() {
        clear();
        assert!(load().is_none());
        save(&StoredCreds {
            identifier: "test.bsky.social".into(),
            password: "app-pass".into(),
            session: Session {
                did: "did:plc:test".into(),
                handle: "test.bsky.social".into(),
                access_jwt: "a".into(),
                refresh_jwt: "r".into(),
            },
        });
        let loaded = load().expect("stored creds load back");
        assert_eq!(loaded.identifier, "test.bsky.social");
        assert_eq!(loaded.session.did, "did:plc:test");
        clear();
        assert!(load().is_none());
    }
}
