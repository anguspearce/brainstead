// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// What needs you, counted once (D-20261119-04): Today's To process, the sidebar's badges and the
// menu-bar window count with these, so each number is the one the screen it opens shows.

import type { BookmarkRow, ChangeRow, InboxItem } from "./api";
import { unclarified } from "./gtd";
import { decisions, held } from "./knowledge";

/** The Inbox's items still to clarify (its captures, sources no page cites, among them). */
export const toClarify = (inbox: InboxItem[]) => inbox.filter(unclarified).length;

/** Bookmarks worth a triage: gone stale (two weeks untouched) or pointing at a missing note. */
export const toTriage = (rows: BookmarkRow[]) => rows.filter((b) => b.stale || b.missing).length;

/** Changes held for you to accept or reject. */
export const heldCount = (rows: ChangeRow[]) => held(rows).length;

/** Wiki health's issues only you can settle, as its To decide says. */
export const toDecide = decisions;
