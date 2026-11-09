# Embedded English bigrams

`bigrams.bin` is an offline, immutable `include_bytes!` asset for continuous
English decoding. It is compiled from the existing SymSpell bigram file pinned
in `data/dict/raw/english-bigrams/SOURCES.json`, not from a Gboard model or a new
Google Books V3 extraction. Source compression is lossless; all original pairs
and counts are retained. The exact upstream generation chain is not established
by the cited upstream documentation. The source archive retains the full MIT
license and precise upstream commit, file URL, compressed and original hashes.

Normal build (no network and no manifest refresh):

```powershell
& C:\Python313\python.exe tools/english-assist/build-bigrams.py
& C:\Python313\python.exe tools/english-assist/build-bigrams.py --verify-only
& C:\Python313\python.exe tools/english-assist/test-build-bigrams.py
```

Only after reviewing an intentional source/compiler change, run the explicit
authoring command `build-bigrams.py --refresh-manifest`, inspect both manifests,
and rerun tests. The Windows build calls `build-english-bigrams.ps1` before any
Cargo compilation, including `-NoTest` builds. The asset is embedded in the
production DLL; it adds no runtime data file or installer payload entry.

## RTENBG01 layout

All numeric values are unsigned little-endian, with no native alignment padding.

| Section | Layout |
| --- | --- |
| Header, 32 bytes | magic `[u8;8]` = `RTENBG01`; `word_count:u32@8`, `pair_count:u32@12`, `arena_bytes:u32@16`, `tree_size:u32@20`; eight reserved zero bytes |
| Word records | `word_count` records, each 24 bytes: `string_offset:u32`, `string_length:u32`, `first_edge:u32`, `edge_count:u32`, `row_total:u64` |
| Edges | `pair_count` records, each 12 bytes: `next_word_id:u32`, `count:u64` |
| Range maximum tree | `2 * tree_size` u32 edge indices, `tree_size=next_power_of_two(pair_count)`; index zero unused; leaves at `tree_size`; missing entries `u32::MAX` |
| String arena | Concatenated lowercase ASCII words; no NUL terminators |

Word IDs are the lexical sort order of all left and right terms. Each row's
adjacent edges are sorted by next-word ID; empty rows retain a zero total.
`row_total` is the sum of the *retained published bigram counts* for that previous
word, not a claim about the total occurrences of that word in the original
corpus. A decoder can estimate a conditional preference from `count/row_total`,
with explicit smoothing/backoff handled by the decoder.

Tree nodes choose the edge with greater count, breaking ties by smaller edge
index. Prefix queries first binary-search lexical word IDs, intersect that
interval with one row's next IDs, then use range-max queries/heap expansion for
top-K. This avoids scanning a large row on every keypress. SHA256 covers the
entire image in the adjacent reviewed `bigrams.json`; the compiler validates all
offsets, lexical order, edge totals, tree leaves and internal winners.
