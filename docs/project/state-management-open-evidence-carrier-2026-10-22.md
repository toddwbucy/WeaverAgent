# State management Open evidence carrier

Date: 2026-10-22
Branch: code/state-open-evidence-carrier
Base: main at 45f8483393ce25af4ade3f0fa7bd96c1e8ed78f1
Epic: #146
Authority: merged #232 and the paired state management amendments

## Scope and authority

This slice implements the paired TYPES-DECODE Opened carrier, SPU-OPEN evidence
production, HARNESS-OPEN evidence validation for existing Open consumers, and the
TRACE-PLACEMENT outer carrier and admission rules. It does not complete the
HARNESS-OPEN transaction owner or activate the replacement serving path.

Opened requires the echoed SessionId, four actual counts and both raw evidence
boxes. Its emitted fields retain their declared order. Equality compares every
fact and the raw box bytes. The outer object rejects missing, duplicate and
unknown fields, and arrays cannot stand in for named objects. TokenAnswer keeps
raw kind/body values until its adjacent-tag decoder can see kind first. Thus
body-first JSON retains the same evidence spelling without normalization. This
adapter also preserves the existing Generated, ReFed, Fault and AtRest shapes.

The SPU renders and tokenizes identity once. The same prepared vector reaches
Session::open and supplies the evidence after successful prefill. The request
contains the complete family render and template. The measurement contains the
selected model, weights hash, exact prefix IDs and one UTF-8 byte block covering
that render. It invents no generation, seed, sampling or timing readings. Ordered
JSON uses existing serde_json string escaping and numeric encoding. No new crate
dependency or shared floor measurement mirror is introduced.

The existing harness Open consumer checks the echoed session, count relation,
terminal token, closed object shapes, every u32 token ID, exact token count and
one complete identity-prefix byte block. Duplicate checking precedes typed use.
The validator drops its temporary parse trees before returning and leaves the
original raw evidence unchanged. It neither renders nor tokenizes, and retains
no usable token cache. Diagnostic Open authors no extra trace event.

Trace adds a serialize-only OpenEvidenceRecord as the required final placement
field. Boot requires evidence objects, assembly zero, no turn, prefix counts and
empty content metadata. Cut and append require null evidence and nonzero assembly.
Trace checks only these outer facts. It does not interpret the SPU boxes or choose
fit. The recorder compares a boot causal parent with the actual assigned sequence,
not a caller placeholder. Placement cannot use terminal reserve or a diagnostic
projection. The later harness owner must prove that parent is its boot recall.

## Instruments and validation

The shared strict corpus covers old Open replies, required and closed fields,
duplicates, numeric domains, wrong echo, malformed evidence and array aliases.
It includes escaped strings, zero counts and body-before-kind input. The floor
compares decoded boxes with independent raw slices from the original wire. The
harness runs the corpus through both the validator and actual diagnostic socket
exchange. The SPU serializer retains the same raw boxes. Trace checks only its
opaque object boundary and outer count constraints on the same corpus.

Producer instruments cover all registered family renderers, one tokenizer call,
exact prepared IDs, Unicode byte offsets, string escaping, empty token vectors
and refusal paths. The existing real-residency instrument now compares the full
Open reply on both GGUF and native backends, with frozen and tunable limits.
Separate transport instruments size the complete producer encoding to exactly
8 MiB and one byte beyond. The oversized message must send no preamble. Synthetic
large evidence establishes transport behavior, not model admission or production
memory qualification.

Author validation passed on Olympus with private target
`/tmp/weaver-146-open-evidence-target`, the pinned toolchain and `--locked`:

- Harness: 1,460 passed, 18 inherited ignored instruments.
- Types: 133 passed in the default suite, 188 with the config feature.
- Trace: 160 passed. SPU with cuda,gguf: 318 passed.
- Strict affected-crate lint, formatting, lock and whitespace checks passed.
- Seventeen compiled protective mutations failed at assertions. Each exact source
  restoration was hash checked and its focused test passed, including real prefill.

The SPU run explicitly named the Qwen2.5 0.5B GGUF and native artifacts and exercised
both real residencies. Its broader native, readout and two-card suites also passed.
The shared malformed/valid corpus has 68 cases. These totals include doctests and
are author checks, not a Planner grade or production qualification.

Evidence, artifact fingerprints, mutation recipes, logs and source hashes are in
`/bulk-store/weaver-testing/state-management-146-olympus-2026-10-22/`
`open-evidence-carrier-45f8483/`. Local originals are `/tmp/state-open-gates/`,
`/tmp/state-open-mutations/` and `/tmp/state-open-host-suites.log`.
Independent source review identified object-array aliases and a weak raw-byte
instrument, both corrected before the final gates. The code-review and verification
skills required by the local process are unavailable in this session. No invocation
of them is claimed. Actual gates and independent source review are recorded here.

## Remaining integration and qualification

The full boot owner must consume private BootHead provenance, reserve the exact
Open exchange on its source port, retain the original Enter bound, prove complete
send and response association, record one truthful boot placement, and check fit.
Only Recorded plus successful fit and a still-live Enter may initialize journal
facts and boot mark readiness. Cancellation must drain an already sent exchange
and account valid evidence through ordinary trace capacity under the inherited
Leave/Halt bound. It grants no placement or generation. Failed fit still records
the seated prefix before boot_failed. None of these absent owner stages is
claimed by this carrier.

Production qualification must measure complete encoded trace size and retained
memory, including the raw outer body during decoding, both evidence boxes, the
temporary harness parse tree and the protected second trace copy. The recorder
has no per-record byte ceiling. Current tests do not elect model/context limits,
qualify the language corpus or classifier, approve a prompt, or establish full
failed-run teardown. Existing serving remains active until the qualified coupled
cutover. The operator alone merges and deploys. Issue #146 remains open.
