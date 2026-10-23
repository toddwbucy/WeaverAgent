//! Offline declaration and selector admission under ADMIN-SELECTION and
//! ADMIN-REBIND. The caller already owns root authorization and both locks.
//! This reads fixed inputs only, never a certificate, checker or state image.

use std::fs::File;
use std::io::Read;
use std::os::fd::{AsFd, BorrowedFd};
use std::os::unix::fs::MetadataExt;

use weaver_types::{AgentName, FieldName, LifecycleRefusal, ServingDraft};

use crate::save_points::serving::codec::ArtifactId;
use crate::{ServiceConfig, inventory};

#[derive(Debug)]
pub(super) struct Prepared {
    pub(super) draft: ServingDraft,
    pub(super) selector: Option<ArtifactId>,
}

pub(super) fn prepare(
    config: &ServiceConfig,
    agent: &AgentName,
    require_selector: bool,
) -> Result<Prepared, LifecycleRefusal> {
    crate::admissible(config, agent)?;
    let boundary = crate::inventory_boundary(config, agent)?;
    prepare_against(config, agent, require_selector, &boundary)
}

/// Separate host facts from file admission so the same path is exercised on
/// an unprovisioned test host. Only prepare supplies production host facts.
pub(super) fn prepare_against(
    config: &ServiceConfig,
    agent: &AgentName,
    require_selector: bool,
    boundary: &inventory::Boundary,
) -> Result<Prepared, LifecycleRefusal> {
    crate::admissible(config, agent)?;
    let owner = nix::unistd::geteuid().as_raw();
    crate::judge_root(&config.root, owner)?;
    let canonical_root = crate::judge_ancestors(&config.root, &[owner, 0])?;
    crate::refuse_retired_keys(&canonical_root, &agent.0)?;
    crate::judge_entries(&canonical_root, owner)?;
    let root = crate::save_points::open_directory(&canonical_root)
        .map_err(|_| LifecycleRefusal::BoundaryUnverified)?;
    let selector = read_selector(root.as_fd(), owner)?;
    if require_selector && selector.is_none() {
        return Err(selector_invalid());
    }

    // Both offline acts keep the existing room inventory's operational
    // checks. Restore consumes only this draft's independently judged
    // identity/path, never its operational settings as the selected start.
    let source = crate::read_declaration(config)?;
    let draft = weaver_types::parse_serving_draft(&source)
        .map_err(|error| LifecycleRefusal::ConfigInvalid { field: error.field })?;
    let mut inventory = inventory::take_serving_inventory(agent, &source, &draft, boundary)?;
    crate::sink_within_territory(&inventory.config.trace_sink, &config.territory)?;
    crate::grant_shell(config, &mut inventory)?;
    Ok(Prepared { draft, selector })
}

fn selector_invalid() -> LifecycleRefusal {
    LifecycleRefusal::ConfigInvalid {
        field: Some(FieldName("restore-artifact".into())),
    }
}

/// Only absence of the entry is absence of this optional key. In particular,
/// a dangling link, special file or failed read never selects latest instead.
fn read_selector(
    directory: BorrowedFd<'_>,
    owner: u32,
) -> Result<Option<ArtifactId>, LifecycleRefusal> {
    let descriptor = match nix::fcntl::openat(
        directory,
        "restore-artifact",
        nix::fcntl::OFlag::O_RDONLY
            | nix::fcntl::OFlag::O_NOFOLLOW
            | nix::fcntl::OFlag::O_NONBLOCK
            | nix::fcntl::OFlag::O_CLOEXEC,
        nix::sys::stat::Mode::empty(),
    ) {
        Ok(descriptor) => descriptor,
        Err(nix::errno::Errno::ENOENT) => return Ok(None),
        Err(_) => return Err(LifecycleRefusal::BoundaryUnverified),
    };
    let file = File::from(descriptor);
    let metadata = file
        .metadata()
        .map_err(|_| LifecycleRefusal::BoundaryUnverified)?;
    if !metadata.is_file() || metadata.uid() != owner || metadata.mode() & 0o022 != 0 {
        return Err(LifecycleRefusal::BoundaryUnverified);
    }
    match crate::carries_access_entries_fd(file.as_fd()) {
        Ok(false) => {}
        _ => return Err(LifecycleRefusal::BoundaryUnverified),
    }
    // The complete representation is 85 ASCII bytes and at most one LF.
    // One byte beyond it detects all longer files without an unbounded read.
    let mut bytes = Vec::new();
    file.take(87)
        .read_to_end(&mut bytes)
        .map_err(|_| selector_invalid())?;
    ArtifactId::parse_selector(&bytes)
        .map(Some)
        .map_err(|_| selector_invalid())
}

#[cfg(test)]
pub(in crate::offline_state) mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::{PermissionsExt, symlink};
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    pub(in crate::offline_state) struct Fixture {
        pub(in crate::offline_state) config: ServiceConfig,
        pub(in crate::offline_state) boundary: inventory::Boundary,
        _scratch: crate::scratch::Scratch,
    }

    impl Fixture {
        pub(in crate::offline_state) fn prepare(
            &self,
            require_selector: bool,
        ) -> Result<Prepared, LifecycleRefusal> {
            prepare_against(
                &self.config,
                &AgentName(self.config.agent.clone()),
                require_selector,
                &self.boundary,
            )
        }

        pub(in crate::offline_state) fn selector(&self, bytes: &[u8]) {
            write(&self.config.root.join("restore-artifact"), bytes, 0o640);
        }

        pub(in crate::offline_state) fn declaration(&self) -> PathBuf {
            self.config.territory.join("agent.toml")
        }

        pub(in crate::offline_state) fn edit(&self, from: &str, to: &str) {
            let path = self.declaration();
            let before = fs::read_to_string(&path).unwrap();
            assert!(before.contains(from));
            write(&path, before.replace(from, to).as_bytes(), 0o640);
        }
    }

    pub(in crate::offline_state) fn fixture() -> Fixture {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let base = std::env::temp_dir().join(format!(
            "weaver-offline-preparation-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&base).unwrap();
        fs::set_permissions(&base, fs::Permissions::from_mode(0o700)).unwrap();
        let scratch = crate::scratch::Scratch(base.clone());
        let root = base.join("root");
        let territory = base.join("territory");
        let binaries = base.join("bin");
        let coordination = base.join("coordination");
        for path in [&root, &territory, &binaries, &coordination] {
            fs::create_dir(path).unwrap();
            fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
        }
        fs::set_permissions(&territory, fs::Permissions::from_mode(0o710)).unwrap();
        let save_points = territory.join("save-points");
        fs::create_dir(&save_points).unwrap();
        fs::set_permissions(&save_points, fs::Permissions::from_mode(0o750)).unwrap();
        let source = DRAFT.replace(
            "\"TRACE_PATH\"",
            &serde_json::to_string(&territory.join("trace.jsonl")).unwrap(),
        );
        write(&territory.join("agent.toml"), source.as_bytes(), 0o640);
        let uid = nix::unistd::geteuid().as_raw();
        let gid = nix::unistd::getegid().as_raw();
        let member_gid = if gid == 0 { 65534 } else { gid };
        let config = ServiceConfig {
            agent: "offline-fixture".into(),
            coordination_root: coordination,
            worker: binaries.join("weaver-worker"),
            spu: binaries.join("weaver-spu"),
            gate: binaries.join("weaver-gate"),
            headroom_bytes: None,
            library_path: None,
            shell_binary: None,
            load_bound: crate::DEFAULT_LOAD_BOUND,
            territory: territory.clone(),
            territory_fd: Some(crate::save_points::open_directory(&territory).unwrap()),
            save_points: Some(crate::save_points::open_directory(&save_points).unwrap()),
            territory_unjudged: false,
            operator: uid,
            access_gid: gid,
            // Offline acts start no agent member and do not require the reader's
            // roles file, just as the existing restore room inventory does.
            boundary: Err("reader is not needed for an offline act".into()),
            root,
        };
        let boundary = inventory::Boundary {
            agent_uid: uid.wrapping_add(1),
            admin_uid: uid,
            agent_gids: vec![member_gid.wrapping_add(1)],
            home: base,
            member_binary: Some(config.worker.with_file_name("weaver-state")),
            member_account: Some(inventory::MemberAccount {
                uid: if uid == 0 { 65534 } else { uid },
                gid: member_gid,
            }),
            member_group: Some(member_gid),
        };
        Fixture {
            config,
            boundary,
            _scratch: scratch,
        }
    }

    fn write(path: &Path, bytes: &[u8], mode: u32) {
        fs::write(path, bytes).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
    }

    fn id() -> String {
        format!("{:020}-{}", 1, "a".repeat(64))
    }

    fn assert_selector_invalid(result: Result<Prepared, LifecycleRefusal>) {
        assert!(matches!(
            result,
            Err(LifecycleRefusal::ConfigInvalid { field: Some(FieldName(field)) })
                if field == "restore-artifact"
        ));
    }

    #[test]
    fn offline_preparation_needs_no_seed_inputs_or_live_checker() {
        let fixture = fixture();
        let prepared = fixture.prepare(false).unwrap();
        assert!(prepared.selector.is_none());
        assert_eq!(prepared.draft.agent().session.0, "offline-session");
        assert!(prepared.draft.state_management().start.is_none());
        assert_eq!(fs::read_dir(&fixture.config.root).unwrap().count(), 0);
        assert_eq!(
            fs::read_dir(fixture.config.territory.join("save-points"))
                .unwrap()
                .count(),
            0
        );
        assert!(!fixture.config.territory.join("state").exists());
    }

    #[test]
    fn selector_is_optional_for_bind_and_required_for_restore() {
        let fixture = fixture();
        assert!(fixture.prepare(false).unwrap().selector.is_none());
        assert_selector_invalid(fixture.prepare(true));
        for bytes in [id(), format!("{}\n", id())] {
            fixture.selector(bytes.as_bytes());
            for required in [false, true] {
                assert_eq!(
                    fixture
                        .prepare(required)
                        .unwrap()
                        .selector
                        .unwrap()
                        .as_str(),
                    id()
                );
            }
        }
    }

    #[test]
    fn selector_grammar_is_exact_and_never_defaults_on_bad_bytes() {
        let fixture = fixture();
        let invalid = [
            Vec::new(),
            format!(" {}", id()).into_bytes(),
            format!("{} ", id()).into_bytes(),
            format!("{}\r\n", id()).into_bytes(),
            format!("{}\n\n", id()).into_bytes(),
            format!("/{}", id()).into_bytes(),
            "a".repeat(64).into_bytes(),
            format!("{:020}-{}", 1, "A".repeat(64)).into_bytes(),
            vec![0xff],
            vec![b'x'; 4096],
        ];
        for bytes in invalid {
            fixture.selector(&bytes);
            assert_selector_invalid(fixture.prepare(false));
        }
    }

    #[test]
    fn selector_refuses_links_special_files_open_modes_and_wrong_owner() {
        let fixture = fixture();
        let name = fixture.config.root.join("restore-artifact");
        let target = fixture.config.root.join("target");
        let root = crate::save_points::open_directory(&fixture.config.root).unwrap();
        let refused = || {
            assert!(matches!(
                fixture.prepare(false),
                Err(LifecycleRefusal::BoundaryUnverified)
            ));
            assert!(matches!(
                read_selector(root.as_fd(), nix::unistd::geteuid().as_raw()),
                Err(LifecycleRefusal::BoundaryUnverified)
            ));
        };
        write(&target, id().as_bytes(), 0o640);
        symlink(&target, &name).unwrap();
        refused();
        fs::remove_file(&target).unwrap();
        refused();
        fs::remove_file(&name).unwrap();
        fs::create_dir(&name).unwrap();
        refused();
        fs::remove_dir(&name).unwrap();
        nix::unistd::mkfifo(&name, nix::sys::stat::Mode::S_IRUSR).unwrap();
        refused();
        fs::remove_file(&name).unwrap();
        fixture.selector(id().as_bytes());
        for mode in [0o660, 0o642] {
            fs::set_permissions(&name, fs::Permissions::from_mode(mode)).unwrap();
            refused();
        }
        fs::set_permissions(&name, fs::Permissions::from_mode(0o640)).unwrap();
        assert!(matches!(
            read_selector(
                root.as_fd(),
                nix::unistd::geteuid().as_raw().wrapping_add(1)
            ),
            Err(LifecycleRefusal::BoundaryUnverified)
        ));
    }

    #[test]
    fn held_root_descriptor_does_not_follow_a_replacement_path() {
        let fixture = fixture();
        fixture.selector(id().as_bytes());
        let held = crate::save_points::open_directory(&fixture.config.root).unwrap();
        let moved = fixture.config.root.with_file_name("held-root");
        fs::rename(&fixture.config.root, &moved).unwrap();
        fs::create_dir(&fixture.config.root).unwrap();
        write(&fixture.config.root.join("restore-artifact"), b"bad", 0o640);
        assert_eq!(
            read_selector(held.as_fd(), nix::unistd::geteuid().as_raw())
                .unwrap()
                .unwrap()
                .as_str(),
            id()
        );
    }

    #[test]
    fn selector_descriptor_refuses_an_acl_hidden_by_the_closed_mode() {
        use std::os::fd::AsRawFd;

        let fixture = fixture();
        fixture.selector(id().as_bytes());
        let root = crate::save_points::open_directory(&fixture.config.root).unwrap();
        let owner = nix::unistd::geteuid().as_raw();
        assert!(read_selector(root.as_fd(), owner).unwrap().is_some());

        let selector = File::open(fixture.config.root.join("restore-artifact")).unwrap();
        let mode = selector.metadata().unwrap().mode();
        let mut acl = 2u32.to_le_bytes().to_vec();
        // The named reader adds authority while the mask preserves 0640.
        // This is Linux's POSIX ACL xattr format, as in repository tests.
        for (tag, permissions, id) in [
            (1u16, ((mode >> 6) & 7) as u16, u32::MAX),
            (2u16, 4u16, owner ^ 1),
            (4u16, ((mode >> 3) & 7) as u16, u32::MAX),
            (16u16, ((mode >> 3) & 7) as u16, u32::MAX),
            (32u16, (mode & 7) as u16, u32::MAX),
        ] {
            acl.extend_from_slice(&tag.to_le_bytes());
            acl.extend_from_slice(&permissions.to_le_bytes());
            acl.extend_from_slice(&id.to_le_bytes());
        }
        // SAFETY: the descriptor is held, the name is NUL terminated, and
        // the complete buffer remains readable for this synchronous call.
        let result = unsafe {
            nix::libc::fsetxattr(
                selector.as_raw_fd(),
                c"system.posix_acl_access".as_ptr(),
                acl.as_ptr().cast(),
                acl.len(),
                0,
            )
        };
        assert_eq!(
            result,
            0,
            "ACL fixture setup must succeed: {}",
            std::io::Error::last_os_error()
        );
        assert_eq!(selector.metadata().unwrap().mode(), mode);
        assert_eq!(mode & 0o7777, 0o640);
        assert!(crate::judge_entries(&fixture.config.root, owner).is_ok());
        assert!(crate::carries_access_entries_fd(selector.as_fd()).unwrap());
        // Call the descriptor reader directly, so no earlier root judgment
        // can make this pass after its own ACL refusal is removed.
        assert!(matches!(
            read_selector(root.as_fd(), owner),
            Err(LifecycleRefusal::BoundaryUnverified)
        ));
    }

    #[test]
    fn root_ancestry_and_retired_keys_are_judged() {
        let fixture = fixture();
        let parent = fixture.config.root.parent().unwrap();
        fs::set_permissions(parent, fs::Permissions::from_mode(0o777)).unwrap();
        assert!(matches!(
            fixture.prepare(false),
            Err(LifecycleRefusal::BoundaryUnverified)
        ));
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700)).unwrap();
        write(&fixture.config.root.join("run-tool"), b"retired", 0o600);
        assert!(matches!(
            fixture.prepare(false),
            Err(LifecycleRefusal::ConfigInvalid { .. })
        ));
    }

    #[test]
    fn declaration_is_read_through_held_territory_and_requires_exact_custody() {
        let fixture = fixture();
        let path = fixture.declaration();
        for mode in [0o600, 0o644, 0o660] {
            fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();
            assert!(matches!(
                fixture.prepare(false),
                Err(LifecycleRefusal::BoundaryUnverified)
            ));
        }
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
        let elsewhere = fixture.config.root.join("elsewhere");
        fs::rename(&path, &elsewhere).unwrap();
        symlink(&elsewhere, &path).unwrap();
        assert!(matches!(
            fixture.prepare(false),
            Err(LifecycleRefusal::BoundaryUnverified)
        ));
    }

    #[test]
    fn every_offline_act_runs_the_shared_operational_inventory() {
        for required in [false, true] {
            let fixture = fixture();
            fixture.selector(id().as_bytes());
            fixture.edit("refeed-permission = false", "refeed-permission = true");
            assert!(matches!(
                fixture.prepare(required),
                Err(LifecycleRefusal::ConfigInvalid { field: Some(FieldName(field)) })
                    if field == "spu-instruction.decoder.refeed-permission"
            ));
        }
        let mut fixture = fixture();
        fixture.boundary.member_binary = None;
        assert!(matches!(
            fixture.prepare(false),
            Err(LifecycleRefusal::BoundaryUnverified)
        ));
    }

    #[test]
    fn sink_is_held_to_the_judged_territory_and_shell_grant_is_not_skipped() {
        let fixture = fixture();
        let other = fixture.config.root.with_file_name("other-territory");
        fs::create_dir(&other).unwrap();
        fs::set_permissions(&other, fs::Permissions::from_mode(0o700)).unwrap();
        fixture.edit(
            &serde_json::to_string(&fixture.config.territory.join("trace.jsonl")).unwrap(),
            &serde_json::to_string(&other.join("trace.jsonl")).unwrap(),
        );
        assert!(matches!(
            fixture.prepare(false),
            Err(LifecycleRefusal::ConfigInvalid { field: Some(FieldName(field)) })
                if field == "trace-sink"
        ));
        let fixture = self::fixture();
        fixture.edit("tool-set = []", "tool-set = [\"sh\"]");
        fixture.edit(
            "state-management.tools = {}",
            "state-management.tools.sh.clock-ms = 60000\nstate-management.tools.sh.measurement-ref = \"synthetic-shell\"",
        );
        assert!(
            weaver_types::parse_serving_draft(&fs::read_to_string(fixture.declaration()).unwrap())
                .is_ok()
        );
        assert!(matches!(
            fixture.prepare(false),
            Err(LifecycleRefusal::ConfigInvalid { field: None })
        ));
    }

    const DRAFT: &str = r#"session = "offline-session"
tool-set = []
binding-kind = "serving"
spu-instruction.decoder.model-binding.artifact = "synthetic-decoder"
spu-instruction.decoder.model-binding.devices = [0]
spu-instruction.decoder.residual-readout-election = false
spu-instruction.decoder.surprisal-election = false
spu-instruction.decoder.refeed-permission = false
spu-instruction.decoder.column-permission = false
spu-instruction.decoder.tunable-values = {}
gate-instruction.access-rule.allowed-uids = [0]
gate-instruction.access-rule.allowed-gids = []
gate-instruction.access-rule.denied-uids = []
trace-sink.kind = "file"
trace-sink.path = "TRACE_PATH"
trace-sink.create = true
state-store.engine = "sqlite"
state-management.flow.version = 1
state-management.flow.partition = []
state-management.flow.gate-wait.elapsed-ns = "11"
state-management.flow.gate-wait.measurement-ref = "synthetic-gate"
state-management.flow.cut-wait.elapsed-ns = "12"
state-management.flow.cut-wait.measurement-ref = "synthetic-cut"
state-management.flow.observation-interval.elapsed-ns = "13"
state-management.flow.observation-interval.measurement-ref = "synthetic-observation"
state-management.flow.backlog.max-pending-blocks = "14"
state-management.flow.backlog.max-pending-bytes = "15"
state-management.flow.backlog.lag-alarm-turns = "16"
state-management.flow.backlog.measurement-ref = "synthetic-backlog"
state-management.flow.scheme.scheme-id = "ste100:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
state-management.flow.scheme.artifact-sha256 = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
state-management.flow.scheme.verbs = ["read"]
state-management.persistence.save-wait.elapsed-ns = "17"
state-management.persistence.save-wait.measurement-ref = "synthetic-save"
state-management.persistence.snapshot-wait.elapsed-ns = "120000000000"
state-management.persistence.snapshot-wait.measurement-ref = "synthetic-snapshot"
state-management.reads.max-result-bytes = "1024"
state-management.reads.measurement-ref = "synthetic-read"
state-management.ste100.model-binding.artifact = "/synthetic/checker-manifest.json"
state-management.ste100.model-binding.devices = [1]
state-management.ste100.model-artifact-sha256 = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
state-management.ste100.instances = 1
state-management.ste100.max-wire-bytes = "1024"
state-management.ste100.measurement-ref = "synthetic-checker"
state-management.ste100.repair-cap.attempts = 3
state-management.ste100.repair-cap.measurement-ref = "synthetic-repair"
state-management.ste100.decision-wait.elapsed-ns = "20"
state-management.ste100.decision-wait.measurement-ref = "synthetic-decision"
state-management.tools = {}
state-management.placement.c-head = 21
state-management.placement.c-tail = 22
state-management.placement.measurement-ref = "synthetic-placement"
"#;
}
