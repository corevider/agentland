# Engine parity

Chief, commander and worker roles can use Claude Code, Codex or Gemini. Choose
the engine in the crew panel. Stop a running agent before changing its engine;
the next start keeps its role and project but starts a new provider conversation.
Provider-specific model names, login and conversation IDs are cleared on a move.

## Shared instructions

Every crew start, resume and login handover builds a rule bundle outside the
checkout, in Agentland's data directory. It contains the house rules plus the
work directory's `AGENTS.md`, `CLAUDE.md`, `.claude/CLAUDE.md` and `GEMINI.md`.
Existing project files are never overwritten. Missing files are optional;
unreadable rule files prevent startup instead of silently losing instructions.
The bundle also asks the agent to read scoped rules in nested directories,
`.claude/rules` and referenced instruction files as applicable.

| Engine | Delivery | Agentland tools |
| --- | --- | --- |
| Claude Code | Appended system instruction file | MCP |
| Codex | Additional developer instruction referring to the bundle | MCP |
| Gemini | Bundle reference in every launch/resume brief | MCP |
| Other prompt-capable engines | Bundle reference in launch brief | Depends on adapter |
| Engines without prompt delivery | Crew startup refused | Not considered equivalent |

Codex uses `developer_instructions`, which adds session instructions, rather
than replacing its built-in instructions with `model_instructions_file`.
See the [official configuration reference](https://learn.chatgpt.com/docs/config-file/config-reference).
Instruction contents are kept off process command lines. Restart agents after
editing rules. A hand-opened CLI gets these instructions only when its authority
is explicitly set to `crew`.

Put conventions currently held only in a personal Claude configuration into
Agentland's house rules to share them across engines. Personal provider homes,
hooks, plugins and permission allowlists are not automatically imported.

## Unavailable engines

Usage-limit and rate-limit notices suggest a different installed engine with
Agentland tool support and at least one login allowed by the hiring and budget
policies. Codex is preferred when leaving Claude. Missing-engine errors during
chief/commander selection include the same suggestion. This does not switch
providers automatically or claim that an unverified login is authenticated.

## What is verified

Automated tests cover shared file contents, refresh on restart, preservation of
project files, unreadable-rule failure, delivery without a task, CLI permission
arguments, and alternative selection respecting availability and tool support.
Existing tests cover role permissions and provider switches clearing stale state.

These are integration contracts, not a guarantee of equal model quality or
identical native enforcement. Claude permission settings and hooks are not
translated into Codex/Gemini settings. Codex maps `default` and `acceptEdits` to
the same conservative approval mode. Gemini receives shared rules through its
brief, rather than an appended system instruction. Scoped rule compliance and
model behavior still require live evaluation.

For a live comparison, run the same fixture project and task on each engine as
chief, commander, implementer and reviewer. Verify rule recall after resume and
compaction, successful Agentland tool calls, delegation and review, project tests,
and refusal of actions outside the role's permissions. Keep task, rules, tool
access and acceptance criteria identical. Record engine version, model, pass/fail
and evidence; do not mark an engine equivalent solely because it launches.

## Shared workflow enforcement

[Workflow hardening](workflow-hardening.md) adds a common commit-bound merge
gate, isolated `repo_test` execution and durable, bounded repair feedback.
Agentland normalizes Claude/Gemini lifecycle hooks and Codex native task events;
this does not import, translate or bypass trust for a person's provider hooks.
The same PR and test requirements apply regardless of a role's selected engine.
