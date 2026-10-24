# State management: worker startup composition

Date: 2026-10-23. Author check for Issue #146.
Branch: `code/state-worker-startup-composition`.
Parent: #302, `53a3933b9f5469df63b1cb78af36250e77a05012`.

## Completed connection

The worker can now own replacement startup through opaque harness types. Admission
uses the existing root credential check, sole recvmsg reader and actual SCM_RIGHTS
Enter. It retains the original bytes and peer through adoption, including failures.
The outer `FoundationRun` owns one RecordingContext and FailedEnterResources. Startup
and serving borrow that same run rather than recreating context or cloning streams.

The compiled program supplies organ paths, host SPU parameters, composer identity
and static templates. The original Enter supplies runtime instructions, shell
election, declaration, start identities and scheme text. A failed startup join keeps
the original source in the outer run. No caller can supply a replacement load event,
gate instruction, state stream, author or placement grant through this API.

`worker/dev_loop/foundation.rs` drives the existing fixed lifecycle stages: state
Open, Recorded load, semantic activation, initial Grants, classifier admission, SPU
residency, template checks, prompt/Open/boot repair/summary/install, gate Raise and
aggregate Ready. Each transition consumes only a completed owner. Pending and
stopped stages retain their actual resources and evidence; failed consuming joins
return the owner. Only the whole original-exchange Ready grants `FoundationServing`.

Later management peers remain queued while the original Enter occupies the pump's
primary peer. The completed serving seat retains that listener. Same-peer Stop and
EOF stop the startup owner. No second reader, timer, workflow policy or runtime
selection is introduced. Public unsafe advancement carries the existing serving
thread/child lifetime contract. A returned stopped owner still owes cleanup.

The active worker entry point and dispatcher are unchanged. The new dev_loop
function is compiled and exercised by the circuit instrument, but is not selected
for a deployed agent. The earlier true-remaining-Enter-budget caller duty remains;
unmerged #282's original window carrier is not implemented.

## Evidence

Private Cargo target: `/tmp/weaver-146-boot-composition-target`. Lock first, serial
Cargo with `--locked`; incremental compilation disabled for these runs.

- Harness: 1937 passed, 80 ignored, excluding duplicate subprocess summaries.
- All 17 new root/delegated circuits pass separately, as do the 45 prior delegated
  composition cases. Eighteen inherited ignored instruments remain.
- New cases cover the real dev_loop, successful resource handoff, each component's
  refusal, malformed/missing-handle Enter, root peer enforcement, original Stop/EOF,
  exact remaining allowance, failed-start source retention and queued management.
- Eleven compiled protective mutations fail as intended. All 257 harness Rust
  source hashes restore exactly.
- External compile instruments positively accept the public owner and reject raw
  context access (E0616), consuming a borrowed startup owner (E0507) and cloning a
  serving seat (E0277). The same properties have persistent doctests.
- Harness clippy, formatting, initial/final lock and diff checks pass.

The full circuit uses real root-peer judgment, descriptors, fork/exec, child custody
and gate containment in isolated delegated cgroups. One case executes the exact
worker/dev_loop function with MonotonicIo and a separate member responder. Other
cases use the paired fake clock/wait instrument. Member/classifier/model responses
and template words are declared doubles; no SQLite/model/language qualification is
claimed. Fixture teardown reaps actual children but is not production cleanup.

The first run's two queued-peer expectations assumed concurrent management accept.
The existing pump deliberately serializes that listener behind the active original
peer. The tests were corrected to verify preservation of the queued connection
through Ready; pump scheduling was not changed.

Direct author review covered all public constructors, consuming transitions,
retained failure owners and the sole production caller. Requested review/verification
skills are unavailable here. These checks are not an independent Planner or security
grade. Exact published source, scripts and logs accompany the authorized evidence
deposit for Issue #146.

## Authority and remaining composition

| Implements | Author verdict in this act |
| --- | --- |
| Harness Spec sections 2, 3 and 6; admin-harness contract sections 2, 3 and 5 | Conforms for the worker-facing original Enter, fixed startup sequence and successful resource handoff exercised here |
| Harness amendment section 1 HARNESS-OWNERS / HARNESS-RECORD-LIFETIME | Conforms for one owned context and opaque borrowed seats; full runtime migration remains owed |
| Foundation FOUNDATION-TRANSITION | Conforms for retaining the current serving path until qualified coupled cutover |

The admin producer and worker adapter still need one coupled end-to-end run. Serving
turn orchestration, public checked operation ports, full failed-start/healthy rollback
and final teardown, active command/show migration, the reviewed production template,
language/classifier/prompt artifacts and measured limits remain owed. Held proposal
semantics still require review and merge. No merge, deployment or Issue #146 completion
is claimed by this slice. Standalone review followups remain deferred unless they block
composition, as the operator directed.
