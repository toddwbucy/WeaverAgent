# State management: worker input composition

Date: 2026-10-23. Author check for Issue #146.
Branch: `code/state-worker-input-composition`.
Parent: #303, `611c07f69467117196f8ac8272e82d9fa0d2e7c0`.

## Completed connection

The dormant worker/dev_loop now drives a real gate request from the successful
startup seat through the existing input check and source-authoring primitives.
FoundationInput owns the original received envelope and raw bytes, actual gate
exchange, same context, state/classifier owners, listener, residency/decode ports,
child custody and template certificate. It exposes no raw recorder, socket, accepted
token constructor, placement or generation grant.

Eligible input validates its actual port, gate opener, Open position and unused
input exchange ordinal. Busy intake retains the frame without assigning an input ID
or deadline. input.offered records before parsing. The existing classifier checks
exact text, and the original Enter operator and gate-authenticated dialer determine
origin. Malformed shape, role and language refusal use the existing typed records;
a missing authenticated dialer remains a boundary failure.

The refusal owner carries the primitive's exact unnamed close on the original
exchange through a tracked send. Busy and partial progress keep custody. Only the
whole actual send permits the response acknowledgement and a return to the same
serving seat. Gate wait ends at Recorded disposition, and no replacement timeout is
armed for the close. A refused request followed by a fresh accepted request works on
the same seat. A queued frame behind accepted input acquires no input ID or clock.

The loop chooses a turn identity after acceptance and consumes the token once. The
existing primitive records turn.started and message.user, links input.offered as
causal parent, and retains the selected copy with unchanged checked bindings. Only
then does FoundationTurn expose immutable message/source metadata. It still owns the
original open gate exchange and serving resources. Filing acknowledgement and the
later cut barrier remain owed before a model generation; this handoff grants none.

The common pump adds a bounded observation mode that uses its existing clocks,
readiness probes, buffers and transport owners. With nothing ready it returns Idle
or an actually due Observation without waiting. Ordinary waits are unchanged. This
observes terminal failure before source admission or close completion, without a
second reader or an invented deadline. Fatal transport stops latch the context and
revoke input while preserving the typed cause and original source for teardown.

Gate and residency fault reports remain opaque facts authored through the common
recorder. A newly accepted root connection is retained as ManagementPending. It is
not a judged Leave and does not itself cancel input. Pending/failed finish calls
return the entire owner. Stopped custody is not completed teardown or a delivered
halt response. The old Enter peer is not serviced after aggregate Ready; its normal
close cannot revoke the serving seat.

## Evidence

Private Cargo target: `/tmp/weaver-146-boot-composition-target`. Lock first, serial
Cargo with `--locked`, and incremental compilation disabled.

- Harness: 1941 passed, 99 ignored, excluding duplicate subprocess summaries.
- All 19 new delegated/root cases pass separately; all 17 worker startup and 45
  earlier delegated composition cases also pass. Eighteen inherited ignores remain.
- Thirteen compiled protective mutations fail as intended. All 259 harness Rust
  source hashes restore exactly.
- External compile checks positively accept the public input owner and reject raw
  serving access (E0616), consuming a borrowed input owner (E0507), and cloning the
  turn (E0277). Persistent doctests cover the same three negative properties.
- Harness clippy, formatting, initial/final lock, diff and three DocsMirror checks
  pass.

The circuits reuse complete root-authenticated startup, actual forks, sockets and
contained gate resources. Gate/member/classifier/model responses remain declared
protocol doubles. Two circuits execute the exact worker/dev_loop input function,
with MonotonicIo from actual Enter adoption and continuous member response. Their
five-second gate budget and 100 ms observation interval are fixture values, not
measured production limits. The other circuits pair the fake clock and wait.

Initial instruments incorrectly expected ordinary refusal for missing dialer; the
existing boundary-failure rule was retained and the expectation corrected. The first
observation prototype capped a wait at now, which correctly failed the pump's
no-early-timeout guard; the final implementation uses an explicit observation mode.
The actual-clock accepted circuit needed its own declared timing values and a
continuous member responder instead of the paired fake fixture's one-ms budget.

Direct author review, compile pins and protective mutations are author evidence.
The required review/verification skills are unavailable in this environment. No
independent Planner, security or production qualification pass is claimed. Exact
published source, logs and scripts accompany Issue #146's authorized evidence deposit.
Fixture cleanup reaps children but does not establish production teardown.

## Authority and remaining composition

| Implements | Author verdict in this act |
| --- | --- |
| Spec 02 section 5 INPUT / REUSE; harness amendment section 2 HARNESS-INPUT | Conforms for actual gate intake, checked acceptance, refusal delivery and retained source handoff exercised here; judged Leave/cancellation and failed-run completion remain owed |
| Harness amendment sections 1 HARNESS-OWNERS / HARNESS-AUTHOR / HARNESS-AWAIT-AUTHOR and 3 HARNESS-PUMP / HARNESS-CLOCK | Conforms for shared owners, original attribution, common author and bounded observation; full Engine/Ports migration remains owed |
| Harness-gate contract sections 1-3; organ-channel section 1 | Conforms for retained actual frame exchange and exact response carry |
| Foundation section 1 FOUNDATION-TRANSITION | Conforms for retaining the active serving path until qualified coupled cutover |

Next joins are management judgment/cancellation, the checked turn's workflow and
selection/placement/generation ports, and actual failed-run/final teardown. The
admin producer and worker still need one coupled run. Legacy author/Ports and
command/show migration, approved but unmerged rules, reviewed language/classifier/
prompt artifacts, measured limits and qualification remain owed. No active worker
entry, deployment or complete Issue #146 checklist item is changed. Standalone
review followups remain deferred unless they block composition.
