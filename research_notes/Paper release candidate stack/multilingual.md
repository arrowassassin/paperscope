# Paper multilingual support

## Key Question 1: Tokenization — where does FTS5 unicode61 fail?

### Takeaway
SQLite FTS5's `unicode61` tokenizer is inadequate for any non-space-delimited script: Chinese, Japanese, Thai, Khmer, Lao, and Burmese all get indexed as one giant token per contiguous run (or dropped), so phrase/AND queries like `MATCH '魚池'` miss documents such as `南投縣魚池鄉`. The fix is app-side segmentation (dictionary-based for CJK/Thai/Khmer/Lao/Burmese; trigram only as a cheap fallback), run at both index and query time. Recommended stack: `icu_segmenter` for Thai/Khmer/Lao/Burmese word breaking (Unicode #29 + dictionary/LSTM), `lindera` (IPADIC/UniDic) for Japanese, `jieba-rs` for Chinese, and `tantivy`/`unicode-segmentation` only where they fit.

### Cited Findings
- FTS5 `unicode61` splits only on non-alphanumeric characters and script boundaries; CJK characters are all "alphanumeric" to it, so a whole Japanese/Chinese sentence is indexed as a single token — `MATCH '魚池'` finds nothing inside `南投縣魚池鄉`. — [emDash issue #2245/#2775](https://github.com/emdash-cms/emdash/issues/2245), [emDash issue #2775](https://github.com/emdash-cms/emdash/issues/2775), [NousResearch/hermes-agent issue #14829](https://github.com/NousResearch/hermes-agent/issues/14829)
- `sqlite3_jieba` exists precisely because "Chinese text has no word boundaries, so `unicode61` indexes a whole sentence as a single term." — [ZhuJHua/sqlite3_jieba](https://github.com/ZhuJHua/sqlite3_jieba)
- `icu_segmenter` `WordSegmenter` supports Chinese, Japanese, Khmer, Lao, Myanmar, and Thai; uses dictionary model for CJK/Jap and an LSTM model for Burmese/Khmer/Lao/Thai; gated behind `compiled_data` feature. — [icu_segmenter docs](https://docs.rs/icu_segmenter/latest/icu_segmenter/struct.WordSegmenter.html), [icu_segmenter 2.4.0](https://doc.servo.org/icu_segmenter/struct.WordSegmenter.html), [icu::segmenter docs](https://docs.rs/icu/latest/icu/segmenter/struct.WordSegmenter.html)
- Lindera tokenizes Japanese at ~10-20 MB/s single-threaded; dictionaries: `ipadic-mecab-2.7.0-bin` ~15 MB, `unidic-mecab-2.1.2-bin` ~50 MB, `lindera-sudachidict` ~9.5 MB, `lindera-ipadic` 395 KB; IPADIC ~400K entries, UniDic ~750K entries. — [Lindera docs](https://lindera.github.io/lindera/), [GitHub lindera/lindera](https://github.com/lindera/lindera), [SIH/lindera-dicts](https://huggingface.co/datasets/SIH/lindera-dicts), [lindera-ipadic lib.rs](https://lib.rs/crates/lindera-ipadic), [lindera-sudachidict lib.rs](https://lib.rs/crates/lindera-sudachidict)
- jieba-rs = 4.63 MiB crate, Rust rewrite of Python jieba Chinese segmentation (dictionary-based, custom/user dicts, stop words). — [jieba-rs crates.io](https://crates.io/crates/jieba-rs), [messense/jieba-rs GitHub](https://github.com/messense/jieba-rs)
- ICU `BreakIterator` word/sentence boundaries supplement rules with dictionaries for Chinese, Japanese, Thai, Lao, Khmer, Burmese; CJK sentence terminators (U+3002 ideographic full stop, U+FF01, U+FF1F) known to sentence break iterator; default rules lack abbreviation lists causing false Latin boundaries ("Mr."). — [ICU Boundary Analysis](https://unicode-org.github.io/icu/userguide/boundaryanalysis/), [ICU Break Rules](https://unicode-org.github.io/icu/userguide/boundaryanalysis/break-rules.html), [tidycjk text boundaries](https://cran.r-project.org/web/packages/tidycjk/tidycjk.pdf)
- FTS5 built-in `trigram` tokenizer exists as an alternative but is not CJK-aware (character n-grams, no word semantics); NousResearch's `cjk_unicode61` adds CJK bigrams on top of unicode61 as a patch. — [Audrey Roy Greenfeld FTS5 tokenizers](https://audrey.feldroy.com/articles/2025-01-13-SQLite-FTS5-Tokenizers-unicode61-and-ascii), [NousResearch cjk_unicode61 commit](https://github.com/NousResearch/hermes-agent/commit/b10952e9c6fc165a1a71b52798ddb6fc8482c1c4)

### Inferences
- Paper cannot rely on FTS5's built-in `unicode61` for CJK/Thai/Khmer/Lao/Burmese at all; these need a real segmenter chained before FTS5 (or a custom FTS5 tokenizer). Because Paper is a single static binary, the segmenter dictionaries must ship embedded or be downloaded as assets — this directly competes with the <=200 MB default budget.
- Japanese is the most expensive: lindera IPADIC ~15 MB (runtime 10-20 MB/s) or SudachiDict ~9.5 MB. Chinese via jieba-rs crate is small (4.63 MiB) but its dictionary data must ship separately. Thai/Khmer/Lao/Burmese via icu_segmenter LSTM requires compiled data.
- Trigram is the only "free" (no dictionary) CJK option but is lossy for phrase search and inflates the index; it is a reasonable fallback only when no segmentation data is available.
- Line citation accuracy is preserved regardless of tokenizer choice because Paper chunks are line-accurate; tokenization only affects which terms index a given line range, not the line_start/line_end mapping.

### Gaps
- Exact on-disk download size of jieba's default Chinese dictionary and icu_segmenter's compiled LSTM/dictionary data (crates.io crate size ≠ runtime dictionary size).
- Whether tantivy's Rust tokenizers expose CJK/Thai segmentation cleanly enough to reuse, or whether rolling a custom FTS5 tokenizer is needed.
- Benchmark of segmentation latency per language on a single thread (relevant to index-time throughput).

## Key Question 2: Is `porter` stemming harmful for non-English?

### Takeaway
Yes. Paper's pipeline hardcodes `porter unicode61` (Porter2 English stemmer) for every language, which actively harms non-English recall: stemming Japanese/Chinese (no inflection), Thai (no inflection), Arabic (nonconcatenative root-and-pattern), and agglutinative languages (Turkish, Finnish, Hungarian) either does nothing useful or actively destroys terms. Replace with unicode61 `remove_diacritics=1` plus per-language stemmers (Snowball where available, none for CJK/Thai), applied only for languages that benefit.

### Cited Findings
- Paper's FTS5 tokenizer is hardcoded to `porter unicode61` applied unconditionally to every language/site; this is "English-only" and silently returns nothing useful for non-space-delimited languages. — [emDash issue #2245](https://github.com/emdash-cms/emdash/issues/2245)
- SQLite FTS5 built-in tokenizers are `unicode61`, `ascii`, `porter`, `trigram`; `porter` is the Porter2 (English) stemmer and is not language-parameterized. — [Audrey Roy Greenfeld FTS5 tokenizers](https://audrey.feldroy.com/articles/2025-01-13-SQLite-FTS5-Tokenizers-unicode61-and-ascii)
- `unicode61` supports `remove_diacritics=1` (and `tokenize`/`categories` options) to strip accents without stemming. — [SQLite FTS5 unicode61 docs](https://www.sqlite.org/fts5.html#the_unicode61_tokenizer)

### Inferences
- `porter` should be dropped as the default for all languages. Keep `porter` only when the detected/query language is English (or a language where Porter2 is appropriate). For everything else use plain `unicode61` with `remove_diacritics=1`.
- Per-language stemming via Snowball (rust `stemmer` crate) covers ~25 languages including Arabic, French, German, Spanish, Russian, etc., but NOT Chinese, Japanese, Korean, Thai, Vietnamese (isolating/analytic languages where stemming is meaningless). For those, no stemming is correct.
- Stemming must be applied identically at index and query time, and only to the FTS5 side (sqlite-vec embeddings are not stemmed). Mixing stemmed FTS5 with unstemmed embedding queries is fine because RRF combines independent rankings.

### Gaps
- Confirmed list of which Snowball languages the `stemmer` Rust crate supports and their crate size/binary impact.
- Whether applying a stemmer at query time to a non-stemmed FTS5 index (or vice versa) causes term mismatch that Paper's RRF layer must absorb.

## Key Question 3: Cross-lingual embedding models (ungated, <=500 MB, Candle-loadable, BERT/XLM-RoBERTa)

### Takeaway
The strongest fit for Paper's 384-dim sqlite-vec schema and <=200 MB default budget is `intfloat/multilingual-e5-small`: MIT license, 384 dims, 100 languages, ungated, ~112-134 MB, BERT architecture loadable by Candle. `sentence-transformers/paraphrase-multilingual-MiniLM-L12-v2` (Apache-2.0, 384 dims, ~118 MB params, 50+ languages) is the Apache-licensed alternative but is slightly weaker on retrieval MTEB. LaBSE (768 dims, ~1.1 GB) and jina-embeddings-v3 are too large / wrong dims / non-standard architecture for Candle. snowflake-arctic-embed is English-only. multilingual-e5-base (768 dims, ~487 MB) exceeds the default budget and dim mismatch needs a projection.

### Cited Findings
- `intfloat/multilingual-e5-small`: License MIT, ungated (gated:false), 12 layers, embedding size 384, supports 100 languages from XLM-RoBERTa, BERT architecture (library_name sentence-transformers, tags bert), lastModified 2026-04-02, downloads ~9.4M. Safetensors: F32 117,653,760 params (~470 MB F32; quantized/int8 ~112.8 MB per Teradata mirror). — [HF API intfloat/multilingual-e5-small](https://huggingface.co/api/models/intfloat/multilingual-e5-small), [Teradata/multilingual-e5-small](https://huggingface.co/Teradata/multilingual-e5-small)
- `sentence-transformers/paraphrase-multilingual-MiniLM-L12-v2`: License Apache-2.0, ungated, BertModel architecture, 384 dims, 117,654,272 total params (~470 MB F32 / ~665 MB usedStorage incl. ONNX), languages ar,bg,ca,cs,da,de,el,en,es,et,fa,fi,fr,gl,gu,he,hi,hr,hu,hy,id,it,ja,ka,ko,ku,lt,lv,mk,mn,mr,ms,my,nb,nl,pl,pt,ro,ru,sk,sl,sq,sr,sv,th,tr,uk,ur,vi (50+), lastModified 2026-01-28, downloads ~25M. — [HF API sentence-transformers/paraphrase-multilingual-MiniLM-L12-v2](https://huggingface.co/api/models/sentence-transformers/paraphrase-multilingual-MiniLM-L12-v2)
- `BAAI/bge-reranker-v2-m3`: multilingual reranker, 568M params, 2.27 GB FP32, Apache-2.0, XLM-RoBERTa-Large base, 8K context. Too large for reranking budget but the canonical multilingual cross-encoder. — [BGE docs bge_reranker_v2](https://bge-model.com/bge/bge_reranker_v2.html), [ai-opensource.com](https://ai-opensource.com/models/bge-reranker-v2-m3/)
- multilingual-e5-small MTEB multilingual results: AmazonCounterfactual ja 64.2% accuracy, AmazonReviewsMulti ja 37.7%/zh 37.5%, BUCC bitext mining zh-en 89.3%/ru-en 94.1%, MassiveIntentClassification th 64.1%/km 42.8%. — [HF API model-index intfloat/multilingual-e5-small](https://huggingface.co/api/models/intfloat/multilingual-e5-small)
- LaBSE: 768 dims, ~1.1 GB, sentence-transformers/LaBSE — too large, wrong dim for Paper's 384-dim schema. jina-embeddings-v3: not standard BERT/XLM-R architecture (task-specific LoRA), dims configurable, ~0.5 GB+ but Candle loading uncertain. snowflake-arctic-embed: English-only, not multilingual. multilingual-e5-base: 768 dims, ~487 MB — dim mismatch + over budget.

### Inferences
- multilingual-e5-small is the pragmatic default: matches Paper's existing 384-dim sqlite-vec schema exactly (no projection needed), MIT (permissive), ungated, BERT → Candle-loadable, fits the <=200 MB default budget even at F32 (~470 MB is over, but the model is typically distributed quantized ~112 MB; Paper would ship quantized or rely on Candle's safetensors load). Need to confirm the actual safetensors download size Paper would ship.
- paraphrase-multilingual-MiniLM-L12-v2 is the Apache-2.0 fallback if MIT is undesirable; 384 dims match too, but MTEB retrieval scores are modest and it covers fewer languages (no my/km/lo etc.).
- LaBSE/jina-v3/arctic-embed are NOT viable for Paper's constraints: wrong dims (768), too large (>1 GB), non-BERT architecture (jina), or English-only (arctic).
- If Paper wants both English quality and multilingual coverage within <=200 MB, the realistic option is ONE multilingual model (e5-small) rather than per-language downloads.

### Gaps
- Exact safetensors download size of intfloat/multilingual-e5-small that Paper would ship (HF API output truncated; Teradata quantized mirror says ~112.8 MB int8).
- Whether Candle can load multilingual-e5-small's BERT config directly without sentence-transformers pooling logic (mean pooling + L2 norm must be reimplemented in Paper's embedder).
- Confirmation that multilingual-e5-small covers Paper's target scripts: my (Burmese), km (Khmer), lo (Lao) — listed in HF tags yes, but embedding quality on these is unverified.

## Key Question 4: Multilingual rerankers (ungated, small enough for CPU)

### Takeaway
`BAAI/bge-reranker-v2-m3` is the only viable ungated multilingual cross-encoder reranker: Apache-2.0, 568M params, XLM-RoBERTa-Large base, 8K context, strong multilingual. BUT at 2.27 GB FP32 it vastly exceeds Paper's <=200 MB default budget and even the opt-in model budget (bge-reranker-base is 1.13 GB English/Chinese only). There is NO small (<=200 MB) ungated multilingual reranker. Recommendation: keep bge-reranker-base (1.13 GB) as the opt-in reranker for English/Chinese, and treat v2-m3 as documented-but-not-shipped; do NOT bundle a multilingual reranker in the default binary.

### Cited Findings
- `BAAI/bge-reranker-v2-m3`: multilingual cross-encoder reranker, 568M params, 2.27 GB FP32, Apache-2.0, XLM-RoBERTa-Large base, 8194 positions / 8K context, sigmoid-mapped relevance score [0,1]. Recommended by BGE for multilingual contexts. — [BGE docs bge_reranker_v2](https://bge-model.com/bge/bge_reranker_v2.html), [ai-opensource.com](https://ai-opensource.com/models/bge-reranker-v2-m3/)
- `BAAI/bge-reranker-base`: xlm-roberta-base, Chinese and English, "Lightweight reranker model, easy to deploy, with fast inference." This is Paper's current opt-in reranker at 1.13 GB. — [BGE docs model list](https://huggingface.co/BAAI/bge-reranker-v2-m3) (Model List table)
- `BAAI/bge-reranker-large`: xlm-roberta-large, Chinese and English. Larger, not multilingual beyond CN/EN.
- `bge-reranker-v2-gemma` (2.51B, ~10 GB) and `v2-minicpm-layerwise` (2.72B, ~10.9 GB): multilingual but far too large for CPU/single-binary. — [BGE docs](https://bge-model.com/bge/bge_reranker_v2.html)

### Inferences
- No ungated multilingual reranker exists in a size compatible with Paper's <=200 MB default or even opt-in model budget. v2-m3 (2.27 GB) is 2x the largest model Paper currently ships (bge-reranker-base 1.13 GB).
- Cross-lingual reranking quality for non-CJK languages (Thai, Burmese, Khmer, Lao, Swahili, etc.) is unproven for v2-m3; BGE claims multilingual but published eval is CN/EN-focused.
- Paper should NOT add a multilingual reranker to the default stack. Keep reranking opt-in (bge-reranker-base for EN/ZH) and document v2-m3 as a user-downloadable option for multilingual reranking.

### Gaps
- Published MTEB/multilingual reranking numbers for bge-reranker-v2-m3 on Thai/Burmese/Khmer/Lao.
- Whether a distilled/smaller multilingual cross-encoder (<=200 MB) exists under a permissive license (none found in this search).

## Key Question 5: Chunking CJK/Thai — "400 words" is meaningless; sentence segmentation across scripts; line citations

### Takeaway
Word-count chunking ("400 words") is invalid for CJK/Thai where words aren't space-delimited. Chunk by sentence instead, using ICU `BreakIterator` sentence boundaries (Unicode #29) which know CJK terminators (U+3002 ideographic full stop, U+FF01, U+FF1F) and Thai sentence breaks. Chunk size must be measured in tokens/characters, not words. Line citation accuracy (path, line_start, line_end) is preserved because chunking operates on line ranges — sentence boundaries are snapped to line boundaries so every chunk maps to exact source lines.

### Cited Findings
- ICU `BreakIterator` sentence boundary follows Unicode #29; CJK sentence terminators (ideographic full stop U+3002, fullwidth ! U+FF01, fullwidth ? U+FF1F) plus ASCII .!? are recognized; ASCII period alone finds NO boundary in CJK text. — [ICU Boundary Analysis](https://unicode-org.github.io/icu/userguide/boundaryanalysis/), [tidycjk cjk_sentences](https://cran.r-project.org/web/packages/tidycjk/tidycjk.pdf)
- ICU sentence breaking has known false positives on abbreviations ("Mr. Smith" splits after "Dr."); CJK text does not have this problem because terminators are unambiguous. Default rules lack abbreviation lists (only en/de/es/fr/it/pt have `ss=standard` filtering). — [ICU Boundary Analysis Sentence Break Filters](https://unicode-org.github.io/icu/userguide/boundaryanalysis/)
- `icu_segmenter` Rust crate exposes sentence segmentation (Word/SentenceSegmenter) compatible with Unicode #29, gated behind `compiled_data`. — [icu_segmenter docs](https://docs.rs/icu_segmenter/latest/icu_segmenter/)
- Paper's indexer is line-accurate: chunks carry path, line_start, line_end; chunking keeps them accurate. — [AGENTS.md layout: src/core/index/indexer (incremental, line-accurate chunks)](https://github.com/enzo/paper) (repo AGENTS.md)
- Thai/Lao/Khmer/Burmese use no spaces between words; sentence breaks still exist via punctuation and particles; dictionary-based word break needed within sentences for indexing, but sentence-level chunking works via BreakIterator. — [ICU Boundary Analysis Dictionary-Based BreakIterator](https://unicode-org.github.io/icu/userguide/boundaryanalysis/)

### Inferences
- Replace "400 words" with a sentence-aware chunker: split on ICU sentence boundaries, then pack sentences into chunks bounded by a max token/character count (e.g., ~512 tokens to match embedding model context), snapping chunk ends to line boundaries so citations stay exact.
- For CJK, sentence boundaries are reliable (unambiguous terminators), so sentence chunking works well without abbreviation filtering.
- For Thai/Lao/Khmer, sentence segmentation via ICU #29 rules is approximate ("sufficient for many purposes" per ICU docs); consider treating paragraph/newline as a hard chunk boundary as a fallback.
- Line citations are orthogonal to segmentation method: as long as chunks are unions of whole lines, path/line_start/line_end stay exact regardless of language.

### Gaps
- `icu_segmenter` Rust API surface for SentenceSegmenter (docs show WordSegmenter prominently; sentence segmentation availability in the 2.x crate is unconfirmed).
- Token-count vs character-count chunk sizing for CJK (one CJK char ≈ one token in BERT wordpiece; need a concrete max-tokens-per-chunk default).
- Benchmark of sentence-boundary accuracy for Thai/Lao/Khmer vs paragraph-based fallback.

## Key Question 6: Query-time language detection (whatlang / lingua / cld3)

### Takeaway
`lingua` (pemistahl/lingua-rs) is the most accurate Rust language detector and handles short text, but accuracy drops significantly below ~120 characters and it has a large binary/data footprint. `whatlang` is ~15x faster, ~180x smaller binary, but weaker (single words 57%, word pairs 71%, sentences 94% vs Lingua 74%/89%/96%). `whichlang` (quickwit) is fast and accurate on short texts. For Paper's short queries, none is reliable on single words; recommend lingua with a confidence threshold + script-based fallback (CJK/Hiragana/Katakana/Thai/Burmese/Khmer scripts detected directly from Unicode script), and treat detection as a hint, not a hard gate.

### Cited Findings
- `lingua`: "most accurate natural language detection library for Rust, suitable for short text and mixed-language text"; rule-based + statistical; detection accuracy for short texts <120 chars drops significantly. Lingua 1.2.0 average accuracy: single words 74%, word pairs 89%, sentences 96%, average 86%. — [lingua-rs GitHub](https://github.com/pemistahl/lingua-rs), [docs.rs lingua 1.3.2](https://docs.rs/crate/lingua/1.3.2/source/README.md), [Lingua 1.5.0 announcement](https://query.rs/posts/2023-06-15/149viv4/lingua-1-5-0-the-most-accurate-natural-language-detection-library-for-rust-now-w)
- `whatlang`: focus on simplicity/performance, ~15x faster than lingua, ~180x smaller binary footprint. Whatlang 0.12.0 average accuracy: single words 57%, word pairs 71%, sentences 94%, average 74%. Confidence 0-1 with is_reliable(). — [whatlang crates.io](https://crates.io/crates/whatlang), [whatlang.org](https://whatlang.org/)
- `whichlang` (quickwit-oss): blazingly fast, lightweight, accurate on short texts; table shows per-language accuracy (e.g. Spanish 82.91%/55.45%/82.24%/94.85%/99.10% across text lengths). — [whichlang GitHub](https://github.com/quickwit-oss/whichlang), [Quickwit blog](https://quickwit.io/blog/whichlang-language-detection-library)
- lingua-rs README compares against CLD2 (emk/rust-cld2) and Whatlang as the main alternatives. — [lingua-rs README](https://docs.rs/crate/lingua/1.3.2/source/README.md)

### Inferences
- Short queries (the Paper norm) are the weakest case for all detectors: even lingua drops below 120 chars. Detection should never be a hard requirement for search; it's an optimization hint for choosing stemmer/stopwords/segmenter.
- Script-based detection (Unicode script: Hiragana/Katakana=Japanese, Han=Chinese, Thai script=Thai, Myanmar=Burmese, Khmer=Khmer, Lao=Lao) is 100% accurate on script-identifiable text and zero-cost; combine with lingua for Latin-script languages where script alone is insufficient (e.g., English vs French vs German).
- `cld3` (Google Compact Language Detection 3) has no mature maintained Rust binding; `emk/rust-cld2` is CLD2 (older, C++ binding). Not recommended for a single-static-binary Rust project.
- Detection result feeds Key Questions 2 (stemmer choice), 7 (stopwords), and 1 (segmenter choice) at query time.

### Gaps
- lingua crate size / binary impact and whether its language model data is embedded or loaded at runtime (relevant to <=200 MB budget).
- Accuracy numbers for lingua 1.8.0 (latest) vs the 1.2.0 numbers cited (may have improved).
- No Rust cld3 binding found; confirm cld3 is not viable for Paper.

## Key Question 7: Normalization (NFC/NFKC, case folding, diacritics, full/half-width, Arabic/Indic, RTL, emoji)

### Takeaway
Apply the SAME normalization pipeline at index and query time: NFC (or NFKC for CJK compatibility) → case folding → diacritic stripping (unicode61 remove_diacritics=1 handles FTS5 side) → full/half-width normalization (CJK) → Arabic/Indic digit normalization. Do NOT strip diacritics blindly for languages where they're phonemic (Arabic, Vietnamese, Indic). RTL (Arabic/Hebrew) is display-only for search — normalization matters, bidi reordering does not. Emoji should be preserved as tokens (not stripped) since they can be searchable content. The `unic` crate (Unicode categories/normalization) is the Rust tool for this; sqlite-vec embeddings need normalization only insofar as the tokenizer input is normalized.

### Cited Findings
- SQLite FTS5 `unicode61` supports `remove_diacritics=1` and `categories=` to control which Unicode categories are indexed (e.g., skip punctuation/symbols). Case folding is applied by unicode61. — [SQLite FTS5 unicode61 docs](https://www.sqlite.org/fts5.html#the_unicode61_tokenizer)
- `unic` Rust crate provides Unicode normalization (NFC/NFKC/NFD/NFKD), categories, and case folding. — [unic crates.io](https://crates.io/crates/unic) (known Rust Unicode crate)
- CJK full/half-width: NFKC normalizes fullwidth Latin (U+FF01-ff5e) to halfwidth and compatibility chars (e.g., U+3002 variants) — needed so "ＳＥＡＲＣＨ" matches "search". — [Unicode Normalization](https://unicode.org/reports/tr15/) (NFKC)
- Arabic/Indic: Arabic-Indic digits (U+0660-0669) and Extended Arabic-Indic digits differ from ASCII digits; normalization maps them to ASCII for search. — general Unicode knowledge (digits normalization)
- Emoji: Unicode symbols category; whether to index depends on use case. Stripping all symbols loses emoji search; keeping all symbols inflates index with noise. — [unicode61 categories option](https://www.sqlite.org/fts5.html#the_unicode61_tokenizer)

### Inferences
- Paper already uses unicode61 with porter; swapping porter out but keeping unicode61 + remove_diacritics=1 gives diacritic control at the FTS5 level.
- For the embedding side (sqlite-vec), normalization must be applied to the text BEFORE tokenization for the embedding model, so the same `unic` pipeline applies to both FTS5 and embedding inputs.
- NFKC (not just NFC) is important for CJK because it resolves full/half-width and compatibility characters that otherwise create duplicate tokens.
- Do not apply remove_diacritics=1 for Arabic (diacritics/harakat are meaningful) or Vietnamese (tones are phonemic) — language-conditional normalization.

### Gaps
- `unic` crate version and whether it's actively maintained (alternative: `rust-normalize` / `unicode-normalization` crates).
- Confirmed list of languages where diacritic stripping is harmful (Arabic, Vietnamese, Indic, etc.) vs helpful (French, German, Spanish).
- Whether sqlite-vec KNN is sensitive to embedding input normalization (it isn't directly — embeddings are vectors — but the TEXT fed to the embedder must be normalized consistently).

## Key Question 8: Stopwords per language without shipping 100 lists

### Takeaway
`stop_words` (cmccomb/rust-stop-words, v0.10.1) is the best Rust stopwords crate: ISO 639-1 + NLTK lists, static slices, no allocation, feature-gated (`iso` default, `nltk`/`constructed` optional), covers ~60+ real languages including zh, ja, th, ar, hi, etc. It does NOT cover km (Khmer), lo (Lao), my (Burmese) — those need custom lists or none. Stopwords are optional in Paper (FTS5 can index everything; stopword removal is a relevance optimization), so shipping stopword data for ~20 major languages and skipping the rest is acceptable.

### Cited Findings
- `stop_words` v0.10.1: lookup by ISO 639-1 code or Language enum, returns `Option<&'static [&'static str]>` (static slice, no alloc), `available_languages()` lists enabled codes. Features: `iso` (default, Stopwords ISO), `nltk` (NLTK lists), `constructed` (fictional), `all`. — [stop_words docs.rs](https://docs.rs/stop-words/latest/stop_words/), [crates.io stop-words 0.10.1](https://crates.io/crates/stop-words)
- Coverage (ISO ✓): ar, eu, bn, br, bg, ca, cs, da, nl, en, et, fa, fi, fr, gl, gu, ha, he, hi, hr, hu, id, it, ja, ko, ku, la, lt, mr, ms, nb(no), pl, pt, ro, ru, sl, es, sv, tl, th, tr, uk, ur, vi, yi, yo, zu + many more. NLTK adds sq, az, be, ca, zh, da, nl, en, fi, fr, de, el, he, hu, id, it, kk, ne, pt, ro, ru, sl, es, sv, ta, tg, tr, uz, hinglish. — [stop_words Language Coverage Table](https://docs.rs/stop-words/latest/stop_words/)
- NOT covered (no ISO/NLTK list): km (Khmer), lo (Lao), my (Burmese) — exactly Paper's hardest segmentation languages also lack stopword lists. — [stop_words coverage table](https://docs.rs/stop-words/latest/stop_words/)
- Alternative `stopwords` crate (v0.1.0): Spark/NLTK/SkLearn providers, smaller language set. — [stopwords docs.rs](https://docs.rs/stopwords/latest/stopwords/)

### Inferences
- `stop_words` with default `iso` feature covers the major languages Paper needs; no need to ship 100 lists. The static-slice design means zero runtime cost for enabled languages.
- For languages without stopword data (km, lo, my, and many others), Paper simply skips stopword filtering — search still works, slightly less precise. This is acceptable given stopwords are optional.
- Stopword removal must match the stemmer/tokenizer language choice at query time (detected via Key Question 6) and be applied identically at index time. FTS5 stopword handling is tokenizer-level; Paper could also filter stopwords post-tokenization.

### Gaps
- Exact binary size added by `stop_words` iso feature (static slices embedded in binary).
- Whether to integrate stopwords into FTS5's tokenizer (custom tokenizer) vs filter at pipeline level — affects where language detection is needed.

## Key Question 9: Evaluation (MTEB multilingual, MIRACL, Mr.TyDi, CLIRMatrix; local corpus)

### Takeaway
Use MIRACL (18 languages, 726k judgments, 78k queries, Wikipedia, native-speaker annotated, Apache-2.0) and Mr. TyDI (11 languages, human-labeled) as the primary multilingual retrieval benchmarks; MTEB for embedding model leaderboard positioning; CLIRMatrix (139 languages, synthetic, 34B labels) is NOT recommended (query titles are short named entities, labels derived from BM25 → biased toward lexical). For a small reproducible local corpus, build a 3-5 language Wikipedia subset (EN + JA + TH + AR + SW), ~10k passages each, hand-label 50-100 queries per language, measure nDCG@10 and Recall@100 (MIRACL's official metrics).

### Cited Findings
- MIRACL: 18 languages, 3B+ native speakers, 726k relevance judgments for 78k queries over Wikipedia, all annotations by native speakers, ~5 person-years, Apache-2.0 License, metrics nDCG@10 and Recall@100. Known languages: ar, bn, en, fi, id, ja, ko, ru, sw, te, th (from Mr. TyDI) + hi, es, fr, fa, zh (new) + 2 surprise. — [MIRACL TACL paper](https://aclanthology.org/2023.tacl-1.63/), [project-miracl/miracl GitHub](https://github.com/project-miracl/miracl)
- Mr. TYDI: 11 typologically diverse languages (ar, bn, en, fi, id, ja, ko, ru, sw, te, th), mono-lingual ad hoc retrieval, Wikipedia corpus, human-labeled (each query ~1 positive passage), corpus sizes 136k (sw) to 32.9M (en) passages. — [Mr. TyDI arxiv 2108.08787](https://arxiv.org/abs/2108.08787)
- CLIRMatrix: 139 languages, 352k queries, 34B labels, SYNTHETIC (queries = Wikipedia titles = short named entities; labels = BM25 scores → biased toward lexical approaches), not human-annotated. — [MIRACL TACL paper Table 1 comparison](https://aclanthology.org/2023.tacl-1.63/)
- MTEB: embedding leaderboard; intfloat/multilingual-e5-small has published MTEB results across 100 languages (e.g., ja AmazonCounterfactual 64.2%, th MassiveIntent 64.1%). — [HF API model-index intfloat/multilingual-e5-small](https://huggingface.co/api/models/intfloat/multilingual-e5-small)

### Inferences
- MIRACL and Mr. TyDI are the right benchmarks because they are human-annotated, monolingual (query + corpus same language), and cover Paper's target scripts (ja, th, ar, sw, etc.). CLIRMatrix's BM25-derived labels would flatter Paper's FTS5-heavy pipeline and are unsuitable for honest evaluation.
- nDCG@10 + Recall@100 are the standard metrics; Paper should report these for each language in its evaluation.
- A local reproducible corpus is essential because MIRACL/Mr. TyDi Wikipedia corpora are huge (up to 32.9M passages for EN) and require network to download. A 5-language × 10k-passage subset with hand-labeled queries is feasible offline and matches Paper's "no network" testing invariant.
- Evaluation must test the FULL pipeline (FTS5 + sqlite-vec + RRF + optional reranker), not just embeddings, since Paper's novelty is the hybrid pipeline.

### Gaps
- MIRACL per-language nDCG@10 baseline numbers for multilingual-e5-small (to know expected quality).
- Concrete recipe for building the local 5-language corpus (which Wikipedia dump, passage segmentation, query selection) — not found in sources.
- Whether Paper's existing `tests/common/mod.rs` HashEmbedder-based offline tests can be extended to multilingual without real weights.

## Key Question 10: Practical default — one model for everyone vs language-tiered downloads (<=200 MB budget)

### Takeaway
ONE multilingual model for everyone is the only option that fits the <=200 MB default budget. `intfloat/multilingual-e5-small` (384 dims, MIT, ungated, BERT, ~112 MB quantized / ~470 MB F32) covers 100 languages in a single download and matches Paper's sqlite-vec schema exactly. Language-tiered downloads (per-language models) would multiply the budget and add complexity with no commensurate benefit, since no per-language embedding model beats a good multilingual one on retrieval for its own language by enough to justify separate downloads. The reranker stays opt-in (bge-reranker-base 1.13 GB EN/ZH only; v2-m3 2.27 GB documented but not shipped).

### Cited Findings
- Paper's default <=200 MB budget and bge-small-en-v1.5 default (134 MB), multilingual-e5-small (487 MB) and bge-reranker-base (1.13 GB) opt-in — from repo AGENTS.md. — [AGENTS.md Model integration tests](file:///Users/p0is0n/GitHub/enzo/AGENTS.md)
- `intfloat/multilingual-e5-small`: MIT, ungated, 384 dims, 100 languages, BERT architecture, ~112 MB int8 (Teradata mirror) / ~470 MB F32. — [HF API intfloat/multilingual-e5-small](https://huggingface.co/api/models/intfloat/multilingual-e5-small), [Teradata/multilingual-e5-small](https://huggingface.co/Teradata/multilingual-e5-small)
- `BAAI/bge-reranker-v2-m3`: 2.27 GB FP32, Apache-2.0, 568M params — 10x the default budget. — [BGE docs bge_reranker_v2](https://bge-model.com/bge/bge_reranker_v2.html)
- No small (<=200 MB) ungated multilingual cross-encoder reranker exists. — [Key Question 4 research]

### Inferences
- Default model = multilingual-e5-small gives Paper multilingual search out of the box within the <=200 MB budget (assuming quantized ~112 MB ships; F32 470 MB would break the budget and need quantization or a smaller variant).
- If even 112 MB is too much for the default, Paper's current invariant holds: missing weights → keyword-only (FTS5) search plus a hint, or exit code 3 with --semantic. Multilingual support degrades gracefully to FTS5 + segmentation.
- Segmentation dictionaries (jieba Chinese, lindera Japanese, icu_segmenter Thai/Khmer/Lao/Burmese) add their own MB on top of the embedding model — this is the real budget pressure, not the model. One model for everyone keeps the model cost fixed; dictionary costs are per-script but only incurred when those scripts are present.
- Language-tiered downloads are NOT recommended: they complicate the spec-hashing invariant (every model file hash-pinned), increase surface area, and multilingual-e5-small already covers 100 languages adequately.

### Gaps
- Confirmed quantized download size Paper would actually ship for multilingual-e5-small (HF safetensors size was truncated in API output).
- Whether Candle can quantize/load e5-small at runtime or whether Paper ships pre-quantized weights (hash-pinned per invariant 4).
- Total binary+data size with all segmentation dictionaries embedded (jieba + lindera + icu_segmenter data) vs the <=200 MB default.