# Explicit begin-task owner

Date: 2026-10-22
Branch: code/state-begin-task
PR: pending assignment
Epic: #146

## Current scope

This slice implements the explicit begin_task port from HARNESS-MARKS and Spec 06
BOUNDARIES, MARK-FILE, MARK-STORAGE and MARK-READ. It retains one real AcceptedInput,
authors its one turn and message.user through the existing input owner, and waits for
that exact input's committed receipt. Ordinary input consumption creates no task.

The sole mark owner reserves the input receipt before source authoring. Its private
admission check uses the existing input-slot identity and revocation checks. Recorded
alone is insufficient, as are public SeatedInput fields and a copied source ID. The
receipt must name the same complete Text, turn and offered-input causal parent.

After filing, the owner reads marks through the ordinary paged protocol. The private
read purpose prevents another caller from taking or cancelling that result. Cursor
release and recall Recorded precede catalog use. Read refusal remains a typed refusal,
with its operation evidence already recorded by the read owner.

Catalog validation checks the loaded seed, revision 1, consecutive task/prompt IDs,
canonical active parent chain, allowed scope kinds, open boundary facts and current
root. An open scope cannot reuse the closed prompt's identity. These are facts carried
by Marks. The owner does not claim to inspect the closed prompt's hidden store rows.
An existing open task returns its scope without a new mark. Otherwise the next two IDs
are proposals until the store commits the effect. Checked successor exhaustion refuses
this operation. No harness allocation counter persists and no uncertain ID is retried.

The compiled consumer elects the literal-only mark.begin_task template family from the
actual recorded template certificate. Identity and source remain in typed context.
The state.mark event has one role-user Text, the current turn and internal/loop source.
The ordinary classifier and ledger construct the licensed effect after acceptance.
The owner verifies the exact selected context, source and final receipt before returning
its private BegunTask value. Correct template rejection records state_template_rejected
and returns an operation refusal without a component latch. Required unselected input
or mark content and invalid source/context associations are component failures.

The existing Decision window covers the transaction. The read keeps its ordinary
absolute control deadline. Completion retires Decision atomically, so late retrieval
cannot expire already-completed work. Cancellation retains its input and evidence.
Before an author ticket, a staged mark can be withdrawn. An already-owned author and
filing finish their obligations without a successful task result. No new retry clock is
created after a lifecycle yield.

## Validation and review

The first focused run compiled and passed ten of eleven tests. Its missing-selection
fixture failed during real partition activation, before the mark owner could exist.
The corrected test now asserts that actual upstream refusal for both message.user and
state.mark. TRACE-VALIDATION requires these projections, so downstream empty-selection
guards are defensive equivalents under current admission. No fabricated active
partition or bypassed AcceptedInput is used. The corrected run and compiled mutations
remain pending the root's serialized Cargo lane. The focused harness filter is
context::marks_tests. Eleven tests use the real recorder, classifier packet channel,
member stream and existing accepted-input owner with synthetic replies. They exercise:

- Delayed input filing, release and mark filing, with no earlier task result.
- Existing-task reuse, correct template rejection and upstream activation refusal
  when either required input or mark projection is missing.
- Pre-author cancellation and continued filing after an authored mark is cancelled.
- Explicit read refusal, successor exhaustion and malformed catalog identities.
- Closed-prompt reuse in the open path and exclusion of unrelated read/author calls.
- Original Decision expiry, timely completion before late retrieval, and thirteen
  nested Leave checkpoints through the original input handoff.

Compile-time pins reject Clone for both BegunTask and FiledTaskInput. They also reject
conversion from a public TaskScopeRef into BegunTask, or from SeatedInput into
FiledTaskInput. Four external trait-addition probes must each reach the intended
E0283 ambiguity and restore the exact source. Those probes are prepared but not run.

An early independent Executor reading identified closed-prompt reuse and completion
wait retirement. Both corrections and their regressions are included. That reading
was bounded and is not a Planner grade or complete independent review.

The scripted member validates the consumer protocol. It does not establish new SQL
coverage or production language qualification. Existing store begin_task implements the
atomic task/prompt effect. A final integration review must connect its existing tests
to this adapter's validated inputs without presenting scripted receipts as a real store.

## Integration boundary

This is a dormant primitive. It grants a task scope, never a placement, a decoder
operation or generation. It does not activate serving or change worker/dev_loop.
The production instruction words, language artifacts and measured limits remain owed.
Journal mark registration, Open evidence, native mark constructors, plan effects,
placement and generation confirmation remain separate work.

The sibling freshness owner is being developed in a separate worktree. When their
ancestry joins, marks admission must exclude retained freshness ownership, and freshness
admission must exclude retained marks ownership, including completed results. This
record does not claim that join or its gates are complete.
