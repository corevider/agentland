---
name: Test-driven development
description: Drive a change from a failing test that describes the behaviour.
when_to_use: Adding behaviour, or fixing a defect that a test could have caught.
---
Write the test first, and watch it fail for the right reason.

1. Name the behaviour in the test's own name. `a_deleted_document_is_gone`
   tells a reader what the system promises; `test_delete_2` does not.
2. Run it and read the failure. A test that fails because of a typo or a
   missing import has not yet tested anything.
3. Write the smallest code that makes it pass.
4. Clean up with the test as a safety net, running it after each step.

Test behaviour through the interface a caller uses, not private helpers,
so a refactor does not break the suite. Cover the edge that worries you:
the empty input, the concurrent writer, the file that cannot be read.

For a defect, the failing test comes before the fix and reproduces the
report exactly. If you cannot write it, you do not yet understand the
defect.

Never delete or weaken a test to make a suite green. A failing test is
information; a deleted one is a lie.
