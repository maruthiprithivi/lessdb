# BL-0167 Lessville realism loop

One command (from the `loop/base` worktree `/workspace/lessville-godot-src/work-loop`):

    python3 tools/realism_loop/loop.py start     # detached (new session), survives the caller
    python3 tools/realism_loop/loop.py status    # state, last iterations, log tail
    python3 tools/realism_loop/loop.py stop      # STOP file: exits after the current step
    python3 tools/realism_loop/loop.py summarize RUNS/<run_id>   # re-derive SUMMARY.md from iterations.jsonl
    python3 -m unittest tools/realism_loop/test_core.py          # stop rules, revert-on-regression, fail-before gates

`start` freezes a copy of this directory into `RUNS/<run_id>/harness/` and runs from it,
so code the loop edits can never change the code judging it.

Per run (`runs_dir` in config.yaml): `iterations.jsonl` (raw, one line per iteration),
`SUMMARY.md` (read this instead of logs), `coverage.json`, `state.json`, `loop.log`,
`baseline/` and `iter-NN/{quick,after}/` with logs, `results.json` and `gallery/` (4x crops,
motion strips/GIFs for CoS G4), `lessons/` (Lessons Keeper format; G1 on the harness itself).

Per iteration: G15 (clear .godot/imported and .ctex, re-import, verify every texture's
source_md5) -> renders (xvfb + GL, clock sweep 05:30 08:00 12:30 18:30 22:00 02:30) ->
full suite + render checks + seeded multi-hour crossing sims -> vision review of zoomed crops
(qwen3-vl-8b, <=4 images/request, <=1024 px, max_tokens <=512; advisory) -> coverage vs
realism-spec.yaml -> pick target (config order, then MISSING checks) -> local Qwen3.8
proposes SEARCH/REPLACE edits on `loop/scratch/<run>-iNN` -> quick gate (suite) -> full
re-check -> accept only if a target check went fail->pass and nothing that passed regressed;
otherwise the run branch is reset and the scratch branch kept for audit.  Missing-check
targets: Qwen writes `tools/realism_checks/<slug>.py` plus a MUTATION; accepted only if the
check fails on the mutant (fail-before) and runs on current code.

Stop: done when failing == 0, coverage 100%, N seeds and every sweep clock time pass;
or K iterations without progress; or the max-iteration cap; or the same harness error twice.
A target rejected twice is parked (G1) with a lesson file.

Git: writes only on `loop/*` branches (GitGuard refuses main/master/step3-attire/
step4-office-bed/step2b-pathfollow/client-v1). Nothing is pushed. client-v1 stays HELD.

Limits (Machine Steward, INVENTORY.md 16 Nov 13:55): 2 concurrent calls per endpoint;
back off while vllm:num_requests_waiting > 0 or megatron memory > 85%, stop at 95%;
bumble 18001 text fallback never 01:45-03:00 SGT. Renders and sims currently run on the box
(nice 10). The megatron render executor (bench.slice, <=6 CPU, <=12 GiB, CPU only) is
configured but disabled: megatron is aarch64 and has no Godot arm64 build or Xvfb yet.

## Run 20261116-1525 fixes (ROOT-CAUSE-iter1-parse-iter2-truncation.md)
- G15 import check FAILS on any `SCRIPT ERROR` / `Parse Error` / `ERROR` line (it used to pass with
  "rc=0 errors=1"); an evaluation stops right after a failed import (no 10-25 min suite hang).
- Pre-gate: after edits are applied, `godot --check-only` parses every edited `.gd` plus `parse_also`
  (about 1 s each) before any suite run. Failures are labelled `patch broke parse: <first error lines>`
  and every rejection reason is fed into the next prompt for that target.
- Size control: `fix_max_replace_chars` (2500 per edit) and `fix_max_total_replace_chars` (6000) are
  stated in the prompt and enforced before anything is applied. Prompts carry only the header and the
  named functions (`context_funcs`, `re:` patterns allowed) of big files (job 1a prompt ~19 KB, was 82 KB).
- Job 1 is split into 1a arc-length follower -> 1b follow gap -> 1c zebra reservation + deadlock
  breaker (`after:` makes each wait for the previous one), each with its own fail-before suite case.

## Run 20261116-1653 fixes (ROOT-CAUSE-job1a-scope.md)
- Whole-function edits: a job with `edit_mode: whole_function` (job 1a) answers `{plan, functions:[{file, target, code}]}`.
  The harness finds `func target(` and replaces the span up to the next top-level func/var/const/class/... (or EOF,
  trailing blank/comment lines kept), atomically. Rejected before apply: signature name != target, code not starting at
  column 0 with the signature, a second top-level declaration or any column-0 statement in the body (a splice), duplicate
  targets, replacing a function that was not shown, new functions unless `allow_new_functions` (appended whole at EOF),
  and code over `fix_max_func_chars` / `fix_max_total_func_chars`.
- Repair pass (G20): the Godot `--check-only` pre-check runs on every patched file inside the iteration (fails on any
  SCRIPT ERROR / Parse Error / ERROR line even at rc=0). On failure the exact error lines and the offending function as
  applied (numbered) go back to the model in the same conversation for exactly ONE repair pass (`iter-NN/repair.*`);
  it is re-checked and rejected only if it still fails.
- `max_attempts_per_target` (default 2) sets the G1 park threshold; `only_targets` restricts a run to named targets.
  Park lessons now say a ROOT-CAUSE file must be written, and list every rejection reason.

## Run 20261116-2022 fixes (ROOT-CAUSE-job1a-arc-follower.md)
- `answer_format: code_blocks` (job 1a): the model answers `PLAN: ...` then `### TARGET: <func> [FILE: <path>]` + one fenced
  raw GDScript block per whole function. No JSON, no escaping. Same whole-function rules, parse pre-check and single repair pass.
- `fix_stream: true`: fix and repair calls are streamed through `core.RepeatDetector`. The call is aborted as soon as the same non-blank
  line repeats more than 20 times in a row, or the tail is more than 20 back-to-back copies of one unit of 4+ chars. It is labelled
  `degenerate repetition` (finish `degenerate_repetition`), never a format error. Raw output is saved. `fix_max_tokens` is 8192.
