# SPU admission ownership review followups

Date: 2026-10-23. Branch: code/state-spu-admission-ownership.
Exact parent: #283 at 1de7990ba6f35ffe9bb7b77ff6a35edff407a80f.
This implementation record advances issue #146 and answers the seven followup
rows on #277 at 65d3b111. It is not a Spec amendment or qualification record.

## Authority and behavior

Implements: weaver-spu-Spec section 2 inherited descriptor ownership and section
11 classify entry; merged SPU state management amendment separate STE100 entry
and worker lifetime; Spec 02 compiled-adapter admission and protected artifacts.
Author assessment: conforms within these entry and admission primitives.

| #277 row | Implementation and evidence boundary |
| --- | --- |
| 1 | The three inherited raw-fd adopters are unsafe APIs with explicit exclusive first-ownership preconditions. The public STE100 run wrapper preserves that contract. All actual exec/test callers carry local safety reasons. Validation still precedes the permanent shared claim; malformed layouts consume no claim. |
| 2 | The source manifest instrument recursively follows declared default, literal-path and inline modules, including common helpers for every consuming target. Canonical paths prevent duplicate visits and detect cycles. Missing, ambiguous, conditional or escaped paths refuse explicitly. |
| 3 | Only the target root's inner cfg contributes its target gate. Child file and inline gates stay local. Disabled descendants remain visible to the fixture inventory. |
| 4 | AdapterAdmission::precheck owns a typed refusal on path, supplied and acquired routes, after the existing cheap checks. Both precheck and final admission use one refusal validator. NoCompiledAdapter owns one exact UnsupportedArtifact refusal used by both methods. |
| 5 | Remove the redundant per-chunk reservation after the complete observed length was reserved. Growth still refuses before extension, shrink refuses at EOF, and the length-plus-one sentinel and same-descriptor digest checks remain. |
| 6 | WorkerState owns Notify as its first field and the admitted backend second. Field drop order disconnects and wakes the reactor before backend destruction can block, including unwind. Construction, checks and destruction remain on the worker. |
| 7 | AlreadyAdopted distinguishes a permanent prior ownership claim from descriptor enumeration/open failures. The exhaustive decoder diagnostic is updated and raw F_GETFD has its safety explanation. |

The raw-number APIs cannot establish unique ownership from a count and F_GETFD.
Their unsafe precondition supplies that fact before OwnedFd construction. After
any successful claim, retries cannot wrap or close live or reused numbers. The
actual decoder/classify/STE100 entry paths parse only argv before adoption.
The STE100 run wrapper cannot become a safe route around this requirement.

Precheck success grants no Ready, grammar compatibility or language certificate.
Existing instruction/binding checks retain precedence. An invalid adapter refusal
is normalized by the same helper as final admission. No production fixture
selector or compatible adapter is introduced.

The manifest walker is a lexical declared-module instrument, not a Rust parser,
macro expander or cfg evaluator. It collects complete WEAVER_* string literals
and skips comments and character/string bodies when finding declarations. Its
feature table compares named feature sets, not Boolean cfg equivalence.
Macro-generated module discovery remains outside this instrument's claim.

WorkerState does not solve a backend that blocks inside admission before it
returns an owner. Existing bounded process retirement, detach/reap duties and
backend-before-retained-input disposal remain separate requirements.

## Validation

Full Olympus SPU cuda,gguf gates pass: 407 tests, zero failures and zero ignored,
including actual native/GGUF two-card and larger-than-one-card model fixtures.
The focused precheck group passes two tests; the recursive scanner group passes
nine. All-target SPU Clippy with warnings denied, formatting, lock and diff
checks pass. All dependency-resolving Cargo commands use --locked and one
private target serially. No source correction was needed after these gates.

All eleven source-pinned controls compile and fail their intended named tests,
then all eleven restored named runs pass. Seven exercise production behavior;
four exercise the source inventory instrument. The runner checks the named
failure, not just a nested FAILED line. Four separate compiler pins produce
exactly E0133 for safe calls to the three adopters and the public STE100 run
wrapper; each exact restoration passes one named admission test. These pins
are separate from the eleven behavioral/instrument controls.

All fifteen source hashes match the tested pre-control snapshot after every
restoration. The incoming source has twelve new test functions and two removed
scanner tests (net ten), plus four new compile-fail docs. The removed scanner
expectation incorrectly promoted a child gate to the target. Reciprocal source
reviews cover the original thirteen runtime and two manifest paths separately;
root applied all fifteen exact preimages and only rustfmt changed their layout.
There are no surviving controls, invalid probes or initial failed gate runs.

The descriptor controls use real isolated same/cross/reused-fd processes. The
worker control reverses structural field order while an actual backend
destructor blocks after panic. The precheck tests retain exact valid/invalid
adapter refusal and earlier cheap-check precedence. These establish mechanics,
not a production language classifier or compatible production Ready path.

## Remaining boundary

Fifteen source paths join above #283 with exact preimages; later harness and floor
changes remain intact. The original #277 eighteen controls remain evidence for
that original source. Its static-availability and declaration-order recipes are
superseded here by adapter-owned precheck and structural field-order controls.
The redundant reserve removal needs no invented behavior-change mutation.

This slice changes shared decoder/classify entry APIs as well as the dormant
STE100 process. It does not activate state management serving. Production still
has no compatible STE100 adapter. Actual lifecycle refusal/rollback composition,
review followups on the harness owners, production language artifacts, reviewed
system prompt, measured limits and coupled cutover remain open. The separate
#260/#263/#268/#276/#282 proposals remain unimplemented. Operator alone merges
and deploys; new PRs start as drafts, and only the operator requests CodeRabbit.
Author gates and source audits are not a Planner grade or full-goal completion.
