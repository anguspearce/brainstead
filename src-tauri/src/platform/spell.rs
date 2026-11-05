// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Spelling from macOS's own checker (the one TextEdit and Mail use), as the sibling journal app
//! does it: the misspelled words in a text, the guesses for a word, and learning or ignoring one.
//! Grammar is Harper's (grammar.rs), as on Windows. Offsets are UTF-16, as in NSString and
//! JavaScript strings. Called on the main thread (the commands aren't async), where AppKit wants it.

use std::sync::OnceLock;

use objc2_app_kit::NSSpellChecker;
use objc2_foundation::{NSRange, NSString};

/// At most this many misspellings from one call.
const MAX_SPELLING: usize = 1000;

/// The language to check in: `lang` when it's an English macOS has, else macOS's own spelling
/// language when that's English, else plain English. Always an English.
fn language(sc: &NSSpellChecker, lang: Option<&str>) -> objc2::rc::Retained<NSString> {
    let english = |l: &str| l == "en" || l.starts_with("en_");
    if let Some(l) = lang.filter(|l| english(l)) {
        let have = sc.availableLanguages();
        if (0..have.count()).any(|i| have.objectAtIndex(i).to_string() == l) {
            return NSString::from_str(l);
        }
    }
    let system = sc.language();
    if english(&system.to_string()) {
        system
    } else {
        NSString::from_str("en")
    }
}

/// The English variants macOS can check in (en, en_GB, en_ZA…), A–Z.
pub fn english_languages() -> Vec<String> {
    let have = NSSpellChecker::sharedSpellChecker().availableLanguages();
    let mut out: Vec<String> =
        (0..have.count()).map(|i| have.objectAtIndex(i).to_string()).filter(|l| l == "en" || l.starts_with("en_")).collect();
    out.sort();
    out
}

/// macOS's own spelling language.
pub fn system_language() -> String {
    NSSpellChecker::sharedSpellChecker().language().to_string()
}

/// One document tag for the app, so words ignored stay ignored until it quits.
fn tag() -> isize {
    static T: OnceLock<isize> = OnceLock::new();
    *T.get_or_init(NSSpellChecker::uniqueSpellDocumentTag)
}

/// The misspelled words, as (start, length), in macOS's spelling language (named: left to detect
/// the language itself, it accepts words of any other language installed).
pub fn check(text: &str, lang: Option<&str>) -> Vec<(usize, usize)> {
    let sc = NSSpellChecker::sharedSpellChecker();
    let lang = language(&sc, lang);
    let s = NSString::from_str(text);
    let (len, mut at, mut out) = (s.length(), 0usize, Vec::new());
    while at < len && out.len() < MAX_SPELLING {
        let r = unsafe {
            sc.checkSpellingOfString_startingAt_language_wrap_inSpellDocumentWithTag_wordCount(
                &s,
                at as isize,
                Some(&lang),
                false,
                tag(),
                std::ptr::null_mut(),
            )
        };
        if r.length == 0 || r.location >= len {
            break;
        }
        out.push((r.location, r.length));
        at = r.location + r.length;
    }
    out
}

/// The grammar problems: Harper's, in the spelling language (grammar.rs), as macOS's own checker
/// caught little more than a doubled word.
pub fn grammar(text: &str, lang: Option<&str>) -> Vec<super::grammar::GrammarIssue> {
    super::grammar::check(text, &grammar_language(lang))
}

/// The language grammar is checked in: the spelling language.
pub fn grammar_language(lang: Option<&str>) -> String {
    language(&NSSpellChecker::sharedSpellChecker(), lang).to_string()
}

pub fn guesses(word: &str, lang: Option<&str>) -> Vec<String> {
    let sc = NSSpellChecker::sharedSpellChecker();
    let s = NSString::from_str(word);
    let lang = language(&sc, lang);
    let Some(a) = sc.guessesForWordRange_inString_language_inSpellDocumentWithTag(NSRange::new(0, s.length()), &s, Some(&lang), tag())
    else {
        return vec![];
    };
    (0..a.count()).map(|i| a.objectAtIndex(i).to_string()).take(6).collect()
}

/// Adds the word to the user's dictionary (shared with every app).
pub fn learn(word: &str) {
    NSSpellChecker::sharedSpellChecker().learnWord(&NSString::from_str(word));
}

/// Leaves the word alone until the app quits.
pub fn ignore(word: &str) {
    NSSpellChecker::sharedSpellChecker().ignoreWord_inSpellDocumentWithTag(&NSString::from_str(word), tag());
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    /// The tests share macOS's one spell checker, which isn't made for two threads at once.
    static CHECKER: Mutex<()> = Mutex::new(());

    /// `f`'s answer once `ok` takes it, asking again for up to two seconds: a cold spell checker
    /// (a first call in a busy test run) can answer before its dictionaries are in.
    fn settled<T>(f: impl Fn() -> T, ok: impl Fn(&T) -> bool) -> T {
        let mut got = f();
        for _ in 0..20 {
            if ok(&got) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
            got = f();
        }
        got
    }

    #[test]
    fn finds_misspellings_in_utf16_offsets() {
        let _g = CHECKER.lock().unwrap_or_else(|e| e.into_inner());
        // "–" is one UTF-16 unit, so "Recieve" starts at 8.
        let found = settled(|| super::check("Notes – Recieve this.", None), |f| f.contains(&(8, 7)));
        assert!(found.contains(&(8, 7)), "{found:?}");
        assert!(super::check("All words here are fine.", None).is_empty());
        // In the user's language, not "automatic": that lets through words of any other language
        // installed (Dutch "speling").
        assert_eq!(super::check("This sentense has a speling mistake.", Some("en_GB")), vec![(5, 8), (20, 7)]);
    }

    #[test]
    fn grammar_is_harpers_in_the_spelling_language() {
        let _g = CHECKER.lock().unwrap_or_else(|e| e.into_inner());
        let found = super::grammar("He went to the the store.", Some("en_GB"));
        assert!(found.iter().any(|g| g.corrections == ["the"]));
        let en = super::english_languages();
        assert!(en.iter().all(|l| l == "en" || l.starts_with("en_")) && !en.is_empty(), "{en:?}");
    }
}
