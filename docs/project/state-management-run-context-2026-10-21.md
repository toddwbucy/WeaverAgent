# Run recording, transport pump and failure-account support

Date: 2026-10-21. Status: draft PR #203, pending Planner and operator review.

[PR #203][pr] is stacked on [PR #202][base]. It implements the recording lifetime
and transport scheduler needed by the replacement harness. The whole existing
serving path remains active under [FOUNDATION-TRANSITION][foundation]. The loop
stays in worker/dev_loop, and classification stays behind the SPU contract.

## Scope and authority

The [harness amendment][harness] requires one author, recorder, clock origin,
coordinator and set of completion owners for a serving run. This slice supplies
those mechanics and the frozen failure account. Lifecycle caller migration,
input checking, capture orchestration and the containment ladder remain owed.

| Component | Owning clauses | Implemented boundary |
| --- | --- | --- |
| RunRecorder and RecordingContext | HARNESS-RECORD-LIFETIME, HARNESS-AWAIT-AUTHOR, HARNESS-RECORD | Preserve recorder typestate, original caller input and ticket through load, activation, refusal, clean release and drain |
| Startup and retirement | TEMPLATE-CHECK, HARNESS-LEDGER, HARNESS-RECORD-LIFETIME | Structural Open before load, exact measurement after activation, held classifier admission, explicit port retirement |
| CommonPump | HARNESS-PUMP, HARNESS-CLOCK | One bounded operation per owner, lifecycle and terminal priority, rotating ordinary readiness, one monotonic clock and absolute bounds |
| FlowHaltAccount and HaltLatch | HARNESS-HALT-ACCOUNT, HARNESS-HALT-BOUND, HALT-ACCOUNT-SIZE | Freeze the first cause and original deadline, project real obligations, count and encode the complete account |
| Shared fault cases | TYPES-FAULT, TRACE-HALT, TEMPLATE-CHECK | Add the already specified state_flow_failed and state_template_rejected spellings |

## Recording ownership

RecordingContext starts with an unused state owner and preflight recorder. It
captures the common monotonic origin before sending Open. Load must agree with
that origin, the retained flow declaration, the verified start and read limits,
and the member identity returned by Open. The copy registry is state-copy/1.
The store schema remains state-content/1. Neither name substitutes for the other.

The load record does not release Open. Semantic partition validation converts the
same declaration into the activated tap. The exact Open measurement must record
before the coordinator grants its result. Classifier Ready and Refused answers
stay held until the caller takes them. Ready checks terminal status again before
transferring the port. A correct refusal has an explicit retirement path.
The classifier bootstrap must match the exact scheme bytes and instruction in
load, including the checker artifact and wire bound, before it sends any bytes.

author_and_wait retains the original six-member AuthoredEvent before a trace
ticket exists. It waits for a whole-copy backlog through the same coordinator and
pump. Once a ticket exists, transport can progress but no completion is reduced
behind that author. Resumption keeps the original source and prepared event.
If a lifecycle deadline interrupts just after Recorded, the original Sequence
stays held for the caller. No retry submits that event twice.

A correct partition refusal keeps the never-activated recorder. Existing admin
rollback authorizes disposal of the unused Open exchange after a final terminal
probe. Its evidence remains retained without becoming a completion proof. The
same context records refusal and unload without tapping either. Clean serving
release still requires no outstanding filing, held completion or pending author.
Port retirement cannot turn an unexpected EOF into a successful release.

## Common transport progress

The pump borrows the standing listener, admin peer, residency, gate, decoder and
legacy label owners. The coordinator supplies state and STE100, and the recorder
supplies its capacity descriptor. No transport has a second reader. Lifecycle
descriptors preserve SCM_RIGHTS ownership and reject malformed or excess handles.

A nonconsuming readiness probe orders lifecycle work, terminal indications and
rotating ordinary sources. A terminal indication goes through the owning parser,
so a complete refusal before peer close remains available. Each source performs
at most one bounded transport unit per step. Full completion slots retain terminal
interest without repeatedly reading new messages. Buffered work and interruptions
stay visible before a blocking wait.

The common clock checks the nearest original deadline before and after each
unit. It never resets a wait on progress. Enter, leave and legacy-label timeouts
return to their existing owners. Gate, cut, decision and undisputed state-control
expiry use their specified flow-failure reasons. Snapshot and finished timeout
cleanup remain with the later capture owner.

## Failure evidence

The first flow failure revokes authoring and freezes the copy budget and ledger
before reporting. HaltLatch retains that cause and the earlier of the existing
leave deadline and the 150-second halt allowance. A missing clock or overflow
cannot create a replacement time anchor. This is a deadline representation,
not the complete containment sequence.

The snapshot reads frozen owners without receiving bytes, draining an inbox or
releasing a proof. It records the ledger in order, queued and failed whole copies,
the original unadmitted author ticket, exact request identities and transport byte
counts. Commit evidence distinguishes not requested, partial, unknown, committed
and refused. An already valid receipt may establish commit evidence before its
measurement grants release. A malformed reply cannot establish it.

External input, pair and capture owners must supply their own frozen facts.
There is no default that can hide an absent producer. A snapshot failure returns
those facts unchanged. Before an Open baseline exists the snapshot refuses with
SnapshotMissing instead of making up a flow sample. The same installed first
snapshot supplies fault and unload account bytes. Later failures append only to
the unload account. Counting uses the escaped JSON bytes before allocation and
keeps raw payloads, null options and decimal-string counters.

## Validation

The full affected-crate suites pass 909 tests with zero failures and 17 inherited
ignored instruments: 665 harness, 137 trace and 107 types tests, including doctests.
The command was cargo test -p weaver-types -p weaver-harness -p weaver-trace --locked.
Real Unix sockets ran on the host socket lane. The log is held locally at
/tmp/state-run-context-final-tests.log.

All-target clippy for weaver-types and weaver-harness passes with warnings denied.
Workspace formatting, the lock gate and diff checks pass. The lint log is
/tmp/state-run-context-clippy2.log. These checks do not qualify the SPU CUDA lane,
production classifier artifacts or deployment measurements.

Twenty-six compiled guard removals fail their intended named assertions. They
cover pump rotation and terminal priority, final-unit and drain deadlines,
first-failure precedence, original bound identity, frozen byte and commit facts,
held Ready custody, refused rollback, retained author completion, load/registry
agreement and bootstrap identity. The local evidence directory is
/tmp/state-run-context-mutations-final, with results.json and one log per probe.

An initial deadline probe was too weak because a later precheck caught the same
expiry. It is excluded from the final evidence. Its replacement exercises the
final trace unit, where no later precheck can hide removal of the guard. Every
final probe compiled and ran its exact test. Independent review confirmed all 26
failures and exact restoration hashes for the eight mutated source files. The
full passing suites above ran after restoration.

Independent source review also corrected load/bootstrap disagreement, interrupted
author return custody, refused-enter rollback, pending Ready release, first-cause
precedence and final-drain timing. This evidence proves the component boundary
below. It does not claim a complete serving migration or containment sequence.

## Integration still owed

This component does not replace the serving Run, Ports or lifecycle call sites.
The finite author-call census and every ignored-result disposition remain part of
that migration. The context owns its recording state, while the full failed-exit
ladder still owes tool containment, caller close, gate lowering, member release,
terminal fault/unload submission and bounded final exit.

The input transaction, template inventory and certificate, boot read and placement,
read pagination, model repair and save cleanup remain their own contracted work.
The serving recall payload amendment and snapshot timing amendment still require
their review. No production STE100 artifact, system prompt or measured bound is
qualified by the scripted fixtures in this slice.

[base]: https://github.com/toddwbucy/WeaverAgent/pull/202
[foundation]: ../crates/state-management/state-management-foundation-Spec.md
[harness]: ../crates/state-management/weaver-harness-Spec-amendment.md
[pr]: https://github.com/toddwbucy/WeaverAgent/pull/203
