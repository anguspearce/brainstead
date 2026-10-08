// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! `brainstead-mcp [--data <app data folder>]`: Brainstead's MCP server on stdin and stdout, for a
//! CLI in a terminal. The app runs the same server as `Brainstead --mcp`.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // The app's own data folder (brainstead_core::paths::data_dir), so both read the same index.
    std::process::exit(brainstead_mcp::main_with(&args, brainstead_core::paths::data_dir()));
}
