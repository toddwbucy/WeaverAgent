# Serving rollback composition

Date: 2026-10-25. Author working record for issue #146.

## Plan and authority

This act starts at draft #346, commit
`b2c37640c1851ecba371adeae195bc39d27f9c82`, on
`code/state-serving-rollback-composition`. It extends the actual successful
Ready rollback owner to retained serving input and engine work. The selected
dispatcher remains `dev_loop::drive`.

Authority is the merged admin Spec section 3 and harness Spec section 6
rollback rules, harness-gate contract sections 2-3, Spec 02 INPUT, Spec 05
pair cancellation and filing, Spec 06 task/placement/generation cancellation,
and HARNESS-ENTER-WINDOW, HARNESS-INPUT, HARNESS-MARKS, HARNESS-CLOCK,
HARNESS-RECORD-LIFETIME, HARNESS-HELD-BOUNDARY, HARNESS-HALT and
HARNESS-HALT-BOUND. The existing trace turn-close rule distinguishes the
accepted Leave cause from the load cause carried by terminal Unload.

| Boundary | Implemented composition |
| --- | --- |
| Actual admission | Consume the original management rollback Leave and serving loans. Keep run/port/Ready/Raise/Admit identities and the actual accepted directive. History does not prohibit rollback of a reused idle session. Unadmitted caller/template holds return unchanged before Leave is armed; an admitted pending author retains its own continuation. |
| Input | Retain the original received bytes, acceptance token and engine allocation. Revoke accepted input, drain submitted checks, and retain already-admitted source authors and their actual filing terminals. Queued unoffered Frames receive Leave/Unloading trace records without input IDs. |
| Interrupted author | A temporary permit for the actual serving listener yields a blocked input drive to its existing management owner. It grants no Leave or directive authority. The original event, ticket, source account and deadline survive. |
| Engine | Share cancellation with Stop while retaining the distinct Unload election. Settle original task/read/mark/place/generation/repair/pair owners; do not elect further generation or dispatch. |
| Gate | Send the actual Unload ToolInterrupt before Lower, without waiting for its result. Lower uses current outgoing ordinal plus one. A blocked interrupt permits only bounded send progress and secondary management transport before Lower. |
| Replies and records | Route the original ToolAnswer to its pair owner. A received result wins over no-result cleanup. Socket retirement alone cannot invent GateFault: the actual child must be reaped. Private gate claims can resume only their own noncontent author ticket. |
| World close | Preserve an already-started or Recorded close. Withdraw an exact wholly unsent world datagram before Lower; retain its ticket and envelope without claiming delivery. A new rollback turn close uses Unload and the actual Leave cause. |
| Release | After current debts settle, reuse measured Grants, listener sealing, classifier/SPU/state release, load-cause forced Unload, drain, actual reaps and the exact Left/refusal. No wind-down or save point is elected. |
| Failure and cap | Preserve first fatal cause and min(original Leave, halt). Retain stopped placement accounts and cancel exact worker-held grants into their original slots. Healthy cap disposal retains evidence without manufacturing a failure. Recorded Unload permits no new author. |

Lower and Leave keep their original absolute bounds; an earlier Decision or
check bound still wins. The ten-millisecond control wake is a poll wake capped
by the nearest existing deadline, not a replacement operation budget.

## Verification scope

The two initial actual idle/accepted-input cases failed on the parent because
its handoff admitted only pristine pre-turn input. The development logs retain
those failures; no complete baseline-source archive was made at that stage.
Final evidence identifies the source used by the completed checks.

The parent's nonpristine-refusal case is retired: accepted input now has a
positive cleanup case. Twenty-three new delegated cases cover reused idle,
accepted input, a pending
input check, elected source, actual pending TurnStarted, source/mark filing,
task read, placement and its earlier Decision bound, active generation,
queued Frames, existing clean close, wholly unsent world close, sent external
execution, crossed result, silent result and actual reap, queued Frame before
result, interrupt backpressure, Leave expiry and actual clock/trace failures.
The blocked interrupt case also requires a real secondary Observe to receive
its existing InTransition reply before gate reads resume, without renewing
the actual AdminRead bound.

The pending TurnStarted case blocks the real trace sink through management
handoff. It retains the original ticket and then requires the original
MessageUser, its selected File and matching Recorded filing terminal before
turn closure and Unload. The world-close case releases the peer through an
independent observer of the original send ticket's actual withdrawn state.
The no-result case holds the real gate child alive after GateStopped and
prohibits terminal settlement until that child is actually reaped.

Three ordinary pair-filing tests pin exact returned-event custody across
Lower/AdminRead interruptions and the original bounds. Existing real gate
pressure cases now probe the actual gate claim/ticket against competing
generic author/resume calls and preserve a later first fatal cause.

All process cases use actual Unix sockets, child processes, root management
connections, trace receipts and member requests with scripted organs/member.
The original Load uid is 73 and rollback Leave uid is 74. Tests distinguish
those causes and require no new decode after cancellation. These doubles do
not qualify model artifacts or production capacity.

All 20 final checks passed: first/last lock gates, formatting, strict
harness clippy for all targets, harness and admin test builds, the full
harness suite, twelve delegated families and diff check.

Harness: 2,065 ordinary tests/doctests (1995 library,
43 binary/integration and 27 doctests); 598 library cases ignored
in that invocation. Delegated: 534 cases passed, including all
23 new serving rollback cases and the two shared gate-author pressure
probes. These totals use the outer test result, not nested child output.

| Delegated family | Passed |
| --- | ---: |
| session | 50 |
| startup | 207 |
| input | 19 |
| generation | 34 |
| emission | 34 |
| correction | 27 |
| inward | 24 |
| external | 64 |
| return | 13 |
| final | 31 |
| observe | 14 |
| stop | 17 |

Eleven compiled protective controls were detected after nine positive baselines:
current Lower ordinal, Unload interruption, actual child reap, failed placement
grant cancellation, completed placement accounting, held-gate yield, management
yield, exact world withdrawal, source filing, control-before-Lower ordering and
original deferred filing allocation. All seven controlled files were restored
byte-for-byte and rebuilt. The first held-gate-yield control survived a weaker
case; the retained second run detects it with real trace backpressure.

After those controls, lint required conditional/Option simplifications and a
documented large move-owned error exception in five files. The external family
then caught early result routing suppressing Stop's required ToolCancel. The
fix limits pre-control result routing to Unload and restores Stop's original
send-before-observe order. The session family also caught the removed
unadmitted-original guard; the handoff again checks the shared held-input
predicate before arming Leave, while admitted pending authors remain supported.
Final suites ran after these edits; earlier mutation sources and failed
regression runs remain preserved.

The runs use pinned nightly-2026-02-13, locked dependencies, default harness
features and private target `/tmp/weaver-state-146-worker-session-target`.
The Observe family initially lacked its explicit admin test-binary setting.
The separately built admin test executable was then supplied to Observe and
Stop; the remaining gates resumed only after source hashes matched. The
failed setup run is retained separately from the corrected Stop regression.

Cargo and Git mutations are serial. Ordinary tests use four threads; delegated
families use one test thread. No maximum-concurrency stress pass is claimed.

The evidence deposit is
`/bulk-store/weaver-testing/state-management-146-olympus-2026-10-22/state-serving-rollback-composition-local-20261025/`.
The source archive, hashes, checkpoint, logs, review notes and mutation patches
identify the final tested source. Development failures remain included.

## Development findings and after-action review

The plan was a consuming composition across serving positions, keeping the
original owners. Real process cases exposed gaps between independently tested
primitives: completed decoder accounts still held journal debt; a failed
worker-held placement grant needed explicit cancellation; direct source
authoring still owed filing after MessageUser recorded; and nested waits
could starve a queued gate packet or management connection. The fixes keep
those original accounts and make the relevant owner resume them.

The first mutation run detected five controls, then removing the held-gate
yield still passed the original queued-frame case. That run remains in the
evidence; it did not qualify the yield. The stronger trace-pressure instrument waits for an actual pending
Unloading refusal and held ToolAnswer before releasing the sink. Its compiled
control is reported separately with the completed mutation results.

Independent reads found an unclocked control-send classification, retirement
before actual child reap, retained-result routing, and a blocked interrupt's
Lower ordering. The last needed a second read: a later pump send could make
an earlier readiness observation stale and start a result/record wait before
Lower. The final false-readiness branch performs only bounded transport work
and returns to the outer owner. Secondary management transport continues.

Two test corrections matter to interpretation. Source election initially used
start_task, which had already advanced to an engine state; the direct source
case now uses start_turn. A blocked-world fixture originally waited for a
potentially blocking owner call before releasing its peer; it now observes
the actual ticket from a separate thread. Both failed runs remain evidence.

Continue testing the transition between owners before adding more outer
workflow. In particular, a completed internal flag does not settle an owned
account, and transport retirement does not establish process reap. Keep the
bounded fixture watchdog and the exact receipt/ticket checks in later acts.
When sharing cancellation mechanics, run the existing Stop family before the
long mutation pass; this act found the send-order regression only in the full
regression run, after the new rollback controls had passed.

## Completion boundary

This advances #146's serving rollback and failed-run teardown integration. It
does not close a broad epic checkbox. Rollback during correction placement or
generation and during a pending rejected-answer author has no new combined
Lower test in this act; existing Stop cases are narrower evidence.

Healthy Leave/save, capture cleanup, remaining admission/author composition,
dispatcher/process exit, production language/dictionary/corpus and classifier
artifacts, reviewed system prompt, measured limits, security disposition and
qualified coupled cutover remain owed. The #255 accepted-before-TurnStarted
Stop election remains open. Foundation remains unselected.

These checks are author evidence, not a Planner grade, security pass or
production qualification. The required `code-review` and
`verification-before-completion` skills are unavailable in this session;
manual checks and independent reads do not claim an invocation. All new PRs
remain drafts. No CodeRabbit review is requested; only the operator merges
or deploys.
