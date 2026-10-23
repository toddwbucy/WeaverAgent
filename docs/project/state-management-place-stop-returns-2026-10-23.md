# Placement custody after Stop

Date: 2026-10-23. Issue: #146. Branch: code/state-place-stop-returns.
Parent: exact #272 044d1049b8f2966ed31f3c4a7744c7c2070f8823.
This record names the branch before its draft PR number is assigned.

## Scope and authority

This slice answers fix rows 1 and 2 of #269 Planner review 5624951821.
A turn-scoped Stop could leave a confirmed reload reservation inaccessible,
or leave completed native marks holding their owner indefinitely. Both blocked
later work while the run remained Entered.

Implements: Spec 06 RELOAD-STATES, CUT-RELOAD, RELOAD-CLASSIFY, STAGED-APPEND
and MARK-POSITION; harness HARNESS-PLACE and turn-scoped Stop; Spec 07
post-admission settlement of committed facts. Author assessment: conforms at
these dormant owner boundaries, subject to Planner verification.

A cancelled reload account can now transfer its exact fully Recorded Staged
reservation once. Confirmed cut remains a persistent fact after extraction,
so the old pre-cut inputs cannot be returned even after a no-op Elide. A
complete no-mutation account can instead return its original prefix and tail.
Returned custody distinguishes the original tail, already filed ordered units
and the actual rejected source disposition. A genuine rejection refuses retry
conversion and returns the same object. Otherwise conversion retains each
consumed pending SourceLease at its original index and uses the exact derived
Held unit. Unresolved pending inputs stay pending. No source is reclassified
or filed twice, and no capability is reconstructed or cloned.

Fully settled cancelled native marks can leave their owner in Entered only
after every original position has Recorded. Completed native scope facts reach
the live engine as truthful settlement of an admitted committed effect. The
account retains its original proposal, prerequisite, bound, positions and
Recorded sequences, including completion that preceded Stop. Incomplete,
Failed and Releasing accounts cannot adopt successful native state.

The original Decision remains unchanged. Stop during a turn still bars work
until its actual TurnClosed records. At-rest Stop creates no run-wide latch.
Leave and JoinLeave continue to refuse later append and turn admission.
Settling recorded facts grants neither placement nor generation. Failed and
Releasing extraction remains trace-only and cannot revive the engine.

## Callers and validation

The new consuming paths are PlaceAccount::take_staged for Reload,
PlaceAccount::take_reload_inputs, ReturnedReloadInputs::into_retry and
RecordingContext::take_cancelled_place_marks. Their current callers are the
owner fixtures. Actual worker/dev_loop composition remains separate work.

Nine new tests drive actual owner, socket and recorder paths. Five reload
tests cover confirmed Flush/Elide, next-turn measured append and Generated,
queued Stop through actual TurnClosed, mixed filed/pending custody and
independent Leave refusal with a real turn and Decision. Four native tests
cover Stop during and after recording, next-turn measured generation,
Leave/JoinLeave and recorder pressure. The existing pending-source rejection
fixture now checks the real rejected disposition and refused conversion.
The existing Releasing test still forbids native cache adoption.

Serialized author gates used the pinned toolchain, --locked and private target
/tmp/weaver-146-place-stop-returns-target, with lock.sh first:

- Reload-return tests: 5 passed. Native-return tests: 4 passed.
- Full harness: 1,703 passed, 18 inherited ignored.
- Trace: 160 passed. Affected harness all-target Clippy, fmt, lock and diff pass.
- Fourteen behavior controls compiled and failed their intended assertions.
  Every exact restoration passed its named test. All ten source hashes match
  the pre-control tested snapshot. Existing capability type pins are separate.

The first native run passed two tests and failed two because the fixtures
expected generation counts where the journal records messages. Input plus
assistant is two messages, even for an empty assistant. The tests now retain
the actual original count and assert two further messages on the next
generation, or no change after Leave. Production code did not change for this
correction. Initial and final logs remain evidence.

Controls cover Staged transfer, irreversible cut-input closure, original input
return, resolved-unit reuse, consumed lease custody, rejected-source refusal,
native extraction and truthful settlement, retained proposal and Decision,
complete Recorded settlement, Stop through TurnClosed and both Leave guards.
The completion control removes the complete equivalent readiness predicate.
Its individual redundant terms are not claimed as independent protections.

Scripted decoder counts establish ownership and ordering, not renderer or
model qualification. Independent source and evidence review accompanies the
author gates and does not substitute for the Planner's grade.

## Remaining review and completion boundary

#269 rows 1-2 are addressed for review, not retroactively cleared at their old
head. Row 3's caller-dropped extracted grant window remains filed. Rows 4-8
(assembly identity, private cancellation surface, exact author tickets and
settlement/drain cleanup) remain separate work. The placement record already
names #269 and its published head; the carried filing-date convention remains
tracked. Q6, Q12/Q13 and Q24 still owe targeted evidence. Q2/Q7 are redundant,
Q8/Q9 overlap and Q14 was redundant or untested in the original grade.

#267's remaining batch and #270/#271's followups remain tracked in #146.
Approved but unmerged #260/#263/#268 supply no implementation authority yet.
Full lifecycle, serving selection, plan ports, failed-run composition and
qualified coupled cutover remain required. This slice changes no active
serving path, deployment or system prompt and closes no issue.
