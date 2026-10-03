//! conforms: types-config-format-toml
//! conforms: types-config-names-kebab
//! conforms: types-no-default-derive
//! conforms: types-trace-sink-discriminated
//! conforms: types-config-parse-total
//!
//! The agent config: the declarative document that defines an agent, per
//! `weaver-types-Spec` section 2. Written by the operator, validated by admin
//! before a process exists, read by the harness for the elections it carries.
//! The format is TOML, elected against the charter's writer-audience criterion,
//! and the parser sits behind the non-default `config` cargo feature: only admin
//! and the harness parse the file, and the wire types below compile with the
//! feature off.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::identity::AccessRule;
use crate::wire::SessionId;

/// The declared surface of an agent.
///
/// Absence is a refusal rather than a default: an operator who stated no
/// residual readout has not thereby declined it, and admin refusing the load is
/// how that operator learns the file is incomplete. This is why the type derives
/// no `Default` and [`parse`] returns no partial value. The optional fields
/// are each optional by that rule's own exception, a governing document ruling
/// what absence means, and each names its ruling where it is declared.
///
/// Field names are kebab-case on disk and snake_case here, by explicit election,
/// and `deny_unknown_fields` is the fixed-surface mechanism: a field no organ
/// registered refuses rather than being ignored, so a typo in `permission-mode`
/// cannot silently vanish.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct AgentConfig {
    pub session: SessionId,
    pub spu_instruction: SpuInstruction,
    pub tool_set: Vec<ToolName>,
    pub permission_mode: weaver_traits::PermissionMode,
    /// What this load is for, per `weaver-types-Spec` section 2 and
    /// `weaver-agent-PRD` section 6: absence means serving, so a declaration
    /// written before the member existed declares what it always meant. Admin
    /// resolves the absence at inventory, the one site, per
    /// `weaver-admin-Spec` section 7.
    #[serde(default)]
    pub binding_kind: Option<BindingKind>,
    /// Optional at the parse and conditional at the inventory: presence
    /// follows the resolved kind, a serving binding requiring it and a
    /// diagnostic binding excluding it, per `weaver-types-Spec` section 2.
    /// The parse checks each field alone, so the cross-field rule is admin's.
    #[serde(default)]
    pub gate_instruction: Option<GateInstruction>,
    pub trace_sink: TraceSink,
    /// The tee's election, per `weaver-types-Spec` section 2: the one
    /// optional field, optional by the required-field rule's own exception
    /// because `weaver-state-PRD` section 4 rules what absence means - the
    /// default election, the envelope of every kind. Admin resolves the
    /// absence at inventory, so the worker never re-derives it.
    #[serde(default)]
    pub state_election: Option<StateElection>,
    /// The store the state member stands on, per `weaver-types-Spec` section
    /// 2 and the ruling of 2026-09-04 that the store is a port: absent means
    /// the embedded engine with the member standing, and the cross-field
    /// rules - `database` and `role` exactly under the service engine, and
    /// no election beside `none` - are admin's at inventory, this parse
    /// checking each field alone.
    #[serde(default)]
    pub state_store: Option<StateStore>,
    /// The agent's loop file, per `weaver-types-Spec` section 2 and the
    /// ruling of 2026-08-20 on issue #243: the loop is a member of this
    /// agent's harness and unique to it. The second optional field, by the
    /// required-field rule's own exception because `weaver-harness-PRD`
    /// section 2 rules what absence means - the worker's own default loop.
    /// It reaches the worker on the unit's argument vector, never in an
    /// exchange, because no exchange carries a path.
    #[serde(default)]
    pub loop_file: Option<PathBuf>,
    /// The record the session stands from and the cut, per
    /// `weaver-types-Spec` section 2 as of 2026-09-04 and issue #432: absent
    /// means the load stands from nothing, which is what every load did
    /// before the ruling. Present, it names a record admin reads under its
    /// own custody and the harness never sees, and the session name decides
    /// whether the restore is a resume or a branch, judged at the inventory.
    #[serde(default)]
    pub restore: Option<Restore>,
}

/// The restore election, per `weaver-types-Spec` section 2: the record's
/// path and, where the session stands on a prefix of it, the cut.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct Restore {
    pub record: PathBuf,
    #[serde(default)]
    pub through: Option<Cut>,
}

/// A cut in a record: a run by its reference and a turn within it, the run
/// carried because a turn's number recurs across a session's runs and a cut
/// naming the turn alone would name several.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct Cut {
    pub run: crate::wire::RunId,
    pub turn: u64,
}

/// The store election, per `weaver-types-Spec` section 2: which port the
/// deployment elects, and the service engine's database and role.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct StateStore {
    pub engine: StoreEngine,
    #[serde(default)]
    pub database: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
}

impl Default for StateStore {
    /// The embedded engine, which is what an absent election means.
    fn default() -> Self {
        StateStore {
            engine: StoreEngine::Sqlite,
            database: None,
            role: None,
        }
    }
}

/// The engines the state charter charters, and the declared absence of a
/// member. Closed at three: a further engine is a state act before it is a
/// variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StoreEngine {
    None,
    Sqlite,
    Postgres,
}

/// What a load is for, per `weaver-agent-PRD` section 6 as amended
/// 2026-08-24: a serving binding raises the whole interior and a diagnostic
/// binding raises it without Gate, for the replay apex section 8 places
/// outside the agent.
///
/// Closed at two variants because the apex names exactly two kinds: a third
/// kind is an apex act before it is a variant. Not an election - an election
/// turns an observation on or off inside a load whose shape is settled, and
/// the kind is the shape.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BindingKind {
    Serving,
    Diagnostic,
}

/// The probability field's election, per `weaver-types-Spec` section 2: how
/// many candidates are ranked and reported at each decode position.
///
/// **The depth has a floor and the floor is the sampling cutoff**, judged
/// by the SPU at admit per `weaver-spu-Spec` section 7.5 rather than here.
/// The sampler truncates before it draws, so top-k puts a real wall in the
/// field where the reported depth is an artifact of the reporting, and
/// telling them apart requires reporting past the wall.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct FieldElection {
    pub depth: u32,
}

/// The operator's election of payload key paths for the state tee, per
/// `weaver-types-Spec` section 2. The resolved default's spelling is fixed
/// there so two resolvers cannot disagree: `all_kinds` true and `keys`
/// empty. The empty list is only the default's spelling: `keys` stays
/// meaningful beside `all_kinds` true, each named kind adding payload
/// paths on top of the envelope every kind already crosses with. When the
/// block is present in the file, both members are required, the
/// required-field discipline resuming inside it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct StateElection {
    pub all_kinds: bool,
    pub keys: Vec<ElectedKindConfig>,
}

impl Default for StateElection {
    fn default() -> Self {
        StateElection {
            all_kinds: true,
            keys: Vec::new(),
        }
    }
}

/// One kind's election: the kind as the canonical form spells it, and the
/// payload key paths elected for it, dotted from the payload root.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ElectedKindConfig {
    pub kind: String,
    pub paths: Vec<String>,
}

/// The SPU's section of the declaration, per `weaver-types-Spec` section 2:
/// an organ's fields are named together and cross together, the pattern
/// `gate_instruction` already takes. The operator writes it, admin validates
/// it, the harness carries it uninterpreted, and the SPU consumes it at
/// admit.
///
/// `decoder` names a role rather than a slot. The organ's domain is every
/// semantic operation in the text modality, and the decode role is the one
/// whose seam stands, so an embedder key arrives in the act that builds an
/// embedder rather than being carried empty here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct SpuInstruction {
    pub decoder: DecoderInstruction,
    /// The classify role, the section's second key, per `weaver-types-Spec`
    /// section 2 as of the classifier act: optional by presence, absence
    /// being the operator's declaration that the agent runs no classifier,
    /// per `weaver-spu-PRD` section 15.3.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub classify: Option<ClassifyInstruction>,
}

/// The classify role's declaration: the model binding at the smaller size,
/// per `weaver-types-Spec` section 2. Every field of a present section is
/// required, the no-defaulting rule untouched by the section's own option.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ClassifyInstruction {
    pub model_binding: ModelBinding,
}

/// The decode role's declaration: which model serves it and whether the
/// residual readout is elected for the load. Absence of either field refuses
/// the parse at any depth, the no-defaulting rule of `weaver-types-Spec`
/// section 2 surviving the nesting untouched.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct DecoderInstruction {
    pub model_binding: ModelBinding,
    pub residual_readout_election: bool,
    /// The probability field's election, per `weaver-types-Spec` section 2
    /// and `weaver-spu-PRD` section 13.11. Optional because the election is
    /// what makes the field exist and its absence is the ordinary posture
    /// rather than a value withheld, which is why it takes no default and
    /// its absence is not read as one.
    ///
    /// **It stands beside the readout's election and is never merged with
    /// it.** Each diagnostic election stands alone and none is bundled
    /// under a name for a set: a named set drifts as members join it, and
    /// every record already carrying the name becomes a record of
    /// something else without any event saying so.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field_election: Option<FieldElection>,
    /// The per-position surprisal's election, per `weaver-types-Spec`
    /// section 2 and `weaver-spu-PRD` section 13.12. A bare boolean beside
    /// the field's option because a per-position reading has no size to
    /// declare: the vector exists at every position or at none.
    ///
    /// **This election runs the other way from the two above it.** They
    /// elect an observation into existence, so unelected there is nothing.
    /// This one elects a reading out: the vector was unconditional until
    /// 2026-08-21 and the default posture is now the smaller record, with
    /// the generation's perplexity standing in its place. `default` is
    /// therefore `false` and means the vector is not produced, which is
    /// also what a declaration written before the election existed says by
    /// omitting the field.
    #[serde(default)]
    pub surprisal_election: bool,
    /// The re-feed drive's permission, per `weaver-spu-PRD` section 13.14:
    /// admin's member and never the declaration's. It parses with `default`
    /// because one type serves the declaration and the seam, so a parse
    /// that refused the member would also refuse the instruction admin
    /// authored at the next seam, per `weaver-types-Spec` section 2 as
    /// corrected 2026-08-31. The declaration's refusal is admin's at the
    /// inventory, which refuses a granted permission naming this field,
    /// and admin's construction overwrites the member from the resolved
    /// kind, so a written `false` is inert and indistinguishable from
    /// absence, per `weaver-admin-Spec` section 7.
    #[serde(default)]
    pub refeed_permission: bool,
    /// The column stream's permission, per `weaver-spu-PRD` section 13.7:
    /// admin's member on exactly `refeed_permission`'s terms - parsed with
    /// `default` because one type serves the declaration and the seam,
    /// refused at admin's inventory where a declaration grants it, and
    /// written from the resolved binding kind at admin's construction, per
    /// `weaver-admin-Spec` section 7.
    #[serde(default)]
    pub column_permission: bool,
    /// The session's identity material: the canonical messages the identity
    /// prefix is rendered from, configuration rather than history, per
    /// `weaver-types-Spec` section 2. Required like every field, and an
    /// empty list is a declaration the operator made where an absent field
    /// is a file unfinished.
    pub identity: Vec<weaver_traits::Message>,
    /// Values for whatever parameters this deployment's SPU left tunable, per
    /// `weaver-types-Spec` section 2 and `weaver-spu-Spec` section 8. This is
    /// the route `Disposition::OperatorTunable` names, keyed by the
    /// parameter's name.
    ///
    /// **A map rather than a field per parameter**, because which parameters a
    /// binary leaves tunable is that binary's election and moves with a
    /// recompile: a floor type enumerating them would move with every
    /// deployment that changed its mind. Required like every field and may be
    /// empty, an empty map being a declaration that supplies nothing. A name
    /// this binary froze is ignored where it appears, frozen meaning compiled
    /// in and never carried, so a declaration cannot move what a deployment
    /// locked.
    pub tunable_values: std::collections::BTreeMap<String, f64>,
}

/// The model artifact and the devices it is assigned to.
///
/// The vector is ordered and the order is the shard order. An empty set is a
/// parse error rather than a default, because a binding assigning no device is a
/// declaration the operator did not finish, and defaulting it to device zero is
/// the placement decision this crate exists not to make. Whether the devices
/// exist and can shard is admission's; this crate answers well-formed only.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ModelBinding {
    pub artifact: ArtifactRef,
    pub devices: Vec<DeviceOrdinal>,
}

/// The instruction the gate resolves and never interprets beyond its fields.
///
/// The operator writes it, admin validates it, the harness carries it
/// uninterpreted, and the gate consumes it beside the socket the raise
/// directive carries, per `weaver-gate-Spec` section 3.
///
/// **The socket is not here, and that is the ruling of 2026-08-15.** Where a
/// door stands is the program's and only who may pass is the operator's, per
/// `weaver-gate-PRD` section 2. A pathname the operator wrote could sit
/// outside the unit's runtime directory, where it outlives its worker and
/// refuses the next bind, so the program places it inside and the hazard
/// becomes unreachable rather than checked for.
///
/// The group survives with one field rather than collapsing to a bare rule,
/// on the pattern `spu_instruction` takes, so a field the gate workflow adds
/// later has somewhere to land.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct GateInstruction {
    pub access_rule: AccessRule,
}

/// Where the stream lands: a sink, not only a path.
///
/// A file, a pipe, or a socket are all conforming sinks, so the field carries a
/// discriminated shape and admin opens by the discriminant rather than guessing
/// from the filesystem. `File` and `Pipe` carry a creation flag because admin
/// can make either; a socket sink is different in kind, something on the
/// operator's side must already be listening, so a creation flag would promise
/// an act admin cannot perform and a missing socket sink always refuses.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum TraceSink {
    File { path: PathBuf, create: bool },
    Pipe { path: PathBuf, create: bool },
    Socket { path: PathBuf },
}

/// What an operator writes to name a model artifact. Resolution and readability
/// are the SPU's at admission, under the agent's identity, admin asking only that
/// it is named, per `weaver-admin-PRD` section 4.3. This crate answers well-formed
/// only.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArtifactRef(pub String);

/// An unsigned device number, so a negative one is a parse error rather than a
/// check.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeviceOrdinal(pub u32);

/// A tool's name. A name today, gaining its element type with the tool
/// workflow, because it elects from `tool-trait`, which the traits charter
/// holds blocked.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolName(pub String);

/// A config field's name, as the operator spells it, kebab-case.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldName(pub String);

/// A refused parse, typed: the field it names where one names itself, and what
/// went wrong.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConfigError {
    pub field: Option<FieldName>,
    pub kind: ConfigErrorKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigErrorKind {
    Malformed,
    MissingField,
    UnknownField,
    BadValue,
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let kind = match self.kind {
            ConfigErrorKind::Malformed => "malformed document",
            ConfigErrorKind::MissingField => "missing required field",
            ConfigErrorKind::UnknownField => "unknown field",
            ConfigErrorKind::BadValue => "bad value",
        };
        match &self.field {
            Some(FieldName(name)) => write!(f, "{kind}: {name}"),
            None => write!(f, "{kind}"),
        }
    }
}

impl std::error::Error for ConfigError {}

/// **The boundary file's parse**, per `weaver-types-Spec` section 3.1: the
/// agent's `roles.toml`, read by admin from the agent's root, the one trace
/// reader and nothing else, an unknown key refused. A total parse like the
/// declaration's, and kept apart from it, the file being boundary and never
/// constitution.
#[cfg(feature = "config")]
pub fn parse_boundary(source: &str) -> Result<crate::BoundaryFile, ConfigError> {
    toml::from_str(source).map_err(|e| classify_toml_error(e.message()))
}

/// A total parse into a typed value: the whole config or a typed refusal, and
/// nothing partial, so a half-valid config is unrepresentable rather than
/// merely refused.
///
/// The signature is the pin (`types-config-parse-total`): the crate exposes
/// this and no builder and no field-by-field accessor.
///
/// ```
/// let _: fn(&str) -> Result<weaver_types::AgentConfig, weaver_types::ConfigError> =
///     weaver_types::parse;
/// ```
#[cfg(feature = "config")]
pub fn parse(source: &str) -> Result<AgentConfig, ConfigError> {
    let config: AgentConfig =
        toml::from_str(source).map_err(|e| classify_toml_error(e.message()))?;
    if config
        .spu_instruction
        .decoder
        .model_binding
        .devices
        .is_empty()
    {
        return Err(ConfigError {
            field: Some(FieldName("model-binding".to_string())),
            kind: ConfigErrorKind::BadValue,
        });
    }
    check_tunable_values(&config.spu_instruction.decoder.tunable_values)?;
    check_identity_roles(&config.spu_instruction.decoder.identity)?;
    check_trace_sink_surface(source, &config.trace_sink)?;
    check_declared_paths(&config)?;
    Ok(config)
}

/// **A path in a declaration carries no control character**, per
/// `weaver-types-Spec` section 2 and the operator's ruling of 2026-09-28:
/// every reader in the suite, this parser, the deploy script's shell and the
/// experiment harness, must agree on what a path is, and a control character
/// is where they part, a shell's command substitution dropping a trailing
/// newline this parser keeps. The three path fields are the restore's record,
/// the loop file and the trace sink's path, each refused by its own name.
#[cfg(feature = "config")]
fn check_declared_paths(config: &AgentConfig) -> Result<(), ConfigError> {
    let sink = match &config.trace_sink {
        TraceSink::File { path, .. }
        | TraceSink::Pipe { path, .. }
        | TraceSink::Socket { path } => path,
    };
    let declared = [
        ("trace-sink.path", Some(sink)),
        ("loop-file", config.loop_file.as_ref()),
        ("restore.record", config.restore.as_ref().map(|r| &r.record)),
    ];
    for (field, path) in declared {
        if path.is_some_and(|p| p.to_string_lossy().chars().any(char::is_control)) {
            return Err(ConfigError {
                field: Some(FieldName(field.to_string())),
                kind: ConfigErrorKind::BadValue,
            });
        }
    }
    Ok(())
}

/// **The identity prefix is `system` and the parse is where that binds**, per
/// `weaver-types-Spec` section 2 and the door of `weaver-harness-Spec`
/// section 6.
///
/// The door writes `message.system` and refuses every other role, per
/// `harness-identity-door-writes-system-only`, so a declaration carrying one
/// seats a prefix into the decode context that the record cannot show - the
/// condition `weaver-harness-PRD` section 5's fifth case reports and issue
/// #369 found in the field. **The door is the last place that can be caught
/// rather than the first**: a rule enforced only there is one the operator
/// meets as a fault in a running agent instead of as a refusal to load, so
/// the judgment belongs where declarations are judged.
///
/// **The role and the block are both judged**, the identity door refusing
/// both: `weaver-traits-Spec` section 3 licenses a `System` message to carry
/// `Text` blocks only. Judging the role alone left half the door's refusal at
/// runtime, which is the condition this check exists to prevent rather than
/// to halve.
///
/// The index rides the field name, an operator with several messages needing
/// to know which one, the way `tunable-values.<name>` already names its own.
///
/// conforms: types-identity-role-is-system
#[cfg(feature = "config")]
fn check_identity_roles(identity: &[weaver_traits::Message]) -> Result<(), ConfigError> {
    for (at, message) in identity.iter().enumerate() {
        if !matches!(message.role, weaver_traits::Role::System) {
            return Err(ConfigError {
                field: Some(FieldName(format!("identity.{at}.role"))),
                kind: ConfigErrorKind::BadValue,
            });
        }
        // **An identity message carries something.** An empty content list
        // parses as a message and renders as an empty turn - `[INST][/INST]`
        // on mistral, a bare user turn on gemma - seated into every session
        // for the life of the agent. The argument for judging here rather
        // than at the door is that the operator meets it as a refusal to load
        // instead of as a fault in a running agent, and this case belongs to
        // that argument as much as the role does. An empty `identity` list is
        // a different thing and stays lawful: an agent with no prefix.
        if message.content.is_empty() {
            return Err(ConfigError {
                field: Some(FieldName(format!("identity.{at}.content"))),
                kind: ConfigErrorKind::BadValue,
            });
        }
        // **The block is judged beside the role**, the door refusing both.
        // `weaver-traits-Spec` section 3 licenses a `System` message to carry
        // `Text` and nothing else, so a declaration naming the right role with
        // a `tool_call` in it parsed cleanly, authored an
        // `IdentityPrefixUnrecorded` fault without aborting the load, and was
        // refused at the SPU's `Open` - leaving the operator to meet in a
        // running agent what this check exists to answer at the load.
        for (block_at, block) in message.content.iter().enumerate() {
            match block {
                // **A block carrying no text is an empty turn by another
                // route.** The list is non-empty and the block is licensed,
                // so both checks above pass, and what reaches the model is
                // the same seated nothing an empty list would have given it.
                weaver_traits::ContentBlock::Text { text } if text.is_empty() => {
                    return Err(ConfigError {
                        field: Some(FieldName(format!("identity.{at}.content.{block_at}.text"))),
                        kind: ConfigErrorKind::BadValue,
                    });
                }
                weaver_traits::ContentBlock::Text { .. } => {}
                _ => {
                    return Err(ConfigError {
                        field: Some(FieldName(format!("identity.{at}.content.{block_at}"))),
                        kind: ConfigErrorKind::BadValue,
                    });
                }
            }
        }
    }
    Ok(())
}

/// **Finiteness is the whole of what this crate can judge about a tunable
/// value, and the boundary is deliberate.** A `NaN` compares false against
/// every bound and an infinity is one no filter clamps, and neither is
/// specific to which parameter carried it, so the check belongs here.
///
/// Whether a value suits its parameter is not this crate's to say. Which names
/// are counts and which are reals is the SPU's election, held in its
/// dispositions and moving with its recompile, so a list of them here would be
/// the same fact in two places with no authority named, which G5 files as a
/// defect rather than resolves. The SPU judges that at resolve, before any
/// device work, per `weaver-spu-Spec` section 8.
#[cfg(feature = "config")]
fn check_tunable_values(
    values: &std::collections::BTreeMap<String, f64>,
) -> Result<(), ConfigError> {
    for (name, value) in values {
        if !value.is_finite() {
            return Err(ConfigError {
                field: Some(FieldName(format!("tunable-values.{name}"))),
                kind: ConfigErrorKind::BadValue,
            });
        }
    }
    Ok(())
}

/// The fixed-surface check the derive cannot carry: serde's
/// `deny_unknown_fields` does not compose with an internally tagged enum, so an
/// unknown key inside `trace-sink` would be silently discarded by the typed
/// parse alone, which is the vanishing-declaration failure the refusal exists
/// to prevent. The raw table is read back and its keys are judged against the
/// selected variant's surface: `file` and `pipe` carry `kind`, `path`, and
/// `create`, and `socket` carries `kind` and `path`.
#[cfg(feature = "config")]
fn check_trace_sink_surface(source: &str, sink: &TraceSink) -> Result<(), ConfigError> {
    let malformed = || ConfigError {
        field: None,
        kind: ConfigErrorKind::Malformed,
    };
    let document: toml::Table = toml::from_str(source).map_err(|_| malformed())?;
    let table = document
        .get("trace-sink")
        .and_then(|v| v.as_table())
        .ok_or_else(malformed)?;
    let allowed: &[&str] = match sink {
        TraceSink::File { .. } | TraceSink::Pipe { .. } => &["kind", "path", "create"],
        TraceSink::Socket { .. } => &["kind", "path"],
    };
    for key in table.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(ConfigError {
                field: Some(FieldName(format!("trace-sink.{key}"))),
                kind: ConfigErrorKind::UnknownField,
            });
        }
    }
    Ok(())
}

/// Sorts a toml error's message into the typed kinds. The message is taken
/// without the source excerpt the error's display adds, so a backtick in the
/// operator's own text cannot be read as a field name. Where serde raised it,
/// its shapes are stable across the 1.x derive: `missing field `name``,
/// `unknown field `name``, `invalid type`, `unknown variant`. A syntax error is
/// toml's own and sorts as malformed.
#[cfg(feature = "config")]
fn classify_toml_error(message: &str) -> ConfigError {
    fn backticked(message: &str, after: &str) -> Option<FieldName> {
        let rest = message.split(after).nth(1)?;
        let name = rest.split('`').nth(1)?;
        Some(FieldName(name.to_string()))
    }
    if message.contains("missing field") {
        ConfigError {
            field: backticked(message, "missing field"),
            kind: ConfigErrorKind::MissingField,
        }
    } else if message.contains("unknown field") {
        ConfigError {
            field: backticked(message, "unknown field"),
            kind: ConfigErrorKind::UnknownField,
        }
    } else if message.contains("invalid type")
        || message.contains("invalid value")
        || message.contains("unknown variant")
    {
        ConfigError {
            field: None,
            kind: ConfigErrorKind::BadValue,
        }
    } else {
        ConfigError {
            field: None,
            kind: ConfigErrorKind::Malformed,
        }
    }
}
