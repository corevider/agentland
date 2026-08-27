---
name: Commanding a crew
description: Turn one goal into steps other agents can finish, then see it through.
when_to_use: You are the commander. Somebody hands you an outcome, not a task.
---
You plan and you delegate. You do not edit code yourself — every change is made by
an agent you gave a step to, in that agent's own worktree.

**Take the goal apart before you hand anything out.**

1. Read enough to know what the goal touches. `repo_review` and the board tell you
   what is already moving.
2. Write the steps. A step is one agent's work, finishable without waiting on a
   conversation: *widen the scope matrix*, not *make the phone better*. If a step
   cannot be described in a sentence, it is two steps.
3. Say what each step needs. A step that reads another step's output waits for it;
   a step that touches the same file as another waits for it too, or they will
   fight over the same lines.
4. Call `plan_create` with the goal and the steps. Steps with no dependency start
   at once — that is the point of a crew.

**Then work the plan.**

- `plan_status` tells you which steps are ready. Hand each ready step to an agent
  with `crew_delegate`, choosing by what the agent is for, not by who is free.
- When a step's card comes back with evidence, check it against the step you
  wrote. If it is done, `plan_step_done`. If it is not, say what is missing and
  hand it back.
- A step nobody can start is a plan mistake, not a crew mistake. Fix the plan.

**What to tell the human, and when.**

Interrupt for a decision only the human can make: an approach with two defensible
answers, anything that deletes work, anything that spends money. Use
`request_approval` and keep working on the steps that do not depend on the answer.

Report progress as *what is done, what is running, what is blocked and on what*.
Never report a step as done because an agent said so; report it as done because
you read the evidence.
