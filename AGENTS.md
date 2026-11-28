# AGENTS.md

Rules for the coding agents and the people who work in this repo.
Qodo reads this file as review context.

## What this repo is

lekha is a small SQL database that gives the same answers as SQLite.
AI coding agents write it.
SQLite's public sqllogictest files score it: `tests/slt.rs` runs them and prints one line per file, `slt: <file> <passed>/<applicable>`.

## Setup

- Rust 1.96.1. `rust-toolchain.toml` pins it.
- No other tool and no dependency.
- Build: `cargo build`

## Verify

Run these before each commit.
CI runs the same commands.

```
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test -- --nocapture
```

To see the first 20 failing test records with the SQL, the expected result and the result or error:

```
SLT_FAILURES=20 cargo test --test slt -- --nocapture
```

To run without a local Rust install (no network, read-only source):

```
docker run --rm --network none -v "$PWD":/src:ro rust:1.96.1 sh -c 'cp -r /src /tmp/w && cd /tmp/w && cargo test --offline -- --nocapture'
```

## Rules

### 1. An error is an answer. A guess is not.

- Scope: `src/**`.
- Rule: when the engine does not support an input, it returns an error. It never returns a value that it did not compute by SQLite's rules. Do not add a default branch that returns something plausible.
- Reason: the score counts a wrong value and an error in the same way, as one failed record. They are not the same. An error tells the next ticket what to build. A wrong value can pass by luck on one record and then hide. After the first capability (pull request #4), 5,435 records failed, and each one was an error for a feature that was not there. That list became the backlog.
- Check: none that is automatic. The review and the hidden test file are the checks.

### 2. The engine does not contain SQLite

- Scope: `Cargo.toml`, `Cargo.lock`, `src/**`.
- Rule: no dependency, no `unsafe`, and no call to an external program or library.
- Reason: the fastest way to pass each test is to link SQLite and hand it the query. That gives a full score and builds nothing.
- Check: `ci/guard.sh` fails when `Cargo.lock` has more than one package, and the cargo commands in CI use `--locked`, so a dependency that is only in `Cargo.toml` stops the build. The crate forbids `unsafe` code.

### 3. The test data is not yours to change

- Scope: `tests/slt/**/*.test`, `tests/slt/SHA256SUMS`, `tests/slt.rs`, `ci/`, `.github/`, `yantra/`, `.claude/skills/unseen-check/`.
- Rule: a ticket for the engine does not edit a test file, the checksum file or the harness. If the harness is wrong, that is a separate ticket.
- Reason: the score is the one number that this project trusts. A change that raises the score by a change to the thing that measures it is worse than no change.
- Check: `ci/guard.sh` compares the list of test files with the list in `tests/slt/SHA256SUMS`, path by path, and each file with its checksum. In a pull request it also fails when one change touches the code (`src/`, `Cargo.toml`, `Cargo.lock`, `server/`, `site/`) and the things that measure it (`tests/slt/` but not the baseline, `tests/slt.rs`, `.github/`, `ci/`, `yantra/`, `.claude/skills/unseen-check/`). A pull request that changes only the measuring side is not stopped by a check. A person reads those.

### 4. The baseline is the measured number

- Scope: `tests/slt/baseline.txt`.
- Rule: write the exact number that the test prints. The number goes up or stays the same. A pull request that lowers it says why in its description.
- Reason: the number in the repo is always the true score, so nobody has to run the tests to know it.
- Check: the test fails when the measured count is not equal to the baseline. "It does not go down" has no automatic check yet.

### 5. Take SQLite's behaviour from a source, not from memory

- Scope: `tests/slt.rs`, and each place in `src/**` that copies a rule of SQLite.
- Rule: for the test file format, read `tests/slt/FORMAT.wiki`. For SQL behaviour, name the SQLite document or the test record that shows it, in a code comment or in the pull request.
- Reason: pull request #3. The harness was written from a description of the format that came from memory. Qodo found three places where it differed from the format document, and all three were real.
- Check: none that is automatic. The review is the check.

### 6. A check does not come from the change that it checks

- Scope: `ci/guard.sh`, `.github/workflows/ci.yml`, `yantra/runner.py`, and each new check.
- Rule: a check runs from the base branch, or from a place outside the tree of the change. The change cannot edit, replace or redirect it. When you add a check, say in a comment where its code comes from at run time.
- Reason: four findings of Qodo in three pull requests are this one mistake. In #7 a pull request could change a test file and its checksum together. In #17 the runner ran the guard from the agent's copy, and the agent could reach the git config that the runner's push reads. In #22 a relative path let the agent put its own script where the runner's command was expected.
- Check: the runner takes `ci/guard.sh` from `origin/main`, and the one command of the agent is a file outside its clone. The workflow `guard.yml` runs on `pull_request_target`, so GitHub takes it from the base branch and a pull request cannot change or remove it. It runs the guard of the base branch on the files of the pull request. The guard step in `ci.yml` is a fast signal only, because that file comes from the pull request. A pull request that changes only the workflow can still weaken a check, for example with `continue-on-error`. No check stops that today. A person reads each change to `.github/`. Not covered: a red guard blocks a merge only when the person or the runner reads the checks. The runner needs each check to be green.

## Quality gate

The yantra runner enforces this gate on each ticket, in this order.
A person merges.
The runner never merges.

1. Local review: before each commit, Qodo reviews the work that is not committed yet (`qodo review`). A finding at the threshold goes back to the agent 1 time in a run, and that uses a fix round of step 7. The runner commits only when no such finding is open. If the review cannot run, there is no commit. Qodo reviews a snapshot of the work in a copy that the agent cannot write to, and the runner makes the commit from that same snapshot.
2. Scope: each changed path of the commit is inside the `Scope:` line of the ticket.
3. Guard: `ci/guard.sh` from the base branch passes.
4. Build, lint and tests: `cargo fmt --check`, `cargo clippy` with warnings as errors, and `cargo test` pass.
5. Score: the test prints the exact number that is in `tests/slt/baseline.txt`, and the number does not go down.
6. Review: the Qodo review of the head commit is complete. The runner reads Qodo's review record, not the text of its comment.
7. Findings: each open `action_required` and `remediation_recommended` finding is fixed. There are at most 2 fix rounds for one ticket: failed checks, findings of the local review and findings of the pull request review use the same 2 rounds. So the agent starts at most 3 times. Each commit is a commit by the runner.
8. Open `informational` findings and `dismissed` findings are listed for the person. The runner does not fix them.

A finding is open when its status is `pending`, `partial_implementation`, `not_implemented` or `focus_areas_edited`.
A status, a level or a record shape that the runner does not know is never read as clean.
When Qodo skips a head commit that it judges trivial, the gate passes only after an earlier full review of the same pull request.

The end states:

- `ready-for-human`: each step passed, and CI is green on the head commit.
- NOT DONE, `needs-human`: a finding at the threshold is still open after the rounds, a check could not run, the review does not cover the head, or a round changed nothing.

`python3 yantra/runner.py audit` lists the findings that Qodo recorded after a merge. Qodo files them as closed, but no pull request fixed them.

## Pull requests

- One ticket per pull request. The description starts with `Closes #<issue>` and lists the acceptance criteria of the ticket.
- A fix for a review finding is a separate commit, with a test that fails on the commit before it.
- A person merges a pull request when CI is green and the quality gate passed.
