# Verified agent workflow

Chief routes project goals to commanders. Commanders plan, hire and delegate.
Implementers open PRs; independent reviewer, tester and security roles judge
that work. The core follows forge feedback and merges only when its shared
gate passes and a person has requested a merge or enabled automatic merging.

## Review and merge

`repo_review` returns the committed PR diff, `head_sha` and `pull_url`. It uses
the PR's actual base commit, including PRs against nondefault branches. Working
files cannot change the committed diff being reviewed. `pr_review` requires both
identifiers; a stale review or a card belonging to another worktree is refused.

All three roles are required even when nobody has been hired for one. Their
roles are recorded with their reviews, so dismissing an agent cannot remove a
check. A request for changes clears earlier approvals for that PR commit. Old
reviews without a commit remain history and cannot authorize a merge.

The reviewer must be different from the card's author. A tester's approval
also requires a passing `repo_test` test run by that tester on that commit;
an install or build command alone does not count. Forge checks must be present
and completed with success, neutral or skipped results. Missing, pending,
cancelled, failing or unknown results hold the merge. Projects without CI need
to configure a forge check before merging through Agentland.

Manual merge, automatic merge and the PR watcher use the same gate. Review
recording and merge decisions are serialized. GitHub receives
`--match-head-commit <sha>` to reject a push racing the merge. New commits trigger
fresh review requests. Moving a card to Done by hand remains a board operation,
not evidence that its code was merged or validated.

## Isolated tests

`repo_test` makes a detached clone of the requested PR commit under the
Agentland data directory's `test-checkouts`. It runs a supported test/build/lint
or dependency-install program there, records its result and retains the checkout
and a log. Pass the returned `checkout` into the next call to reuse installed
dependencies. Use `directory` to select a package within that checkout.

Supported programs are cargo, npm, pnpm, yarn, pytest, python/python3 and go,
with test/build/lint/install subcommands as appropriate. Each command has a
300-second timeout and returns the last 32 KiB of output. A command that alters
tracked source or moves HEAD cannot produce a passing proof. Retained checkouts
and recovery directories are not automatically deleted.

This is the same MCP tool for Claude, Codex and Gemini, including roles whose
native shell permissions are read-only. The clone isolates Git state and normal
build artifacts; it is not an OS sandbox for malicious test code.

## Activity and delivery

Crew launches install native lifecycle hooks for Claude Code, Codex and Gemini.
Each launch, including a resume or provider change, gets a separate activity
file. Old-session events cannot change the new session's state. Only activity
and timestamps are stored; hook payloads are not retained.

Active, blocked and exited native states veto automatic delivery. Pane checks
remain necessary to protect text a person is composing, and provide fallback
when hooks are absent or disabled. Automatic deliveries are serialized per
session and rechecked before pasting and submitting. Explicit plan/resume
answers use a separate guarded path. Enter is never repeatedly pressed into an
unrecognized screen. A stopped process is reported as interrupted; quietness
with a diff requests verification rather than asserting successful completion.
A failed process probe conservatively keeps a session alive.

Hook contracts were checked against local Claude Code 2.1.274, Codex 0.154.0
and Gemini 0.58.0. Native provider permissions and model behavior remain distinct;
see [engine parity](engine-parity.md).

## Feedback and recovery

CI failures, change requests and unresolved GitHub inline discussions use a
durable outbox. Identical feedback is deduplicated across restarts. Inline
feedback retains thread IDs, file names, line numbers, comment links and edit
timestamps; outdated unresolved discussions are labelled rather than hidden.
Threads are paginated; the latest 20 comments per thread are included.

After three distinct repair feedback rounds, automatic repairs pause. The
commander receives a durable escalation and the user sees a notice and card
history. A person can choose **resume repairs** in the card details after
reviewing the cause. Agents cannot reset their own repair budget.

Forced worktree removal moves its files into `recoveries/<id>/files` rather
than discarding them. `worktree.json` identifies the original branch and path.
When Git is available, the original index is retained and staged content is
pinned at the ref named in `recovery-ref.txt`. Tracked, untracked, ignored and
binary working files survive. An index Git cannot snapshot blocks removal;
missing main checkouts still permit a file recovery. Failed recovery leaves the
original worktree in place. Branches are retained.
