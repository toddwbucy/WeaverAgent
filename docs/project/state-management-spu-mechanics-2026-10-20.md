# STE100 SPU mechanics implementation

Date: 2026-10-20. Status: implementation evidence, not a Spec amendment.

This slice implements the complete transport, admission-proof and constrained
binding mechanics of [Spec 02][protocol]. It is C1a in the implementation map and
depends on the shared floor in PR #184. It adds no serving path or deployment
election. The production STE100 duty remains C1b and C1c work.

## Implemented boundaries

The SPU's ste100/channel module owns the connected Unix SEQPACKET end, bounded
26-byte framing, one partial logical receive and one pending send. It exposes
nonblocking progress so a driver can observe unwanted traffic while computation is
held. Framing faults retire both channel directions. JSON interpretation belongs to
the dispatcher. Existing label and decode channels keep their contracts.

The dispatcher implements Starting, admission, Idle, Checking and terminal states.
Its Check action transfers one whole request to the internal SPU worker. The driver
must continue polling the dispatcher while that worker runs. Completion checks for
already queued traffic before accepting the result. A correct language rejection
allows another request. An invalid result or declared component failure attempts
one terminal reply and ends the duty. No timer or request-priority policy lives in
this primitive.

The admission module accepts exact bytes acquired by a separate custody owner. It
checks the bootstrap limit, strict JSON, scheme and manifest SHA-256, grammar blobs,
fixed vocabulary, binder ABI and every manifest-named file. Only verified file bytes
reach a compiled adapter. That adapter must compile the grammar and instantiate the
compatible backend before producing the private Admitted proof. The dispatcher
compares the complete proven bootstrap with its own, including device assignment.
Test adapters and their constructor are unavailable to production callers.

The binder accepts grammar-valid candidates and an aligned word-score matrix. It
uses the fixed seven labels, source-order f64 accumulation from finite f32 scores,
and exact lexicographic ties. It returns shared Binding values with exact UTF-8
source spans, compound nouns and the scheme's step condition representation. A
fault in any action returns no accepted prefix. Candidate completeness and lexical
grammar correctness belong to the producer, not this scoring operation.

These modules live in the SPU library that already hosts its internal mechanics and
doctests. No other organ links it. Their tests exercise genuine socket pairs and
explicit synthetic artifact and model inputs. Such fixtures prove the mechanics,
not STE100 accuracy or model capacity.

## Remaining C1 work and Spec gap

C1b must supply O02-1's normative grammar compiler, condition grammar, dictionary,
rule correspondence and symbolic template inclusion proof. C1c must supply O02-2's
concrete encoder, tokenizer, span-head and alignment adapter, followed by measured
qualification. The existing ModernBERT sequence scorer cannot supply word spans.

Before filesystem admission can be implemented, Spec 02 must say whether
ModelBinding.artifact names the ste100-model/1 manifest file or a directory, and
name the manifest lookup rule. The present text defines the contents and relative
file references but not this locator. This slice chooses neither form. Supplying
already acquired immutable bytes to the admission helper does not assert path
custody, symlink confinement or file-descriptor pinning.

The third binary, descriptor-count and dumpable entry checks, compiled adapter
selection, internal sentence-worker assembly and real driver remain with that duty.
The binary-set manifest instrument arrives with its executable, not as a claim that
an incomplete binary is deployable. The complete replacement must pass the
foundation cutover before the worker starts it for serving.

O02-3 through O02-5 still owe measured waits, repair cap, wire limits and the reviewed
system prompt. The Issue 9 PDF is the normative source, not those artifacts. No
numeric production value is supplied by the fixture limits used here.

## Review and evidence

Independent local review checks the admission proof, channel failure paths and
binding algorithm. A review found that matching only Ready fields omitted device
assignment. Full-bootstrap equality and a regression for non-echo fields close that
gap. The mutation runs remove individual guards, require a compiled assertion
failure, and restore the source before the final suite.

The required GPU gate exposed an existing cancellation fixture that held a duplicate
child socket and accepted a receive timeout as closure. The corrected fixture drops
that handle, waits for the child's own exit and requires nonblocking EOF. Retaining
the handle fails at the EOF assertion. Removing the cancellation guard fails the
bounded exit wait. Both probes restore their source before the final device suite.

The pull request records the exact final validation commands and their results.
Host mechanics tests do not replace the crate's required Olympus cuda,gguf lane.
Neither a green fixture run nor this implementation note clears production cutover.

The transport review on 2026-10-21 found that recv returning zero also represents an
empty SEQPACKET datagram. A single immediate status probe now distinguishes live
malformed traffic from full or write-half EOF. A failed probe is terminal I/O failure,
including interruption. Returning Pending after consuming that packet would lose its
readiness and could permit a dispatcher reply after malformed traffic. Complete
nonempty messages still reach the dispatcher before a later EOF. Three compiled
guard removals exercise the empty-packet distinction, write-half closure and failed
probe. Each source is restored exactly before the gates. No new timer, retry or
dispatcher state is introduced.

[protocol]:
../crates/state-management/state-management-spec-02-protocol-classifier-gate.md
