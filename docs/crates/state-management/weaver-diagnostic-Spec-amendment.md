# weaver-diagnostic Spec: isolated legacy conversion amendment

**Status:** MERGED.

**Landing PR:** #312

**Version:** 0.9 **Date filed:** 2026-10-20 **Parent:** weaver-diagnostic-PRD

## 0. What this document is

This replacement applies at the [foundation cutover][cutover]. Until then, the previous
diagnostic contract remains applicable.

Amend harness Spec 4's Record conversion, diagnostic Spec 3.1/3.2's received vocabulary
and harness-diagnostic contract 3/7. The converter is
crates/weaver-harness/src/record.rs::convert, not code in weaver-diagnostic. This is the
local near side of R8. WeaverAnalysis's serving trace-reader changes remain external
work. No diagnostic path becomes a serving state feed. [SCOPE-1, FLOW-1, October 18 R8]

## 1. Vocabulary and preload, destinations sections 3.1 and 3.2

This replacement applies at the [foundation cutover][cutover]. Until then, the previous
diagnostic contract remains applicable.

**DIAGNOSTIC-CONVERT.** Keep diagnostic's legacy record envelope and existing 20-kind
vocabulary. Diagnostic native constructors keep their existing envelope. For shared
legacy author calls, Record's diagnostic arm still receives a trace Event. Its convert
function explicitly checks the supported kind/payload, copies each legacy member, and
discards channel/origin by a named mapping rule, since diagnostic owns neither field.
Tests require those two omissions and every other field's preservation. The harness's
converter to diagnostic refuses the new kinds in TRACE-REGISTRY and new serving
recall/save payload variants and unloads carrying either TRACE-HALT failure field as
OutsideDiagnosticVocabulary, rather than dropping fields and claiming member-for-member
conversion. Legacy fixtures keep their existing canonical spelling. Reason: replay and
serving are distinct bindings, not interchangeable schemas. [SCOPE-1, FLOW-3]

**DIAGNOSTIC-STATE.** The diagnostic binding selects the old store engine interface,
election and preload door on a separate connection/catalog. It never adopts a
state-content/1 serving dump or supplies a serving seed. No serving load can enable the
preload door. This stages the local interpretation of analysis-state contract 1/2/3,
whose copy in this repository is authoritative under R20-5. WeaverAnalysis mirrors it
and owns its counterpart changes. TRACE-REJECTED-RETURN's reader compatibility for
object-valued and string-valued rejected-return evidence is required before the
foundation cutover, as tracked by
[WeaverAnalysis#15](https://github.com/toddwbucy/WeaverAnalysis/issues/15).
Other diagnostic/preload counterpart work does not become a serving deployment gate.
The reader obligation grants no preload traffic and does not change
DIAGNOSTIC-CONVERT's refusal of new serving kinds. [SCOPE-1, PERSIST-5, October 18 R8]

## 2. Enforcement, destinations sections 3.1 and 3.2

This replacement applies at the [foundation cutover][cutover]. Until then, the previous
diagnostic contract remains applicable.

| Claim | Instrument | Protective change that must fail |
| --- | --- | --- |
| DIAGNOSTIC-CONVERT | Harness record.rs total conversion match and old/new vocabulary corpus | Silently discard a serving field or admit a new kind |
| DIAGNOSTIC-STATE | Diagnostic/serving launch matrix | Open preload on serving or adopt the wrong catalog |

## 3. Open elections and dependencies

No new replay format is elected. R20-5 elects this repository's analysis-state contract
as authoritative. WeaverAnalysis owns mirror conformance and its reader tests.

[cutover]:
state-management-foundation-Spec.md#1-applicability-and-the-serving-cutover
