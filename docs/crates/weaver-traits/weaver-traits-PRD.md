# weaver-traits - PRD (crate charter)

**Status:** MERGED. In `main` and the source of truth. The crate PRD set is
written together and merged together, and no Spec is written against any member
before the whole set is merged. **Ratification became per-charter on 2026-08-23**, per
Working
Process section 2 as amended, so this document is ratified on its own terms rather
than waiting on the set. The set-wide act of 2026-08-04 established the pattern it
conforms to.

**Date filed:** 2026-07-29
**Document ID:** `weaver-traits-PRD`
**Parent:** `weaver-agents-PRD`
**Editorial:** Per the Working Rules.
**Landing PR:** #298

---

## 1. What this crate is

`weaver-traits` holds **the contracts the engine is written against**. It depends on
nothing internal, binds no socket, performs no work, and holds no cognition. It is
definitions only, and a thing that does work is in the wrong crate.

**It is a requirements document, not a catalogue.** Under the apex vocabulary-clause
rule every contract in this program carries a clause naming the vocabulary it depends
on, and a contract without that clause is not valid. This charter is therefore not a
listing of what happens to be inside a crate. It states the terms any crate must satisfy
to speak to the harness, and those terms are enforced at every seam rather than admired
in one place.

## 2. Why the floor exists here rather than inside the harness

Every trait in this charter is implemented or consumed by the harness today, so the
obvious question is why they are not harness-internal.

The answer comes from `weaver-types`, not from any crate not yet chartered. The agent
config file names a permission mode and a tool set. If those definitions lived in the
harness, then `weaver-types` would depend on `weaver-harness`, and the floor would
sit above the thing it is meant to be beneath. A neutral crate below both breaks that
inversion.

That is a complete argument from the two crates that exist. It does not appeal to
what the SPU or the gate might turn out to need, because those are demands their own
passes will make and this charter is derived from demand rather than from
anticipation.

## 3. What it holds

Everything here is present because the harness demonstrably needs it. The set grows
when a later crate demands more, and it grows by the ritual: a crate PRD added or
changed names the floor edit it needs, which lands with it or is named as owed.

```graph
node: weaver-traits
kind: crate

node: tool-trait
kind: vocabulary

node: provider-trait
kind: vocabulary

node: message-model
kind: vocabulary

node: permission-mode-vocabulary
kind: vocabulary

edge: parent
from: weaver-traits
to: WeaverTools

edge: defines
from: weaver-traits
to: tool-trait

edge: defines
from: weaver-traits
to: provider-trait

edge: defines
from: weaver-traits
to: message-model

edge: defines
from: weaver-traits
to: permission-mode-vocabulary
```

Identifiers are kebab-case per the Document Format, so the `Tool` and `Provider`
traits are `tool-trait` and `provider-trait` here. **`permission-mode-vocabulary` is
drawn by no contract and the reason is the `elects` edge**: its consumer is the
config's `permission-mode` field, the floor's own argument in edge form per
`weaver-types-PRD` section 2.1, so the definition is internal to that argument rather
than seam vocabulary a contract could draw. The mode vocabulary is
`permission-mode-vocabulary` rather than `permission-modes`, because the agent state
file holds a `permission-mode` field electing one of them and two identifiers a
character apart are a collision in the graph and on the page both.

This crate declares no seam and no floor link. It defines and performs nothing, so
it signs nothing, and its governance runs entirely through the `draws` edges that
contracts declare against these four.

### 3.1 The tool contract

`Tool` is the interface a registered outward service implements, and a tool
must declare what the model may call it with. An earlier form of this
sentence had every tool implementing it and the harness's engine dispatching
tools it does not know the identity of, and the tool boundary ruling of
2026-08-18 narrowed both halves: the gate's one held tool is the shell, its
own verb dispatched with no table, the inward callables of `weaver-internal`
are reached by their own surface, and what remains for object dispatch is
the elected outward corner `weaver-traits-Spec` section 5 always named.

The vocabulary its signature names comes with it: the schema a tool advertises, the
input it accepts, the output and outcome it produces, and the error it returns. These
are pure data the contract requires, with no dependency on anything that does work.

**This definition opened with the tool workflow on 2026-08-17, and the block's
own terms are what opened it.** The block held because tool dispatch was
harness-internal and no seam crossed it, so no vocabulary clause could draw
`tool-trait`. The ratified loop boundary of 2026-08-11 inverted that ground:
every tool sits outside the reasoning loop, dispatch crosses the gate seam,
and `weaver-harness-gate-contract` section 7 draws the definition - the
clause that could not exist while no seam crossed is the clause that
demanded it. The definition is now shaped against its consumers, the gate's
executor and the harness's dispatch, which is what the anticipatory-contract
rule always required and the workflow now supplies. The block's history
stands as stated: recorded on the working list from 2026-07-29, landed here
2026-08-01, held until the workflow demanded the shape.

**What this contract deliberately does not carry is a safety classification.** The
previous tree made `Tool::invocation_properties` the crate's flagship enforcement type,
a per-invocation read, mutate, or destructive judgment that every tool was obliged to
return. That is gone, and the ground it rests on outlived the clause that first stated
it. Apex section 3 step 7 moved tool safety off the tool: it read that a tool executes
as the agent's constrained user bounded by filesystem permissions, sudoers, and cgroups,
and the egress ruling of 2026-08-07 replaced that with a registered application the
agent addresses across a socket, whose containment is the application's own and not this
program's to state. **Both readings put the judgment somewhere other than in a method
the tool answers**, which is the whole of what this negative needed. A trait method
asking a tool whether it is dangerous is a heuristic standing where a boundary already
stands, and its presence would invite the belief that the answer is load-bearing. The
ruling narrows what this program claims about that boundary and takes nothing back from
the reason this crate carries no classification.

### 3.2 The provider contract

`Provider` is the abstraction that keeps the engine transport-agnostic. The harness
issues decode requests without naming a wire format, and the concrete transport is
constructed at the worker composition root and injected. That injection has to be
against something, and this is that something. Its vocabulary comes with it: the
request, the streamed response, and the error. **No vocabulary clause draws
`provider-trait`, and the reason is the same shape as `tool-trait`'s:** its consumer is
the composition root's injection, inside the body, on the near side of the seam's
transport, so no seam crosses it and no contract has a clause to draw it with.

### 3.3 The message model

Provider-agnostic messages: what a conversation is made of, and what an assistant
turn contains. The harness assembles prompts from it, and the trace records message
events whose payloads carry it, which is why the harness-to-trace contract names this
crate in its vocabulary clause.

It is here rather than in `weaver-types` because it is contract vocabulary the
provider signature requires, not part of an agent's declaration.

### 3.4 Permission modes

The operator-facing policy vocabulary: whether the operator wants to be consulted
before a class of action.

**This is policy, not enforcement,** and the charter says so where the definition
lives rather than only in the harness that reads it. The kernel bounds what a tool
can reach. A permission mode governs whether the operator is asked first. Anything
that reads as a security control while the kernel is the actual control is a thing
that will be trusted wrongly, and the definition site is where that misreading starts.

## 4. What it must not hold

**No logic, no I/O, no async runtime, no model or GPU or storage dependency.** If it
does work it belongs elsewhere.

**No internal dependency.** This is the floor. External dependencies stay minimal and
each one is a decision rather than a convenience, because everything above links this
crate and carries whatever it brings.

**Nothing one crate needs.** A definition earns a place here by crossing a boundary.
A type only the harness uses lives in the harness, however tempting the symmetry of
putting all the vocabulary together.

**No anticipatory contract.** A trait added because a crate not yet chartered will
probably want it is a reserved slot with a trait's shape, and apex section 9 forbids
it in interface form as plainly as in data form.

**No safety classification, per section 3.1.**

## 5. The terms of participation

These bind any crate that links this floor, and they are what make the vocabulary
governable rather than conventional.

**Declare what you use.** Every contract names what it draws from this crate in its
vocabulary clause, and the clause is present even when the answer is nothing. The
union of those clauses is this crate's required surface. A definition here that no
clause names is unused, and a definition a clause names that is absent here is a gap.
That check is the reason the clause is mandatory.

**The declared vocabulary is the only vocabulary.** A consumer does not invent a
parallel definition of something this crate defines, and does not reach around the
contract to a shape of its own. The previous tree's attribute vocabulary was a
convention emitters could ignore rather than a boundary they had to cross, and the
result was that the declared names became a strict subset of the emitted ones.

**A change here is loud by design.** Everything above links this crate, so a change to
a contract ripples to every consumer. That is correct rather than unfortunate. The
ritual carries it: a change to the floor names every PRD and contract it affects,
and what does not land with it is named as owed in the register that tracks it. An
earlier form required the whole change in one act and called anything else
unconsidered. That held while the set was being established and is retired per the
ruling of 2026-08-23. **What survives is the naming, which is what the requirement
was protecting.**

**Thin is the point.** The value of this crate is that it is small enough to audit.
A floor that accumulates becomes a place to put things rather than a place that means
something, and at that point the enforcement argument in section 2 is rhetoric.

## 6. What does not cross

**`Embedder`.** The previous tree's SPU implemented it and its memory crate consumed
it. Memory is out of scope and the encoder is not in the proto-stateful MVP, so
nothing in this program demands it. It arrives when a crate demands it.

**`Channel`.** Implemented there by the harness and the gate for interior message
passing. The agreement between two crates is the contract rather than a shared trait,
carried by a socket where the seam crosses a process line and by a link where it does
not. If the gate pass finds a shared abstraction the contracts cannot carry, it enters
then.

**`AgentMemory` and `GNNInference`.** The memory leg in trait form.

**The safety vocabulary.** Covered in section 3.1.

## 7. The seams

This crate is party to no contract. It performs nothing, so there is nothing to agree
to. It is **named in** contracts rather than signing them, through the vocabulary
clause of the apex vocabulary-clause rule, and the first such naming is
`weaver-harness-trace-contract`, which draws the message model from here.

That distinction is worth keeping straight. `weaver-trace` is a **contract party**,
because it records, validates, projects, and can refuse in named ways. It is not a floor
crate, per `weaver-agents-PRD` section 5.1, since the floor is drawn by every domain and
this crate has one caller. This crate defines and does nothing, so it has
obligations to no one and its governance runs through the clauses that cite it.
