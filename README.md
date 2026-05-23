<div align="center">

# 🇨🇳 pizza-analysis-pinyin

**Chinese Pinyin conversion plugin for [INFINI Pizza](https://pizza.rs)**

[![Crate](https://img.shields.io/badge/crate-pizza--analysis--pinyin-blue)](https://github.com/pizza-rs/analysis-pinyin)
[![License](https://img.shields.io/badge/license-MIT-green)](LICENSE)

</div>

---

A fast, compact, **zero-copy** Chinese-Pinyin analyzer/tokenizer for
[Pizza](https://github.com/infinilabs/pizza), ported from the Java
[`infinilabs/analysis-pinyin`](https://github.com/infinilabs/analysis-pinyin)
Elasticsearch / OpenSearch / Easysearch plugin.

It converts Chinese text into pinyin tokens with the same configurable
behaviour as the Java plugin, plus a runtime [`Rules`](src/rules.rs) overlay
for polyphone / proper-noun disambiguation.

```text
刘德华            → liu, de, hua, ldh
刘德华 Andy Lau   → liu, de, hua, andy, lau, ldhandylau
liudehua          → liu, de, hua          (alphabet re-segmentation)
银行家  + rule    → yin, hang, jia        (phrase override; default would be xing)
```

## Highlights

- **Compact**: a fixed `[u16; 20736]` primary table plus CSR-encoded
  polyphone / homophone tables — roughly **50 KB** of static data for the
  default build. The full curated phrase dictionary (~450 K entries) is
  opt-in behind the `polyphone-dict` cargo feature.
- **Zero-copy hot path**: emitted `Token<'a>` carries a
  `Cow::Borrowed(&'static str)` for pinyin syllables (pointing into the
  bundled dictionary) and `Cow::Borrowed(&'a str)` for per-character Chinese
  tokens and ASCII pass-throughs. The only allocations are the optional
  joined-first-letter and joined-full-pinyin strings.
- **Extensible**: [`Rules`](src/rules.rs) lets you override the readings of a
  single character (`重` → `chong`) or a whole phrase (`银行` → `yin hang`)
  at runtime, without rebuilding the dictionary.
- **Polyphone / homophone APIs**: enumerate every reading of a character,
  test whether a character is a polyphone, or look up every Chinese
  character that shares a syllable.
- **Java-plugin parity**: all 17 original config flags are preserved with
  matching defaults, so existing index mappings keep their semantics.

## Installation

```toml
[dependencies]
pizza-pinyin = { path = "../pizza/contrib/pinyin" }
# Optional: bundle the curated ~450 K-entry phrase dictionary (~11 MB).
# pizza-pinyin = { path = "...", features = ["polyphone-dict"] }
```

## Quick start

```rust
use pizza_engine::analysis::Tokenizer;
use pizza_pinyin::{PinyinConfig, PinyinTokenizer};

let tokenizer = PinyinTokenizer::new(PinyinConfig::default());
let tokens = tokenizer.tokenize("刘德华");
for t in &tokens {
    println!("{} [{}, {}] @ {}", t.term, t.start_offset, t.end_offset, t.position);
}
// liu, de, hua, ldh
```

`tokenize` returns `Vec<Token<'a>>` where `Token::term: Cow<'a, str>` —
borrowed from the static dictionary or the input wherever possible.

## Indexing-friendly default

The default [`PinyinConfig`] mirrors the Java plugin's recommended index
analyzer:

| Behaviour                                  | Default  |
|--------------------------------------------|----------|
| Lowercase every term                       | `true`   |
| Trim whitespace                            | `true`   |
| Keep non-Chinese tokens                    | `true`   |
| Group consecutive ASCII into one token     | `true`   |
| Re-segment ASCII as pinyin                 | `true`   |
| Emit per-character full pinyin             | `true`   |
| Emit joined first-letter token             | `true`   |
| Limit joined first-letter length           | `16`     |
| Ignore real pinyin offsets                 | `true`   |
| Emit original input as a token             | `false`  |
| Emit joined full pinyin                    | `false`  |
| Emit each Chinese character as a token     | `false`  |

## Configuration reference

[`PinyinConfig`](src/config.rs) exposes the following fields. Defaults match
the Java plugin.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `lowercase` | `bool` | `true` | Lowercase every emitted term. |
| `trim_whitespace` | `bool` | `true` | Trim leading/trailing whitespace from each term. |
| `keep_none_chinese` | `bool` | `true` | Emit non-Chinese characters (ASCII letters/digits) as tokens. |
| `keep_none_chinese_in_first_letter` | `bool` | `true` | Include non-Chinese characters in the joined first-letter token. |
| `keep_none_chinese_in_joined_full_pinyin` | `bool` | `false` | Include non-Chinese characters in the joined full-pinyin token. |
| `keep_original` | `bool` | `false` | Emit the original input as a token. |
| `keep_first_letter` | `bool` | `true` | Emit the joined first letters (`刘德华` → `ldh`). |
| `keep_separate_first_letter` | `bool` | `false` | Emit each first letter as a separate token (`刘德华` → `l`, `d`, `h`). |
| `keep_none_chinese_together` | `bool` | `true` | Keep a run of non-Chinese characters in a single token. |
| `none_chinese_pinyin_tokenize` | `bool` | `true` | Re-segment a run of ASCII letters as pinyin (`liudehua` → `liu`, `de`, `hua`). |
| `limit_first_letter_length` | `usize` | `16` | Max length of the joined first-letter token; `0` disables truncation. |
| `keep_full_pinyin` | `bool` | `true` | Emit each character's full pinyin (`刘` → `liu`). |
| `keep_joined_full_pinyin` | `bool` | `false` | Emit a single joined full-pinyin token (`刘德华` → `liudehua`). |
| `remove_duplicate_term` | `bool` | `false` | Drop duplicate terms regardless of position. |
| `fixed_pinyin_offset` | `bool` | `false` | Use fixed 1-char offsets when re-tokenizing ASCII pinyin. |
| `ignore_pinyin_offset` | `bool` | `true` | Skip strict offset checks (matches the Java plugin's relaxed mode). |
| `keep_separate_chinese` | `bool` | `false` | Emit each Chinese character as its own token alongside its pinyin. |

At least one of `keep_first_letter`, `keep_separate_first_letter`,
`keep_full_pinyin`, `keep_joined_full_pinyin`, or `keep_separate_chinese`
must be `true`; otherwise [`PinyinConfig::validate`] returns an error and
`PinyinTokenizer::new` panics.

### Common analyzer recipes

**Search-side analyzer** (joined pinyin only, deduped):

```rust
use pizza_pinyin::PinyinConfig;

let cfg = PinyinConfig {
    keep_first_letter: false,
    keep_full_pinyin: false,
    keep_joined_full_pinyin: true,
    remove_duplicate_term: true,
    ..PinyinConfig::default()
};
```

**First-letter prefix matching**:

```rust
let cfg = PinyinConfig {
    keep_first_letter: true,
    keep_full_pinyin: false,
    keep_joined_full_pinyin: false,
    limit_first_letter_length: 8,
    ..PinyinConfig::default()
};
```

**Per-character pinyin + per-character Chinese (highlighting-friendly)**:

```rust
let cfg = PinyinConfig {
    keep_full_pinyin: true,
    keep_separate_chinese: true,
    keep_first_letter: false,
    ignore_pinyin_offset: false,
    ..PinyinConfig::default()
};
```

## Rules — runtime overrides

[`Rules`](src/rules.rs) lets you customise readings **without** rebuilding the
dictionary. Char-level and phrase-level overrides combine naturally:
phrase overrides take precedence over char overrides, which take precedence
over the bundled dictionary's primary reading.

```rust
use pizza_pinyin::{PinyinConfig, PinyinTokenizer, Rules};
use pizza_engine::analysis::Tokenizer;

let rules = Rules::new()
    // Proper-noun pronunciation
    .with_char('重', &["chong"])
    // Phrase disambiguation — defaults would emit "xing" for 行
    .with_phrase("银行", &["yin", "hang"])
    .with_phrase("中国银行", &["zhong", "guo", "yin", "hang"]);

let tk = PinyinTokenizer::new(PinyinConfig::default()).with_rules(rules);
let tokens = tk.tokenize("中国银行家"); // yin hang, not yin xing
```

Phrase matching is **longest-match**, bucketed by the phrase's first
character for cheap scans. The number of readings supplied must equal the
character count of the phrase, otherwise the entry is silently ignored.

### Bundled polyphone dictionary (opt-in)

Enable the `polyphone-dict` feature to bake the curated
`polyphone.txt` (~450 K phrase entries) into the binary:

```toml
[dependencies]
pizza-pinyin = { path = "...", features = ["polyphone-dict"] }
```

```rust
use pizza_pinyin::{PinyinTokenizer, Rules};

let rules = Rules::new().with_builtin_polyphones();
let tk = PinyinTokenizer::with_defaults().with_rules(rules);
```

The phrase table is stored as flat `&'static` arrays (sorted keys + CSR
data) so loading is `O(1)` — no parsing or hashing at startup.

## Dictionary API

[`PinyinDict`](src/dict.rs) exposes the bundled dictionary as static
functions — no instance needed.

```rust
use pizza_pinyin::PinyinDict;

// Primary reading (most common pronunciation).
assert_eq!(PinyinDict::primary('刘'), Some("liu"));

// All readings for a polyphone.
let xs: Vec<&'static str> = PinyinDict::readings('行').collect();
assert!(xs.contains(&"xing"));
assert!(xs.contains(&"hang"));

// Is this character a polyphone?
assert!(PinyinDict::is_polyphone('行'));

// Reverse lookup — characters that share a syllable (homophones).
let homo = PinyinDict::homophones("hua");
assert!(homo.contains(&'华'));

// Lookup a syllable id (for compact storage).
let id = PinyinDict::syllable_id("liu").unwrap();
assert_eq!(id.as_str(), "liu");
```

## Alphabet re-segmentation

The pinyin-string segmenter is also exposed standalone:

```rust
use pizza_pinyin::segment_pinyin_alphabet;

let pieces = segment_pinyin_alphabet("liudehua");
assert_eq!(pieces, vec!["liu", "de", "hua"]);
```

It uses a positive/reverse max-match against the ~410-entry syllable
alphabet (`pinyin_alphabet.dict`), so unknown chunks fall through unchanged.

## Performance notes

- Per `tokenize` call, the only required allocations are the `Vec<Token>`
  itself and (when enabled) one `String` each for the joined first-letter
  and joined full-pinyin tokens. Both are pre-sized from the input's char
  count.
- The hot loop is a single pass over `char_indices`. Per Chinese character
  it does one `u16` table read; per ASCII run it does one re-segmentation
  pass if `none_chinese_pinyin_tokenize` is on.
- `Rules` adds an `O(chars)` pre-pass for phrase matching, gated behind
  `Rules::is_empty()`, so the fast path is free when no rules are
  configured.
- All syllable strings are `&'static str` interned in a sorted
  `SYLLABLES` table; `Rules` overrides auto-intern through this table
  whenever possible (`Reading::Static(&'static str)`), so typical user
  overrides like `"hang"` cost zero allocations.

## Cargo features

| Feature           | Default | Effect                                                                   |
|-------------------|---------|--------------------------------------------------------------------------|
| `polyphone-dict`  | off     | Bundle the ~450 K-entry phrase dictionary (~11 MB) and enable [`Rules::with_builtin_polyphones`]. |

## Normalizer

[`PinyinNormalizer`](src/normalizer.rs) implements the `pizza-engine`
`Normalizer` trait, converting Chinese characters to pinyin **in-place**
before tokenization. Three output modes are available:

| Mode | Input | Output |
|------|-------|--------|
| `FullPinyin` (default) | `"刘德华"` | `"liu de hua"` |
| `JoinedPinyin` | `"刘德华"` | `"liudehua"` |
| `FirstLetter` | `"刘德华"` | `"ldh"` |

```rust
use pizza_pinyin::{PinyinNormalizer, PinyinNormalizeConfig, PinyinNormalizeMode};
use pizza_engine::analysis::Normalizer;

let normalizer = PinyinNormalizer::new(PinyinNormalizeConfig {
    mode: PinyinNormalizeMode::FullPinyin,
    separator: " ",
    lowercase: true,
});
let mut text = String::from("刘德华");
normalizer.normalize(&mut text);
assert_eq!(text, "liu de hua");
```

Non-Chinese characters are passed through unchanged, making it safe to use
on mixed-language text.

## License

MIT — see [LICENSE](LICENSE).

Pizza-pinyin is part of the [Pizza](https://github.com/infinilabs/pizza)
search engine project. Visit [pizza.rs](http://pizza.rs) for more details.
