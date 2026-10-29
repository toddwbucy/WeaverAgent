# Residency fixtures through actual decode EOF

Date: 2026-10-29. Author evidence for #146 composition checks, #369 and Planner
Y1 from #383/#396. PR #399 remains stacked on exact #397 at
52d189af58757fae7dc37ef408a526fd6699eef1.

## Scope and authority

Implements: weaver-harness Spec section 3; weaver-harness-spu-contract sections
2 and 3; and, for the Diagnostic test observer, weaver-diagnostic Spec 3.2 and
4. Conforms on the exercised fixture paths. FOUNDATION-TRANSITION remains
unchanged. The new helper and its module declaration are test-only. This act
changes no runtime behavior, wire, refusal, production bound or selection.

Two independent fixture defects made valid teardown depend on scheduling.
The normal SPU returns from serve_decode to its residency loop, but the shared
fixture's default full mode exited on decode EOF. Rollback closes decode before
sending Release. A temporary 100 ms gap between those actions reproduced the
exact #396 Y1 error: entered rollback Release completion, Closed, packet peer
closed. The same worker case passed with retained fixture residency.

#369's captured child failure is a different ordering: Release is received
while a descriptor still keeps decode open, and a one-shot receive sees EAGAIN.
A temporary instrument retaining a duplicate decode endpoint for 150 ms
reproduced the exact named teardown test's Unconfirmed-versus-Confirmed failure.
The same instrumented case passed with the bounded EOF wait. This establishes
the failed ordering; it does not identify which process held an extra descriptor
in the original parallel run. Both temporary runtime edits were restored byte
for byte. Neither measurement establishes a production deadline failure.

## Correction and evidence

The generic peer records actual decode EOF once and retains residency for
Release or lifecycle EOF. A Release after that observation does not reread the
retired socket. A Release before EOF uses fixture_eof::await_eof_until with the
peer's original absolute watchdog. EAGAIN and EINTR wait; received bytes fail;
only actual EOF permits the peer to answer. No wait renews the deadline.

The watchdog is ten seconds from peer startup, including Admit, Open, decode,
Release and any scripted delay. Retention therefore requires the controlling
fixture to send Release or close lifecycle inside that same window. This is a
fixture coupling, not a production limit or evidence for longer loads.

Fixture residency now distinguishes BeforeAdmit, Refused and Admitted. The
refuse, cleanup-refuse and cleanup-release-refusal modes retain Refused after
their original typed refusal, so a later Release answers the existing scripted
NoResidency branch, never Released. Corrupted successful Admit envelopes still
represent an admitted fixture model; their bad wire remains a harness refusal.

The forked child and ordinary regression tests use the same peer loop, with
OrganChannel for lifecycle and DecodeChannel for decode. An optional FnOnce
notification witnesses either observed EOF or a Release waiting for EOF. The
Release-first test retains an actual duplicate descriptor until the peer has
observed EAGAIN, then closes it and requires the exact terminal reply. The
EOF-first test sends no Release until EOF was observed. A sibling ends through
lifecycle EOF. A separate case checks all three refusal modes.

The shared helper also rejects bytes arriving after an initial pending receive
and fails a live socket at its deadline. These checks strengthen the refused
Diagnostic child through EOF (AK6), rather than accepting one empty receive.
Tests retain their peer's internal watchdog; the scoped-thread join still relies
on that watchdog, so AL6's request for independent parent-controlled termination
remains a fixture-instrument carry. The notification itself cannot block a
panicking parent.

## Consumer and mode census

| Consumer or mode | Effect and exercised path |
| --- | --- |
| full, delay | Decode EOF now retains residency; residency boot/delayed-Admit cases and worker plan/flag cases exercise them. |
| refuse, cleanup-refuse, cleanup-release-refusal | No admitted residency survives their refusal. The new refused-admission test checks each mode through EOF and Release; typed-refusal and teardown-refusal cases retain their original assertions. |
| wrong, position, opener, arm | Successful fixture admission with deliberately corrupted reply evidence. Residency foreign-exchange/position/opener and wrong-answer-arm cases reject that evidence. EOF now retains the child for cleanup; this gives the harness no valid Admit proof. |
| early | The pre-send answer stays invalid. The answer-before-send case rejects it; a later real Admit alone changes the fixture's residency state. |
| expiry, stop | Existing expiry and Stop cases retain their actual reply/send assertions. If decode later closes, residency stays available under the fixture watchdog. |
| eof | Explicit pre-loop child exit is unchanged and still grants no boot. |
| cleanup-ready, failed-startup, expired-* | These modes already retained residency on decode EOF. Startup/expired Release cases preserve wrong-ordinal, wrong-arm, wrong-position, early, silent and held-child outcomes. |
| generation session helpers | After Open, the save/organ-loss modes that return to the generic loop retain their prior EOF behavior. Other generation branches still return after spu_after_open, or run their explicit exit release_peer before returning. The generic-loop change does not make those non-save branches newly answer Release. |
| Failed-run release_peer | Replaces its one-shot EOF assertion with the same helper under its existing ten-second deadline; worker exit/halt cases exercise it. |
| Existing failed-enter review helper | Delegates its bounded EOF observation to the shared helper; existing process custody and timeout cases remain. |
| Invalid STE100 bootstrap | Its rejected local socket must close without bytes. The helper waits through incidental descriptor retention and still rejects any bytes. |
| Refused Diagnostic child | Reads decode through actual EOF before its scripted release answer; all three refused/invalid Enter process cases exercise this observer. |

The early-answer branch uses the passed lifecycle descriptor; the fork still
maps lifecycle to fd3 and decode to fd4. A source sweep found the three one-shot
recv-equals-EOF assertions above and the separate Diagnostic EAGAIN acceptance.
The wait helper is shared across that class rather than adding mode exceptions.

## Verification and review

Five compiled controls cover exit on decode EOF, the one-shot Release
assertion, cleanup-refuse marked admitted, discarded bytes after a pending
receive, and the omitted EOF deadline guard. The first four fail their named
assertions; the missing deadline is caught by the control driver's three-second
process watchdog. Their exact mutations and logs are in the evidence deposit.

The final full harness package passes 2,226 tests: 2,143 library, eight worker,
43 integration and 32 doctests. There are 924 inherited ignored cases; the known
independent #331 pending-author pump flake is explicitly skipped. All 296
selected actual worker process cases pass: admission/Diagnostic, plan/flags,
startup, exit and halt paths. The complete 828-case worker inventory was not
repeated. Lock, format, strict all-target harness lint, three DocsMirror cases,
ASCII and whitespace checks pass. All 815 Rust/configuration hashes match the
restored controls, gates and committed archive.
Cargo is unprivileged, with the pinned toolchain, --locked, CARGO_INCREMENTAL=0
and private target /tmp/weaver-state-146-serving-organ-loss-target. Root is used
only for compiled process fixtures inside owned delegated scopes.

Planner review 5696929528 grades the earlier 9a0d21c3 head at zero fix and closes
Y1, including the related #392 flake. The expanded source answers AL1-AL5 and
AL7, and AK6. AL2's non-save-generation claim is declined for the control-flow
reason in the census; its missing mode census is supplied. AL6 and unavailable
review skills (AL8) remain explicit. The latest head needs its own Planner grade.

The deposits preserve failing baseline runs, helper compile errors, the first
plan-family invocation's missing admin-test-binary setup failure, the test-enum
naming lint, the caught
controls and restored-source gates. The verified deposit and archive digests
are in the PR body. The code-review and verification-before-completion skills
were unavailable; these are manual author checks, not production qualification.

## Remaining work

#369's reproduced assertion defect is corrected subject to review and merge.
Other timing issues, pending Diagnostic/plan Spec elections, inherited runtime
review carries, language artifacts, measured qualification and coupled cutover
remain open. Neither this fixture correction nor #377's candidate language
documents complete #146 or select the foundation runtime.
