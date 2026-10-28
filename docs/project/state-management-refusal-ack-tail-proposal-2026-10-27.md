# Refused capture acknowledgement deadline proposal

Date filed: 2026-10-27. Issue #146, PR #370 Planner finding V1.

This is a proposed Spec election, not implementation authority. The existing text
lets the worker finish capture cancellation under the Leave tail but makes admin
stop at the earlier capture deadline. When that deadline triggers snapshot
cleanup, a later acknowledgement cannot satisfy admin's capture window. A forced
worker can finish while admin reports Unanswered and leaves the marker Open.

## Proposed election

After validated Beginning and before Commit, capture expiry stops successful save
work. Admin cancels its unconfirmed publication and sends its single Aborted where
permitted. It then waits only for refused-capture cleanup and the final lifecycle
response, bounded by the original shared Leave tail and outer admin deadline.
The earlier deadline wins. Nothing restarts either deadline.

A matching CaptureRefused after capture expiry may be timely for that refusal-only
continuation. Forced Leave can conclude normally without a save; a still-graceful
Leave reports SavePointNotTaken and remains at S9. No late image success,
preparation, Commit, publication, CaptureComplete or save report is authorized.
Crossed Commit retains the existing failed-run rule. No new field, event or timer
is proposed. Existing expiry rules before Beginning, on demand and after Commit
remain unchanged.

The alternative keeps capture expiry terminal for admin, including Unanswered and
the Open marker even when required cleanup and forced worker unwind later finish.
That choice would retain the current consequence and require both peers' owning
text and qualification cases to state it consistently. Reserving a new cleanup
margin would introduce a different timing policy and is not this proposal.

## Implementation and review plan

| Act | Required work |
| --- | --- |
| This proposal | Operator election and Planner review of foundation 5.2, harness amendment 3, admin amendment 3; merge before implementation. Types already delegates timing to PERSIST-LEAVE-REFUSAL and needs no wire change. |
| Following implementation | Distinguish admin's refusal-only wait from success completion; handle pre-Commit expiry before the receive window rejects the acknowledgement; preserve one Aborted and all publication custody. Keep worker cleanup and original tail. |
| Verification | Actor cases for exact capture/tail/outer boundaries, crossed frames and post-Commit failure; actual admin/worker/SQLite cases where capture and snapshot deadlines coincide; require forced normal conclusion without a save or graceful S9, and Unanswered after either remaining deadline. Protect against deadline renewal and late publication. |

Do not deploy foundation. Neither this proposal nor the current #370 implementation
qualifies the changed rule. S1's independent S9 scheduling investigation proceeds
without this election. Current runtime remains unchanged by this documentation act.
