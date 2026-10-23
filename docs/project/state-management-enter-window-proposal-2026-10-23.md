# State management: original Enter window proposal

Date: 2026-10-23. Status: proposed; operator ruling, review and merge are pending.
Issue: #146. This document changes no code and authorizes no deployment.

The merged HARNESS-CLOCK rule already gives boot the remainder of admin's original
load-act budget. The required replacement Serving Enter shape has no field carrying
that budget. Passing a sampled remainder into a later constructor would grant the
intervening carry, queue and parsing time again. This proposal closes that
representation gap and names the start point and expiry behavior explicitly.

The proposal is based on main 45f8483393ce25af4ade3f0fa7bd96c1e8ed78f1. The owning
changes are [TYPES-ENTER-WINDOW][types], [ADMIN-ENTER-WINDOW][admin] and
[HARNESS-ENTER-WINDOW][harness]. Their prior merged clauses remain authority until
this proposal is elected, reviewed and merged. Existing Serving remains applicable
until the complete [foundation cutover][cutover]; a required future field is not an
additive deployment plan.

## Proposed decision

Require `enter_window: EnterWindow` in the replacement `EnterBinding::Serving`:

```text
EnterWindow { began_ns: DecimalU64, deadline_ns: DecimalU64 }
```

These are readings of the same Linux CLOCK_MONOTONIC domain. Admin samples `began_ns`
once after admitting the load request and root configuration, immediately before the
first load-specific preparation. The point precedes run-directory preparation and
invocation/run-lock acquisition, as well as inventory, artifact selection, channels,
child start and Enter carry. In the current dispatcher it precedes
`prepare_run_directory`; entry to `load()` or the send is too late for this proposed
boundary. Root argument/configuration admission remains outside the window.

Admin converts its positive root `load-bound-seconds` to nanoseconds with checked
multiplication, then checked-adds that budget to `began_ns`. An unrepresentable
interval is `ConfigInvalid` attributed to that root field before Enter. Neither a
bound declaration nor a caller chooses another budget. Retain the exact original pair
through preparation, encoding, descriptor carry, queueing, parsing, boot and the
answer. Admin also bounds its own send/receive work by that original deadline and
rejects a late Ready.

For original anchor A, budget B and deadline D = A + B, a send-time remainder
R = D - S cannot become T + R at receive time T: that would grant T - S again.
Every later observation uses D directly. The trace keeps its separately captured
origin and elapsed-time encoding; no new load/trace field or LoadedStateFacts/show
field is proposed.

The carrier has exactly two required canonical decimal-string members, with no
unknown or duplicate fields and `began_ns < deadline_ns`. Zero is a valid absolute
anchor. Diagnostic retains its separate election and rejects this Serving-only
member. There is no optional/default window, legacy Serving fallback, serialized
Rust Instant, wall-clock conversion or cross-host/time-namespace translation.
The entire lifecycle envelope still fits its existing byte bound.

## Proposed malformed and expired paths

| Input or observation | Proposed outcome |
| --- | --- |
| Missing, extra, duplicate, incorrectly encoded, equal or reversed carrier fields | Existing malformed-envelope channel fault, retaining and disposing actual received descriptors under that path. No codec repairs the values. |
| Structurally valid anchor later than the receiver's raw sample, taken after complete packet/descriptor receipt and before JSON decoding | Existing before-load `Malformed` refusal on the valid exchange. No state open, wait for the future anchor or fabricated load. |
| Well-formed window with current raw time at or past its deadline | Expired Enter, not malformed and not an invented component failure or `boot_failed`. No new ordinary boot work or Ready. No substitute lifecycle answer is invented for the expired exchange. |
| Expiry before load | Account for this Enter's actual owned descriptors under the existing unadmitted cleanup and retain the before-load lifecycle position for actual rollback. Do not create a load or trace refusal for a run that never stood. |
| Expiry after load | Retain the entered context and original bounds with all real owed author, read, pair, Open and process evidence for explicit rollback. Do not pretend that expiry completed accounting. |
| Admin observes no timely matching Enter answer | Existing unanswered-load `NoResidency`, unless an actual failure already determines the result, followed by existing rollback of what this invocation stood. This local result is not a fabricated peer answer. Containment failure keeps its existing precedence. |
| Actual component/clock failure | Preserve the existing first cause and failure path. Neither a clock error nor expiry creates proof of Release, Unload, process exit, LastWord or successful cleanup. |

The future-anchor test checks an observable violation; it cannot establish that
arbitrary supplied timestamps came from the same clock domain. The admin launch and
receiver must actually share that domain. An elapsed but well-formed window remains
structurally valid through queue and decode delay. Check time after decode and before
and after each admission/work unit, not only on socket readiness. At exact equality,
the window is expired.

An accepted explicit rollback Leave keeps its existing independent cleanup allowance
and cause. It does not extend Enter, authorize a late Ready or discard earlier owned
facts. The proposal supplies no new retry permission, lifecycle code, timeout grace,
refusal trace kind or successful serving/qualification claim.

## Alternatives for the operator

| Alternative | Consequence |
| --- | --- |
| Recommended original `began_ns` and `deadline_ns` | Carries the original interval and absolute cap; mirrors the elected SaveWindow clock pattern without reusing a save-specific type. |
| Original anchor plus positive duration | Same field count; each consumer must checked-add the identical deadline. Still needs the exact original anchor decision and expiry distinction. |
| Sample time plus remaining duration | Can preserve the deadline only by retaining the sample and checked addition. Omits the original anchor; clamping an expired remainder to zero would lose the true original cap. |
| Absolute deadline only | Smallest shape but omits the original interval evidence and floor validation of its positivity. Admin must retain the anchor privately. |
| Anchor at entry to `load()` | Excludes earlier run-directory preparation from this budget. This is a distinct start-point election, not equivalent to the recommendation. |

The proposed outward expiry handling retains the current unanswered-load and explicit
rollback outcomes. Selecting a new timeout refusal or repurposing a semantic failure
code would require paired owning wording and a separate explicit choice; neither is
part of this recommendation.

## Work and evidence after election

The concrete later act spans the floor carrier and all constructors, admin's original
anchor and deadline-preserving send/receive, harness receive/decode/dispatch, and the
dormant Enter facade's exact owner-bound constructor. Keep orchestration in worker's
`dev_loop`; a transport field does not move that responsibility into the harness.
The current construction census contains one production Enter literal in admin, nine
harness lifecycle unit literals, two service integration literals and one floor sample.
Diagnostic and the other users of admin's ordinary `recv_within` retain their own
contracts. External admin callers and deployment budgets remain a coupled-cutover
census duty.

Required instruments include closed/canonical wire cases and whole-envelope exact/one
byte over cases; real seqpacket/process fixtures with preparation/carry/queue/parse
and constructor delays; original descriptor and child custody; exact-deadline expiry
before first state/Open byte and after owner admission; late Ready and readable-at-cap
rejection; real bounded poll and EINTR under an outer watchdog; and unchanged
historical trace/Diagnostic fixtures. Protective changes must catch rebasing the
original deadline, defaulting a missing window, renewing after a partial result,
accepting late Ready, losing received descriptors or inventing completed rollback.
Arithmetic-only tests cannot establish process timing or cleanup.

No implementation is authorized by this document before operator election, review and
merge. The complete healthy-refusal/failed-Enter composition, process/LastWord custody,
production artifacts and measured limits remain separate implementation and
qualification duties. The unmerged positive-generation, correction-counting and read
preflight proposals are outside this change.

[types]: ../crates/state-management/weaver-types-Spec-amendment.md
[admin]: ../crates/state-management/weaver-admin-Spec-amendment.md
[harness]: ../crates/state-management/weaver-harness-Spec-amendment.md
[cutover]: ../crates/state-management/state-management-foundation-Spec.md#1-applicability-and-the-serving-cutover
