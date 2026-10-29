# Diagnostic state writes and setup refusal

Date: 2026-10-29. Author evidence for issue #146 and the wait audit in #390.
Base: draft #394 at 02d87299cb1f3c854c39fa511046ef2b5eb93564.

## Scope and authority

Implements: weaver-harness Spec sections 6 and 6.1 (state asks, failed writes and
Diagnostic identity), weaver-diagnostic Spec sections 3 and 4 (no Enter events before
the replay bracket), and admin-harness contract sections 3 and 4 (Enter refusal and
rollback). Conforms on the exercised setup, write-pressure and rollback paths.
FOUNDATION-TRANSITION remains conforming. Worker main still selects dev_loop::drive.
No production language or model qualification, interruption policy or cutover is
provided. The proposed rules in #384, #387 and #393 remain unmerged proposals.

Diagnostic bypasses the serving tee, which previously supplied O_NONBLOCK on the
shared state socket description. Its opener used blocking write_all and StateSeam
assumed the tee had configured later writes. A peer that stopped reading could hold
one of those writes before the answer timer started. The constructor now acquires
nonblocking mode for every ask owner. Failure retires it without attempting an ask.
The Diagnostic opener goes through that owner before any byte is sent. A refused or
partial opener closes the sole end, grants no ask owner and follows the existing
DescriptorsUnusable Enter refusal. No new timeout or wire field is introduced.

The real worker test also found a refusal event before replay.opened on this failed
setup path. The internal AfterLoad variant includes Diagnostic runs even though
those runs have authored no load. Diagnostic Spec section 3 explicitly says Enter
records nothing, and section 4 requires the replay bracket. The shared Enter-refusal
site now authors only for a serving run with its load bracket. Diagnostic still
answers admin with the exact refusal and retains cleanup custody for rollback.
This changes no refusal record already admitted inside a replay.

## Instruments and protective controls

Two supervised child cases start with ordinary blocking Unix streams. One attempts
a large opener against a peer that does not read. It requires a nonempty proper
prefix followed by EOF, with no complete frame or returned owner. The other fills
the send buffer with per-call MSG_DONTWAIT, then attempts identity without changing
the descriptor's mode in the fixture. It requires a missed ask, the retired seam,
no retry and exactly the already queued bytes. A separate success case checks the
same stream's exact opener-before-identity ordering and the returned identity.
The parent watchdog kills and reaps a blocked child and fails the test.

The root-delegated worker fixture transfers a genuinely full, still-blocking state
end through canonical Diagnostic Enter. It requires DescriptorsUnusable on the
original exchange, no SPU or other organ launch, no state ask or opener bytes beyond
the controlled fill, and an empty diagnostic trace. A forced rollback on the same
connection must answer Left with forced=true, no_state=true and no save point, then
exit successfully. The first run caught the pre-bracket refusal above. Its failure
log is retained beside the corrected result.

Four compiled controls remove nonblocking construction, bypass the opener owner in
lifecycle, grant an owner after a partial opener, and restore the pre-bracket refusal.
The first must trip both parent watchdogs. The integration bypass must miss its
management reply watchdog. The other two must fail their exact output assertions.

Final gates pass 229 ordinary cases across state, engine, lifecycle, replay and
three integration suites. The six ignored lifecycle containment cases pass in their
user-owned delegated scope. All eleven admission process identities pass in a
root-owned delegated scope, including the new pressure/rollback case, with no
nested child failure. Lock, formatting, strict all-target harness lint and three
DocsMirror cases pass. Four compiled controls are caught, and all 813 restored
Rust/configuration hashes agree with the final gates.

Cargo runs used --locked, CARGO_INCREMENTAL=0 and the private target
/tmp/weaver-state-146-serving-organ-loss-target. The whole foundation harness suite
was not repeated for this slice. #394 has its own full-suite evidence, while this
act exercises every non-test consumer of StateSeam and the shared refusal helper.
The PR body names the committed snapshot, evidence deposit and manifest digest.

The #394 record's refusal-vocabulary citation is corrected to types Spec 4.2,
answering AH9's citation item. Its ordinal/descriptor test items and AH1-AH8 remain
carried, alongside the earlier review register. The StateSeam mode fix here is a
different owner from AH5's LifecyclePeer consolidation.

## Remaining Diagnostic wait census

| Phase | Current disposition and remaining work |
| --- | --- |
| State opener and ask writes | Nonblocking immediate-write rule now holds under Diagnostic too. Failed/partial sends grant no further ask |
| Pre-Ready identity answer | Existing 600-second wait polls only the state socket. Prompt rollback reception still needs composition |
| SPU Admit and Open before Ready | Existing synchronous receive sites do not poll management. Their rollback/containment handoff remains to be composed |
| Entered replay answer | Existing 600-second answer wait still blocks management. #393's implementation plan owns the entered replay wait |
| ReFeed | Observe works, Stop and Leave remain refused pending #393's reviewed and merged election |

This is a source census except for the write and rollback paths exercised above.
Admin's S1 table refuses other mutating commands while load holds the invocation
lock, and show reports InTransition through admin's own ladder. The pre-Ready audit
therefore does not invent a new normal Stop or Observe exchange. It must still prove
the load's rollback path under parked identity and SPU waits. #120's record-queue
wait limitation is unchanged. The broad #146 and #390 checkboxes remain open.

The code-review and verification-before-completion skills were unavailable. The
source and evidence were checked manually, without claiming either skill's invocation,
a Planner grade, a security disposition or production qualification. All PRs remain
drafts and the operator merges. No CodeRabbit review was requested.
