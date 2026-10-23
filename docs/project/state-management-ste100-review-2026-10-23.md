# STE100 runtime review followups

Date: 2026-10-23. Branch: code/state-ste100-review. This is an implementation
record for issue #146, not a Spec amendment or production qualification.
The exact parent is PR #275 at 8d86a2545e53ab023bd086756b8852c03d07bd99.
The source answers all eleven rows in #272's Planner grade at 044d1049.
The inherited placement and lifecycle owners remain unchanged by this slice.

## Authority and behavior

Implements: Spec 02 compiled-adapter admission and retained protected artifacts;
weaver-spu-Spec descriptor ownership, separate STE100 entry and bounded worker
lifetime; the existing STE100 protocol, digest and model-binding contracts.
Author assessment: conforms within this separate dormant process and its shared
floor rules. There is no new artifact ABI, classifier election or readiness grant.

| Review row | Change and evidence boundary |
| --- | --- |
| 1 | Static absence of a compiled adapter refuses after bounded bootstrap, instruction, scheme, digest and fixture checks but before manifest acquisition. NoCompiledAdapter yields existing UnsupportedArtifact even for a missing or incompatible manifest it cannot use. This deliberately changes only the later refusal detail precedence. Available adapters retain all original acquisition and compatibility checks. |
| 2 | Worker unwind disconnects and wakes the reactor before dropping the admitted backend. An actual blocked backend destructor fixture requires no_result while disposal is still blocked, then observes disposal after release. The backend stays on its worker thread. |
| 3 | The second retained-file pass consumes at most the prechecked byte length plus one sentinel and refuses observed growth or shrink. The same-descriptor digest still rejects equal-length replacement. This is input stability, not a newly elected artifact size cap. |
| 4 | Every safe inherited-descriptor adopter shares one permanent process-wide claim before creating OwnedFd. Invalid count or number consumes no claim. Repeated legacy, label and STE100 adoption cannot create another owner or close the original handle. |
| 5 | The floor owns the shared decoded-blob digest rule. Existing verify_blobs callers retain their decode/hash order; SPU admission validates already decoded bytes without decoding twice. |
| 6 | The unread ValidatedBootstrap launch-limit field and accessor are removed. Constructor validation of the actual launch length and instruction equality remains. |
| 7 | Only a repeated artifact name repeats the retained digest comparison; first acquisition already verified the same digest. No repeated reference is reopened or rehashed. |
| 8 | The manifest integration scan includes both nested entry sources and every file-level feature gate. Three source-instrumentation controls isolate these scanner duties. |
| 9 | Normal retirement and Drop share one disconnect-before-conditional-join method. Poll uses a fixed two-element array, including the ignored negative fd when no worker stands. |
| 10 | Tests document the prior #272 live admin TOML narrowing: positional model-binding arrays are BadValue, with lower serde text referring to a JSON object; named tables and inline objects remain accepted. This slice changes no parser policy. |
| 11 | New controls reach real reply-writability wait, an actual Idle child-process exit and backend-before-grammar-owner disposal. Their exact instrumentation limits are below. |

The shared inherited-fd guard also protects existing decoder/label entry. The
shared digest helper preserves existing harness/persistence callers. The live
ModelBinding narrowing originated in #272's closed-object migration; calling
that earlier change entirely dormant would be inaccurate.

## Tests and controls

Author gates pass: types 168 default and 226 with config; full SPU cuda,gguf
393, including real native and GGUF two-card tests; admin 498 with 37 inherited
ignored cases; harness 1,733 with 18 inherited ignored cases. Both affected
all-target Clippy gates, formatting, lock and diff checks pass. All dependency-
resolving Cargo commands used --locked and one private target serially.

The full suites precede only consolidation of duplicate test-helper module
imports under cfg(test) in lib.rs, required by Clippy. Final focused STE100
133 and inherited-adoption two tests pass after that change, along with the
final SPU lint and format checks. No production behavior or assertion changed.
Reciprocal source reviews cover the original thirteen Rust paths; the final
fourteenth path only centralizes test helpers. All eighteen controls compiled and failed their intended tests, then all exact restored named tests passed. Fifteen check production behavior and three check manifest instrumentation. Fourteen changed plus one additional probed source hash match after restoration.
These author gates and source reviews are not a Planner grade or qualification.
Initial gates found two test-only integration defects: a DecimalU64 value assigned
to the u32 instances field, then an existing direct retained-read test using zero
as its old allocation hint. Both were corrected without changing production code
or weakening assertions. The latter now supplies the independent actual byte
length; every direct caller was checked. Original failed logs are retained.

The actual reply-pressure test drains the worker wake before blocking in the
production wait and releases only socket pressure to wake it. Peer close is
solely a failed-control rescue. The entry test reexecutes one exact test-only
adapter child through the production drive loop, requires its real Ready
handshake, then closes the seam and checks OS success. The production binary
remains NoCompiledAdapter and cannot reach admitted Idle. There is no fixture
selector or successful production-adapter claim.

The destruction-order test uses a test-only Weak token owned by the actual
VerifiedArtifacts. Backend Drop must observe that grammar owner still alive.
This is an ownership-order instrument, not an unsafe read of retired grammar
bytes. File contents continue to be actually read at backend disposal.

The eighteen controls consist of fifteen production-behavior mutations
and three manifest-scanner instrumentation mutations. R1/R7 from the original
Planner grade remain redundant defenses. The prior fifty author controls remain
historical evidence for their exact original source; they are not represented
as rerun on this branch.

## Remaining completion boundary

The full Failed teardown, boot/worker lifecycle composition, serve-side selection,
and measured coupled cutover remain owed. #273's eight and #274's nine followup
rows are tracked separately. #275 also received a zero-fix grade; its nine new
findings remain separate, including a filed Failed-phase witness interleave. #274's new Planner grade has zero fix rows and closes
its earlier #270/#271 rows within the dormant owners; it does not complete the
composer. In particular, untaken held-event custody must be secured before that
composer can rely on release admission.

Approved but unmerged #260/#263/#268 remain unimplemented. Separate draft #276
proposes move-owned author rejection and awaits operator election/review/merge. The production compiled
adapter, pinned dictionary/corpus, span-binding artifact, separately reviewed
system prompt and measured limits remain qualification prerequisites. Existing
serving remains active. Only the operator merges or deploys, and all new PRs
start as drafts. This slice closes neither issue #146 nor the implementation goal.
