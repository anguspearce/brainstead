// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// The note filename grammar `Type. Title - YYYY-MM-DD.md`, read as src-tauri/core/src/filename.rs
// reads it, and built as the previous app's `composeFilename` (ui/src/lib/note-types.ts) builds it.

export interface NoteName {
  type: string | null;
  title: string;
  date: string | null;
}

const EXT = /\.(md|txt)$/i;

/** The file name without its folders and `.md` / `.txt`. */
export const stem = (rel: string) => (rel.split("/").pop() ?? rel).replace(EXT, "");

/** The folder part of a vault path, with its trailing slash ("" at the root). */
export const folderOf = (rel: string) => (rel.includes("/") ? rel.slice(0, rel.lastIndexOf("/") + 1) : "");

const DATE_SUFFIX = /^\d{4}-\d{2}(-\d{2})?$/;

export function parseName(rel: string): NoteName {
  const s = stem(rel);
  const i = s.indexOf(". ");
  const type = i > 0 ? s.slice(0, i) : null;
  let rest = i > 0 ? s.slice(i + 2) : s;
  let date: string | null = null;
  const j = rest.lastIndexOf(" - ");
  if (j >= 0 && DATE_SUFFIX.test(rest.slice(j + 3))) {
    date = rest.slice(j + 3);
    rest = rest.slice(0, j).trimEnd();
  }
  const title = rest.trim();
  return { type, title: title || s, date };
}

/** Strips characters a filename can't hold (or a wikilink to it can't: # ^ [ ] |), and runs of spaces. */
const clean = (v: string) =>
  v
    .replace(/[/\\:*?"<>|#^[\]]/g, "")
    .replace(/\s+/g, " ")
    .trim();

/** `[Type. ]Title[ - Date].md`, or "" when there's no usable title. */
export function composeFilename(n: { type?: string | null; title: string; date?: string | null }, ext = ".md"): string {
  // A name can't start with a dot: the vault skips hidden files, so ".NET migration.md" would be
  // made and never listed (as Rust's `compose`).
  const undotted = (v: string) => v.replace(/^[. ]+/, "");
  const type = n.type ? undotted(clean(n.type)) : "";
  const title = type ? clean(n.title.replace(EXT, "")) : undotted(clean(n.title.replace(EXT, "")));
  if (!title) return "";
  const date = n.date?.trim();
  let s = type ? `${type}. ${title}` : title;
  if (date) s += ` - ${date}`;
  return s + ext;
}
