# State management implementation map

Date: 2026-10-20. Status: implementation tracking, not a Spec amendment.

The authority is the merged [state management set][set] from PR #182. All code PRs
start as drafts. Gates and Planner verification precede third-party review.
The operator merges them.
The crate remains weaver-state. The loop remains in worker/dev_loop and calls
primitives. The SPU owns classification.

## Consolidated review stack

On 2026-10-21, the completed slices were folded into three roots. This landing
map takes precedence over the historical slice descriptions below. A merge into
a review branch does not mean the change has reached main.

| Root or held branch | Contents and next step |
| --- | --- |
| #184 | Shared floor, store/member via #212, admin #193 through #196, and SPU #191 with #197. Complete consolidated gates and review, then the operator lands this root on main. |
| #185 | Trace copy and recorder, including #187. Complete consolidated gates and review, then the operator lands this independent root on main. |
| #198 | Harness #199 through #210 and model repair #214. Fix the inherited #203 observation-tick finding. After #184 and #185 land, retarget from code/state-member-flow to main and check the combined head. |
| #190 | Independent pure recall query primitive. Its review and landing do not depend on the three roots. |
| #215, #216, #217 | Typed tool causes, external pair execution and paged reads. Keep the descendant stack held for the coupled cutover. |
| #218, #219, #220 | Paired decode wire, owned measurement and outbound identity sort. The #219 union preserves both #217 and #218. Keep that union and its descendants held for the coupled cutover. |
| #189, #192, #213 | Separate Spec proposals. Approval and merge remain owed, so they are not implementation authority. |

No further descendant needs folding into #184 or #185 before their review. Keep
the paired wire out of either additive root. Preserve merge ancestry rather than
squashing the joined stacks. The old serving path remains active until the qualified
foundation cutover. The full implementation and epic #146 remain open.

For #184, the contained seed fixture that launches the real member under root-owned
host containment remains an open instrument. The reviewed scripted seed fixtures
do not replace it. Production language artifacts and measured limits also remain
unqualified. These limits stay explicit in the consolidated PR body.

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
| P1a | F1 | Serving declaration, canonical artifacts and verified reads, admin/types amendments |
| P1b | P1a | Staged artifact publication and restore append, admin/persistence amendments |
| P1c | P1a, P1b, F4 | Contained seed driver, admin ADMIN-SEED and TYPES-SEED |
| P1o | P1a, P1b | Offline bind and restore drivers, admin selection/rebind clauses |
| P1d | P1a, P1b, F4, C1 | Serving lifecycle and verified boot, admin/persistence amendments |
| H1 | F1, T1b, F4, C1, P1 | Flow, receipts and halt, harness/foundation amendments |
| H2 | H1, IN1 | Pair ledger and gate containment, Spec 05 and gate amendment |
| H3a | C1a | Shared SPU rendering and read-only placement counts, SPU amendment |
| H3b | H1, H3a | Marks and paired decode protocol, Spec 06 and SPU amendment |
| H4 | H2, H3b | Plan operations and review evidence, Spec 07 and admin amendment |
| H5 | H3b, H4 | Sort, selection and injection ports, Spec 08 |
| Q1 | All above | Serving qualification and coupled cutover, foundation transition |

Spec 09's required evidence producers travel with the primitive that produces each
measure. Evidence analysis remains in WeaverAnalysis. External diagnostic contracts
are amended through the owning repository's review path.

The first two slices are independent and additive. F2 is the complete empty catalog,
not a serving store or valid seed. F3 joins both branches and supplies the complete
state-content/1 behavior. No partial catalog is a deployment option.

F1 does not replace the legacy lifecycle/configuration grammar. P1a adds a separate
serving parser without activating it. Replacement lifecycle changes travel with
P1d/H1, and tool containment vocabulary travels with H2. This keeps wire changes
with their complete producers and consumers. Every affected document's transition
clause remains binding.

## Current evidence

F2 is [merged PR #183][catalog], merge commit 7a8651d8. Its full crate
suite, formatting, lint and lock checks passed. Seven deliberate mutations failed
their protective tests. Independent local review found no unresolved catalog issue.

F1 is [draft PR #184][floor], head 4770909f, aligned with main at 7a8651d8. Its
default and config-feature tests, lint, formatting and lock checks passed. Eleven
guard-removal tests failed as intended. Independent local findings are fixed.
T1a is [draft PR #185][tap], head f76260ed, independently based on bacbad39. Its
tests, lint, formatting and lock checks passed. Eleven guard removals failed their
tests. Review found and fixed a context-bearing projection that could omit block zero.

F3 is [PR #186][core], stacked on #184. It supplies atomic statement,
pair, mark and plan filing, bounded structured reads, and verified seed/image ports.
The catalog prerequisite is on main. The shared floor in #184 remains its open PR
prerequisite. The full state crate suite passed, as did lint, formatting and lock
checks. Thirty-seven deliberate guard removals failed their protective tests.
Its current head is 82a4a436 after the review fixes and floor refresh. The member
channel, paging and capture lifecycle are in F4.

T1b is [PR #187][recorder], stacked on #185. Its recorder, typed payloads,
capacity reservations and writer wake path passed the trace suite and lint checks.
Sixteen deliberate mutations failed their protective tests. Independent review found
no unresolved defect in that scope. It has no dependency on the shared floor crate.
Its current head is e2252766.

The serving recall payload needs an exact source/boot representation from its owning
contract before that payload can be implemented. The operator authorized drafting
that amendment for review, now [draft PR #189][recall]. No serving recall constructor
is supplied before approval.

F4 is [PR #188][member], stacked on #186. It supplies the
state-flow/1 member, immutable paging, capture cleanup and seed process ports.
The full state suite, lint, formatting and lock checks pass. Twenty-five deliberate
guard removals failed their protective tests. Independent findings in continuation
identity, page publication, canonical encoding and observation integrity are fixed.
Copied and held byte counters remain explicitly uninstrumented. The optional build
without SQLite still has its inherited legacy binary compile failure. Its current
head is 9e3dab9c. The narrow snapshot timing amendment remains a proposal in
[draft PR #192][snapshot]. Its ruling and merge are still owed before the matching
sampling change becomes implementation authority.

IN1 is [draft PR #190][internal-recall], independently based on main at 7a8651d8.
It supplies the pure recall query with an empty-only refusal and an exact borrowed
subject. Its crate suite, lint, formatting and lock checks pass. Seven perturbations
failed their protective tests, including two type pins. H2 owns the caller and read
integration. No serving change or unused harness dependency is added here.

C1a is [PR #191][spu], stacked on #184. It implements the SPU protocol and
binding mechanics. The [mechanics record][spu-mechanics] and
[PR description][spu] name its evidence and remaining production dependencies.
It adds no production checker, manifest locator or
serving activation. C1 in later dependency rows means all three C1 slices, including
qualification.
Host and Olympus cuda,gguf suites, lint, formatting and lock checks pass. Fifty
mechanics perturbations and two cancellation-fixture perturbations failed their
protective tests. The GPU pace instrument reports a skip, so no measured-pace claim
follows from that run. Independent review found no unresolved mechanics defect.
Its current head is 22cf33f4 after the shared-floor refresh.

P1a adds strict draft/bound declaration parsing, admin-owned canonical TOML,
strict binding/index/guard codecs and descriptor-held artifact verification. It is
stacked on #184, independently of #188. Its read half returns the checked copy and
read-only image descriptor, refuses unindexed or guarded artifacts and retains N6.
It does not append a restore selection, publish a save, spawn a seed member or
activate the serving path. Publication and restore append are P1b. Seed process,
boot and the existing locks with complete protocol producer/consumer integration
remain P1c and P1d. Offline callers are P1o. P1 in dependency rows means all five
slices.
P1a is [PR #193][artifacts], head 7aa326d9. Its 45 new admin tests pass within
the full admin suite, as do the floor's default and config-feature suites. Fourteen
compiled guard-removal probes failed their protective assertions. Independent
review found and closed an inherited ACL-check omission and an overlapping refusal
in a test fixture. None of this evidence certifies the later publication protocol.

P1b is [PR #194][publication], stacked on #193. It supplies the
descriptor-held publisher and indexed restore selection. The publisher copies a seed
before channel close and requires successful member cleanup before publication.
Serving copies require the matching finished answer and durable guard before
Prepared, matching Commit before rename,
and a matching final outcome before guard removal. Rebind preserves the image and
scheme. Publication advances through rename, read-back and strict append with caller
checkpoints around each step. No filesystem result stands for a worker outcome.
The full admin suite passes 429 tests, with 33 inherited ignored instruments and
all 53 new tests running. Thirteen compiled guard removals failed their protective
checks, then restored exact source bytes. The isolated publication module has no
remaining finding from independent local review.
P1c supplies seed process, lock and cleanup callbacks. P1d supplies serving lifecycle
callbacks, including the common windows and peer observations. P1b adds no active
lifecycle grammar.

P1c is [draft PR #195][seed-driver]. Its dependency join combines
#194 and #188, so both must merge before this branch. It supplies the contained
seed driver and its preparation, transport, cleanup and publication sequence.
Admin verifies certificate attribution and exact bytes without starting a checker.
The separate qualification tool and production artifacts remain cutover gates.
This slice does not activate a seed command, change serving load, or complete P1d.
The full admin suite passes 474 tests, including 45 new ordinary tests, with 36
ignored instruments. All-targets lint, formatting and lock checks pass. The separate
credential-drop watch and delegated-cgroup cleanup instrument pass. Fifteen compiled
guard removals fail their protective tests and restore exact source bytes.
Review found and closed premature member exit, a fresh lock-wait budget, and reseeding
beside artifacts whose index was lost. Seed admission and publication both refuse that
last case without recovering or promoting the orphaned files.
The combined real-member seed fixture requires a root-owned delegated host scope. It
is written but unrun because passwordless root is unavailable here. Its attempted
user-namespace substitute correctly refused unmapped host-root ancestors, so that
substitute was removed. Production host admission, the joined root-host fixture,
qualified certificates and serving boot remain owed before cutover.

P1o is [draft PR #196][offline-controls], stacked on #195. It adds
the complete dormant bind-state and restore operations.
Both authenticate before reading fixed inputs, hold the invocation and run locks,
and require the existing cgroup ladder to report unloaded. Preparation judges the
fixed selector and territory draft without seed inputs or a live checker. Bind
preserves the image and scheme while publishing a new bound declaration. Restore
appends a verified indexed selection without rewriting its files. Append uncertainty
and publication evidence remain in the typed failures. Command activation, stdout and
admin.log adaptation remain part of the coupled cutover.
The full admin suite passes 496 tests, with 36 inherited ignored instruments. All
22 new offline tests run. Fifteen compiled guard removals fail their protective
checks, and the restored source passes all 22 again. All-targets lint, formatting,
lock and diff checks pass. Independent source review has no open finding.
The fixtures use actual filesystem publication and locks with synthetic host
inventory and image bytes. They do not certify SQLite, root/NSS provisioning or
a populated delegated cgroup. Existing containment instruments own that last duty.

H3a is the draft slice on code/state-spu-placement, stacked on #191. It adds
one complete generation renderer shared by the existing append path and immutable
placement measurement on the existing open session owner. The session supplies
checked actual prefix, resident and capacity counts. The open owner adds the resolved
generation limit and single terminator count. Measurement retains no proposal,
permits oversized counts and preserves malformed-render and component-fault outcomes.
The host suite reports 229 passes. The required Olympus cuda,gguf suite reports
295 passes, with the pace instrument explicitly skipped. Both all-target lint gates,
formatting and lock checks pass. The new real-model tests use Qwen2.5-0.5B through
GGUF and native CUDA and check the actual legacy decode socket as well as the
internal operation. Registry fixtures cover every admitted renderer's bytes. These
are not throughput, deployment-capacity or all-model qualification results.
Fifteen compiled perturbations fail their protective assertions. Six cover rendering,
three session bounds, two preparation failures and four the real GGUF measurement
and append path. Exact source bytes are restored and the corresponding clean
baselines pass. Independent source review has no open finding.
This completes the internal SPU primitive, not its replacement socket protocol.
H3b must change the paired floor, harness and diagnostic consumers together. It also
owns marks and the harness's between-turn grant. The existing serving wire and loop
remain active until the coupled cutover.

H3b has a named open-measurement gap. Spec 06 OPEN-FACTS requires an identity-prefix
label at open, while TYPES-DECODE specifies only four counts in Opened and leaves
Generation unchanged. The existing SPU Spec section 6 emits no open measurement.
No implementation may invent the missing carrier. The existing PromptPartition
offset validator is also not wired into production measurement. Its unit tests
cannot establish the section 6 tokenizer-offset requirement. These remain explicit
work for the SPU slice, not evidence supplied by its binding mechanics.

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
[spu]: https://github.com/toddwbucy/WeaverAgent/pull/191
[spu-mechanics]: state-management-spu-mechanics-2026-10-20.md

[snapshot]: https://github.com/toddwbucy/WeaverAgent/pull/192

[artifacts]: https://github.com/toddwbucy/WeaverAgent/pull/193

[publication]: https://github.com/toddwbucy/WeaverAgent/pull/194

[seed-driver]: https://github.com/toddwbucy/WeaverAgent/pull/195

[offline-controls]: https://github.com/toddwbucy/WeaverAgent/pull/196
