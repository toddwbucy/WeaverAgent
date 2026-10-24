# State management worker emission composition

Date filed: 2026-10-23
Branch: `code/state-worker-outbound-composition`
Issue: #146
Parent: #307 `e23f318273ecb9ac2388e04dac465ce4d4589c07`
Status: completed implementation slice for draft review; goal incomplete.

## Composition and authority

The actual worker's checked generation now reaches the existing repair owner and
its EffectiveEmission, then the recorded SUBJECTS/OUTBOUND boundary. The worker
separately elects repair and supplies the complete outbound act order, including
whether it selects answer material. Source identities and receipts stay with the
original serving owner. Sort grants identity validation, not execution.

This act also corrects the inherited gate startup vocabulary identified by the
Planner in #297 finding 1, #301 finding 1 and #307 finding 0. Raise receives
GateReady from the production gate. The gate join and aggregate Enter admission
now require that exact arm; aggregate admin Ready remains a distinct answer.
The protocol double speaks GateReady, and the wrong-vocabulary circuit explicitly
rejects plain Ready. The gate's own Raise/Lower circuit verifies the production
implementation separately. Aggregate Enter also rejects an actual plain Ready
datagram received on the original gate socket after its checked GateReady.

Implements, graded by the author against merged authority:

| Authority | Grade and boundary |
| --- | --- |
| FOUNDATION-SCOPE and FOUNDATION-TRANSITION | Conforms for explicit worker choices and the retained serving cutover hold. |
| Spec 02 section 6 OUTPUT-UNIT, DECISION-BARRIER, REPAIR, EFFECTIVE-EMISSION and RECALL-REFUSAL | Conforms for consuming the original complete checked output, ordinary effective release, held recorded restatement, runtime instruction refusal and Stop accounting. Correction placement/decode/re-entry and exhaustion through that composed decoder remain owed. |
| Spec 08 SUBJECTS and OUTBOUND | Conforms for checked effective binding ordinals, exact compound/UTF-8 noun bytes, caller-selected ordered acts, certified template filing and trace-before-use. Corrected references remain carried by the private primitive; this act does not claim a composed correction circuit. |
| Spec 05 PAIR-IDENTITY, PAIR-OPEN, PAIR-END, COMPLETED-PAIR and PAIR-FILE | Conforms for the exercised non-dispatch join, original CallIds/StatementRefs, exact terminal filings and both call receipts before Stop closes. Execution remains a later explicit operation. |
| HARNESS-OWNERS, HARNESS-AUTHOR and HARNESS-PUMP | Conforms for retained generation/repair/sort ownership, common transport progress, original Decision bounds, runtime faults and root EOF during Stop. General nested lifecycle and failure teardown remain owed. |
| Harness Spec sections 2.2 and 6; harness-gate contract sections 1-3; admin-harness contract sections 2-4 | Conforms for the corrected GateReady/aggregate Ready seam and the exercised Stop/Observe boundary. Complete serving lifecycle remains owed. |

Approved unmerged proposals remain held. Existing serving stays selected.

## Owned transitions

The complete CheckedGeneration moves from its original ConfirmedGeneration into
OriginalRequests under the exact loaded scheme. Repair borrows the run's actual
template certificate by ownership transfer and returns it on retirement. A
compiled-family precondition failure preserves the Generated owner unchanged.
Busy admission retains the same requests and certificate while the common pump
progresses. Private source leases, raw decoder evidence and the gate exchange
remain with the enclosing owner.

Successful repair exposes a read-only workflow view. SUBJECTS projects only its
effective checked bindings by their original ordinals. The caller chooses
OUTBOUND acts. Identity validation rejects duplicate, foreign, wrong-statement,
wrong-lane and reversed answer identities. The sort owner retains the entire
emission until the decision and every selected runtime-template disposition are
recorded. A selected decision additionally waits for its real filing receipt.
An unselected decision still records and invents no filing. Runtime template
rejection retains its fault; expansion refusal records its instruction refusal.

A correctly rejected generation returns a held, recorded RestatementRequest.
The owner retains that instruction and its exact source without starting another
decode. Correction placement and decode are the next composition boundary.

Stop before repair admission starts the existing repair accounting in cancelled
state, with no instruction family or successful-emission path. Stop during repair
settles its refusals and accepted request dispositions. Stop after effective
release, during outbound filing, or after sorted completion moves the complete
repair account into the existing non-dispatch pair owner. Both external and inward
requests receive their exact stopped terminals and actual filing receipts before
the world close or admin answer. Earlier sort records, runtime faults and all
original evidence remain retained. Root EOF does not revoke an accepted Stop.

## Verification

All Cargo was serial with `--locked`, lock gate first, and private target
`/tmp/weaver-146-boot-composition-target`.

- Harness: 1,946 passed (including 22 doctests), 179 ignored in the normal Cargo
  gate. Counts exclude duplicate summaries from fixture child re-execution.
- All 29 new emission circuits and the new aggregate GateReady negative circuit
  pass separately, alongside all 131 prior generation/Stop/Observe/input/startup
  circuits. The 161 exercised delegated cases leave 18 inherited harness ignores.
- The production gate's targeted Raise/Lower test passes.
- Fifteen compiled protective mutations fail as intended. All 603 workspace Rust
  source hashes are restored and rebuilt; the exact controls are listed below.
- External compiler pins accept the consuming public handoff and reject private
  input access (E0616), borrowing it to consume (E0507), Clone and Default (E0277).
- Affected harness clippy with warnings denied, formatting, initial/final lock,
  diff and three DocsMirror checks pass. Full regression and delegated suites ran
  after restoration, using the rebuilt source.

The final compiled controls below each fail their named circuit. `emission/`
means `flow::context::worker_startup::tests::worker_emission_composition_`;
`gate/` means `flow::context::boot_composition::gate_startup::tests::gate_startup_composition_`.

| Removed property / site | Failing circuit |
| --- | --- |
| Outbound pending transport pump, worker emission | emission/ready |
| Preserve accepted act order, identity_sort | emission/ready |
| Checked identity validation, identity_sort | emission/duplicate |
| Original binding ordinal, SUBJECTS projection | emission/bindings |
| Recorded expansion refusal, worker emission | emission/no_sort |
| Cancel the held repair, worker emission | emission/restatement |
| Account held effective calls before close, worker emission | emission/stop_effective |
| Account a completed generation before repair election, worker generation | emission/stop_generated |
| Wait for the second terminal receipt, emission settlement | emission/stop_second_pair_receipt |
| Preserve accepted Stop after root EOF, management | emission/stop_held_eof |
| Check compiled-family admission before transfer, worker emission | emission/ready |
| Require production GateReady, gate startup | emission/ready |
| Require GateReady at aggregate admission, Enter ready | emission/ready |
| Reject plain Ready as a gate confirmation, gate startup | gate/wrong_answer |
| Reject plain Ready at aggregate admission, Enter ready | gate/aggregate_rejects_plain_ready |

The prior Stop/Observe circuits use `WEAVER_STOP_ADMIN_TEST_BIN` and
`WEAVER_OBSERVE_ADMIN_TEST_BIN`, both naming the actual admin test executable
produced by `cargo test -p weaver-admin --locked --no-run` in the private target.
This run reused that executable after verifying all 74 admin source hashes against
its retained build evidence. Every delegated command runs the rebuilt harness
executable in its own `systemd-run --scope -p Delegate=yes` fixture scope.


The new instruments carry tool, recall and answer identities from actual startup
and checked generation. They exercise empty output, multiple binding ordinals,
compound and multibyte nouns, malformed recall, explicit omission of an Answer,
selected/unselected decisions, held filing, Observe, deadline failure, missing or
rejected runtime templates, held restatement, the actual worker adapter, Stop at
generation/repair/effective/sort boundaries, first and second terminal receipt
pressure, and accepted Stop after root disconnect.

The first composition run exposed the missing transport pump while outbound
filing was pending. A second join closed the pre-repair completed-generation Stop
account. Initial fixture failures also exposed its missing FilePair responder;
the member double now answers the actual requests, with independent receipt
pressure. Initial logs and restored mutation evidence are retained. All checks
are author evidence, using actual sockets/forks/owners with scripted member,
classifier and model replies and fixture budgets. They are not a Planner grade,
security review, production artifact qualification or serving approval.

## Remaining work and consolidation

The operator requested consolidation after this slice. New implementation slices
are paused while the draft stack is assessed and folded into its parents by the
operator. Review clearance is checked at each current head; merge ancestry stays
intact and CodeRabbit requests remain operator-controlled.

Issue #146 still owes correction placement/decode and complete repair iteration,
pair execution, content/plan selection, cuts, world-close paths for other operation
refusals, remaining management/Leave/SavePoint and failed-run teardown, coupled
startup/workflow and the measured artifact/limit qualification. In-flight partial
OUTPUT cancellation still belongs to its inherited owner; the new pre-repair Stop
join here requires the complete checked generation. The existing review carry
lists remain open, including #296's halt-bound evidence and #307's Busy/refusal
terminal paths. This slice closes no full issue or epic checklist item.
