# Diagnostic replay after successful Enter

Date: 2026-10-29. Author evidence for #146 E and the completed-refusal handoff
tracked in #390. Base: draft #396 at 7a0d6aaf93262bdba015b259445a7c211e82a11f.

## Scope and authority

Implements: weaver-harness Spec sections 3 and 6.2, weaver-diagnostic Spec
sections 3.2 and 4, and admin-harness contract sections 3 and 4. Conforms on
the exercised completed-refusal and replay-admission paths. FOUNDATION-TRANSITION
remains conforming: worker main selects dev_loop::drive. No protocol, refusal,
limit or production activation changes. #390 remains open.

A completed refused Admit or Open retained a partial Run as Entered, so its
children could be released by rollback. The same retained SPU handle and an
unset replayed flag also passed serve_replay's old check. The actual worker
fixture reproduced an unwanted state replay ask after the original Admit refusal.
The source audit in #396 is therefore now backed by a process reproduction.

DiagnosticSeat now separately retains successful aggregate admission. It starts
false; dispatch_on sets it only when Enter succeeds, before answering Ready.
Both the canonical adapter and the legacy serve_loop call the same serve_replay,
which requires that fact and the unused one-time grant. A refusal leaves the
partial children owned without authoring ReplayOpened or asking for replay.
The existing answer-delivery policy is unchanged: this flag denotes aggregate
success, not an administrator's receipt acknowledgment. The three test-only seat
constructors describe already entered fixtures and set admission explicitly.

Consumer census: the one production DiagnosticSeat constructor is in enter;
the one transition to admitted is dispatch_on's successful Enter arm; the one
reader is serve_replay. Its callers are serve_loop and serve_diagnostic. The
public grant_seat returns no Diagnostic seat because that recorder has no working
structure. The process instruments use the canonical entry; this act does not
claim a separately scripted legacy admission run.

## Instruments and evidence

Three actual worker cases use a real forked SPU fixture. It refuses Admit,
refuses Open, or returns Opened with a foreign session. The controller requires
the exact original aggregate refusal, then sends same-peer forced rollback and
requires Left/no_state with no save point and successful worker exit. The member
answers identity and then must see EOF with no replay ask, content tee or save.
The SPU must receive Release with no further decode command, answer Released and
leave a parent-visible verdict. The drained Diagnostic sink must be empty. The
existing same-peer and new-peer successful replay cases require one certified
bracket and continue to exercise the positive admission arm.

An initial fixture rejected decode EOF during rollback, although release_spu
closes that end before Release. It now accepts EOF or no queued bytes and rejects
any command. The first mutation caught the bad state ask but left the faulty
worker waiting for its unanswered request. The final fixture shuts down that
state endpoint before asserting, retaining the original defect while allowing
rollback and the parent verdict to finish. A compile error in that fixture edit
and the lint correction of an unnecessary drop are retained with the other
development logs. Runtime behavior changed only at
the successful-admission guard.

Final gates pass 168 ordinary affected harness cases with 13 ignored cases,
all fourteen root-delegated admission cases and six user-delegated containment
cases. Lock, formatting, strict all-target harness lint and three DocsMirror
cases pass. Both compiled controls fail as intended: removing the admission
check fails all three refused-entry cases, and withholding successful admission
fails the positive replay case. All 814 source/configuration hashes agree across
the restored controls and final gates. The full foundation suite was not repeated.
The PR body identifies the committed archive and evidence manifest.
The source hashes cover all Rust/configuration inputs, and the controls restore
those inputs before final gates. Cargo runs unprivileged with the private target
/tmp/weaver-state-146-serving-organ-loss-target, CARGO_INCREMENTAL=0 and --locked.
Root is used only for compiled worker fixtures in delegated scopes. Legacy
containment cases use a user-owned scope so their permission failures remain real.

The code-review and verification-before-completion skills are unavailable; these
are manual author checks, not a Planner, security or production qualification.
The author pass reads the shared callers and successful/refused transition order.
The useful fixture rule is to close a request's endpoint before failing its
responder, so an intentional protocol violation cannot hide behind a long wait.

## Remaining composition and review

#390 still owes responsiveness while identity, Admit or Open is pending, and the
entered replay answer wait and ReFeed interruption. The completed-refusal handoff
fixed here is distinct from those waits. #393 remains a proposal awaiting operator
ruling and merge; its interrupted-ReFeed outcome is not implemented here. #120's
record wait, language artifacts, qualification and coupled cutover remain open.

Planner review 5695982334 grades parent #396 at zero fix. AJ1-AJ7 remain carried:
two-sided handoff preparation, defensive latch/Closed precision, other child
watchdogs, shared-description documentation, the watchdog result shape and the
unavailable skills. Its T2 survival confirms the parent already describes a local
defensive guard rather than a newly reachable idle-loop failure. The earlier
AH3/AH4/AH7, AI1 and the wider register remain open. Y1's new Release Closed cause
belongs to the rollback fixture work, not this replay-admission change. No broad
checkbox closes and no CodeRabbit review is requested.
