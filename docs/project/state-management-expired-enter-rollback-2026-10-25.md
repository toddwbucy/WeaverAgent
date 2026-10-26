# Expired Enter rollback composition

Date: 2026-10-25. Author working record for issue #146.

## Plan and authority

The previous slice, #343 at `9cb76cc2`, consumes failure during the
classifier-refusal rollback. This act connects an original Enter expiry to root's
explicit rollback and retains the actual startup account through disposal. It
does not turn elapsed Enter time into a component failure or renew boot work.

Authority: merged HARNESS-ENTER-WINDOW, HARNESS-OPEN, HARNESS-READ, HARNESS-CLASSIFY,
HARNESS-PLAN, HARNESS-RECORD-LIFETIME, HARNESS-HALT and HARNESS-CLOCK,
HARNESS-BOOT, Spec 05 BOOT-REPAIR, and weaver-harness-Spec section 6's rollback row.
The intended composition site is worker/dev_loop's unselected foundation adapter;
its connection to this owner is still owed.

| Boundary | Required evidence |
| --- | --- |
| Original expiry | The exact registered Enter window expires; no new boot request, Ready, refusal or halt is manufactured. |
| Custody | Every startup representation returns the same context and resource identities with its original records, requests, packets and proofs. Foreign joins return both inputs. |
| Rollback acceptance | An actual authenticated, completely received Leave with rollback true supplies the separate cleanup window. Observe and unrelated directives supply none. |
| Owed work | Preserve its own bound and disposition, settle or explicitly retain incomplete work before retirement, and never grant live boot authority. |
| Release | Release only actual children and endpoints; retain exact send/reply evidence and the load cause. No wind-down or save point. |
| Terminal | An actual load bracket owes its terminal ordering. Left requires the actual terminal and completed drain; expiry or incomplete accounting cannot fabricate success. |
| Before Load | Close unadmitted descriptors without a record, retain the lifecycle position, and answer actual rollback OutOfOrder. Admin's section 3 marker-write rule uses this answer to preserve the prior marker. |
| Failure during rollback | Preserve the first real failure and the earlier accepted Leave/halt cap. |

## Implemented local boundaries

The consuming handoff preserves the exact expired Enter, run/port identities,
startup evidence and actual resources through each retained startup representation.
It grants no boot continuation. Before Load it closes the actual unadmitted state
and trace descriptors, without a record, and keeps the lifecycle listener for
the actual OutOfOrder rollback answer. The trace writer now closes an empty,
unadmitted sink while its recorder remains owned; generic abandonment is unchanged.

After Load the owner waits on the actual root listener, with a separate original
read deadline for each accepted peer. Only the actual rollback Leave arms cleanup.
Observe, Stop and other directives do not grant that authority. A complete received
directive survives the peer's following EOF. The original Enter bytes and bound
remain retained, and no old boot request is pumped while waiting for root.

The local cleanup paths with settled startup accounts reach the actual terminal:

- Before semantic activation, retain the unused structural Open, retire its
  channel, seal the listener, record Unload through ordinary capacity, drain, and
  answer Left on the actual Leave exchange. No initial Grants reading stood, so
  the terminal reports Unreadable and omits SPU release.
- After completed classifier admission, use the accepted Leave to read current
  Grants, retain both actual readings, seal the listener, close and reap the
  classifier, then release the actual SPU and state before Unload. Left requires
  actual child reaps and the completed trace drain.
  A typed Grants refusal records Unreadable without becoming a component halt.
- An SPU child that was forked but never admitted receives its single Release;
  an exact NoResidency reply reports Unconfirmed. An admitted SPU reports
  Confirmed only for its actual matching Released after the complete Release
  send. The owner retains both the original Admit and Release exchanges,
  including any late Admit reply, without granting Open or new boot work.
  An unfinished Admit write is retained and closed, never resumed under Leave.

The unpublished checkpoint `95a6aacb` incorrectly placed the healthy classifier
reap after Unload. Harness Spec section 6 requires it before SPU or member
release. This order and the held-child circuit have been corrected locally.
The classifier is now held alive while the test proves that the member still
stands, no Unload exists and no Left is sent. The two old reap/order mutation
results below cannot establish conformance to this healthy release rule.

On an actual bad Release reply, the consuming failure handoff preserves the
first fault, original Leave, exact prior request/reply and Release owner. The
existing failed-run driver records the failure and terminal under its original
cap. It cannot acquire a second Release. A later failure after Recorded Unload
uses the shared no-record drain, retaining that terminal and appending nothing.
These exercised paths contain no gate. A caller cannot consume a progressing
healthy rollback through this failure interface.

A classifier still awaiting admission moves into a stopped account before the
cleanup Grants comparison. The original bootstrap, Enter, held completion and
framing remain retained; this transfer grants no admission or boot continuation.
The admission continuation below records the exercised boundaries.

The paired TRACE-STOP edit resolves its preactivation wording against the already
merged HARNESS-ENTER-WINDOW rollback rule. A recorded Load can be closed by actual
rollback without inventing a refusal or fault. This transition uses ordinary
capacity, retains the exact Unload under pressure, and spends no halt reservation.
It adds no event kind, field or timing rule. The original filed date stays fixed.

On the exercised healthy Accepted roads, the exact Leave cap disposes retained
startup work without a new
Unload or Left. A sent Open's incomplete account is retained before decode
retirement. Actual endpoints and child ownership survive as evidence; a kill
request is not called a reap. The owner samples the clock after inner work as
well as before it, so a drain that observes the cap cannot leave cleanup pending
behind a stale outer clock. A Recorded terminal sequence remains evidence even
when its drain expires.

## Verification at this local checkpoint

Evidence is under `/tmp/state-startup-rollback-20261025/`, with the source and
logs deposited at the approved #146 evidence root as
`state-expired-enter-local-20261025`. All Cargo
uses the pinned nightly, `--locked`, a default Rust test stack, four build jobs,
`CARGO_INCREMENTAL=0`, and private target
`/tmp/weaver-state-146-worker-session-target`. Cargo and Git mutations are serial.
The socket/process circuits run under disposable delegated root scopes; their
state member, classifier and model answers are declared doubles.

`classifier-checks/results.json` records passing lock gates, formatting, clippy
for both affected crates, the complete trace suite and the complete harness
suite with eight test threads and no filters. The harness library reports 1986
passed and 458 ignored; binary/integration targets add 43 passed and doctests 26.
The separate delegated startup run passes 101 scenarios, including 23 expired
startup cases. The trace targets pass 145 tests and 21 doctests. Ignored tests not
selected by that delegated run have not been qualified here.

The earlier default-thread harness run in `preflight-checks/` hit the existing
#331 pump fixture failure: CommitFailed at Sequence(8), OS error 11. Its isolated
rerun passed. A subsequent serial full-suite attempt was stopped after 225.98
seconds, with its partial log and exact process termination record preserved;
it is not a pass. Both subsequent complete eight-thread runs passed. No fix to
#331 is claimed.

The first blocked-drain circuit found a stale outer clock after an inner wait;
the correction is described above. The first classifier comparison run found
that the circuit fake could not model a wait without descriptors; its local
cleanup clock now advances the same fake timeline for that child wait. The
production clock and limits were unchanged. Initial failing logs are retained.

The preflight checkpoint detected 17 compiled protective changes. Its first
runner accidentally selected three similarly named tests for one control; the
intended assertion failed, but an additional blocked-sink case hung. The two
identified processes were stopped by pidfd, source bytes were verified restored,
and the final two controls passed with exact test selection. The log, timeout,
termination identities and rerun remain evidence. The updated source detected
all 22 compiled controls in `controls-classifier/results.json`, with positive
baselines first and byte-identical source restoration afterward. This runner
uses exact scenario names and adds five controls: current Grants must be read,
changed and refused readings must be distinguished, Left must wait for the
actual reap, and Unload must precede the child wait. The last two controls were
based on the incorrect healthy cleanup ordering described above; their logs
remain evidence of what was run, not evidence of conformance.
`classifier-restored-checks/results.json` records the subsequent clean rebuild,
trace suite, all 23 expired-startup circuits, formatting, locks and diff check.
Those complete suite/lint results belong to checkpoint `95a6aacb`, not to the
subsequent classifier ordering correction or SPU implementation.

Manual source review and invoked gates are author evidence. The required
code-review and verification-before-completion skills are not available in this
session, so neither skill invocation nor a Planner grade is claimed.

## SPU continuation evidence

The continuation adds actual Release/NoResidency, late and queued Admit,
wrong-exchange/position/arm and premature Release replies, silent Release at the
original Leave cap, actual SPU reap, failed release cleanup, Recorded-Unload
precedence, and unfinished classifier admission circuits. The current startup
run passes 114 cases, including 36 expired-startup cases. The model and classifier
remain declared protocol doubles; this is not GPU or production qualification.

The first expanded full startup run caught an incorrect fixture condition: its
new rollback Grants handling also matched the initial Grants read. The condition
now distinguishes the first read from the one actual rollback read, retaining
the prior request history. The failing log is preserved in
`spu-checks/startup-circuits.log`; the corrected run is
`spu-startup-all-2.log`.

The first SPU mutation run found that the seal test checked eventual unlink but
not its order. The held-classifier test now checks that the lifecycle pathname
has already been unlinked while that child remains alive and the member still
stands. The complete final control and gate results are recorded beside the
source snapshot. Older logs remain separate and do not substitute for the final
source's checks.

`spu-final-checks/results.json` records all ten final gates passing: first and
last lock gates, formatting, strict trace and harness clippy, the complete trace
and harness suites, the library build, all startup circuits, and diff check.
The harness library passes 1986 tests with 471 ignored; other targets pass 43
tests and 26 doctests. The trace suite passes 145 tests and 21 doctests. The
delegated startup run passes all 114 cases. Ignores outside that run remain
outside this qualification.

`controls-spu-final/results.json` records 15 compiled deliberate mutations,
each detected by its named exact test after all positive baselines passed.
They cover classifier admission and reap, the lifecycle seal, exact Release
send/exchange/position/answer, NoResidency, late and unsent Admit, the original
Leave, actual SPU reap, decode retirement, no Release retry, and the Recorded
terminal's precedence. Source restoration is verified before the final gates.

The checkpoint's archive, source hashes, controls and logs are preserved under
the approved evidence root at
`/bulk-store/weaver-testing/state-management-146-olympus-2026-10-22/state-expired-spu-local-20261025`.
Its manifest identifies every deposited file. This supersedes no earlier
deposit and publishes no PR.

## Open account continuation

The consuming expired-startup owner now disposes its original Open account before
the cleanup Grants read. A private permit binds the context, original expired
Enter, exact accepted live Leave and original decode port. It moves the request,
received raw answer, account bound and any pending placement event and ticket into
the rollback owner. This detached account has no continuation or journal grant.
Its evidence survives a later failed-exit handoff.

Before the first Open byte, cancellation uses the existing unsent withdrawal.
The same helper serves the earlier stopped-account path. Withdrawal touches no
wire and creates no missing-answer obligation. A sent Open whose account has
not completed by the original Enter bound is retained incomplete. Leave supplies
no new time to send, receive or record that boot account. The cleanup can still
compare actual Grants, seal, reap the classifier, release the SPU and member,
and record and drain the original load's Unload. An incomplete Open prevents
Left even after this cleanup finishes; the actual rollback peer is closed with
its directive retained. An unsent withdrawal can finish the ordinary Left path.

Four additional delegated circuits cover unanswered Open, unsent Open, pending
placement recording under actual writer pressure, and failed Release after Open
disposal. The pressure case compares the retained event and ticket and the raw
Opened response with their originals. The unsent child rejects every Open byte.
The old cap circuit now accepts the incomplete account in its new owner as well
as in the context before detachment.

The first expanded run found that old cap assertion and a fixture deadlock: a
single test thread could not drain the writer while close_after_drain waited.
Only its disposable systemd scope was terminated. The corrected pressure case
starts its trace-reading thread after all children have been forked and joins it
after the real terminal drain. Earlier failed logs remain in `open-checks/`.

The first full harness run passed 1985 library tests and failed the existing
`rollback_halt_composition_expired_leave` test: its immediate EOF assertion saw
EAGAIN. The isolated rerun passed. A concurrent test fork can retain a socket
until exec, so the fixture now allows a bounded two-second wait for EOF while
rejecting any answer immediately. This changes no runtime deadline or release
behavior. A compiled control that leaks the retired descriptor still fails this
assertion. `open-final-checks/harness-eight.log`, the isolated rerun and
`controls-open-eof/` retain the failure, correction and sensitivity evidence.

`open-restored-checks/results.json` records all eight final gates passing after
restoration: locks before and after, formatting, strict harness clippy, the full
harness suite, the library build, all startup circuits and diff check. The library
passes 1986 tests with 475 ignored; other targets add 43 tests and 26 doctests,
for 2055 ordinary passes. The separate delegated run passes 118 startup cases,
including 40 expired-startup cases. Other ignored families and the eight
privileged admin-startup cases are not qualified by this run.

`controls-open-final/results.json` records eleven compiled controls, each detected
after its exact positive baseline passed. They cover incomplete-account Left,
account disposal, pending event/ticket and raw answer retention, original account
bound, context/Enter/Leave/decode identity, custody through failed exit and the
forbidden first Open byte after cancellation. `controls-open-eof/results.json`
adds the descriptor-leak control. Restored source hashes are verified before the
final gates. The ten-control first pass remains separate in `controls-open/`.

Manual review covered both callers of the shared unsent withdrawal, the private
detach permit and its consuming startup caller, pending author custody, the
failed-exit owner and the terminal's Left election. No new wire field, event,
runtime limit or production selection is introduced. The required review skills
remain unavailable; these results are author evidence, not Planner or production
qualification. A fresh review/thread/comment audit of drafts #332 through #343
found no new findings and is retained in `open-checks/review-audit.json`.

This checkpoint's source archive and logs are preserved at the approved evidence
root under `state-expired-open-local-20261025`; its manifest identifies every
file. Earlier deposits remain separate. This is still unpublished work.

## Prompt and summary read continuation

An expired boot prompt or boot plan-summary read can now join the consuming
rollback. The same private startup-account permit binds its context, original
expired Enter and accepted live Leave. Its read purpose, original StateControl
bound, request identity, snapshot, bytes, recall ticket and prior summary/repair
evidence remain owned. It issues no query, snapshot Release, recall retry or boot
success after the original read bound. An earlier read timeout still follows the
existing first-failure path; Enter expiry does not reclassify it.

The coordinator separates custody from closure. It moves only that read's exact
operation, completion, channel and pending record out of the pump, retaining the
endpoint until the listener is sealed and the classifier child is actually reaped.
The SPU release then runs before member closure, followed by the actual Unload,
drain, SPU reap and Left. A pending read occupies the serial member exchange, so
there is no fresh Grants comparison: Unload reports Unreadable. The actual member
close outcome supplies Closed or Unconfirmed. A later real SPU release failure
retains the read through the existing failed-exit handoff.

The previous summary helper assumed that a pending state measurement had no
author ticket. The new writer-pressure circuits exposed that assumption: both
prompt and summary rollback were refused while their actual measurement waited.
The shared read permit now requires the held measurement's request and operation,
the exact pending record purpose/payload and its current author ticket. Recall
still requires its read-owned ticket. The coordinator distinguishes a pending
measurement from a Recorded measurement awaiting the final release probe.

Eight additional delegated circuits cover prompt and summary answers withheld,
both reads holding an acquired snapshot while Release is unanswered, classifier
exit held while the member remains open, a later invalid SPU Release, and prompt
and summary measurements held by the real writer queue. They compare retained
read evidence and assert no fresh member requests. The pressure cases inject
foreign record purpose/payload, reject them, restore the channel's actual
measurement, and compare the final retained event and ticket. Context, Enter,
Leave and StateControl-bound mismatches cannot acquire the disposal permit.

`read-checks/` retains the initial 46-case pass, the two pressure failures, their
48-case corrected pass, and a compile failure from two test-only namespace names.
The first final-gate attempt found an unnecessary unit expression; the corrected
strict lint passed. `read-restored-checks/results.json` records all eight gates:
both locks, fmt, strict harness clippy, full harness, library build, all startup
circuits and diff check. Ordinary harness results remain 1986 library tests plus
43 other tests and 26 doctests, or 2055 passed; 483 library cases are ignored in
that command. The delegated startup run separately passes 126 cases, including
48 expired-startup cases. These counts do not qualify other ignored families,
the privileged admin-startup cases or production artifacts.

All fourteen compiled controls in `controls-read/results.json` were detected
after their exact positive baselines. They cover classifier-before-member
closure, actual closure, original bound and request retention, Unreadable rather
than invented Grants, context/Enter/Leave/read-bound identity, retained pending
record and ticket, exact measurement ownership, ticket-bearing measurement
admission and custody through failed exit. Every changed source was restored.
`read-after-controls/` then rebuilt the ordinary binary and passed all seven
named positive cases again, formatting, both locks and diff check. All 703
source/config hashes match the tree that passed the complete gates; the full
suite's evidence therefore applies to the restored source, not a mutation.

Manual review traced both callers of the shared read permit, immediate versus
deferred summary closure, actual classifier reap, original read bounds, pending
record custody, failed exit and the member outcome in Unload. No wire shape,
event kind, runtime limit or production selection changes. The required review
skills remain unavailable, so this is author evidence, not a Planner grade. The
fresh audit in `read-checks/review-audit.json` found no reviewer submissions or
threads on drafts #332 through #343, only their existing draft-skip comments.

The immutable evidence destination is the approved root's
`state-expired-read-local-20261025` directory, with source archive, source hashes,
all retained logs, control patches and a manifest. This remains an unpublished
local checkpoint.

## Template check continuation

An incomplete template check now moves into the consuming rollback before that
owner retires Enter. The first real circuit corrected the earlier source-only
diagnosis: `complete_wait(Enter)` already refused to retire a bound whose template
owner could still certify. Cleanup treated that refusal as terminal. The new
transfer preserves that guard and moves the entire template transcript, original
Enter, classifier request/reply, pending event/ticket and any Recorded proof into
an account that grants no boot continuation or template certificate.

The exact context, expired Enter and accepted live Leave authorize this transfer.
The coordinator checks the template operation and request, source run, classifier
state and record ownership before moving anything. Foreign work is refused. A
received ComponentFailed remains a real classifier failure with its first cause;
the ordinary failed-exit path keeps that reply and bounds terminal accounting.
Neither this path nor healthy expiry sends another template check or records an
unrecorded result under a renewed Enter allowance.

Custody transfer does not close the endpoint. The cleanup Grants comparison and
lifecycle seal still precede actual classifier closure, and actual classifier
reap precedes SPU/member release. Leave expiry and a later failure explicitly
close the detached endpoint while retaining its semantic and framing evidence.
The retired port implements no classifier transport interface. Recorded proof
and completion evidence stay associated with their actual trace source.

Eleven added delegated cases cover a sent unanswered check, an unsent check, the
gap after the first accepted template, a result under real trace pressure, an
interrupted final proof release, a released completion, a held classifier child,
failed SPU Release, Leave expiry, ComponentFailed and foreign record purpose.
The released-completion fixture advances the actual coordinator after its
interrupted proof release and before the boot driver consumes that completion;
it constructs no proof or result. Its first version ran past that intended
boundary, and the initial failure and corrected run remain in `template-checks/`.
The test-only missing import and initial reproduction are retained there too.

`template-restored-checks/results.json` records all eight final gates passing:
both locks, formatting, strict harness clippy, the complete harness suite,
library build, all delegated startup cases and diff check. The ordinary suite
passes 1986 library tests, 43 other tests and 26 doctests, or 2055 total, with
494 library cases ignored. The delegated run passes 137 startup cases, including
59 expired-startup cases. Other ignored families, privileged admin-startup cases
and production artifacts remain outside this run.

All fourteen compiled controls in `controls-template/results.json` were detected
after their ten exact positive baselines passed. They cover transfer before Enter
retirement, the exact root permit, request/reply/ticket/proof/completion retention,
seal and reap ordering, actual endpoint closure at the cap, custody through failed
exit, real component failure, foreign record purpose and the forbidden first
check byte after expiry. The failure logs name the intended assertions. All 704
gated source/config hashes were restored before final review. That review then
corrected a release-permit comment and strengthened the component-failure case to
require its exact reply once on trace under the halt cap. The final eight gates
above cover both edits and record the final 704 source/config hashes.

Manual review traced the new detach permit and its sole caller, the coordinator's
unrecorded/Recorded/completed ownership states, generic read-only access after
transport retirement, the actual seal/reap order, cap disposal and failed exit.
No wire shape, event, limit, dependency or production selection changes. The
required review skills remain unavailable; no skill invocation, Planner grade or
production qualification is claimed. The audit in `template-checks/review-audit.json`
found no reviewer submissions or threads on drafts #332 through #343, only the
existing draft-skip comments.

The immutable source and evidence destination is the approved root's
`state-expired-template-local-20261025` directory. The manifest names every file;
earlier deposits remain separate. This is an unpublished local checkpoint.

## Initial classifier admission continuation

The actual admission circuit reproduced a rollback stall: the root owner accepted
Leave but excluded the retained initial-startup account from active cleanup.
Admission now moves out of the pump before Enter retires. The private transfer
requires the same context, expired Enter, accepted live Leave and clean pre-input
ownership. The coordinator checks its record identity, admission phase and scheme,
and refuses unrelated state, classifier, record or content obligations.

The held startup retains its original Bootstrap. The stopped admission retains
its exact Ready/refusal if one arrived, pending send or partial receive, original
framing cursor and admission state. Only the later lifecycle seal and release
phase close this detached endpoint. An actual child must reap before SPU/member
release and Unload. Before the classifier was forked, rollback has no classifier
reap to claim; it also omits SPU release if no SPU was forked.

An unsent Bootstrap sends no first byte after expiry. Neither a held Ready nor
cleanup grants templates or renewed admission. An actual unrecorded admission
refusal remains retained under its expired Enter: cleanup may close resources and
the load bracket, but that incomplete account permits no Left or new refusal
record. The original Leave cap closes retained endpoints without manufacturing a
terminal or successful answer. A later real clock failure preserves the first
failure and stopped admission through bounded failed-run cleanup.

Nine added delegated cases cover sent pending admission, before classifier fork,
unsent Bootstrap, held Ready, held refusal, a 65,510-byte partial Ready frame,
held classifier exit, Leave expiry and clock failure after transfer. The earlier
pending-admission cap case now exercises the transfer instead of asserting the
old stall. The before-send and held-completion fixtures step actual spawn and
coordinator transport operations at their existing intermediate boundaries; they
construct no admission result or lifecycle proof. Byte/cursor evidence and buffer
addresses are compared before and after transfer and endpoint retirement.

The first reproduction had a parent/child fixture-path mistake. Its corrected run
reached the actual four-second stall. The first owner run then exposed the old
fixture's assumption that an SPU already existed during classifier admission.
That assertion now follows actual fork ownership. Two missing-import builds and
one build rejected at the shared target's read-only lock are retained in
`admission-checks/`; all successful compilation uses the private target. No test
result is claimed from those failed commands.

Manual review follows the sole detach caller, actual admission consumption,
shared settled-work predicates, original Enter/Leave permits, endpoint release,
real reap, incomplete-account terminal permission, cap disposal and failed exit.
No wire shape, event, dependency, limit or production selection changes. The
required review skills remain unavailable; this is author evidence, not a Planner
grade or production qualification. The fresh `admission-checks/review-audit.json`
finds no reviews or threads on drafts #332 through #343, only draft-skip comments.

`admission-final-checks/results.json` records all eight final gates passing:
first/last lock checks, formatting, strict harness clippy, the full harness
suite, library build, all delegated startup cases and diff check. The ordinary
suite passes 1986 library tests, 43 other tests and 26 doctests (2055 total),
with 503 library cases ignored. The separate delegated run passes 146 startup
cases, including 68 expired-startup cases. Other ignored families, privileged
admin-startup cases and production artifacts remain outside this run.

All twelve compiled controls in `controls-admission/results.json` were detected
after their exact positive baselines passed. They remove initial cleanup,
rollback identity checking, original Enter retention, unsent/partial framing,
held Ready custody, refusal's incomplete-account guard, cap closure, real reap,
failed-exit custody or the no-send rule, and force a fabricated child requirement
before fork. The logs show the intended failure sites, including the terminal
constructor's independent reap assertion. All five controlled source files were
restored byte-for-byte. Final review then made the partial-frame fixture retry a
pending send and asserted that initial admission precedes SPU fork; the final
gates above cover those two test changes. The final manifest covers all 706
crate/configuration source files.

Source and logs are preserved at the approved evidence root's
`state-expired-admission-local-20261025` deposit, with a per-file manifest and
exact source archive. This remains an unpublished local checkpoint.

## Boot-pair repair continuation

Expired repair now transfers its complete original transaction before Enter
retires, provided no terminal filing is active. The stopped account retains the
checked Open proof, StartIdentity, initial and recheck results, every completed
terminal receipt, any prepared terminal and its original Decision bound. It
provides no RepairedBootPairs proof or boot continuation. The transfer requires
the same context, original Enter, accepted live Leave, boot head and registered
waits. Foreign or unfinished unrelated work cannot borrow this authority.

A pending initial or recheck read retains its original StateControl exchange,
snapshot, bytes, recall ticket and any pending measurement. The coordinator parks
the actual member endpoint until classifier reap. That exchange precludes a new
cleanup Grants query, so the actual terminal reports Unreadable. When no read is
active, the ordinary cleanup Grants comparison runs under Leave after the repair
account moves. Neither path sends another open_pairs query or begins a terminal
filing under renewed authority. Earlier independent Decision failure still uses
the existing failure path; the prepared-terminal case exercises a bound capped
by Enter itself.

The initial reproduction reached the actual pending open_pairs read and failed
the rollback admission assertion. Eleven added delegated cases cover that read,
the recheck, unstarted repair, completed untaken repair, one actually filed
terminal, an unstarted prepared terminal, a retained snapshot cursor, a pending
measurement under trace pressure, Leave expiry, held classifier exit and failed
SPU Release. The filed case checks the original CallId, actual StartIdentity,
abandoned_at_load fate, exact PairFile and Filed receipt, and the matching single
trace terminal. Cleanup cannot repeat that filing. Complete account evidence is
compared after healthy cleanup, the Leave cap and failed-exit handoff.

The fixture uses the normal repair driver after consuming the actual checked
Open prerequisite. Its member response contains a declared prior-run pair from
the existing corpus. Initial fixture corrections added the missing pump between
pending driver steps, recognized the parked member at the cap, fixed test-only
imports and serialized receipt comparisons, and left enough fake-clock time to
reach the prepared-terminal boundary. The failed builds and runs remain in
`boot-pairs-checks/`; none is a passing gate.

Manual review follows both detach callers, their exact permit and original wait
checks, actual read/record ownership, lifecycle seal and classifier reap before
member closure, cap disposal, and failed-exit retention. No wire shape, event,
dependency, production limit or serving selection changes. The required review
skills remain unavailable; these are author checks, not a Planner grade or
production qualification. `boot-pairs-checks/review-audit.json` finds no reviews
or threads on drafts #332 through #343, only the existing draft-skip comments.

All twelve compiled controls in `controls-boot-pairs/results.json` were detected
after nine exact positive baselines passed. They remove repair-owner admission,
the exact rollback permit, actual StartIdentity, Decision registration, checked
Open, initial read, recheck, completed receipts or the prepared terminal; close
the member before classifier reap; or discard the account at the cap or during
failed exit. The member-order control is caught by the actual endpoint's early
EOF, and failed-exit loss is caught first by the original read assertion. Every
failure log identifies its assertion. The four controlled source files were
restored byte-for-byte before final gates.

`boot-pairs-final-checks/results.json` records all eight final gates passing:
first and last lock checks, formatting, strict harness clippy, the complete
harness suite, library build, all delegated startup cases and diff check. The
ordinary suite passes 1986 library tests, 43 binary/integration tests and 26
doctests (2055 total), with 514 library cases ignored. Cargo reports the two
compile-fail doctests separately from its 24 other doctests; the earlier totals
in this record remain unchanged, with those categories corrected against the
preserved logs. The delegated run passes 157 startup cases, including 79 expired
startup cases. The final source manifest covers 707 crate/configuration files.
Other ignored families, privileged admin-startup cases and production artifacts
remain outside these runs.

The evidence destination is the approved root's
`state-expired-boot-pairs-local-20261025` directory, with an exact source archive
and per-file hashes. Earlier deposits remain immutable.

These repair cases exclude an active filing. The continuation below exercises
that author's custody, selected copy and original receipt exchange.

## In-flight boot filing continuation

An expired repair filing now transfers into stopped custody under the same
actual rollback permit. Its BootPairs owner, original Decision bound, CallId,
StartIdentity, recorded source, receipt reservation and record barrier must
match the retained repair. The original FilingTransaction, waiting author,
completed sequence and admission state move together. A pending trace record
keeps its prepared event and actual ticket; no successful filing or repaired
boot grant follows this transfer.

An unsubmitted author with no selected content or owed record can use ordinary
rollback Grants comparison after its original owner moves. A timely completed
receipt set can also move, but remains unreleased evidence in the stopped
account. The normal receipt-consumer method would mark it released; this path
uses a separate private move that preserves its original state.

An unfinished selected filing keeps its original ledger, receipt subscription,
PairCopy and retention charges. Before freezing the ledger, the coordinator
retains the last actual transport facts. This neither sends bytes nor releases
a reply. The original StateChannel leaves the pump while retaining every
request byte, send offset, partial reply, measurement and release proof. No
FilePair retry or Grants query follows on that occupied member seam. Cleanup
reports the Grants surface as Unreadable.

The original retention budget stays intact. Its failure freeze is not used as
cleanup permission. A private releasing-obligations predicate permits only the
validated parked inventory at classifier retirement and terminal close; other
admission and release prerequisites remain strict. While parked, the ordinary
drain validates that inventory and performs no content transfer or disposition.
Actual failure still freezes the budget and latches the first cause.

Cleanup seals the lifecycle listener and closes/reaps the actual classifier
before SPU release and member retirement. The live member channel becomes a
retained descriptor-free carrier; close failure remains visible to failed-run
accounting. Actual Unload and trace drain use the accepted Leave cap. An
unfinished filing forbids Left even after those resource steps finish. Leave
expiry and a failed Release retain the same stopped author, account, framing and
first failure, with no duplicate filing, Release or terminal.

Twelve added delegated cases cover the unsubmitted author, selected copy,
unsent FilePair, sent filing awaiting its real reply, a partially received reply,
actual completed receipts before caller collection, author/flow/measurement
records under trace pressure, held classifier exit, Leave expiry and wrong SPU
Release. The author-pressure fixture pauses the real recorder queue at its
pending boundary with the actual original event and allocated ticket. The
partial-reply fixture writes half of the encoded actual Filed response. Neither
constructs a Recorded proof. Identity probes reject a foreign filing owner,
CallId, bound, source reservation and record barrier.

The tests compare the retained author, source material, subscription and actual
receipt, original channel framing and buffer addresses, pending event and ticket,
and original retention charges. Final ledger transport facts are checked against
the actual parked exchange separately. Trace assertions distinguish a Recorded
terminal from a FilePair that has not yet sent; no duplicate terminal or request
is inferred from one count alone.

Initial failures and fixture corrections remain in `boot-filing-checks/`. They
include test-only visibility/import corrections, a stale expectation of Grants
on an occupied seam, the normal consumer's receipt-release flag, and a fixture
that had not consumed an already-sent FilePair before taking its request census.
The two initial cases and all twelve expanded cases passed before the protective
controls. No earlier failed build or run is counted as a passing gate.

All thirteen compiled controls were detected after nine exact positive baselines.
They check the original barrier, filing owner, bound and CallId; custody of the
waiting author, actual pending ticket, completed receipt and partial reply;
retained last transport facts; member close after classifier reap; no Left while
accounting is unfinished; and retention through the cap and failed-exit handoff.
The waiting-author and pending-ticket move controls are caught by the subsequent
admission/author guard; the early member-close control is caught by actual EOF.
All five controlled files were restored byte-for-byte.

The first final gate attempt stopped at strict lint: the new account enlarged
the expired-read enum, and a trace assertion allocated inside a comparison. The
private filing account is now boxed and the decimal-string assertion prepares
its expected value once. The controls precede those two lint corrections; all
five controlled files still match their restored hashes. The initial gate logs
remain preserved.

All eight final gates passed in `boot-filing-final-checks/results.json`: first
and last lock checks, formatting, strict harness clippy, the complete harness
suite, library build, all delegated startup cases and diff check. The ordinary
suite passes 1986 library tests, 43 binary/integration tests and 26 doctests
(2055 total), with 526 library cases ignored. The delegated run passes all 169
startup cases, including 91 expired-startup cases. The final manifest covers
709 crate/configuration files. Other ignored families, privileged admin-startup
cases and production artifacts remain outside these runs.

Evidence is prepared for deposit under the approved root as
`state-expired-boot-filing-local-20261025`, with the exact source archive and
per-file hashes. Earlier deposits remain immutable. Manual review follows the
new predicates through both detach callers, classifier/member retirement,
Unload, cap disposal and failed-exit custody. The required review skills remain
unavailable; these author checks are not a Planner grade or production
qualification. No wire shape, event, dependency, production limit or serving
selection changes.

## Summary account continuation

Rollback now retains a boot summary between reads: before its first read,
after a null or populated result has completed, or after the actual typed read
refusal. The move keeps the entire original transaction, repaired-boot proof,
Recorded read, cache allocation, disposition and accounting bound. Cache use is
revoked separately; the stopped account gains no successful boot grant. A
summary already extracted by the boot owner remains in that owner's handoff.

The new path requires the exact root rollback permit, expired registered Enter,
checked Open and journal, an idle read seam and no foreign work. It does not
send or repeat plan_current. The coordinator checks fatal foreign work before
the recoverable held-input check. An active read still uses the earlier parked
read path and cannot lend its occupied member seam to a Grants comparison.

Revoking the cache also blocks ordinary inspection. Cleanup's private Grants
entry therefore requires the retained summary account and a separate permit
for the actual accepted Leave after the original Enter retires. It bypasses
only the revoked-summary admission check; all other inspection guards and the
ordinary bounded request, measurement and completion path remain in force.
Foreign context, Enter or Leave identities allocate neither an inspection nor
a deadline. This entry grants no Shape inspection, ordinary query or generation.

Eight new delegated cases cover unstarted, completed null, populated, refused
and already-extracted summaries, delayed classifier exit, Leave expiry and a
wrong SPU Release. They retain the original account and cache through cleanup
and failed exit, check the exact refusal code/detail, reject ordinary inspection
and foreign cleanup permits, and count the actual plan_current recall without
inventing or repeating one. The targeted run includes three existing summary
read cases; all eleven pass.

The three initial reproductions failed at the missing summary handoff. The
first implementation run exposed the revoked cache's inspection guard, which
led to the private cleanup entry above. Test corrections used recall's actual
read wrapper and held the cap fixture before its next blocking Grants step.
Failed runs and intermediate build errors remain in `summary-account-checks/`;
none is counted as a passing gate.

All thirteen compiled protective changes were detected after six exact positive
baselines. They remove the rollback permit, Enter key, non-driving guard or
checked boot proof; change the original disposition, Recorded read or refusal;
drop cache revocation or its retained allocation; substitute the cleanup Enter
or Leave; or discard the account at the cap or failed-exit handoff. All four
controlled source files were restored byte-for-byte. The first final gate
attempt required rustfmt's test-import ordering; that correction changes no
controlled file or behavior, and the failed formatting log remains evidence.

All ten final gates passed in `summary-account-final-checks/results.json`: first
and last lock checks, formatting, strict trace and harness clippy, both complete
crate suites, the harness library build, all delegated startup cases and diff
check. The harness passes 1986 library tests, 43 binary/integration tests and 26
doctests (2055 total), with 534 library cases ignored. Trace passes 166 tests and
doctests. The delegated run passes 177 startup cases, including 99 expired-startup
cases. The final manifest covers 710 crate/configuration files. Other ignored
families, privileged admin-startup cases and production artifacts remain outside
these runs.

The approved evidence root's `state-expired-summary-account-local-20261025`
deposit carries the exact source archive, per-file hashes and accumulated
checkpoint logs. `summary-account-checks/manual-review.json` follows the new
entry's callers and original ownership through cleanup and failed exit. The
review audit finds #343 unchanged at `9cb76cc2`, still draft with no reviews or
threads. No review was requested. Required review skills remain unavailable;
these author checks are not a Planner grade or production qualification.

## Completion boundary

Branch `code/state-expired-enter-rollback` is draft work above #343, with no
serving selection. In particular, the handoff and cap disposal tests for later startup
positions do not establish cleanup of all pending accounts or gate ownership.
Those unsupported positions have no ordinary rollback-completion claim. This
owner must not be wired into a dispatcher as completed cleanup.

The same slice still owes full composition coverage of summary clock boundaries,
gate Lower and tool containment,
before-Load and gate-owned stopped cleanup, and the worker/dev_loop adapter. Successful
Ready and pre-adoption expiry need their own lifecycle continuations. The full
#146 scope additionally includes healthy Leave/save, capture cleanup, process
exit and dispatcher selection, production artifacts and prompt qualification,
measured limits, security disposition and the operator's coupled cutover.

The cases above establish only the named expired-startup rollback boundaries.
They do not establish gate teardown, the remaining worker adapter or production
qualification.
