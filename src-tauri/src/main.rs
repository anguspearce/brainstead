// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// No console window on Windows, in dev builds too. The MCP server (--mcp) and the capture
// extensions' host still talk over the pipes they were started with: a windows-subsystem program
// keeps the standard handles it inherits; only a console of its own is never made.
#![cfg_attr(windows, windows_subsystem = "windows")]

fn main() {
    if let Some(code) = brainstead_lib::mcp_main() {
        std::process::exit(code);
    }
    brainstead_lib::run()
}
