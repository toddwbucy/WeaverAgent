# Foundation boot generation limit proposal

Date: 2026-10-22. Issue: #146. Approved election; Spec merge remains pending.
Source: #258 Planner review 5622585660, row 5.
Draft PR: https://github.com/toddwbucy/WeaverAgent/pull/260
Branch: docs/state-positive-generation; base main 45f8483393ce25af4ade3f0fa7bd96c1e8ed78f1.

The operator approved positive generation for boot in comment 6354978619.
Planner review 5622765455 found no fix-bucket rows and recommended this election.
Its two prose-wrap findings are corrected without changing the Spec words.
Implementation still waits for merge by the operator.

## Proposed decision

Require the actual Opened.generation_limit to be positive for successful
foundation serving boot. Treat zero as a failed configured fit, using the
existing StatePersistenceInvalid {stage:boot,case:boot_failed} after the truthful
boot placement. Release no journal, boot mark or serving readiness and run the
existing failed-enter teardown. Keep the original Enter bound.

The SPU continues to report its actual resolved limit, including zero. The
Opened wire remains u64 and zero remains valid evidence. Do not relabel it a
protocol error, normalize it to one, reject unrelated diagnostic operation or
invent a refusal code. Admin cannot infer every frozen/tunable effective limit;
the harness judges the actual Opened fact at the existing fit boundary.

The alternative is to retain successful zero-limit foundation boot explicitly.
That preserves current behavior and permits an agent that can never draw a
generated token under that loaded limit. Either election needs an explicit
record; this draft proposes the first.

## Existing behavior and rationale

Merged Spec 06 OPEN-FACTS carries the resolved max-tokens-per-turn; CAPACITY
requires positive head and tail budgets but does not require a positive
generation limit. The SPU count resolver accepts zero. Its escaping/evidence
unit fixture also constructs zero counts; that fixture tests carrier spelling,
not a qualified boot.

The first append is not necessarily refused. When its delta and terminator fit,
the existing Session::append_and_generate prefills the delta and checks the
zero limit before sampling, producing LimitReached with no sampled token under
the ordinary termination rule. This proposal therefore prevents unusable
foundation serving admission rather than repairing an overflow check. It
preserves the general SPU zero-limit behavior for other admitted uses.

## Paired scope and required evidence

The draft changes Spec 06 CAPACITY and its protective instrument,
HARNESS-OPEN's fit judgment, and SPU-OPEN's producer/consumer responsibility.
The field set, terminal convention, existing refusal vocabulary and counted
placement remain unchanged. No code depends on the proposal until operator
approval, Planner review and merge.

Implementation must distinguish a valid zero-limit Opened from malformed
evidence, retain its exact raw boxes, record one truthful placement before
boot_failed and grant no journal/mark. Positive boundary controls must still
succeed. Exercise trace pressure, cancellation and original-bound expiry
without losing the failed fit's owed account. A mutation that admits zero must
fail that actual owner test. Keep carrier zero roundtrips and diagnostic session
zero-limit behavior valid. The operator has approved this proposal; it does not
complete #146 or qualify serving.
