"""Tests for runner.py. No network, gh, git, docker or claude. YtTest uses a fake docker and git.

Run: python3 -m unittest discover -s yantra -p 'test_*.py'
"""
import contextlib
import io
import os
import subprocess
import sys
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import runner  # noqa: E402

GOOD_BODY = """The engine has no limits.

Acceptance criteria:
- The engine gives an error over the limit.
- [ ] The score does not go down.

Scope: `src/`, `README.md`.
"""


def issue(body=GOOD_BODY, state="OPEN", labels=("ticket",)):
    return {"number": 13, "title": "t", "url": "u", "body": body, "state": state, "labels": [{"name": n} for n in labels]}


def fixture(name):
    with open(os.path.join(HERE, "testdata", name), encoding="utf-8") as f:
        return f.read()


class TicketTest(unittest.TestCase):
    def errors(self, **change):
        return runner.validate_ticket(issue(**change))[0]

    def test_good_body(self):
        errors, criteria, scope = runner.validate_ticket(issue())
        self.assertEqual(errors, [])
        self.assertEqual(criteria, ["The engine gives an error over the limit.", "The score does not go down."])
        self.assertEqual(scope, ["src/", "README.md"])

    def test_no_criteria_heading(self):
        self.assertEqual(len(self.errors(body="- an item\n\nScope: `src/`")), 1)

    def test_heading_with_no_items(self):
        self.assertEqual(len(self.errors(body="Acceptance criteria:\n\nText, not a list.\n- late item\n\nScope: `src/`")), 1)

    def test_no_scope_line(self):
        self.assertEqual(len(self.errors(body="Acceptance criteria:\n- a")), 1)

    def test_scope_with_no_backticked_path(self):
        self.assertEqual(len(self.errors(body="Acceptance criteria:\n- a\n\nScope: src/")), 1)

    def test_two_scope_lines(self):
        self.assertEqual(len(self.errors(body="Acceptance criteria:\n- a\n\nScope: `src/`\nScope: `ci/`")), 1)

    def test_criteria_and_scope_only_in_a_code_block(self):
        body = "Example:\n\n```\nAcceptance criteria:\n- a\n\nScope: `src/`\n```\n"
        self.assertEqual(len(self.errors(body=body)), 2)

    def test_closed_issue(self):
        self.assertEqual(self.errors(state="CLOSED"), ["the issue is not open"])

    def test_missing_ticket_label(self):
        self.assertEqual(len(self.errors(labels=("bug",))), 1)

    def test_empty_body(self):
        self.assertEqual(len(self.errors(body=None)), 2)


class ScopeTest(unittest.TestCase):
    def test_scope(self):
        scope = ["src/", "README.md"]
        self.assertTrue(runner.in_scope("src/exec/mod.rs", scope))
        self.assertTrue(runner.in_scope("README.md", scope))
        self.assertFalse(runner.in_scope("src-extra/x", scope))
        self.assertFalse(runner.in_scope("README.md.bak", scope))
        self.assertFalse(runner.in_scope("src", scope))
        self.assertFalse(runner.in_scope("src/x", []))
        self.assertFalse(runner.in_scope("README.md", []))


class ScoreTest(unittest.TestCase):
    def test_one_line(self):
        text = "running 1 test\nslt: random/expr/slt_good_0.test 7981/10012\ntest slt ... ok\n"
        self.assertEqual(runner.parse_score(text), (7981, 10012))

    def test_two_lines(self):
        self.assertEqual(runner.parse_score("slt: a.test 5/10\nslt: b.test 7/7\n"), (12, 17))

    def test_no_line(self):
        with self.assertRaises(ValueError):
            runner.parse_score("test slt ... ok\n")

    def test_malformed_lines(self):
        for text in ("slt: a.test 5 of 10\n", "slt: a.test 11/10\n", "slt: a.test 1/2\nslt: a.test 1/2\n", "slt:\n"):
            with self.assertRaises(ValueError):
                runner.parse_score(text)

    def test_baseline(self):
        self.assertEqual(runner.parse_baseline("random/expr/slt_good_0.test 7981\n"), 7981)
        self.assertEqual(runner.parse_baseline("a.test 5\n\nb.test 7\n"), 12)
        for text in ("", "a.test\n", "a.test x\n", "a.test -1\n"):
            with self.assertRaises(ValueError):
                runner.parse_baseline(text)


class ReviewTest(unittest.TestCase):
    def test_fixtures(self):
        self.assertEqual(runner.classify_review(fixture("review-with-findings.md")), ("findings", 3))
        self.assertEqual(runner.classify_review(fixture("review-rule-violations.md")), ("findings", 3))
        self.assertEqual(runner.classify_review(fixture("review-clean.md")), ("clean", 0))
        self.assertEqual(runner.classify_review(fixture("review-trivial-not-reviewed.md")), ("not_reviewed", 0))
        self.assertEqual(runner.classify_review(fixture("review-went-wrong.md")), ("went_wrong", 0))

    def test_unknown_shapes_are_errors(self):
        clean = fixture("review-clean.md")
        bodies = [
            "",
            None,
            "Great, no issues found!",
            "<h3>Code Review by Qodo</h3>\n\nA new shape.",
            clean.replace("Bugs (0)", "Bugs (1)"),
            clean.replace("Great, no issues found!", "All good"),
        ]
        for body in bodies:
            self.assertEqual(runner.classify_review(body), ("error", 0))

    def test_find_review(self):
        sha = "a1a1e7caa2f5080e4211538b953a375bae1ce9c7"
        qodo = {"login": runner.REVIEW_AUTHOR}
        clean = {"id": 1, "updated_at": "t1", "user": qodo, "body": fixture("review-clean.md")}
        wrong = {"id": 2, "updated_at": "t2", "user": qodo, "body": fixture("review-went-wrong.md")}
        self.assertEqual(runner.find_review([clean], sha, set())[:2], ("clean", 0))
        self.assertIsNone(runner.find_review([clean], "0" * 40, set()))
        self.assertIsNone(runner.find_review([dict(clean, user={"login": "somebody"})], sha, set()))
        self.assertEqual(runner.find_review([wrong], sha, set())[0], "went_wrong")
        self.assertIsNone(runner.find_review([wrong], sha, {(2, "t2")}))
        self.assertEqual(runner.find_review([wrong, clean], sha, set())[0], "clean")

    def test_ci_state(self):
        done = {"status": "completed", "conclusion": "success"}
        self.assertEqual(runner.ci_state([]), "pending")
        self.assertEqual(runner.ci_state([done, {"status": "in_progress", "conclusion": None}]), "pending")
        self.assertEqual(runner.ci_state([done]), "success")
        self.assertEqual(runner.ci_state([done, {"status": "completed", "conclusion": "failure"}]), "failure")


class PagesTest(unittest.TestCase):
    def test_review_on_a_later_page(self):
        sha = "a1a1e7caa2f5080e4211538b953a375bae1ce9c7"
        talk = [{"id": n, "updated_at": "t", "user": {"login": "somebody"}, "body": "a comment"} for n in range(100)]
        review = {"id": 500, "updated_at": "t", "user": {"login": runner.REVIEW_AUTHOR}, "body": fixture("review-clean.md")}
        self.assertIsNone(runner.find_review(talk, sha, set()))
        comments = runner.join_pages([talk, [review]])
        self.assertEqual(len(comments), 101)
        self.assertEqual(runner.find_review(comments, sha, set())[0], "clean")

    def test_failed_check_run_on_a_later_page(self):
        good = {"status": "completed", "conclusion": "success"}
        bad = {"status": "completed", "conclusion": "failure"}
        pages = [{"total_count": 101, "check_runs": [good] * 100}, {"total_count": 101, "check_runs": [bad]}]
        self.assertEqual(runner.ci_state(pages[0]["check_runs"]), "success")
        self.assertEqual(runner.ci_state(runner.join_pages(pages, "check_runs")), "failure")

    def test_gh_pages_asks_for_each_page(self):
        calls = []
        self.addCleanup(setattr, runner, "sh", runner.sh)
        runner.sh = lambda cmd, **more: calls.append(cmd) or (0, "[[1], [2]]", "")
        self.assertEqual(runner.join_pages(runner.gh_pages("repos/o/r/issues/7/comments?per_page=100")), [1, 2])
        self.assertEqual(calls, [["gh", "api", "--paginate", "--slurp", "repos/o/r/issues/7/comments?per_page=100"]])


class TextTest(unittest.TestCase):
    def test_pr_body(self):
        body = runner.build_pr_body(13, "https://example.test/13", ["first", "second"], 7981, 7990, 10012, ["the report"])
        for part in (
            "Closes #13\n",
            "https://example.test/13",
            "- [ ] first\n- [ ] second\n",
            "Score: 7981 -> 7990 of 10012\n",
            "Opened by the yantra runner. A person merges.\n",
            "<details>",
            "```text\nthe report\n```\n",
        ):
            self.assertIn(part, body)
        self.assertTrue(body.startswith("Closes #13\n"))
        self.assertNotIn("[x]", body)

    def test_report_cannot_act_in_the_pr_body(self):
        report = "Closes #5\n@somebody\n</details>\n```\ncode\n```\n`````\nmore"
        block = runner.fenced(report)
        lines = block.split("\n")
        self.assertEqual((lines[0], lines[-1]), ("``````text", "``````"))  # 6: one more than the longest run
        self.assertEqual("\n".join(lines[1:-1]), report)
        body = runner.build_pr_body(13, "u", ["first"], 1, 2, 3, [report])
        self.assertLess(body.index("``````text"), body.index("Closes #5"))
        self.assertLess(body.index("</details>\n```"), body.rindex("``````"))
        self.assertEqual(len(runner.fenced("x" * 30000)), 20000 + len("```text\n\n```"))

    def test_agent_command(self):
        command = runner.agent_command("<prompt>", "/state/runs/t/repo", "/state/runs/t")
        self.assertEqual(command[:5], ["claude", "-p", "<prompt>", "--tools", "Read,Edit,Write,Glob,Grep,Bash"])
        allow = command[command.index("--allowedTools") + 1 : command.index("--disallowedTools")]
        self.assertEqual(
            allow,
            [
                "Read(//state/runs/t/repo/**)",
                "Read(//state/runs/t/input/**)",
                "Edit(//state/runs/t/repo/**)",
                "Bash(/state/runs/t/bin/yt *)",
            ],
        )
        deny = command[command.index("--disallowedTools") + 1 : command.index("--permission-mode")]
        self.assertEqual(deny, ["Edit(//state/runs/t/repo/.git/**)"])
        rest = command[command.index("--permission-mode") :]
        self.assertEqual(rest, ["--permission-mode", "dontAsk", "--strict-mcp-config", "--safe-mode"])
        rules = " ".join(allow)
        for word in ("docker", "git ", "ls", "cat", "Write(", "Glob", "Grep"):
            self.assertNotIn(word, rules)

    def test_prompt(self):
        prompt = runner.build_prompt("the brief", "/run", issue(), runner.review_text("/run/input/review-1.md"))
        self.assertTrue(prompt.startswith("the brief\n"))
        self.assertIn("`/run/bin/yt`", prompt)
        self.assertIn("`/run/input/`", prompt)
        self.assertIn("Title: t\nURL: u\n", prompt)
        self.assertIn("`/run/input/review-1.md`", prompt)


FAKE_DOCKER = """#!/bin/sh
echo "docker $*" >> "$YT_TEST_LOG"
if [ "$1" = image ]; then exit "${YT_TEST_NO_IMAGE:-0}"; fi
"""
FAKE_GIT = """#!/bin/sh
echo "git[$GIT_CONFIG_GLOBAL $GIT_CONFIG_SYSTEM] $*" >> "$YT_TEST_LOG"
"""


class YtTest(unittest.TestCase):
    """The generated wrapper, with a fake docker and a fake git on the PATH."""

    def setUp(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.root = os.path.realpath(tmp.name)
        self.clone = os.path.join(self.root, "repo")
        self.log = os.path.join(self.root, "log")
        os.makedirs(os.path.join(self.clone, "sub"))
        os.makedirs(os.path.join(self.root, "bin"))
        for name, text in (("docker", FAKE_DOCKER), ("git", FAKE_GIT), ("yt", runner.yt_script(self.clone))):
            path = os.path.join(self.root, "bin", name)
            with open(path, "w", encoding="utf-8") as f:
                f.write(text)
            os.chmod(path, 0o755)
        for path in ("repo/sub/q.sql", "outside.sql"):
            with open(os.path.join(self.root, path), "w", encoding="utf-8") as f:
                f.write("SELECT 1\n")
        os.symlink(os.path.join(self.root, "outside.sql"), os.path.join(self.clone, "link.sql"))

    def yt(self, *args, **env):
        """Return (exit code, stderr, the docker and git calls)."""
        if os.path.exists(self.log):
            os.remove(self.log)
        env.update(PATH=os.path.join(self.root, "bin") + ":/usr/bin:/bin", YT_TEST_LOG=self.log)
        yt = os.path.join(self.root, "bin", "yt")
        done = subprocess.run([yt] + list(args), cwd=self.clone, env=env, capture_output=True, encoding="utf-8")
        calls = []
        if os.path.exists(self.log):
            with open(self.log, encoding="utf-8") as f:
                calls = f.read().splitlines()
        return done.returncode, done.stderr, calls

    def test_the_script_is_correct_sh(self):
        yt = os.path.join(self.root, "bin", "yt")
        self.assertEqual(subprocess.run(["sh", "-n", yt]).returncode, 0)
        self.assertEqual(os.stat(yt).st_mode & 0o777, 0o755)

    def test_check_and_failures(self):
        box = "docker run --rm --network none --pull never -v %s:/src:ro " % self.clone
        code, _, calls = self.yt("check")
        self.assertEqual((code, calls), (0, [box + "yantra-rust:1.96.1 sh -c " + runner.BUILD_SCRIPT]))
        code, _, calls = self.yt("failures", "20")
        self.assertEqual((code, calls), (0, [box + "-e SLT_FAILURES=20 yantra-rust:1.96.1 sh -c " + runner.FAILURES_SCRIPT]))

    def test_sqlite_takes_a_file_inside_the_clone(self):
        for name in ("sub/q.sql", os.path.join(self.clone, "sub", "q.sql"), "sub/../sub/q.sql"):
            code, _, calls = self.yt("sqlite", name)
            self.assertEqual(code, 0)
            self.assertEqual(calls[0], "docker image inspect python:3.13-slim")
            box = "docker run --rm --network none --pull never -v %s:/src:ro " % self.clone
            self.assertTrue(calls[1].startswith(box + "python:3.13-slim python3 -c "))
            self.assertEqual(calls[-1].split()[-1], "/src/sub/q.sql")
            self.assertEqual(" ".join(calls).count(" -v "), 1)

    def test_sqlite_says_when_the_image_is_missing(self):
        code, err, calls = self.yt("sqlite", "sub/q.sql", YT_TEST_NO_IMAGE="1")
        self.assertEqual((code, calls), (3, ["docker image inspect python:3.13-slim"]))
        self.assertIn("python:3.13-slim is not on this machine", err)

    def test_status_and_diff(self):
        git = "git[/dev/null /dev/null] -c core.hooksPath=/dev/null -C %s " % self.clone
        self.assertEqual(self.yt("status"), (0, "", [git + "status --porcelain"]))
        self.assertEqual(self.yt("diff"), (0, "", [git + "diff"]))

    def test_each_other_call_is_refused(self):
        bad = [
            [],
            ["bogus"],
            ["docker", "run", "x"],
            ["check", "extra"],
            ["failures"],
            ["failures", ""],
            ["failures", "x"],
            ["failures", "3; id"],
            ["failures", "3", "4"],
            ["sqlite"],
            ["sqlite", "/etc/passwd"],
            ["sqlite", "../outside.sql"],
            ["sqlite", os.path.join(self.root, "outside.sql")],
            ["sqlite", "link.sql"],
            ["sqlite", "none.sql"],
            ["sqlite", "sub"],
            ["status", "--porcelain"],
            ["diff", "--output=/tmp/x"],
        ]
        for args in bad:
            code, err, calls = self.yt(*args)
            self.assertEqual((code, calls), (2, []), args)
            self.assertTrue(err.startswith("yt: "), args)


class LabelTest(unittest.TestCase):
    """The labels at the start and at the end of a run, with the GitHub writes replaced."""

    def setUp(self):
        self.calls = []
        self.fail = {}  # (action, label) -> the number of times that it fails
        for name in ("add_label", "remove_label", "add_comment", "RETRY_WAIT_S", "STATE"):
            self.addCleanup(setattr, runner, name, getattr(runner, name))
        self.state = tempfile.TemporaryDirectory()
        self.addCleanup(self.state.cleanup)
        self.real_remove_label = runner.remove_label
        runner.RETRY_WAIT_S = 0
        runner.add_label = lambda number, label: self.write("add", number, label)
        runner.remove_label = lambda number, label: self.write("remove", number, label)
        runner.add_comment = lambda number, text: self.write("comment", number, "")

    def write(self, action, number, label):
        self.calls.append((action, number, label))
        if self.fail.get((action, label), 0) > 0:
            self.fail[(action, label)] -= 1
            return 1, "", "HTTP 502"
        return 0, "", ""

    def test_claim(self):
        self.assertIsNone(runner.claim(13, True))
        self.assertEqual(self.calls, [("add", 13, "agent-working"), ("remove", 13, "agent-go")])

    def test_claim_of_a_manual_run_without_agent_go(self):
        self.assertIsNone(runner.claim(13, False))
        self.assertEqual(self.calls, [("add", 13, "agent-working")])

    def test_claim_when_the_add_fails_keeps_agent_go(self):
        self.fail[("add", "agent-working")] = 1
        self.assertIn("agent-working", runner.claim(13, True))
        self.assertEqual(self.calls, [("add", 13, "agent-working")])

    def test_claim_when_the_removal_fails_takes_agent_working_off_again(self):
        self.fail[("remove", "agent-go")] = 1
        self.assertIn("agent-go", runner.claim(13, True))
        self.assertEqual(self.calls, [("add", 13, "agent-working"), ("remove", 13, "agent-go"), ("remove", 13, "agent-working")])

    def test_a_label_that_is_not_on_the_issue_is_removed(self):
        self.addCleanup(setattr, runner, "sh", runner.sh)
        runner.sh = lambda cmd, **more: (1, "", "gh: Label does not exist (HTTP 404)")
        self.assertEqual(self.real_remove_label(13, "agent-go")[0], 0)
        runner.sh = lambda cmd, **more: (1, "", "gh: Server Error (HTTP 502)")
        self.assertEqual(self.real_remove_label(13, "agent-go")[0], 1)

    def finish(self, ready, pr=7):
        runner.STATE = tempfile.mkdtemp(dir=self.state.name)
        run = runner.Run(issue())
        run.pr = pr
        self.printed = io.StringIO()
        with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(self.printed):
            return run.finish(ready, "the note"), run

    def test_finish_ready(self):
        code, run = self.finish(True)
        self.assertEqual(code, 0)
        self.assertEqual(
            self.calls,
            [("comment", 13, ""), ("add", 7, "ready-for-human"), ("remove", 13, "agent-go"), ("remove", 13, "agent-working")],
        )
        self.assertFalse(os.path.exists(os.path.join(run.dir, "HANDOFF_FAILED")))

    def test_finish_needs_a_human(self):
        code, run = self.finish(False)
        self.assertEqual(code, 1)
        self.assertEqual(
            self.calls,
            [
                ("comment", 13, ""),
                ("add", 7, "needs-human"),
                ("add", 13, "needs-human"),
                ("remove", 13, "agent-go"),
                ("remove", 13, "agent-working"),
            ],
        )
        self.calls = []
        self.assertEqual(self.finish(False, pr=None)[0], 1)
        self.assertEqual(self.calls[1], ("add", 13, "needs-human"))

    def test_finish_tries_a_write_again(self):
        self.fail[("add", "ready-for-human")] = 2
        code, run = self.finish(True)
        self.assertEqual(code, 0)
        self.assertEqual(self.calls.count(("add", 7, "ready-for-human")), 3)
        self.assertEqual(self.calls[-1], ("remove", 13, "agent-working"))

    def test_finish_with_a_lost_label_is_exit_code_3(self):
        self.fail[("add", "needs-human")] = 99
        code, run = self.finish(False)
        self.assertEqual(code, 3)
        self.assertEqual(self.calls.count(("add", 7, "needs-human")), 3)
        self.assertEqual(self.calls.count(("add", 13, "needs-human")), 3)
        self.assertIn(("remove", 13, "agent-go"), self.calls)
        self.assertNotIn(("remove", 13, "agent-working"), self.calls)  # it stays as the mark
        with open(os.path.join(run.dir, "HANDOFF_FAILED"), encoding="utf-8") as f:
            text = f.read()
        for part in ("`needs-human` on pull request #7", "`needs-human` on issue #13", "`agent-working` is still on", "the note"):
            self.assertIn(part, text)
        self.assertIn("writes to GitHub failed", self.printed.getvalue())  # on stderr

    def test_finish_with_a_lost_comment_or_a_lost_removal_is_exit_code_3(self):
        self.fail[("comment", "")] = 99
        self.assertEqual(self.finish(True)[0], 3)
        self.fail = {("remove", "agent-working"): 99}
        code, run = self.finish(True)
        self.assertEqual(code, 3)
        with open(os.path.join(run.dir, "HANDOFF_FAILED"), encoding="utf-8") as f:
            self.assertIn("the removal of `agent-working`", f.read())


class FlowTest(unittest.TestCase):
    """The loop of Run.work, with each outside step replaced. No process starts."""

    def flow(self, reviews, checks=(), changes=()):
        """reviews: (kind, ci) of each wait. checks: the result of each check round, then None.

        changes: for each agent round, True when the agent changed a file. Then True.
        """
        state = tempfile.TemporaryDirectory()
        self.addCleanup(state.cleanup)
        self.addCleanup(setattr, runner, "STATE", runner.STATE)
        self.addCleanup(setattr, runner, "add_comment", runner.add_comment)
        runner.STATE = state.name
        self.comments = []
        runner.add_comment = lambda number, text: self.comments.append(text) or (0, "", "")
        run = runner.Run(issue())
        for path, text in (("tests/slt/baseline.txt", "a.test 5\n"), ("yantra/brief.md", "the brief\n")):
            os.makedirs(os.path.dirname(os.path.join(run.clone, path)), exist_ok=True)
            with open(os.path.join(run.clone, path), "w", encoding="utf-8") as f:
                f.write(text)
        self.prompts = []
        self.pushes = 0
        reviews = iter(reviews)
        checks = iter(checks)
        changes = iter(changes)

        def start_agent(prompt):
            run.starts += 1
            self.prompts.append(prompt)

        def push(criteria):
            self.pushes += 1
            run.pr = 7
            return "head-%d" % run.starts

        def wait_for_review(sha, seen):
            kind, ci = next(reviews)
            return kind, 3 if kind == "findings" else 0, "the review", (1, self.pushes), ci

        run.make_clone = lambda: None
        run.start_agent = start_agent
        run.commit = lambda: next(changes, True)
        run.checks = lambda scope: next(checks, None)
        run.push = push
        run.wait_for_review = wait_for_review
        try:
            return "ready", run.work(), run
        except runner.NeedsHuman as stop:
            return "needs", str(stop), run

    def test_clean_review_is_ready(self):
        state, note, run = self.flow([("clean", "success")])
        self.assertEqual((state, run.starts, self.pushes), ("ready", 1, 1))
        self.assertIn(os.path.join(run.dir, "bin", "yt"), self.prompts[0])

    def test_clean_review_with_red_ci_needs_a_human(self):
        self.assertEqual(self.flow([("clean", "failure")])[0], "needs")

    def test_first_review_not_done_needs_a_human(self):
        self.assertEqual(self.flow([("not_reviewed", "success")])[0], "needs")

    def test_unknown_review_needs_a_human(self):
        self.assertEqual(self.flow([("error", "success")])[0], "needs")

    def test_findings_then_not_reviewed_is_ready(self):
        state, note, run = self.flow([("findings", "success"), ("not_reviewed", "success")])
        self.assertEqual((state, run.starts, self.pushes), ("ready", 2, 2))
        self.assertIn("earlier Qodo review", note)
        review = os.path.join(run.dir, "input", "review-1.md")
        self.assertIn(review, self.prompts[1])
        with open(review, encoding="utf-8") as f:
            self.assertEqual(f.read(), "the review")

    def test_findings_three_times_needs_a_human(self):
        state, note, run = self.flow([("findings", "success")] * 3)
        self.assertEqual((state, run.starts, self.pushes), ("needs", 3, 3))

    def test_no_change_in_the_first_round_needs_a_human(self):
        state, note, run = self.flow([], changes=[False])
        self.assertEqual((state, run.starts, self.pushes), ("needs", 1, 0))

    def test_no_change_in_a_review_round_needs_a_human(self):
        state, note, run = self.flow([("findings", "success")], changes=[True, False])
        self.assertEqual((state, run.starts, self.pushes), ("needs", 2, 1))

    def test_went_wrong_is_asked_again_one_time(self):
        state, note, run = self.flow([("went_wrong", "pending"), ("clean", "success")])
        self.assertEqual((state, self.comments), ("ready", ["/agentic_review"]))
        state, note, run = self.flow([("went_wrong", "pending"), ("went_wrong", "pending")])
        self.assertEqual((state, self.comments), ("needs", ["/agentic_review"]))

    def test_failed_checks_use_the_fix_attempts(self):
        failed = ("build", "error[E0308]: mismatched types")
        state, note, run = self.flow([("clean", "success")], checks=[failed])
        self.assertEqual((state, run.starts, self.pushes), ("ready", 2, 1))
        output = os.path.join(run.dir, "input", "check-1.txt")
        self.assertIn(output, self.prompts[1])
        self.assertNotIn("E0308", self.prompts[1])  # the output is in the file, not in the prompt
        with open(output, encoding="utf-8") as f:
            self.assertEqual(f.read(), failed[1])
        state, note, run = self.flow([], checks=[failed] * 3)
        self.assertEqual((state, run.starts, self.pushes), ("needs", 3, 0))

    def test_checks_and_findings_share_the_fix_attempts(self):
        failed = ("scope", "outside")
        state, note, run = self.flow([("findings", "success"), ("findings", "success")], checks=[failed])
        self.assertEqual((state, run.starts, self.pushes), ("needs", 3, 2))


if __name__ == "__main__":
    unittest.main()
