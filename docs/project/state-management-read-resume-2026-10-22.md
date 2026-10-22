# Read continuation ownership after a lifecycle yield

This act answers PR #225's Planner review 5618145792, row 1, and advances
issue #146's recall/provenance and ownership work. Its base is exact #225 head
47bc7b964afe0eea1d0f209d605667e73bfa425e. The child remains a draft while the
parent's security review runs. Only the operator merges.

## Finding and correction

A recorded Released answer can be consumed and validated immediately before a
recoverable lifecycle wait. The read retains its accepted continuation, but has
not yet installed recall ownership. On resume, the coordinator previously ran
first. A held classifier result or queued flow observation could acquire its
recorder ticket; real backpressure then exposed the conflict when the read
attempted to record recall. The result was a latched ProtocolFailed/StateReply,
with detail "recall author has an unsettled predecessor".

The reproduction uses actual selected-source classification, state Read/Release
frames, recorded operation completions, and a full Unix-socket-backed recorder.
A second lifecycle yield around a due observation produces the queued Sampled
case without constructing a private notice or pending recorder state. This is a
demonstrated failure of progress, not evidence of unrecorded content consumption.

RecordingContext::reduce_flow now resumes only an already accepted read before
the coordinator advances. The initial live check still gives failures and expired
bounds priority. The existing recall guard then excludes coordinator authors
until recall is Recorded. Settlement after coordinator advancement still handles
new operation completions. There is no new deadline, protocol, refusal or owner.

Implements: weaver-harness-Spec-amendment HARNESS-PUMP, HARNESS-READ,
HARNESS-RECALL and HARNESS-RECORD-LIFETIME. The author grade is conforms for this
retained-continuation correction only. Complete serving migration and
qualification remain outside this act.

| Related ownership path | Inspection result |
| --- | --- |
| Ordinary reads, Boot and Marks | Share the corrected read core; preserve original StateControl and applicable Enter bounds |
| Capture | Entry points already settle retained completion/cleanup before ordinary advancement |
| Input | No analogous recoverable check after consuming its completion and before installing the owner |
| Measurement, output, freshness and pair handoffs | Retain their author stages or transfer ownership before another recoverable check |
| Other reduce_flow callers | The prepass is inert without an accepted read; admissions already exclude an incompatible read owner |

The related-path inspection is a bounded code review, not new integration
qualification. Existing refusal, page-continuation, deadline and state-channel
failure tests remain part of the harness gate.

## Protective evidence

Three new tests exercise ordinary and cancelled reads with a held classifier
reply, and a clock-boundary sweep with a due sampled observation. They require
one recall, exactly Read plus Release, retained bounds while incomplete, no
premature consumption, and progress of the original deferred classifier result
or sampled observation after recall. The classifier's exact payload and identity
are compared with the actual reply sent through its socket.

Removing the resume prepass compiled and failed all three tests with the actual
unsettled-predecessor fault. The observation test logged a real queued sample.
Removing the post-resume recall guard compiled and failed both classifier tests.
Both mutations were restored. The initial reproduction and failing mutation logs
are retained alongside passing runs, not replaced by their later outcomes.

## Validation and limits

The restored full harness suite passed 1,331 tests, with 18 inherited ignored
cases, using `cargo test -p weaver-harness --locked -- --test-threads=4`.
Strict all-target harness lint, formatting and the lock gate passed. The initial default
parallel full-suite run had one failure in the unchanged pump fixture's
pressure setup: its five-second socket write timeout returned EAGAIN. That
fixture passed in isolation; the complete four-thread rerun then passed. Both
full-suite logs remain in the evidence. Ignored cases are not claimed as passes.

Evidence is local to Olympus at
/tmp/weaver-146-evidence/read-resume-225/. The private target is
/tmp/weaver-225-read-resume-target. Dependencies are locked and Cargo/Git
mutations are serialized. No shared-storage deposit is claimed. The first
reproduction command accidentally selected zero tests with an unqualified exact
filter; its log remains and is not counted as a pass.

The exact code-review skill named by CLAUDE.md was unavailable in the local
catalog/search. The author used independent read-only review via the available
requesting-code-review workflow and applied verification-before-completion.
These checks do not substitute for the Planner or third-party reviews.

The investigation began with a plausible but uninstrumented review concern;
combining real producer work, a retained completion and recorder pressure made
it reproducible. Future continuation reviews should test competing producers at
recoverable yield boundaries as well as the isolated owner. The fixed change
does not activate serving, finish issue #146 or qualify production artifacts.
