// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Reads the sign-in a Microsoft web app keeps in its page (Teams, Outlook on the web): the MSAL
// access tokens, decrypted where the page keeps them encrypted, plus Teams' region settings. Shared
// by the Teams and Outlook extensions: each keeps an identical copy (sign-in-reader.test.js checks
// they match), loads it with importScripts and passes `globalThis.brainsteadReadSignIn` to
// chrome.scripting.executeScript, which runs it in the page.

globalThis.brainsteadReadSignIn = async function readSignIn() {
  // Runs in the Teams page, so it can't use anything outside this function.
  // `seen` is for the hand-over record: what the page keeps, never a secret.
  const out = {
    origin: location.origin,
    rt: null,
    ats: [],
    gtm: null,
    seen: { keys: localStorage.length, encrypted: 0, decrypted: 0, targets: [], problems: [] },
  };
  const plain = []; // entries kept unencrypted: all of them on older MSAL, some on newer
  const take = (e, into = out.ats) => {
    if (e && e.credentialType === "RefreshToken" && e.secret && e.clientId) out.rt = e;
    if (e && e.credentialType === "AccessToken" && e.secret) {
      into.push({ target: e.target, realm: e.realm, clientId: e.clientId, secret: e.secret, expiresOn: e.expiresOn });
      out.seen.targets.push(`${String(e.target || "").split(/\s+/)[0]} until ${new Date(Number(e.expiresOn) * 1000).toISOString()}`);
    }
  };

  // Newer MSAL (which Teams moved to) encrypts its cache entries ({id, nonce, data}) with a key it
  // keeps in the page's `msal.cache.encryption` cookie: AES-GCM under a key derived (HKDF-SHA256)
  // from that key, the entry's nonce as salt and the app's client id as info.
  const unb64 = (t) =>
    Uint8Array.from(atob(t.replace(/-/g, "+").replace(/_/g, "/") + "===".slice((t.length + 3) % 4)), (c) => c.charCodeAt(0));
  let base = null;
  let baseId = null;
  try {
    const c = document.cookie.split("; ").find((x) => x.startsWith("msal.cache.encryption="));
    if (c) {
      const v = JSON.parse(decodeURIComponent(c.slice("msal.cache.encryption=".length)));
      baseId = v.id;
      base = await crypto.subtle.importKey("raw", unb64(v.key), "HKDF", false, ["deriveKey"]);
    }
  } catch (e) {
    out.seen.problems.push(`cache key: ${e?.message || e}`);
  }

  for (let i = 0; i < localStorage.length; i++) {
    const k = localStorage.key(i);
    const v = localStorage.getItem(k) || "";
    if (k.includes("DISCOVER-REGION-GTM")) out.gtm = v;
    let e;
    try {
      e = JSON.parse(v);
    } catch (_) {
      continue; // not a JSON MSAL entry
    }
    if (!(e && e.nonce && e.data)) {
      take(e, plain);
      continue;
    }
    out.seen.encrypted++;
    if (!base) continue;
    if (e.id && baseId && e.id !== baseId) {
      out.seen.problems.push("an entry from an older cache key");
      continue;
    }
    // The client id is part of the entry's key: `…-accesstoken-<client id>-<tenant>-<scopes>`, or
    // `msal.2|<account>|<host>|accesstoken|<client id>|<tenant>|<scopes>|` in newer MSAL.
    const clientId = (k.match(/[-|](?:accesstoken|refreshtoken|idtoken)[-|]([0-9a-f-]{36})[-|]/i) || [])[1];
    if (!clientId) continue; // not a token (an account or app metadata)
    try {
      const key = await crypto.subtle.deriveKey(
        { name: "HKDF", hash: "SHA-256", salt: unb64(e.nonce), info: new TextEncoder().encode(clientId) },
        base,
        { name: "AES-GCM", length: 256 },
        false,
        ["decrypt"],
      );
      const clear = await crypto.subtle.decrypt({ name: "AES-GCM", iv: new Uint8Array(12) }, key, unb64(e.data));
      take(JSON.parse(new TextDecoder().decode(clear)));
      out.seen.decrypted++;
    } catch (err) {
      out.seen.problems.push(`decrypting: ${err?.name || err}`);
    }
  }
  if (!base && out.seen.encrypted) out.seen.problems.push("no cache key cookie");
  // Teams keeps some tokens plain and some encrypted, so both go over. A plain one may be what an
  // earlier sign-in left behind, signed out since: Brainstead picks the longest-lived token for each
  // service (the newest), and stops using one Microsoft turns down.
  out.ats.push(...plain);
  out.seen.problems = [...new Set(out.seen.problems)];
  return out;
};
