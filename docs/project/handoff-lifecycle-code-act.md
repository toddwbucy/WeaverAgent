# Handoff: the lifecycle code act

**Version:** v0.2, 2026-10-09. From the Planner seat to the Executor seat, per the
Handoff Format. Governs the code that implements `weaver-admin-Spec` section 3.0, the
lifecycle state table, as merged in #109.

The act brings the agent's code to the lifecycle Spec merged in #109, row by row. The
base is #109's merge commit on `main`. The parked branch `code/lifecycle-protocol`
(2263196) is raw material, not a base: it holds much of the graceful unload, the
wind-down, the join and `Left.forced`, written against a `main` that predates #100 to
#108 and against rulings since superseded. Take from it what the merged Spec says, and
drop the rest. A trial merge onto 451fef0 met 25 conflicting hunks in six code files, 11
of them in `crates/weaver-harness/src/lifecycle.rs`. Its document changes are
superseded by #109 and are not carried.

**The act is bounded by the operating envelope**, `weaver-agent-PRD` section 6.1: one
command at a time, the single admitted overlap being `force-unload` joining an `unload`;
an exact outcome for every constituent death inside it; conservative recovery outside
it. A scenario outside the envelope is not built for: it is on #110, and a finding that
needs it is answered by citing 6.1, never by code.

The work order is #109's body, section "Work order for the code act": each row names a
Spec row, a code site on 451fef0, and what changes. Every row is either done in this act
or named as moved, with the reason. The act closes #101 and #107's lifecycle items (W2,
area 1's R2, R4). It owes the live run that proves it: `deploy/verify-lifecycle.sh
--apply` on a throwaway agent, driving reload with state carried, restore,
force-unload and recovery after a crash. That run is what "lifecycle working in the
code" means, and state management waits on it (the operator's ruling of 2026-10-09).

## 1. Method: the decisions first, test first

Three of the Spec's tables are decisions, not protocol, and each is built as a pure
function before any plumbing touches it:

| Table (section 3.0) | Owning crate | The function |
|---|---|---|
| the conclusion's outcome table | `weaver-admin` | `(the Left's kind, forced, save point, publication) -> (marker, reset reason, printed answer)` |
| the operator verbs (state x verb) | `weaver-admin` | `(state, verb) -> (next state, what is recorded, what is answered)` |
| the in-run events, and the organ deaths (state x event) | `weaver-harness` | `(state, event) -> the harness's response` |

For each table:

1. **Write the test first**, transcribed row for row from the Spec's table: one case per
   row, the inputs in and the expected outputs out. It fails before the function
   exists. Name each case after its Spec row, so a failing case points at the row.
2. **Write the function as one exhaustive `match`, no wildcard arm**, its inputs enums,
   so a new kind of input does not compile until it has a row. This is the compile-time
   pin that section 10 names.
3. **Perturb**: change one row's expected output in the function and confirm the
   matching case fails; restore it. Record which rows were perturbed in the pull
   request body.
4. Only then **the I/O**: publication, the marker write, the directives, the gate. These
   take the decision's result and apply it, and are tested by integration cases, a few
   per path, not one per row.

The cross-process parts (the quiesce and drain, the join, the S7 seal, the holder verb,
the escalation, the release before `unload`, the first-outcome-wins race at the gate)
are not tables. They are tested after they are written, each test perturbation-verified
against the invariant it names (I1 to I9), as section 10 lists.

**Skills, as working method** (the operator's direction of 2026-10-09). The process
documents and CLAUDE.md decide what is done; these shape how:
- `superpowers:writing-plans` first, turning this handoff and the work order into the
  Executor's own step plan before any code. An ambiguity found there comes to the
  Planner, not into the code.
- `superpowers:test-driven-development` for the three decision functions of section 1:
  the failing case first, then the code.
- `superpowers:verification-before-completion` before any claim that a step passes: the
  command's output, not a summary of it.
- `superpowers:receiving-code-review` when answering the Planner's grade and Codex's
  findings: verify a finding against the code and the Spec before fixing it, and
  answer one that is wrong with the reason rather than a change.

**Four reviews, each a different lens** (the operator's arrangement of 2026-10-09):
1. **The Executor's self-check**: the `code-review` skill at `high` effort on its own
   diff before opening the draft, for correctness bugs, its findings fixed or answered
   in the body on the severity line below.
2. **The Planner's grade**: conformance, each work-order row against the Spec row it
   names, and the gates on a clean extract. It does not repeat the self-check.
3. **Codex**, once undrafted.
4. **The operator**, who merges.

The act stays in one session. The harness and admin halves interlock through one
protocol, so they are not split across subagents: a split is how stale cross-document
sites reached #109's review.

## 2. Order

1. **The floor**: the wire and trace vocabulary in `weaver-types` and `weaver-trace`
   (`JoinLeave`; `Leave.rollback`; `Left`'s `forced` and `no_state`; `Quiesce`/
   `GateQuiesced`; `ToolInterrupt`; `KillCause::Unload` and `KillCause::Fault`;
   `Unloading`; `StopReason::Unload`; `forced_by`; `UnloadClose.release` with
   `MemberRelease`, optional at read; `EnterPayload`'s two bounds; the `[lifecycle]`
   table's parse, a lone bound included).
2. **The three decision functions**, test first, per section 1.
3. **The harness and the gate**: the graceful unload's steps, the wind-down, the forced
   leave, the join (an unload's leave only), the S7 seal's steps, `Observe` answering
   `InTransition`, the release before `unload`, `ToolInterrupt` before `Quiesce`, the
   first outcome winning, and the dead-organ rulings: (A) a graceful leave with the gate
   or the SPU dead turns forced; (B) the SPU dying closes the turn failed and later
   requests refuse `NoResidency`.
4. **Admin**: the holder verb written to `admin.lock`, and the force reading it; the
   conclusion on the decision function, lock first, then the marker rule; the force's
   paths (join, the post-seal wait split by the run lock, escalation, the S8b no-worker
   path); the `show` ladder; the bounds passed on the enter and admin's own deadline;
   `verify-lifecycle.sh` brought to the outcome table.
5. **The live run**, `deploy/verify-lifecycle.sh --apply`, then `--cleanup --apply`.

Items 1 and 2 may land as their own pull request ahead of the rest, since they are fast,
certain and unblock both crates. Say in the draft which you choose.

## 3. The review line, agreed before review

This is a code act, so many Codex rounds mean the Spec was not finished, and a finding
that needs a Spec change stops the round and goes to the Planner rather than into the
code. Every finding is sorted against the envelope first: outside it, it is answered
by citing `weaver-agent-PRD` 6.1 and filed on #110. Otherwise, the severity line of CLAUDE.md: fix what bites in normal use, loses or
restores stale state, or crosses a privilege boundary; batch documentation and
consistency; file crash windows and exotic races.

## 4. Gates

Private target per checkout, outside home. `process/gates/lock.sh`; `cargo fmt --all --
--check`; clippy at `-D warnings` for each crate touched (weaver-types, weaver-trace,
weaver-harness, weaver-gate, weaver-admin); `cargo test --workspace --locked`;
`python3 deploy/test_plans.py`; and the preload-door instruments where the state seam is
touched.

    Base            c304e75, #109's merge on main
    Raw material    code/lifecycle-protocol at 2263196 (not carried whole)
    Authority       weaver-admin-Spec 3.0; harness Spec 6 and 8; gate Spec; the
                    admin-harness, harness-gate, gate-world and admin-operator
                    contracts; trace and types Specs, as merged in #109
    Work order      #109's body, "Work order for the code act"
    Closes          #101; #107's W2, area 1's R2, R4 (and its untried lifecycle legs,
                    by the live run)
    Out of scope    #110 (out of the envelope); #111 (tool shell containment)
    Asked           a draft PR, `Implements:` naming 3.0's rows and the invariants,
                    with a verdict per row of the work order
