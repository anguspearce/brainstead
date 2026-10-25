// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! The sign-in the sync reads the user's own Teams and Outlook with (D-20261025-01). While a Teams
//! tab is open in the user's browser, the Teams extension hands the page's sign-in to the app
//! through the native host; the app keeps it in the login Keychain (`platform::secret_*`), never in
//! a file, and uses it to read the user's chats, mail and calendar. A service token is obtained from
//! the refresh token the standard way (an OAuth refresh_token grant against the user's own Microsoft
//! tenant), exactly as the Teams web app itself does; the sync only ever reads.

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
    pub fn scope(self) -> &'static str {
        match self {
            Service::Spaces => "https://api.spaces.skype.com/.default offline_access",
            Service::Outlook => "https://outlook.office.com/.default offline_access",
            Service::Substrate => "https://substrate.office.com/.default offline_access",
            Service::Csa => "https://chatsvcagg.teams.microsoft.com/.default offline_access",
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
    /// The Teams page the sign-in came from; sent as the Origin when refreshing, which the Teams
    /// web-app client requires.
    pub origin: String,
    pub client_id: String,
    pub tenant: String,
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
}

/// What Settings shows about the sign-in — nothing secret.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SignInState {
    pub signed_in: bool,
    pub handed_over: Option<String>,
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

/// What Settings shows.
pub fn state() -> SignInState {
    match load() {
        Some(s) => SignInState { signed_in: !s.refresh_token.is_empty(), handed_over: Some(s.handed_over) },
        None => SignInState { signed_in: false, handed_over: None },
    }
}

/// Microsoft's token endpoint for the user's tenant.
fn token_endpoint(tenant: &str) -> String {
    format!("https://login.microsoftonline.com/{tenant}/oauth2/v2.0/token")
}

/// Builds a sign-in from a handover message from the Teams extension. The message carries the Teams
/// page's own sign-in state (`origin`, the MSAL refresh-token and access-token entries, and Teams'
/// region settings), which the extension read from the open tab. Returns None if there's no refresh
/// token to go on.
pub fn from_handover(v: &Value) -> Option<SignIn> {
    let origin = v["origin"].as_str()?.to_string();
    let rt = v["rt"].as_object()?;
    let refresh_token = rt.get("secret")?.as_str()?.to_string();
    let client_id = rt.get("clientId")?.as_str()?.to_string();
    let ats = v["ats"].as_array().cloned().unwrap_or_default();
    let tenant = ats.iter().find_map(|a| a["realm"].as_str()).unwrap_or("common").to_string();

    // Teams' region settings, which say where this user's chat service and middle tier live.
    let gtm: Value = v["gtm"].as_str().and_then(|s| serde_json::from_str(s).ok()).unwrap_or(Value::Null);
    let chat_service = gtm["item"]["chatServiceAfd"].as_str().unwrap_or("").to_string();
    let middle_tier = gtm["item"]["middleTier"].as_str().unwrap_or("").to_string();
    let csa_service = gtm["item"]["chatSvcAggAfd"].as_str().unwrap_or("").to_string();

    // Carry over any still-valid service tokens from the page, so the first sync needn't refresh.
    let now = now_secs();
    let mut tokens = BTreeMap::new();
    for svc in [Service::Spaces, Service::Outlook, Service::Substrate, Service::Csa] {
        if let Some(best) = best_page_token(&ats, svc, now) {
            tokens.insert(svc.key().to_string(), best);
        }
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
    })
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

/// The response from Microsoft's token endpoint.
#[derive(Deserialize)]
struct GrantResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

/// A valid access token for `svc`, from the cache when it still has time left, otherwise obtained
/// afresh from the refresh token and cached. A refreshed sign-in (the new refresh token Microsoft
/// returns, and the new token) is written back to the Keychain.
pub fn token(client: &reqwest::blocking::Client, svc: Service) -> Result<String, String> {
    let mut s = load().ok_or("Not signed in to Teams. Open Teams in your browser so the extension can hand the sign-in over.")?;
    let now = now_secs();
    if let Some(t) = s.tokens.get(svc.key()) {
        if t.expires > now + MARGIN_SECS {
            return Ok(t.token.clone());
        }
    }
    let (access, new_refresh) = refresh(client, &s, svc.scope())?;
    let expires = jwt_expiry(&access);
    s.tokens.insert(svc.key().to_string(), Token { token: access.clone(), expires });
    if let Some(rt) = new_refresh {
        s.refresh_token = rt;
    }
    store(&s)?;
    Ok(access)
}

/// Exchanges the refresh token for an access token for `scope` (OAuth refresh_token grant against the
/// user's own tenant). Returns the access token and, when the endpoint rotated it, a new refresh
/// token. An invalid-grant error means the sign-in has lapsed and the user must reopen Teams.
fn refresh(client: &reqwest::blocking::Client, s: &SignIn, scope: &str) -> Result<(String, Option<String>), String> {
    let origin = if s.origin.is_empty() { "https://teams.microsoft.com" } else { &s.origin };
    let resp = client
        .post(token_endpoint(&s.tenant))
        .header("Origin", origin)
        .form(&[
            ("grant_type", "refresh_token"),
            ("client_id", s.client_id.as_str()),
            ("refresh_token", s.refresh_token.as_str()),
            ("scope", scope),
        ])
        .send()
        .map_err(|e| format!("Couldn't reach Microsoft sign-in: {e}"))?;
    let status = resp.status();
    let body: GrantResponse = resp.json().map_err(|e| format!("Microsoft sign-in gave an unreadable answer: {e}"))?;
    if !status.is_success() {
        if body.error.as_deref() == Some("invalid_grant") {
            let _ = forget();
            return Err("Your Teams sign-in has lapsed. Open Teams in your browser to sign in again.".into());
        }
        let msg = body.error_description.or(body.error).unwrap_or_else(|| status.to_string());
        return Err(format!("Microsoft sign-in refused: {}", msg.lines().next().unwrap_or("").trim()));
    }
    let access = body.access_token.ok_or("Microsoft sign-in returned no token.")?;
    Ok((access, body.refresh_token))
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
    fn a_handover_without_a_refresh_token_is_refused() {
        let v = json!({ "origin": "https://teams.microsoft.com", "ats": [], "gtm": "{}" });
        assert!(from_handover(&v).is_none());
    }

    #[test]
    fn outlook_only_takes_a_mail_scoped_token() {
        // An Outlook-audience token without Mail.Read (e.g. calendar-only) isn't picked as Outlook.
        let now = now_secs();
        let v = json!({
            "origin": "https://teams.microsoft.com",
            "rt": { "credentialType": "RefreshToken", "secret": "r", "clientId": "c" },
            "ats": [{ "secret": jwt("https://outlook.office.com", "Calendars.Read", now + 3600), "realm": "t" }],
            "gtm": "{}",
        });
        let s = from_handover(&v).unwrap();
        assert!(!s.tokens.contains_key(Service::Outlook.key()));
    }
}
