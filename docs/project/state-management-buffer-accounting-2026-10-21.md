# State management buffer accounting

Date: 2026-10-21. Executor implementation record for issues #221 and #146.
Branch: `code/state-buffer-accounting`, stacked on draft PR #233 at
`284f73f`. Author gates and protective checks passed; Planner review remains pending.
This record claims no production qualification, serving activation or issue closure.

## Authority and implemented scope

Implements: state-management-spec-09-qualification-costs STORE-OBSERVATION and
SNAPSHOT-SAMPLING, scoped to local copied_bytes and held_bytes in the additive serving
member. The merged definitions and observation/2 timing remain unchanged. Author grade:
conforms for the implemented scoped clauses, not a Planner grade.

held_bytes now measures the complete live owner inventory below on every observed
serving reply, including open refusals and control refusals. copied_bytes is complete
for Next, Release, Shape, Held, Position, Grants, Snapshot, Acknowledge, Abort and
ReleaseCapture. Open, File, FilePair and Read remain null with uninstrumented. Their
adoption, content validation and query-decoding copies require the remaining #221 act.
A partial subtotal never becomes an available total merely because a later buffer was
measured. Seed mode does not acquire a serving operation observation in this act.

The colon list in Spec 09 describes payload categories. This implementation counts
owned decoded raw buffers, encoded operation/result/page buffers and captured images.
It does not turn copied_bytes into total allocation traffic. Parser keys, scalar-ID
strings, enum labels, diagnostic strings, filenames and hash bookkeeping are excluded
as independent copies. They still count when present inside an actual raw or encoded
payload buffer. This explicit category inventory is part of the requested review.

## Actual buffers and their lifetimes

| Owner | copied_bytes | held_bytes at ended_ns |
| --- | --- | --- |
| Complete receive line | Framing predates began_ns and is not a decoded copy | Actual current Vec length, including LF |
| Channel read-ahead | Receiving bytes is not a decoded copy | Actual buffered tail length, including a pipelined request |
| Operation hint | RawValue values borrow the original frame | No retained hint owner |
| Strict floor decoder | Exact sum of its owned outer RawValue lengths | Decoder temporaries are gone before dispatch completes |
| Typed request | Deeper Text/raw/context copies remain uninstrumented for filing, adoption and content Read | Dispatch consumes it. Only the current Common and optional filing SourceBlockId strings remain |
| Open echo | Adoption copies remain uninstrumented | The original read-limit reference and separate member identity remain until metadata insertion. The typed echo is dropped before finish |
| Query Output | Counts bytes actually appended, including a refused prefix | Complete query Vec moves into Pages without copying |
| Shape Output | Counts its actual encoded array, including a refused prefix | Temporary encoding and decoded rows drop before finish |
| Pages | Each base64 page is one new buffer | The complete frozen result remains through the final page until release, never only its unsent suffix |
| Snapshot image | One successful SQLite-to-Rust Vec copy, even if the room write later refuses | Image Vec drops before answer completion. A room file is not a live image buffer |
| Capture continuation | No second image copy on acknowledge, abort or release_capture | Held filename, plus the Finished admin-hash string. Abort and release remove these owners |
| Payload encoder | Actual written bytes in every attempt, including a failed prefix and fallback | Final encoded payload remains. Consumed typed Reply is explicitly dropped before ended_ns |
| Sizing probes | Actual encoded dummy-page and header buffers, each once | Probe buffers are dropped before finish |
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
- A real filing with escaped Unicode retains explicit missing copy coverage. Shape,
  Held, Position and Grants verify complete control totals and actual request owners.
- Next uses the actual Session dispatch and page encoder. It counts a new base64 page
  once, retains the full frozen result and ignores spare Vec capacity.
- A real socket framing fixture retains two pipelined frames and checks the buffered
  tail contributes until consumed. It does not assume bytes remain in a kernel queue.
- An oversized Position payload checks the exact prefix written before encoding
  refusal, the fallback bytes and holding after initial-page discard or cursor retain.
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

#221 remains open for complete filing, adoption and content-query copy accounting.
#146 retains integration, lifecycle, artifacts and the coupled cutover. These local
buffer counts are measured payload work and holding, not total RAM or device memory.
