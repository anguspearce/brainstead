// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Brainstead's engine: reading the vault, parsing notes, and the SQLite index. No Tauri here, so
//! it is unit-tested on its own and builds on every platform.

pub mod activity;
pub mod ask;
pub mod assets;
pub mod bridge;
pub mod canonical;
pub mod catalogue;
pub mod changes;
pub mod chats;
pub mod claims;
pub mod contradictions;
pub mod dataview;
pub mod drafts;
pub mod extract;
pub mod features;
pub mod filename;
pub mod find;
pub mod fixname;
pub mod frontmatter;
pub mod graph;
pub mod help;
pub mod images;
pub mod inbox;
pub mod index;
pub mod ingest;
pub mod links;
pub mod lint;
pub mod lists;
pub mod markdown;
pub mod meeting;
pub mod names;
pub mod note;
pub mod pageshape;
pub mod pageview;
pub mod paths;
pub mod projects;
pub mod proposals;
pub mod read;
pub mod rename;
pub mod reviews;
pub mod scripts;
pub mod search;
pub mod starter;
pub mod taskquery;
pub mod tasks;
pub mod trash;
pub mod vault;
pub mod watch;
pub mod webcapture;
pub mod workflows;
pub mod write;

pub use index::{Index, IndexStats};
pub use vault::{Layer, VaultFile};
