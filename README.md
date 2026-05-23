<div align="center">

# 🔤 pizza-analysis-pinyin

**Chinese Pinyin conversion plugin for [INFINI Pizza](https://pizza.rs)**

[![Crate](https://img.shields.io/badge/crate-pizza--analysis--pinyin-blue)](https://github.com/pizza-rs/analysis-pinyin)
[![License](https://img.shields.io/badge/license-MIT-blue)](LICENSE)

</div>

---

## Overview

`pizza-analysis-pinyin` provides Chinese → Pinyin romanization for the [INFINI Pizza](https://pizza.rs) search engine. Ported from [infinilabs/analysis-pinyin](https://github.com/infinilabs/analysis-pinyin).

### Key Features

- **Full Pinyin** — Convert Chinese characters to complete Pinyin syllables
- **First Letter** — Extract first letter of each syllable (e.g., "北京" → "bj")
- **Polyphone Disambiguation** — Context-aware pronunciation selection
- **Tone Options** — With or without tone numbers/marks
- **Mixed Mode** — Output both original Chinese and Pinyin simultaneously
- **Zero-Copy** — Minimal allocations using `Cow<str>`

## Components

| Type | Name | Description |
|:-----|:-----|:------------|
| Tokenizer | `pinyin` | Chinese → Pinyin tokenizer |
| Filter | `pinyin` | Per-token Pinyin conversion |
| Normalizer | `pinyin` | Full-text Pinyin conversion |
| Normalizer | `pinyin_first_letter` | First-letter extraction |
| Analyzer | `pinyin` | pinyin tokenizer → lowercase |

## Example

```text
Input:   "中国人民"
Pinyin:  ["zhong", "guo", "ren", "min"]
First:   ["z", "g", "r", "m"]
```

## Installation

```toml
[dependencies]
pizza-analysis-pinyin = "0.1"
```

Or via `pizza-analysis-all`:

```toml
[dependencies]
pizza-analysis-all = { version = "0.1", features = ["pinyin"] }
```

## Usage

```rust
use pizza_engine::analysis::AnalysisFactory;

let mut factory = AnalysisFactory::new();
pizza_analysis_pinyin::register_all(&mut factory);
```

## License

MIT

---

<div align="center">
<sub>Part of the <a href="https://pizza.rs">INFINI Pizza</a> ecosystem</sub>
</div>
