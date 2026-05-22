//! Register Pinyin analysis components into [`AnalysisFactory`].

use alloc::boxed::Box;
use alloc::vec;

use pizza_engine::analysis::AnalysisFactory;
use pizza_engine::analysis::Analyzer;

use crate::{PinyinConfig, PinyinNormalizeConfig, PinyinNormalizeMode, PinyinNormalizer, PinyinTokenizer};

/// Register Pinyin tokenizers, normalizers, and analyzers.
///
/// Matches Elasticsearch's analysis-pinyin plugin registration:
/// - Tokenizer: `pinyin` (splits Chinese text into pinyin tokens)
/// - Normalizer: `pinyin` (converts Chinese characters to pinyin in-place)
/// - Analyzer: `pinyin` (PinyinAnalyzer = just the PinyinTokenizer)
///
/// Note: The Java plugin also registers a `pinyin` token filter and
/// `pinyin_first_letter` tokenizer. The token filter is not yet implemented
/// in the Rust crate; `pinyin_first_letter` is provided via config.
pub fn register_all(factory: &mut AnalysisFactory) {
    // Tokenizers
    factory.register_tokenizer("pinyin", Box::new(PinyinTokenizer::new(PinyinConfig::default())));

    // Normalizer: converts Chinese chars to pinyin (pre-tokenization, like a char_filter)
    factory.register_normalizer("pinyin", Box::new(PinyinNormalizer::with_defaults()));
    factory.register_normalizer(
        "pinyin_first_letter",
        Box::new(PinyinNormalizer::new(PinyinNormalizeConfig {
            mode: PinyinNormalizeMode::FirstLetter,
            ..PinyinNormalizeConfig::default()
        })),
    );

    // Analyzer: pinyin (just the tokenizer, matches Java PinyinAnalyzer)
    factory.register_analyzer(
        "pinyin",
        Analyzer::new(vec![], Box::new(PinyinTokenizer::new(PinyinConfig::default())), vec![]),
    );
}
