---
title: weaver-types
summary: the floor's shapes: the agent declaration, peer identity, and the wire vocabulary loop 0 speaks
version: v0.1
date: 2026-08-24
commit: unreleased
parent: WeaverTools Technical Documentation
---

# weaver-types

**Status:** technical documentation. Describes, decides nothing.

**Rough draft, first pass.**

The code described here is unreleased and is scheduled for release in the first
quarter of 2027.

## What it is

**The declaration that configures an agent, the identity by which processes
recognize each other, and the vocabulary the internal seams speak.** Where
[weaver-traits](weaver-traits.md) holds the contracts the engine is written
against, this crate holds the shapes that cross boundaries: the config file an
operator writes, the credential a socket peer is judged by, and the directives,
answers, and refusals the organs exchange.

It is not a config framework and not an IPC library. It binds no socket, spawns
no process, opens no file, and holds no handle - a reader expecting the types
crate to also do the parsing, the dialing, and the retrying is carrying an
assumption this program spends deliberately. This crate defines what crosses a
boundary, and crossing it is somebody else's crate. For a data kernel the
compiler is the primary enforcement: the struct definitions are the schema and
the serialization derives are the format, so the documents govern only what the
compiler cannot - semantic invariants and the obligations that run from a
producer to a consumer.

## What it owns

**The agent config.** The declarative document that defines an agent. The
operator produces it, admin validates it before a process exists, and the engine
consumes the elections it carries - no part of this program authors it, because
creating an agent is an operator act and the file is its declaration. It names
the session, the model instruction with its measurement elections, the tool set,
the permission mode, the trace sink, and the binding kind - serving or
diagnostic, declared at the load and never entered afterward, so an agent to be
served and an agent to be replayed are two loads rather than two states of one.
The session is declared and the run is minted: a session spans runs
and only the operator can draw that grouping, while a run identifier is minted
fresh at each load by the party that performs it.

**Peer identity and the one policy function.** A peer is three numbers the
kernel reports - user, group, process - and an access rule is three sets: allowed
users, allowed groups, denied users. One free function judges a peer against a
rule. The rule is a shared type rather than a shape each consumer invents,
because two processes enforcing one boundary with two private rule shapes is a
disagreement waiting for its moment. The function reaches nothing - no file, no
environment, no clock - so where the rule comes from and what happens on refusal
stay with the crates that own those decisions.

**The wire vocabulary.** The envelope every internal exchange rides - who opened
it, an ordinal, a position, a payload - and the payload vocabularies themselves:
the lifecycle trio of directives, answers, and refusals that loads and unloads an
agent, the token trio the decode seam speaks, the label trio the classifier
answers, the turn frame, and the fault report. The refusal vocabularies are
closed sets, which is a promise about what an operator can be told: a condition
the set does not name cannot be reported, so the sets grow by document rather
than by whim.

## Seams

**None.** This crate is floor: drawn by every domain and asking nothing of any of
them. It carries one internal link, to [weaver-traits](weaver-traits.md), because
the config's permission mode and tool set elect from that crate's vocabulary. It
signs no contract - the contracts name what they draw from it in their vocabulary
clauses, and that naming is its whole governance. See
[the contracts page](../contracts.md).

## How it works

**The config parse divides refusal three ways, and each way has an owner.** A
registered field the file omits is admin's refusal - the operator learns the file
is incomplete before any process exists. A field the file carries that nothing
registered is also admin's, and it is refused rather than ignored because that is
the typo case: an ignored unknown field is a silent misconfiguration wearing a
valid file's face. A field present, registered, and wrong is the owning
component's refusal, delivered from its own seam - admin checks presence without
domain knowledge, and what a good value looks like is the component's to know.

**The format answers to its writer.** The file is written by a human, so the
format carries nesting without ceremony and survives an operator's comments. The
wire is written by a program and answers to different pressures, and the two
being different formats is a deliberate cost - one format for both audiences
would make one of them worse off on a file an operator hand-edits under load.

**Identity is supplied by the kernel and judged by one rule.** Both named
sockets are dialable by anything that can resolve a path, so admission cannot
rest on reachability and must rest on identity. The kernel reports who dialed -
the peer never asserts it - and the shared predicate judges the report. Denial
precedes permission: a peer in the denied set is refused whatever the allow sets
say, because the boundary exists to keep one principal out even where a broad
group grant would otherwise readmit it.

**A refusal may be narrowed for a reader and may never be the only record.** A
refusal crosses boundaries on its way to whoever must act on it, and every
boundary is a chance to lose what it carried. A client's one-sentence account is
derived from the recorded refusal rather than matched in parallel, so the
account cannot say what the record does not - and the record holds the whole
case, which is the condition the narrowing rests on.

## What it refuses

**An unknown config field.** Refused, never ignored, for the typo reason above.

**A deserialized peer identity.** The identity type serializes - a refusal that
names the peer it refused is worth recording - and deliberately does not
deserialize. Constructing a peer identity from bytes that arrived over a socket
is one careless call away from the exact substitution kernel-supplied credentials
exist to prevent, and the absent derive is pinned by a test that fails to compile
the day someone adds it.

**A decision keyed on the process id.** The kernel reports it, so the type
carries it, and it is never the basis of a judgment - a process id is reused, so
it exists for the record and the diagnostic alone.

**Absence read as a default.** No type here implements a default. What an absent
value means is a charter's decision, made field by field, and never a derive's.

**Doing anything.** No socket crate, no async runtime, no logging - checked in
the manifest. The parsing dependency for the config file sits behind a feature
switch, so the processes that never parse a config do not carry a parser whose
whole argument against them is that they hold little.

## What is not built

- **The decode seam's encoding.** The trio's shapes are settled and the encoding
  election waits on the decode work, which owes the hot-path measurement the
  per-token volume demands.
- **The tool set's element type.** The field is a list of names today and gains
  its element shape with the tool protocol.
- **What an operator can be told about an agent.** The state and summary answers
  enumerate exactly what crosses out of admin, the four lifecycle states are the
  floor of that set, and whether it carries more is settled with the operator
  interface's own design rather than by a builder.
- **The config file's directory and naming convention.** Operator provisioning,
  outside what this program governs. What is fixed is that a name resolves to
  exactly one file or the load refuses.
- **The floor moves whenever an election is chartered.** The declaration gained
  four fields in the ten days to 2026-08-24 - the session, the binding kind, the
  state election, and the loop file - and it will move again. Each arrival is a
  document first and a field second, which is why the papers here describe shapes
  the released code will match rather than shapes it has all reached.
