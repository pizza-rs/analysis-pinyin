//! User-extensible rule overlays.
//!
//! [`Rules`] lets callers customise the pinyin output without rebuilding the
//! crate:
//!
//! - **char overrides** — force one or more readings for a single character
//!   (e.g. for a proper noun: `'重' → ["chong"]`).
//! - **phrase overrides** — replace the per-character readings inside a
//!   matching Chinese substring (e.g. `"银行" → ["yin", "hang"]`).
//!
//! Phrase matching is longest-match; the [`Rules`] struct stores phrases in
//! a hash map keyed by the first character so the per-token scan stays cheap.
//!
//! With the `polyphone-dict` feature enabled, [`Rules::with_builtin_polyphones`]
//! pre-loads ~450 K curated phrase entries from the original Java project's
//! `polyphone.txt`.

use hashbrown::HashMap;

use crate::dict::{PinyinDict, SyllableId};

/// Either a borrowed `&'static str` from the bundled dictionary, or an owned
/// override supplied by the user. Both compare/hash by the underlying text.
#[derive(Debug, Clone)]
pub enum Reading {
    /// Points into the bundled `SYLLABLES` table — zero allocation.
    Static(&'static str),
    /// User-supplied. Allocated string.
    Owned(String),
}

impl Reading {
    #[inline]
    pub fn as_str(&self) -> &str {
        match self {
            Reading::Static(s) => s,
            Reading::Owned(s) => s.as_str(),
        }
    }

    /// First ASCII byte (panics on empty — readings are always non-empty by
    /// construction).
    #[inline]
    pub fn first_letter(&self) -> u8 {
        self.as_str().as_bytes()[0]
    }

    /// Yield this reading as a `Cow` whose borrow lifetime is fully decoupled
    /// from `&self`. `Static` readings yield `Cow::Borrowed(&'static str)`;
    /// `Owned` readings clone (rare path).
    #[inline]
    pub(crate) fn to_cow<'a>(&self) -> std::borrow::Cow<'a, str> {
        match self {
            Reading::Static(s) => std::borrow::Cow::Borrowed(*s),
            Reading::Owned(s) => std::borrow::Cow::Owned(s.clone()),
        }
    }

    fn from_str(s: &str) -> Self {
        // Try to intern through the bundled syllable table for zero-alloc.
        if let Some(id) = PinyinDict::syllable_id(s) {
            return Reading::Static(id.as_str());
        }
        Reading::Owned(s.to_string())
    }
}

impl From<&str> for Reading {
    fn from(s: &str) -> Self {
        Reading::from_str(s)
    }
}

impl From<SyllableId> for Reading {
    fn from(id: SyllableId) -> Self {
        Reading::Static(id.as_str())
    }
}

/// Match record for a phrase override hit.
#[derive(Debug, Clone)]
pub(crate) struct PhraseHit<'r> {
    /// Number of characters this hit covers (≥ 1).
    pub char_len: usize,
    /// Per-character readings the user wants emitted for the hit's range.
    pub readings: &'r [Reading],
}

#[derive(Debug, Clone, Default)]
pub struct Rules {
    char_overrides: HashMap<char, Vec<Reading>>,
    // Group phrase entries by their first character to avoid scanning the
    // whole phrase table per input character.
    phrase_index: HashMap<char, Vec<PhraseEntry>>,
}

#[derive(Debug, Clone)]
struct PhraseEntry {
    /// Phrase text (e.g. "银行"). Stored as `String` so it may be owned (user
    /// input) or borrowed (`'static`); `String` is simplest.
    phrase: String,
    /// Per-character readings, one per char of `phrase`.
    readings: Vec<Reading>,
}

impl Rules {
    pub fn new() -> Self {
        Self::default()
    }

    /// Override the readings for a single character. The first reading
    /// becomes the new primary; subsequent ones are alternates.
    pub fn override_char<S: AsRef<str>>(&mut self, c: char, readings: &[S]) -> &mut Self {
        let v: Vec<Reading> = readings.iter().map(|s| Reading::from(s.as_ref())).collect();
        if !v.is_empty() {
            self.char_overrides.insert(c, v);
        } else {
            self.char_overrides.remove(&c);
        }
        self
    }

    /// Override the readings inside a Chinese substring. The number of
    /// readings must equal the number of characters in `phrase`; otherwise
    /// the entry is silently ignored.
    pub fn override_phrase<S: AsRef<str>>(&mut self, phrase: &str, readings: &[S]) -> &mut Self {
        let chars: Vec<char> = phrase.chars().collect();
        if chars.is_empty() || chars.len() != readings.len() {
            return self;
        }
        let entry = PhraseEntry {
            phrase: phrase.to_string(),
            readings: readings.iter().map(|s| Reading::from(s.as_ref())).collect(),
        };
        let bucket = self.phrase_index.entry(chars[0]).or_default();
        // Replace existing entry for the same phrase (last write wins).
        if let Some(i) = bucket.iter().position(|e| e.phrase == phrase) {
            bucket[i] = entry;
        } else {
            bucket.push(entry);
        }
        // Keep buckets sorted by descending char length so longest-match wins.
        bucket.sort_by(|a, b| b.phrase.chars().count().cmp(&a.phrase.chars().count()));
        self
    }

    /// Builder-style variant of [`Rules::override_char`].
    pub fn with_char<S: AsRef<str>>(mut self, c: char, readings: &[S]) -> Self {
        self.override_char(c, readings);
        self
    }

    /// Builder-style variant of [`Rules::override_phrase`].
    pub fn with_phrase<S: AsRef<str>>(mut self, phrase: &str, readings: &[S]) -> Self {
        self.override_phrase(phrase, readings);
        self
    }

    /// Load the curated phrase dictionary bundled at build time
    /// (`polyphone.txt`). Only available with the `polyphone-dict` feature.
    #[cfg(feature = "polyphone-dict")]
    pub fn with_builtin_polyphones(mut self) -> Self {
        use crate::polyphone_dict::{PHRASE_DATA, PHRASE_KEYS, PHRASE_PTR};
        for (i, phrase) in PHRASE_KEYS.iter().enumerate() {
            let start = PHRASE_PTR[i] as usize;
            let end = PHRASE_PTR[i + 1] as usize;
            let ids = &PHRASE_DATA[start..end];
            let chars: Vec<char> = phrase.chars().collect();
            if chars.len() != ids.len() {
                continue;
            }
            let readings: Vec<Reading> =
                ids.iter().map(|id| Reading::Static(SyllableId(*id).as_str())).collect();
            let entry = PhraseEntry {
                phrase: phrase.to_string(),
                readings,
            };
            let bucket = self.phrase_index.entry(chars[0]).or_default();
            bucket.push(entry);
        }
        for bucket in self.phrase_index.values_mut() {
            bucket.sort_by(|a, b| b.phrase.chars().count().cmp(&a.phrase.chars().count()));
        }
        self
    }

    /// Whether any rules are configured (used by the tokenizer to skip the
    /// phrase-scan fast path).
    #[inline]
    pub(crate) fn is_empty(&self) -> bool {
        self.char_overrides.is_empty() && self.phrase_index.is_empty()
    }

    /// Apply char-level overrides. Returns `None` if no override is set; the
    /// caller falls back to the bundled dict.
    #[inline]
    pub(crate) fn char_readings(&self, c: char) -> Option<&[Reading]> {
        self.char_overrides.get(&c).map(Vec::as_slice)
    }

    /// Try to match the longest phrase override starting at character `start`
    /// of `chars`. Returns the matched length (in chars) and the readings
    /// slice, or `None`.
    pub(crate) fn match_phrase<'r>(
        &'r self,
        chars: &[char],
        start: usize,
    ) -> Option<PhraseHit<'r>> {
        let first = *chars.get(start)?;
        let bucket = self.phrase_index.get(&first)?;
        for entry in bucket {
            let plen = entry.phrase.chars().count();
            if start + plen > chars.len() {
                continue;
            }
            // Compare char-by-char to avoid a String allocation per probe.
            if entry.phrase.chars().zip(&chars[start..start + plen]).all(|(a, b)| a == *b) {
                return Some(PhraseHit {
                    char_len: plen,
                    readings: &entry.readings,
                });
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn char_override_applies() {
        let r = Rules::new().with_char('刘', &["LIU"]);
        let v = r.char_readings('刘').unwrap();
        assert_eq!(v[0].as_str(), "LIU");
    }

    #[test]
    fn phrase_override_matches_longest() {
        let r = Rules::new()
            .with_phrase("银行", &["yin", "hang"])
            .with_phrase("银", &["yin"]);
        let chars: Vec<char> = "银行家".chars().collect();
        let hit = r.match_phrase(&chars, 0).unwrap();
        assert_eq!(hit.char_len, 2);
        assert_eq!(hit.readings[0].as_str(), "yin");
        assert_eq!(hit.readings[1].as_str(), "hang");
    }

    #[test]
    fn no_match_for_other_text() {
        let r = Rules::new().with_phrase("银行", &["yin", "hang"]);
        let chars: Vec<char> = "你好".chars().collect();
        assert!(r.match_phrase(&chars, 0).is_none());
    }
}
