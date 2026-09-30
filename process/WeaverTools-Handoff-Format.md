# WeaverTools Handoff Format

**Version:** v0.4, 2026-07-31. Companion to the Working Process. Project documents
carry a version and a date and no state, per Working Process section 2.
**Parent:** WeaverTools Working Process

The Working Process says who is primary and in what order the work moves. This says
what shape work takes when it leaves one seat for the other, and what the prompt
accompanying it must carry.

Two kinds of work move between seats. A batch carries edits to documents that already
exist, and sections 2 through 5 govern it. A commission asks the other seat to produce a
document that does not exist yet, and section 6 governs that. They share the prose
prompt and manifest form and differ in what the manifest can name, since a commission
has no edits to account for and no prior state to diff against.

## 1. Why this exists

Working Process section 1 defines the shape advice comes back in: facts found, advice
offered, and requests to reopen a closed decision. It defines no shape for work going
out. The outbound side has been improvised, and improvising it produced two failures
worth naming.

Sending the whole corpus at once lets an edit land in a document the batch was not
about, where it goes unnoticed until a later sweep. Sending documents without a stated
baseline makes the receiving seat diff against a remembered previous version, which is
the same staleness the two-seat split exists to prevent.

## 2. Transport and baseline

The transport is a shared working tree. The receiving seat reads files rather than a
diff, so what a batch consists of is the state of that tree at the moment it is sent.
Reading files makes content checkable and makes change invisible, which is why the
baseline is stated rather than inferred.

Every batch names the commit it departs from. Two branches cut from the same commit
will edit the same documents, and without the base the receiving seat cannot tell which
prior state a file is a change against. The base is the one mechanical fact in a batch
that cannot be reconstructed from the documents themselves.

## 3. Branch scope

One branch per batch.

**A batch's extent is the act's extent, not the crate's directory.** A change to a
declared name or a declared node reaches every document that cites it, and those
documents are part of the act rather than visitors to it. Renaming a contract and
leaving its citations for a later pass ships a corpus that spells one name two ways,
which is the failure the rename was performed to end. The same holds for a note one
pass owes another crate's stub: work the current pass generates is the current pass's
work wherever it lands.

What does not belong is a defect in another document that this pass did not create and
does not reach. That is orthogonal work and goes to its own branch.

A batch is sent when the branch has something checkable, not when the branch is done.
An incomplete batch is legitimate and says so.

## 4. The prompt for a batch

Two to three paragraphs of prose, followed by a manifest. The prose carries the
argument and the manifest carries the accounting. Neither substitutes for the other.

Paragraph one names the branch, the seat sending, the base commit, and the state of the
working tree. It states in one sentence what this batch is about, at the level of the
claim being made rather than the files being changed.

Paragraph two names what changed and why, document by document, as assertions rather
than as a diff. The receiving seat can read the files. What it cannot read is which
ruling produced the edit and what the edit is meant to make true.

Paragraph three names the open items this batch closes and the items left open on
purpose. Items are named by substance and never by number, because the working list
renumbers as it shrinks. Where the working list is organized by branch, an item's
destination is already recorded there and this paragraph does not restate it per line.

The manifest that follows is a true enumeration and takes list form:

    Base <commit>
    Documents in this batch
      <document>  <what the edit asserts>
    Reached by this act from outside the crate in hand
      <document>  <what reached it>
    Not reviewable in this batch
      <document and section>  <why, and what is superseding it>
    Gates run and their result
      <gate>  <pass, or the defect filed and whether it is fixed>
    Asked of the receiving seat
      <the review scope, and the merge question if there is one>

## 5. What a batch may not do

It may not carry two branches.

It may not close an item silently. An item that leaves the working list without a named
home is the unlanded ruling G7 exists to catch.

**Neither seat sends the other seat's edits already made.** The receiving seat's job is
to check and to advise, and a batch that arrives with its conclusions applied has
consumed the check rather than requested it. This binds whichever seat is sending, so it
holds unchanged when the seats trade places at phase three. Working Process section 1
states the same rule for returns, which is the case of it visible from the other side.

## 6. Commissioning a rough draft

Working Process phase one step 1 has the implementation seat cut a rough draft from the
old code, because that is where the raw material lives and because that seat holds the
old tree, the merged corpus, and the graph at once. Asking for that draft is a handoff
and takes a prompt, and almost nothing in sections 2 through 5 applies to it. There are
no edits to account for, no base commit to diff a file against, and the return is a
whole document rather than a change to one.

What a commission names instead is what the draft is drafted against. The crate. The
workflows in scope, since chartering proceeds workflow by workflow and a crate is not
drafted whole. The merged documents the draft may not contradict. The stub, if one
exists.

**The stub clause is required and is stated in every commission.** Nothing in a stub is
authoritative. It is context, accumulated by earlier passes that ran through a crate
before it was chartered, and it decides nothing. Where the stub and the old tree
conflict the stub wins, per Working Process section 2. Where the stub applies, the
drafting seat has a free hand to take a note, reshape it, or leave it. The draft is
written fresh at the stub's name and the stub is deleted in the same act, a tracked
deletion per Working Process section 2, so a note that did not survive is recoverable
from the commit that cut the draft rather than from a file beside it.

The clause is written out rather than cited. A commission is read by a seat starting
fresh on a crate, and a pointer to a rule about how much weight to give the material in
front of it is the one place a pointer costs more than it saves.

The manifest for a commission:

    Crate <name>
    Workflows in scope
      <workflow>  <what it has to reach end to end>
    Drafted against
      <document>  <merged, or stub, or old tree>
    Out of scope for this draft
      <what, and why it waits>
    Returned as
      DRAFT, carrying no decisions, per Working Process section 2

A commission returns a draft and never a decision. The draft is authored in the
architecture seat at step 2, which is where staleness is caught, so a commission that
comes back with the crate settled has performed step 2 in the wrong seat.

## 7. What this document does not do

It does not say what shape a document takes, which is the Document Format's. It does not
say what a batch must contain to be complete, because completeness is a property of the
branch and is ruled by the human at merge. It does not say what a commissioned draft
must contain either, for the same reason and one further one: a draft that knew what it
had to contain would not need drafting.
