//! Compact, statically-baked Chinese-character → pinyin tables.
//!
//! All data is produced by [`build.rs`] at compile time and included via
//! `include!`. Lookups are O(1) array indexing or O(log N) binary search and
//! perform **no heap allocation**.
//!
//! Memory footprint of the default build (no `polyphone-dict` feature):
//! - `SYLLABLES`        : ~410 `&'static str` (~10 KB of pointers + ~2 KB of strings)
//! - `PRIMARY`          : `[u16; 20_736]` = 41 KB
//! - polyphone aux      : a few KB
//! - homophone reverse  : ~100 KB
//!
//! With `polyphone-dict` the embedded phrase dictionary adds roughly 8–11 MB
//! of static data.

include!(concat!(env!("OUT_DIR"), "/generated.rs"));

/// A unique id into [`SYLLABLES`]. Returned by [`syllable_id`] etc.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct SyllableId(pub u16);

impl SyllableId {
    /// Look up the plain (no-tone, lowercase) syllable text.
    #[inline]
    pub fn as_str(self) -> &'static str {
        SYLLABLES[self.0 as usize]
    }

    /// First letter of the syllable as an ASCII byte (e.g. `'l'` for `"liu"`).
    #[inline]
    pub fn first_letter(self) -> u8 {
        // Every entry in SYLLABLES is non-empty ASCII.
        self.as_str().as_bytes()[0]
    }
}

/// Read-only accessor namespace for the bundled pinyin tables.
pub struct PinyinDict;

impl PinyinDict {
    /// Primary syllable id for a character. Returns `None` for characters
    /// outside the bundled range or with no known reading.
    #[inline]
    pub fn primary_id(c: char) -> Option<SyllableId> {
        let cp = c as u32;
        if cp < CJK_START || cp >= CJK_END_EXCL {
            return None;
        }
        let id = PRIMARY[(cp - CJK_START) as usize];
        (id != NO_SYLLABLE).then_some(SyllableId(id))
    }

    /// Plain (no-tone, lowercase) primary pinyin of `c`, e.g. `'刘'` → `"liu"`.
    #[inline]
    pub fn primary(c: char) -> Option<&'static str> {
        Self::primary_id(c).map(SyllableId::as_str)
    }

    /// Iterator over every known reading of `c` (primary first, then
    /// polyphone alternates in source order).
    ///
    /// Allocation-free; returned `&'static str`s point into the baked
    /// `SYLLABLES` table.
    pub fn readings(c: char) -> impl Iterator<Item = &'static str> + Clone {
        ReadingIter::new(c).map(SyllableId::as_str)
    }

    /// Same as [`PinyinDict::readings`] but returns syllable ids (cheaper to
    /// compare / store than `&'static str`).
    pub fn reading_ids(c: char) -> impl Iterator<Item = SyllableId> + Clone {
        ReadingIter::new(c)
    }

    /// Whether `c` has more than one known reading (a 多音字).
    #[inline]
    pub fn is_polyphone(c: char) -> bool {
        match Self::primary_id(c) {
            Some(_) => poly_slice(c).is_some(),
            None => false,
        }
    }

    /// Reverse lookup: every character that reads as the given syllable
    /// (homophones, 同音字). Empty slice for unknown syllables.
    pub fn homophones(syllable: &str) -> &'static [char] {
        match syllable_id_by_name(syllable) {
            Some(id) => homophone_chars(id),
            None => &[],
        }
    }

    /// Resolve a syllable text to a [`SyllableId`].
    #[inline]
    pub fn syllable_id(name: &str) -> Option<SyllableId> {
        syllable_id_by_name(name).map(SyllableId)
    }

    /// All known plain syllables, sorted alphabetically.
    #[inline]
    pub fn all_syllables() -> &'static [&'static str] {
        SYLLABLES
    }

    /// Is `text` a valid pinyin alphabet syllable (used by the alphabet
    /// re-segmenter)? O(log N) binary search.
    #[inline]
    pub fn is_alphabet_syllable(text: &str) -> bool {
        ALPHABET_NAMES.binary_search(&text).is_ok()
    }
}

// --- Internal helpers ----------------------------------------------------

#[inline]
fn syllable_id_by_name(name: &str) -> Option<u16> {
    // SYLLABLES is sorted, so binary search works.
    SYLLABLES.binary_search(&name).ok().map(|i| i as u16)
}

/// Borrow the extra-reading slice (id list) for `c` if any.
fn poly_slice(c: char) -> Option<&'static [u16]> {
    let cp = c as u32;
    if cp < CJK_START || cp >= CJK_END_EXCL {
        return None;
    }
    let off = (cp - CJK_START) as u16;
    let i = POLY_OFFSETS.binary_search(&off).ok()?;
    let start = POLY_DATA_PTR[i] as usize;
    let end = POLY_DATA_PTR[i + 1] as usize;
    Some(&POLY_DATA[start..end])
}

fn homophone_chars(id: u16) -> &'static [char] {
    let start = HOMO_PTR[id as usize] as usize;
    let end = HOMO_PTR[id as usize + 1] as usize;
    // Reinterpret &[u32] as &[char]: char is `repr(transparent)` over a
    // 32-bit Unicode scalar value, but the only sound way to cast in stable
    // Rust is to leak a const-built slice. We instead transmute the slice
    // pointer, relying on the build-script invariant that every entry is a
    // valid scalar value (it came from `as u32` of a real char).
    let data = &HOMO_DATA[start..end];
    // Safety: every u32 in HOMO_DATA was produced by `c as u32` for a real
    // `char`, and `char` and `u32` share size + alignment.
    unsafe { core::slice::from_raw_parts(data.as_ptr() as *const char, data.len()) }
}

/// Iterator over (primary + polyphone) syllable ids for a character.
#[derive(Clone)]
struct ReadingIter {
    primary: Option<SyllableId>,
    extras: core::slice::Iter<'static, u16>,
}

impl ReadingIter {
    fn new(c: char) -> Self {
        Self {
            primary: PinyinDict::primary_id(c),
            extras: poly_slice(c).unwrap_or(&[]).iter(),
        }
    }
}

impl Iterator for ReadingIter {
    type Item = SyllableId;
    fn next(&mut self) -> Option<Self::Item> {
        if let Some(p) = self.primary.take() {
            return Some(p);
        }
        self.extras.next().copied().map(SyllableId)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primary_is_correct() {
        assert_eq!(PinyinDict::primary('刘'), Some("liu"));
        assert_eq!(PinyinDict::primary('德'), Some("de"));
        assert_eq!(PinyinDict::primary('华'), Some("hua"));
        assert_eq!(PinyinDict::primary('A'), None);
    }

    #[test]
    fn polyphone_readings() {
        let rs: Vec<&str> = PinyinDict::readings('中').collect();
        // 中 is a classic polyphone (zhōng / zhòng).
        assert!(rs.contains(&"zhong"));
        assert!(rs.len() >= 1);
        let rs2: Vec<&str> = PinyinDict::readings('行').collect();
        // 行 — hang/xing/heng
        assert!(rs2.contains(&"xing") || rs2.contains(&"hang"));
        assert!(rs2.len() >= 2);
    }

    #[test]
    fn homophone_lookup() {
        let liu = PinyinDict::homophones("liu");
        assert!(liu.contains(&'刘'));
        assert!(PinyinDict::homophones("doesnotexist").is_empty());
    }

    #[test]
    fn alphabet_membership() {
        assert!(PinyinDict::is_alphabet_syllable("liu"));
        assert!(PinyinDict::is_alphabet_syllable("zhuang"));
        assert!(!PinyinDict::is_alphabet_syllable("xyz"));
    }
}
