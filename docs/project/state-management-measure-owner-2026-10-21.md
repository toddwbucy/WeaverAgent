# Owned decode measurement implementation

Date: 2026-10-21. Status: implementation evidence, not a Spec amendment.
Branch: code/state-measure-owner.

## Contract and scope

This slice implements the read-only measurement owner under [Spec 06] MEASURE-DELTA
and the measurement portion of [HARNESS-PLACE]. The existing common pump carries the
request and reads its answer. The harness holds canonical Messages and ordered
schema strings. The SPU remains the rendering and token-count authority.

A private decode reservation holds one request until its terminal reply is consumed.
Whole-send evidence follows the final segment. A reply's evidence records whether
its first segment arrived after that send. Generic staging and completion-taking
cannot borrow the reservation. A foreign port or an abandoned ticket cannot release
it. Every measurement must enter through this reservation. Generic staging refuses
MeasureDelta, so an unowned measurement cannot leave a reply for the next owner.
A Fault emission closes no exchange. A complete reply retains its exact bytes
even when a later transport failure prevents its use.

The context retains the exact proposal, its expected resident position, assembly
and journal revision. It checks the returned resident, prefix, capacity and clamped
keep. Oversized input counts remain counts. Capacity policy belongs to place.
MalformedDelta produces the existing decode refusal and the specified placement
refusal. The shared RefusalRecord::Placement arm follows [TYPES-DECODE], including
decimal assembly IDs and the closed PlacementCode set.

An unexpected protocol refusal latches failure before reporting it. Its owed decode
refusal then uses the existing trace-only drain under the halt deadline. Queue
pressure retains the exact author and raw reply. A reporting failure is secondary
and cannot replace the first cause. This path grants no copy or serving result.

Measurement borrows an existing Decision or Cut bound. It creates no clock and
does not retire the enclosing placement obligation. Cancellation consumes the
outstanding exchange and yields no measured proposal. A changed revision yields
stale_position. The returned private evidence grants no append, cut or generation.
Place must supply the actual journal revision and recheck it before append.
The engine must also establish that any earlier nonmeasurement decode exchange has
settled. The reservation guards this measurement's ownership. It does not certify
the old generation/cancel protocol's idle state from an empty outgoing buffer.

## Dependency and remaining work

The branch starts from #217 at bdfa3a76 and incorporates #218 at 8b387eff. The
combining commit is 32a57b57. Its only manual merge resolution was the implementation
map. The implementation changes follow that merge and can be reviewed separately.
Both parent reviews remain required. No deployed serving path changes in this slice.

The later place owner still owes BootHead, budget admission, source maps, the journal,
marks, recorded placement and the single-use append grant. This measurement result
does not replace any of them. The known OPEN-FACTS identity-prefix carrier gap and
the unmerged recall amendment #189 remain open. Production language and capacity
qualification are also separate obligations.

## Validation

The harness suite passed 1,101 tests, with 18 inherited ignored instruments. The
shared-types suite passed 127 tests, and its config-feature suite passed 182 tests.
The private target is /tmp/weaver-state-measure-owner-target. These tests qualify
consumer mechanics, not production capacity, classifier quality or decoder speed.

The new cases comprise 24 recording-context tests, 17 reserved-transport tests,
two shared refusal-wire tests and one trace-only author test. They use actual Unix
packets and the actual trace writer, including segmented requests and replies,
queue pressure, exact deadline expiry, early emissions, typed and fatal refusals,
cancellation, stale revisions and foreign ownership. Two inherited append fixtures
were adapted to the required schema field when the parent branches joined.

The SPU implementation is unchanged from #218. Its device-lane evidence remains in
the [paired decode record][paired]. This slice adds consumer checks and does not
claim a new device or production qualification run.

Independent local review found and corrected refusal-record custody after a fatal
reply and generic staging of unowned measurements. It is not the Planner grade or
third-party review. Both changed-crate all-target lint gates, formatting, the lock
gate, documentation checks and diff checks passed.

Fifteen perturbations compiled and failed their protective tests. Each probe restored
its saved source bytes. The final suites and gates ran on restored code, including
the lint corrections and the additional halt-bound reporting test.

| Removed or changed protection | Detecting test |
| --- | --- |
| Reserved generic completion guard | generic_stage_and_take_cannot_steal_the_reserved_wait_or_held_reply |
| Reserved generic staging guard | generic_stage_and_take_cannot_steal_the_reserved_wait_or_held_reply |
| Measurement's exclusive entry | every_measurement_requires_the_reserved_entry_point |
| First response segment's send status | reply_preamble_before_whole_send_cannot_be_laundered_by_late_segments |
| Required terminal before settlement | fault_emission_does_not_close_the_reserved_exchange |
| Invalid reply history | wrong_answer_is_retained_and_a_later_right_answer_cannot_erase_it |
| Exact ordered schemas | exact_measure_proposal_and_oversized_count_cross_without_placement_authority |
| Capacity agreement | each_inconsistent_serial_count_is_a_protocol_failure_not_zero_or_empty |
| Post-validation deadline sample | validation_crossing_the_original_bound_cannot_publish_measurement_facts |
| Cancellation flag | cancellation_drains_the_same_answer_without_a_measurement_grant |
| Journal revision check | a_stale_revision_is_recorded_as_refusal_without_releasing_the_old_measure |
| Owned deadline guard | exact_measure_proposal_and_oversized_count_cross_without_placement_authority |
| Read-owner exclusion | a_measure_owner_refuses_a_read_without_starting_another_control_wait |
| Halt-only trace refusal queue | owed_refusal_queues_only_after_halt_and_cannot_replace_an_original_or_admit_content |
| Placement refusal wire name | placement_refusals_preserve_exact_assembly_and_closed_code |

Local logs use /tmp/state-measure-*. Probe logs, results and restoration hashes are
in /tmp/state-measure-mutations/. The SPU and classifier fixtures are not substitutes
for the foundation's production qualification evidence.

[Spec 06]: ../crates/state-management/state-management-spec-06-marks-renderer-engine.md
[HARNESS-PLACE]: ../crates/state-management/weaver-harness-Spec-amendment.md
[TYPES-DECODE]: ../crates/state-management/weaver-types-Spec-amendment.md
[paired]: state-management-decode-measurement-2026-10-21.md
