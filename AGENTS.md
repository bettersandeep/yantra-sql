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
- Check: `#![forbid(unsafe_code)]` in the crate, and a CI step that fails when `Cargo.lock` has more than one package.

### 3. The test data is not yours to change

- Scope: `tests/slt/**/*.test`, `tests/slt/SHA256SUMS`, `tests/slt.rs`.
- Rule: a ticket for the engine does not edit a test file, the checksum file or the harness. If the harness is wrong, that is a separate ticket.
- Reason: the score is the one number that this project trusts. A change that raises the score by a change to the thing that measures it is worse than no change.
- Check: a CI step compares each test file with its checksum in `tests/slt/SHA256SUMS`, and fails when a test file is not in that list. A change to the harness has no automatic check yet.

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

## Pull requests

- One ticket per pull request. The description starts with `Closes #<issue>` and lists the acceptance criteria of the ticket.
- A fix for a review finding is a separate commit, with a test that fails on the commit before it.
- A pull request merges when CI is green and Qodo's review of the head commit has no open findings.
