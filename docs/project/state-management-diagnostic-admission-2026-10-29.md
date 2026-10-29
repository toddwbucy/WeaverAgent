# Diagnostic admission and replay composition

Version: 1.0. Date filed: 2026-10-29. Issue #146.

Branch code/state-diagnostic-admission builds on draft #389 at a22e7559.
The dormant worker entry now routes both canonical bindings. Diagnostic takes
its existing recorder and replay lifecycle. Serving takes the foundation
adopted-run dispatcher. Main still selects dev_loop::drive.

Implements: foundation Spec section 1 FOUNDATION-TRANSITION, types amendment
TYPES-ENTER and TYPES-ENTER-WINDOW, harness Spec sections 2.2, 2.3, 3 and 6,
admin-harness contract sections 2, 3, 4 and 6, and diagnostic-replay-loop
sections 2 and 3. These claims concern the handoff, observations, completed
replay and at-rest Leave. The missing replay interruption composition is #390.

## Ownership and behavior

A checked Diagnostic Enter moves the original listener, peer and ancillary
handles into the existing Diagnostic lifecycle. Its exact source envelope and
wire bytes remain in the calling frame through that lifecycle. The projection
moves no descriptor and changes no declared fact. It accepts only Diagnostic,
retains the election inside that binding on the paired wire, and supplies the
old in-memory payload shape only to the Diagnostic implementation.

A pre-load refusal returns the same listener and peer to admission. Queued
requests remain on that peer. A peer which departed after receipt follows the
same recovery, with the existing lifecycle's stderr report on failed delivery.
No child or recorder is supplied for a missing sink. A Serving source offered
to the Diagnostic adapter returns intact without a reply or projection.

The codec is selected by the checked binding handoff. Child organ channels
keep their existing format. New management connections inherit the Diagnostic
codec from the retained listener. Observe uses paired LoadFacts with
LoadedStateFacts::Diagnostic, including during ReFeed. An out-of-order Enter
is refused under its own exchange and its received descriptors close unused.
The failed-record check runs before that decoder can answer during Leave.

The four management receive sites are the idle lifecycle, generation control,
re-feed control and Leave's dial sweep. The listener accept and channel clone
preserve the selected codec. Pair creation and child adoption do not select it.
There is one recvmsg descriptor-custody implementation shared by both decoders.

The worker uses its existing process-wide lifetime contract and first-act
fd8/fd9 hygiene. A completed Diagnostic Leave releases its SPU, answers Left
with no_state and exits. It starts no gate or classifier and writes no serving
load, unload, tee content or save request.

## Author verification

The final serial harness package gate passes 2,209 cases: 2,126 library, eight
worker, 45 integration and 30 doctests. It retains 920 ignored entries and the
explicit known #331 skip. Nested child summaries are not counted again. All
ten admission cases pass in a root-owned delegated scope, and all six legacy
containment cases pass in their documented user-owned delegated scope.
Lock, formatting, strict harness all-target clippy, three DocsMirror cases and
diff checks pass. All 812 Rust/configuration hashes match before and after the
six final compiled controls and the final gates.

The pinned nightly-2026-02-13 uses --locked, CARGO_INCREMENTAL=0 and private
target /tmp/weaver-state-146-serving-organ-loss-target. Cargo runs unprivileged.
Logs, source hashes and the exact commands are retained under
/tmp/state-diagnostic-admission-20261029 and deposited under the authorized
issue #146 evidence root at publication.

The new process cases cover a successful Diagnostic replay on the original
management connection, another on a replacement connection, and a complete
Serving session through the two-binding dispatcher. Diagnostic includes a
missing-descriptor refusal followed by queued Observe, canonical Ready,
Active/Idle observations, rejected re-entry handles, at-rest Stop and no-state
Leave. SPU and member behavior are scripted. The process owns real sockets,
forks, containment and inherited custody descriptors.

Owner tests separately fix the departure after complete Enter receipt, prove
that Serving never transfers, and check a repeated Enter during Leave against
both record-latch states. These tests do not claim whole-process record
pressure or live-model qualification.

The controls remove the paired observation encoding, Observe during ReFeed,
retained refused peer, pre-load return, Serving guard and failed-record guard.
Each must compile and fail its named test. The original source is restored
between controls. The final gate runs against the restored source.

The requested code-review and verification-before-completion skills are not
available in this session. Manual source review and executed author gates do
not substitute a Planner or security grade.

## Remaining work and after-action record

Do not select or deploy foundation from this slice. #390 records the inherited
Stop/Leave refusal while ReFeed runs. The SPU currently faults a cancelled
ReFeed without a ReFed answer. Connecting a cancel alone would not establish
correct interruption, retained evidence or completion. The preload identity
and replay waits, Diagnostic backpressure and failure teardown still need
integrated qualification. Production language, prompts, limits and the coupled
cutover remain open. #377 supplies candidate documents, not qualified artifacts.

The intended act joins the separately elected Diagnostic lifecycle to canonical
admission. The first fixture left its state peer nonblocking after a no-traffic
assertion. Another selected the generic invalid-Enter fixture for the new
Serving case. Both fixture errors were corrected before their passing runs.
A deterministic lost-refusal test also showed that the existing lifecycle
already reports send failure and returns normally. A redundant recovery branch
was removed instead of maintaining a second report path. The source audit found
the failed-record guard needed to precede the new re-entry decoder and added a
protective test. Preserve these separate checks of transport ownership, record
authority and actual management behavior in later composition work.

The first added legacy containment invocation ran as root: five cases passed,
but root bypassed the sixth case's permission-denial fixture and no undelivered
fault was produced. All six pass unchanged in the user-owned delegated scope
their source documents. Both logs are retained. Match each instrument's account
and containment requirements instead of applying the worker fixture's root lane
to all delegated tests.
