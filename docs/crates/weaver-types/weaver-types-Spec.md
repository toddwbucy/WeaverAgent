# weaver-types - Spec

**Status:** MERGED. Cut 2026-08-01 with `weaver-traits-Spec` as the floor pass of phase
one's Spec pass. Code is written against it under the gates of Working Process section
6.

**Date filed:** 2026-08-01
**Document ID:** `weaver-types-Spec`
**Parent:** `weaver-types-PRD`
**Editorial:** Per the Working Rules.
**Landing PR:** #72

---

## 0. What this document is

Build instructions for `weaver-types`: the module layout, the item signatures, the file
format the operator writes, the wire encoding the organ channels carry, and the
elections a builder would otherwise invent. It is derived from `weaver-types-PRD` and
from every contract that draws this crate, which is seven: the coordination agreement,
the residency agreement with its decode and classify siblings, the gate agreement, and
the two outward boundaries, `weaver-admin-operator-contract` drawing the refusal and the
trace door's shapes and `weaver-gate-world-contract` drawing the identity pair with the
gate instruction, `weaver-admin-systemd-contract` having retired with systemd on
2026-10-03 (#50). **The trace agreement is not among them and an earlier wording counted
it.** Its vocabulary clause answers `weaver-types` with nothing in as many words, so the
count stood at six while naming a contract that draws none of this crate and omitting
three that do, and the identity pair it credited to the operator boundary left that
clause on 2026-08-05.

Level discipline. The charter says what the crate holds and why. This document
says how it is represented, which for this crate means two representation
questions the corpus deliberately left here: what shape the agent config takes on
disk, and what octets the organ wire vocabulary becomes on the wire. Where this
document and the charter disagree the charter yields nothing.

**This document declares its crate's assertion records, and one edge that is
another crate's,** per Document Format sections 3 and 4 as of the notation of
2026-08-03. The exception is the shared tagging test of section 4.3: one claim
is one node with an `asserts` edge per crate bound by it, the node lives at the
statement both floor Specs share, and `weaver-traits`' edge is therefore
declared here beside it. **`weaver-traits-Spec` declares its own assertion
records except that edge,** and says so, which is the other half of the same
rule: without both halves the traits act either redeclares an edge already
declared here, which is the duplicate the format forbids, or drops part of its
crate's assertion set with nothing recording where it went. The charter
stays the source of the crate node, the `agent-config` artifact, its eight
`holds` edges, and the twenty-seven vocabulary definitions. What this document
sources is the claims code must conform to, declared at the clauses that argue
them rather than gathered in one place, per that format's section 6, and
`asserts` runs from the crate rather than from this document, which is why the
document needs no node of its own.

## 1. The crate

**Layout.** One module per charter subsection, re-exported at the root.

    src/lib.rs        re-exports, and nothing else
    src/config.rs     the agent config and its eleven fields, section 2
    src/identity.rs   peer identity and the authorization predicate, section 3
    src/wire.rs       the organ wire vocabulary and the envelope, section 4

**Edition and toolchain.** Edition 2024 on the pinned nightly, no nightly feature
used, for the reason `weaver-traits-Spec` section 1 gives: a nightly requirement
on the floor is a nightly requirement everywhere.

**The dependency set is four crates, one of them optional, and each is argued.**
`weaver-traits`, the one internal dependency, declared as the `floor-link` the charter
carries and required because the config's permission mode and tool set elect from that
crate's vocabulary. `serde` with `derive`, for the config and the wire types both.
`serde_json` for the wire encoding, per section 4.3, with its `raw_value` feature on,
because section 4.4's measurement is spliced JSON and the splicing type is what that
feature gates. A TOML implementation for the config file, per the election of section 2,
**behind a non-default `config` cargo feature**. Nothing else, and specifically no
socket crate, no async runtime, and no logging: this crate defines what crosses a
boundary and crossing it is somebody else's crate, per charter section 3.

```graph
node: types-one-floor-link
kind: assertion
tag: manifest

edge: asserts
from: weaver-types
to: types-one-floor-link

edge: grounds
from: types-one-floor-link
to: axiom-floor-is-vocabulary-behavior-is-socket

node: types-no-socket-no-runtime-no-io
kind: assertion
tag: manifest

edge: asserts
from: weaver-types
to: types-no-socket-no-runtime-no-io

edge: grounds
from: types-no-socket-no-runtime-no-io
to: axiom-floor-is-vocabulary-behavior-is-socket
```

**The feature gate is thinness applied where the floor is widest.** Every crate in the
program links this one, and only admin and the harness parse the config file, per
charter section 2.1. Without the gate the gate crate and the SPU carry a config parser
they never call, into processes whose whole argument is that they hold little.
`weaver-traits-PRD` section 5 states the doctrine this follows: thin is the point, and a
floor that accumulates stops meaning anything. The wire types and the identity pair are
unconditional, because more than one crate draws each.

**Two format crates rather than one is a deliberate cost.** The config is written
by a human and the wire is written by a program, and the two audiences want
different things, per the elections below. A single format for both would be one
dependency lighter and would make one audience worse off, which is the wrong trade
on a file an operator hand-edits under load.

## 2. The agent config

The declarative document that defines an agent, per charter section 2.1: written
by the operator, validated by admin before a process exists, read by the harness
for the elections it carries.

**The format is TOML, on the operator's ruling of 2026-09-28, which reverses the YAML
election that stood here on a ground that election did not weigh.** `weaver-types-PRD`
section 2.1 states the criterion, as amended on the same ruling: the file's reader is a
human writing it, so the format answers to a writer rather than a parser, carries the
declaration's nesting, and survives an operator's comments, and the parser's surface in
the binary that runs as root is part of the criterion, ceremony accepted where that
safety is bought. TOML meets it: it is a writer's format, its `#` comments survive every
edit an operator makes, and it carries the nesting the declaration uses, at the cost of
ceremony on the deeper tables, a section header where YAML would indent. The ground the
YAML election did not weigh is its parser. `serde_yaml_ng` sits on `unsafe-libyaml`, the
C library `libyaml` translated into Rust under `unsafe`, compiled into the binary that
runs as root for one purpose, parsing a declaration, and the shipped-dependency audit of
pull request #725 measured it at six crates compiled for every binary that links this
crate under the deploy's unified build. The `toml` crate, Cargo's own format, is safe
Rust, and five crates compiled where the YAML parser was six. **Declarations are written
in the TOML 1.0 grammar**, the subset every reader in the suite shares: admin's `toml`
crate reads TOML 1.1, the deploy script and the experiment harness read with Python's
`tomllib`, which reads 1.0, and a declaration only one of them can read is one the
others refuse. Admin's parser is the authority on what a declaration says, and syntax
only 1.1 has, a multiline inline table or a `\e` escape among it, is outside what the
program guarantees. **A path in a declaration carries no control character**, on the
operator's ruling of 2026-09-28: the restore's `record`, the `loop-file` (retiring with
the pyworker lane, the compiled path's state-management loop being compiled into the
worker on the operator's ruling of 2026-10-02 on #1) and the trace sink's `path` are
refused by name where one does,
because every reader in the suite, admin's parser, the deploy script's shell and the
harness, must agree on what a path is, and a control character is where they part.
JSON stays rejected on the criterion's comments clause: it has none, and a
trailing-comma error at three in the morning on a file that gates a load is a bad way
to learn about JSON.

```graph
node: types-config-format-toml
kind: assertion
tag: review

edge: asserts
from: weaver-types
to: types-config-format-toml
```

**What this election must survive is the implementation's maintenance**, so the
implementation is a maintained one and the Spec names that as a requirement rather than
a preference. A builder confirms maintenance status at the moment of writing the
manifest, and the `toml` crate being the one Cargo itself reads is why the requirement
is expected to hold.

**The session is declared and the run is minted.** `session` joins the
declaration because a session spans runs and an agent outlives a session, per
`weaver-admin-PRD` section 4.4, so the grouping is one only the operator can
draw and the same agent serves many of them by editing one field. The run is
the other half and does not appear here: `RunId` is minted at each load by the
party that performs it, and a declared run would be a value the operator had to
change before every load or watch collide. What the floor fixes is that both
are identifiers rather than numbers, which is what lets a run reference carry a
stamp that distinguishes without anything being remembered between invocations,
per the identity ruling of 2026-08-14.

**One declaration per agent, in the agent's territory, on the operator's ruling of
2026-10-07 (#1), and the system prompt is not in it**, on the operator's ruling of
2026-10-06 (#1). Admin resolves an agent's declaration as `agent.toml` in the territory
the agent's root names as its `territory`, `/var/lib/weaver-agent/weaver-<agent>/` as the
deploy scripts lay it out, per `weaver-admin-Spec` section 9. It is root's, which the
operator edits with `sudoedit` and the access group reads, and never readable by the
agent, the territory being closed to the agent's uids. **The declaration says how the agent is built; state says who it is**:
the system prompt is an entry in state management, entered through the gate as the
agent's first turn and carried by its save points, per `weaver-harness-Spec` section
6.1 and `weaver-gate-world-contract` section 2, and `system-prompt.md` beside the
declaration is the operator's draft of it, read once by the seeding step and never by
admin or by any organ. The directory's place stays operator provisioning, outside what
this program governs, per `weaver-admin-PRD` section 1. Admin resolves an agent name to
exactly one declaration
and refuses a load where it resolves to none. **The keys that name the programs admin
starts stay in the root**, root-owned, and never in this directory, per
`weaver-admin-Spec` section 9: the declaration is data the operator edits, and which
binary runs under the agent's identity is placed where the admission is.

```rust
pub struct AgentConfig {
    pub session: SessionId,
    pub spu_instruction: SpuInstruction,
    pub tool_set: Vec<ToolName>,
    pub permission_mode: weaver_traits::PermissionMode,
    pub binding_kind: Option<BindingKind>,
    pub gate_instruction: Option<GateInstruction>,
    pub trace_sink: TraceSink,
    pub state_election: Option<StateElection>,
    pub state_store: Option<StateStore>,
    pub loop_file: Option<PathBuf>,
    pub restore: Option<Restore>,
}

pub struct Restore {
    pub save_point: String,
}
```

The one member is the published name or the digest, per the paragraph above.

```rust

pub struct StateStore {
    pub engine: StoreEngine,
    pub database: Option<String>,
    pub role: Option<String>,
}

pub enum StoreEngine {
    None,
    Sqlite,
}

pub struct StateElection {
    pub all_kinds: bool,
    pub keys: Vec<ElectedKindConfig>,
}

pub struct ElectedKindConfig {
    pub kind: String,
    pub paths: Vec<String>,
}

pub enum BindingKind {
    Serving,
    Diagnostic,
}

pub struct SpuInstruction {
    pub decoder: DecoderInstruction,
    pub classify: Option<ClassifyInstruction>,
}

pub struct ClassifyInstruction {
    pub model_binding: ModelBinding,
}

pub struct DecoderInstruction {
    pub model_binding: ModelBinding,
    pub residual_readout_election: bool,
    pub field_election: Option<FieldElection>,
    pub surprisal_election: bool,
    pub column_permission: bool,
    pub refeed_permission: bool,
    pub identity: Vec<weaver_traits::Message>,
    pub tunable_values: BTreeMap<String, f64>,
}

pub struct FieldElection {
    pub depth: u32,
}

pub enum TraceSink {
    File { path: PathBuf, create: bool },
    Pipe { path: PathBuf },
    Socket { path: PathBuf },
}

pub fn parse(source: &str) -> Result<AgentConfig, ConfigError>

pub struct ConfigError {
    pub field: Option<FieldName>,
    pub kind: ConfigErrorKind,
}

pub enum ConfigErrorKind {
    Malformed,
    MissingField,
    UnknownField,
    BadValue,
}
```

**The identity is the state member's to serve at every load**, on the operator's
ruling of 2026-10-06 (#1), which supersedes the ruling of 2026-10-02 on #58's fifth
question that the prompt file was authoritative: the declaration carries no identity,
the enter carries none, and the open seats what the member answers to the `identity`
ask of `weaver-harness-state-contract` section 2, the turnless `message.system` events
of the newest run the holdings carry, which a save point carries across loads. The
store keeps the history of what the loads seated and decides none of them, per
`weaver-state-PRD` section 4.

**`[state-management]` is the declaration's table for the compiled loop's settings**,
and its grammar is this crate's, on the operator's ruling of 2026-10-02 on #58's sixth
question, and its meaning is `weaver-harness-Spec`'s, with the loop. It is an optional
top-level table: absent, every setting stands at its compiled default. Its members
arrive with the act that compiles the loop, each a typed key this section then names,
and until then the table carries no key, so any member refuses `UnknownField` under the
unknown-key rule below, as every key no organ registered does.

**The declaration carries no identity**, on the operator's ruling of 2026-10-06 (#1).
The decoder's section carries neither the inline `[[spu-instruction.decoder.identity]]`
table of before 2026-10-02 nor the `identity-file` key of #57, and each is a key no
organ registers, refusing `UnknownField` by name under the unknown-key rule below, so a
declaration written under either form fails its parse at the first load rather than
seating a prefix that state does not hold. The system prompt was a string inside the
declaration, then a file the declaration named; both were a reading of the prompt as
configuration, and it is state: the prompt enters state management through the gate as
the agent's first turn, the seeding turn of `weaver-harness-Spec` section 6.1, from the
operator's draft `system-prompt.md` beside the declaration, and from then on it is held
and served by the state member and carried by the agent's save points, the declaration
saying how the agent is built and state saying who it is. **The mechanism holds no
opinion about the prompt's content**, on the operator's rulings of 2026-10-02: the
operator is assumed competent, and the trace's `message.system` event of the seeding
turn shows, byte for byte, what the model was given. `DecoderInstruction` loses its
`identity` member with this ruling, per section 4, the open's prefix being the state
member's answer under either binding.

**A diagnostic declaration carries no identity either**, the ruling of 2026-10-02 on
this act's sixth question being unchanged in substance: the replay's identity is the
preloaded record's own, a diagnostic replay always standing a member preloaded from the
record, so the open seats the record's own `message.system` events, in order and with
their boundaries, byte for byte, and certification is unaffected.

**The name is a bare file name, resolved in the declaration's own directory and
nowhere else.** It carries no `/`, is not `.` or `..`, is not empty and carries no
control character, and anything else refuses `BadValue` naming the field. A relative
path that could climb, or an absolute one, would send admin's read out of the
directory `weaver-admin-Spec` section 9 judges, and a judgment of one directory is
worth only as much as the reads that stay inside it. The name is the operator's, and
`system-prompt.md` is the convention the deploy scripts write.

**The parse stays total and reads nothing.** This crate holds no I/O, per section 5's
manifest clause, so the read is the caller's: `parse` takes the declaration's text
alone, on the operator's ruling of 2026-10-06 (#1): no reader, no file, the whole
`AgentConfig` or a typed error. The shape the enter carries is the resolved one, the
same pattern the state election takes, admin filling at inventory what the worker never
re-derives.

**Both earlier forms of the identity refuse by name, and the grammar's own rules
decide it.** The `[[spu-instruction.decoder.identity]]` table of before 2026-10-02 and
the `identity-file` key of #57 are each a key no organ registers, so each refuses
`UnknownField` naming it under the unknown-key rule below, and a declaration written
under either fails its parse by name at the first load rather than seating a prefix
state does not hold. The remedy is the seeding turn: the prompt's text goes into
`system-prompt.md` beside the declaration, the key or the table is removed, and the
seeding step sends the draft through the gate after the load, per
`weaver-harness-Spec` section 6.1.

**`surprisal_election` is a bare `bool` and `field_election` is an
`Option`, which is a difference in what each has to say rather than an
inconsistency.** A field that is elected carries a depth, so the option
holds the value and its absence is the whole of an unelected field. A
surprisal has no size: the vector exists per decode position or it does
not, so the flag is the whole of it either way. **False is not the same as
absent here**, per `weaver-trace-Spec` section 3: a declaration written
before this election existed omits the member, and serde fills the default,
so the two states a reader must keep apart are kept apart in the record
this crate feeds rather than in this struct.


**`tunable_values` is the route `Disposition::OperatorTunable` names.** The SPU
elects per parameter at its composition root whether a value is compiled in or
supplied, and this map is where a supplied one arrives. It is keyed by the
parameter's name and carries a number, which is the shape the SPU's resolve
already takes, and it covers the sampling knobs and the session parameters
alike because the election is the same election in both cases.

**It is a map rather than a field per parameter, and the reason is which side
owns the list.** Which parameters a binary leaves tunable is that binary's
election and changes with a recompile, so a floor type enumerating them would
have to move whenever a deployment changed its mind, and a declaration naming a
parameter this binary froze is a fact with no effect rather than an error. The
map is present always and may be empty, the no-defaulting rule reaching the
field and not its contents: an empty map is a declaration that supplies
nothing, and a binary with a tunable parameter and nothing supplied for it
refuses the load naming the parameter.

**A value is judged against what the parameter is before the session opens.**
The map carries `f64` because a configuration's numbers are, and most of what it
feeds is not a real: `top-k`, the repetition window, the context capacity and
the per-turn ceiling are counts. So a value bound for a count must be integral,
non-negative, and inside the range its type can hold, and a value bound for a
real must be finite. A `NaN` reaching a sampler is a temperature that compares
false against every bound and an infinity is one no filter clamps.

**A bad count is worse than either, and the reason is that the conversion never
fails.** A float cast to an integer in Rust answers for every input: it
truncates toward zero, so `3.7` becomes `3`, it maps `NaN` to zero, and it
saturates at the target's bounds, so `-1.0` becomes `0` and `1e20` becomes
`u32::MAX`. Measured on this workspace's toolchain rather than recalled. So a
fractional count is silently rounded, a negative one silently becomes zero, and
an oversized one silently becomes four billion, and none of the three reaches
the operator as an error. The truncating case and the saturating case are
different failures and the check refuses both, because a capacity of
`u32::MAX` would fail later at allocation with nothing pointing back to the
declaration that asked for it.

**The refusal is the load's, not the turn's.** A bad value is `BadValue` naming
its field, refused where the declaration is read and before any device work,
which is the same shape every other malformed field in this section takes. The
earlier home for this check was the token seam, where the map used to travel and
where the refusal was `MalformedDelta`, and it moved here with the map rather
than being invented.

**A value here never overrides a frozen parameter.** Frozen means compiled in
and not carried on the wire, so a name the binary froze is ignored where it
appears, and the record still reports the effective value whichever side set
it. That is what keeps a deployment's lock a lock rather than a default.

**The SPU's fields arrive as one section, and the gate's already did.** Charter
section 2.1 rules that an organ's fields are named together and cross together,
and `spu-instruction` is that rule's first application: one declaration admin
validates, the harness carries uninterpreted, and the SPU consumes.
**`decoder` names a role rather than a slot, and `classify` is the second
role, arriving 2026-08-19 with the act that builds it.** The organ's
chartered domain is
every semantic operation in the text modality, per `weaver-spu-PRD` section 8,
and the decode role is the one whose seam stood first,
`weaver-harness-spu-decode-contract`, so each key names something built, with a
reader today. The `classify` subsection carries the model binding at the
smaller size and is optional by presence, per `weaver-spu-PRD` section 15.3:
its absence is the operator declaring the agent runs no classifier, a
declaration rather than a default, so the no-defaulting rule below is
untouched by it. An embedder key arrives in the act that builds an embedder, named
here as absent rather than carried empty, which is the near side of apex
section 9. The no-defaulting argument below survives the nesting untouched:
every field of a present section is required, absence of a required field
refuses the load, and the depth
at which a field sits changes nothing about what its absence means.

**The identity material rides the decoder's section**, resolving the open
exchange's source per the harness handoff's first question: the canonical
messages the identity prefix is rendered from are state rather than
configuration since the operator's ruling of 2026-10-06 (#1): the operator enters
them through the gate as the seeding turn, the state member holds them, the
harness seats what the member answers at every load and carries them
uninterpreted, and the SPU consumes them at the session's open. The declaration
names none, and an agent not yet seeded opens with no prefix, which is
a declaration the operator made, an agent with no identity prefix being a
legitimate agent, where an absent key is a file unfinished. The messages
are `weaver-traits`' `Message`, drawn rather than restated, and the field is
representation interior to the section the way the gate instruction's access
rule is, no vocabulary node moving.

**Every message of the identity carries `role: system`, and this clause states
the rule ahead of the instrument that will hold it.** The identity door of
`weaver-harness-Spec` section 6 writes `message.system` and refuses every
other role, per `harness-identity-door-writes-system-only`, so a declaration
carrying any other role seats a prefix into the decode context that the record
cannot show - the condition `weaver-harness-PRD` section 5's fifth case exists
to report, found in the field and filed as issue #369. Every message carries
at least one `Text` block, each with text in it and nothing else, per
`weaver-traits-Spec` section 3, and the parse judges that beside the role: the
door refuses both, and judging one here would leave the other to be met as a
fault in a running agent. **The rule belongs
here rather than at the door.** The door refuses what the declaration should
never have contained, which makes it the last place the condition can be
caught rather than the first, and a rule enforced only at the last place is a
rule the operator meets as a fault in a running agent instead of as a refusal
to load. Where declarations are judged is the parse, and the judgment is this
crate's.

**The check is the parse's, and it is in force.** The instrument is the shape
`model-binding`'s empty device list already takes: a `ConfigErrorKind::BadValue`
naming `identity.<n>.role`, so a declaration that would seat an unwritable
prefix does not parse and never reaches a door. The index rides the name
because a list with several messages needs to say which one, as
`tunable-values.<name>` already does. **Since 2026-10-02 the declaration cannot
write a message the check refuses**: the file seats one `system` message with one
non-empty `Text` block or seats nothing, so the role, the block and the empty text are
held by the seating's construction and the check runs over what was seated. It stays in
`parse` on the reasoning above, the door's refusal being the last place and the parse
the first, and an `identity.<n>` name it returns now names a defect in the seating
rather than a line the operator wrote.

**It was written as owed for two days and the reason is worth keeping.**
Changing a declaration's identity role changes the prompt - a system role
renders `<|im_start|>system` where a user role renders `<|im_start|>user` - so
the tokens differ and every run made under the old role stops being a
baseline. The cross-precision deposit of 2026-08-25 and the determinism matrix
of 2026-08-27 were both produced under `role: user`, and a refusal landing
before issue #346's cross-architecture arms ran would have voided that
comparison rather than delayed it. Those arms deposited 2026-08-27 and
2026-08-28, and the operator's ruling of 2026-08-28 retires those deposits as
a comparison basis in favour of a re-baseline on one frozen build, which is
what dissolved the gate. **An empty identity still parses**, an agent with no
prefix being a legitimate agent: the rule judges the messages present and does
not require one to be.

```graph
node: types-identity-role-is-system
kind: assertion
tag: perturbation

edge: asserts
from: weaver-types
to: types-identity-role-is-system
```

**Field names are kebab-case on disk and snake_case in Rust, which takes an
explicit election rather than a convention.** `#[serde(rename_all = "kebab-case",
deny_unknown_fields)]` on the struct, so the operator writes `model-binding` and
`trace-sink` as the charter's vocabulary names them and the Rust field names stay
idiomatic. The `deny_unknown_fields` half is the fixed-surface mechanism of section 2
and stands until the spec round
`weaver-types-PRD` section 2.1 names. Without the rename the two spellings diverge,
which is the collision Document Format section 5 rules against for graph identifiers and
which reads the same way on a page.

```graph
node: types-config-names-kebab
kind: assertion
tag: perturbation

edge: asserts
from: weaver-types
to: types-config-names-kebab
```

**Every field of the declared surface is required and absence is a refusal, with the
temptation named.** The surface is the union of what the organs register, per
`weaver-types-PRD` section 2.1, so what is required is a per-binary fact rather than one
struct's field list, and the refusal is against that surface rather than against a fixed
type. The property below is unchanged by that and is why the rule exists. Charter
section 5 rules that absence is never read as a default unless the charter says a field
is optional and says what its absence means. Three fields are optional by exactly that
rule's own terms, and one more is an option at the parse for a different reason, named
after them. `state_election` may be absent because `weaver-state-PRD` section 4 rules
what absence means, the default election, the envelope of every kind and nothing more,
so a deployment that elects nothing still holds the session's shape by the charter's
sentence rather than by a parser's guess. The resolved spelling of that default is fixed
here so two resolvers cannot disagree: `all_kinds` true and `keys` empty. The empty list
is only the default's spelling, not a constraint on the pair: `keys` stays meaningful
beside `all_kinds` true, each named kind adding payload paths on top of the envelope
every kind already crosses with. **Every kind means every kind but the save-point
kinds**, which no election matches, per `weaver-harness-state-contract` section 3 on
the rulings of 2026-10-02 on #58. `EnterPayload` carries the election resolved, admin
filling that ruled default at inventory, so the worker never re-derives an absence. When
the block is present, both its members are required, the required-field discipline
resuming inside it. `loop_file` may be absent because `weaver-harness-PRD` section 2
rules what absence means, the worker's own default loop, the compiled body or the
installed file, so a declaration written before the member existed still parses and
still means what it meant. Present, it names the loop file the agent's worker runs, the
loop being a member of that agent's harness and unique to it per the same section's
ruling of 2026-08-20, and it reaches the worker in its argument vector per
`weaver-admin-Spec` section 6 rather than in any exchange, because no exchange carries a
path. `state_store` may be absent because `weaver-state-PRD` section 4 rules what
absence means, the embedded engine, so a declaration written before the member existed
still parses and still means what it meant, and the state member stands. Present, its
`engine` names which port the deployment elects, as of 2026-09-04: `none` declares that
no member stands, which is a deployment's real posture and the one the
instrument-validation matrices ran under, and `sqlite` the embedded engine, for which
`database` and `role` are refused if present, the same cross-field rule admin holds for
the gate instruction, judged at inventory before a process exists. **An engine this
build does not provide refuses at the parse**, `BadValue` naming `state-store.engine`,
which admin answers `ConfigInvalid`, so a declaration electing one fails naming the
field rather than as an unnamed unknown value. No engine is refused by its own name: on
the operator's ruling of 2026-10-05 (#86) another deployment may provide another engine
behind the same contract, and the service engine, postgres, is simply not provided here
since the ruling of 2026-10-02 on #1. `database` and `role` stay in the type, refused
for both provided engines. **`none` beside a present `state_election`
is refused by the same rule**: the election says what the tee sends to the member, and a
declaration that elects what to send to a member it declined is malformed rather than
surplus, refused `ConfigInvalid` naming the election, the way a granted permission
naming a field is. `none` with the election absent is whole, the ruled default still
written on the load event as the record's posture, per `weaver-trace-PRD` section 3.1,
beside a `state_member` of false. The three are the binding's members: they change only
across the load boundary, they ride the enter directive resolved, and the load event
records them, per `weaver-trace-PRD` section 3.1. The enum is closed at two because
this build provides one engine, the embedded one, beside the absence of a member, and a
further engine is a state act before it is a variant. `binding_kind` may be absent because
`weaver-types-PRD` section 2.1 rules what absence means, a serving binding, so a
declaration written before the member existed still parses and still means what it
meant, on the same footing as `loop_file` above. The enum is closed at two cases because
`weaver-agent-PRD` section 6 names exactly two kinds, and a third kind is an apex act
before it is a variant. `gate_instruction` is an `Option` for a reason the three above
do not share: presence follows the resolved kind rather than standing alone, a serving
binding requiring it and a diagnostic binding excluding it, per the contract's shape
rule of 2026-08-24. The parse cannot see a cross-field rule, checking each field alone,
so it parses as an option and the rule is admin's at inventory, before a process exists,
per `weaver-admin-Spec` section 4. **`trace_sink` is required under either kind**, and
briefly was not: an act of 2026-08-24 made it conditional on the reading that a
diagnostic binding authors nothing, and the operator's ruling of the same date replaced
that reading with a composition, every binding authoring a record and the kind selecting
the mechanism rather than the presence, per `weaver-agent-PRD` section 6. What the
declaration names is a sink, and what the run writes into it follows the kind without
the field moving. Every other field is required. The residual-readout election is what a
builder will reach to default, to off, and it is exactly the one that must not: an
operator who stated no readout has not thereby declined it, and admin refusing the load
is how that operator learns the file is incomplete rather than discovering it in a
record with no reductions in it. This is why `AgentConfig` derives no `Default` and
`parse` returns no partial value. **`restore` names a save point**, on the operator's
ruling of 2026-10-02 on #58, which retires the record restore of 2026-09-04 (issue
#432) and its cut. It may be absent because `weaver-state-PRD` section 4 rules what
absence means: the load restores the latest save point published, the latest being the
highest ordinal of admin's manifest whose file stands and whose digest matches, per
`weaver-admin-Spec` section 6 on the operator's ruling of 2026-10-06 on #1, never a run
reference, a name or a file time, and with none state is rebuilt from the trace
offline. **Restoring needs a member to restore into**: where the
declaration's store engine is `none`, no save point is selected, by name or by default,
and a `restore` member present beside that engine refuses `ConfigInvalid` naming
`restore`, so no load names holdings that no member restored. Present, `save-point` is the
published name of a save point in the declaration's own directory,
`<YYYYMMDDTHHMMSSZ>-<digest>.save-point`, or its digest alone, a bare name carrying no
`/`, not `.` or `..`, not empty and with no control character, refusing `BadValue`
naming `restore.save-point` otherwise. Admin resolves it through its manifest and opens
it under its own custody, per `weaver-admin-Spec` section 4, and the harness never sees
it; the record-and-cut form of before A3.2 retired with the record restore. A save point the
operator built offline from a record names that record and cut inside itself, not here,
so a branch from a record is the builder's act and this member keeps one meaning.

```graph
node: types-required-field-refuses
kind: assertion
tag: perturbation

edge: asserts
from: weaver-types
to: types-required-field-refuses

node: types-no-default-derive
kind: assertion
tag: review

edge: asserts
from: weaver-types
to: types-no-default-derive
```

**This Spec is authoritative for the tee's election's declared shape, and a second
spelling of it exists on purpose.** What the term means on the state seam is
`weaver-harness-state-contract`'s, which defines it there as that seam's vocabulary, so
the two authorities answer different questions rather than one question twice.
`weaver-trace` holds its own `Election` of the same shape because that crate depends on
nothing, not even this floor, and cannot spell a type it does not link. From 2026-08-25
the record carries that second form on the `load` event, per `weaver-trace-Spec` section
3, which makes four representations of one fact: this declaration, the enter payload,
the tee's own, and the record's. **Field spellings are each renderer's and are not
shape**: this declaration renames for the operator's file and the record renders
`all_kinds`, which `weaver-trace-Spec` section 3 argues cannot be renamed without moving
a live seam. This side renames for a file rather than a seam, per the kebab-case
election above. **A divergence of shape among them is a defect against this section**
rather than a choice a reader makes, per G5, and unifying the types is not owed: it
would cost `weaver-trace` a no-dependency property that is load bearing for its own
reasons in order to buy a name.

**`trace-sink` names a sink and not only a path.** A file, a pipe, or a socket are
all conforming sinks, per `weaver-admin-operator-contract` section 3, so the field
carries a discriminated shape and admin opens by the discriminant. A bare path
would force admin to guess from the filesystem what the operator meant, and the
guess is wrong exactly when the operator meant a named pipe that does not exist
yet, which is the discriminant's whole argument.

**Admin is the field's one reader**, per `weaver-admin-Spec` section 5's assertion that
the sink path dies at its one open site and issue #311: the harness receives the opened
descriptor over the coordination socket and never the path, so the name this field
carries reaches no process but admin's, and the harness charter's sentence that the
agent is never told it is the same fact from the other side.

```graph
node: types-trace-sink-discriminated
kind: assertion
tag: review

edge: asserts
from: weaver-types
to: types-trace-sink-discriminated

edge: grounds
from: types-trace-sink-discriminated
to: axiom-contract-is-a-complete-interface
```

**`File` and `Pipe` carry a creation flag and `Socket` does not, and the
asymmetry has a reason rather than an oversight.** Admin can create either of the
first two, an empty file or a `mkfifo`, and the charter's validate step reads the
flag to decide whether a missing sink refuses the load or is made. A socket sink
is different in kind: admin connects to it and something on the operator's side
must already be listening, so a creation flag would promise an act admin cannot
perform. A missing socket sink therefore always refuses.

**`ModelBinding` carries the artifact and the devices it is assigned to, and
the devices are a set.**

```rust
pub struct ModelBinding {
    pub artifact: ArtifactRef,
    pub devices: Vec<DeviceOrdinal>,
}
```

Per charter section 2.1 as the ruling of 2026-08-03 states it. The vector is
ordered and the order is the shard order, so a two-device assignment says which
device holds which half rather than leaving a builder to pick, and a set of one
is the ordinary case with no special shape. **An empty set is a parse error
rather than a default**, because a binding assigning no device is a
declaration the operator did not finish, and defaulting it to device zero is
the placement decision this crate exists not to make. Whether the devices
exist, whether they can reach each other, and whether the backend can shard
across that many are all admission's, per `weaver-spu-Spec` section 3, and
this crate answers well-formed and nothing more.

**`GateInstruction` carries the access rule and not the socket, ruled
2026-08-15.** Where a socket sits is the program's and only who may pass it is
the operator's, per `weaver-gate-PRD` section 2, so the instruction the
operator writes carries the rule alone and the raise directive carries the
socket beside it as a second field. **Two fields because two authors**, and a
single one would put the operator's name on a value they do not choose. The
grouping survives the loss: `gate_instruction` stays a named group rather than
collapsing to a bare rule, on the same pattern `spu_instruction` takes, so a
field the gate workflow adds later has somewhere to land.

**`SpuInstruction` and `GateInstruction` are defined here and are the same types the
wire carries, and the feature gate never reaches them.** The `config` feature gates the
parser and the `parse` surface alone, and never a type: the two are wire types the loop
0 directives carry, so they compile with the feature off, which the harness's
featureless link constructs directives against, per `weaver-harness-Spec` section 1. A
build with the feature off holds every type of this crate and no config-format
dependency, which is section 1's unconditional-wire sentence stated as the gate's
mechanical scope. Both travel two paths, into the config from the operator and across a
seam inside a directive, and one type for both is what keeps admin from re-encoding what
it validated. `ToolName` is a name today and gains its element type with the tool
workflow, because it elects from `tool-trait`, which `weaver-traits-PRD` section 3.1
holds blocked.

**Validation is a total parse into a typed value, and admin performs it.** The
crate exposes `parse` and nothing partial: no builder, no field-by-field accessor
over a half-read document. A partially valid config is the shape that lets a load
proceed on half a declaration, and the type system is where that is prevented
rather than in admin remembering to check.

```graph
node: types-config-parse-total
kind: assertion
tag: compile-pin

edge: asserts
from: weaver-types
to: types-config-parse-total
```

What admin adds beyond the parse is
existence where admin holds custody, that the model artifact is named, the sink
exists or its creation flag is set, and the boundary is as the operator wrote it,
per `weaver-admin-PRD` section 4.3, whether the artifact resolves being the SPU's
to answer at admission under the agent's identity. The parse answers well-formed
and the checks answer real, and this crate owns the first only.

**A field no organ registered refuses rather than being ignored.** Unknown is measured
against the declared surface, per `weaver-types-PRD` section 2.1. **The derive this
crate carries denies unknown fields against one fixed type, which is the mechanism for a
fixed surface, and it is what holds the property today.** It stands until the spec round
that charter section names, and what survives that round is the property rather than the
derive. The three sites below that name the derive say the same and are qualified rather
than rewritten, because the representation is that round's to change and not this act's.
An ignored field is an operator's declaration silently discarded, and a typo in
`permission-mode` that parses as an unknown field and vanishes is the failure this
rejection exists to prevent.

```graph
node: types-unknown-key-refuses
kind: assertion
tag: perturbation

edge: asserts
from: weaver-types
to: types-unknown-key-refuses
```

## 3. Peer identity and the authorization predicate

The identity type, the rule it is judged against, and the one policy function, per
charter section 2.2, drawn by the two seams that judge a peer by credential: the
gate's client socket, governed by `weaver-gate-world-contract`, and the
coordination socket the harness binds, governed by
`weaver-admin-harness-contract`. The second was the operator surface until the
recut of 2026-08-05 retired it, and the coordination seam took its place when the
inversion gave that seam a name and a credential to check.

```rust
pub struct PeerIdentity {
    pub uid: u32,
    pub gid: u32,
    pub pid: i32,
}

pub struct AccessRule {
    pub allowed_uids: BTreeSet<u32>,
    pub allowed_gids: BTreeSet<u32>,
    pub denied_uids: BTreeSet<u32>,
}

pub fn authorized(peer: &PeerIdentity, against: &AccessRule) -> bool
```

**The rule is a type here rather than a shape each consumer invents, because that
is the whole of the charter's carve-out argument.** `weaver-types-PRD` section 2.2
rests the exception on one shared definition being the only way separate processes
provably enforce the same rule, and a predicate that took a rule shaped by its
caller would deliver a shared signature over two different rules, which is the
disagreement the carve-out exists to prevent. The three sets are what the two
consumers need and no more: the gate admits front-end principals by uid or group
and excludes the agent uid, and the coordination seam admits root alone, per the
inversion of 2026-08-05, which excludes the agent uid by the same reading. The
operator surface was the second consumer until that date and retired with the
service account it admitted to.

**Denial wins over permission, and the ordering is the security property.**
`authorized` returns false if the peer's uid is in `denied_uids`, whatever the
allow sets say. The gate's predicate must exclude the agent uid even where a
broad group grant would otherwise admit it, per `weaver-gate-PRD` section 2, and a
rule evaluated permission-first would let a group membership readmit the one
principal the boundary exists to keep out.

```graph
node: types-denial-precedes-permission
kind: assertion
tag: perturbation

edge: asserts
from: weaver-types
to: types-denial-precedes-permission
```

**The predicate is a free function over data and reaches nothing.** No file, no
environment, no clock, no global. Where the rule comes from, how a failure is
handled, and what happens on refusal live in the consuming crates, per charter
section 2.2, and a predicate that loaded its own access list would have taken the
first of those back.

```graph
node: types-one-policy-function
kind: assertion
tag: review

edge: asserts
from: weaver-types
to: types-one-policy-function
```

**`pid` is carried and is never the basis of a decision.** `SO_PEERCRED` reports
it, so the type would be lying by omission to drop it, and it is unsound as an
authorization input because a pid is reused. It exists for the record and for a
diagnostic, and the predicate ignores it. A builder tempted to key on it is the
reason this sentence is here rather than in a comment.

**`PeerIdentity` derives `Serialize` and deliberately not `Deserialize`, and this
is a compile-fail pin.** The type's whole point is that the kernel supplies it and
the peer never asserts it. A derived `Deserialize` is the machinery for
constructing a peer identity from bytes that arrived over a socket, which is one
careless call away from the exact substitution `SO_PEERCRED` exists to prevent.
Serialization stays, because a refusal that names the peer it refused is worth
recording.

```graph
node: types-peer-identity-no-deserialize
kind: assertion
tag: compile-fail

edge: asserts
from: weaver-types
to: types-peer-identity-no-deserialize

edge: grounds
from: types-peer-identity-no-deserialize
to: axiom-floor-is-vocabulary-behavior-is-socket
```

**The threat walk.** This Spec names the adversary each security mechanism
defeats and derives that mechanism's test from the attack, which is how a
perturbation test under apex section 11 becomes a scenario rather than an
assertion. The adversary is a
process on the host that is not a front-end principal and dials one of the two
named sockets: an elected tool running as the agent uid reaching for the agent's
own mouth, or the same tool reaching the coordination socket its harness binds.
The mechanism is
that both sockets are named and therefore dialable by anything that can resolve
the path, so admission cannot rest on reachability and must rest on identity. The
kernel supplies that identity rather than the peer asserting it, which is what
`SO_PEERCRED` is for and what the missing `Deserialize` keeps true, and the
predicate judges it by one shared rule so that two processes enforcing one
boundary cannot disagree.

**The walk yields tests at two levels, and each names the crate that owns it.** In
this crate, over values: `authorized` returns false for a peer whose uid is the
agent's against a rule whose allow sets include the agent's group, confirmed by
watching it return true when denial stops preceding permission. In the gate's
crate, over behavior: a connection from the agent uid is refused at accept, before
any content is read, confirmed by watching content reach the harness when the
predicate is weakened. The second is named here because the rule is defined here
and owed by `weaver-gate-Spec`, which is the G7 shape read forward rather than a
test this crate can pretend to run.

### 3.1 The boundary file, the trace door's wire, and a change's cause

**Admin's lifecycle has no wire of its own**, on the operator's rulings of 2026-10-03 on
#50: a caller that is not the operator at the shell reaches the verbs through a sudo
rule the operator installs, which grants this agent's connector fixed `weaver-admin`
command lines and reads nothing else from the caller, per `weaver-admin-Spec` section 2.
What crosses is the command line the rule names and the one `lifecycle-answer` or
`lifecycle-refusal` object it prints, the floor's existing shapes, so this section
carries no request type for it. **The trace door has one**, and the boundary file and a
change's cause are the two other shapes the act adds.

```rust
pub struct BoundaryFile {
    pub trace_reader: String,
}

pub struct TraceRequest {
    pub offset: u64,
    pub prior_digest: Option<String>,
}

pub struct TraceHeader {
    pub device: u64,
    pub inode: u64,
    pub birth_ns: Option<i128>,
}

pub struct TraceLine {
    pub trace_stream: TraceControl,
}

pub enum TraceControl {
    Header(TraceHeader),
    Heartbeat { wall_ms: u64 },
    Truncated { size: u64 },
}

pub struct Cause {
    pub uid: u32,
}
```

**`BoundaryFile` is the agent's `roles.toml`, and it keeps only the trace door's
half**, on the operator's rulings of 2026-10-03 on #50 and #63. The lifecycle half
#63 drafted, a map of role groups to verbs, does not return: who may issue which verb is
the sudo rule's, the box's own boundary, and never a file admin reads. What stays is the
one `trace_reader` the trace relay admits, in a root-owned file in the agent's root,
`/etc/weaver/admin/<agent>/roles.toml`, required, an unknown key refused. **It is read
by `parse_boundary`**, behind the `config` feature beside `parse`, a total parse like
the declaration's, `fn parse_boundary(source: &str) -> Result<BoundaryFile,
ConfigError>`, kept apart from it because the file is boundary and never constitution,
per `weaver-admin-Spec` section 9. **The trace reader is a user, and only a user**, the
connector's own service user, on the operator's ruling of 2026-10-03 on #50: a group
would admit every member of it to the agent's whole record, and a bare name that could
resolve to a user and a group of the same name is what an explicit kind once guarded
against, so the kind is fixed instead. The relay admits that uid alone. **The name
resolves at `validate` and `load`**: a reader that is the agent's or its state member's
account refuses, per `weaver-admin-PRD` section 7's rule that no grant names the agent,
so the agent never reaches its own record through the boundary, and a reader that does
not hold the agent's access group `weaver-<agent>-admin`, which the trace socket is
grouped to, refuses too, a reader the filesystem would turn away before the relay's
credential check runs being a door that cannot open.

```toml
trace-reader = "weaver-<agent>-admincon"
```

**The door's framing is lines on a stream socket.** The reader sends exactly one
request: one `TraceRequest` as a JSON object on one line ending in a newline, at most
4096 bytes with the newline, within five seconds of the connection. A request that is
longer, ends without a newline, does not parse, or arrives late is refused and logged
and the connection closed, and the relay reads nothing after the newline. Every line
the relay writes ends in a newline: the header, the trace's own lines, which already
end in one, and each control line.

**`TraceRequest` names where the stream starts**: a byte offset that falls on a record
boundary, and the sha256 hex of the record that ends at that offset, the record's bytes
from the start of its line through its terminating newline, absent only at offset zero.
The relay refuses a position whose prior record does not hash to the digest, so a reader
resuming after a rotation or a truncation learns that its position no longer names the
record it read rather than receiving bytes from another file. **The stream is a
`TraceHeader` line and then the trace's own lines exactly as written**, on the
operator's ruling of 2026-10-03 on #63's fourth question: the header names the file's
identity, its device, inode and birth time in nanoseconds since the epoch, so a reader
holds what it is reading. **The birth time is absent where the filesystem reports
none**, on #73's fourth item: never zero, which would let a reused device and inode read
as the same file, and a reader holding no birth time to compare leans on the position's
digest, which a different file fails. At the end of the file the stream keeps following,
with a heartbeat line while idle. **Every line the stream adds is a `TraceLine`**, so
the reader in another repository parses one fixed shape: a JSON object whose one member
is `trace_stream`, holding a `TraceControl` externally tagged by case, which no trace
event carries, the envelope having no such member. The three lines are
`{"trace_stream":{"header":{"device":...,"inode":...,"birth_ns":...}}}` first,
`{"trace_stream":{"heartbeat":{"wall_ms":...}}}` while idle, and
`{"trace_stream":{"truncated":{"size":...}}}` when the run's file shrinks below the
stream's position, which leaves its identity unchanged. **No line reports the sink's
path naming another file**: the relay holds the run's file by descriptor and never its
path, which dies at section 5's open, so it cannot see a rotation, and a rotation
changes nothing it reads. A reader learns of one at the next run, whose header names the
new file's identity. **After `truncated` the stream ends**, and the reader resumes with
a new request from offset zero, so no follower stays positioned past the end of a file
that was rewritten. The check is the file's size against the stream's position, so a
file truncated and grown back past that position between two checks goes unseen until
the reader's next request, whose digest check then refuses the stale position, and that
regrowth is the check's blind spot, named rather than hidden. Every other line is the
trace's own, byte for byte.

**`Cause` is who changed the agent, and it is the uid sudo reports and nothing else**,
on the operator's ruling of 2026-10-03 on #50: admin reads it from sudo's own
`SUDO_UID`, or takes uid 0 for the operator at a root shell, and reads nothing from the
caller, neither an argument it chose nor its standard input, so the rule's command lines
stay fixed. Which person asked is WeaverWeb's record and never the agent's. It rides the
enter payload as `cause`, and the `Leave` and `Stop` directives as theirs, per section
4.2, and the harness records it on the `load`, `unload` or stop event, per
`weaver-trace-Spec` section 3, admin never writing the trace.

## 4. The organ wire vocabulary

**What this section owns is loop 0's definitions in full, plus the carriage of
anything the envelope carries,** which is a boundary the act of 2026-08-02
made visible and worth stating rather than leaving to be inferred. Loop 0's
four definitions are shaped here entire. `turn-frame` and `fault-report` are
shaped here only as far as the envelope carries them, their payload variants
and, for the frame, the election section 6 holds, because
`organ-envelope`'s representation is this document's. **The token trio is held
at section 4.4 and its encoding is not this document's**, a division the act of
2026-08-08 made after an earlier wording deferred both together. It rides the
decode socket, which is not an organ channel per `weaver-spu-PRD` section 13.2,
so the hot-path measurement elects what it looks like on the wire. What cases it
carries is the decode contract's determination and this crate holds it, because
this crate holds types rather than creating them. A later workflow reads that
boundary and knows which side its vocabulary lands on before it asks.

Four of the charter's thirteen definitions, then: the envelope every organ
channel carries, and loop 0's trio named for the loop whose traffic it carries
rather than for a sender, per the naming ruling of 2026-08-01. The nine that
stood when this sentence was written became thirteen with the label trio of
2026-08-19 and `tool-name` of the tool workflow, neither of which moves the
four.

### 4.1 The envelope

```rust
pub struct OrganEnvelope {
    pub exchange: ExchangeId,
    pub position: Position,
    pub payload: Payload,
}

pub struct ExchangeId {
    pub opener: Opener,
    pub ordinal: u64,
}

pub enum Opener {
    Admin,
    Harness,
    Spu,
    Gate,
}

pub enum Position {
    Open,
    Continue,
    Close,
}

pub enum Payload {
    Directive(LifecycleDirective),
    Answer(LifecycleAnswer),
    Refusal(LifecycleRefusal),
    Frame(TurnFrame),
    Fault(FaultReport),
    Tool(ToolExecution),
    ToolAnswer(ToolOutcome),
    ToolCancel,
}

pub enum RefusingOrgan {
    Spu,
    Gate,
}
```

**`Position` is three, per `weaver-organ-channel` section 1**, which defines a
message's position in its exchange as open or continue or close. A two-variant
reading would serve the minimal exchange, which opens and closes at once, and
would have nothing to say about a directive whose answer arrives after an
intermediate message, which the channel's own layer permits.

**`ExchangeId` is the opening party and an ordinal**, per the same section, and
`Opener` enumerates the four parties that hold an organ channel. Naming the party
rather than carrying a bare bit is what lets a capture read as itself, and the
enum grows when an organ does, which is a floor edit in the act that charters the
organ.

**The payload is a typed enum rather than octets, which is what makes the
encoding of 4.3 possible.** An earlier draft of this Spec carried
`payload: Vec<u8>` with a separate kind discriminant, on the reasoning that a
generic envelope would push each seam's vocabulary into the floor's signature.
The reasoning failed against the naming ruling: loop naming means the payload
types live on the floor already, so naming them here adds no dependency and
removes a layer of encoding. It also removed a defect, since octets inside a JSON
envelope render as an array of numbers by default, tripling the size and
destroying exactly the legibility the JSON election rests on.

**`Frame` is that rule's first exercise, from the token workflow's gate act of
2026-08-02.** The gate's turn exchanges cross the same organ channel the
lifecycle directives do, so the frame enters this enum rather than taking a
carrier of its own, and every consumer's match sees the addition, which is
what the rule below is for. **Its shape is elected, per the operator's ruling
of 2026-08-12: the frame carries its octets base64-encoded in one member, and
the measurement the earlier deferral waited on is not needed.**

```rust
pub struct TurnFrame {
    pub octets: String,
    pub dialer: Option<u32>,
}

pub struct TurnRequest {
    pub text: String,
    pub role: Option<serde_json::Value>,
}
```

`octets` is the line's octets encoded base64, both directions, one
definition per the charter. **`dialer` is the peer uid the gate read at accept**, as
of the operator's ruling of 2026-10-06 (#1): present on every frame the gate carries
inward, absent on a response, and absent from a line written without it so a frame
of before the member still parses, per `weaver-harness-gate-contract` section 2; the
harness admits the seeding line by it. **`TurnRequest` is the request line as the
harness parses it**, per `weaver-gate-world-contract` section 2: `text`, a string, and
at most `role` beside it, each member once, decoded typed rather than read as a value
because a value's map collapses a repeated member to its last spelling and would read
`{"text":"x","role":"user","role":"system"}` as the seeding line; a repeated member,
an unknown member and a missing `text` refuse by name, and `role` crosses as whatever
the line wrote, `null` included, for the harness to refuse every value but the string
`system`, naming it. The parse is `TurnRequest::parse`, which refuses a line that does
not open an object before the typed decode, since serde reads a struct from an array
by position and `["x","system"]` would otherwise read as the seeding line. The encoding is RFC 4648 section 4's standard
alphabet, padded, with no line breaks and no interior whitespace, and the
decode refuses input the encode would not produce, so one octet sequence has
exactly one carried form. The encoding's implementation rides with the type,
a floor obligation the wiring act lands, so one implementation holds the
canonical form for every party rather than two crates agreeing by luck. The
deferral offered two honest answers, a
splice of the line as it stands or an encoding that survives arbitrary
octets, and held them for a measurement over real client traffic. The
constraint set decides without one. The gate reads no frame, per its opacity
rule and the `gate-client-content-unread` assertion, so no party validates a
line before it crosses: the world contract names UTF-8 NDJSON as the
client's format and names the harness as the party that refuses a line that
does not parse, which admits arbitrary octets onto the seam by construction.
A splice therefore fails exactly when a client misbehaves, and it fails at
the wrong layer: an unparseable member makes the envelope undecodable, and
an undecodable envelope is a channel fault that ends service, so a hostile
line would convert a refused turn into a dead channel. The encoding that
survives arbitrary octets is the only answer left, and base64 is its boring
form: ASCII inside the JSON envelope, about a third larger than the line
rather than the tripling a numeric array costs, on a channel that carries
one frame per turn rather than the decode seam's per-token stream, so the
hot-path concern the deferral carried does not reach it. Encoding is
carriage rather than reading, a byte-blind transform parsing nothing, so the
gate's opacity holds. The harness decodes the member back to octets before
the parse question the gate's turn half owns, and what an undecodable member
means behaviorally is that act's to state, the representation alone being
this document's.

```graph
node: types-frame-survives-arbitrary-octets
kind: assertion
tag: perturbation

edge: asserts
from: weaver-types
to: types-frame-survives-arbitrary-octets
```

**`Fault` enters on different grounds and the difference is worth stating.**
It is not a loop's vocabulary, so the rule below does not reach it: a fault
report is what any organ hands the harness across whatever channel it holds,
and it enters this enum because the gate's channel is an organ channel and
the gate has no second socket to carry it. The decode socket carries the same
floor definition inside its own trio rather than inside this envelope, which
is what makes `fault-report` one definition with two carriages instead of two
definitions that would drift.

**A later loop's vocabulary enters this enum in the act that charters that loop,**
which is the same loudness the trio's own case sets carry: one owner, contracts
drawing rather than growing, and a floor edit every consumer's match then sees.

**`Tool` and `ToolAnswer` are that rule's second exercise, from the tool
workflow's opening act of 2026-08-17.** The execution exchange that
`weaver-harness-gate-contract` section 2 opens crosses the same organ channel
the raise and the lower cross, so the ask and the answer enter this enum rather
than taking a carrier of their own. Two cases rather than one carrying
a discriminant, on this Spec's standing rule that a name earns its own variant
where a reader would otherwise infer which direction ran, which is the shape
the trio's directive and answer already take. `ToolCancel` joined them on
2026-09-22, the cancel that contract's section 2 sends inside an open
execution at the continue position, a unit case because it carries nothing
beyond its kind, and the first traffic in this program to take that position.

```rust
pub struct ToolExecution {
    pub name: ToolName,
    pub arguments: String,
    pub clock_ms: u64,
}

pub enum ToolOutcome {
    Result { content: String },
    Refused { reason: String },
    Errored { detail: String },
    Killed { partial: Option<String>, by: KillCause },
}

pub enum KillCause {
    Clock,
    Cancel,
}
```

**The ask carries what the family parse recovered and the harness interprets
none of it**, per that contract's section 2: the tool's name as the model
spoke it, the arguments as the emission held them, and the caller's clock.
`ToolName` is the definition the parse mints, drawn by that contract's
vocabulary clause and defined at `weaver-types-PRD` section 2.3. The clock is
counted in milliseconds, which is this definition's election, the contract
fixing the rule that one clock governs and not the unit it is read in.

**The answer's four contents are the contract's and no case is this
document's**, told apart by tag alone, the rule beneath the four being who
speaks in the return: a result is the tool's own words, a refusal is the
gate's voice with nothing run, an error is the machinery's, and a kill carries
no tool voice by construction, the output drained before the kill riding as an
attachment rather than as a result. Every one of them is content rather than a
channel fault, because each is a fact the model must learn. Since 2026-09-22 a
kill names what ended it, the clock or the caller's cancel, in a field rather
than a fifth case, because the rule beneath the four is who speaks and nobody
speaks in either kill: one content with two causes, so the record can tell a
tool that ran out of time from one the operator stopped.

**The two definitions are owed to the charter and no act has carried them.**
`weaver-types-PRD` section 2.3 admits a wire definition when a contract draws
it, and `weaver-harness-gate-contract` section 7 draws `tool-name` and nothing
else of this exchange, so the ask, the cancel, and the answer are represented here while
the vocabulary nodes they would map to do not exist. Declaring them in the
charter on this document's demand would invert the direction the corpus runs
in, so the owing is a contract act reaching both parties to that seam rather
than this one, and it is named here so a reader does not read the silence as a
ruling.

### 4.2 The trio

```rust
pub enum LifecycleDirective {
    Enter { payload: Box<EnterPayload> },
    Leave { cause: Cause, forced: bool },
    SavePoint { cause: Cause },
    Stop { cause: Cause },
    Observe,
    Admit { instruction: SpuInstruction },
    Release,
    Raise { instruction: GateInstruction, socket: PathBuf },
    Lower,
    Load { agent: AgentName },
    Unload { agent: AgentName },
    Validate { agent: AgentName },
    Show { agent: AgentName },
    SavePointVerb { agent: AgentName },
    Restore { agent: AgentName },
    ForceUnload { agent: AgentName },
}

pub enum LifecycleAnswer {
    Ready,
    Left { save_point: Option<SavePointReport> },
    SavePointTaken { report: SavePointReport },
    RestoreNamed { save_point: String, name: String },
    TurnAborted { turn: TurnKey },
    AtRest,
    Admitted,
    Released,
    GateReady,
    GateStopped,
    Validated,
    State { state: AgentState, load: Option<Box<LoadFacts>>, constituents: Vec<u32> },
    InTransition,
}

pub struct SavePointReport {
    pub save_point: String,
    pub name: String,
    pub run: RunId,
    pub sequence: u64,
    pub turn: u64,
    pub position: Sequence,
}
```

**`Leave` carries `forced`, `SavePoint` is the on-demand save point, and `Left` names
the leave's save point**, as of A3.2 on the operator's rulings of 2026-10-06 on #1:
`SavePointReport` is what the harness reports of a finished save point, its digest, the
finished name the member gave it, the position it covers and the trace position of the
`save_point` event, so admin's manifest records the event's position without reading
the record; `Left` carries none where the leave was forced or the binding diagnostic.
`RestoreNamed` is the `restore` verb's answer, the save point it judged and entered in
the manifest. The three verbs mirror the command line as `SavePointVerb`, `Restore` and
`ForceUnload`, the first named apart from the directive the worker receives.

**`Enter`'s payload and `State`'s load are boxed**, as of 2026-09-06 per issue #475:
the two variants carried the whole of their enums' size onto every unit variant, four
hundred bytes on the directive and two hundred and eighty on the answer, on the two
channels a status read goes through since the observation exchange of 2026-09-04. A
boxed field crosses this seam as the field does, the serialization treating the box as
its content, so no contract's stated shape moves, and `OrganRefused`'s boxed reason
below is the same demonstration made earlier for a different reason.

```rust
pub struct LoadFacts {
    pub session: SessionId,
    pub run: RunId,
    pub declaration: String,
    pub artifact: ArtifactRef,
    pub residual_readout: bool,
    pub field: Option<u32>,
    pub surprisal: bool,
    pub state_election: StateElection,
    pub state_store: StateStore,
    pub state_member: bool,
    pub composer: Composer,
}

pub struct Composer {
    pub binary: String,
    pub file: Option<PathBuf>,
    pub sha256: Option<String>,
}

pub enum LifecycleRefusal {
    Unauthorized,
    Malformed,
    NoSuchAgent,
    CarriedWork,
    OutOfOrder,
    DescriptorsUnusable,
    BoundaryUnverified,
    ConfigInvalid { field: Option<FieldName> },
    ArtifactUnresolvable,
    ArtifactUnreadable,
    DeviceCannotAdmit,
    NoResidency,
    BindFailed,
    AgentRunning,
    InvocationInFlight,
    LockHolderUnknown,
    WorkerWouldNotExit,
    Unanswered,
    OrganRefused { organ: RefusingOrgan, reason: Box<LifecycleRefusal> },
    ActivityNotAtRest,
    SavePointNotTaken { missed: SavePointLeg },
}

pub enum SavePointLeg {
    Write,
    Answer,
    Finished,
    MemberDead,
    Published,
}

pub struct EnterPayload {
    pub session: SessionId,
    pub run: RunId,
    pub spu_instruction: SpuInstruction,
    pub binding: EnterBinding,
    pub state_election: StateElection,
    pub state_store: StateStore,
    pub restore: Option<Lineage>,
    pub reset: Option<Reset>,
    pub stack: BTreeMap<String, String>,
    pub declaration: String,
    pub boundary: String,
    pub cause: Cause,
    pub operator: u32,
    pub library_path: Option<String>,
}

pub struct Lineage {
    pub save_point: String,
    pub run: RunId,
    pub sequence: u64,
    pub turn: u64,
    pub named_at_restore: bool,
    pub built_from: Option<Branch>,
}

pub struct Branch {
    pub parent: SessionId,
    pub run: RunId,
    pub through: u64,
}

pub struct Reset {
    pub prior_run: RunId,
    pub reason: ResetReason,
}

pub enum ResetReason {
    NoCleanUnload,
    ForcedUnload,
}

pub enum EnterBinding {
    Serving { gate_instruction: GateInstruction },
    Diagnostic,
}

pub struct FaultReport {
    pub case: FaultCase,
    pub account: Box<serde_json::value::RawValue>,
}

pub enum FaultCase {
    DeviceFaultDuringGeneration,
    ResidencyDegraded,
    ReadoutFaultWhileElected,
    ListenerLost,
    ClientConnectionFailedMidTurn,
    AdmissionFailingSystematically,
    RecorderCommitPressure,
    StreamWriteFailed,
    OrganDeathObserved,
    MessageRecordUndecodable,
    IdentityPrefixUnrecorded,
}
```

**`state_store` rides the enter resolved, beside the election, as of
2026-09-04.** The config holds it as an option whose absence means the
embedded engine, per section 2, and the payload holds it as admin resolved it
at inventory, so the harness never re-derives an absence and the load event it
authors names the engine, per `weaver-trace-PRD` section 3.1. `database` and
`role` are absent from the resolved value, the cross-field rule having been
judged before any process existed, and the member's own vector carries no
engine word, per `weaver-admin-Spec` section 6, one engine standing.

**`restore` and `stack` ride the enter as of 2026-09-04.** `restore` rides as `Lineage`,
resolved by admin from the save point the load restores, on the operator's ruling of
2026-10-02 on #58: the save point's digest, the run, sequence and last turn it covers,
whether it was named at a restore (`named_at_restore`, which replaced
`operator_supplied` with A3.2 on the operator's ruling of 2026-10-06 on #1 that there is
no operator-supplied save point), and, where the offline builder made it from a record,
that record's session and the run and turn of its cut. **`reset` rides beside it and
apart from it, present where the agent's last run did not end in a clean unload**,
whether or not any save point stands, because a run that stopped before its first save
point still owes the record its reset, on the operator's ruling of 2026-10-02 on #58
that an unclean stop resets to the latest known-good save point and records the reset:
admin resolves it from its own clean-unload marker, per `weaver-admin-Spec` section 4,
naming the prior run and the reason, `NoCleanUnload` where the marker says the run never
unloaded cleanly, `ForcedUnload` where admin's `force-unload` ended it without its leave
save point (A3.2), `UnitFailed` having retired with the unit it read on 2026-10-03 (#50),
and the harness authors the reset event from it. It never carries the save point's path,
which admin read under its own custody and the harness has no business holding, on the
same discipline as the sink. The harness names the save point on the load event, its
last turn a recorded fact that numbers nothing, a restoring run's turns starting at one
per `weaver-harness-Spec` section 6.1, without opening anything, the save point having
reached the member by descriptor at the spawn per `weaver-admin-Spec` section 6. `stack`
is the digests of the organ binaries admin started and of the agent's SPU and the gate
it hands the worker to fork, keyed by the binary's name, so the load event names the
stack that ran it and a record is sufficient for its own conditions without a deposit
beside it, per `weaver-trace-PRD` section 3.1. Both are admin's facts and the harness
authors them as it authors the store's. **`declaration` rides beside them as of
2026-09-04**, the digest of the declaration file as admin read it at the inventory, so
the harness names it on the load event and answers it to an observation without holding
the file, per issue #435. **`boundary` and `cause` ride beside them as of 2026-10-03**,
on the operator's rulings on #50 and #63: `boundary` is the sha256 hex of the agent's
`roles.toml` as admin read it at the inventory, which the harness copies onto the load
event as a member marked boundary and never constitution, so each run's record declares
who could read it without the boundary file joining the tuple or the declaration's
digest. `cause` is section 3.1's `Cause`, which the harness records on the load event,
as the `Leave` and `Stop` directives' own causes are recorded on the unload and stop
events. **No prompt rides the enter, in bytes or by digest**, on the operator's ruling
of 2026-10-06 (#1): the system prompt is state, entered through the gate and held by
the member, so the enter names no file and admin reads none, and the record of which
prompt was in force is the `message.system` event the seeding turn authors, per
`weaver-trace-Spec` section 3. **`operator` rides the enter as of 2026-10-06**, the uid
the root's `operator` key names, per `weaver-admin-Spec` section 9, so the harness can
admit the seeding line from the operator's peer identity alone and refuse it from every
other dialer, per `weaver-gate-world-contract` section 2 and `weaver-harness-Spec`
section 6.1; it is admin's fact, copied from the root and read from no deployment by
the harness.

**`EnterBinding` is the kind resolved, and a directive disagreeing with its
kind is unrepresentable rather than refused.** The config holds the kind as
the operator may state it, an option whose absence means serving, and the
payload holds it as admin resolved it, so the resolution point is visible in
the types: what crosses the seam has already been decided. The gate
instruction rides inside the serving case, which answers the question
`weaver-admin-harness-contract` section 8 left to this round: no refusal
catches a directive whose members disagree with its kind, because the shape
leaves no such directive to construct. A diagnostic enter has no field for
the instruction and a serving enter cannot omit it, so the wrong pairing is
a struct that does not exist, on the same ground as the one-case outcome of
`weaver-harness-Spec` section 3. The grouping is representation rather than
a term of its own, per the standing rule that the draws name the definitions
and not the grouping, and the vocabulary node stays `binding-kind` alone.

```graph
node: types-enter-binding-disagreement-unrepresentable
kind: assertion
tag: compile-pin

edge: asserts
from: weaver-types
to: types-enter-binding-disagreement-unrepresentable
```

**`AgentRunning` replaces `PriorUnitUnreaped`**, on the operator's ruling of 2026-10-03
on #50 that the agent leaves systemd. `PriorUnitUnreaped` named a failed unit whose name
the manager still held, and with no unit there is no name to hold. What a second `load`
meets instead is the run lock of `weaver-admin-Spec` section 3, held by the run's
processes and released by the kernel when the last of them is gone, so the refusal says
one thing and no more: a run of this agent holds the lock now, and the load touched
nothing, a load never ending an existing run. It does not say the run is serving,
healthy or ever entered, which `show` answers through the observation exchange, and
ending it is `unload`'s, which the caller issues. A run that died leaves no lock behind,
so no case for a dead one is needed and none is kept.

**`State` carries the run's constituents where admin found any**, per
toddwbucy/WeaverWeb#15: every pid holding the run lock's description, the worker, the
state member and the relay, which `show` adds from the holder scan the escalation
already runs, so a caller can check each sits in its own containment. It is admin's
fact and never the harness's, absent from the observation the harness answers and
wherever no run holds the lock, and empty is absent on the wire.

**`InTransition` is an answer, not a state**, on the same ruling: `show` answers it
where another invocation holds the agent's invocation lock, a load or an unload in
flight, per `weaver-admin-Spec` section 3. It claims no `AgentState`, apex section 6's
four states being the harness's to know and the harness being busy with that very
invocation, so the caller polls again.

**Four more cases join it on the same ruling, each one fact.** `InvocationInFlight` says
another invocation holds this agent's invocation lock, so this one touched nothing, per
`weaver-admin-Spec` section 3. `LockHolderUnknown` says the run lock was held and no
process holding it could be found, the description having been carried where no
descriptor table shows it, so no signal was sent. `WorkerWouldNotExit` says a
constituent of the run still held the run lock after the unload's escalation, so the
agent was not reported unloaded. `Unanswered` says a stop's or an observation's answer
did not arrive within its bound, at `stop`, at `show`, or at a `load` meeting a silent
run, which never ends it, per `weaver-admin-Spec` section 3, so the run was left
as it stands and the invocation lock released. None claims more than its fact, and each
tells the caller to read the agent's state with the next `show`.

**`FaultReport` is two members, and the custody rule of apex section 5.2 is
the whole argument for the split.** The `case` is what the harness itself
consumes: the judgment of what a fault means for the turn and the residency is
the harness's to make, so the fact it judges is typed, exhaustive, and closed.
The `account` is the reporting organ's own rendering of what happened, opaque
here by construction: a raw value crosses verbatim and splices verbatim, so
the floor carries no organ's descriptive vocabulary and a later organ's richer
account grows no shared type. This is the standing exception of apex section
5.2 corrected in the act that corrects it, the fault path now running the same
direction as the generation measurement.

**The case set is the three charters' closed enumeration and no case is this
document's.** Three from `weaver-spu-PRD` section 13.10, three from
`weaver-gate-PRD` section 13.4, five from `weaver-harness-PRD` section 5,
which closed the corpus-wide set across all three organs for the seams that
exist. A twelfth case is a charter act before it is a code change, and the
charter has widened twice under that rule rather than the enum widening
first: the code act that typed these found the harness one short of its own
crate's standing practice, the assembly fault, and the act answering
issue #369 found it one short again, the seated prefix the record cannot account
for, which is the further-case rule exercised in the acts that stated it
rather than a rule waiting for its first test. And the
harness's own five ride the same shape although they cross no socket, because
the same shape serves the wire and the `fault` event's payload and electing it
twice would be two shapes for one fact.

**The reporting organ names its own case, and the harness classifies
nothing.** The report arrives whole, per the contracts' wording that an
emission carries a `fault-report` naming a case of its charter's enumeration,
so a case exists before any `fault` event is authored and no raw account ever
reaches the record wrapped without one. A report the harness would have to
diagnose before it could file it would put the judgment on the wrong side of
the seam, the organ being the party that knows.

**No member names the raiser and no member names the turn.** Which subsystem
raised a fault is the trace envelope's field, per `weaver-trace-PRD` section
3.2, and which turn it belongs to travels as the seam's context per apex
section 5.2, so a member for either would be one fact in two places. The
payload carries the what and never the where.

**All three enums are exhaustive, and the absence of `#[non_exhaustive]` is the
loudness this section claims.** The attribute would force every out-of-crate
consumer to carry a wildcard arm, and a case added later would land in that
wildcard silently, which is the opposite of the property the naming ruling bought.
Exhaustive, a new case breaks every consumer's match at compile time, in the same
act that edits the floor, and every contract's drawn subset is re-read by a human
because the compiler made them look.

```graph
node: types-wire-enums-exhaustive
kind: assertion
tag: compile-pin

edge: asserts
from: weaver-types
to: types-wire-enums-exhaustive

edge: grounds
from: types-wire-enums-exhaustive
to: axiom-contract-is-a-complete-interface
```

An earlier draft of this Spec carried the
attribute beside this same argument, which the review seat caught as the
contradiction it was.

**One directive type for loop 0, carrying every case that crosses any of its four
seams, and each contract's vocabulary clause names the subset that crosses its
own:** enter, leave, and stop at coordination, admit and release at residency,
raise and lower at the gate, and the verbs with the observations at the operator
surface. The answer and the refusal follow the same rule.

**Every directive receives exactly one answer, and the mapping from directive to
answering case is stated because the operator contract requires that and a builder would
otherwise invent it.** The case is determined per directive, and for `Stop` by what it
interrupted. `Enter` answers `Ready`, `Leave` answers `Left`, `Stop` answers
`TurnAborted` or `AtRest`, `Admit` answers `Admitted`, `Release` answers `Released`,
`Raise` answers `GateReady`, `Lower` answers `GateStopped`, `Validate` answers
`Validated`, `Load`, `Unload`, `Show`, and `Observe` answer `State`, the last carrying
the load's facts beside the state where a run stands. Eleven of the twelve have a
single answering case. `Stop` has two, `TurnAborted` or
`AtRest`, selected by whether a turn was in flight, and both are clean closes rather
than a refusal, per `weaver-admin-harness-contract` section 3. Any directive may answer
a `LifecycleRefusal` instead, which is the second half of what one answer per request
means. `Validated` exists because validation reports an outcome without transitioning
anything, per `weaver-admin-PRD` section 4.3, and answering it with a state would report
a transition that did not happen.

**A receiving party matches its own cases and refuses the rest as `OutOfOrder`,
which is a real obligation rather than a formality.** The gate receiving an
`Admit` is not a case the gate implements, and the contracts already rule that a
directive out of order for the channel's state is refused and not queued. The enum
being wide is what makes that refusal expressible in the type the seam already
carries, and the per-seam test of section 5 is what confirms each party wrote the
refusing arms rather than a wildcard.

**`OrganRefused` boxes an inner refusal because the aggregate carries it
unchanged.** `weaver-admin-harness-contract` section 6 requires a refusing organ's
reason to reach admin without translation, so the harness wraps rather than
re-encodes, and the box is what keeps the enum's size from being set by its
deepest case. **It names a `RefusingOrgan` rather than an `Opener`**, because
only the SPU and the gate refuse inside a fan-out: admin does not refuse to
itself and the harness returns the aggregate rather than appearing inside it, so
reusing the four-case `Opener` would let a well-typed aggregate claim that admin
refused as an organ.

**The refusal cases are drawn from the merged contracts and the set is closed at
this crate.** Whether the SPU's admit cases needed a type of their own was the
cell `weaver-spu-PRD` section 10 held, and the naming ruling settled it as
extension: they are loop 0 refusals because they refuse loop 0's directives.

**`StateNotObservable` joined the set on 2026-08-06 and left it on 2026-09-04.** It
refused a question rather than an act, because the party that knows an agent's lifecycle
state is the harness and no exchange asked it, and it was minted as a marker with a
scheduled death, leaving exactly one thing to delete when the observation exchange
landed. The exchange landed with issue #435: `weaver-admin-harness-contract` section 3
charters `Observe`, the harness answers `State` from whichever position it holds with
`LoadFacts` beside it where a run stands, and `show` answers rather than refuses. The case is deleted rather than kept, per the apex's rule that a case nothing
produces is a reserved slot.

**`LoadFacts` is what the observation carries beside the state, and it overlaps what the
`load` event names without being its shape**: the session and run, which the event
carries in its envelope rather than its payload, the declaration's digest as admin read
the file at the enter, the artifact, which the event does not carry, the readout, field,
and surprisal elections, the tee's election under the name `state_election`, where the
event spells it `tee`, the store the member stands on and whether its end arrived, and
the composing loop by binary and, where it is a file, path and digest. It lacks the
event's stack, lineage, reset and prompt digest, the last for the reason the enter
payload's clause above gives, so a consumer storing both stores two shapes, and the
rework of `show` (#52 and the load confirmation) is where a shared core is weighed.
`Composer` is this crate's spelling of the loop's identity because the record's own is
`weaver-trace`'s and the floor links downward only.

### 4.3 The encoding

**Loop 0's traffic is JSON, one envelope to one message, elected against the
charter's criterion.** `weaver-types-PRD` section 2.3 states the ground: this
traffic is low in volume, so compactness buys nothing measurable, and diagnostic
in audience, read from a capture when a load refuses unexpectedly.

**The instrument is review and the non-purchase is named rather than assumed.**
A perturbation wants a mutation that removes the property, and this crate holds
nothing to mutate: it defines the types and encodes nothing, the write and the
read belonging to the pair-creating crates per section 5, and no derive or
attribute here carries the encoder's identity. `serde_json` enters this crate's
source as a type, `RawValue` at three members, and never as a call. So there is
no one change here that leaves loop 0's traffic being something other than
JSON. What was taken in its place is the layout pin of
`envelope_layout_is_the_stated_shape`, which holds the envelope's octets exactly
as this subsection states them and goes red on any change to them, so the
election's consequence is watched while the election itself stays review's.
Whether that split should become two records is section 6's election.

```graph
node: types-loop0-encoding-json
kind: assertion
tag: review

edge: asserts
from: weaver-types
to: types-loop0-encoding-json
```

**The tagging follows the same mechanical test `weaver-traits-Spec` section 3
states, and the two floor Specs share it so they cannot drift.** A fieldless enum
is a plain renamed string. An enum whose every variant is struct-shaped or wraps
a struct is internally tagged. An enum with any variant wrapping a primitive, a
sequence, or another tagged enum is adjacently tagged, because internal tagging
cannot represent those shapes and fails at serialization rather than at compile
time. **The same arm takes an enum with a variant wrapping a struct that
carries a spliced member**, and this clause is the trio's code act finding a
failure the rule's rationale did not name: internal tagging buffers the
content on the read side and the buffer cannot represent pre-serialized JSON,
so the round trip fails at deserialization rather than at serialization,
measured in this workspace as an invalid-type error at the splice. A wire type
one party can write and the other cannot read is not a wire type, so the arm
extends to the read side's failures rather than the write side's alone.

```graph
node: types-tagging-test
kind: assertion
tag: perturbation

edge: asserts
from: weaver-types
to: types-tagging-test

edge: asserts
from: weaver-traits
to: types-tagging-test
```

Applied here: `Position`, `Opener`, `RefusingOrgan`, and `FaultCase` are
fieldless and serialize as plain renamed strings, the rename being
`snake_case` as everywhere in this vocabulary, so a case crosses as
`residency_degraded` and never as its Rust spelling. **The trio is internally
tagged**, which is why every case carrying a value in 4.2 takes a struct
variant, `Load { agent }`
rather than `Load(AgentName)`. **`Payload` is adjacently tagged**, `#[serde(tag =
"kind", content = "body")]`, because its variants wrap enums that carry a tag of
their own. **The token trio of section 4.4 divides under the same test**:
`TokenDirective` and `TokenRefusal` are internally tagged, their value-carrying
cases struct-shaped, `Finish` is fieldless and a plain renamed string, and
`TokenAnswer` is adjacently tagged under the spliced-member arm, two of its
cases wrapping the vocabulary's two `RawValue` carriers: `Generation`, whose
`request` and `measurement` are both spliced, and `FaultReport`, whose
`account` is. **`ToolOutcome` divides the same way and is internally tagged**,
every variant struct-shaped and none wrapping a primitive, a sequence, or a
tagged enum, so the second case holds. Its tag member is `outcome` rather than
the `kind` the trio carries, an election naming what the tag tells apart: the
body is one execution's outcome rather than a case of a loop's set.

**The envelope's layout is stated rather than left to a reader.** Nothing is
flattened. `exchange`, `position`, and `payload` are three members of one object,
and the adjacent tagging nests the payload's own tagged object under `body`, so
no two layers can contribute one key:

    {"exchange":{"opener":"admin","ordinal":7},
     "position":"open",
     "payload":{"kind":"directive","body":{"kind":"load","agent":"alpha"}}}

**Both halves of this election were verified against serde 1.x rather than
reasoned from memory, because an earlier draft got both wrong.** That draft
tagged `Payload` and the trio with the same `kind` name, which emits two `kind`
members into one object and fails to deserialize with a duplicate-field error,
and it used newtype variants, which fail at serialization for any case wrapping a
name or a sequence. The wire shape section 0 claims this Spec settles has to
round-trip, and the shape above does, including the sequence-carrying cases.

**This election does not reach the decode seam and must not be read as reaching
it.** Decode traffic is the hot path, its volume is per token rather than per run,
and its encoding is the token workflow's election with a measurement behind it.
Decode does not share this channel, per the decoder-cut ruling of 2026-08-02
recorded at `weaver-spu-PRD` section 10: it takes its own socket, owned by that
crate, so the head-of-line question this sentence once held open is closed.

**The envelope is length-free, because the socket type carries boundaries.**
`weaver-organ-channel` section 2 requires one write to be one read and rules that
the property comes from the socket type rather than from framing above it, leaving
the type to the Spec. The election is `SOCK_SEQPACKET`: it preserves message
boundaries and connection semantics together, where `SOCK_DGRAM` on a Unix socket
would preserve boundaries but invite the datagram reading that document explicitly
disclaims, and `SOCK_STREAM` would push a length prefix into every contract that
draws the channel, which is the layer violation that document exists to prevent.

**The election binds the crates that create the pairs, and it is owed to their
Specs.** This crate opens no socket. `weaver-admin-Spec` creates the coordination
pair and `weaver-harness-Spec` creates the residency and gate pairs, per their
charters, and each carries this election rather than re-deciding it. Naming the
landing sites here is what keeps a decision made in one Spec from reading as
settled everywhere while binding nothing.

**One write is one read only while the reader's buffer covers the message, so the
election arrives with an obligation.** A short read on `SOCK_SEQPACKET` truncates
silently and discards the remainder unless the receiver checks `MSG_TRUNC`. The
receiving crates therefore size their buffer against a **maximum envelope of 64
kibibytes** and treat a truncation flag as a channel fault rather than a message,
because a silently shortened directive is the failure mode the boundary property
was elected to prevent. The bound is generous against loop 0's traffic, whose
largest case is an enter payload of identifiers, and it is stated as a number so
that a builder sizing a buffer has one to use.

```graph
node: types-socket-seqpacket
kind: assertion
tag: review

edge: asserts
from: weaver-types
to: types-socket-seqpacket

edge: grounds
from: types-socket-seqpacket
to: axiom-floor-is-vocabulary-behavior-is-socket

node: types-envelope-bound-64k
kind: assertion
tag: review

edge: asserts
from: weaver-types
to: types-envelope-bound-64k

edge: grounds
from: types-envelope-bound-64k
to: axiom-floor-is-vocabulary-behavior-is-socket
```


### 4.4 The token trio

**This crate holds types rather than creating them.** What shape a type takes is
determined by the demand that construction puts on it, and the party under that
demand is the one that determines it. The decode contract's sections 2 and 5 are
that determination for the token trio, and this subsection is where the floor
holds the result. The charter said as much when it declared the trio: the cases
are the decode contract's enumeration, one owner and one drawer today.

**The trio is held here now because the demand exists now.** Section 9 of
`weaver-spu-Spec` requires the decode seam to answer a `token-refusal` and the
crate cannot write one, and apex section 3 steps 5 and 6 put the seam inside the
turn the program is built to complete, so no loop 1 avoids it however simple. An
earlier reading of this section deferred the shape along with the encoding. The
two come apart, and the deferral belongs to the encoding alone.

```rust
pub enum TokenDirective {
    Open { session: SessionId, messages: Vec<Message>, column_ask: bool },
    AppendAndGenerate {
        turn: TurnKey,
        delta: Vec<Message>,
    },
    ReFeed {
        turn: TurnKey,
        rendered: String,
        path: Vec<u32>,
    },
    Cancel { turn: TurnKey },
    Flush { keep: u64 },
    Elide { from: u64, to: u64 },
}

pub enum TokenAnswer {
    Opened,
    Token { token: u32, piece: String },
    Field { position: u64, ranked: Vec<Candidate>, realized: u32 },
    Column { position: u64, layers: Vec<Vec<f32>> },
    Generated(Generation),
    ReFed(Generation),
    AtRest,
    Flushed {
        resident_before: u64,
        resident_after: u64,
    },
    Elided {
        resident_before: u64,
        resident_after: u64,
    },
    Fault(FaultReport),
}

/// What a seam turned away, as the record carries it.
///
/// **The seam is named and the case is the seam's own.** A consumer
/// dispatches on the variant, which is what typing this here buys over a
/// reason field the trace would have to carry as prose.
pub enum RefusalRecord {
    Decode { asked: TokenAsk, refusal: TokenRefusal },
    Lifecycle { asked: LifecycleAsk, refusal: LifecycleRefusal },
    Classify { refusal: LabelRefusal },
}

/// Which ask a decode refusal answered, so the record says what was
/// refused and not only why.
#[serde(tag = "ask", rename_all = "snake_case")]
pub enum TokenAsk {
    Open,
    AppendAndGenerate,
    ReFeed,
    Cancel,
    Flush { keep: u64 },
    Elide { from: u64, to: u64 },
}

/// Which lifecycle ask a refusal answered.
#[serde(tag = "ask", rename_all = "snake_case")]
pub enum LifecycleAsk {
    Enter,
    Leave,
    Stop,
    Observe,
    Admit,
    Release,
}

pub enum TokenRefusal {
    NotOpen,
    OutOfOrder,
    Overflow { resident: u64, requested: u64, capacity: u64 },
    MalformedDelta,
    UnremovableSpan { from: u64, to: u64, prefix: u64, resident: u64 },
    RefeedPermissionAbsent,
    RefeedPathEmpty,
    ColumnPermissionAbsent,
    ColumnReadoutUnelected,
    ColumnUndeclared,
}
```

**`RefusalRecord` names the ask beside the refusal, and carries a value only
where the record holds it nowhere else.** A refusal alone says why a party
said no. What a diagnostic reader works backwards from is what was asked, so
the variant always says which directive was refused. **What the variant does
not do is reproduce the directive**, and the rule deciding that is one rule
rather than a judgment per case: **a value rides the ask when no other event
carries it, and is named and left alone when one does.**

Applied to the decode seam:

```
Open                  the messages are the identity prefix, and
                      message.system carries them since the prefix act
AppendAndGenerate     the delta is authored as the turn's message kinds
                      before the exchange, so a refused append still has
                      its content in the record
Cancel                the turn is the envelope's field on every event
Flush { keep }        carried: no event holds the cut a flush asked for
Elide { from, to }    carried: no event holds a span that was refused,
                      the elision event recording only removals that
                      happened
```

**Completing the three would be duplication rather than fidelity**, one fact
in two places with no authority named, which this corpus files as a defect.
The record is not lossless about the ask and does not claim to be: it is
complete about the ask's identity and about the values that would otherwise
be lost.

**Classify carries no ask at all**, which is the same rule at its limit. Its
directive is the content, `classify.request` holds it under its own kind,
and nothing of the ask would otherwise be lost.

**`LifecycleAsk` follows the rule and has a gap the rule cannot close.** The
directive is named and the declaration is not reproduced, the load event
carrying the run's posture.

**An enter refused before its bracket is established has no run record**,
because the `load` event is what opens the run and a refusal reaching that
far has not authored one. There is no run to author the refusal into and no
later event that could carry it, whether or not a sink descriptor was taken
before the refusal fell. **The refusal reaches the operator's answer and
nothing else**, which is unchanged by this act and is the whole of what such
a refusal leaves behind. **That is a hole this act does not fill**, named
here so a reader does not take the lifecycle arm as covering it.

**The three arms are the seams that produce typed refusals**, per the
inventory of 2026-08-22: the decode seam, the lifecycle seams which share
one vocabulary, and the classify seam. The gate's `RaiseRefusal` is absent
because it crosses no seam, reaching the harness as
`LifecycleRefusal::BindFailed`, which the lifecycle arm already carries.

**`Elide` names a half-open span and `Flush` names a length, which is the
difference between removing an interior and shortening a tail.** The bounds
are resident positions, `from` inclusive and `to` exclusive, and the pair
describes what leaves rather than what stays: the flush's `keep` says what
the session returns to, and a span says what the session loses. **They take
opposite rules at their edges**, per the decode contract as amended
2026-08-22: an over-large `keep` bounds, and a span describing no removable
region refuses, because a smaller true version of the first exists and of
the second does not.

**`Elided` carries the same pair `Flushed` carries** and for the same
reason: the SPU is the one authority on either count and the harness
authors the record's event from exactly them. Two answers of one shape are
two facts rather than one, so they take two cases rather than a shared case
with a discriminant, on this Spec's standing rule that a name earns its own
variant where a reader would otherwise infer which operation ran.

**The span is state.** These positions index the resident sequence the SPU
holds, never the trace, and no directive on this seam removes anything from
a record.

**The tunable map left this directive in the act that routed it to the
declaration**, per `weaver-spu-Spec` section 8, so nothing sampling-related
arrives with a turn and the value discipline that guarded it moved with it to
section 2. What remains here is a turn's delta, and a delta this seam cannot
serve is `MalformedDelta` as before.

**`Message` is `weaver-traits`' and is drawn rather than restated.** The decode
contract's canonical messages are that type, shaped at `weaver-traits-Spec`
section 3 as a role and its content blocks, and this crate already links that
one as its floor-link. An earlier wording of this subsection named a
`CanonicalMessage` that exists nowhere, which is the naming slip a first reading
of the contract's prose invites: the contract describes the messages as
canonical and does not rename the type.

**`Generation` shapes what the harness reads and splices what it forwards.** The
decode contract's section 2 determines the answer as the generation and its
measurement, the measurement being the token identifiers, the per-token signals,
the timings, the block partition, and the residual reductions where the
residency was admitted with readout elected. The template identity is the
request's member and not the measurement's, per the ruling of 2026-08-17
on #129, and this sentence restates the contract's enumeration rather than
owning it, so it moves when the contract moved.

```rust
pub struct Generation {
    pub emission: String,
    pub finish: Finish,
    pub content: Vec<weaver_traits::ContentBlock>,
    pub request: Box<serde_json::value::RawValue>,
    pub measurement: Box<serde_json::value::RawValue>,
    pub resident: u64,
    pub capacity: u64,
}

pub struct Candidate {
    pub token: u32,
    pub probability: f32,
}

pub enum Finish {
    Completed,
    Stopped,
    Length,
}
```

**The split is between what the harness consumes and what it carries through, and
`weaver-trace` states the rule it follows:** what is shaped in a crate is what no
other crate defines. The emission, the content, and the finish are consumed
here: the emission is the verbatim the record holds, the content is the
canonical parse the working structure takes as the assistant's message and the
harness dispatches from, and the finish closes the turn, so all three are
shaped. The measurement and the rendered prompt are
consumed by nothing on the way past and are two members rather than one: each
is forwarded whole into its own box, the measurement into the measurement
event and the rendered prompt into the request event's `rendered`, because one
spliced member per box is what keeps the splice a move and never a transform.
A rendered prompt inside the measurement would reach a box whose accepting
shape carries no such member, and the harness would have to open the splice to
remove it, which is the transform the splice exists to refuse. **Seven of its
satellites are `weaver-trace`'s** and one is `weaver-spu`'s, against a crate that
links one internal dependency, so shaping it here would restate seven types that
already exist with no named authority over either copy, which is the duplication
G5 files as a defect rather than resolves by picking.

**`content` is the canonical parse and it crosses beside the verbatim
emission**, from the tool workflow's opening act. It is the family module's
bridge from what the model spoke to the conversation's blocks, text as text and
every recovered call as a `ToolCall` block, in emission order. The decode
contract's section 4 has the harness author both the verbatim emission and the
canonical parse into the record, so both are members here rather than the
harness re-parsing what the family module already read, and family knowledge
stays in that module, which is what lets the harness dispatch a call it never
parsed for. **The element type is `weaver-traits`' `ContentBlock` and is drawn
rather than restated**, on the rule the `Message` clause above states and
against the one internal dependency this crate already links, so it is not a
ninth satellite. A fragment that opened a call and could not be recovered is
the SPU's report on its operator channel and is deliberately not a member
here: a failed call arriving as prose would reach the model as content.

**`request` is the model.request content whole, not the rendered prompt alone.**
The custody act of 2026-08-11 makes `model.request` a spliced payload the SPU
renders, so this member carries what that event carries, the turn's delta as
rendered with its template identity and the turn's effective sampling values,
per the ruling of 2026-08-12, rendered by
the SPU because the template and the knobs are the SPU's and assembling them in
the harness would be the transform the splice refuses. It was named `rendered`
for one act, when only the prompt crossed, and the name narrows to the truth the
custody act settled.

**Spliced JSON and not octets, which is a distinction this crate enforced once
already.** `RawValue` carries pre-serialized JSON written in place, so the
measurement is a member of the answer's object rather than a string of bytes
inside it. What section 4.1 refused was double encoding, and splicing is what
avoids it, so that refusal is satisfied here rather than worked around.
`weaver-trace` took the same election for the same reason, its message and fault
payloads being spliced because a tag would wrap the bytes in an object of that
crate's making.

**The SPU renders and the harness authors, which is a ruling this restates rather
than a position this invents.** The fault-carrier ruling already has the SPU
handing over a `fault-report` and the harness authoring it as the `fault` event,
per the decode contract's section 2. The measurement travels that path to the
model events, so no crate holds a second copy of a shape `weaver-trace` owns and
the sole-writer rule is untouched: what the SPU produces is data, and the event
is still the harness's to author.

**The flush confirmation carries both resident counts**, because the SPU
is the one authority on either number and the harness authors the
record's `flush` event from exactly them: the count before the truncate
and the count after, the kept length as it held under the cut's bounds.
A confirmation without the counts would leave the event's payload to a
party that cannot know it, and under a bounded cut the counts are also
where the asking loop learns what its `keep` resolved to.

**The fullness rides every generation, added 2026-08-19 by issue #221's
arc.** `resident` is the session's token count as the generation closed,
terminator included, and `capacity` is the ceiling the load resolved, the
same two numbers the overflow refusal carries after the wall is hit,
carried here so the asking loop sees the pressure before it. Plain counts
with no judgment: when a flush is worth its cost is the loop's business,
per `weaver-state-PRD` section 2.

**Three cases, each one fact.** `Completed` is the family's stop condition
reached, the model's own end. `Stopped` is the generation ended from
outside the model's own signal: the operator's stop directive, or the
session's context capacity reached mid-generation, the resident-length
limit named here explicitly as the non-turn limit so the two ceilings
cannot be read as one. `Length` is the turn's token limit and that limit
alone, `max-tokens-per-turn` reached, added 2026-08-19 by
issue #218's evidence: the cap had exited as `Completed`, so a reader of
the record could not distinguish a finished answer from a cut one, which
is an ambiguity in the one artifact whose reason for existing is that it
never lies.

**`Finish` is shaped here and `weaver-trace` shapes its own, with the harness
converting.** That is the arrangement `SessionId` and `SessionRef` already take,
per `weaver-trace-Spec` section 1, that crate linking no internal crate on purpose
and the harness converting at one call site. Two names for one fact with a named
conversion point is the corpus's existing answer for a floor type the trace needs,
and a different answer here would be a second one.

**What the harness must check, because the type cannot.** A spliced member is
opaque to the compiler, so the measurement's conformance to what the trace's model
events accept is the harness's to enforce at the submit call, in the same place the
kind-to-payload pairing is already enforced by admission rather than by serde. The
election buys thinness in the floor and pays for it there, and naming where it is
paid is what keeps the payment from going missing.

**The five refusal cases are the contract's section 5 and this document adds
none.** The session is not open or residency is not confirmed for the ask. The
directive is out of order for the seam's state, a second generation or a
mid-flight flush above all. The session cannot take the next delta, and the
overflow carries the session's own account of itself because the harness decides
what a full context means and cannot decide it without the numbers. The delta is
malformed for the family. The elision's span describes no removable region, and
the refusal carries the span beside the bounds it was judged against so the
loop learns which of the four edges it crossed. **`OutOfOrder` is the same word
loop 0's refusal carries and is not the same case**, because the states it is
judged against are the decode seam's, which is why the trio carries its own
rather than drawing loop 0's.

**A cancel with nothing in flight answers `AtRest` rather than refusing**, per
the contract's section 2, an answer because it is not a failure of the ask.
**The fault report takes no answer at all, and its case is `Fault`**, per the
decode contract's second ruling of 2026-08-12: the report is the seam's one
SPU-originated emission, and the `Received` case an earlier shape of this trio
carried to close it as an exchange left with the receipt, a case nothing
constructs being the reserved slot apex section 7 forbids in data. The
report's own wire case arrived with `FaultReport`'s shape, the election
section 6 records as closed, and it is a case of the answer per the streaming
ruling's own logic, the SPU-to-harness traffic being one enum, with the prose
fact the type cannot carry stated here: the case closes no exchange and
answers nothing, the trace entry being the acknowledgment, per the contract.

**The stream is a case of the answer and never a fourth type**, per the
contract's section 2 as of the streaming ruling of 2026-08-11. `Token` is the
intermediate: any number cross before the close, each carrying one drawn
identifier and its rendered piece, and none closes the exchange, which stays
`Generated`'s alone. The identifier is a bare `u32` because the wire's numbers
are, the backend's own token type staying on the SPU's side of the seam, and
the piece is the family's rendering of that one token, so a consumer
accumulating pieces holds the emission as it grows and a consumer ignoring
every intermediate reads the seam the batch way.

**The encoding stays deferred and the deferral is the encoding's alone.** The
hot-path measurement elects it, per section 4.3's boundary rule and
`weaver-spu-Spec` section 12, and the measurement is taken against real decode
traffic which the first demonstration produces. Until that traffic exists the
seam carries this trio as JSON, the same encoding loop 0 carries, and **that is a
provisional election with a stated trigger rather than an answer**: it is
reconsidered when the first demonstration produces traffic to measure, and a
build that has produced that traffic and not reconsidered has an open election
reading as a settled one. The decode socket carries octets at its boundary
already, so the election changes one function on each side and nothing above it.

**No record is added here.** The trio's nodes are the charter's, declared at
`weaver-types-PRD` section 2.3 with their `defines` edges, and the instrument
that buys the ordering is `weaver-spu-Spec` section 9's, which names the
assertion and the seam it is watched on. A node restated here would give the
mapper two sources for one record.

**The segment series, added 2026-08-20 per the decode contract's section 1
and issue #236.** A frame whose serialized form exceeds
[`MAX_ENVELOPE_BYTES`] crosses as a series: one preamble datagram spelling
`{"segments":N,"bytes":M}`, then exactly N datagrams of raw octet slices in
order, each within the envelope bound, whose concatenation is the M bytes
of the one serialized frame, parsed as though it had crossed whole. The
preamble is recognized by what it lacks, and the absence is a reserved
shape rather than a guess: every trio frame carries `kind`, so a frame
without one is either exactly the preamble - two members, both unsigned
integers, nothing else - or a channel fault, and a kindless frame that is
not the exact preamble refuses as undecodable rather than being read
around. The preamble validates before any slice is read: `bytes` must
exceed the envelope bound, or the sender had no series to send, and must
not exceed the total bound. `segments` must equal exactly the datagrams
the byte length requires at the envelope size, so a count inconsistent
with `bytes` - too many for the length, too few to carry it, or past the
hundred-twenty-eight the total bound admits - refuses before the first
slice, and the slices must total exactly `bytes`, a short, long, or
interrupted series a channel fault, never a partial message. The
total bound is eight mebibytes, `DECODE_MESSAGE_BOUND`, elected against the
close's growth: a four-thousand-token turn's close measures in the
hundreds of kibibytes, the bound covers a sixteenfold cap without
renegotiation. A frame within
the envelope crosses as it always did, so the series costs nothing where
it is not needed, and either end may send one, the closes being the only
frames that grow today.

### 4.5 The label trio

**The demand is the classify contract's**, sections 2 and 5, drawn 2026-08-19
with the classifier's charter, and this subsection holds the result the way
4.4 holds the token trio's: the party under the construction's demand
determines the shape and the floor holds it.

```rust
pub enum LabelDirective {
    Classify {
        turn: Option<TurnKey>,
        content: String,
    },
}

pub enum LabelAnswer {
    Ready,
    Scored {
        turn: Option<TurnKey>,
        labels: Vec<ScoredLabel>,
    },
    Fault(FaultReport),
}

pub struct ScoredLabel {
    pub label: String,
    pub score: f64,
}

pub enum LabelRefusal {
    NotAdmitted { reason: String },
    NotReady,
    Oversized { requested: u64, bound: u64 },
    MalformedContent,
}
```

**The cases are the contract's, spelled once.** `Classify` carries the
content and the turn identity as the floor's satellite type, optional
because apex invariant 5.3 is conditional on an existing turn: the loop
that classifies between turns, composing its re-entry before any turn
opens, belongs to none and carries none, and a classify within a turn
carries the key. The classifier echoes the turn identity back on `Scored`
exactly as it arrived, absence included, and the equality is the decoded
value's: the satellite type compares by its string, the wire spelling
being serde's, which is the level the contract's echo is checked at.
`Ready` is the readiness emission, `NotAdmitted` its typed failure
traveling in the enter aggregate, and `NotReady` refuses a directive that
arrived before either, per the contract's ordering. `Oversized` names the
artifact's own bound the way the decode seam's overflow names the
session's, both counts denominated in the artifact's own tokens, the
tokenizer's count of the content against the bound the artifact resolved,
one unit stated once. `MalformedContent` is a frame the process could
decode as a directive and not serve. `Fault` rides the answer set the way
the token trio carries it, one `fault-report` definition with a further
carriage. Every label of the artifact's head crosses in `Scored` with
none elided and none beyond, per the contract's completeness guarantee,
so the vector is the head and its order is the head's own. A score is a
finite JSON number, one per label, `NaN` and either infinity excluded: the
wire cannot carry what JSON cannot spell, and a scorer producing one has
faulted rather than answered.

**Tagged the way 4.4 tags, and JSON without a deferral.** `LabelDirective`
and `LabelRefusal` are internally tagged, their value-carrying cases plain
data. `LabelAnswer` is adjacently tagged with `kind` and `body`, because
`Fault` carries the floor's `fault-report` whose account is a spliced
member, unquoted pre-serialized JSON under the shared field rules: a
spliced member cannot ride an internally tagged enum, whose deserializer
buffers the content a splice must read raw, which is the same fact that
shaped `TokenAnswer`. The traffic is one ask and one whole answer per the
asking loop's election, low in volume and diagnostic in audience, the
loop 0 criterion rather than the decode seam's, so JSON is a settled
election against a stated criterion, not 4.4's provisional one: there is
no hot-path measurement to wait for, because there is no hot path.

**No record is added here.** The trio's nodes are the charter's, declared
at `weaver-types-PRD` section 2.3 with their `defines` edges, per the same
one-source rule the token trio states above.

## 5. What is enforced, and by which instrument

**Enforced by the compiler.**

- The crate's one internal dependency is `weaver-traits`, checked against the
  graph's single `floor-link` under gate H2.
- The three wire enums are exhaustive, so a case added to loop 0 breaks every
  consumer's match rather than landing in a wildcard.
- The config parse yields a fully typed value or a typed error and exposes no
  partial value, so a half-valid config is unrepresentable rather than merely
  refused.
- `deny_unknown_fields` makes an unrecognized config key a parse error rather than a
  silent discard, against the surface this crate declares as one type today.
- A directive disagreeing with its binding kind is unconstructable, the gate
  instruction riding inside `EnterBinding`'s serving case, per section 4.

**Enforced by a compile-fail test, because the property is an absence.**
`PeerIdentity` implements no `Deserialize`: a doctest attempting to deserialize
one fails to compile, and it starts passing the day someone adds the derive.

**Enforced by review rather than by a mechanism, and said so.** The carve-out
bound of charter section 2.2, that this crate holds one policy function and gains
no second, is not a compile property: a signature cannot prevent a sibling
function being added beside it. It is charter enforcement read at review, and an
earlier draft of this Spec listed it as a compiler pin, which is the overclaim the
apex's enforcement section exists to prevent.

**Enforced by the manifest.** No socket crate, no runtime, no I/O in the
dependency tree, checked as `weaver-traits-Spec` section 7 checks its own, by a
build-time assertion over the resolved external tree rather than by H2.

**Enforced by a perturbation-verified test at the parse.** Section 2's rule
that every identity message carries `role: system` is a `BadValue` naming
`identity.<n>.role`, beside the empty-device-list check. The test declares
each of the three refused roles in turn and watches the parse refuse, and it
is watched to fail when the call is removed from `parse` - which is not a
hypothetical state but the one that produced the cross-precision deposit of
2026-08-25, a run whose prefix named the agent and whose record never said so.

**This entry read "enforced by nothing today" for two days**, carrying its
date and its reason, because the instrument was withheld on an experimental
ground rather than a technical one. It is recorded here rather than quietly
overwritten: an unenforced rule that no section admits is indistinguishable
from an enforced one, and the corpus is better served by a reader being able
to see that the gap was known and named while it stood.

**Which invariant each claim serves, and why most serve none.** Seven of the twenty
carry a `grounds` edge, five to `axiom-floor-is-vocabulary-behavior-is-socket` and two
to `axiom-contract-is-a-complete-interface`. The other three axioms take nothing from
this crate. The floor states no claim about a turn key, it is not an organ, and it is
not integrated: the fifth invariant binds what crosses between domains and the floor
crosses nothing, being linked rather than reached. **A crate can be drawn by every
domain and still be the subject of none of the invariants about domains**, which is the
floor's whole character stated from the graph's side. **The test applied is whether the
axiom is the reason the claim exists.** Remove the socket invariant and this crate has
no reason to elect a socket type, no reason to bound an envelope, and no reason to
withhold a deserializer from a credential, so those three ground in it. Remove it and
the config format is still TOML, the names are still kebab-case, and the parse is still
total, so those three ground in nothing. **Thirteen claims grounding in no invariant is
the expected result and not a gap**, per Document Format section 4: a floor crate is
mostly representation, and representation is what the invariants are not about.

The two edges to the contract invariant are the ones worth stating rather than
leaving to be read. The wire enums are exhaustive so that every case a contract
can return reaches a match loudly, which is that invariant's completeness enforced
at the type level rather than asserted in prose. The sink is a discriminated shape
rather than a bare path for the same reason at the vocabulary layer: a bare path
is vocabulary without meaning, and the consumer would have to guess what the
operator meant, which is the guess the discriminant exists to refuse.

**Where the assertion records sit, and which of these this crate declares.**
The records are at the clauses that argue the claims, across sections 1 through
4, rather than gathered here, per Document Format section 6: this section sorts
by instrument and the arguments are elsewhere, so a block here would sit apart
from the prose that earns it. **A claim this section owes to another crate is
declared by that crate,** not here, because the assertion belongs where its test
lives and a node declared twice is the one-name-two-nodes defect the format
forbids for identifiers. Three of the bullets below are such owings and carry no
record in this document: the out-of-order refusal owed to each organ, the
boundary and truncation tests owed to the pair-creating crates, and the
accept-time refusal owed to the gate, which `weaver-gate-Spec` section 6 has
already discharged.

**Requiring a perturbation-verified test, with the owning crate named.**

- In this crate: the denial-precedence test of section 3, confirmed by watching
  the agent uid pass when denial stops preceding permission.
- In this crate: a missing required field refuses the parse, run separately for
  the residual-readout election, since it is the one a builder is most likely to
  make optional.
- In this crate: a misspelled key refuses, confirmed by watching a mistyped
  `permission-mode` vanish when `deny_unknown_fields` is removed. The watch is the
  derive's removal while the surface is one type's, and it moves with the surface.
- In this crate: an identity message refuses the parse unless it is a
  `System` message carrying at least one `Text` block and every block carries
  text. **Four ways of seating nothing or seating the unwritable, and each is
  watched:**
    - a role the door does not write, naming `identity.<n>.role`, confirmed by
      watching all three refused roles parse when `check_identity_roles` is
      removed from `parse` - the state that produced the cross-precision
      deposit of 2026-08-25, a run whose prefix named the agent and whose
      record never said so
    - an empty content list, naming `identity.<n>.content`, confirmed by
      watching it parse when its check is removed
    - a block the licensing rule does not admit, naming
      `identity.<n>.content.<m>`, confirmed by watching a `tool_call` parse
      when the block loop is removed
    - a `Text` block carrying no text, naming
      `identity.<n>.content.<m>.text`, confirmed by watching it parse when
      that arm is removed

  **The door refuses the role and the block both**, so judging only the role
  left half of it at runtime: a `role: system` declaration carrying a
  `tool_call` parsed clean, authored a fault without aborting the load, and
  was refused at the SPU's open. An empty `identity` list stays lawful, that
  being an agent with no prefix. **This entry read "enforced by
  nothing today" from 2026-08-26 to 2026-08-28**, carrying its date and the
  experimental reason the instrument was withheld. It is recorded rather than
  quietly overwritten: an unenforced rule that no section admits is
  indistinguishable from an enforced one. **Since 2026-10-06 no declaration
  carries an identity at all**, so the watches call the check over the messages
  directly, as the harness's identity door and the seeding turn apply it, and
  keep their perturbations unchanged.
- In this crate: the declaration carries no identity, per section 2 on the ruling of
  2026-10-06, and both earlier forms refuse `UnknownField` by name, the
  `[[spu-instruction.decoder.identity]]` table and the `identity-file` key, each
  confirmed by watching it parse when its name is registered again
- In this crate: the config's field names are kebab-case on disk and snake_case
  in Rust, per section 2, confirmed by watching `rename_all` dropped from
  `AgentConfig` and `deny_unknown_fields` refuse the operator's own spellings.
- In this crate and in `weaver-traits`, one claim with an edge from each: the
  tagging test of section 4.3 yields the shapes it states, confirmed by watching
  `LabelAnswer` tagged internally and the answer's members flatten out of the
  adjacent body the test reads them from, which the `Scored` arm meets before
  the fault arm reaches the splice at all.
- In each organ's crate: a directive case belonging to another seam is refused as
  `OutOfOrder` rather than acted on, confirmed per seam by watching a wildcard arm
  swallow it.
- In the pair-creating crates, with a socket dev-dependency that the
  no-socket-crate rule does not reach because a dev-dependency ships in no build:
  one envelope write is one envelope read at the elected socket type, and a
  message exceeding the buffer sets the truncation flag rather than arriving
  short, both confirmed by watching a message split or silently shorten when the
  type is changed to `SOCK_STREAM`.
- In the gate's crate: the accept-time refusal of section 3.

**The kebab-case bullet and the tagging bullet moved from `review` with this
act**, per Document Format section 5's rule of v0.22 that `review` means an
instrument was not bought and never that none exists. Both were cited from a
file under `tests/` while their tags said no instrument stood, which is that
rule's sighting, and what the act bought is the `Perturbation:` line each test
now carries: the tags were the clauses' error and the watches were owed rather
than run. **A third claim of that shape keeps `review` and its clause now names
the non-purchase**, `types-loop0-encoding-json`, which is Document Format
section 5's fourth disposition rather than a retag declined. **That answers the
sighting's first question and not its second**, per the same section: a clause
settles the tag and never the citation. Where the citation belongs and whether
the claim divides are both open and section 6 carries them together.

## 6. Open elections

- **The trace door's wire, opened 2026-10-02 on #50 and ruled 2026-10-03 on #50 and
  #63.** Section 3.1 fixes the boundary file, the stream's framing and its control
  lines, and the cause. What stays the code act's is the heartbeat's interval.
- **`Generation`'s shape settled at section 4.4 and this bullet retires with it.**
  The emission, the canonical content, and the finish are shaped in the floor
  because the harness consumes them, and the measurement splices because nothing
  consumes it on the way to the trace's model events. The seven satellites stay
  `weaver-trace`'s, neither restated nor drawn, and the floor's dependency set is
  unchanged. **What does not retire is the block partition's own question**,
  which party labels a block's spans. Splicing decides where that answer is
  owed rather than what it is: the renderer of a shape the trace accepts is the
  SPU, so the label is written there, and the question is answered before the
  first measurement crosses rather than carried as an open election. It is filed
  against `weaver-spu` rather than here, this crate no longer having a stake in
  it.
- **The config file's directory.** Operator provisioning, outside what this program
  governs, per section 2. The file's name within it is fixed there, `<agent>.toml`.
- **The TOML implementation.** The `toml` crate, confirmed maintained at the moment the
  manifest is written, per section 2.
- **The decode seam's encoding.** The token workflow's, with the hot-path
  measurement, per section 4.3, the channel question having closed with the
  decoder-cut ruling, decode on its own socket. **The trio's representation
  left this bullet when section 4.4 landed it,** the directive and the refusal
  being shaped there in full, and the one open part of the answer,
  `Generation`'s shape, carrying the bullet above. This bullet reads as the
  encoding alone, which is the separation section 4.4 argues and which an
  unnarrowed wording here contradicted.
- **`DeviceOrdinal` and `ArtifactRef`.** Satellites of section 2 with no
  cross-crate consequence beyond being well-formed: the first is an unsigned
  device number, so a negative one is a parse error rather than a check, and
  the second is what an operator writes to name an artifact, whose resolution
  is admin's and whose readability is the SPU's.
- **`FaultReport`'s shape is closed, 2026-08-17, and its case set widened by
  one the same day**, the code act finding the harness charter one case short
  of its crate's standing practice. Elected at section 4.2
  against the closed case set of `weaver-spu-PRD` section 13.10,
  `weaver-gate-PRD` section 13.4, and `weaver-harness-PRD` section 5: a typed
  `case` the harness consumes and an organ-rendered `account` it splices,
  which is apex section 5.2's custody rule deciding the shape, the fault path
  thereby corrected from the standing exception that section named. One shape
  serves the wire and the `fault` event's payload, as this entry always
  required. **The same judgment decides the run bracket's payload question and
  is recorded here so it is not re-argued**: the load line's reconstruction
  fields are admin's account of a load it caused, so when they land they land
  as organ-rendered content the harness splices, no floor type growing for
  them. That act, and not this one, takes the two reopenings it needs by name,
  `weaver-trace-Spec`'s rule that a bracket kind carries no payload member and
  `weaver-trace-PRD` section 3.2's settlement, because a reopening lands only
  with the mechanism it records. It need not take a third: recording the
  measured hash of what ran is reading what the run produced, so
  `weaver-types-PRD` section 2.1's ruling that no mechanism authors run
  conditions stands untouched by that member. A config identity asserted at
  load is a different matter, the mechanism 2.1's ruling names, and whether
  the load line carries one is the mechanism act's judgment to make by
  reopening 2.1 by name or by declining the member, not this election's to
  foreclose either way.
- **`TurnFrame`'s shape is closed, 2026-08-12.** Elected at section 4.1 by
  argument rather than the deferred measurement, per the operator's ruling of
  this date: the gate validates nothing, so arbitrary octets reach the seam
  by construction, a splice would convert a refused turn into a channel
  fault, and the frame carries its octets base64-encoded in one member.
  Recorded as closed rather than deleted, this list naming what settled each
  entry.
- **The `tool-set` field's element shape.** It elects from `tool-trait`, which
  `weaver-traits-PRD` section 3.1 holds blocked, so the field is a list of names
  today and gains its element type with the tool workflow.
- **`SessionId`, `RunId`, `TurnKey`, `AgentName`, and `FieldName`.** Named in the
  signatures above and shaped in this crate, their representations being
  identifier choices with no cross-crate consequence.
- **`AgentState`, whose case set is not free.** Its Rust
  representation is this crate's, but it rides a `lifecycle-answer` out of
  admin's invocation, so what an operator can be told about an agent is exactly
  what these enumerate. The lifecycle's four states, per apex section 6, are the
  floor of that set, and whether it carries more is settled with the operator
  interface's own design rather than by a builder.
- **`EnterPayload`'s field list**, which follows what admin supplies in the enter
  directive, per `weaver-admin-harness-contract` sections 3 and 5, and moves when
  that contract does.
- **Whether `types-loop0-encoding-json` divides, and where its citation
  belongs.** Section 4.3 now names what was declined and what was taken in its
  place, which is Document Format section 5's fourth disposition and settles
  the tag: `review` is right and the clause says why. **That section grants no
  more than the tag**, a clause settling the tag and never the citation, so two
  questions stay open and they are halves of one. The first is representation:
  one node covers an election no mutation can remove and a layout
  `envelope_layout_is_the_stated_shape` watches byte for byte, which are two
  properties under one identifier. The second is placement:
  `crates/weaver-types/tests/wire.rs` carries the citation at its header while
  the clause above says that file watches the election's consequence rather
  than the election, so it sits at the unit holding what was taken in place of
  an instrument rather than at one holding the claim. **They answer together.**
  Divide the claim and the layout half takes the citation while the format half
  carries none, which is the honest shape of a claim no unit holds. Keep it one
  and the citation has nowhere to move to and is either deleted, leaving the
  claim uncited and invisible to every census reading because the tag is not
  `perturbation`, or kept with this paragraph as its reason. **This act read
  the instrument and declined to decide**, a split taken over the reading being
  as much a guess as a retag would have been.
