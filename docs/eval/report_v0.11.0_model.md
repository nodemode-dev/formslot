# formslot model evaluation

formslot 0.10.2 · gold set `docs/eval/gold.jsonl` · 30 tasks sampled (seed 1, stratified by kind) · 1 run(s) per task per arm

## Set

- sampled kinds: convention 4, decline 3, find 14, reuse 9
- corpus formvis at 8fe4ce45bcfa1b85c509db4bb34da29942db964a
- model: claude-sonnet-5-5 (CLI binary /opt/node22/bin/claude; `--model` not given, the CLI's default)
- sampled task ids: g007 g008 g010 g017 g019 g021 g022 g023 g046 g047 g051 g054 g057 g061 g072 g073 g083 g086 g093 g098 g102 g103 g105 g121 g130 g132 g134 g136 g137 g147

## Results

| arm | kind | n | pass % | full % | timeouts | errors | input tok | fresh input tok | output tok | cost USD | wall s median | tool calls |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| formslot | all | 30 | 100.0 | 56.7 | 0 | 0 | 53163 | 8171 | 798 | 0.046 | 8.3 | 1.5 |
| formslot | convention | 4 | 100.0 | 0.0 | - | - | - | - | - | - | - | - |
| formslot | decline | 3 | 100.0 | 33.3 | - | - | - | - | - | - | - | - |
| formslot | find | 14 | 100.0 | 50.0 | - | - | - | - | - | - | - | - |
| formslot | reuse | 9 | 100.0 | 100.0 | - | - | - | - | - | - | - | - |
| baseline | all | 30 | 96.7 | 36.7 | 0 | 0 | 132651 | 13632 | 1519 | 0.082 | 17.0 | 7.4 |
| baseline | convention | 4 | 100.0 | 0.0 | - | - | - | - | - | - | - | - |
| baseline | decline | 3 | 100.0 | 0.0 | - | - | - | - | - | - | - | - |
| baseline | find | 14 | 92.9 | 50.0 | - | - | - | - | - | - | - | - |
| baseline | reuse | 9 | 100.0 | 44.4 | - | - | - | - | - | - | - | - |

pass: at least one expected file, symbol, slot or rule named; full: every expected item named. Tokens, cost and tool calls are means per run as the CLI reports them (input counts uncached, cache-creation and cache-read tokens; fresh input leaves out cache reads). A timeout or an answer that is not the JSON shape counts as a fail.

## Arms

- formslot: `claude -p` with the 4000-token formslot pack for the task in the prompt, tools Read, Grep and Glob.
- baseline: the same prompt without the pack, same tools.
- both: working directory is a clean export of the corpus at the indexed commit under the data folder; `--tools Read,Grep,Glob`, `--disallowedTools Write Edit MultiEdit Bash NotebookEdit`, `--permission-mode default`, `--safe-mode` (no CLAUDE.md, hooks, plugins or MCP servers), `--no-session-persistence`, 120 s limit per run; 3 runs in parallel.

## Gates

- Pass rate within 1 point: measured, met. formslot 100.0 percent against 96.7 percent without it (n 30 and 30 runs, difference +3.3 points).
- Token change (model arm, measured by the CLI): input per run 53163 with formslot against 132651 without, -59.9 percent (fresh input, cache reads left out: 8171 against 13632, -40.1 percent); output 798 against 1519, -47.5 percent; cost per run $0.046 against $0.082.
- Spend: $3.84 in total (formslot $1.38, baseline $2.45).

## Reading the numbers

- 30 tasks and one run each: one task moves a pass rate by 3.3 points, so "within 1 point" is met here by a margin that is one task (the baseline missed one find task). Treat it as: no measured loss at n 30, not as a proven gain. Rerun with `--sample 0 --runs 3` for a tighter figure; at this run's cost per task that is about $50.
- Pass is lenient by the contract (one expected item named). The full-match rate is the stricter view: 56.7 percent with the pack against 36.7 percent without; on convention tasks neither arm names every expected rule, because the expected rule phrases are matched literally and a paraphrase is a miss.
- `--safe-mode` removes the corpus's CLAUDE.md and hooks from both arms, so the baseline cannot read owner rules from CLAUDE.md as a real agent would; decline and convention tasks favour the pack more than a normal checkout would.
- Tool calls count every tool_use block, parallel calls included. Input tokens include cache reads, which dominate; the CLI's system prompt (about 17,000 tokens) is in both arms.
- The gold tasks are written from commits of the corpus (the `source` field); the indexed tree may already contain the code those commits added, which would affect both arms alike. Not checked here.
- The report header shows formslot 0.10.2 because the crate version is not bumped in this change.

## Per task

| task | kind | arm | run | pass | full | timeout | tools | input tok | output tok | cost USD | wall s | note |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| g007 | find | formslot | 0 | true | false | false | 0 | 24511 | 898 | 0.045 | 7.4 |  |
| g007 | find | baseline | 0 | true | false | false | 4 | 63336 | 1544 | 0.057 | 13.3 |  |
| g008 | find | formslot | 0 | true | false | false | 2 | 49491 | 1352 | 0.056 | 11.7 |  |
| g008 | find | baseline | 0 | true | false | false | 6 | 110727 | 1517 | 0.112 | 14.0 |  |
| g010 | find | formslot | 0 | true | true | false | 0 | 24715 | 1128 | 0.015 | 7.6 |  |
| g010 | find | baseline | 0 | true | true | false | 6 | 92798 | 1517 | 0.075 | 14.5 |  |
| g017 | find | formslot | 0 | true | true | false | 0 | 24596 | 286 | 0.039 | 3.4 |  |
| g017 | find | baseline | 0 | true | true | false | 7 | 169001 | 1147 | 0.076 | 17.3 |  |
| g019 | find | formslot | 0 | true | false | false | 0 | 23269 | 831 | 0.039 | 8.4 |  |
| g019 | find | baseline | 0 | true | true | false | 11 | 199900 | 2740 | 0.157 | 26.7 |  |
| g021 | find | formslot | 0 | true | true | false | 1 | 48914 | 1192 | 0.051 | 9.4 |  |
| g021 | find | baseline | 0 | true | true | false | 8 | 142579 | 2078 | 0.086 | 20.2 |  |
| g022 | find | formslot | 0 | true | true | false | 1 | 49903 | 677 | 0.018 | 8.4 |  |
| g022 | find | baseline | 0 | true | true | false | 14 | 235962 | 2524 | 0.099 | 27.9 |  |
| g023 | find | formslot | 0 | true | true | false | 1 | 48568 | 747 | 0.046 | 7.8 |  |
| g023 | find | baseline | 0 | true | true | false | 14 | 256043 | 2935 | 0.128 | 31.2 |  |
| g046 | find | formslot | 0 | true | false | false | 1 | 49001 | 823 | 0.048 | 8.2 |  |
| g046 | find | baseline | 0 | true | false | false | 5 | 87558 | 1288 | 0.058 | 16.7 |  |
| g047 | find | formslot | 0 | true | false | false | 3 | 74492 | 1066 | 0.057 | 11.6 |  |
| g047 | find | baseline | 0 | true | false | false | 4 | 63384 | 971 | 0.051 | 12.8 |  |
| g051 | find | formslot | 0 | true | true | false | 2 | 53055 | 922 | 0.064 | 7.5 |  |
| g051 | find | baseline | 0 | true | false | false | 8 | 166940 | 1507 | 0.102 | 22.7 |  |
| g054 | find | formslot | 0 | true | false | false | 2 | 48704 | 839 | 0.047 | 9.5 |  |
| g054 | find | baseline | 0 | true | false | false | 8 | 119271 | 1712 | 0.084 | 20.7 |  |
| g057 | find | formslot | 0 | true | false | false | 0 | 24315 | 659 | 0.042 | 6.2 |  |
| g057 | find | baseline | 0 | false | false | false | 7 | 150772 | 1561 | 0.089 | 18.8 |  |
| g061 | find | formslot | 0 | true | true | false | 2 | 50670 | 1040 | 0.058 | 9.3 |  |
| g061 | find | baseline | 0 | true | true | false | 7 | 124891 | 1598 | 0.089 | 16.2 |  |
| g072 | reuse | formslot | 0 | true | true | false | 0 | 24370 | 471 | 0.040 | 7.3 |  |
| g072 | reuse | baseline | 0 | true | false | false | 7 | 111478 | 1225 | 0.067 | 15.9 |  |
| g073 | reuse | formslot | 0 | true | true | false | 8 | 215901 | 2101 | 0.097 | 22.8 |  |
| g073 | reuse | baseline | 0 | true | false | false | 11 | 178077 | 2573 | 0.100 | 24.3 |  |
| g083 | reuse | formslot | 0 | true | true | false | 0 | 24625 | 398 | 0.040 | 4.6 |  |
| g083 | reuse | baseline | 0 | true | false | false | 6 | 126382 | 1263 | 0.077 | 13.5 |  |
| g086 | reuse | formslot | 0 | true | true | false | 0 | 22883 | 219 | 0.031 | 3.6 |  |
| g086 | reuse | baseline | 0 | true | true | false | 9 | 173658 | 1306 | 0.076 | 24.4 |  |
| g093 | reuse | formslot | 0 | true | true | false | 1 | 46980 | 664 | 0.041 | 12.5 |  |
| g093 | reuse | baseline | 0 | true | false | false | 7 | 150027 | 1424 | 0.096 | 19.7 |  |
| g098 | reuse | formslot | 0 | true | true | false | 5 | 118683 | 1428 | 0.101 | 13.4 |  |
| g098 | reuse | baseline | 0 | true | true | false | 12 | 193423 | 2064 | 0.125 | 25.1 |  |
| g102 | reuse | formslot | 0 | true | true | false | 4 | 102703 | 962 | 0.063 | 9.8 |  |
| g102 | reuse | baseline | 0 | true | false | false | 9 | 160402 | 1873 | 0.107 | 18.3 |  |
| g103 | reuse | formslot | 0 | true | true | false | 0 | 23967 | 350 | 0.007 | 3.8 |  |
| g103 | reuse | baseline | 0 | true | true | false | 7 | 113077 | 1200 | 0.061 | 14.0 |  |
| g105 | reuse | formslot | 0 | true | true | false | 0 | 23279 | 385 | 0.035 | 4.7 |  |
| g105 | reuse | baseline | 0 | true | true | false | 6 | 83106 | 1074 | 0.047 | 11.1 |  |
| g121 | convention | formslot | 0 | true | false | false | 2 | 73965 | 803 | 0.020 | 12.9 |  |
| g121 | convention | baseline | 0 | true | false | false | 10 | 182851 | 1835 | 0.095 | 18.8 |  |
| g130 | convention | formslot | 0 | true | false | false | 0 | 24187 | 248 | 0.037 | 3.7 |  |
| g130 | convention | baseline | 0 | true | false | false | 3 | 66857 | 552 | 0.046 | 11.3 |  |
| g132 | convention | formslot | 0 | true | false | false | 0 | 24103 | 185 | 0.036 | 3.9 |  |
| g132 | convention | baseline | 0 | true | false | false | 2 | 40839 | 411 | 0.036 | 6.5 |  |
| g134 | convention | formslot | 0 | true | false | false | 6 | 125313 | 1269 | 0.065 | 16.4 |  |
| g134 | convention | baseline | 0 | true | false | false | 5 | 88260 | 821 | 0.055 | 14.5 |  |
| g136 | decline | formslot | 0 | true | false | false | 0 | 24177 | 169 | 0.036 | 4.7 |  |
| g136 | decline | baseline | 0 | true | false | false | 5 | 97512 | 756 | 0.069 | 12.9 |  |
| g137 | decline | formslot | 0 | true | true | false | 1 | 50467 | 643 | 0.051 | 11.2 |  |
| g137 | decline | baseline | 0 | true | false | false | 5 | 89965 | 908 | 0.060 | 12.3 |  |
| g147 | decline | formslot | 0 | true | false | false | 4 | 75087 | 1188 | 0.058 | 13.6 |  |
| g147 | decline | baseline | 0 | true | false | false | 9 | 140468 | 1649 | 0.074 | 20.8 |  |
