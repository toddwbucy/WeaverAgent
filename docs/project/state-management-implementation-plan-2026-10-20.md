# State management implementation map

Date: 2026-10-20. Status: implementation tracking, not a Spec amendment.

The authority is the merged [state management set][set] from PR #182. All code PRs
remain drafts until the prescribed review completes. The operator merges them.
The crate remains weaver-state. The loop remains in worker/dev_loop and calls
primitives. The SPU owns classification.

## Dependency and merge order

Each row is a reviewable code slice. Dependent branches may be stacked before their
base merges. A PR body must name its prerequisite PRs and the exact owning clauses
it implements. Rows can be split further without changing the Spec or cutover.

| Slice | Prerequisites | Implementation and owning Spec |
| --- | --- | --- |
| F1 | PR #182 | Shared state_flow shapes, codecs and pure validators, types amendment |
| F2 | PR #182 | RAM connection and full catalog, state STATE-CONNECTION/STATE-SCHEMA |
| F3 | F1, F2 | Atomic filing, effects, reads and verified images, state amendment |
| F4 | F1, F3 | Member, paging and capture cleanup, state/contract amendments |
| T1a | PR #182 | Pure trace registry and validated copy, trace amendment |
| T1b | T1a | Typed trace authoring and recorder integration, trace amendment |
| IN1 | PR #182 | Pure borrowed recall query, internal amendment |
| C1a | F1 | SPU transport, admission proof and constrained binder, Spec 02 |
| C1b | C1a | Normative grammar and template proof, Spec 02 O02-1 |
| C1c | C1a, C1b | Model adapter, duty entry and qualification, Spec 02 O02-2 |
| P1 | F1, F4, C1 | Seed, custody and boot, admin/persistence amendments |
| H1 | T1b, F4, C1, P1 | Harness flow, receipts and halt, harness/foundation amendments |
| H2 | H1, IN1 | Pair ledger and gate containment, Spec 05 and gate amendment |
| H3 | H1 | Marks, renderer and SPU buffer boundary, Spec 06 and SPU amendment |
| H4 | H2, H3 | Plan operations and review evidence, Spec 07 and admin amendment |
| H5 | H3, H4 | Sort, selection and injection ports, Spec 08 |
| Q1 | All above | Serving qualification and coupled cutover, foundation transition |

Spec 09's required evidence producers travel with the primitive that produces each
measure. Evidence analysis remains in WeaverAnalysis. External diagnostic contracts
are amended through the owning repository's review path.

The first two slices are independent and additive. F2 is the complete empty catalog,
not a serving store or valid seed. F3 joins both branches and supplies the complete
state-content/1 behavior. No partial catalog is a deployment option.

F1 does not replace the legacy lifecycle/configuration grammar. Those changes travel
with P1/H1, and tool containment vocabulary travels with H2. This keeps wire changes
with their complete producers and consumers. Every affected document's transition
clause remains binding.

## Current evidence

F2 is [merged PR #183][catalog], merge commit 7a8651d8. Its full crate
suite, formatting, lint and lock checks passed. Seven deliberate mutations failed
their protective tests. Independent local review found no unresolved catalog issue.

F1 is [draft PR #184][floor], head 8044c759, aligned with main at 7a8651d8. Its
default and config-feature tests, lint, formatting and lock checks passed. Eleven
guard-removal tests failed as intended. Independent local findings are fixed.
T1a is [draft PR #185][tap], commit 19bf1f56, independently based on bacbad39. Its
tests, lint, formatting and lock checks passed. Eleven guard removals failed their
tests. Review found and fixed a context-bearing projection that could omit block zero.

F3 is [draft PR #186][core], stacked on #184. It supplies atomic statement,
pair, mark and plan filing, bounded structured reads, and verified seed/image ports.
The catalog prerequisite is on main. The shared floor in #184 remains its open PR
prerequisite. The full state crate suite passed, as did lint, formatting and lock
checks. Thirty-seven deliberate guard removals failed their protective tests.
The member channel, paging and capture lifecycle remain F4 work.

T1b is [draft PR #187][recorder], stacked on #185. Its recorder, typed payloads,
capacity reservations and writer wake path passed the trace suite and lint checks.
Sixteen deliberate mutations failed their protective tests. Independent review found
no unresolved defect in that scope. It has no dependency on the shared floor crate.

The serving recall payload needs an exact source/boot representation from its owning
contract before that payload can be implemented. The operator authorized drafting
that amendment for review, now [draft PR #189][recall]. No serving recall constructor
is supplied before approval.

F4 is [draft PR #188][member], stacked on #186. It supplies the
state-flow/1 member, immutable paging, capture cleanup and seed process ports.
The full state suite, lint, formatting and lock checks pass. Twenty-five deliberate
guard removals failed their protective tests. Independent findings in continuation
identity, page publication, canonical encoding and observation integrity are fixed.
Copied and held byte counters remain explicitly uninstrumented. The optional build
without SQLite still has its inherited legacy binary compile failure.

IN1 is [draft PR #190][internal-recall], independently based on main at 7a8651d8.
It supplies the pure recall query with an empty-only refusal and an exact borrowed
subject. Its crate suite, lint, formatting and lock checks pass. Seven perturbations
failed their protective tests, including two type pins. H2 owns the caller and read
integration. No serving change or unused harness dependency is added here.

C1a implements the SPU protocol and binding mechanics. Its scope and the remaining
production dependencies are recorded in the [mechanics note][spu-mechanics]. It adds
no production checker, manifest locator or serving activation. C1 in the later
dependency rows means all three C1 slices, including qualification.
Host and Olympus cuda,gguf suites, lint, formatting and lock checks pass. Fifty
mechanics perturbations and two cancellation-fixture perturbations failed their
protective tests. The GPU pace instrument reports a skip, so no measured-pace claim
follows from that run. Independent review found no unresolved mechanics defect.
The later rows remain owed.
This is a progress record, not a claim that the full goal or any serving cutover is
complete. Each draft carries its own fresh test and perturbation evidence.

The operator supplied docs/ASD-STE100_ISSUE9.pdf as the normative language source.
The inspected copy is Issue 9, January 2025, with SHA-256:

```text
d1f4ea9e7cd6e46b47aa9057209f99e78c0e9cfc4e27a5b07895b05c1a166431
```

Locating this reference does not qualify the project dictionary, conformance corpus,
span-binding classifier or system prompt. Their evidence remains required by Spec 02
and the foundation cutover. The PDF is not added by this implementation slice.

## Cutover gate

The complete existing serving implementation remains until one code PR retires it
and activates the qualified replacement. Its body must supply the foundation's four
evidence items: tested revisions and tuple, pinned language and reviewed prompt
artifacts, measured classifier/limit selections, and the checked retirement map.
There is no deployment switch for an incomplete replacement.

Measured values remain open until measured on the intended hardware. Local fixture
tests prove mechanics and refusal paths. They do not certify production language
quality, decoder capacity, throughput or absence of paging waits.

[set]: ../crates/state-management/README.md
[catalog]: https://github.com/toddwbucy/WeaverAgent/pull/183
[floor]: https://github.com/toddwbucy/WeaverAgent/pull/184
[tap]: https://github.com/toddwbucy/WeaverAgent/pull/185
[core]: https://github.com/toddwbucy/WeaverAgent/pull/186
[recorder]: https://github.com/toddwbucy/WeaverAgent/pull/187
[member]: https://github.com/toddwbucy/WeaverAgent/pull/188
[recall]: https://github.com/toddwbucy/WeaverAgent/pull/189
[internal-recall]: https://github.com/toddwbucy/WeaverAgent/pull/190

[spu-mechanics]: state-management-spu-mechanics-2026-10-20.md
