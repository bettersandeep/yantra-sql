# yantra

"Yantra" is Sanskrit for a machine.
It is the name of the factory around lekha.
This folder holds its runner.

The runner takes one ticket from a label to a reviewed pull request without a person.
A label on a GitHub issue starts one headless coding agent.
The agent changes files.
The runner commits them, checks the result with its own commands, opens the pull request, waits for the Qodo review, and gives the findings back to the agent.
Then it stops with a label for a person.
A person merges.
The runner never merges.

## Files

- `runner.py`: the runner.
  Python 3, standard library only.
- `brief.md`: the standing brief that each coding agent gets, before the ticket.
- `test_runner.py` and `testdata/`: the tests.
  `testdata/` holds real review records of Qodo.
- `com.yantra.tick.plist`, `install.sh`, `uninstall.sh`: the launchd user agent that starts the runner on a Mac.

## A ticket

A ticket is an open GitHub issue with the label `ticket`.
Its body has these two parts, outside of a code block:

```
Acceptance criteria:
- The first criterion.
- The second criterion.

Scope: `src/`, `README.md`.
```

A scope path that ends with `/` covers everything below it.
Each other scope path is one file.
There are no globs.

`python3 yantra/runner.py check-ticket <N>` reads a ticket and prints the result as JSON.
It changes nothing.

## The flow

`python3 yantra/runner.py run <N>` does these steps for issue `<N>`.

1. It checks that `gh` and `qodo` are logged in, that Docker runs, that the two Docker images of the checks are on the machine, and that `claude` is on the PATH.
   If not, it exits with code 2 and writes nothing to GitHub.
   For a tick, the issue keeps `agent-go`.
2. It reads the issue.
   It puts the label `agent-working` on the issue, and then takes `agent-go` off.
   Then it validates the ticket.
3. It makes a new clone in the run directory and a branch from `origin/main`.
   The branch name is the first four words of the ticket title and the ticket number, for example `case-expressions-in-select-24`.
   If the remote branch is there already, it stops.
   It copies `ci/guard.sh` of `origin/main` into the run directory, and writes the command `yt` there.
4. It records the old score from `tests/slt/baseline.txt`.
5. It starts one agent in the clone with `claude -p`.
   The prompt is `yantra/brief.md` from the clone, the paths of the run, and the ticket.
   The time limit is 45 minutes.
6. It runs the local review: `qodo review` on the changes of the agent that are not committed yet.
   If the agent changed no file, the run needs a human, and there is no review.
   The section "The local review" below has the rules.
   When that review has no open finding at the threshold, it commits the changes of the agent as one commit.
   Then it removes each file that git ignores, so the checks see the commit and nothing more.
7. It runs its own checks.
   Each changed path is inside the scope.
   The guard of `origin/main` passes.
   Format, clippy and the tests pass in Docker with no network.
   The score is not lower than the old score.
8. If a check fails, it starts the agent again with the output of the check in a file.
   Then steps 6 and 7 run again.
9. It pushes the branch and opens the pull request.
   The body has `Closes #<N>`, the criteria as a task list that is not ticked, the score, and the report of the agent as plain text.
10. It waits for CI and for the Qodo review of the head commit.
    It reads Qodo's review record with `qodo read pr-review-session findings`.
    The review is in when the record has the status `completed` for the full SHA of the head commit.
    The time limit is 20 minutes.
    If Qodo's newest comment says that something went wrong, the runner comments `/agentic_review` one time and waits again.
11. If the record has an open finding at the threshold, it writes those findings to a file and starts the agent again.
    Then steps 6, 7, 9 and 10 run again.
12. It stops: it comments on the issue, sets the end label, and then removes `agent-working`.

The runner has 2 fix attempts for one ticket.
A failed check, a local review with findings and a pull request review with findings each use one.
So one ticket gets 3 agent starts at most, and the pull request gets 3 commits at most.
The commit of the first start has the message `Ticket #<N>: <title>`.
Each later one has `Ticket #<N>: fixes after checks or review, round <k>`.

The runner fails closed.
When a step cannot read or understand its input, the run stops as "needs a human" with the reason.
"Could not check" is never "passed".

## The local review

The pull request review comes late: by then the commit is public.
So Qodo also reviews the work before the runner commits it.
The runner runs `qodo review --json` in the clone, not the agent, and the tool limits of the agent do not change.

- The review gets the ticket URL (`--ticket`) and a context file (`--context-file`).
  Its `notes` has the ticket title and the acceptance criteria.
  After a fix round, its `recent_work` says what the round was for and which paths changed in it.
  The runner writes both. No text of the agent is in the file.
- Qodo runs with the environment of the agent: the GitHub tokens of the runner are not in it.
- The time limit is 10 minutes.
  One review took 8 to 51 seconds in the tests.
- The first review of a run has `--full`, so that no result of an older run of the ticket counts.
- The findings have the same fields and the same words as the review record of a pull request, and the same rules apply.
  The threshold is an open finding with the level `action_required` or `remediation_recommended`.
- When the review has findings at the threshold, they go to a file in `input/`, and the agent gets a round to fix them.
  Then the review runs again.
  That round is one of the 2 fix attempts of the ticket.
- The runner commits only when the local review has no open finding at the threshold.
  If one is still open and no fix attempt is left, the run needs a human, and the work of the last round is not committed.
  It stays in the clone of the run directory.
- If the agent changes no file in its round, for example because it judges each finding wrong, the run needs a human at once.
  If the agent removes each change, the run needs a human too, and Qodo is not asked.
- The pull request body and the comment on the issue get the line `Local review before the commit: <n> findings, <m> fixed, <k> open`.
  `<n>` counts the findings at the threshold in the local reviews of the run.
  Only a finding that Qodo closed as `implemented` counts as fixed.
  A finding with a different closed status is counted under that word, for example `1 dismissed` or `1 outdated`.
  The titles of the open ones, of the open `informational` ones and of the `dismissed` ones are listed too.
- The run needs a human, and there is no commit, when the review cannot run: an error of `qodo`, the time limit, a result that is not complete, or a result that the runner cannot read.
  That includes a finding with no text `id`, an id in `finding_state.still_open` or `finding_state.introduced` that has no open finding in `findings`, and a finding of an earlier review that a later result does not list.
  It also needs a human when the clone has the file `.qodo/session-context.json`, because Qodo reads that file as context and the agent could write it.
- The pull request body has a hard limit of 60000 characters.
  The reports of the agent share what the other parts leave.

## The review record

The runner does not read the text of Qodo's review comment.
That text changes its shape, and the record does not.
The one exception is the comment that says "something went wrong", because then there is no record.

Each finding in the record has a status and a level.

- The status says open or closed.
  Open: `pending`, `partial_implementation`, `not_implemented`, `focus_areas_edited`.
  Closed: `implemented`, `full_implementation`, `dismissed`, `detected_after_merge`, `outdated`.
- The level is the severity: `action_required`, `remediation_recommended` or `informational`.
  It does not say open or closed.

The threshold of the runner is an open finding with the level `action_required` or `remediation_recommended`.
Such a finding goes back to the agent.
The file for the agent has its title, description, category, level, location and evidence.
An open `informational` finding does not block, and the comment on the issue lists it as flagged, not fixed.
The comment also lists each `dismissed` finding, so that a person sees what was closed without a fix.

These cases need a human, with the reason in the comment:

- a status or a level that is not in the lists above, or a finding with no title, status or level;
- no review session in the record when the time limit ends;
- a session for an older commit when the time limit ends. The comment gives the commit that Qodo reviewed and the head commit;
- `review_complete` is false in the record, on the first review. Qodo sets that for a change that it judges trivial;
- `review_complete` is not there, or is not true or false;
- a finding with a `location` or an `evidence` that is not an object, or with `citations` that are not a list of objects.

`AGENTS.md` has the full gate under "Quality gate".

## Audit

`python3 yantra/runner.py audit [--limit N]` reads the record of the last N merged pull requests.
The default for N is 30.
It prints JSON with each finding that is open or has the status `detected_after_merge`.
The JSON also lists, as `no_full_review`, each pull request that Qodo did not review in full.
Such a pull request has no findings, and that is not the same as clean.
Qodo files `detected_after_merge` as closed, but it is a fault on the main branch that no pull request fixed.
The command only reads.

| Exit code | Meaning |
|---|---|
| 0 | No such finding at the threshold. |
| 1 | One or more such findings have the level `action_required` or `remediation_recommended`. |
| 2 | A record could not be read. The JSON names the pull request. |

A finding stays `detected_after_merge` in the record after a later pull request fixed it.
So a person reads the list and decides.

## What the runner executes from the candidate

The candidate is the tree that the agent changed.
The candidate must not change the checks that check it.

- The scope check is `git diff` of the runner on the Mac.
  It executes no file of the candidate.
- The runner's git reads no git config of the Mac, runs no hook, and gets the author from the command line.
  The runner stops when `.git/config` or `.git/hooks` of the clone changed.
- The checks run on the tree of the commit.
  A file that git ignores, such as a file in `target/` or `.yt/`, is removed before them.
- The guard is `ci/guard.sh` of `origin/main`.
  The runner mounts it over the guard of the candidate.
  So a candidate with a changed guard is checked by the guard of the base.
- The build check executes the candidate by design: `cargo fmt`, `cargo clippy` and `cargo test` run its code, its tests, `tests/slt.rs` and its cargo and toolchain files.
  That happens in a container with no network and with the clone mounted read-only.
  The scope check runs before it, so each of those files is the file of `origin/main` unless the Scope of the ticket names it.
- `runner.py` and `yt` are not in the clone.
  `brief.md` is read from the clone before the first agent starts.

## Labels

The runner makes these labels when they are missing.

| Label | On | Who sets it | Meaning |
|---|---|---|---|
| `agent-go` | issue | a person | Start the runner on this ticket. |
| `agent-working` | issue | the runner | A run is in progress. |
| `ready-for-human` | pull request | the runner | CI is green, and the Qodo review of the head commit is complete with no open finding at the threshold. |
| `needs-human` | issue, and pull request if there is one | the runner | The run stopped, and the comment on the issue gives the reason. |

`ready-for-human` has one more case.
Qodo does not review a change that it judges trivial, and then `review_complete` is false in the record.
If that happens after the runner handled an earlier review of the same pull request, and no finding at the threshold is open, the pull request is ready, and the comment says so.
If it happens on the first review, the run needs a human.

At the start, the runner adds `agent-working` first.
If that fails, `agent-go` stays and the next tick takes the ticket again.
If the removal of `agent-go` fails, the runner takes `agent-working` off again.
A manual `run` does the same when the issue has `agent-go`.

At the end, the runner tries each write to GitHub 3 times.
It removes `agent-go` in each case.
It removes `agent-working` last, and only when the comment and the end label are there.
If a write still fails, the run directory gets the file `HANDOFF_FAILED` with the writes that a person must do, and the exit code is 3.
If that file cannot be written, its text goes to stderr, which is `tick.log` for a tick.

The handoff does not need the run directory.
The log never stops a run: when `run.log` cannot be written, the line goes to stderr.
When the run directory is gone during the wait for the review, the wait goes on, because it needs only GitHub and Qodo.
An error in the runner after the label change ends as `needs-human`, with the error in the comment.

Exit codes of `run` and `tick`:

| Code | Meaning |
|---|---|
| 0 | Ready for a human. |
| 1 | Needs a human. |
| 2 | The run could not start. |
| 3 | The run ended, and a last write to GitHub failed. |

## Images

The runner needs three Docker images. It never pulls an image itself, so make them one time, with a network:

```
docker build -t yantra-rust:1.96.1 -f yantra/rust.Dockerfile yantra
docker pull golang:1.23
docker pull python:3.13-slim
```

`yantra-rust:1.96.1` runs the build checks.
`golang:1.23` runs the guard, because it has git and sha256sum.
`python:3.13-slim` is for `yt sqlite`.
A run does not start when the first or the second image is missing.
A cleanup of unused Docker images removes them, and then they must be made again.

## Start and stop

Work one ticket by hand:

```
python3 yantra/runner.py run 13
```

Let the Mac take tickets without a person:

```
sh yantra/install.sh
```

This installs a launchd user agent, `com.yantra.tick`.
Each 600 seconds it runs `python3 yantra/runner.py tick`.
`tick` takes the open issue with the lowest number that has the label `agent-go`, and runs the ticket.
If there is no such issue, it exits and prints nothing.
One ticket runs at a time.
The lock is the directory `lock` in the state directory, with the pid inside.
A lock with a dead pid is taken over.
A run releases only the lock that has its own pid.
When the state directory is lost during a run, the run puts its lock back with the next log line.
If a later tick took a new lock before that, the run leaves that lock and writes this to its log.

To start a ticket, put the label `agent-go` on it.

Stop:

```
sh yantra/uninstall.sh
```

This also stops a run that is in progress.
The runner then kills its agent and removes the lock.
The issue keeps the label `agent-working`.
Remove it by hand, and put `agent-go` again to start a new run.
A new run needs the remote branch of the ticket to be gone.

launchd runs `runner.py` from this checkout, with the branch that is checked out here.
The agent gets `yantra/brief.md` from the new clone, which is `origin/main`.

## Update

After each update of this checkout, run `sh yantra/install.sh` again.
The installed plist holds the state path of the time of the install, and gives it to the runner as `YANTRA_STATE`.
So an old install keeps its old state directory until you install again.
`install.sh` prints the state path that it wrote into the plist.
Run directories of the old state directory stay there.

## Logs

The state directory is `~/.local/state/yantra`.
The variable `YANTRA_STATE` changes it.
Do not put the state in a cache folder, because a cleanup tool can empty that while a run waits for the review.
The path can have only letters, digits and `_ @ % + = : . / -`.
With white space, a quote or a different special character in the real path, the runner does not start: it exits with code 2 and writes nothing to GitHub.

- `tick.log`: the output of each tick from launchd.
- `runs/<branch>-<UTC time>/run.log`: each command of one run, with its exit code and output, and UTC times.
- `runs/<branch>-<UTC time>/agent-prompt-<k>.md` and `agent-report-<k>.md`: the prompt and the report of agent start `<k>`.
- `runs/<branch>-<UTC time>/input/`: the files that the agent can read, which are the output of a failed check and the open findings of a Qodo review.
- `runs/<branch>-<UTC time>/local-review-<k>.json` and `local-review-context.json`: the result of local review `<k>`, and its context file.
- `runs/<branch>-<UTC time>/bin/yt` and `guard.sh`: the command of the agent, and the guard of `origin/main`.
- `runs/<branch>-<UTC time>/HANDOFF_FAILED`: only when a last write to GitHub failed.
- `runs/<branch>-<UTC time>/repo`: the clone.

The runner does not delete run directories.

## What the agent can and cannot do

The agent can:

- read files in the clone and in the `input/` directory of the run, with the tools Read, Glob and Grep.
- edit and write files in the clone, with the tools Edit and Write.
- run one command, `yt`, with the tool Bash.

`yt` is a script that the runner writes to `bin/yt` in the run directory, outside the clone.
The agent calls it with its full path, and only that text is allowed.
A Bash rule compares the text of the command, so a call with a relative path is denied: the agent could change its working directory to a folder where it can write a file with that name.
`yt` does not depend on the working directory or on the PATH of the agent: the path of the clone and the PATH for `docker` and `git` are fixed in the script.
It builds each Docker command itself: only the clone is mounted, read-only, with `--network none --pull never`.

| Command | What it does |
|---|---|
| `yt check` | The build check of the runner: format, clippy and the tests. |
| `yt failures <n>` | The first `<n>` failing test records. `<n>` must be digits. |
| `yt sqlite <file>` | Runs each line of `<file>` as one statement in the real SQLite, in one in-memory database. `<file>` must be a regular file inside the clone. |
| `yt status` | `git status --porcelain` of the clone. |
| `yt diff` | `git diff` of the clone. |

The agent cannot:

- read or write a file of the Mac outside the clone.
  The only exception is that it can read `input/`.
- write in `.git/` of the clone.
- run `docker`, `git`, `gh`, `ls`, `cat` or a different command, or two commands in one call.
  The permission mode `dontAsk` denies each call that is not on the list.
- mount a directory, start a container with a network, or download an image.
- commit, push, open a pull request, set a label, comment or merge.
  The runner does the first five.
  A person merges.
- use a web tool, a sub-agent or an MCP server.
  The runner gives it only the six tools above, and no MCP configuration.
- get the hooks, plugins, skills or `CLAUDE.md` files of the user of the Mac.
  The runner starts it with `--safe-mode`.
- tick an acceptance criterion.
  The runner writes the task list not ticked.
- make the run pass with its report.
  The runner runs the checks again and does not read the report as a result.
- make its report act on GitHub.
  The report is in the pull request body as a code block, so `Closes #x`, `@name` and HTML in it are plain text.
- delete or rename a file.
  A ticket that needs that needs a person.

Limits that you must know:

- The fence is the permission rules of Claude Code and the script `yt`.
  It is not a virtual machine.
  The agent process is a process of the user of the Mac.
- The code of the candidate runs in the containers of `yt check` and of the runner's build check.
  The containers have no network and see only the clone, read-only.
- `.yt/` in the clone is the scratch directory of the agent.
  git does not see it, so the runner does not commit it.
  The runner empties it after each agent start.
- The ticket goes into the prompt of an agent.
  Put `agent-go` only on a ticket that you read.
  The author of an issue can edit its body later.
- The output of a failed check and the findings of a review are files that the agent reads as data.
