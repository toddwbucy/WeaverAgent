# The save-point corpus

One corpus, two readers: `weaver-state`'s `SavePoint::parse` and `weaver-admin`'s
`save_points::judge` are hand-written readers of one format, `weaver-state-Spec`
section 3, and admin links no interior crate, so each crate's test reads every file
here and asserts the verdict `verdicts.txt` gives it (`sound` or `refuses`). A reader
that admits what the other refuses fails its own test, which is the drift this corpus
pins. `sound.save-point` is the format exactly as the member writes it; each other file
is malformed one way, named by its file name.
The corpus grows by the case that showed the readers apart: `nonce-wall-clock-overlong`
is a wall clock of digits past an unsigned 64-bit count of nanoseconds, which one reader
admitted and the other refused until both held it to what a clock can be.
The `run-*` and `schema-*` cases came with the operator's ruling of 2026-10-08 on #1:
a run of at most 128 printable ASCII bytes and a schema of exactly 64 lowercase hex,
each file sound but for that one rule, so both readers refuse it for that rule alone.
