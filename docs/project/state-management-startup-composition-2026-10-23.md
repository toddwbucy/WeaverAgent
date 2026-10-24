# State management startup composition

Date: 2026-10-23. Version: 0.1. Author implementation record, not a Spec.

## Scope and pass statement

This composition-first act starts from #287 c1e12866b8fc494d9cec65bdb35b8c9a13dfaaab.
It connects the actual structural state Open, untapped load, partition activation,
Recorded Open observation, classifier fork and classifier admission. The same
recording context, resource bag and original Enter wait remain in custody.
The load origin and member observation identity come from this context and its
checked Open, rather than a prediction supplied before the peer answered.
Other load facts remain the caller's judged inputs and are checked by the existing
load author. The caller still supplies the state channel, trace sink, declaration,
seed/start identity, scheme/bootstrap, compiled inventory and executable path.
These inputs do not themselves establish admin provenance or artifact qualification.

PreparedStartup returns the actual context, resource bag, original inventory and
consumed Open/Ready observations. The caller can connect these to #287's boot
sequence after actual decoder residency admission. StoppedStartupAccount retains
all unconsumed inputs and consumed observations beside the same context/resources.
It grants neither retry nor a healthy lifecycle answer; the aggregate still owes
its recorded refusal, accepted rollback, process cleanup and response.

The joined integration instrument drives startup and then #287 through placement.
It uses an actual fork/exec classifier protocol double and actual socket/recorder
owners; the decoder and state replies are scripted. The classifier double is a
Rust test child reached through a temporary shell launcher, not a deployed runtime.
Its fake clock is paired with scripted readiness. Child termination/reaping at the
fixture boundary is test hygiene, not lifecycle cleanup qualification.
A pass establishes these ordered connections and custody only. It does not qualify
SQL, production language checking, rendering/tokenization, actual SPU residency,
admin entry provenance, lifecycle Ready, user gate start or the serving loop.
No replacement serving election changes in this act.

## Authority and limits

Implements: HARNESS-BOOT, HARNESS-RECORD-LIFETIME, HARNESS-CLOCK and TEMPLATE-CHECK
startup ordering in the merged harness state-management amendment; the base harness
Spec's section 2.2 fork custody and after-load refusal boundary. #287 retains its
own boot/Open/repair/summary/place conformance scope. No new wire, event, deadline,
policy or Cargo dependency is added. Approved but unmerged Spec proposals remain
unimplemented.

The production entry remains private/dormant. The final same-run Ready requires
actual decoder residency, boot, gate and remaining Enter obligations. All healthy
and failed stopped results still need their complete outer lifecycle consumer.
Existing active serving and worker/dev_loop ownership remain unchanged.

## Validation

Final author gates pass: 1,826 harness tests with 18 inherited ignored cases,
160 trace tests, all-target harness Clippy, formatting, lock and diff checks.
Ten nested child result summaries are excluded from the harness top-level count.
The focused eight-test run comprises seven parent scenarios and the child entry
(which returns without work outside an explicit child invocation). Scenarios cover
successful startup; startup through #287 placement; delayed actual Open; refused
Open with retained unwritten load and no fork; partition refusal without fork;
classifier refusal with actual exit1/reap and no component halt; and expiry at
final extraction with retained observations. Actual child custody is protected
by a fixture Drop guard even when an assertion panics.

The first exit-status assertion failed compilation because its new fixture lacked
a ChildState import. The corrected import changes no production guard. That log
and earlier runs remain retained. All gates above followed the final correction.
Six compiled protective controls fail their named tests and their exact restored
runs pass: premature load, lost actual origin, changed classifier refusal cause,
omitted final original-Enter check, lost stopped Open observation, and lost
unwritten load on preflight refusal. All 228 harness source hashes match the
pre-control snapshot. The private Cargo target remains associated with this same
isolated checkout: /tmp/weaver-146-boot-composition-target.

These are author checks. Prescribed code-review/verification skills were unavailable;
direct source/sequence self-review and executable gates were performed instead.
No independent or Planner grade, security review, production qualification or
Issue #146 completion is claimed.

The composition-first sequence makes previously separate startup fixtures run
through one production caller. Preserve that test boundary when connecting actual
SPU residency and lifecycle outcomes; isolated constructor passes are not enough.
