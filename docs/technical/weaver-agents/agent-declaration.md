---
title: the agent declaration
summary: agent.toml, field by field - what the operator declares, what refuses, and what this build leaves tunable
version: v0.1
date: 2026-08-25
commit: unreleased
parent: WeaverTools Technical Documentation
---

# The agent declaration

**Status:** technical documentation. Describes, decides nothing.

**Rough draft, first pass**, written the day the first from-scratch agent on a
second box was declared, refused twice by name, corrected from the refusals
alone, and loaded.

One file per agent, TOML, kebab-case keys, owned by the operator and read by
admin at `validate` and `load` from the directory admin's own config names
(`agent-config-directory`). **Nothing defaults.** An absent required field
refuses the parse at any depth, because a default would be the program
finishing a declaration the operator did not, and an unknown key refuses too,
because a misspelled field that silently vanishes is a declaration that lies.
The named exceptions - fields whose absence a document rules the meaning of -
are called out below where they occur.

## The surface

```toml
session = "s-karl-1"
tool-set = []
permission-mode = "ask"

[spu-instruction.decoder]
residual-readout-election = false
tunable-values = { seed = 451234785645, context-capacity = 8192, max-tokens-per-turn = 1024 }

[spu-instruction.decoder.model-binding]
artifact = "/opt/weaver/models/qwen2.5-0.5b-instruct-q6_k.gguf"
devices = [0]

[[spu-instruction.decoder.identity]]
role = "system"

[[spu-instruction.decoder.identity.content]]
type = "text"
text = "You are Karl, a small local agent running on this laptop."

[gate-instruction.access-rule]
allowed-uids = [1000]
allowed-gids = []
denied-uids = []

[trace-sink]
kind = "file"
path = "/home/operator/.weaveragents/karl/trace.ndjson"
create = true
```

The top-level keys come first and each section follows as its own table: TOML reads a
bare key after a table header as that table's, so a top-level key written below a
table lands inside it and the parse refuses it there as unknown.

**`session`** - the session's identifier, carried on every trace event the run
emits.

**`spu-instruction.decoder.model-binding`** - the artifact path and the
ordered device list, the order being the shard order. An empty device list
refuses the parse: a binding assigning no device is a declaration the operator
did not finish, and defaulting it to device zero would be a placement decision
the program refuses to make. Whether the devices exist and can shard is judged
at admission, not at parse. The SPU refuses a conflicted device and never
evicts, so two declarations naming one device serve one loaded agent at a
time.

**`spu-instruction.decoder.residual-readout-election`** - whether the load
raises the per-layer residual readout. A diagnostic election, `false` for a
plain serving agent.

**`spu-instruction.decoder.identity`** - the canonical messages the identity prefix
renders from. The seed, since 2026-09-04: the first load of a session seats it and the
store holds it from then on, every later load of the session reading the identity from
state and this field only where no member stands. An empty list is a declaration the
operator made, where an absent field is a file unfinished. Worth writing with care - an
identity that advertises tools the `tool-set` does not grant is a lie told to the model
every turn.

**Every message here carries `role: system`, and any other role refuses the
load** with a `config_invalid` refusal naming the field, `identity.<n>.role`.
The message carries `Text` blocks and at least one of them, so an unlicensed
block names `identity.<n>.content.<m>` and an empty one names
`identity.<n>.content`. The identity door writes
`message.system` and refuses the rest, so a prefix under another role would
be seated into the model and left out of the record. Declarations written
before 2026-08-28 carry `role: user` and need the one-word change. A family
whose own template names no system turn, gemma and mistral among them, folds
a system message into the user turn that follows it when rendering, so the
role is what the operator writes everywhere and the shape is the family's.

**That folding is within one rendering and not across them.** The prefix is
rendered and seated when the agent loads, and each later turn is rendered
separately, so on those families a seated prefix becomes one user turn and the
first turn's own framing becomes another after it. The declaration is written
the same way regardless: `role: system`, and the family decides the shape.

**`spu-instruction.decoder.tunable-values`** - the knob map, its own section
below.

**`spu-instruction.decoder.field-election`** and **`surprisal-election`** -
further diagnostic elections, each standing alone by design. The first two
elect an observation into existence and are absent by ordinary posture. The
surprisal election runs the other way: `false`, the default, means the
per-position vector is not produced and the generation's perplexity stands in
its place.

**`spu-instruction.classify`** - optional. Present, it binds the classifier
role's model at the smaller size. Absent, the operator declares the agent
runs no classifier.

**`tool-set`** - the granted tools by name. Empty is toolless, the current
deliverable's shape.

**`permission-mode`** - `ask`, `allow`, or `deny`.

**`binding-kind`** - optional, and absence means `serving`, per the ruling
that a declaration written before the diagnostic member existed declares what
it always meant. A `serving` binding requires `gate-instruction` and a
`diagnostic` binding excludes it - the one cross-field rule, checked by admin
at inventory rather than by the parse.

**`gate-instruction.access-rule`** - who may pass the gate, judged by
`SO_PEERCRED` at connect: `allowed-uids`, `allowed-gids`, `denied-uids`.
**The socket's path is deliberately not here.** Where the door stands is the
program's (inside the unit's runtime directory, `/run/weaver-<agent>/`), and
only who may pass is the operator's.

**A uid named here must also be in the agent's group.** The socket lands
`0770` owned by `weaver-<agent>`, so connecting takes group membership before
the credential check is ever reached - and `weaver-admin validate` refuses a
declaration naming a uid outside it, so the two locks cannot contradict. The
refusal is `boundary_unverified` rather than `config_invalid`, because the
declaration is right and the box's provisioning is not: the answer is
`gpasswd -a <user> weaver-<agent>`, **not** deleting the uid from this list.
Deleting it makes validate pass and breaks the connector for good, the
credential check then denying it with nothing saying why.

`allowed-gids` is not checked that way. A gid names no particular peer, and
whether one holding it reaches the socket turns on that peer's own
memberships, so that half rests on the credential check alone. Uid 0 is never
refused here: root reaches the socket whatever group it holds.

**`trace-sink`** - where the record leaves the program. Three kinds. `file`
opens append-only, creating when `create` is true. `pipe` creates the FIFO
when asked, then opens nonblocking so a reader-less pipe refuses the load
loudly instead of hanging it. `socket` connects to a listener of the
operator's that must already stand. The sink's directory must deny the agent
uid traversal and be owned by root or the admin principal - denial and
custody, two halves, neither implying the other - because the record's
custody is admin's by charter and the agent must not be able to reach the
file that testifies about it.

**`state-election`** - optional, the tee's election, absence meaning the
default election per the state charter.

**`loop-file`** - optional, the agent's own loop, absence meaning the
worker's default loop.

## Tunable values, and the disposition mechanism

Which parameters a declaration may set is **not a property of the format. It
is a property of the binary**: every knob in the SPU carries a compiled
disposition, `Frozen(value)` or `OperatorTunable`, and the set moves only
with a recompile. The map in the declaration is keyed by name so that no
floor type has to enumerate a set that is some deployment's to elect.

The rules of the mechanism, each the source of a refusal met in practice:

- A name the binary froze is **ignored where it appears** - frozen means
  compiled in and never carried, so a declaration cannot move what the
  deployment locked.
- A name the binary left tunable **must be supplied**. The load refuses by
  name - `organ_refused`, `config_invalid`, `tunable-values.<name>` - which
  makes the refusal the discovery protocol: load, read the name, supply it,
  load again. Karl's first two loads were exactly this loop.
- A non-finite value refuses at parse, before any organ runs.

This deployment's build (2026-08-25) leaves three names tunable:

| name | meaning |
|---|---|
| `seed` | The base the per-generation seed derives from. Each generation's stream is fixed by the declared seed, the turn's reference, and the generation's index, so no two draws share a stream and a rerun of a turn reproduces one. |
| `context-capacity` | The session's context size in tokens, sized by the operator against the KV cache's device footprint. Unfroze on the ruling of 2026-08-19 (#221), 32768 the ruled starting point on the primary deployment. |
| `max-tokens-per-turn` | The per-turn generation ceiling, the stop condition's backstop for a model that never emits a stop token. Unfroze the same day (#218) after the frozen 512 met its first real code answer. |

The sampling surface stands frozen beside them: temperature 0.7, top-k 40,
top-p 0.95, repetition-penalty 1.1, repetition-window 64. Frozen on purpose -
a distribution gathered while the sampler varied would report the sampler
rather than the task. A deployment iterating on an agent flips the knobs it
is moving and freezes them back for production.

**The table above is a fact about one build.** The authoritative list for
the binary in front of you is the one its refusals name, and reading a
refusal costs one failed load.

## What refuses where

Three layers judge a declaration, in order, and each refusal is typed:

1. **Parse** (weaver-types): unknown key, missing required field, empty
   device list, non-finite tunable value, malformed trace-sink surface.
2. **Inventory** (admin, at validate and load): the agent resolves in the
   account database and its home exists, the artifact is readable, the sink
   exists or is creatable, the sink directory's denial and custody both
   hold, the allow-list carries the name, the cross-field binding rule.
3. **The organs** (at load): each may refuse its own instruction, the SPU's
   unsupplied-knob refusal being the one an operator meets first.

A refused load leaves a record: the run's trace carries `load`, the typed
`refusal`, and `unload`, so the box's history of almost-agents is readable
after the fact from the sink alone.
