# The save-point corpus

One corpus, two readers: `weaver-state`'s `SavePoint::parse` and `weaver-admin`'s
`save_points::judge` are hand-written readers of one format, `weaver-state-Spec`
section 3, and admin links no interior crate, so each crate's test reads every file
here and asserts the verdict `verdicts.txt` gives it (`sound` or `refuses`). A reader
that admits what the other refuses fails its own test, which is the drift this corpus
pins. `sound.save-point` is the format exactly as the member writes it, and
`run-at-bound` is sound at an edge; each other file is malformed one way, named by its
file name.
The corpus grows by the case that showed the readers apart: `nonce-wall-clock-overlong`
is a wall clock of digits past an unsigned 64-bit count of nanoseconds, which one reader
admitted and the other refused until both held it to what a clock can be.
The `run-*` and `schema-*` cases came with the operator's ruling of 2026-10-08 on #1:
a run of at most 128 printable ASCII bytes and a schema of exactly 64 lowercase hex,
each malformed file sound but for that one rule, so both readers refuse it for that
rule alone. `run-at-bound` is the edge they meet at, a run of exactly 128 bytes;
`run-control-1f` and `run-control-7f` are the control bytes either side of the printable
range, and `stamp-not-utf8` a stamp line holding a byte no UTF-8 reading admits.
`stamp-eight-members`, `stamp-member-missing`, `sequence-wrong-type` and
`turn-wrong-type` hold the stamp line to exactly the format's seven members, each of
its type. Every case's check line is the check over its own bytes, so each refuses
for its named rule and never for its check.
