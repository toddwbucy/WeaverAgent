# Outbound identity sort implementation

Date: 2026-10-21. Status: implementation evidence, not a Spec amendment.
Branch: code/state-outbound-sort.
Base: #219 at 7da5366cadeecc785114f679e570a273e902b245.

## Contract and scope

This slice implements [Spec 08] SUBJECTS and OUTBOUND, its outbound SORT-RECORD,
and the corresponding [HARNESS-SORT] recording adapter. It advances issue #1 and
epic #146's delivery item "Implement and qualify the merged primitive contracts".
Neither issue is complete. The branch preserves #219's exact #217 and #218 ancestry.

The pure noun function returns unique exact noun bytes in UTF-8 order. It keeps
compound nouns intact and performs no query or classification. The identity sort
accepts an owned EffectiveEmission and the caller's ordered acts. It checks the
original source, exact request lane, CallId and StatementRef, rejects duplicate
identities, and verifies answer blocks in the effective logical order. Corrected
statements retain their later source IDs while external calls retain their original
CallIds. A recall binding retains its original ordinal within the block.

The caller chooses the acts, including an empty list and whether to answer. Sort
neither opens a pair nor dispatches, places, generates, chooses a workflow or infers
final-answer policy. Serve-side selection, statement-unit construction and the
journal remain separate work. Loop orchestration remains in worker/dev_loop,
classification remains in the SPU, and the crate remains weaver-state.

## Template and recording ownership

The compiled family requires the certified sort.outbound inventory entry. It accepts
only its closed identity technical-name substitution, checks that name against the
exact loaded scheme, and binds the certificate to the recorded load and template
acceptance transcript. Literal-only restatement keeps its existing restriction.
No model text or source identifier enters a template slot. The synthetic test
inventory is not a production language artifact or an election of production words.

The context owns the effective emission and immutable acts until the exact
sort.decision has recorded. It uses the existing exclusive filing owner and the
caller's Decision bound. Where the partition selects the decision, completion also
waits for its recorded filing disposition. A correctly rejected runtime expansion
records state_template_rejected before returning refusal, without latching a
component failure. An invalid entry-time expansion returns the complete input and
an owed typed Instruction refusal author, following the existing repair entry port.
The caller must record that refusal before reporting it.

Queue pressure keeps the same author ticket and source leases. Generic filing
consumers cannot take or cancel this owner. Cancellation finishes the decision and
any owed template fault under the existing bound, then returns custody without a
sorted result. Deadline failure retains the owner for the failed run. This slice
does not complete the run's later teardown and author migration.

## Review and remaining work

Review this child against #219 and keep it in the held harness stack. The paired
wire inherited from #218 remains held for the coupled cutover. No serving loop,
deployment declaration, SPU implementation or store implementation changes here.
Only the operator merges. Planner verification and external review remain owed.

The Executor checked the local diff and exercised its guards. The named code-review
skill was not available in this session's skill catalog or local SKILL.md search.
The available Claude plugin command is a different workflow that excludes drafts.
No invocation or Planner grade is claimed for that missing review step.

## Validation

On 2026-10-21, the final source passed the lock gate, the complete weaver-harness
suite with 1,121 passing tests and 18 inherited ignored cases, the harness
all-targets clippy gate with warnings denied, workspace formatting and git diff
checks. This slice adds 20 tests. Cargo used the pinned nightly, --locked, the
private /tmp/weaver-state-measure-owner-target directory and four test threads.
The harness socket tests ran outside the restricted filesystem sandbox.

Before editing, the restored harness and types suites passed 1,228 tests with the
same 18 ignored cases at four threads. Only the harness changes in this slice.
The final gate commands and exits are in /tmp/state-sort-final-gates.json, with
logs at /tmp/state-sort-final-*.log. The tested Rust source hashes are recorded in
/tmp/state-sort-source-sha256.json. These files are local to Olympus and are not
production qualification or a Planner grade.

## Protective probes

Fourteen guard-removal probes compiled and failed their named tests. Each probe
restored saved source bytes. The last two cover fixes from the local review and
ran after the first twelve. Evidence is under /tmp/state-sort-mutations/ and
/tmp/state-sort-review-mutations/. These are Olympus-local test records.

| Removed protection | Detecting test |
| --- | --- |
| Exact original source | outbound_refuses_unknown_duplicate_foreign_and_wrong_lane_identities_unchanged |
| Duplicate CallId guard | outbound_refuses_unknown_duplicate_foreign_and_wrong_lane_identities_unchanged |
| Exact request StatementRef | outbound_recall_keeps_its_original_nonzero_binding_ordinal |
| Request lane | outbound_refuses_unknown_duplicate_foreign_and_wrong_lane_identities_unchanged |
| Logical answer-block order | outbound_answers_use_effective_block_order_and_never_binding_indices |
| Unique sorted noun bytes | subjects_preserve_compound_nouns_and_sort_unique_exact_utf8_bytes |
| Source-lease custody | outbound_busy_returns_second_owner_and_foreign_source_leases_never_grant_use |
| Cancellation disposition | outbound_cancellation_drains_its_record_without_a_sort_grant |
| Owed runtime-template fault | outbound_runtime_template_rejection_records_fault_then_returns_no_grant |
| Technical-name membership | outbound_expansion_refuses_absent_names_step_slots_and_multiple_variables |
| Exclusive filing consumer | outbound_owner_excludes_generic_filing_take_advance_and_cancel |
| Exact trace act order | outbound_records_exact_acts_before_use_and_never_opens_a_pair |
| Outbound deadline ownership | outbound_cancelled_runtime_rejection_keeps_its_owed_fault_under_pressure |
| Consumption of the retained author completion | outbound_resumes_its_recorded_fault_completion_before_releasing_the_owner |

The compile pins check the consumer API from a sibling scope and reject Clone or
Default construction of the checked outbound owner. The runtime fixtures use the
real trace recorder and Unix sockets with scripted checker and member replies.
They establish ownership and refusal mechanics, not production language quality,
measured limits or throughput.

## After action

The planned boundary was the independent SUBJECTS and OUTBOUND primitive. Reusing
filing custody also made the selected template's runtime rejection observable, so
the adapter retains that custody through its owed fault. The first test partition
did not select sort.decision, which exercised only the unselected path. The final
fixtures cover both elections and verify selected acceptance and rejection.

Local review found that ordinary filing could retire the enclosing wait before an
outbound fault was recorded, and that an interrupted post-Recorded hook could leave
an unconsumed author completion. The owner now retains the Decision bound until its
result transfers and consumes its own completed fault before release. The tests
exercise both transitions. When reusing an owner, check its completion and deadline
retirement rules against every additional record the composed operation still owes.

On this host, the default test concurrency exhausted a five-second timeout in an
inherited trace-pressure fixture. That fixture passed alone, and the untouched
baseline passed with four test threads. Use the private target and four threads for
this host's full suite, and inspect the partition before claiming a classifier or
filing path was exercised. Keep those conditions with the test result.

[Spec 08]: ../crates/state-management/state-management-spec-08-sort-selection.md
[HARNESS-SORT]: ../crates/state-management/weaver-harness-Spec-amendment.md
