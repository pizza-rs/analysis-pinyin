//! Configuration for [`crate::PinyinTokenizer`].
//!
//! Mirrors the options exposed by the Java `analysis-pinyin` plugin's
//! `PinyinConfig` class so the behaviour stays compatible.

#[derive(Debug, Clone)]
pub struct PinyinConfig {
    /// Lowercase every emitted term.
    pub lowercase: bool,
    /// Trim leading/trailing whitespace from every emitted term.
    pub trim_whitespace: bool,
    /// Emit non-Chinese characters (ASCII letters / digits) as tokens.
    pub keep_none_chinese: bool,
    /// Include non-Chinese characters in the joined first-letter token.
    pub keep_none_chinese_in_first_letter: bool,
    /// Include non-Chinese characters in the joined full-pinyin token.
    pub keep_none_chinese_in_joined_full_pinyin: bool,
    /// Emit the original input as a token.
    pub keep_original: bool,
    /// Emit the joined first letters (e.g. `刘德华` -> `ldh`) as a token.
    pub keep_first_letter: bool,
    /// Emit each first letter as a separate token (e.g. `刘德华` -> `l`, `d`, `h`).
    pub keep_separate_first_letter: bool,
    /// Keep consecutive non-Chinese characters in a single token instead of
    /// splitting them character by character.
    pub keep_none_chinese_together: bool,
    /// Re-segment a chunk of consecutive ASCII letters as if it were pinyin
    /// (`liudehua` -> `liu`, `de`, `hua`).
    pub none_chinese_pinyin_tokenize: bool,
    /// Max length of the joined first-letter token. `0` disables truncation.
    pub limit_first_letter_length: usize,
    /// Emit each character's full pinyin (e.g. `刘` -> `liu`).
    pub keep_full_pinyin: bool,
    /// Emit a single joined full-pinyin token (e.g. `刘德华` -> `liudehua`).
    pub keep_joined_full_pinyin: bool,
    /// Drop duplicate terms regardless of position.
    pub remove_duplicate_term: bool,
    /// Use fixed (1-char) offsets when re-tokenizing ASCII pinyin strings.
    pub fixed_pinyin_offset: bool,
    /// When true (default), do not attach real offsets to pinyin tokens
    /// (matching the Java plugin's relaxed-offset mode).
    pub ignore_pinyin_offset: bool,
    /// Emit each Chinese character as its own token alongside its pinyin.
    pub keep_separate_chinese: bool,
}

impl Default for PinyinConfig {
    fn default() -> Self {
        Self {
            lowercase: true,
            trim_whitespace: true,
            keep_none_chinese: true,
            keep_none_chinese_in_first_letter: true,
            keep_none_chinese_in_joined_full_pinyin: false,
            keep_original: false,
            keep_first_letter: true,
            keep_separate_first_letter: false,
            keep_none_chinese_together: true,
            none_chinese_pinyin_tokenize: true,
            limit_first_letter_length: 16,
            keep_full_pinyin: true,
            keep_joined_full_pinyin: false,
            remove_duplicate_term: false,
            fixed_pinyin_offset: false,
            ignore_pinyin_offset: true,
            keep_separate_chinese: false,
        }
    }
}

impl PinyinConfig {
    /// Validate the configuration — at least one output kind must be enabled.
    pub fn validate(&self) -> Result<(), &'static str> {
        if !(self.keep_first_letter
            || self.keep_separate_first_letter
            || self.keep_full_pinyin
            || self.keep_joined_full_pinyin
            || self.keep_separate_chinese)
        {
            return Err(
                "pinyin config error: can't disable separate_first_letter, first_letter, \
                 full_pinyin, joined_full_pinyin and separate_chinese at the same time.",
            );
        }
        Ok(())
    }
}
