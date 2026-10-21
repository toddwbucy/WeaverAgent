# State read acquisition implementation

Date: 2026-10-21. Status: draft implementation evidence, not a Spec amendment.

This slice implements the paged acquisition part of [HARNESS-READ][harness] on
its existing state channel and the consumer checks for [STATE-PAGES][state].
It also checks the source and binding structures required by HARNESS-MATERIAL.
It leaves the recall record, HeldMaterial, public recall methods, boot seating,
placement and loop scheduling to their owning primitives. Draft #189 still
holds the unresolved recall payload contract. Nothing here treats that draft
as authority or activates the replacement serving path.

## Contract and ownership

RecordingContext exposes begin_read, advance_read and take_read to the private
flow layer. A refused begin returns its selector and turn with a Busy or
WrongPhase reason. Busy assigns no request ID and starts no wait. The context
uses the admitted read byte limit and the same coordinator, socket, request
counter, recorder and PumpIo clock as filing. No store connection is created.

The fixed two-second read allowance starts once when the operation begins.
An existing Enter or Leave bound caps it. Pages, trace pressure, refusal
cleanup and release all spend that same allowance. advance_read performs
bounded local reduction. The caller drives transport through the common pump.
No page restarts a deadline, and complete_wait cannot remove the owned wait.
The clock is checked again after result validation, before publishing a result.

PageRead owns the selected query, one snapshot identity, expected offset and
complete result bytes. It checks the declared total before reserving its
reassembly buffer. A page may split UTF-8. Content becomes available only after
strict decoding of the whole tagged result and the exact Released answer.
Each accepted answer first passes the coordinator's actual operation-measurement record.
Socket completion alone cannot release a read result.

A first valid refusal creates no cursor and sends no release. A valid refusal
of a later page retains the original code and detail until one release succeeds.
Malformed results, failed carry, an invalid release or an expired read bound
latch the existing failure path. The state socket closes so the member drops
its cursor. No guessed release or empty successful history replaces failure.
Earlier lifecycle deadlines keep their existing lifecycle disposition.

The returned CompletedRead has private fields and immutable accessors for its
selector, snapshot, decoded result and exact assembled bytes. It is not a
recall-provenance proof, HeldMaterial, SelectedUnit, boot grant or tool grant.
A future consumer still owes its recorded query provenance and context checks.
No public constructor converts inspection data into held content.

## Result validation

The adapter checks the result tag and required selector echoes. It preserves
original binding ordinals, whole Text blocks and whole raw tool-pair payloads.
It checks UTF-8 intervals, source attribution, retained pair bindings and
mirrored opening and terminal identities. Subjects and conversation require
whole binding sequences. Filtered reads keep their original ordinals.
The full seed prompt requires its complete seeded interval and binding sequence.
It is read as held content and is never filed again.

Plan and successor reads keep exact revision and query references. Plan flag
counts must agree without ordering unrelated run names as commit positions.
Observation reads check their run and frontier echoes, ordered operation IDs,
member identity and source nullability. These metadata results gain no content
or cut authority. The store still owns plan legality and commit ordering.

## Evidence

The normal harness suite passed 1,055 tests with no failures and 18 ignored
instruments. This includes 45 new active tests: 14 paging tests, 13 structural
validation tests and 18 actual-context tests. The context fixtures use real
Unix sockets and the real trace recorder with scripted member answers.

The added normal-member fixture was also run explicitly and passed. It creates
an image through the real seed member, opens the actual serving member, files
a Text block padded to require at least three pages, and checks exact content,
source and binding preservation. A second read succeeds with a new snapshot,
which checks cleanup. This fixture uses the existing recording-proof double
only at its lower channel boundary. The separate context fixtures exercise
actual recorder proofs. The synthetic language fixture is not qualification.

Thirteen protective mutations compiled and failed their named tests. Each run
restored the saved source bytes. Hash checks verified all four restored files
before the final lint-only annotation. The table names each detecting test.

| Removed check | Detecting test |
| --- | --- |
| Release before data publication | final_page_is_not_a_result_until_the_exact_snapshot_releases_once |
| Snapshot identity pin | page_snapshot_offset_total_and_done_must_stay_exact |
| Declared byte limit | an_oversized_declared_total_is_refused_before_any_reassembly_allocation |
| Cleanup after a later refusal | later_refusal_owns_the_snapshot_until_one_release_and_keeps_original_detail |
| Original statement ordinal | filtered_reads_keep_original_ordinals_and_exact_selector_values |
| Complete binding sequence | subjects_and_conversation_require_complete_unique_binding_and_source_sequences |
| Pair terminal identity | pair_attachments_keep_exact_binding_and_complete_raw_opening_and_terminal |
| Observation frontier echo | observation_queries_validate_echoes_order_frontier_identity_and_source_nullability |
| Clock check before wait retirement | validation_crossing_the_deadline_cannot_publish_a_page_or_released_result |
| Lifecycle release ownership | release_cannot_cross_the_gap_between_a_page_and_its_next_request |
| Generic completion custody | read_refuses_another_begin_and_generic_consumers_cannot_steal_its_completion |
| Wait ownership | a_read_wait_cannot_be_disarmed_during_pages_or_release |
| Failed-read socket closure | a_wrong_result_tag_or_invalid_json_never_becomes_read_content |

The restored harness suite passed with the same 1,055 successes. Changed-crate
clippy with all targets and denied warnings, formatting, the lock gate and
diff checks pass. The private target is /tmp/weaver-state-read-target, with
CARGO_INCREMENTAL=0 and --locked on resolving Cargo commands.

The local review found and fixed deadline retirement and lifecycle-release
ownership errors. It also added explicit whole-statement and seeded-prompt
binding checks. The review is not a Planner grade, and the draft remains
subject to the repository's review process.

## Remaining work and merge order

The branch is code/state-paged-read, based on draft #216 at 8f5aa872. Fold this
child into #216 before folding #216 into #215 and #215 into #198. Those merges
consolidate review branches. They do not qualify the replacement for deployment.

HARNESS-READ-CALLERS and HARNESS-MATERIAL still need the approved recall payload
and their complete consumer adapters. Fresh-history barriers remain a caller
duty. The new read operation neither drains unrelated content automatically nor
chooses when history should be queried. Boot repair, inward recall, placement,
plan progression and the coordinated serving cutover remain open in the
[implementation map][map]. Production limits and throughput still need measured
qualification on the declared tuple.

[harness]: ../crates/state-management/weaver-harness-Spec-amendment.md
[state]: ../crates/state-management/weaver-state-Spec-amendment.md
[map]: state-management-implementation-plan-2026-10-20.md
