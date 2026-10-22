# STE100 manifest locator proposal

Date: 2026-10-21. Scope: issue #146, item F. Draft PR: #235.

This is a document-only proposal for the locator gap in Spec 02 DECLARATION.
Operator approval and merge are owed. It supplies no runtime implementation,
language artifact, qualification result or serving cutover. Owning headers use
the post-merge state required by Working Process section 2; that header is not
approval of this branch.

## Proposed election

For Ste100Instruction alone, ModelBinding.artifact is the absolute pathname of
the exact regular ste100-model/1 manifest file. Relative values refuse as syntax
errors. No basename is imposed. The SPU opens it relative
to its pinned parent directory, refuses a symbolic-link final component, and
uses that parent as the root for every manifest reference. Referenced files may
use relative links only when every intermediate resolution step stays under
that root. Leaving and later re-entering the root refuses. Directory discovery,
alternate-root fallback, absolute links and magic links refuse. Object-kind
checks cannot wait for data from a FIFO or other special file.

The exact acquired manifest bytes are both parsed and hashed. The compiled adapter
consumes only checked, manifest-named inputs. It retains the hashed owned bytes, or
the same acquired file instance through a descriptor or mapping, until its final read.
A pathname replacement cannot select a new instance between checking and loading.
Pinning does not prevent an external writer changing an open inode. Provisioned
immutability remains part of the existing artifact custody obligation. Unreferenced
files are not inputs. Their presence alone does not refuse admission. The manifest
hash covers its bytes only, while each referenced file has a separate declared digest.

The SPU owns acquisition as the agent uid after exec. Admin preserves its existing
scheme/executable custody and the declared binding. The harness transports that
instruction without preopening model files or inheriting extra artifact descriptors.
Missing, unreadable, nonregular, escaping or digest-mismatched files use
unsupported_artifact. DECLARATION maps that and the other non-scheme admission
refusals to lifecycle template/template_failed. The scheme_mismatch code maps to
template/scheme_mismatch. Detail keeps the checker code and exact detail.
EnterFailure::AfterLoad's existing refusal and rollback use the original load budget.
The manifest admission replaces legacy container resolution for this duty, preserving
shared syntax and device judgments.

## Grounds and alternatives

[Spec 02][protocol] already fixes the manifest's strict representation,
SHA-256, named-file digests and no-escape rule. It did not choose the object
ModelBinding.artifact names. Its new MANIFEST-LOCATOR clause is the definition
site for that election.

[SPU PRD section 7][spu-prd] places model artifacts outside agent ownership and
forbids agent alteration. The same section assigns resolution/readability to
the SPU as the agent uid; root's ability to read a path does not establish that
the agent can read it. [SPU Spec section 3][spu-spec] already pins file instances
so hashing and loading cannot disagree after a pathname replacement. This
proposal carries that ownership requirement into the separate STE100 duty. It
keeps its existing manifest SHA-256 identity rather than importing the legacy
decoder's BLAKE3 directory walk.

| Alternative | Proposed disposition |
| --- | --- |
| Exact manifest file | Select: the operator supplies the exact file; no conventional filename or search order is needed |
| Directory plus a fixed manifest basename | Do not select: adds a filename convention and a second locator interpretation |
| Directory scan or file/directory fallback | Do not select: the same declaration could select a different input after a directory change |
| Admin opens and passes every model file | Do not select: changes the inherited descriptor boundary and moves readability judgment away from the agent uid |
| Reuse legacy decoder discovery/hash | Do not select: the STE100 manifest already fixes its named-file SHA-256 identities and compatible adapter |

No ModelBinding field, wire version, digest algorithm, capacity or numeric limit
changes. Device assignment keeps its existing judgment. Decoder and legacy
label-classifier artifacts retain their existing binding rules.

## Owning documents

- Spec 02 MANIFEST-LOCATOR defines acquisition, custody and failure disposition.
- The SPU amendment assigns that acquisition and retained ownership to its duty.
- The STE100 contract binds both parties to the locator and checked inputs.
- The types amendment preserves shared shape and puts filesystem judgment in
  admission, without making the config decoder a filesystem validator.
- The admin and harness amendments preserve the binding and existing process
  boundary; the domain index carries each revised version.

## Owed implementation and instruments

The next code slice can acquire the manifest and its named files into an owner
consumed by the existing supplied-byte admission helper. It must not provide
Ready through a test adapter or activate an incomplete serving checker.

| Property | Required evidence and protective mutation |
| --- | --- |
| Exact locator | Accept an absolute pathname naming a regular manifest; refuse relative paths and directories even with plausible manifests; adding cwd resolution or basename guessing must fail |
| Pinned parent and confinement | Exercise nested references, allowed relative links, intermediate escape followed by re-entry and a parent replaced after acquisition; accepting any escape must fail |
| Regular readable files | Refuse a linked manifest, missing/unreadable file, directory, FIFO or device without waiting for special-file data; removing the object-kind guard must fail |
| Exact manifest bytes | Changing whitespace without updating the declared digest refuses; hashing a reserialized manifest must fail |
| Every referenced digest | Missing file or mismatched digest refuses before Ready; repeated names may share an instance only when all hashes match; skipping a digest must fail |
| Checked instance feeds adapter | Replace a referenced path between acquisition and adapter read and require the checked instance; reopening the path must fail |
| Retained ownership | Exercise a deferred adapter read after acquisition scope ends and after Ready; dropping its necessary owner early must fail |
| Closed model input inventory | An unreferenced sidecar may be present but cannot become an adapter input; refusing its presence or consuming it must fail |
| Failure and launch custody | Acquisition failure sends admission_refused without Ready and maps to the existing lifecycle code with checker detail retained; no new artifact descriptor is inherited and the original load budget remains in force |

Those are owed tests, not tests run by this proposal. Fixtures prove acquisition
mechanics, not production classification. The existing primitive's supplied-byte
tests do not establish filesystem confinement or adapter ownership.

## Remaining production obligations

Resolving the locator does not supply O02-1's grammar ABI/compiler, complete rule
correspondence, dictionary, condition/StepId grammar or conformance corpus.
O02-2 still owes a real source-word span head and alignment adapter, measured
selection and a frozen held-out protocol. The available ModernBERT sequence
head supplies whole-message labels and is only an encoder candidate. The Issue
9 PDF is a reference, not qualification of any of these artifacts.

The executable/driver, symbolic template inclusion proof, separate prompt review
and certificates, measured bounds and coupled production cutover remain owed.
No deployment path or installed artifact is changed by this proposal.

## Validation

The revision addresses Planner review 5609275124's nine findings: legacy
container admission, absolute paths, unreferenced-file handling, special-file
nonblocking checks, file-instance custody, lifecycle mapping, the transport
record, intermediate link escape and manifest-hash scope. No operator election
or runtime conformance follows from these document corrections.

ASCII, prose width, local links, version-index agreement and diff whitespace checks
pass for this revision. The coordinating author's fresh lock gate passed. Two
independent document readings found no concrete semantic defect within the
nine-finding correction scope. Operator approval and merge remain owed. No Cargo, GPU,
artifact download or deployment test is claimed.

[protocol]:
../crates/state-management/state-management-spec-02-protocol-classifier-gate.md
[spu-prd]: ../crates/weaver-spu/weaver-spu-PRD.md#7-identity-process-boundaries-and-the-model-artifact
[spu-spec]: ../crates/weaver-spu/weaver-spu-Spec.md
