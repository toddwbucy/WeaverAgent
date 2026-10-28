# Walk: the 53 STE100 rules, graded for the checker

Status: walk v0.1, 2026-10-28, for the operator's review. From the drafting seat. Serves
delta D7 of the verb dictionary candidate (v0.3): every rule in Spec 02 SCHEME's
`reference.rule_ids` declared checked or taught. Source: ASD-STE100 Issue 9, Part 1,
sections 1 to 9, 53 rules, from the operator's copy. The rule wordings below are
paraphrased in our words, not quoted. The grades are proposals. Overturn any of them in
the margin.

## 0. The grades

- **token**: checkable with no parse, by lookup against the dictionary or by counting.
  A regex or a table walk.
- **structure**: checkable only once the binder has bound the sentence. The fixture
  tests the binder and the checker together, so it is also a binder regression test.
- **taught**: turns on meaning or judgment. No mechanical check reaches it. It goes in
  the prompt and is measured by a scored sample under O02-2.
- **n/a**: concerns a text form the agent does not emit.
- **partly**: one half mechanical, one half not. The table says which half is which.

Every rule graded token or structure owes a fixture pair before the compiler exists: one
conforming sentence that admits, one violating sentence that fails under that rule's
identifier. A checked rule with no fixture pair is not checked. If a fixture cannot be
written, that is the tell that the rule belongs in the taught column.

## 1. A decision before the walk: which mode is an emission

Sections 5 and 6 split the standard into procedural writing (instructions, imperative,
20 words) and descriptive writing (information, no imperative, 25 words). The agent
emits both: a tool request is an instruction, a note to its future self is a
description. The checker cannot tell which mode a sentence is in without a marker. Three
options, for the operator:

1. One limit for all emissions, 20 words, the stricter one. Imperative is permitted but
   not required. Simplest, and it loses nothing the binder needs.
2. The grammar infers mode from the verb form: imperative means procedural, otherwise
   descriptive. Then 5.3 and 6.3 become structure rules that follow from the inference.
3. The emission declares its mode. A new field on the wire, and a Spec 02 delta.

Lean: option 1 for the first release. The 20-word limit is the one MODEL's assumption
(one governing verb per span) wants anyway.

## 2. The walk

### Section 1, words, 14 rules

| Rule | In short | Grade | Why, and the fixture seed |
| --- | --- | --- | --- |
| 1.1 | Use only approved words, technical nouns, and technical verbs | partly | Approved words and the closed lists are token. Whether an unlisted word is a technical noun is the D6 question. Seed: UTILIZE THE TOOL fails, USE THE TOOL admits |
| 1.2 | Use an approved word only as its listed part of speech | structure | Needs the parse to know what part of speech a word is serving as. Seed: TEST THE FILE fails (TEST is a noun), DO A TEST OF THE FILE admits |
| 1.3 | Use an approved word only in its approved meaning | taught | FOLLOW used to mean obey passes every lexical check. The headline taught rule |
| 1.4 | Use only the approved forms of verbs and adjectives | token | The forms are listed in the entry. Seed: REMOVING THE FILE fails, REMOVE THE FILE admits |
| 1.5 | A word may be used if it fits a technical noun category | taught | Category membership is a judgment. Checkable only for the closed program nouns under D6 |
| 1.6 | An unapproved word is allowed only as a technical noun or part of one | partly | Same split as 1.1. The marking convention of D6 decides what the checker can see |
| 1.7 | Do not use a technical noun as a verb | structure | Needs the parse to see the word in verb position. Seed: CLAMP THE CABLE fails, PUT A CLAMP ON THE CABLE admits |
| 1.8 | Use technical nouns approved in your company or field | taught | Only the closed program nouns are checkable. Everything else is judgment |
| 1.9 | Choose a technical noun that is short and easy to understand | taught | Judgment |
| 1.10 | No regional, slang, or jargon words as technical nouns | taught | Judgment |
| 1.11 | One technical noun per item, never two names for one thing | partly | Checkable within one block for the closed program nouns (two spellings fail). Across blocks and for the open class it is the program's core discipline and is taught |
| 1.12 | A verb may be used if it fits a technical verb category | partly | Category membership for the listed keys is token (the dictionary carries them). The preference for an approved verb is a meaning judgment and is taught. This is D7's origin |
| 1.13 | Do not use a technical verb as a noun | structure | Mirror of 1.7. Seed: DO A COMPILE fails, COMPILE THE CRATE admits |
| 1.14 | American English spelling | token | A spelling table. Seed: INITIALISE fails. Low value for an emission protocol, but free |

### Section 2, noun phrases, 2 rules

| Rule | In short | Grade | Why, and the fixture seed |
| --- | --- | --- | --- |
| 2.1 | Multi-word nouns of no more than three words | structure | Count adjacent nouns in a bound noun span. Seed: a four-word noun cluster fails |
| 2.2 | A technical noun of more than three words is written in full, then shortened or hyphenated | partly | The "in full" half is checkable against the closed list. The shortening is a writing practice and is taught. Candidate for n/a if no program noun exceeds three words |

### Section 3, verbs, 7 rules

| Rule | In short | Grade | Why, and the fixture seed |
| --- | --- | --- | --- |
| 3.1 | Use only the verb forms given in the dictionary | token | Same table as 1.4. One fixture pair serves both. Consider citing 1.4 and 3.1 together |
| 3.2 | Use only infinitive, imperative, simple present, simple past, simple future, and past participle as adjective | structure | Needs the parse to identify tense. Seed: THE FILE IS BEING WRITTEN fails, THE FILE IS WRITTEN admits |
| 3.3 | Use the past participle only as an adjective | structure | Needs the parse. Seed: THE SERVER HAS STARTED fails, THE SERVER STARTED admits |
| 3.4 | No auxiliary verbs in complex constructions | structure | Needs the parse. Seed: THE AGENT HAS BEEN WAITING fails. Overlaps 3.2 |
| 3.5 | The -ing form only as a technical noun or its modifier | partly | The form is token. Whether an -ing word is a technical noun is D6. Seed: STOP WRITING THE FILE fails |
| 3.6 | Active voice, passive only in description when the agent is unknown | structure | Passive is findable in the parse. The exception needs mode (section 1). Seed: THE FILE WAS WRITTEN BY THE AGENT fails, THE AGENT WROTE THE FILE admits |
| 3.7 | Describe an action with a verb, not a noun or other part of speech | taught | Nominalization is a meaning judgment. Seed only as a scored sample |

### Section 4, sentences, 5 rules

| Rule | In short | Grade | Why, and the fixture seed |
| --- | --- | --- | --- |
| 4.1 | Write short and clear sentences | taught | Short is 5.1 and 6.3. Clear is judgment. This rule carries nothing of its own to check |
| 4.2 | Do not omit words or use contractions | partly | Contractions are token (DON'T fails). Omitted words such as a missing THAT are structure at best and taught in practice |
| 4.3 | Use a vertical list for complex texts | n/a | The emission is a block of sentences, not a page layout. Revisit if emissions carry lists |
| 4.4 | Use connecting words and phrases between related sentences | taught | Judgment |
| 4.5 | Use an article or a demonstrative before a noun where applicable | structure | Needs the noun span. "Where applicable" makes a full check impossible, but a bare countable noun is findable. Seed: WRITE FILE fails, WRITE THE FILE admits. Proper nouns and identifiers must be exempt, which is D6 again |

### Section 5, procedural writing, 5 rules

| Rule | In short | Grade | Why, and the fixture seed |
| --- | --- | --- | --- |
| 5.1 | Maximum 20 words per sentence | token | Count, with section 8's counting rules. Seed: a 21-word sentence fails |
| 5.2 | One instruction per sentence unless the actions are simultaneous | structure | One governing verb span per sentence. This is MODEL's assumption written as a rule. Seed: REMOVE THE FILE AND START THE SERVER fails as two instructions |
| 5.3 | Instructions in the imperative | structure | Depends on mode, section 1. Under option 1 it becomes permitted, not required |
| 5.4 | A condition the reader needs first goes before the command, divided by a comma | structure | This is the condition span of the binding. Seed: START THE SERVER IF THE PORT IS FREE fails, IF THE PORT IS FREE, START THE SERVER admits. High value: it fixes where the binder looks for the condition |
| 5.5 | Notes give information, not instructions | n/a | Notes are a document form. Revisit if the emission grows a note channel |

### Section 6, descriptive writing, 6 rules

| Rule | In short | Grade | Why, and the fixture seed |
| --- | --- | --- | --- |
| 6.1 | Give information gradually, one subject per sentence | taught | Judgment, though "one subject" is partly the same as 5.2 |
| 6.2 | Use key words and phrases to give the text structure | taught | Judgment |
| 6.3 | Maximum 25 words per sentence | token | Same count as 5.1. Under section 1 option 1 this rule is superseded by 5.1 |
| 6.4 | Paragraphs show related information | n/a | A block is not a paragraph. Revisit if blocks are long |
| 6.5 | One topic per paragraph | n/a | Same |
| 6.6 | No more than six sentences per paragraph | token if kept | Trivial count per block. Lean n/a unless a block limit is wanted, and then it is a deployment value, not this rule |

### Section 7, safety instructions, 3 rules

| Rule | In short | Grade | Why, and the fixture seed |
| --- | --- | --- | --- |
| 7.1 | Name the risk level with a word such as WARNING or CAUTION | n/a | The agent writes no safety instructions. If hard constraints ("DO NOT RESTART THIS SERVER") are ever marked, this is the rule to borrow, as a mark, not a check |
| 7.2 | Start a safety instruction with a clear command or condition | n/a | Same |
| 7.3 | Give the reason that shows the risk | n/a | Same |

### Section 8, punctuation and word count, 7 rules

| Rule | In short | Grade | Why, and the fixture seed |
| --- | --- | --- | --- |
| 8.1 | All standard punctuation except the semicolon | token | Seed: a semicolon fails. Already the program's own rule |
| 8.2 | Hyphens connect directly related words | taught | Judgment, and the hyphen also carries the D6 identifier question |
| 8.3 | Parentheses only for the listed uses | taught | The listed uses are judgments. Low value |
| 8.4 | A colon in a vertical list ends a sentence for counting | n/a | Vertical lists are n/a under 4.3 |
| 8.5 | Text in parentheses counts as one word | token | Part of the 5.1 counter. Not a rule of its own to fail, a rule the counter obeys |
| 8.6 | Numbers, units, abbreviations, identifiers, quoted text, titles, proper nouns each count as one word | token | Part of the 5.1 counter. Identifiers counting as one word is exactly D6's marking convention doing double duty |
| 8.7 | Hyphenated words count as one word | token | Part of the 5.1 counter |

### Section 9, writing practices, 4 rules

| Rule | In short | Grade | Why, and the fixture seed |
| --- | --- | --- | --- |
| 9.1 | Rewrite the sentence when a word-for-word replacement does not keep the meaning | taught | The repair path's rule. It is what the model does under CLASS-9, and nothing checks it |
| 9.2 | Use each approved word correctly | taught | Restates 1.3 |
| 9.3 | Do not make phrasal verbs from two words | structure | A verb followed by a particle not in its forms is findable. Seed: SET UP THE SERVER fails, PREPARE THE SERVER admits. Note that COME ON, GO OFF, PUT ON and MAKE SURE are approved multi-word keys, so the fixture must not trip on them |
| 9.4 | Consistent terminology and wording | taught | The cross-block half of 1.11 |

## 3. The tally, as proposed

- token: 1.4, 1.14, 3.1, 5.1, 6.3, 8.1, 8.5, 8.6, 8.7, and 6.6 if kept. Ten.
- structure: 1.2, 1.7, 1.13, 2.1, 3.2, 3.3, 3.4, 3.6, 4.5, 5.2, 5.3, 5.4, 9.3. Thirteen.
- partly: 1.1, 1.6, 1.11, 1.12, 2.2, 3.5, 4.2. Seven, each with its checked half named.
- taught: 1.3, 1.5, 1.8, 1.9, 1.10, 3.7, 4.1, 4.4, 6.1, 6.2, 8.2, 8.3, 9.1, 9.2, 9.4.
  Fifteen.
- n/a: 4.3, 5.5, 6.4, 6.5, 7.1, 7.2, 7.3, 8.4. Eight.

Fifty-three. The checked set, token plus structure plus the checked halves, is thirty
rules and owes thirty fixture pairs. Five of them (1.1, 1.6, 1.11, 3.5, 4.5) wait on D6
for their technical-name half and get their fixtures last.

## 4. What the walk exposes

**The binder does most of the checking.** Thirteen rules are structure grade. That means
the checker is not a regex pass in front of the model. It is the binder's parse read
back against the rules. The fixtures for those thirteen are binder regression tests, and
O02-2's model selection should run them.

**Rule 5.4 is the condition grammar.** It says where a condition goes and how it is
divided from the command. Spec 02 BINDING gives the condition span to the scheme's
condition grammar under O02-1. That grammar is rule 5.4 plus the step-reference form.
It should cite 5.4 by number.

**Rule 8.6 and D6 are one decision.** The standard already treats identifiers,
abbreviations, numbers and quoted text as single tokens for counting. The marking
convention D6 needs for identifiers is the same token class. One election serves both.

**Three rules carry nothing to check.** 4.1, 9.2 and 6.1 restate other rules or state a
goal. They go in the table as taught so the coverage is complete, with a note that their
substance is checked elsewhere.
