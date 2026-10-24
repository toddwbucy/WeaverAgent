# Dormant STE100 duty runtime

This slice advances #146 and answers #259 Planner review 5622813219 rows 1
and 10, #264 review 5624067784 rows 1-10/T9, and #266's shared-codec row 9.
It is stacked above exact #271, head
9ca28702d04e17b55570189b6fda06cd927570c4, tree
4c7f2304ca51f1afd40716fc5510d451eb779328. The source archive SHA-256 is
12ee67f9b13fa9de87b0a40b91490f1fb28aac52e0cc667872f904da43ab3a67.
Each of the 22 runtime paths matched its reviewed immutable base before
integration. Parent harness and worker serving source remain unchanged. Earlier
source snapshots and independent reviews remain in the evidence deposit.

The shared ModelBinding decoder does narrow live admin TOML parsing: positional
arrays such as model-binding = ["/m", [0]], previously accepted by serde, now
refuse. Canonical object bindings remain supported. The refusal remains BadValue,
although its type-mismatch text now says "expected a JSON object". Planner
review of #272 at 044d1049 names this as finding 10. No serving activation follows.

## Authority and boundary

| Implements | Author source assessment |
| --- | --- |
| Spec 02 PROTOCOL-OWNER / CHANNEL | Conforms for the separate executable, exact launch arguments, single inherited fd3, and existing connected SEQPACKET framing. Worker launch, credential, load-budget and reaping composition remain held. |
| Spec 02 DECLARATION / MANIFEST-LOCATOR and SPU amendment section 6 | Conforms for the opaque validated-bootstrap handoff into actual declared-path acquisition and retained byte ownership. Actual grammar/model compatibility and measured qualification remain dependencies. |
| Spec 02 PORT / PORT-SERIALIZATION and STE100 contract section 2 | Conforms for one whole request, a sole socket reactor during held work, actual immutable admission identity, one whole reply and terminal disposal. No sentence scheduler or instance policy is elected. |
| FOUNDATION-TRANSITION | Conforms: the current serving implementation remains active. This standalone executable is not installed into the worker's serving launch path. |

The normal executable has no compatible compiled grammar/model adapter. It
answers an actual unsupported_artifact admission refusal and exits; it never
chooses a test adapter, the legacy label scorer or a synthetic Ready. Invalid
bootstrap/path inputs retain their existing typed admission refusals. Test-only
success requires the separate cfg(test) constructor and actual declared-file
acquisition. Those test bytes are not qualified production artifacts.

## Ownership and behavior

`weaver-spu-ste100` accepts exactly `--max-wire-bytes <positive-decimal>`.
Before any worker or artifact acquisition, it adopts the only nonstandard
inherited descriptor at fd3, sets CLOEXEC and clears dumpable. The STE100 Channel
then verifies a connected Unix SEQPACKET end. Existing decoder and legacy label
binaries keep their own contracts; the label binary reuses the same fd hygiene
helper, with no new framing or classifier mapping.

The dispatcher decodes and validates the bootstrap once against the launch
limit. Its opaque ValidatedBootstrap moves into declared-path admission without
serialization or a second permissive parse. The dispatcher retains the full
original parsed identity. Only actual Admitted can mint the private admission
identity used to queue Ready, and every original field must match, including
ones absent from Ready's visible echo.

A single SPU-local worker constructs, uses and drops the backend. Its narrow
operation accepts one immutable Ste100Request and returns one whole Ste100Result.
No backend extraction, mutable backend accessor or arbitrary caller closure is
introduced. The backend may be non-Send: its checked files, manifest, scheme and
compiled state remain on that worker through the last read and destruction.
This thread is separate from the harness and does not add a harness runtime.

The reactor alone owns the duty socket and both framing cursors. Bounded local
command/completion channels and a separate completion wake descriptor connect
the worker. The reactor observes socket input before accepting a completion;
the dispatcher repeats the overlap check immediately before queuing a result.
A first overlapping fragment is enough to fail, even while admission or model
work is held. Readiness polling watches the socket throughout work and watches
writability only when a complete owned response is pending.

A terminal component result attempts its one correlated reply and stops. A lost
worker during checking supplies no_result, never language rejection. A lost
admission worker supplies unsupported_artifact, never Ready. Protocol or peer
failure closes the socket and retires both local handoff directions; a late
worker cannot publish another answer. Worker completion sender disconnection
precedes its final wake, including unwind, so the reactor cannot consume that
wake while the completion channel still appears live.

Disposal never joins an unfinished backend call on the socket owner. A detached
worker retains its actual backend and verified bytes until its work finishes,
then drops them on its own thread. Executable exit or the parent's existing
containment/kill/reap boundary ends a permanently stuck call. No new timeout,
retry budget, successful cancellation verdict or process closure claim is
invented. A finished worker can be joined without waiting.

## Caller census and remaining composition

The executable's entry is the sole production Runtime constructor. Runtime is
the sole production consumer of Action::Bootstrap, actual path admission and
the private backend check operation. Public standalone artifact acquisition
keeps its existing validation boundary. Supplied-byte admission and the fixture
path remain absent from production builds. The dispatcher public Admitted
adapter uses the same private whole-bootstrap comparison as the runtime.

Worker main, OrganBinaries, admin launch and the active harness lifecycle still
provision the old optional label duty. The dormant RecordingContext and
SocketPort already own the new Bootstrap and Loaded inventory but do not launch
this executable. Final composition must start the exact duty with its remaining
load bound, retain its process custody, complete mandatory template checks, and
install the resulting genuine Ready before boot. No caller-supplied Boolean,
cloned inventory or independent successful primitive substitutes for that proof.

O02-1 through O02-5 remain open: pinned licensed language scheme/dictionary/corpus,
real grammar compiler, elected seven-column aligned span-model adapter and exact
instance count, separately reviewed system prompt, and measured CPU/device,
retained-memory, message-size and operation bounds. The supplied Issue 9 PDF
and these mechanical tests do not qualify any of those artifacts. Independent
sentence scheduling and source-order assembly belong to that actual adapter;
this slice tests only transport of an injected complete ordered block result.

## Review followup dispositions

| PR #259 Planner row | Final source disposition |
| --- | --- |
| 1 | Action::Bootstrap now transfers an opaque validated owner directly to the runtime's declared-path admission job. The production route has no raw-byte reparse, and every acquisition route rechecks the instruction before path access. Full dispatcher identity remains checked before Ready. |
| 10 | The retained owner now supplies the actual SPU-internal whole-block check operation. Worker construction, use and drop preserve non-Send backend affinity and verified input custody. No public bare-backend or mutable escape is added. Actual grammar/model implementation remains O02-1/O02-2. |

## PR #264 review dispositions

| Row | Final source disposition |
| --- | --- |
| 1 | Bootstrap, instruction, repair-cap and model-binding use the shared closed-object decoder. Direct and nested positional-array aliases, duplicate/unknown/missing fields and valid controls are covered. This is codec consistency, not a new bootstrap policy. |
| 2 | An unavailable root permission lane fails unless WEAVER_STE100_ALLOW_PERMISSION_TEST_SKIP=1 explicitly elects the skip. The diagnostic states that no permission-boundary result was obtained. A normal non-root run still exercises the actual unreadable file. A skipped root lane is not reported as enforcement evidence. |
| 3 | The internal compatibility entry validates the exact instruction before opening even its parent directory. It has no comment-only prevalidated bypass. The dispatcher still transfers the opaque full ValidatedBootstrap into admission. |
| 4 | Every referenced regular file, as well as the manifest, receives a same-descriptor fixed-scratch digest precheck before retained allocation. A wrong large or sparse file cannot request metadata-sized materialization. No size cap is invented. A digest check only while growing a retained buffer would leave that allocation problem open. |
| 5 | The three scheme-only fixture markers are rejected in verify_scheme before any manifest access. Manifest family/alignment markers remain checked before the first reference open. Each marker has a separate missing-file control. |
| 6 | The floor returns both decoded blobs from the same complete scheme validation. SPU hashes and retains those exact vectors. The production path no longer calls validate, verify_blobs and decode in succession. Compatibility and grammar/model qualification remain separate duties. |
| 7 | Retained reads update SHA-256 in the same chunk loop that owns the bytes. The manifest is parsed once after its retained digest matches. Repeated references compare each declared digest to the cached actual digest and share the original allocation, with no extra payload hash or path reopen. |
| 8 | RuleDisposition serialization and deserialization use exhaustive matching for the same two spellings. T9 now refuses unknown disposition strings including technical. |
| 9 | Clearing O_NONBLOCK masks the raw integer before from_bits_retain and preserves every other status bit, including bits absent from this nix version. The type check still precedes blocking file reads. |
| 10 | Named scheme and model objects use the existing shared closed_object helper, with a single ordered field list. This retains unknown/duplicate/missing and map-only checks without a new wire shape. |
| 11 | The published parent already labels the implementation-map row #264. That correction is retained. |

The same shared-helper act answers PR #266 review row 9. Its supported
container rename_all spelling reaches both writer and closed map reader. Other
container serde attributes are rejected structurally, and required-nullable
field attributes remain shared. Existing plan consumers keep their shapes.

The fixed-memory precheck and retained read deliberately remain two passes over
one acquired descriptor. The first rejects wrong large inputs before allocation.
The second hashes the bytes being retained and refuses in-place changes between
passes. Only the second-pass owner can reach parsing or backend admission. A
matching artifact's memory cost remains an O02 qualification duty.

The runtime has no production adapter and cannot turn these admission mechanics
into Ready. Neither the closed codecs nor test-only adapters elect a model,
grammar ABI, sentence scheduler, artifact limit or production language policy.

## Author validation

The new instruments use actual declared files, SEQPACKET pairs, controlled held
admission/check work, non-Send backends, weak byte owners and child exec. They
cover exact fd/argv boundaries; no production Ready; peer/overlap failure during
held work; same-iteration completion races; correlated accepted/rejected/component
outcomes; pending writes; wakeup/disconnection; and input lifetime through drop.
The original runtime instruments contain 16 runtime tests, seven process-entry
tests and one manifest instrument. This review also adds acquisition, decoded
blob, bootstrap object and disposition-code tests. Existing strict codec,
manifest acquisition and transport suites remain required.
The manifest instrument pins all three executable names/paths and the old
classifier's cuda gate without adding a new top-level fixture target.

Author gates pass: types 168 default and 224 config; SPU cuda,gguf 380;
admin 498 with 37 inherited ignores; harness 1,694 with 18 inherited ignores;
both changed-crate all-target Clippy gates with warnings denied, formatting,
lock and diff checks. Gates ran serially on Olympus with a private target,
lock first and --locked. The SPU lane includes the actual native and GGUF
two-card model regressions. These decoder regressions do not qualify a STE100
model or grammar. The permission-lane skip was not elected.

The first SPU compile caught a test helper's incorrect nested module path;
its import was corrected without changing production. Lint then required
renaming the private Progress enum to RuntimeProgress. The first mechanical
rename also touched an unrelated SendProgress reference in a test; compilation
caught it and the reference was restored. All initial logs are retained.
Independent comparison confirms the final three-file change is only the private
type rename and import ordering. The 124 affected STE100 tests and SPU lint
pass afterward; the full GPU result predates that identifier-only change.

All 50 protective controls compiled and failed their intended assertions:
48 production-behavior probes, one binary-manifest structure check and one
permission-test reporting check. Every exact restored control passed, and all
21 changed source-file hashes match the final tested source after restoration.
The existing capability compile-fail doctests are separate type/API checks.
No compiler failure, command timeout or missing test is counted as a behavioral bite.

Independent author source, integration and formatting reviews are retained
beside exact source hashes, all initial failures and final gate logs. Author
checks are not Planner grades or production qualification. #269's two Stop
fixes, the remaining #270/#271 review followups, active worker/lifecycle wiring
and the qualified coupled cutover remain separate work. #260/#263/#268 remain
unmerged and unimplemented. Existing serving remains active.
