# Diagnostic ReFeed interruption producer

**Version:** 0.1, 2026-10-30.
**Parent:** Issue 146, state management implementation.

Implements: weaver-types-Spec section 4.4's interrupted ReFeed carrier and
cancellation constant, weaver-spu-Spec sections 1.2 and 4.6, and
weaver-harness-spu-decode-contract sections 2 and 3's producer obligations.
Merged #393 is behavior authority. This slice is on
code/state-refeed-interruption, based on exact #410 at
5e6d1b663360d304d03826aa7c5cf73ccf592d2d. It retains #408's ratified main
through that parent and advances #146's Diagnostic composition work.

**Do not deploy this paired wire until its harness consumer is complete.**
The next harness act owns entered replay interruption, exact terminal
validation, both exchange completions, trace accounting and the original
cancellation interval. The existing engine still refuses interruption and
accepts only ReFed. No broad completion item closes from this producer slice.
Existing serving remains selected. Foundation selection and coupled cutover
remain separate, qualified acts.

## Behavior and ownership

The floor exports DIAGNOSTIC_CANCEL_BOUND_MS at 60,000 milliseconds and adds
RefeedInterrupted with required turn, consumed, resident and capacity fields.
It has no emission, request or measurement. The existing strict adjacent-tag
encoding rejects absent, null, mistyped, unknown and duplicate members.
The harness must check the original turn, requested cancellation, nonempty
original path and checked consumed + 1 <= resident <= capacity, with consumed
strictly below the original path length. This slice does not claim those
consumer checks or an operating cancellation timer.

Only ReFeed gives SeamCancel its retained turn. A foreign Cancel sends
OutOfOrder and leaves that drive running. Ordinary generation keeps its
existing cancellation behavior. The session retains its existing boundary
poll, recorded-token append and one terminator. The interrupted terminal
uses the original turn, the exact appended path count and the session's
actual resident and capacity facts. RefeedInterrupted precedes AtRest.
The caller then skips complete Generation rendering, parsing and measurement.

A full path that completes before cancellation is polled keeps ReFed. The
queued Cancel then answers AtRest. Engine, malformed-seam and intermediate
send failures remain faults. A failed terminal send cannot report successful
settlement. Field and column frames already sent remain in their original
order. No session or backend algorithm changes in this slice.

| Surface | Owner and consumer |
| --- | --- |
| TokenAnswer and exported bound | Required vocabulary only. Harness arithmetic and timer ownership remain owed. |
| SeamCancel | ReFeed checks the retained turn. Ordinary generation passes no ReFeed identity. |
| finish_refeed | Checks poll/stream failure before accepting the interrupted result and sends both terminals in order. |
| serve_decode ReFeed | The single caller exits the completed-path branch after interrupted settlement. |
| Session::refeed | Existing boundary poll, exact recorded prefix and terminator remain authoritative. |
| engine::refeed | Compiles with the new carrier but still refuses it. The paired deployment hold remains. |
| loaded test fixtures | Both GGUF and native safetensors are declared in the Spec and manifest inventory. |

## Validation

Two floor cases exercise actual JSON bytes in both envelope orders, reader
round trips and required-body failures. Seven socket/session cases use the
production cancel poll, Session drive and terminal sender. Their recording
backend checks exact token IDs and absolute positions, zero and interior
prefixes, retained Column/Field order and failure outcomes. Cancel scheduling
is independent of the intermediate callbacks. The failed-stream case fills
an actual socket queue, observes send failure and restores the channel before
settlement, so a permanently dead socket cannot hide a removed failure check.

Two loaded-child cases run on real CUDA models, GGUF and native Qwen2.5.
They stop and observe the owned child before queuing a request and Cancel,
then resume it. A foreign-turn Cancel refuses before three consecutive Field
positions and a completed ReFed. A matching Cancel after the last Field gets
AtRest after that complete result. Flush restores the original Open prefix.
The repeated request with queued matching Cancel stops at zero, reports the
complete resident count minus exactly three path tokens, and answers AtRest.
A no-op Flush verifies the resulting resident count before Release and reap.
Neither new loaded case skips in the focused or restored runs.

| Compiled control | Intended failure |
| --- | --- |
| R1, default missing consumed | Missing consumed unexpectedly decodes. |
| R2, ignore ReFeed turn | The foreign Cancel is accepted. |
| R3, skip the interrupted terminal | The helper reports no interrupted settlement. |
| R4, increment consumed | Interior-prefix terminal count differs. |
| R5, increment resident | Zero-prefix fullness differs. |
| R6, echo a foreign turn | Terminal attribution differs. |
| R7, send AtRest first | The first terminal is wrong. |
| R8, omit AtRest | The bounded receive cannot read the second terminal. |
| R9, ignore failed stream | A recovered socket permits forbidden successful settlement. |
| R10, ignore malformed poll | A poll failure is accepted as nonfatal. |
| R11, ignore terminator decode failure | The session falsely returns a prefix. |
| R12, omit terminator decode and residency | The exact token/position log is incomplete. |
| R13, increment capacity | Terminal capacity differs. |
| R14, omit Field callback | The next expected intermediate is wrong. |
| R15, omit Column callback | The next expected intermediate is wrong. |
| R16, continue to Generation after interruption | The real loaded child emits a forbidden ReFed before the later Flush answer. |

Every control compiles independently and fails its intended test. Source
restores byte-for-byte, then both floor cases, all seven socket/session cases
and both real loaded cases pass. The subsequent fixture-inventory correction
changes only the manifest test, the Spec row and the loaded file's introductory
comment. Production source and all controlled test bodies remain unchanged.

The final types suite with config reports 264 passed. The full SPU suite with
cuda,gguf reports 415 passed, zero failed and zero ignored, including 22
successful doctests. Its optional pace case explicitly skips because
WEAVER_MEASURE_PACE is unset. The marker/flush audit discloses missing
vocabularies for five fixture entries and an unresolvable llama template.
Those audit gaps are not counted as qualified families. The permission-skip
message comes from the deliberate unavailable-lane unit case, not from the
new loaded fixtures.

Both changed-crate strict all-target lint gates, lock, format, diff and three
DocsMirror checks pass. All harness test targets compile with the new floor
carrier. A final test-only correction constructs both wire envelope orders
explicitly, after which all 264 types tests and strict types lint pass again.
SPU runtime, tests and production consumer code are unchanged by that final
wire-fixture correction. The full harness suite is not claimed here.

The gate uses nightly-2026-02-13, --locked, CARGO_INCREMENTAL=0 and private
target /var/tmp/weaver-state-refeed-interruption-target. CUDA is at /opt/cuda,
with scratch under /var/tmp/weaver-state-refeed-interruption-tmp. The runner
and descendants have eight-CPU affinity. The successful CMake cache retained
its earlier broader architecture set, so the requested 86/89 restriction is
not claimed effective. The two model fixtures are:

```text
/opt/weaver/models/qwen2.5-0.5b-instruct-q6_k.gguf
/bulk-store/models/Qwen--Qwen2.5-0.5B-Instruct
```

Source snapshots, checksums, command arrays, failed cohorts and all controls
are retained. The final commit/archive reconciliation is stated in the PR.
The evidence destination is:

```text
/bulk-store/weaver-testing/state-management-146-olympus-2026-10-22/refeed-interruption-20261030
```

## Review and limits

Manual self-review traces both SeamCancel constructors, the sole terminal
helper caller, the complete-path fallthrough, every TokenAnswer consumer,
fixture prerequisites and the paired-wire hold. A second pass checks the
fixture-inventory correction and its documentation. It also catches a wire
fixture whose Value serialization could test body-first twice. The final
fixture constructs kind-first and body-first raw strings explicitly. The code-review and
verification-before-completion skills are unavailable in this session.
Manual reviews and executed gates are not reported as those invocations.
Planner and subsequent reviews remain required. The PR remains draft and no
CodeRabbit review is requested.

These models exercise the producer. They do not qualify production STE100
artifacts, a span classifier, prompts/templates, limits or complete worker
interruption. The harness's 60-second owner and failed-organ cleanup remain
owed. Token-boundary polling makes no wall-time promise for a stuck kernel.
No deployed agent changes and no production cutover follows from this slice.

## After-action

The new valid wire case first fails the parent at the unknown variant. An
initial malformed-body fixture used Value deserialization, which could fail
on RawValue buffering before reaching field validation. It was replaced by
actual JSON bytes. No malformed-field protection is claimed from that first
red source.

The first CUDA build exhausted /tmp before compiling the new SPU source.
Scratch moved to /var/tmp and the cold build then completed. The first loaded
case requested field depth four, but the actual effective top-k was forty.
The fixture now uses forty. The full crate gate caught the new native fixture
missing from its inventory. Both the Spec row and manifest pin now name it.
Each failed cohort is preserved. The runtime tests and controls do not stand
in for the full inventory gate or for the next harness integration act.
