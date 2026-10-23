# State management boot Open owner

Date: 2026-10-22. Issue: #146. Branch: code/state-boot-open-owner.
Base: draft #257 at ee7488e65efd6a73c3664682274e206cbfc77e94, retaining
#256 and #255 above merged main 45f8483.

This implementation adds a dormant HARNESS-OPEN owner. It consumes the real
Released/Recorded BootHead and the recorded load budget, retains the exact Open,
source port, reservation, response and original Enter bound, and records boot
placement before releasing checked journal facts. It does not activate the
replacement serving path or complete issue #146.

## Authority and scope

Implements: state-management/weaver-harness-Spec-amendment HARNESS-OPEN,
HARNESS-OPEN-LIVE, HARNESS-RECORD-LIFETIME and the bounded HARNESS-HALT Open account;
state-management-spec-06-marks-renderer-engine OPEN-FACTS, CAPACITY,
PLACEMENT-RECORD and boot MARK-POSITION; weaver-types-Spec-amendment's existing
StatePersistenceInvalid lifecycle refusal vocabulary.

The owner follows the merged rules. Its complete primitive is a conformance claim
for review, not a Planner grade. Worker lifecycle composition, placement and generation
owners, production artifact qualification and coupled serving cutover remain held.
The implementation uses #255's paired Open evidence carrier and retains the
PlanSummary and malformed-recall prerequisites when stacked on #257.

## Retained custody and continuation

The request contains the existing BootHead system Message and trace session. The
owner does not read or file the prompt again. Complete-send, source-port and reserved
exchange identity are checked before a terminal reply supplies evidence. The harness
validates the SPU's closed raw boxes and counts using the carrier validator; it keeps
the original boxes and response without retaining a usable token-vector cache.

A truthful assembly-zero placement names the real boot recall as its causal parent.
Its N and resident-before equal the prefix count; appended input is zero. The owner
retains its exact author ticket under recorder pressure and saves Recorded identity
and the next state before any recoverable lifecycle or clock check. Generic author,
filing and halt continuations cannot consume that ticket.

Checked head/tail/generation/terminal arithmetic decides configured fit. A valid but
nonfitting prefix still has its placement recorded, followed by the existing
StatePersistenceInvalid {stage:boot,case:boot_failed} refusal. Invalid evidence is a
protocol failure and grants no accepted placement. Typed Open refusal retains its
original refusal record. Fault before Open completes is an invalid emission under
the decode contract's at-rest, already-open restriction; its raw evidence is retained.

The placement builder requires a harness-private ValidatedOpen borrow of the actual
checked Opened; the raw evidence and placement counts come from that same immutable
answer. Valid JSON boxes containing literal CR/LF whitespace remain valid wire
evidence. Their exact unchanged spelling cannot be represented in the trace's single
line, so the existing trace-encoding failure path applies and releases no grant. The
compact SPU producer escapes string line breaks and does not emit such whitespace.
Normalizing boxes or adding a new whitespace protocol refusal would change the merged
rule; neither occurs here. This limitation is covered by owner tests and remains part
of production qualification.

LIVE is checked before first send and after Recorded. A held Stop or Leave is observed
before transport can send the first byte, including a stop observed before the owner
exists. Cancellation is sticky for this Enter. Unrelated Observe does not cancel.
After grant transfer, the boot-specific transport guard no longer blocks subsequent
serving work.

The checked continuation has no scalar constructor, Clone, Default or deserialize
path. It initializes a private journal once, with a separate prefix sentinel, empty
logical layout, next occurrence and assembly IDs one, and recorded boot mark zero at
the actual seeded span's Head. PriorBootOpen consumes this proof to license the dormant
boot pair repair owner; that repair still grants no user-gate readiness.

## Stopped and failed accounting

An unsent request is withdrawn only before any preamble or payload byte leaves.
Partial sends and answers retain the same reservation and transport. Releasing and
Failed can drain and record the original placement or refusal under the earliest
original Enter, inherited Leave and halt bounds. Retiring and rearming a global Leave
cannot renew the retained deadline. Expired Enter can still reach stopped disposal.
Recoverable lifecycle Wait revokes readiness without inventing component failure.

The stopped author permit is private to the already-held boot account, uses ordinary
writer capacity and creates no mark, journal, filing or generation permission. Failed
accounting does not run ahead of the mandatory first fault. If the channel, trace or
bound prevents settlement, the owner retains an explicit incomplete disposition and
poisons the unfinished reservation rather than releasing it for reuse.

Decode retirement and every Unload author route require the account to settle or be
marked incomplete. An actually Recorded Unload with the stopped tee is accepted only
as shutdown evidence, after the account is settled and the recorder is closed. The
ordinary tap remains stopped.

## Validation record

The source includes actual BootHead, certified template, state stream, SEQPACKET
transport and trace-writer instruments. Coverage includes successful fit and proof
transfer, all failed-fit arithmetic cases, typed and protocol refusals, wrong source
and session, complete-send requirements, recorder pressure and exact ticket custody,
pre-owner and same-pump Stop, harmless Observe, cancellation before send and during a
segmented answer, both stopped account roads, first-fault priority, retained deadlines,
Recorded-before-clock-failure, and complete Releasing retirement/Unload/drain closure.
Compile-time checks pin the continuation's nonfabricable type properties. The floor
suite independently enumerates the lifecycle stage/case matrix and checks nested,
malformed and legacy wire carriers.

Olympus author gates used the pinned toolchain, --locked and the private target
/tmp/weaver-146-boot-open-owner-target. Lock ran before Cargo. The final harness
suite passed 1,500 tests with 18 inherited ignored cases, using eight test threads.
Types passed 140 tests by default and 195 with config; state passed 218 with eight
inherited ignored cases; trace passed 160. The focused boot suite passed 26 and
the decode-reservation suite passed 19. Both changed crates passed all-target
clippy with -D warnings. Formatting and whitespace checks passed. The SPU code is
unchanged from the carrier stack; this slice claims no new SPU device run.

All 21 protective mutations compiled and failed, and exact hash-checked
restorations passed their focused reruns. Fifteen owner mutations cover retained
source identity, complete send, validation, total/head fit, pre-owner Stop,
same-pump first send, cancelled generic pumping, exclusive author custody,
earliest bound, recoverable Wait, absent-owner release, unload ordering and
continuation, and raw evidence preservation. Six floor mutations cover the
stage/case matrix, artifact omission, closed/map-only fields and strict string
stage/case codes.

Initial gates exposed a real refusal-code object alias, a trace/shared mark
mirror mismatch and an invalid fake readiness mask; all were corrected. The
first same-pump and cancelled-generic-pump mutations survived because their
fixtures did not reach the protected progress opportunity. The strengthened
tests pair each cancellation with a live control that demonstrably sends or
consumes the actual exchange. Both mutations then failed. The final full harness
suite includes these stronger tests. Initial and final logs remain evidence.
One final lint attempt exhausted /tmp; removing inactive, regenerable Cargo
targets restored space, and the unchanged source passed the resumed lint gate.
No source checkout or evidence was removed by that cleanup.

Evidence deposit:
/bulk-store/weaver-testing/state-management-146-olympus-2026-10-22/boot-open-owner-ee7488e6/.
It holds the gate logs, initial failures, mutation recipes/results, independent
review notes, source hashes and exact published source archive. The 19 original
successful mutations and the two stronger reruns have a combined final summary;
the initial survivals are retained separately. Local working evidence is under
/tmp/state-boot-open-gates, /tmp/state-boot-open-final-gates,
/tmp/state-boot-open-mutation-evidence and /tmp/state-boot-open-mutation-recheck.

Independent source review and re-review cleared the lifecycle, custody and new
evidence-builder changes. The process's code-review and verification skills are
unavailable in this session; no invocation or Planner grade is claimed. These
are author checks, not production qualification.

## After-action record

The planned owner remained a dormant primitive. Review found gaps between
ordinary pumping, actual Recorded and stopped cleanup; retaining the exact
author ticket, earliest bound and completed-owner scope fixed those paths.
Protective testing then showed why a no-send assertion alone was insufficient:
it must run at an actual send opportunity, with a positive control. Future owner
fixtures should preserve that control and distinguish subscribed terminal events
from observed readiness. The failure logs are retained so a later review can
reconstruct both corrections rather than infer coverage from the final count.

## Remaining issue #146 work

The actual worker must still compose boot and failed-enter teardown, boot pair repair,
plan/forward flags, serve-side selection, placement grants and matching generation
confirmation, ordinary lifecycle and author migration, and the coupled cutover. The
language scheme, dictionary/corpus, span-binding classifier, separately reviewed
system prompt and measured runtime limits remain qualification prerequisites. Full
encoded trace-line size and simultaneously retained evidence/line memory require
measurement; the recorder has no per-record byte ceiling to borrow from the decode
bound. No deployment or goal-completion claim follows from this slice.
