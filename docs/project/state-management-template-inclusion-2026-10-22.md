# Private finite-state inclusion kernel

Date: 2026-10-22.

Branch: code/state-template-inclusion.
Epic: #146.

This independent branch starts from merged main c2646e2. It registers no runtime
consumer and supplies no production grammar or classifier artifact.

## Authority and precise scope

Implements: Spec 02 TEMPLATES, the finite-state product-emptiness algorithm only.
Author grade: conforms for this algorithm body on the checks below.
This does not complete TEMPLATES, O02-1 or C1b.

The owning clause elects intersection of the template language with the grammar's
complement. `inclusion.rs` computes that intersection for two checked, total,
deterministic machines. It visits every reachable product state unless it first
finds a concrete counterexample. An accepting template state and a nonaccepting
grammar state produce that witness. Exhausting the reachable product proves
inclusion for these two machines, including cycles and the empty word.

The kernel does not define what a machine means. The future compiler must establish
the exact template, admitted grammar and shared token-class meanings. No result
constructs a language reply, Ready, Admitted, TemplateAcceptance or a harness
TemplateCertificate. No wire, artifact ABI, language rule, numeric language limit,
model, filename policy or runtime activation is added.

## Representation and resource choices

The private constructor consumes complete input arrays. A nonempty acceptance array
defines all states. The start and every transition target must be in that array.
The row-major matrix must have exactly states times alphabet-width entries. There
is no partial-transition interpretation. Callers must supply an explicit rejecting
sink when their language needs one.

Alphabet IDs are internal u32 class identities in strictly increasing order. Both
machines must contain the exact same ID vector. This is a canonical internal order
for deterministic witnesses, not a serialized grammar ABI or a claim that an ID
proves semantic equivalence. An empty alphabet is valid and describes only the
empty-word possibility. An unsupported compiler construct is still the compiler's
separate certification refusal duty.

Breadth-first traversal returns the shortest witness, breaking equal-length ties
by ascending class ID. First discovery retains one predecessor per product state.
The predecessor chain yields the concrete class sequence, including an empty vector
for an empty-word counterexample. A class sequence is not source text or a model
diagnosis. A later compiler must map it to a truthful substitution diagnostic.

Dimensions, product indices, allocation byte sizes and witness lengths use checked
arithmetic. The predecessor table, queue and witness use fallible reservations.
Invalid machines, mismatched alphabets, arithmetic capacity and allocation failure
are distinct from valid inclusion and counterexample results. No timeout, sample
count or arbitrary work cap can become a successful proof. The kernel reserves
space proportional to the complete product even when only part is reachable.
Future resource policy can refuse this work, but cannot relabel it as inclusion or
a language defect.

The private allocation seam permits deterministic failure at each allocation in
tests. It supplies no transition, acceptance bit or proof answer. Already allocated
constructor inputs and the fixture builders are outside the kernel's allocation
account. Host termination by an external memory policy is not made recoverable by
`try_reserve_exact` and is not a completed proof.

## Installation and validation

The private module lives in crates/weaver-spu/src/ste100. A narrowly documented
allow(dead_code) applies to its module declaration until the real compiler consumes
this internal algorithm. It is not registered as an admission adapter. All offered
API items are pub(super), and the Dfa fields remain private.

Olympus validation used /tmp/weaver-146-template-inclusion-target with the pinned
toolchain and cuda,gguf enabled. All twelve focused tests and 311 full SPU tests
passed, with no ignored tests reported. Strict all-target SPU lint, formatting
and the lock gate passed. The full suite retains its existing environment-based
instrument selection. That count is not production STE100 qualification.

All twenty protective mutations compiled, failed their named tests and passed
after exact-byte restoration. Source and independent author readings found no
remaining concrete algorithm defect. The code-review skill was unavailable, so
no invocation or Planner grade is claimed. Logs, recipes and source hashes are
deposited under weaver-testing/state-management-146-olympus-2026-10-21/
template-inclusion.

The twenty recipes each replace one exact source anchor and record the installed
source hash. The serial runner separately records compilation, intended assertion
failure and the passing restored test. No mutated source remains in this branch.

## Test coverage and deliberate limits

Twelve tests include all 16,384 pairs of the 128 finite languages over two classes
with words through length two. A separate trie builder compiles each explicit set
to a total machine. Expected inclusion and the shortest witness come from set
difference over the independently enumerated words.

The remaining fixtures cover strict subsets, nonfirst slot alternatives, accepting
and nonzero starts, the empty alphabet, unreachable acceptance, cyclic languages,
a counterexample beyond a short sampling prefix, explicit rejecting sinks and
artificial word and consecutive-cluster counter expansions. Invalid rows, targets,
starts, alphabet order and identity refuse admission. Allocation injection checks
all three allocation sites. Pure arithmetic helpers test unrepresentable sizes
without manufacturing impossibly allocated machines.

These are algorithm fixtures. Their class IDs and counter parameters have no
STE100 or production authority. Missing compiler/lowering, unsupported-construct
handling, grammar ABI, conditions and StepId source syntax, model/span artifacts,
reviewed prompt and measured production limits remain separate work. A qualified
template reply still requires those producers and the existing recorded harness
certificate path.
