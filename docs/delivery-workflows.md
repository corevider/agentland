# Project Git workflows

Under **Repositories → project settings → Git workflow**, enable a custom
workflow and select commit, push, PR creation and merge independently. The same
choices are available while opening a project. No selection implies another:

| Selection | Result |
| --- | --- |
| Commit only | Commit locally; nothing reaches a remote |
| Commit + push | Publish the branch without creating a PR |
| PR only | Open a PR only if the clean local branch already matches the remote |
| Merge only | Merge an existing PR only after all current-commit checks pass |
| All four | Commit, push, open the PR, then wait for independent checks before merge |

Projects without a custom workflow have automatic commit, push and PR creation
off. Merge inherits the board's **default auto-merge** switch. A custom workflow
replaces that default, including an explicitly disabled merge stage. Changing a
switch affects subsequent steps; it does not undo a completed commit or push.
Only a person can change these settings or the default merge switch.

Choose a trigger: the agent reporting finished, the card entering review, or
only your explicit **run selected steps** action. Agents call `workflow_run`
with their task ID when finished. It reports completed steps and the reason it
is waiting. A manual trigger rejects agent-initiated runs. A review trigger
waits until the card is in review or ready to merge.

The supervisor moves finished implementation cards to review even when a manual
Git step is pending. A background worker can deliver them once all sessions in
the checkout are stopped or report idle through native lifecycle information.
Active, blocked and unknown sessions prevent background delivery. Review agents
are notified only after an actual PR exists. Attempts are recorded on the card;
the same attempt does not repeat every poll or after restart. New completion
reports, commits or workflow configuration allow another attempt. Use **run
selected steps** to retry explicitly after resolving a failed prerequisite.

**Card details → Git workflow** shows the generated commit message. **commit**,
**push**, and **open PR** are separate manual actions and may be used when their
automatic counterparts are off. These are explicit actions, not changes to the
project's automatic settings. Merge remains subject to the existing card review
controls. The card's **run selected steps** button obeys the project switches.
PR creation never pushes implicitly, including the `pr_open` tool. Existing
open PRs are reused. A failed later step does not undo an earlier successful one.

## Commit messages

Default: `{type}: {summary} [{task_id}]`, for example:

```
fix: repair startup [task42]
```

Edit the template using `{type}`, `{summary}`, `{task_id}`, `{issue}` (`#17`),
`{issue_number}` (`17`), and `{project}`. Missing issues expand to empty text.
Ordered classification rules match an issue label or title prefix; the first
match wins. Defaults recognize bug/fix, enhancement/feat and documentation/docs.
Conventional task titles such as `refactor(core): simplify startup` also supply
the type and summary. Otherwise the configurable default type is used. The
template is data, not executable code; arbitrary scripts are not supported.

Managed commits and pushes reject AI attribution such as generated-with footers
and AI co-author trailers. Product names describing a real change remain valid.
Existing history is never rewritten. Outgoing task commits are validated before
push and PR creation. Squash merges use an explicit task-derived subject and
body rather than GitHub's automatically assembled commit text. Reviewer, tester,
security, CI, unresolved-thread and exact-head merge checks remain mandatory.

## Scope of enforcement

These rules apply to Agentland's managed Git endpoints, workflow worker and MCP
tools, with common instructions for every engine. A CLI with unrestricted shell
access can still run Git outside those tools: delivery settings are not an OS
sandbox or Git server policy. Use native CLI restrictions and repository branch
protections when hard enforcement beyond managed operations is required.
Running sessions receive new settings in their next brief; restart a session to
refresh its standing instructions immediately. Server checks use current values.
