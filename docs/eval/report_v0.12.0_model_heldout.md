# formslot model evaluation

formslot v0.11.0 binary · gold set `docs/eval/gold_heldout.jsonl` · 24 tasks sampled (seed 1, stratified by kind) · 2 run(s) per task per arm

Reading it plainly: on held-out subform tasks, which formslot was not tuned on, the pack did not raise the pass rate. formslot passed 95.8 percent of runs and the baseline 97.9 percent (n 48 each; 95 percent Wilson intervals about 86 to 99 and 89 to 100, so the gap is within noise). The pack halved input tokens (48,151 against 100,656 per run), cost (0.031 against 0.059 USD per run), median wall time (8.3 against 16.9 s) and tool calls (1.2 against 6.1). The formslot-corpus half of this run (16 tasks) was cut off twice by container restarts after 19 of 64 calls and is not reported. Spend this turn: about 6.3 USD.

## Set

- sampled kinds: convention 6, decline 3, find 10, reuse 5
- corpus subform at 1706f026764bb896fa34235e883111fbada733b9
- model: claude-sonnet-5-5 (CLI binary /opt/node22/bin/claude; `--model` not given, the CLI's default)
- sampled task ids: h002 h006 h009 h010 h019 h020 h022 h024 h026 h032 h038 h041 h042 h047 h049 h055 h056 h057 h063 h069 h072 h085 h086 h088

## Results

| arm | kind | n | pass % | full % | timeouts | errors | input tok | fresh input tok | output tok | cost USD | wall s median | tool calls |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| formslot | all | 48 | 95.8 | 58.3 | 0 | 0 | 48151 | 4986 | 570 | 0.031 | 8.3 | 1.2 |
| formslot | convention | 12 | 83.3 | 50.0 | - | - | - | - | - | - | - | - |
| formslot | decline | 6 | 100.0 | 0.0 | - | - | - | - | - | - | - | - |
| formslot | find | 20 | 100.0 | 70.0 | - | - | - | - | - | - | - | - |
| formslot | reuse | 10 | 100.0 | 80.0 | - | - | - | - | - | - | - | - |
| baseline | all | 48 | 97.9 | 62.5 | 0 | 0 | 100656 | 9329 | 1230 | 0.059 | 16.9 | 6.1 |
| baseline | convention | 12 | 91.7 | 33.3 | - | - | - | - | - | - | - | - |
| baseline | decline | 6 | 100.0 | 0.0 | - | - | - | - | - | - | - | - |
| baseline | find | 20 | 100.0 | 90.0 | - | - | - | - | - | - | - | - |
| baseline | reuse | 10 | 100.0 | 80.0 | - | - | - | - | - | - | - | - |

pass: at least one expected file, symbol, slot or rule named; full: every expected item named. Tokens, cost and tool calls are means per run as the CLI reports them (input counts uncached, cache-creation and cache-read tokens; fresh input leaves out cache reads). A timeout or an answer that is not the JSON shape counts as a fail.

## Arms

- formslot: `claude -p` with the 4000-token formslot pack for the task in the prompt, tools Read, Grep and Glob.
- baseline: the same prompt without the pack, same tools.
- both: working directory is a clean export of the corpus at the indexed commit under the data folder; `--tools Read,Grep,Glob`, `--disallowedTools Write Edit MultiEdit Bash NotebookEdit`, `--permission-mode default`, `--safe-mode` (no CLAUDE.md, hooks, plugins or MCP servers), `--no-session-persistence`, 120 s limit per run; 3 runs in parallel.

## Gates

- Pass rate within 1 point: measured, not met. formslot 95.8 percent against 97.9 percent without it (n 48 and 48 runs, difference -2.1 points).
- Token change (model arm, measured by the CLI): input per run 48151 with formslot against 100656 without, -52.2 percent (fresh input, cache reads left out: 4986 against 9329, -46.6 percent); output 570 against 1230, -53.6 percent; cost per run $0.031 against $0.059.
- Spend: $4.31 in total (formslot $1.48, baseline $2.83).

## Per task

| task | kind | arm | run | pass | full | timeout | tools | input tok | output tok | cost USD | wall s | note |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| h002 | find | formslot | 0 | true | false | false | 1 | 48130 | 687 | 0.044 | 10.3 |  |
| h002 | find | baseline | 0 | true | true | false | 12 | 167067 | 1917 | 0.087 | 30.3 |  |
| h002 | find | formslot | 1 | true | true | false | 1 | 49516 | 727 | 0.050 | 13.7 |  |
| h002 | find | baseline | 1 | true | true | false | 11 | 199367 | 2192 | 0.092 | 25.8 |  |
| h006 | find | formslot | 0 | true | true | false | 1 | 48514 | 582 | 0.044 | 7.6 |  |
| h006 | find | baseline | 0 | true | false | false | 7 | 114993 | 1440 | 0.073 | 16.9 |  |
| h006 | find | formslot | 1 | true | true | false | 1 | 48813 | 693 | 0.016 | 8.1 |  |
| h006 | find | baseline | 1 | true | false | false | 8 | 130289 | 1368 | 0.107 | 20.1 |  |
| h009 | find | formslot | 0 | true | false | false | 0 | 24174 | 483 | 0.039 | 10.1 |  |
| h009 | find | baseline | 0 | true | true | false | 8 | 148629 | 1661 | 0.084 | 38.2 |  |
| h009 | find | formslot | 1 | true | false | false | 0 | 24174 | 567 | 0.009 | 6.4 |  |
| h009 | find | baseline | 1 | true | true | false | 8 | 127834 | 1655 | 0.086 | 25.2 |  |
| h010 | find | formslot | 0 | true | true | false | 1 | 46965 | 886 | 0.044 | 12.1 |  |
| h010 | find | baseline | 0 | true | true | false | 10 | 156697 | 1664 | 0.092 | 30.1 |  |
| h010 | find | formslot | 1 | true | true | false | 2 | 49250 | 1053 | 0.028 | 10.0 |  |
| h010 | find | baseline | 1 | true | true | false | 12 | 183168 | 1906 | 0.093 | 24.2 |  |
| h019 | find | formslot | 0 | true | false | false | 1 | 50053 | 477 | 0.048 | 6.1 |  |
| h019 | find | baseline | 0 | true | true | false | 7 | 110081 | 1261 | 0.061 | 21.0 |  |
| h019 | find | formslot | 1 | true | false | false | 1 | 50053 | 477 | 0.017 | 7.4 |  |
| h019 | find | baseline | 1 | true | true | false | 8 | 138434 | 1697 | 0.067 | 24.1 |  |
| h020 | find | formslot | 0 | true | false | false | 2 | 47280 | 1032 | 0.048 | 10.1 |  |
| h020 | find | baseline | 0 | true | true | false | 10 | 117082 | 2253 | 0.078 | 22.3 |  |
| h020 | find | formslot | 1 | true | true | false | 4 | 70549 | 1333 | 0.027 | 15.7 |  |
| h020 | find | baseline | 1 | true | true | false | 8 | 120955 | 1930 | 0.075 | 24.3 |  |
| h022 | find | formslot | 0 | true | true | false | 1 | 50467 | 786 | 0.052 | 7.5 |  |
| h022 | find | baseline | 0 | true | true | false | 8 | 85775 | 1370 | 0.056 | 15.6 |  |
| h022 | find | formslot | 1 | true | true | false | 1 | 50478 | 786 | 0.020 | 8.6 |  |
| h022 | find | baseline | 1 | true | true | false | 8 | 86733 | 1577 | 0.054 | 24.1 |  |
| h024 | find | formslot | 0 | true | true | false | 2 | 74771 | 887 | 0.056 | 12.1 |  |
| h024 | find | baseline | 0 | true | true | false | 4 | 62188 | 1057 | 0.050 | 15.2 |  |
| h024 | find | formslot | 1 | true | true | false | 2 | 75152 | 944 | 0.024 | 9.7 |  |
| h024 | find | baseline | 1 | true | true | false | 4 | 62179 | 1015 | 0.044 | 10.8 |  |
| h026 | find | formslot | 0 | true | true | false | 3 | 75762 | 717 | 0.054 | 10.3 |  |
| h026 | find | baseline | 0 | true | true | false | 5 | 83153 | 1074 | 0.047 | 10.7 |  |
| h026 | find | formslot | 1 | true | true | false | 2 | 49804 | 545 | 0.017 | 6.7 |  |
| h026 | find | baseline | 1 | true | true | false | 5 | 82180 | 988 | 0.037 | 10.6 |  |
| h032 | find | formslot | 0 | true | true | false | 2 | 74028 | 562 | 0.050 | 13.3 |  |
| h032 | find | baseline | 0 | true | true | false | 2 | 55116 | 549 | 0.024 | 10.5 |  |
| h032 | find | formslot | 1 | true | true | false | 2 | 73884 | 601 | 0.021 | 11.4 |  |
| h032 | find | baseline | 1 | true | true | false | 3 | 75449 | 717 | 0.025 | 11.6 |  |
| h038 | reuse | formslot | 0 | true | true | false | 1 | 48405 | 520 | 0.043 | 7.1 |  |
| h038 | reuse | baseline | 0 | true | false | false | 7 | 121787 | 1242 | 0.076 | 19.8 |  |
| h038 | reuse | formslot | 1 | true | true | false | 1 | 48387 | 462 | 0.012 | 9.8 |  |
| h038 | reuse | baseline | 1 | true | false | false | 8 | 134566 | 1465 | 0.095 | 22.1 |  |
| h041 | reuse | formslot | 0 | true | true | false | 1 | 48634 | 534 | 0.043 | 7.6 |  |
| h041 | reuse | baseline | 0 | true | true | false | 12 | 212486 | 2566 | 0.095 | 36.6 |  |
| h041 | reuse | formslot | 1 | true | true | false | 1 | 48633 | 705 | 0.014 | 7.6 |  |
| h041 | reuse | baseline | 1 | true | true | false | 7 | 130862 | 1486 | 0.083 | 25.4 |  |
| h042 | reuse | formslot | 0 | true | true | false | 0 | 24027 | 194 | 0.036 | 3.6 |  |
| h042 | reuse | baseline | 0 | true | true | false | 7 | 101191 | 1034 | 0.049 | 17.4 |  |
| h042 | reuse | formslot | 1 | true | true | false | 0 | 24027 | 179 | 0.005 | 5.0 |  |
| h042 | reuse | baseline | 1 | true | true | false | 7 | 121309 | 1305 | 0.048 | 16.3 |  |
| h047 | reuse | formslot | 0 | true | false | false | 0 | 23057 | 404 | 0.034 | 10.3 |  |
| h047 | reuse | baseline | 0 | true | true | false | 6 | 90102 | 1493 | 0.066 | 15.5 |  |
| h047 | reuse | formslot | 1 | true | false | false | 0 | 23057 | 404 | 0.007 | 4.3 |  |
| h047 | reuse | baseline | 1 | true | true | false | 5 | 63213 | 1529 | 0.054 | 16.9 |  |
| h049 | reuse | formslot | 0 | true | true | false | 4 | 79483 | 884 | 0.064 | 12.2 |  |
| h049 | reuse | baseline | 0 | true | true | false | 5 | 80455 | 943 | 0.046 | 11.2 |  |
| h049 | reuse | formslot | 1 | true | true | false | 5 | 101635 | 1131 | 0.032 | 18.6 |  |
| h049 | reuse | baseline | 1 | true | true | false | 5 | 80674 | 945 | 0.042 | 12.5 |  |
| h055 | convention | formslot | 0 | false | false | false | 3 | 98571 | 634 | 0.052 | 10.2 |  |
| h055 | convention | baseline | 0 | true | false | false | 3 | 62227 | 834 | 0.051 | 17.2 |  |
| h055 | convention | formslot | 1 | false | false | false | 1 | 48637 | 324 | 0.011 | 6.7 |  |
| h055 | convention | baseline | 1 | true | false | false | 5 | 96837 | 966 | 0.034 | 14.6 |  |
| h056 | convention | formslot | 0 | true | false | false | 1 | 47139 | 404 | 0.039 | 9.7 |  |
| h056 | convention | baseline | 0 | true | false | false | 5 | 97007 | 1092 | 0.070 | 13.7 |  |
| h056 | convention | formslot | 1 | true | false | false | 1 | 47145 | 409 | 0.011 | 9.0 |  |
| h056 | convention | baseline | 1 | false | false | false | 2 | 65735 | 693 | 0.041 | 12.2 |  |
| h057 | convention | formslot | 0 | true | false | false | 1 | 46803 | 182 | 0.036 | 4.8 |  |
| h057 | convention | baseline | 0 | true | false | false | 2 | 40117 | 510 | 0.034 | 8.2 |  |
| h057 | convention | formslot | 1 | true | false | false | 1 | 46809 | 179 | 0.008 | 6.9 |  |
| h057 | convention | baseline | 1 | true | false | false | 3 | 57170 | 488 | 0.024 | 8.9 |  |
| h063 | convention | formslot | 0 | true | true | false | 0 | 24133 | 202 | 0.036 | 6.7 |  |
| h063 | convention | baseline | 0 | true | true | false | 5 | 98114 | 1162 | 0.044 | 29.7 |  |
| h063 | convention | formslot | 1 | true | true | false | 0 | 24133 | 202 | 0.036 | 4.2 |  |
| h063 | convention | baseline | 1 | true | true | false | 5 | 98129 | 988 | 0.042 | 18.3 |  |
| h069 | convention | formslot | 0 | true | true | false | 0 | 22955 | 272 | 0.032 | 5.2 |  |
| h069 | convention | baseline | 0 | true | true | false | 4 | 82603 | 808 | 0.043 | 14.6 |  |
| h069 | convention | formslot | 1 | true | true | false | 0 | 22955 | 252 | 0.006 | 4.3 |  |
| h069 | convention | baseline | 1 | true | true | false | 6 | 85979 | 1065 | 0.048 | 18.1 |  |
| h072 | convention | formslot | 0 | true | true | false | 1 | 46688 | 338 | 0.038 | 8.4 |  |
| h072 | convention | baseline | 0 | true | false | false | 4 | 76560 | 816 | 0.032 | 16.1 |  |
| h072 | convention | formslot | 1 | true | true | false | 1 | 46633 | 355 | 0.011 | 8.3 |  |
| h072 | convention | baseline | 1 | true | false | false | 2 | 57002 | 594 | 0.024 | 10.7 |  |
| h085 | decline | formslot | 0 | true | false | false | 1 | 49200 | 468 | 0.047 | 11.8 |  |
| h085 | decline | baseline | 0 | true | false | false | 5 | 107061 | 844 | 0.093 | 26.3 |  |
| h085 | decline | formslot | 1 | true | false | false | 1 | 49013 | 527 | 0.016 | 9.8 |  |
| h085 | decline | baseline | 1 | true | false | false | 5 | 75491 | 659 | 0.091 | 14.7 |  |
| h086 | decline | formslot | 0 | true | false | false | 2 | 46054 | 789 | 0.042 | 11.5 |  |
| h086 | decline | baseline | 0 | true | false | false | 4 | 59321 | 792 | 0.035 | 16.1 |  |
| h086 | decline | formslot | 1 | true | false | false | 1 | 45276 | 573 | 0.012 | 7.6 |  |
| h086 | decline | baseline | 1 | true | false | false | 4 | 59989 | 924 | 0.039 | 12.2 |  |
| h088 | decline | formslot | 0 | true | false | false | 0 | 23978 | 502 | 0.039 | 5.3 |  |
| h088 | decline | baseline | 0 | true | false | false | 5 | 84045 | 1170 | 0.052 | 14.1 |  |
| h088 | decline | formslot | 1 | true | false | false | 0 | 23978 | 528 | 0.009 | 6.8 |  |
| h088 | decline | baseline | 1 | true | false | false | 5 | 84106 | 1356 | 0.048 | 22.0 |  |
