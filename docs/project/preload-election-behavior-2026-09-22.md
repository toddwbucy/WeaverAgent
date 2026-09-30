# Explicit preload election behavior

Written 2026-09-22 against main at `3509e0b`, following the W5b measurement in
PR #649 and the operator's direction that the election distinction be explicit
in behavior. This is an authoring proposal. It records the ruled distinction
and explains the representation and refusal cases in the accompanying corpus
amendment. The amendment remains subject to review and the human merge call.
Implementation does not precede that authorization.

## The ruled distinction

An ordinary reconstruction uses the election the record says governed its
holdings. A diagnostic projection uses analysis's own election only when the
operator explicitly selects that behavior and names a different destination
session. A different selection must not silently replace the original session's
holdings while retaining its identity.

The harness-state contract already states the distinction in its election
vocabulary: replay uses the recorded rule or preloads a different session under
a rule of its own. Analysis's current Spec instead requires one fixed election
for every preload and derives the diagnostic declaration's session from the
source envelope. The accompanying corpus edits change those clauses together before
the code does.

## The evidence

W5b's canonical record contains 24 events. Under its load's declared election,
`all_kinds: true, keys: []`, the live store holds all 24 envelopes. Under the
analysis election it holds 17 events with selected payload fields. The seven
excluded events are three starts, three closes, and one tool-call event.
Turnless system messages carry their payload under the tee's standing exception
in trace Spec section 11, including under an election with no payload paths.

Analysis currently selects seven kinds from a constant in `project.rs`. It
preserves the recorded `load.tee` value as data without using it to select the
preload's election. That selection can therefore remove events and add payload
fields relative to the original holdings. The full trace can contain fields
that the serving election did not retain in state.

The current deployment template is a third concrete spelling: every kind, plus
`content` for user and assistant messages. The older serving-source fixture and
W5b record declare every kind with no elected paths. Neither is equivalent to
analysis's fixed seven-kind election.

W5b proves three-way equality when both construction paths use the same
analysis election. It does not prove that the current preload reconstructs a
serving session under the rule its record names. The raw-value renderer repair
in #649 is independent of this policy decision and remains valid under either
mode.

## Proposed command behavior

The default `preload` uses the recorded election. An explicit `--diagnostic`
selects the existing analysis election and requires `--as <session>`, where the
destination differs from the source session. This proposal retains the existing
cut and rename spellings rather than introducing another driver or wire form.

| Invocation | Election | Destination |
| --- | --- | --- |
| `preload <trace> <socket>` | Recorded rule | Source session |
| `preload <trace> <socket> --as <branch>` | Recorded rule | Named branch, whole record |
| `preload <trace> <socket> --through <run>:<turn> --as <branch>` | Recorded rule | Named nonempty source-distinct branch |
| Ordinary cut with missing, empty, or source-equal `--as` | Refused | No traffic |
| `preload <trace> <socket> --diagnostic --as <diagnostic>` | Analysis rule | Named distinct session |
| `preload <trace> <socket> --diagnostic` | Refused | No traffic |
| Diagnostic mode with an empty or source-equal destination | Refused | No traffic |

`--through <run>:<turn>` applies in either mode only with a nonempty destination
different from the source. Ordinary mode therefore requires `--as` when cut, and
a bare or source-named cut refuses at the driver before connecting or sending an
opener. The whole record under its own name is a resume, per `weaver-state-PRD`
section 4 and `weaver-admin-Spec` section 4. The cut selects the prefix ending
at the named close event before election validation and projection. The existing
refusals for a missing run, missing turn, and unclosed turn remain. Every
preload accepts one source session, and a selected prefix containing multiple
source sessions refuses rather than coalescing them through a rename.

For ordinary reconstruction, each included run must have its recorded load and
an unambiguous valid tee rule before its projected events. An absent rule,
malformed rule, duplicate conflicting load, or event without its run's governing
load refuses before connecting. Missing evidence is named, never supplied from
the deployment template, analysis's constant, or another run. A false
`all_kinds` and an empty kind list is a valid explicit election, not absence.

A preload sends one opener with one election. The proposed first implementation
therefore refuses ordinary reconstruction when included runs declare different
elections. It does not choose the first or last rule, union their paths, or send
multiple retiring openers. Recorded elections retain the tee's first-matching-kind rule.
When multiple
entries name one kind, only the first elects its paths. Resolve that precedence
before comparing effective rules. Reordering distinct kinds or paths within an
effective entry is harmless, but reversing conflicting entries for one kind can
change selection. The all-kinds bit is never inferred from the events present. Support
for faithfully restoring a history
whose election changes is a separate representation decision. A cut excluding
the change can still reconstruct the earlier prefix.

Diagnostic projection may use its own rule across source runs with different
recorded elections because the destination explicitly represents that new
selection. It preserves available source election evidence verbatim. Missing
source evidence cannot be manufactured or certified by choosing diagnostic
mode. The diagnostic loop's evidence and certification refusals still apply.

Semantic readers continue to ignore kinds and members they do not understand.
The raw projection retains them: an all-kinds rule carries a future kind's
envelope, and an elected unknown path crosses verbatim. Unknown content does
not become diagnostic evidence merely because custody retains it.

Both modes keep landing order, splice elected payload values verbatim, and
preserve absent, null, and empty-string distinctions. Recorded-rule projection
also reproduces the tee's turnless-system exception independently, without
adding an analysis-to-trace dependency. State receives the existing opener,
distillates, and seal. Its parser, retirement transaction, and ask vocabulary
need no new policy.

Mode, destination, cut, and election must be validated before the driver connects
or sends an opener, because the opener can retire existing holdings. The
successful driver report names the selected mode, source session, destination
session, effective election, projected count, and seal. Reporting the rule is
an account of the selection, not a claim that the destination was previously
unused. The driver does not gain a query on the preload seam to establish that.

## Diagnostic declaration and identity

The diagnostic declaration and preload opener must name the same destination.
The proposed `derive` command therefore requires `--as <diagnostic>` with
the same nonempty, source-distinct rule as diagnostic preload. The record remains
the authority for source-run facts, while the diagnostic destination is an
explicit analyst input. This changes the charter's present list of three
analyst inputs and must be reflected there, not only in the CLI Spec.

The declaration's session and each projected envelope use the destination name.
Source run, turn, sequence, payload values, and source-record digest retain their
recorded meanings. Documentation and reports must distinguish source identity
from the diagnostic session's identity. Diagnostic certification must not call
the richer diagnostic holdings a reconstruction of the original state election.
Any identity comparison that currently equates source and destination session
names must be revised alongside declaration derivation, without weakening the
comparisons of source evidence.

A concrete operator sequence is:

```text
weaver-analysis derive trace.ndjson --as diagnostic-session [existing options]
# Load that declaration through the existing admin procedure.
weaver-analysis preload trace.ndjson preload.sock --diagnostic --as diagnostic-session
```

The driver remains outside the agent and the existing privileged preload door
remains unchanged. A name distinct from the source is required, not a claim of
global uniqueness. Repeating the same explicit diagnostic destination retains
the existing retry and retirement semantics for that destination alone.

## Corpus amendment before code

The amendment follows one account across these authorities:

| Authority | Clause to amend |
| --- | --- |
| `weaver-analysis-PRD` section 3 and section 4's closed election cell | Separate reconstruction and diagnostic selection, and make diagnostic destination an analyst input |
| `weaver-analysis-state-contract` Vocabulary and sections 2-3 | Bind the opener to the destination, state the election distinction and preflight refusals without CLI spellings |
| `weaver-analysis-Spec` sections 3-4 and its instrument list | Define mode selection, recorded-rule validation, diagnostic rename, declaration input, and observable report |
| `diagnostic-replay-loop` sections 2-3 | Coordinate declaration and preload destination, separate source evidence from diagnostic holdings in certification |
| `weaver-harness-state-contract` Vocabulary | Retain its recorded-rule-or-different-session requirement as the governing distinction |

The authoring pass must also find callers and instructions that presently rely
on bare preload's diagnostic election. They migrate to explicit diagnostic mode
and the same renamed declaration. W5b's analysis-election comparison is one such
caller. Its measured recorded-rule branch becomes the basis for a second
three-way comparison of ordinary reconstruction. A passing old comparison must
not be preserved by silently keeping the old default.

## Implementation evidence owed after ratification

Ordinary reconstruction must be checked against the independent record walk
and the real tee under the recorded election at both cuts, including the
all-kinds record and the turnless-system exception. Explicit diagnostic
projection must be checked under analysis's election with both comparison paths
using the same distinct destination. Both modes must preserve raw pair values
and absent/null/empty distinctions through the real member's asks. Include a
future kind with an elected unknown path, an all-kinds rule with no named kinds,
and a restrictive empty rule retaining only the turnless-system exception.
An ordinary excluded event must remain absent.

Refusal tests must establish that a bare or source-named cut, invalid mode,
destination, cut, source
session, or required election evidence sends no opener. A pre-existing
destination's holdings must survive each refusal. Missing and conflicting rules
must not fall back to the fixed analysis election. Conflicting duplicate-kind
entries must retain first-match behavior, and reversing those entries across
runs must trigger the differing-rule refusal. Reordering distinct kinds or the
effective entry's paths must not change that selection. Multiple runs with equivalent
rules must work, and a recorded-rule change within the selected prefix must
refuse under the proposed one-opener limit. A change after the cut must not
invalidate the earlier prefix.

Perturbations must restore the fixed default, ignore the diagnostic rename,
accept a source-equal diagnostic destination, default missing election evidence,
and move validation after the opener. Each must fail at the property it removes.
Dead-driver retry must still replace only its declared destination and release
parked asks only at the seal. The dependency graph, raw-value independence, and
operator ownership of merge remain unchanged.

## Related claims retained and enforcement owed

The state-claim sentences in `weaver-agents-PRD` section 8 and
`weaver-diagnostic-PRD` section 4 remain unchanged: both constrain a claim to
reproduce the source state, while an explicitly renamed diagnostic projection
makes no such claim. The loop document now distinguishes that diagnostic
operation from a faithful state reconstruction. This act does not change the
apex's certification boundary or waive the diagnostic charter's input-evidence
requirements. A new name alone cannot certify source-state equivalence.

Five new perturbation nodes name the unimplemented behavior: recorded-rule
reconstruction, validation before the opener, a distinct diagnostic destination,
explicit declaration destination, and the effective-selection report. Existing
nodes retain their previous scope. All five new enforcement rows are marked
owed and remain uncited until their implementation and perturbations land.
The census baseline deliberately records five additional uncited perturbations,
from 28 to 33, and one fewer tableless document, from 11 to 10, because the
analysis Spec now has a complete enforcement inventory. Every other counter is
unchanged. The code act must add real instruments and remove those five owed
entries from the uncited baseline, not merely add citations to old tests.

## Landing sequence

This proposal is a reviewable record of the operator's direction and the concrete
behavior proposed to implement it. The accompanying corpus amendment is the authoring
act. Its review and human merge precede implementation under Working Process
section 6, H1: no code without a ratified Spec. PR #649 remains the completed
measurement and renderer act. This proposal does not reopen it or silently add
new production behavior to its reviewed head.
