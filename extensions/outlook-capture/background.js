// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// MV3 service worker. Owns every capture run; the popup is only a controller
// and a renderer.
//
// WHY THE WORK LIVES HERE
//
// Chrome destroys an action popup's JS context the moment it loses focus, so a
// loop running there dies when the user switches tabs — mid-batch, with the
// remaining threads uncaptured, and (in combined mode) with everything captured
// so far thrown away because the POST happens at the end. Even a single capture
// was lost that way: `await chrome.scripting.executeScript(...)` resolves the
// transcript into a context that no longer exists.
//
// A service worker has no such lifetime tie to the UI. It does idle out after
// 30s, but every `chrome.*` call resets that timer and an in-flight one holds it
// open, and this loop is awaiting `executeScript` (or a native message) more or
// less continuously — the longest gap is one `openRowAndSettle` call, ~8s worst
// case. That's why there is no `chrome.alarms` keepalive here: it would buy
// nothing. (The alarm at the end is for handing the mail sign-in over.)
//
// STATE
//
// The run's state lives in `chrome.storage.session`, so it is the single source
// of truth: the popup renders whatever is there, whether it was open for the
// whole run, opened half-way through, or opened after it finished. Progress is
// also broadcast for a popup that happens to be listening (a no-op when none
// is), and mirrored onto the toolbar badge for when it isn't.

/** Keep in sync with MAX_BATCH_THREADS in brainstead-core's webcapture.rs. */
const MAX_BATCH_THREADS = 25;

const STATE_KEY = "run";

/**
 * A run whose heartbeat is older than this is treated as interrupted. Covers the
 * case the worker cannot report itself: Chrome killing it mid-run (a crash, or
 * the browser shutting down), which would otherwise leave `status: "running"`
 * in session storage forever.
 */
const STALE_RUN_MS = 45_000;

const OWA_HOST_RE = /^https:\/\/outlook\.(office\.com|office365\.com|cloud\.microsoft|live\.com|office\.de|office365\.us)\//;

// ---------------------------------------------------------------- state -----

const emptyState = () => ({
  status: "idle", // idle | running | done
  kind: null, // single | batch
  message: null, // { text, kind } — the headline status line
  lines: [], // [{ text, kind }] — per-thread log
  total: 0,
  index: 0,
  ok: 0,
  failed: 0,
  startedAt: null,
  finishedAt: null,
  lastBeatAt: null,
});

async function readState() {
  const stored = await chrome.storage.session.get(STATE_KEY);
  const state = stored[STATE_KEY];
  if (!state) return emptyState();
  // Resurrect nothing: a "running" run with a cold heartbeat is over, and
  // saying so is more useful than a progress bar that never moves.
  if (state.status === "running" && state.lastBeatAt && Date.now() - state.lastBeatAt > STALE_RUN_MS) {
    return {
      ...state,
      status: "done",
      message: {
        text: `Interrupted after ${state.index} of ${state.total}. Anything already written is in sources/.`,
        kind: "error",
      },
      finishedAt: Date.now(),
    };
  }
  return state;
}

let writing = Promise.resolve();

/** Serialise writes so concurrent updates can't interleave read-modify-write. */
function updateState(mutate) {
  writing = writing.then(async () => {
    const state = await readState();
    const next = mutate({ ...state });
    next.lastBeatAt = Date.now();
    await chrome.storage.session.set({ [STATE_KEY]: next });
    // Fire-and-forget: no popup open is the normal case, and that rejects.
    chrome.runtime.sendMessage({ type: "owa:progress", state: next }).catch(() => {});
    paintBadge(next);
    return next;
  });
  return writing;
}

function paintBadge(state) {
  if (state.status === "running") {
    chrome.action.setBadgeText({ text: `${state.index}/${state.total}` });
    chrome.action.setBadgeBackgroundColor({ color: "#1976D2" });
    return;
  }
  if (state.status === "done") {
    const bad = state.failed > 0;
    chrome.action.setBadgeText({ text: bad ? `${state.failed}!` : "✓" });
    chrome.action.setBadgeBackgroundColor({ color: bad ? "#B3261E" : "#1B7031" });
    return;
  }
  chrome.action.setBadgeText({ text: "" });
}

const setMessage = (text, kind = null) => updateState((s) => ({ ...s, message: { text, kind } }));
const appendLine = (text, kind = null) => updateState((s) => ({ ...s, lines: [...s.lines, { text, kind }] }));

// -------------------------------------------------------------- messages ----

chrome.runtime.onMessage.addListener((msg, _sender, respond) => {
  if (!msg || typeof msg.type !== "string") return false;
  switch (msg.type) {
    case "owa:get-state":
      // The rejection path matters as much as the happy one: this listener has
      // already returned `true`, so if `respond` is never called Chrome closes
      // the channel and the popup's `sendMessage` REJECTS. The popup used to
      // swallow that and sit on its placeholder forever, which reads exactly
      // like "the extension can't see my selection".
      void readState().then(respond, (err) => respond({ error: err instanceof Error ? err.message : String(err) }));
      return true; // async respond
    case "owa:clear-badge":
      chrome.action.setBadgeText({ text: "" });
      respond({ ok: true });
      return true;
    case "owa:scan":
      void scanSelection(msg.tabId).then(respond, (err) => respond({ error: err instanceof Error ? err.message : String(err) }));
      return true;
    case "owa:single":
      // Deliberately not awaited: reply at once so the popup can close freely,
      // and let the run outlive it.
      void startRun(() => runSingle(msg.tabId));
      respond({ started: true });
      return true;
    case "owa:batch":
      void startRun(() => runBatch(msg.tabId, msg.options ?? {}));
      respond({ started: true });
      return true;
    default:
      return false;
  }
});

/**
 * One run at a time, and a fresh slate for each.
 *
 * `active` is the authority, and it is assigned SYNCHRONOUSLY. Reading
 * `status === "running"` out of storage first was a check-then-act across an
 * await: two clicks in quick succession both passed the check and two runs
 * interleaved over the same tab. It is also the more accurate signal — a run can
 * only exist inside a live worker, so if `active` is null nothing is in flight
 * anywhere, whatever a stale storage record says (a worker killed mid-run leaves
 * `status: "running"` behind, which `readState` surfaces via the heartbeat).
 */
let active = null;
function startRun(run) {
  if (active) return active;
  active = (async () => {
    await chrome.storage.session.set({
      [STATE_KEY]: { ...emptyState(), status: "running", startedAt: Date.now(), lastBeatAt: Date.now() },
    });
    chrome.action.setBadgeText({ text: "0/0" });
    try {
      await run();
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

// ------------------------------------------------------------------ runs ----

/** Resolve a tab id, asserting it is still on an Outlook web host. */
async function owaTab(tabId) {
  const tab = tabId ? await chrome.tabs.get(tabId).catch(() => null) : (await chrome.tabs.query({ active: true, currentWindow: true }))[0];
  if (!tab?.id || !tab.url) throw new Error("No Outlook tab. Open Outlook on the web and try again.");
  if (!OWA_HOST_RE.test(tab.url)) {
    throw new Error(`That tab isn't on an Outlook web host (got ${new URL(tab.url).host}).`);
  }
  return tab;
}

/** Read the checked rows out of the message list. Scrolls; see list-extractor.js. */
async function scanSelection(tabId) {
  const tab = await owaTab(tabId);
  const [{ result }] = await chrome.scripting.executeScript({
    target: { tabId: tab.id },
    files: ["list-extractor.js"],
  });
  return { tabId: tab.id, listing: result ?? { rows: [] } };
}

async function runSingle(tabId) {
  const tab = await owaTab(tabId);
  await updateState((s) => ({ ...s, kind: "single", total: 1 }));
  await setMessage("Reading the thread…");
  const extraction = await extractOpenThread(tab.id);
  if (!extraction?.messages?.length) {
    await updateState((s) => ({ ...s, failed: 1 }));
    await setMessage("Couldn't find a thread in the page. Open a single email or expand a conversation.", "error");
    return;
  }
  await setMessage(`Posting ${extraction.messages.length} message${extraction.messages.length === 1 ? "" : "s"}…`);
  const data = await postThread(extraction);
  await updateState((s) => ({ ...s, index: 1, ok: 1 }));
  await setMessage(`✓ Wrote ${data.path}`, "ok");
}

async function runBatch(tabId, { mode = "separate", label = "", limit = 10 } = {}) {
  const tab = await owaTab(tabId);
  const cap = Math.min(Math.max(1, Math.floor(limit) || 1), MAX_BATCH_THREADS);

  await setMessage("Scanning the message list for checked threads…");
  const { listing } = await scanSelection(tab.id);
  const rows = (listing.rows ?? []).slice(0, cap);
  if (rows.length === 0) {
    await setMessage("Nothing selected to capture.", "error");
    return;
  }
  await updateState((s) => ({ ...s, kind: "batch", total: rows.length }));

  const captured = [];
  // Fingerprint the pane BEFORE the first click, so the first settle-check has
  // a real "before" value rather than passing on whatever was already open.
  let signature = await currentPaneSignature(tab.id);

  for (let i = 0; i < rows.length; i++) {
    const row = rows[i];
    const position = `${i + 1}/${rows.length}`;
    await updateState((s) => ({ ...s, index: i + 1 }));
    await setMessage(`Opening ${position}: ${row.label}…`);

    let settled;
    try {
      const [{ result }] = await chrome.scripting.executeScript({
        target: { tabId: tab.id },
        func: openRowAndSettle,
        // row.scrollTop is where the scan found this row. The list is
        // virtualised, so by now it may not exist in the DOM; the hint scrolls
        // it back into existence.
        args: [row.convid, signature, row.scrollTop ?? null],
      });
      settled = result;
    } catch (err) {
      settled = { ok: false, reason: err instanceof Error ? err.message : String(err) };
    }
    if (!settled?.ok) {
      await countFailure(`✗ ${position} ${row.label}: ${describeFailure(settled?.reason)}`);
      continue;
    }
    signature = settled.signature;

    await setMessage(`Reading ${position}: ${row.label}…`);
    let extraction;
    try {
      extraction = await extractOpenThread(tab.id);
    } catch (err) {
      await countFailure(`✗ ${position} ${row.label}: extractor failed, ${err instanceof Error ? err.message : String(err)}`);
      continue;
    }
    if (!extraction?.messages?.length) {
      await countFailure(`✗ ${position} ${row.label}: opened, but nothing readable in the reading pane`);
      continue;
    }

    // document.title is "Inbox - <mailbox>" in this OWA build, so a mail with
    // no quoted chain and no pane heading yields a useless name. The list row
    // has the real subject, so prefer it when the extractor fell that far.
    if (extraction.titleSource === "document-title" && row.subject) {
      extraction.title = stripReplyPrefixes(row.subject);
    }

    if (mode === "combined") {
      captured.push(extraction);
      await updateState((s) => ({ ...s, ok: s.ok + 1 }));
      await appendLine(`• ${position} ${extraction.title} (${describeCapture(extraction)})`);
      continue;
    }
    try {
      const data = await postThread(extraction);
      await updateState((s) => ({ ...s, ok: s.ok + 1 }));
      const note = extraction.kind === "reading-pane" ? " (reading-pane capture)" : "";
      await appendLine(`✓ ${position} ${data.filename}${note}`, "ok");
    } catch (err) {
      await countFailure(`✗ ${position} ${row.label}: ${err instanceof Error ? err.message : String(err)}`);
    }
  }

  if (mode === "combined") {
    if (captured.length === 0) {
      await setMessage(`Captured nothing: all ${rows.length} threads failed.`, "error");
      return;
    }
    await setMessage(`Posting ${captured.length} threads as one document…`);
    const data = await postBatch(captured, label);
    await appendLine(`✓ ${data.filename}`, "ok");
  }

  const state = await readState();
  await setMessage(
    state.failed === 0
      ? `✓ Captured all ${state.ok} thread${state.ok === 1 ? "" : "s"}. Your checkbox selection was cleared by the walk.`
      : `Captured ${state.ok} of ${rows.length}; ${state.failed} failed (see below).`,
    state.failed === 0 ? "ok" : "error",
  );
}

async function countFailure(line) {
  await updateState((s) => ({ ...s, failed: s.failed + 1 }));
  await appendLine(line, "error");
}

// ------------------------------------------------------------- transport ----

async function postThread(extraction) {
  return sendToBrainstead({ type: "outlook", ...extraction });
}

async function postBatch(threads, label) {
  return sendToBrainstead({ type: "outlook-batch", label: label?.trim() || undefined, threads });
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

// --------------------------------------------------------------- helpers ----

function describeFailure(reason) {
  if (reason === "row-not-found") {
    return "row never reappeared in the list, even after scrolling to find it";
  }
  if (reason === "pane-did-not-change") {
    return "clicking the row changed nothing in the reading pane";
  }
  return reason ?? "unknown error";
}

/** "3 msg" for a thread, or a flag for a non-threaded item. */
function describeCapture(extraction) {
  if (extraction.kind === "reading-pane") return "reading-pane capture";
  return `${extraction.messages.length} msg`;
}

/** Mirrors stripReplyPrefixes in extractor.js; the row subject never passes
 *  through there because it comes from the list, not the reading pane. */
function stripReplyPrefixes(text) {
  return text.replace(/^(?:\s*(?:re|fw|fwd|forward|aw|wg)\s*:\s*)+/i, "").trim();
}

/** Run the reading-pane extractor against the currently-open thread. */
async function extractOpenThread(tabId) {
  const [{ result }] = await chrome.scripting.executeScript({
    target: { tabId },
    files: ["extractor.js"],
  });
  return result;
}

/**
 * Fingerprint of whatever the reading pane is showing right now.
 *
 * Deliberately duplicates `paneSignature` inside `openRowAndSettle`: both run
 * in the page and chrome.scripting serialises each one on its own, so neither
 * can call a shared helper from this file. Keep the two in sync.
 */
async function currentPaneSignature(tabId) {
  try {
    const [{ result }] = await chrome.scripting.executeScript({
      target: { tabId },
      func: () => {
        const first = document.querySelector('[aria-label="Message body"], div[id^="UniqueMessageBody"]');
        const text = (first?.innerText ?? "").replace(/\s+/g, " ").trim().slice(0, 300);
        return text ? `${document.title}||${text}` : "";
      },
    });
    return result ?? "";
  } catch {
    return "";
  }
}

/**
 * Injected into the page: open the list row with this `convid` and wait for the
 * reading pane to actually show it.
 *
 * Serialised by chrome.scripting, so it must not close over anything here.
 *
 * Clicking a row is what makes OWA render the full message bodies: checkbox
 * selection alone gives you a ~30-word preview and nothing else. The side
 * effect is that OWA clears the multi-selection on the first click, which is
 * why the caller snapshots every convid up front and re-finds each row by
 * attribute rather than holding element references.
 *
 * The list is virtualised, so "re-find" often means "scroll back to where it
 * was and wait for it to be re-created". `hintScrollTop` is the position the
 * snapshot scan saw this row at; failing that we sweep the container.
 *
 * Two settle signals, because not every row is a conversation:
 *
 *   1. The reading pane's text changed. Strongest evidence, and what any mail
 *      with readable content gives us, including a meeting invitation, whose
 *      calendar card has no message body but does have pane text.
 *   2. The selection collapsed to just this row AND the pane exposes no text at
 *      all. Last resort for a genuinely empty pane. Deliberately gated on the
 *      pane being empty: with virtualisation, "only one row is selected" can be
 *      true simply because the other selected rows are unmounted, so on its own
 *      it would happily settle on the previous thread and capture it twice.
 */
async function openRowAndSettle(convid, prevSignature, hintScrollTop) {
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  const afterScroll = async () => {
    await new Promise((r) => requestAnimationFrame(() => r()));
    await sleep(140);
  };

  // Fan-out, most specific first. A card-only mail can have an empty
  // [aria-label="Message body"], so an empty match falls through to the next
  // selector rather than counting as "no pane".
  const PANE_SELECTORS = [
    '[aria-label="Message body"]',
    'div[id^="UniqueMessageBody"]',
    '[aria-label="Reading Pane"]',
    'div[id="ReadingPaneContainerId"]',
    '[data-app-section="ConversationContainer"]',
    'div[role="main"]',
  ];

  /** Cheap fingerprint of what the reading pane is currently showing. */
  const paneSignature = () => {
    for (const sel of PANE_SELECTORS) {
      const el = document.querySelector(sel);
      const text = (el?.innerText ?? "").replace(/\s+/g, " ").trim();
      if (text) return `${sel}||${text.slice(0, 300)}`;
    }
    return "";
  };

  const allRows = () => [...document.querySelectorAll('[role="option"]')];
  const findRow = () => allRows().find((el) => el.getAttribute("data-convid") === convid);

  /** Same walk-up-from-a-row approach as list-extractor.js. */
  const scrollParent = (el) => {
    let node = el?.parentElement;
    while (node && node !== document.body && node !== document.documentElement) {
      const overflowY = getComputedStyle(node).overflowY;
      if (/(auto|scroll|overlay)/.test(overflowY) && node.scrollHeight > node.clientHeight + 4) {
        return node;
      }
      node = node.parentElement;
    }
    return null;
  };

  /**
   * Find the row, scrolling the virtualised list until it materialises: first
   * at the remembered position, then by sweeping from the top.
   */
  async function locateRow() {
    let row = findRow();
    if (row) return row;
    const container = scrollParent(allRows()[0]);
    if (!container) return undefined;

    if (typeof hintScrollTop === "number") {
      container.scrollTop = hintScrollTop;
      await afterScroll();
      row = findRow();
      if (row) return row;
    }

    container.scrollTop = 0;
    await afterScroll();
    for (let pass = 0; pass < 60; pass++) {
      row = findRow();
      if (row) return row;
      const before = container.scrollTop;
      const atBottom = before + container.clientHeight >= container.scrollHeight - 2;
      if (atBottom) break;
      container.scrollTop = before + Math.max(container.clientHeight * 0.8, 200);
      await afterScroll();
      if (container.scrollTop === before) break;
    }
    return findRow();
  }

  /** True once OWA has collapsed the multi-selection onto this row alone. */
  const isOnlyRowSelected = () => {
    const selected = allRows().filter((el) => el.getAttribute("aria-selected") === "true");
    return selected.length === 1 && selected[0]?.getAttribute("data-convid") === convid;
  };

  const row = await locateRow();
  if (!row) return { ok: false, reason: "row-not-found" };

  row.scrollIntoView({ block: "nearest" });
  await sleep(50);
  row.click();

  // React handles isTrusted=false events fine, so .click() normally opens the
  // thread. Some builds bind the row on mousedown instead, so if nothing has
  // moved after ~1.2s, replay a full pointer sequence before giving up.
  for (let attempt = 0; attempt < 2; attempt++) {
    const deadline = Date.now() + (attempt === 0 ? 1200 : 7000);
    const graceAt = Date.now() + (attempt === 0 ? 900 : 1800);
    while (Date.now() < deadline) {
      const sig = paneSignature();
      if (sig && sig !== prevSignature) {
        // Bodies can still be streaming in; give the DOM a beat to finish.
        await sleep(400);
        return { ok: true, signature: paneSignature(), hasPaneText: true };
      }
      if (!sig && isOnlyRowSelected() && Date.now() > graceAt) {
        return { ok: true, signature: "", hasPaneText: false };
      }
      await sleep(200);
    }
    if (attempt === 0) {
      const again = findRow();
      if (!again) return { ok: false, reason: "row-not-found" };
      for (const type of ["pointerdown", "mousedown", "mouseup", "click"]) {
        again.dispatchEvent(new MouseEvent(type, { bubbles: true, cancelable: true, view: window }));
      }
    }
  }
  return { ok: false, reason: "pane-did-not-change" };
}

// ---------------------------------------------------------- mail sign-in ----

// Brainstead's Teams and Outlook sync reads mail with the token Outlook on the web already holds:
// every 15 minutes, while an Outlook tab is open, this hands that tab's Outlook token over
// (D-20261028-02). Teams' own page stopped reliably keeping one that can read mail. The rest of the
// sync's sign-in still comes from the Teams extension; Brainstead takes only the mail token from here.

importScripts("sign-in-reader.js");

const MAIL_HANDOVER_ALARM = "brainstead-mail-handover";
/** What the last hand-over did, in chrome.storage.local, so mail that stops arriving can be
 *  explained. Holds no tokens. */
const LAST_HANDOVER_KEY = "signin-last-handover";
/** How long reading a tab or Brainstead's answer may take: a tab Chrome has frozen never answers. */
const STEP_MS = 20_000;

function inTime(p, what) {
  let timer;
  const late = new Promise((_, reject) => {
    timer = setTimeout(() => reject(new Error(`${what} took over ${STEP_MS / 1000}s`)), STEP_MS);
  });
  return Promise.race([p, late]).finally(() => clearTimeout(timer));
}

async function handOverMailSignIn() {
  const notes = [];
  const outcome = await handOverMail(notes).catch((e) => `failed: ${e?.message || e}`);
  chrome.storage.local
    .set({ [LAST_HANDOVER_KEY]: { at: new Date().toISOString(), version: chrome.runtime.getManifest().version, outcome, notes } })
    .catch(() => {});
}

async function handOverMail(notes) {
  const tabs = (await chrome.tabs.query({})).filter((t) => OWA_HOST_RE.test(t.url || ""));
  if (!tabs.length) return "no Outlook tab open";
  for (const tab of tabs) {
    // Keep Chrome's Memory Saver from putting the tab to sleep, so its sign-in stays readable.
    chrome.tabs.update(tab.id, { autoDiscardable: false }).catch(() => {});
    if (tab.discarded) {
      notes.push(`tab ${tab.id} asleep`);
      continue;
    }
    let page;
    try {
      const results = await inTime(
        chrome.scripting.executeScript({ target: { tabId: tab.id }, func: globalThis.brainsteadReadSignIn }),
        "reading the tab",
      );
      page = results?.[0]?.result;
    } catch (e) {
      notes.push(`tab ${tab.id} couldn't be read: ${e?.message || e}`);
      continue;
    }
    if (page?.seen) notes.push(`tab ${tab.id} keeps ${JSON.stringify(page.seen)}`);
    delete page?.seen;
    if (!page?.ats?.length) continue;
    try {
      const reply = await inTime(
        chrome.runtime.sendNativeMessage(HOST, { type: "sign-in", ...page, version: chrome.runtime.getManifest().version }),
        "Brainstead's answer",
      );
      return reply?.ok ? "handed over" : `Brainstead refused: ${reply?.message || "no answer"}`;
    } catch (e) {
      return `Brainstead unreachable: ${e?.message || e}`;
    }
  }
  return "no tab had a sign-in to hand over";
}

chrome.alarms.create(MAIL_HANDOVER_ALARM, { periodInMinutes: 15 });
chrome.alarms.onAlarm.addListener((a) => {
  if (a.name === MAIL_HANDOVER_ALARM) handOverMailSignIn();
});
chrome.runtime.onStartup.addListener(handOverMailSignIn);
chrome.runtime.onInstalled.addListener(handOverMailSignIn);
