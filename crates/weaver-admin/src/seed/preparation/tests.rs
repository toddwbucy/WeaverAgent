//! Synthetic custody and certificate fixtures, never language qualification.

use super::*;
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::sync::atomic::{AtomicU64, Ordering};

use weaver_types::state_flow::{Binding, NounBinding, PromptDuty, Span};

pub(crate) struct Fixture {
    pub(crate) config: ServiceConfig,
    pub(in crate::seed) prepared: PreparedSeed,
    pub(crate) boundary: inventory::Boundary,
    _scratch: crate::scratch::Scratch,
}

impl Fixture {
    fn prepare(&self) -> Result<PreparedSeed, PreparationFailure> {
        prepare_against(
            &self.config,
            &AgentName(self.config.agent.clone()),
            "1".repeat(32),
            &self.boundary,
        )
    }

    fn write_territory(&self, name: &str, bytes: &[u8]) {
        write(&self.config.territory.join(name), bytes, 0o640);
    }

    fn certificate(&self) -> SeedCertificate {
        self.prepared.request.certificate.clone()
    }
}

pub(crate) fn fixture() -> Fixture {
    let uid = nix::unistd::geteuid().as_raw();
    let gid = nix::unistd::getegid().as_raw();
    fixture_with_member(inventory::MemberAccount {
        uid: if uid == 0 { 65534 } else { uid },
        gid: if gid == 0 { 65534 } else { gid },
    })
}

/// Test-only host identity input. The same private preparation validates it,
/// and root host instruments can exercise an actual distinct credential drop.
pub(crate) fn fixture_with_member(member: inventory::MemberAccount) -> Fixture {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let base = std::env::temp_dir().join(format!(
        "weaver-seed-preparation-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&base).unwrap();
    // A distinct member may traverse to its territory, but cannot list the
    // fixture root. Each sibling retains its own narrower custody.
    fs::set_permissions(&base, fs::Permissions::from_mode(0o711)).unwrap();
    let scratch = crate::scratch::Scratch(base.clone());
    let root = base.join("root");
    let territory = base.join("territory");
    let binaries = base.join("bin");
    for directory in [&root, &territory, &binaries] {
        fs::create_dir(directory).unwrap();
        fs::set_permissions(directory, fs::Permissions::from_mode(0o700)).unwrap();
    }
    if member.gid != nix::unistd::getegid().as_raw() {
        nix::unistd::chown(
            &territory,
            None,
            Some(nix::unistd::Gid::from_raw(member.gid)),
        )
        .unwrap();
    }
    fs::set_permissions(&territory, fs::Permissions::from_mode(0o710)).unwrap();
    fs::create_dir(territory.join("save-points")).unwrap();
    fs::set_permissions(
        territory.join("save-points"),
        fs::Permissions::from_mode(0o750),
    )
    .unwrap();
    let checker = binaries.join("weaver-spu-ste100");
    write(&checker, b"synthetic executable bytes, never run", 0o700);
    write(
        &root.join("ste100-binary"),
        format!("{}\n", checker.display()).as_bytes(),
        0o640,
    );
    let scheme = scheme();
    let scheme_digest = hash(scheme.as_bytes());
    write(&root.join("ste100-scheme.json"), scheme.as_bytes(), 0o640);
    let prompt = "Read item.";
    let model_digest = hash(b"synthetic model manifest, never qualified");
    let certificate = SeedCertificate {
        prompt_sha256: hash(prompt.as_bytes()),
        scheme_id: format!("ste100:{scheme_digest}"),
        checker_artifact_sha256: model_digest.clone(),
        review_reference: "synthetic fixture, not operator qualification".into(),
        bindings: vec![binding(prompt)],
        duties: PromptDuty::ALL.to_vec(),
    };
    write(
        &territory.join("system-prompt.md"),
        prompt.as_bytes(),
        0o640,
    );
    write(
        &territory.join("system-prompt-certificate.json"),
        &serde_json::to_vec(&certificate).unwrap(),
        0o640,
    );
    let source = draft(&territory, &scheme_digest, &model_digest);
    write(&territory.join("agent.toml"), source.as_bytes(), 0o640);
    let uid = nix::unistd::geteuid().as_raw();
    let gid = nix::unistd::getegid().as_raw();
    let config = ServiceConfig {
        agent: "seed-fixture".into(),
        coordination_root: base.join("coordination"),
        worker: binaries.join("weaver-worker"),
        spu: binaries.join("weaver-spu"),
        gate: binaries.join("weaver-gate"),
        headroom_bytes: None,
        library_path: None,
        shell_binary: None,
        load_bound: crate::DEFAULT_LOAD_BOUND,
        territory: territory.clone(),
        territory_fd: Some(crate::save_points::open_directory(&territory).unwrap()),
        save_points: Some(
            crate::save_points::open_directory(&territory.join("save-points")).unwrap(),
        ),
        territory_unjudged: false,
        operator: uid,
        access_gid: gid,
        boundary: Ok(crate::BoundaryRead {
            digest: "0".repeat(64),
            reader: "synthetic-reader".into(),
        }),
        root,
    };
    let boundary = inventory::Boundary {
        agent_uid: uid.wrapping_add(1),
        admin_uid: uid,
        agent_gids: vec![member.gid.wrapping_add(1)],
        home: base,
        member_binary: Some(config.worker.with_file_name("weaver-state")),
        member_account: Some(member),
        member_group: Some(member.gid),
    };
    let prepared = prepare_against(
        &config,
        &AgentName(config.agent.clone()),
        "1".repeat(32),
        &boundary,
    )
    .unwrap();
    Fixture {
        config,
        prepared,
        boundary,
        _scratch: scratch,
    }
}

fn write(path: &Path, bytes: &[u8], mode: u32) {
    fs::write(path, bytes).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}

fn binding(text: &str) -> Binding {
    Binding {
        statement_span: Span {
            start: 0.into(),
            end: (text.len() as u64).into(),
        },
        verb: "read".into(),
        verb_span: Span {
            start: 0.into(),
            end: 4.into(),
        },
        condition: None,
        condition_span: None,
        nouns: vec![NounBinding {
            value: "item".into(),
            span: Span {
                start: 5.into(),
                end: 9.into(),
            },
        }],
    }
}

fn scheme() -> String {
    serde_json::json!({
        "format":"ste100-scheme/1",
        "reference":{"edition":"synthetic-seed-test",
            "document_sha256":hash(b"reference"),"rule_ids":["test-rule"],
            "dictionary_sha256":hash(b"dictionary")},
        "grammar":{"format":"test-grammar/1","bytes_base64":"Z3JhbW1hcg==",
            "sha256":hash(b"grammar")},
        "verbs":[{"key":"read","forms":["Read"],"sense":"test",
            "authority":{"rule_id":"test-rule","disposition":"approved_word"}}],
        "technical_names":[],
        "conditions":{"format":"test-conditions/1","bytes_base64":"Y29uZGl0aW9ucw==",
            "sha256":hash(b"conditions")},
        "recall_keys":["read"],
        "qualification":{"report_sha256":hash(b"synthetic report"),
            "corpus_sha256":hash(b"synthetic corpus"),"checker_abi":"test/1"}
    })
    .to_string()
}

fn draft(territory: &Path, scheme: &Digest, checker: &Digest) -> String {
    let trace = serde_json::to_string(&territory.join("trace.jsonl")).unwrap();
    format!(
        r#"session = "fixture-session"
tool-set = []
binding-kind = "serving"
spu-instruction.decoder.model-binding.artifact = "synthetic-decoder"
spu-instruction.decoder.model-binding.devices = [0]
spu-instruction.decoder.residual-readout-election = false
spu-instruction.decoder.surprisal-election = false
spu-instruction.decoder.refeed-permission = false
spu-instruction.decoder.column-permission = false
spu-instruction.decoder.tunable-values = {{}}
gate-instruction.access-rule.allowed-uids = [0]
gate-instruction.access-rule.allowed-gids = []
gate-instruction.access-rule.denied-uids = []
trace-sink.kind = "file"
trace-sink.path = {trace}
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
state-management.flow.scheme.scheme-id = "ste100:{scheme}"
state-management.flow.scheme.artifact-sha256 = "{scheme}"
state-management.flow.scheme.verbs = ["read"]
state-management.persistence.save-wait.elapsed-ns = "17"
state-management.persistence.save-wait.measurement-ref = "synthetic-save"
state-management.persistence.snapshot-wait.elapsed-ns = "120000000000"
state-management.persistence.snapshot-wait.measurement-ref = "synthetic-snapshot"
state-management.reads.max-result-bytes = "1024"
state-management.reads.measurement-ref = "synthetic-read"
state-management.ste100.model-binding.artifact = "/synthetic/checker-manifest.json"
state-management.ste100.model-binding.devices = [1]
state-management.ste100.model-artifact-sha256 = "{checker}"
state-management.ste100.instances = 1
state-management.ste100.max-wire-bytes = "1024"
state-management.ste100.measurement-ref = "synthetic-checker"
state-management.ste100.repair-cap.attempts = 3
state-management.ste100.repair-cap.measurement-ref = "synthetic-repair"
state-management.ste100.decision-wait.elapsed-ns = "20"
state-management.ste100.decision-wait.measurement-ref = "synthetic-decision"
state-management.tools = {{}}
state-management.placement.c-head = 21
state-management.placement.c-tail = 22
state-management.placement.measurement-ref = "synthetic-placement"
"#
    )
}

#[test]
fn preparation_retains_exact_certified_bytes_without_an_existing_artifact() {
    let fixture = fixture();
    let prepared = &fixture.prepared;
    assert!(prepared.draft().state_management().start.is_none());
    assert_eq!(prepared.request().prompt_text, "Read item.");
    assert_eq!(prepared.request().bindings, prepared.verified.bindings());
    assert_eq!(prepared.verified.prompt_sha256(), &hash(b"Read item."));
    assert_ne!(
        prepared.checker_binary_sha256(),
        &prepared.request().checker_artifact_sha256,
        "executable digest and declared model digest are distinct facts"
    );
    assert_eq!(
        fs::read_dir(fixture.config.territory.join("save-points"))
            .unwrap()
            .count(),
        0,
        "preparation never publishes or requires a prior artifact"
    );
    assert!(!fixture.config.territory.join("state").exists());
}

#[test]
fn certificate_cannot_supply_its_own_expected_prompt_checker_or_scheme_identity() {
    for changed in [
        "prompt",
        "checker",
        "scheme",
        "duty",
        "binding",
        "empty-bindings",
    ] {
        let fixture = fixture();
        let mut certificate = fixture.certificate();
        match changed {
            "prompt" => certificate.prompt_sha256 = hash(b"another prompt"),
            "checker" => certificate.checker_artifact_sha256 = hash(b"another checker"),
            "scheme" => certificate.scheme_id = format!("ste100:{}", hash(b"another scheme")),
            "duty" => {
                certificate.duties.pop();
            }
            "binding" => certificate.bindings[0].nouns[0].value = "forged".into(),
            "empty-bindings" => certificate.bindings.clear(),
            _ => unreachable!(),
        }
        fixture.write_territory(
            "system-prompt-certificate.json",
            &serde_json::to_vec(&certificate).unwrap(),
        );
        assert!(
            matches!(fixture.prepare(), Err(PreparationFailure::Certificate(_))),
            "{changed} mismatch must retain the persistence certificate refusal"
        );
    }
}

#[test]
fn certificate_duplicates_are_refused_before_a_seed_request_exists() {
    let fixture = fixture();
    let serialized = serde_json::to_string(&fixture.certificate()).unwrap();
    let duplicate = serialized.replacen('{', "{\"review_reference\":\"forged\",", 1);
    fixture.write_territory("system-prompt-certificate.json", duplicate.as_bytes());
    assert!(matches!(
        fixture.prepare(),
        Err(PreparationFailure::Certificate(_))
    ));
}

#[test]
fn changing_the_held_prompt_or_scheme_does_not_reuse_the_old_certificate() {
    for changed in ["prompt", "scheme"] {
        let fixture = fixture();
        if changed == "prompt" {
            fixture.write_territory("system-prompt.md", b"Read item.\n");
        } else {
            write(
                &fixture.config.root.join("ste100-scheme.json"),
                format!("{}\n", fixture.prepared.request.ste100_scheme).as_bytes(),
                0o640,
            );
        }
        assert!(matches!(
            fixture.prepare(),
            Err(PreparationFailure::Certificate(_))
        ));
    }
}

#[test]
fn seed_input_files_refuse_symlinks_and_wider_modes() {
    for name in [
        "agent.toml",
        "system-prompt.md",
        "system-prompt-certificate.json",
    ] {
        for changed in ["link", "mode"] {
            let fixture = fixture();
            let path = fixture.config.territory.join(name);
            if changed == "link" {
                let moved = path.with_extension("original");
                fs::rename(&path, &moved).unwrap();
                symlink(&moved, &path).unwrap();
            } else {
                fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
            }
            assert!(
                matches!(
                    fixture.prepare(),
                    Err(PreparationFailure::Inventory {
                        refusal: LifecycleRefusal::BoundaryUnverified,
                        ..
                    })
                ),
                "{name} {changed} must fail custody"
            );
        }
    }
}

#[test]
fn checker_custody_refuses_links_writable_files_and_missing_execute_permission() {
    for changed in ["link", "writable", "not-executable", "relative"] {
        let fixture = fixture();
        let path = fixture.prepared.checker_binary();
        match changed {
            "link" => {
                let moved = path.with_extension("original");
                fs::rename(path, &moved).unwrap();
                symlink(moved, path).unwrap();
            }
            "writable" => fs::set_permissions(path, fs::Permissions::from_mode(0o722)).unwrap(),
            "not-executable" => {
                fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap()
            }
            "relative" => write(
                &fixture.config.root.join("ste100-binary"),
                b"relative",
                0o640,
            ),
            _ => unreachable!(),
        }
        assert!(
            matches!(fixture.prepare(), Err(PreparationFailure::Inventory { .. })),
            "{changed}"
        );
    }
}

#[test]
fn checker_ancestor_alias_is_resolved_once_and_only_the_canonical_path_is_retained() {
    let fixture = fixture();
    let canonical = fixture.prepared.checker_binary();
    let alias = fixture.config.root.parent().unwrap().join("binary-alias");
    symlink(canonical.parent().unwrap(), &alias).unwrap();
    let aliased = alias.join(canonical.file_name().unwrap());
    write(
        &fixture.config.root.join("ste100-binary"),
        aliased.to_str().unwrap().as_bytes(),
        0o640,
    );
    let prepared = fixture.prepare().unwrap();
    assert_eq!(prepared.checker_binary(), canonical);
    assert_ne!(prepared.checker_binary(), aliased);
    assert_eq!(
        prepared.checker_binary_sha256(),
        fixture.prepared.checker_binary_sha256()
    );
}

#[test]
fn seed_inventory_reuses_the_existing_boundary_and_permission_judgments() {
    let fixture = fixture();
    for changed in ["home", "member", "permission", "sink"] {
        let mut boundary = fixture.boundary.clone();
        let original = fs::read_to_string(fixture.config.territory.join("agent.toml")).unwrap();
        match changed {
            "home" => boundary.home = fixture.config.territory.join("absent-home"),
            "member" => boundary.member_account = None,
            "permission" => fixture.write_territory(
                "agent.toml",
                original
                    .replace("refeed-permission = false", "refeed-permission = true")
                    .as_bytes(),
            ),
            "sink" => boundary.agent_uid = nix::unistd::geteuid().as_raw(),
            _ => unreachable!(),
        }
        assert!(
            prepare_against(
                &fixture.config,
                &AgentName(fixture.config.agent.clone()),
                "1".repeat(32),
                &boundary
            )
            .is_err(),
            "{changed}"
        );
        fixture.write_territory("agent.toml", original.as_bytes());
    }
}

#[test]
fn seed_inventory_refuses_a_root_member_or_the_agents_own_uid() {
    let fixture = fixture();
    for uid in [0, fixture.boundary.agent_uid] {
        let mut boundary = fixture.boundary.clone();
        boundary.member_account.as_mut().unwrap().uid = uid;
        assert!(
            matches!(
                prepare_against(
                    &fixture.config,
                    &AgentName(fixture.config.agent.clone()),
                    "1".repeat(32),
                    &boundary,
                ),
                Err(PreparationFailure::Inventory {
                    refusal: LifecycleRefusal::BoundaryUnverified,
                    ..
                })
            ),
            "member uid {uid} must not acquire a prepared seed"
        );
    }
}

#[test]
fn preparation_does_not_accept_a_draft_for_a_different_agent() {
    let fixture = fixture();
    assert!(matches!(
        prepare_against(
            &fixture.config,
            &AgentName("other".into()),
            "1".repeat(32),
            &fixture.boundary
        ),
        Err(PreparationFailure::Inventory {
            refusal: LifecycleRefusal::NoSuchAgent,
            ..
        })
    ));
}
