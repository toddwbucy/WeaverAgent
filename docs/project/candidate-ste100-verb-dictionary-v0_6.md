# Candidate: the STE100 verb dictionary for state management

Status: candidate v0.6, 2026-10-28, for operator ratification. v0.1 was the drafting
seat's. v0.2 to v0.6 are the Planner seat's revisions after the drafting seat's reviews
and the operator's words of 2026-10-28: the four strikes, no write key, and, agreed in
session and proposed here for ratification, the pair as the recall shape, the flag as a
mark on a write (D9), and the order of work in section 8. The operator holds v0.1 to
v0.5. Serves O02-1 of State Management Spec 02
(MERGED, file version 0.18), the computing dictionary part only. Source standard:
ASD-STE100 Issue 9, 2025-01-15, the operator's copy, SHA-256
d1f4ea9e7cd6e46b47aa9057209f99e78c0e9cfc4e27a5b07895b05c1a166431.
Repository read at WeaverAgent main 0c80e52 (merge of #371, 2026-10-27).

## 0. Boundary

This document IS the election of which verbs enter the `verbs` array of the
`ste100-scheme/1` artifact, which of them are recall keys, and the policy for technical
names. It IS NOT the artifact itself, the grammar or its compiler, the condition and
StepId grammar, the negative corpus, the model selection (O02-2), or the system prompt
(O02-5). It changes no merged Spec. Where a finding needs a Spec change, section 6 names
it as a delta for its own pull request and drafts nothing.

The store schema is not owed. `state-content/1` is merged in the weaver-state Spec
amendment v0.22, STATE-SCHEMA, with `statement_binding` holding the verb key and
`binding_noun` holding the nouns. This document fills the vocabulary that schema files
against.

## 1. What the standard gives

Part 2 of Issue 9 states 875 approved words and 1274 unapproved words with approved
alternatives. Two independent mechanical extractions of the operator's copy (the
drafting seat's and the Planner's) each find 208 approved verb headwords, four of them
multi-word, and 29 with more than one numbered approved meaning (BE's two meanings are
numbered in the text). That figure corrects the "roughly 150 approved verbs" used in the
session of 2026-10-12. Each verb entry lists its approved forms (KEEP, KEEPS, KEPT), so
the artifact's `forms` field is mechanically fillable. The extraction is a census for
this candidate, not the qualified source, and O02-1 still owes a checked extraction.

The dictionary carries no technical verbs. Rule 1.12 admits a verb outside the
dictionary when it falls in one of four technical verb categories, and gives example
lists for each. Category 2 is computer processes and applications, with three
subcategories and 50 example verbs (section 4 lists them). Rule 1.12 also states the
governing preference, verbatim: "If there is an approved verb in the dictionary that
accurately gives the instruction or the information, use the approved verb. Do not use a
technical verb if it is possible to write the same sentence with the words (verbs and
other related words) that are approved in the dictionary." The standard's own example is
"apply grease" in place of "grease", and it adds that the category lists "are only
examples" and do not give a full list of possible technical verbs.

## 2. Election rules proposed

**E1. Every approved verb enters unchanged.** All 208 headwords become keys with their
approved forms, with no subject filter. Reason: the operator's ruling of 2026-10-15
keeps the verbs as close to the standard as possible so the decoder relies on its base
training, and the correction of 2026-10-12 retired the split between aerospace and
general vocabulary. A verb the agent never emits costs the classifier nothing, since the
set is closed, the keys are generated without a model, and the binder's label set is
seven labels regardless of the key count.

**E2. The standard's category 2 example list enters wholesale.** Every example verb the
standard gives under rule 1.12 category 2 (computer processes and applications) becomes
a key with disposition `technical_use` and authority rule 1.12, category 2, with its
subcategory letter. Where the example is already an approved headword (CLOSE, CUT,
ERASE, INSTALL, OPEN, TAP) the approved key stands and no second key is made. Reason:
the standard itself selected these verbs for computing texts, so this is the election
that is most defensible on authority (each key cites the standard, though taking the
list is still an extraction from it) and that best serves the 2026-10-15 reason, the
decoder's base training. v0.1 applied the rule 1.12 preference as a dictionary
exclusion against members of this very list (delete, store, abort, boot, enable,
disable, validate, update, download, upload, debug) while admitting others from it
(save, load) on the same kind of argument. The preference is a writing rule, and
section 2 E5 puts it where the standard puts it.

**E3. A technical verb outside the category 2 list enters only with a recorded
reason.** Each one names its category, the approved construction it was tested against,
and why that construction does not give the same instruction. Reason: this is rule 1.12
as written for verbs the standard did not already name, and it is the 2026-10-15
decision that a verb is added only when no approved verb carries the meaning.

**E4. One key per headword.** By the same extraction, 29 approved verbs carry more
than one numbered approved meaning, among them OPEN, CLOSE, START, STOP, RECORD, SHOW,
CANCEL and DO. Each keeps one key. The store does not resolve which meaning a statement
used. That is a stated limit of the first release, and section 6 names it.

**E5. The rule 1.12 preference is taught, not checked, and the scheme says so.** The
prompt (PROMPT-3, first duty) teaches the model to prefer the approved verb and the
approved verb-plus-noun construction. The checker admits any key in the dictionary.
Spec 02 PARSER says an unsupported rule is failed admission, never a rule silently
omitted, so this cannot be left implicit: the checkable content of rule 1.12 is category
membership, and its preference is a meaning judgment ("accurately gives the
instruction") that no mechanical checker can make, as the standard itself shows by
listing delete beside erase. The scheme's rule coverage therefore declares, rule by
rule, checked or taught, with rule 1.12's preference among the taught (delta D7, whose
table is the walk). The conformance claim is then "STE100 under the declared
checked rules". Whether the decoder follows the preference is a measurement (section
7, light verbs), not a conformance failure.

**E6. Senses are cited for approved verbs and written for technical verbs.** The `sense`
field of an approved verb names its dictionary entry rather than copying the approved
meaning. A technical verb's sense is written here in our own words, or cites the
category 2 subcategory for the verbs entering under E2. Reason: the copyright notice
reserves reproduction and publication to ASD's written authority, and the special usage
rights list does not obviously include this program. See section 7.

Five approved keys have a computing sense that comes only from rule 1.12: CLOSE, CUT,
INSTALL, OPEN and TAP, whose dictionary meanings are physical or circuit meanings. They
cite both their entry and rule 1.12 category 2, and they join D4's count of keys with
an unresolved sense. ERASE is not among them: its approved meaning is to remove data
from a medium, so its computing sense is the approved one.

## 3. Auxiliary and modal verbs

Seven approved headwords do not name an action the store can file as one: CAN, CANNOT,
MUST and WILL are modal, DO in its second meaning is an auxiliary, and BE and HAVE state
a condition or a property more often than an act. The first five enter the dictionary
because the grammar must accept them, but they are proposed as never governing a
binding. The governing verb of "THE AGENT MUST KEEP THE LOG" is KEEP. BE and HAVE stay
as binding keys, because "THE FILE IS EMPTY" is a statement the store must hold. The
scheme has no field for this role, so the grammar under O02-1 carries it.

This choice loses information. "DO NOT START THE SERVER AGAIN" files as verb START with
noun SERVER, and nothing in `statement_binding` records the NOT. The span text keeps it,
so placed history reads correctly. A comparison by verb key does not, and a history
query that returns the opposite of what was said is worse than one that returns
nothing. This document therefore proposes delta D2 for ruling before the schema has
consumers, with polarity first and modality second. Section 6, delta D2.

## 4. Technical verbs proposed

### 4.1 Under E2: the standard's category 2 examples, 50 verbs, 40 new keys

| Subcategory | Example verbs in the standard | Already approved, no new key |
| --- | --- | --- |
| 2 a) input and output processes | click, digitize, enter, press, print, swipe, tap, type | TAP |
| 2 b) user interface and application processes | clear, close, copy, cut, delete, deselect, disable, drag, drag and drop, enable, encrypt, erase, filter, highlight, invalidate, maximize, minimize, navigate, open, paste, save, scroll, sort, store, tweet, validate, zoom in, zoom out | CLOSE, CUT, ERASE, OPEN |
| 2 c) system operations | abort, boot, communicate, debug, download, format, install, load, manage, process, reboot, update, upgrade, upload | INSTALL |

Each of the 40 enters with authority {rule_id: "1.12", disposition: technical_use} and a
sense naming its subcategory. Several have an unapproved entry in Part 2 that sends them
to an approved verb (load to INSTALL, save and store to KEEP, delete to ERASE, enter,
press, clear, copy, filter, minimize, communicate, process). Those entries govern the
general sense and rule 1.12 category 2 governs the computing sense, and the key carries
the computing sense only. The operator struck four entries with no use in this program
on 2026-10-28 (tweet, swipe, zoom in, zoom out), and a strike changes nothing
structurally.
The remaining GUI verbs (click, press, drag, drag and drop, highlight, maximize,
minimize, scroll, deselect, navigate) are kept, pending the operator's word at
ratification. Both seats lean to keeping them: a closed set costs nothing, keys are
generated without a model so an admitted key owes no fixture, they are the standard's
own examples, and each strike is a judgment line the record must defend. The four
strikes stay as the operator's exception, and E2 closes in one act at ratification.

### 4.2 Under E3: four verbs outside the list

| Key | Category | Sense, in our words | Approved route tested, and why it fails |
| --- | --- | --- | --- |
| compile | 2 c) system operations | To change source code into a form that a computer can run | The dictionary sends compile to MAKE A LIST or RECORD, a different sense |
| commit | 2 c) system operations | To record a set of changes as one durable revision in a version-control repository | RECORD THE CHANGES does not say that the changes become one named revision, and there is no Part 2 entry |
| merge | 2 c) system operations | To join two lines of revisions into one | No close route, and no Part 2 entry |
| recall | 2 c) system operations, program use | To get the recorded history of one subject from state | See below |

**recall** is the only recall key proposed. It must not be an approved verb. If FIND or
READ were a recall key, the ordinary work sentence "FIND THE ERROR IN THE LOG" would
become a history query. A dedicated key keeps the internal call separate from the work.
Spec 02 requires `recall_keys` to be a nonempty subset of `verbs` with one shared
noun-history action, and one key meets that. READ-3 asks for "a small number of recall
verbs", each an internal call whose action is already decided, and one is a small
number.

## 5. Contested and declined

**Resolved by E2.** sort, filter, download and upload were contested in v0.1 and are
members of the category 2 list, so they enter under E2 with no further argument.

**Declined: run.** Its Part 2 entry sends run to OPERATE, and START and DO carry the
rest. OPERATE THE TESTS, START THE SERVER and DO THE TESTS are the routes. v0.2 also
declined it for the VERB/NOUN collision with this program's noun "run" (RunId, the
standing run). That reason is withdrawn as a dictionary argument, because it applies
at least as hard to keys E2 admits (store, process, type, load, format, sort, update,
filter, enter, copy are all nouns the agent meets constantly). Collision is a binder
measurement under O02-2, and the collision words go into the negative corpus.

**Declined.** An approved verb gives the instruction, so rule 1.12 excludes the
technical verb. v0.1's entries that are members of the category 2 list have moved to
section 4.1.

| Not entered | Approved route |
| --- | --- |
| restart | START AGAIN (boot and reboot enter under E2) |
| verify, check | MAKE SURE (validate enters under E2) |
| test | DO A TEST |
| execute | DO |
| compute | CALCULATE |
| create, build | MAKE, ASSEMBLE |
| modify, edit, convert | CHANGE (update enters under E2) |
| list, log | RECORD, SHOW |
| search, query | FIND, EXAMINE |
| request | TELL, WRITE |
| insert | PUT |

**The program's own architecture verbs are not model verbs.** The terms ruled on
2026-10-13 for weaver-state's work fail the dictionary: retain goes to KEEP, place goes
to PUT, ingest goes to SWALLOW, preload goes to LOAD as a technical noun, serve is
unapproved, and report is not listed. Sort and store are now keys under E2 because the
standard lists them, and that changes nothing about the architecture terms: they stay
what they are, technical terms of the PRDs and the papers, and the record calling them
approved verbs should be corrected.

## 6. Spec deltas this exposes

None is drafted here. Each would be its own pull request under the rule that a design
change during review lands separately.

**D1. Lexicon-change request.** The ruling of 2026-10-17 has the model, at the repair
cap, log a marker with its intended verb as a request to the operator, and log near-miss
substitutions. Spec 02 records `repair_exhausted` but not the intended verb. Without
that record nothing measures whether this dictionary is large enough.

**D2. Polarity and modality on the binding.** `statement_binding` has no field for
negation or for a modal. Proposed columns for review: polarity (affirmative or negative)
and modality (none, must, can, cannot, will). Polarity is proposed for ruling now,
before the schema has consumers, for the reason in section 3. Modality may follow
measurement. The delta names where polarity comes from (DO NOT, CANNOT, MUST NOT and
the approved negators) and scopes it to the governing verb only, with the condition
left as text.

**D3. The dictionary inside the store.** On 2026-10-12 the dictionary was ruled a table
with foreign keys from filed statements. What merged ties the store to the artifact by
digest only, and `statement_binding.verb` is unconstrained text. A `scheme_verb` table
filled at seed would give the store its own refusal of an unknown key, and that refusal
can be tested by removing it.

**D4. Meaning per statement.** E4 leaves 29 multi-meaning keys unresolved, and the five
E2 overlaps in E6 add to them. The
2026-10-17 lesson from Caterpillar Technical English was to resolve the sense at write
time. That would need a sense column and a harder classifier job. The proposal is to
measure first.

**D5. CLASS-1's wording.** The PRD says "a closed set of computing verbs". Under E1 and
E2 the set is the standard's approved verbs plus the category 2 technical verbs plus the
E3 additions, closed at load. One sentence in CLASS-1 should say so, so that nobody
later reads "computing" as a filter that removes the approved verbs with no computing
use.

**D6. Technical names as an open lexical class.** Spec 02 SCHEME makes `technical_names`
a closed list of exact spellings and TEMPLATES makes SlotClass `technical_name` depend
on it. Section 7's first item asks for an election. If the election is the open class,
it is a Spec 02 delta to SCHEME, PARSER and TEMPLATES and must land before the compiler
is written, because it changes what the grammar accepts.

**D7. Rule coverage declared checked or taught.** Spec 02 SCHEME's `reference.rule_ids`
lists rules without saying which the checker enforces. The declaration is made rule by
rule over every rule of Part 1 (53: Issue 9 moved rule 2.3 into section 4 as 4.5), not
by naming a few. The drafting seat's `walk-ste100-rules-v0_1.md` is that table in
draft, with grades token, structure, partly, taught and n/a and a fixture seed per
checked rule. Each rule is checked, taught,
or checked for one class and taught for another, with the qualification record
carrying the table, so that PARSER's "an unsupported rule is failed admission" applies
to the checked set and the taught set is the prompt's and the review's. The largest
taught rule is 1.3, approved words only with their approved meanings: FOLLOW used to
mean obey passes any lexical check. The walk's grades are the table, and this document
does not enumerate them a second time. The meaning-protocol text (CLASS-1) states the
conformance claim as "under the declared checked rules".

**D8. Dispatch by verb and program noun, read side only.** Spec 02 RECALL reserves a
verb (`recall`, one noun, no condition) and TOOL-STATEMENT excludes "a recall key" from
tool statements. The operator's reading of 2026-10-28 is that the verb is one key and
the noun says where the action goes. On the write side that is already how filing
works and needs no dispatch (section 7, no write key), so D8 is a read-side delta only:
READ + STATE would become the inward history query and READ + LOG a shell read.
Dispatch by pair would make `recall_keys` a recall pair (verb, program noun), change
RECALL's one noun into a target noun and a subject, change the recall refusal shape
with it, and make the program's nouns (STATE, HISTORY, PLAN, STEP) a closed
technical-name list the grammar knows, which is the closed half of D6.

The cost is where control flow rests. With a reserved verb, whether a sentence is a
recall is a dictionary lookup with no model in the loop, which is the CLASS-2 property.
With a pair the decision rests on the noun, and if the binder assigns that noun a binder
error turns a work read into a history query or the reverse. The program nouns also
collide with the work's words: STATE is unapproved as a general noun (Part 2 sends it to
CONDITION), so in conforming text it can only be a technical noun, and this program's
own work says "READ THE STATE FILE" constantly, and STEP is an approved general noun
("DO STEPS 13 THRU 16"). The pair is safe only when the grammar matches the program
nouns lexically and marks them apart from the work's identifiers, which is D6's marking
convention. What it buys is further inward targets later (PLAN, STEP).

Agreed with the operator in session on 2026-10-28 and proposed here for ratification:
the pair is the shape, it lands with D6, and `recall` stays the one reserved verb until
the compiler exists. No repository document records this yet, so it is a ruling only
when this document is ratified.

**D9. The flag, a mark on a write.** Agreed with the operator in session on 2026-10-28
and proposed here for ratification: "WRITE THIS TO STATE" is a mark on a write, not a
write. The statement files like any other, and a
flag on the filed row is read by the serve-side sort as a ranking input inside the
injection budget. It lands as a Spec 08 (sort and selection) delta and is the sibling
of Spec 07's forward flag: one mechanism, keyed to a step or to a subject. Three
cautions travel with it. Reach, never the guarantee: the harness floor under READ-2
stays, and a flag only raises a row within it. Flag inflation is measured, by flag rate
per turn and by use at equal rank, before the flag earns weight. No second write path:
the partition still admits, and promotion stays the operator's under the ruling of
2026-10-17.

## 7. Open items

**Technical names.** The scheme's `technical_names` is a closed list of exact spellings.
A code agent meets new file paths, identifiers and crate names on every task. If the
grammar admits only listed names, each new identifier is a conformance failure, and the
repair rate measures the list rather than the model. O02-1 must elect either a closed
list or an open lexical class with a marking convention, such as code spans, and that
election belongs in the grammar. Lean: an open class for identifiers and paths, and a
closed list for the program's own nouns. This is the largest decision in this document
and it is a grammar decision. The compiler act waits on it (D6).

**No write key.** #1's `/remember` predates the PRD. Under STORE-2 and CLASS-2/3 every
accepted statement is filed by its verb and nouns, from the model and from the gate,
so ordinary filing is the write and no verb needs reserving for it. WRITE is one key and
the noun says where the write goes: an external write (a file, a database, an index) is
a tool request whose statement, under TOOL-STATEMENT, supplies the verb and noun (WRITE
THE REPORT TO THE FILE, PUT THE RECORD IN THE DATABASE, UPLOAD THE CHUNKS TO THE INDEX)
while the ToolCall supplies the executable, and the pair is filed under that verb and
noun with its return. No verb is a write trigger, KEEP included: "KEEP THE OLD CONFIG"
is ordinary work talk, and a trigger on it would recreate the collision that made
`recall` a reserved word. Retention and promotion belong to the operator through the
partition (ruling of 2026-10-17), not to the model. "Remember this", meaning more than
filing, is the flag of D9: a mark on the filed row that the serve-side sort reads, not
a verb and not a write. If #1's `/remember` is an operator command it is not a model
verb at all. #1 should be reconciled with the PRD: re-scoped to the read side or closed
against it.

**Clause and verb objects.** Some approved verbs take a clause or another verb as their
object: MAKE SURE takes a THAT-clause ("MAKE SURE THAT THE FILE IS EMPTY") and is now
the route for verify, check and test. LET is causative ("LET THE MATERIAL CURE"). TRY,
CONTINUE, START and STOP take another verb. Spec 02 MODEL gives each action one
governing verb span and one or more object spans, so the grammar under O02-1 must say
what the object is when it is a clause. Lean: the embedded clause yields its own
Binding and the matrix binding takes the embedded binding's nouns, so "MAKE SURE THAT
YOU REMOVE THE FILE" gives (REMOVE, FILE) and (MAKE SURE, FILE), never (MAKE SURE,
YOU), and the noun index holds FILE under both. For verb-plus-verb the inner verb
governs the noun and the outer binding takes the same nouns. MAKE SURE is the
verification verb, so it is tested first, not last.

**The mode of an emission.** Sections 5 and 6 of the standard split procedural writing
(instructions, imperative, 20 words) from descriptive writing (information, no
imperative, 25 words), and the agent emits both. The checker cannot tell the mode
without a marker. This is an O02-1 grammar election, raised in the walk's section 1,
with three options: one 20-word limit for everything with the imperative permitted but
not required, mode inferred from the verb form, or a declared mode on the wire. Lean:
the first, which needs no wire field and is what Spec 02 MODEL's one-governing-verb
assumption wants anyway. For the operator, and the ratification split carries it.

**Light verbs.** Rule 1.12's preference, an approved verb with a noun, moves the act
into the noun: "DO A TEST", "MAKE A COPY", "PUT THE ITEMS IN AN ORDER". Keying by verb
then concentrates on DO, MAKE, PUT and GET. Retrieval enters by noun under the noun
index, so this costs less than it seems, but it weakens the verb as a label.
Measurement: the share of bindings whose key is one of these light verbs, over the
first long sessions with Claude Code as the sender. Under E2 the technical verbs are
already admitted, so a high share is evidence about the prompt's teaching of E5, not an
argument for widening the dictionary.

**The record of the cited rulings.** The rulings of 2026-10-12 (dictionary as a store
table, aerospace split retired), 2026-10-15 (verbs close to the standard, additions only
when no approved verb carries the meaning) and 2026-10-17 (the lexicon-change marker,
the Caterpillar lesson) are cited here from the sessions and appear in no repository
document. Only the 2026-10-13 sort-twice node refers to "the store ruling of
2026-10-15". A ratified election should carry them into the record, or quote them, so
its authority chain does not dangle. v0.1 also cited "A15" for the verb-key
comparison. It is named in the decision sheet of 2026-10-16, which no repository holds,
so no repository document names A15, and this document refers to the noun index
instead.

**Rights.** O02-1 names "lexical rights". The production artifact lives in admin's
config root, not in the public repository, and only a one-verb fixture is committed
today. Nothing in this document or the artifact should be committed to WeaverAgent until
ASD's written authority or a clear exemption is in hand. Appendix A is itself an extract
of Part 2 and the artifact's `verbs` array will be the same list, so this public copy
carries sections 0 to 8 and not the appendices, which stay with the operator's full copy
beside the standard. The contact named in the
standard is info@asd-europe.org, and the request is a long-lead item worth sending now.

## 8. Ratification, split

Firm now, independent of every delta, and enough to start the checked extraction of
the forms that O02-1 owes either way:

- E1 to E4, the approved verbs and the category 2 list as keys.
- E6, with the five dual-sense keys.
- The declined list of section 5, run included.
- The operator's four strikes.

Waiting on a delta:

- E5 waits on D7, the rule-coverage table.
- The form of the recall key waits on D8.
- The technical-names policy waits on D6.

The mode election (section 7) is ruled with D6 and D8. All three land before the
compiler is written. The remaining GUI verbs are the operator's word either way and
block nothing.

The order of work the ratification feeds, agreed with the operator in session on
2026-10-28 and proposed here for ratification:

1. The walk table, every rule of Part 1 graded, is the D7 artifact.
2. The fixture pairs, one conforming and one violating sentence per checked rule, with
   the rule id in the `non_ste100` detail. They are the negative corpus O02-1 owes, and
   they are STE100 sentences of ours, so they carry no rights problem.
3. The compiler, written against those pairs.

The checked extraction of the forms (appendix A) runs beside step 1 and blocks nothing.
