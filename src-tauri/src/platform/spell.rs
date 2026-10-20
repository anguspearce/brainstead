// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Spelling and grammar from macOS's own checker (the one TextEdit and Mail use), as the sibling
//! journal app does it: the misspelled words in a text, its grammar problems, the guesses for a
//! word, and learning or ignoring one. Offsets are UTF-16, as in NSString and JavaScript strings.
//! Called on the main thread (the commands aren't async), where AppKit wants it.

use std::sync::OnceLock;

use objc2::runtime::AnyObject;
use objc2_app_kit::NSSpellChecker;
use objc2_foundation::{NSArray, NSDictionary, NSRange, NSString, NSValue};

/// At most this many problems from one call.
const MAX_SPELLING: usize = 1000;
const MAX_GRAMMAR: usize = 500;

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

/// A grammar problem: where it is (UTF-16, as for spelling), macOS's explanation, and its fixes.
pub struct GrammarIssue {
    pub start: usize,
    pub len: usize,
    pub description: String,
    pub corrections: Vec<String>,
}

/// The grammar problems, a sentence at a time. macOS needs the language named to check grammar
/// at all; a problem inside another is left to the outer one, and one that is the whole sentence
/// (a fragment, which notes are full of) is left out.
pub fn grammar(text: &str, lang: Option<&str>) -> Vec<GrammarIssue> {
    let sc = NSSpellChecker::sharedSpellChecker();
    let s = NSString::from_str(text);
    let lang = language(&sc, lang);
    let key = |k: &str, d: &NSDictionary<NSString, AnyObject>| d.objectForKey(&NSString::from_str(k));
    let (len, mut at, mut out) = (s.length(), 0usize, Vec::<GrammarIssue>::new());
    while at < len && out.len() < MAX_GRAMMAR {
        let mut details: Option<objc2::rc::Retained<NSArray<NSDictionary<NSString, AnyObject>>>> = None;
        let r = unsafe {
            sc.checkGrammarOfString_startingAt_language_wrap_inSpellDocumentWithTag_details(
                &s,
                at as isize,
                Some(&lang),
                false,
                tag(),
                Some(&mut details),
            )
        };
        if r.length == 0 || r.location >= len {
            break;
        }
        let first = out.len();
        for d in details.iter().flat_map(|ds| (0..ds.count()).map(move |i| ds.objectAtIndex(i))) {
            // The range is from the start of the sentence.
            let Some(g) = key("NSGrammarRange", &d).and_then(|v| v.downcast::<NSValue>().ok()).and_then(|v| v.get_range()) else {
                continue;
            };
            let (start, glen) = (r.location + g.location, g.length);
            let whole = s.substringWithRange(r).to_string();
            let body = whole.trim_end_matches(|c: char| c.is_whitespace() || ".!?".contains(c)).encode_utf16().count();
            if g.location == 0 && glen >= body {
                continue;
            }
            if glen == 0 || out[first..].iter().any(|o| o.start <= start && start + glen <= o.start + o.len) {
                continue;
            }
            let description =
                key("NSGrammarUserDescription", &d).and_then(|v| v.downcast::<NSString>().ok()).map(|v| v.to_string()).unwrap_or_default();
            let corrections = key("NSGrammarCorrections", &d)
                .and_then(|v| v.downcast::<NSArray>().ok())
                .map(|a| (0..a.count()).filter_map(|i| a.objectAtIndex(i).downcast::<NSString>().ok().map(|x| x.to_string())).collect())
                .unwrap_or_default();
            out.push(GrammarIssue { start, len: glen, description, corrections });
        }
        at = r.location + r.length;
    }
    out
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
    fn grammar_finds_a_doubled_word_and_skips_fragments() {
        let _g = CHECKER.lock().unwrap_or_else(|e| e.into_inner());
        let text = "This is fine. He went to the the store.";
        let u: Vec<u16> = text.encode_utf16().collect();
        let doubled = |g: &super::GrammarIssue| String::from_utf16_lossy(&u[g.start..g.start + g.len]) == "the the";
        let found = settled(|| super::grammar(text, None), |f| f.iter().any(doubled));
        let g = found.iter().find(|g| doubled(g)).expect("the doubled word");
        assert_eq!(g.corrections, vec!["the".to_string()]);
        assert!(super::grammar("A full week behind me and a fuller one ahead.", None).is_empty());
        let en = super::english_languages();
        assert!(en.iter().all(|l| l == "en" || l.starts_with("en_")) && !en.is_empty(), "{en:?}");
    }
}
