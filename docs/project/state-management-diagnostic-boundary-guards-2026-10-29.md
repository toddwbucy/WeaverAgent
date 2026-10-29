# Diagnostic boundary guards and review instruments

Date: 2026-10-29. Author evidence for issue #146 E and #390.
Base: draft #395 at e21e91f3f949228ded1e707ede012a1cca0fb8ab.

## Scope and authority

Implements: weaver-harness Spec sections 2.3, 3, 4 and 7, admin-harness contract
sections 2 and 4, and weaver-types Spec section 4.2. Conforms on the exercised
refusal, record-latch and custody paths. FOUNDATION-TRANSITION remains conforming.
Worker main still selects dev_loop::drive. No new wire kind, refusal, limit or
production activation is introduced. #384, #387 and #393 remain unmerged proposals.

This act answers #394 AH1, AH2, AH5, AH6 and the remaining test items of AH9. It
also answers #395 AI2-AI5, which arrived during the author checks. AH3, AH4 and
AH7-AH8, AI1 and AI6, and the earlier review register remain carried.

The four receive_entered callers now supply their current record latch. The
entered service loop reads it through Harness::record_holds, both engine waits
read their recorder, and the Leave waiter supplies its existing latch snapshot.
The decoder refuses before receiving or answering on a failed record. Its test
uses a Diagnostic recorder latched by a submission outside its vocabulary, then
checks both a canonical reentry and an ordinary malformed payload. The healthy
arms retain exact exchange and refusal identity. The failed arms send nothing
and leave the original request unread. This is a local guard instrument, not a
claim that a new late-latch interleaving was reproduced in the serial idle loop.

The canonical reentry maps directly to the existing Refused outcome. Ordinary
shape failures, descriptor failures and both Enter spellings use one refusal
constructor. All four consumers retain their previous delivery and failure rules.
The foreign-opener policy and the difference from the legacy decoder are unchanged,
with AH3/AH4 still open.

A rejected peer handoff is checked before the listener changes mode. The stopped
owner test pins the retained listener's nonblocking flag beside original packet,
handle and pending-answer custody. This does not claim an atomic two-descriptor
flag transaction. A failed handoff remains terminal for its admission owner.

LifecyclePeer::from_owned establishes O_NONBLOCK for every entry, including the
from_channel adapter. The two non-test routes are the authenticated listener's
accept and the original peer returned after a Diagnostic pre-load refusal. The
new test starts with a blocking socket and queued input, checks the acquired mode,
then receives that exact request and returns Pending with no new traffic. Existing
handoff tests still check blocking mode after the deliberate Diagnostic transfer.
The separate StateSeam constructor from #395 is unchanged in behavior.

The two Diagnostic replay process cases now use separate ordinal ranges during
ReFeed and Idle, with a distinct Observe after each refusal. Ordinary and legacy
Enter requests carry one descriptor and two in separate cases. Every refusal
checks the complete expected envelope, closes transferred handles, and keeps the
same peer usable. The canonical reentry cases remain alongside them.

## Watchdog and validation

The state-write child fixture drains stdout and stderr concurrently while its
parent watches exit. A new child writes one mebibyte to each stream and then fails
with a sentinel assertion. Its parent must retain the assertion and failed status
without reporting a blocked write. The three Diagnostic child-exit waits now use
one test-only helper with the caller's original absolute deadline. Expiry kills
and reaps the child. This helper is not an agent runtime port or a new runtime bound.

The initial sandbox run refused Unix socket creation. The host run exposed a
fixture error: Diagnostic rewrites source identity, so a foreign-session fault does
not latch it. The corrected test uses an event outside Diagnostic vocabulary.
Both logs are retained. An initial single-descriptor control failed while decoding
an unexpected State answer. The process assertion was sharpened to compare the
whole refusal envelope before the final controls. These were instrument changes,
not changes to accepted management behavior.

Final gates pass 2,220 ordinary harness cases: 2,137 lib, eight worker, 43 integration
and 32 doctests. There are 921 inherited ignored cases and the known #331 pump test
is skipped. All eleven admission identities pass in a root-owned delegated scope.
The six legacy containment cases pass in a user-owned delegated scope, preserving
their permission-denial checks. Lock, formatting, strict all-target harness lint
and three DocsMirror cases pass. Seven compiled controls are caught. All 814
Rust/configuration hashes agree across the restored controls and final gates.
The PR body names the committed snapshot, evidence deposit and manifest digest.
The code-review and verification-before-completion skills are unavailable in this
session. The source, callers and evidence receive manual author checks, with no
claim of either skill's invocation or a Planner, security or qualification pass.

## Remaining composition

#390's wait census still stands. The pre-Ready identity answer and SPU Admit/Open
wait inside synchronous Enter. Composing rollback there must preserve the original
Enter exchange and every partially stood child, then complete the existing no-state
rollback without Ready. No new pre-Ready Stop/Observe behavior is elected here.
Entered replay asks and ReFeed interruption await #393's ruling and merge. AI1's
pre-bracket fault/recorder guard question, #120's record wait, production artifacts,
qualification and coupled cutover remain open. No broad checkbox closes.

The useful change to the author checks is to compare the complete expected refusal
before interpreting a response as that type, and to drain a watched child's pipes
while it can still write. Both rules are now executable in these fixtures.
