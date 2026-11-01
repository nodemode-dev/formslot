# formslot evaluation

formslot 0.10.1 · gold set `docs/eval/gold.jsonl` · 147 tasks

## Set

- kinds: find 67, reuse 47, convention 20, decline 13
- corpus formvis at 8fe4ce45bcfa1b85c509db4bb34da29942db964a
- token estimate: ceil(bytes / 3.6) for code and envelopes, ceil(bytes / 4) for prose

## Results

| arm | kind | n | R@1 | R@4 | R@10 | MRR | nDCG@10 | R@4k | R@4k+P0 | tok mean | tok median | ms median |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| formslot | all | 147 | 0.327 | 0.599 | 0.837 | 0.481 | 0.342 | 0.871 | 0.918 | 3792 | 3922 | 256.7 |
| formslot | find | 67 | 0.284 | 0.537 | 0.851 | 0.449 | 0.351 | 0.970 | 0.970 | 3848 | 3935 | 251.6 |
| formslot | reuse | 47 | 0.191 | 0.511 | 0.766 | 0.361 | 0.266 | 0.851 | 0.851 | 3682 | 3882 | 256.8 |
| formslot | convention | 20 | 0.550 | 0.800 | 0.850 | 0.662 | 0.352 | 0.550 | 0.900 | 3832 | 3936 | 282.0 |
| formslot | decline | 13 | 0.692 | 0.923 | 1.000 | 0.801 | 0.559 | 0.923 | 0.923 | 3835 | 3922 | 261.5 |
| baseline | all | 147 | 0.041 | 0.156 | 0.286 | 0.110 | 0.079 | 0.156 | - | 460127 | 471047 | 65.1 |
| baseline | find | 67 | 0.075 | 0.254 | 0.493 | 0.192 | 0.141 | 0.254 | - | 465847 | 476526 | 68.2 |
| baseline | reuse | 47 | 0.021 | 0.128 | 0.191 | 0.069 | 0.045 | 0.128 | - | 451843 | 461812 | 63.6 |
| baseline | convention | 20 | 0.000 | 0.000 | 0.000 | 0.000 | 0.000 | 0.000 | - | 459085 | 461296 | 65.8 |
| baseline | decline | 13 | 0.000 | 0.000 | 0.000 | 0.000 | 0.000 | 0.000 | - | 462199 | 461349 | 63.7 |

R@k: share of tasks with an expected file, symbol, slot or memory phrase in the top k of the ranking. R@4k: share with an expected item in the pack sent at 4,000 tokens (baseline: in the top 4 files read); R@4k+P0 also counts the stable prefix. Tokens are per task.

## Arms

- formslot: the fused search ranking and the T4 pack at 4000 tokens; tokens are the pack's sent count; latency is search plus pack.
- baseline: ripgrep 14.1.0 over the clean checkout at the indexed commit; the task's terms with stop words dropped, case-insensitive, fixed strings; files ranked by matching-line count; tokens are the estimate of the top 4 files read whole; the R@4k columns are the top 4 files.
- dense: not run. this build has no `dense` cargo feature, so there is no embedding index or model to rank with.

## Gates

- Input tokens down at least 30 percent: measured, met. The formslot pack averages 3792 tokens per task against 460127 for the baseline (top 4 files read whole), a change of -99.2 percent. The baseline reads the top 4 matching files whole, which is a generous upper bound on what an agent without formslot would send, not a measured agent run. This compares what is sent, not whether the answer is right.
- Recall@4k at least 0.80: measured, met. 0.871 of the gold tasks have an expected item in the 4000-token pack outside the stable prefix and the echoed task (with the prefix counted: 0.918); the baseline reaches 0.156 with its top 4 files.
- Pass rate within 1 point: not run, it needs model calls and there is no ANTHROPIC_API_KEY in the environment.

## Matching

- A file counts when a unit or slot hit has that path; a symbol when a unit hit's `path#qualified` is the expected string; a slot when a slot hit has the id; a memory phrase when a memory hit's title or text holds it, ignoring case.
- The baseline ranks files only, so a symbol counts for it when the file is the symbol's file, and it cannot reach slot or memory items.
- nDCG@10 gives a gain of 1 to each hit that meets an expected entry not met before; the ideal run has one gain per expected entry, up to 10.

## Tuning and held-out halves (formslot arm)

`formslot eval --half tuning|held-out` splits the gold set by cluster, not by line. Tasks that share any expected file, slot, symbol or source commit are joined (union-find, 49 clusters, the largest 33, 20 and 10 tasks), and each whole cluster goes to one half by the parity of the first byte of the SHA-256 of the cluster's smallest task id. No item is shared between the halves. The split is deterministic but not balanced: the held-out half has 33 tasks (find 22, reuse 11) and no convention or decline task, because those clusters hashed to the tuning half (114 tasks).

| set | n | R@1 | R@4 | R@10 | MRR | nDCG@10 | R@4k | R@4k+P0 | tok mean |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| all | 147 | 0.327 | 0.599 | 0.837 | 0.481 | 0.342 | 0.871 | 0.918 | 3792 |
| tuning | 114 | 0.316 | 0.596 | 0.807 | 0.470 | 0.324 | 0.851 | 0.912 | 3807 |
| held-out | 33 | 0.364 | 0.606 | 0.939 | 0.517 | 0.407 | 0.939 | 0.939 | 3738 |

Read these as indicative only. The halves are cluster-separated, but the gold set itself was written before tuning and parameters were partly chosen on all 147 tasks, not on the tuning half alone. The held-out number rests on 33 tasks of two kinds, so one task moves it by 0.03.

## What changed since the first v0.10.2 run, and what the numbers mean

- The first v0.10.2 run claimed R@4k 0.912. That figure counted owner rules restated in P2 (at most 2, when among the first five hits); the stable prefix P0 already carries those rules, so the restate moved the R@4k column without adding anything the agent did not have. The restate is removed. R@4k is now 0.871 and R@4k+P0 is 0.918 (unchanged: P0 is counted in that column). Convention tasks fall from 0.850 to 0.550 on R@4k for that reason, and stay 0.900 on R@4k+P0. The other kinds do not move on R@4k.
- The `files:` line stays. It lists up to 14 further candidate file paths (30 at T16) in P1, ordered by summed fused score of their units. It lists paths, not content: it raises recall by giving the agent more places to look, and does not rank the right file higher (R@1, R@4 and MRR do not move with it).
- R@4k is 0.871, above the 0.80 gate, so the gate text uses 0.871 (all 147 tasks, the figure with the prefix excluded). No parameter was tuned after this change.
