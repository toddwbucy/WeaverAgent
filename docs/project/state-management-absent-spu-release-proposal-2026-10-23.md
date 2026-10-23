# Absent SPU in terminal release evidence

Date: 2026-10-23. Status: PROPOSED; operator ruling, Planner review and merge are
pending. No implementation is authorized by this note. Exact proposal base is
main 45f8483393ce25af4ade3f0fa7bd96c1e8ed78f1. This answers PR #280 Planner batch
row 6 and the no-SPU branch of Issue #146's failed/healthy Enter composition.

## Decision proposed

Keep `release` mandatory on every newly authored Unload. Change only its `spu`
member to optional: omit it exactly when the harness created no SPU child for the
run. A present value keeps the existing Confirmed/Unconfirmed meaning. Do not
write null; explicit null is invalid on read. The separate `member` field keeps
its actual Closed/Unconfirmed/absent rule. No new event, enum spelling, timeout or
successful lifecycle result is proposed.

The ownership boundary is successful process creation: once fork returns a child
PID in the parent, that SPU stood even if later exec, Load or Open failed. A reaped
child still stood. Only actual noncreation permits absence. Missing transport,
missing reply, a cleared optional resource slot, or unknown/lost custody is not
proof of noncreation. Keep that fact through descriptor retirement and reap.

## Why this needs a ruling

Current trace Spec section 3 defines `Release { spu: ReleaseOutcome, member:
Option<MemberRelease> }`, says every current Unload writes `release`, and permits
only Confirmed/Unconfirmed for the SPU. It defines absent member, not absent SPU.
The implementation mirrors the mandatory `spu` field. Thus simply omitting the
whole `release` object contradicts its current required-write meaning, and writing
Unconfirmed for an organ never created invents a release fact.

The foundation's real STE100 NoCompiledAdapter can refuse admission before any
SPU decoder fork. That is a healthy semantic refusal, leaves the bracket Entered,
and waits for actual rollback Leave. A genuine clock, state, trace or process
failure can also halt before an SPU child exists. Both may need a terminal Unload;
neither is permission to fork an unnecessary SPU or claim a confirmed release.

| Actual state | Proposed release.spu | Other terminal facts |
| --- | --- | --- |
| Valid classifier refusal, no SPU created, later explicit rollback | Omitted | Healthy forced/cause and actual member/grants facts; no synthetic component-failure account |
| Real failure before SPU creation or fork returns no child | Omitted | Original first failure, component-failure account, actual member disposition; no successful Left |
| SPU child created, then exec/Load/Open fails | Unconfirmed unless exact sent Release later gets Released | Original failure/refusal and process evidence remain |
| SPU child created, exact sent Release gets Released | Confirmed | Independent child reap and member facts remain truthful |
| SPU child created, silent/died/reaped without confirmation | Unconfirmed | Never replace the known child with absence |

If neither SPU nor member stood, `release` is an empty object. If only a member
stood, it contains only that member's outcome. This does not invent a new meaning
for an absent whole `release` on historical records; those still mean the older
record did not carry release evidence. All existing present SPU values remain
valid and unchanged. Readers must accept the new omission before the new writer
is used; old records are neither rewritten nor reinterpreted.

## Alternatives

1. Recommended: optional omitted `spu` with actual noncreation semantics, preserving
   the required outer release object and independent member evidence.
2. Add an explicit `not_started` ReleaseOutcome. This keeps the field mandatory but
   expands an enum currently describing a created SPU's exchange; every exhaustive
   reader must learn the new spelling. It is a valid alternative requiring the
   same coordinated reader/writer review.
3. Omit the entire release object before SPU fork. This changes its historical
   absence meaning and loses a standing member's release outcome; it is not
   recommended.
4. Leave the schema unchanged and restrict the new cleanup's complete terminal
   domain to an actually created SPU. This does not finish pre-SPU Issue #146
   cleanup; those cases remain explicitly unhandled custody. It is not a complete
   solution for the valid early classifier refusal.

## Bounded terminal consequences and separate work

This proposal changes representation only. It does not elect an early kill
sub-deadline, permit waiting beyond original halt/Leave caps, change child reap
semantics, or allow successful Left without recorded/drained Unload. PR #280 row 1
still needs an explicit implementation disposition so a child that never exits
cannot consume all time before the terminal attempt. Merged HARNESS-HALT already
orders release/close, then Unload, then bounded drain; a bounded reap observation
and retained unreaped custody can be considered in that implementation without
inventing an extra clock. Do not treat this representation ruling as approval of
any unreviewed ordering algorithm.

PR #280 row 2's stale residency reply must retain the actual old reply and continue
the exact tracked Release; it cannot confirm Release before send. Row 7's finished
owner still needs truthful terminal evidence accessors. Required stderr
pipe/tee/LastWord and bounded child custody remain; no blocking post-cap join or
new LastWord timeout is authorized.

#276 author-return/boundary policy and #282 EnterWindow remain separate pending
elections. This proposal implements neither and changes no existing serving path.
The stopped Grants/refusal account slice and accepted rollback custody handoff are
independent implementation dependencies, not completed by this document.

## Implementation and review requirements after approval and merge

Change the owning trace payload/strict read representation together with every
writer and consumer that exhaustively assumes `release.spu`. Keep Release's
mandatory-write/optional-historical-read rule. Preserve explicit null rejection
for the newly optional inner field, the two existing present spellings, and the
member field's current rules. Audit diagnostic conversion, serving record
validation, analysis/consumer contracts and external readers before coupled use;
this repository does not claim a sibling reader migration happened here.

Use actual process-owner fixtures for before-fork failure, expected classifier
refusal then rollback, failed exec after real fork, reaped actual child, exact
Released and unconfirmed release. Read terminal trace JSON back and require the
exact omission/presence and member facts. Pair them with first-cause, original-cap,
no-fabricated-Release and no-Left-on-abandon controls. Cover historical present
records, present outer empty/member-only objects, and invalid explicit null.
Compiled protective mutations must distinguish real process absence from a
retired port; source-only assertions do not establish this behavior.

The owning Specs use prospective landing form. Their MERGED headers do not
record operator approval or make an unmerged branch implementation authority.
All three owning Specs name #285 as the prospective Landing PR. The harness
amendment needs version sequencing with #276 and #282: the first actual merged
revision advances 0.29 to 0.30, each later join retains all merged clauses and
increments again. Original Date filed headers are preserved; no merge order or
Date filed convention is elected here.

Implements (proposal, not implementation authority): weaver-trace Spec section 3;
state-management trace amendment TRACE-HALT; harness amendment HARNESS-HALT and
proposed HARNESS-RELEASE-ABSENT. These are proposed changes, not a conforms claim
against their pre-proposal mandatory-SPU wording. Issue #146 remains open.
