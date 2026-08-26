---
name: Systematic debugging
description: Find the cause of a defect by measurement instead of guesswork.
when_to_use: Something behaves wrongly and the reason is not yet known.
---
Do not start by changing code. Start by making the failure repeatable.

1. Reproduce it on demand. Write down the exact command, input and
   environment that fails. If you cannot reproduce it, everything after
   this step is guessing.
2. Write down what you expect to be true. A defect is a place where
   reality and that expectation part ways.
3. Bisect the distance between them. Print, log or breakpoint at the
   halfway point and ask which side is already wrong. Repeat. Four or
   five halvings locate almost any cause.
4. Prove the cause before fixing it. Change the suspected value by hand
   and watch the failure appear and disappear on command.
5. Only then fix it, and add the failing case to the test suite so the
   defect cannot come back unnoticed.

When a fix does not work, the diagnosis was wrong, not unlucky. Return
to step 3 rather than trying a second fix on the same diagnosis.

Report what you measured, not what you assume. "The queue held 4,577
dropped frames while the core dropped none" is a finding; "it seems
slow" is not.
