# formslot evaluation · v0.15.0 · slot weights

Written: 2026-11-01 · Sets: `gold.jsonl` (147, formvis) and `gold_heldout.jsonl` (subform 72, formslot 33, formvis 13) · Before: the v0.14.0 binary on the v0.14.0 store; after: this build on a copy of the same store (the index is unchanged, so no rebuild is needed). Changes were chosen on the tuning halves and judged on tuning, held-out and gold.jsonl together.

## Changes

- Rule slots (manifest kind rule, an id starting `rule.`, or files that are all a root CLAUDE.md, AGENTS.md or NOTICE.md) are native to every searched repository: weight 2.0 when grounded on rule-document text (query idf coverage at least 0.6, or a top-40 pool unit in a rule file), else 1.0.
- Any other slot from another repository is weighted 0.5 + 1.5 * min(1, coverage / 0.6), and at least 1.0 when it is the top slot and one of its files is among the top 40 fused units. Slots of the searched repository keep 2.0. The top five slot hits fuse as one list.
- The doc unit that tops the text lexical list also gets a rank-1 entry in the text trigram list; the 0.8 doc penalty stays.
- An ablation of the identifier and path words was run and not adopted (below).

## Before and after

Columns: n · R@1 · R@4 · R@10 · MRR · R@4k · R@4k+P0 · median ms (noisy on a shared 2-core machine).

| set · half | v0.14.0 | v0.15.0 |
| --- | --- | --- |
| subform · tuning | 37 · .378 · .649 · .784 · .503 · .946 · .946 · 87 | 37 · .378 · .649 · .784 · .506 · .946 · .946 · 75 |
| subform · held-out | 35 · .257 · .457 · .543 · .358 · .771 · .857 · 77 | 35 · .286 · .486 · .571 · .392 · .829 · .829 · 67 |
| subform · all | 72 · .319 · .556 · .667 · .432 · .861 · .903 · 82 | 72 · .333 · .569 · .681 · .450 · .889 · .889 · 72 |
| formslot · tuning | 16 · .312 · .625 · .750 · .492 · .875 · .875 · 75 | 16 · .312 · .625 · .750 · .490 · .875 · .875 · 61 |
| formslot · held-out | 17 · .294 · .412 · .647 · .392 · .824 · .882 · 73 | 17 · .294 · .471 · .647 · .406 · .824 · .882 · 59 |
| formslot · all | 33 · .303 · .515 · .697 · .441 · .848 · .879 · 74 | 33 · .303 · .545 · .697 · .447 · .848 · .879 · 60 |
| formvis (held-out set) · all | 13 · .154 · .385 · .615 · .278 · 1.000 · 1.000 · 105 | 13 · .154 · .385 · .615 · .278 · 1.000 · 1.000 · 84 |
| gold.jsonl · tuning | 114 · .333 · .640 · .798 · .494 · .886 · .930 · 86 | 114 · .333 · .649 · .798 · .497 · .895 · .930 · 84 |
| gold.jsonl · held-out | 33 · .394 · .576 · .879 · .515 · .939 · .939 · 92 | 33 · .394 · .576 · .879 · .512 · .939 · .939 · 92 |
| gold.jsonl · all | 147 · .347 · .626 · .816 · .498 · .898 · .932 · 88 | 147 · .347 · .633 · .816 · .500 · .905 · .932 · 85 |

Named tasks, first rank (in or out of the 4,000-token pack): h036 79 to 39 (in), h038 46 to 1 (in), h085 82 out to 62 in, h080 none to 53 (in), h071 20 to 20, h092 8 to 8, h095 17 to 20, h049 33 in to 36 out, h008 47 to 52 (out), h039 48 to 54 (out), h054 and h087 stay out.

## Ablation of identifier and path words (not adopted)

Cells are R@1 · R@4 · R@10.

| set · half | v0.15.0 | body and path words off | body words off, path words on |
| --- | --- | --- | --- |
| subform · tuning | .378 · .649 · .784 | .297 · .622 · .703 | .324 · .676 · .784 |
| subform · held-out | .286 · .486 · .571 | .257 · .514 · .657 | .257 · .457 · .600 |
| formslot · tuning | .312 · .625 · .750 | .188 · .500 · .750 | .312 · .688 · .750 |
| formslot · held-out | .294 · .471 · .647 | .353 · .706 · .706 | .294 · .529 · .647 |
| gold.jsonl · tuning | .333 · .649 · .798 | .342 · .640 · .833 | .325 · .658 · .807 |
| gold.jsonl · held-out | .394 · .576 · .879 | .394 · .576 · .879 | .333 · .576 · .879 |

The condition was that h049 and h095 recover without losing h008 and h039; neither variant meets it, so the index is unchanged. Dropping body words only is better on the tuning halves and worse on gold.jsonl held-out R@1 (.394 to .333): an owner call.

## Reading it plainly

- Gains are small: subform held-out R@1, R@4, R@10 and MRR each rise about .03 and R@4k about .06; formslot held-out R@4 rises .06; tuning halves and gold.jsonl move by at most .01. With 17 to 35 tasks per half, one task is .03 to .06, so none is clearly outside noise.
- Regressions: subform held-out R@4k+P0 falls from .857 to .829 (one task); h049 leaves the pack; about 36 tasks lose 1 to 5 ranks, mostly because the doc-unit credit lifts a doc unit above code units.
- Rule nativeness fixes h036, h038 and h085 and gives h080 a rank, the cases the v0.13.0 diagnosis named.
