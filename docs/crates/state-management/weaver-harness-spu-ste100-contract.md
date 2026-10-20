# weaver-harness / weaver-spu - STE100 contract

**Status:** MERGED.

**Landing PR:** #182

**Version:** 0.4 **Date filed:** 2026-10-20 **Document ID:**
weaver-harness-spu-ste100-contract **Parent:** weaver-agent-PRD, invariant 5.3
**Editorial:** Per the Working Rules.

**Landing path:** docs/crates/state-management/weaver-harness-spu-ste100-contract.md.
The domain container holds this agreement with the final set under R20-14. Its apex
parent matches the other harness/SPU seams. The state-management PRD charters the
elected duty, without changing that contract parent.

## 0. Scope and applicability

This agreement governs the separate STE100 duty chartered by CLASS-1 through CLASS-10
and October 18 R10. Its parties are the loop compiled into the weaver-harness worker,
the caller, and weaver-spu, the checker. The seam is state-ste100/1. The checker runs
as its own SPU process behind its own Unix socket. It applies at the [foundation
cutover][cutover]. Until then the previous serving contracts apply. The label-scoring,
residency and decode agreements retain their distinct duties. [CLASS-2, SEAM-2, R10]

Representation, transport and state tables belong to [Spec 02][protocol] and the types,
harness and SPU amendments. This agreement states what the parties may assume, what each
must refuse and what an outcome means. It does not grant the checker a store, tool,
trace or argument-writing duty. Reason: a classification result and the loop's use of it
are separate acts. [CLASS-1, CLASS-2, SEAM-1, SEAM-2, FLOW-1]

## 1. Admission and identity

The harness supplies one immutable scheme and the declared checker artifact. It may
assume that readiness confirms the exact admitted identities and declared instance
count. It must refuse a missing or mismatched admission result and may submit no content
request before readiness. Admission failure prevents serving load completion through the
existing rollback. [CLASS-2, CLASS-4, CLASS-8, PERSIST-6]

The checker may assume that the harness supplies the inventory's exact bytes. It must
validate its artifact, grammar, vocabulary and compatibility before claiming readiness.
It refuses an unsupported language construct or incompatible model instead of silently
reducing the language checked. It neither extends the verb set during service nor
changes the scheme between requests. The parties' load pass proves identity and
compatibility, while production fidelity requires the separately recorded qualification.
[CLASS-1, CLASS-2, CLASS-7, STORE-8]

The protected seed certificate is an operator-provided attestation produced by offline
qualification against this same checking contract. It is not a serving store write and
not permission to skip the normal input or output checks. Its producer assumes the
caller's obligations when exercising the checker offline. The certificate's
interpretation and seed verification belong to PERSIST-5 and Spec 02's SEED-CERTIFICATE.
[PERSIST-5, PROMPT-2, CLASS-2]

## 2. Requests and outcomes

The harness supplies an exact whole Text block with its source or input target, or a
template from the recorded inventory with its template target. It retains the target,
scheme and bytes until the result is disposed. The checker may assume one outstanding
request and must return one complete, correlated result. It may divide computation
internally, but it cannot return a partial acceptance that permits a prefix of the
requested block to be filed. [CLASS-2, CLASS-4, STORE-3, PLACE-4]

For accepted Text, the result binds every admitted statement's verb to its noun or nouns
and governing condition, with exact source spans. The harness validates the
representation and correlation before using it. Acceptance is a qualified
language/binding judgment, not a claim of truth, safety or authorization. The checker
does not know whether the partition selected that source for state. [CLASS-2, CLASS-4,
STORE-2, STORE-3, TAP-1]

For a template, acceptance means that every substitution from its declared classes
satisfies the checked grammar. It is not sampled approval of a few example expansions
and carries no invented bindings over template syntax. The harness may instantiate only
the certified template and typed substitution classes. Arbitrary strings cannot acquire
that certificate. [CLASS-2, FLOW-8, PROMPT-2, review S2]

A correctly reported language defect is an ordinary result. The harness records it and
refuses external input or restates a model emission under CLASS-9. A bad template
refuses load or its current operation. None of those outcomes is component failure. The
checker cannot request a retry or decide when the agent's plan, input or model response
is accepted. [GATE-1, CLASS-8, CLASS-9, CLASS-10, PLAN-5]

A crash, absent terminal result or malformed result is component failure. Capacity that
prevents a conforming input from being checked is absent work, not a language defect.
The harness refuses wrong targets, changed schemes, invalid spans, extra results and
incompatible reply vocabularies. It does not reinterpret a label score or channel
closure as an accepted STE100 result. [CLASS-2, CLASS-8, FLOW-4, R10]

## 3. Ordering, lag and termination

The harness records the load before the checker starts and records every complete result
before reuse, filing, repair or a dependent decision. No reply changes an earlier trace
event. Template checks complete before boot placement and opening the user channel. The
checker authors no trace event of its own. [FLOW-2, FLOW-3, SORT-4, PROMPT-1, PERSIST-6]

Ordinary output lag is measured. Where a check blocks input, a cut or another model
decision, the harness owns the declared bound and its failure disposition. Partial
progress does not renew a bound. A pending result below the bound is not missing. A
result still pending at the bound is missing and the flow halts. The checker may not
extend that wait by withholding its terminal answer. [CLASS-6, CLASS-8, GATE-1, MARK-4,
FLOW-4]

Once failed, the serving flow does not reattach this checker and continue. The halt
stops further decisions and uses the foundation's terminal account. It creates no
recovery dump under October 18 R3. During orderly teardown the parties account for an
outstanding exchange rather than inventing success from closure. [FLOW-4, FLOW-5,
October 18 R3]

## 4. Custody and permitted effects

Only the paired caller may submit requests to a checker instance. Neither party permits
another child or caller to acquire this access. The checker receives no capability to
read the store, modify the trace, dispatch tools or place text in the argument. Its
model instances share one immutable scheme and answer through the one owned port. The
harness remains responsible for source attribution and one-time disposition. [SEAM-1,
SEAM-2, FLOW-1, FLOW-8]

The trace remains the operating agent's one content feed into state. Reusing accepted
input labels requires the same bytes, scheme and single-use target token, followed by
the normal selected tap copy. A conformance result alone never grants a store write.
Correct rejection owes no filing. [FLOW-1, FLOW-2, CLASS-4, CLASS-10, STORE-8]

## 5. Vocabulary and definition sites

The floor's state_flow module owns Ste100Instruction, Ste100Bootstrap, Ste100Ready,
Ste100AdmissionRefused, Ste100Target, Ste100Request and Ste100Reply. Their
representations are TYPES-CONFIG, TYPES-STE100-ADMISSION and TYPES-CLASSIFIER in the
[types amendment][types]. TemplateSpec and its slot vocabulary are Spec 02's TEMPLATES,
carried as shared floor types. No legacy LabelDirective, LabelAnswer or LabelRefusal is
an alias for this vocabulary. [CLASS-2, CLASS-4, SEAM-2, R10]

Binding, NounBinding, Span and source identity use the existing foundation definitions.
The harness owns the private AcceptedInput, TemplateExpansion and EffectiveEmission
capabilities. They are local proofs of disposition, not checker-issued permission to
act. [CLASS-4, CLASS-9, SEAM-1, STORE-3]

## 6. Conformance and changes

| Claim | Owning instrument and required evidence | Protective change that must fail |
| --- | --- | --- |
| Admission identity | TYPES-STE100-ADMISSION and SPU-STE100 readiness corpus | Accept a mismatched scheme, model, count or initial limit |
| Whole result and correlation | CHANNEL/PORT tests plus the harness ledger fixture | Accept a partial answer, wrong target, extra reply or legacy label score |
| Meaning of a pass | MODEL/BINDING corpus and TEMPLATES inclusion proof | Treat samples as template proof or a capacity fault as bad language |
| Defect versus failure | INPUT/REPAIR and terminal fault matrix | Halt on correct rejection, continue after broken checking or report closure as success |
| Trace and one feed | HARNESS-CLASSIFY, REUSE and TRACE-RESULT fixture | Use an unrecorded reply, reuse different bytes or write without a tap copy |
| Custody | Process/descriptor and manifest instruments | Give the checker another organ's handle or add a second harness owner |
| Bounds and lag | Pump fixture under gate/cut/decision waits | Reset the deadline on partial traffic or halt background lag by age |

These instruments prove the named mechanics under their fixtures. The production
grammar, model and template evidence remains the separate qualification required by
CLASS-7. A double never certifies a production language instance. Both parties and their
owning documents change together before code relies on a changed contract. [CLASS-7,
FLOW-7, SEAM-2]

[cutover]:
state-management-foundation-Spec.md#1-applicability-and-the-serving-cutover
[protocol]: state-management-spec-02-protocol-classifier-gate.md
[types]: weaver-types-Spec-amendment.md
