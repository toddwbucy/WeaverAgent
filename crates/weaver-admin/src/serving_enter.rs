//! Compose the exact replacement Enter from a retained verified artifact.
//! The active load dispatcher does not select this path before coupled cutover.
//! Selection, invocation/run locks, spawn and rollback remain the load owner's.
use std::collections::BTreeMap;
use std::os::fd::{AsFd, BorrowedFd};
use std::path::PathBuf;

use crate::save_points::serving::repository::VerifiedArtifact;
use crate::seed::preparation::{
    PreparationFailure, custody, invalid, judge_governing_sources, read_checker,
};
use crate::{
    ServiceConfig,
    channel::{ChannelFault, Coordination},
    inventory,
};
use weaver_types::state_flow::Digest;
use weaver_types::state_flow::enter::{
    EnterBinding, EnterEnvelope, EnterPayload, LoadedStateFacts,
};
use weaver_types::{AgentName, Cause, Reset, RunId};

/// Root/runtime facts do not replace any operational setting in the artifact.
pub(crate) struct RuntimeFacts {
    pub(crate) enter_window: weaver_types::state_flow::EnterWindow,
    pub(crate) run: RunId,
    pub(crate) reset: Option<Reset>,
    pub(crate) stack: BTreeMap<String, String>,
    pub(crate) cause: Cause,
}

pub(crate) struct PreparedEnter {
    artifact: VerifiedArtifact,
    inventory: inventory::Inventory,
    checker: Option<(PathBuf, Digest)>,
    envelope: EnterEnvelope,
}
impl PreparedEnter {
    /// Called after selection under the existing admin locks. The exact verified
    /// dump remains open for the member's inherited fd4 through this owner's life.
    pub(crate) fn prepare(
        config: &ServiceConfig,
        agent: &AgentName,
        artifact: VerifiedArtifact,
        facts: RuntimeFacts,
        ordinal: u64,
    ) -> Result<Self, PreparationFailure> {
        crate::admissible(config, agent)?;
        crate::judge_reader(&config.require_boundary()?.reader, agent)?;
        let boundary = crate::inventory_boundary(config, agent)?;
        Self::prepare_against(config, agent, artifact, facts, ordinal, &boundary)
    }

    pub(super) fn prepare_against(
        config: &ServiceConfig,
        agent: &AgentName,
        artifact: VerifiedArtifact,
        mut facts: RuntimeFacts,
        ordinal: u64,
        boundary: &inventory::Boundary,
    ) -> Result<Self, PreparationFailure> {
        crate::admissible(config, agent)?;
        let owner = nix::unistd::geteuid().as_raw();
        crate::judge_root(&config.root, owner)?;
        let root = crate::judge_ancestors(&config.root, &[owner, 0])?;
        crate::refuse_retired_keys(&root, &agent.0)?;
        crate::judge_entries(&root, owner)?;
        let root = crate::save_points::open_directory(&root)
            .map_err(|error| custody("config root", error))?;
        let source = std::str::from_utf8(artifact.configuration_bytes())
            .map_err(|error| invalid("declaration", error.to_string()))?;
        let mut inventory = inventory::take_bound_inventory(
            agent,
            source,
            artifact.declaration(),
            boundary,
            config.operator,
        )?;
        crate::sink_within_territory(&inventory.config.trace_sink, &config.territory)?;
        crate::grant_shell(config, &mut inventory)?;
        // ADMIN-STE100, C3 and Q8. Load takes bytes from its verified save point.
        let state = artifact.declaration().state_management();
        let checker = state
            .ste100
            .as_ref()
            .map(|_| read_checker(root.as_fd(), owner))
            .transpose()?;
        judge_governing_sources(root.as_fd(), owner, config.access_gid)?;
        let scheme_text = artifact.governing().scheme().to_owned();
        let thesaurus_text = artifact.governing().thesaurus().map(str::to_owned);
        let weaver_types::EnterBinding::Serving {
            gate_instruction,
            shell,
        } = &inventory.binding
        else {
            return Err(invalid(
                "binding-kind",
                "verified serving artifact requires Serving",
            ));
        };
        // Stack keys are executable basenames. A renamed checker must not be
        // reported under an invented filename or overwrite another binary fact.
        if let Some((binary, digest)) = &checker {
            let checker_name = binary
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| invalid("ste100-binary", "checker has no UTF-8 basename"))?;
            if facts
                .stack
                .get(checker_name)
                .is_some_and(|existing| existing != digest.as_str())
            {
                return Err(invalid(
                    "ste100-binary",
                    "checker basename conflicts with another stack digest",
                ));
            }
            facts
                .stack
                .insert(checker_name.to_owned(), digest.to_string());
        }
        let mut spu_instruction = inventory.config.spu_instruction.clone();
        spu_instruction.decoder.refeed_permission = false;
        spu_instruction.decoder.column_permission = false;
        let envelope = EnterEnvelope::new(
            ordinal,
            EnterPayload {
                session: inventory.config.session.clone(),
                run: facts.run,
                spu_instruction,
                binding: EnterBinding::Serving {
                    enter_window: facts.enter_window,
                    gate_instruction: gate_instruction.clone(),
                    shell: shell.clone(),
                    state_management: state.clone(),
                    start: artifact.identity(),
                    ste100_scheme: scheme_text,
                    ste100_thesaurus: thesaurus_text,
                },
                state_store: inventory.config.state_store.clone().unwrap_or_default(),
                declaration: artifact.identity().config_sha256.to_string(),
                restore: None,
                reset: facts.reset,
                stack: facts.stack,
                boundary: config.require_boundary()?.digest.clone(),
                cause: facts.cause,
                operator: config.operator,
                library_path: config
                    .library_path
                    .as_ref()
                    .map(|p| p.display().to_string()),
                drain_bound: inventory
                    .config
                    .lifecycle
                    .as_ref()
                    .and_then(|b| b.drain_bound),
                wind_down_bound: inventory
                    .config
                    .lifecycle
                    .as_ref()
                    .and_then(|b| b.wind_down_bound),
            },
        );
        Ok(Self {
            artifact,
            inventory,
            checker,
            envelope,
        })
    }
    pub(crate) fn inventory(&self) -> &inventory::Inventory {
        &self.inventory
    }
    pub(crate) fn dump(&self) -> BorrowedFd<'_> {
        self.artifact.dump()
    }
    pub(crate) fn checker_binary(&self) -> Option<&std::path::Path> {
        self.checker.as_ref().map(|(path, _)| path.as_path())
    }
    pub(crate) fn envelope(&self) -> &EnterEnvelope {
        &self.envelope
    }
    pub(crate) fn loaded_state(&self) -> LoadedStateFacts {
        let EnterBinding::Serving {
            state_management,
            start,
            ste100_scheme,
            ste100_thesaurus,
            ..
        } = &self.envelope.enter().binding
        else {
            unreachable!()
        };
        LoadedStateFacts::Serving {
            state_management: state_management.clone(),
            start: start.clone(),
            ste100_scheme: ste100_scheme.clone(),
            ste100_thesaurus: ste100_thesaurus.clone(),
            ste100_binary_sha256: self.checker.as_ref().map(|(_, digest)| digest.clone()),
        }
    }
    /// Uses the same retained envelope and passes the existing state endpoint.
    /// Oversize returns Truncated before sendmsg; load still owes its existing
    /// enter_frame_too_large diagnostic and start rollback at coupled cutover.
    pub(crate) fn send_within(
        &self,
        peer: &Coordination,
        sink: BorrowedFd<'_>,
        state: BorrowedFd<'_>,
        window: &crate::serving_load::window::LoadWindow,
    ) -> Result<(), ChannelFault> {
        peer.send_replacement_enter_within(&self.envelope, sink, state, window)
    }
    #[cfg(test)]
    pub(crate) fn send(
        &self,
        peer: &Coordination,
        sink: BorrowedFd<'_>,
        state: BorrowedFd<'_>,
    ) -> Result<(), ChannelFault> {
        peer.send_replacement_enter(&self.envelope, sink, Some(state))
    }
}

#[cfg(test)]
mod tests;
