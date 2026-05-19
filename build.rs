// Build script: parse the raw `data/*.txt` dictionaries once at compile time
// and emit a compact, statically-typed Rust source file consumed by `src/dict.rs`.
//
// Goals:
// - All lookups at runtime are O(1) array access or O(log N) binary search.
// - Zero heap allocations on the read path: every syllable is `&'static str`.
// - Compact: per-character primary syllable id stored as u16 in a flat array
//   covering the CJK Unified Ideographs range (U+4E00..=U+9FFF, ~20.7 K chars,
//   ~42 KB), plus a small auxiliary table for polyphones.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

// CJK Unified Ideographs block we care about. The Java source dictionary only
// covers \u4E00..=\u9FA5; we allocate up to \u9FFF to leave room for the few
// extra mapped code points (and to round things off).
const CJK_START: u32 = 0x4E00;
const CJK_END_EXCL: u32 = 0xA000; // exclusive upper bound
const CJK_LEN: usize = (CJK_END_EXCL - CJK_START) as usize;
const NO_SYLLABLE: u16 = u16::MAX;

fn main() {
    println!("cargo:rerun-if-changed=data/pinyin.txt");
    println!("cargo:rerun-if-changed=data/pinyin_alphabet.dict");
    println!("cargo:rerun-if-changed=data/polyphone.txt");
    println!("cargo:rerun-if-changed=build.rs");

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));

    let pinyin_txt = manifest.join("data/pinyin.txt");
    let alphabet_dict = manifest.join("data/pinyin_alphabet.dict");

    let pinyin_raw = fs::read_to_string(&pinyin_txt).expect("read data/pinyin.txt");
    let alphabet_raw = fs::read_to_string(&alphabet_dict).expect("read data/pinyin_alphabet.dict");

    // ---- 1) Collect unique syllables (no tone) and per-char readings ----
    //
    // Each line: `<ch>=<reading1>,<reading2>,...` where each reading ends with
    // a tone digit '0'..='5'. We keep all readings per char in source order,
    // but for the primary table we only need the first.

    // Sorted, deduplicated set of plain syllables.
    let mut syllables_set: BTreeMap<String, ()> = BTreeMap::new();
    // Per-char vector of plain syllables (preserves order from pinyin.txt).
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
            // Outside the bundled range — ignore (the Java source dict is
            // strictly within CJK Unified Ideographs).
            continue;
        }
        let mut seen = std::collections::HashSet::new();
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
                syllables_set.insert(plain.clone(), ());
                readings.push(plain);
            }
        }
        if !readings.is_empty() {
            char_readings.insert(ch, readings);
        }
    }

    // Also fold in any syllables from the alphabet dict so the SYLLABLES table
    // is a strict superset (lets us reuse SyllableIds for alphabet matching).
    for line in alphabet_raw.lines() {
        let s = line.trim();
        if s.is_empty() {
            continue;
        }
        syllables_set.insert(s.to_ascii_lowercase(), ());
    }

    let syllables: Vec<String> = syllables_set.into_keys().collect();
    let syllable_id: BTreeMap<&str, u16> = syllables
        .iter()
        .enumerate()
        .map(|(i, s)| (s.as_str(), i as u16))
        .collect();
    assert!(
        syllables.len() < (NO_SYLLABLE as usize),
        "too many syllables ({}) — bump id type",
        syllables.len()
    );

    // ---- 2) Build the per-char primary syllable table ----
    let mut primary = vec![NO_SYLLABLE; CJK_LEN];
    // Polyphone extra readings: char_offset_in_range -> list of ids (excluding primary)
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

    // ---- 3) Build the homophone reverse index (syllable -> sorted chars) ----
    let mut homo: Vec<Vec<u32>> = vec![Vec::new(); syllables.len()];
    for (ch, readings) in &char_readings {
        for r in readings {
            let id = *syllable_id.get(r.as_str()).expect("known syllable");
            homo[id as usize].push(*ch as u32);
        }
    }
    for v in &mut homo {
        v.sort_unstable();
        v.dedup();
    }

    // ---- 4) Alphabet syllable id list (subset of SYLLABLES, sorted) ----
    let mut alphabet_ids: Vec<u16> = alphabet_raw
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_ascii_lowercase())
        .map(|s| *syllable_id.get(s.as_str()).expect("alphabet syllable in table"))
        .collect();
    alphabet_ids.sort_unstable();
    alphabet_ids.dedup();

    // ---- 5) Emit generated.rs ----
    let dest = out_dir.join("generated.rs");
    let mut w = std::io::BufWriter::new(fs::File::create(&dest).expect("create generated.rs"));

    writeln!(
        w,
        "// AUTO-GENERATED by build.rs from data/pinyin.txt and data/pinyin_alphabet.dict.\n\
         // DO NOT EDIT.\n\
         pub const CJK_START: u32 = 0x{CJK_START:04X};\n\
         pub const CJK_END_EXCL: u32 = 0x{CJK_END_EXCL:04X};\n\
         pub const CJK_LEN: usize = {CJK_LEN};\n\
         pub const NO_SYLLABLE: u16 = u16::MAX;\n"
    )
    .unwrap();

    // SYLLABLES (sorted alphabetically — also lets us binary-search by name).
    writeln!(w, "pub static SYLLABLES: &[&str] = &[").unwrap();
    for s in &syllables {
        writeln!(w, "    {:?},", s).unwrap();
    }
    writeln!(w, "];\n").unwrap();

    // PRIMARY: flat [u16; CJK_LEN]
    writeln!(w, "pub static PRIMARY: &[u16; CJK_LEN] = &[").unwrap();
    for chunk in primary.chunks(16) {
        write!(w, "    ").unwrap();
        for v in chunk {
            write!(w, "{}, ", v).unwrap();
        }
        writeln!(w).unwrap();
    }
    writeln!(w, "];\n").unwrap();

    // POLY: parallel arrays (sorted by char offset) for binary search.
    // POLY_OFFSETS[i] = char offset within CJK range
    // POLY_DATA_PTR[i..i+1] slice POLY_DATA for that char's extra readings.
    let poly_entries: Vec<(u16, &[u16])> = poly.iter().map(|(k, v)| (*k, v.as_slice())).collect();
    let mut poly_data_flat: Vec<u16> = Vec::new();
    let mut poly_data_ptr: Vec<u32> = Vec::with_capacity(poly_entries.len() + 1);
    poly_data_ptr.push(0);
    for (_, ids) in &poly_entries {
        poly_data_flat.extend_from_slice(ids);
        poly_data_ptr.push(poly_data_flat.len() as u32);
    }
    writeln!(w, "pub static POLY_OFFSETS: &[u16] = &[").unwrap();
    for chunk in poly_entries.chunks(16) {
        write!(w, "    ").unwrap();
        for (k, _) in chunk {
            write!(w, "{}, ", k).unwrap();
        }
        writeln!(w).unwrap();
    }
    writeln!(w, "];\n").unwrap();
    writeln!(w, "pub static POLY_DATA_PTR: &[u32] = &[").unwrap();
    for chunk in poly_data_ptr.chunks(16) {
        write!(w, "    ").unwrap();
        for v in chunk {
            write!(w, "{}, ", v).unwrap();
        }
        writeln!(w).unwrap();
    }
    writeln!(w, "];\n").unwrap();
    writeln!(w, "pub static POLY_DATA: &[u16] = &[").unwrap();
    for chunk in poly_data_flat.chunks(16) {
        write!(w, "    ").unwrap();
        for v in chunk {
            write!(w, "{}, ", v).unwrap();
        }
        writeln!(w).unwrap();
    }
    writeln!(w, "];\n").unwrap();

    // HOMOPHONE (CSR style: HOMO_PTR[id..id+1] -> HOMO_DATA slice of chars u32)
    let mut homo_data: Vec<u32> = Vec::new();
    let mut homo_ptr: Vec<u32> = Vec::with_capacity(homo.len() + 1);
    homo_ptr.push(0);
    for v in &homo {
        homo_data.extend_from_slice(v);
        homo_ptr.push(homo_data.len() as u32);
    }
    writeln!(w, "pub static HOMO_PTR: &[u32] = &[").unwrap();
    for chunk in homo_ptr.chunks(16) {
        write!(w, "    ").unwrap();
        for v in chunk {
            write!(w, "{}, ", v).unwrap();
        }
        writeln!(w).unwrap();
    }
    writeln!(w, "];\n").unwrap();
    writeln!(w, "pub static HOMO_DATA: &[u32] = &[").unwrap();
    for chunk in homo_data.chunks(16) {
        write!(w, "    ").unwrap();
        for v in chunk {
            write!(w, "{}, ", v).unwrap();
        }
        writeln!(w).unwrap();
    }
    writeln!(w, "];\n").unwrap();

    // ALPHABET_IDS (sorted) — also stored as a sorted slice of names for
    // ergonomic binary search by syllable text.
    writeln!(w, "pub static ALPHABET_IDS: &[u16] = &[").unwrap();
    for chunk in alphabet_ids.chunks(16) {
        write!(w, "    ").unwrap();
        for v in chunk {
            write!(w, "{}, ", v).unwrap();
        }
        writeln!(w).unwrap();
    }
    writeln!(w, "];\n").unwrap();

    // Sorted names slice (for `is_syllable(&str)` binary search).
    let mut alphabet_names: Vec<&str> = alphabet_ids
        .iter()
        .map(|id| syllables[*id as usize].as_str())
        .collect();
    alphabet_names.sort_unstable();
    writeln!(w, "pub static ALPHABET_NAMES: &[&str] = &[").unwrap();
    for s in &alphabet_names {
        writeln!(w, "    {:?},", s).unwrap();
    }
    writeln!(w, "];\n").unwrap();

    // Stats banner so `cargo build -vv` users can see the footprint.
    writeln!(
        w,
        "// Generated: {} syllables, {} mapped chars, {} polyphone chars, {} alphabet syllables.",
        syllables.len(),
        char_readings.len(),
        poly_entries.len(),
        alphabet_names.len()
    )
    .unwrap();

    // ---- 6) Optional: polyphone phrase dictionary ----
    #[cfg(any())]
    let _ = "feature gating happens at runtime via cfg in src/polyphone_dict.rs";

    let polyphone_txt = manifest.join("data/polyphone.txt");
    let polyphone_enabled = env::var_os("CARGO_FEATURE_POLYPHONE_DICT").is_some();

    let phrase_dest = out_dir.join("polyphone_phrases.rs");
    let mut pw =
        std::io::BufWriter::new(fs::File::create(&phrase_dest).expect("create polyphone_phrases"));

    if polyphone_enabled {
        let polyphone_raw =
            fs::read_to_string(&polyphone_txt).expect("read data/polyphone.txt");
        // Parse: each non-#, non-empty line is `<phrase>=<r1> <r2> ...` where
        // r_i = `<syll><tone-digit>`.
        // Build BTreeMap<phrase, Vec<SyllableId>> so we can emit a sorted
        // `&[(&str, &[u16])]` slice for binary search.
        let mut phrases: BTreeMap<String, Vec<u16>> = BTreeMap::new();
        for line in polyphone_raw.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((phrase, readings)) = line.split_once('=') else {
                continue;
            };
            let phrase = phrase.trim();
            let char_count = phrase.chars().count();
            let mut ids: Vec<u16> = Vec::with_capacity(char_count);
            let mut ok = true;
            for r in readings.split_whitespace() {
                let plain = strip_tone(r).to_ascii_lowercase();
                if plain.is_empty() {
                    ok = false;
                    break;
                }
                match syllable_id.get(plain.as_str()) {
                    Some(id) => ids.push(*id),
                    None => {
                        ok = false;
                        break;
                    }
                }
            }
            if !ok || ids.len() != char_count {
                continue;
            }
            // Last-write-wins: matches the Java behaviour where later entries
            // override earlier ones for the same phrase.
            phrases.insert(phrase.to_string(), ids);
        }

        writeln!(
            pw,
            "// AUTO-GENERATED — phrase-level polyphone overrides.\n\
             // Sorted by phrase for binary search.\n\
             pub static PHRASE_KEYS: &[&str] = &["
        )
        .unwrap();
        for k in phrases.keys() {
            writeln!(pw, "    {:?},", k).unwrap();
        }
        writeln!(pw, "];\n").unwrap();

        // Flat values: PHRASE_PTR[i..i+1] slice into PHRASE_DATA.
        let mut data_flat: Vec<u16> = Vec::new();
        let mut data_ptr: Vec<u32> = Vec::with_capacity(phrases.len() + 1);
        data_ptr.push(0);
        for v in phrases.values() {
            data_flat.extend_from_slice(v);
            data_ptr.push(data_flat.len() as u32);
        }
        writeln!(pw, "pub static PHRASE_PTR: &[u32] = &[").unwrap();
        for chunk in data_ptr.chunks(16) {
            write!(pw, "    ").unwrap();
            for v in chunk {
                write!(pw, "{}, ", v).unwrap();
            }
            writeln!(pw).unwrap();
        }
        writeln!(pw, "];\n").unwrap();
        writeln!(pw, "pub static PHRASE_DATA: &[u16] = &[").unwrap();
        for chunk in data_flat.chunks(16) {
            write!(pw, "    ").unwrap();
            for v in chunk {
                write!(pw, "{}, ", v).unwrap();
            }
            writeln!(pw).unwrap();
        }
        writeln!(pw, "];").unwrap();
    } else {
        // Emit empty stubs so `include!` always succeeds.
        writeln!(
            pw,
            "pub static PHRASE_KEYS: &[&str] = &[];\n\
             pub static PHRASE_PTR: &[u32] = &[0];\n\
             pub static PHRASE_DATA: &[u16] = &[];"
        )
        .unwrap();
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
