# Admission expiry and the original connection

Version: 0.1, 2026-10-29.

This act answers #379 V3, V4 and V7, and #400 AN2, AN7 and AN8. It advances
issue #146 section E and is stacked on draft #400 at
a39b54590caa280265d6cc111785ad6be4a01778.

## Authority and disposition

Implements: weaver-harness Spec sections 2.3 and 3, admin-harness contract
sections 2 and 4, the harness amendment's HARNESS-ENTER-WINDOW, and foundation
FOUNDATION-TRANSITION. Conforms on the exercised unadmitted expiry, custody,
answer-delivery and same-connection continuation paths. No entered-run rollback
policy changes. The selected worker still serves dev_loop::drive.

An expired Enter before load closes its own unused trace and state handles and
returns its original peer to the sole receive owner. This applies at the first
window check, at adoption and after context-construction expiry. The lifecycle
position stays before Enter. No load, refusal record, replacement answer, Ready
or new Enter allowance is manufactured. Admin still owns when its connection
closes. The resumed reader can hear the next directive or EOF before another
connection is accepted.

An already owed refusal that expires before staging, during staging or under
send pressure has a completed non-delivery disposition. Its exact unsent send is
withdrawn first, then the peer resumes. The worker retains one delivery-loss notice
until its driver takes and reports it. While that notice is owed, no new input is
read and no second notice can replace it. Expiry before an answer was owed emits
no such notice. The original deadline remains the only send cap. Once the expired
exchange is settled, its deadline is not imposed on a new directive and no new
deadline is invented for the standing before-Enter receive.

| Position | Event | Disposition |
| --- | --- | --- |
| Unadmitted Enter, no answer owed | Original deadline reached | Close unused handles, resume its peer, no answer or delivery notice |
| Owed refusal, not staged | Original deadline reached | Report non-delivery once, resume its peer |
| Owed refusal, wholly unsent in the port | Original deadline reached | Withdraw only its current ticket, report non-delivery once, resume its peer |
| Owed refusal, withdrawal cannot be proved | Original deadline reached | Retain source and response, stop admission with the failure |
| Answer physically sent | Send clock fails | Existing terminal failure, retain its send evidence, no non-delivery claim |
| New complete input on the resumed peer | Successful receive and clock observation | Clear the prior input's detail before judging the new input |

The live ordinary port now exposes a ticket-checked withdrawal around its
existing wholly-unsent datagram operation. It rejects a foreign port and a prior
ticket, including an earlier send with the same exchange identifier. It retains
latched failure and cannot relabel a sent packet as withdrawn. The ordinary
expiry adapter is its only new non-test caller. The existing capture and closed
transport withdrawal paths are unchanged.

## Composition and instruments

FoundationAdmission is the only owner changed. Its normal completed answers and
expired dispositions share the same receive-resumption operation. ReceivedEnter
and ReceivedBeforeEnter still each own their exact received source. The existing
Response flag and the separate source options remain, so this is not a claim
that #379 V10's representation change is complete. The dormant worker's two
admission entries share await_enter, which already takes and reports one
delivery-loss notice before continuing.

The owner instruments cover exact-deadline receipt, expiry before adoption and
after context construction, expiry before staging and during serialization, and
an answer held behind a full socket. They inspect real handle EOF, the current
send ticket's withdrawal status, no late answer when capacity returns, one notice,
the original peer's read interest, an unloaded Observe, and a subsequent Enter
with its own window and no stale refusal detail. The original post-construction
clock failure still retains its source and grants only failed process exit.

The actual worker expiry case now sends Observe on the same connection after
its expired Enter, before the existing later connection and successful Serving
run. It checks exact exchange/position and unloaded state. A control removing
receive resumption must fail that composed case as well as the owner test.
The port instrument rejects foreign and obsolete tickets without consuming the
current answer, then proves the current withdrawal and refusal after a latched
transport failure. These are fixture and process composition checks, not a
production artifact qualification.

The initial reproduction found the stranded reader and missing delivery notices.
The first corrected focused gate exposed an old test that expected an expired
source to remain parked. That expectation now distinguishes settled expiry,
which resumes, from a clock failure, which retains the failed source. Both arms
still require actual trace/state handle closure with no fabricated load.

The Diagnostic handoff also rejects an already stopped admission owner before
checking its retained binding. A second call keeps the original stop detail and
source and sends nothing. Without this guard, the new test reaches a fresh
refusal even though the original owner had stopped. The live driver does not
retry today; this pins the owner boundary for later callers (#400 AN2).

Both Diagnostic handoff cleanup instruments now use the shared bounded EOF
helper for all retained peers and ancillary handles, under one three-second
fixture deadline. They reject bytes and require actual EOF while allowing a
concurrent fork's temporary descriptor copy to close (#400 AN8). This replaces
the one-shot cleanup assertions; it does not establish the strict fork custody
or Release ordering instruments still owed under #399 AM1/AM2. The earlier
handoff record's broken census wrap is corrected (#400 AN7).
Final author review applied the same helper to the new expiry handle-closure
check, avoiding another one-shot EOF assertion in this act.

The evidence retains two source cohorts. Six compiled expiry controls and the
first full gates belong to fc88626a7b3927decf1fa6a6c5465f388bb84d82 and its
archive. The additional stopped-handoff control and final full gates belong to
the final source archive. Each cohort has its own 815-file Rust/configuration
hash record; the two changed Rust files are the admission owner and its tests.
The actual worker's receive-resumption control fails at the fixture's 35-second
reply watchdog, not at a typed runtime refusal. All controls restore source.

## Review and remaining work

The PR body records final gates, compiled controls, source hashes and the verified
evidence deposit. The code-review and verification-before-completion skills are
unavailable in this session. Manual author review covers all changed callers and
the rework, with no Planner, security or production qualification claim.

#379 V1/V2/V5/V6/V8-V10 and the inherited review register remain. #400
AN1/AN3-AN6 remain: the two descriptor mode changes are not atomic, the Spec
wording and consolidation work are separate, and the explicitly root-only
instrument still asserts its prerequisite rather than silently passing without
running. #393/#398 still need operator elections and merges before the separate
Diagnostic interruption and setup rollback work. #399's AM1/AM2 custody
instruments and other batch rows remain owed. No broad #146 checkbox closes, and
no deployment or cutover occurs.

Carry into later admission changes: settlement must dispose of the send ticket
and re-establish the connection's reader together. A test that checks only that
an expired answer was not sent cannot establish that the next directive works.
