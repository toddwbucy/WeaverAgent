//! The actual admin participant in the worker/SQLite capture circuit. Its
//! parent owns the temporary files and starts it outside the agent cgroup.
use super::*;
use crate::save_points::serving::publication::{
    Progress as PublicationProgress, Publication, SeedImage,
};
use crate::save_points::serving::repository::LoadScope;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::Instant;
use weaver_types::state_flow::enter::{EnterBinding, EnterPayload};
use weaver_types::state_flow::{SeedCreated, StartIdentity};

fn wait(root: &Path, name: &str) -> Vec<u8> {
    let end = Instant::now() + Duration::from_secs(25);
    loop {
        match fs::read(root.join(name)) {
            Ok(bytes) => return bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => panic!("{name}: {e}"),
        }
        assert!(Instant::now() < end, "joint worker did not provide {name}");
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn write(root: &Path, name: &str, value: &serde_json::Value) {
    let part = root.join(format!("{name}.part"));
    fs::write(&part, serde_json::to_vec(value).unwrap()).unwrap();
    fs::rename(part, root.join(name)).unwrap();
}

fn config(root: &Path, agent: String) -> ServiceConfig {
    let territory = root.join("territory");
    for (path, mode) in [
        (root.join("admin"), 0o700),
        (root.join("coord"), 0o755),
        (territory.clone(), 0o710),
        (territory.join("state"), 0o700),
        (territory.join("save-points"), 0o750),
        (root.join("seed-source"), 0o700),
    ] {
        fs::create_dir(&path).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
    }
    existing_config(root, agent)
}

fn existing_config(root: &Path, agent: String) -> ServiceConfig {
    let territory = root.join("territory");
    ServiceConfig {
        agent,
        coordination_root: root.join("coord"),
        worker: root.join("unused-worker"),
        spu: root.join("unused-spu"),
        gate: root.join("unused-gate"),
        headroom_bytes: None,
        library_path: None,
        shell_binary: None,
        load_bound: Duration::from_secs(30),
        territory_fd: Some(save_points::open_directory(&territory).unwrap()),
        save_points: Some(save_points::open_directory(&territory.join("save-points")).unwrap()),
        territory,
        territory_unjudged: false,
        operator: 42,
        access_gid: 0,
        boundary: Err("synthetic root fixture, not role qualification".into()),
        root: root.join("admin"),
    }
}

struct JoiningAdmin(std::process::Child);
impl Drop for JoiningAdmin {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
#[ignore = "separate invocation launched by the demand-save joint participant"]
fn demand_save_joint_competing_admin() {
    let root =
        PathBuf::from(std::env::var_os("WEAVER_LEAVE_JOINT_ROOT").expect("joint parent root"));
    let host: serde_json::Value = serde_json::from_slice(&wait(&root, "host.json")).unwrap();
    let config = existing_config(&root, host["agent"].as_str().unwrap().into());
    let failure = crate::serving_leave::save::run(
        &config,
        &AgentName(config.agent.clone()),
        Cause { uid: 75 },
        |_| Ok(0),
    )
    .unwrap_err();
    assert!(
        matches!(
            failure.cause,
            crate::serving_leave::save::Fault::Admission(LifecycleRefusal::InvocationInFlight)
        ),
        "second save: {failure:?}"
    );
    let failure = run_authorized(
        &config,
        &AgentName(config.agent.clone()),
        Cause { uid: 75 },
        crate::UNLOAD_BOUNDS,
        |_| Ok(0),
    )
    .unwrap_err();
    assert!(
        matches!(
            failure.cause,
            Fault::Admission(LifecycleRefusal::InvocationInFlight)
        ),
        "forced unload during save: {failure:?}"
    );
}

#[test]
#[ignore = "separate force invocation in the capture-refusal joint circuit"]
fn capture_refusal_joint_force_admin() {
    let root = PathBuf::from(std::env::var_os("WEAVER_LEAVE_JOINT_ROOT").unwrap());
    let host: serde_json::Value = serde_json::from_slice(&wait(&root, "host.json")).unwrap();
    wait(&root, "capture-collision.json");
    let config = existing_config(&root, host["agent"].as_str().unwrap().into());
    crate::TEST_WORKER_UID.with(|uid| uid.set(Some(0)));
    let result = run_authorized(
        &config,
        &AgentName(config.agent.clone()),
        Cause { uid: 75 },
        crate::UNLOAD_BOUNDS,
        |_| Ok(0),
    )
    .unwrap();
    crate::TEST_WORKER_UID.with(|uid| uid.set(None));
    let evidence = result
        .joined
        .as_ref()
        .expect("force joined the held original unload");
    let received = evidence.received.as_ref().unwrap();
    assert!(matches!(
        received.envelope.payload,
        weaver_types::state_flow::capture::CapturePayload::Answer(
            weaver_types::state_flow::capture::CaptureAnswer::Left {
                save_point: None,
                forced: true,
                no_state: false,
            }
        )
    ));
    fs::write(root.join("joined-refused.json"), &received.wire).unwrap();
}

fn publish_seed(root: &Path, config: &ServiceConfig, payload: &EnterPayload) -> StartIdentity {
    let EnterBinding::Serving {
        gate_instruction,
        state_management,
        ..
    } = &payload.binding
    else {
        panic!("serving circuit");
    };
    let agent = weaver_types::AgentConfig {
        session: payload.session.clone(),
        spu_instruction: payload.spu_instruction.clone(),
        tool_set: vec![],
        binding_kind: Some(weaver_types::BindingKind::Serving),
        gate_instruction: Some(gate_instruction.clone()),
        trace_sink: weaver_types::TraceSink::File {
            path: config.territory.join("trace.jsonl"),
            create: true,
        },
        state_election: None,
        state_store: Some(payload.state_store.clone()),
        restore: None,
        lifecycle: Some(weaver_types::Lifecycle {
            drain_bound: payload.drain_bound,
            wind_down_bound: payload.wind_down_bound,
        }),
    };
    let bound = crate::save_points::serving::declaration::render_fixture(&agent, state_management);
    let draft_text = bound
        .lines()
        .filter(|line| !line.starts_with("state-management.start."))
        .collect::<Vec<_>>()
        .join("\n");
    let draft = weaver_types::parse_serving_draft(&draft_text).unwrap();
    let trace_path = crate::save_points::serving::trace_path(&agent.trace_sink);
    let scope = LoadScope {
        session: &agent.session,
        trace_path,
    };
    let image = fs::read(root.join("seed.sqlite")).unwrap();
    let seed_id = "a".repeat(32);
    let room_name = format!("seed-{seed_id}.sqlite");
    let room_path = root.join("seed-source");
    fs::write(room_path.join(&room_name), &image).unwrap();
    fs::set_permissions(
        room_path.join(&room_name),
        fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    let room = save_points::open_directory(&room_path).unwrap();
    let created = SeedCreated {
        schema_id: "state-content/1".into(),
        seed_id,
        room_name,
        size_bytes: (image.len() as u64).into(),
    };
    let mut repository = Repository::from_judged_directory(
        config
            .save_points_fd()
            .unwrap()
            .try_clone_to_owned()
            .unwrap(),
        save_points::ROOT,
        0,
    )
    .unwrap();
    let directory = config.run_directory();
    let _invocation =
        start::take_invocation_lock(&directory, crate::verb_table::HolderVerb::Load).unwrap();
    let _run = start::take_run_lock(&directory).unwrap().unwrap();
    let mut publication = Publication::seed(
        &mut repository,
        &scope,
        draft,
        SeedImage {
            expected_seed_id: &created.seed_id,
            created: &created,
            room: room.as_fd(),
            member_uid: 0,
        },
        &mut |_| Ok(()),
    )
    .unwrap();
    // The harness has waited for the normal seeder's successful exit before
    // sending setup. This copy is input to publication, not a live member room.
    publication.seed_cleanup_complete(&mut |_| Ok(())).unwrap();
    loop {
        match publication.advance(&mut |_| Ok(())).unwrap() {
            PublicationProgress::Renamed | PublicationProgress::Verified => (),
            PublicationProgress::Complete(identity) => return identity,
            other => panic!("seed publication: {other:?}"),
        }
    }
}

#[test]
#[ignore = "participant launched by the delegated worker/SQLite/admin circuit"]
fn forced_leave_joint_actual_admin() {
    assert!(nix::unistd::geteuid().is_root());
    let root =
        PathBuf::from(std::env::var_os("WEAVER_LEAVE_JOINT_ROOT").expect("joint parent root"));
    let host: serde_json::Value = serde_json::from_slice(&wait(&root, "host.json")).unwrap();
    let cgroup = host["cgroup"].as_str().unwrap();
    let config = config(&root, host["agent"].as_str().unwrap().into());
    let directory = crate::prepare_run_directory(&config, 0).unwrap();
    start::prepare_runtime_directory(&config.coordination_root, &config.agent, 0, 0).unwrap();
    write(
        &root,
        "admin-host.json",
        &json!({"socket":config.coordination_socket(),"run_directory":directory,"room":config.territory.join("state")}),
    );
    let payload: EnterPayload = serde_json::from_slice(&wait(&root, "setup.json")).unwrap();
    let loaded = publish_seed(&root, &config, &payload);
    write(&root, "loaded.json", &json!(loaded));
    wait(&root, "ready.json");
    let path = PathBuf::from(format!("/sys/fs/cgroup{cgroup}"));
    let metadata = fs::metadata(&path).unwrap();
    let record = crate::containment::Record {
        path: cgroup.into(),
        dev: metadata.dev(),
        ino: metadata.ino(),
        run: payload.run.0.clone(),
    };
    let run_dir = agent_cgroup::open_run_directory(&directory).unwrap();
    crate::containment::write_record(run_dir.as_fd(), &record).unwrap();
    save_points::write_marker(
        &config.root,
        Some(&save_points::Marker::Open {
            run: payload.run.0.clone(),
        }),
    )
    .unwrap();
    crate::TEST_WORKER_UID.with(|uid| uid.set(Some(0)));
    if host["mode"].as_str().unwrap().contains("-demand-save-") {
        let taken = std::thread::scope(|scope| {
            let contender = scope.spawn(|| {
                wait(&root, "demand-save-accepted");
                // Invocation locks are process-owned POSIX record locks.
                // A second thread is the same holder, not another invocation.
                let status = std::process::Command::new(std::env::current_exe().unwrap())
                    .args([
                        "--exact",
                        "serving_leave::invocation::tests::joint::demand_save_joint_competing_admin",
                        "--ignored",
                        "--nocapture",
                        "--test-threads=1",
                    ])
                    .env("WEAVER_LEAVE_JOINT_ROOT", &root)
                    .status()
                    .unwrap();
                assert!(status.success(), "competing admin invocation: {status}");
                fs::write(
                    root.join("demand-resume-generation"),
                    b"save lock rejects second save and forced unload",
                )
                .unwrap();
            });
            let taken = crate::serving_leave::save::run(
                &config,
                &AgentName(config.agent.clone()),
                Cause { uid: 74 },
                |_| Ok(0),
            )
            .unwrap();
            contender.join().unwrap();
            taken
        });
        crate::TEST_WORKER_UID.with(|uid| uid.set(None));
        assert!(
            start::run_lock_held(&directory).unwrap(),
            "demand save retains the serving run"
        );
        assert!(path.exists());
        assert_eq!(
            save_points::read_marker(&config.root).unwrap(),
            save_points::Marker::Open {
                run: payload.run.0.clone()
            }
        );
        assert_eq!(taken.evidence.loaded, loaded);
        assert_eq!(taken.evidence.completed.as_ref(), Some(&taken.report.start));
        assert!(taken.evidence.committed && taken.evidence.publication_failure.is_none());
        fs::write(
            root.join("admin-save-request.json"),
            taken
                .evidence
                .request
                .encode(weaver_types::state_flow::capture::CaptureSender::Admin)
                .unwrap(),
        )
        .unwrap();
        for (index, received) in taken.evidence.received.iter().enumerate() {
            fs::write(
                root.join(format!("admin-received-{index}.json")),
                &received.wire,
            )
            .unwrap();
        }
        let _inspection =
            start::take_invocation_lock(&directory, crate::verb_table::HolderVerb::SavePoint)
                .unwrap();
        let repository = Repository::from_judged_directory(
            config
                .save_points_fd()
                .unwrap()
                .try_clone_to_owned()
                .unwrap(),
            save_points::ROOT,
            0,
        )
        .unwrap();
        let report = &taken.report;
        let verified = repository
            .verify_indexed(
                &crate::save_points::serving::codec::ArtifactId::parse(&report.start.artifact_id)
                    .unwrap(),
                &LoadScope {
                    session: &payload.session,
                    trace_path: &config.territory.join("trace.jsonl"),
                },
            )
            .unwrap();
        assert_eq!(verified.identity(), report.start);
        assert_eq!(
            fs::read_to_string(
                config
                    .territory
                    .join("save-points")
                    .join(save_points::MANIFEST)
            )
            .unwrap()
            .lines()
            .count(),
            2
        );
        let artifact = config
            .territory
            .join("save-points")
            .join(&report.start.artifact_id)
            .join("state.sqlite");
        write(
            &root,
            "concluded.json",
            &json!({"report":report,"artifact":artifact}),
        );
        return;
    }
    let mode = host["mode"].as_str().unwrap();
    let joining = mode.contains("-joined-capture-refusal");
    let mut joiner =
        joining.then(|| {
            JoiningAdmin(std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "serving_leave::invocation::tests::joint::capture_refusal_joint_force_admin",
                "--ignored",
                "--nocapture",
            ])
            .env("WEAVER_LEAVE_JOINT_ROOT", &root)
            .spawn()
            .unwrap())
        });
    let mut result = run_mode(
        &config,
        &AgentName(config.agent.clone()),
        Cause { uid: 74 },
        UnloadBounds {
            leave: Duration::from_secs(20),
            after_left: Duration::from_secs(5),
            term: Duration::from_secs(1),
            kill: Duration::from_secs(1),
        },
        !mode.contains("-graceful-save"),
        |_| Ok(0),
    );
    if let Some(joiner) = &mut joiner {
        assert!(
            joiner.0.wait().unwrap().success(),
            "actual joining force failed"
        );
    }
    let failed_cleanup = mode.contains("-failed-cleanup-capture-refusal");
    if mode.ends_with("-capture-refusal")
        && mode.contains("-graceful-save")
        && !joining
        && !failed_cleanup
    {
        let failure = result
            .as_ref()
            .expect_err("first graceful capture must refuse");
        assert!(
            failure.cleanup.is_none(),
            "S9 performs no failed-run escalation: {failure:?}"
        );
        let evidence = failure.evidence.as_ref().unwrap();
        assert!(evidence.report.is_none() && evidence.completed.is_none() && !evidence.committed);
        assert!(evidence.received.iter().any(|received| matches!(
            received.envelope.payload,
            weaver_types::state_flow::capture::CapturePayload::SavePointCaptureRefused(_)
        )));
        // The parent checks the first recorded cause against this exact leg
        // after the worker drains its trace. At S9 that file is not yet saved.
        let expiry_missed = mode.contains("-joint-expiry-").then(|| {
            evidence
                .received
                .iter()
                .find_map(|received| match &received.envelope.payload {
                    weaver_types::state_flow::capture::CapturePayload::SavePointCaptureRefused(
                        value,
                    ) => Some(value.missed),
                    _ => None,
                })
                .expect("cleanup acknowledgement before graceful refusal")
        });
        assert!(matches!(
            evidence.received.last().unwrap().envelope.payload,
            weaver_types::state_flow::capture::CapturePayload::Refusal(
                weaver_types::state_flow::capture::CaptureRefusal::Save(
                    weaver_types::state_flow::capture::SaveRefusal::SavePointNotTaken {
                        missed,
                    }
                )
            )
        if missed == if let Some(missed) = expiry_missed {
            missed
        } else if mode.contains("-preparation-capture-refusal") {
            weaver_types::state_flow::capture::SavePointLeg::Published
        } else if mode.contains("-finished-timeout-capture-refusal") {
            weaver_types::state_flow::capture::SavePointLeg::Finished
        } else {
            weaver_types::state_flow::capture::SavePointLeg::Answer
        }));
        for (index, received) in evidence.received.iter().enumerate() {
            fs::write(
                root.join(format!("s9-admin-received-{index}.json")),
                &received.wire,
            )
            .unwrap();
        }
        wait(&root, "worker-s9.json");
        assert!(start::run_lock_held(&directory).unwrap());
        assert!(path.exists());
        assert_eq!(
            save_points::read_marker(&config.root).unwrap(),
            save_points::Marker::Open {
                run: payload.run.0.clone()
            }
        );
        assert_eq!(
            fs::read_to_string(
                config
                    .territory
                    .join("save-points")
                    .join(save_points::MANIFEST)
            )
            .unwrap()
            .lines()
            .count(),
            1,
            "refusal publishes no index entry"
        );
        // The same actual admin path retries after releasing its invocation
        // lock. Observe supplies the retained run facts; no new Ready is made.
        result = run_mode(
            &config,
            &AgentName(config.agent.clone()),
            Cause { uid: 74 },
            UnloadBounds {
                leave: Duration::from_secs(20),
                after_left: Duration::from_secs(5),
                term: Duration::from_secs(1),
                kill: Duration::from_secs(1),
            },
            false,
            |_| Ok(0),
        );
    }
    crate::TEST_WORKER_UID.with(|uid| uid.set(None));
    let evidence = match &result {
        Ok(result) => result.evidence.as_deref().expect("actual capture evidence"),
        Err(failure) => failure
            .evidence
            .as_deref()
            .expect("actual captured exchange evidence"),
    };
    for (index, received) in evidence.received.iter().enumerate() {
        fs::write(
            root.join(format!("admin-received-{index}.json")),
            &received.wire,
        )
        .unwrap();
    }
    assert!(!start::run_lock_held(&directory).unwrap());
    assert!(
        !path.exists(),
        "actual cgroup end must remove the agent subtree"
    );
    assert!(!directory.join(crate::containment::RECORD).exists());
    // Reinspection is its own held invocation, after the Leave owner's actual
    // conclusion. It cannot race a fresh loader in this private fixture.
    let _inspection =
        start::take_invocation_lock(&directory, crate::verb_table::HolderVerb::Load).unwrap();
    if failed_cleanup {
        let failure =
            result.expect_err("failed cleanup cannot acknowledge refusal or complete Left");
        assert!(failure.cleanup.is_none(), "{failure:?}");
        let public: serde_json::Value =
            serde_json::from_str(&crate::surface::render_serving_refusal(&failure.refusal()))
                .unwrap();
        // The worker retained its failure on trace and closed this peer without
        // a typed terminal answer. ADMIN-SAVE forbids inferring a phase or a
        // FailedRun message from that absence: admin reports Unanswered.
        assert_eq!(public["kind"], "unanswered");
        let evidence = failure.evidence.as_ref().unwrap();
        assert!(evidence.report.is_none() && evidence.completed.is_none() && !evidence.committed);
        assert!(!evidence.received.iter().any(|received| matches!(
            received.envelope.payload,
            weaver_types::state_flow::capture::CapturePayload::SavePointCaptureRefused(_)
                | weaver_types::state_flow::capture::CapturePayload::Answer(_)
        )));
        assert_eq!(
            save_points::read_marker(&config.root).unwrap(),
            save_points::Marker::Open { run: payload.run.0 }
        );
        let saves = config.territory.join("save-points");
        assert_eq!(
            fs::read_to_string(saves.join(save_points::MANIFEST))
                .unwrap()
                .lines()
                .count(),
            1
        );
        assert_eq!(fs::read_dir(saves).unwrap().count(), 2);
        write(
            &root,
            "failed-cleanup.json",
            &json!({"admin":"unanswered","open":true}),
        );
        return;
    }
    if mode.ends_with("-capture-refusal") && (!mode.contains("-graceful-save") || joining) {
        let result = result.expect("refused forced capture must still complete real Leave");
        let evidence = result.evidence.as_ref().unwrap();
        assert!(evidence.report.is_none() && evidence.completed.is_none() && !evidence.committed);
        assert!(evidence.received.iter().any(|received| matches!(
            received.envelope.payload,
            weaver_types::state_flow::capture::CapturePayload::SavePointCaptureRefused(_)
        )));
        assert!(matches!(
            evidence.received.last().unwrap().envelope.payload,
            weaver_types::state_flow::capture::CapturePayload::Answer(
                weaver_types::state_flow::capture::CaptureAnswer::Left {
                    save_point: None,
                    forced: true,
                    no_state: false,
                }
            )
        ));
        assert_eq!(
            save_points::read_marker(&config.root).unwrap(),
            save_points::Marker::Forced { run: payload.run.0 }
        );
        let saves = config.territory.join("save-points");
        assert_eq!(
            fs::read_to_string(saves.join(save_points::MANIFEST))
                .unwrap()
                .lines()
                .count(),
            1
        );
        assert_eq!(
            fs::read_dir(saves).unwrap().count(),
            2,
            "only the seed and index remain"
        );
        write(
            &root,
            "concluded-refused.json",
            &json!({"save_report":null,"forced":true}),
        );
        return;
    }
    if host["mode"].as_str().unwrap().ends_with("-bad-release") {
        let failure = result.expect_err("a wrong member receipt cannot complete Left");
        assert!(failure.cleanup.is_none(), "{failure:?}");
        let refusal = failure.refusal();
        let rendered = crate::surface::render_serving_refusal(&refusal);
        let public: serde_json::Value = serde_json::from_str(&rendered).unwrap();
        assert_eq!(public["kind"], "failed_run");
        assert_eq!(public["run"], payload.run.0);
        assert!(!public["artifacts"].as_array().unwrap().is_empty());
        fs::write(root.join("admin-public-refusal.json"), rendered).unwrap();
        let evidence = failure.evidence.as_ref().unwrap();
        assert!(evidence.completed.is_none() && evidence.report.is_none());
        assert!(
            evidence.publication_failure.is_some(),
            "post-Commit failure retains publication evidence"
        );
        assert_eq!(
            save_points::read_marker(&config.root).unwrap(),
            save_points::Marker::Open {
                run: payload.run.0.clone()
            }
        );
        let saves = config.territory.join("save-points");
        let guarded: Vec<_> = fs::read_dir(&saves)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| {
                p.file_name()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .ends_with(".pending")
            })
            .collect();
        assert_eq!(guarded.len(), 1);
        assert_eq!(
            fs::read_to_string(saves.join(save_points::MANIFEST))
                .unwrap()
                .lines()
                .count(),
            2
        );
        let repository = Repository::from_judged_directory(
            config
                .save_points_fd()
                .unwrap()
                .try_clone_to_owned()
                .unwrap(),
            save_points::ROOT,
            0,
        )
        .unwrap();
        let trace_path = config.territory.join("trace.jsonl");
        assert!(
            repository
                .load(
                    None,
                    &LoadScope {
                        session: &payload.session,
                        trace_path: &trace_path
                    }
                )
                .is_err(),
            "guarded newest save must refuse without falling back to the seed"
        );
        write(
            &root,
            "failed.json",
            &json!({"guarded":true,"cause":format!("{:?}",failure.cause),"published":evidence.publication_failure.as_ref().unwrap().published}),
        );
        return;
    }
    let result = result.unwrap();
    let evidence = result.evidence.as_ref().expect("actual capture evidence");
    let completed = evidence
        .completed
        .as_ref()
        .expect("actual Left, cleanup and guard finish");
    assert_eq!(evidence.loaded, loaded);
    let report = evidence.report.as_ref().unwrap();
    assert_eq!(&report.start, completed);
    assert_eq!(
        completed.parent_artifact_id.as_ref(),
        Some(&loaded.artifact_id)
    );
    assert!(!start::run_lock_held(&directory).unwrap());
    assert!(
        !path.exists(),
        "actual cgroup end must remove the agent subtree"
    );
    assert!(!directory.join(crate::containment::RECORD).exists());
    assert_eq!(
        save_points::read_marker(&config.root).unwrap(),
        save_points::Marker::Closed { run: payload.run.0 }
    );
    let repository = Repository::from_judged_directory(
        config
            .save_points_fd()
            .unwrap()
            .try_clone_to_owned()
            .unwrap(),
        save_points::ROOT,
        0,
    )
    .unwrap();
    let trace_path = config.territory.join("trace.jsonl");
    let selected = repository
        .load(
            None,
            &LoadScope {
                session: &payload.session,
                trace_path: &trace_path,
            },
        )
        .unwrap();
    assert_eq!(selected.identity(), *completed);
    let artifact = config
        .territory
        .join("save-points")
        .join(&completed.artifact_id)
        .join("state.sqlite");
    assert_eq!(
        fs::read_dir(config.territory.join("state"))
            .unwrap()
            .count(),
        0
    );
    write(
        &root,
        "concluded.json",
        &json!({"answer":result.answer,"report":report,"loaded":loaded,"artifact":artifact}),
    );
}
