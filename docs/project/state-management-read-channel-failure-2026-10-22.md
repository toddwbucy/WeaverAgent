# State failure while recall owns recording

## Scope and authority

This slice answers #229 Planner review 5609135461 row 7 under issue #146. A pending
recall owns its recorder ticket and must prevent unrelated flow reduction, while an
observed state-channel failure must retain its real reason. The original read and
lifecycle bounds remain in force. No serving replacement or coupled cutover is
activated.

Implements: weaver-harness-Spec-amendment HARNESS-RECALL, HARNESS-RECORD-LIFETIME,
HARNESS-PUMP and HARNESS-HALT's failure attribution table, limited to this boundary.
Author grades remain pending compilation, affected gates and protective mutations.
This record does not claim Planner verification or production qualification.

The implementation adds one passive check to RecordingContext::live. After the existing
context, coordinator and recorder failure checks, it inspects the standing state
channel's retained failure before checking deadlines. Existing flow_failed mapping
preserves CarryFailed or ProtocolFailed, its channel point and detail. The existing
first latch remains immutable. An absent intentionally retired state channel adds no
failure. No transport is read, author resumed or unrelated completion reduced by this
inspection.

## What the reported failure means in the current owner

The inspected parent is boot/read head ad53706bf786cb2f633e2880a3e24dc2e37dbb27.
The common pump already includes the standing state fd during pending recall and
maps errors from the real StateChannel parser through the coordinator immediately.
The new check covers a channel failure already observed but not yet mapped by that
coordinator. The tests distinguish these two boundaries rather than describing a
synthetic unmapped state as a reproduced normal pump path.

Actual socket tests shut down the scripted peer or write unsolicited bytes while the
real trace writer is full and recall holds one author ticket. They require prompt
CarryFailed or ProtocolFailed at StateReply, no RecordedRead, and the unchanged ticket,
payload, original bound, request count and completion ownership. A boot test uses the
real load/template setup and requires no BootHead after the same EOF.

A test-only coordinator helper invokes the existing channel parser without its usual
failure mapping. It creates the exact intermediate channel latch from actual socket
I/O, without assigning a fabricated failure enum. Local advancement must then observe
that status before the bound, and retain it after the original bound passes. This
fixture deliberately isolates the passive status check and is not a claim that
production transport_one normally discards its error.

Additional cases preserve an existing context latch, preserve the existing precedence
of a known recorder failure, retain a recoverable Leave wait, and keep ControlBound
for a healthy channel whose pending recall reaches its original deadline. Existing
recall ownership tests continue to prohibit generic reduction or a competing author.

## Validation and protective changes

Validation is pending. Root owns the serialized lock, Cargo and formatting gates,
using --locked and a private target. No runtime result is asserted by this record yet.

The mutation plan removes the passive check and runs two distinct expectations. The
actual pump and boot EOF tests are expected to pass unchanged, establishing that their
transport mapping pre-existed this guard. The isolated real-parser latch tests must
fail. Moving the check after deadline expiry must lose the pre-observed channel cause.
Moving it ahead of a known recorder failure must violate existing precedence. Changing
the channel failure mapping to a bound reason must fail the real pump regression.

All behavioral mutations must compile, fail their named assertion when applicable and
pass after exact source restoration. The positive baseline probes are recorded as
passes, not counted as protective failures. Review row 7's broader claim is not treated
as reproduced unless those results establish it.

Capture-frontier semantics, read phase representation, neutral fixture relocation and
number/SourcePosition cleanup remain separate review dispositions. This change adds no
socket reader, retry loop, author reserve, state request or fresh wait interval.
