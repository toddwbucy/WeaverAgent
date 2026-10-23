# Stopped release review followups

This source change answers the ten batch findings and six surviving perturbations
in [the #265 Planner grade][grade], reviewed at a17eba63. Preparation used
#267 e4dbb002440e3d651f31c080d530d713982a962e. The final join is based on #269
d2896bcd336b1b1ec78f370a6ff657b6ae9bcbef; #265's original published parent is
#264 285ab7fb1dcda0b23f16f4dcbd4c671f0433720a. Epic #146 remains open.

Implements: weaver-harness-Spec-amendment HARNESS-RECORD-LIFETIME, HARNESS-OPEN,
HARNESS-OPEN-LIVE, HARNESS-HALT and HARNESS-HALT-BOUND;
weaver-trace-Spec-amendment TRACE-STOP and TRACE-CAPACITY. This is a consistency
followup under the existing scoped authority. Final joined gates and protective evidence are recorded in the combined
release/repair followup record. No serving
activation, new reserve, protocol election or #263 count change is included.

## Findings and dispositions

| Row | Disposition |
| --- | --- |
| 1 | Fixed: resume selects release authority by the retained release attempt, not by phase. A defensive fixture retains an ordinary pending ticket in Releasing and requires that exact ticket to record on the live tap. Public begin_releasing still requires settled obligations. |
| 2 | Fixed structurally: the release mark now belongs to AuthorTicket. Clearing or taking pending_author also clears or takes the mark, including retain_summary_rollback_record. There is no separate pending_release slot to orphan. |
| 3 | Fixed: BusyAuthored identifies OwedAuthor::BootOpen. The context retains the refused original, retires the unadmitted release attempt, and returns BootOpenOwned without latching TraceFailed. The ordinary Unload guard uses the same reason. Four shutdown-kind defensive cases require no preparation, no admission, no latch, and later progress after debt clears. BusyAuthored originals are retained before fallible intake; a real barrier failure keeps precedence over a nonfatal hold. |
| 4 | The caller table below records the stopped-tap admission change and its halt-account implications. |
| 5 | Retained as defense: barrier identity, exact kind, closed shutdown membership and Observation purpose stay checked at each resume. The context currently supplies its own barrier and only Observation, so these guards are not all independently reachable failures through the production API. No production coverage claim is inferred from private corruption fixtures. |
| 6 | Partly consolidated: ReleaseTicket is now a mark on AuthorTicket, not a second independently retained pending slot. The borrowed ReleaseScope remains context-owned. Moving its phase/failure/attempt proof into a barrier-only boolean would lose the current revalidation on every wake; a new context-to-barrier callback or permit protocol would be a broader change. This followup deliberately retains that dependency and its existing bounds. |
| 7 | Fixed: retain_recorded_author is the shared writer of last_author_sequence and completed_author for the ordinary and release author-completion paths. Release invokes it before fallible intake and owner hooks; ordinary authors retain their existing boundary after owner binding and intake. Each path calls it once. The release-specific hook only marks its attempt Recorded. Freshness, boot and read account paths have separate retained-evidence writes. Caller completion is still consumed once. |
| 8 | Fixed: one scope/mark match rejects missing or mismatched permission, including on a live tap. The remaining stopped-tap guard checks only whether the matched permission authorizes the stopped submission. |
| 9 | The original record now separates preliminary join inputs from exact published parent #264 and links the durable shared-store evidence and Planner grade. |
| 10 | Retained test placement: the three release integration tests compose the existing boot and summary fixtures, whose private adapters, fields and helper methods are inherited by the nested module. Extracting shared support would move a large unrelated fixture surface. The path include and test filters stay stable; they remain integration tests of release, not evidence that PlanSummary owns release. |

The new surface is the typed owed-account reason on BusyAuthored. The original
AuthoredEvent and RecordPurpose remain in that return, no ticket is allocated and
no raw timestamp is sampled. Other BusyAuthored cases retain their existing halt
handling. The capability type remains non-Clone and has no public constructor.

## Retraced callers

| Caller | Behavior at a stopped tap and ownership |
| --- | --- |
| RecordingContext::submit_context_author / resume_context_author | Healthy Releasing shutdown uses the exact release attempt and original mark. Ordinary stopped authoring refuses before Recorded; its ticket remains available to halt accounting. No admitted_before_failure value is fabricated. |
| FlowCoordinator::advance (pending_record resume, formerly coordinator.rs:2890) | Uses ordinary resume_with_clock. A stopped tap fails before admission and keeps the pending record/original; recorded() returns FlowFailure. It cannot borrow healthy release authority or consume a completion proof. |
| FlowCoordinator::author / recorded (formerly coordinator.rs:3132) | Uses submit_authored_with_clock. Pending originals stay in the barrier; BusyAuthored originals and purpose move to refused_author. Coordinator events are measurements/flow evidence, never the shutdown Unload covered by the typed boot hold. A stopped submission produces no required proof and cannot advance the coordinator. |
| RecordBarrier::submit / submit_with_clock | Raw path delegates to the same ordinary resume. It retains the prepared event on pre-admission failure, has no original AuthorTicket, and receives no release mark. Its source callers are barrier and integration instruments; serving authors use the authored path. |
| BootOpen author account | Authored StatePlacement uses its existing exact original and private trace-only resume after stop. It cannot receive the Unload-only ordinary boot hold. Its own unexpected BusyAuthored handling retains the event and halts. |
| Read owner author account | Authored Recall uses the ordinary path and retains any unexpected BusyAuthored original before halt; it cannot receive the closed shutdown boot hold. Summary rollback retains its exact private read account. |
| Healthy shutdown with a boot account still owed | The context precheck normally returns BootOpenOwned. The barrier's typed defense returns the original unchanged if that precheck is bypassed; wait_recorded translates the reason without converting the hold into a trace failure. |
| Failed and bounded trace-only accounting | Uses the existing private owners. resume_trace_only retains actual admission in last_trace_only_sequence and clears the complete original ticket, including its release mark. Terminal reserve use and deadlines are unchanged. |

## Surviving Planner perturbations

| Probe | Reviewed disposition |
| --- | --- |
| H2, scope ignores failure | Keep the guard. Public live() and the barrier failure latch precede it. This is defense in depth, not an independently established behavioral claim. |
| H3, completed attempt replayable | Keep the guard. finish_release_author clears the attempt and caller completion ownership prevents replay. The existing replay instrument asserts that combined boundary. |
| H6/H7, purpose and content schema unchecked | Keep both guards. The four closed shutdown kinds currently have ContentSchema::None and the context supplies Observation. Private wrong-purpose admission tests remain useful but do not claim each resume check independently bites. |
| H8, owed boot account ignored | The new private-boundary fixture bypasses the earlier context precheck deliberately and requires the typed refusal for all four shutdown kinds. It is a defensive test, not a new reachable public state. Its recipe removes this barrier hold; root must execute it before claiming a bite. |
| H12, lost scope tolerated | The redundant second pending_release check is removed. A release mark with no scope is now rejected by one match; the existing live-tap generic-resume fixture and the new wrong-phase live-tap fixture exercise it without relying on stop. |

## Validation boundary

Independent review also found that the existing wait_recorded intake hook could
drop a returned BusyAuthored original before its match arm retained it. The hook now
runs after custody transfer for that variant. A real invalid-purpose refusal and a
boot hold followed by a separate author clock failure both retain the exact returned
original; the first real failure remains the halt cause.

The source author added four focused tests and supplied mutation recipes to root.
No Cargo or formatting command ran in the source-author snapshot. Root owns serial
lock, formatting, affected lint, full harness/trace tests, compiled perturbations,
restored passes, final source hashes and the evidence deposit. The original #265
counts do not describe this followup. Independent review and final joined gates passed before publication; the
combined record gives their exact scope. The final placement join adds its own typed reason. Its actual pending
MeasureDelta owner refuses release before admission, preserves the original and
Leave bound, then settles before that same shutdown original records. The final
joined evidence is recorded in state-management-release-repair-review-followups-2026-10-23.md.

Original #265 source and evidence remain at
/bulk-store/weaver-testing/state-management-146-olympus-2026-10-22/stopped-release-admission-285ab7fb/.

[grade]: https://github.com/toddwbucy/WeaverAgent/pull/265#pullrequestreview-5624154624
