# Proposal: Diagnostic replay interruption

**Version:** 0.1, 2026-10-29.
**Parent:** WeaverTools Working Process

This proposal answers the protocol and lifecycle election in #390 and advances #146
section E. It is based on main f6f707799e2b0349180582032bc909a1e021a383. The dormant
canonical Diagnostic entry is draft #391, followed by the fixture correction #392.
No implementation, qualification or deployment is authorized by this draft. The
operator's ruling, Planner review and merge must precede the implementation act.

## Evidence and prior decision

The current replay owner hears Observe during ReFeed but refuses Stop and Leave.
Harness::serve_replay does not lend the held-Leave owner that serving uses. The SPU's
Session::refeed can retain a cancelled prefix and its terminator, but main.rs exits
without ReFed when cancellation is observed. A Cancel send alone cannot compose a
clean Stop because the harness would await a terminal the SPU deliberately withholds.

The loaded SPU test a_cancelled_refeed_faults_and_certifies_nothing explicitly
attributes that fault behavior to the operator's ruling of 2026-09-14. The source test
is evidence of that election, not a new benchmark run. No separate ruling document
was found in the repository audit. This proposal asks to replace that attributed
behavior explicitly. It is not an assertion that cancellation has never been decided.

The existing test queues Cancel behind ReFeed and requires the SPU to exit without an
answer. The existing session test separately checks a retained two-token prefix and
terminator. Its comment saying no seam-level test exists is stale. The implementation
must replace the loaded fault expectation and correct that comment together with the
new paired protocol, not merely remove the SPU's cancellation guard.

## Elections requested

| Election | Proposed rule and consequence |
| --- | --- |
| R1, replace fault on ordinary cancellation | RefeedInterrupted is a distinct terminal carrying original turn, consumed path count and resident/capacity. It carries no Generation. It precedes Cancel's AtRest and leaves the coherent prefix available for release. Engine and stream failures still fault |
| R2, bound interrupted accounting | One 60,000 ms worker interval starts at the accepted condition requiring the current turn to stop, before Cancel send. It covers decode settlement and the turn-close record, is capped by earlier applicable bounds and never renews. It is independent of admin's existing Stop receive deadline, whose timeout grants no continuation or successful answer |
| R3, certification and management | Stop against an open turn, or Leave against the entered pass, admits no new replay.closed. Earlier accepted closing records retain their original author and outcome. Interrupted passes otherwise remain unended, with their existing turn-close cause and truthful partial observations. At-rest Stop leaves the pass alone |

A graceful Leave finishes only the current recorded turn under its original drain
bound, then returns the seat without new certification or replay work. Forced Leave,
rollback, joining force or drain expiry can interrupt that turn. A new replay attempt
requires a fresh diagnostic load. This remains a gateless, no-save Diagnostic run.

The sixty-second worker duration is a proposed operational limit, chosen to bound the
accepted cancellation duty without adding a Diagnostic declaration field. It is not a
measured model limit or a promise to meet admin's remaining receive time. Admin's
existing wait begins at its directive, whereas this worker interval begins at its
accepted interruption. A caller can time out first. Its departure does not erase the
worker's retained duties. No SPU-side wall clock or asynchronous kernel cancellation
is introduced. If a kernel cannot settle, the worker takes the existing bounded
organ-failure cleanup path. Left still requires its own cleanup and record conditions.

## Alternatives

Keeping the attributed fault rule avoids a new decode answer but makes an ordinary
Stop terminate the SPU. The harness would still need explicit fault, turn-close,
requester and teardown composition, with no retained model after that stop. It would
not make the current refusal of Stop and Leave conforming by itself.

A partial ReFed(Generation) with Finish::Stopped could reuse the outer variant, but
would change the meaning of its emission and measurement. Every reader and comparator
would have to distinguish a supplied prefix from a completed supplied path. The
proposed distinct answer keeps that distinction in the type and carries no invented
completed-path data.

A new abandoned/interrupted diagnostic outcome could distinguish an operator's stop
from other unended records. That would add outcome vocabulary and require paired
reader changes. This proposal instead retains the current reader rule: every unclosed
bracket produces no downstream artifact, regardless of why it is unclosed. The Agent
copies of WeaverAnalysis PRD section 4 and Spec section 3 already state that rule.
No Analysis schema or compatibility obligation is introduced here.

## Paired ownership and implementation plan

| Owner | What this proposal changes |
| --- | --- |
| SPU PRD 13.14 | Permits a coherent interrupted account while forbidding partial certification |
| Harness-SPU decode contract 2-4, 8 | Gives interrupted ReFeed its terminal, correlation, ordering and failure guarantees |
| Types Spec 4.4 | Names RefeedInterrupted's exact fields and the shared worker duration constant |
| SPU Spec 4.6 | Defines prefix/terminator accounting, completion race, foreign-turn refusal and fatal-path separation |
| Harness Spec 6.2 DIAGNOSTIC-INTERRUPT | Owns retained peers and exchanges, Stop/Leave transitions, original intervals and process instruments |
| Harness-diagnostic contract 3, 5 and Diagnostic Spec 3.3 | Keeps interrupted brackets unended, earlier admitted closes owed and existing observations truthful |
| Diagnostic replay loop 3-4 | Stops the walk at management's boundary and forbids an in-run retry |

Both parties to each changed contract are in this repository and their owning Specs
change here. The public diagnostic shape and the Analysis reader's unclosed-bracket
rule do not change. This is a proposal for the authoritative Agent documents, not a
mirror edit in WeaverAnalysis. No production consumer can accept the new decode arm
until the paired implementation is built and reviewed.

Do not deploy these Spec changes ahead of their code. The planned implementing act is
code/state-diagnostic-interruption, stacked on the current implementation stack after
this proposal is approved, reviewed and merged. It must join the merged authority by
ancestry and bring all of the following together.

| Stage in that act | Required work and evidence |
| --- | --- |
| Floor and SPU | Add the typed terminal and codecs, retain the exact prefix/terminator, separate ordinary cancellation from fatal conditions, update the existing loaded test, and test zero/interior-prefix cancellation plus completion races with cuda,gguf on Olympus |
| Harness owner and loop | Retain management peer/opener/cause, lend Leave, drain the exact ReFeed and Cancel terminals, record the same turn once, keep earlier author custody, stop further work and run bounded cleanup |
| Entered wait composition | Hear management during the parked replay ask, between turns and under decode/trace pressure. Retire an at-rest Leave's pending ask through the existing channel-close/member cleanup path without accepting its late answer |
| Validation and publication | Strict gates on all affected crates, compiled protective controls, real process cases and a source-bound evidence deposit. Report skipped device/model cases explicitly and preserve the production selector |

The evidence matrix includes Stop, graceful/forced Leave, rollback, joining force,
drain expiry, foreign turn, zero consumed tokens, invalid counts, completion crossing
Cancel, missing or early AtRest, duplicate terminals, requester EOF, queued management,
silent SPU, record pressure and record failure. A no-active-decode boundary owes no
invented Cancel or AtRest. Earlier accepted replay.closed must remain its same author
under later Leave. Controls remove each custody, ordering, interval or certification
guard and must fail the relevant case. Child fixture passes are not counted as proof
of an unobserved parent verdict.

The implementation does not close all of #390 merely by handling ReFeed. The pre-Ready
Diagnostic identity wait still needs its separate admission audit and any ruling that
audit requires. The existing diagnostic recorder's blocking queue limitation (#120)
also remains explicit until a measured composition removes it. No scripted peer or
source audit qualifies the real model, language artifacts, complete replay readers or
production deployment.

## Author review and validation scope

This act changes documents only. Lock, the three DocsMirror tests, added-text editorial
checks and diff checks are its gates. Rust, device and process suites belong to the
implementing act. The code-review and verification-before-completion skills remain
unavailable in this session, so manual author inspection is not claimed as either
skill, a Planner grade or security review.

The source audit changed the initial premise: an existing process test cites a prior
operator election and already pins the fault behavior. Keep that election visible,
and replace its instrument only with the paired implementation after a new ruling.
Also preserve earlier accepted author work at cancellation boundaries rather than
turning a late management request into permission to drop a truthful queued record.
