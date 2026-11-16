"""Unit tests for the BL-0167 loop: stop rules, revert-on-regression, fail-before gates.
Run: python3 -m unittest tools/realism_loop/test_core.py -v"""
import json
import pathlib
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import core  # noqa: E402
import loop  # noqa: E402

P, F, X = {"status": "pass"}, {"status": "fail"}, {"status": "xfail"}
CFG = core.StopCfg(n_seeds=3, k_no_progress=3, max_iterations=5, require_coverage=100.0, clock_times=("0530", "1230"))


def rec(**kw):
    d = {"failing": 1, "coverage_pct": 50.0, "seeds_all_pass": 0, "times_all_pass": [], "progress": False, "error_sig": ""}
    d.update(kw)
    return d


class StopRules(unittest.TestCase):
    def test_done_needs_everything(self):
        good = rec(failing=0, coverage_pct=100.0, seeds_all_pass=3, times_all_pass=["0530", "1230"], progress=True)
        self.assertTrue(core.decide_stop([good], CFG)[0])
        for bad in (dict(failing=1), dict(coverage_pct=99.0), dict(seeds_all_pass=2), dict(times_all_pass=["1230"])):
            r = dict(good, **bad)
            self.assertFalse(core.decide_stop([r], CFG)[0], bad)

    def test_no_progress_k(self):
        h = [rec(progress=True), rec(), rec()]
        self.assertFalse(core.decide_stop(h, CFG)[0])
        h.append(rec())
        stop, why = core.decide_stop(h, CFG)
        self.assertTrue(stop)
        self.assertIn("no progress", why)

    def test_max_iterations(self):
        h = [rec(progress=True) for _ in range(5)]
        stop, why = core.decide_stop(h, CFG)
        self.assertTrue(stop)
        self.assertIn("max iterations", why)

    def test_same_error_twice_g1(self):
        h = [rec(progress=True, error_sig="llm unavailable"), rec(progress=True, error_sig="llm unavailable")]
        self.assertIn("same harness error", core.decide_stop(h, CFG)[1])

    def test_empty(self):
        self.assertFalse(core.decide_stop([], CFG)[0])


class Gates(unittest.TestCase):
    def test_regressions(self):
        before = {"a": P, "b": X, "c": F, "d": P}
        after = {"a": P, "b": X, "c": P}
        self.assertEqual(core.regressions(before, after), ["d"])  # missing counts as regression
        self.assertEqual(core.regressions(before, dict(after, d=F)), ["d"])
        self.assertEqual(core.regressions(before, dict(after, d=P)), [])
        self.assertEqual(core.regressions(before, dict(after, d=P, b=P)), ["b"])  # XPASS of a strict xfail is a regression

    def test_advisory_ignored(self):
        self.assertEqual(core.regressions({"v": P, "a": P}, {"v": F, "a": P}, ignore=["v"]), [])

    def test_fail_before_gate(self):
        self.assertTrue(core.fail_before_gate({"t": F}, {"t": P}, ["t"])[0])
        self.assertFalse(core.fail_before_gate({"t": P}, {"t": P}, ["t"])[0])  # never failed: not a proven fix
        self.assertFalse(core.fail_before_gate({"t": F}, {"t": F}, ["t"])[0])
        self.assertFalse(core.fail_before_gate({}, {"t": P}, ["t"])[0])

    def test_new_check_gate(self):
        self.assertTrue(core.new_check_gate({"g": F}, {"g": P}, "g")[0])
        self.assertTrue(core.new_check_gate({"g": F}, {"g": F}, "g")[0])   # becomes a fix target
        self.assertFalse(core.new_check_gate({"g": P}, {"g": P}, "g")[0])  # does not fail on known-bad
        self.assertFalse(core.new_check_gate({"g": F}, {"g": {"status": "error"}}, "g")[0])

    def test_pick_target_order(self):
        targets = [{"id": "job1", "kind": "fix", "checks": ["t"]}, {"id": "gw", "kind": "generate_check", "attribute": "wind"}]
        cov = {"rows": [{"id": "wind", "check": "missing"}, {"id": "other", "check": "missing"}]}
        self.assertEqual(core.pick_target({"t": F}, targets, cov)["id"], "job1")
        self.assertEqual(core.pick_target({"t": P}, targets, cov)["id"], "gw")
        self.assertEqual(core.pick_target({"t": P}, targets, {"rows": [{"id": "other", "check": "missing"}]})["id"], "gen-check:other")
        self.assertIsNone(core.pick_target({"t": P}, targets, {"rows": []}))

    def test_coverage(self):
        spec = [{"id": "a", "check": "x"}, {"id": "b", "check": "MISSING"}, {"id": "c", "check": "nope"}, {"id": "d", "check": "y"}]
        c = core.coverage(spec, {"x": P, "y": F}, {"x", "y"})
        self.assertEqual((c["attributes"], c["with_check"], c["passing"], c["failing"]), (4, 2, 1, 1))
        self.assertEqual(c["coverage_pct"], 50.0)

    def test_parse_suite(self):
        log = ("[lessville-tests] PASS: facing matches heading\n"
               "[lessville-tests] FAIL: crossing overlap (dense seeded sim) — 4029 zebra overlap\n"
               "[lessville-tests] FAIL: 1 of 370 (4 expected failures)\n")
        r = core.parse_suite(log)
        self.assertEqual(r["suite.facing_matches_heading"]["status"], "pass")
        self.assertEqual(r["suite.crossing_overlap"]["status"], "fail")
        self.assertNotIn("suite.1_of_370_4_expected_failures", r)


class Edits(unittest.TestCase):
    def test_apply_unique_and_atomic(self):
        with tempfile.TemporaryDirectory() as d:
            root = pathlib.Path(d)
            (root / "scripts").mkdir()
            (root / "scripts/a.gd").write_text("x = 1\ny = 2\n")
            txt = "FILE: scripts/a.gd\n<<<<<<< SEARCH\nx = 1\n=======\nx = 3\n>>>>>>> REPLACE\n"
            core.apply_edits(root, core.parse_edits(txt), ("scripts/",))
            self.assertEqual((root / "scripts/a.gd").read_text(), "x = 3\ny = 2\n")
            bad = txt.replace("x = 1", "zzz") + "FILE: scripts/a.gd\n<<<<<<< SEARCH\ny = 2\n=======\ny = 9\n>>>>>>> REPLACE\n"
            with self.assertRaises(ValueError):
                core.apply_edits(root, core.parse_edits(bad), ("scripts/",))
            self.assertEqual((root / "scripts/a.gd").read_text(), "x = 3\ny = 2\n")  # nothing written
            with self.assertRaises(ValueError):
                core.apply_edits(root, [{"file": "../etc/x", "new": "1"}], ("scripts/",))
            with self.assertRaises(ValueError):
                core.apply_edits(root, [{"file": "test/t.gd", "new": "1"}], ("scripts/",))


def git(repo, *a):
    return subprocess.run(["git", "-C", str(repo), *a], capture_output=True, text=True, check=True).stdout.strip()


def make_repo(d):
    repo = pathlib.Path(d) / "repo"
    repo.mkdir()
    git(repo, "init", "-q", "-b", "main")
    (repo / "scripts").mkdir()
    (repo / "scripts/walk.gd").write_text("var speed = 1\nvar safe = true\n")
    git(repo, "add", "-A")
    git(repo, "-c", "user.name=T", "-c", "user.email=t@t", "commit", "-q", "-m", "init")
    git(repo, "branch", "loop/base")
    return repo


class GitSafety(unittest.TestCase):
    def test_refuses_protected(self):
        with tempfile.TemporaryDirectory() as d:
            repo = make_repo(d)
            g = core.GitGuard(repo)
            with self.assertRaises(RuntimeError):
                g.assert_loop_branch()
            with self.assertRaises(RuntimeError):
                g.revert_to(g.head())
            git(repo, "checkout", "-q", "-b", "step3-attire")
            with self.assertRaises(RuntimeError):
                g.commit(["scripts/walk.gd"], "x", "A <a@a>")


class RevertOnRegression(unittest.TestCase):
    """Drive Loop.iteration with a fake model and fake evaluations on a temp repo."""

    def _loop(self, d, quick, full):
        repo = make_repo(d)
        cfg = {"repo": str(repo), "base_branch": "loop/base", "worktree": str(pathlib.Path(d) / "wt"),
               "runs_dir": str(pathlib.Path(d) / "runs"), "spec": str(pathlib.Path(d) / "none.yaml"),
               "author": "Maruthi Prithivirajan <maruthiprithivi@gmail.com>", "allowed_edit_prefixes": ["scripts/"],
               "endpoints": [], "limits": {}, "checks": [], "targets": [{"id": "job1", "kind": "fix", "checks": ["t"]}],
               "stop": {"clock_times": []}}
        L = loop.Loop(cfg, "test")
        L.setup_worktree()
        L.propose_fix = lambda target, results: ("FILE: scripts/walk.gd\n<<<<<<< SEARCH\nvar safe = true\n=======\nvar safe = false\n>>>>>>> REPLACE\n", {"model": "fake", "endpoint": "fake", "secs": 0, "usage": {}})
        L.eval_quick = lambda tag: {"results": quick, "info": {}}
        L.eval_full = lambda tag: {"results": full, "info": {}}
        return L

    def test_regression_is_reverted(self):
        with tempfile.TemporaryDirectory() as d:
            L = self._loop(d, quick={"t": P, "keep": P}, full={"t": P, "keep": F})
            head0 = L.git.head()
            base = {"results": {"t": F, "keep": P}, "info": {}}
            r = L.iteration(1, base)
            self.assertFalse(r["accepted"])
            self.assertIn("keep", r["rejected"])
            self.assertEqual(L.git.branch(), "loop/run-test")
            self.assertEqual(L.git.head(), head0)
            self.assertEqual((L.wt / "scripts/walk.gd").read_text(), "var speed = 1\nvar safe = true\n")
            self.assertIn("loop/scratch/test-i01", git(L.wt, "branch", "--list", "loop/scratch/*"))  # kept for audit
            self.assertEqual(git(pathlib.Path(L.cfg["repo"]), "rev-parse", "main"), git(pathlib.Path(L.cfg["repo"]), "rev-parse", "loop/base"))

    def test_fix_without_fail_before_is_rejected(self):
        with tempfile.TemporaryDirectory() as d:
            L = self._loop(d, quick={"t": P}, full={"t": P})
            r = L.iteration(1, {"results": {"t": P, "u": F}, "info": {}})
            self.assertFalse(r.get("accepted"))

    def test_good_fix_is_committed_on_run_branch(self):
        with tempfile.TemporaryDirectory() as d:
            L = self._loop(d, quick={"t": P, "keep": P}, full={"t": P, "keep": P})
            head0 = L.git.head()
            base = {"results": {"t": F, "keep": P}, "info": {}}
            r = L.iteration(1, base)
            self.assertTrue(r["accepted"], r)
            self.assertEqual(L.git.branch(), "loop/run-test")
            self.assertNotEqual(L.git.head(), head0)
            self.assertEqual(git(L.wt, "log", "-1", "--format=%an <%ae>"), "Maruthi Prithivirajan <maruthiprithivi@gmail.com>")
            self.assertEqual(base["results"]["t"], P)  # new baseline

    def test_two_strikes_parks_target(self):
        with tempfile.TemporaryDirectory() as d:
            L = self._loop(d, quick={"t": F}, full={"t": F})
            for i in (1, 2):
                L.iteration(i, {"results": {"t": F}, "info": {}})
            self.assertIn("job1", L.blocked)
            self.assertTrue(list((L.run_dir / "lessons").glob("*.md")))


class Limits(unittest.TestCase):
    def test_vision_caps(self):
        import inspect
        src = inspect.getsource(loop.vision)
        self.assertIn("min(int(vc.get(\"images_per_request\", 4)), 4)", src)
        self.assertIn("min(int(vc.get(\"max_side_px\", 1024)), 1024)", src)
        self.assertIn("min(int(vc.get(\"max_tokens\", 400)), 512)", src)

    def test_crop_resized(self):
        from PIL import Image
        import base64, io
        with tempfile.TemporaryDirectory() as d:
            p = pathlib.Path(d) / "big.png"
            Image.new("RGB", (2400, 1200)).save(p)
            b = loop._crop_b64(str(p), 1024)
            im = Image.open(io.BytesIO(base64.b64decode(b.split(",", 1)[1])))
            self.assertLessEqual(max(im.size), 1024)


FIX = pathlib.Path(__file__).resolve().parent / "fixtures"
GOOD_JSON = json.dumps({"plan": "slow walkers instead of pushing", "edits": [
    {"file": "scripts/walk.gd", "search": "var speed = 1", "replace": "var speed = 0"}]})


class ProposalParsing(unittest.TestCase):
    """ROOT-CAUSE-unparseable-patch.md (run 20261116-1409): replay the two recorded raw proposals."""

    def test_recorded_proposals_rejected_as_truncated(self):
        for name in ("run-20261116-1409-iter01.json", "run-20261116-1409-iter02.json"):
            r = json.loads((FIX / name).read_text())
            self.assertEqual(r["text"], "")                       # what was recorded: 0 bytes
            self.assertEqual(core.parse_edits(r["text"]), [])     # old parser: silently nothing
            edits, why = core.parse_proposal(r["text"], r["finish"], r["usage"])
            self.assertEqual(edits, [])
            self.assertIn("truncated", why)
            self.assertIn("16000", why)
            self.assertNotIn("parseable", why)

    def test_empty_or_unclosed_think_is_empty_not_format(self):
        self.assertIn("empty content", core.parse_proposal("<think>long plan never closed", "stop")[1])
        self.assertIn("empty content", core.parse_proposal("", "stop")[1])

    def test_valid_json_parses_and_applies(self):
        with tempfile.TemporaryDirectory() as d:
            repo = make_repo(d)
            edits, why = core.parse_proposal(GOOD_JSON, "stop")
            self.assertEqual(why, "ok (json)")
            self.assertEqual(core.check_edits(repo, edits, ("scripts/",)), "")
            core.apply_edits(repo, edits, ("scripts/",))
            self.assertEqual((repo / "scripts/walk.gd").read_text(), "var speed = 0\nvar safe = true\n")

    def test_fenced_json_and_think_prefix(self):
        edits, why = core.parse_proposal("<think>x</think>\n```json\n" + GOOD_JSON + "\n```", "stop")
        self.assertEqual(len(edits), 1, why)

    def test_loose_text_blocks(self):
        t = ("<think>plan</think>\r\n**FILE:** `scripts/walk.gd`\r\n```gdscript\r\n<<<<<<< SEARCH\r\nvar speed = 1\r\n"
             "=======\r\nvar speed = 0\r\n>>>>>>> REPLACE\r\n```\r\n")
        edits, why = core.parse_proposal(t, "stop")
        self.assertEqual(edits, [{"file": "scripts/walk.gd", "search": "var speed = 1", "replace": "var speed = 0"}], why)

    def test_malformed_rejected_with_reason(self):
        self.assertIn("invalid JSON", core.parse_proposal('{"edits": [', "stop")[1])
        self.assertIn("no non-empty 'edits'", core.parse_proposal('{"plan": "x", "edits": []}', "stop")[1])
        self.assertIn("malformed", core.parse_proposal("FILE: a\n<<<<<<< SEARCH\nx\n", "stop")[1])
        self.assertIn("prose", core.parse_proposal("I would change the speed.", "stop")[1])

    def test_check_edits_reports_non_unique(self):
        with tempfile.TemporaryDirectory() as d:
            repo = make_repo(d)
            why = core.check_edits(repo, [{"file": "scripts/walk.gd", "search": "var ", "replace": "x"}], ("scripts/",))
            self.assertIn("matched 2 times", why)

    def test_schema_limits_files(self):
        s = core.edit_schema(["scripts/a.gd"])
        self.assertEqual(s["properties"]["edits"]["items"]["properties"]["file"]["enum"], ["scripts/a.gd"])


class FixRequestShape(unittest.TestCase):
    """The fix call must not repeat the run-20261116-1409 request (thinking on, no schema), and must keep raw output."""

    def test_propose_fix_request_and_raw_saved(self):
        seen = {}

        def fake_post(url, body, timeout):
            seen.update(body)
            return {"choices": [{"message": {"content": GOOD_JSON, "reasoning": None}, "finish_reason": "stop"}],
                    "usage": {"completion_tokens": 40}}
        with tempfile.TemporaryDirectory() as d:
            repo = make_repo(d)
            cfg = {"llm": {"max_tokens": 16000, "thinking": True, "temperature": 0.2},
                   "endpoints": [{"name": "e", "role": "text", "url": "http://x", "model": "m"}],
                   "allowed_edit_prefixes": ["scripts/"], "limits": {}}
            jobs = pathlib.Path(d) / "job.yaml"
            jobs.write_text("context_files: [scripts/walk.gd]\n")
            L = loop.Loop.__new__(loop.Loop)
            L.cfg, L.wt, L.cur_dir, L.log = cfg, repo, pathlib.Path(d), lambda m: None

            class Lim:
                sems = {"e": __import__("threading").Semaphore(2)}

                def gate(self, ep, log):
                    pass
            L.limits = Lim()
            old, oldhere = loop.post_json, loop.HERE
            loop.post_json = fake_post
            try:
                loop.HERE = pathlib.Path(d)
                (pathlib.Path(d) / "jobs").mkdir()
                (pathlib.Path(d) / "jobs" / "job.yaml").write_text("context_files: [scripts/walk.gd]\n")
                text, meta = L.propose_fix({"id": "j", "job": "job.yaml"}, {"t": F})
            finally:
                loop.post_json, loop.HERE = old, oldhere
            self.assertFalse(seen["chat_template_kwargs"]["enable_thinking"])
            self.assertEqual(seen["response_format"]["type"], "json_schema")
            self.assertEqual(seen["response_format"]["json_schema"]["schema"]["properties"]["edits"]["items"]["properties"]["file"]["enum"], ["scripts/walk.gd"])
            raw = json.loads((pathlib.Path(d) / "proposal.raw.json").read_text())
            self.assertEqual(raw["finish"], "stop")
            self.assertIn("reasoning", raw)
            self.assertEqual(core.parse_proposal(text, meta["finish"], meta["usage"])[1], "ok (json)")


class TruncatedProposalInLoop(unittest.TestCase):
    def test_iteration_reports_truncation_and_logs_raw(self):
        with tempfile.TemporaryDirectory() as d:
            L = RevertOnRegression()._loop(d, quick={"t": P}, full={"t": P})
            r0 = json.loads((FIX / "run-20261116-1409-iter01.json").read_text())
            L.propose_fix = lambda target, results: (r0["text"], {"model": "m", "endpoint": "e", "secs": 1, "usage": r0["usage"], "finish": r0["finish"]})
            r = L.iteration(1, {"results": {"t": F}, "info": {}})
            self.assertIn("truncated", r["rejected"])
            self.assertIn("raw:", (L.run_dir / "loop.log").read_text())
            self.assertEqual((L.wt / "scripts/walk.gd").read_text(), "var speed = 1\nvar safe = true\n")



# ---------------------------------------------------------------- run 20261116-1525 root causes
# ROOT-CAUSE-iter1-parse-iter2-truncation.md: iter-01 patch broke the GDScript parse but G15
# reported PASS ("import rc=0 errors=1") and the suite ran to a 1500 s timeout; iter-02 rewrote
# whole functions and hit finish_reason=length at 12000 tokens against an 82 KB prompt.
REPO_ROOT = pathlib.Path(__file__).resolve().parents[2]
GODOT = "/tmp/godot-4.7.2/Godot_v4.7.2-stable_linux.x86_64"


def _fake_eval(wt, godot_out, rc=0):
    ev = loop.Evaluator.__new__(loop.Evaluator)
    ev.cfg, ev.wt, ev.limits, ev.log, ev.godot = {"limits": {}}, pathlib.Path(wt), None, lambda m: None, "godot"
    ev._godot = lambda args, **kw: (pathlib.Path(kw["log"]).write_text(godot_out) if kw.get("log") else None, (rc, godot_out))[1]
    return ev


class ImportGate(unittest.TestCase):
    def setUp(self):
        self._v = core.verify_import_md5
        core.verify_import_md5 = lambda wt: (175, [])

    def tearDown(self):
        core.verify_import_md5 = self._v

    def test_import_errors_found(self):
        bad = (FIX / "run-20261116-1525-iter01-import.log").read_text()
        errs = core.import_errors(bad)
        self.assertGreaterEqual(len(errs), 2)
        self.assertTrue(any("Parse Error" in e for e in errs), errs)
        self.assertEqual(core.import_errors((FIX / "run-20261116-1525-baseline-import-tail.log").read_text()), [])

    def test_g15_fails_on_parse_error_even_with_rc0(self):
        with tempfile.TemporaryDirectory() as d:
            ev = _fake_eval(d, (FIX / "run-20261116-1525-iter01-import.log").read_text(), rc=0)
            r = ev.g15(pathlib.Path(d))
            self.assertEqual(r["status"], core.FAIL, r)
            self.assertIn("Parse Error", r["detail"])

    def test_quick_eval_aborts_without_suite(self):
        with tempfile.TemporaryDirectory() as d:
            ev = _fake_eval(d, (FIX / "run-20261116-1525-iter01-import.log").read_text(), rc=0)
            ev.suite = lambda out: self.fail("suite must not run after an import failure")
            r = ev.evaluate(pathlib.Path(d) / "q", [], 0, quick=True)
            self.assertEqual(r["results"]["g15.import_md5"]["status"], core.FAIL)
            self.assertIn("import", r["info"].get("aborted", ""))

    def test_iteration_labels_broken_parse_and_feeds_back(self):
        with tempfile.TemporaryDirectory() as d:
            L = RevertOnRegression()._loop(d, quick={}, full={"t": P})
            L.eval_quick = lambda tag: {"results": {"g15.import_md5": {"status": "fail", "detail": "import errors: SCRIPT ERROR: Parse Error: Could not preload resource script"}},
                                        "info": {"aborted": "import failed"}}
            L.eval_full = lambda tag: self.fail("full eval must not run")
            r = L.iteration(1, {"results": {"t": F}, "info": {}})
            self.assertTrue(r["rejected"].startswith("patch broke parse:"), r["rejected"])
            self.assertIn("Parse Error", L.feedback["job1"])
            self.assertEqual((L.wt / "scripts/walk.gd").read_text(), "var speed = 1\nvar safe = true\n")

    def test_missing_target_after_is_labelled_not_run(self):
        ok, why = core.fail_before_gate({"t": F}, {}, ["t"])
        self.assertFalse(ok)
        self.assertIn("did not run", why)


class ParsePreGate(unittest.TestCase):
    def test_pregate_rejects_before_suite(self):
        with tempfile.TemporaryDirectory() as d:
            L = RevertOnRegression()._loop(d, quick={"t": P}, full={"t": P})
            L.parse_gate = lambda changed: "scripts/walk.gd: SCRIPT ERROR: Parse Error: Expected end of statement"
            L.eval_quick = lambda tag: self.fail("quick suite must not run after a parse failure")
            r = L.iteration(1, {"results": {"t": F}, "info": {}})
            self.assertTrue(r["rejected"].startswith("patch broke parse:"), r["rejected"])
            self.assertIn("Expected end of statement", L.feedback["job1"])

    @unittest.skipUnless(pathlib.Path(GODOT).exists(), "godot not installed")
    def test_check_only_on_real_godot(self):
        with tempfile.TemporaryDirectory() as d:
            root = pathlib.Path(d)
            (root / "project.godot").write_text('config_version=5\n[application]\nconfig/name="t"\n')
            (root / "scripts").mkdir()
            (root / "scripts/ok.gd").write_text("extends Node\nfunc f() -> int:\n\treturn 1\n")
            (root / "scripts/bad.gd").write_text("extends Node\nfunc f(a, a) -> int:\n\treturn 1 +\n")
            ev = loop.Evaluator.__new__(loop.Evaluator)
            ev.cfg, ev.wt, ev.godot = {}, root, GODOT
            self.assertEqual(ev.check_only(["scripts/ok.gd"]), "")
            why = ev.check_only(["scripts/ok.gd", "scripts/bad.gd"])
            self.assertIn("scripts/bad.gd", why)
            self.assertIn("Parse Error", why)


class EditSize(unittest.TestCase):
    def test_recorded_iter01_whole_function_rewrite_rejected(self):
        r = json.loads((FIX / "run-20261116-1525-iter01.json").read_text())
        edits, _ = core.parse_proposal(r["text"], r["finish"], r["usage"])
        why = core.edit_size_problem(edits, max_replace=2500, max_total=6000)
        self.assertIn("too large", why)
        self.assertEqual(core.edit_size_problem(json.loads(GOOD_JSON)["edits"], 2500, 6000), "")

    def test_iteration_rejects_oversized_before_apply(self):
        with tempfile.TemporaryDirectory() as d:
            L = RevertOnRegression()._loop(d, quick={"t": P}, full={"t": P})
            L.cfg["llm"] = {"fix_max_replace_chars": 50, "fix_max_total_replace_chars": 80}
            big = json.dumps({"plan": "p", "edits": [{"file": "scripts/walk.gd", "search": "var safe = true", "replace": "x" * 100}]})
            L.propose_fix = lambda target, results: (big, {"model": "m", "endpoint": "e", "secs": 0, "usage": {}, "finish": "stop"})
            L.eval_quick = lambda tag: self.fail("must not evaluate oversized edits")
            r = L.iteration(1, {"results": {"t": F}, "info": {}})
            self.assertIn("too large", r["rejected"])
            self.assertIn("too large", L.feedback["job1"])
            self.assertEqual((L.wt / "scripts/walk.gd").read_text(), "var speed = 1\nvar safe = true\n")


class PromptTrim(unittest.TestCase):
    def test_gd_excerpt_keeps_header_and_named_funcs(self):
        src = "extends Node\nconst A := 1\nvar b := 2\n\n\nfunc keep() -> void:\n\tpass\n\n\nfunc drop() -> void:\n\tpass\n\n\nfunc keep2() -> void:\n\tpass\n"
        ex = core.gd_excerpt(src, ["keep", "keep2"])
        self.assertIn("const A := 1", ex)
        self.assertIn("func keep() -> void:\n\tpass", ex)
        self.assertIn("func keep2()", ex)
        self.assertNotIn("func drop", ex)
        self.assertIn("omitted", ex)

    def test_job1a_prompt_is_small_and_states_size_rule(self):
        cfg = loop.load_cfg(loop.DEFAULT_CFG)
        t = next(x for x in cfg["targets"] if x["id"].startswith("job1a"))
        L = loop.Loop.__new__(loop.Loop)
        L.cfg, L.wt, L.feedback = cfg, REPO_ROOT, {t["id"]: "patch broke parse: SCRIPT ERROR: Parse Error: X"}
        prompt = L.build_fix_prompt(t, {"suite.arc_follow": F, "suite.crossing_overlap": F})
        self.assertLess(len(prompt), 45000, len(prompt))
        self.assertIn("func _update_walkers", prompt)
        self.assertNotIn("func _draw_bus(", prompt)
        self.assertIn("6000", prompt)
        self.assertIn("Parse Error: X", prompt)

    def test_prerequisite_target_waits(self):
        targets = [{"id": "a", "kind": "fix", "checks": ["ta"]}, {"id": "b", "kind": "fix", "checks": ["tb"], "after": ["a"]}]
        self.assertEqual(core.pick_target({"ta": F, "tb": F}, targets, {"rows": []})["id"], "a")
        self.assertIsNone(core.pick_target({"ta": F, "tb": F}, targets[1:], {"rows": []}, all_targets=targets))  # a parked: b waits
        self.assertEqual(core.pick_target({"ta": P, "tb": F}, targets, {"rows": []})["id"], "b")



class SearchLocate(unittest.TestCase):
    """G5 probe after 6c03bcf: 6/6 proposals parsed and fit the caps, but 0/6 applied. Searches dropped or
    mangled double quotes (&quot;, curly quotes, or none) and single-line searches matched 2-3 times."""

    def test_recorded_mangled_searches_locate_in_town_view(self):
        src = (REPO_ROOT / "scripts/lessville/town_view.gd").read_text()
        rec = json.loads((FIX / "g5-probe-1525-mangled-searches.json").read_text())
        for k, e in rec.items():
            self.assertNotEqual(src.count(e["search"]), 1, k)  # the exact search fails, as recorded
        e = rec["p1-job1a-arc-follower"]                        # every double quote dropped
        span, why = core.locate(src, e["search"])
        self.assertEqual(why, "quote-insensitive")
        self.assertEqual(src[span[0]:span[1]], '_vis[id] = {"pos": start, "path": [], "goal": String(g["node"]) + str(g["point"]),')
        e = rec["p3-job1c-zebra-reservation"]                   # curly quotes, line repeated in 2 functions
        self.assertIn("2 times", core.locate(src, e["search"])[1])
        span, why = core.locate(src, e["search"], "_update_background")
        self.assertEqual((why, src[span[0]:span[1]]), ("quote-normalised", 'var path_now: Array = v["path"]'))
        self.assertGreater(span[0], core.func_span(src, "_update_background")[0])
        e = rec["p2-job1b-follow-gap"]                          # invented trailing comment: still rejected
        self.assertIn("0 times", core.locate(src, e["search"], "_update_walkers")[1])

    def test_func_scope_makes_repeated_line_unique(self):
        src = "func a() -> void:\n\tvar budget := 1\n\n\nfunc b() -> void:\n\tvar budget := 1\n"
        self.assertIsNone(core.locate(src, "var budget := 1", "")[0])
        span, why = core.locate(src, "var budget := 1", "b")
        self.assertEqual(span[0], src.rindex("var budget"), why)
        with tempfile.TemporaryDirectory() as d:
            root = pathlib.Path(d)
            (root / "scripts").mkdir()
            (root / "scripts/x.gd").write_text(src)
            e = [{"file": "scripts/x.gd", "func": "b", "search": "var budget := 1", "replace": "var budget := 2"}]
            self.assertEqual(core.check_edits(root, e, ("scripts/",)), "")
            core.apply_edits(root, e, ("scripts/",))
            self.assertEqual((root / "scripts/x.gd").read_text(), src.replace("\tvar budget := 1\n", "\tvar budget := 2\n").replace("func a() -> void:\n\tvar budget := 2", "func a() -> void:\n\tvar budget := 1"))

    def test_replace_entities_normalised(self):
        self.assertEqual(core.clean_code('v[&quot;path&quot;] + “x”'), 'v["path"] + "x"')

    def test_schema_has_func_field(self):
        s = core.edit_schema(["scripts/a.gd"])
        self.assertIn("func", s["properties"]["edits"]["items"]["required"])


# ---------------------------------------------------------------- run 20261116-1653 root cause
# ROOT-CAUSE-job1a-scope.md: job1a search/replace splices landed in the wrong scope (iter-01: the same insert applied
# 3 times -> duplicate var start_poly; iter-02: 1-tab continuation lines inside a 2-tab loop -> continue outside loop).
# Fix: whole-function replacement only for job1a, Godot parse pre-check inside the iteration, one repair pass.
WF_SRC = ("extends Node\nconst A := 1\n\n\nfunc walk(n: int) -> int:\n\tvar t := 0\n\tfor i in n:\n\t\tif i == 2:\n\t\t\tcontinue\n\t\tt += i\n"
          "\treturn t\n\n## doc comment of the next declaration stays\nvar after := 2\n\n\nstatic func other() -> int:\n\treturn 1\n")
WF_NEW = "func walk(n: int) -> int:\n\tvar t := 0\n\tfor i in n:\n\t\tt += i * 2\n\treturn t\n"


def _wf_root(d, src=WF_SRC):
    root = pathlib.Path(d)
    (root / "scripts").mkdir(exist_ok=True)
    (root / "scripts/w.gd").write_text(src)
    return root


class WholeFunction(unittest.TestCase):
    def test_recorded_1653_splices_reproduce_the_scope_errors(self):
        """The recorded proposals, applied with the old search/replace path, give exactly the logged breakage."""
        src = (REPO_ROOT / "scripts/lessville/town_view.gd").read_text()
        out = {}
        for it in ("iter01", "iter02"):
            cur = src
            for e in json.loads((FIX / f"run-20261116-1653-{it}-proposal.json").read_text())["edits"]:
                span, _ = core.locate(cur, e["search"], e.get("func", ""))
                cur = cur[:span[0]] + core.clean_code(e["replace"]) + cur[span[1]:]
            out[it] = cur
        f = core.func_span(out["iter01"], "_update_walkers")
        self.assertEqual(out["iter01"][f[0]:f[1]].count("var start_poly"), 4)
        self.assertIn("\n\tif bool(g[hidden]) and float(v[alpha]) <= 0.01:\n", out["iter02"])  # 1 tab: closes the for loop

    def test_span_ends_at_next_top_level_decl_and_keeps_comments(self):
        a, b = core.func_decl_span(WF_SRC, "walk")
        self.assertTrue(WF_SRC[a:b].startswith("func walk("))
        self.assertTrue(WF_SRC[a:b].rstrip().endswith("return t"))
        self.assertNotIn("after", WF_SRC[a:b])
        self.assertNotIn("## doc", WF_SRC[a:b])
        a, b = core.func_decl_span(WF_SRC, "other")
        self.assertEqual(WF_SRC[a:b], "static func other() -> int:\n\treturn 1\n")
        self.assertIsNone(core.func_decl_span(WF_SRC, "nope"))

    def test_whole_span_replaced_rest_untouched(self):
        with tempfile.TemporaryDirectory() as d:
            root = _wf_root(d)
            fs = [{"file": "scripts/w.gd", "target": "walk", "code": WF_NEW}]
            self.assertEqual(core.check_functions(root, fs, ("scripts/",)), "")
            self.assertEqual(core.apply_functions(root, fs, ("scripts/",)), ["scripts/w.gd"])
            out = (root / "scripts/w.gd").read_text()
            self.assertEqual(out.count("func walk("), 1)
            self.assertNotIn("continue", out)
            self.assertIn("t += i * 2", out)
            self.assertTrue(out.startswith("extends Node\nconst A := 1\n\n\nfunc walk("))
            self.assertTrue(out.endswith("\treturn t\n\n## doc comment of the next declaration stays\nvar after := 2\n\n\nstatic func other() -> int:\n\treturn 1\n"), out)

    def test_whole_function_on_real_town_view_replaces_only_that_function(self):
        src = (REPO_ROOT / "scripts/lessville/town_view.gd").read_text()
        a, b = core.func_decl_span(src, "_update_walkers")
        body = src[a:b]
        with tempfile.TemporaryDirectory() as d:
            root = _wf_root(d, src)
            new = body.replace("var budget := speed * delta", "var budget := speed * delta * 1.0", 1)
            core.apply_functions(root, [{"file": "scripts/w.gd", "target": "_update_walkers", "code": new}], ("scripts/",))
            out = (root / "scripts/w.gd").read_text()
            self.assertEqual(out, src[:a] + new.rstrip("\n") + "\n" + src[a + len(body.rstrip("\n")) + 1:])

    def test_name_mismatch_rejected(self):
        with tempfile.TemporaryDirectory() as d:
            root = _wf_root(d)
            why = core.check_functions(root, [{"file": "scripts/w.gd", "target": "walk", "code": WF_NEW.replace("func walk(", "func walk2(")}], ("scripts/",))
            self.assertIn("does not match target", why)
            with self.assertRaises(ValueError):
                core.apply_functions(root, [{"file": "scripts/w.gd", "target": "walk", "code": WF_NEW.replace("func walk(", "func walk2(")}], ("scripts/",))
            self.assertEqual((root / "scripts/w.gd").read_text(), WF_SRC)

    def test_partial_splices_rejected(self):
        with tempfile.TemporaryDirectory() as d:
            root = _wf_root(d)
            bad = {
                "fragment": "\tvar start_poly: Array = []\n",                         # not a function at all
                "indented signature": "\tfunc walk(n: int) -> int:\n\t\treturn 0\n",
                "second decl": WF_NEW + "\nvar leak := 1\n",                         # spills a top-level var
                "second func": WF_NEW + "\nfunc extra() -> void:\n\tpass\n",
                "col0 statement": "func walk(n: int) -> int:\n\tvar t := 0\nfor i in n:\n\tcontinue\n\treturn t\n",
                "empty body": "func walk(n: int) -> int:\n",
            }
            for k, code in bad.items():
                why = core.check_functions(root, [{"file": "scripts/w.gd", "target": "walk", "code": code}], ("scripts/",))
                self.assertTrue(why, k)
            self.assertEqual((root / "scripts/w.gd").read_text(), WF_SRC)

    def test_duplicate_target_and_new_function_rules(self):
        with tempfile.TemporaryDirectory() as d:
            root = _wf_root(d)
            fs = [{"file": "scripts/w.gd", "target": "walk", "code": WF_NEW}] * 2
            self.assertIn("more than once", core.check_functions(root, fs, ("scripts/",)))
            helper = {"file": "scripts/w.gd", "target": "_poly_point", "code": "static func _poly_point(poly: Array, s: float) -> Vector2:\n\treturn Vector2.ZERO\n"}
            self.assertIn("not found", core.check_functions(root, [helper], ("scripts/",)))
            self.assertEqual(core.check_functions(root, [helper], ("scripts/",), allow_new=True), "")
            core.apply_functions(root, [helper], ("scripts/",), allow_new=True)
            out = (root / "scripts/w.gd").read_text()
            self.assertTrue(out.startswith(WF_SRC))
            self.assertTrue(out.endswith("\n\n\nstatic func _poly_point(poly: Array, s: float) -> Vector2:\n\treturn Vector2.ZERO\n"))
            self.assertIn("outside allowed", core.check_functions(root, [dict(helper, file="test/t.gd")], ("scripts/",), allow_new=True))

    def test_func_proposal_parse_and_schema(self):
        s = core.func_schema(["scripts/w.gd"])
        it = s["properties"]["functions"]["items"]
        self.assertEqual(sorted(it["required"]), ["code", "file", "target"])
        self.assertEqual(it["properties"]["file"]["enum"], ["scripts/w.gd"])
        txt = json.dumps({"plan": "p", "functions": [{"file": "scripts/w.gd", "target": "walk", "code": WF_NEW}]})
        fs, why = core.parse_func_proposal(txt, "stop", {})
        self.assertEqual((len(fs), why), (1, "ok (json functions)"))
        self.assertIn("truncated", core.parse_func_proposal(txt[:40], "length", {})[1])
        self.assertIn("functions", core.parse_func_proposal(json.dumps({"plan": "p", "edits": []}), "stop", {})[1])

    def test_parse_error_locations_and_func_at_line(self):
        err = 'scripts/lessville/town_view.gd: SCRIPT ERROR: Parse Error: Cannot use "continue" outside of a loop. at: GDScript::reload (res://scripts/lessville/town_view.gd:414)'
        self.assertEqual(core.parse_error_locs(err), [("scripts/lessville/town_view.gd", 414)])
        self.assertEqual(core.func_at_line(WF_SRC, 9), "walk")
        self.assertEqual(core.func_at_line(WF_SRC, 2), None)
        self.assertEqual(core.func_at_line(WF_SRC, 18), "other")

    def test_job1a_is_whole_function_mode(self):
        job = loop.yaml.safe_load((pathlib.Path(__file__).resolve().parent / "jobs" / "job1a-arc-follower.yaml").read_text())
        self.assertEqual(job.get("edit_mode"), "whole_function")


class RepairPass(unittest.TestCase):
    """Parse pre-check inside the iteration; on failure ONE repair call with the exact error and the offending function."""

    def _loop(self, d, proposals, gates, quick=None, full=None):
        repo = pathlib.Path(d) / "repo"
        repo.mkdir()
        git(repo, "init", "-q", "-b", "main")
        _wf_root(repo)
        git(repo, "add", "-A")
        git(repo, "-c", "user.name=T", "-c", "user.email=t@t", "commit", "-q", "-m", "init")
        git(repo, "branch", "loop/base")
        jobs = pathlib.Path(d) / "jobs"
        jobs.mkdir()
        (jobs / "j.yaml").write_text("edit_mode: whole_function\nallow_new_functions: true\ncontext_files: [scripts/w.gd]\n")
        cfg = {"repo": str(repo), "base_branch": "loop/base", "worktree": str(pathlib.Path(d) / "wt"),
               "runs_dir": str(pathlib.Path(d) / "runs"), "spec": str(pathlib.Path(d) / "none.yaml"),
               "author": "Maruthi Prithivirajan <maruthiprithivi@gmail.com>", "allowed_edit_prefixes": ["scripts/"],
               "endpoints": [], "limits": {}, "checks": [], "llm": {},
               "targets": [{"id": "job1", "kind": "fix", "job": "j.yaml", "checks": ["t"]}], "stop": {"clock_times": []}}
        L = loop.Loop(cfg, "test")
        L.setup_worktree()
        L._jobs_dir = jobs
        self.calls = []
        meta = {"model": "fake", "endpoint": "fake", "secs": 0, "usage": {}, "finish": "stop"}

        def fake_propose(target, results, out_dir=None):
            self.calls.append(("propose", None))
            return proposals[0], meta

        def fake_repair(target, prev_text, perr, offending, out_dir=None):
            self.calls.append(("repair", (perr, offending)))
            return proposals[1], meta
        L.propose_fix, L.repair_fix = fake_propose, fake_repair
        gate_iter = iter(gates)
        L.parse_gate = lambda changed: next(gate_iter)
        L.eval_quick = lambda tag: {"results": quick or {"t": P}, "info": {}}
        L.eval_full = lambda tag: {"results": full or {"t": P}, "info": {}}
        return L

    def _prop(self, code):
        return json.dumps({"plan": "p", "functions": [{"file": "scripts/w.gd", "target": "walk", "code": code}]})

    def test_repair_pass_fixes_parse_and_is_accepted(self):
        broken = "func walk(n: int) -> int:\n\tvar t := 0\n\tvar t := 1\n\treturn t\n"
        err = "scripts/w.gd: SCRIPT ERROR: Parse Error: There is already a variable named \"t\" declared in this scope. at: GDScript::reload (res://scripts/w.gd:7)"
        with tempfile.TemporaryDirectory() as d:
            old = loop.HERE
            try:
                L = self._loop(d, [self._prop(broken), self._prop(WF_NEW)], [err, ""])
                loop.HERE = L._jobs_dir.parent
                r = L.iteration(1, {"results": {"t": F}, "info": {}})
            finally:
                loop.HERE = old
            self.assertTrue(r["accepted"], r)
            self.assertEqual([c[0] for c in self.calls], ["propose", "repair"])
            perr, offending = self.calls[1][1]
            self.assertIn("already a variable named", perr)        # exact parse error fed back
            self.assertIn("var t := 1", offending)                  # the offending function as applied
            self.assertIn("func walk(", offending)
            self.assertEqual(r["repair"]["parse_gate"], "ok")
            self.assertIn("t += i * 2", (L.wt / "scripts/w.gd").read_text())

    def test_still_broken_after_one_repair_is_rejected(self):
        broken = "func walk(n: int) -> int:\n\tvar t := 0\n\tvar t := 1\n\treturn t\n"
        err = "scripts/w.gd: SCRIPT ERROR: Parse Error: dup at: GDScript::reload (res://scripts/w.gd:7)"
        with tempfile.TemporaryDirectory() as d:
            old = loop.HERE
            try:
                L = self._loop(d, [self._prop(broken), self._prop(broken)], [err, err, err])
                L.eval_quick = lambda tag: self.fail("no suite after a failed repair")
                loop.HERE = L._jobs_dir.parent
                r = L.iteration(1, {"results": {"t": F}, "info": {}})
            finally:
                loop.HERE = old
            self.assertFalse(r["accepted"])
            self.assertEqual([c[0] for c in self.calls], ["propose", "repair"])  # exactly one repair
            self.assertIn("after 1 repair pass", r["rejected"])
            self.assertEqual((L.wt / "scripts/w.gd").read_text(), WF_SRC)

    def test_splice_in_whole_function_mode_rejected_without_apply(self):
        with tempfile.TemporaryDirectory() as d:
            old = loop.HERE
            try:
                L = self._loop(d, [self._prop("\tvar x := 1\n"), self._prop(WF_NEW)], [""])
                L.eval_quick = lambda tag: self.fail("must not evaluate a splice")
                loop.HERE = L._jobs_dir.parent
                r = L.iteration(1, {"results": {"t": F}, "info": {}})
            finally:
                loop.HERE = old
            self.assertFalse(r["accepted"])
            self.assertIn("whole function", r["rejected"])
            self.assertEqual((L.wt / "scripts/w.gd").read_text(), WF_SRC)

    def test_check_only_fails_on_error_line_even_with_rc0(self):
        ev = loop.Evaluator.__new__(loop.Evaluator)
        with tempfile.TemporaryDirectory() as d:
            _wf_root(d)
            ev.cfg, ev.wt, ev.godot = {}, pathlib.Path(d), "godot"
            old = loop.sh
            loop.sh = lambda *a, **k: (0, "SCRIPT ERROR: Parse Error: Cannot use \"continue\" outside of a loop.\n   at: GDScript::reload (res://scripts/w.gd:9)\n")
            try:
                why = ev.check_only(["scripts/w.gd"])
            finally:
                loop.sh = old
            self.assertIn("outside of a loop", why)

    @unittest.skipUnless(pathlib.Path(GODOT).exists(), "godot not installed")
    def test_real_godot_flags_recorded_1653_iter02_and_passes_whole_function(self):
        src = (REPO_ROOT / "scripts/lessville/town_view.gd").read_text()
        with tempfile.TemporaryDirectory() as d:
            root = pathlib.Path(d)
            (root / "project.godot").write_text('config_version=5\n[application]\nconfig/name="t"\n')
            (root / "scripts").mkdir()
            body = "extends Node\n\n\nfunc walk(n: int) -> int:\n\tvar t := 0\n\tfor i in n:\n\t\tif i == 2:\n\t\t\tcontinue\n\t\tt += i\n\treturn t\n"
            (root / "scripts/w.gd").write_text(body)
            ev = loop.Evaluator.__new__(loop.Evaluator)
            ev.cfg, ev.wt, ev.godot = {}, root, GODOT
            self.assertEqual(ev.check_only(["scripts/w.gd"]), "")
            # the iter-02 shape: a continuation line one indent shallower than the loop
            (root / "scripts/w.gd").write_text(body.replace("\t\tif i == 2:\n\t\t\tcontinue\n", "\t\tif i == 2:\n\t\t\tpass\n\tif n == 2:\n\t\tcontinue\n"))
            self.assertIn("outside of a loop", ev.check_only(["scripts/w.gd"]))
            core.apply_functions(root, [{"file": "scripts/w.gd", "target": "walk", "code": WF_NEW}], ("scripts/",))
            self.assertEqual(ev.check_only(["scripts/w.gd"]), "")


# ---------------------------------------------------------------- run 20261116-2022 root cause
# ROOT-CAUSE-job1a-arc-follower.md: whole-function code inside a strict-JSON string looped ' -> void:' 2640x with
# no \n escape until max_tokens.  Fix: raw fenced code blocks per function + streamed early abort on repetition.
BLOCKS_OK = ("PLAN: follow the polyline by arc length.\n\n### TARGET: walk\n```gdscript\n" + WF_NEW + "```\n\n"
             "### TARGET: _poly_point\n```gdscript\nstatic func _poly_point(poly: Array, s: float) -> Vector2:\n\treturn Vector2.ZERO\n```\n")


class CodeBlocks(unittest.TestCase):
    def test_blocks_parse_raw_code_unescaped(self):
        fs, why = core.parse_func_blocks(BLOCKS_OK, "stop", {}, default_file="scripts/w.gd")
        self.assertEqual(why, "ok (code blocks)")
        self.assertEqual([f["target"] for f in fs], ["walk", "_poly_point"])
        self.assertEqual(fs[0]["file"], "scripts/w.gd")
        self.assertEqual(fs[0]["code"], WF_NEW)
        self.assertIn("\t", fs[0]["code"])            # real tabs, nothing JSON-escaped
        self.assertNotIn("\\n", fs[0]["code"])

    def test_blocks_with_file_header_and_quotes(self):
        txt = '### TARGET: walk FILE: scripts/w.gd\n```\nfunc walk(n: int) -> int:\n\tvar d := {"a": 1}\n\treturn d["a"]\n```\n'
        fs, why = core.parse_func_blocks(txt, "stop", {}, default_file="scripts/other.gd")
        self.assertEqual((fs[0]["file"], fs[0]["target"]), ("scripts/w.gd", "walk"))
        self.assertIn('d["a"]', fs[0]["code"])

    def test_blocks_rejections_are_labelled(self):
        self.assertIn("truncated", core.parse_func_blocks(BLOCKS_OK[:60], "length", {}, "scripts/w.gd")[1])
        self.assertIn("degenerate repetition", core.parse_func_blocks("x -> void:" * 30, "degenerate_repetition", {"abort": "line repeated 21x"}, "scripts/w.gd")[1])
        self.assertIn("no '### TARGET:", core.parse_func_blocks("just prose", "stop", {}, "scripts/w.gd")[1])
        self.assertIn("unclosed", core.parse_func_blocks("### TARGET: walk\n```gdscript\nfunc walk() -> void:\n\tpass\n", "stop", {}, "scripts/w.gd")[1])

    def test_blocks_go_through_whole_function_rules(self):
        with tempfile.TemporaryDirectory() as d:
            root = _wf_root(d)
            fs, _ = core.parse_func_blocks(BLOCKS_OK, "stop", {}, "scripts/w.gd")
            self.assertEqual(core.check_functions(root, fs, ("scripts/",), allow_new=True), "")
            bad, _ = core.parse_func_blocks("### TARGET: walk\n```gdscript\n\tvar x := 1\n```\n", "stop", {}, "scripts/w.gd")
            self.assertIn("not a whole function", core.check_functions(root, bad, ("scripts/",)))

    def test_job1a_uses_code_blocks(self):
        job = loop.yaml.safe_load((pathlib.Path(__file__).resolve().parent / "jobs" / "job1a-arc-follower.yaml").read_text())
        self.assertEqual((job.get("edit_mode"), job.get("answer_format")), ("whole_function", "code_blocks"))

    def test_blocks_prompt_has_no_json_code(self):
        cfg = loop.load_cfg(loop.DEFAULT_CFG)
        t = next(x for x in cfg["targets"] if x["id"].startswith("job1a"))
        L = loop.Loop.__new__(loop.Loop)
        L.cfg, L.wt, L.feedback = cfg, REPO_ROOT, {}
        prompt = L.build_fix_prompt(t, {"suite.arc_follow": F})
        self.assertIn("### TARGET:", prompt)
        self.assertNotIn('"code":', prompt)


class RepetitionAbort(unittest.TestCase):
    def test_recorded_2022_output_aborts_early(self):
        txt = (FIX / "run-20261116-2022-iter01-proposal-degenerate.txt").read_text()
        det = core.RepeatDetector()
        hit_at, pos = None, 0
        for i in range(0, len(txt), 7):                    # feed like a stream
            r = det.feed(txt[i:i + 7])
            if r:
                hit_at = i + 7
                break
        self.assertIsNotNone(hit_at)
        self.assertLess(hit_at, 1600)                      # the loop starts at char ~1036; 24796 were generated
        self.assertIn("repeat", r)

    def test_repeated_line_aborts_after_20(self):
        det = core.RepeatDetector()
        out = [det.feed("\t\tv[\"s\"] = 0.0\n") for _ in range(21)]
        self.assertFalse(any(out[:20]))
        self.assertTrue(out[20])

    def test_normal_code_does_not_abort(self):
        det = core.RepeatDetector()
        src = (REPO_ROOT / "scripts/lessville/town_view.gd").read_text()
        self.assertFalse(any(det.feed(src[i:i + 5]) for i in range(0, len(src), 5)))
        det = core.RepeatDetector()
        self.assertFalse(det.feed("# " + "-" * 120 + "\n" + "\tpass\n" * 3 + "}" * 40))   # 1-char runs are not 4+ char units

    def test_streamed_llm_aborts_and_labels(self):
        chunks = [{"choices": [{"delta": {"content": "### TARGET: walk\n```gdscript\nfunc walk() -> void:"}, "finish_reason": None}]}]
        chunks += [{"choices": [{"delta": {"content": " -> void:"}, "finish_reason": None}]}] * 500
        sent = {"n": 0}

        def fake_stream(url, body, timeout):
            assert body["stream"] is True and body["max_tokens"] <= 8192
            for c in chunks:
                sent["n"] += 1
                yield c
        cfg = {"llm": {"fix_stream": True}, "endpoints": [{"name": "e", "role": "text", "url": "http://x", "model": "m"}]}

        class Lim:
            sems = {"e": __import__("threading").Semaphore(2)}

            def gate(self, ep, log):
                pass
        old = loop.post_stream
        loop.post_stream = fake_stream
        try:
            r = loop.llm(cfg, Lim(), [{"role": "user", "content": "x"}], lambda m: None, max_tokens=8192, thinking=False, stream=True)
        finally:
            loop.post_stream = old
        self.assertEqual(r["finish"], "degenerate_repetition")
        self.assertLess(sent["n"], 60)
        self.assertIn("repeat", r["usage"]["abort"])
        self.assertTrue(r["text"].startswith("### TARGET: walk"))
        self.assertIn("degenerate repetition", core.parse_func_blocks(r["text"], r["finish"], r["usage"], "scripts/w.gd")[1])


if __name__ == "__main__":
    unittest.main()
