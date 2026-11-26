// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Times Wiki health's checks over a vault, read-only:
//! `cargo run --release -p brainstead-core --example lint_bench -- <vault>`.

fn main() {
    let root = std::env::args().nth(1).expect("usage: lint_bench <vault>");
    let today = chrono::Local::now().date_naive();
    let r = brainstead_core::lint::run(std::path::Path::new(&root), today, &Default::default());
    for c in &r.checks {
        println!("{:>5}  {}", c.items.len(), c.id);
    }
    println!("{} wiki pages, {} sources, {} ms", r.wiki_pages, r.sources, r.ms);
}
