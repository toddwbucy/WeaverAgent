# WeaverTools Working Rules

**Version:** v0.5, 2026-09-15. Companion to the Working Process, the Document Format,
and the Handoff Format. Project documents carry a version and a date and no state, per
Working Process section 2.
**Parent:** WeaverTools Working Process

The apex says what we are building. The Working Process says how the work moves. The
Document Format says what shape a document takes. The Handoff Format says what shape a
batch takes when it moves between seats. This says how we write.

## 1. Editorial rules, hard

- ASCII only. No em-dashes, no semicolons.
- Never the words genuinely, honestly, or actually.
- Prose lines wrap at 88 characters. Tables and fenced blocks are exempt, because
  neither wraps without changing meaning.
- Dense prose over bullet lists. Use a list only when the content is a true
  enumeration.
- No filler openers. Open on the substance.
- A header carries no history, and nothing accumulates there. The fields are
  drawn from a named set - Status, Version, Date filed, Document ID, Parent,
  Editorial, Landing PR - which is what the kinds below elect from rather than
  the whole vocabulary: a document naming a further field, as four charters
  name their companion contract and `weaver-admin-PRD` names its external
  boundaries, is electing and not accumulating. Each document kind carries the
  ones it uses: a process document carries Version and Parent and no state,
  per Working Process section 2, a corpus document carries Status with the
  identification fields, the apex and an experiment's charter carry no Parent
  because they have none, and a workflow document's version and date ride its
  Status line. The rule binds what accumulates, not which fields a kind elects.
  The change narrative lives in the commit that makes the change, and git is the
  archive, which is the stub ruling of 2026-07-31 applied to headers: history is
  the archive and the tree is not. Attributions inside the body stay, because a
  sentence marking a claim as a ruling is content rather than history. Per the
  operator's ruling of 2026-08-04, which swept the standing stacks, and the
  ruling of 2026-09-15, which swept the stacks that grew back.
- Landing PR names the pull request that last changed what the document says.
  One line, one number, and an act overwrites what it finds there. It carries no
  list, no earlier entry, no ordinal, and no date beside it, because a field that
  can grow a second line is the stack the rule above it retired wearing a shorter
  name. A number and never a commit sha: a number is assigned when the pull
  request opens and never changes, so it is written on the branch and is correct
  at the moment it lands, where a sha written into a file is stale as it is
  committed. A pass that changes no document's content does not move the field,
  so a mechanical sweep over every header leaves each number naming the act that
  last said what its document says rather than writing its own into all of them.
  Where the number cannot be recovered the field is absent rather than guessed. A
  process document does not elect the field, carrying Version and Parent alone.
  Per the operator's ruling of 2026-09-15.
- Avoid visual collisions, repeated adjacent tokens or abbreviations that run
  together when read. The reader is dyslexic, so "HAH. HAH" reads as "HAHA" and is
  rewritten. Two identifiers differing by one character are the same defect in a
  graph block.
- Self-citation format is (Bucy, 2026). Take your own work seriously and others
  tend to follow.

## 2. Working discipline

Adversarial by default. The work wants falsification-seeking critique, not
validation. When given a correction, grant it cleanly, then test whether it
dissolves the critique or only relocates it.

Read before assuming. When a repo, document, or prior decision is referenced, read
it. Do not answer from memory when the source is reachable.

Document-first, batch-edit. On load-bearing changes, state the rationale before
drafting. Draft candidates and let the human ratify. Hold edits and make one batch
rather than a stream of spot changes.

Decide from measurement. Defer architecture decisions until there is a measurement
to decide from. If a claim cannot be stated before measuring, that is the bias tell.

IS and IS-NOT is an authoring tool here, used to draw sharp boundaries in prose, and
it is welcome. What is not welcome is knowledge-graph governance shipped as a
product feature: axioms, smells, conformance scoring, and the rest of the previous
tree's apparatus. The framing device is fine and the product stays out.

This does not reach the gates and phases this project runs on itself. Those govern how
documents come to exist and are checked by looking, which is the opposite of the thing
being excluded. The rule is about what WeaverTools ships, not about how WeaverTools is
built.

## 3. Scope guardrails

These exist because this project was split off to escape stale context. Hold them.

This project is the OS-primitives program and nothing wider. Decisions from the
earlier full-application work do not carry forward unless re-ratified here.

When something feels in scope, check it against the apex scope criteria before acting.
Document processing, orchestration, and application concerns are out.

Build primitives, not the application. Do not chase parity with what already exists.
Each primitive is bounded by what it alone must do.

Keep the boundary between a primitive's mechanic and a consumer's reasoning. If a
proposed feature carries a motive, that motive probably belongs to a consumer, not
the primitive.

## 4. Vocabulary precision

Hold these distinctions as hard, because collapsing them is how scope drifts.

A primitive is not an application. The application is what you get by composing
primitives.

OS-level is not network-first. OS-level inherits the operating system's trust model.
Network-first assumes the network and adds auth above it.

Structural composition is not orchestration. A primitive reaching one level down to
do its own job is structural. A consumer pulling several together toward a purpose
is orchestration.

A mechanic is not its motive. The primitive does the doing. The consumer holds the
why.

Contract-coupled is not implementation-coupled. Primitives fit by a shared wire
contract, never by depending on a specific neighbor crate.

## 5. Tone

Concise. Warm and direct. Push back constructively, with the work's interest in mind
rather than deference.
