# Model request sources and receipt custody

Date: 2026-10-21. Status: implementation record, not a Spec amendment.

[Draft #209][draft] follows draft #208. It adds per-source receipt custody to the complete
output check and derives original model requests from that checked message. The
serving path remains on the legacy implementation until the coupled cutover. The
loop still owns invocation, repair scheduling, execution eligibility and dispatch.

## Contract and scope

[Spec 02][protocol], section 6, OUTPUT-UNIT, DECISION-BARRIER and TOOL-STATEMENT,
requires exact recorded Text checks and send receipts without waiting for unrelated
accepted siblings. [Spec 05][pairs], sections 1 and 2, PAIR-IDENTITY and PAIR-OPEN,
names recovered external ordinals, recall ordinals and request statement references.
This slice implements source custody and original request association within those
clauses. It does not implement EffectiveEmission, repair, pair opening, terminal
accounting, gate execution, inward retrieval or result placement.

The generation owns a private receipt lease registered before any selected source
can retire. Registration binds the recorder's actual source IDs, including a copy
whose installation waits for retention capacity. After all checks record, the
complete generation can return while its statement filings remain pending. A caller
can then take each committed source independently. That fact carries the original
filing, correlated Filed answer and recorded terminal disposition. Correct rejection
has a distinct result and never becomes a committed receipt.

The lease token names one ledger instance, including when another instance has equal
wire identifiers. It cannot be cloned or decoded. Sources outside its finite set,
repeated takes, overlapping ownership and historical sources refuse. Multiple
leases can coexist with the earlier bulk filing transaction. Taking a source fact
creates no tool, pair, placement or answer permission, and starts no deadline.

Explicit release removes consumer interest and preserves outstanding filings.
Dropping a token abandons that interest immediately. Weak ownership lets later
ledger progress prune its retained storage. A cancelled author that later completes
its trace ticket cannot recreate the abandoned lease. Failure freezes receipt
access while still permitting cleanup. Existing owed-source gauges and save
barriers continue to describe filings, not retained consumer facts.

The request reader consumes CheckedGeneration and the exact verified scheme
artifact. It retains that proof and its lease. Each recovered external call keeps
its ordinal, original content index, name and argument bytes. Only its immediately
preceding Text with one accepted non-recall binding supplies a StatementRef. A
correctly rejected adjacent Text stays unresolved for repair with its original call
bytes. A missing or ambiguous association preserves refusal evidence.

Recall candidates use the scheme's recall keys, one noun and no condition. Their
ordinals follow the total recovered external count, while the candidate list stays
in source-content order. UnrecoveredCall does not consume an external ordinal or
acquire a send. Accepted requests remain discoverable when a sibling is rejected,
including in a correction pass. No candidate is an execution grant.

Multiple requests may name different binding ordinals in the same Text. The later
pair owner must take that source receipt once and validate each request against it.
It must not try to consume one receipt per request or recreate commit evidence from
a plain StatementRef. Repaired associations still need the private repair proof.

## Evidence

The full harness suite passes 871 tests with zero failures and 17 inherited ignored
instruments. All 49 added tests run: 19 request-association tests, 14 ledger/lease
tests and 16 actual-context tests. The latter use the real recorder and Unix socket
owners with scripted classifier and state-member peers. They are not new SQLite
commit tests or production classifier qualification.

The required invocations use the private target directory
/tmp/weaver-state-request-accounting-target, TMPDIR=/tmp, CARGO_INCREMENTAL=0 and
CARGO_PROFILE_DEV_DEBUG=0. cargo test -p weaver-harness --locked and
cargo clippy -p weaver-harness --all-targets --locked -- -D warnings both pass.
cargo fmt --all -- --check, process/gates/lock.sh, git diff --check and the changed
documents' editorial and local-link checks pass.

Thirteen compiled protective mutations each fail their named assertion. They cover
external and recall identity, adjacency, exact scheme, sibling independence, foreign
lease ownership, single transfer, retained early receipt, deferred copy, two
cancellation paths, frozen access and abandoned storage. All mutated files are
restored byte for byte. An independent reader verified the logs, assertion failures
and source hashes. Non-Clone/private construction and sibling method access have
compile-time instruments.

Local logs are /tmp/state-request-final-tests.log, /tmp/state-request-clippy.log and
/tmp/state-request-mutations/results.json with each probe's before image and log.
Independent implementation review is complete for this slice. The Planner's grade
and the operator's merge remain owed.

## Remaining work and Spec question

Full repair must author certified restatement instructions and account for valid
requests that cannot dispatch. Pair opening must obtain both the statement receipt
and the PairStarted receipt. Later terminal accounting, gate correlation, stop/leave
handling, boot repair and result delivery remain required. This slice does not
claim those contracts complete.

Spec 02 RECALL says that a malformed recall shape becomes a typed call refusal.
PairRequest::Recall carries one subject, while ProtocolRepairCase has no
invalid-recall case. The set does not state a subject-selection rule for a compound
request or an exact refusal carrier for this path. The reader retains its CallId,
StatementRef and defect evidence without inventing a subject, pair or wire event.
The operator question asks whether to define a refusal before pair creation or
extend the pair representation. That edge remains open pending an approved Spec
amendment. Existing store code must not be described as enforcing this whole shape
merely because it checks that a supplied subject matches a held noun.

Production grammar, the StepId lexical codec, the span-binding model, system prompt
and qualification remain separate prerequisites. Scripted replies test the mechanics
here and qualify none of those artifacts. The serving cutover remains closed.

## Review and after-action record

Independent readers found two ownership defects before filing: cancellation could
revive a lease when a pending author completed, and abandoned handles could retain
storage indefinitely. The implementation now suppresses registration for cancelled
authors and tracks lease lifetimes with weak ownership. Both have actual-context
regression tests. A further composition fixture connects a model request dependency
to its actual source receipt while an ordinary sibling remains pending.

The initial intention was to connect repair and request accounting. Inspection
showed that early receipt retirement had to be solved first and that malformed
recall lacked a defined disposition. This branch keeps that ownership step complete
and leaves the larger lifecycle explicit. The review lesson is to test the sequence
from recorded output through dependency selection and committed receipt, in addition
to each endpoint's local tests. This note carries that lesson into the next slice.

[protocol]: ../crates/state-management/state-management-spec-02-protocol-classifier-gate.md
[pairs]: ../crates/state-management/state-management-spec-05-tool-pairs.md

[draft]: https://github.com/toddwbucy/WeaverAgent/pull/209
