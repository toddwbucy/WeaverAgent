use super::*;
use crate::save_points::serving::publication::{Progress, Publication, SeedImage};
use crate::seed::preparation::tests::{Fixture, fixture, fixture_with_member};
use std::fs;
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::Duration;
use weaver_types::state_flow::{DecimalU64, SeedCreated, StartIdentity};

const IMAGE: &[u8] = b"synthetic member image, not SQLite or language qualification";

fn write(path: &Path, bytes: &[u8], mode: u32) {
    fs::write(path, bytes).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}
pub(crate) fn setup(fixture: &Fixture) -> StartIdentity {
    fs::create_dir_all(&fixture.config.coordination_root).unwrap();
    fs::set_permissions(
        &fixture.config.coordination_root,
        fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    let source = fs::read_to_string(fixture.config.territory.join("agent.toml")).unwrap();
    let draft = weaver_types::parse_serving_draft(&source).unwrap();
    let scope = LoadScope {
        session: &draft.agent().session,
        trace_path: trace_path(&draft.agent().trace_sink),
    };
    let room = fixture.config.root.with_file_name("image-input");
    fs::create_dir(&room).unwrap();
    fs::set_permissions(&room, fs::Permissions::from_mode(0o700)).unwrap();
    let seed_id = "1".repeat(32);
    let room_name = format!("seed-{seed_id}.sqlite");
    write(&room.join(&room_name), IMAGE, 0o600);
    let created = SeedCreated {
        schema_id: "state-content/1".into(),
        seed_id,
        room_name,
        size_bytes: DecimalU64(IMAGE.len() as u64),
    };
    let mut repository = Repository::from_judged_directory(
        fixture
            .config
            .save_points_fd()
            .unwrap()
            .try_clone_to_owned()
            .unwrap(),
        save_points::Owner {
            uid: nix::unistd::geteuid().as_raw(),
            gid: nix::unistd::getegid().as_raw(),
        },
        fixture.config.access_gid,
    )
    .unwrap();
    let room = save_points::open_directory(&room).unwrap();
    let mut publication = Publication::seed(
        &mut repository,
        &scope,
        draft.clone(),
        SeedImage {
            expected_seed_id: &created.seed_id,
            created: &created,
            room: room.as_fd(),
            member_uid: nix::unistd::geteuid().as_raw(),
        },
        &mut |_| Ok(()),
    )
    .unwrap();
    // Artifact fixture has no child. This is not member cleanup evidence.
    publication.seed_cleanup_complete(&mut |_| Ok(())).unwrap();
    loop {
        match publication.advance(&mut |_| Ok(())).unwrap() {
            Progress::Renamed | Progress::Verified => {}
            Progress::Complete(identity) => return identity,
            Progress::Published(_) => panic!("seed fixture"),
        }
    }
}
fn agent(f: &Fixture) -> AgentName {
    AgentName(f.config.agent.clone())
}
fn prepare(
    f: &Fixture,
    artifact: VerifiedArtifact,
    facts: RuntimeFacts,
    ordinal: u64,
) -> Result<PreparedEnter, PreparationFailure> {
    assert!(start::run_lock_held(&f.config.run_directory()).unwrap());
    // POSIX locks belong to the process: a same-process open/close would
    // itself release the invocation lock. Probe from an independent process.
    let locked = std::process::Command::new("/usr/bin/python3")
        .args(["-c", "import sys,fcntl,errno; f=open(sys.argv[1],'r+');\ntry: fcntl.lockf(f,fcntl.LOCK_EX|fcntl.LOCK_NB); sys.exit(1)\nexcept OSError as e: sys.exit(0 if e.errno in (errno.EAGAIN,errno.EACCES) else 2)"])
        .arg(f.config.run_directory().join("admin.lock"))
        .status().unwrap();
    assert!(locked.success(), "invocation lock was not retained");
    PreparedEnter::prepare_against(&f.config, &agent(f), artifact, facts, ordinal, &f.boundary)
}

#[test]
fn admin_startup_selects_verified_settings_without_publishing_room_leftovers() {
    let f = fixture();
    let identity = setup(&f);
    let directory = f.config.territory.join("save-points");
    let original_index = fs::read(directory.join(save_points::MANIFEST)).unwrap();
    fs::create_dir(f.config.territory.join("state")).unwrap();
    write(
        &f.config.territory.join("state/unpublished.sqlite"),
        b"not a selectable save",
        0o600,
    );
    let path = f.config.territory.join("agent.toml");
    let draft = fs::read_to_string(&path).unwrap();
    write(
        &path,
        draft
            .replace("synthetic-decoder", "later-draft-model")
            .as_bytes(),
        0o640,
    );
    let run_directory =
        crate::prepare_run_directory(&f.config, nix::unistd::geteuid().as_raw()).unwrap();
    let _invocation =
        start::take_invocation_lock(&run_directory, crate::verb_table::HolderVerb::Load).unwrap();
    let _run = start::take_run_lock(&run_directory).unwrap().unwrap();
    let selected = select(&f.config, &agent(&f)).unwrap();
    assert_eq!(selected.identity(), identity);
    assert_eq!(
        selected
            .declaration()
            .agent()
            .spu_instruction
            .decoder
            .model_binding
            .artifact
            .0,
        "synthetic-decoder"
    );
    assert_eq!(
        fs::read(directory.join(save_points::MANIFEST)).unwrap(),
        original_index
    );
    assert!(f.config.territory.join("state/unpublished.sqlite").exists());
}

#[test]
fn admin_startup_refuses_selector_scope_and_corrupt_image_before_preparation() {
    for case in ["selector", "session", "image", "missing-index"] {
        let f = fixture();
        let identity = setup(&f);
        match case {
            "selector" => write(
                &f.config.root.join("restore-artifact"),
                format!("00000000000000000002-{}", "a".repeat(64)).as_bytes(),
                0o640,
            ),
            "session" => {
                let p = f.config.territory.join("agent.toml");
                let s = fs::read_to_string(&p).unwrap();
                write(
                    &p,
                    s.replace("fixture-session", "another-session").as_bytes(),
                    0o640,
                );
            }
            "image" => write(
                &f.config
                    .territory
                    .join("save-points")
                    .join(identity.artifact_id)
                    .join("state.sqlite"),
                b"changed",
                0o640,
            ),
            "missing-index" => {
                fs::remove_file(
                    f.config
                        .territory
                        .join("save-points")
                        .join(save_points::MANIFEST),
                )
                .unwrap();
            }
            _ => unreachable!(),
        }
        let result = run_authorized(
            &f.config,
            &agent(&f),
            Cause { uid: 0 },
            |_, _, _, _, _| panic!("selection reached preparation: {case}"),
            |_| panic!("account"),
        );
        assert!(
            matches!(result, Err(Failure::Artifact(_))),
            "{case}: {result:?}"
        );
        assert!(!start::run_lock_held(&f.config.run_directory()).unwrap());
        assert!(!f.config.territory.join("state").exists());
        assert_eq!(save_points::load_marker(&f.config.root).unwrap(), None);
    }
}

#[test]
fn admin_startup_preparation_and_frame_bound_precede_every_fork() {
    for oversized in [false, true] {
        let f = fixture();
        setup(&f);
        let result = run_authorized(
            &f.config,
            &agent(&f),
            Cause { uid: 0 },
            |_, _, artifact, mut facts, ordinal| {
                if !oversized {
                    return Err(crate::seed::preparation::invalid(
                        "test",
                        "refused before fork",
                    ));
                }
                facts.stack.insert(
                    "oversized".into(),
                    "x".repeat(weaver_types::MAX_ENVELOPE_BYTES),
                );
                prepare(&f, artifact, facts, ordinal)
            },
            |_| panic!("must not resolve spawn account"),
        );
        if oversized {
            assert!(
                matches!(result, Err(Failure::Admission(ref refusal)) if **refusal == LifecycleRefusal::DescriptorsUnusable),
                "{result:?}"
            );
            assert!(
                fs::read_to_string(f.config.admin_log())
                    .unwrap()
                    .contains("enter_frame_too_large")
            );
        } else {
            assert!(matches!(result, Err(Failure::Preparation(_))), "{result:?}");
        }
        assert!(!f.config.territory.join("state").exists());
        assert!(!start::run_lock_held(&f.config.run_directory()).unwrap());
    }
}

#[test]
fn admin_startup_held_run_refuses_before_selection_without_ending_it() {
    let f = fixture();
    setup(&f);
    let directory =
        crate::prepare_run_directory(&f.config, nix::unistd::geteuid().as_raw()).unwrap();
    let _run = start::take_run_lock(&directory).unwrap().unwrap();
    fs::remove_file(
        f.config
            .territory
            .join("save-points")
            .join(save_points::MANIFEST),
    )
    .unwrap();
    let result = run_authorized(
        &f.config,
        &agent(&f),
        Cause { uid: 0 },
        |_, _, _, _, _| panic!("prepare"),
        |_| panic!("account"),
    );
    assert!(
        matches!(result,Err(Failure::Admission(ref r)) if **r == LifecycleRefusal::AgentRunning),
        "{result:?}"
    );
    assert!(start::run_lock_held(&directory).unwrap());
}

const MEMBER: &str = r#"#!/usr/bin/python3
import os, sys, json, fcntl
assert sys.argv[1] == '--mode=serving'
assert len(sys.argv) == 3
image=os.read(4, 4096)
try:
    os.write(4, b'forbidden')
    raise AssertionError('dump is writable')
except OSError as e:
    assert e.errno == 9
report={'image':image.decode(), 'dump_inode':os.fstat(4).st_ino,
        'gifts':[n for n in range(3,64) if os.path.exists('/proc/self/fd/'+str(n))],
        'lock_inode':os.fstat(9).st_ino, 'uid':os.getuid(), 'gid':os.getgid(),
        'groups':os.getgroups(), 'cwd':os.getcwd(), 'env_leaked':'LOAD_TEST_SECRET' in os.environ,
        'cgroup':open('/proc/self/cgroup').read(), 'room':sys.argv[2]}
os.write(3,(json.dumps(report)+'\n').encode())
while os.read(3,4096): pass
"#;
const WORKER: &str = r#"#!/usr/bin/python3
import os, sys, json, socket, array
case=CASE
s=socket.socket(socket.AF_UNIX,socket.SOCK_SEQPACKET)
s.bind(sys.argv[1]); s.listen(4)
c,_=s.accept()
body,anc,flags,_=c.recvmsg(65536,socket.CMSG_SPACE(8))
assert not flags
fds=array.array('i')
for level,kind,data in anc:
    assert level==socket.SOL_SOCKET and kind==socket.SCM_RIGHTS
    fds.frombytes(data)
assert len(fds)==2
enter=json.loads(body)
member=json.loads(os.read(fds[1],4096))
assert member['image']==IMAGE
print(json.dumps({'enter':enter,'member':member,'args':sys.argv[1:],
    'worker_cgroup':open('/proc/self/cgroup').read(),'worker_lock':os.fstat(9).st_ino}),flush=True)
os.write(fds[0],b'exact inherited trace endpoint')
answer=json.loads(READY)
answer['exchange']=enter['exchange']
if case=='wrong-exchange': answer['exchange']['ordinal']+=1
if case=='wrong-position': answer['position']='open'
if case=='refused': answer=json.loads(REFUSED); answer['exchange']=enter['exchange']
if case not in ('timeout','eof'): c.send(json.dumps(answer).encode())
if case!='timeout': c.close()
# The real rollback dials a new exchange. No save is taken by this double.
leave,_=s.accept()
request=json.loads(leave.recv(65536))
print(json.dumps({'rollback':request}),flush=True)
reply=json.loads(OUT_OF_ORDER); reply['exchange']=request['exchange']
leave.send(json.dumps(reply).encode()); leave.close(); c.close()
os.close(fds[1]); os.close(fds[0]); s.close()
"#;

/// Fixture-only cleanup on panic too. It ends only this test's recorded group.
struct Cleanup<'a>(&'a ServiceConfig);
impl Drop for Cleanup<'_> {
    fn drop(&mut self) {
        let Ok(directory) = crate::agent_cgroup::open_run_directory(&self.0.run_directory()) else {
            return;
        };
        if let Ok(Some(record)) = crate::containment::read_record(directory.as_fd(), &self.0.agent)
        {
            let _ = crate::agent_cgroup::run_end(
                directory.as_fd(),
                &self.0.agent,
                crate::agent_cgroup::Concluding::Run(&record.run),
                Duration::from_secs(5),
            );
        }
    }
}
fn answer(payload: weaver_types::Payload) -> String {
    serde_json::to_string(&weaver_types::OrganEnvelope {
        exchange: weaver_types::ExchangeId {
            opener: weaver_types::Opener::Admin,
            ordinal: 1,
        },
        position: Position::Close,
        payload,
    })
    .unwrap()
}
fn root_case(case: &str) {
    assert!(
        nix::unistd::geteuid().is_root(),
        "run inside a root-owned delegated scope"
    );
    let mut f = fixture_with_member(inventory::MemberAccount {
        uid: 65533,
        gid: 65533,
    });
    f.boundary.agent_uid = 65534;
    f.boundary.agent_gids = vec![65534];
    f.config.load_bound = Duration::from_millis(250);
    let path = f.config.territory.join("agent.toml");
    let source = fs::read_to_string(&path)
        .unwrap()
        .replace("trace-sink.kind = \"file\"", "trace-sink.kind = \"socket\"")
        .replace("trace-sink.create = true\n", "");
    write(&path, source.as_bytes(), 0o640);
    let identity = setup(&f);
    write(
        &path,
        source
            .replace("synthetic-decoder", "ignored-draft-model")
            .as_bytes(),
        0o640,
    );
    let trace =
        std::os::unix::net::UnixListener::bind(f.config.territory.join("trace.jsonl")).unwrap();
    let binaries = f.config.worker.parent().unwrap();
    fs::set_permissions(binaries, fs::Permissions::from_mode(0o755)).unwrap();
    write(
        &f.config.worker.with_file_name("weaver-state"),
        MEMBER.as_bytes(),
        0o755,
    );
    let worker = WORKER
        .replace("CASE", &format!("{case:?}"))
        .replace(
            "IMAGE",
            &format!("{:?}", std::str::from_utf8(IMAGE).unwrap()),
        )
        .replace(
            "READY",
            &format!(
                "{:?}",
                answer(weaver_types::Payload::Answer(LifecycleAnswer::Ready))
            ),
        )
        .replace(
            "REFUSED",
            &format!(
                "{:?}",
                answer(weaver_types::Payload::Refusal(
                    LifecycleRefusal::NoResidency
                ))
            ),
        )
        .replace(
            "OUT_OF_ORDER",
            &format!(
                "{:?}",
                answer(weaver_types::Payload::Refusal(LifecycleRefusal::OutOfOrder))
            ),
        );
    write(&f.config.worker, worker.as_bytes(), 0o755);
    for binary in [&f.config.spu, &f.config.gate] {
        write(binary, b"#!/bin/sh\nexit 0\n", 0o755);
    }
    let _cleanup = Cleanup(&f.config);
    crate::TEST_WORKER_UID.with(|uid| uid.set(Some(65534)));
    let image_path = f
        .config
        .territory
        .join("save-points")
        .join(&identity.artifact_id)
        .join("state.sqlite");
    let inode = std::os::unix::fs::MetadataExt::ino(&fs::metadata(&image_path).unwrap());
    if case.starts_with("ready-marker-") {
        let marker_part = f.config.root.join(".run.marker.new");
        let fail_write = case == "ready-marker-failure";
        BEFORE_READY_MARKER.with_borrow_mut(|hook| {
            *hook = Some(Box::new(move |window| {
                let until = window.remaining().expect("Ready was accepted within Enter");
                std::thread::sleep(until + Duration::from_millis(10));
                assert_eq!(window.check(), Err(LifecycleRefusal::NoResidency));
                if fail_write {
                    fs::create_dir(marker_part).unwrap();
                }
            }))
        });
    }
    let started = Instant::now();
    let result = run_authorized(
        &f.config,
        &agent(&f),
        Cause { uid: 0 },
        |_, _, artifact, facts, ordinal| {
            // Selection's descriptor must survive unlink and never reopen by name.
            fs::remove_file(&image_path).unwrap();
            let prepared = prepare(&f, artifact, facts, ordinal)?;
            if case == "worker-exec" {
                write(&f.config.worker, b"#!/no/such/interpreter\n", 0o755);
            }
            if case == "member-exec" {
                write(
                    &f.config.worker.with_file_name("weaver-state"),
                    b"#!/no/such/interpreter\n",
                    0o755,
                );
            }
            Ok(prepared)
        },
        |_| {
            Ok(AgentAccount {
                uid: 65534,
                gid: 65534,
                home: f.boundary.home.clone(),
            })
        },
    );
    if matches!(case, "worker-exec" | "member-exec") {
        assert!(
            matches!(result, Err(Failure::Admission(ref r)) if **r == LifecycleRefusal::BindFailed),
            "{case}: {result:?}"
        );
        assert!(!start::run_lock_held(&f.config.run_directory()).unwrap());
        assert!(matches!(
            crate::unloaded_by_ladder(&f.config).unwrap(),
            LifecycleAnswer::State {
                state: weaver_types::AgentState::Unloaded,
                ..
            }
        ));
        assert_eq!(save_points::load_marker(&f.config.root).unwrap(), None);
        return;
    }
    if case == "timeout" {
        assert!(
            started.elapsed() >= f.config.load_bound,
            "EOF is not expiry evidence"
        );
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "the original answer bound was extended"
        );
    }
    if case == "ready" || case == "ready-marker-delay" {
        let loaded = result.unwrap();
        let LoadedStateFacts::Serving { start, .. } = loaded.state else {
            panic!("serving facts");
        };
        assert_eq!(start, identity.clone());
        assert!(matches!(
            save_points::load_marker(&f.config.root).unwrap(),
            Some(save_points::Marker::Open { .. })
        ));
        assert!(start::run_lock_held(&f.config.run_directory()).unwrap());
        // The test explicitly ends its successful fixture through real rollback.
        let record = crate::containment::read_record(
            crate::agent_cgroup::open_run_directory(&f.config.run_directory())
                .unwrap()
                .as_fd(),
            &f.config.agent,
        )
        .unwrap()
        .unwrap();
        let mut standing = Standing {
            entered: true,
            forked: true,
            cgroup_run: Some(record.run.clone()),
            run_reference: Some(record.run),
            ..Standing::default()
        };
        assert!(crate::roll_back(&f.config, &mut standing).1.is_ok());
    } else {
        let expected = if matches!(case, "timeout" | "refused" | "eof") {
            LifecycleRefusal::NoResidency
        } else if case == "ready-marker-failure" {
            LifecycleRefusal::BoundaryUnverified
        } else {
            LifecycleRefusal::Malformed
        };
        assert!(
            matches!(result,Err(Failure::Admission(ref r)) if **r==expected),
            "{case}: {result:?}"
        );
        assert_eq!(save_points::load_marker(&f.config.root).unwrap(), None);
    }
    assert!(!start::run_lock_held(&f.config.run_directory()).unwrap());
    assert!(matches!(
        crate::unloaded_by_ladder(&f.config).unwrap(),
        LifecycleAnswer::State {
            state: weaver_types::AgentState::Unloaded,
            ..
        }
    ));
    let (mut stream, _) = trace.accept().unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut delivered = String::new();
    stream.read_to_string(&mut delivered).unwrap();
    assert_eq!(delivered, "exact inherited trace endpoint");
    let log = fs::read_to_string(f.config.worker_log()).unwrap();
    let report: serde_json::Value = serde_json::from_str(
        log.lines()
            .find(|s| s.starts_with('{'))
            .unwrap_or_else(|| panic!("worker log: {log}")),
    )
    .unwrap();
    let delivered_enter = weaver_types::state_flow::enter::EnterEnvelope::decode(
        &serde_json::to_vec(&report["enter"]).unwrap(),
    )
    .unwrap();
    assert_eq!(
        delivered_enter
            .enter()
            .spu_instruction
            .decoder
            .model_binding
            .artifact
            .0,
        "synthetic-decoder"
    );
    let weaver_types::state_flow::enter::EnterBinding::Serving {
        start,
        ste100_scheme,
        ..
    } = &delivered_enter.enter().binding
    else {
        panic!("serving Enter");
    };
    assert_eq!(start, &identity);
    assert_eq!(
        ste100_scheme,
        &fs::read_to_string(f.config.root.join("ste100-scheme.json")).unwrap()
    );
    let rollback: serde_json::Value = serde_json::from_str(
        log.lines()
            .find(|line| line.contains("\"rollback\":"))
            .expect("rollback directive was actually delivered"),
    )
    .unwrap();
    let rollback: weaver_types::OrganEnvelope =
        serde_json::from_value(rollback["rollback"].clone()).unwrap();
    assert!(matches!(
        rollback.payload,
        weaver_types::Payload::Directive(weaver_types::LifecycleDirective::Leave {
            forced: true,
            rollback: true,
            ..
        })
    ));
    let m = &report["member"];
    assert_eq!(m["dump_inode"], inode);
    assert_eq!(m["uid"], 65533);
    assert_eq!(m["gid"], 65533);
    assert_eq!(m["groups"], serde_json::json!([65533]));
    assert_eq!(m["gifts"], serde_json::json!([3, 4, 9]));
    assert_eq!(m["lock_inode"], report["worker_lock"]);
    assert_eq!(m["cwd"], "/");
    assert_eq!(m["env_leaked"], false);
    assert!(m["cgroup"].as_str().unwrap().contains("/constituents"));
    assert!(
        report["worker_cgroup"]
            .as_str()
            .unwrap()
            .contains("/worker/run")
    );
    assert!(
        report["args"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == "--classify-binary")
    );
    assert!(BEFORE_READY_MARKER.with_borrow_mut(Option::take).is_none());
    crate::TEST_WORKER_UID.with(|uid| uid.set(None));
}

#[test]
#[ignore = "root-owned delegated cgroup scope; isolated protocol-double processes"]
fn admin_startup_composition_ready_retains_exact_image_and_descriptor_custody() {
    root_case("ready");
}
#[test]
#[ignore = "root-owned delegated cgroup scope; isolated protocol-double processes"]
fn admin_startup_composition_timely_ready_survives_marker_work_after_enter_deadline() {
    root_case("ready-marker-delay");
}
#[test]
#[ignore = "root-owned delegated cgroup scope; isolated protocol-double processes"]
fn admin_startup_composition_marker_failure_after_ready_still_rolls_back() {
    root_case("ready-marker-failure");
}
#[test]
#[ignore = "root-owned delegated cgroup scope; isolated protocol-double processes"]
fn admin_startup_composition_wrong_exchange_rolls_back_without_loaded() {
    root_case("wrong-exchange");
}
#[test]
#[ignore = "root-owned delegated cgroup scope; isolated protocol-double processes"]
fn admin_startup_composition_wrong_position_rolls_back_without_loaded() {
    root_case("wrong-position");
}
#[test]
#[ignore = "root-owned delegated cgroup scope; isolated protocol-double processes"]
fn admin_startup_composition_refusal_rolls_back_without_loaded() {
    root_case("refused");
}
#[test]
#[ignore = "root-owned delegated cgroup scope; isolated protocol-double processes"]
fn admin_startup_composition_timeout_rolls_back_without_loaded() {
    root_case("timeout");
}

const _: fn() = || {
    trait NoClone<M> {
        fn check() {}
    }
    impl<T: ?Sized> NoClone<()> for T {}
    impl<T: Clone> NoClone<u8> for T {}
    let _ = <Loaded as NoClone<_>>::check;
};

#[test]
#[ignore = "root-owned delegated cgroup scope; isolated protocol-double processes"]
fn admin_startup_composition_failed_member_exec_removes_its_group_without_enter() {
    root_case("member-exec");
}
#[test]
#[ignore = "root-owned delegated cgroup scope; isolated protocol-double processes"]
fn admin_startup_composition_failed_worker_exec_ends_the_member_without_enter() {
    root_case("worker-exec");
}

#[test]
#[ignore = "root-owned delegated cgroup scope; isolated protocol-double processes"]
fn admin_startup_composition_eof_rolls_back_without_loaded() {
    root_case("eof");
}

#[test]
fn admin_original_enter_window_includes_preparation_and_preserves_actual_failure() {
    for actual_failure in [false, true] {
        let mut f = fixture();
        setup(&f);
        f.config.load_bound = Duration::from_millis(50);
        let result = run_authorized(
            &f.config,
            &agent(&f),
            Cause { uid: 0 },
            |_, _, artifact, facts, ordinal| {
                let original = facts.enter_window;
                assert_eq!(original.deadline_ns.0 - original.began_ns.0, 50_000_000);
                let prepared = prepare(&f, artifact, facts, ordinal)?;
                std::thread::sleep(Duration::from_millis(70));
                if actual_failure {
                    return Err(PreparationFailure::from(
                        LifecycleRefusal::ArtifactUnreadable,
                    ));
                }
                Ok(prepared)
            },
            |_| panic!("no account, child, descriptor carry or Ready after preparation expiry"),
        );
        if actual_failure {
            assert!(matches!(result, Err(Failure::Preparation(_))), "{result:?}");
        } else {
            assert!(
                matches!(result, Err(Failure::Admission(ref r)) if **r == LifecycleRefusal::NoResidency),
                "{result:?}"
            );
        }
        assert!(!start::run_lock_held(&f.config.run_directory()).unwrap());
        assert_eq!(save_points::load_marker(&f.config.root).unwrap(), None);
    }
}

#[test]
fn admin_original_enter_window_bounds_courtesy_lock_wait_before_inventory() {
    use std::io::{BufRead, BufReader};
    use std::process::{Command, Stdio};
    let mut f = fixture();
    setup(&f);
    let directory =
        crate::prepare_run_directory(&f.config, nix::unistd::geteuid().as_raw()).unwrap();
    // The lock uses POSIX process ownership; only a separate process can hold it.
    let mut reader = Command::new("/usr/bin/python3").args(["-c", "import sys,fcntl; f=open(sys.argv[1],'a+'); fcntl.lockf(f,fcntl.LOCK_SH); print('locked',flush=True); sys.stdin.read()"])
        .arg(directory.join("admin.lock")).stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap();
    let mut line = String::new();
    BufReader::new(reader.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    assert_eq!(line.trim(), "locked");
    f.config.load_bound = Duration::from_millis(30);
    let before = Instant::now();
    let result = run_authorized(
        &f.config,
        &agent(&f),
        Cause { uid: 0 },
        |_, _, _, _, _| panic!("lock wait consumed the load window"),
        |_| panic!("account"),
    );
    drop(reader.stdin.take());
    assert!(reader.wait().unwrap().success());
    assert!(
        matches!(result, Err(Failure::Admission(ref r)) if **r == LifecycleRefusal::NoResidency),
        "{result:?}"
    );
    assert!(
        before.elapsed() < Duration::from_millis(500),
        "courtesy wait extended Enter"
    );
}
