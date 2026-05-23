//! Comprehensive tests for pizza-analysis-pinyin (Chinese Pinyin analysis).

use pizza_analysis_pinyin::{
    PinyinConfig, PinyinDict, PinyinNormalizeConfig, PinyinNormalizeMode, PinyinNormalizer,
    PinyinTokenizer, Reading, Rules,
};
use pizza_engine::analysis::{Normalizer, Token, Tokenizer};

// ═══════════════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════════════

fn terms(tokens: &[Token]) -> Vec<String> {
    tokens.iter().map(|t| t.term.to_string()).collect()
}

// ═══════════════════════════════════════════════════════════════════════════════
// PinyinDict — static dictionary lookups
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn dict_primary_common_chars() {
    assert_eq!(PinyinDict::primary('中'), Some("zhong"));
    assert_eq!(PinyinDict::primary('国'), Some("guo"));
    assert_eq!(PinyinDict::primary('人'), Some("ren"));
}

#[test]
fn dict_primary_non_cjk_returns_none() {
    assert_eq!(PinyinDict::primary('A'), None);
    assert_eq!(PinyinDict::primary('1'), None);
    assert_eq!(PinyinDict::primary(' '), None);
}

#[test]
fn dict_primary_id_valid() {
    let id = PinyinDict::primary_id('中');
    assert!(id.is_some());
    let sid = id.unwrap();
    assert_eq!(sid.as_str(), "zhong");
}

#[test]
fn dict_primary_id_non_cjk() {
    assert!(PinyinDict::primary_id('!').is_none());
}

// ═══════════════════════════════════════════════════════════════════════════════
// PinyinConfig — validation
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn config_default_valid() {
    let cfg = PinyinConfig::default();
    assert!(cfg.validate().is_ok());
}

#[test]
fn config_all_outputs_disabled_invalid() {
    let cfg = PinyinConfig {
        keep_first_letter: false,
        keep_separate_first_letter: false,
        keep_full_pinyin: false,
        keep_joined_full_pinyin: false,
        keep_separate_chinese: false,
        ..PinyinConfig::default()
    };
    assert!(cfg.validate().is_err());
}

// ═══════════════════════════════════════════════════════════════════════════════
// PinyinTokenizer — construction
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn tokenizer_with_defaults() {
    let _t = PinyinTokenizer::with_defaults();
}

#[test]
fn tokenizer_with_config() {
    let _t = PinyinTokenizer::new(PinyinConfig::default());
}

#[test]
fn tokenizer_clone() {
    let t1 = PinyinTokenizer::with_defaults();
    let _t2 = t1.clone();
}

#[test]
fn tokenizer_config_access() {
    let t = PinyinTokenizer::with_defaults();
    let cfg = t.config();
    assert!(cfg.keep_first_letter);
}

// ═══════════════════════════════════════════════════════════════════════════════
// PinyinTokenizer — basic tokenization
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn tokenize_chinese_produces_pinyin() {
    let t = PinyinTokenizer::with_defaults();
    let tokens = t.tokenize("中国");
    let ts = terms(&tokens);
    // Should contain pinyin like "zhong", "guo" or first letters "zg"
    assert!(!ts.is_empty());
    // At least one token should be pinyin-like (ASCII)
    assert!(ts.iter().any(|s| s.chars().all(|c| c.is_ascii_alphabetic())));
}

#[test]
fn tokenize_single_chinese_char() {
    let t = PinyinTokenizer::with_defaults();
    let tokens = t.tokenize("中");
    assert!(!tokens.is_empty());
}

#[test]
fn tokenize_produces_first_letter() {
    let cfg = PinyinConfig {
        keep_first_letter: true,
        keep_full_pinyin: false,
        keep_separate_first_letter: false,
        keep_joined_full_pinyin: false,
        keep_separate_chinese: false,
        ..PinyinConfig::default()
    };
    let t = PinyinTokenizer::new(cfg);
    let tokens = t.tokenize("刘德华");
    let ts = terms(&tokens);
    // Should produce "ldh" or similar first-letter token
    assert!(ts.iter().any(|s| s.len() <= 16 && s.chars().all(|c| c.is_ascii_alphabetic())));
}

#[test]
fn tokenize_ascii_passthrough() {
    let t = PinyinTokenizer::with_defaults();
    let tokens = t.tokenize("hello");
    let ts = terms(&tokens);
    assert!(ts.iter().any(|s| s == "hello"));
}

// ═══════════════════════════════════════════════════════════════════════════════
// PinyinTokenizer — edge cases
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn tokenize_empty_string() {
    let t = PinyinTokenizer::with_defaults();
    let tokens = t.tokenize("");
    assert!(tokens.is_empty());
}

#[test]
fn tokenize_whitespace_only() {
    let t = PinyinTokenizer::with_defaults();
    let tokens = t.tokenize("   ");
    // May or may not produce tokens; should not panic
}

#[test]
fn tokenize_mixed_chinese_ascii() {
    let t = PinyinTokenizer::with_defaults();
    let tokens = t.tokenize("Hello中国World");
    assert!(!tokens.is_empty());
}

#[test]
fn tokenize_digits() {
    let t = PinyinTokenizer::with_defaults();
    let _tokens = t.tokenize("123");
    // Digits handled based on keep_none_chinese config
}

// ═══════════════════════════════════════════════════════════════════════════════
// PinyinTokenizer — offsets
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn tokenize_offsets_valid() {
    let t = PinyinTokenizer::with_defaults();
    let text = "中国人民";
    let tokens = t.tokenize(text);
    for tok in &tokens {
        assert!(tok.start_offset <= tok.end_offset);
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// PinyinNormalizer — construction
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn normalizer_with_defaults() {
    let _n = PinyinNormalizer::with_defaults();
}

#[test]
fn normalizer_clone() {
    let n1 = PinyinNormalizer::with_defaults();
    let _n2 = n1.clone();
}

// ═══════════════════════════════════════════════════════════════════════════════
// PinyinNormalizer — normalization modes
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn normalize_full_pinyin() {
    let n = PinyinNormalizer::new(PinyinNormalizeConfig {
        mode: PinyinNormalizeMode::FullPinyin,
        separator: " ",
        lowercase: true,
    });
    let mut text = "刘德华".to_string();
    n.normalize(&mut text);
    // Should contain space-separated pinyin
    assert!(text.contains("liu"));
    assert!(text.contains("de"));
    assert!(text.contains("hua"));
}

#[test]
fn normalize_joined_pinyin() {
    let n = PinyinNormalizer::new(PinyinNormalizeConfig {
        mode: PinyinNormalizeMode::JoinedPinyin,
        separator: " ",
        lowercase: true,
    });
    let mut text = "刘德华".to_string();
    n.normalize(&mut text);
    assert_eq!(text, "liudehua");
}

#[test]
fn normalize_first_letter() {
    let n = PinyinNormalizer::new(PinyinNormalizeConfig {
        mode: PinyinNormalizeMode::FirstLetter,
        separator: " ",
        lowercase: true,
    });
    let mut text = "刘德华".to_string();
    n.normalize(&mut text);
    assert_eq!(text, "ldh");
}

#[test]
fn normalize_ascii_passthrough() {
    let n = PinyinNormalizer::with_defaults();
    let mut text = "hello world".to_string();
    n.normalize(&mut text);
    assert_eq!(text, "hello world");
}

#[test]
fn normalize_empty_string() {
    let n = PinyinNormalizer::with_defaults();
    let mut text = String::new();
    n.normalize(&mut text);
    assert!(text.is_empty());
}

#[test]
fn normalize_mixed_content() {
    let n = PinyinNormalizer::new(PinyinNormalizeConfig {
        mode: PinyinNormalizeMode::FullPinyin,
        separator: " ",
        lowercase: true,
    });
    let mut text = "Hello中国World".to_string();
    n.normalize(&mut text);
    // Should contain pinyin for 中国 mixed with ASCII
    assert!(text.contains("zhong"));
}

// ═══════════════════════════════════════════════════════════════════════════════
// Rules — custom overrides
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn rules_new() {
    let _r = Rules::new();
}

#[test]
fn tokenizer_with_rules() {
    let rules = Rules::new();
    let t = PinyinTokenizer::with_defaults().with_rules(rules);
    let _r = t.rules();
}

// ═══════════════════════════════════════════════════════════════════════════════
// Registration
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn register_all_does_not_panic() {
    let mut factory = pizza_engine::analysis::AnalysisFactory::new();
    pizza_analysis_pinyin::register_all(&mut factory);
}

// ═══════════════════════════════════════════════════════════════════════════════
// Unicode handling
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn tokenize_cjk_extension_chars() {
    let t = PinyinTokenizer::with_defaults();
    // Should not panic on rare CJK characters
    let _tokens = t.tokenize("𠀀");
}

#[test]
fn tokenize_emoji_mixed() {
    let t = PinyinTokenizer::with_defaults();
    let _tokens = t.tokenize("你好😊世界");
}

#[test]
fn tokenize_japanese_hiragana() {
    let t = PinyinTokenizer::with_defaults();
    // Hiragana are not CJK ideographs — should pass through
    let _tokens = t.tokenize("こんにちは");
}
