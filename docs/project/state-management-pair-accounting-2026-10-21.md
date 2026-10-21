# Committed accounting for requests that cannot dispatch

Date: 2026-10-21. Status: implementation record, not a Spec amendment.

[Draft #210][draft] follows [draft #209][request-custody]. It implements the complete
opening and terminal accounting path for accepted model requests that the caller
bars from execution. The serving implementation remains unchanged until the
qualified coupled cutover. The loop supplies the cause and decides when to invoke
this primitive.

## Contract and scope

[Spec 05][pairs] sections 1 through 3 require a conforming recovered request to
receive an opening and terminal even when it cannot execute. The pair owner consumes
the complete original request proof. It records each opening before waiting for its
statement receipt, then obtains both exact commits. Each barred request receives a
not_dispatched terminal with the caller's declared cause. The seven closed causes
remain those in PAIR-END. The owner elects no pass purpose, plan, retry or tool clock.

The generation's private lease must belong to this recording context. Equal wire
identifiers from another context grant nothing. The active turn, full scheme and
selected-source membership must agree, and each required source must still be held
or pending. A caller refusal returns the original generation unchanged.

One committed statement can support several recall bindings. The owner holds that
source once, checks each exact binding and shares only a private immutable reference.
Original external-call identities, argument bytes and recall ordinals remain those
derived by #209. Unresolved requests and protocol refusal evidence remain in the
original generation, which returns after the known conforming requests are accounted.

Each opening and terminal uses the real recorder, tap and file_pair path. The private
filing owner prevents generic take, cancellation or author methods from taking its
work. A background observation owning the author is ordinary backpressure, so the
pair waits and progresses it. No unrelated bulk receipt barrier is added. Existing
source-order filing still applies and can delay the required receipt behind an
earlier ordinary filing.

The original Decision bound covers the complete accounting operation. No byte,
receipt or transition renews it. Earlier enclosing deadlines still apply. A stop
or unload changes unclaimed terminal intent, with unload taking precedence. A
record already handed to the recorder is immutable, including while its ticket is
pending. Stop never cancels an owed opening or terminal filing.

Only exact statement, opening and terminal commits construct CompletedPair and its
private FiledReceipt. The terminal preserves the source and state request identity.
Its borrowed model-facing result is a harness notice that no call was dispatched.
It contains no fabricated gate outcome. The completion remains available after the
active batch returns, and failure revokes access to grants.

Recorded source identities enter the pair owner before later intake or clock checks
can fail. The halt account keeps actual opening and terminal sources and commit
facts, including a recorded receipt not yet transferred to the pair consumer.
Unrecorded starts remain author intents, with no guessed trace sequence. The context
refuses an external halt account that omits or changes its retained pair facts.

## Evidence

The restored harness suite passes 902 tests with zero failures and 17 inherited
ignored instruments. All 31 added tests run: 13 core owner tests, 12 recorder and
Unix socket tests, five typed-mirror tests and one constructor-site instrument.
Core fixtures acquire commitments through the existing ledger phases. The socket
tests use the real recorder and framing owners with scripted classifier and member
peers. They do not qualify production language artifacts or claim a new SQLite
integration result.

Twelve compiled protective mutations fail their intended named assertions. They
remove statement and terminal barriers, duplicate-call protection, frozen-grant and
immutable-intent checks, author provenance, original-wait ownership, halt accounting,
commit evidence, busy handling, foreign custody and cancellation ownership. Each
source is restored byte for byte. Private construction and non-Clone properties
have compile-time checks, and sibling-scope pins check the consumer port's visibility.

The target directory is /tmp/weaver-state-pair-ledger-target, with TMPDIR=/tmp,
CARGO_INCREMENTAL=0 and CARGO_PROFILE_DEV_DEBUG=0. The full cargo test -p
weaver-harness --locked suite and cargo clippy -p weaver-harness --all-targets
--locked -- -D warnings pass. Formatting, lock, diff, editorial and local-link checks
also pass. Logs are /tmp/state-pair-final-tests.log, /tmp/state-pair-clippy.log and
/tmp/state-pair-mutations/results.json with each before image and assertion log.
Independent readers reviewed the code, contracts and recorded evidence. This is
local implementation review, not the Planner grade.

## Remaining work

The complete external execution path still needs the owned gate receive proof,
exact raw rejected-return evidence, containment and the already specified
flow_failure interrupt vocabulary. No arbitrary ToolOutcome is accepted as proof.
Repaired associations and EffectiveEmission remain the repair owner's work.

Inward recall execution needs the bounded read/result owner. Review authorization,
retry lineage, boot repair and placement acknowledgement remain separate parts of
Spec 05. This slice supplies no dispatch or delivery-acknowledgement API. A real
placement receipt must govern later delivery accounting. The malformed-recall
representation question from #209 remains open for an approved Spec amendment.

The production grammar, span-binding classifier, StepId lexical codec, separately
reviewed prompt and measured qualification remain cutover prerequisites. The
snapshot timing amendment and serving recall amendment remain separate draft PRs.

## Review and after-action record

An independent reader found that an ordinary busy author could be treated as a
component failure between pair legs. The driver now acquires filing custody before
claiming the leg, and progresses background work on a busy return. A real socket
regression holds the background measurement at that boundary. The same review
required selected-source filtering when checking the generation's lease.

The evidence boundary remains explicit: a private completion is constructed from
recorded commits, never from the public pair record or a caller-provided outcome.
The next execution slice must maintain that boundary through the actual gate socket.
The Planner grade and operator merge remain owed.

[pairs]: ../crates/state-management/state-management-spec-05-tool-pairs.md
[request-custody]: https://github.com/toddwbucy/WeaverAgent/pull/209

[draft]: https://github.com/toddwbucy/WeaverAgent/pull/210
