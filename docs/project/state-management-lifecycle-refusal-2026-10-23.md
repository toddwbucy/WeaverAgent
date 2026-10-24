# State management lifecycle refusal carrier

Date: 2026-10-23. Branch: code/state-management-refusal. Exact parent #281
eddcd9cafdb890f82a17ceb505f1cda3226b4d08. Advances issue #146; closes no issue.
This adds the elected floor representation and a dormant value adapter, not
the complete healthy-refusal lifecycle owner.

Implements: merged TYPES-ENTER, Spec 02 DECLARATION and HARNESS-CLASSIFY
admission mapping and complete-envelope rules. Author assessment: conforms
within this protocol/value scope. No unmerged proposal supplies authority.

## Floor and mapping

StateManagementInvalid carries the four declared stages, nine codes and ten
legal stage/code combinations. Its variant-specific decoder enforces that
relation through direct Deserialize, Payload, OrganEnvelope and recursive
OrganRefused. Stage/code must be strings and the variant's named fields are
closed; unknown, missing, duplicate and malformed fields are refused. The
similarly named read, placement and persistence vocabularies remain separate.
Optional rule_id/path are omitted when absent; explicit null retains existing
Option semantics. No extra nonempty or stage-specific attribution rule is added.
The exhaustive public enum doctest gains the new arm.

The private adapter borrows the actual exchange and checker admission refusal.
It validates source protocol/detail, maps scheme_mismatch to template /
scheme_mismatch, and maps the other four checker admission codes to template /
template_failed. The lifecycle detail contains the canonical checker code,
a readable colon separator and the unchanged original detail. This separator
is an implementation rendering, not a new machine-parsed protocol. There is
no path or rule_id inferred from prose: the checker source has no such fields.

The adapter constructs the original exchange's Close/Refusal envelope and
checks its complete serialized bytes against MAX_ENVELOPE_BYTES (65536).
An oversized response returns that entire envelope with existing Truncated;
serialization failure returns it with Undecodable. Neither branch shortens
or replaces the detail. Invalid typed source returns its validation error
while both borrowed inputs remain at the caller. The original error envelope
stays inline under a local lint allowance, avoiding a new allocation merely
to return rejected custody.

The success value is an ordinary OrganEnvelope, not a Recorded proof. This
helper owns no recorder, port, clock or child. Its future caller still owes
actual admission provenance, trace-before-answer and explicit rollback under
original bounds. Existing transport byte checks remain independent backstops.
There is no active lifecycle or worker caller.

## Tests and evidence limits

Three floor tests cover all 36 stage/code combinations, exact optional strings
and malformed fields. Nested coverage uses three carrier depths: direct,
one OrganRefused and two OrganRefused wrappers, with Payload/OrganEnvelope at
each depth. It does not claim three additional nested wrappers.

Five mapping tests cover all five checker codes, exact source detail and
correlation, complete envelope size, actual Unix SEQPACKET transport and Busy
ownership. Exactly 65536 serialized bytes cross the actual port; one byte over
returns the original complete envelope and independently fails transport
staging without shortened peer bytes. Escaping, multibyte UTF-8 and wrapper
overhead are counted. Busy keeps the first queued packet, returns the exact
second and permits its later send under original correlation.

The NoCompiledAdapter detail check is a value fixture, not a process test or
qualification of a backend. Invalid protocol/empty detail are defensive typed
inputs. The current concrete fields serialize successfully, so Undecodable
encoding failure is not claimed independently exercised. Public vocabulary
is cloneable data; it grants no executable capability.

## Validation and remaining scope

All eight source paths matched their exact preimages on #281. Initial source
review is clear. Root formatting changed no semantics; a unique mutation
anchor was refreshed for rustfmt's SchemeMismatch match-arm braces.

Types pass 171 default and 229 config-feature tests. Harness passes 1,798 with
18 inherited ignored cases; trace passes 160; admin passes 498 with 37
inherited ignored cases. Gate/state all-target checks and the SPU cuda,gguf
all-target consumer check pass. Ten nested child-exec summaries are retained
separately from the harness top-level count.

Those full harness/admin/consumer gates precede one exact private five-field
tuple alias added for Clippy type_complexity. The decoder body and return
representation are unchanged. Final types suites, all eight focused tests,
types/harness all-target lints, formatting, lock and diff checks pass after
that alias. The initial lint failure and prior source are retained.

All 15 source-pinned behavioral mutations compile and fail their named tests,
with exact restored passes. All eight restored source hashes match the tested
pre-control snapshot. No failure or survivor required a production/test change.
These are author checks, not a Planner grade or production qualification.

Healthy Enter refusal still needs the real classifier/admission owner,
trace-before-answer, original context/resources, explicit rollback Leave and
its cause, actual Grants/inspection custody, waiting-author handoffs and
bounded child/LastWord/Release/Unload/drain composition. This adapter does
none of those operations. The production classifier remains without a
compatible compiled adapter; a complete language artifact, reviewed prompt
and measured qualification remain owed. The required external Enter budget
carrier is proposed in #282, and the revised author-return/boundary choice
remains in #276. Neither is implementation authority while unelected/unmerged.
#260/#263/#268 remain held as well. Existing serving stays active until full
qualified coupled cutover. New PRs are drafts; the operator merges and
controls CodeRabbit requests. Issue #146 remains open.
