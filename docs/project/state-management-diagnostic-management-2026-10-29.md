# Diagnostic management refusal and handoff checks

Date: 2026-10-29. Author evidence for issue #146. Base: draft #392 at
695cfcd5724f772711078f3ca4c064bfeb2a7187. No deployment or qualification claim.

## Scope and authority

Implements: weaver-harness Spec sections 2.3, 3 and 7, admin-harness contract
sections 2 and 4, and weaver-types Spec section 3.1. Conforms on the exercised
Diagnostic refusal, connection and custody paths. FOUNDATION-TRANSITION remains
conforming: worker main selects dev_loop::drive. The inherited ReFeed Stop/Leave
behavior remains drifted under #390. Proposal #393 is not implementation authority.

This answers Planner review 5693929433 on #391: AF1, AF2, AE1 and AE3. The runtime
change affects only the Diagnostic management decoder and the nonblocking flag on
an original peer returned to admission. A decoded non-Admin, non-Open or non-directive
packet receives Malformed on its original exchange. An ordinary directive with
stray descriptors also receives Malformed, with the received descriptors closed.
A decoded legacy-shaped Enter receives OutOfOrder without entering or selecting
legacy behavior. Unreadable, duplicate-key and truncated packets retain the existing
channel-fault boundary. Repeated canonical Enter retains its OutOfOrder disposition.

Every refusal uses the original exchange and closes it. The accepted connection
remains available for the next directive. The shared receive helper owns ancillary
handles before interpreting the packet, including failure paths. No new wire kind,
lifecycle outcome, limit or retry is introduced.

## Instruments and consumers

The same-peer and new-peer process cases send a refused canonical Enter and then an
Observe on that exact connection during ReFeed and after reaching Idle. Each phase
also sends foreign-opener, continuation, answer, descriptor-bearing Observe and
legacy Enter packets. The parent checks the refusal identity, Close position and
reason, sees EOF on rejected descriptor endpoints, and successfully observes the
original run on the same connection after each refusal. The run then finishes
at-rest Stop and no-state Leave with a certified completed replay.

The transport cases exercise a real held received frame and a queued unsent answer.
Preparation refuses both without consuming custody. After settlement the original
socket transfers in blocking mode. A separate assertion test forbids consuming a
reserved sender without preparation. Returning the original connection to admission
sets O_NONBLOCK and retains a request already queued on it.

A stopped-handoff owner case answers AF2's admin-outcome question: the original
packet, pending answer and sink remain held, progress is Stopped and process_exit is
Failure. No Ready or other reply is invented. The fixture models process FD cleanup
by dropping this owner and verifies admin/sink EOF. The real entry retains the owner
through process exit. This is an invariant-failure path, not a recoverable admission
refusal. The silent terminal-reporting concern remains AE2 below.

The new private DiagnosticRequest refusal is consumed only by receive_entered.
Its existing consumers are the entered lifecycle loop, turn exchange and ReFeed
management, plus the Leave waiter. No consumer gains a new duty after receiving
None: it means the same packet was already refused. The real process cases exercise
the first and third consumers. Existing latch-before-refusal tests cover the Leave
path. The general receive helper and legacy codec are unchanged.

## Validation

The restored source passed 2,213 ordinary harness tests, with 920 inherited ignored
cases and known #331 skipped. That is 2,130 library, 8 worker, 43 integration
and 32 documentation cases. Ten admission process identities passed in an owned
root-delegated scope. Six legacy containment cases passed in an owned user scope,
which preserves their permission-denial fixtures. No process child failed. Lock,
formatting, strict all-target harness lint and three DocsMirror cases passed.
Cargo used --locked, CARGO_INCREMENTAL=0 and the private target
/tmp/weaver-state-146-serving-organ-loss-target. No Cargo or Git mutations overlapped.

Seven compiled faults were detected: dropping the entered peer after re-entry,
turning a decodable malformed request into a channel fault, ignoring stray handles,
transferring held input, transferring pending output, bypassing the consuming
transport assertion, and returning a blocking peer. The first script stopped at an
ambiguous source match before changing that mutation. The resumed script narrows the
match to Diagnostic preparation and retains the first three results. Both scripts,
the failed script log and all seven build/test logs are retained. All 812 source
and configuration hashes agree between restored controls and final gates.

Evidence is deposited below the operator-approved Issue #146 bulk-store directory.
The PR body names the exact committed snapshot, deposit and manifest digest.

## Remaining work

#391 AE2, AE4-AE8 remain: terminal error reporting, codec ownership and fault precision,
duplicate entry consolidation, parse efficiency and unavailable skills. The guarded
handoff checks are tested invariant checks. These cases do not show that an ordinary
admin invocation can create a busy handoff. #392 AG1-AG8 and the earlier review register
remain carried. No production language tuple, real model qualification, interruption
implementation or coupled cutover is supplied here. No broad #146 checkbox closes.

The code-review and verification-before-completion skills were unavailable in this
session. This is a manual author check, not either skill's invocation, a Planner grade,
a security disposition or production qualification. No CodeRabbit request was made.
