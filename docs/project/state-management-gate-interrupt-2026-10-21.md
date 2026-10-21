# Gate fault interrupt implementation record

Date: 2026-10-21. Status: draft PR #215, Planner grade owed.

## Scope and authority

The slice implements GATE-FAULT-INTERRUPT in the merged
[gate amendment][gate] sections 1 and 2, the paired wire in the
[contract amendments][contract] section 4, and the cause vocabulary in
[Spec 05][pairs]. The code remains in the gate primitive. The loop selects when
an interrupt is needed. The store remains weaver-state.

The branch starts at a1ce2df5, the consolidated #198 head after #214 merged.
It supplies both ends of the changed wire. Every existing unload sender now
sends ToolInterrupt with Unload explicitly. FlowFailure is a separate cause in
the shared wire and the trace mirror. The store still refuses to file a
flow_failure terminal account through a broken flow.

This wire replacement applies only at the coordinated foundation cutover.
Keep this draft stacked on #198 until that cutover is ready. Its code is not
an independently landable update to the old serving contract. The failed-flow
caller, lifecycle teardown and trace-only terminal account remain H1b2/H2 work.

## Contract implemented

Only Unload and FlowFailure are valid interrupt commands. Other decoded causes
fail the channel protocol. Invalid input still leaves the running call's
containment duty intact. An idle valid interrupt refuses OutOfOrder. A late
control for an answered exchange cannot produce a second close, including when
a later call is running.

An execution answer is privately owned until its terminal datagram is sent.
The gate kills the tools cgroup, observes it empty and reaps its leader before
it can send that answer. Containment and reap use one original deadline.
Repeated interrupts do not renew it. An unsent Unload can become FlowFailure,
including under socket pressure. Clock, cancel and completed result outcomes
keep their first cause. A sent answer cannot change.

The gate never sends Fault as proof of containment. Failure to contain leaves
the execution unanswered and exits through the existing gate-death path.
No new harness execution clock or measured timeout is introduced.

## Validation

The restored full suites pass: gate 75, harness 952, state 188, trace 137 and
shared types 115 tests. The types config-feature suite separately passes 159.
The gate default suite has 56 ignored instruments. The delegated lane runs 31
of them explicitly: all 28 library shell and confinement tests and all three
served-binary tests pass. This includes all four new containment cases. The
other ignored instruments remain under their existing runners.

The new live fixture holds a real process in tools after the initial kill,
requires the execution thread to observe populated 1, and requires no answer
until that process is released. The test then observes populated 0. Socket
fixtures separately cover backpressure, late exchanges and invalid causes.
Twelve new host tests and four new delegated tests run, with one additional
idle-dispatch test and the expanded existing late-control serving test.

Nine isolated protective changes compile and fail their named assertions.
They remove or alter valid-cause acceptance, invalid-cause rejection, unload
escalation, late-control handling during execution, idle late-control handling,
send-time escalation, the fixed deadline, the actual cgroup-empty wait and the
trace cause mirror. Every changed file is restored to its exact saved hash.
The restored full suites and delegated containment suite pass again.

All changed-crate all-target lint gates pass, including both shared-types
feature modes. Formatting, lock, diff, ASCII, editorial and local-link checks
pass. Independent local review found and verified the fix for a late old-call
interrupt while a new call runs, and verified the strengthened containment
instrument. No local review finding remains open. This is not a Planner grade.

The checkout uses /tmp/weaver-state-gate-target as its private Cargo target.
Final tests disable incremental caching after an initial link attempt filled
/tmp. Only this checkout's disposable cache was cleared. The attempt is not
reported as a test failure or a passing gate. An initial all-ignored invocation
also selected drain instruments outside their root-namespace runner. The final
delegated command selects the proper library and binary targets instead.

The delegated command is systemd-run --user --scope -p Delegate=yes with the
repository-built pinned BusyBox at /tmp/weaver-state-gate-busybox/busybox.
It runs cargo test -p weaver-gate --lib --bin weaver-gate --locked with
--ignored --test-threads=1. No artifact is installed or deployed.

Local evidence is under /tmp/weaver-state-gate-*.log and
/tmp/weaver-state-gate-mutations/. The restored tests are in
weaver-state-gate-restored-tests.log, the delegated result is in
weaver-state-gate-delegated-restored.log, and the mutation directory records
individual outputs and the restored hashes.

## Remaining work

The loop must later connect the halt latch to this primitive and preserve the
result as trace-only evidence after flow failure. Production language artifacts,
serving migration, boot, executing pair custody, placement, plan and strategy
composition remain tracked in the [implementation map][map]. Independent local
review and test evidence do not replace the Planner grade or operator approval.

[gate]: ../crates/state-management/weaver-gate-Spec-amendment.md
[contract]: ../crates/state-management/contract-amendments.md
[pairs]: ../crates/state-management/state-management-spec-05-tool-pairs.md
[map]: state-management-implementation-plan-2026-10-20.md
