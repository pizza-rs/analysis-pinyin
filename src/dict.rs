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

// ─── Dictionary table source ────────────────────────────────────────────────
//
// `embed` (default): the optimized tables are baked at build time by `build.rs`
// and accessed directly. The `tab::*` accessors are `#[inline(always)]` wrappers
// that compile to a plain static reference, so the per-character hot path is
// identical to indexing the generated statics — the fastest path.
//
// Without `embed`: the same tables are built **once** on first use from the
// external `config/analysis/pinyin/{pinyin.txt,pinyin_alphabet.dict}` (when a
// dictionary directory is configured) or the embedded raw text, leaked to
// `'static`, and cached — so lookups keep the same O(1)/O(log N) cost; only
// table construction moves from compile time to a one-off startup parse.

#[cfg(feature = "embed")]
include!(concat!(env!("OUT_DIR"), "/generated.rs"));

#[cfg(all(not(feature = "embed"), not(feature = "std")))]
compile_error!(
    "pizza-analysis-pinyin requires either the `embed` feature (compile-time \
     dictionaries) or the `std` feature (runtime dictionary loading)."
);

#[cfg(all(feature = "polyphone-dict", not(feature = "embed")))]
compile_error!("the `polyphone-dict` feature requires the `embed` feature.");

#[cfg(feature = "embed")]
mod tab {
    use super::*;

    #[inline(always)]
    pub(super) fn syllables() -> &'static [&'static str] {
        SYLLABLES
    }
    #[inline(always)]
    pub(super) fn primary() -> &'static [u16] {
        PRIMARY
    }
    #[inline(always)]
    pub(super) fn poly_offsets() -> &'static [u16] {
        POLY_OFFSETS
    }
    #[inline(always)]
    pub(super) fn poly_data_ptr() -> &'static [u32] {
        POLY_DATA_PTR
    }
    #[inline(always)]
    pub(super) fn poly_data() -> &'static [u16] {
        POLY_DATA
    }
    #[inline(always)]
    pub(super) fn homo_ptr() -> &'static [u32] {
        HOMO_PTR
    }
    #[inline(always)]
    pub(super) fn homo_data() -> &'static [u32] {
        HOMO_DATA
    }
    #[inline(always)]
    pub(super) fn alphabet_names() -> &'static [&'static str] {
        ALPHABET_NAMES
    }
}

#[cfg(not(feature = "embed"))]
const CJK_START: u32 = 0x4E00;
#[cfg(not(feature = "embed"))]
const CJK_END_EXCL: u32 = 0xA000;
#[cfg(not(feature = "embed"))]
const CJK_LEN: usize = (CJK_END_EXCL - CJK_START) as usize;
#[cfg(not(feature = "embed"))]
const NO_SYLLABLE: u16 = u16::MAX;

#[cfg(not(feature = "embed"))]
mod tab {
    use alloc::borrow::Cow;
    use alloc::boxed::Box;
    use alloc::collections::BTreeMap;
    use alloc::collections::BTreeSet;
    use alloc::string::String;
    use alloc::vec;
    use alloc::vec::Vec;
    use std::sync::OnceLock;

    use super::CJK_END_EXCL;
    use super::CJK_LEN;
    use super::CJK_START;
    use super::NO_SYLLABLE;

    /// Runtime mirror of the build-script tables, all leaked to `'static`.
    pub(super) struct Tables {
        pub syllables: &'static [&'static str],
        pub primary: &'static [u16],
        pub poly_offsets: &'static [u16],
        pub poly_data_ptr: &'static [u32],
        pub poly_data: &'static [u16],
        pub homo_ptr: &'static [u32],
        pub homo_data: &'static [u32],
        pub alphabet_names: &'static [&'static str],
    }

    fn cell() -> &'static Tables {
        static C: OnceLock<Tables> = OnceLock::new();
        C.get_or_init(build)
    }

    #[inline]
    pub(super) fn syllables() -> &'static [&'static str] {
        cell().syllables
    }
    #[inline]
    pub(super) fn primary() -> &'static [u16] {
        cell().primary
    }
    #[inline]
    pub(super) fn poly_offsets() -> &'static [u16] {
        cell().poly_offsets
    }
    #[inline]
    pub(super) fn poly_data_ptr() -> &'static [u32] {
        cell().poly_data_ptr
    }
    #[inline]
    pub(super) fn poly_data() -> &'static [u16] {
        cell().poly_data
    }
    #[inline]
    pub(super) fn homo_ptr() -> &'static [u32] {
        cell().homo_ptr
    }
    #[inline]
    pub(super) fn homo_data() -> &'static [u32] {
        cell().homo_data
    }
    #[inline]
    pub(super) fn alphabet_names() -> &'static [&'static str] {
        cell().alphabet_names
    }

    #[cfg(feature = "embed-fallback")]
    const EMBEDDED_PINYIN: Option<&str> = Some(include_str!("../data/pinyin.txt"));
    #[cfg(not(feature = "embed-fallback"))]
    const EMBEDDED_PINYIN: Option<&str> = None;
    #[cfg(feature = "embed-fallback")]
    const EMBEDDED_ALPHABET: Option<&str> = Some(include_str!("../data/pinyin_alphabet.dict"));
    #[cfg(not(feature = "embed-fallback"))]
    const EMBEDDED_ALPHABET: Option<&str> = None;

    fn load(file: &str, embedded: Option<&'static str>) -> Cow<'static, str> {
        #[cfg(feature = "std")]
        {
            // The shipped pizza build compiles no embedded copy in: the
            // external dictionary under `<dict_dir>/pinyin/` is the only
            // source, so a missing file must fail loudly instead of quietly
            // producing empty tables (and unmapped characters).
            pizza_engine::analysis::dict::load_str("pinyin", file, embedded).unwrap_or_else(|e| {
                panic!(
                    "pinyin dictionary '{file}' is not available: {e}; stage it under \
                     config/analysis/pinyin/ ('make copy-analysis-dicts') or build \
                     pizza-analysis-pinyin with the 'embed-fallback' feature"
                )
            })
        }
        #[cfg(not(feature = "std"))]
        {
            let _ = file;
            Cow::Borrowed(embedded.unwrap_or(""))
        }
    }

    fn strip_tone(s: &str) -> &str {
        let bytes = s.as_bytes();
        if let Some(&last) = bytes.last() {
            if last.is_ascii_digit() {
                return &s[..s.len() - 1];
            }
        }
        s
    }

    /// Build the lookup tables, mirroring `build.rs`.
    fn build() -> Tables {
        let pinyin_raw = load("pinyin.txt", EMBEDDED_PINYIN);
        let alphabet_raw = load("pinyin_alphabet.dict", EMBEDDED_ALPHABET);

        // Collect unique plain syllables and per-char readings (source order).
        let mut syllables_set: BTreeSet<String> = BTreeSet::new();
        let mut char_readings: BTreeMap<char, Vec<String>> = BTreeMap::new();
        for line in pinyin_raw.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            let mut chars = k.chars();
            let (Some(ch), None) = (chars.next(), chars.next()) else {
                continue;
            };
            let cp = ch as u32;
            if cp < CJK_START || cp >= CJK_END_EXCL {
                continue;
            }
            let mut seen: BTreeSet<String> = BTreeSet::new();
            let mut readings: Vec<String> = Vec::new();
            for raw in v.split(',') {
                let raw = raw.trim();
                if raw.is_empty() {
                    continue;
                }
                let plain = strip_tone(raw).to_ascii_lowercase();
                if plain.is_empty() {
                    continue;
                }
                if seen.insert(plain.clone()) {
                    syllables_set.insert(plain.clone());
                    readings.push(plain);
                }
            }
            if !readings.is_empty() {
                char_readings.insert(ch, readings);
            }
        }
        for line in alphabet_raw.lines() {
            let s = line.trim();
            if !s.is_empty() {
                syllables_set.insert(s.to_ascii_lowercase());
            }
        }

        let syllables: Vec<String> = syllables_set.into_iter().collect();
        let syllable_id: BTreeMap<&str, u16> = syllables
            .iter()
            .enumerate()
            .map(|(i, s)| (s.as_str(), i as u16))
            .collect();

        // Per-char primary syllable + polyphone extras.
        let mut primary = vec![NO_SYLLABLE; CJK_LEN];
        let mut poly: BTreeMap<u16, Vec<u16>> = BTreeMap::new();
        for (ch, readings) in &char_readings {
            let idx = (*ch as u32 - CJK_START) as usize;
            let ids: Vec<u16> = readings
                .iter()
                .map(|r| *syllable_id.get(r.as_str()).expect("known syllable"))
                .collect();
            primary[idx] = ids[0];
            if ids.len() > 1 {
                poly.insert(idx as u16, ids[1..].to_vec());
            }
        }

        // Flatten polyphone CSR arrays (offsets sorted by BTreeMap).
        let mut poly_offsets: Vec<u16> = Vec::with_capacity(poly.len());
        let mut poly_data: Vec<u16> = Vec::new();
        let mut poly_data_ptr: Vec<u32> = Vec::with_capacity(poly.len() + 1);
        poly_data_ptr.push(0);
        for (off, ids) in &poly {
            poly_offsets.push(*off);
            poly_data.extend_from_slice(ids);
            poly_data_ptr.push(poly_data.len() as u32);
        }

        // Homophone reverse index (CSR of char codepoints).
        let mut homo: Vec<Vec<u32>> = vec![Vec::new(); syllables.len()];
        for (ch, readings) in &char_readings {
            for r in readings {
                let id = *syllable_id.get(r.as_str()).expect("known syllable");
                homo[id as usize].push(*ch as u32);
            }
        }
        let mut homo_data: Vec<u32> = Vec::new();
        let mut homo_ptr: Vec<u32> = Vec::with_capacity(homo.len() + 1);
        homo_ptr.push(0);
        for v in &mut homo {
            v.sort_unstable();
            v.dedup();
            homo_data.extend_from_slice(v);
            homo_ptr.push(homo_data.len() as u32);
        }

        // Alphabet syllable names, sorted for binary search.
        let mut alphabet_names_owned: Vec<String> = alphabet_raw
            .lines()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_ascii_lowercase())
            .collect();
        alphabet_names_owned.sort_unstable();
        alphabet_names_owned.dedup();

        // Leak everything to `'static` (one-time, lives for the process).
        let syllables_static: Vec<&'static str> = syllables
            .into_iter()
            .map(|s| &*Box::leak(s.into_boxed_str()))
            .collect();
        let alphabet_static: Vec<&'static str> = alphabet_names_owned
            .into_iter()
            .map(|s| &*Box::leak(s.into_boxed_str()))
            .collect();

        Tables {
            syllables: Box::leak(syllables_static.into_boxed_slice()),
            primary: Box::leak(primary.into_boxed_slice()),
            poly_offsets: Box::leak(poly_offsets.into_boxed_slice()),
            poly_data_ptr: Box::leak(poly_data_ptr.into_boxed_slice()),
            poly_data: Box::leak(poly_data.into_boxed_slice()),
            homo_ptr: Box::leak(homo_ptr.into_boxed_slice()),
            homo_data: Box::leak(homo_data.into_boxed_slice()),
            alphabet_names: Box::leak(alphabet_static.into_boxed_slice()),
        }
    }
}

/// A unique id into [`SYLLABLES`]. Returned by [`syllable_id`] etc.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct SyllableId(pub u16);

impl SyllableId {
    /// Look up the plain (no-tone, lowercase) syllable text.
    #[inline]
    pub fn as_str(self) -> &'static str {
        tab::syllables()[self.0 as usize]
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
        let id = tab::primary()[(cp - CJK_START) as usize];
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
        tab::syllables()
    }

    /// Is `text` a valid pinyin alphabet syllable (used by the alphabet
    /// re-segmenter)? O(log N) binary search.
    #[inline]
    pub fn is_alphabet_syllable(text: &str) -> bool {
        tab::alphabet_names().binary_search(&text).is_ok()
    }
}

// --- Internal helpers ----------------------------------------------------

#[inline]
fn syllable_id_by_name(name: &str) -> Option<u16> {
    // The syllable table is sorted, so binary search works.
    tab::syllables().binary_search(&name).ok().map(|i| i as u16)
}

/// Borrow the extra-reading slice (id list) for `c` if any.
fn poly_slice(c: char) -> Option<&'static [u16]> {
    let cp = c as u32;
    if cp < CJK_START || cp >= CJK_END_EXCL {
        return None;
    }
    let off = (cp - CJK_START) as u16;
    let i = tab::poly_offsets().binary_search(&off).ok()?;
    let start = tab::poly_data_ptr()[i] as usize;
    let end = tab::poly_data_ptr()[i + 1] as usize;
    Some(&tab::poly_data()[start..end])
}

fn homophone_chars(id: u16) -> &'static [char] {
    let start = tab::homo_ptr()[id as usize] as usize;
    let end = tab::homo_ptr()[id as usize + 1] as usize;
    // Reinterpret &[u32] as &[char]: char is `repr(transparent)` over a
    // 32-bit Unicode scalar value, but the only sound way to cast in stable
    // Rust is to leak a const-built slice. We instead transmute the slice
    // pointer, relying on the build-script invariant that every entry is a
    // valid scalar value (it came from `as u32` of a real char).
    let data = &tab::homo_data()[start..end];
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
