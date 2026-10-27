//! Private delegated process instruments for the complete admin invocation.
use super::*;
use crate::seed::preparation::tests::fixture_with_member;
use serde_json::json;
use std::fs;
use std::os::fd::AsRawFd;
use std::os::unix::fs::PermissionsExt;
use std::time::Duration;

const RUN: &str = "forced-leave-fixture";

#[path = "serving_leave_escalation_tests.rs"]
mod escalation;
#[path = "serving_leave_join_tests.rs"]
mod joins;
#[path = "serving_leave_joint_tests.rs"]
mod joint;
#[path = "serving_leave_recovery_tests.rs"]
mod recovery;

// A root fixture process performs the scripted worker and member duties. Peer
// PID/run-lock and all file judgments are real. This is not role-isolation,
// SQLite, model, trace or language qualification.
const WORKER: &str = r#"#!/usr/bin/python3
import errno,fcntl,hashlib,json,os,signal,socket,sys,time
f=json.load(open(sys.argv[1])); case=f['case']; signal.alarm(15)
s=socket.socket(socket.AF_UNIX,socket.SOCK_SEQPACKET); s.bind(f['socket']); s.listen(3)
open(f['ready'],'w').close()
def locked():
    with open(f['lock'],'r+') as lock:
        try: fcntl.lockf(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
        except OSError as e: assert e.errno in (errno.EAGAIN,errno.EACCES)
        else: raise AssertionError('admin invocation lock escaped')
def reply(c,exchange,kind,body,position='continue'):
    locked(); c.send(json.dumps({'exchange':exchange,'position':position,'payload':{'kind':kind,'body':body}}).encode())
def receive(c,kind):
    value=json.loads(c.recv(65536)); assert value['payload']['kind']==kind,value
    locked(); return value['payload']['body']
c,_=s.accept(); observed=json.loads(c.recv(65536)); assert observed['payload']['body']['kind']=='observe'
locked(); open(f['observed'],'w').close()
if case=='graceful-transition':
    reply(c,observed['exchange'],'answer',{'kind':'in_transition','constituents':[os.getpid()]},'close'); c.close()
    while True: time.sleep(.001)
reply(c,observed['exchange'],'answer',{'kind':'state','state':'idle','load':f['facts']},'close'); c.close()
if case=='peer-change':
    s.close(); os.unlink(f['socket'])
    if os.fork()!=0:
        while True: time.sleep(1)
    s=socket.socket(socket.AF_UNIX,socket.SOCK_SEQPACKET); s.bind(f['socket']); s.listen(1)
c,_=s.accept(); leave=json.loads(c.recv(65536)); exchange=leave['exchange']
assert leave['payload']['body']=={'kind':'leave','cause':{'uid':73},'forced':f['forced'],'rollback':False},leave
open(os.path.join(f['root'],'leave-began'),'w').write(str(time.monotonic_ns()))
def term(sig,frame):
    locked()
    marker=os.path.join(f['root'],'run.marker')
    actual=list(open(marker,'rb').read()) if os.path.exists(marker) else None
    assert actual==f['initial_marker'],'marker changed before run end'
    with open(os.path.join(f['root'],'term-seen'),'w') as out: out.write(str(time.monotonic_ns()))
signal.signal(signal.SIGTERM,term)
def remain():
    open(os.path.join(f['root'],'waiting'),'w').close()
    while True: time.sleep(.001)
if case.startswith('graceful-'):
    assert open(f['lock']).read().split()[0]=='unload'
    if case=='graceful-refused':
        reply(c,exchange,'refusal',{'kind':'out_of_order'},'close'); remain()
    if case=='graceful-timeout': remain()
    time.sleep(.15)
if case.startswith('initial-'):
    if case=='initial-eof': sys.exit(0)
    if case=='initial-bad-record':
        with open(f['cgroup_record'],'w') as out: out.write('bad record')
    if case in ('initial-out-of-order','initial-foreign-exchange','initial-refused'):
        if case=='initial-foreign-exchange': exchange['ordinal']+=1
        refusal='descriptors_unusable' if case=='initial-refused' else 'out_of_order'
        reply(c,exchange,'refusal',{'kind':refusal},'close')
    if case=='initial-malformed': c.send(b'{')
    remain()
locked(); now=time.monotonic_ns(); capture='9'
reply(c,exchange,'save_point_beginning',{'capture_id':capture,
    'window':{'began_ns':str(now),'deadline_ns':str(now+f['save_ns'])},
    'tail_window':{'began_ns':str(now),'deadline_ns':str(now+150000000000)}})
if case.startswith('capture-'):
    if case=='capture-out-of-order': reply(c,exchange,'refusal',{'kind':'out_of_order'},'close')
    remain()
part=os.path.join(f['room'],'capture-9.sqlite.part'); image=b'exact scripted captured image'
with open(part,'wb') as out: out.write(image)
os.chown(part,65533,65533); os.chmod(part,0o600)
reply(c,exchange,'save_point_image',{'capture_id':capture,'room_name':'capture-9.sqlite.part','size_bytes':str(len(image)),'position':None})
checked=receive(c,'save_point_image_checked'); assert checked['dump_sha256']==hashlib.sha256(image).hexdigest()
finished=os.path.join(f['room'],'capture-9.sqlite'); os.rename(part,finished)
reply(c,exchange,'save_point_image_finished',{'capture_id':capture,'room_name':'capture-9.sqlite'})
prepared=receive(c,'save_point_prepared'); guard=os.path.join(f['saves'],'.'+prepared['artifact_id']+'.pending')
assert os.path.isfile(guard); assert len(open(f['index']).readlines())==1
if case=='eof-precommit': sys.exit(0)
reply(c,exchange,'save_point_commit',{'capture_id':capture})
published=receive(c,'save_point_published'); assert published==prepared
assert os.path.isfile(guard); entry=json.loads(open(f['index']).readlines()[-1])
assert entry['artifact_id']==published['artifact_id']
if case=='eof-postcommit': sys.exit(0)
os.unlink(finished)
reply(c,exchange,'save_point_capture_complete',{'capture_id':capture})
if case=='late-silent': remain()
if case in ('guard-tamper','guard-tamper-cleanup'):
    guard_json=json.load(open(guard)); guard_json['capture_id']='10'
    with open(guard,'w') as out: json.dump(guard_json,out)
if case in ('cgroup-record','failed-cleanup','guard-tamper-cleanup'):
    with open(f['cgroup_record'],'w') as out: out.write('broken record')
if case=='marker-write': os.mkdir(os.path.join(f['root'],'.run.marker.new'))
report={'event_run':f['facts']['run'],'event_sequence':'12','capture_id':capture,'start':{k:entry[k] for k in ['seed_id','artifact_id','dump_sha256','config_sha256','parent_artifact_id','position']}}
if case in ('foreign-left','failed-cleanup'): report['event_run']='another-run'
reply(c,exchange,'answer',{'kind':'left','save_point':report,'forced':f['forced'] or case=='graceful-converted','no_state':False},'close')
c.close(); s.close(); locked()
# A timely Left followed by EOF remains valid past the capture window. A
# descendant without lockfd9 must still be removed by the actual cgroup end.
if case=='success':
    child=os.fork()
    if child==0:
        os.close(9)
        while True: time.sleep(1)
    print('orphan',child,flush=True)
    time.sleep(0.4); locked()
if case=='escalate':
    signal.signal(signal.SIGTERM,signal.SIG_IGN)
    while True: time.sleep(1)
print('script-complete',flush=True)
"#;

const WATCH: &str = r#"
import errno,fcntl,json,os,signal,sys,time
f=json.load(open(sys.argv[1])); signal.alarm(15)
while not os.path.exists(f['observed']): time.sleep(.001)
with open(f['lock'],'r+') as lock:
    while True:
        try: fcntl.lockf(lock,fcntl.LOCK_EX|fcntl.LOCK_NB); break
        except OSError as e: assert e.errno in (errno.EAGAIN,errno.EACCES)
        time.sleep(.001)
    assert json.load(open(os.path.join(f['root'],'run.marker')))=={'run':f['facts']['run'],'state':'closed'}
    assert not os.path.exists(f['cgroup_record'])
    assert not any(n.endswith('.pending') for n in os.listdir(f['saves']))
"#;

struct Children<'a> {
    config: &'a ServiceConfig,
    record: crate::containment::Record,
    worker: std::process::Child,
    watcher: Option<std::process::Child>,
}
impl Drop for Children<'_> {
    fn drop(&mut self) {
        if let Some(watcher) = &mut self.watcher {
            let _ = watcher.kill();
            let _ = watcher.wait();
        }
        // Restore deliberate record corruption only for test teardown, never
        // for the invocation whose failure is being measured.
        let dir = agent_cgroup::open_run_directory(&self.config.run_directory()).unwrap();
        if fs::metadata(self.config.run_directory().join(crate::containment::RECORD)).is_ok() {
            crate::containment::write_record(dir.as_fd(), &self.record).unwrap();
        }
        let _ = agent_cgroup::run_end(
            dir.as_fd(),
            &self.config.agent,
            agent_cgroup::Concluding::Run(RUN),
            Duration::from_secs(3),
        );
        let _ = self.worker.kill();
        let _ = self.worker.wait();
    }
}

fn root_case(case: &str) {
    assert!(
        nix::unistd::geteuid().is_root(),
        "requires a private root delegated scope"
    );
    let f = fixture_with_member(inventory::MemberAccount {
        uid: 65533,
        gid: 65533,
    });
    let draft_path = f.config.territory.join("agent.toml");
    // Success deliberately outlives this capture window after the real Left.
    let escalation_case = escalation::applies(case);
    let graceful = case.starts_with("graceful-");
    let save_ns = if case.starts_with("capture-") {
        100_000_000
    } else if case == "success" {
        250_000_000
    } else {
        3_000_000_000_u64
    };
    let draft = fs::read_to_string(&draft_path).unwrap().replace(
        "save-wait.elapsed-ns = \"17\"",
        &format!("save-wait.elapsed-ns = \"{save_ns}\""),
    );
    fs::write(&draft_path, &draft).unwrap();
    let loaded = crate::serving_load::tests::setup(&f);
    // Loaded-save preparation must not parse the fresh draft or the selector.
    fs::write(&draft_path, "this is not a declaration").unwrap();
    fs::write(f.config.root.join("restore-artifact"), "not an artifact ID").unwrap();
    let repository = Repository::from_judged_directory(
        f.config
            .save_points_fd()
            .unwrap()
            .try_clone_to_owned()
            .unwrap(),
        save_points::ROOT,
        f.config.access_gid,
    )
    .unwrap();
    let source = repository
        .verify_loaded(
            &loaded,
            &weaver_types::SessionId("fixture-session".into()),
            loaded.config_sha256.as_str(),
            &f.config.territory,
        )
        .unwrap();
    let mut facts = json!({"session":"fixture-session","run":RUN,"declaration":loaded.config_sha256,
        "artifact":"/models/fixture.gguf","residual_readout":false,"surprisal":false,
        "state":{"serving":{"state_management":source.declaration().state_management(),"start":loaded,
            "ste100_scheme":"retained fixture","ste100_binary_sha256":"e".repeat(64)}},
        "state_store":{"engine":"sqlite"},"state_member":true,"composer":{"binary":"worker"}});
    if matches!(
        case,
        "graceful-drain-only" | "graceful-bounded" | "graceful-timeout"
    ) {
        facts["drain_bound"] = json!(1);
    }
    if matches!(
        case,
        "graceful-wind-only" | "graceful-bounded" | "graceful-timeout"
    ) {
        facts["wind_down_bound"] = json!(1);
    }
    let run_dir = crate::prepare_run_directory(&f.config, 0).unwrap();
    start::prepare_runtime_directory(&f.config.coordination_root, &f.config.agent, 0, 0).unwrap();
    let invocation =
        start::take_invocation_lock(&run_dir, crate::verb_table::HolderVerb::Load).unwrap();
    let run_lock = start::take_run_lock(&run_dir).unwrap().unwrap();
    let dir = agent_cgroup::open_run_directory(&run_dir).unwrap();
    let made = agent_cgroup::make(dir.as_fd(), &f.config.agent, RUN, (0, 0)).unwrap();
    let initial_marker = match case {
        "closed" | "initial-closed-marker" => Some(save_points::Marker::Closed { run: RUN.into() }),
        "foreign-marker" | "initial-foreign-marker" => Some(save_points::Marker::Open {
            run: "unrelated".into(),
        }),
        "absent-marker" | "initial-absent-marker" => None,
        _ => Some(save_points::Marker::Open { run: RUN.into() }),
    };
    save_points::write_marker(&f.config.root, initial_marker.as_ref()).unwrap();
    let room = f.config.territory.join("state");
    fs::create_dir(&room).unwrap();
    fs::set_permissions(&room, fs::Permissions::from_mode(0o700)).unwrap();
    nix::unistd::chown(
        &room,
        Some(nix::unistd::Uid::from_raw(65533)),
        Some(nix::unistd::Gid::from_raw(65533)),
    )
    .unwrap();
    let saves = f.config.territory.join("save-points");
    let before_room = recovery::files(&room);
    let before_saves = recovery::files(&saves);
    let fixture_path = f.config.root.join("leave-fixture.json");
    let inputs = json!({"case":case,"forced":!graceful,"save_ns":save_ns,"facts":facts,"socket":f.config.coordination_socket(),
        "initial_marker":fs::read(f.config.root.join("run.marker")).ok(),
        "room":room,"saves":saves,"index":saves.join(save_points::MANIFEST),"root":f.config.root,
        "lock":run_dir.join("admin.lock"),"observed":f.config.root.join("observed"),
        "ready":f.config.root.join("worker-ready"),
        "cgroup_record":run_dir.join(crate::containment::RECORD)});
    fs::write(&fixture_path, serde_json::to_vec(&inputs).unwrap()).unwrap();
    fs::write(&f.config.worker, WORKER).unwrap();
    fs::set_permissions(&f.config.worker, fs::Permissions::from_mode(0o700)).unwrap();
    let log_path = f.config.root.join("worker.log");
    let log = start::open_log(&log_path, None).unwrap();
    let worker = start::spawn_worker(start::WorkerStart {
        binary: &f.config.worker,
        arguments: vec![fixture_path.to_str().unwrap().into()],
        uid: 0,
        gid: 0,
        home: &f.boundary.home,
        library_path: None,
        log: &log,
        run_lock: &run_lock,
        relay_write: None,
        placement: made.run_procs().as_raw_fd(),
    })
    .unwrap();
    drop(run_lock);
    drop(invocation);
    let watcher = (case == "success").then(|| {
        std::process::Command::new("/usr/bin/python3")
            .args(["-c", WATCH])
            .arg(&fixture_path)
            .spawn()
            .unwrap()
    });
    let mut children = Children {
        config: &f.config,
        record: made.record,
        worker,
        watcher,
    };
    let ready_deadline = std::time::Instant::now() + Duration::from_secs(3);
    while !f.config.root.join("worker-ready").exists() {
        assert!(
            std::time::Instant::now() < ready_deadline,
            "fixture worker did not listen"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    crate::TEST_WORKER_UID.with(|uid| uid.set(Some(0)));
    let result = run_mode(
        &f.config,
        &AgentName(f.config.agent.clone()),
        Cause { uid: 73 },
        UnloadBounds {
            leave: if graceful {
                Duration::from_millis(50)
            } else if escalation_case {
                Duration::from_millis(500)
            } else {
                Duration::from_secs(4)
            },
            after_left: if escalation_case {
                Duration::from_millis(100)
            } else {
                Duration::from_secs(1)
            },
            term: Duration::from_millis(100),
            kill: Duration::from_secs(1),
        },
        !graceful,
        |_| Ok(65533),
    );
    crate::TEST_WORKER_UID.with(|uid| uid.set(None));
    let log = fs::read_to_string(&log_path).unwrap();
    if case == "graceful-timeout" {
        let concluded = result.unwrap_or_else(|err| panic!("{err:?}\n{log}"));
        assert_eq!(concluded.answer, crate::unloaded_answer());
        let retained = concluded.evidence.unwrap();
        assert!(matches!(
            retained.force_trigger,
            Some(FailureCause::Channel(
                crate::channel::ChannelFault::Unanswered
            ))
        ));
        assert!(retained.capture_id.is_none() && retained.report.is_none() && !retained.committed);
        assert!(!start::run_lock_held(&run_dir).unwrap());
        assert!(!run_dir.join(crate::containment::RECORD).exists());
        assert_eq!(
            save_points::load_marker(&f.config.root).unwrap(),
            Some(save_points::Marker::Forced { run: RUN.into() })
        );
        assert_eq!(recovery::files(&room), before_room);
        assert_eq!(recovery::files(&saves), before_saves);
        let ns = |name: &str| {
            fs::read_to_string(f.config.root.join(name))
                .unwrap()
                .parse::<u64>()
                .unwrap()
        };
        assert!(ns("term-seen") - ns("leave-began") >= 1_900_000_000);
        return;
    }
    if matches!(case, "graceful-refused" | "graceful-transition") {
        let failure = result.unwrap_err();
        assert_eq!(
            failure.refusal(),
            weaver_types::state_flow::capture::CaptureRefusal::Lifecycle(
                LifecycleRefusal::OutOfOrder
            )
        );
        assert!(failure.cleanup.is_none());
        assert!(start::run_lock_held(&run_dir).unwrap());
        assert!(children.worker.try_wait().unwrap().is_none());
        assert!(!f.config.root.join("term-seen").exists());
        assert_eq!(
            save_points::load_marker(&f.config.root).unwrap(),
            initial_marker
        );
        assert_eq!(recovery::files(&room), before_room);
        assert_eq!(recovery::files(&saves), before_saves);
        if case == "graceful-transition" {
            assert!(failure.evidence.is_none());
            assert!(
                matches!(failure.cause, Fault::Sequence(FailureCause::InTransition { ref constituents }) if constituents == &[children.worker.id()])
            );
            assert!(!f.config.root.join("leave-began").exists());
        } else {
            let retained = failure.evidence.unwrap();
            assert!(
                retained.capture_id.is_none() && retained.report.is_none() && !retained.committed
            );
        }
        return;
    }
    if escalation_case {
        escalation::check(
            case,
            &f.config,
            result,
            initial_marker,
            (&before_room, &before_saves),
            &mut children.worker,
            &log,
        );
        return;
    }
    if case == "peer-change" {
        assert_eq!(
            result.as_ref().unwrap_err().refusal(),
            weaver_types::state_flow::capture::CaptureRefusal::Lifecycle(
                LifecycleRefusal::BoundaryUnverified
            )
        );
        assert!(
            matches!(
                result,
                Err(Failure {
                    cause: Fault::Sequence(FailureCause::Channel(
                        crate::channel::ChannelFault::WrongPeer { .. }
                    )),
                    evidence: None,
                    ..
                })
            ),
            "different live worker accepted: {result:?}\n{log}"
        );
        assert_eq!(
            fs::read_to_string(saves.join(save_points::MANIFEST))
                .unwrap()
                .lines()
                .count(),
            1
        );
        assert_eq!(fs::read_dir(&room).unwrap().count(), 0);
        assert!(start::run_lock_held(&run_dir).unwrap());
        return;
    }
    assert!(!log.contains("Traceback"), "{case}: fixture failed: {log}");
    let index = fs::read_to_string(saves.join(save_points::MANIFEST))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .collect::<Vec<_>>();
    let guarded = fs::read_dir(&saves)
        .unwrap()
        .filter_map(|e| {
            let p = e.unwrap().path();
            p.file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .ends_with(".pending")
                .then_some(p)
        })
        .collect::<Vec<_>>();
    let selected = repository.load(
        None,
        &crate::save_points::serving::repository::LoadScope {
            session: &source.declaration().agent().session,
            trace_path: crate::save_points::serving::trace_path(
                &source.declaration().agent().trace_sink,
            ),
        },
    );
    assert_eq!(
        selected.is_ok(),
        index.len() == 1 || guarded.is_empty(),
        "{case}: an indexed guarded save must not select or fall back: {selected:?}"
    );
    if matches!(
        case,
        "success" | "escalate" | "closed" | "foreign-marker" | "absent-marker"
    ) || graceful
    {
        let concluded = result.unwrap_or_else(|error| panic!("{case}: {:?}\n{log}", error.cause));
        assert_eq!(concluded.answer, crate::unloaded_answer());
        let evidence = concluded.evidence.expect("actual capture evidence");
        assert_eq!(evidence.loaded, loaded);
        assert_eq!(evidence.run.0, RUN);
        assert_eq!(index.len(), 2);
        if matches!(case, "success" | "escalate") || graceful {
            assert_eq!(
                index[1]["cause"],
                if graceful && case != "graceful-converted" {
                    "graceful_unload"
                } else {
                    "forced_unload"
                }
            );
            assert_eq!(evidence.completed, Some(evidence.report.unwrap().start));
            assert!(guarded.is_empty());
            assert_eq!(
                save_points::load_marker(&f.config.root).unwrap(),
                Some(save_points::Marker::Closed { run: RUN.into() })
            );
            if let Some(watcher) = &mut children.watcher {
                assert!(watcher.wait().unwrap().success());
            }
            let status = children.worker.wait().unwrap();
            if case == "success" || graceful {
                assert!(
                    status.success(),
                    "timely Left must receive its after-left wait: {status}\n{log}"
                );
                assert!(log.contains("script-complete"), "{log}");
            } else {
                use std::os::unix::process::ExitStatusExt;
                assert_eq!(status.signal(), Some(nix::libc::SIGKILL));
            }
        } else {
            assert!(evidence.completed.is_none());
            assert_eq!(guarded.len(), 1);
            assert_eq!(
                save_points::load_marker(&f.config.root).unwrap(),
                initial_marker
            );
        }
    } else {
        let failure = result.unwrap_err();
        let public = failure.refusal();
        let rendered = crate::surface::render_serving_refusal(&public);
        let value: serde_json::Value = serde_json::from_str(&rendered).unwrap();
        let expected = match case {
            "eof-postcommit" | "foreign-left" | "failed-cleanup" => "failed_run",
            "eof-precommit" => "unanswered",
            "guard-tamper" | "guard-tamper-cleanup" => "state_persistence_invalid",
            "cgroup-record" | "marker-write" => "boundary_unverified",
            _ => unreachable!(),
        };
        assert_eq!(value["kind"], expected, "{case}: {rendered}");
        if expected == "failed_run" {
            let evidence = failure.evidence.as_ref().unwrap();
            assert!(evidence.committed && evidence.report.is_none());
            assert_eq!(value["run"], RUN);
            assert_eq!(value["capture_id"], "9");
            assert_eq!(
                value["published"],
                index
                    .last()
                    .map(|entry| {
                        json!({"seed_id":entry["seed_id"],"artifact_id":entry["artifact_id"],
                    "dump_sha256":entry["dump_sha256"],"config_sha256":entry["config_sha256"],
                    "parent_artifact_id":entry["parent_artifact_id"],"position":entry["position"]})
                    })
                    .unwrap()
            );
            assert!(!value["artifacts"].as_array().unwrap().is_empty());
            for artifact in &evidence.publication_failure.as_ref().unwrap().artifacts {
                assert!(failure.to_string().contains(&artifact.location));
            }
        } else if matches!(case, "guard-tamper" | "guard-tamper-cleanup") {
            assert_eq!(value["stage"], "publish");
            assert_eq!(value["case"], "publication_failed");
        } else if matches!(case, "cgroup-record" | "marker-write") {
            assert!(failure.to_string().contains("completed save remains"));
            assert!(value.get("published").is_none());
        }
        match case {
            "eof-precommit" | "eof-postcommit" => assert!(
                matches!(
                    failure.cause,
                    Fault::Sequence(FailureCause::Channel(crate::channel::ChannelFault::Closed))
                ),
                "{case}: {:?}",
                failure.cause
            ),
            "foreign-left" | "failed-cleanup" => assert!(
                matches!(
                    failure.cause,
                    Fault::Sequence(FailureCause::Protocol(
                        "Left differs from the published capture"
                    ))
                ),
                "{case}: {:?}",
                failure.cause
            ),
            "guard-tamper" | "guard-tamper-cleanup" => assert!(
                matches!(failure.cause, Fault::Sequence(FailureCause::Publication(_))),
                "{case}: {:?}",
                failure.cause
            ),
            "cgroup-record" | "marker-write" => assert!(
                matches!(
                    failure.cause,
                    Fault::Admission(LifecycleRefusal::BoundaryUnverified)
                ),
                "{case}: {:?}",
                failure.cause
            ),
            _ => unreachable!(),
        }
        assert_eq!(
            failure.cleanup.as_deref(),
            (matches!(case, "failed-cleanup" | "guard-tamper-cleanup"))
                .then_some(&LifecycleRefusal::BoundaryUnverified)
        );
        let evidence = failure
            .evidence
            .as_ref()
            .unwrap_or_else(|| panic!("{case}: {failure:?}\n{log}"));
        assert_eq!(evidence.run.0, RUN);
        assert!(
            evidence.publication_failure.is_some(),
            "{case}: {failure:?}"
        );
        assert_eq!(index.len(), if case == "eof-precommit" { 1 } else { 2 });
        assert_eq!(
            save_points::load_marker(&f.config.root).unwrap(),
            initial_marker
        );
        if matches!(case, "cgroup-record" | "marker-write") {
            assert_eq!(
                evidence.completed,
                Some(evidence.report.as_ref().unwrap().start.clone())
            );
            assert!(guarded.is_empty());
            assert!(
                selected.is_ok(),
                "a cleanup failure cannot revoke a completed save"
            );
        } else {
            assert!(evidence.completed.is_none());
            assert_eq!(guarded.len(), 1);
        }
    }
    assert!(!start::run_lock_held(&run_dir).unwrap(), "{case}: {log}");
    if !matches!(
        case,
        "cgroup-record" | "failed-cleanup" | "guard-tamper-cleanup"
    ) {
        assert!(
            !run_dir.join(crate::containment::RECORD).exists(),
            "{case}: {log}"
        );
    }
}

#[test]
#[ignore = "root-owned delegated cgroup scope; private scripted peer and actual admin"]
fn forced_leave_invocation_completes_after_left_eof_and_cgroup_end() {
    root_case("success");
}

#[test]
#[ignore = "root-owned delegated cgroup scope; private scripted peer and actual admin"]
fn forced_leave_invocation_escalates_only_after_timely_left() {
    root_case("escalate");
}

#[test]
#[ignore = "root-owned delegated cgroup scope; private scripted peer and actual admin"]
fn forced_leave_invocation_retains_guard_on_protocol_and_conclusion_failure() {
    for case in [
        "eof-precommit",
        "eof-postcommit",
        "foreign-left",
        "guard-tamper",
        "guard-tamper-cleanup",
        "cgroup-record",
        "failed-cleanup",
        "marker-write",
    ] {
        root_case(case);
    }
}

#[test]
#[ignore = "root-owned delegated cgroup scope; private scripted peer and actual admin"]
fn forced_leave_invocation_rejects_changed_worker_before_capture() {
    root_case("peer-change");
}

#[test]
#[ignore = "root-owned delegated cgroup scope; private scripted peer and actual admin"]
fn forced_leave_invocation_respects_prior_marker_without_completing_a_new_save() {
    for case in ["closed", "foreign-marker", "absent-marker"] {
        root_case(case);
    }
}

#[test]
#[ignore = "root-owned delegated cgroup scope; actual graceful admin and scripted worker"]
fn graceful_leave_invocation_preserves_loaded_windows_and_final_forced_state() {
    for case in [
        "graceful-success",
        "graceful-drain-only",
        "graceful-wind-only",
        "graceful-bounded",
        "graceful-converted",
    ] {
        root_case(case);
    }
}

#[test]
#[ignore = "root-owned delegated cgroup scope; actual graceful admin and scripted refusal"]
fn graceful_leave_invocation_refused_before_capture_keeps_the_run() {
    root_case("graceful-refused");
    root_case("graceful-transition");
}

#[test]
#[ignore = "root-owned delegated cgroup scope; silent graceful worker and actual escalation"]
fn graceful_leave_invocation_escalates_only_after_both_loaded_bounds() {
    root_case("graceful-timeout");
}
