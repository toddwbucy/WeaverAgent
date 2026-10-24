# STE100 protected artifact acquisition

Date: 2026-10-22. Scope: issue #146 item F; merged Spec proposal #235.
Branch: code/state-ste100-artifact-owner, stacked on #258 at
066d51a307d969bd7199d8173c8a53648e1d54ac, retaining #257, #256 and #255.
The assigned PR number will be recorded with the published gate evidence.

## Implements and limits

| Owning section | Author grade and scope |
| --- | --- |
| Spec 02 DECLARATION, MANIFEST-LOCATOR | Conforms for STE100-only absolute locator validation, exact parent/root acquisition, regular-file and confined-reference checks, SHA-256 identity and retained input ownership. |
| TYPES-CONFIG STE100 locator and manifest shapes | Conforms for absolute syntax at the exact config field, direct bootstrap validation and strict named manifest/encoder/file objects. Shared legacy binding grammar remains separate. |
| SPU-STE100 manifest acquisition | Conforms for an owned acquisition primitive and the sole production admission entry. No production adapter or executable is selected here. |
| STE100 contract section 1, checked inputs | Conforms for manifest-named owned inputs, every digest, no implicit sidecar discovery and retention through backend handoff. |
| HARNESS-CLASSIFY / ADMIN-STE100 active lifecycle | Existing transport vocabulary is retained; active executable launch, Serving enter composition, StateManagementInvalid mapping and failed-enter integration remain owed at the coupled cutover. This act does not claim those sections complete. |

The exact manifest file is named by an absolute path in Ste100Instruction.
The parser attributes a relative locator to
state-management.ste100.model-binding.artifact. Direct bootstrap checks the same
syntax before acquisition, so it cannot resolve a relative name against cwd.
Nine existing synthetic STE100 fixture locators now use explicit absolute names;
legacy decoder and label-classifier relative bindings retain their meaning.

The SPU opens the unnormalized absolute parent as a directory instance, allowing
normal ancestor-symlink and parent-component resolution. It then opens only the
manifest's exact final name with no-follow and nonblocking flags. A directory,
linked manifest or special file is not a manifest. Each declared reference is
resolved with openat2 beneath that pinned instance, without magic links. Relative
links wholly contained beneath the root remain valid; escape followed by re-entry
and absolute targets refuse. There is no alternate root, name convention or
legacy container discovery.

The owner reads regular files into immutable owned byte buffers, hashes those
buffers and retains them. The exact manifest bytes, including whitespace, are
both parsed and hashed. The compiled adapter receives only checked named inputs.
Repeated names share one acquired buffer, but every declared digest is checked.
Unreferenced files may exist without becoming inputs or causing a refusal.

VerifiedFiles now owns the checked buffers and can retain their exact Arc owners.
Admitted retains all of them, plus the original manifest, in RetainedBackend.
into_backend transfers that owner as a whole; no public raw backend or mutable
backend accessor can separate deferred reads from their inputs. Concrete adapter
operations still need an owned API when O02-2 lands. Test backends hold weak
references, proving the wrapper supplies actual lifetime rather than copied
metadata or a temporary hash result. The backend drops before its file owners.

Production admission now has one path, admit_from_path. Supplied-byte admission
is cfg(test), and the test adapter cannot create production Ready. The existing
sealed adapter admission must still compile the grammars, load the declared
model, validate ABI/alignment and capacity and instantiate the requested models
before Dispatcher can emit Ready. No such production adapter is supplied here.

## Failure and custody boundaries

Acquisition and digest failures return unsupported_artifact with the exact
failed locator or reference in detail. Relative bootstrap locators return
invalid_instruction. Existing grammar, scheme, capacity and adapter admission
codes retain their meanings. No acquisition failure grants Ready.

The later lifecycle composer must map scheme_mismatch to template/scheme_mismatch
and the other admission codes to template/template_failed, retaining exact
checker code/detail when the complete escaped lifecycle envelope fits its
existing bound. It must not parse prose for path, truncate the refusal or reset
the original load budget. StateManagementInvalid's production carrier and that
rollback integration are still absent; this mechanical slice does not activate
an incomplete replacement to reach them.

Provisioning remains responsible for model artifacts owned outside the agent
uid and not alterable by it. This code adds no mode-only ownership policy, new
file-size limit, descriptor passing, model choice or language qualification.
Owned-byte retention protects the admitted input even after pathname or inode
contents change; it does not attest that production provisioning is adequate.

## Instruments and author gates

The new filesystem corpus covers exact arbitrary locators; relative syntax
before acquisition; symlink-plus-parent semantics; parent replacement after pin;
manifest symlinks; allowed nested relative links; absolute/magic/escaping and
re-entering paths; missing, unreadable and nonregular inputs; bounded FIFO
children; malformed manifest bytes; every named digest; repeated references;
sidecar exclusion; and replacement after each acquired input.

A real Dispatcher/socket Ready fixture checks deferred reads after acquisition
scope ends, after Ready, after path replacement and after into_backend. Weak
references cease to upgrade only after the retained backend is dropped. Separate
fixtures exercise exact AdmissionRefused transport, unsupported production
adapters and forbidden fixture production readiness. Compile-fail probes pin
backend extraction and supplied-byte production admission as unavailable.

Floor tests independently cover locator syntax/field attribution, legacy
bindings, and closed named manifest/encoder/ArtifactFile shapes.

The root ran the pinned toolchain, lock gate first, --locked dependency commands
and serialized Cargo in /tmp/weaver-146-ste100-artifact-target on Olympus:

| Gate | Result |
| --- | --- |
| Full weaver-types, default | 142 passed, none ignored. |
| Full weaver-types, config | 198 passed, none ignored. |
| Focused artifact acquisition corpus | 15 passed. |
| Full weaver-spu, cuda,gguf | 336 passed, none ignored, including real GPU/model instruments and compile-fail doctests. |
| Full weaver-admin | 498 passed, 37 inherited ignored instruments. |
| Full weaver-harness | 1,500 passed, 18 inherited ignored instruments. |
| Changed host crates and SPU all-targets clippy | Passed with warnings denied; SPU used cuda,gguf on the Olympus lane. |
| Formatting, lock and diff checks | Passed. |
| Protective mutations | All 24 compiled and failed their named assertions; every exact restored source passed. |

The mutations pin locator syntax and field attribution; closed map-only manifest,
encoder and file objects; exact pinned parent; manifest no-follow; nonblocking
special-file refusal and regular-file checks; reference confinement; exact and
repeated digests; sidecar exclusion; retained manifest/file ownership; and both
production fixture guards. The bounded FIFO children fail on the lost progress
property instead of hanging the mutation runner. Compile-fail probes additionally
pin the absence of raw backend extraction and supplied-byte production admission.
The root inspected the assertion failures, not only the process exit codes.

The first sandboxed GPU build could not obtain CUDA compute capabilities; the
same source passed on the escalated host GPU lane. SPU lint then found a test's
zero permission literal written as 0; it is now 0o0, with identical permission
semantics, and lint/format checks passed. These initial logs remain in the evidence.
The full suites preceded that literal-only correction; mutation/restored checks
and final lint used the corrected source.

Local evidence is /tmp/state-ste100-artifact-gates/ and
/tmp/state-ste100-artifact-mutation-evidence/. The published source archive,
file hashes, exact tree/commit mapping and copied evidence belong under
/bulk-store/weaver-testing/state-management-146-olympus-2026-10-22/
ste100-artifact-owner-066d51a3/. The PR carries the final publication identity.
Internal source review closed backend-owner extraction and supplied-byte proof
bypass before these gates. It is an author-team review, not a Planner grade,
security review or production qualification.

O02-1/O02-2 artifacts, the dedicated executable, active lifecycle/author migration,
separate system prompt review, measured production bounds and coupled cutover
remain open. The supplied Issue 9 PDF and synthetic fixture artifacts do not
qualify production classification. This slice does not complete issue #146.
