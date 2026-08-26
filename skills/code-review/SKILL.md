---
name: Code review
description: Review a diff for defects that matter, in a fixed order.
when_to_use: Reading a change before it merges, yours or somebody else's.
---
Read the diff in this order, and stop at the first pass that finds
something serious.

1. Correctness. What input makes this wrong? Look at boundaries: empty,
   one, many, concurrent, failing. Follow every early return.
2. Data loss. Does anything overwrite, delete or truncate? Does it read
   what was there first? An append that becomes a rewrite destroys the
   user's own content.
3. Security. Where does untrusted input enter, and what does it reach?
   Secrets do not belong in code, logs or error messages.
4. Failure behaviour. When a dependency is down, does the error say what
   to do about it? Are exceptions swallowed?
5. Tests. Does a test fail if the change is reverted? If not, the change
   is untested regardless of coverage numbers.
6. Clarity, last. Names, duplication, dead code.

Say what breaks and under which input. A review comment without a
failure scenario is a preference, and should be marked as one.
