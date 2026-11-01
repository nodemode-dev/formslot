# formslot model evaluation

formslot 0.13.0 · gold set `docs/eval/gold_heldout.jsonl` · 16 tasks sampled (seed 1, stratified by kind) · 2 run(s) per task per arm

## Set

- sampled kinds: convention 4, decline 3, find 7, reuse 2
- corpus formslot at e8c68ab01cd0389ed45dd4ca3976a3a01f651419
- model: claude-sonnet-5-5 (CLI binary /opt/node22/bin/claude; `--model` not given, the CLI's default)
- sampled task ids: h073 h074 h076 h078 h081 h093 h096 h097 h101 h102 h103 h104 h108 h111 h112 h113

## Results

| arm | kind | n | pass % | full % | timeouts | errors | input tok | fresh input tok | output tok | cost USD | wall s median | tool calls |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| formslot | all | 32 | 96.9 | 59.4 | 0 | 0 | 52781 | 3919 | 551 | 0.027 | 6.1 | 1.3 |
| formslot | convention | 8 | 100.0 | 25.0 | - | - | - | - | - | - | - | - |
| formslot | decline | 6 | 83.3 | 50.0 | - | - | - | - | - | - | - | - |
| formslot | find | 14 | 100.0 | 71.4 | - | - | - | - | - | - | - | - |
| formslot | reuse | 4 | 100.0 | 100.0 | - | - | - | - | - | - | - | - |
| baseline | all | 32 | 100.0 | 46.9 | 0 | 0 | 94466 | 8007 | 1095 | 0.052 | 10.6 | 6.4 |
| baseline | convention | 8 | 100.0 | 0.0 | - | - | - | - | - | - | - | - |
| baseline | decline | 6 | 100.0 | 33.3 | - | - | - | - | - | - | - | - |
| baseline | find | 14 | 100.0 | 78.6 | - | - | - | - | - | - | - | - |
| baseline | reuse | 4 | 100.0 | 50.0 | - | - | - | - | - | - | - | - |

pass: at least one expected file, symbol, slot or rule named; full: every expected item named. Tokens, cost and tool calls are means per run as the CLI reports them (input counts uncached, cache-creation and cache-read tokens; fresh input leaves out cache reads). A timeout or an answer that is not the JSON shape counts as a fail.

## Arms

- formslot: `claude -p` with the 4000-token formslot pack for the task in the prompt, tools Read, Grep and Glob.
- baseline: the same prompt without the pack, same tools.
- both: working directory is a clean export of the corpus at the indexed commit under the data folder; `--tools Read,Grep,Glob`, `--disallowedTools Write Edit MultiEdit Bash NotebookEdit`, `--permission-mode default`, `--safe-mode` (no CLAUDE.md, hooks, plugins or MCP servers), `--no-session-persistence`, 120 s limit per run; 2 runs in parallel.

## Gates

- Pass rate within 1 point: measured, not met. formslot 96.9 percent against 100.0 percent without it (n 32 and 32 runs, difference -3.1 points).
- Token change (model arm, measured by the CLI): input per run 52781 with formslot against 94466 without, -44.1 percent (fresh input, cache reads left out: 3919 against 8007, -51.1 percent); output 551 against 1095, -49.7 percent; cost per run $0.027 against $0.052.
- Spend: $2.52 in total (formslot $0.86, baseline $1.66).

## Per task

| task | kind | arm | run | pass | full | timeout | tools | input tok | output tok | cost USD | wall s | note |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| h073 | convention | formslot | 0 | true | true | false | 1 | 48668 | 552 | 0.013 | 7.8 |  |
| h073 | convention | baseline | 0 | true | false | false | 6 | 82968 | 1049 | 0.042 | 12.7 |  |
| h073 | convention | formslot | 1 | true | false | false | 0 | 24117 | 412 | 0.007 | 4.3 |  |
| h073 | convention | baseline | 1 | true | false | false | 6 | 80198 | 806 | 0.031 | 9.5 |  |
| h074 | convention | formslot | 0 | true | false | false | 0 | 23202 | 126 | 0.004 | 3.5 |  |
| h074 | convention | baseline | 0 | true | false | false | 7 | 101458 | 1237 | 0.041 | 10.8 |  |
| h074 | convention | formslot | 1 | true | true | false | 0 | 23202 | 91 | 0.004 | 2.9 |  |
| h074 | convention | baseline | 1 | true | false | false | 7 | 106170 | 1297 | 0.051 | 11.7 |  |
| h076 | convention | formslot | 0 | true | false | false | 0 | 23908 | 356 | 0.007 | 3.8 |  |
| h076 | convention | baseline | 0 | true | false | false | 3 | 56790 | 569 | 0.024 | 6.2 |  |
| h076 | convention | formslot | 1 | true | false | false | 0 | 23908 | 503 | 0.008 | 4.6 |  |
| h076 | convention | baseline | 1 | true | false | false | 3 | 56443 | 545 | 0.022 | 6.8 |  |
| h078 | convention | formslot | 0 | true | false | false | 2 | 45138 | 761 | 0.039 | 6.7 |  |
| h078 | convention | baseline | 0 | true | false | false | 9 | 120302 | 1530 | 0.080 | 15.2 |  |
| h078 | convention | formslot | 1 | true | false | false | 1 | 44836 | 594 | 0.013 | 5.8 |  |
| h078 | convention | baseline | 1 | true | false | false | 8 | 79423 | 1420 | 0.041 | 11.6 |  |
| h081 | decline | formslot | 0 | false | false | false | 2 | 73314 | 687 | 0.048 | 7.0 |  |
| h081 | decline | baseline | 0 | true | true | false | 8 | 180325 | 1157 | 0.081 | 14.5 |  |
| h081 | decline | formslot | 1 | true | true | false | 2 | 74083 | 748 | 0.020 | 7.5 |  |
| h081 | decline | baseline | 1 | true | true | false | 5 | 93541 | 769 | 0.061 | 9.9 |  |
| h093 | decline | formslot | 0 | true | true | false | 0 | 23775 | 165 | 0.035 | 3.6 |  |
| h093 | decline | baseline | 0 | true | false | false | 6 | 84844 | 952 | 0.054 | 10.8 |  |
| h093 | decline | formslot | 1 | true | true | false | 0 | 23775 | 165 | 0.005 | 3.2 |  |
| h093 | decline | baseline | 1 | true | false | false | 5 | 83059 | 887 | 0.040 | 10.3 |  |
| h096 | decline | formslot | 0 | true | false | false | 0 | 24273 | 147 | 0.037 | 4.9 |  |
| h096 | decline | baseline | 0 | true | false | false | 8 | 137984 | 1360 | 0.068 | 14.8 |  |
| h096 | decline | formslot | 1 | true | false | false | 0 | 24273 | 147 | 0.004 | 2.8 |  |
| h096 | decline | baseline | 1 | true | false | false | 10 | 195148 | 1573 | 0.076 | 18.0 |  |
| h097 | find | formslot | 0 | true | false | false | 0 | 24322 | 195 | 0.037 | 3.2 |  |
| h097 | find | baseline | 0 | true | false | false | 5 | 76995 | 848 | 0.037 | 9.6 |  |
| h097 | find | formslot | 1 | true | false | false | 0 | 24322 | 195 | 0.005 | 3.0 |  |
| h097 | find | baseline | 1 | true | true | false | 6 | 80859 | 954 | 0.038 | 10.3 |  |
| h101 | find | formslot | 0 | true | true | false | 1 | 48775 | 452 | 0.043 | 5.5 |  |
| h101 | find | baseline | 0 | true | true | false | 9 | 83867 | 1534 | 0.057 | 14.0 |  |
| h101 | find | formslot | 1 | true | true | false | 2 | 73527 | 596 | 0.017 | 7.6 |  |
| h101 | find | baseline | 1 | true | true | false | 6 | 85281 | 962 | 0.047 | 10.3 |  |
| h102 | find | formslot | 0 | true | true | false | 3 | 96468 | 817 | 0.052 | 8.2 |  |
| h102 | find | baseline | 0 | true | true | false | 4 | 58853 | 690 | 0.038 | 8.2 |  |
| h102 | find | formslot | 1 | true | true | false | 1 | 47694 | 404 | 0.011 | 5.7 |  |
| h102 | find | baseline | 1 | true | true | false | 5 | 82253 | 965 | 0.041 | 10.2 |  |
| h103 | find | formslot | 0 | true | true | false | 2 | 74174 | 762 | 0.052 | 8.9 |  |
| h103 | find | baseline | 0 | true | true | false | 7 | 85343 | 1095 | 0.064 | 11.6 |  |
| h103 | find | formslot | 1 | true | true | false | 3 | 99873 | 1157 | 0.030 | 13.2 |  |
| h103 | find | baseline | 1 | true | true | false | 6 | 94501 | 971 | 0.064 | 9.9 |  |
| h104 | find | formslot | 0 | true | false | false | 1 | 49257 | 886 | 0.049 | 7.4 |  |
| h104 | find | baseline | 0 | true | false | false | 7 | 108947 | 1303 | 0.060 | 12.6 |  |
| h104 | find | formslot | 1 | true | false | false | 0 | 24185 | 664 | 0.010 | 5.6 |  |
| h104 | find | baseline | 1 | true | false | false | 9 | 112614 | 1612 | 0.065 | 14.0 |  |
| h108 | find | formslot | 0 | true | true | false | 4 | 76374 | 1195 | 0.067 | 9.6 |  |
| h108 | find | baseline | 0 | true | true | false | 5 | 62004 | 624 | 0.048 | 7.2 |  |
| h108 | find | formslot | 1 | true | true | false | 6 | 128527 | 864 | 0.034 | 9.3 |  |
| h108 | find | baseline | 1 | true | true | false | 5 | 62003 | 633 | 0.043 | 6.8 |  |
| h111 | find | formslot | 0 | true | true | false | 1 | 49068 | 762 | 0.047 | 6.6 |  |
| h111 | find | baseline | 0 | true | true | false | 9 | 120602 | 1884 | 0.086 | 14.2 |  |
| h111 | find | formslot | 1 | true | true | false | 1 | 49000 | 693 | 0.015 | 6.0 |  |
| h111 | find | baseline | 1 | true | true | false | 6 | 92048 | 1245 | 0.062 | 10.2 |  |
| h112 | reuse | formslot | 0 | true | true | false | 3 | 98850 | 689 | 0.056 | 7.9 |  |
| h112 | reuse | baseline | 0 | true | false | false | 5 | 60226 | 769 | 0.040 | 8.9 |  |
| h112 | reuse | formslot | 1 | true | true | false | 2 | 74170 | 588 | 0.020 | 6.1 |  |
| h112 | reuse | baseline | 1 | true | false | false | 5 | 59052 | 813 | 0.035 | 9.0 |  |
| h113 | reuse | formslot | 0 | true | true | false | 2 | 74995 | 686 | 0.052 | 7.1 |  |
| h113 | reuse | baseline | 0 | true | true | false | 9 | 133521 | 1660 | 0.074 | 17.2 |  |
| h113 | reuse | formslot | 1 | true | true | false | 2 | 74931 | 575 | 0.020 | 6.2 |  |
| h113 | reuse | baseline | 1 | true | true | false | 7 | 105276 | 1314 | 0.047 | 12.7 |  |
