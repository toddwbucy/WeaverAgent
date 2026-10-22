# State management - the domain's documents

This folder holds the state-management PRD, primitive Specs and their owning-document
amendments. The domain spans several crates and has no crate of its own. Document
Format section 2 and DocsMirrorTests admit this container under the operator's ruling
of 2026-10-19.

The PRD states the requirements. Foundation section 0 records the later operator rulings
that govern the Specs. The owning amendments below are normative parts of the same set
and name each crate Spec, charter and contract they amend. An Implements: line may name
sections here as it names a crate Spec's sections.

The set specifies primitive mechanics and contracts. The loop owns orchestration and
remains in crates/weaver-harness/src/bin/worker/dev_loop/ until pre-release. The crate
name remains weaver-state. The operator deferred its rename to pre-release on
2026-10-19.

The document merge does not activate a replacement serving path. The previous complete
implementation and contracts apply until FOUNDATION-TRANSITION's qualified code cutover.
Each affected section names that boundary. Production artifacts, measured values and
implementation instruments remain the obligations recorded by the Specs. Approval and
merge of the specifications precede their implementation.

## PRD and Specs

The writing order is foundation, protocol, pairs, marks/place, plan, selection, then
qualification. Foundation incorporates the earlier flow, store/read and persistence
source drafts. Numbers 03 and 04 are therefore not separate Specs. Conditional later
work remains at the deferred seams already named by the PRD and these Specs.

| Document | Version |
| --- | --- |
| [State Management Primitives - PRD](state-management-primitives-prd-v0_6.md) | 0.6 |
| [State management foundation - Spec](state-management-foundation-Spec.md) | 0.18 |
| [State management Spec 02 - STE100 protocol, classifier and input check](state-management-spec-02-protocol-classifier-gate.md) | 0.15 |
| [State management Spec 05 - Tool-call pairs](state-management-spec-05-tool-pairs.md) | 0.8 |
| [State Management Spec 06 - Marks, Place, and the Decode Boundary](state-management-spec-06-marks-renderer-engine.md) | 0.11 |
| [State Management Spec 07 - Locked Plan and Forward Flags](state-management-spec-07-plan-forward-flags.md) | 0.12 |
| [State Management Spec 08 - Sort and Selection Primitives](state-management-spec-08-sort-selection.md) | 0.14 |
| [State Management Spec 09 - Evidence and Qualification Boundary](state-management-spec-09-qualification-costs.md) | 0.13 |

## Owning amendments and the STE100 contract

These files state complete replacement clauses and their applicability. They are part
of the specification set, not requests for the implementation author to design a
contract. Existing code continues to use the previous contracts until the named cutover.

| Document | Version |
| --- | --- |
| [Dependable flow: charter amendment text](charter-amendments.md) | 0.18 |
| [Dependable flow: contract amendment text](contract-amendments.md) | 0.17 |
| [State persistence - contract amendment text](persistence-contract-amendments.md) | 0.8 |
| [weaver-admin Spec: flow, seed, publication, and boot amendment](weaver-admin-Spec-amendment.md) | 0.15 |
| [weaver-diagnostic Spec: isolated legacy conversion amendment](weaver-diagnostic-Spec-amendment.md) | 0.8 |
| [weaver-gate Spec: input refusal and fault interrupt amendment](weaver-gate-Spec-amendment.md) | 0.7 |
| [weaver-harness Spec: dependable-flow amendment](weaver-harness-Spec-amendment.md) | 0.25 |
| [weaver-harness / weaver-spu - STE100 contract](weaver-harness-spu-ste100-contract.md) | 0.5 |
| [weaver-internal Spec: pure recall query amendment](weaver-internal-Spec-amendment.md) | 0.4 |
| [weaver-spu Spec: placement measurement and STE100 duty amendment](weaver-spu-Spec-amendment.md) | 0.6 |
| [weaver-state Spec: flow, schema, read, and persistence amendment](weaver-state-Spec-amendment.md) | 0.19 |
| [weaver-trace Spec: dependable-flow amendment](weaver-trace-Spec-amendment.md) | 0.22 |
| [weaver-types Spec: dependable-flow amendment](weaver-types-Spec-amendment.md) | 0.25 |

The frozen [graphable paper v0.14][paper] is supporting theory. The PRD and later
operator rulings take precedence over its alternatives. Local audits, review records,
evidence, old drafts and loop scratch are not part of this published set.

[paper]: ../../project/graphable-state-management-primitives-v0_14.md
