//! Normalizer integration with `pizza-engine`.
//!
//! [`PinyinNormalizer`] converts Chinese characters in the input text to their
//! pinyin representation in-place, before tokenization. Non-Chinese characters
//! are passed through unchanged.
//!
//! ## Modes
//!
//! - **Full pinyin** (default): `"刘德华"` → `"liu de hua"`
//! - **Joined**: `"刘德华"` → `"liudehua"`
//! - **First letter**: `"刘德华"` → `"ldh"`

use pizza_engine::analysis::{Normalizer, NormalizerClone};

use crate::dict::PinyinDict;

const CJK_START: char = '\u{4E00}';
const CJK_END: char = '\u{9FA5}';

#[inline]
fn is_chinese(c: char) -> bool {
    (CJK_START..=CJK_END).contains(&c)
}

/// Output mode for the pinyin normalizer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinyinNormalizeMode {
    /// Space-separated full pinyin: `"刘德华"` → `"liu de hua"`
    FullPinyin,
    /// Joined full pinyin (no separator): `"刘德华"` → `"liudehua"`
    JoinedPinyin,
    /// First letters only: `"刘德华"` → `"ldh"`
    FirstLetter,
}

/// Configuration for [`PinyinNormalizer`].
#[derive(Debug, Clone)]
pub struct PinyinNormalizeConfig {
    /// Output mode.
    pub mode: PinyinNormalizeMode,
    /// Separator between pinyin syllables (only used in `FullPinyin` mode).
    pub separator: &'static str,
    /// Lowercase the output.
    pub lowercase: bool,
}

impl Default for PinyinNormalizeConfig {
    fn default() -> Self {
        Self {
            mode: PinyinNormalizeMode::FullPinyin,
            separator: " ",
            lowercase: true,
        }
    }
}

/// Normalizer that converts Chinese characters to pinyin in-place.
///
/// Use this as a pre-tokenization step in an analysis chain. Non-Chinese
/// characters (ASCII, punctuation, etc.) are preserved as-is.
#[derive(Clone)]
pub struct PinyinNormalizer {
    config: PinyinNormalizeConfig,
}

impl PinyinNormalizer {
    pub fn new(config: PinyinNormalizeConfig) -> Self {
        Self { config }
    }

    /// Create a normalizer with default config (full pinyin, space-separated).
    pub fn with_defaults() -> Self {
        Self::new(PinyinNormalizeConfig::default())
    }
}

impl Normalizer for PinyinNormalizer {
    fn normalize(&self, text: &mut String) {
        // Quick scan: does the text contain any Chinese characters?
        if !text.chars().any(is_chinese) {
            // Apply lowercase if needed, otherwise nothing to do.
            if self.config.lowercase {
                text.make_ascii_lowercase();
            }
            return;
        }

        let mut out = String::with_capacity(text.len() * 2);
        let mut need_sep = false;

        for c in text.chars() {
            if is_chinese(c) {
                match self.config.mode {
                    PinyinNormalizeMode::FullPinyin => {
                        if let Some(pinyin) = PinyinDict::primary(c) {
                            if need_sep {
                                out.push_str(self.config.separator);
                            }
                            out.push_str(pinyin);
                            need_sep = true;
                        } else {
                            // Unknown character — pass through.
                            if need_sep {
                                out.push_str(self.config.separator);
                            }
                            out.push(c);
                            need_sep = true;
                        }
                    }
                    PinyinNormalizeMode::JoinedPinyin => {
                        if let Some(pinyin) = PinyinDict::primary(c) {
                            out.push_str(pinyin);
                        } else {
                            out.push(c);
                        }
                        need_sep = false;
                    }
                    PinyinNormalizeMode::FirstLetter => {
                        if let Some(pinyin) = PinyinDict::primary(c) {
                            out.push(pinyin.as_bytes()[0] as char);
                        } else {
                            out.push(c);
                        }
                        need_sep = false;
                    }
                }
            } else {
                // Non-Chinese character — pass through.
                if need_sep && self.config.mode == PinyinNormalizeMode::FullPinyin && !c.is_whitespace() {
                    out.push_str(self.config.separator);
                }
                out.push(c);
                need_sep = false;
            }
        }

        if self.config.lowercase {
            out.make_ascii_lowercase();
        }

        *text = out;
    }
}

impl NormalizerClone for PinyinNormalizer {
    fn clone_box(&self) -> Box<dyn Normalizer> {
        Box::new(self.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_full_pinyin() {
        let n = PinyinNormalizer::new(PinyinNormalizeConfig {
            mode: PinyinNormalizeMode::FullPinyin,
            separator: " ",
            lowercase: true,
        });
        let mut text = String::from("刘德华");
        n.normalize(&mut text);
        assert_eq!(text, "liu de hua");
    }

    #[test]
    fn test_joined_pinyin() {
        let n = PinyinNormalizer::new(PinyinNormalizeConfig {
            mode: PinyinNormalizeMode::JoinedPinyin,
            separator: " ",
            lowercase: true,
        });
        let mut text = String::from("刘德华");
        n.normalize(&mut text);
        assert_eq!(text, "liudehua");
    }

    #[test]
    fn test_first_letter() {
        let n = PinyinNormalizer::new(PinyinNormalizeConfig {
            mode: PinyinNormalizeMode::FirstLetter,
            separator: "",
            lowercase: true,
        });
        let mut text = String::from("刘德华");
        n.normalize(&mut text);
        assert_eq!(text, "ldh");
    }

    #[test]
    fn test_mixed_content() {
        let n = PinyinNormalizer::with_defaults();
        let mut text = String::from("hello刘德华world");
        n.normalize(&mut text);
        assert_eq!(text, "hello liu de hua world");
    }

    #[test]
    fn test_pure_ascii_unchanged() {
        let n = PinyinNormalizer::with_defaults();
        let mut text = String::from("hello world");
        n.normalize(&mut text);
        assert_eq!(text, "hello world");
    }
}
