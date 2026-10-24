# STE100 artifact acquisition review followups

Date: 2026-10-22. Issue #146; PR #259 review 5622813219.
Authoring seat implementation record; ordinary gates and 32 protective mutations pass.
Branch: code/state-ste100-review-followups. Parent: exact #262
7cad4da7c4611d214d2fcbdc28442da88ef3cbb2. The acquisition edits and closed
scheme codecs were independently staged against exact #259, then joined with
a three-way comparison. Their only conflict was two independent appended test
groups; both complete groups are retained.

## Authority and scope

| Implements | Author source assessment |
| --- | --- |
| Spec 02 DECLARATION / MANIFEST-LOCATOR | Conforms for exact declared path, pinned parent and regular descriptor, checked retained bytes, confined references, each digest and owner lifetime. Dedicated-target runtime verification passes within this scope. |
| TYPES-CONFIG STE100 locator | Conforms for earlier refusal of an empty, dot or parent terminal component at the existing exact config field and direct-bootstrap boundary. Parent spelling and legacy bindings remain unchanged. |
| SPU-STE100 / STE100 contract section 1 | Conforms for cheap input validation before referenced acquisition and retained owned inputs through the existing admission primitive. No adapter, executable or production activation is supplied. |

The merged locator rule requires an exact regular manifest file and declares no
filesystem manifest byte ceiling. The bootstrap wire limit remains a message
bound. This change does not repurpose that limit or elect a new admission size.

## Acquisition and admission

The owner now keeps each file as an Arc holding its original Vec allocation.
Sharing ownership does not copy the file payload into a second allocation. The
adapter still receives immutable views and retained shared owners, and the backend
still cannot be extracted from its input owner. This is an allocation-shape fix;
peak memory and model-load latency still need production qualification.

Scheme identity, grammar and condition checks run before filesystem acquisition.
The exact acquired manifest is checked next; scheme/manifest ABI and production
fixture compatibility are checked before opening any referenced model input.
Known fixture scheme markers still reach manifest acquisition before that combined
compatibility check. No broader claim about all cheap incompatibilities is made.

The manifest's verified regular descriptor is read through a fixed scratch buffer
for its digest first. A wrong large locator refuses before retained-byte allocation.
A matching digest permits materialization from the same descriptor; those retained
bytes are hashed again and parsed once with the existing closed codec. The second
hash is necessary because an in-place modification can occur between the two reads.
The parsed manifest moves through admission with its byte owner rather than being
rehashed, reparsed and revalidated downstream. A caller-declared large artifact
with a matching digest still requires allocation; no size qualification follows.

Nonblocking open still precedes the regular-file check. After that check the same
descriptor has O_NONBLOCK cleared. Confined reference acquisition retries EAGAIN
at most three times with the same root, path and flags; other errors return
immediately. The finite retry count is an implementation choice, not a new timeout
or renewed load budget. NO_MAGICLINKS remains explicit although this kernel's
BENEATH behavior already prevents magic links, so a separate removal mutation
cannot distinguish the two flags on this kernel.

## Review dispositions

| Row | Disposition |
| --- | --- |
| 1 | Carried to actual duty wiring: transfer the genuinely validated Bootstrap from Action::Bootstrap to production admission without serialization or a permissive alternate constructor. The current production helper still accepts raw bytes and has no production loop caller. |
| 2 | Changed shared storage to preserve each original Vec payload allocation; deferred ownership remains covered. Measured peak memory remains owed. |
| 3 | Moved scheme validation before acquisition and manifest compatibility before referenced inputs. |
| 4 | Added streaming digest precheck and same-descriptor retained-byte verification, without a new size ceiling. |
| 5 | Root-only unprivileged re-exec checks target execution and temporary-directory access as the actual target uid/gid. An unavailable environment is reported as a skip, not permission-boundary evidence. |
| 6 | Refuse terminal empty, dot and parent components in the existing STE100-only lexical validator, preserving raw parent components and exact config field attribution. |
| 7 | Added bounded same-root EAGAIN retries and cleared nonblocking mode only after the regular-file check. |
| 8 | Validate the instruction once on the production entry and carry the single parsed manifest through admission. Standalone acquisition still independently validates its input. |
| 9 | Corrected comments to distinguish shared production bootstrap parsing from test-only supplied admission; documented the current NO_MAGICLINKS mutation limitation. |
| 10 | Carried to O02-2 duty wiring: concrete operations on the retained backend must preserve input lifetime and prohibit bare-backend extraction. No fake runtime operation or arbitrary borrowing callback is added. |

## Evidence boundary

Nine new source tests cover original payload allocation, a large incorrect manifest
stream, same-descriptor pathname replacement and in-place modification, ordinary
regular-file flags, finite retries, early scheme and compatibility refusal, fixture
instruction binding and root-only environment preflight. Existing actual Ready and
deferred-read custody tests use the new retained owner representation. Existing
floor/config/direct-bootstrap tests now cover the terminal-component cases.

Fourteen compiled protective mutations fail as intended for those guards and their
ordering. The root-only preflight test depends on a suitable privileged environment
and is not counted as a mutation on the author uid 1000 lane. The separate
NO_MAGICLINKS limitation is also explicit. All fourteen restore controls pass; their final source hashes match the tested files.

The production dispatcher/adapter integration, concrete grammar compiler and span
adapter, qualified language scheme/dictionary/corpus and model, measured limits,
complete caller migration and coupled serving cutover remain open under #146.

## Closed scheme objects

Merged Spec 02 SCHEME and paired SPU admission require the named JSON objects,
not serde positional aliases. Seven named scheme/nested objects now decode only
maps with required fields, rejecting unknown/duplicate keys through every entry.
RuleDisposition accepts only its existing approved_word/technical_use strings.
Public Rust fields, serialized spellings and legitimate arrays remain unchanged.
Nine floor tests plus one actual filesystem/production-admission fixture cover
all nested object positions and enum aliases. The valid control reaches the
rejecting production adapter after actual acquisition; malformed schemes now
fail in the cheap pre-filesystem phase. No fixture creates production Ready.
This section is author-graded conforms within the closed decoder boundary.

Independent source reviews found no remaining concrete blocker before execution.
The combined slice passes all 32 compiled protective failures (18 codec, 14
acquisition), with 32 exact restored passes and every final source hash verified.
The private target is /tmp/weaver-146-ste100-review-followups-target.

## Executed author gates

| Gate | Result |
| --- | --- |
| Lock | 0, before Cargo |
| Types default / config | 158 / 214 passed |
| SPU artifact-focused | 25 passed |
| Full SPU cuda,gguf | 346 passed, zero ignored; real loaded, native, readout and two-card tests ran |
| Admin | 498 passed, 37 inherited ignores |
| Harness | 1,551 passed, 18 inherited ignores |
| Types/admin/harness all-target clippy | Pass with warnings denied |
| SPU cuda,gguf all-target clippy | Pass with warnings denied |
| Formatting and diff whitespace | Pass |

Every resolving Cargo invocation uses --locked. Gates and mutations run serially;
source stays in this checkout's private target. Ordinary gates had no failures.
The 32 expected mutation failures each compiled, failed the targeted test, and
passed after exact source restoration. The root-only preflight remains conditional
on privilege and environment; this uid-1000 run is not a root permission-boundary
qualification. These results are author evidence, not a Planner grade or a
production artifact qualification.

Host evidence is /tmp/state-ste100-review-followups-gates/ and
/tmp/state-ste100-review-followups-mutation-evidence/. The published source archive,
source/input hashes, logs, recipes and independent reviews are deposited at the
shared evidence destination
/bulk-store/weaver-testing/state-management-146-olympus-2026-10-22/ste100-review-followups-7cad4da7/.

