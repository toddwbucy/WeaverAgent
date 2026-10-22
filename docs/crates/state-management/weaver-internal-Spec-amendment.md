# weaver-internal Spec: pure recall query amendment

**Status:** MERGED.

**Version:** 0.5 **Date filed:** 2026-10-20 **Parent:** weaver-internal-PRD

## 0. Scope and owning clauses

Amend internal Spec sections 1, 2 and 5 and harness Spec sections 1 and 6. The
corresponding charter change is INTERNAL-RECALL in the [charter amendments][charters].
These clauses apply at the [foundation cutover][cutover]. Until then, the previous
serving contract applies. The calculator, empty normal dependency set and
operator-promotion boundary remain unchanged. [READ-3, SEAM-3, CLASS-2]

At 2eb5c5a2 the library exports calculator only and the harness does not depend on
weaver-internal. This amendment adds a second pure framework callable, the apex's
function loop and names that dependency change. It adds no process, socket, tool
registry or runtime service. Reason: READ-3 fixes the noun-history action, so the
helper's only input is the subject. [READ-1, READ-3, SEAM-3]

## 1. Pure constructor, destinations sections 1 and 2

This addition applies at the [foundation cutover][cutover]. Until then, the previous
serving contract remains applicable.

**INTERNAL-QUERY.** Add pub mod recall at src/recall.rs, with query(subject: &str) ->
Result<RecallQuery<'_>, EmptySubject>. RecallQuery<'a> has one private field subject:
&'a str and the public accessor subject(&self) -> &'a str. EmptySubject is a unit error
with Debug, Clone, Copy, PartialEq and Eq. query returns it exactly when
subject.is_empty(). Otherwise it borrows the complete original string. No trimming, case
folding, normalization, allocation or parsing occurs. A whitespace-only nonempty string
is not silently replaced by an empty query. The caller's qualified binding, not this
helper, establishes that the noun belongs to an accepted statement. [READ-1, READ-3,
CLASS-2, STORE-3]

The function reaches no filesystem, clock, model, state channel, randomness or other
crate. Its pass states only that a nonempty exact subject was supplied and retained by
reference. It does not say that history exists or that a store query succeeded. The one
library target and zero normal dependencies remain the manifest obligations. [READ-3,
SEAM-3]

## 2. Harness use, destination harness sections 1 and 6

This addition applies at the [foundation cutover][cutover]. Until then, the previous
serving contract remains applicable.

**HARNESS-RECALL.** Add weaver-internal to the harness's normal dependencies and to
tests/manifest.rs's exact allowed set. No dependency on weaver-state or weaver-spu is
added. The existing floor and trace/diagnostic dependencies keep their duties. The only
new library use in this act is recall::query. [READ-3, SEAM-2, SEAM-3]

The harness recognizes the recall key in a checked binding before constructing a query.
If that binding has several nouns or a governing condition, it completes Spec 02
RECALL-REFUSAL with the reserved identity and invokes neither this library nor state.
Zero nouns in an Accepted reply is a malformed classifier result. Only a conforming
binding with exactly one noun and no governing condition enters the pair coordinator. It
calls query on that noun, then submits ReadSelector::History with that exact subject,
verb null and condition null through the sole state owner. The request and return use
Spec 05's inward pair accounting and the ordinary bounded read/release contract.
EmptySubject becomes ToolOutcome::Refused with reason empty_subject. A state read
refusal uses ToolOutcome::Refused with reason equal to the compact JSON object
{code,detail} from the typed read refusal, in that field order. Both are completed
inward pairs under PAIR-END, with execution and reason null on the outer PairEnded.
Neither produces an empty successful history. [READ-1, READ-3, PAIR-1, SEAM-3]

The harness owns the subject bytes for the whole use of the borrowed query. It owns
scheduling, trace authoring, store I/O and result placement. The helper owns none of
those effects. Recall is recognized from the elected protocol verb, never advertised as
a new ToolDefinition or added to the gate's tool registry. The gate's sole innate sh
entry and confinement stay unchanged. [READ-3, CLASS-2, PAIR-1, October 18 R4]

## 3. Enforcement, destination sections 5 and harness 8

This addition applies at the [foundation cutover][cutover]. Until then, the previous
serving contract remains applicable.

| Claim | Instrument and evidence | Protective change that must fail |
| --- | --- | --- |
| INTERNAL-QUERY bytes and borrowing | Unit tests for empty, whitespace, mixed case and multibyte input, plus a lifetime compile pin | Trim or normalize a noun, return a different slice or permit a query to outlive its input |
| INTERNAL-QUERY purity | Existing empty-dependency/one-library manifest test and review of src/recall.rs | Add a normal dependency, target, I/O operation, clock or hidden state |
| HARNESS-RECALL scope | Updated exact dependency-set test and gate schema regression | Link the store/SPU, add recall to the gate's tool registry or introduce a new process |
| Invalid operation shape | Association/repair owner with query and state spies | Invoke query, invent a subject or open a pair for RECALL-REFUSAL |
| Query and pair outcome | Pair/read spies for exact selector, typed refusal, successful read/release and later placement | Skip the receipt, change the noun, map refusal to empty success or omit the inward terminal |

These are implementation instruments. The helper's unit pass cannot prove retrieval
coverage, pair lifecycle or task behavior. Those retain their owning Specs and
qualification. No measured numeric value is added by this amendment. [READ-3, PAIR-1,
SEAM-3, COST-5]

[charters]: charter-amendments.md
[cutover]:
state-management-foundation-Spec.md#1-applicability-and-the-serving-cutover
