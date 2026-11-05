// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Grammar on macOS and Windows alike, from Harper, as Two-edged Sword's journal has it: it runs
//! here on the computer, so nothing written leaves it. macOS's own grammar checker caught little
//! more than a doubled word, and Windows has none for apps. Spelling stays with each system's own
//! checker (spell.rs, spell_win.rs). Offsets are UTF-16, as in JavaScript strings.

use harper_core::linting::{LintGroup, LintKind, Linter, Suggestion};
use harper_core::spell::FstDictionary;
use harper_core::{Dialect, Document};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// At most this many problems from one call.
const MAX: usize = 500;

/// A grammar problem: where it is (UTF-16, as for spelling), Harper's explanation, and its fixes.
pub struct GrammarIssue {
    pub start: usize,
    pub len: usize,
    pub description: String,
    pub corrections: Vec<String>,
}

/// Harper's English for the language ("en_GB" on macOS, "en-GB" on Windows): British outside
/// North America, Australia, New Zealand and India, as Two-edged Sword picks it. macOS's plain
/// "en" is American English.
fn dialect(lang: &str) -> Dialect {
    match lang.replace('_', "-").as_str() {
        "en" | "en-US" => Dialect::American,
        "en-CA" => Dialect::Canadian,
        "en-AU" | "en-NZ" => Dialect::Australian,
        "en-IN" => Dialect::Indian,
        _ => Dialect::British,
    }
}

/// Harper's checks for a dialect: made the first time it's asked for, as that takes a few seconds.
fn harper(d: Dialect) -> &'static Mutex<LintGroup> {
    static H: OnceLock<Mutex<HashMap<String, &'static Mutex<LintGroup>>>> = OnceLock::new();
    let mut all = H.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner());
    all.entry(format!("{d:?}")).or_insert_with(|| Box::leak(Box::new(Mutex::new(LintGroup::new_curated(FstDictionary::curated(), d)))))
}

/// The kinds of Harper's checks that are grammar: not spelling (the system does that), and not
/// its advice on style.
fn is_grammar(k: &LintKind) -> bool {
    matches!(
        k,
        LintKind::Agreement
            | LintKind::BoundaryError
            | LintKind::Capitalization
            | LintKind::Eggcorn
            | LintKind::Grammar
            | LintKind::Malapropism
            | LintKind::Punctuation
            | LintKind::Repetition
            | LintKind::Typo
            | LintKind::Usage
            | LintKind::WordChoice
            | LintKind::WordOrder
    )
}

/// The grammar problems in `lang`: one that is the whole sentence (a fragment, which notes are
/// full of) is left out, and so is one inside another.
pub fn check(text: &str, lang: &str) -> Vec<GrammarIssue> {
    let chars: Vec<char> = text.chars().collect();
    // Each char's UTF-16 offset, and the end.
    let mut at16 = Vec::with_capacity(chars.len() + 1);
    let mut n = 0;
    for c in &chars {
        at16.push(n);
        n += c.len_utf16();
    }
    at16.push(n);
    // Harper doesn't know a lowercase "i" for the pronoun, so it reads it as "I" (the same length,
    // so every offset holds), and the lowercase one is reported here.
    let lone_i = |k: usize| {
        chars[k] == 'i' && (k == 0 || !chars[k - 1].is_alphanumeric()) && chars.get(k + 1).is_none_or(|c| !c.is_alphanumeric() && *c != '.')
    };
    let lower_i: Vec<usize> = (0..chars.len()).filter(|&k| lone_i(k)).collect();
    let fixed: String = chars.iter().enumerate().map(|(k, &c)| if lower_i.contains(&k) { 'I' } else { c }).collect();
    let lints = harper(dialect(lang)).lock().unwrap_or_else(|e| e.into_inner()).lint(&Document::new_plain_english_curated(&fixed));
    let is_end = |c: char| ".!?\n".contains(c);
    // A sentence's text up to its end, without the closing punctuation and space.
    let sentence = |i: usize| {
        let from = chars[..i].iter().rposition(|&c| is_end(c)).map_or(0, |p| p + 1);
        let to = chars[i..].iter().position(|&c| is_end(c)).map_or(chars.len(), |p| i + p);
        let from = from + chars[from..to].iter().take_while(|c| c.is_whitespace()).count();
        let to = to - chars[from..to].iter().rev().take_while(|c| c.is_whitespace()).count();
        (from, to)
    };
    let mut out: Vec<GrammarIssue> = lower_i
        .iter()
        .map(|&k| GrammarIssue {
            start: at16[k],
            len: 1,
            description: "“I” is always a capital letter.".into(),
            corrections: vec!["I".into()],
        })
        .collect();
    for l in lints.into_iter().filter(|l| is_grammar(&l.lint_kind)) {
        let (s, e) = (l.span.start.min(chars.len()), l.span.end.min(chars.len()));
        if s >= e {
            continue;
        }
        let (from, to) = sentence(s);
        if s <= from && e >= to {
            continue;
        }
        let (start, len) = (at16[s], at16[e] - at16[s]);
        if out.iter().any(|o| o.start <= start && start + len <= o.start + o.len) {
            continue;
        }
        let here: String = chars[s..e].iter().collect();
        let corrections = l
            .suggestions
            .iter()
            .map(|g| match g {
                Suggestion::ReplaceWith(r) => r.iter().collect(),
                Suggestion::InsertAfter(x) => format!("{here}{}", x.iter().collect::<String>()),
                Suggestion::Remove => String::new(),
            })
            .collect();
        out.push(GrammarIssue { start, len, description: l.message, corrections });
        if out.len() >= MAX {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    fn flagged(text: &str) -> Vec<String> {
        let u: Vec<u16> = text.encode_utf16().collect();
        super::check(text, "en_US").iter().map(|g| String::from_utf16_lossy(&u[g.start..g.start + g.len])).collect()
    }

    #[test]
    fn finds_a_doubled_word_and_agreement_and_skips_fragments() {
        let found = super::check("This is fine. He went to the the store.", "en_US");
        assert!(
            found.iter().any(|g| g.corrections.iter().any(|c| c == "the")),
            "{:?}",
            found.iter().map(|g| &g.description).collect::<Vec<_>>()
        );
        assert!(!flagged("He go to the shop every morning.").is_empty());
        assert!(flagged("Notes from the call").is_empty());
        assert!(flagged("A full week behind me and a fuller one ahead.").is_empty());
    }

    #[test]
    fn a_lowercase_i_is_capitalised() {
        let found = super::check("i think so", "en_GB");
        assert!(found.iter().any(|g| g.start == 0 && g.corrections == ["I"]));
    }

    #[test]
    fn dialect_reads_both_systems_names() {
        use harper_core::Dialect;
        assert_eq!(super::dialect("en"), Dialect::American);
        assert_eq!(super::dialect("en_GB"), Dialect::British);
        assert_eq!(super::dialect("en-AU"), Dialect::Australian);
        assert_eq!(super::dialect("en_ZA"), Dialect::British);
    }
}
