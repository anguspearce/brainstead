// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! A secret kept across several small slots, for a store that caps the size of one. Windows'
//! Credential Manager takes 2,560 bytes in a credential, and a Teams sign-in (up to four tokens) is
//! several times that. The value's bytes go into numbered slots and the base slot holds only how
//! many there are; a value that fits is kept whole in the base slot, as it always was, so one
//! written before this still reads. Nothing here is Windows' own: the store is behind [`Slots`].

/// The most bytes put in one slot, under Credential Manager's 2,560.
pub const SLOT_BYTES: usize = 2048;

/// What the base slot holds when the value is in numbered slots: this, then how many.
const HEADER: &str = "chunks:";

/// A store of small secrets: the base slot (`None`) and the numbered ones after it.
pub trait Slots {
    fn read(&self, slot: Option<usize>) -> Option<Vec<u8>>;
    fn write(&mut self, slot: Option<usize>, bytes: &[u8]) -> Result<(), String>;
    /// Removes a slot; whether there was one.
    fn remove(&mut self, slot: Option<usize>) -> Result<bool, String>;
}

/// How many numbered slots the base slot says the value is in, or None when it holds the value itself.
fn count(base: &[u8]) -> Option<usize> {
    std::str::from_utf8(base).ok()?.strip_prefix(HEADER)?.parse().ok()
}

/// The value: the base slot's own, or its numbered slots joined. None when there is none, or a slot
/// is missing (a write was cut short).
pub fn get(store: &impl Slots) -> Option<String> {
    let base = store.read(None)?;
    let Some(n) = count(&base) else { return String::from_utf8(base).ok() };
    let mut value = Vec::new();
    for i in 0..n {
        value.extend(store.read(Some(i))?);
    }
    String::from_utf8(value).ok()
}

/// Keeps `value`, whole in the base slot when it fits and in numbered slots when it doesn't. The
/// base slot is written last, so it never counts slots that aren't there yet; slots left over from
/// a longer value go afterwards.
pub fn set(store: &mut impl Slots, value: &str) -> Result<(), String> {
    let bytes = value.as_bytes();
    // A value that reads as a count would be taken for one, so it goes in slots however short.
    let n = if bytes.len() <= SLOT_BYTES && count(bytes).is_none() {
        store.write(None, bytes)?;
        0
    } else {
        let chunks: Vec<&[u8]> = bytes.chunks(SLOT_BYTES).collect();
        for (i, chunk) in chunks.iter().enumerate() {
            store.write(Some(i), chunk)?;
        }
        store.write(None, format!("{HEADER}{}", chunks.len()).as_bytes())?;
        chunks.len()
    };
    // The value is kept by now: a left-over slot that won't go is only clutter.
    let _ = remove_from(store, n);
    Ok(())
}

/// Removes the value: the base slot first, so nothing reads half of it, then every numbered slot.
pub fn delete(store: &mut impl Slots) -> Result<(), String> {
    store.remove(None)?;
    remove_from(store, 0)
}

/// Removes the numbered slots from `first` up to the first that isn't there.
fn remove_from(store: &mut impl Slots, first: usize) -> Result<(), String> {
    let mut i = first;
    while store.remove(Some(i))? {
        i += 1;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    /// A store that refuses anything over the slot size, as Credential Manager does.
    #[derive(Default)]
    struct Fake(BTreeMap<Option<usize>, Vec<u8>>);

    impl Slots for Fake {
        fn read(&self, slot: Option<usize>) -> Option<Vec<u8>> {
            self.0.get(&slot).cloned()
        }
        fn write(&mut self, slot: Option<usize>, bytes: &[u8]) -> Result<(), String> {
            if bytes.len() > SLOT_BYTES {
                return Err("too big for one slot".into());
            }
            self.0.insert(slot, bytes.to_vec());
            Ok(())
        }
        fn remove(&mut self, slot: Option<usize>) -> Result<bool, String> {
            Ok(self.0.remove(&slot).is_some())
        }
    }

    /// A sign-in the size of a Teams one: a few long tokens in JSON.
    fn long(n: usize) -> String {
        format!(r#"{{"tokens":"{}","tenant":"Acme é"}}"#, "eyJ0eXAi".repeat(n))
    }

    #[test]
    fn a_value_too_big_for_one_slot_is_split_and_read_back() {
        let mut s = Fake::default();
        let value = long(1200);
        assert!(value.len() > 4 * SLOT_BYTES);
        set(&mut s, &value).unwrap();
        assert_eq!(s.0[&None], b"chunks:5");
        assert_eq!(s.0.len(), 6);
        assert_eq!(get(&s).as_deref(), Some(value.as_str()));

        // A shorter one after it leaves no slots behind, and a short one is kept whole.
        let shorter = long(600);
        set(&mut s, &shorter).unwrap();
        assert_eq!((s.0[&None].as_slice(), s.0.len()), (b"chunks:3".as_slice(), 4));
        assert_eq!(get(&s).as_deref(), Some(shorter.as_str()));
        set(&mut s, r#"{"tokens":"short"}"#).unwrap();
        assert_eq!(s.0.len(), 1);
        assert_eq!(get(&s).as_deref(), Some(r#"{"tokens":"short"}"#));

        set(&mut s, &value).unwrap();
        delete(&mut s).unwrap();
        assert!(s.0.is_empty());
        assert_eq!(get(&s), None);
        delete(&mut s).unwrap();
    }

    #[test]
    fn a_value_written_whole_before_still_reads() {
        let mut s = Fake::default();
        s.0.insert(None, br#"{"tokens":"from an earlier version"}"#.to_vec());
        assert_eq!(get(&s).as_deref(), Some(r#"{"tokens":"from an earlier version"}"#));
    }

    #[test]
    fn a_missing_slot_reads_as_no_value_and_a_count_like_value_survives() {
        let mut s = Fake::default();
        set(&mut s, &long(1200)).unwrap();
        s.0.remove(&Some(2));
        assert_eq!(get(&s), None);

        set(&mut s, "chunks:7").unwrap();
        assert_eq!(get(&s).as_deref(), Some("chunks:7"));
        assert_eq!(s.0[&None], b"chunks:1");
    }
}
