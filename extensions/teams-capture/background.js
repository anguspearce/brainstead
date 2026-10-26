// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// MV3 service worker. Owns the capture; the popup only starts it and renders
// progress.
//
// WHY: Chrome destroys an action popup's JS context the moment it loses focus.
// The capture used to run there, which meant `await executeScript(...)` resolved
// a long-scrolled transcript into a context that no longer existed — switching
// tabs part-way through a big channel thread or meeting recap threw the whole
// capture away, even though the page-side scroll loop had already finished the
// work. Here, nothing about the UI can interrupt it.
//
// A service worker idles out after 30s, but each `chrome.*` call resets that and
// an in-flight one holds it open; the extractor's scroll loop is a single long
// `executeScript` await, so the worker stays alive for its duration.
//
// Progress arrives from the page as `teams-ingest-progress` messages (the
// content script emits them during its scroll passes). Those are recorded into
// `chrome.storage.session` so a popup opened half-way through shows the real
// state, and re-broadcast for a popup that is already listening.

const STATE_KEY = "run";

/** A run whose heartbeat is this cold was killed without reporting (worker
 *  crash, browser shutdown); better to say so than to spin forever. */
const STALE_RUN_MS = 120_000;

const TEAMS_HOST_RE = /^https:\/\/teams\.(microsoft\.com|live\.com|cloud\.microsoft|microsoft\.us|office365\.us)\//;
/** Microsoft's meeting recap (its Transcript tab included), which Teams now loads into its recap
 *  frame (`RecapxPlatIframe`) from the organiser's SharePoint rather than from a Teams host. */
const RECAP_FRAME_RE = /^https:\/\/[a-z0-9-]+\.sharepoint\.(?:com|us)\/(?:.*\/)?_layouts\/15\/xplatplugins\.aspx/i;

const emptyState = () => ({
  status: "idle", // idle | running | done
  message: null, // { text, kind }
  startedAt: null,
  finishedAt: null,
  lastBeatAt: null,
});

async function readState() {
  const stored = await chrome.storage.session.get(STATE_KEY);
  const state = stored[STATE_KEY];
  if (!state) return emptyState();
  if (state.status === "running" && state.lastBeatAt && Date.now() - state.lastBeatAt > STALE_RUN_MS) {
    return {
      ...state,
      status: "done",
      message: { text: "Interrupted before the capture finished. Nothing was written.", kind: "error" },
      finishedAt: Date.now(),
    };
  }
  return state;
}

let writing = Promise.resolve();
function updateState(mutate) {
  writing = writing.then(async () => {
    const state = await readState();
    const next = mutate({ ...state });
    next.lastBeatAt = Date.now();
    await chrome.storage.session.set({ [STATE_KEY]: next });
    chrome.runtime.sendMessage({ type: "teams:progress", state: next }).catch(() => {});
    paintBadge(next);
    return next;
  });
  return writing;
}

function paintBadge(state) {
  if (state.status === "running") {
    chrome.action.setBadgeText({ text: "…" });
    chrome.action.setBadgeBackgroundColor({ color: "#5b5fa6" });
    return;
  }
  if (state.status === "done") {
    const bad = state.message?.kind === "error";
    chrome.action.setBadgeText({ text: bad ? "!" : "✓" });
    chrome.action.setBadgeBackgroundColor({ color: bad ? "#B3261E" : "#1B7031" });
    return;
  }
  chrome.action.setBadgeText({ text: "" });
}

const setMessage = (text, kind = null) => updateState((s) => ({ ...s, message: { text, kind } }));

chrome.runtime.onMessage.addListener((msg, _sender, respond) => {
  if (!msg || typeof msg.type !== "string") return false;
  switch (msg.type) {
    case "teams:get-state":
      // The rejection path matters as much as the happy one: this listener has
      // already returned `true`, so if `respond` is never called Chrome closes
      // the channel and the popup's `sendMessage` REJECTS. The popup used to
      // swallow that and render nothing, with no clue why.
      void readState().then(respond, (err) => respond({ error: err instanceof Error ? err.message : String(err) }));
      return true;
    case "teams:clear-badge":
      chrome.action.setBadgeText({ text: "" });
      respond({ ok: true });
      return true;
    case "teams:start":
      void startRun(msg.tabId);
      respond({ started: true });
      return true;
    // Progress from the content script's scroll loop. Recorded so a popup that
    // is closed (or opened later) still sees where the capture got to.
    case "teams-ingest-progress":
      void setMessage(describeProgress(msg));
      return false;
    default:
      return false;
  }
});

function describeProgress(msg) {
  if (msg.pass === 0) {
    return `Captured ${msg.captured} messages from initial view — scrolling for history…`;
  }
  const top = msg.atTop ? " (reached top)" : "";
  return `Pass ${msg.pass}: ${msg.captured} messages so far${top}…`;
}

/**
 * One capture at a time. `active` is assigned synchronously: checking
 * `status === "running"` out of storage first is a check-then-act across an
 * await, which two quick clicks slip straight through (the OWA worker's test
 * caught exactly that). It's also the truer signal, since a run can only exist
 * inside a live worker.
 */
let active = null;
function startRun(tabId) {
  if (active) return active;
  active = (async () => {
    await chrome.storage.session.set({
      [STATE_KEY]: { ...emptyState(), status: "running", startedAt: Date.now(), lastBeatAt: Date.now() },
    });
    paintBadge({ status: "running" });
    try {
      await capture(tabId);
    } catch (err) {
      await setMessage(err instanceof Error ? err.message : String(err), "error");
    } finally {
      await updateState((s) => ({ ...s, status: "done", finishedAt: Date.now() }));
    }
  })().finally(() => {
    active = null;
  });
  return active;
}

async function capture(tabId) {
  const tab = tabId ? await chrome.tabs.get(tabId).catch(() => null) : (await chrome.tabs.query({ active: true, currentWindow: true }))[0];
  if (!tab?.id || !tab.url) throw new Error("No Teams tab. Open Teams on the web and try again.");
  // Match the Teams web hostnames Microsoft uses: the standard work/school
  // (teams.microsoft.com), personal (teams.live.com), the newer
  // teams.cloud.microsoft, and the sovereign-cloud (.us / .microsoft.us).
  if (!TEAMS_HOST_RE.test(tab.url)) {
    throw new Error(`That tab isn't on a Teams web host (got ${new URL(tab.url).host}).`);
  }

  await setMessage("Reading the conversation…");

  // Teams renders the meeting Recap → Transcript tab inside an about:blank-style
  // iframe whose sandbox attribute defeats both contentDocument access and
  // `allFrames: true` (Chrome skips about:blank iframes when walking frames).
  // Enumerating frames explicitly via webNavigation and passing their ids is the
  // reliable path. Chat ingestion has one useful frame, so the per-frame picker
  // degrades to "pick the one with messages".
  // Only Teams' own frames are read: the top frame, frames on a Teams host, and about:blank /
  // about:srcdoc frames and Microsoft's recap page on SharePoint (the recap iframe) whose parent
  // is one of those. A third-party frame
  // embedded in Teams (an app tab, a preview) can't supply the capture.
  const frames = await chrome.webNavigation.getAllFrames({ tabId: tab.id });
  const frameIds = teamsFrames(frames ?? []);
  const target = { tabId: tab.id, frameIds: frameIds.length > 0 ? frameIds : [0] };
  // meeting-date.js first: it gives extractor.js the meeting-date reader (the result is the last
  // file's).
  const results = await chrome.scripting.executeScript({ target, files: ["meeting-date.js", "extractor.js"] });
  // Diagnostic: per-frame summary. Inspect the service worker (chrome://extensions
  // → "service worker") to see this.
  console.log("[teams-ingest] executeScript returned", results.length, "frame result(s) (of", frameIds.length, "frames)");
  for (const r of results) {
    console.log(
      "[teams-ingest] frame",
      r.frameId,
      "kind=",
      r.result?.kind,
      "messages=",
      r.result?.messages?.length,
      "title=",
      r.result?.title,
      "error=",
      r.error,
    );
  }
  const extraction = pickBestExtraction(results);
  console.log("[teams-ingest] picked extraction kind=", extraction?.kind, "messages=", extraction?.messages?.length);

  if (!extraction?.messages?.length) {
    throw new Error(
      "Couldn't find any messages on the page. Open a chat, expand a channel thread, or switch to a meeting's Recap → Transcript tab first.",
    );
  }

  const passSummary = extraction.passes ? ` over ${extraction.passes} pass${extraction.passes === 1 ? "" : "es"}` : "";
  await setMessage(`Posting ${extraction.messages.length} message${extraction.messages.length === 1 ? "" : "s"}${passSummary}…`);

  const data = await sendToBrainstead({ type: "teams", ...extraction });
  const atTopSuffix = extraction.atTop === false ? " — didn't reach top of chat" : "";
  const containerSuffix = extraction.scrollContainerDesc ? ` [scroll container: ${extraction.scrollContainerDesc}]` : "";
  // A transcript capture that came out as a chat (a recap page, where the transcript should have
  // won: a recap frame, a frame that read as a transcript, or a recap header's meeting date) says
  // what each frame gave, in the popup: the console isn't always to hand. Each frame by id (and its
  // parent's), its host, and what it returned. An ordinary chat on a page with other frames doesn't.
  const recap =
    (frames ?? []).some((f) => RECAP_FRAME_RE.test(f.url ?? "")) ||
    results.some((r) => r.result?.kind === "transcript" || r.result?.meetingDate);
  const frameNote =
    extraction.kind !== "transcript" && recap && (frames ?? []).length > 1
      ? ` — frames: ${frames
          .map((f) => {
            const r = results.find((x) => x.frameId === f.frameId);
            const host = (f.url ?? "").split("?")[0].split("/").slice(0, 3).join("/") || "?";
            const what = !frameIds.includes(f.frameId)
              ? "not read"
              : !r
                ? "no result"
                : r.error
                  ? `error: ${String(r.error?.message ?? r.error).slice(0, 100)}`
                  : `${r.result?.kind ?? "nothing"}, ${r.result?.messages?.length ?? 0} messages`;
            return `${f.frameId}${f.parentFrameId >= 0 ? ` (in ${f.parentFrameId})` : ""} ${host}: ${what}`;
          })
          .join("; ")}`
      : "";
  await setMessage(`✓ Wrote ${data.path}${atTopSuffix}${containerSuffix}${frameNote}`, "ok");
}

/** The ids of the tab's frames that belong to Teams itself (see `capture`). */
function teamsFrames(frames) {
  const byId = new Map(frames.map((f) => [f.frameId, f]));
  const ok = new Map();
  const isTeams = (f) => {
    if (!f) return false;
    if (ok.has(f.frameId)) return ok.get(f.frameId);
    ok.set(f.frameId, false); // A cycle can't make a frame Teams'.
    let yes;
    if (f.frameId === 0) yes = true;
    // A blank frame, or Microsoft's recap page on SharePoint, counts when Teams embeds it: the same
    // page in any other frame (or another SharePoint page) isn't read.
    else if (/^about:(blank|srcdoc)/.test(f.url ?? "") || RECAP_FRAME_RE.test(f.url ?? "")) yes = isTeams(byId.get(f.parentFrameId));
    else yes = TEAMS_HOST_RE.test(f.url ?? "");
    ok.set(f.frameId, yes);
    return yes;
  };
  return frames.filter(isTeams).map((f) => f.frameId);
}

/**
 * Pick the most useful extractor result from across all frames. Preference:
 *   1. `kind === 'transcript'` with real messages (recap iframe path).
 *   2. `kind === 'chat'` with at least one non-fallback sender.
 *   3. Anything with messages.
 *   4. Null.
 *
 * The chat extractor's "(unstructured capture)" fallback (which dumps
 * `document.body.innerText` when no message selectors match) is specifically
 * deprioritised — it's what fires in the Teams top frame when the actual
 * conversation lives in a sibling iframe.
 *
 * Title merge: the recap iframe is sandboxed in a way that prevents the
 * inside-iframe extractor from reading `window.top.document`, so its
 * `getMeetingTitle()` returns the "Meeting transcript" fallback. The top-frame
 * extractor's title selector *can* see the recap header (it's in its own DOM),
 * so when we pick a transcript result with a fallback title we borrow the title
 * from any other frame's result that managed to find a real one.
 */
function pickBestExtraction(results) {
  const candidates = [];
  for (const r of results ?? []) {
    if (r?.result?.messages?.length) candidates.push(r.result);
  }
  if (candidates.length === 0) return null;
  candidates.sort((a, b) => {
    const score = (r) => {
      const real = r.messages.some((m) => m.sender !== "(unstructured capture)");
      // Higher score = better. Transcripts win; real messages beat
      // unstructured fallback; ties broken by message count.
      let s = 0;
      if (r.kind === "transcript") s += 1000;
      if (real) s += 100;
      s += Math.min(r.messages.length, 99);
      return s;
    };
    return score(b) - score(a);
  });
  const best = candidates[0];

  // The meeting's date is for transcripts only. The recap iframe can't see the header with the
  // date in it; the top frame (which reads as a chat) can, so its date is lent to the transcript.
  if (best.kind === "transcript") {
    if (!best.meetingDate) {
      const lender = (results ?? []).map((r) => r?.result).find((r) => r && r !== best && r.meetingDate);
      if (lender) {
        best.meetingDate = lender.meetingDate;
        if (lender.meetingStart) best.meetingStart = lender.meetingStart;
      }
    }
  } else {
    delete best.meetingDate;
    delete best.meetingStart;
  }

  // Borrow a better title from a sibling frame if our best result fell back.
  // "Meeting transcript" and the empty string are the only two fallbacks the
  // extractor emits.
  const isFallbackTitle = !best.title || best.title === "Meeting transcript";
  if (isFallbackTitle) {
    for (let i = 1; i < candidates.length; i++) {
      const t = candidates[i].title;
      if (t && t !== "Meeting transcript" && !/^Microsoft Teams$/i.test(t)) {
        best.title = t;
        console.log("[teams-ingest] borrowed title from sibling frame:", t);
        break;
      }
    }
  }
  return best;
}

/** Brainstead's native messaging host: Chrome starts Brainstead's helper, which writes the
 *  capture into the vault's sources/ folder. Brainstead registers it with Chrome when it starts. */
const HOST = "com.wayned.brainstead";

async function sendToBrainstead(message) {
  let res;
  try {
    res = await chrome.runtime.sendNativeMessage(HOST, message);
  } catch (err) {
    throw new Error(
      `Couldn't reach Brainstead. Open Brainstead once so it can register with Chrome, then try again. (${err instanceof Error ? err.message : err})`,
    );
  }
  if (!res?.ok) throw new Error(`Brainstead refused: ${String(res?.message ?? "no answer").slice(0, 200)}`);
  return res;
}

// ── Sign-in handover (Brainstead's Teams and Outlook sync, D-20261025-01) ──────────────
//
// While a Teams tab is open, Brainstead can read the user's own chats, mail and calendar if it has
// the page's sign-in. The page keeps it (MSAL's cache) in localStorage; this reads it from an open
// Teams tab and hands it to Brainstead's host on a timer. It never leaves the user's own machine —
// it goes to the local app only. Capturing by button is unchanged; this is separate and silent.

const HANDOVER_ALARM = "brainstead-signin-handover";
const HANDOVER_PERIOD_MIN = 15;

/** Read the Teams page's sign-in from localStorage. Runs in the page (localStorage is the page's
 *  origin). Returns the refresh/access-token entries and Teams' region settings — no decoding. */
function readSignIn() {
  const out = { origin: location.origin, rt: null, ats: [], gtm: null };
  for (let i = 0; i < localStorage.length; i++) {
    const k = localStorage.key(i);
    const v = localStorage.getItem(k) || "";
    if (k.includes("DISCOVER-REGION-GTM")) out.gtm = v;
    try {
      const e = JSON.parse(v);
      if (e && e.credentialType === "RefreshToken" && e.secret && e.clientId) out.rt = e;
      if (e && e.credentialType === "AccessToken" && e.secret)
        out.ats.push({ target: e.target, realm: e.realm, clientId: e.clientId, secret: e.secret });
    } catch (_) {
      /* not a JSON MSAL entry */
    }
  }
  return out;
}

async function handOverSignIn() {
  let tabs;
  try {
    tabs = await chrome.tabs.query({
      url: [
        "https://teams.microsoft.com/*",
        "https://teams.cloud.microsoft/*",
        "https://teams.microsoft.us/*",
        "https://teams.office365.us/*",
      ],
    });
  } catch (_) {
    return;
  }
  for (const tab of tabs) {
    let results;
    try {
      results = await chrome.scripting.executeScript({ target: { tabId: tab.id }, func: readSignIn });
    } catch (_) {
      continue; // a tab still loading, or one we can't script
    }
    const page = results?.[0]?.result;
    if (page?.rt && page.gtm) {
      try {
        await chrome.runtime.sendNativeMessage(HOST, { type: "sign-in", ...page });
        return; // one good hand-over is enough
      } catch (_) {
        // Brainstead not running or not registered yet; try again on the next alarm.
        return;
      }
    }
  }
}

chrome.alarms.create(HANDOVER_ALARM, { periodInMinutes: HANDOVER_PERIOD_MIN });
chrome.alarms.onAlarm.addListener((a) => {
  if (a.name === HANDOVER_ALARM) handOverSignIn();
});
chrome.runtime.onStartup.addListener(handOverSignIn);
chrome.runtime.onInstalled.addListener(handOverSignIn);
