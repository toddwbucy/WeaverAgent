# Harness transport progress primitives

Date: 2026-10-21. Status: implementation evidence, not a Spec amendment.

The merged [harness amendment][harness] requires a common pump to keep transport
moving while one trace event waits for capacity. The existing decode receive reads
an entire segment series in one call. Organ reads and sends block, and the label
reader waits against its own clock. Wrapping those methods in a poll loop cannot
meet HARNESS-PUMP. This slice supplies their bounded transport replacements.

## Contract and ownership

Each adapter consumes its existing channel owner. It keeps the same connected Unix
SEQPACKET end and supplies a borrowed readiness descriptor. The old reader cannot
continue after that move. A retired label channel cannot become a live adapter.

Organ traffic remains OrganEnvelope. Decode traffic remains TokenDirective and the
existing answer or refusal, including the exact segment preamble from types Spec
4.4. Label traffic keeps its directive, answer and refusal. A refusal is a received
value. These adapters hold no exchange-order policy or timeout. They cannot turn a
received answer into permission to make a decision.

Each progress call attempts bounded socket work. Partial decode messages and pending
sends remain owned across calls. Each source holds one complete reply. Its full slot
prevents a second receive, while terminal status remains observable. Taking a reply
transfers it once. The loop still owes its trace record before applying its effect.
An observed transport failure preserves any held reply and partial evidence. A valid
terminal reply followed by EOF must remain available to the caller that judges its
lifecycle phase.

Normal traffic carries no ancillary descriptors. The new adapter refuses such a
packet and closes every delivered descriptor. The existing enter-only receive owns
the initial sink and state-end transfer. Its replacement belongs to the paired
lifecycle migration, where the complete Enter producer and consumer change together.

The wire formats, limits and custody follow [the organ mechanics][organ], the
[decode contract][decode] and the [label contract][label]. No new dependency, worker
thread, scheduler, clock or declaration election is added.

A full reply slot requests half-close status without ordinary read readiness. The
shared wait preserves raw Linux poll flags, including POLLRDHUP. The pinned nix
decoder would discard that unknown bit and lose the entire readiness result.
Keeping the raw flags preserves the existing absolute-deadline and interruption
rules while letting the source observe termination without consuming another reply.

## Integration still owed

The complete legacy serving path stays active until the foundation cutover. These
adapters are mechanics for the run-wide RecordingContext, not an alternate serving
path. That context still owes preflight open/load, the enter transition, held
completion reduction, the author-call census, a first-failure latch and reserved
terminal reporting. PumpPorts must cover both SPU lifecycle and admin coordination
ends, as well as gate, decode and label traffic. State and STE100 keep their own
contracted transport owners.

The common pump must select the nearest existing absolute bound, check it around
each bounded work unit, and process buffered work before waiting. It must preserve
one author ticket and record each completion before its effect. None of those
orchestration claims follows merely from a transport fixture passing.

## Verification scope

The transport fixtures use real Unix socket pairs and the existing shared wire
types. They exercise fragmentation, backpressure, typed refusals, held replies,
terminal faults and custody. A trace-pressure fixture tests their use beside the
existing recording barrier. It does not certify the future scheduler, production
decode budget or qualified cutover. The PR records the exact commands and results.

The full harness suite passes 561 tests with 17 inherited ignored instruments. The
20 new transport tests and the new raw-readiness clock test run. Eleven compiled
guard removals fail their intended behavioral assertions, then all source bytes are
restored. The restored transport suite passes 20 tests and the clock suite 13.
Independent review verifies the failure logs and restoration hashes. All-target
lint, formatting, lock and diff checks pass.

## After-action review

The planned scope was bounded transport progress with the existing wire contracts.
Composing full reply slots with the real wait exposed the pinned flag decoder's
loss of POLLRDHUP. Preserving raw readiness closes that defect without changing a
deadline. Initial compilation found a generic helper recursion, corrected by a
separate status helper. Initial lint findings were fixed before the final gates.
Neither failed run is counted as passing evidence. Keep composition tests alongside
the byte tests, and carry the outstanding lifecycle census into the context act.

[harness]: ../crates/state-management/weaver-harness-Spec-amendment.md
[organ]: ../crates/contracts/weaver-organ-channel.md
[decode]: ../crates/contracts/weaver-harness-spu-decode-contract.md
[label]: ../crates/contracts/weaver-harness-spu-classify-contract.md
