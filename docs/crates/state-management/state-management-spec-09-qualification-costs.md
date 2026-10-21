# State Management Spec 09 - Evidence and Qualification Boundary

**Status:** MERGED.

**Landing PR:** #192

**Version:** 0.13 **Date filed:** 2026-10-20 **Document ID:** `state-management-spec-09`
**Parent:** state-management-primitives-PRD **Editorial:** Per the Working Rules.

## 0. What this document is

**SCOPE.** The agent emits the evidence required by COST-1 through COST-5. Analysis,
falsifier evaluation, model comparison and release qualification run in WeaverAnalysis
under October 18 R9. No test-fixture loader, miss judge or report writer is installed in
the serving harness. This Spec defines the producer contract and the obligations its
external reader must meet. It does not claim the reader already exists. [COST-1 through
COST-5, FALS-1, SEAM-4, SCOPE-1]

Every owning amendment follows the [foundation transition][transition]. Until cutover
the previous serving contract applies. Evidence uses the real load event,
model.measurement, model.output, refusal, flush and elision records, plus the state
records defined with their producing features. It does not invent a second trace head or
put measurements into residue. [FLOW-3, TAP-3, COST-2]

## 1. Identity, clocks and costs

**IDENTITIES.** The loaded configuration digest B and recorded deployment facts identify
a run, with dump A and the binary/model identities. The existing load is the declaration
record. RUN-EVIDENCE below binds its binary digests to source revisions and adds the
qualification observer's hardware and memory observations. It supplies no agent input
and changes no load ordering. Do not put task ID, run ID, A or workload instance into
the qualification tuple. A workload manifest in WeaverAnalysis binds those run facts to
task instances, the driver version and evaluation protocol. Reason: several tasks must
be comparable under one declared tuple, and a later observation cannot be written into
an earlier load. [FLOW-2, FLOW-3, CLASS-7, FALS-1, October 18 R9]

A task instance is the external driver's task ID plus its attempt ordinal. Its manifest
lists the run and turn ranges and any plan/task SpanRefs. This covers a refusal before a
task mark exists and a task spanning a reload. The producer never guesses a task from
prose. The qualification tuple is the canonical JSON of model and checker artifacts,
scheme, templates, engine settings, hardware, memory limits, OS/kernel, serving criteria
and instrument versions. The analysis-side encoder sorts object keys, preserves array
order, encodes integers exactly and hashes UTF-8 with SHA-256. Floating-point parameter
bit patterns use typed hexadecimal strings before this encoding. It rejects nonfinite
values and duplicate keys. The resulting digest is not a second file-integrity hash for
persistence. [COST-5, CLASS-7, PERSIST-3]

**RUN-EVIDENCE.** WeaverAnalysis owns one completed RunEvidence record per evaluated
run. This is the run-record option in finding S09-12. Its exact JSON members are format,
run, trace_sha256, load_sequence, load_record_sha256, builds, environment, driver,
protocol_sha256 and instrument_versions. format is state-run-evidence/1. run is the
existing trace RunRef string. load_sequence is DecimalU64. Digests are 64 lowercase
hexadecimal SHA-256 characters. trace_sha256 hashes the complete captured trace bytes.
load_record_sha256 hashes the exact load line, including its final LF, at that run and
sequence. The reader verifies both before joining any external facts. It also verifies
that the line is load with no turn. A live capture has no completed record until its
bytes are fixed. [FLOW-2, FLOW-3, COST-5, FALS-1, October 18 R9]

builds is an ordered list of {name, binary_sha256, provenance}. It covers every entry of
load.stack and the serving load.ste100_binary_sha256 exactly once. The STE100 entry uses
the name ste100. Each stack entry uses stack:<basename>. If the STE100 executable also
appears in load.stack, retain both role names with the same digest and identical
provenance. They identify two declared roles, not two executing processes. Count a
process once by (pid, start_ticks), using ste100 when that process is the admitted
checker and stack:<basename> otherwise. A legacy label scorer remains its stack role
even when it shares binary bytes. Sort builds by name byte order. The reader compares
each binary_sha256 with the trace value. An absent, empty or malformed trace digest
cannot match a guessed build. The admin stack_digests function currently permits an
empty digest on a failed read. Such a trace is insufficient for qualification even if
serving completed. This Spec does not silently alter that inventory failure rule.
[FLOW-3, COST-5, FALS-1]

provenance is either {verified: BuildProvenance} or {unavailable: EvidenceUnavailable}.
BuildProvenance is exactly {record_sha256, repository, source_revision,
source_tree_sha256, toolchain, target, profile, features, lockfile_sha256}. repository,
toolchain, target and profile are nonempty strings. source_revision is a full Git object
ID, 40 or 64 lowercase hexadecimal characters. features is a sorted unique string list.
The other digest fields use the common digest form. record_sha256 addresses the external
build record. That record holds binary_sha256 and the remaining BuildProvenance fields,
with no record_sha256 member of its own. Its fields must match this entry.
source_tree_sha256 addresses the exact source bundle used, including any local changes.
A branch name or the reviewer's current checkout cannot replace it. EvidenceUnavailable
is absent, denied, read_failed, unsupported, changed or unverified. [FLOW-3, COST-5,
FALS-1]

The build pipeline produces the build record when it builds the named binary. The
qualification reader verifies its digest, referenced source bundle and binary match.
This pass proves recorded provenance and artifact agreement, not reproducible
compilation. WeaverAnalysis keeps these artifacts with its report. Admin gains no
provenance-file inventory key, build command or device probe. No serving component opens
the report. Reason: a digest identifies the loaded binary now, while its source revision
is a fact of the build that produced it. [FLOW-3, SEAM-4, SCOPE-1, October 18 R9]

environment is either {observed: EnvironmentEvidence} or {unavailable:
EvidenceUnavailable}. EnvironmentEvidence is exactly {observer_sha256, began_ns,
ended_ns, host, processes, devices, artifacts}. The observer is the operator-run
qualification collector, under the same custody boundary as DISK-EVIDENCE. Its own
executable digest is observer_sha256. began_ns and ended_ns are raw CLOCK nanoseconds
enclosing its collection, with began_ns <= ended_ns. artifacts is a sorted unique list
of content digests for the raw observations. All referenced artifacts must be present
and verified. This is evidence gathered outside the agent, not another trace or state
feed. [COST-5, FALS-1, FALS-3, SCOPE-1]

host is exactly {architecture, kernel_release, cpu_models, logical_cpus, ram_bytes}. The
first two are nonempty strings from uname. cpu_models is a nonempty sorted unique list
of model descriptions from the host CPU inventory. logical_cpus and ram_bytes are
positive DecimalU64 values from that inventory and the host's physical-memory total, not
free memory at a sample. The observer retains the raw inventory artifacts. An
architecture with no supported model identity yields unavailable, not a substituted
architecture name. [COST-5, CLASS-7, FALS-3]

processes is a list sorted by (pid, start_ticks), with one entry per executing binary
instance named by builds. Each entry is exactly {name, pid, start_ticks,
ticks_per_second, binary_sha256, allowed_cpus, address_space_limit, cgroup_memory}. pid
is positive u32. start_ticks and positive ticks_per_second are DecimalU64 from
/proc/<pid>/stat and sysconf(_SC_CLK_TCK). allowed_cpus is a sorted unique list of u32
logical CPU IDs from the process affinity. The observer reads the run's containment
cgroup and descendants under weaver-admin-Spec section 6.1, validates the run's recorded
member PID/start identity, and matches each executing binary digest to builds. It
samples PID start identity before and after reading facts. admin show returns one JSON
object whose State answer includes constituent PIDs but no start identity. The
cgroup/start checks prevent attribution to a reused PID. It hashes the executing file
through /proc/<pid>/exe and compares it with builds. It never attributes a reused PID to
the run. [FLOW-3, COST-5, FALS-3]

address_space_limit is the current soft RLIMIT_AS. A MemoryLimit is either {bytes:
DecimalU64} or {unlimited: true}. cgroup_memory is a nonempty list ordered from the
process's cgroup through each ancestor to the cgroup-v2 root. Each entry is exactly
{path, memory_max, memory_high, swap_max}. path is the absolute kernel cgroup path
observed through /proc/<pid>/cgroup, not an admin-selected artifact path. The other
fields are MemoryLimit values read from memory.max, memory.high and memory.swap.max. For
a root or controller position where the kernel exposes no such control by definition,
record unlimited and retain evidence of that position. Failure to read an existing
control is unavailable. Do not treat it as unlimited. Keep every ancestor value. A
single minimum cannot describe a shared ancestor's pressure or the distinct high and
swap controls. [STORE-1, COST-5, FALS-3]

devices is sorted by stable_id and contains exactly the accelerators assigned to the
observed SPU and classifier processes. Each entry is {stable_id, model, total_bytes,
driver_version, assignments}. The identity, model and driver version are nonempty
strings from the host's device inventory. total_bytes is positive DecimalU64 for the
assigned device or partition, not a whole-board value substituted for a partition.
assignments is a sorted unique list of {pid, start_ticks, visible_ordinal}, with u32
ordinal and the same process identity rules as processes. The observer must establish
the process-visible ordinal mapping from that process's environment and the device
inventory. A bare host ordinal is insufficient. CPU-only execution uses an empty list.
Unavailable mapping, capacity or driver evidence makes environment unavailable. Device
identity is evidence for comparison. It is no substitute for the SPU's admission
judgment. [CLASS-7, COST-5, FALS-1]

The collector records the process and environment facts before and after each evaluated
task range, retains the observations as artifacts and reports changed if the comparison
fields disagree. EnvironmentEvidence is the common verified value only where they agree.
The frozen qualification protocol must state how it holds relevant settings fixed during
that interval and detects changes. Endpoint agreement alone does not prove that no
intervening change occurred. Incomplete coverage makes the affected claim inconclusive.
The observer does not modify cgroups, devices or process limits. Failed collection
changes the evidence verdict, never the serving run's halt state. [COST-5, FALS-1,
FALS-3]

driver is exactly {artifact_sha256, source_revision, workload_sha256}. artifact_sha256
identifies the external task driver, workload_sha256 its frozen corpus, and
source_revision uses the full Git object ID form above. The task manifest supplies each
task ID, attempt ordinal and run/turn ranges. Those instance identities stay outside the
qualification tuple. protocol_sha256 names CALIBRATE-THEN-TEST's frozen protocol.
instrument_versions is a string map with exactly run_evidence, store, flow, plan and
observer. The first four are state-run-evidence/1, state-observation/2,
state-flow-observation/1 and plan-observation/1. observer is the nonempty version of the
external collector's definitions. The reader checks store and plan against the trace
versions, and flow against the producer definition and binary provenance. [CLASS-7,
COST-5, FALS-1]

No optional OS or provenance value is encoded as zero, an empty digest or an inferred
default. An unavailable required fact prevents qualification of the corresponding claim.
Unsupported hardware remains eligible for development and may yield a trace. It cannot
receive an unearned comparison or disk-wait pass. The record verifier refuses unknown
fields, duplicate JSON keys, malformed digests, duplicate identities, invalid clocks and
contradictory trace bindings. It preserves a valid unavailable result as inconclusive.
[COST-5, FALS-1, FALS-3]

**EVIDENCE-SOURCES.** The following join is exhaustive for IDENTITIES. No producer fills
a field on another producer's behalf. In particular, load remains before SPU admission,
and model.measurement remains turn-bound. [FLOW-2, FLOW-3, SEAM-4, SCOPE-1]

| Fact | Existing or proposed producer and carrier | Reader obligation |
| --- | --- | --- |
| Declaration, starting A/B, stack and checker executable digests | Admin inventory and verified start through EnterBinding, harness TRACE-LOAD | Read the exact load, do not infer a declaration from a later config root |
| Scheme, templates, limits and serving criteria | Verified serving declaration and template inventory in TRACE-LOAD | Compare exact identities and bytes before aggregating runs |
| Actual decoder artifact and effective rendering/sampling | SPU render_measurement weights_hash and model.request, carried by the existing Generation path | Check every evaluated generation, no inference from an unused declaration path |
| Clock origin and store instrument/process identity | Harness TRACE-LOAD and member opened response | Join raw clocks and the verified PID/start identity |
| Source revisions and build inputs | External build records bound to load binary digests | Verify RunEvidence.builds, never use the evaluator's current HEAD |
| Hardware, process memory controls and driver versions | External qualification collector and RunEvidence.environment | Verify raw artifacts and coverage, unknown is inconclusive |
| Workload, driver and evaluation protocol | External task manifest and RunEvidence.driver/protocol_sha256 | Keep task instances out of the qualification tuple |

The qualification tuple takes builds, host, device identities/capacities/drivers, the
normalized memory tree and recorded scheme, templates, model and effective
engine/serving controls. A normalized process row is exactly {name, binary_sha256,
allowed_cpus, address_space_limit, devices}. devices lists that process's {stable_id,
visible_ordinal} assignments, sorted by stable_id then ordinal. Exclude PID,
start_ticks, sample times and raw-observation artifact digests. [CLASS-7, COST-5,
FALS-1]

Construct one memory tree from the observed cgroup paths and ancestor links. Each node
is exactly {memory_max, memory_high, swap_max, processes, children}. processes contains
normalized rows for processes directly in that cgroup. children contains the next
observed descendant nodes, with no skipped ancestor. Require all observations of one
path to agree on its limits. Sort both lists by canonical JSON bytes and retain
duplicates. Remove path names only after building the tree. This keeps shared and
separate limiting ancestors distinct without putting run-specific names into the tuple.
Instrument versions, driver artifact, workload corpus and protocol are comparison
inputs. Per-generation seed values and task instances stay in the run/task evidence,
while the protocol's seed schedule stays fixed for the comparison. [CLASS-7, COST-5,
FALS-1]

**CLOCK.** Use Linux CLOCK_MONOTONIC nanoseconds for cross-process observations. The
harness captures origin_ns once at run start and records it with clock_source in load.
Envelope monotonic_ns remains nanoseconds since that origin, preserving its merged
meaning. Member samples carry raw clock readings and the harness subtracts the same
origin when presenting them. A Rust Instant never crosses a seam. wall_ms remains the
independent civil timestamp. Deadline enforcement and measurements use the same
monotonic source. [COST-4, CLASS-8]

**OBSERVATION-IDENTITY.** opened also carries member_observation:
MemberObservationIdentity {instrument_version, pid, start_ticks, ticks_per_second,
unavailable_reason}. instrument_version is the literal state-observation/2 for the
definitions in this Spec. pid is the member's positive u32 process ID. start_ticks and
ticks_per_second are both present DecimalU64 or both null. They come from its own
/proc/self/stat start time and sysconf(_SC_CLK_TCK), with a positive tick rate.
unavailable_reason is null for present values or the closed ObservationUnavailable
reason for nulls. No caller supplies these values. [COST-2, COST-5, FALS-3]

The harness validates the shape and instrument version, then records that exact identity
in load.state_member_observation before releasing the held open sample. The external
observer verifies the live process start identity before attributing counters to this
run. Missing start facts make that attribution inconclusive, not a serving failure or
permission to use a different process. A changed instrument definition requires a new
version in both endpoints and the analysis reader. Reason: raw counters need a named
producer and measurement definition. [FLOW-3, COST-5, FALS-3]

**AXES.** Resources and elapsed time remain separate. Resource values include recomputed
tokens, deciding-pass tokens, selection evaluations, bytes held/copied and CPU
nanoseconds. Elapsed intervals include queue, classification, filing, read, cut and gate
waits. An unavailable value is null with a reason, not zero. Analysis takes interval
unions for overlapping waits, and never adds CPU duration to elapsed duration or byte
counts to time. Conditioning is a task-effect measurement, not a token-cost synonym.
[COST-1, COST-2, COST-3]

## 2. Producer records and store carriage

**RECORD-OWNERS.** Each quantity has one producer record. A consumer may derive an
aggregate, but no feature emits a duplicate total under a generic new kind. [COST-2,
COST-4, COST-5]

| Quantity | Record and source |
| --- | --- |
| Decoder tokens, timing and rendered partition | Existing model.measurement and request |
| Current resident count and capacity | Existing model.output's Generation facts |
| Removed resident prefix/suffix counts | Existing flush and elision plus state.placement.N |
| Placement references and cut wait | Spec 06 state.placement |
| Stored-span occurrences and canonical endpoints | Spec 06 state.mark.position, correlated with append completion and phased state.placement |
| Selection budget and overflow | Spec 08 state.sort.measurement and sort.decision.decision.serve.dropped_refs |
| Classifier work and accepted/rejected outcome | ste100.result, source target and result |
| Filing lag, receipt throughput and bound outcomes | Foundation state.flow |
| Model repair and cap exhaustion | Spec 02 state.flow phases repair_requested and repair_exhausted |
| Refused external operation | Existing refusal with STE100 refusal and input_id |
| Tool outcome and review result | Spec 05 tool.call.completed and Spec 07 step.outcome |
| Held flag targets, successor argument readiness and mandatory-only fit refusals | Spec 07 state.plan.measurement, plan-observation/1 |
| Store operation cost | StateOperationObservation, carried below |

The existing score kind retains its one-run task verdict, integer ratio terms and
positive-denominator rule. This act does not emit each protocol rate as another score.
Analysis computes per-task ratios from the counts below. A zero denominator is not
applicable and produces no score ratio. [COST-5, FALS-2]

**STORE-OBSERVATION.** Every correlatable state-flow/1 response has one required
observation beside its existing payload, never in filed or a read entry. A refusal also
has one. The member samples one operation for each incoming frame, including each
snapshot continuation. It allocates operation_id before validation, starting at one for
open and increasing by one with checked arithmetic. The counter is member-run-local and
never restored from state. request_id echoes the outer frame. Continuations share a
request_id but have distinct operation_id values. Reason: measurement must cover
refusals and waiting as operation metadata. The store files that category beside its
operation under STATE-OBSERVE, never through the classifier or into a content unit.
Retention does not elect presentation. Only an explicit observations query returns it as
query data. Response metadata is harness-bound, not model-bound. [COST-2, COST-5,
SEAM-2, SEAM-3, R20-19]

StateOperationObservation has exactly {operation_id, parent_id, request_id, operation,
began_ns, ended_ns, phases, resource, disk}. IDs, counts and times are DecimalU64.
parent_id is required null in this release. All work within a request is represented by
phases, not hidden child requests. StateOperation is open, file, file_pair, read, next,
release, shape, held, position, grants, snapshot, acknowledge, abort or release_capture.
Each spelling names the received request or control variant exactly. acknowledge is the
member's image-finish operation, never admin's publication Commit. [COST-2, SEAM-2,
SEAM-3, PERSIST-4]

A phase is {name, began_ns, ended_ns}. PhaseName is validate, query, transaction,
capture, filesystem or encode. Each phase occurs at most once. validate and encode are
required. The others occur only when that work ran. A filing that starts a transaction
reports transaction through commit or rollback. snapshot reports capture through
serialization of the image and filesystem for its room write. acknowledge, abort and
release_capture report filesystem when they touch the room. The member sorts phases by
(began_ns, name), permits nesting, and requires every interval inside [began_ns,
ended_ns]. Analysis unions intersecting intervals. [COST-1, COST-2, COST-4]

began_ns is sampled after the complete request frame is in hand, before any operation
hint parsing, decoded-frame allocation or validation. This includes snapshot. Retain
that timestamp when the operation is identified. Resource sampling never moves it.
ended_ns follows encoding of the answer's operation payload, before the observation and
final wrapper are encoded. The encode phase is that payload encoding, including
construction of an explicit refusal when necessary. It does not claim to measure its own
metadata encoding or the subsequent socket send. The harness interval below includes
that remaining work. No transport phase is invented inside an answer whose send has not
happened yet. [COST-2, COST-4]

resource is {cpu_ns, copied_bytes, held_bytes, unavailable}. The first three are
nullable DecimalU64. unavailable is a unique list of {field, reason}, sorted in that
field order, with one entry for each null and none for a present value. ResourceField is
cpu_ns, copied_bytes or held_bytes. ObservationUnavailable is unsupported, denied,
disabled, read_failed, counter_reset, overflow, uninstrumented or not_sampled. These are
closed enums, not unbounded diagnostic strings. [COST-1, COST-2]

Kernel CPU and disk reads run only for snapshot, while serving is paused for capture.
For open, file, file_pair, ordinary reads and every other response, cpu_ns is null with
reason not_sampled. disk has enabled, before and after null, and unavailable_reason
not_sampled. No resource-sampling getrusage, /proc read or thread CPU clock runs on
those operation paths. The one-time MemberObservationIdentity at channel open is boot
identity evidence and retains OBSERVATION-IDENTITY's existing process reads. Monotonic
wall timestamps and local byte counters remain on every operation. [STORE-1, COST-2,
COST-4, R20-4]

**SNAPSHOT-SAMPLING.** For snapshot alone, cpu_ns and disk counters bracket capture work
separately from the full operation interval. cpu_ns is the CLOCK_THREAD_CPUTIME_ID delta
across these samples. After validation and capture preflight, read the before counters
immediately before image serialization. Read the after counters when capture work ends,
after its attempted room-file write and any recoverable cleanup, before constructing and
encoding the answer payload. The capture phase still covers serialization and filesystem
covers the room write and its cleanup. Neither is renamed or expanded to stand for the
resource bracket. This follows the operator's ruling of 2026-10-20: include parsing in
wall time and bracket capture separately. [COST-1, COST-2, COST-4, STORE-1]

A snapshot refused before capture starts reads no resource counters. cpu_ns is null
with not_sampled, and disk uses the same not_sampled form as an ordinary operation. If
capture starts and yields a recoverable refusal, take both samples and report their
availability under the existing rules. A terminal failure retains the existing missing
observation behavior. A failed optional counter read never turns a valid capture or
refusal into component failure. [COST-2, COST-5, FLOW-4]

Counter collection is inside the operation wall interval and therefore the enclosing
harness interval. It is outside the capture and filesystem phase intervals. Sequential
CPU and disk reads may include sampler overhead in their deltas. Do not subtract an
estimated overhead or equate those deltas with either phase's wall duration. The sample
characterizes lifecycle capture, not per-turn disk wait. [COST-1, COST-2, COST-4,
STORE-1]

| Sampling state | Event | Required result |
| --- | --- | --- |
| Frame complete | Any operation hint parse | Preserve the earlier began_ns |
| Validated snapshot | Preflight refuses before capture | No counter reads, not_sampled |
| Validated snapshot | Capture starts | Before samples, then image serialization |
| Capture started | Room-file result or recoverable failure | After samples before answer encoding |
| Sampling | Optional source unavailable or counter reset | Existing null and reason, no new halt |
| Payload encoded | Observation finish | ended_ns after any samples, metadata rules unchanged |

The state-observation/2 identity distinguishes this bracket from the earlier /1
definition. Current opened and load require /2. Historical /1 records retain their
original identity when adopted or read, and are never rewritten as /2. Qualification
under the current definition requires /2 evidence. Analysis must use the recorded
version's definitions when interpreting historical records. The /1 source is pinned to
a commit reachable from main, under the Working Process archive rule. [COST-5,
PERSIST-1]

| Instrument | Measurement definition |
| --- | --- |
| state-observation/1 | [Spec 09 section 2 at 7a8651d8](https://github.com/toddwbucy/WeaverAgent/blob/7a8651d8/docs/crates/state-management/state-management-spec-09-qualification-costs.md#2-producer-records-and-store-carriage) |
| state-observation/2 | STORE-OBSERVATION and SNAPSHOT-SAMPLING in this Spec |

copied_bytes counts bytes in buffers owned by the serving adapter:
decoded Text/raw/context copies, encoded result/page buffers, and captured image
buffers, each materialization once. It excludes SQLite's and the allocator's internal
copies. held_bytes is the sum of live read-snapshot buffers, capture buffers and
in-flight request/answer buffers at the end sample. It excludes the SQLite heap,
allocator capacity and kernel queues. The instrument version pins these definitions.
Neither quantity claims total RAM or device memory. A path with an uninstrumented copy
or holding category reports null, not a partial total. [COST-1, COST-2, STORE-1]

disk is {enabled, before, after, unavailable_reason}. enabled is bool or null, sampled
only at snapshot from the member's readable delay-accounting setting. before and after
are both complete DiskSample objects or both null. DiskSample is {major_faults,
read_bytes, write_bytes, blkio_delay_ticks, ticks_per_second}, all DecimalU64, with
ticks_per_second positive and equal in both samples. The sources are the member's own
getrusage, /proc/self/io and /proc/self/stat, with sysconf(_SC_CLK_TCK). No other
process's protected file is read. If a source is unavailable or a counter decreases,
both samples are null and unavailable_reason gives the closed reason. If enabled is
null, a reason is also required. Otherwise the reason is null when the samples are
complete. Disabled accounting alone sets enabled false, keeps any readable samples and
cannot prove absence of delay. [STORE-1, COST-5, FALS-3]

**OBSERVATION-CARRY.** TYPES-OBSERVATION owns the shared Rust types. Its observation
field is required on opened, filed, refused and control_answer, forbidden on requests.
No extra measurement message or socket is added. STATE-OBSERVE brackets answer_frame's
serving replacement and carries the payload into SendingAnswer. The complete response,
including observation, remains within ANSWER_BOUND. Page construction reserves the
maximum encoded observation size before choosing rows. The maximum comes from the closed
schema and a worst-case encoding fixture, not an unmeasured new limit. A too-large
result yields answer_too_large with its own observation. The reservation is frame
accounting. ReadPage content belongs only to the query, and no ordinary result gains
stored metadata. No counter operation writes a content table. STATE-OBSERVATION-SCHEMA
and STATE-OBSERVE own its separate RAM record. [SEAM-2, STORE-1, COST-2]

The harness validates the response and observation together before granting its
completion. It checks the request/operation match, next operation_id, phase and counter
rules, and raw clock order. filed still proves a transaction committed, not that the
socket accepted a write. A filed payload with a malformed observation is a malformed
response with an uncertain acknowledged outcome. It halts and is never resent. A valid
observation with unavailable counters grants no such halt. An uncorrelatable malformed
request or dead member has no invented observation. The fault and outstanding ledger
name that missing evidence. [FLOW-4, FLOW-5, SEAM-2, COST-5]

StateStoreMeasurement is {observation, request_source, harness_queued_ns,
harness_sent_ns, harness_received_ns}. request_source is the filing's SourceBlockId for
file/file_pair and null for controls/open. Queue time starts when the owner accepts the
operation for sequencing. sent is just before the first request byte, and received is
when its complete answer is available. Require queued <= sent <= observation.began_ns <=
observation.ended_ns <= received. All are raw values from CLOCK. The harness authors
state.store.measurement, channel measurement, origin runtime, before releasing the
completion. It follows the source event's turn for a filing and the current turn, if
any, for a control. [COST-2, COST-4, FLOW-3]

The valid open observation waits in one held completion until load is recorded and the
partition is activated. It is authored before boot reads. An open failure before load
follows the existing pre-load diagnostic path and supplies no complete trace claim.
Measurement authoring uses the ordinary pending-author slot. The tee cannot select it,
so authoring it never creates another store operation. A terminal fault does not wait
for a fresh measurement. [FLOW-2, TAP-3, FLOW-5, COST-5]

**FLOW-COUNTS.** state.flow already owns the ledger. Add no second lag clock. The
definitions in this clause are state-flow-observation/1 in RUN-EVIDENCE. They are
checked against the producer binary's recorded build, without adding a second flow
sample or changing the existing FlowObservation payload discriminator. Required samples
include every disposition transition and each observation tick while work remains, so
zero-receipt intervals stay visible. FlowConfig adds the required
observation_interval:MeasuredInterval, with positive elapsed_ns and measurement_ref. Its
TOML table is state-management.flow.observation-interval, using elapsed-ns and
measurement-ref. This is a cadence, never a wait bound. Qualification varies it against
recording overhead and lag visibility, then freezes it before held-out runs. [CLASS-6,
FALS-5, COST-4]

HARNESS-ACCOUNT keeps its existing fields and adds sample_start_ns, sampled_ns,
pending_bytes, filed_count, filed_bytes, rejected_count, input_pending, input_started
and input_completed, all DecimalU64. The two timestamps are raw CLOCK readings.
pending_count/bytes and input_pending are current gauges. The remaining counts are
deltas since sample_start_ns, including this transition. Bytes count the exact admitted
raw_block plus context bytes when present, once per obligation. They are traffic
measures, not heap accounting. A correctly rejected statement increments rejected_count
and never filed_count. A duplicate receipt increments neither, because it fails the
channel. [CLASS-6, COST-2, FALS-5]

Each sample also carries retained_blocks:DecimalU64, retained_bytes:DecimalU64 and
lag_alarm:bool. The retained gauges are HARNESS-BACKLOG's reservation totals, including
queued copies and unfinished ledger entries. They differ from pending_bytes, which
measures raw filing traffic. The waiting copy is outside those totals and is reported
separately as waiting_copy:Option<CopyAtHalt>, using HARNESS-HALT-ACCOUNT's identity
shape. pending_turns:DecimalU64 counts distinct non-null turn IDs with owed entries or a
waiting copy. Turnless work contributes to block/byte gauges, not that count.
backlog_waiting:bool means the whole-copy reserve is occupied. lag_alarm is true when
backlog_waiting or pending_turns >= flow.backlog.lag_alarm_turns. It returns to false
when both predicates clear. Oldest age remains a measurement only. The sampled Boolean
is an operator-visible alarm, not a fault, missing result or new event kind. Observation
cadence bounds its normal reporting delay, with existing trace-pressure coalescing
stated in the record. [CLASS-6, CLASS-8, FALS-5, COST-4]

Qualify max_pending_blocks and max_pending_bytes together. Run the declared tuple with
bursty output, compound statements, maximum classifier replies and delayed store
receipts. Measure peak retained source charge, metadata, resident RAM, encoded-frame
cost, the one failed-copy reserve and time to recover after receipts resume. Select
limits within the available memory, with the evidence naming the largest complete source
event exercised. Measure the turn threshold against the qualified context tail and
sample cadence against visibility of increasing lag. Tail slack is measured in turns,
not a receipt-age failure clock. Freeze all four values before held-out runs. The
document elects no number and no claim that source-byte charge equals allocator usage.
[CLASS-6, COST-2, COST-4, FALS-5]

The state.flow phase set also includes backlog_wait and backlog_resumed. Emit these
transitions once on entering and leaving whole-copy wait, before another model request
can proceed. Partial authoring never duplicates them. Their sample uses the same
required gauges and backlog_waiting flag. [FLOW-3, CLASS-6, R20-2]

The first interval starts at open confirmation. Each sample freezes its counts and
timestamp once. Only Recorded advances the next interval's baseline to that sample, so a
pending author event cannot silently consume a count. Add phase sampled for a tick or
turn boundary without another transition. Ticks run only with pending filing or input
checks. A full author slot coalesces missed ticks into one sample when capacity returns,
with the whole elapsed interval, never a fabricated series of zero-duration samples. The
existing halt account carries one final flow_sample with the same fields and phase
sampled, ending the last recorded interval without another state.flow event. It
supersedes any unrecorded pending sample. No trace event is generated in response to a
measurement alone. [FLOW-5, CLASS-6, FALS-5, COST-4]

| Count | Exact source predicate |
| --- | --- |
| Input submissions | Distinct input_id entering Spec 02 input validation |
| Input refusals | Refusal case non_ste100 for that input_id |
| Rejected model blocks | ste100.result source target naming message.assistant, result not_ste100 |
| Repair operations | First repair_requested for each original source_id |
| Cap exhaustions | repair_exhausted for that source_id, counted once |
| Flow component halts | First fault with case state_flow_failed per run, separate from every conformance outcome |

A missing terminal trace is incomplete evidence, never proof that no failure occurred.
Analysis reports that task's affected measure as inconclusive. No flow.halted event kind
is introduced. [FLOW-6, FALS-1, COST-5]

HARNESS-HALT-ACCOUNT defines the fault's exact correlation, wait, pending-author and
commit-status fields. TRACE-HALT repeats that first snapshot at unload with secondary
evidence and fault_sequence. Join a present fault_sequence to the same run's first
fault, and count it once. A null reference is explicit missing fault evidence, not a
second component halt or a successful run. Reject inconsistent first causes or source
identities as unusable qualification evidence. This reader check neither changes the
serving failure outcome nor changes R20-7 broken-save-point handling. [FLOW-3, FLOW-5,
COST-5, FALS-5]

**PLAN-MEASURES.** Consume Spec 07's three tagged plan-observation/1 readings. flags is
a snapshot after its named committed effect, not an interval counter. Do not sum
repeated unmatched_flags samples as distinct missed decisions. Their scope is all held
canonical step conditions against that current revision, including closed targets. A
typed read refusal is unavailable, not a zero sample. Correlate request_id with the
state read observation and through with its holding boundary. next_ready measures
close-receipt consumption to successor argument readiness in one run. It includes an
intervening external-input wait, which turn brackets expose. It excludes later request
transport and decode. A replacement, reload or missing endpoint cannot be bridged into a
measured interval. [PLAN-3, PLAN-4, COST-4, COST-5]

Count fit_refused once by run/assembly_id and check its ordinary refusal and selection
record. The mandatory_probe and place_backstop variants are alternative producers of one
outcome, not additive failures. Optional-unit overflows stay in their existing records.
A flag-target count is not a selection-miss verdict. The external task fixture and
MISSES determine whether missing placement changed a decision. The reader fixture must
fail if it sums gauges, substitutes zero for unavailable, crosses run clocks, drops idle
waits silently or counts both mapped refusal records. [COST-4, FALS-4, PLAN-3, PLAN-4,
SCOPE-1]

## 3. External qualification contract

**ANALYSIS-OWNER.** WeaverAnalysis owns the workload driver adapter, interval joins,
rate aggregation, selection-miss fixtures, model ranking, falsifier verdicts and
qualification report. It consumes trace files or the operator trace stream, never a
serving store connection. The analysis-state contract's preload door remains a separate
diagnostic interface. This repository owns the authoritative copy under R20-5, and
WeaverAnalysis mirrors it. [SEAM-4, SCOPE-1, October 18 R8, October 18 R9]

**EVIDENCE-VERIFY.** The external verifier returns valid, inconclusive or refused as
{verdict, field, reason}. field is a JSON Pointer string and reason is the closed reason
code. Both are null for valid and nonnull otherwise. An empty pointer names the whole
record. Reasons are malformed, missing_artifact, digest_mismatch, identity_mismatch,
incomplete_coverage, unavailable_fact or changed_environment. valid means the complete
run record matches its captured trace and evidence artifacts. It does not mean a
falsifier passed. inconclusive preserves a well-formed unavailable fact or incomplete
coverage. refused means the representation or its binding is contradictory. Both prevent
qualification of any claim that needs the affected evidence. Reason: unknown facts and
invented or mismatched facts are different outcomes. [COST-5, FALS-1, FALS-3]

| Reader state | Input or observation | Result and next state |
| --- | --- | --- |
| Unbound | Malformed RunEvidence or duplicate JSON key | refused/malformed, terminal |
| Unbound | Complete trace and named load match digests, run and sequence | TraceBound |
| Unbound | Missing capture or digest/identity mismatch | refused with the corresponding reason, terminal |
| TraceBound | Build list covers exact stack/checker digests and all referenced build artifacts match | ProvenanceBound |
| TraceBound | Well-formed unavailable provenance | inconclusive/unavailable_fact, terminal |
| ProvenanceBound | Environment identities, artifacts, task coverage and frozen controls agree | EvidenceBound |
| ProvenanceBound | Missing evidence, unknown facts or changes | inconclusive with the corresponding reason, terminal |
| Any bound state | An artifact or identity contradicts the trace or record | refused/digest_mismatch or identity_mismatch, terminal |
| EvidenceBound | Driver, protocol and instrument definitions agree | valid, ready for claim evaluation |
| EvidenceBound | Missing coverage or unknown instrument definition | inconclusive/incomplete_coverage or unavailable_fact, terminal |

This is an offline reader's table, not a new serving exchange. There is no response to
the agent and no modification of its trace, declaration or state. Every source artifact
remains inspectable beside the verdict. [SEAM-4, SCOPE-1]

**CALIBRATE-THEN-TEST.** The external evaluation protocol pins task corpus, driver
version, repetitions, random seeds, acceptable task behavior, observation duration,
metrics, confidence method and tie-break rule before held-out evaluation. Calibration
chooses numeric limits and thresholds. The frozen protocol hash accompanies every
held-out report. Changing a threshold requires new held-out runs. Reason: a test cannot
choose its pass line after seeing the observations it judges. [FALS-1]

Decoder comparison varies the candidate model only, with the same hardware, workloads,
driver and declared controls. Report each candidate artifact as a variant of that fixed
comparison setup. An unsupported candidate is ineligible, not moved to different
hardware and ranked as the same test. Rank eligible candidates by held-out task success,
then STE100 fidelity, then recall correctness, then lower elapsed time, then artifact
digest as a deterministic tie break. The protocol states its repetition count and
aggregation before execution. [CLASS-7]

Classifier selection first requires conformance and binding correctness on the pinned
corpus, then sustainable two-direction throughput and bounded gate/cut waits. Among
eligible model/instance combinations select the least elapsed overhead, then least
measured memory, then artifact digest and instance count. A stock label scorer is not a
span classifier. Spec 02's compatible adapter and normative scheme are required. No
measured artifact or winner is claimed in this document. [CLASS-2, CLASS-6, CLASS-7,
FALS-5]

**DISK-EVIDENCE.** For qualification, a root-run WeaverAnalysis observer uses Linux
TASKSTATS delay accounting for the state member's exact pid and start identity. It
requires delay accounting enabled and collects blkio_delay_total and swapin_delay_total
at brackets enclosing each evaluated turn and its pending filing. It records kernel
version, counter availability and lost/missed samples. The member's own trace intervals
locate operations within those brackets. A positive interval that overlaps an operation
is examined with scheduler/block-I/O tracing before assigning causality. Unknown
attribution stays inconclusive. [STORE-1, COST-5, FALS-3]

Zero delay increments across complete brackets, together with complete operation
coverage and no counter loss/wrap, can support a zero observed disk-wait result.
Positive delay attributable to a turn's store operation falsifies STORE-1. Sampling with
missing privileges or disabled accounting cannot pass it. WeaverAnalysis owns the
observer under R20-4 of 2026-10-20. Its issue #14 carries the Spec section 1 amendment
for a root collector and one netlink socket, while retaining the no-socket-crate pin.
The observer is operator-run qualification tooling, not an added agent privilege or
resident service. Its concrete probe and synthetic known-wait calibration belong to the
WeaverAnalysis implementation act and remain a cutover evidence dependency. [STORE-1,
FALS-3, SCOPE-1]

On-demand save is tested only where the foundation permits it, at rest. This Spec does
not invent a save during a live turn. Measure capture/publication as lifecycle work
separately, and test ordinary turns under memory pressure without concurrent save
requests. A shutdown save is not a turn's disk wait. A halt writes no save point under
R3. [PERSIST-1, PERSIST-4, FALS-3, October 18 R3]

**MISSES.** A fixture names required content before a run using exact content digest and
task/step address, not a future trace sequence. Analysis binds each fixture to actual
sources through the completed trace and filing receipts. It matches a specified decision
using its task/step and generation ordinal. It compares the filed sources with the
placement map and observed decision. Filed-but-unplaced required material is a selection
miss. Unfiled material is lag, not that miss. [MARK-4, COST-4, FALS-4, DEFER-3]

A live annotation is a hypothesis until WeaverAnalysis, under its issue 14 qualification
work, validates its required source and a controlled replay tests the consequence with
that source placed, under the same declared controls. Analysis records the annotation
and its validation outside the original serving trace. A one-turn-late prefetch
observation additionally names the first later placement of the needed source. It is
distinct from a query budget overflow, whose producer is state.sort.measurement.
[COST-4, PREFETCH-2, FALS-4, October 18 R9]

**VERDICTS.** A pass is limited to the tested tuple, workload, duration and complete
evidence. It is neither universal correctness nor authorization to change a PRD. Each
claim is pass, falsified or inconclusive under these conditions. [FALS-1]

| Claim | Pass evidence | Falsifying evidence |
| --- | --- | --- |
| FALS-2 protocol | Frozen per-task obstruction criterion holds, all refusals/repairs counted | Refusal/cap burden defeats the declared task criterion |
| FALS-3 disk | Complete calibrated brackets show no attributable turn store wait | Any attributable disk wait, including swap |
| FALS-4 selection | Every predeclared required-source decision is accounted for with no confirmed miss | Filed material omitted after a cut causes the required decision failure |
| FALS-5 pace | Frozen load/duration sustains bounded lag and no gate/cut bound hit | Growing lag trend under the declared protocol or either bound reached |

**RELEASE-EVIDENCE.** Qualified means the tested declared tuple completed all
first-release paths with every contract delta integrated and every protective test
accounted for. The artifact is a WeaverAnalysis report with protocol hash, task/run
manifest, trace digests, instrument coverage, mutation results, measured values, all
falsifier verdicts and known limits. Any required inconclusive or falsified claim
prevents qualification. Required paths include seed/load, both classifier directions,
receipts, every typed read, both cut methods, staged next-append confirmation, selection
and placement refusal paths, pair repair on load, both category closure modes,
save/reload and no-publication on a broken seam. The tests exercise primitive ports
directly. They prescribe no strategy or review schedule. [RELEASE-1, FLOW-7, FALS-1]

Each mutation result is {test_id, source_revision, protective_change, baseline, mutant,
artifact_sha256}. Baseline must pass and the altered protection must fail the claimed
observation, not merely fail compilation unless it is a compile pin. Spec questions,
missing scheme/model artifacts, the separate PROMPT-3 document owed under Spec 02 O02-5,
and unfinished C02 through C09 deltas block their corresponding claims. P05-1 is
resolved by R3/R4 and is tested through unclassified fate and boot repair. [PLAN-2,
PROMPT-2, FALS-1, FLOW-7]

## 4. Enforcement and owning amendments

**INSTRUMENTS.** These are agent checks and external acceptance obligations, with
ownership kept explicit. [COST-1 through COST-5, FALS-1]

| Decision | Owner and instrument | Protective change that must fail |
| --- | --- | --- |
| IDENTITIES | Analysis fixture, several tasks share one comparison setup | Put task ID in qualification tuple |
| RUN-EVIDENCE, EVIDENCE-VERIFY | External reader corpus for exact load/trace, build hashes, process starts, memory ancestors, device mapping and unavailable facts, with every reader-table row | Join a different load, replace the source revision with current HEAD, drop a finite ancestor, accept a reused PID or call an unknown fact valid |
| EVIDENCE-SOURCES | Compare load, Generation and external evidence producers at 2eb5c5a2, then exercise paired producer/reader fixtures | Require SPU admission before load, invent a turnless model.measurement, move a device judgment into admin or lose a process instance during tuple normalization |
| CLOCK | Cross-process fixture with known ordered intervals | Serialize process-local Instant or omit origin |
| AXES | Analysis overlap fixture | Sum overlapping waits or mix resource units |
| RECORD-OWNERS | Trace schema/consumer fixture | Count one generation or cap twice |
| OBSERVATION-IDENTITY | Open/load/observer identity fixture | Attribute counters from a reused PID or unknown instrument |
| STORE-OBSERVATION, OBSERVATION-CARRY | Store response corpus and process fixture | Omit a required phase, use the wrong continuation ID, count a partial send twice or encode unknown as zero |
| SNAPSHOT-SAMPLING | Deterministic clock/counter hooks around the real member parse, capture, refusal and encoding paths, plus old-run read/adoption fixtures | Start wall timing after parsing, use operation edges instead of the capture bracket, sample on preflight refusal or ordinary requests, turn unavailable counters into failure, or relabel historical /1 as /2 |
| FLOW-COUNTS | Pump fixture with no receipts and later recovery | Suppress zero-rate interval |
| CALIBRATE-THEN-TEST | Analysis report verifier | Accept post-run threshold or missing protocol hash |
| DISK-EVIDENCE | Root qualification fixture with induced I/O/swap wait | Report pass with absent accounting or dropped samples |
| MISSES | Analysis fixture with known omitted source and separate lag case | Match nonexistent source or confuse lag with miss |
| VERDICTS, RELEASE-EVIDENCE | Analysis report verifier | Qualify incomplete/failed required evidence |

**AMENDMENTS.** The producer clauses below land with this document set. External
implementation remains WeaverAnalysis's work and cannot be supplied by importing an
agent crate. [SEAM-4, SCOPE-1]

| Document | Section | Amendment |
| --- | --- | --- |
| weaver-trace-PRD and Spec | 3.1 and Spec 3 | Add state.store.measurement and feature-owned state measurement kinds, keep score unchanged |
| weaver-harness-trace-contract | 2, 3 and 5 | New measurement payloads, no residue admission |
| weaver-harness-Spec | 4 and 6 | CLOCK, load metadata and STORE-OBSERVATION authoring |
| weaver-state-Spec | 2 and 4 | Operation wall/byte counters and snapshot-only OS samples |
| weaver-harness-state-contract | 2 and 3 | Required observation beside responses, missing optional OS attribution is unknown |
| weaver-types-Spec | 4 | StateOperationObservation and phase/resource/disk shapes |
| toddwbucy/WeaverAnalysis, docs/weaver-analysis-Spec.md | Owed external act, owned by WeaverAnalysis | RUN-EVIDENCE, EVIDENCE-VERIFY and sections 3 and 4's collector, build-record reader, calibration and verdict obligations |

C09-1 is joined through TYPES-OBSERVATION, STATE-OBSERVE and HARNESS-OBSERVE, with
TRACE-OBSERVATION as the record. C09-2 joins the clock origin, member instrument
identity and required flow counters. RUN-EVIDENCE and EVIDENCE-SOURCES define the
external evidence record for facts absent from load, without moving a later SPU fact
into that event or creating another serving port. C09-3 links the new record to
TRACE-REGISTRY and the charter/contract vocabulary. The final producer and finding
checks remain. INV-1 through INV-5 are eventual qualified behaviors, not repairs claimed
implemented. [FLOW-7, COST-2, COST-4]

## 5. Open measured values and deferred triggers

The agent documents leave classifier model/instances, capacities, wait bounds, repair
cap, caller-selected injection shares, tool clocks and observation cadence measured.
WeaverAnalysis also owes the sample sizes, durations, confidence method, calibration
corpus and frozen falsifier thresholds. Missing instruments are explicit evidence
dependencies, not permission to report success. [FALS-1, CLASS-6, CLASS-7, PLACE-3]

Per-interval member CPU/disk sampling is the unselected alternative. It remains open for
a later measured amendment if snapshot-only lifecycle evidence and the external observer
prove insufficient. No timer or background sampling worker is added here. The
qualification tuple is the exact identity described by IDENTITIES and RUN-EVIDENCE,
including the instrumentation and host facts needed to compare these measurements.
[COST-1, COST-2, FALS-3]

Spec 10 waits on first-release proof. Spec 11 waits on confirmed one-turn-late material
costing a decision and has no KV prerequisite. Spec 12 waits on structured retrieval
misses and overflows. Specs 13 and 14 wait on world-content and safety screen needs
respectively. Their interface notes remain separate from any far-side design. [DEFER-1
through DEFER-5, PREFETCH-1 through PREFETCH-3]

[transition]:
state-management-foundation-Spec.md#1-applicability-and-the-serving-cutover
