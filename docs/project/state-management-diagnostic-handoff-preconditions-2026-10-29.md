# Diagnostic handoff preconditions

Version: 0.1, 2026-10-29.

This act answers #396 AJ1 and AJ5 and advances issue #146 section E and #390's
Diagnostic composition. Its base is draft #399 at
7abe4388520198932850b1a0239256e12bfceac3. No broad completion item closes.

## Authority and behavior

Implements: weaver-harness Spec sections 2.3 and 3, admin-harness contract
sections 2 and 4, and state-management-foundation-Spec FOUNDATION-TRANSITION.
Conforms on the exercised custody and handoff paths. These are private adapter
checks, with no new wire outcome, deadline or accepted directive. The existing
worker main still selects dev_loop::drive. Production qualification and the
coupled cutover remain open.

ReceivedEnter prepares its original peer and listener together. Its binding and
adoption checks run first, then the peer's and listener's side-effect-free checks,
then both descriptor mode changes. A held request, pending answer, latched failure
or held listener therefore refuses before either descriptor changes mode. The
private individual preparations retain their own checks. The external flow owner
can no longer prepare the two endpoints independently.

An operating-system failure during the descriptor changes can still leave one
mode changed. The existing failure path stops the admission owner, retains its
evidence and permits process failure exit. This act promises no atomic two-file
transaction and introduces no retry or mode rollback after that failure.

LifecyclePeer's constructors now state their existing ownership requirement:
the caller transfers the sole retained handle to that socket end. O_NONBLOCK is
part of the open file description and changes any duplicate as well. This is a
caller obligation, not a property proved by OwnedFd. The two non-test routes are
the authenticated accept4 result and the original OrganChannel returned after a
Diagnostic pre-load refusal. Neither route duplicates the end.

## Caller and instrument census

The only non-test caller of ReceivedEnter::prepare_diagnostic is
FoundationAdmission::serve_diagnostic. It retains the complete source on a failed
preparation, stops before Harness::listen or any Diagnostic load, and sends no
Ready. Success retains the raw bytes while moving the original peer, listener,
envelope and ancillary handles into the existing Diagnostic path. A pre-load refusal
returns that same peer and listener through their nonblocking constructors. The
dormant foundation admission driver is the public adapter's only non-test consumer.

The new owner test receives an original Diagnostic Enter with trace and state
handles, then accepts a second root-authenticated connection into the listener's
held slot. It calls the public serve_diagnostic adapter and checks the stopped
result, original peer and listener descriptors, exact Enter bytes, retained handles,
second connection's acceptance time and unread request, all nonblocking modes,
no answer or premature EOF, and final closure when the owners drop. It is an
ignored root instrument and must be selected explicitly. The current serial
admission driver does not itself accept a second connection after receiving Enter.
The test establishes the adapter's refusal behavior for that owned state, not a
newly reproduced scheduling path in the selected worker.

Before the fix, that test fails because the original peer becomes blocking.
The existing busy-peer sibling covers the other refusal direction. Two compiled
controls remove the combined preconditions and prepare peer first or listener
first. The new and existing tests respectively reject those changes. Source is
restored before the final gates. Existing pump tests cover held-request custody,
pending-send custody, successful blocking transfer and nonblocking return with a
queued request. The actual worker admission family exercises the composed
Diagnostic replay, refusal and return paths.

## Review and limits

Final gates pass 2,226 ordinary harness tests and doctests, with 925 ignored cases
and the known #331 pump case skipped. The ignored count includes this act's new
root instrument. Fourteen actual worker admission cases, that root instrument,
and six user-delegated containment cases pass separately, with no nested child
failure. Strict all-target harness lint, formatting, lock and three DocsMirror
cases pass. Both compiled ordering controls fail at their intended assertions.
All 815 Rust/configuration hashes agree between the restored controls and gates.
The entire ignored worker inventory was not repeated.

The code-review and verification-before-completion skills are unavailable in
this session. Manual author review covers the changed methods, every non-test
caller and the rework. Its checks are not a Planner or security grade. The PR body
records final gate counts, source hashes and the evidence deposit.

#396 AJ2-AJ4 and AJ6, #397's remaining findings, #399 AL6, and the earlier review
register remain open. #393 and #398 still require their operator elections and
merges before Diagnostic interruption and setup rollback can be implemented.
The production language artifacts and qualified deployment tuple remain separate.

The initial plan was to check both owners before changing either mode. The
reproduction confirmed the exact ordering failure, and the implementation keeps
that plan. Carry into subsequent handoff work: test each refusing owner while the
other is eligible, and inspect both retained endpoints before accepting a
successful refusal test. A one-sided test did not establish the combined rule.
