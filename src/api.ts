// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// The window's calls into Rust (src-tauri/src/lib.rs) and the events it listens for.

import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import type { Platform } from "./platform";

export type Theme = "system" | "light" | "dark";

/** What this system can do (src-tauri/core/src/features.rs); a screen shows a control only when its feature is on. */
export interface PlatformFeatures {
  platform: Platform;
  quickLook: boolean;
  spelling: boolean;
  grammar: boolean;
  readAloud: boolean;
  exportPdf: boolean;
  richCopy: boolean;
  fullDiskAccess: boolean;
  openAtLogin: boolean;
  imageText: boolean;
}

export interface Settings {
  vaultPath: string | null;
  excluded: string[];
  readOnly: boolean;
  theme: Theme;
  skippedFullDiskAccess: boolean;
  /** The global capture shortcut, as "Control+Alt+Space". */
  captureShortcut?: string;
  /** Document text size in px (the A/A control on a document's top bar). */
  readSize?: number;
  /** The whole window's size as a percentage, 80 to 150 (Settings › General › Appearance; src/interfaceSize.ts). */
  interfaceSize?: number;
  /** Back/forward history and where in it the window is, kept across restarts (src/nav.ts). */
  places?: { stack: import("./nav").Place[]; i: number };
  /** The Projects screen's order, dragged by hand: areas by name, projects by path. What isn't in
   *  it goes after, A–Z (D-20261012-01). */
  projectOrder?: { areas?: string[]; projects?: string[] };
  /** Each list's search results order (by the list's plural noun): best match or latest first. */
  searchOrder?: Record<string, "ranked" | "latest">;
  /** Jobs & schedule: ingest a new source as it arrives (a capture, a file dropped or imported);
   *  and, in the daily check, ingest again the sources that changed since pages cited them. */
  ingestOnArrival?: boolean;
  /** Teams and Outlook sync is on (Settings › Teams and Outlook sync; macOS only). */
  syncEnabled?: boolean;
  /** Channel conversation ids chosen to sync (1:1 and group chats always sync). */
  syncChannels?: string[];
  /** Senders or domains whose mail the sync skips ("Don't sync this sender"). */
  syncIgnoreSenders?: string[];
  /** How many days of history the first sync loads (0 = no limit). */
  syncInitialDays?: number;
  /** Bring a synced email's attachments into sources/ (PDF and Office files up to 30 MB). On by default. */
  syncMailAttachments?: boolean;
  /** After a meeting note is made: ingest it (default on), then move its transcript to the Trash (default on). */
  meetingIngest?: boolean;
  meetingTrash?: boolean;
  refreshStale?: boolean;
  /** A model per background job (summaries, weekprep, find, ingest, meeting, clarify, contradictions, skills); a
   *  job not named uses `askModel`. */
  jobModels?: Record<string, string>;
  /** Prompts sent in Ask, oldest first, for ↑ in the message box (the last 200). */
  askHistory?: string[];
  /** Tasks' saved lists (a view with its filters), by name. */
  taskLists?: SavedTaskList[];
  /** Spelling and grammar marks in the editor (Settings › Notes); both on unless false. */
  spellCheck?: boolean;
  grammarCheck?: boolean;
  /** An English variant to check in (en_GB, en_ZA…); unset follows the system. */
  spellLanguage?: string;
  /** Days of the app's own logs kept (Settings › About; src-tauri/src/applog.rs). */
  logDays?: number;
  /** Read aloud (Settings › Notes, src/speech/): the voice's identifier, the speed (1 is normal), and
   *  whether each word is highlighted (on unless false) or the whole paragraph. */
  speechVoice?: string;
  speechRate?: number;
  speechHighlight?: boolean;
  /** The menu-bar icon (Settings › General; on unless false), and leaving the Dock when the window
   *  is closed (src-tauri/src/tray.rs). */
  menuBar?: boolean;
  menuBarOnly?: boolean;
  /** The help's getting-started guides finished, by id (src/help/HelpDrawer.tsx). */
  helpGuides?: Record<string, boolean>;
  /** Offer the Getting started guides when the app opens (src/help/GuideOffer.tsx; on unless false). */
  offerGuide?: boolean;
  /** Moving over from another notes app: the checklist's items the user ticks by hand (Settings › General). */
  movingOver?: Record<string, boolean>;
  /** Each screen's last choices, for a fresh visit to it (useViewState in src/nav.ts). */
  viewMemory?: Record<string, unknown>;
  /** Documents opened lately (src/recent.ts). */
  recent?: { path: string; title: string; at: number }[];
  /** The default document theme and colour (Settings › Notes, src/docLook.ts). */
  docStyle?: string;
  docAccent?: string;
  /** Templater's user scripts folder, vault-relative (Settings › Notes; default Templates/scripts). */
  templateScripts?: string;
  /** Notes with their own theme or colour, by vault path. */
  docLooks?: Record<string, { style?: string; accent?: string }>;
  /** Documents light or dark on their own; absent: as the app. */
  docTheme?: "light" | "dark";
  /** Sidebar sections the user has folded. */
  folded?: Record<string, boolean>;
  /** Ask: the model new chats start with, the open tabs and the one showing (src/askState.ts). */
  askModel?: string;
  /** How long Changes keeps its history: days and megabytes, whichever comes first (default 90 and 500). */
  changesKeepDays?: number;
  changesKeepMb?: number;
  /** Suggest a next message after each answer in Ask (on unless false). */
  askSuggest?: boolean;
  askTabs?: { id: string; filename: string }[];
  askActive?: string;
  /** The daily and weekly summaries (Settings › Jobs & schedule), and whether Brainstead runs them. */
  summaries?: SummarySchedule;
  summariesHere?: boolean;
  /** "Hold the summaries for me": the daily and weekly summaries wait in Changes instead of being written. */
  holdSummaries?: boolean;
  /** When you do the guided weekly review (Today reminds you then); no job runs. Friday 16:00 by default. */
  weeklyReview?: WeeklyReviewTime;
  /** The daily check (stage 7b) and its local HH:MM. */
  dailyCheckEnabled?: boolean;
  dailyCheckTime?: string;
  /** The weekly review's preparation runs on schedule (default on). */
  weekprepEnabled?: boolean;
  /** The user's first name as Teams writes it (notes from transcripts). */
  ownerName?: string;
  /** The editor's highlighter colour, kept until another is picked. */
  hlColour?: "red" | "orange" | "yellow" | "green" | "teal" | "blue" | "purple" | "grey";
  /** A note's side pane (Linked from) folded away, for the full width. */
  docSideFolded?: boolean;
}

export type Weekday = "mon" | "tue" | "wed" | "thu" | "fri" | "sat" | "sun";

export interface SummarySchedule {
  dailyEnabled: boolean;
  /** HH:MM, local. */
  dailyTime: string;
  weeklyEnabled: boolean;
  weeklyDay: Weekday;
  weeklyTime: string;
}

export interface WeeklyReviewTime {
  day: Weekday;
  /** HH:MM, local. */
  time: string;
}

export interface ReviewRun {
  id: string;
  kind: "daily" | "weekly";
  /** 2026-10-01 or 2026-W40. */
  target: string;
  trigger: "schedule" | "manual" | "retry" | "unattended";
  model: string;
  startedAt: string;
  finishedAt: string | null;
  status: "running" | "done" | "error" | "stopped";
  error: string | null;
  /** The review note written, vault-relative. */
  file: string | null;
  /** It replaced an earlier block for the same target. */
  replaced: boolean;
  undone: boolean;
  /** The chat file holding the run. */
  chat: string | null;
}

export interface ReviewsStatus {
  runs: ReviewRun[];
  next: { daily: string | null; weekly: string | null };
  running: ("daily" | "weekly")[];
}

/** A daily or weekly summary block, as Today shows it. */
export interface LatestSummary {
  /** The monthly file, vault-relative. */
  file: string;
  /** "## Daily summary 2026-10-04" (or its name before the rename). */
  heading: string;
  /** "2026-10-04" or "2026-W40". */
  label: string;
  /** The block, heading included. */
  text: string;
}

export interface SavedTaskList {
  name: string;
  view: string;
  context: string;
  effort: string;
  group: string;
}

export const DEFAULT_SETTINGS: Settings = {
  vaultPath: null,
  excluded: [],
  readOnly: true,
  theme: "system",
  skippedFullDiskAccess: false,
};

export interface IndexStats {
  files: number;
  notes: number;
  wiki: number;
  sources: number;
  templates: number;
  tasks: number;
  openTasks: number;
  links: number;
  unresolvedLinks: number;
  updatedAt: number;
  lastSyncMs: number;
}

export interface VaultStatus {
  state: "none" | "indexing" | "ready" | "error";
  vaultPath: string | null;
  error: string | null;
  stats: IndexStats;
}

export interface Permissions {
  fullDiskAccess: boolean | null;
  applies: boolean;
}

/** A newer version, offered by src-tauri/src/updates.rs. `date` is when it was released (RFC 3339). */
export interface UpdateOffer {
  name: string;
  version: string;
  current: string;
  notes: string | null;
  date: string | null;
}

/** An update installing: `total` is null when the download's size isn't known. */
export interface UpdateProgress {
  phase: "downloading" | "installing" | "restarting";
  downloaded: number;
  total: number | null;
}

export interface AppInfo {
  version: string;
  build: string;
  dataDir: string;
  /** This program, which also runs as the MCP server with `--mcp`. */
  exe: string;
}

export interface VaultChanges {
  changed: string[];
  removed: string[];
}

export type Layer = "note" | "wiki" | "source" | "template";

export interface FileSummary {
  path: string;
  layer: Layer;
  type: string | null;
  title: string;
  /** From the filename, else the date or created property. */
  date: string | null;
  size: number;
  mtime: number;
  tags: string[];
}

/** A file on the Graph screen, or `ghost:<name>` for a link to nothing (core/src/graph.rs). */
export interface GraphNode {
  id: string;
  title: string;
  layer: Layer | "ghost";
  type: string | null;
  /** Links in and out across the whole vault. */
  degree: number;
  /** Steps from the centre. */
  depth: number;
}

export interface Graph {
  nodes: GraphNode[];
  edges: [string, string][];
  /** Cut at the most nodes one graph has. */
  truncated: boolean;
}

/** How many files have one type or tag; `""` is none (core/src/activity.rs). */
export interface Count {
  name: string;
  n: number;
}

/** The vault at a glance (core/src/activity.rs): notes and wiki pages by type, most first, their
 *  top tags, and the files the most others link to. */
export interface Glance {
  noteTypes: Count[];
  noteTags: Count[];
  wikiTypes: Count[];
  wikiTags: Count[];
  mostLinked: { path: string; title: string; layer: Layer; links: number }[];
}

/** One `log.md` entry (core/src/activity.rs). */
export interface LogEntry {
  date: string;
  time: string | null;
  action: string;
  title: string;
  description: string;
}

/** Files that last changed on one day, by layer. */
export interface ActivityDay {
  date: string;
  notes: number;
  wiki: number;
  sources: number;
}

export interface OutLink {
  target: string;
  kind: "link" | "embed";
  heading: string | null;
  block: string | null;
  resolved: string | null;
}

export interface Backlink {
  path: string;
  title: string;
  layer: Layer;
  line: number | null;
  context: string;
}

export interface DocMeta {
  summary: FileSummary;
  frontmatter: Record<string, unknown> | null;
  frontmatterError: string | null;
  aliases: string[];
  links: OutLink[];
  backlinks: Backlink[];
}

/** A capture from an extension, as the app data folder's captures.json notes it. */
export interface Captured {
  extension: "outlook" | "teams" | "other";
  what: string;
  path: string | null;
  error: string | null;
  at: string;
  /** Made by the Teams and Outlook sync rather than an extension button (ingested unattended). */
  sync?: boolean;
  /** The item's own date/time (RFC 3339): a chat's last message, a mail's received time, a meeting's start. */
  itemDate?: string | null;
}

/** The Teams and Outlook sign-in state (Settings › Teams and Outlook sync). */
export interface SyncSignIn {
  signedIn: boolean;
  handedOver: string | null;
  /** What the sync can't reach while signed in, its token missing or run out: "chats", "mail", "transcripts", "channel list". */
  stale: string[];
  /** What Microsoft turned the sign-in down for (signed out), until a new one is handed over: "chats", "mail", … */
  rejected: string[];
  /** The Teams extension version Chrome is running, when older than the one this build ships ("" when too old to say). */
  extensionOutdated: string | null;
  /** The Teams extension version this build ships. */
  extensionShipped: string;
}

/** What one sync pass did. */
export interface SyncReport {
  chats: number;
  mail: number;
  transcripts: number;
  problems: string[];
  blocked: string | null;
}

/** An upcoming meeting for the Today screen. */
export interface SyncMeeting {
  subject: string;
  start: string;
  end: string;
  organizer: string;
  online: boolean;
}

export interface Doc {
  meta: DocMeta;
  content: string;
  root: string;
  /** What was read, as `doc_save` checks it: a hash of the bytes on disk. */
  version: string;
  /** Why the file can't be edited here, when it can't: it isn't UTF-8 text. */
  readOnly?: string;
}

export interface SearchHit {
  file: FileSummary;
  score: number;
  heading: string;
  snippet: { text: string; hit: boolean }[];
  passages: number;
}

export interface SearchResults {
  hits: SearchHit[];
  total: number;
  ms: number;
}

export interface TaskQuery {
  status?: "done" | "not-done" | "any";
  period?: "this-week" | "last-week";
  tagsInclude?: string[];
  tagsExclude?: string[];
  sort?: "done-asc" | "done-desc" | "due-asc" | "due-desc" | "created-asc" | "created-desc" | "manual" | "unsorted";
  limit?: number;
  today?: string;
}

export interface TaskRow {
  path: string;
  title: string;
  line: number;
  /** The whole line as in the file, sent back with an edit so Rust can find it again. */
  lineText: string;
  text: string;
  /** The character between the brackets: " ", "x", "/", "-"… */
  status?: string;
  /** Done or cancelled. */
  done: boolean;
  due: string | null;
  scheduled: string | null;
  start: string | null;
  doneOn: string | null;
  created: string | null;
  cancelled?: string | null;
  rank: number | null;
  tags: string[];
  heading: string | null;
  /** The project note it belongs to (stage 6): the one it's written in, or one its line links. */
  project?: string | null;
  /** From `#context/<name>` tags, without the prefix. */
  contexts?: string[];
  /** As written in `[effort:: 15m]`, and in minutes. */
  effort?: string | null;
  effortMin?: number | null;
}

export interface ProjectRow {
  path: string;
  name: string;
  status: "active" | "on-hold" | "someday" | "done";
  area: string | null;
  outcome: string | null;
  next: number;
  waiting: number;
  someday: number;
  done: number;
  /** ms: the latest change to the note or a file holding one of its tasks. */
  lastTouched: number;
  /** Notes that link the project. */
  links: string[];
}

/** Something to clarify: a Scratchpad block or a To Do `#### Other` task. */
export interface InboxItem {
  /** capture: a file in sources/ from the Outlook or Teams extension (`path`; `lineText` says what it is). */
  kind: "thought" | "task" | "capture";
  path: string;
  /** The block's heading line, or the task's line (0-based). */
  line: number;
  text: string;
  /** The task line or the block as it is in the file, to check before a change. */
  lineText: string;
  /** A thought's "YYYY-MM-DD HH:MM". */
  stamp: string | null;
  /** A thought's block as it is in the file (heading to body, LF), to remove it by. */
  block?: string | null;
}

export interface SourceRow extends FileSummary {
  ingested: boolean;
}

export interface Bookmark {
  target: string;
  path: string | null;
  title: string;
}

export interface SmartList {
  name: string;
  query: string;
  layers: Layer[];
}

/** The task dates Brainstead writes: 📅 due, ⏳ scheduled ("defer"), 🛫 start, ➕ created. */
export type TaskDateKind = "due" | "scheduled" | "start" | "created";

export interface Edited {
  line: number;
  lineText: string;
  /** What ⌘Z would undo now, for the toast. */
  undo: string | null;
}

/** Why Rust refused an edit. `changed`: the file changed on disk since it was read (doc_save). */
export interface EditError {
  code: "read-only" | "stale" | "conflict" | "not-found" | "invalid" | "io" | "changed" | "exists";
  message: string;
  /** With `changed`: the file as it is on disk now, and its version. */
  current?: { content: string; version: string };
}

/** Unsaved text for one file, kept by Rust in the app data folder. */
export interface Draft {
  path: string;
  content: string;
  /** The version of the file the draft was started from. */
  base: string;
  /** ms since the epoch, last keystroke. */
  at: number;
}

/** One line a rename would rewrite. */
export interface LinkChange {
  path: string;
  line: number;
  before: string;
  after: string;
}

export interface RenamePlan {
  from: string;
  to: string;
  changes: LinkChange[];
}

export interface TrashEntry {
  id: string;
  originalRel: string;
  layer: Layer;
  basename: string;
  deletedAt: string;
  sizeBytes: number;
}

// Stage 7a: Changes and Knowledge health (src-tauri/src/knowledge.rs).
export interface MeetingInference {
  filename: string;
  kind: string;
  type: string | null;
  name: string | null;
  date: string | null;
  topic: string | null;
  confident: boolean;
  ask: string[];
  suggestedFilename: string | null;
  existingNotes: string[];
  /** The names the person's existing 1-1 notes use, when the transcript's is none of them: to pick from. */
  nameForms?: string[];
  exists: boolean;
  /** The date is only the day the transcript was captured: the user confirms or changes it. */
  dateCheck: boolean;
}

export interface Transcript {
  path: string;
  mtime: number;
  inferred: MeetingInference;
  /** Dealt with already: a wiki page cites it, or a note links to it. */
  done: "ingested" | "linked" | null;
  /** Its meeting note exists already and a wiki page cites it: only the transcript is left. */
  noteIngested: boolean;
}

export interface NoteSpec {
  type: string;
  name: string;
  date: string;
}

export interface DailyCheckStatus {
  lastRun: string | null;
  next: string | null;
  running: boolean;
  summary: string;
  /** While running: the step, in words, and how far it has got (0 to 1). */
  doing: string;
  progress: number;
}

export interface ClashClaim {
  page: string;
  value: string;
  asOf: string;
  quote: string;
}

export interface ContradictionVerdict {
  id: string;
  verdict: string;
  severity: string;
  summary: string;
  correct: string;
  fix: string;
  patch: { page: string; find: string; replace: string } | null;
  judged: string;
}

export interface ContradictionItem {
  id: string;
  subject: string;
  attribute: string;
  claims: ClashClaim[];
  verdict: ContradictionVerdict | null;
}

/** A fact ingest checked and kept with a wiki page (wiki/.claims/). */
export interface KeptClaim {
  subject: string;
  attribute: string;
  value: string;
  asOf?: string;
  quote: string;
  anchor?: string;
  source: string;
  entry?: string;
  recorded: string;
}

export interface PageFact {
  subject: string;
  attribute: string;
  latest: KeptClaim;
  earlier: KeptClaim[];
}

export interface ContradictionsReport {
  last: {
    started: string;
    finished: string | null;
    running: boolean;
    doing: string;
    pages: number;
    extracted: number;
    claims: number;
    clashes: number;
    judged: number;
    contradictions: number;
    proposed: number;
    /** Fixes that couldn't be made, and why. */
    failed: number;
    failures: string[];
    error: string | null;
  };
  items: ContradictionItem[];
}

export interface IngestStep {
  name: string;
  /** waiting, running, done, skipped or failed. */
  status: string;
  detail: string;
  ms: number;
}

/** A source's provenance pane (src-tauri/src/ingest.rs `source_provenance`). */
export interface Provenance {
  sha256: string;
  size: number;
  mtime: number;
  /** Passages indexed for search. */
  chunks: number;
  citers: { path: string; title: string; listed: boolean; passages: { line: number; anchor: string | null; context: string }[] }[];
  lastRun: IngestRun | null;
}

/** A wiki page's sources count and aliases (the wiki list). */
export interface WikiMeta {
  path: string;
  sources: number;
  aliases: string[];
}

/** Triage bookmarks (src-tauri/src/skills.rs). */
export interface BookmarkRow {
  target: string;
  path: string | null;
  title: string;
  mtime: number | null;
  days: number | null;
  stale: boolean;
  missing: boolean;
}

export interface BookmarkSuggestion {
  target: string;
  decision: "keep" | "promote" | "task" | "archive";
  why: string;
  summary: string;
  page: string | null;
  kind: "concept" | "entity" | null;
  task: string | null;
}

/** Draft reply. */
export interface Reply {
  answered: "yes" | "partly" | "no" | "n/a" | string;
  verdict: string;
  callouts: { point: string; status: "answered" | "partly" | "ignored" | string }[];
  draft: string;
  grounded: string[];
  gaps: string[];
  task: string | null;
}

/** Doc check: the canonical documents register, and a check's result. */
export interface CanonicalEntry {
  key: string;
  title: string;
  version: string;
  status: "canonical" | "draft" | "superseded";
  path: string;
  resolvedPath: string;
  exists: boolean;
  aliases: string[];
  line: number;
}

export interface CanonicalRegister {
  entries: CanonicalEntry[];
  problems: string[];
  exists: boolean;
}

export interface Finding {
  kind: "diverges" | "not-covered" | "superseded-term" | "beyond-scope" | "aligned" | string;
  title: string;
  where: string;
  candidate: string;
  canonical: string;
  material: boolean;
}

export interface CheckResult {
  verdict: string;
  summary: string;
  findings: Finding[];
  against: CanonicalEntry | null;
  excluded: CanonicalEntry[];
  /** The two documents aren't about the same thing (its first look, or the comparison itself): nothing compared. */
  unrelated?: boolean;
}

/** Doc check's modes, and Compare's (two documents, A and B). */
export type CheckMode = "standard" | "callouts" | "differences" | "changes";

/** A Doc check under way or done this session (doc_check_start), with its steps. */
export interface CheckRun {
  id: string;
  candidate: string;
  doc: string;
  mode: CheckMode;
  /** running, done, failed or stopped. */
  status: string;
  started: string;
  steps: { name: string; status: string; ms: number }[];
  error: string | null;
  result: CheckResult | null;
}

/** A finished Doc check, kept with the versions it read; stale when either document changed since. */
export interface SavedCheck {
  candidate: string;
  doc: string;
  mode: CheckMode;
  at: string;
  result: CheckResult;
  stale: boolean;
}

export interface IngestRun {
  id: string;
  /** ingest, or meeting (a meeting note drafted from a transcript). */
  kind?: string;
  /** A meeting note's type, name and date. */
  note?: NoteSpec | null;
  source: string;
  started: string;
  finished: string | null;
  model: string;
  /** queued, running, done, failed or stopped. */
  status: string;
  steps: IngestStep[];
  /** Its changes' ids, in Changes. */
  proposals: string[];
  dropped: { page: string; reason: string }[];
  pages: string[];
  error: string | null;
  /** "scheduled" when the daily check or an unattended assistant started it; absent when the user did. */
  trigger?: string;
}

export interface FixNameRequest {
  wrong: string;
  right: string;
  rightPage: string | null;
  guards: string[];
  ambiguous: boolean;
  note: string;
  skipSubstitution?: boolean;
}

export interface FixNameRow {
  file: string;
  layer: Layer;
  inFilename: boolean;
  count: number;
  lines: { line: number; text: string }[];
  /** rewrite, alias, leave or guarded. */
  action: string;
}

/** A task or project the vault scan suggests (src-tauri/src/find.rs). */
export interface FindSuggestion {
  id: string;
  kind: "task" | "project";
  /** The task, or the project's name. */
  text: string;
  project?: string;
  due?: string;
  waiting: boolean;
  outcome?: string;
  tasks: string[];
  sources: { path: string; quote: string }[];
}

export interface FindState {
  run: {
    id: string;
    startedAt: string;
    finishedAt: string | null;
    status: "running" | "done" | "error" | "stopped";
    error: string | null;
    model: string;
    notes: number;
    batches: number;
    done: number;
    found: number;
    dropped: number;
  } | null;
  suggestions: FindSuggestion[];
  decided: string[];
}

/** A tool of yours that starts Claude Code sessions: its sessions match the folder, the opening, or both. */
export interface Automation {
  label: string;
  cwd_contains: string;
  opening: string;
}

/** A folder whose sessions look like a tool's: nearly all one prompt and done. */
export interface AutomationSuggestion {
  cwd: string;
  /** Its single-prompt sessions in the last two weeks. */
  count: number;
  /** All its sessions then. */
  total: number;
  last: string;
}

export interface ChangeOrigin {
  /** chat (Ask or a terminal), ingest, meeting, lint (Knowledge health), review (a summary or the weekly review) or contradiction. */
  kind: string;
  chat?: string;
  label?: string;
  /** The run it was part of: an ingest, the daily check (daily-check-…), a summary (summary-…), a chat. */
  run?: string;
  /** "scheduled" for a scheduled run, or a terminal session that said it's unattended. */
  trigger?: string;
}

/** What an agent asked for (core/src/changes.rs's Instruction). */
export type Instruction =
  | { op: "section"; section: string; content: string }
  | { op: "replace"; edits: { find: string; replace: string }[] }
  | { op: "page"; content: string }
  | { op: "add_task"; line: string; heading?: string }
  /** A thought at the top of the Scratchpad under a `## YYYY-MM-DD HH:MM` heading, as Quick capture adds it. */
  | { op: "add_thought"; text: string; stamp: string }
  | { op: "delete_line"; line: string; at?: number }
  /** Lines from `at` (0-based) rewritten: a task changed, a thought taken out. */
  | { op: "lines"; at: number; old: string[]; new: string[] }
  /** A paragraph added at the end of the page. */
  | { op: "append"; text: string }
  /** Properties set, or taken out with null. */
  | { op: "properties"; set: [string, string | null][] }
  | { op: "rename"; to: string }
  | { op: "trash" }
  /** A Trash entry put back, at the change's page. */
  | { op: "restore"; id: string }
  /** An image added to `images/`, at the change's page. */
  | { op: "image" };

/** One edit to a task's line (src-tauri/src/changes.rs's LineEdit). */
export type TaskLineEdit =
  | { op: "toggle"; done: boolean; today: string }
  | { op: "date"; kind: TaskDateKind; date: string | null }
  | { op: "contexts"; names: string[] }
  | { op: "effort"; effort: string | null }
  | { op: "project"; project: string | null }
  | { op: "rank"; prev: number | null; next: number | null };

export type ChangeKind = "edit" | "new" | "task" | "rename" | "trash" | "restore" | "image";

/** A change in Changes: what an assistant or a run did, or a change held for you. */
/** Write Current state's run, going or last. */
export interface CurrentStateRun {
  running: boolean;
  run: string;
  total: number;
  done: number;
  written: number;
  nothing: number;
  failed: string[];
  error: string | null;
  model: string;
}

export interface ChangeRow {
  id: string;
  created: string;
  origin: ChangeOrigin;
  model: string | null;
  page: string;
  kind: ChangeKind;
  title: string;
  reason: string;
  /** held: waiting for you; rejected: a held change you turned down; reverted: applied, then put back. */
  status: "held" | "applied" | "rejected" | "reverted";
  decided: string | null;
  /** Checks it didn't pass: why it's held, or what to look at in one applied anyway. */
  flags: string[];
  warnings: string[];
  /** A rename's new path. */
  to?: string | null;
  /** The run it belongs to. */
  group: string;
  revertable: boolean;
}

export interface Quote {
  source: string;
  anchor?: string;
  text: string;
  /** true: found in the source; false: not found; null: not checked (Brainstead has no text for it). */
  checked: boolean | null;
  path?: string;
}

export interface Hunk {
  index: number;
  at: number;
  old: string[];
  new: string[];
  ctxBefore: string[];
  ctxAfter: string[];
  section: string;
}

export interface ChangeView extends ChangeRow {
  quotes: Quote[];
  /** Its diff: for a held change, what it would do to the page as it is now. */
  changes: Hunk[];
  /** A held change that can't be made any more, and why. */
  problem: string | null;
}

/** A change to make, as the app takes it (src-tauri/src/changes.rs's Submit). */
export interface ChangeSubmit {
  page: string;
  kind: ChangeKind;
  title: string;
  reason?: string;
  instruction: Instruction;
  quotes?: Quote[];
  warnings?: string[];
  flags?: string[];
  origin: ChangeOrigin;
  model?: string | null;
}

export interface ChangeOutcome {
  id: string;
  applied: boolean;
  page: string;
  flags: string[];
  message: string;
}

export interface LintItem {
  text: string;
  page?: string;
  name?: string;
  pages?: string[];
  count?: number;
  detail?: string;
  safe: boolean;
}

export interface LintCheck {
  id: string;
  title: string;
  classic: boolean;
  /** Issues the user ignored, left out of items until their page changes. */
  ignored?: number;
  items: LintItem[];
}

export interface LintReport {
  checks: LintCheck[];
  wikiPages: number;
  sources: number;
  ms: number;
}

export interface HealthView {
  report: LintReport | null;
  history: [string, { total: number; decisions: number }][];
  ranAt: string | null;
}

export interface HealthFix {
  check: string;
  page: string;
  name?: string;
  detail?: string;
}

export const api = {
  meetingTranscripts: () => invoke<Transcript[]>("meeting_transcripts"),
  /** `unattended` (an assistant nobody is watching): its changes are held when a check fails. */
  meetingDraft: (items: [string, NoteSpec][], unattended = false) => invoke<string[]>("meeting_draft", { items, unattended }),
  dailyCheckStatus: () => invoke<DailyCheckStatus>("daily_check_status"),
  dailyCheckRunNow: (unattended = false) => invoke<void>("daily_check_run_now", { unattended }),
  dailyCheckStop: () => invoke<void>("daily_check_stop"),
  reviewsStop: (kind: "daily" | "weekly") => invoke<void>("reviews_stop", { kind }),
  contradictionsStop: () => invoke<void>("contradictions_stop"),
  onDailyCheckChanged: (f: () => void): Promise<UnlistenFn> => listen("daily-check-changed", () => f()),
  contradictionsReport: () => invoke<ContradictionsReport>("contradictions_report"),
  contradictionsRun: (unattended = false) => invoke<boolean>("contradictions_run", { unattended }),
  contradictionsMark: (id: string, verdict: string) => invoke<void>("contradictions_mark", { id, verdict }),
  pageContradictions: (path: string) => invoke<ContradictionItem[]>("page_contradictions", { path }),
  pageFacts: (path: string) => invoke<PageFact[]>("page_facts", { path }),
  onContradictionsChanged: (f: () => void): Promise<UnlistenFn> => listen("contradictions-changed", () => f()),
  /** `unattended`: an assistant nobody is watching asked, so its changes are held when a check fails. */
  ingestStart: (paths: string[], unattended = false) => invoke<string[]>("ingest_start", { paths, unattended }),
  wikiMeta: () => invoke<WikiMeta[]>("wiki_meta"),
  /** A PowerPoint or Excel file's text by slide or sheet. */
  sourceParts: (path: string) => invoke<{ kind: string; parts: { label: string; text: string }[] }>("source_parts", { path }),
  /** Spelling and grammar, by macOS's checker; offsets are UTF-16 (src-tauri/src/platform/spell.rs). */
  spellCheck: (text: string, lang: string | null) => invoke<[number, number][]>("spell_check", { text, lang }),
  spellGrammar: (text: string, lang: string | null) =>
    invoke<{ start: number; len: number; description: string; corrections: string[] }[]>("spell_grammar", { text, lang }),
  spellGuesses: (word: string, lang: string | null) => invoke<string[]>("spell_guesses", { word, lang }),
  /** The English spelling languages macOS has, and its own spelling language. */
  spellLanguages: () => invoke<[string[], string]>("spell_languages"),
  spellAccept: (word: string, learn: boolean) => invoke<void>("spell_accept", { word, learn }),
  bookmarksStatus: () => invoke<BookmarkRow[]>("bookmarks_status"),
  bookmarksSuggest: (items: { target: string; path: string }[]) => invoke<BookmarkSuggestion[]>("bookmarks_suggest", { items }),
  bookmarkRemove: (target: string) => invoke<void>("bookmark_remove", { target }),
  /** Triage's Keep: not stale again for two weeks. */
  bookmarkKeep: (target: string) => invoke<void>("bookmark_keep", { target }),
  /** With `current` and `change`, revises that reply as asked rather than drafting afresh. */
  draftReply: (thread: string | null, text: string | null, tone: string, current: string | null = null, change: string | null = null) =>
    invoke<Reply>("draft_reply", { thread, text, tone, current, change }),
  canonicalRegister: () => invoke<CanonicalRegister>("canonical_register"),
  docCheck: (candidate: string, doc: string, mode: CheckMode) => invoke<CheckResult>("doc_check", { candidate, doc, mode }),
  /** Starts a check in the background; its steps and result come as onDocCheckChanged. */
  docCheckStart: (candidate: string, doc: string, mode: CheckMode) => invoke<CheckRun>("doc_check_start", { candidate, doc, mode }),
  docCheckRuns: () => invoke<CheckRun[]>("doc_check_runs"),
  docCheckStop: (id: string) => invoke<void>("doc_check_stop", { id }),
  docCheckSaved: (candidate: string, doc: string, mode: CheckMode) =>
    invoke<SavedCheck | null>("doc_check_saved", { candidate, doc, mode }),
  onDocCheckChanged: (f: (r: CheckRun) => void): Promise<UnlistenFn> => listen<CheckRun>("doccheck-changed", (e) => f(e.payload)),
  sourceProvenance: (path: string) => invoke<Provenance | null>("source_provenance", { path }),
  ingestRuns: () => invoke<IngestRun[]>("ingest_runs"),
  ingestStop: (id: string) => invoke<void>("ingest_stop", { id }),
  onIngestChanged: (f: (r: IngestRun) => void): Promise<UnlistenFn> => listen<IngestRun>("ingest-changed", (e) => f(e.payload)),
  /** Bookmarks the note or takes its bookmark away; whether it's bookmarked now. */
  bookmarkToggle: (path: string) => invoke<boolean>("bookmark_toggle", { path }),
  smartListSave: (name: string, query: string, layers: string[]) => invoke<void>("smart_list_save", { name, query, layers }),
  smartListDelete: (name: string) => invoke<void>("smart_list_delete", { name }),
  /** Quits (⌘Q, once unsaved edits have been dealt with). */
  appQuit: () => invoke<void>("app_quit"),
  /**
   * `ask` is false for a quit that shouldn't ask "Quit Brainstead?": closing the window on Windows
   * with no tray icon, or the Windows install script's `Brainstead --quit`.
   */
  onQuitRequested: (f: (ask: boolean) => void): Promise<UnlistenFn> =>
    listen<string | null>("quit-requested", (e) => f(e.payload !== "unasked")),
  /** Restarts into an update just installed, once unsaved edits have been dealt with. */
  appRestart: () => invoke<void>("app_restart"),
  onRestartRequested: (f: () => void): Promise<UnlistenFn> => listen("restart-requested", () => f()),
  /** The update dialog (src/Update.tsx): an offer, how its install is going, and what there is to say. */
  onUpdateOffer: (f: (o: UpdateOffer) => void): Promise<UnlistenFn> => listen<UpdateOffer>("update-offer", (e) => f(e.payload)),
  onUpdateProgress: (f: (p: UpdateProgress) => void): Promise<UnlistenFn> => listen<UpdateProgress>("update-progress", (e) => f(e.payload)),
  onUpdateMessage: (f: (m: { title: string; text: string }) => void): Promise<UnlistenFn> =>
    listen<{ title: string; text: string }>("update-message", (e) => f(e.payload)),
  /** The offer waiting for an answer, if one is (for a window that loaded after it was made). */
  updateOffered: () => invoke<UpdateOffer | null>("update_offered"),
  updateInstall: () => invoke<void>("update_install"),
  updateLater: () => invoke<void>("update_later"),
  updateCheck: () => invoke<void>("update_check"),
  updateSkip: () => invoke<void>("update_skip"),
  fixnamePlan: (req: FixNameRequest) => invoke<{ rightPage: string | null; rows: FixNameRow[] }>("fixname_plan", { req }),
  fixnameApply: (req: FixNameRequest, rows: FixNameRow[]) => invoke<string>("fixname_apply", { req, rows }),
  /** Changes, newest first; with page, only that page's. */
  changesList: (page?: string) => invoke<ChangeRow[]>("changes_list", { page: page ?? null }),
  changeGet: (id: string) => invoke<ChangeView>("change_get", { id }),
  /** Makes a change, or holds it, by the rule for agent changes. */
  changeSubmit: (change: ChangeSubmit) => invoke<ChangeOutcome>("change_submit", { change }),
  /** `assistant`: an assistant asked, which may accept only a change held for a failed check. */
  changeAccept: (id: string, assistant = false) => invoke<ChangeOutcome>("change_accept", { id, assistant }),
  /** Changes that belong together, made or held together (an assistant's task, Inbox and project edits). */
  changesSubmitMany: (changes: ChangeSubmit[]) => invoke<ChangeOutcome[]>("changes_submit_many", { changes }),
  /** A task's line with one edit made, without writing it. */
  changeTaskLine: (line: string, edit: TaskLineEdit) => invoke<string>("change_task_line", { line, edit }),
  /** A new project note's path and text, without writing it. */
  changeProjectNote: (name: string, status: string, area: string | null, outcome: string | null) =>
    invoke<{ path: string; content: string }>("change_project_note", { name, status, area, outcome }),
  changeReject: (id: string) => invoke<void>("change_reject", { id }),
  /** Every held change in a run (or every one when group is null), oldest first; only those in ids
   *  when given (a search on Changes). */
  changesAcceptAll: (group: string | null, assistant = false, ids?: string[]) =>
    invoke<{ done: number; failed: string[] }>("changes_accept_all", { group, assistant, ids: ids ?? null }),
  changesRejectAll: (group: string | null, ids?: string[]) =>
    invoke<{ done: number; failed: string[] }>("changes_reject_all", { group, ids: ids ?? null }),
  /** Undoes every applied change in a run (only those in ids when given) on the pages as they are now. */
  changesRevertAll: (group: string, ids?: string[]) =>
    invoke<{ done: number; failed: string[] }>("changes_revert_all", { group, ids: ids ?? null }),
  /** Undoes an applied change on the page as it is now; when its lines were edited since, says so with the page as it was before. */
  changeRevert: (id: string) => invoke<{ ok: boolean; message: string; before: string | null }>("change_revert", { id }),
  /** The page with the held change made, as its draft for the editor; returns the page. */
  changeForEditing: (id: string) => invoke<string>("change_for_editing", { id }),
  healthReport: (fresh = false) => invoke<HealthView>("health_report", { fresh }),
  healthFix: (fixes: HealthFix[]) => invoke<string>("health_fix", { fixes }),
  /** Reshape pages: every page that can be reshaped by itself, or those named. */
  healthReshape: (pages: string[] | null, unattended = false) =>
    invoke<{ run: string; applied: number; failed: string[]; left: number }>("health_reshape", { pages, unattended }),
  healthDismiss: (a: string, b: string) => invoke<void>("health_dismiss", { a, b }),
  /** Ignores an issue (its check and its line) until its page changes. */
  healthIgnore: (check: string, text: string) => invoke<void>("health_ignore", { check, text }),
  /** Orphan pages' Add them to index.md: index.md's catalogue brought up to date now; what happened. */
  catalogueUpdate: () => invoke<string>("catalogue_update"),
  /** Ignore all, on stale pages and claims with no citation: how many were ignored. */
  healthIgnoreAll: (check: string) => invoke<number>("health_ignore_all", { check }),
  /** Shows a check's ignored issues again (every check's with null); returns how many. */
  healthUnignore: (check: string | null) => invoke<number>("health_unignore", { check }),
  healthTrashImage: (path: string) => invoke<string>("health_trash_image", { path }),
  healthCreatePage: (name: string, folder: "entities" | "concepts") => invoke<string>("health_create_page", { name, folder }),
  healthLinkGhost: (target: string, to: string, pages: string[], unattended = false) =>
    invoke<number>("health_link_ghost", { target, to, pages, unattended }),
  onChangesChanged: (f: () => void): Promise<UnlistenFn> => listen("changes-changed", () => f()),
  /** Write Current state: a run in the background on the pages named, or every page that wants one, at most `limit`. False when one is going. */
  currentStateStart: (pages: string[] | null, limit: number | null, unattended = false) =>
    invoke<boolean>("current_state_start", { pages, limit, unattended }),
  currentStateStatus: () => invoke<CurrentStateRun>("current_state_status"),
  currentStateStop: () => invoke<void>("current_state_stop"),
  onCurrentStateChanged: (f: (r: CurrentStateRun) => void): Promise<UnlistenFn> =>
    listen<CurrentStateRun>("current-state-changed", (e) => f(e.payload)),
  onHealthChanged: (f: (c: { decisions: number; total: number }) => void): Promise<UnlistenFn> =>
    listen<{ decisions: number; total: number }>("health-changed", (e) => f(e.payload)),
  settingsRead: () => invoke<Settings>("settings_read"),
  settingsWrite: (settings: Settings) => invoke<Settings>("settings_write", { settings }),
  vaultStatus: () => invoke<VaultStatus>("vault_status"),
  rebuildIndex: () => invoke<void>("rebuild_index"),
  /** Which features this system has (src/features.ts keeps the answer). */
  platformFeatures: () => invoke<PlatformFeatures>("platform_features"),
  permissions: () => invoke<Permissions>("permissions"),
  openFullDiskAccess: () => invoke<void>("open_full_disk_access"),
  reveal: (path: string) => invoke<void>("reveal", { path }),
  /** Shows the app's log folder in the file manager. */
  logsReveal: () => invoke<void>("logs_reveal"),
  /** Whether Brainstead opens at login; null where the system can't say. */
  loginItem: () => invoke<boolean | null>("login_item"),
  loginItemSet: (on: boolean) => invoke<boolean | null>("login_item_set", { on }),
  /** Open at login changed, from Settings or the menu-bar window. */
  onLoginItemChanged: (f: (on: boolean | null) => void): Promise<UnlistenFn> =>
    listen<boolean | null>("login-item-changed", (e) => f(e.payload)),
  /** The switch-over checklist's own checks (Settings › General). */
  /** `previous`: the previous app's own files are in the vault, so the checklist shows. */
  switchoverStatus: () =>
    invoke<{ automated: number | null; skills: boolean; previous?: boolean; scripts: boolean; claudeMd: boolean }>("switchover_status"),
  /** Moves the previous app's skills and scripts to the Trash and tells agents in CLAUDE.md to use the MCP tools. */
  switchoverRetire: () => invoke<string>("switchover_retire"),
  /** Your own tools that start Claude Code sessions (automated.json), and saving them. */
  automatedList: () => invoke<Automation[]>("automated_list"),
  /** Find tasks and projects: the last scan and its suggestions, a new scan, stopping it, and accepting or skipping one. */
  findStatus: () => invoke<FindState>("find_status"),
  findRun: () => invoke<string>("find_run"),
  findStop: () => invoke<void>("find_stop"),
  /** `unattended` (an assistant nobody is watching): an accepted suggestion is held when a check fails. */
  findDecide: (id: string, action: "accept" | "skip", edit: Record<string, unknown> | null, unattended = false) =>
    invoke<string | null>("find_decide", { id, action, edit, unattended }),
  onFindChanged: (f: () => void): Promise<UnlistenFn> => listen("find-changed", () => f()),
  automatedSave: (list: Automation[]) => invoke<void>("automated_save", { list }),
  /** Folders whose recent sessions look like a tool's, as tools to list. */
  automatedSuggest: () => invoke<AutomationSuggestion[]>("automated_suggest"),
  appInfo: () => invoke<AppInfo>("app_info"),
  /** The files around `center` to `depth` links, or the whole wiki when `center` is null. */
  graph: (center: string | null, depth: number) => invoke<Graph>("graph", { center, depth }),
  activity: () => invoke<{ log: LogEntry[]; days: ActivityDay[] }>("activity"),
  glance: () => invoke<Glance>("glance"),
  activityDay: (date: string) => invoke<FileSummary[]>("activity_day", { date }),
  /** Which of these vault files are still there, in the same order. */
  pathsExist: (paths: string[]) => invoke<boolean[]>("paths_exist", { paths }),
  filesList: (layer: Layer | null) => invoke<FileSummary[]>("files_list", { layer }),
  docRead: (path: string) => invoke<Doc>("doc_read", { path }),
  linksResolve: (targets: string[]) => invoke<(string | null)[]>("links_resolve", { targets }),
  search: (q: string, layers: Layer[] = [], limit = 200) => invoke<SearchResults>("search", { req: { q, layers, limit } }),
  tasksQuery: (query: TaskQuery) => invoke<TaskRow[]>("tasks_query", { query }),
  /** Every task outside the templates, for task query blocks (src/tasksq/). */
  tasksAll: () => invoke<TaskRow[]>("tasks_all"),
  /** Replaces a task's line with `lines` (src/tasksq/edits.ts works them out); undoable. */
  taskReplace: (t: { path: string; line: number; lineText: string }, lines: string[], label: string) =>
    invoke<Edited>("task_replace", { path: t.path, line: t.line, lineText: t.lineText, lines, label }),
  sourcesList: () => invoke<SourceRow[]>("sources_list"),
  /** Files from outside the vault (full paths, dropped from Finder) copied into sources/. */
  sourcesImport: (paths: string[]) => invoke<{ name: string; path: string | null; error: string | null }[]>("sources_import", { paths }),
  bookmarks: () => invoke<Bookmark[]>("bookmarks"),
  smartLists: () => invoke<SmartList[]>("smart_lists"),
  assetFind: (name: string) => invoke<string | null>("asset_find", { name }),
  copyText: (text: string) => invoke<void>("copy_text", { text }),
  copyFile: (path: string) => invoke<void>("copy_file", { path }),
  taskToggle: (t: TaskRow, done: boolean, today: string) =>
    invoke<Edited>("task_toggle", { path: t.path, line: t.line, lineText: t.lineText, done, today }),
  taskSetDate: (t: TaskRow, kind: TaskDateKind, date: string | null) =>
    invoke<Edited>("task_set_date", { path: t.path, line: t.line, lineText: t.lineText, kind, date }),
  taskRank: (t: TaskRow, prev: number | null, next: number | null) =>
    invoke<boolean>("task_rank", { path: t.path, line: t.line, lineText: t.lineText, prev, next }),
  tasksRankAll: (ts: TaskRow[]) =>
    invoke<number>("tasks_rank_all", { tasks: ts.map((t) => ({ path: t.path, line: t.line, lineText: t.lineText })) }),
  undo: () => invoke<string | null>("undo"),
  undoPeek: () => invoke<{ label: string } | null>("undo_peek"),
  capture: (kind: "task" | "thought", text: string, stamp: string) => invoke<string>("capture", { kind, text, stamp }),
  captureClose: () => invoke<void>("capture_close"),
  captureShow: () => invoke<void>("capture_show"),
  captureShortcutStatus: () => invoke<[string | null, string | null]>("capture_shortcut_status"),
  scene: () => invoke<string | null>("scene"),
  // Stage 4: the editor and the files around it (src-tauri/src/notes.rs).
  /** Saves the whole file if it's still at `base`; returns the new version. */
  docSave: (path: string, content: string, base: string) => invoke<string>("doc_save", { path, content, base }),
  /** Creates a new file (vault-relative); refuses with `exists`. Returns its version. */
  docCreate: (path: string, content: string) => invoke<string>("doc_create", { path, content }),
  /** Makes a new vault called `name` inside `parent`, filled with the example notes; returns its path. */
  vaultCreate: (parent: string, name: string) => invoke<string>("vault_create", { parent, name }),
  /** Adds the example notes the open vault doesn't have; returns how many were added. */
  vaultAddExamples: () => invoke<number>("vault_add_examples"),
  draftWrite: (draft: Draft) => invoke<void>("draft_write", { draft }),
  draftRead: (path: string) => invoke<Draft | null>("draft_read", { path }),
  draftDiscard: (path: string) => invoke<void>("draft_discard", { path }),
  draftsList: () => invoke<Draft[]>("drafts_list"),
  /** Saves image bytes (base64) to the vault's `images/` folder; returns the vault-relative path. */
  imageSave: (name: string, base64: string) => invoke<string>("image_save", { name, base64 }),
  /** Same, for a file dropped from Finder (absolute path). */
  imageImport: (abs: string) => invoke<string>("image_import", { abs }),
  /** An image an assistant adds (add_image): saved to `images/` and recorded in Changes. */
  imageAdd: (name: string, base64: string, reason: string | null, origin: ChangeOrigin, model: string | null) =>
    invoke<ChangeOutcome>("image_add", { name, base64, reason, origin, model }),
  renamePreview: (path: string, to: string) => invoke<RenamePlan>("rename_preview", { path, to }),
  renameCommit: (plan: RenamePlan) => invoke<string>("rename_commit", { plan }),
  trashMove: (path: string) => invoke<TrashEntry>("trash_move", { path }),
  /** Quick Look on a trashed file, where it lies. */
  trashQuickLook: (id: string) => invoke<void>("trash_quick_look", { id }),
  /** A vault file in the system's Quick Look panel, as Finder's space bar shows it. */
  fileQuickLook: (path: string) => invoke<void>("file_quick_look", { path }),
  trashList: () => invoke<TrashEntry[]>("trash_list"),
  /** Puts it back where it was, or at `as` (vault-relative); refuses with `exists`. */
  trashRestore: (id: string, as: string | null) => invoke<string>("trash_restore", { id, as }),
  trashDelete: (id: string) => invoke<void>("trash_delete", { id }),
  trashEmpty: () => invoke<number>("trash_empty"),
  /** The clipboard's text (Templater's tp.system.clipboard). */
  clipboardRead: () => invoke<string>("clipboard_read"),
  /** Templater's user scripts: the `.js` files in a vault folder. */
  userScripts: (folder: string) => invoke<{ name: string; source: string }[]>("user_scripts", { folder }),
  /** HTML (asset:// images inlined by Rust as data URIs) and plain text onto the clipboard. */
  copyRich: (html: string, text: string) => invoke<void>("copy_rich", { html, text }),
  /** Prints the main window's page (its @media print styles) to a PDF at `dest`, with `footer`
   *  (the title) and page numbers along the bottom of every page. */
  /** `forcedBreaks`: the page was given forced page breaks (split tables), so a blank last page goes. */
  exportPdf: (dest: string, footer: string | null = null, forcedBreaks = false) =>
    invoke<void>("export_pdf", { dest, footer, forcedBreaks }),
  /** Whether a note is open, so the Note menu's items apply. */
  menuNoteOpen: (open: boolean) => invoke<void>("menu_note_open", { open }),
  /** A Note menu item was chosen: its id, as `note:save`. */
  onMenu: (f: (id: string) => void): Promise<UnlistenFn> => listen<string>("menu", (e) => f(e.payload)),
  onIndexStatus: (f: (s: VaultStatus) => void): Promise<UnlistenFn> => listen<VaultStatus>("index-status", (e) => f(e.payload)),
  /** A file renamed (`to`) or trashed (`to` null), from any screen, an assistant or Changes; `draft`
   *  is where its unsaved edits belong now. */
  onFileMoved: (f: (m: { from: string; to: string | null; draft: string }) => void): Promise<UnlistenFn> =>
    listen<{ from: string; to: string | null; draft: string }>("file-moved", (e) => f(e.payload)),
  onVaultChanged: (f: (c: VaultChanges) => void): Promise<UnlistenFn> => listen<VaultChanges>("vault-changed", (e) => f(e.payload)),
  /** A capture from the Outlook or Teams extension, written (`path`) or refused (`error`). */
  /** Settings › Capture extensions: the folder to load them from, the browsers that know the host, the last captures. */
  captureStatus: () =>
    invoke<{
      folder: string | null;
      browsers: { name: string; registered: boolean; current: boolean }[];
      recent: Captured[];
      /** Each extension's version in the folder, by its folder name (teams-capture). */
      versions: Record<string, string>;
    }>("capture_status"),
  captureReveal: (which: "outlook-capture" | "teams-capture") => invoke<void>("capture_reveal", { which }),
  onCapture: (f: (c: Captured) => void): Promise<UnlistenFn> => listen<Captured>("capture", (e) => f(e.payload)),
  // Teams and Outlook sync (src-tauri/src/sync/). macOS only.
  syncSignInState: () => invoke<SyncSignIn>("sync_sign_in_state"),
  syncSignOut: () => invoke<void>("sync_sign_out"),
  syncNow: () => invoke<SyncReport>("sync_now"),
  syncTodayMeetings: () => invoke<SyncMeeting[]>("sync_today_meetings"),
  /** The last calendar read's meetings that haven't ended — instant, no network. */
  syncCachedMeetings: () => invoke<SyncMeeting[]>("sync_cached_meetings"),
  syncListChannels: () => invoke<{ id: string; topic: string }[]>("sync_list_channels"),
  syncPull: (query: string, days: number, kinds?: string[]) =>
    invoke<{
      items: { kind: string; title: string; date: string; path: string; alreadyHad: boolean }[];
      problems: string[];
      blocked: string | null;
    }>("sync_pull", { query, days, kinds }),
  // Dataview (src-tauri/src/dataview.rs, src/md/dataview/).
  /** Pages changed since generation `since` (0: all), and every page's path now. */
  dataviewPages: (since: number) => invoke<DvPagesDelta>("dataview_pages", { since }),
  /** A vault text file, read-only; null when it isn't there. */
  dataviewRead: (path: string) => invoke<string | null>("dataview_read", { path }),
  // GTD (src-tauri/src/gtd.rs, src/gtd.ts).
  projectsList: () => invoke<ProjectRow[]>("projects_list"),
  inboxList: () => invoke<InboxItem[]>("inbox_list"),
  /** A capture clarified in the Inbox: it leaves the Inbox (the file stays in sources/). */
  inboxCaptureDone: (path: string) => invoke<void>("inbox_capture_done", { path }),
  taskSetContexts: (t: TaskRef, contexts: string[]) =>
    invoke<Edited>("task_set_contexts", { path: t.path, line: t.line, lineText: t.lineText, contexts }),
  taskSetEffort: (t: TaskRef, effort: string | null) =>
    invoke<Edited>("task_set_effort", { path: t.path, line: t.line, lineText: t.lineText, effort }),
  taskSetProject: (t: TaskRef, projectPath: string | null) =>
    invoke<Edited>("task_set_project", { path: t.path, line: t.line, lineText: t.lineText, projectPath }),
  projectCreate: (name: string, status: string, area: string | null, outcome: string | null) =>
    invoke<{ path: string; undo: string }>("project_create", { name, status, area, outcome }),
  projectAddTask: (path: string, text: string, heading = "Next actions") => invoke<Edited>("project_add_task", { path, text, heading }),
  /** A task at the top of the To Do list's `#### Other`, as Quick capture adds it, but undoable. */
  taskAdd: (text: string) => invoke<Edited>("task_add", { text }),
  projectSet: (path: string, key: "status" | "area" | "outcome", value: string | null) =>
    invoke<string | null>("project_set", { path, key, value }),
  /** Renames an area on each of its projects (`areaProjects`), undone together. */
  projectsSetArea: (paths: string[], from: string, to: string) => invoke<string>("projects_set_area", { paths, from, to }),
  /** `block` is the item's block, checked before it's removed. */
  inboxRemoveThought: (line: number, text: string) => invoke<string | null>("inbox_remove_thought", { line, text }),
  inboxMoveTask: (line: number, lineText: string, toPath: string, heading: string, newLine: string) =>
    invoke<string | null>("inbox_move_task", { line, lineText, toPath, heading, newLine }),
  /** What each Inbox item could become, by the default model (src-tauri/src/weekly.rs). */
  clarifySuggest: (items: { id: string; kind: string; text: string }[], projects: string[]) =>
    invoke<ClarifySuggestion[]>("clarify_suggest", { items, projects }),
  // The guided weekly review (src-tauri/src/weekly.rs, src/Weekly.tsx).
  weeklyContext: (week: string) => invoke<WeeklyContext>("weekly_context", { week }),
  weeklyStateRead: () => invoke<WeeklyState | null>("weekly_state_read"),
  weeklyStateWrite: (state: WeeklyState | null) => invoke<void>("weekly_state_write", { state }),
  /** Writes the week's own review note (Me. Weekly Review - YYYY-Www.md); returns its path. */
  weeklyFinish: (week: string, body: string) => invoke<string>("weekly_finish", { week, body }),
  /** The week's review note, once the review has been finished. */
  weeklyReviewNote: (week: string) => invoke<string | null>("weekly_review_note", { week }),
  // The weekly review's preparation (src-tauri/src/weekprep.rs).
  weekprepStatus: (week: string) => invoke<WeekPrepStatus>("weekprep_status", { week }),
  /** Prepares the week now (without one, the week a review started today is for); returns the run's chat id. */
  weekprepRun: (week?: string) => invoke<string>("weekprep_run", { week: week ?? null }),
  /** Prepares the week if it has no preparation or the vault has changed since; true when a run started. */
  weekprepEnsure: (week: string) => invoke<boolean>("weekprep_ensure", { week }),
  /** An accepted link suggestion, made as an agent change (revertable in Changes), or held there.
   *  `unattended` (an assistant nobody is watching): held when a check fails. */
  weekprepLink: (week: string, path: string, phrase: string, target: string, unattended = false) =>
    invoke<ChangeOutcome>("weekprep_link", { week, path, phrase, target, unattended }),
  /** The job's last run and next scheduled time (Settings › Jobs & schedule). */
  weekprepJob: () => invoke<WeekPrepJob>("weekprep_job"),
  /** Stops the preparation running now. */
  weekprepStop: () => invoke<void>("weekprep_stop"),
  onWeekprepChanged: (f: () => void): Promise<UnlistenFn> => listen("weekprep-changed", () => f()),
  // Ask (src-tauri/src/ask.rs, src/askState.ts).
  askClis: () => invoke<CliInfo[]>("ask_clis"),
  askSend: (chatId: string, prompt: string, model: string, session: string | null) =>
    invoke<void>("ask_send", { chatId, prompt, model, session }),
  askCancel: (chatId: string) => invoke<void>("ask_cancel", { chatId }),
  /** How a chat's latest turn stands, for an `ask-done` the window missed. */
  askOutcome: (chatId: string) => invoke<{ running: boolean; done: AskDone | null }>("ask_outcome", { chatId }),
  askLog: (line: string) => invoke<void>("ask_log", { line }),
  chatTitle: (user: string, assistant: string) => invoke<string | null>("chat_title", { user, assistant }),
  askNext: (title: string, turns: [string, string][]) => invoke<string | null>("ask_next", { title, turns }),
  chatsList: () => invoke<ChatSummary[]>("chats_list"),
  chatRead: (filename: string) => invoke<SavedChat>("chat_read", { filename }),
  /** Writes a chat where it is; `keep` saves it to the vault, where it stays. */
  chatSave: (chat: SavedChat, keep = false) => invoke<ChatSummary>("chat_save", { chat, keep }),
  chatRename: (filename: string, title: string) => invoke<string>("chat_rename", { filename, title }),
  chatTrash: (filename: string) => invoke<void>("chat_trash", { filename }),
  skillsList: () => invoke<Skill[]>("skills_list"),
  onAskEvent: (f: (e: { chatId: string; event: AskEvent }) => void): Promise<UnlistenFn> =>
    listen<{ chatId: string; event: AskEvent }>("ask-event", (e) => f(e.payload)),
  onAskDone: (f: (d: AskDone) => void): Promise<UnlistenFn> => listen<AskDone>("ask-done", (e) => f(e.payload)),
  // Scheduled reviews (src-tauri/src/reviews.rs, src/Jobs.tsx).
  reviewsStatus: () => invoke<ReviewsStatus>("reviews_status"),
  /** `day` (YYYY-MM-DD) runs it for that day, or that day's week. */
  reviewRunNow: (kind: "daily" | "weekly", day?: string, unattended = false) =>
    invoke<string>("review_run_now", { kind, day: day ?? null, unattended }),
  reviewUndo: (id: string) => invoke<void>("review_undo", { id }),
  /** The latest daily summary block (within a month) or weekly one (this week's or last week's). */
  reviewLatest: (kind: "daily" | "weekly") => invoke<LatestSummary | null>("review_latest", { kind }),
  onReviewsChanged: (f: () => void): Promise<UnlistenFn> => listen("reviews-changed", () => f()),
  /** A review run started: its chat id and title, to open as a tab. */
  onReviewStarted: (f: (r: { chatId: string; title: string; model: string; prompt: string }) => void): Promise<UnlistenFn> =>
    listen<{ chatId: string; title: string; model: string; prompt: string }>("review-started", (e) => f(e.payload)),
  /** A review run finished and its chat was saved. */
  onReviewDone: (f: (r: { chatId: string; filename: string | null }) => void): Promise<UnlistenFn> =>
    listen<{ chatId: string; filename: string | null }>("review-done", (e) => f(e.payload)),
};

export type TaskRef = { path: string; line: number; lineText: string };

export interface ClarifySuggestion {
  id: string;
  /** next | project | waiting | done | someday | reference | delete */
  becomes: string;
  /** The task text (next, waiting, someday), the project's name (project) or the note to file it in (reference). */
  text: string;
  project: string | null;
  context: string | null;
  effort: string | null;
  due: string | null;
  why: string;
}

/** What the weekly review's side pane shows: the week, the weekly summary's block and counts. */
export interface WeeklyContext {
  week: string;
  /** "28 Sep – 4 Oct". */
  range: string;
  start: string;
  end: string;
  /** The weekly summary's block for the week, if it ran. */
  block: string | null;
  file: string;
  /** The week's own review note, once the review has been finished. */
  review: string | null;
  notes: { path: string; title: string; mtime: number }[];
  meetings: { path: string; title: string; mtime: number }[];
  ghostLinks: { target: string; refs: number }[];
}

/** The steps the weekly review's suggestions are for (brainstead_core::reviews::prep::STEPS). */
export type WeekPrepStep = "loose" | "inbox" | "notes" | "back" | "next" | "ahead" | "projects" | "waiting" | "someday" | "creative";

/** A task a suggestion acts on; `line` is 0-based. */
export interface WeekPrepTask {
  path: string;
  line: number;
  lineText: string;
  text: string;
}

/** The Inbox item a clarify suggestion is on, as the Inbox lists it. */
export interface WeekPrepInboxItem {
  kind: "capture" | "thought" | "task";
  path: string;
  line: number;
  lineText: string;
  text: string;
}

/** What accepting a suggestion does. Nothing is applied without a click. */
export type WeekPrepAction =
  | {
      do: "add";
      text: string;
      /** An active project's name. */
      project: string | null;
      context: string | null;
      /** Without the `#`. */
      tags: string[];
      due: string | null;
      scheduled: string | null;
    }
  | { do: "tick"; task: WeekPrepTask }
  | { do: "defer"; task: WeekPrepTask; date: string }
  | { do: "waiting"; task: WeekPrepTask }
  | { do: "someday"; task: WeekPrepTask }
  | { do: "edit"; task: WeekPrepTask; text: string }
  | {
      do: "clarify";
      item: WeekPrepInboxItem;
      becomes: "next" | "project" | "waiting" | "done" | "someday" | "reference" | "delete";
      text: string | null;
      project: string | null;
      context: string | null;
      due: string | null;
    }
  /** The first plain mention of `phrase` in the note linked to `target`, through Changes (`weekprepLink`). */
  | { do: "link"; path: string; phrase: string; target: string };

export interface WeekPrepSuggestion {
  /** Stable across preparations of the same week. */
  id: string;
  step: WeekPrepStep;
  text: string;
  source: { path: string; quote: string } | null;
  action: WeekPrepAction | null;
}

/** A week's prepared suggestions. */
export interface WeekPrep {
  week: string;
  /** Local `YYYY-MM-DDTHH:MM:SS`. */
  preparedAt: string;
  model: string;
  /** The vault it saw. */
  stamp: string;
  suggestions: WeekPrepSuggestion[];
  /** Suggestions left out because they failed the checks. */
  dropped: number;
}

export interface WeekPrepStatus {
  prep: WeekPrep | null;
  running: boolean;
  /** The week's last preparation failed: why. */
  error: string | null;
}

/** A run of the weekly review's preparation (src-tauri/src/weekprep.rs). */
export interface WeekPrepRun {
  id: string;
  week: string;
  /** schedule, start (the review was started) or manual. */
  trigger: string;
  model: string;
  startedAt: string;
  finishedAt: string | null;
  status: "running" | "done" | "error" | "stopped";
  error: string | null;
  /** How many suggestions it made. */
  count: number;
  /** The run's chat, to open in Ask. */
  chat: string | null;
}

/** The preparation's row in Settings › Jobs & schedule. */
export interface WeekPrepJob {
  /** The week being prepared now. */
  running: string | null;
  last: WeekPrepRun | null;
  /** When it next runs on schedule; null when it won't (switched off, or Brainstead doesn't run the jobs). */
  next: string | null;
  enabled: boolean;
}

/** A guided weekly review in progress, kept so it can be paused. */
export interface WeeklyState {
  week: string;
  step: number;
  done: number[];
  /** Decisions and notes so far, one line each, for the summary. */
  log: string[];
  notes: string;
  startedAt: number;
  /** The prepared suggestions accepted or skipped, by id, so they stay hidden after a pause. */
  handled?: Record<string, "accepted" | "skipped">;
}

export type CliName = "claude" | "codex" | "antigravity" | "copilot";

export interface CliInfo {
  cli: CliName;
  path: string | null;
  version: string | null;
  models: { id: string; name: string }[];
}

export type AskEvent =
  | { kind: "text"; text: string }
  | { kind: "status"; text: string }
  | { kind: "usage"; tokens: number; window: number | null }
  | { kind: "compacted" };

export interface AskDone {
  chatId: string;
  text: string;
  session: string | null;
  error: string | null;
}

/** One turn in a chat, as the previous app saves them. */
export interface ChatItem {
  id: string;
  role: "user" | "assistant" | "tool" | "system" | "error";
  text: string;
  /** The model that answered, on an assistant turn. */
  meta?: string;
}

export interface ChatSummary {
  /** Vault-relative, or `local:<name>` while it waits in the app data folder. Empty for a new chat. */
  filename: string;
  id: string;
  title: string;
  claudeSessionId: string | null;
  sessionId: string | null;
  model: string | null;
  createdAt: string;
  updatedAt: string;
  contextTokens: number | null;
  contextWindow: number | null;
  compactions: number;
  state: "pinned" | "archived";
  /** "review" for a scheduled review's run. */
  kind?: string | null;
}

export interface SavedChat extends ChatSummary {
  transcript: ChatItem[];
}

export interface Skill {
  name: string;
  description: string;
  /** brainstead: the app's own workflow, for any assistant; vault: a Claude Code skill in the vault. */
  source: "brainstead" | "vault";
  /** A vault skill Brainstead now does on its own screen: that screen (fixname opens the dialog). */
  opens?: string;
}

/** A link as Dataview's pages carry it: as written, and where it goes (null: nowhere). */
export interface DvRawLink {
  target: string;
  path: string | null;
  display: string | null;
  subpath: string | null;
  embed: boolean;
  line: number;
}

/** An inline field: key, value as written, 0-based line. */
export type DvRawField = [string, string, number];

export interface DvRawItem {
  line: number;
  lineCount: number;
  lineText: string;
  text: string;
  /** The character in `[ ]`, or null for a plain list item. */
  status: string | null;
  parent: number | null;
  section: string | null;
  blockId: string | null;
  tags: string[];
  links: DvRawLink[];
  fields: DvRawField[];
}

export interface DvRawPage {
  path: string;
  title: string;
  size: number;
  ctime: number;
  mtime: number;
  day: string | null;
  frontmatter: Record<string, unknown>;
  aliases: string[];
  etags: string[];
  links: DvRawLink[];
  fields: DvRawField[];
  lists: DvRawItem[];
}

export interface DvPagesDelta {
  gen: number;
  all: string[];
  changed: DvRawPage[];
}
