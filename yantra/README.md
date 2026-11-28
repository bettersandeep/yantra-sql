# yantra

"Yantra" is Sanskrit for a machine.
It is the name of the factory around lekha.
This folder holds its runner.

The runner takes one ticket from a label to a reviewed pull request without a person.
A label on a GitHub issue starts one headless coding agent.
The runner checks the result with its own commands, opens the pull request, waits for the Qodo review, and gives the findings back to the agent.
Then it stops with a label for a person.
A person merges.
The runner never merges.

## Files

- `runner.py`: the runner.
  Python 3, standard library only.
- `brief.md`: the standing brief that each coding agent gets, before the ticket.
- `test_runner.py` and `testdata/`: tests for the pure functions.
  `testdata/` holds real Qodo review comments.
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

1. It checks that `gh` is logged in, that Docker runs and that `claude` is on the PATH.
   If not, it exits with code 2 and writes nothing to GitHub.
2. It reads the issue and validates the ticket.
3. It makes a new clone in the run directory and the branch `ticket-<N>` from `origin/main`.
   If the remote branch is there already, it stops.
4. It records the old score from `tests/slt/baseline.txt`.
5. It starts one agent in the clone with `claude -p`.
   The prompt is `yantra/brief.md` from the clone and the ticket.
   The time limit is 45 minutes.
6. It runs its own checks.
   The tree is clean and has a commit.
   Each changed path is inside the scope.
   `ci/guard.sh` passes.
   Format, clippy and the tests pass in Docker with no network.
   The score is not lower than the old score.
7. If a check fails, it starts the agent again with the output of the check, and runs each check again.
8. It pushes the branch and opens the pull request.
   The body has `Closes #<N>`, the criteria as a task list that is not ticked, the score, and the report of the agent.
9. It waits for CI and for the Qodo review of the head commit.
   The time limit is 20 minutes.
   If Qodo answers that something went wrong, the runner comments `/agentic_review` one time and waits again.
10. If the review has findings, it saves the review to a file and starts the agent again.
    The agent fixes the real findings in one separate commit.
    Then steps 6, 8 and 9 run again.
11. It stops with a label, comments on the issue, and removes `agent-working`.

The runner has 2 fix attempts for one ticket.
A failed check and a review with findings each use one.
So one ticket gets 3 agent starts at most.

The runner fails closed.
When a step cannot read or understand its input, the run stops as "needs a human" with the reason.
"Could not check" is never "passed".

## Labels

The runner makes these labels when they are missing.

| Label | On | Who sets it | Meaning |
|---|---|---|---|
| `agent-go` | issue | a person | Start the runner on this ticket. |
| `agent-working` | issue | the runner | A run is in progress. The runner removes it at the end. |
| `ready-for-human` | pull request | the runner | CI is green and the Qodo review of the head commit is clean. A person reads and merges. |
| `needs-human` | issue, and pull request if there is one | the runner | The run stopped. The comment on the issue gives the reason. |

`ready-for-human` has one more case.
Qodo does not review a change that it judges trivial.
If that happens after the runner handled an earlier review of the same pull request, the pull request is ready, and the comment says so.
If it happens on the first review, the run needs a human.

Exit codes of `run` and `tick`: 0 for ready, 1 for needs a human, 2 when the run could not start.

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
`tick` takes the open issue with the lowest number that has the label `agent-go`, changes the label to `agent-working`, and runs the ticket.
If there is no such issue, it exits and prints nothing.
One ticket runs at a time.
The lock is the directory `lock` in the state directory, with the pid inside.
A lock with a dead pid is taken over.

To start a ticket, put the label `agent-go` on it.

Stop:

```
sh yantra/uninstall.sh
```

This also stops a run that is in progress.
The runner then kills its agent and removes the lock.
The issue keeps the label `agent-working`.
Remove it by hand, and put `agent-go` again to start a new run.
A new run needs the remote branch `ticket-<N>` to be gone.

launchd runs `runner.py` from this checkout, with the branch that is checked out here.
The agent gets `yantra/brief.md` from the new clone, which is `origin/main`.

## Logs

The state directory is `~/Library/Caches/yantra`.
The variable `YANTRA_STATE` changes it.

- `tick.log`: the output of each tick from launchd.
- `runs/ticket-<N>-<UTC time>/run.log`: each command of one run, with its exit code and output, and UTC times.
- `runs/ticket-<N>-<UTC time>/agent-prompt-<k>.md` and `agent-report-<k>.md`: the prompt and the report of agent start `<k>`.
- `runs/ticket-<N>-<UTC time>/review-<k>.md`: a Qodo review that went back to the agent.
- `runs/ticket-<N>-<UTC time>/repo`: the clone.

The runner does not delete run directories.

## What the agent can and cannot do

The agent can:

- read, search, edit and write files with the tools Read, Glob, Grep, Edit and Write.
- run a shell command with the tool Bash, if the command starts with `docker run --rm --network none`, `git add`, `git commit`, `git status`, `git diff`, `git log`, `git checkout`, `ls` or `cat`.

The agent cannot:

- use a web tool, a sub-agent or an MCP server.
  The runner gives it only the six tools above, and no MCP configuration.
- run `gh`, `git push` or a command with a different start.
  The permission mode `dontAsk` denies each command that is not on the list.
- push, open a pull request, set a label, comment or merge.
  The runner does the first four.
  A person merges.
- tick an acceptance criterion.
  The runner writes the task list not ticked.
- make the run pass with its report.
  The runner runs the checks again and does not read the report as a result.

Limits that you must know:

- The list of commands is a fence for an agent that follows its brief.
  It is not a sandbox.
  `docker run --rm --network none -v <path>:<path>` can mount a directory of the Mac, and the file tools are not held inside the clone by this list.
- `--network none` is for the container.
  Docker itself downloads an image that is not present, unless the command has `--pull never`.
- The runner stops the run if the agent changed `.git/config` or `.git/hooks` of the clone, because the runner's own `git push` reads them.
  A hook that removes itself is not seen.
- The settings, hooks and `CLAUDE.md` of the user of the Mac also apply to the agent.
- The ticket, the output of a failed check and the review text go into the prompt of an agent.
  Put `agent-go` only on a ticket that you read.
  The author of an issue can edit its body later.
