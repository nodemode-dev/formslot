# formslot evaluation · v0.11.0 · dense tier

Written: 2026-10-30 · Set: `gold.jsonl` (147 tasks, formvis at 8fe4ce45) · Command: `formslot eval --gold docs/eval/gold.jsonl --corpus formvis --arm all` in a `--features dense` build

## Setup

- Model gte-modernbert-base int8, ONNX Runtime 1.23.2 placed in `<data>/dense/lib/` from a pip wheel.
- `dense build --corpus formvis` embedded 17,608 of 17,608 units in 4,235 s (70.6 minutes, 4.16 units per second on 2 cores), none skipped. formMCP and formskills were not embedded.
- The store was a copy indexed before the v0.11.0 section and text units, so the formslot arm here matches the v0.10.2 numbers (R@4k 0.871) and the dense arm is compared against that, not against the held-out report's v0.11.0 store.
- The dense list is fused at weight 1.0 and was not tuned.

## Results

Columns: n · R@1 · R@4 · R@10 · MRR · nDCG@10 · R@4k · R@4k+P0 · tokens mean · ms median.

| arm | kind | n | R@1 | R@4 | R@10 | MRR | nDCG@10 | R@4k | R@4k+P0 | tok mean | ms med |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| formslot | all | 147 | 0.327 | 0.599 | 0.837 | 0.481 | 0.342 | 0.871 | 0.918 | 3792 | 243.6 |
| formslot | find | 67 | 0.284 | 0.537 | 0.851 | 0.449 | 0.351 | 0.970 | 0.970 | 3848 | 245.9 |
| formslot | reuse | 47 | 0.191 | 0.511 | 0.766 | 0.361 | 0.266 | 0.851 | 0.851 | 3682 | 242.5 |
| formslot | convention | 20 | 0.550 | 0.800 | 0.850 | 0.662 | 0.352 | 0.550 | 0.900 | 3832 | 241.9 |
| formslot | decline | 13 | 0.692 | 0.923 | 1.000 | 0.801 | 0.559 | 0.923 | 0.923 | 3835 | 237.4 |
| baseline | all | 147 | 0.041 | 0.156 | 0.286 | 0.110 | 0.079 | 0.156 | - | 460127 | 62.0 |
| dense | all | 147 | 0.367 | 0.612 | 0.837 | 0.507 | 0.374 | 0.891 | 0.939 | 3847 | 323.5 |
| dense | find | 67 | 0.537 | 0.776 | 0.910 | 0.666 | 0.492 | 0.985 | 0.985 | 3905 | 336.8 |
| dense | reuse | 47 | 0.298 | 0.532 | 0.787 | 0.437 | 0.320 | 0.894 | 0.894 | 3749 | 322.6 |
| dense | convention | 20 | 0.200 | 0.500 | 0.800 | 0.363 | 0.228 | 0.550 | 0.900 | 3841 | 314.2 |
| dense | decline | 13 | 0.000 | 0.231 | 0.692 | 0.163 | 0.189 | 0.923 | 0.923 | 3910 | 318.6 |

## Reading it plainly

- Overall dense adds little: R@1 0.327 to 0.367, MRR 0.481 to 0.507, R@10 unchanged. R@4k 0.871 to 0.891 is 3 tasks of 147, within noise.
- `find` tasks gain a lot (R@1 0.284 to 0.537, R@4 0.537 to 0.776); `reuse` gains moderately.
- `convention` and `decline` lose badly in the ranked list (decline R@1 0.692 to 0, convention R@1 0.550 to 0.200), because the dense list holds units only and pushes memory hits down. The pack still carries them, so their R@4k is unchanged.
- Cost: tokens +1.5 percent, median latency +80 ms (including embedding the task), a 70-minute build, a 150 MB model and an ONNX Runtime library the owner places.

## Verdict

A05's condition for turning dense on by default is not met: neither CodeRankEmbed nor jina-v2-base-code was run, and the gold set has 147 tasks, not 200. Dense stays opt-in. The next step worth trying is fusing the dense list into unit ranking only, or at a lower weight for memory-bearing queries, then re-running on both gold sets with the v0.11.0 store.
