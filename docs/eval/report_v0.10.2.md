# formslot evaluation

formslot 0.10.1 · gold set `docs/eval/gold.jsonl` · 147 tasks

## Set

- kinds: find 67, reuse 47, convention 20, decline 13
- corpus formvis at 8fe4ce45bcfa1b85c509db4bb34da29942db964a
- token estimate: ceil(bytes / 3.6) for code and envelopes, ceil(bytes / 4) for prose

## Results

| arm | kind | n | R@1 | R@4 | R@10 | MRR | nDCG@10 | R@4k | R@4k+P0 | tok mean | tok median | ms median |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| formslot | all | 147 | 0.327 | 0.599 | 0.837 | 0.481 | 0.342 | 0.912 | 0.918 | 3833 | 3930 | 247.7 |
| formslot | find | 67 | 0.284 | 0.537 | 0.851 | 0.449 | 0.351 | 0.970 | 0.970 | 3881 | 3955 | 244.9 |
| formslot | reuse | 47 | 0.191 | 0.511 | 0.766 | 0.361 | 0.266 | 0.851 | 0.851 | 3763 | 3899 | 256.1 |
| formslot | convention | 20 | 0.550 | 0.800 | 0.850 | 0.662 | 0.352 | 0.850 | 0.900 | 3838 | 3933 | 237.0 |
| formslot | decline | 13 | 0.692 | 0.923 | 1.000 | 0.801 | 0.559 | 0.923 | 0.923 | 3836 | 3922 | 247.6 |
| baseline | all | 147 | 0.041 | 0.156 | 0.286 | 0.110 | 0.079 | 0.156 | - | 460127 | 471047 | 64.4 |
| baseline | find | 67 | 0.075 | 0.254 | 0.493 | 0.192 | 0.141 | 0.254 | - | 465847 | 476526 | 63.7 |
| baseline | reuse | 47 | 0.021 | 0.128 | 0.191 | 0.069 | 0.045 | 0.128 | - | 451843 | 461812 | 68.2 |
| baseline | convention | 20 | 0.000 | 0.000 | 0.000 | 0.000 | 0.000 | 0.000 | - | 459085 | 461296 | 61.4 |
| baseline | decline | 13 | 0.000 | 0.000 | 0.000 | 0.000 | 0.000 | 0.000 | - | 462199 | 461349 | 66.3 |

R@k: share of tasks with an expected file, symbol, slot or memory phrase in the top k of the ranking. R@4k: share with an expected item in the pack sent at 4,000 tokens (baseline: in the top 4 files read); R@4k+P0 also counts the stable prefix. Tokens are per task.

## Arms

- formslot: the fused search ranking and the T4 pack at 4000 tokens; tokens are the pack's sent count; latency is search plus pack.
- baseline: ripgrep 14.1.0 over the clean checkout at the indexed commit; the task's terms with stop words dropped, case-insensitive, fixed strings; files ranked by matching-line count; tokens are the estimate of the top 4 files read whole; the R@4k columns are the top 4 files.
- dense: not run. this build has no `dense` cargo feature, so there is no embedding index or model to rank with.

## Gates

- Input tokens down at least 30 percent: measured, met. The formslot pack averages 3833 tokens per task against 460127 for the baseline (top 4 files read whole), a change of -99.2 percent. The baseline reads the top 4 matching files whole, which is a generous upper bound on what an agent without formslot would send, not a measured agent run. This compares what is sent, not whether the answer is right.
- Recall@4k at least 0.80: measured, met. 0.912 of the gold tasks have an expected item in the 4000-token pack outside the stable prefix and the echoed task (with the prefix counted: 0.918); the baseline reaches 0.156 with its top 4 files.
- Pass rate within 1 point: not run, it needs model calls and there is no ANTHROPIC_API_KEY in the environment.

## Matching

- A file counts when a unit or slot hit has that path; a symbol when a unit hit's `path#qualified` is the expected string; a slot when a slot hit has the id; a memory phrase when a memory hit's title or text holds it, ignoring case.
- The baseline ranks files only, so a symbol counts for it when the file is the symbol's file, and it cannot reach slot or memory items.
- nDCG@10 gives a gain of 1 to each hit that meets an expected entry not met before; the ideal run has one gain per expected entry, up to 10.

## Tuning and held-out halves (formslot arm)

Tuning half: gold lines with even numbers (73 tasks); held-out half: odd lines (74). Only tuning-half failures were read when choosing changes. Run with `formslot eval --half even|odd`.

| set | version | R@1 | R@4 | R@10 | MRR | R@4k | R@4k+P0 | tok mean |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| all 147 | v0.10.0 | 0.340 | 0.605 | 0.810 | 0.484 | 0.796 | 0.844 | 3797 |
| all 147 | v0.10.2 | 0.327 | 0.599 | 0.837 | 0.481 | 0.912 | 0.918 | 3833 |
| tuning 73 | v0.10.0 | 0.288 | 0.603 | 0.795 | 0.458 | 0.795 | 0.836 | 3810 |
| tuning 73 | v0.10.2 | 0.274 | 0.616 | 0.836 | 0.466 | 0.932 | 0.945 | 3849 |
| held-out 74 | v0.10.0 | 0.392 | 0.608 | 0.824 | 0.510 | 0.797 | 0.851 | 3784 |
| held-out 74 | v0.10.2 | 0.378 | 0.581 | 0.838 | 0.495 | 0.892 | 0.892 | 3818 |

The held-out half improves on R@4k (0.797 to 0.892) and R@10, and loses a little on R@1, R@4 and MRR (-0.014, -0.027, -0.015). Most of the R@4k gain comes from listing more candidate files in P1, which raises recall but not precision: it is not a ranking improvement. The dense arm was not run: see the Arms section.
