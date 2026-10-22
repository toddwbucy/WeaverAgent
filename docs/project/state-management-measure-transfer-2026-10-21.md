# Measurement reservation transfer regression

Date: 2026-10-21. Issue #146, carried PR #215 review Q3.
Branch: `code/state-measure-transfer-pin`, stacked on draft PR #230.

## Scoped claim

Implements: Spec 06 MEASURE-DELTA and PLACE-CALLS, and the harness amendment's
placement wait, scoped to transferring a completed measurement after reservation
settlement. Author assessment: conforms for this ownership boundary. This change
adds a protective instrument to the existing implementation, not a placement grant.

The final PR #215 grade found that inserting a recoverable live check between
decode.finish_reserved and self.measure.take survived the existing suite. The
pre-settlement check was already pinned. The missing case permits both existing
checks, then advances Leave to its deadline at the next clock read. Successful,
typed-refused and cancelled exchanges must each return their exact retained result
once, retire the reservation and preserve the enclosing Decision bound. Leave must
still surface at the next recoverable boundary.

The fixture uses the real decode socket and recording context. It does not assert
only a clock-read count. The deliberate extra check returns Wait after consuming
the reservation, so the expected result transfer fails. A later consumer cannot
repair that lost ownership by retrying.

## Validation

Serial Olympus checks used /tmp/weaver-146-recall-target and --locked, after the
lock gate. The harness suite passed 1194 tests with 18 inherited ignored cases.
Strict all-target harness clippy, formatting and diff checks passed. The Q3
mutation compiled and failed the new regression. Exact source restoration passed
the focused rerun. Independent source review found no concrete defect.

Evidence and tested source hashes are in the shared bulk-store deposit
`weaver-testing/state-management-146-olympus-2026-10-21/measure-transfer/`.
These are author checks, not a Planner grade or production qualification. No
journal truth, boot authority, placement or generation is established by this test.
The full Issue #146 goal remains open and only the operator merges.

[Final PR #215 grade](https://github.com/toddwbucy/WeaverAgent/pull/215#pullrequestreview-5608104965)
