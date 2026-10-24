# State management: aggregate Enter Ready composition

Date: 2026-10-23. Author check for Issue #146.
Branch: `code/state-enter-ready-composition`.
Parent: #300, `76b5828d5380db7253af8936efdd79747e8ec06e`.

## Completed connection

The consumed, completed gate startup now feeds an aggregate Enter answer owner.
Construction requires actual residency and the retained original Enter attached to
this recording context, alongside the gate's confirmation, containment and child.
A primitive-only boot/gate result without the original source is returned intact.

The answer is Ready at Close on the original Enter exchange. Its tracked send is
bound to that same peer; busy or failed staging retains the unaccepted response.
No caller supplies a replacement exchange or connection. A blocked send retains
all resource and evidence ownership and the original Enter deadline.

For this explicitly staged answer, the common pump observes other standing
components before the final admin write. A bounded receive on the same original
peer then checks cancellation. Held Stop/Leave/JoinLeave prevents the write, and
deadline checks surround that probe. Ordinary pump sends retain their existing
ordering. There is no second reader, timer, queue or thread.

Whole-send evidence is retained before the post-send clock observation. A missing
or out-of-window timestamp grants no retry or successful handoff. A failed clock
observation latches the existing failure path even though bytes may already have
left. Only the valid whole send retires the original Enter wait.

Successful extraction transfers the same context, decode/residency ports,
classifier/SPU/gate child custody, gate containment/listener, template certificate
and original Enter/Raise/Ready evidence into `EnteredServing`. The admin peer may
close normally after its answer. Repeated advancement cannot send again or check
the retired Enter deadline. This is a dormant lifecycle handoff for the eventual
worker/dev_loop composition, not selection of the replacement serving dispatcher.

## Evidence

Private Cargo target: `/tmp/weaver-146-boot-composition-target`. Lock first, serial
Cargo with `--locked`. Delegated scenarios use isolated cgroups and actual forked
protocol-double children; no deployed agent is changed.

- Harness: 1933 passed, 63 ignored. All 45 delegated composition scenarios in the
  ignored set pass separately; 18 inherited ignored instruments remain.
- Thirteen new aggregate scenarios cover success, saturated send, Stop before and
  during pressure, original EOF, expiry, dead gate/SPU, observed gate terminal with
  its child still Running, post-send clock failure/expiry, busy staging and missing
  original source. Successful handoff tolerates normal admin close and later time.
- Trace: 160 passed. Harness clippy, formatting, lock and diff checks pass.
- Ten compiled protective mutations fail: original exchange, Close position,
  component-before-answer ordering, cancellation, original receive probe, whole-send
  gate, Enter retirement, post-send clock latch, completed handoff and busy evidence.
- E0507 prevents consuming a borrowed Ready owner; E0277 prevents cloning the
  entered resource bundle. All 254 harness Rust source pins are restored.

The post-send clock instruments observe the actual emitted packet with a
non-consuming peek before failing or advancing time. They do not count internal
clock calls. The trace/member/model behavior remains scripted; these instruments
do not qualify a real model, SQLite image or language/classifier artifact.

Direct author review covered constructor custody, every send outcome, common-pump
ordering and successful/stopped transfers. Requested review/verification skills
are unavailable here; no independent Planner or security grade is claimed. Exact
published source and logs accompany the authorized Issue #146 evidence deposit.

## Authority and remaining composition

| Implements | Author verdict in this act |
| --- | --- |
| Harness Spec section 3; admin-harness contract sections 3 and 5 | Conforms for the composed Serving aggregate answer and original-exchange resource handoff |
| Harness amendment section 1 HARNESS-OWNERS / HARNESS-RECORD-LIFETIME | Conforms for one context, common pump and retained resources through Ready; complete runtime migration remains owed |
| Foundation FOUNDATION-TRANSITION | Conforms for keeping the current serving dispatcher active until qualified coupled cutover |

Actual admin selection/locks/member spawn, worker/dev_loop orchestration, serving
selection and complete failed-account/rollback/gate cleanup remain owed. Active
show/LoadFacts, language/classifier/prompt artifacts, measured limits and full
qualification remain. Existing caller duties for scope, binary paths/arguments,
inventory and true remaining load budget are unchanged. No unmerged proposal,
deployment or Issue #146 completion is claimed.
