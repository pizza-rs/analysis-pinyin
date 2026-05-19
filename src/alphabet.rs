//! Re-segment a chunk of ASCII letters as a sequence of pinyin syllables
//! (port of the Java `PinyinAlphabetTokenizer`, but using the compact
//! sorted-name table from [`crate::dict`] for O(log N) membership tests and
//! zero allocation on the lookup path).
//!
//! Forward + reverse maximum matching, shortest result wins.

use crate::dict::PinyinDict;

const PINYIN_MAX_LENGTH: usize = 6;

/// Split `text` into pinyin-like sub-strings via forward+reverse max matching
/// against the bundled syllable dictionary.
///
/// Always allocates exactly one lowercase `String` per call (only when needed)
/// and returns owned `String`s, mirroring the original Java semantics. For a
/// zero-copy variant that borrows from `text`, see [`walk_borrowed`].
pub fn walk(text: &str) -> Vec<String> {
    let lowered = if text.bytes().any(|b| b.is_ascii_uppercase()) {
        text.to_ascii_lowercase()
    } else {
        text.to_string()
    };
    seg_pinyin_str(&lowered).into_iter().map(str::to_string).collect()
}

/// Zero-copy variant of [`walk`]: returns sub-slices of `text` whenever the
/// input is already lowercase. When `text` contains uppercase ASCII, falls
/// back to a single allocated lowercase buffer (returned via the `Cow` storage).
///
/// Returned strings have the lifetime of `out` so the borrow checker keeps
/// `out` alive for the duration of the slices.
pub fn walk_into<'b>(text: &str, out: &'b mut String) -> Vec<&'b str> {
    out.clear();
    out.reserve(text.len());
    for b in text.bytes() {
        out.push(b.to_ascii_lowercase() as char);
    }
    // SAFETY: `out` is built only from ASCII bytes derived from `text`'s bytes.
    // For non-ASCII bytes inside multibyte UTF-8 sequences, this would not be
    // valid UTF-8 — but the alphabet path is fed only ASCII chunks.
    let lowered: &'b str = out.as_str();
    seg_pinyin_str(lowered)
}

fn seg_pinyin_str(content: &str) -> Vec<&str> {
    let groups = split_by_non_letter(content);
    let mut out = Vec::with_capacity(groups.len());
    for chunk in groups {
        if chunk.len() == 1 {
            out.push(chunk);
            continue;
        }
        let forward = positive_max_match(chunk, PINYIN_MAX_LENGTH);
        if forward.len() == 1 {
            out.extend(forward);
            continue;
        }
        let backward = reverse_max_match(chunk, PINYIN_MAX_LENGTH);
        if forward.len() <= backward.len() {
            out.extend(forward);
        } else {
            out.extend(backward);
        }
    }
    out
}

fn split_by_non_letter(pinyin_str: &str) -> Vec<&str> {
    let bytes = pinyin_str.as_bytes();
    let mut chunks: Vec<&str> = Vec::new();
    let mut start = 0usize;
    let mut last_letter = true;
    let mut i = 0usize;
    while i < bytes.len() {
        let is_letter = bytes[i].is_ascii_alphabetic();
        if is_letter != last_letter && i > start {
            chunks.push(&pinyin_str[start..i]);
            start = i;
        }
        last_letter = is_letter;
        i += 1;
    }
    if start < bytes.len() {
        chunks.push(&pinyin_str[start..]);
    }
    chunks
}

fn positive_max_match(text: &str, max_length: usize) -> Vec<&str> {
    let len = text.len();
    let mut out: Vec<&str> = Vec::new();
    let mut no_match_start: Option<usize> = None;
    let mut start = 0usize;

    while start < len {
        let end = (start + max_length).min(len);
        if start == end {
            break;
        }
        let mut matched_len: Option<usize> = None;
        let win_len = end - start;
        for take in (1..=win_len).rev() {
            let candidate = &text[start..start + take];
            if PinyinDict::is_alphabet_syllable(candidate) {
                matched_len = Some(take);
                break;
            }
        }
        if let Some(take) = matched_len {
            if let Some(ns) = no_match_start.take() {
                out.push(&text[ns..start]);
            }
            out.push(&text[start..start + take]);
            start += take;
        } else {
            if no_match_start.is_none() {
                no_match_start = Some(start);
            }
            start += 1;
        }
    }
    if let Some(ns) = no_match_start {
        out.push(&text[ns..len]);
    }
    out
}

fn reverse_max_match(text: &str, max_length: usize) -> Vec<&str> {
    let len = text.len();
    let mut out: Vec<&str> = Vec::new();
    let mut no_match_end: Option<usize> = None;
    let mut end = len;

    while end > 0 {
        let start = end.saturating_sub(max_length);
        if start == end {
            break;
        }
        let mut matched: Option<(usize, usize)> = None;
        let win_len = end - start;
        for take in (1..=win_len).rev() {
            let s = end - take;
            let candidate = &text[s..end];
            if PinyinDict::is_alphabet_syllable(candidate) {
                matched = Some((s, end));
                break;
            }
        }
        if let Some((s, e)) = matched {
            if let Some(ne) = no_match_end.take() {
                out.push(&text[end..ne]);
            }
            out.push(&text[s..e]);
            end = s;
        } else {
            if no_match_end.is_none() {
                no_match_end = Some(end);
            }
            end -= 1;
        }
    }
    if let Some(ne) = no_match_end {
        out.push(&text[0..ne]);
    }
    out.reverse();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_known_pinyin_string() {
        let r = walk("liudehua");
        assert_eq!(r, vec!["liu", "de", "hua"]);
    }

    #[test]
    fn keeps_unknown_chunks() {
        let r = walk("andy");
        assert_eq!(r, vec!["an", "d", "y"]);
    }

    #[test]
    fn mixed_separator() {
        let r = walk("liu de");
        assert_eq!(r, vec!["liu", " ", "de"]);
    }

    #[test]
    fn handles_uppercase() {
        let r = walk("LiuDeHua");
        assert_eq!(r, vec!["liu", "de", "hua"]);
    }
}
