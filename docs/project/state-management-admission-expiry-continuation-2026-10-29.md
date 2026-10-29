# Admission expiry and the original connection

Version: 0.1, 2026-10-29.

This act answers #379 V3, V4 and V7 and advances issue #146 section E. It is
stacked on draft #400 at a39b54590caa280265d6cc111785ad6be4a01778.

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

## Review and remaining work

The PR body records final gates, compiled controls, source hashes and the verified
evidence deposit. The code-review and verification-before-completion skills are
unavailable in this session. Manual author review covers all changed callers and
the rework, with no Planner, security or production qualification claim.

#379 V1/V2/V5/V6/V8-V10 and the inherited review register remain. #393/#398 still
need operator elections and merges before the separate Diagnostic interruption
and setup rollback work. #399's AM1/AM2 custody instruments and other batch rows
remain owed. No broad #146 checkbox closes, and no deployment or cutover occurs.

Carry into later admission changes: settlement must dispose of the send ticket
and re-establish the connection's reader together. A test that checks only that
an expired answer was not sent cannot establish that the next directive works.
