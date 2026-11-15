# Spelling recovery

Spella is for people who know a word by ear but not by letter. This note
explains how a misspelling becomes a suggestion, where the suggestion is shown,
and how to tune it.

Code: [`core/english/src/spelling.rs`](../../core/english/src/spelling.rs)
(finding and ranking) and `Vocabulary::merge_corrections` in
[`core/english/src/lib.rs`](../../core/english/src/lib.rs) (placement).

## Finding candidates

For each typed spelling of three letters or more that is not already a common
word, the background worker scans the vocabulary words that start with a
plausible first letter and keeps a word when either holds:

- **It is within two edits.** An edit is one letter added, removed or replaced,
  or two neighbouring letters swapped. Two unrelated edits are allowed from five
  letters up; shorter inputs carry too little evidence.
- **It sounds the same.** Both spellings are reduced to a *sound key*: the
  consonant skeleton with each vowel group collapsed to one mark, after rules for
  silent and equivalent letters (`ph`=`f`, `kn`=`n`, `wr`=`r`, `c` before
  `e`/`i`=`s`, silent `gh`, `ould`=`ud`, silent final `e`, doubled consonants).
  The first vowel must also belong to the same class, so `fone` is *phone* and
  never *fine*.

Plausible first letters are the typed one, the ones it is confused with by sound
(`c`/`k`/`q`, `f`/`p`, `n`/`k`/`g`, `r`/`w`, `s`/`c`/`z`, a vowel/`h`) and the
second typed letter, for a swap at the start (`hte`).

The scan reads the memory-mapped vocabulary directly and builds no index. It
takes under a millisecond and costs nothing until a misspelling is typed.

## Ranking

Each candidate scores `ln(count) - cost`. The cost is low for the mistakes people
actually make:

| Mistake | Example | Cost |
|---|---|---|
| Two neighbouring letters swapped | `teh` → the | 1.5 |
| A letter doubled or undoubled | `realy` → really | 1.5 |
| One edit, same sound | `seperate` → separate | 2.0 |
| Two edits, same sound | `fone` → phone | 2.5 |
| The right letters in the wrong order | `tounge` → tongue | 3.0 |
| Same sound, further apart | `nite` → night | 3.5 |
| One other edit | `goverment` → government | 4.5 |
| Two letters left out | `probly` → probably | 5.5 |
| Two letters added | `publically` → publicly | 7.0 |
| Two other edits | | 8.0 |

Extra cost applies when the typed ending is thrown away (`finaly` → final), when
the first letter differs, and when the candidate is a personal or place name.
Archaic words are never suggested. Words below a frequency floor are not
suggested; the floor is lower for swapped, doubled and sound-alike matches.

## Placement

Suggestions arrive just after the ordinary candidates. A suggestion goes to the
front when it is likelier than everything the typed letters could still become:
its `count × e^-cost` must exceed the combined count of the five most frequent
words that start with the typed letters. `teh` is *the*, not the start of
*Tehran*; `thr` is still on its way to *three* and *through*.

These keep their places ahead of any suggestion:

- words from your personal dictionary;
- the typed spelling itself when it is a real word. A rare real word is still
  followed by the common word it resembles: `calender`, then *calendar*;
- exactly typed joined words, when the phrase is likelier than the correction
  (`iam` stays *I am*; `adress` becomes *address*, not *a dress*);
- the meanings of complete pinyin (`nihao`).

At most three suggestions lead; the rest follow the other candidates. A
suggestion never changes what Space or Enter commit, and if you have already
moved to a candidate with the arrow keys, the selection stays on it.

## Tuning and tests

[`core/english/src/misspellings.txt`](../../core/english/src/misspellings.txt)
lists `typed intended` pairs. `spelling_recovers_common_misspellings` requires
the intended word first for at least 90% of them and in the first three for at
least 96%; `spelling_suggestions_do_not_lead_ordinary_typing` guards ordinary
prefixes, joined words and pinyin. When you find a real slip that is handled
badly, add it to the file first, then adjust the costs or the sound rules.

```powershell
cargo test -p retype-english spelling -- --nocapture
```

prints the totals and every pair that is not first.
