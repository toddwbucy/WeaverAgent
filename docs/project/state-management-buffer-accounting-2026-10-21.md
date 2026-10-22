# State management buffer accounting

Date: 2026-10-21. Executor implementation record for issues #221 and #146.
Branch: `code/state-buffer-accounting`, stacked on draft PR #233 at
`284f73f`. Prior author and Planner gates passed. The review followup below awaits
its new gates and protective checks.
This record claims no production qualification, serving activation or issue closure.

## Authority and implemented scope

Implements: state-management-spec-09-qualification-costs STORE-OBSERVATION and
SNAPSHOT-SAMPLING, scoped to local copied_bytes and held_bytes in the additive serving
member. The merged definitions and observation/2 timing remain unchanged. Author grade:
conforms for the implemented scoped clauses, not a Planner grade.

held_bytes now measures the complete live owner inventory below on every observed
serving reply, including open refusals and control refusals. copied_bytes is complete
for Next, Release, Held, Position, Grants, Snapshot, Acknowledge, Abort and
ReleaseCapture. Open, File, FilePair, Read and Shape remain null with uninstrumented.
Their adoption, content validation and query-decoding copies require the remaining
#221 act. In particular, Shape's encoded rows are decoded into owned strings before
answer encoding; its partial encoder subtotal cannot claim that category is complete.
A partial subtotal never becomes an available total merely because a later buffer was
measured. Seed mode does not acquire a serving operation observation in this act.

The colon list in Spec 09 describes payload categories. This implementation counts
owned decoded raw buffers, encoded operation/result/page buffers and captured images.
It does not turn copied_bytes into total allocation traffic. Parser keys, scalar-ID
strings, enum labels, diagnostic strings, filenames and hash bookkeeping are excluded
as independent copies. They still count when present inside an actual raw or encoded
payload buffer. The in-flight Common and filing source ID strings are counted as
live holdings, even though their creating parser/ID clones are outside the copied
payload categories. Thus the copied and held definitions intentionally differ;
neither is an allocation counter.

## Actual buffers and their lifetimes

| Owner | copied_bytes | held_bytes at ended_ns |
| --- | --- | --- |
| Complete receive line | Framing predates began_ns and is not a decoded copy | Actual current Vec length, including LF |
| Channel read-ahead | Receiving bytes is not a decoded copy | Actual buffered tail length, including a pipelined request |
| Operation hint | RawValue values borrow the original frame | No retained hint owner |
| Strict floor decoder | Exact sum of its owned outer RawValue lengths | Decoder temporaries are gone before dispatch completes |
| Typed request | Deeper Text/raw/context copies remain uninstrumented for filing, adoption and content Read | Dispatch consumes it. Only the current Common and optional filing SourceBlockId strings remain |
| Open echo | Adoption copies remain uninstrumented | The separate member identity remains until metadata insertion. read_limits is session configuration and excluded here and later. The typed echo drops before finish |
| Query Output | Optional local tally counts actual writes, including a refused prefix. Serving Read skips this subtotal while decoded copies remain uninstrumented | Complete query Vec moves into Pages without copying |
| Shape Output | Optional local tally counts its encoded array, including a refused prefix. Serving Shape skips this subtotal while decoded row copies remain uninstrumented and the total is null | Temporary encoding and decoded rows drop before finish |
| Pages | Each base64 page is one new buffer | The complete frozen result remains through the final page until release, never only its unsent suffix |
| Snapshot image | One successful SQLite-to-Rust Vec copy, even if the room write later refuses | Image Vec drops before answer completion. A room file is not a live image buffer |
| Capture continuation | No second image copy on acknowledge, abort or release_capture | Held filename, plus the Finished admin-hash string. Abort and release remove these owners |
| Payload encoder | Actual written bytes in every attempt, including a failed prefix and fallback | Final encoded payload remains. Consumed typed Reply is explicitly dropped before ended_ns |
| Sizing probes | Excluded wrapper/metadata sizing, not an actual result or page payload | Probe buffers are dropped before finish |
| Observation and final wrapper | Produced after ended_ns, outside this sample | Not yet constructed at the boundary |

The fixed Session/store configuration, SQLite heap, allocator capacity and kernel
queues are outside the held definition. The open line, encoded payload and identity
are explicitly dropped after their last use so they do not silently survive into later
operations. The current request line stays owned through its response. No extra copy is
charged for moving a query Vec into Pages or borrowing an image for the room write.

The shared floor's observed decoder uses the same strict frame implementation as the
ordinary decoder. Its count is explicitly one category, not an operation total. It
keeps original raw spelling, duplicate-key checks, bounds and direction checks.
Member hint parsing now borrows RawValues instead of creating another raw copy.

BufferBytes uses checked addition. Overflow produces null with overflow and does not
change the operation result. Missing coverage stays null with uninstrumented. Holding
is assembled after any AnswerTooLarge cursor cleanup and after payload encoding.
The existing CPU/disk sampling and observation/1 historical acceptance are untouched.

## Instruments

The new tests use literal byte lengths, actual decoded wire payloads and actual owner
lifetimes. Production never serializes a value merely to estimate its memory size.

- The shared wire fixture distinguishes escaped raw spelling from equal decoded IDs
  and checks strict refusal guards on the observed decoder.
- A real serving-process fixture holds a complete read while snapshot, preflight
  refusal, acknowledge and release_capture run. It checks one image copy and only the
  live filename/hash, then checks read release removes the whole frozen query.
- Another process fixture checks zero-ID preflight, abort and a real create_new room
  collision. A failed write retains the image copy count and leaves no held image.
- A real filing with escaped Unicode retains explicit missing copy coverage. Empty
  and populated Shape replies also require null/Uninstrumented, while Held, Position
  and Grants verify complete control totals and actual request owners.
- Next uses the actual Session dispatch and page encoder. It counts a new base64 page
  once, retains the full frozen result and ignores spare Vec capacity. Its exact
  tally excludes dummy-page and wrapper-header sizing buffers.
- A real socket framing fixture retains two pipelined frames and checks the buffered
  tail contributes until consumed. It does not assume bytes remain in a kernel queue.
- An oversized Position payload checks the exact prefix written before encoding
  refusal, the fallback bytes and holding after initial-page discard or cursor retain.
  Only actual payload attempts count, not a wrapper sizing probe.
- Engine fixtures check complete and refused query/shape encoder writes. Overflow
  checks verify null/overflow with an otherwise unchanged valid answer.

Existing capture timing, historical observation, page protocol and partial-send tests
remain required regression gates. The partial-send fixture still concerns carrying an
already finished observation, not host transport timing qualification. No test above
qualifies a production memory limit or makes deeper uninstrumented paths complete.

## Validation

Serial Olympus checks used /tmp/weaver-146-accounting-target, --locked and the
lock gate before Cargo. Types passed 130 tests, or 185 with config. State passed
212 tests with 8 inherited ignored. The joined material/accounting harness passed
1218 tests with 18 inherited ignored. No-engine state passed 31 tests. Strict
all-target type/config, state, no-engine state and joined harness clippy passed,
as did formatting and diff checks.

Fifteen compiled protective mutations failed their targeted behavioral tests.
Every byte-exact restoration passed its focused rerun. Two independent source
reviews found no concrete defect within this inventory and explicit unavailable
coverage. No host memory limit or production qualification follows from these checks.

Logs, tested source hashes and mutation/restoration records are deposited under
`weaver-testing/state-management-146-olympus-2026-10-21/accounting/`
on the shared bulk store. Local staging is /tmp/weaver-146-evidence/accounting.

## Planner followup

[Planner review 5609272813][planner] passed the prior gates and twelve independent
perturbations, then identified Shape's partial copy subtotal and three inventory
questions. This followup makes the following corrections under the existing
state-observation/2 definitions. It changes no operation answer or instrument version.

- Row 4: Shape now reports null with Uninstrumented because its typed row decoding
  is not fully counted. The actual process fixture checks both empty and populated
  results, unchanged answers, complete held totals and measured sibling controls.
- Row 1: Dummy-page and wrapper-header sizing buffers no longer contribute to
  copied_bytes. Actual result/page/payload buffers still contribute once. The Next
  and oversized-answer fixtures pin the distinction with literal payload totals.
- Row 2: Open excludes read_limits.measurement_ref, which becomes retained session
  configuration and is excluded from every later operation. Its encoded echo remains
  part of the answer payload. The actual Open observation asserts this inventory.
- Row 3: The record now states the difference between live ID holdings and the
  copied payload categories rather than implying a symmetric allocation tally.

The renamed process fixture is
metadata_controls_keep_partial_shape_and_filing_copy_totals_unavailable. Protective
recipes remove Shape's unavailable coverage, restore Open's configuration holding,
charge the completion header probe, and charge Next's dummy page. All four compiled
and failed their intended assertions. Every byte-exact restoration passed. The
current default state suite passed 212 tests with 8 inherited ignored, and the
no-default suite passed 31. Both strict all-target clippy gates, formatting and lock
passed. The earlier broader matrix above describes the reviewed predecessor.
Two independent source readings found no concrete defect in this corrected inventory.
Current logs, recipes and source hashes are in the shared accounting/review deposit.

## Remaining batch followup

Rows 5-9 are addressed without changing the measurement categories or version:

- Row 5: Each accounting child now asserts its exact post-Open observation count:
  six for read/capture cleanup, five for preflight/failed-write cleanup, and six
  for filing and metadata controls. Open remains separately checked by the parent
  at the actual Opened response. The timing and metadata-failure modes also supply
  their exact expected counts through child-only test configuration.
- Row 6: OperationSample.finish consumes its own copied and held counters. The
  completion owner no longer extracts them merely to pass them back.
- Row 7: The five query, Shape and payload observed/unobserved pairs are single
  entry points. Their bounded writers accept an optional local tally and add only
  successful writes, preserving the actual prefix when a later write refuses.
  Ordinary, sizing and wrapper callers use no tally instead of a discarded counter.
- Row 8: Serving Read and Shape skip the preparatory query-encoding subtotal.
  Shared raw decoding, page and payload work still passes through the sticky
  operation counter, so those paths remain null with Uninstrumented. The focused
  encoder instruments explicitly request a local tally; they claim only encoded
  writes, never a complete content-query or Shape copy total.
- Row 9: Both line APIs share strict line, decode and direction validation. Typed
  frame construction shares the same closed-field parser. Only the observed
  DecodedStateFrame entry folds owned RawValue lengths; ordinary StateFrame
  deserialization no longer computes an unused subtotal.

The existing actual-process, failed-encoding-prefix, refused-query-prefix and strict
wire fixtures remain the regression checks. Protective recipes omit the final
Release from the child count, omit payload writer bytes, and lose an exact-bound
query prefix before its later refusal. No new tests mirror the pure cleanup.
The Olympus run used private target /tmp/weaver-146-accounting-target. Types
passed 130 tests, and its config-feature suite passed 185. State passed 212 tests
with 8 inherited ignored, and the no-default-feature suite passed 31. Strict
all-target clippy passed for both types configurations and both state feature
configurations. Formatting and lock passed. All three compiled mutations failed
their intended assertions and exact restorations passed.

The first state compile found one test call missing the new optional tally
argument. The corrected call uses None. Its failed log is retained beside the
successful run. Two independent source readings found no concrete defect in the
cleanup. Logs, recipes and source hashes are in accounting/batch in the shared
evidence deposit. Earlier results above describe their named predecessor source.
These author checks neither replace Planner review nor complete #221.

#221 remains open for complete filing, adoption, content-query and Shape copy
accounting.
#146 retains integration, lifecycle, artifacts and the coupled cutover. These local
buffer counts are measured payload work and holding, not total RAM or device memory.

[planner]: https://github.com/toddwbucy/WeaverAgent/pull/234#pullrequestreview-5609272813
