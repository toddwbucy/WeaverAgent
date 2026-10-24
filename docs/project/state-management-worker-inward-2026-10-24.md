# Worker inward pair composition

Date filed: 2026-10-24

Branch: `code/state-worker-inward`, based on exact #321
`54da139722e8bb274053ed88384fcadb0244b54f`. Executor author record for #146;
not a Planner grade or production qualification.

## Result and authority

The dormant foundation worker can explicitly open its recorded outbound batch and
elect the next inward recall. It carries that request through the existing pure
constructor, bounded read/release owner and actual terminal filing. The completed
result stays in the existing private pair pool. No result placement, subsequent
decode or external tool execution follows automatically.

Implements: Spec 02 RECALL; Spec 05 PAIR-IDENTITY, PAIR-OPEN, PAIR-END,
PAIR-STATES, COMPLETED-PAIR and the S3 portion of PAIR-LEAVE; Spec 08 OUTBOUND's
caller boundary; internal amendment HARNESS-RECALL.

| Authority | Author assessment for this join |
| --- | --- |
| Spec 02 RECALL; HARNESS-RECALL | Conforms for exercised valid requests and typed read refusal: exact noun and null verb/condition filters; successful reads retain the whole encoding, release and recall record before terminal completion; refused reads retain the exact typed refusal. |
| Spec 05 PAIR-IDENTITY and PAIR-OPEN | Conforms: original CallIds and actual StatementRefs survive sorting and correction. Both actual receipts precede readiness, and readiness alone never starts a read. |
| Spec 05 PAIR-END and PAIR-STATES | Conforms for the inward and non-dispatch rows: source order, one actual terminal, per-call plan_break for omitted choices, and no selected suffix erased by a preceding omission. |
| Spec 05 COMPLETED-PAIR | Conforms at the worker boundary: terminal facts are exposed only from the existing private CompletedPair. Recorded without Filed is insufficient. |
| Spec 05 PAIR-LEAVE, S3 | Conforms in the tested Stop roads: pre-read election is withdrawn; an already admitted read finishes its original account; remaining requests close with Stop before the turn close. |
| Spec 08 OUTBOUND | Conforms as a consumer: retain the recorded decision and exact ordered acts; execution follows the conforming request order required by Spec 05. Sort itself opens nothing. |
| Full serving and lifecycle | Incomplete under #146. External dispatch, result placement, complete Leave and failed-run teardown are not supplied by this act. |

## Ownership and transitions

`begin_pairs` accepts only the actual Sorted completion. The worker moves its
EffectiveEmission into the existing pair coordinator and retains the decision's
source, ordered acts and filing completion. The coordinator's private generation
and receipt checks remain the admission authority. Busy returns the same effective
owner for continuation.

Each fully opened selected request yields PairReady. The loop's `recall(CallId)`
must name that exact ready inward request. Wrong IDs, external lanes and premature
calls return the supplied ID without starting a read. The developer-loop adapter
elects these operations through the same public worker boundary.

An omitted conforming request still opens and files. Its per-call dispatch bar
produces not_dispatched/plan_break with no gate exchange or fabricated return. The
new narrow bar uses the next ready pair, preserving a selected suffix. Existing
Stop/Unload cause dominance still applies until terminal author admission; an
already admitted cause remains immutable.

The pair owner handles requests in source order, even when the caller's outbound
acts use another order. It completes the current request's terminal filing before
opening the next. Readiness and progress are distinct: the driver pumps only when
the current transition requires external progress, not after a runnable pair
transition.

Recall delegates to the existing inward owner. The statement supplies the subject;
the pure helper selects no policy and owns no I/O. Decision's opening wait retires
at read admission, and the original bounded read/release window governs that read.
A typed refusal remains a completed inward pair with its exact code/detail JSON,
not an empty-history success. No gate tool schema or tool clock is introduced.

Stop before read admission removes that election and accounts every request.
During an admitted read, the same owner drains and releases it, retaining its
actual result before the terminal. The suffix is barred. All terminal commits
precede the outer Stop close; the instrumented prefix whose terminal was Filed
before Stop keeps its original fate. Stop may replace an unclaimed plan_break
terminal under Spec 05's existing precedence. Deadline expiry retains the failed read and pair owners and grants
no PairsComplete. This is a failed handoff, not completed failed-run teardown.

After every terminal commits, the original EffectiveEmission returns to the
worker. The private CompletedPairs remain in the session result pool, along with
their whole outcomes and filing proofs. `pair_outcome` is a borrowed read-only
view; it constructs no model-result or placement grant. The original world
exchange, corrected emissions and decoder accounts remain owned.

## Instruments and validation

Twenty-four delegated scenarios use actual worker, gate, classifier, decoder,
recorder and state-channel owners with scripted model/member responses. They cover:

- two distinct recalls, identical recalls, reversed outbound order, an empty
  batch, all omitted requests and an omitted prefix with a selected suffix;
- mixed external/inward requests, wrong ID/lane and repeated operation calls;
- held statement, opening, read, release and terminal filing boundaries;
- exact typed read refusal and expiry of the original read bound;
- Stop before opening, at readiness, after election, during read, during terminal
  filing and after completion, including a preserved plan_break prefix;
- the actual developer-loop adapter with real clock/sockets and a prior #321
  correction whose original CallId and corrected StatementRef reach pair records.

Checks and evidence:

Checks used the pinned toolchain, --locked, CARGO_INCREMENTAL=0,
CARGO_BUILD_JOBS=4, and private target
`/var/tmp/weaver-state-146-worker-inward-target`.

| Check | Result |
| --- | --- |
| Lock before/after Cargo, formatting, strict harness clippy and diff checks | Passed |
| Full harness | 2013 passed, including 22 doctests; 226 ordinary ignored |
| Delegated process cases, serial | 208 passed: gate 46, startup 17, input 19, Observe 14, Stop 16, generation 20, emission 33, correction 19, inward 24 |
| Protective mutations | Nine compiled and detected |
| Source restoration | All 667 crate/configuration files match the full gate manifest |
| Restored focused checks | All 24 inward process cases passed again |

The mutations enable an automatic read, ignore outbound choices, bar a selected
suffix, admit a read after Stop election, release before terminal Filed, change
an omission's cause, accept a foreign CallId, lose the returned EffectiveEmission,
and open the next pair before the prior terminal. Every mutant compiled and
failed its intended instrument; no build failure was counted as protection.

The full and delegated gates ran on the same source before mutation. Restoration
hashes and the focused rebuild establish that the committed code is that tested
source, not a surviving mutant. Source manifest:
`5774ede1e7cf931f20e5ce5e38ca2db81336c0ca8e5962d35e8904be5e91541f`.

Evidence is retained locally in `/tmp/state-146-worker-inward/` and deposited
under the already approved Issue #146 evidence root after exact-head publication.
The deposit leaf and hash are in the PR and tracker comment. The initial failed fixture run is preserved. Eighteen inherited harness ignores and
eight privileged admin-startup instruments remain outside these gates. Serial
passes do not close #313's parallel races.


The first run passed 19 of 20 scenarios. The held-statement fixture incorrectly
expected to reach Sorted while an earlier statement filing was still held. The
corrected instrument asserts the earlier outbound barrier and absence of pair/read
activity, then releases the real receipt. This was a fixture correction; no
runtime bypass was added. Initial logs are retained, without claiming an archived
exact source for that earlier failed fixture.

The author pass certifies the exercised custody and ordering joins. It does not
certify actual SQL history, model language quality, safe command side effects or a
complete production turn. The tests use real transports with scripted endpoints;
separate primitive and store instruments remain necessary.

## Remaining scope and after-action review

[#321 Planner review 5637888299][parent-review] passed at unchanged `54da1397`,
tree `bdcc08ed`, with 0 fix, 11 batch and 1 file, and marked the parent ready.
The Planner passed its gates, trace 160, harness 2012 plus one skipped case, and
all 184 prior delegated cases in parallel. Six of eleven protective mutations
bit. Its five surviving guards are not described here as mutation-proven. Codex's
separate code-review summary also completed at that head without findings; no
security qualification follows.

| Parent review row | Disposition |
| --- | --- |
| 1, five defensive guards | Carry: filed/issued instruction text identity at both bridges, matching refused-placement account, no-cut admission and cancel-time Decision retirement remain unpinned by the Planner's S3/S8/S9/S10/S11. The new corrected-source case proves the ordinary identity path, not these defensive guards. |
| 2, Decision retirement at correction cancellation | Carry: prove safe retirement while original requests owe accounting, or preserve the original bound and pin it. The inward worker's separate read-bound instruments do not answer this correction-owner question. |
| 3, other correction refusals | Explicitly open: missing selected instruction filing and decoder refusal can leave OperationRefused with a live repair and no worker-elected close. The parent closes fit/measurement refusal only; Stop recovery is not a complete refusal-close implementation. |
| 4, refused-close election after cancellation | Carry: reject a cancelled or non-Refused emission before electing close, preserving its completed Stop account. |
| 5, held retry bound and stale prefix | Carry: bound Prepare/Start Busy, re-prepare the exact current prefix where required, and keep stale-prefix refusal from closing ahead of original pairs. This act's pair-begin wait is separate and makes no claim to fix it. |
| 6, independently revoked placement | Carry: map Cancelled placement into the emission cancellation road even when cancellation arrived through another actual owner. |
| 7, close frame provenance | Carry: retain whether the staged frame came from place_close or correction refusal; do not re-infer the required completion handshake at send time. |
| 8, material error provenance | Carry: propagate the actual live/reduce_flow RunFailure instead of relabeling it WrongPhase. |
| 9, duplicate predicates and internal toggles | Carry: one committed-instruction predicate and one scoped repair-internal continuation wrapper. |
| 10, finished correction retention | Carry: retain required facts while retiring unnecessary completed lease registrations. No ownership is dropped merely because only the current attempt is driven. |
| 11, unavailable self-check skills | Same disclosed process carry below. |
| 12, defensive panic outside halt accounting | Filed under #316: replace unreachable outstanding-loan expect/assert exits with an ownership-preserving fault return under the original halt bound. No reachable production failure or closure is claimed. |

[parent-review]: https://github.com/toddwbucy/WeaverAgent/pull/321#pullrequestreview-5637888299

This planned act connects recorded sort to inward execution and actual terminal
proofs. The selected external lane remains an explicit PairReady owner awaiting
the next execution join. Results remain unplaced. The worker still owes tool
execution and interruption, RETURN-CARRY delivery acknowledgement and model-result
trace evidence, result placement, content/plan selection and cuts,
healthy final-answer policy, nested lifecycle judgments, Leave/SavePoint and full
healthy/failed teardown. Existing serving remains active until coupled cutover.

#313's parallel clock races and #316's complete failed-owner handoffs remain open.
The expired-read scenario retains ownership but does not close #316. All parent
review carries mapped in #321 and earlier records remain explicit; this act does
not grade or close their defensive mutation gaps, returned-author boundaries,
admission/validation questions or representation cleanup. #317's refusal-precedence
Spec election remains owed.

The fixture correction reinforces one practice: identify the earliest actual
owner that can block a composed path before asserting a later port's behavior.
Keep real-clock developer-loop cases alongside stepped socket instruments; both
are needed to distinguish runnable transitions from readiness waits.

Pinned language artifacts, measured limits, classifier/prompt qualification,
privileged/system gates and security review remain required. Unavailable
code-review and verification-before-completion skills remain disclosed; invocation
is not claimed. Independent review, all merges and deployment remain the
operator's decisions. No CodeRabbit review is requested by this act.
