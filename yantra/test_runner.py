"""Tests for the pure functions of runner.py. No network, gh, git, docker or claude.

Run: python3 -m unittest discover -s yantra -p 'test_*.py'
"""
import os
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
            "the report",
        ):
            self.assertIn(part, body)
        self.assertTrue(body.startswith("Closes #13\n"))
        self.assertNotIn("[x]", body)

    def test_agent_command(self):
        command = runner.agent_command("<prompt>")
        self.assertEqual(command[:3], ["claude", "-p", "<prompt>"])
        self.assertIn("Bash(git add *)", command)
        self.assertEqual([part for part in command if "push" in part or "gh" in part or "Web" in part], [])

    def test_prompt(self):
        prompt = runner.build_prompt("the brief", issue(), runner.review_text("/run/review-1.md"))
        self.assertTrue(prompt.startswith("the brief\n"))
        self.assertIn("Title: t\nURL: u\n", prompt)
        self.assertIn("`/run/review-1.md`", prompt)


class FlowTest(unittest.TestCase):
    """The loop of Run.work, with each outside step replaced. No process starts."""

    def flow(self, reviews, checks=(), commits=True):
        """reviews: (kind, ci) of each wait. checks: the result of each check round, then None."""
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

        def start_agent(prompt):
            run.starts += 1
            self.prompts.append(prompt)

        def push(criteria):
            self.pushes += 1
            run.pr = 7
            return run.head()

        def wait_for_review(sha, seen):
            kind, ci = next(reviews)
            return kind, 3 if kind == "findings" else 0, "the review", (1, self.pushes), ci

        run.make_clone = lambda: None
        run.start_agent = start_agent
        run.head = lambda: "head-%d" % (run.starts if commits else 0)
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
        self.assertIn(os.path.join(run.dir, "review-1.md"), self.prompts[1])

    def test_findings_three_times_needs_a_human(self):
        state, note, run = self.flow([("findings", "success")] * 3)
        self.assertEqual((state, run.starts, self.pushes), ("needs", 3, 3))

    def test_findings_and_no_new_commit_needs_a_human(self):
        state, note, run = self.flow([("findings", "success")], commits=False)
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
        self.assertIn("error[E0308]", self.prompts[1])
        state, note, run = self.flow([], checks=[failed] * 3)
        self.assertEqual((state, run.starts, self.pushes), ("needs", 3, 0))

    def test_checks_and_findings_share_the_fix_attempts(self):
        failed = ("scope", "outside")
        state, note, run = self.flow([("findings", "success"), ("findings", "success")], checks=[failed])
        self.assertEqual((state, run.starts, self.pushes), ("needs", 3, 2))


if __name__ == "__main__":
    unittest.main()
