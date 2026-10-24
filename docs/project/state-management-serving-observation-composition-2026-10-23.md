# State management: serving observation composition

Date: 2026-10-23. Author check for Issue #146.
Branch: `code/state-serving-observation-composition`.
Parent: #304, `56c0cb370c919b810713ca3ac9d4adea1fdc5396`.

## Completed connection

A root management connection retained by the input owner can now answer Observe
through the shared pump and return the exact suspended input. The paired admin
client consumes the replacement LoadFacts representation. A separate admin process
and the actual worker/dev_loop adapter exercise this join over the real socket,
after the complete worker startup circuit.

The replacement observation has LoadFacts.state, with externally tagged Serving
and Diagnostic facts. Serving requires sqlite and a present state member. The
codec rejects mixed legacy state_election, missing loaded facts, duplicate keys,
invalid observation position/opener, and oversized whole escaped envelopes.
Legacy serving remains selected until the coupled cutover; no parser accepts both
serving spellings.

Successful startup retains observation facts from the same original Enter and
runtime inputs that authored load: loaded declaration B, exact scheme bytes,
root-judged checker executable digest, state configuration and start, actual
member presence and store, model/elections, composer, and optional lifecycle
bounds. Observation reads no mutable declaration file. The checker executable
identity remains distinct from the scheme and model artifact identities.

FoundationManagement owns the accepted root connection, input owner, retained
request and answer, and sealed send receipt. It uses the existing pump, with no
second socket reader or new timeout. Observe reports Idle before the recorded
turn open and Active while that recorded turn remains open, authors nothing, and
leaves input identity, token, source and deadline intact. Backpressure retains
custody. Only the whole actual response on the original exchange and peer grants
Answered. A departed or malformed management connection grants no successful
answer. Other source failures stop the input and retain the whole owner; a failed
post-send clock observation remains a trace failure.

Valid Leave, Stop and SavePoint requests remain DirectivePending with their
original message, raw transport evidence and descriptors. Acceptance alone grants
no cancellation or lifecycle work. Their handlers are still owed. Input advancement
also observes management while an accepted or completed source remains in its
input owner. The worker observation adapter returns a premature call's input
unchanged instead of dropping it through a panic.

The admin client uses the existing authenticated Coordination channel, exact
exchange correlation and bounded receive loop. It rejects truncation, ancillary
response handles and invalid replacement shapes, and preserves typed lifecycle
refusals. Ordinary directive writes now explicitly check whole-send length, as
the descriptor sender already did. Active CLI/show migration is still owed.

## Evidence

Private Cargo target: `/tmp/weaver-146-boot-composition-target`. Lock first, serial
Cargo with `--locked`, and incremental compilation disabled.

- Types: 178 default and 236 config-feature tests passed.
- Admin: 508 passed, 45 ignored. The new real admin process runs separately inside
  the delegated observation circuit, including UID, pidfd and OFD run-lock checks.
- Harness: 1942 passed, 113 ignored, excluding duplicate subprocess summaries.
  All 14 new root/delegated cases, 19 prior input, 17 worker startup and 45 earlier
  delegated composition cases pass separately. Eighteen inherited ignores remain.
- Thirteen compiled protective controls fail as intended; all 371 Rust source
  hashes across the three affected crates restore exactly. The opener control
  removes both independent guards; the duplicate control models a lossy Value
  round trip. Removing just the harness opener guard remains safely rejected by
  the floor, and that initial non-failing control is retained in the evidence.
- External compile checks accept the public handoff and reject raw input access,
  borrowed-owner consumption and Clone (E0616/E0507/E0277). A persistent doctest
  also pins the consuming finish operation.
- Per-crate lint, config-feature types lint, formatting, initial/final lock, diff
  and three DocsMirror checks pass. Restored source is recompiled and rechecked
  after the controls; the latest worker-adapter custody correction is included.

The checking-input instrument initially omitted its valid Accepted intermediate
step. One gate used an older cached admin test executable; that child selected
zero tests and the harness correctly failed for its missing dial. The gate now
selects the actual executable from the current Cargo log. All initial runs remain
in the evidence.

The child protocols, state/model answers and timing are fixture values. Actual
socket/process composition is established here; production language/model and
measured-limit qualification are not. Root fixture cleanup does not establish
production teardown. Direct author review is not a Planner or security grade;
the review/verification skills are unavailable in this environment. Exact source,
logs, scripts and hash manifests accompany the authorized Issue #146 deposit.

## Authority and remaining composition

| Implements | Author verdict in this act |
| --- | --- |
| Types amendment section 4 TYPES-ENTER | Conforms for paired replacement LoadFacts/LoadedStateFacts and bounded Observe envelope; active paired cutover remains owed |
| Harness Spec section 2.2 Observe; admin-harness contract sections 2-3 | Conforms for observed run facts and turn extent through the input-owned management connection; full lifecycle dispatch and nested workflow polling remain owed |
| Harness amendment sections 1 HARNESS-OWNERS/HARNESS-AUTHOR and 3 HARNESS-PUMP | Conforms for retained input, actual peer/send ownership and shared pump; complete serving/Engine migration remains owed |
| Admin Spec sections 3 and 7; organ-channel section 1 | Conforms for the dormant Observe client's bounded, correlated, actual channel exchange; active command/show migration remains owed |
| Foundation section 1 FOUNDATION-TRANSITION | Conforms for keeping existing serving active until qualified coupled cutover |

Next joins are Stop/Leave/SavePoint judgment and cancellation, management inside
nested author waits and after FoundationTurn transfer, checked turn workflow and
selection/placement/generation, failed-run and final teardown, and a single coupled
admin-Enter/worker run. Ordinary silent management retains existing input/flow
bounds; this act does not apply the Leave-only two-second read cap to serving.
Full management arbitration remains owed. Approved but unmerged proposals,
reviewed language/classifier/prompt artifacts, measured limits and qualification
also remain owed. No deployment, complete Issue #146 checkbox or goal completion
is claimed. Standalone review followups remain deferred unless composition needs
them.
