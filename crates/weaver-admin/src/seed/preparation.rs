//! The dormant seed inventory, under ADMIN-SEED and ADMIN-STE100.
//!
//! Preparation reads fixed root-owned inputs and creates nothing. It does not
//! qualify a model, start a classifier, select an artifact or grant publication.
//! The active legacy inventory and command surface do not call this path.

use std::fs::File;
use std::io::Read;
use std::os::fd::{AsFd, BorrowedFd};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use sha2::{Digest as _, Sha256};
use weaver_types::state_flow::{
    Digest, PersistenceCase, STATE_CONTENT_SCHEMA, SeedCertificate, SeedRefusal, SeedRequest,
    VerifiedSeed, decode_strict,
};
use weaver_types::{AgentName, FieldName, LifecycleRefusal, ServingDraft};

use crate::{ServiceConfig, inventory};

/// Only this module can construct prepared inputs. The floor's private
/// VerifiedSeed is retained, but is not a claim of language qualification.
#[derive(Debug)]
pub(super) struct PreparedSeed {
    draft: ServingDraft,
    inventory: inventory::Inventory,
    request: SeedRequest,
    verified: VerifiedSeed,
    checker_binary: PathBuf,
    checker_binary_sha256: Digest,
}

impl PreparedSeed {
    pub(super) fn draft(&self) -> &ServingDraft {
        &self.draft
    }

    pub(super) fn inventory(&self) -> &inventory::Inventory {
        &self.inventory
    }

    pub(super) fn member_account(&self) -> inventory::MemberAccount {
        self.inventory
            .member_account
            .expect("seed inventory requires the state member")
    }

    pub(super) fn identity(&self) -> &str {
        &self.inventory.identity
    }

    pub(super) fn request(&self) -> &SeedRequest {
        &self.request
    }

    pub(super) fn checker_binary(&self) -> &Path {
        &self.checker_binary
    }

    pub(super) fn checker_binary_sha256(&self) -> &Digest {
        &self.checker_binary_sha256
    }

    pub(super) fn into_parts(self) -> (ServingDraft, inventory::Inventory, SeedRequest) {
        (self.draft, self.inventory, self.request)
    }
}

/// Certificate refusal has its own persistence case. It must not be flattened
/// into ConfigInvalid by the caller (TYPES-SEED).
#[derive(Debug)]
pub(crate) enum PreparationFailure {
    Inventory {
        refusal: LifecycleRefusal,
        detail: String,
    },
    Certificate(String),
}

impl PreparationFailure {
    pub(super) fn case(&self) -> Option<PersistenceCase> {
        match self {
            Self::Certificate(_) => Some(PersistenceCase::SeedCertificateInvalid),
            Self::Inventory { .. } => None,
        }
    }
}

impl std::fmt::Display for PreparationFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Inventory { refusal, detail } => write!(f, "{refusal:?}: {detail}"),
            Self::Certificate(detail) => write!(f, "seed_certificate_invalid: {detail}"),
        }
    }
}

impl std::error::Error for PreparationFailure {}

impl From<LifecycleRefusal> for PreparationFailure {
    fn from(refusal: LifecycleRefusal) -> Self {
        Self::Inventory {
            refusal,
            detail: "seed inventory refused".into(),
        }
    }
}

pub(super) fn prepare(
    config: &ServiceConfig,
    agent: &AgentName,
    seed_id: String,
) -> Result<PreparedSeed, PreparationFailure> {
    crate::admissible(config, agent)?;
    crate::judge_reader(&config.require_boundary()?.reader, agent)?;
    let boundary = crate::inventory_boundary(config, agent)?;
    prepare_against(config, agent, seed_id, &boundary)
}

/// Host account facts are separated from file judgment so the instruments do
/// not depend on a provisioned agent account. Production has one caller above.
fn prepare_against(
    config: &ServiceConfig,
    agent: &AgentName,
    seed_id: String,
    boundary: &inventory::Boundary,
) -> Result<PreparedSeed, PreparationFailure> {
    crate::admissible(config, agent)?;
    let owner = nix::unistd::geteuid().as_raw();
    crate::judge_root(&config.root, owner)?;
    let root = crate::judge_ancestors(&config.root, &[owner, 0])?;
    crate::refuse_retired_keys(&root, &agent.0)?;
    crate::judge_entries(&root, owner)?;
    let root =
        crate::save_points::open_directory(&root).map_err(|error| custody("config root", error))?;

    let source = crate::read_declaration(config)?;
    let draft = weaver_types::parse_serving_draft(&source).map_err(|error| {
        PreparationFailure::Inventory {
            refusal: LifecycleRefusal::ConfigInvalid { field: error.field },
            detail: "invalid seed authoring declaration".into(),
        }
    })?;
    let mut inventory = inventory::take_seed_inventory(agent, &source, &draft, boundary)?;
    crate::sink_within_territory(&inventory.config.trace_sink, &config.territory)?;
    crate::grant_shell(config, &mut inventory)?;

    let checker_path = read_text(root.as_fd(), "ste100-binary", owner, None, None)?;
    let checker_path = PathBuf::from(checker_path.trim());
    if !checker_path.is_absolute() {
        return Err(invalid(
            "ste100-binary",
            "checker binary path is not absolute",
        ));
    }
    let (checker_binary, checker_binary_sha256) = judge_checker(&checker_path, owner)?;
    let scheme = read_text(
        root.as_fd(),
        "ste100-scheme.json",
        owner,
        Some(config.access_gid),
        Some(0o640),
    )?;
    let prompt = read_text(
        config.territory_fd()?,
        "system-prompt.md",
        owner,
        Some(config.access_gid),
        Some(0o640),
    )?;
    let certificate = read_text(
        config.territory_fd()?,
        "system-prompt-certificate.json",
        owner,
        Some(config.access_gid),
        Some(0o640),
    )?;
    let certificate: SeedCertificate = decode_strict(&certificate).map_err(|error| {
        PreparationFailure::Certificate(format!("certificate does not parse: {error}"))
    })?;
    if certificate.bindings.is_empty() {
        return Err(PreparationFailure::Certificate(
            "certificate supplies no statement bindings".into(),
        ));
    }
    let state = draft.state_management();
    let raw_block = serde_json::value::RawValue::from_string(
        serde_json::json!({"type": "text", "text": prompt}).to_string(),
    )
    .expect("JSON serialization produces a complete Text block");
    let request = SeedRequest {
        schema_id: STATE_CONTENT_SCHEMA.into(),
        seed_id,
        scheme_id: state.flow.scheme.scheme_id.clone(),
        ste100_scheme: scheme,
        checker_artifact_sha256: state.ste100.model_artifact_sha256.clone(),
        prompt_text: prompt,
        raw_block,
        bindings: certificate.bindings.clone(),
        certificate,
    };
    let verified = request
        .validate_structure(
            hash,
            &state.flow.scheme,
            &state.ste100.model_artifact_sha256,
        )
        .map_err(|refused| match refused {
            SeedRefusal::SeedCertificateInvalid => PreparationFailure::Certificate(
                "prompt, scheme, checker, bindings or reviewed duties do not agree".into(),
            ),
            other => invalid("seed", format!("invalid constructed request: {other:?}")),
        })?;
    Ok(PreparedSeed {
        draft,
        inventory,
        request,
        verified,
        checker_binary,
        checker_binary_sha256,
    })
}

fn read_text(
    directory: BorrowedFd<'_>,
    name: &str,
    owner: u32,
    group: Option<u32>,
    mode: Option<u32>,
) -> Result<String, PreparationFailure> {
    let mut file = open_judged(directory, name, owner, group, mode)?;
    let mut text = String::new();
    file.read_to_string(&mut text)
        .map_err(|error| invalid(name, format!("file does not read as UTF-8: {error}")))?;
    Ok(text)
}

fn open_judged(
    directory: BorrowedFd<'_>,
    name: &str,
    owner: u32,
    group: Option<u32>,
    mode: Option<u32>,
) -> Result<File, PreparationFailure> {
    let descriptor = nix::fcntl::openat(
        directory,
        name,
        nix::fcntl::OFlag::O_RDONLY
            | nix::fcntl::OFlag::O_NOFOLLOW
            | nix::fcntl::OFlag::O_NONBLOCK
            | nix::fcntl::OFlag::O_CLOEXEC,
        nix::sys::stat::Mode::empty(),
    )
    .map_err(|error| custody(name, error))?;
    let file = File::from(descriptor);
    let metadata = file.metadata().map_err(|error| custody(name, error))?;
    if !metadata.is_file()
        || metadata.uid() != owner
        || metadata.mode() & 0o022 != 0
        || group.is_some_and(|gid| metadata.gid() != gid)
        || mode.is_some_and(|expected| metadata.mode() & 0o7777 != expected)
    {
        return Err(custody(
            name,
            "file kind, owner, group or mode does not judge",
        ));
    }
    match crate::carries_access_entries_fd(file.as_fd()) {
        Ok(false) => Ok(file),
        Ok(true) => Err(custody(name, "file carries an access-control entry")),
        Err(error) => Err(custody(name, error)),
    }
}

fn judge_checker(path: &Path, owner: u32) -> Result<(PathBuf, Digest), PreparationFailure> {
    let metadata =
        std::fs::symlink_metadata(path).map_err(|error| custody("ste100-binary", error))?;
    if metadata.file_type().is_symlink() {
        return Err(custody("ste100-binary", "checker binary is a link"));
    }
    let canonical = crate::judge_ancestors(path, &[owner, 0])?;
    let parent = canonical
        .parent()
        .ok_or_else(|| invalid("ste100-binary", "checker binary has no parent"))?;
    let name = canonical
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| invalid("ste100-binary", "checker binary has no UTF-8 filename"))?;
    let directory = crate::save_points::open_directory(parent)
        .map_err(|error| custody("ste100-binary", error))?;
    let mut file = open_judged(directory.as_fd(), name, owner, None, None)?;
    if file
        .metadata()
        .map_err(|error| custody("ste100-binary", error))?
        .mode()
        & 0o111
        == 0
    {
        return Err(custody("ste100-binary", "checker file is not executable"));
    }
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 8192];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| custody("ste100-binary", error))?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    let digest =
        Digest::new(format!("{:x}", digest.finalize())).expect("SHA-256 is a lower-case digest");
    Ok((canonical, digest))
}

fn hash(bytes: &[u8]) -> Digest {
    Digest::new(format!("{:x}", Sha256::digest(bytes))).expect("SHA-256 is a digest")
}

fn invalid(field: &str, detail: impl Into<String>) -> PreparationFailure {
    PreparationFailure::Inventory {
        refusal: LifecycleRefusal::ConfigInvalid {
            field: Some(FieldName(field.into())),
        },
        detail: detail.into(),
    }
}

fn custody(field: &str, detail: impl std::fmt::Display) -> PreparationFailure {
    PreparationFailure::Inventory {
        refusal: LifecycleRefusal::BoundaryUnverified,
        detail: format!("{field}: {detail}"),
    }
}

#[cfg(test)]
pub(in crate::seed) mod tests;
