// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Spelling and grammar on Windows, as spell.rs gives them on macOS, the way Two-edged Sword's
//! journal has them. Spelling is Windows' own spell checker (the one Edge and Office use): the
//! misspelled words, the guesses for one, and adding or ignoring a word. Windows has no grammar
//! checker for apps, so grammar is Harper's, which runs here on the PC: nothing written leaves it.
//! Offsets are UTF-16, as in JavaScript strings. Languages are named as on macOS ("en_GB"), so
//! the setting reads the same on both.

use harper_core::linting::{LintGroup, LintKind, Linter, Suggestion};
use harper_core::spell::FstDictionary;
use harper_core::{Dialect, Document};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use windows::core::{HSTRING, PWSTR};
use windows::Win32::Globalization::{
    GetUserDefaultLocaleName, ISpellChecker, ISpellCheckerFactory, SpellCheckerFactory, CORRECTIVE_ACTION_DELETE,
};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED};

/// At most this many problems from one call, as on macOS.
const MAX: usize = 500;

/// A grammar problem: where it is (UTF-16, as for spelling), Harper's explanation, and its fixes.
pub struct GrammarIssue {
    pub start: usize,
    pub len: usize,
    pub description: String,
    pub corrections: Vec<String>,
}

thread_local! {
    /// A spell checker per language, made on each thread that asks (COM wants it made where it's
    /// used); None where Windows can't make one.
    static CHECKERS: RefCell<HashMap<String, Option<ISpellChecker>>> = RefCell::new(HashMap::new());
}

fn factory() -> Option<ISpellCheckerFactory> {
    // SAFETY: COM set up for this thread (already set up as the web view's on the main thread,
    // which is as good), then the factory made in it.
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        CoCreateInstance(&SpellCheckerFactory, None, CLSCTX_INPROC_SERVER).ok()
    }
}

/// The user's language as Windows names it ("en-ZA"), or US English.
fn locale() -> String {
    let mut buf = [0u16; 85];
    // SAFETY: the buffer is LOCALE_NAME_MAX_LENGTH long.
    let n = unsafe { GetUserDefaultLocaleName(&mut buf) };
    if n > 1 {
        String::from_utf16_lossy(&buf[..n as usize - 1])
    } else {
        "en-US".into()
    }
}

fn english(l: &str) -> bool {
    l == "en" || l.starts_with("en_") || l.starts_with("en-")
}

/// The English variants Windows can check in, as macOS names them (en_GB, en_US…), A–Z.
pub fn english_languages() -> Vec<String> {
    let Some(f) = factory() else { return vec![] };
    let mut out = Vec::new();
    // SAFETY: an enumeration Windows made, read to its end; each string freed by `take`.
    unsafe {
        let Ok(list) = f.SupportedLanguages() else { return out };
        loop {
            let mut got = [PWSTR::null()];
            let mut n = 0u32;
            if list.Next(&mut got, Some(&mut n)).is_err() || n == 0 {
                break;
            }
            let l = take(got[0]).replace('-', "_");
            if english(&l) {
                out.push(l);
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Windows' own language, as macOS names it ("en_ZA").
pub fn system_language() -> String {
    locale().replace('-', "_")
}

/// The language to check in, as Windows names it: `lang` when it's an English Windows has, else
/// the user's own when that's English and checkable, else US English. Always an English.
fn language(lang: Option<&str>) -> String {
    let have = english_languages();
    let ok = |l: &str| english(l) && have.iter().any(|h| h == &l.replace('-', "_"));
    let system = system_language();
    let l = match lang.filter(|l| ok(l)) {
        Some(l) => l.to_string(),
        None if ok(&system) => system,
        None => "en_US".into(),
    };
    l.replace('_', "-")
}

fn with<T>(lang: Option<&str>, f: impl FnOnce(&ISpellChecker) -> T) -> Option<T> {
    let lang = language(lang);
    CHECKERS.with(|c| {
        let mut c = c.borrow_mut();
        let sc = c
            .entry(lang.clone())
            // SAFETY: the factory made on this thread, asked for a language it says it has.
            .or_insert_with(|| factory().and_then(|f| unsafe { f.CreateSpellChecker(&HSTRING::from(lang.as_str())) }.ok()));
        sc.as_ref().map(f)
    })
}

/// Windows' errors in `text`: (start, length).
fn errors(sc: &ISpellChecker, text: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    // SAFETY: COM calls on a checker made on this thread; the enumeration is read to its end.
    unsafe {
        let Ok(errs) = sc.Check(&HSTRING::from(text)) else { return out };
        loop {
            let mut next = None;
            // S_FALSE, with nothing, after the last.
            if errs.Next(&mut next) != windows::Win32::Foundation::S_OK {
                break;
            }
            let Some(e) = next else { break };
            let (start, len) = (e.StartIndex().unwrap_or(0) as usize, e.Length().unwrap_or(0) as usize);
            if len == 0 || out.len() >= 1000 {
                break;
            }
            // A word said twice ("the the") is grammar's to report, not spelling's, as on macOS.
            let action = e.CorrectiveAction().ok();
            if action == Some(CORRECTIVE_ACTION_DELETE) {
                continue;
            }
            out.push((start, len));
        }
    }
    out
}

/// A string Windows allocated, read and freed.
unsafe fn take(p: PWSTR) -> String {
    // SAFETY: (the caller's) a string Windows allocated with CoTaskMemAlloc.
    unsafe {
        let s = p.to_string().unwrap_or_default();
        CoTaskMemFree(Some(p.0 as *const _));
        s
    }
}

/// The misspelled words, as (start, length).
pub fn check(text: &str, lang: Option<&str>) -> Vec<(usize, usize)> {
    with(lang, |sc| errors(sc, text)).unwrap_or_default()
}

/// Harper's English for the language: British outside North America, Australia, New Zealand and
/// India, as Two-edged Sword picks it.
fn dialect(lang: &str) -> Dialect {
    match lang {
        "en-US" => Dialect::American,
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

/// The kinds of Harper's checks that are grammar, as macOS's are: not spelling (Windows does
/// that), and not its advice on style.
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

/// The grammar problems, as macOS gives them: one that is the whole sentence (a fragment, which
/// notes are full of) is left out, and so is one inside another.
pub fn grammar(text: &str, lang: Option<&str>) -> Vec<GrammarIssue> {
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
    let lints =
        harper(dialect(&language(lang))).lock().unwrap_or_else(|e| e.into_inner()).lint(&Document::new_plain_english_curated(&fixed));
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

/// Windows' guesses for a misspelled word, best first.
pub fn guesses(word: &str, lang: Option<&str>) -> Vec<String> {
    with(lang, |sc| {
        let mut out = Vec::new();
        // SAFETY: COM calls on a checker made on this thread; each string freed by `take`.
        unsafe {
            let Ok(list) = sc.Suggest(&HSTRING::from(word)) else { return out };
            loop {
                let mut got = [PWSTR::null()];
                let mut n = 0u32;
                if list.Next(&mut got, Some(&mut n)).is_err() || n == 0 {
                    break;
                }
                out.push(take(got[0]));
            }
        }
        out
    })
    .unwrap_or_default()
}

/// Adds the word to the user's dictionary (shared with every app).
pub fn learn(word: &str) {
    // SAFETY: a COM call on a checker made on this thread.
    with(None, |sc| unsafe { sc.Add(&HSTRING::from(word)) });
}

/// Leaves the word alone until the app quits.
pub fn ignore(word: &str) {
    // SAFETY: a COM call on a checker made on this thread.
    with(None, |sc| unsafe { sc.Ignore(&HSTRING::from(word)) });
}

#[cfg(test)]
mod tests {
    #[test]
    fn finds_a_misspelling_in_utf16_offsets_with_guesses() {
        let found = super::check("This sentance is fine otherwise.", Some("en_US"));
        assert_eq!(found, vec![(5, 8)], "{found:?}");
        assert!(super::guesses("sentance", Some("en_US")).iter().any(|g| g == "sentence"));
        assert!(super::english_languages().iter().all(|l| l.starts_with("en")));
    }

    #[test]
    fn grammar_finds_a_doubled_word_and_skips_fragments() {
        let found = super::grammar("He went to the the store.", Some("en_US"));
        assert!(
            found.iter().any(|g| g.corrections.iter().any(|c| c == "the")),
            "{:?}",
            found.iter().map(|g| &g.description).collect::<Vec<_>>()
        );
        assert!(super::check("He went to the the store.", Some("en_US")).is_empty());
        assert!(super::grammar("Notes from the call", Some("en_US")).is_empty());
    }

    #[test]
    fn a_lowercase_i_is_capitalised() {
        let found = super::grammar("i think so", Some("en_GB"));
        assert!(found.iter().any(|g| g.start == 0 && g.corrections == ["I"]));
    }
}
