// MIT License
//
// Copyright (C) INFINI Labs & INFINI LIMITED. <hello@infini.ltd>
//
// Pinyin analyzer/tokenizer for Pizza, ported from the Java
// `infinilabs/analysis-pinyin` plugin.

// Tiny, scoped `unsafe` usage in `dict::homophone_chars` (sound u32→char cast
// — every value was produced by `c as u32`). All other modules are unsafe-free.
#![deny(unsafe_op_in_unsafe_fn)]

pub mod dict;

mod alphabet;
mod config;
#[cfg(feature = "polyphone-dict")]
mod polyphone_dict;
mod rules;
mod tokenizer;

pub use config::PinyinConfig;
pub use dict::{PinyinDict, SyllableId};
pub use rules::{Reading, Rules};
pub use tokenizer::PinyinTokenizer;

/// Re-export of the alphabet re-segmentation helper, in case callers want to
/// reuse the pure pinyin-string max-matching outside the tokenizer.
pub use alphabet::walk as segment_pinyin_alphabet;
