//! Port of the Java `PinyinTokenizer` from
//! [`infinilabs/analysis-pinyin`](https://github.com/infinilabs/analysis-pinyin),
//! redesigned for zero allocation on the hot path:
//!
//! - Pinyin syllables are returned as `Cow::Borrowed(&'static str)` (pointing
//!   into the baked dictionary).
//! - Per-character Chinese tokens (when [`PinyinConfig::keep_separate_chinese`]
//!   is on) borrow directly from the input as `Cow::Borrowed(&'a str)`.
//! - ASCII pass-through tokens borrow from the input.
//! - Only the joined first-letter and joined-full-pinyin tokens require
//!   allocation, and each is built into a single pre-sized `String`.
//!
//! The tokenizer is also configurable via [`crate::Rules`] for per-character
//! and phrase-level pinyin overrides (polyphone disambiguation).

use std::borrow::Cow;
use std::collections::HashSet;

use pizza_engine::analysis::Token;
use pizza_engine::analysis::Tokenizer;

use crate::alphabet;
use crate::config::PinyinConfig;
use crate::dict::PinyinDict;
use crate::rules::{Reading, Rules};

const CJK_START: char = '\u{4E00}';
const CJK_END: char = '\u{9FA5}';

#[inline]
fn is_chinese(c: char) -> bool {
    (CJK_START..=CJK_END).contains(&c)
}

#[inline]
fn is_ascii_letter_or_digit(c: char) -> bool {
    c.is_ascii_alphanumeric()
}

/// FNV-1a 64-bit hash — fast, allocation-free, good enough for dedup.
#[inline]
fn fnv1a_hash(bytes: &[u8], extra: u32) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    // Mix in the position/extra discriminator.
    let eb = extra.to_le_bytes();
    for &b in &eb {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// Internal candidate (kept compact: 16 + 4 + 4 + 4 + 1 = 29 bytes per entry).
#[derive(Debug, Clone)]
struct Candidate<'a> {
    term: Cow<'a, str>,
    start_offset: u32,
    end_offset: u32,
    position: u32,
}

/// Pinyin tokenizer with [`PinyinConfig`] + optional [`Rules`] overlay.
#[derive(Clone)]
pub struct PinyinTokenizer {
    config: PinyinConfig,
    rules: Rules,
}

impl PinyinTokenizer {
    /// Construct with the given configuration and no extra rules. Panics if
    /// the configuration is invalid (every output kind disabled).
    pub fn new(config: PinyinConfig) -> Self {
        config.validate().expect("invalid PinyinConfig");
        Self {
            config,
            rules: Rules::new(),
        }
    }

    /// Construct with the default configuration.
    pub fn with_defaults() -> Self {
        Self::new(PinyinConfig::default())
    }

    /// Attach custom [`Rules`] (chainable).
    pub fn with_rules(mut self, rules: Rules) -> Self {
        self.rules = rules;
        self
    }

    /// Borrow the active rules (read-only).
    pub fn rules(&self) -> &Rules {
        &self.rules
    }

    /// Borrow the configuration.
    pub fn config(&self) -> &PinyinConfig {
        &self.config
    }
}

impl Tokenizer for PinyinTokenizer {
    fn tokenize<'a>(&self, text: &'a str) -> Vec<Token<'a>> {
        let cfg = &self.config;
        if text.is_empty() {
            return Vec::new();
        }

        // Build byte-offset table + char array in a single pass (avoids
        // the old `char_indices().collect()` + separate `only_chars` vec).
        let mut byte_off: Vec<u32> = Vec::with_capacity(text.len() / 2 + 1);
        let mut only_chars: Vec<char> = Vec::with_capacity(text.len() / 2);
        for (b, c) in text.char_indices() {
            byte_off.push(b as u32);
            only_chars.push(c);
        }
        byte_off.push(text.len() as u32);
        let char_count = only_chars.len();
        if char_count == 0 {
            return Vec::new();
        }

        let to_byte = |s: usize, e: usize| -> (u32, u32) {
            (
                byte_off.get(s).copied().unwrap_or(text.len() as u32),
                byte_off.get(e).copied().unwrap_or(text.len() as u32),
            )
        };

        // Precompute phrase overrides per char position. For each char index
        // we store an optional `&Reading` to use instead of the dict lookup.
        let mut overlay: Vec<Option<&Reading>> = vec![None; char_count];
        if !self.rules.is_empty() {
            let mut i = 0usize;
            while i < char_count {
                if let Some(hit) = self.rules.match_phrase(&only_chars, i) {
                    for (k, r) in hit.readings.iter().enumerate() {
                        overlay[i + k] = Some(r);
                    }
                    i += hit.char_len;
                } else {
                    i += 1;
                }
            }
        }

        let mut candidates: Vec<Candidate<'a>> = Vec::with_capacity(char_count * 2 + 4);
        // Dedup set: stores u64 hashes of (term, position) or (term) depending
        // on config. Avoids per-candidate String allocation entirely.
        let mut terms_filter: HashSet<u64> = HashSet::with_capacity(char_count * 2 + 4);
        let mut first_letters: Vec<u8> = Vec::with_capacity(char_count);
        let mut joined_full = String::with_capacity(char_count * 4);
        let mut position: u32 = 0;
        let mut last_offset: usize = 0;

        // ASCII run buffer
        let mut buff_byte_start: usize = 0;
        let mut buff_byte_end: usize = 0;
        let mut buff_char_start: usize = 0;
        let mut buff_char_end: usize = 0;
        let buff_active = |start: usize, end: usize| start < end;

        for i in 0..char_count {
            let c = only_chars[i];
            let byte_idx = byte_off[i] as usize;
            if (c as u32) < 128 {
                if !buff_active(buff_byte_start, buff_byte_end) {
                    buff_byte_start = byte_idx;
                    buff_char_start = i;
                }
                if is_ascii_letter_or_digit(c) {
                    if cfg.keep_none_chinese {
                        if cfg.keep_none_chinese_together {
                            buff_byte_end = byte_idx + c.len_utf8();
                            buff_char_end = i + 1;
                        } else {
                            position += 1;
                            let (s, e) = to_byte(i, i + 1);
                            push_candidate(
                                cfg,
                                &mut candidates,
                                &mut terms_filter,
                                Candidate {
                                    term: Cow::Borrowed(&text[s as usize..e as usize]),
                                    start_offset: s,
                                    end_offset: e,
                                    position: (buff_char_start as u32) + 1,
                                },
                            );
                        }
                    }
                    if cfg.keep_none_chinese_in_first_letter {
                        first_letters.push(c as u8);
                    }
                    if cfg.keep_none_chinese_in_joined_full_pinyin {
                        joined_full.push(c);
                    }
                }
            } else {
                // Flush any pending ASCII run before handling a Chinese char.
                if buff_active(buff_byte_start, buff_byte_end) {
                    self.flush_ascii_buff(
                        text,
                        buff_byte_start,
                        buff_byte_end,
                        buff_char_start,
                        buff_char_end,
                        last_offset,
                        &mut position,
                        &mut candidates,
                        &mut terms_filter,
                        &byte_off,
                    );
                    buff_byte_start = 0;
                    buff_byte_end = 0;
                }

                if is_chinese(c) {
                    let mut incr_position = false;

                    // Resolve readings into a Cow<'a, str> whose lifetime is
                    // either the static dict or a one-off owned clone for
                    // user-supplied rules. We also need the first ASCII byte.
                    let reading: Option<(Cow<'a, str>, u8)> = if let Some(r) = overlay[i] {
                        let s = r.as_str();
                        if s.is_empty() {
                            None
                        } else {
                            Some((r.to_cow(), s.as_bytes()[0]))
                        }
                    } else if let Some(o) = self.rules.char_readings(c) {
                        o.first().and_then(|r| {
                            let s = r.as_str();
                            if s.is_empty() {
                                None
                            } else {
                                Some((r.to_cow(), s.as_bytes()[0]))
                            }
                        })
                    } else {
                        PinyinDict::primary(c).and_then(|s| {
                            if s.is_empty() {
                                None
                            } else {
                                Some((Cow::Borrowed(s), s.as_bytes()[0]))
                            }
                        })
                    };

                    if let Some((pinyin_cow, fl_byte)) = reading {
                        let pinyin_str = pinyin_cow.as_ref();
                        first_letters.push(fl_byte);

                        if cfg.keep_separate_first_letter && pinyin_str.len() > 1 {
                            position += 1;
                            incr_position = true;
                            let (s, e) = to_byte(i, i + 1);
                            // Borrow the first byte from the Cow (always
                            // ASCII). When pinyin_cow is owned we'd have to
                            // own this too; but slicing &Cow<str>[..1] gives
                            // a borrow with the Cow's lifetime, so we route
                            // through the static dict by re-interning.
                            let term: Cow<'a, str> = first_letter_cow(&pinyin_cow);
                            push_candidate(
                                cfg,
                                &mut candidates,
                                &mut terms_filter,
                                Candidate {
                                    term,
                                    start_offset: s,
                                    end_offset: e,
                                    position,
                                },
                            );
                        }
                        if cfg.keep_full_pinyin {
                            if !incr_position {
                                position += 1;
                            }
                            let (s, e) = to_byte(i, i + 1);
                            push_candidate(
                                cfg,
                                &mut candidates,
                                &mut terms_filter,
                                Candidate {
                                    term: pinyin_cow.clone(),
                                    start_offset: s,
                                    end_offset: e,
                                    position,
                                },
                            );
                        }
                        if cfg.keep_separate_chinese {
                            let (s, e) = to_byte(i, i + 1);
                            push_candidate(
                                cfg,
                                &mut candidates,
                                &mut terms_filter,
                                Candidate {
                                    term: Cow::Borrowed(&text[s as usize..e as usize]),
                                    start_offset: s,
                                    end_offset: e,
                                    position,
                                },
                            );
                        }
                        if cfg.keep_joined_full_pinyin {
                            joined_full.push_str(pinyin_str);
                        }
                    } else if cfg.keep_separate_chinese {
                        let (s, e) = to_byte(i, i + 1);
                        push_candidate(
                            cfg,
                            &mut candidates,
                            &mut terms_filter,
                            Candidate {
                                term: Cow::Borrowed(&text[s as usize..e as usize]),
                                start_offset: s,
                                end_offset: e,
                                position,
                            },
                        );
                    }
                }
            }
            last_offset = i;
        }

        if buff_active(buff_byte_start, buff_byte_end) {
            self.flush_ascii_buff(
                text,
                buff_byte_start,
                buff_byte_end,
                buff_char_start,
                buff_char_end,
                last_offset,
                &mut position,
                &mut candidates,
                &mut terms_filter,
                &byte_off,
            );
        }

        if cfg.keep_original {
            push_candidate(
                cfg,
                &mut candidates,
                &mut terms_filter,
                Candidate {
                    term: Cow::Borrowed(text),
                    start_offset: 0,
                    end_offset: text.len() as u32,
                    position: 1,
                },
            );
        }

        if cfg.keep_joined_full_pinyin && !joined_full.is_empty() {
            push_candidate(
                cfg,
                &mut candidates,
                &mut terms_filter,
                Candidate {
                    term: Cow::Owned(joined_full.clone()),
                    start_offset: 0,
                    end_offset: text.len() as u32,
                    position: 1,
                },
            );
        }

        if cfg.keep_first_letter && !first_letters.is_empty() {
            let lim = cfg.limit_first_letter_length;
            let take_len = if lim > 0 && first_letters.len() > lim {
                lim
            } else {
                first_letters.len()
            };
            // ASCII bytes only -> UTF-8 is the same buffer.
            let mut s = String::with_capacity(take_len);
            for b in &first_letters[..take_len] {
                let c = if cfg.lowercase {
                    b.to_ascii_lowercase()
                } else {
                    *b
                };
                s.push(c as char);
            }
            let len = s.len();
            if !(cfg.keep_separate_first_letter && s.chars().count() <= 1) {
                push_candidate(
                    cfg,
                    &mut candidates,
                    &mut terms_filter,
                    Candidate {
                        term: Cow::Owned(s),
                        start_offset: 0,
                        end_offset: len as u32,
                        position: 1,
                    },
                );
            }
        }

        // Stable sort by position (mirrors Java `Collections.sort`).
        candidates.sort_by(|a, b| a.position.cmp(&b.position));

        let mut out: Vec<Token<'a>> = Vec::with_capacity(candidates.len());
        let mut last_pos: i64 = -1;
        let mut emit_pos: u32 = 0;
        for item in candidates.into_iter() {
            if (item.position as i64) > last_pos {
                if last_pos >= 0 {
                    emit_pos += 1;
                }
                last_pos = item.position as i64;
            }
            out.push(Token {
                term: item.term,
                start_offset: item.start_offset,
                end_offset: item.end_offset,
                position: emit_pos,
            });
        }
        out
    }
}

impl PinyinTokenizer {
    #[allow(clippy::too_many_arguments)]
    fn flush_ascii_buff<'a>(
        &self,
        text: &'a str,
        byte_start: usize,
        byte_end: usize,
        char_start: usize,
        char_end: usize,
        _last_offset: usize,
        position: &mut u32,
        candidates: &mut Vec<Candidate<'a>>,
        terms_filter: &mut HashSet<u64>,
        byte_off: &[u32],
    ) {
        let cfg = &self.config;
        if !cfg.keep_none_chinese {
            return;
        }
        let raw = &text[byte_start..byte_end];

        if cfg.none_chinese_pinyin_tokenize {
            // Re-segment as pinyin. We need lowercase ASCII for matching; the
            // alphabet API will borrow from `raw` when already lowercase.
            let mut scratch = String::new();
            let pieces = alphabet::walk_into(raw, &mut scratch);
            // Map alphabet output back to byte offsets inside `text`.
            // Since `scratch` is exactly `raw` lowercased, indices line up
            // byte-for-byte (ASCII).
            let mut cur_char = char_start;
            let mut cur_byte_in_raw = 0usize;
            for piece in pieces {
                // Locate the piece inside scratch by current byte cursor.
                let plen = piece.len();
                // The borrowed `piece` lives in `scratch`; we want a Cow that
                // borrows from `text` if `text[byte_start+cur..byte_start+cur+plen]`
                // matches the piece exactly (case-preserving). If it does, we
                // can borrow from input; otherwise it's an owned lowercase
                // copy.
                let src = &text[byte_start + cur_byte_in_raw..byte_start + cur_byte_in_raw + plen];
                let term: Cow<'a, str> = if src.bytes().eq(piece.bytes()) {
                    Cow::Borrowed(src)
                } else if cfg.lowercase {
                    Cow::Owned(piece.to_string())
                } else {
                    // Keep source case if not lowercasing.
                    Cow::Borrowed(src)
                };

                let take_chars = piece.chars().count();
                let end_char = if cfg.fixed_pinyin_offset {
                    cur_char + 1
                } else {
                    cur_char + take_chars
                };
                *position += 1;
                let s = byte_off
                    .get(cur_char)
                    .copied()
                    .unwrap_or(text.len() as u32);
                let e = byte_off.get(end_char).copied().unwrap_or(text.len() as u32);
                push_candidate(
                    cfg,
                    candidates,
                    terms_filter,
                    Candidate {
                        term,
                        start_offset: s,
                        end_offset: e,
                        position: *position,
                    },
                );
                cur_byte_in_raw += plen;
                cur_char = end_char;
                let _ = char_end;
            }
        } else if cfg.keep_first_letter
            || cfg.keep_separate_first_letter
            || cfg.keep_full_pinyin
            || !cfg.keep_none_chinese_in_joined_full_pinyin
        {
            *position += 1;
            let s = byte_off
                .get(char_start)
                .copied()
                .unwrap_or(text.len() as u32);
            let e = byte_off.get(char_end).copied().unwrap_or(text.len() as u32);
            push_candidate(
                cfg,
                candidates,
                terms_filter,
                Candidate {
                    term: Cow::Borrowed(&text[byte_start..byte_end]),
                    start_offset: s,
                    end_offset: e,
                    position: *position,
                },
            );
        }
    }
}

#[inline]
fn push_candidate<'a>(
    cfg: &PinyinConfig,
    candidates: &mut Vec<Candidate<'a>>,
    terms_filter: &mut HashSet<u64>,
    mut item: Candidate<'a>,
) {
    // Normalize: lowercase + trim (only allocate if a transform is needed).
    if cfg.lowercase {
        let needs_lower = item.term.bytes().any(|b| b.is_ascii_uppercase());
        if needs_lower {
            item.term = Cow::Owned(item.term.to_lowercase());
        }
    }
    if cfg.trim_whitespace {
        let trimmed = item.term.trim();
        if trimmed.len() != item.term.len() {
            item.term = Cow::Owned(trimmed.to_string());
        }
    }
    if item.term.is_empty() {
        return;
    }
    // Hash-based dedup: FNV-1a of term bytes + optional position discriminator.
    // Replaces the old per-candidate String allocation.
    let hash = if cfg.remove_duplicate_term {
        fnv1a_hash(item.term.as_bytes(), 0)
    } else {
        fnv1a_hash(item.term.as_bytes(), item.position)
    };
    if !terms_filter.insert(hash) {
        return;
    }
    candidates.push(item);
}

/// Borrow the first ASCII byte of a (Static) syllable as a `&'static str` when
/// possible, otherwise fall through to interning via the syllable table by
/// using the first letter directly from the `Cow`. Since Pinyin syllables
/// always start with one of 26 ASCII letters, we can route through a
/// pre-built static 1-byte slice table — but for simplicity we just slice
/// `pinyin_cow`. When the Cow is Owned, the resulting slice would have a
/// shorter lifetime than `'a`; in that case we allocate a 1-byte String.
fn first_letter_cow<'a>(pinyin_cow: &Cow<'a, str>) -> Cow<'a, str> {
    match pinyin_cow {
        Cow::Borrowed(s) => Cow::Borrowed(&s[..1]),
        Cow::Owned(s) => Cow::Owned(s[..1].to_string()),
    }
}

/// Coerce any `&str` to `&'static str` when its provenance is `SYLLABLES`.
/// Kept for completeness even though the tokenizer now uses Cows directly.
#[inline]
#[allow(dead_code)]
fn static_or_borrowed(s: &'static str) -> &'static str {
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn terms(toks: &[Token<'_>]) -> Vec<String> {
        toks.iter().map(|t| t.term.to_string()).collect()
    }

    #[test]
    fn default_config_liudehua() {
        let tk = PinyinTokenizer::with_defaults();
        let t = tk.tokenize("刘德华");
        let ts = terms(&t);
        assert!(ts.contains(&"liu".to_string()));
        assert!(ts.contains(&"de".to_string()));
        assert!(ts.contains(&"hua".to_string()));
        assert!(ts.contains(&"ldh".to_string()));
    }

    #[test]
    fn separate_first_letter_only() {
        let cfg = PinyinConfig {
            keep_first_letter: false,
            keep_separate_first_letter: true,
            keep_full_pinyin: false,
            ..PinyinConfig::default()
        };
        let tk = PinyinTokenizer::new(cfg);
        let t = tk.tokenize("刘德华");
        let ts = terms(&t);
        assert_eq!(ts, vec!["l", "d", "h"]);
    }

    #[test]
    fn joined_full_pinyin() {
        let cfg = PinyinConfig {
            keep_first_letter: false,
            keep_full_pinyin: false,
            keep_joined_full_pinyin: true,
            ..PinyinConfig::default()
        };
        let tk = PinyinTokenizer::new(cfg);
        let t = tk.tokenize("刘德华");
        assert!(terms(&t).contains(&"liudehua".to_string()));
    }

    #[test]
    fn rules_phrase_overrides_polyphone() {
        let rules = Rules::new().with_phrase("银行", &["yin", "hang"]);
        let tk = PinyinTokenizer::with_defaults().with_rules(rules);
        let t = tk.tokenize("银行家");
        let ts = terms(&t);
        // Without the rule the dict primary for 行 is 'xing'. With the rule
        // applied, 'hang' should now show up.
        assert!(ts.contains(&"hang".to_string()));
        assert!(ts.contains(&"yin".to_string()));
        assert!(ts.contains(&"jia".to_string()));
    }

    #[test]
    fn rules_char_override() {
        let rules = Rules::new().with_char('重', &["chong"]);
        let tk = PinyinTokenizer::with_defaults().with_rules(rules);
        let t = tk.tokenize("重");
        assert!(terms(&t).contains(&"chong".to_string()));
    }

    #[test]
    fn separate_chinese_borrows_from_input() {
        let cfg = PinyinConfig {
            keep_separate_chinese: true,
            ..PinyinConfig::default()
        };
        let tk = PinyinTokenizer::new(cfg);
        let text = String::from("刘德华");
        let t = tk.tokenize(&text);
        // At least one Chinese-char token should borrow directly from `text`.
        let has_borrowed_input = t.iter().any(|tk| {
            matches!(tk.term, Cow::Borrowed(s) if s.as_ptr() >= text.as_ptr()
                     && (s.as_ptr() as usize) < text.as_ptr() as usize + text.len())
        });
        assert!(has_borrowed_input, "no token borrowed from input");
    }

    #[test]
    fn syllables_are_borrowed_static() {
        let tk = PinyinTokenizer::with_defaults();
        let t = tk.tokenize("刘");
        let liu = t.iter().find(|tk| tk.term == "liu").unwrap();
        // `liu` should be a static borrow, not an owned alloc.
        assert!(matches!(liu.term, Cow::Borrowed(_)));
    }
}
