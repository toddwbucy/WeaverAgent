//! The serving authoring and bound grammar, under TYPES-CONFIG and TYPES-PERSIST.
//!
//! These separate entry points do not activate the replacement serving path or
//! change the legacy parser. They judge declaration syntax and floor invariants.
//! Inventory still owns artifact custody and complete serving binding legality.

mod grammar;

use crate::state_flow::{
    Digest, FlowConfig, PersistenceConfig, PlacementConfig, ReadLimits, StateManagementConfig,
    StateStartConfig, Ste100Instruction, ToolPolicies,
};

use super::{AgentConfig, BindingKind, ConfigError, ConfigErrorKind, FieldName, StoreEngine};

/// Parsed authoring values. Only the starting dump hash may be absent.
///
/// A borrowed view is available before seeding so inventory can judge the scheme
/// and classifier. It cannot be injected into a declaration wrapper by callers.
#[derive(Debug, Clone, PartialEq)]
pub struct DraftStateManagement {
    pub flow: FlowConfig,
    pub start: Option<StateStartConfig>,
    pub persistence: PersistenceConfig,
    pub reads: ReadLimits,
    pub ste100: Ste100Instruction,
    pub tools: ToolPolicies,
    pub placement: PlacementConfig,
}

impl DraftStateManagement {
    fn bind(self, dump_sha256: Digest) -> StateManagementConfig {
        StateManagementConfig {
            flow: self.flow,
            start: StateStartConfig { dump_sha256 },
            persistence: self.persistence,
            reads: self.reads,
            ste100: self.ste100,
            tools: self.tools,
            placement: self.placement,
        }
    }
}

/// Complete operational authoring syntax with an optional starting dump hash.
/// There is no public constructor, deserializer or mutable field access.
#[derive(Debug, Clone, PartialEq)]
pub struct ServingDraft {
    agent: AgentConfig,
    state_management: DraftStateManagement,
}

impl ServingDraft {
    pub fn agent(&self) -> &AgentConfig {
        &self.agent
    }

    pub fn state_management(&self) -> &DraftStateManagement {
        &self.state_management
    }

    /// Bind the seed or rebind result without changing any operational setting.
    pub fn bind_start(self, dump_sha256: Digest) -> ServingDeclaration {
        ServingDeclaration {
            agent: self.agent,
            state_management: self.state_management.bind(dump_sha256),
        }
    }
}

/// Parsed published syntax. The start hash is present and has valid SHA-256 form.
/// This is not proof that the named artifact or model exists or is qualified.
///
/// ```compile_fail
/// let _ = weaver_types::ServingDeclaration {
///     agent: todo!(),
///     state_management: todo!(),
/// };
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct ServingDeclaration {
    agent: AgentConfig,
    state_management: StateManagementConfig,
}

impl ServingDeclaration {
    pub fn agent(&self) -> &AgentConfig {
        &self.agent
    }

    pub fn state_management(&self) -> &StateManagementConfig {
        &self.state_management
    }

    /// A save publishes the loaded operational settings with only the new A.
    pub fn with_start(&self, dump_sha256: Digest) -> Self {
        let mut declaration = self.clone();
        declaration.state_management.start = StateStartConfig { dump_sha256 };
        declaration
    }
}

/// Parse a root-owned authoring draft. A missing start table or hash is allowed.
/// An invalid present hash, retired key or incomplete operational table refuses.
pub fn parse_serving_draft(source: &str) -> Result<ServingDraft, ConfigError> {
    let mut document: toml::Table =
        toml::from_str(source).map_err(|error| super::classify_toml_error(error.message()))?;
    for retired in ["state-election", "state-flow", "restore"] {
        if document.contains_key(retired) {
            return Err(grammar::error(retired, ConfigErrorKind::UnknownField));
        }
    }
    let state = document.remove("state-management");
    // Reuse the existing common grammar and judgments without serializing a
    // different TOML document or admitting aliases on the wire types.
    super::check_provided_engine(source)?;
    super::check_lifecycle_bounds(source)?;
    let agent: AgentConfig = document
        .try_into()
        .map_err(|error: toml::de::Error| super::classify_toml_error(error.message()))?;
    if matches!(agent.binding_kind, Some(BindingKind::Diagnostic)) {
        return Err(grammar::error("binding-kind", ConfigErrorKind::BadValue));
    }
    if agent
        .state_store
        .as_ref()
        .is_some_and(|store| store.engine == StoreEngine::None)
    {
        return Err(grammar::error(
            "state-store.engine",
            ConfigErrorKind::BadValue,
        ));
    }
    if agent
        .spu_instruction
        .decoder
        .model_binding
        .devices
        .is_empty()
    {
        return Err(ConfigError {
            field: Some(FieldName("model-binding".into())),
            kind: ConfigErrorKind::BadValue,
        });
    }
    super::check_tunable_values(&agent.spu_instruction.decoder.tunable_values)?;
    super::check_trace_sink_surface(source, &agent.trace_sink)?;
    super::check_declared_paths(&agent)?;
    super::check_restore_name(&agent)?;
    super::check_tool_set(&agent)?;
    let state =
        state.ok_or_else(|| grammar::error("state-management", ConfigErrorKind::MissingField))?;
    let state_management = grammar::parse_state(&state, &agent.tool_set)?;
    Ok(ServingDraft {
        agent,
        state_management,
    })
}

/// Parse a published operational declaration. Unbound drafts are never loadable.
pub fn parse_serving_declaration(source: &str) -> Result<ServingDeclaration, ConfigError> {
    let draft = parse_serving_draft(source)?;
    let hash = draft
        .state_management
        .start
        .as_ref()
        .ok_or_else(|| {
            grammar::error(
                "state-management.start.dump-sha256",
                ConfigErrorKind::MissingField,
            )
        })?
        .dump_sha256
        .clone();
    Ok(draft.bind_start(hash))
}

#[cfg(test)]
mod tests;
