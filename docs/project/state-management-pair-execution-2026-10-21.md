# External pair execution implementation record

Date: 2026-10-21. Status: draft implementation, Planner grade owed.

## Scope and authority

This slice implements the external execution portion of [Spec 05][pairs]
sections 1 through 3. It extends PAIR-OPEN, PAIR-END, PAIR-MATCH, PAIR-STATES
and RETRY. RETURN-CARRY's placement acknowledgment remains a later consumer.
The branch starts at c9aa7709, the head of draft #215 above #198.

The primitive retains checked emission, statement receipts, pair receipts,
exchange identity and received bytes. The loop chooses execution, retry,
cancellation, placement and lifecycle transitions. This slice adds no worker
activation, automatic retry, tool timeout or production language artifact.
Keep it above #215 for the coordinated foundation cutover. Its parent changes
both ends of the interrupt wire and is not independently landable on main.

## Contracts

Only the repair owner's successful EffectiveEmission can authorize execution.
A RepairAccount cannot authorize it, even when its blocks happen to conform.
The first unfinished request is the serial barrier. An inward request cannot
be skipped to run a later external call. The statement and opening each need
their exact committed receipt before a call can be staged. The loaded sh
policy supplies the tool clock. An absent grant produces the full harness
refusal, with no gate exchange.

A queued packet has a sealed send ticket tied to its socket's stable identity.
Only the complete atomic send publishes success. Its time comes from the
existing common clock. A foreign socket, an empty send slot or a withdrawn
packet cannot establish that fact. A successful send observed after EOF keeps
its evidence. An on-time send retires the dispatch wait even if the owner
observes it later. A send at the bound retains its actual execution fact and
fails that bound. There is no Decision wait while the gate executes the call.
A matched answer starts the measured terminal filing wait. Existing earlier
leave and boot deadlines keep their authority.

An accepted stop or leave withdraws an unsent packet by exact identity before
recording not_dispatched. The reserved exchange cannot be reused. A sent call
keeps its first terminal outcome. The loop sends its cancel or interrupt
through the gate protocol. Gate-fault closure cannot assert a remote execution
for a packet that was never sent.

A received reply transfers its typed value and exact datagram together. Only
the mapped sent exchange can become a terminal candidate. The gate cannot
supply the harness's Fault cause. A frozen flow retains its outcome for trace
accounting and cannot file it or grant a result. A healthy gate terminal is
recorded with origin tool and subsystem gate. The exact terminal receipt is
required before the private CompletedPair can expose its result.

Duplicate, late and unexpected replies retain their bytes until their own
Recorded rejection. That owner survives batch release, trace pressure and
lifecycle wait interruptions. It borrows an existing Decision window or arms
one measured window for its blocked trace record. Progress never renews it.
The rejection is not filed. Malformed semantic replies preserve their raw
custody and return to the existing gate fault owner. Unrelated gate traffic
stays in its original completion slot.

RejectedReturn uses the Spec's RawValue carrier. The original JSON value's
internal whitespace, key order, escapes and unknown fields are preserved.
Outer datagram whitespace remains in the received byte owner until Recorded.
It is framing outside the RawValue and is not reserialized into that field.

Retry lineage is an explicit caller choice. Its predecessor must be a filed
terminal external call in the same run with exactly the same request. The
primitive schedules no retry. Completed results stay held for the later real
placement acknowledgment. There is no boolean delivered shortcut.

## Validation

The restored full harness suite passes 1,010 tests with zero failures and
17 inherited ignored instruments. All 58 new tests run: 17 ledger cases,
18 context cases and 23 transport evidence cases. The tests use real Unix
sockets and trace recording. Scripted classifier and member replies test
protocol mechanics and receipt custody. They do not qualify the production
classifier or a deployment tuple.

Twelve isolated protective mutations compile and fail their named tests.
They remove effective-emission authority, each opening dependency, terminal
receipt validation, sent matching, sent-withdrawal protection, clock outcome
mapping, socket identity, the actual-send deadline and the observed-send
requirement. Two further checks within that twelve cover the common pump's
observed send and its reconciliation before deadline reduction. Original and
restored hashes match for all five mutated files. The unobserved-send probe
fails after it reaches the forbidden send path, at the missing clock check.
It is a runtime protective failure, not a compile failure.

Independent local review found and verified the fixes for unsent cancellation,
exact socket custody, send timing, post-release rejection ownership and the
repair guard. It independently checked the mutation logs, source restoration
and restored suite totals. No local review finding remains open. This is not
a Planner grade.

The changed-crate all-target lint, formatting, lock and diff checks are
recorded with the draft PR. Local logs use /tmp/weaver-state-pair-*.log.
Mutation evidence is in /tmp/weaver-state-pair-mutations/ and
/tmp/weaver-state-pair-pump-mutations/. The private Cargo target is
/tmp/weaver-state-pair-execution-target with incremental caching disabled.
No installation or deployment occurs.

## Remaining composition

The serving loop still owes its cutover migration, actual cancel and interrupt
calls, gate-death containment and lower-drain routing. The core preserves the
needed exchange mappings and raw evidence. Inward recall, trusted review,
boot repair and returned-coverage acknowledgment remain separate consumers.
Their absence does not authorize partial serving activation. The full goal
and its dependencies remain open in the [implementation map][map].

[pairs]: ../crates/state-management/state-management-spec-05-tool-pairs.md
[map]: state-management-implementation-plan-2026-10-20.md
