---
name: unseen-check
description: Check a lekha engine change against two SQLite test files that the coding agents never see, and count the wrong values. Use it before a person merges a pull request that changes `src/`, and when someone asks if an engine change is right on queries that it was not built against.
---

# Unseen check

The score in `tests/slt/baseline.txt` is measured on a test file that the coding agent can read.
An agent can fit its code to that file.
This check runs the same engine on two more files of SQLite's sqllogictest suite, about 20,000 queries, that are not in the repo.
The engine has one hard rule: for an input that it does not support, it returns an error, and it never returns a guessed value.
So the result that counts is the number of wrong values, and it must be 0.

## Inputs

- The path of a lekha directory with the change. A checkout of a pull request branch or the work tree of a run are both correct.
- Docker, with the image `yantra-rust:1.96.1`. `yantra/rust.Dockerfile` builds it.
- The two unseen files. The script gets them one time from a public git mirror of SQLite's suite, at a fixed commit, and checks them against `SHA256SUMS` in this folder.
  They go to `~/.local/state/yantra/unseen`, or to the directory in `UNSEEN_DIR`.

## Rules

- Never copy the unseen files, their queries or their expected results into the lekha directory, a ticket, a pull request, a commit message or a prompt for a coding agent. If the agents see them, the check has no value.
- Never open an output file of the script. When you report failures, give counts only.
- Do not change `unseen.sh` or `SHA256SUMS` to make a run pass.

## Steps

1. Find the directory to check. If the person gave a pull request number, make a checkout of its head in a new directory outside this repo. Print the commit with `git -C <dir> rev-parse HEAD`, and then show the changes that are not committed with `git -C <dir> status --short`. Run each command in a call of its own.
2. Run the script from the root of this repo:

   ```
   sh .claude/skills/unseen-check/unseen.sh <dir>
   ```

   It takes 1 to 3 minutes. The container has no network, and it works on a copy. It runs the tests two times: one time to measure, and one time with the measured numbers, where each test must pass.
3. Read the four lines that start with `unseen:` and the exit code.
4. Run the script one more time on `main`, for the comparison: `sh .claude/skills/unseen-check/unseen.sh .` when this repo is a clean checkout of `main`. If it is not, say so and leave the comparison out.
5. When the exit code of step 2 is 1: do not open the output file. It has the unseen queries and their expected results, and no agent reads them. Report the count and the path of the file. A person opens it.
6. Report in this form, with the real numbers:

   ```
   Unseen check of <commit or directory>
   slt_good_1.test: <passed>/<applicable> (main: <passed>)
   slt_good_2.test: <passed>/<applicable> (main: <passed>)
   Wrong values: <n>
   Result: PASS (no wrong value) or FAIL (<n> wrong values, a person must read the output file)
   Output file: <the file that step 2 named>
   ```

## Expected result

- Exit code 0 and `unseen: wrong values 0`: PASS. The misses are errors for features that are not built, and that is allowed.
- Exit code 1: FAIL. The change gives a value that SQLite does not give. It must not merge.
- The two pass counts are information. They must not go down against the last run on `main`, and a lower count is a reason to look, not a FAIL by itself.

## Evidence to save

The four `unseen:` lines, the exit code, and the commit that was checked.
Each run writes its full output to a file of its own, `run-<time>-<random>`, in the directory of the unseen files. Do not open or publish these files: they have the queries.

## Stop and ask

Stop, change nothing, and tell the person what is missing when:

- the exit code is 2. The message says why: no lekha directory, no Docker image, no network for the first download, a checksum that does not match, or a directory that does not pass its own tests. The check needs a tree whose own tests pass, with the correct number in `tests/slt/baseline.txt`.
- the directory has no `tests/slt.rs`, or the commit is older than the harness.
- you are not sure which commit or directory the person means.
