# Worker external execution composition

Date filed: 2026-10-24

Issue: #146. Parent: draft #323 at
`64d6460bc6f061ee5f566e6f30fdb262fdc5e0c2`.
Branch: `code/state-worker-external`, based on
`code/state-worker-return-delivery`.

This act joins explicit worker elections to the existing external pair ports.
The active serving path is unchanged. This is a dormant composition slice,
not production qualification or the complete worker lifecycle.

## Authority and behavior

Implements: state-management Spec 05 PAIR-STATES, PAIR-MATCH, TOOL-CLOCK's loaded
wire clock and S3 PAIR-LEAVE; harness-gate contract section 2's execution and
cancel protocol. Uses the existing RETURN-CARRY and PLACE-CALLS owners from #323.

| Surface | Author assessment |
| --- | --- |
| Pair readiness and election | Conforms in the exercised paths: exact selected CallId and external lane, both opening receipts, source order, no implicit dispatch. |
| Serial external execution | Conforms: the existing pair owner binds and sends the request, retains its actual outcome and files the terminal before the next request becomes ready. |
| Exchange identity | Conforms: a serving owner starts from its actual Raise ordinal, allocates distinct harness ordinals and retains each reservation across admission pressure. Exhaustion returns the unconsumed election. |
| PAIR-MATCH | Conforms for the exercised single-line replies: exact exchange and first outcome, raw rejected return before discard, no duplicate terminal. Multiline raw evidence has the trace-format conflict below. |
| TOOL-CLOCK | The execution uses the actual loaded clock. The Decision wait retires after the execution send and resumes for terminal filing. Schema prose and measured workload qualification remain owed. |
| S3 Stop | Conforms in exercised boundaries: withdraw an unsent packet or send one ToolCancel on the original exchange; preserve the actual answer and file every remaining not_dispatched/stop terminal before turn.closed. |
| S4/S5/S10 and broken-run teardown | Not implemented by this act. Custody stops intact on transport/trace failure; no synthetic success or completed cleanup is claimed. |

`FoundationInput::execute` and the developer-loop adapter elect a ready request.
They neither choose a tool nor change the source/request identity. Invalid phase,
foreign identity and inward lane return the original CallId. The unique serving
owner keeps the outbound ordinal across worker operations; gate-opened input
ordinals remain a separate identity set.

The worker owns admission, send, outcome and rejection stages. Existing pair
ports retain the original send receipt and actual gate response. A cancel owns
its envelope until staged and its exact send ticket thereafter. Repeated Stop
progress cannot send another cancel. Completed external owners stay with the
batch, including the cancel receipt, and later result placement retains the
whole previous round. Cancellation grants no retry or generation.

An inward election now carries its CallId in its stage. If another legitimate
bar removes readiness before its read starts, it returns to opening/terminal
accounting without inventing a read failure. This answers #322 review rows 4
and 7 while the external join advances row 2. The remaining #322/#323 carries
stay open; the complete parent map is in the #323 record.

## Instruments and measurement

The new cases use the actual worker startup, source, checker, recorder, pair,
management and decoder ports with process peers. The gate double checks the
original exchange, unchanged tool name/arguments and loaded 1234 ms clock. It
runs no shell command. The member and model are scripted doubles, not SQLite,
language artifacts or a production decoder.

The cases cover two external calls, identical commands, reversed outbound acts,
a mixed external/recall batch, a revoked inward election, no grant, each gate
outcome, a held terminal, five Stop boundaries, wrong/duplicate returns and the
actual developer-loop adapter. Successful results pass through #323's actual
placement and subsequent-generation path. No second execution appears before
the prior terminal filing, and Stop's close follows all owed terminals.

Three additional boundaries are explicit limits, not successful workflow claims:
gate death retains the active execution for failed-run teardown, multiline raw
rejection evidence remains retained after the recorder refuses it, and a queued
world input demonstrates the missing routing join described below.

Checks used the pinned toolchain, --locked, CARGO_INCREMENTAL=0,
CARGO_BUILD_JOBS=4 and private target
`/var/tmp/weaver-state-146-worker-external-target`.

| Check | Result |
| --- | --- |
| Lock before/after Cargo, formatting, strict harness clippy and diff checks | Passed |
| Full harness | 2019 passed, including 22 doctests; 259 ordinary ignored |
| Delegated process cases, serial | 241 passed: gate 46, startup 17, input 19, Observe 14, Stop 16, generation 20, emission 33, correction 19, inward 24, result delivery 12, external 21 |
| Protective mutations | Nine compiled and detected |
| Source restoration | All 670 crate/configuration files match the full gate manifest |
| Restored focused check | All 21 external process cases passed again |

The mutations reuse an exchange, change the loaded clock, omit a cancel, repeat
it, misaddress it, skip terminal filing, lose a revoked inward election,
normalize a rejected wire envelope and apply the pending-only bar to an
executing call. Each compiled and failed its intended instrument. A compilation
failure was never counted as protection. Source manifest:
`0fb7d153b3397348eb9deef858e4fd0d4b7576ad071a3ea8fc69d785e093a9c6`.

All source, first-run logs, complete gates and mutation patches/logs remain in
`/tmp/state-146-worker-external/`. Exact source archives precede both fixture
repairs. The approved bulk evidence deposit is identified by exact head and hash
in the PR and #146. The admin fixture helper was built; its eight privileged
admin-startup cases were not run. Eighteen inherited ignores remain; serial
passes do not close #313. The required code-review and verification-before-
completion skills were unavailable in the configured skill locations. The
self-check and actual gates here do not claim those skill invocations or an
independent Planner grade.


## Qualification blockers found by the join

1. The loaded wire clock is correct, but `context_place::install_place_engine`
   still obtains `tools::shell_schema()` from the legacy global renderer. Its
   description uses `engine::TOOL_CALL_CLOCK_MS`, not the loaded policy. Spec 05
   also requires the fixed schema prose in the certified template inventory.
   A grant-bearing process fixture exposed this path; its schema is checked as
   the current implementation, not graded as conformance. The next composition
   work must join loaded-clock rendering, certification and every consumer before
   activation. Removing the old global is coupled to legacy caller migration.
2. A valid JSON organ packet can contain literal newlines. Spec 05 requires its
   exact raw bytes in RejectedReturn.raw_envelope, while the recorder rejects
   embedded CR/LF to preserve one trace event per line. The multiline instrument
   observes TraceFailed/PayloadMalformed and proves the exact envelope stays
   with the pending rejection. Neither normalizing the packet nor changing the
   trace field is authorized here. A paired representation ruling and review are
   owed before claiming all PAIR-MATCH inputs can be recorded.
3. A next world input received before the active tool answer occupies the sole
   gate completion slot. The external primitive correctly leaves OtherTraffic
   to its caller, but the worker has no joined caller for that held frame yet.
   The queued-input case reproduces no terminal progress across sixteen steps,
   with the exact gate opening held and no fabricated result. Routing and its
   ownership/budget must be composed before a complete multi-client workflow is
   claimed. This act adds no overflow queue or new caller-input limit.

These findings remain on #146. Existing tool containment and unload/lower joins,
full selection/cuts, round limit and healthy final-answer/close policy, source
and author migration, #316's failed-run duties, #313's parallel races, certified
language artifacts/system prompt and measured deployment limits remain owed.
The operator alone merges and activates a qualified coupled cutover.

## After-action review

The planned join was an explicit external election plus Stop settlement. The
first process run passed only the no-grant case because its reused decoder double
assumed no schema. The fixture now verifies the actual granted schema. The next
run passed fourteen of seventeen: two pretty-printed rejection frames exposed
the trace-format conflict, and the unsent-send fixture used the other Stop helper
and never released its held socket. The ordinary rejection cases now preserve
single-line whitespace, the multiline refusal remains its own regression, and
the correct Stop helper releases the fixture only after the unsent boundary.
The first wrong-cancel mutation did not compile because it moved a non-Copy
Opener. Its failed-build log is retained separately; the corrected experiment
clones the opener and tests only the wrong ordinal. It is not counted until its
compiled instrument fails. No runtime validation was relaxed for these failures. Exact source archives and
all first-run logs are retained. The expanded twenty-case run then passed before
the queued-input characterization was added and the complete gates ran.

Sustain actual port joins and exact source retention before fixture repairs.
The next act must include unrelated gate traffic and the schema's loaded policy
in its initial sequence audit; a primitive's refusal/OtherTraffic result needs
a concrete worker owner as much as its successful result does.

## Review and corrected parent join

Planner review 5638144682 cleared b5f48f6a with zero fix and ten batch rows.
The Planner marked that head ready. This merge joins corrected #323 cc27588c;
its separate review record answers the inherited Decision/decode regression and
delivery guards. The original two parents remain in history. The join changes
no external execution policy and still needs review at its resulting head.

| Planner row | Disposition |
| --- | --- |
| 1 | Open, already characterized: an unrelated world frame can occupy the gate completion slot and hide both a tool answer and Stop cancellation answer. Production selection of execute remains barred until an owning routing path is joined. |
| 2 | Open: TOOL-CLOCK's offered schema still advertises the legacy constant; the loaded wire clock is correct. The schema assertion must change with the certified renderer, not be treated as conformance evidence. |
| 3 | Open: the held-answer-before-cancel ordering needs a separate held-answer instrument and a true wire-crossing case in the external control join. |
| 4 | Open: distinguish an actual non-dispatch terminal from an unexplained NotReady election; pin the inward/external disposition together. No new refusal election is made by this merge. |
| 5 | Open dormant error path: None gate after cancellation election can set the cancelled flag without barring an executing pair. Actual serving supplies its gate; the control join must eliminate or safely return the absent-gate road. |
| 6 | E2/E3/E4 remain unpinned: exact cancel ticket exchange/port and Stop before staging require independent corruption/interleaving instruments. Existing ordinary cancel cases prove neither isolated check. |
| 7 | Retaining completed external owners is intentional evidence custody, including actual cancel sends. A stage-payload cleanup must preserve that completed account rather than silently discard it. The separate empty-Vec unwrap remains a cleanup carry. |
| 8 | Open efficiency carry: repeated Stop polling re-bars the batch. A successful-bar state belongs in the external control composition with its refusal/retry tests. |
| 9 | Open test-boundary cleanup: use a test-only re-export when narrowing execution visibility. The current visibility is within worker_startup only. |
| 10 | Required self-check skills remain unavailable; no invocation is claimed. |

The review also reported two unrelated rollback timing failures under parallel
userns load. They are not credited as protective mutations. #313's parallel
fixture qualification remains open; the serial gates below do not resolve it.
No CodeRabbit review is requested.

Combined-source gates used the pinned --locked toolchain and private target
`/var/tmp/weaver-state-146-worker-external-target`. Lock before/after, formatting,
strict harness clippy and diff checks passed. The union passed **2026 harness
tests**, including 22 doctests, and **243 delegated cases serially**: gate 46,
startup 17, input 19, Observe 14, Stop 16, generation 21, emission 33,
correction 19, inward 24, return 13 and external 21. Its 261 ordinary ignores
are those 243 delegated cases and 18 inherited ones. Eight privileged admin
startup instruments remain unrun.

All 670 crate/configuration files match source manifest
`d3c7d631cde233e1dd580be667e0e4488b2ed8154b4d40b5bd9e409fbbe23ef4`. No new mutation round is claimed for this ancestry join; #323's
correction has its own 13 compiled controls, and the original #324 evidence
retains its separate nine. The full union rerun checks the shared task/result
road alongside all external cases. Evidence is in
`/tmp/state-146-worker-external-review/`; the exact-head bulk leaf and hash are
on #324 and #146. This is an author check, not the resulting head's regrade or
production qualification.
