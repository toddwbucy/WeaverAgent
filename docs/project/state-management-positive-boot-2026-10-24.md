# Positive generation limit in composed boot

Date: 2026-10-24. Branch: `code/state-boot-positive`.
Parent: draft #317 at `b413e6b28bc8f7361804ad02aa429d4dbbcd3d6c`.
Stack: main, #312, #314, #315, #317, then this act.
Authority: merged #260, Spec 06 CAPACITY and the harness amendment HARNESS-OPEN.
This is implementation evidence for #146, not production qualification.

## Behavior and authority

Foundation boot requires the actual Opened.generation_limit to be positive in
addition to the existing checked head/tail/capacity fit. Zero remains valid wire
evidence. The harness preserves that zero and the exact raw Open boxes, records
assembly-zero placement, then records the existing StatePersistenceInvalid
boot/boot_failed refusal. A zero result creates no CheckedBootOpen, journal or
boot mark. It does not become a protocol failure or replace an earlier cause.

The check is part of consume_boot_open's configured fit, after correlation and
ValidatedOpen evidence validation. The existing after_boot_placement transition
handles refusal only after Recorded placement. Backpressure retains the same
author ticket, decode evidence and original Enter bound. A fitting positive
limit of one remains sufficient for this condition; ordinary capacity checks
still apply.

The existing BootComposition receives Open(Refused), retains the stopped owner
and cannot progress into prior-pair repair, summary, installation or completed
boot. The outer lifecycle still owns bounded failed-enter cleanup. This act
does not supply a missing full worker/lifecycle exit. The previous serving path
stays active until the separately qualified coupled cutover.

| Spec scope | Author assessment |
| --- | --- |
| Spec 06 CAPACITY, foundation positive effective limit | Conforms for runtime admission: zero fails configured fit; one and the existing larger limits can pass the unchanged checked budget. |
| HARNESS-OPEN, truthful placement before boot_failed | Conforms for the changed case: exact zero evidence, one Recorded placement before its attributed refusal, original Enter bound and no readiness grant. |
| OPEN-FACTS and PLACEMENT-RECORD | Existing carrier and raw-box rules retained; zero is decoded and recorded without changing its value or inventing a new wire outcome. |
| Full lifecycle and production qualification | Remain incomplete under #146. Scripted SPU/member/checker peers prove composition mechanics, not actual model, language artifacts or measured production limits. |

## Caller and evidence boundaries

| Road | Effect |
| --- | --- |
| consume_boot_open | Validates actual correlated Opened, then includes positive generation in the configured fit. |
| after_boot_placement | Existing fit-failure branch records boot_failed after the truthful placement; no journal/mark capability is constructed. |
| take_boot_open | Existing revoked-owner refusal prevents a checked proof from escaping after zero fit failure. |
| BootComposition Open and finish | Retains Open(Refused) and the original stopped chain; no successful composed boot can be taken. |
| SPU Open and diagnostic generation | No producer or carrier change. Their resolved-limit and diagnostic behavior are outside this changed harness admission check. |

## Instruments and validation

Three new tests exercise the real recorder and socket owners. The zero-limit
case runs both without pressure and with a full recorder. While pending, it
checks the same ticket/event, unchanged Enter and absence of placement/refusal.
After drain, it requires exactly one zero placement and one causally linked
boot_failed refusal, retained raw response, no fatal context latch and no
journal/mark grant. The existing stopped release can settle that boot account.

The one-token control takes the actual checked boot proof with the exact limit
and one boot mark. The composed circuit runs certified templates, prompt read
and release, and decoder Open before stopping with the typed fit refusal. It
checks that pair/summary reads and installation have not started and that finish
returns the stopped owner.

The first focused build found two fixture imports using the wrong enum module.
Its exact source and compiler log are retained. The corrected imports use the
existing state_flow types; no runtime change followed from that compile error.

Final restored-source checks passed with the pinned toolchain,
CARGO_INCREMENTAL=0, CARGO_BUILD_JOBS=4, --locked and private target
`/var/tmp/weaver-state-146-boot-positive-target`.

| Gate or instrument | Result |
| --- | --- |
| lock.sh before/after Cargo, formatting, strict harness clippy and diff check | Passed |
| Full harness suite with four test threads | 1,999 passed, including 22 doctests; 183 ignored in the ordinary run |
| Three focused boot_positive_ cases | Passed, including zero with and without recorder pressure |
| Delegated gate/startup/input/Observe/Stop/generation/emission | 46/17/19/14/16/20/33 passed; 165 total, serially in isolated scopes |
| Protective mutations | All three compiled and failed the intended tests: accept zero, reject one, or classify zero as protocol failure |
| Final source manifest | All 660 tracked crate/configuration files restored and verified |

Source manifest SHA-256: `cda2ee490d4461323bffbf6bb6ff0bfd1cf6e706900b561307f0366b22133905`.
Observe and Stop used the actual admin test executable rebuilt in this private
target. Eighteen inherited harness ignores and eight privileged admin-startup
instruments remain outside this gate. Serial passes do not close #313. No Cargo
or Git mutations overlapped, deployed agent changed or CodeRabbit review was
requested. These are author checks, not an independent Planner grade, security
disposition or production qualification.

## Remaining work and after-action

This implements #260's positive-generation admission rule within the existing
boot composition. It closes no full implementation or qualification checkbox.
The remaining #314 caller/lifecycle findings, #313 clock-race instruments, #316
cancelled-owner completion, worker correction/pair composition, full failed-run
teardown, production artifacts, measured limits, security disposition and
qualified coupled cutover remain open under #146.

The code-review and verification-before-completion skills remain unavailable;
no invocation or independent grade is claimed. All new PRs remain drafts for
review. Only the operator merges, deploys or requests CodeRabbit reviews.

The integration boundary matters here: a truthful resolved zero must survive
evidence validation and recording before configured boot admission fails.
Putting the rule in wire parsing would erase the specified distinction between
valid measurement and a usable serving configuration.
