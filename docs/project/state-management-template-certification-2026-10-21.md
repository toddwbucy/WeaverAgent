# Harness template certification

Date: 2026-10-21. Status: implementation under local validation.

This slice follows [PR #205][base]. It implements load-time certification under
[TEMPLATES][protocol], [TEMPLATE-CHECK][harness] and [TYPES-TEMPLATE][types]. The
harness binds the compiled inventory to the recorded load and derives its private
certificate from the SPU's recorded replies. The existing serving path remains
active under FOUNDATION-TRANSITION.

## Implemented boundary

The inventory retains its exact ordered compact JSON bytes and SHA256 digest. It
rejects duplicate identities, invalid template shapes and conflicting slot classes.
The load record holds the same templates, scheme and checker identity. A mismatch
refuses certification before a request reaches the classifier.

Each template travels through the existing STE100 socket, one request at a time,
in inventory order. The recorder supplies the result's exact source position. A
decoded reply or a later author sequence cannot substitute for that proof. The
transcript retains every consumed reply and its source. Only a complete accepted
transcript can issue the private certificate, once, for that run and inventory.

Correct non-STE100 results refuse certification without a component halt. Broken
transport, invalid replies and recording failure retain the existing component
failure behavior. Certification uses the original enter allowance. Progress does
not renew it. The lifecycle caller still owns the after-load refusal and rollback.

The check owns its completion until it enters the transcript. Input, capture and
content authorship wait outside that boundary. A blocked author returns through an
explicit owned slot. A second attempt before taking it is caller protocol failure,
with both originals retained. No blocked event is silently replaced or authored.

A terminal rejection can retire Enter for rollback, while its content boundary stays
closed. An in-flight timeout retains the pending request and original bound for the
lifecycle's forced rollback owner. Clean release cannot discard that exchange.

## Validation

The full harness suite passes 773 tests with zero failures and 17 inherited ignored
instruments. The 23 added tests include 12 context tests using actual Unix sockets
and recorder capacity. They cover exact load identity and result sources, sequential
requests, private one-use certification, pressure, refusal, original Enter expiry,
rollback, broken replies and recoverable blocked-author ownership. Core tests cover
inventory shape, order, exact bytes and transcript validity. A coordinator regression
checks source custody across a later pending author.

Thirteen protective mutations compile and fail their intended assertions. The
probes cover exact load/result binding, complete one-use certification, completion
custody, Enter retirement, rejection rollback and input/author/capture exclusion.
Every changed source is restored byte for byte. The restored template-filtered run
passes 25 tests with zero failures or ignored tests. Independent reviews cover the
core, the driver and its proof path, and the saved evidence.

The private target is /tmp/weaver-state-capture-target. The full-suite evidence is
/tmp/state-template-final-tests.log. Probe evidence and saved source baselines are
in /tmp/state-template-mutations-final/. The restored targeted log is
/tmp/state-template-restored-tests.log. All-target harness clippy at -D warnings,
formatting, the lock gate, editorial/link checks and git diff --check pass. The lint
log is /tmp/state-template-clippy.log. sha2 was already resolved in Cargo.lock and
is now a direct harness dependency for the compiled inventory digest.

## After-action

Review found that Enter retirement must distinguish a pending check from a terminal
rejection. The former keeps its original bound. The latter can retire that wait for
rollback without reopening content or granting a certificate. Review also caught
refused-author custody under repeated attempts. Its dedicated slot returns the first
original, and a second attempt before that return preserves both as protocol failure.
The corresponding tests exercise the real context boundary. The capture guard probe
uses a valid declared window and idle member, so removing the guard admits the capture.

## Integration still owed

The production grammar compiler, inclusion proof, qualified checker and complete
compiled template inventory remain C1b/C1c dependencies. The fixture replies prove
custody and protocol behavior. They do not qualify production language acceptance.
Runtime expansion also waits on C1b's step-reference lexical provider. The stored
condition key is not a license to invent that token's source spelling. The private
certificate does not itself authorize a plan step, model pass or arbitrary text.

The paired boot integration must invoke this port after the recorded load and SPU
admission, before the boot read or user-channel opening. It must consume a valid
certificate and retain the transcript. No caller migration, new wire vocabulary,
production template wording or Spec amendment is part of this slice.

H1b2 still owns forced rollback and organ teardown after an in-flight Enter timeout.
The existing clean-release port cannot discard a submitted classifier request. The
template port returns the typed timeout and retains that request for the lifecycle
owner. This slice neither reclassifies a lifecycle timeout nor grants a retry.

[base]: https://github.com/toddwbucy/WeaverAgent/pull/205
[protocol]:
  ../crates/state-management/state-management-spec-02-protocol-classifier-gate.md
[harness]: ../crates/state-management/weaver-harness-Spec-amendment.md
[types]: ../crates/state-management/weaver-types-Spec-amendment.md
