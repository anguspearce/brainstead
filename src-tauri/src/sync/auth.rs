// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! The sign-in the sync reads the user's own Teams and Outlook with (D-20261025-01, D-20261026-01).
//! While a Teams tab is open in the user's browser, the Teams extension hands the page's live access
//! tokens to the app through the native host every few minutes; the app keeps the latest set in the
//! login Keychain (`platform::secret_*`), never in a file, and uses only those to read the user's
//! chats, mail and calendar. It never mints its own tokens (no refresh-token grant): some tenants
//! reject app-refreshed tokens, and using only the browser's own tokens keeps the sync's access to
//! exactly what the browser already has. When the handed-over tokens expire (Teams closed for a
//! while), a read just reports that the user should reopen Teams. The sync only ever reads.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::platform;

/// The Keychain account the sign-in is kept under.
const ACCOUNT: &str = "teams-sign-in";
/// A token with less than this many seconds left is obtained afresh.
const MARGIN_SECS: i64 = 5 * 60;

/// The services the sync reads from, each with the scope its token is requested for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Service {
    /// Chats, meetings and the Teams calendar.
    Spaces,
    /// Mail and the Outlook calendar.
    Outlook,
    /// Meeting transcripts.
    Substrate,
    /// The chat-service aggregator (teams and channels list).
    Csa,
}

impl Service {
    /// What the sync reads with it, as Settings names it.
    pub fn what(self) -> &'static str {
        match self {
            Service::Spaces => "chats",
            Service::Outlook => "mail",
            Service::Substrate => "transcripts",
            Service::Csa => "channel list",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Service::Spaces => "spaces",
            Service::Outlook => "outlook",
            Service::Substrate => "substrate",
            Service::Csa => "csa",
        }
    }
}

/// An access token and the second (since 1970) it stops being valid.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Token {
    pub token: String,
    pub expires: i64,
}

/// The sign-in, as kept in the Keychain.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct SignIn {
    /// The Teams page the sign-in came from (kept for reference; the sync no longer refreshes tokens).
    pub origin: String,
    pub client_id: String,
    pub tenant: String,
    /// The MSAL refresh token from the handover, when the page still keeps one there (Teams has stopped
    /// doing so). Unused: the sync does not mint tokens (D-20261026-01).
    pub refresh_token: String,
    /// Cached service tokens, by [`Service::key`].
    pub tokens: BTreeMap<String, Token>,
    /// The chat service address for the user's region (`…/api/chatsvc/<region>`).
    pub chat_service: String,
    /// The middle-tier address (`…/api/mt/part/<region>`), for the Teams calendar.
    pub middle_tier: String,
    /// The chat-service aggregator address (`…/api/csa/<region>`), for the teams and channels list.
    #[serde(default)]
    pub csa_service: String,
    /// When the extension last handed the sign-in over (RFC 3339).
    pub handed_over: String,
    /// The Teams extension's version, as it sent it with the handover (empty from versions before
    /// 1.1.8, which didn't send one).
    #[serde(default)]
    pub extension_version: String,
    /// Services whose token Microsoft turned down (a 401: signed out, or the session ended) →
    /// a fingerprint of that token, so the same dead token handed over again isn't used.
    #[serde(default)]
    pub rejected: BTreeMap<String, String>,
}

/// What Settings shows about the sign-in — nothing secret.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SignInState {
    pub signed_in: bool,
    pub handed_over: Option<String>,
    /// What the sync can't reach while signed in, because that service's token is missing or has
    /// run out: "chats", "mail", "transcripts", "channel list".
    pub stale: Vec<String>,
    /// What Microsoft turned the sign-in down for (a 401), until a new sign-in is handed over:
    /// "chats", "mail", … Signing in again to Teams (or Outlook, for mail) in Chrome clears it.
    pub rejected: Vec<String>,
    /// The Teams extension Chrome is running, when it's older than the one this build ships (Chrome
    /// only loads an unpacked extension's new files when it's reloaded): "" for one too old to say.
    pub extension_outdated: Option<String>,
    /// The Teams extension version this build ships.
    pub extension_shipped: String,
}

/// The Teams extension version this build ships, from its manifest.
pub fn shipped_extension_version() -> String {
    let m: Value = serde_json::from_str(include_str!("../../../extensions/teams-capture/manifest.json")).unwrap_or(Value::Null);
    m["version"].as_str().unwrap_or("").to_string()
}

/// Whether version `a` (dotted numbers; empty for unknown) is older than `b`.
fn older(a: &str, b: &str) -> bool {
    let parts = |v: &str| v.split('.').map(|n| n.parse::<u64>().unwrap_or(0)).collect::<Vec<_>>();
    a.is_empty() || parts(a) < parts(b)
}

pub fn now_secs() -> i64 {
    chrono::Utc::now().timestamp()
}

/// A JWT's claims, decoded (not verified — only Microsoft's own services verify the signature).
pub fn claims(jwt: &str) -> Value {
    use base64::Engine;
    let part = jwt.split('.').nth(1).unwrap_or("");
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(part.trim_end_matches('='))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or(Value::Null)
}

fn jwt_expiry(jwt: &str) -> i64 {
    claims(jwt)["exp"].as_i64().unwrap_or(0)
}

/// Reads the sign-in from the Keychain.
pub fn load() -> Option<SignIn> {
    platform::secret_get(ACCOUNT).and_then(|s| serde_json::from_str(&s).ok())
}

/// Keeps the sign-in in the Keychain.
pub fn store(s: &SignIn) -> Result<(), String> {
    platform::secret_set(ACCOUNT, &serde_json::to_string(s).map_err(|e| e.to_string())?)
}

/// Forgets the sign-in (Settings › the user signs out, or the refresh token stops working).
pub fn forget() -> Result<(), String> {
    platform::secret_delete(ACCOUNT)
}

/// What Settings shows. Signed in means we currently hold at least one unexpired token from a
/// handover — once they all expire (Teams closed in the browser for a while), it reads as not
/// signed in, prompting the user to reopen Teams.
pub fn state() -> SignInState {
    match load() {
        Some(s) => {
            let now = now_secs();
            let signed_in = s.tokens.values().any(|t| t.expires > now + MARGIN_SECS);
            let all = [Service::Spaces, Service::Outlook, Service::Substrate, Service::Csa];
            let rejected: Vec<String> =
                all.into_iter().filter(|svc| s.rejected.contains_key(svc.key())).map(|svc| svc.what().to_string()).collect();
            let stale = all
                .into_iter()
                .filter(|svc| !s.rejected.contains_key(svc.key()))
                .filter(|svc| s.tokens.get(svc.key()).is_none_or(|t| t.expires <= now + MARGIN_SECS))
                .map(|svc| svc.what().to_string())
                .collect();
            let shipped = shipped_extension_version();
            let extension_outdated = older(&s.extension_version, &shipped).then(|| s.extension_version.clone());
            SignInState { signed_in, handed_over: Some(s.handed_over), stale, rejected, extension_outdated, extension_shipped: shipped }
        }
        None => SignInState {
            signed_in: false,
            handed_over: None,
            stale: Vec::new(),
            rejected: Vec::new(),
            extension_outdated: None,
            extension_shipped: shipped_extension_version(),
        },
    }
}

/// Builds a sign-in from a handover message from the Teams extension. The message carries the Teams
/// page's own sign-in state (`origin`, the MSAL refresh-token and access-token entries, and Teams'
/// region settings), which the extension read from the open tab. Returns None if it carries no live
/// token the sync uses, so a handover from a signed-out tab doesn't replace a good sign-in.
pub fn from_handover(v: &Value) -> Option<SignIn> {
    let origin = v["origin"].as_str()?.to_string();
    let rt = &v["rt"];
    let refresh_token = rt["secret"].as_str().unwrap_or("").to_string();
    let client_id = rt["clientId"].as_str().unwrap_or("").to_string();
    let ats = v["ats"].as_array().cloned().unwrap_or_default();
    let tenant = ats.iter().find_map(|a| a["realm"].as_str()).unwrap_or("common").to_string();

    // Teams' region settings, which say where this user's chat service and middle tier live.
    let gtm: Value = v["gtm"].as_str().and_then(|s| serde_json::from_str(s).ok()).unwrap_or(Value::Null);
    let chat_service = gtm["item"]["chatServiceAfd"].as_str().unwrap_or("").to_string();
    let middle_tier = gtm["item"]["middleTier"].as_str().unwrap_or("").to_string();
    let csa_service = gtm["item"]["chatSvcAggAfd"].as_str().unwrap_or("").to_string();

    // The page's own live access tokens, as handed over — the only tokens the sync uses.
    let now = now_secs();
    let mut tokens = BTreeMap::new();
    for svc in [Service::Spaces, Service::Outlook, Service::Substrate, Service::Csa] {
        if let Some(best) = best_page_token(&ats, svc, now) {
            tokens.insert(svc.key().to_string(), best);
        }
    }
    if tokens.is_empty() {
        return None;
    }

    Some(SignIn {
        origin,
        client_id,
        tenant,
        refresh_token,
        tokens,
        chat_service,
        middle_tier,
        csa_service,
        handed_over: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        extension_version: v["version"].as_str().unwrap_or("").to_string(),
        rejected: BTreeMap::new(),
    })
}

/// A Teams handover with the earlier sign-in's live tokens for the services it lacks kept, so the
/// mail token the Outlook extension handed over survives a Teams page that has none.
pub fn keep_missing(mut new: SignIn, old: Option<SignIn>) -> SignIn {
    let now = now_secs();
    for (k, t) in old.map(|o| o.tokens).unwrap_or_default() {
        if t.expires > now + MARGIN_SECS {
            new.tokens.entry(k).or_insert(t);
        }
    }
    new
}

/// The sign-in with the mail token from an Outlook-extension handover (an Outlook on the web tab's
/// tokens) put in, or None when the tab has no live one. Nothing else changes; with no sign-in yet it
/// starts one holding only the mail token.
pub fn with_mail_handover(old: Option<SignIn>, v: &Value) -> Option<SignIn> {
    let ats = v["ats"].as_array().cloned().unwrap_or_default();
    let token = best_page_token(&ats, Service::Outlook, now_secs())?;
    let mut s = old.unwrap_or_else(|| SignIn {
        origin: v["origin"].as_str().unwrap_or("").to_string(),
        tenant: ats.iter().find_map(|a| a["realm"].as_str()).unwrap_or("common").to_string(),
        handed_over: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        ..SignIn::default()
    });
    s.tokens.insert(Service::Outlook.key().to_string(), token);
    Some(s)
}

/// A short fingerprint of a token (the end of its signature), to recognise it again.
fn fingerprint(token: &str) -> String {
    token.chars().rev().take(16).collect()
}

/// Microsoft turned `svc`'s token down (a 401): drops it and remembers it, so the sync says to sign
/// in again rather than retrying it, until a different token for `svc` is handed over.
pub fn reject(svc: Service) {
    let Some(mut s) = load() else { return };
    if let Some(t) = s.tokens.remove(svc.key()) {
        s.rejected.insert(svc.key().to_string(), fingerprint(&t.token));
        let _ = store(&s);
    }
}

/// A handed-over sign-in with what the earlier one had rejected applied: a service handed the same
/// dead token again stays rejected (and the token is dropped); one handed a new token is cleared.
pub fn keep_rejected(mut new: SignIn, old: Option<&SignIn>) -> SignIn {
    let mut still = BTreeMap::new();
    for (k, fp) in old.map(|o| o.rejected.clone()).unwrap_or_default() {
        match new.tokens.get(&k) {
            Some(t) if fingerprint(&t.token) != fp => {}
            _ => {
                new.tokens.remove(&k);
                still.insert(k, fp);
            }
        }
    }
    new.rejected = still;
    new
}

/// The longest-lived unexpired access token in the page for `svc`.
fn best_page_token(ats: &[Value], svc: Service, now: i64) -> Option<Token> {
    ats.iter()
        .filter_map(|a| {
            let token = a["secret"].as_str()?.to_string();
            let c = claims(&token);
            let aud = c["aud"].as_str().unwrap_or("").trim_end_matches('/');
            let scp = c["scp"].as_str().unwrap_or("");
            let matches = match svc {
                Service::Spaces => aud == "https://api.spaces.skype.com",
                Service::Outlook => aud == "https://outlook.office.com" && scp.contains("Mail.Read"),
                Service::Substrate => aud == "https://substrate.office.com",
                Service::Csa => aud == "https://chatsvcagg.teams.microsoft.com",
            };
            let expires = jwt_expiry(&token);
            (matches && expires > now + MARGIN_SECS).then_some(Token { token, expires })
        })
        .max_by_key(|t| t.expires)
}

/// A valid access token for `svc` from the latest handover. The sync only ever uses the tokens the
/// browser itself holds (handed over by the Teams extension while a Teams tab is open) — it never
/// mints its own, because some tenants reject app-refreshed tokens and because it keeps the sync's
/// access strictly to what the browser already has (D-20261026-01). A token not present or expired is
/// an error asking the user to keep Teams open so the extension hands a fresh one over.
pub fn token(svc: Service) -> Result<String, String> {
    let s = load().ok_or("Not signed in to Teams. Open Teams in your browser so the extension hands the sign-in over.")?;
    if s.rejected.contains_key(svc.key()) {
        let app = if svc == Service::Outlook { "Outlook" } else { "Teams" };
        return Err(format!(
            "Microsoft turned down the {} sign-in. If {app} in Chrome asks you to sign in, do so; the extension hands the new sign-in over within 15 minutes.",
            svc.what()
        ));
    }
    match s.tokens.get(svc.key()) {
        Some(t) if t.expires > now_secs() + MARGIN_SECS => Ok(t.token.clone()),
        _ => {
            Err("The Teams sign-in has gone stale. Keep Teams open in your browser (Chrome) so the extension hands a fresh one over."
                .into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A JWT (unsigned payload only) with the given audience, scope and expiry — enough for the
    /// parsing the sync does, which never verifies the signature.
    fn jwt(aud: &str, scp: &str, exp: i64) -> String {
        use base64::Engine;
        let enc = |v: &Value| base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(serde_json::to_vec(v).unwrap());
        format!("{}.{}.sig", enc(&json!({"alg": "RS256"})), enc(&json!({"aud": aud, "scp": scp, "exp": exp})))
    }

    #[test]
    fn spots_an_older_extension() {
        assert!(older("", "1.1.8"), "too old to send a version");
        assert!(older("1.1.7", "1.1.8"));
        assert!(older("1.9", "1.10"), "compared as numbers");
        assert!(!older("1.1.8", "1.1.8"));
        assert!(!older("1.2.0", "1.1.8"));
        assert!(!shipped_extension_version().is_empty());
    }

    #[test]
    fn reads_a_handover_and_picks_the_live_service_tokens() {
        let now = now_secs();
        let good = now + 3600;
        let stale = now + 60; // inside the margin, so not carried over
        let v = json!({
            "origin": "https://teams.cloud.microsoft",
            "rt": { "credentialType": "RefreshToken", "secret": "refresh-xyz", "clientId": "5e3ce6c0" },
            "ats": [
                { "secret": jwt("https://api.spaces.skype.com", "Authorization.ReadWrite", good), "realm": "tenant-1" },
                { "secret": jwt("https://outlook.office.com", "Mail.ReadWrite Calendars.ReadWrite", good), "realm": "tenant-1" },
                { "secret": jwt("https://substrate.office.com", "SubstrateSearch-Internal.ReadWrite", stale), "realm": "tenant-1" },
                { "secret": jwt("https://graph.microsoft.com", "User.Read", good), "realm": "tenant-1" },
            ],
            "gtm": serde_json::to_string(&json!({"item": {"chatServiceAfd": "https://teams.microsoft.com/api/chatsvc/emea", "middleTier": "https://teams.microsoft.com/api/mt/part/emea-02"}})).unwrap(),
        });
        let s = from_handover(&v).expect("a sign-in");
        assert_eq!(s.origin, "https://teams.cloud.microsoft");
        assert_eq!(s.refresh_token, "refresh-xyz");
        assert_eq!(s.client_id, "5e3ce6c0");
        assert_eq!(s.tenant, "tenant-1");
        assert_eq!(s.chat_service, "https://teams.microsoft.com/api/chatsvc/emea");
        assert_eq!(s.middle_tier, "https://teams.microsoft.com/api/mt/part/emea-02");
        // Live Spaces and Outlook carried over; the stale Substrate and the unrelated Graph are not.
        assert!(s.tokens.contains_key(Service::Spaces.key()));
        assert!(s.tokens.contains_key(Service::Outlook.key()));
        assert!(!s.tokens.contains_key(Service::Substrate.key()));
        assert_eq!(s.tokens.len(), 2);
    }

    #[test]
    fn the_outlook_tab_supplies_the_mail_token() {
        let now = now_secs();
        let teams = json!({
            "origin": "https://teams.cloud.microsoft",
            "ats": [{ "secret": jwt("https://api.spaces.skype.com", "Authorization.ReadWrite", now + 3600), "realm": "t" }],
            "gtm": "{}",
        });
        let outlook = json!({
            "origin": "https://outlook.cloud.microsoft",
            "ats": [
                { "secret": jwt("https://outlook.office.com", "Mail.ReadWrite Calendars.ReadWrite", now + 3600), "realm": "t" },
                { "secret": jwt("https://substrate.office.com", "SubstrateSearch-Internal.ReadWrite", now + 3600), "realm": "t" },
            ],
        });
        // Only the mail token is taken from the Outlook tab; the Teams sign-in is otherwise untouched.
        let s = with_mail_handover(from_handover(&teams), &outlook).expect("a sign-in");
        assert_eq!(s.origin, "https://teams.cloud.microsoft");
        assert!(s.tokens.contains_key(Service::Outlook.key()));
        assert!(s.tokens.contains_key(Service::Spaces.key()));
        assert!(!s.tokens.contains_key(Service::Substrate.key()));
        // A later Teams handover without a mail token keeps the Outlook tab's.
        let again = keep_missing(from_handover(&teams).unwrap(), Some(s));
        assert!(again.tokens.contains_key(Service::Outlook.key()));
        // An Outlook tab with no mail token changes nothing.
        assert!(with_mail_handover(None, &json!({ "ats": [] })).is_none());
    }

    #[test]
    fn a_rejected_token_stays_rejected_until_a_new_one_arrives() {
        let now = now_secs();
        let dead = jwt("https://api.spaces.skype.com", "Authorization.ReadWrite", now + 3600);
        let handover = |t: &str| {
            json!({ "origin": "https://teams.cloud.microsoft", "ats": [
                { "secret": t, "realm": "t" },
                { "secret": jwt("https://substrate.office.com", "x", now + 3600), "realm": "t" },
            ], "gtm": "{}" })
        };
        let mut old = from_handover(&handover(&dead)).unwrap();
        // As reject() does on a 401.
        let t = old.tokens.remove(Service::Spaces.key()).unwrap();
        old.rejected.insert(Service::Spaces.key().into(), fingerprint(&t.token));
        // The same dead token handed over again stays out.
        let again = keep_rejected(from_handover(&handover(&dead)).unwrap(), Some(&old));
        assert!(!again.tokens.contains_key(Service::Spaces.key()));
        assert!(again.rejected.contains_key(Service::Spaces.key()));
        assert!(again.tokens.contains_key(Service::Substrate.key()));
        // A new one (after signing in again) clears it.
        let fresh = jwt("https://api.spaces.skype.com", "Authorization.ReadWrite", now + 7200);
        let back = keep_rejected(from_handover(&handover(&fresh)).unwrap(), Some(&again));
        assert!(back.tokens.contains_key(Service::Spaces.key()));
        assert!(back.rejected.is_empty());
    }

    #[test]
    fn a_handover_with_no_live_token_is_refused() {
        let v = json!({ "origin": "https://teams.microsoft.com", "ats": [], "gtm": "{}" });
        assert!(from_handover(&v).is_none());
    }

    #[test]
    fn a_handover_needs_no_refresh_token() {
        // Teams no longer keeps a refresh token in the page; the access tokens are enough.
        let now = now_secs();
        let v = json!({
            "origin": "https://teams.cloud.microsoft",
            "ats": [{ "secret": jwt("https://api.spaces.skype.com", "Authorization.ReadWrite", now + 3600), "realm": "t" }],
            "gtm": "{}",
        });
        let s = from_handover(&v).expect("a sign-in");
        assert_eq!(s.refresh_token, "");
        assert!(s.tokens.contains_key(Service::Spaces.key()));
    }

    #[test]
    fn outlook_only_takes_a_mail_scoped_token() {
        // An Outlook-audience token without Mail.Read (e.g. calendar-only) isn't picked as Outlook.
        let now = now_secs();
        let v = json!({
            "origin": "https://teams.microsoft.com",
            "rt": { "credentialType": "RefreshToken", "secret": "r", "clientId": "c" },
            "ats": [
                { "secret": jwt("https://outlook.office.com", "Calendars.Read", now + 3600), "realm": "t" },
                { "secret": jwt("https://api.spaces.skype.com", "Authorization.ReadWrite", now + 3600), "realm": "t" },
            ],
            "gtm": "{}",
        });
        let s = from_handover(&v).unwrap();
        assert!(!s.tokens.contains_key(Service::Outlook.key()));
    }
}
