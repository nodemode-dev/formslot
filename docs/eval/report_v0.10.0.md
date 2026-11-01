# formslot evaluation

formslot 0.10.0 · gold set `docs/eval/gold.jsonl` · 147 tasks

## Set

- kinds: find 67, reuse 47, convention 20, decline 13
- corpus formvis at 8fe4ce45bcfa1b85c509db4bb34da29942db964a
- token estimate: ceil(bytes / 3.6) for code and envelopes, ceil(bytes / 4) for prose

## Results

| arm | kind | n | R@1 | R@4 | R@10 | MRR | nDCG@10 | R@4k | R@4k+P0 | tok mean | tok median | ms median |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| formslot | all | 147 | 0.340 | 0.605 | 0.810 | 0.484 | 0.345 | 0.796 | 0.844 | 3797 | 3925 | 254.9 |
| formslot | find | 67 | 0.313 | 0.597 | 0.851 | 0.474 | 0.367 | 0.896 | 0.896 | 3849 | 3942 | 252.4 |
| formslot | reuse | 47 | 0.234 | 0.553 | 0.766 | 0.396 | 0.282 | 0.766 | 0.766 | 3703 | 3877 | 255.7 |
| formslot | convention | 20 | 0.450 | 0.650 | 0.700 | 0.550 | 0.301 | 0.450 | 0.800 | 3810 | 3922 | 248.9 |
| formslot | decline | 13 | 0.692 | 0.769 | 0.923 | 0.759 | 0.524 | 0.923 | 0.923 | 3842 | 3951 | 270.4 |
| baseline | all | 147 | 0.041 | 0.156 | 0.286 | 0.110 | 0.079 | 0.156 | - | 460127 | 471047 | 66.1 |
| baseline | find | 67 | 0.075 | 0.254 | 0.493 | 0.192 | 0.141 | 0.254 | - | 465847 | 476526 | 67.9 |
| baseline | reuse | 47 | 0.021 | 0.128 | 0.191 | 0.069 | 0.045 | 0.128 | - | 451843 | 461812 | 64.5 |
| baseline | convention | 20 | 0.000 | 0.000 | 0.000 | 0.000 | 0.000 | 0.000 | - | 459085 | 461296 | 64.0 |
| baseline | decline | 13 | 0.000 | 0.000 | 0.000 | 0.000 | 0.000 | 0.000 | - | 462199 | 461349 | 65.8 |

R@k: share of tasks with an expected file, symbol, slot or memory phrase in the top k of the ranking. R@4k: share with an expected item in the pack sent at 4,000 tokens (baseline: in the top 4 files read); R@4k+P0 also counts the stable prefix. Tokens are per task.

## Arms

- formslot: the fused search ranking and the T4 pack at 4000 tokens; tokens are the pack's sent count; latency is search plus pack.
- baseline: ripgrep 14.1.0 over the clean checkout at the indexed commit; the task's terms with stop words dropped, case-insensitive, fixed strings; files ranked by matching-line count; tokens are the estimate of the top 4 files read whole; the R@4k columns are the top 4 files.
- dense: not run. this build has no `dense` cargo feature, so there is no embedding index or model to rank with.

## Gates

- Input tokens down at least 30 percent: measured, met. The formslot pack averages 3797 tokens per task against 460127 for the baseline (top 4 files read whole), a change of -99.2 percent. This compares what is sent, not whether the answer is right.
- Recall@4k at least 0.80: measured, not met. 0.796 of the gold tasks have an expected item in the 4000-token pack outside the stable prefix and the echoed task (with the prefix counted: 0.844); the baseline reaches 0.156 with its top 4 files.
- Pass rate within 1 point: not run, it needs model calls and there is no ANTHROPIC_API_KEY in the environment.

## Matching

- A file counts when a unit or slot hit has that path; a symbol when a unit hit's `path#qualified` is the expected string; a slot when a slot hit has the id; a memory phrase when a memory hit's title or text holds it, ignoring case.
- The baseline ranks files only, so a symbol counts for it when the file is the symbol's file, and it cannot reach slot or memory items.
- nDCG@10 gives a gain of 1 to each hit that meets an expected entry not met before; the ideal run has one gain per expected entry, up to 10.
