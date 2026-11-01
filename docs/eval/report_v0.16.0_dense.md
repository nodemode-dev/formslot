# formslot evaluation · v0.16.0 · dense fused into unit ranking only

## Setup

- Gold set: `gold_heldout.jsonl`, corpora subform (72 tasks) and formslot (33 tasks), 105 tasks in all, split into the cluster-separated tuning and held-out halves.
- One binary built with `--features dense` and one store for every column. The formslot arm differs slightly from the v0.15.0 report because slot ranking changed in the same working tree.
- ONNX Runtime 1.23.2 from the pip wheel in `<data>/dense/lib`; the model from `formslot dense fetch --yes`.
- Embedding took 556 s for subform (6,705 units) and about 215 s for formslot (1,779 units). Section and text units are indexed, so unit counts are about three times the code-only estimate.
- Median latency of the dense arm is about 30 to 50 ms above the formslot arm. The 0.6 s model load is not counted.

## Variants

- **plain**: the v0.11.0 fusion. The dense list joins the global fusion as one more list.
- **cap**: plain, with the dense weight multiplied by 0.4 or 0.2 when rule memory or notes are covered.
- **pre** (chosen): lexical, trigram and dense code-unit lists are fused first (weights 1, 1, 1); that list replaces lexical and trigram in the global fusion at weight 2.0, their combined weight. Memory, rule, derived, notes, slot, graph and text lists are unchanged.
- **precap**: pre with the cap. Tried and dropped.

The final code (pre, with the trial switches removed) reproduced the pre numbers exactly on all four cells.

## Results

Cells are n · R@1 · R@4 · R@10 · MRR · R@4k · R@4k+P0.

| set · half | formslot (no dense) | dense plain (v0.11.0) | cap 0.4 | cap 0.2 | pre (chosen) |
| --- | --- | --- | --- | --- | --- |
| subform tuning | 37 · .378 · .649 · .757 · .511 · .946 · .946 | 37 · .514 · .676 · .784 · .610 · .946 · .946 | 37 · .541 · .649 · .730 · .612 · .946 · .946 | 37 · .514 · .622 · .757 · .592 · .946 · .946 | 37 · .432 · .703 · .784 · .568 · .946 · .946 |
| subform held-out | 35 · .286 · .486 · .571 · .396 · .857 · .857 | 35 · .171 · .457 · .600 · .329 · 1.000 · 1.000 | 35 · .171 · .400 · .629 · .308 · .886 · .886 | 35 · .229 · .429 · .571 · .348 · .886 · .886 | 35 · .257 · .571 · .657 · .413 · .943 · .943 |
| subform all | 72 · .333 · .569 · .667 · .455 · .903 · .903 | 72 · .347 · .569 · .694 · .473 · .972 · .972 | 72 · .361 · .528 · .681 · .465 · .917 · .917 | 72 · .375 · .528 · .667 · .473 · .917 · .917 | 72 · .347 · .639 · .722 · .493 · .944 · .944 |
| formslot tuning | 16 · .312 · .688 · .750 · .483 · .875 · .875 | 16 · .312 · .688 · .812 · .489 · 1.000 · 1.000 | 16 · .438 · .750 · .812 · .591 · .875 · .875 | 16 · .375 · .750 · .750 · .557 · .875 · .875 | 16 · .312 · .625 · .812 · .459 · 1.000 · 1.000 |
| formslot held-out | 17 · .235 · .471 · .588 · .353 · .824 · .882 | 17 · .059 · .294 · .471 · .188 · .824 · .882 | 17 · .059 · .294 · .647 · .202 · .824 · .882 | 17 · .118 · .412 · .647 · .269 · .824 · .882 | 17 · .118 · .471 · .588 · .305 · .824 · .882 |
| formslot all | 33 · .273 · .576 · .667 · .416 · .848 · .879 | 33 · .182 · .485 · .636 · .334 · .909 · .939 | 33 · .242 · .515 · .727 · .391 · .848 · .879 | 33 · .242 · .576 · .697 · .409 · .848 · .879 | 33 · .212 · .545 · .697 · .380 · .909 · .939 |

By kind, both corpora and both halves (105 tasks). Cells are n · R@1 · R@4 · R@10 · MRR · R@4k.

| variant | find (44) | reuse (18) | convention (27) | decline (16) |
| --- | --- | --- | --- | --- |
| no dense | .432 · .727 · .818 · .566 · .955 | .333 · .500 · .500 · .418 · .778 | .222 · .444 · .630 · .356 · .926 | .125 · .438 · .500 · .278 · .750 |
| plain | .568 · .705 · .841 · .650 · 1.000 | .222 · .444 · .500 · .353 · .889 | .037 · .370 · .556 · .221 · 1.000 | .062 · .500 · .625 · .260 · .812 |
| cap 0.4 | .545 · .727 · .818 · .652 · .977 | .444 · .444 · .500 · .471 · .778 | .037 · .296 · .667 · .202 · .926 | .062 · .438 · .625 · .232 · .750 |
| pre (chosen) | .455 · .727 · .841 · .580 · 1.000 | .222 · .444 · .500 · .328 · .889 | .222 · .556 · .704 · .402 · .926 | .125 · .562 · .625 · .360 · .812 |

## Reading

- The v0.11.0 harm reproduces on these corpora: plain fusion drops convention R@1 from .222 to .037 and decline R@1 from .125 to .062. Capping the dense weight does not repair it.
- Pre-fusion removes the harm. Convention and decline R@1 equal the no-dense arm, and R@4 and MRR rise on both kinds.
- Pre-fusion keeps part of the find gain: find R@1 goes from .432 to .455 (plain .568), and all 44 find tasks have the target in the pack.
- Reuse R@1 falls from .333 to .222 on 18 tasks, too few to tell from noise.
- Over all tasks, pre helps subform (R@4 .569 to .639, MRR .455 to .493). On formslot it is below no-dense on R@1, R@4 and MRR (.273 to .212, .576 to .545, .416 to .380). Pack recall rises on both.
- On the tuning halves alone, cap 0.4 won on MRR (.606 over 53 tasks, against .578 plain and .536 pre). It was not chosen: it leaves convention R@1 at .037 and has the worst formslot held-out R@1 (.059). Pre ties cap 0.4 on tuning R@4 (.679) and is the best dense variant on the held-out halves. Choosing pre over the tuning winner is a judgement, made because the aim of this run was to stop the convention and decline harm.
- Held-out R@1 stays at or below no-dense: subform .286 to .257, formslot .235 to .118.

## Limits

- 105 tasks in cells of 16 to 37; differences under about 0.05 are noise.
- formvis was not embedded, so `gold.jsonl`, where v0.11.0 measured the harm, was not re-run.
- The halves are the cluster-separated split, so the numbers are indicative.

## Verdict

Dense stays opt-in. With pre-fusion it no longer harms convention and decline retrieval, and R@4, R@10 and pack recall improve. Held-out R@1 does not improve, reuse R@1 falls, and the cost is unchanged: a 150 MB model, an external ONNX Runtime library and minutes of embedding per repository. Reconsider after embedding formvis, re-running `gold.jsonl` with this fusion, and comparing a code model.
