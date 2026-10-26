//! Actual force admission, locks, containment and indexed-file verification.
//! The graceful holder and worker's terminal replies are scripted test peers.
use super::*;
use crate::save_points::serving::publication::{Progress, Publication, ServingImage};
use crate::save_points::serving::repository::LoadScope;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Instant;
use weaver_types::state_flow::capture::CaptureRefusal;
use weaver_types::state_flow::{PersistenceCause, SavePointReport};

const JOIN_WORKER: &str = r#"#!/usr/bin/python3
import errno,fcntl,json,os,select,signal,socket,sys,time
f=json.load(open(sys.argv[1])); mode=f['mode']; signal.alarm(15)
def wait(name):
    while not os.path.exists(f[name]): time.sleep(.001)
def held():
    with open(f['lock'],'r+') as lock:
        try: fcntl.lockf(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
        except OSError as e: assert e.errno in (errno.EAGAIN,errno.EACCES)
        else: raise AssertionError('join did not wait behind original holder')
if mode=='sealed':
    os.close(9); open(f['ready'],'w').close(); sys.exit(0)
s=socket.socket(socket.AF_UNIX,socket.SOCK_SEQPACKET); s.bind(f['socket']); s.listen(3)
open(f['ready'],'w').close()
if f['verb']!='unload':
    while not os.path.exists(f['returned']):
        if select.select([s],[],[],.001)[0]:
            raise AssertionError('force dialed a non-unload holder')
    sys.exit(0)
for attempt in range(2 if mode in ('retry','retry-holder') else 1):
    c,_=s.accept(); value=json.loads(c.recv(65536)); held()
    assert value['payload']['body']=={'kind':'join_leave','cause':{'uid':73}},value
    if mode in ('retry','retry-holder') and attempt==0:
        c.send(json.dumps({'exchange':value['exchange'],'position':'close','payload':{'kind':'refusal','body':{'kind':'out_of_order'}}}).encode()); c.close()
        if mode=='retry-holder':
            open(f['joined'],'w').close()
            while not os.path.exists(f['returned']):
                if select.select([s],[],[],.001)[0]:
                    raise AssertionError('force retried after holder changed verb')
            sys.exit(0)
        continue
    open(f['joined'],'w').close()
    if mode=='silent':
        while True: time.sleep(1)
    if mode=='eof': c.close(); sys.exit(0)
    report=f['report']; exchange=value['exchange']
    if mode=='foreign-exchange': exchange['ordinal']+=1
    if mode=='foreign-run': report['event_run']='another-run'
    if mode=='wrong-capture': report['capture_id']='10'
    if mode=='wrong-hash': report['start']['dump_sha256']='b'*64
    payload={'kind':'answer','body':{'kind':'left','save_point':report,'forced':mode!='late','no_state':False}}
    position='close'
    if mode=='refused': payload={'kind':'refusal','body':{'kind':'descriptors_unusable'}}
    if mode=='failed-run':
        payload={'kind':'refusal','body':{'kind':'failed_run','run':f['run'],'capture_id':'9','phase':'publishing',
            'published':report['start'],'artifacts':[{'location':'/worker-supplied/claim','kind':'dump','sha256':None}],
            'detail':'worker claims root publication'}}
    if mode=='beginning':
        now=time.monotonic_ns(); position='continue'
        payload={'kind':'save_point_beginning','body':{'capture_id':'9','window':{'began_ns':str(now),'deadline_ns':str(now+1000000000)},'tail_window':{'began_ns':str(now),'deadline_ns':str(now+150000000000)}}}
    wire=json.dumps({'exchange':exchange,'position':position,'payload':payload}).encode()
    with open(f['wire'],'wb') as out: out.write(wire)
    c.send(wire); c.close(); s.close()
    if mode=='foreign-marker':
        while True: time.sleep(1)
    # A timely Left must wait behind the invocation lock before killing holders.
    if mode in ('closed','open','late','guard','unjudged','retry'):
        wait('holder_done')
        time.sleep(.02)
    sys.exit(0)
"#;

const HOLDER: &str = r#"
import fcntl,json,os,signal,sys,time
f=json.load(open(sys.argv[1])); signal.alarm(15)
with open(f['lock'],'r+') as lock:
    fcntl.lockf(lock,fcntl.LOCK_EX)
    lock.seek(0); lock.truncate(); lock.write(f['verb']+' '+str(os.getpid())); lock.flush()
    open(f['holder_ready'],'w').close()
    if f['verb']!='unload':
        while not os.path.exists(f['returned']): time.sleep(.001)
    elif f['mode']!='sealed':
        while not os.path.exists(f['joined']): time.sleep(.001)
    if f['mode']=='silent':
        with open(f['run_lock'],'r+') as run:
            while True:
                try: fcntl.lockf(run,fcntl.LOCK_EX|fcntl.LOCK_NB); break
                except BlockingIOError: time.sleep(.001)
    if f['mode']=='retry-holder':
        lock.seek(0); lock.truncate(); lock.write('save-point '+str(os.getpid())); lock.flush()
        while not os.path.exists(f['returned']): time.sleep(.001)
    time.sleep(.15)
    if f['mode']=='late-marker-foreign':
        with open(f['marker'],'w') as out: json.dump({'run':f['run'],'state':'open'},out)
    assert json.load(open(f['marker']))=={'run':f['run'],'state':'open'},'join concluded before it owned the lock'
    if f['mode']=='closed':
        with open(f['marker'],'w') as out: json.dump({'run':f['run'],'state':'closed'},out)
    if f['mode']=='foreign-marker':
        with open(f['marker'],'w') as out: json.dump({'run':'unrelated','state':'open'},out)
    open(f['holder_done'],'w').close()
"#;

struct Holder(Child);
impl Drop for Holder {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn wait(path: &Path) {
    let end = Instant::now() + Duration::from_secs(3);
    while !path.exists() {
        assert!(
            Instant::now() < end,
            "fixture did not produce {}",
            path.display()
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn report(config: &ServiceConfig, loaded: &StartIdentity, mode: &str) -> SavePointReport {
    let mut repository = Repository::from_judged_directory(
        config
            .save_points_fd()
            .unwrap()
            .try_clone_to_owned()
            .unwrap(),
        save_points::ROOT,
        config.access_gid,
    )
    .unwrap();
    let source = repository
        .verify_loaded(
            loaded,
            &weaver_types::SessionId("fixture-session".into()),
            loaded.config_sha256.as_str(),
            &config.territory,
        )
        .unwrap();
    let scope = LoadScope {
        session: &source.declaration().agent().session,
        trace_path: crate::save_points::serving::trace_path(
            &source.declaration().agent().trace_sink,
        ),
    };
    let room = config.territory.join("state");
    fs::create_dir(&room).unwrap();
    fs::set_permissions(&room, fs::Permissions::from_mode(0o700)).unwrap();
    let path = room.join("capture-9.sqlite.part");
    let image = b"scripted capture image, not SQLite qualification";
    fs::write(&path, image).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    let room = save_points::open_directory(&room).unwrap();
    let run = RunId(
        if mode == "late-marker-foreign" {
            "another-run"
        } else {
            RUN
        }
        .into(),
    );
    let mut publication = Publication::serving(
        &mut repository,
        &scope,
        ServingImage {
            loaded,
            run: run.clone(),
            cause: if mode == "wrong-cause" {
                PersistenceCause::Requested
            } else {
                PersistenceCause::GracefulUnload
            },
            capture_id: 9.into(),
            room_name: "capture-9.sqlite.part",
            size_bytes: (image.len() as u64).into(),
            position: None,
            room: room.as_fd(),
            member_uid: 0,
        },
        &mut |_| Ok(()),
    )
    .unwrap();
    fs::rename(&path, path.with_file_name("capture-9.sqlite")).unwrap();
    publication
        .image_finished(&run, 9.into(), "capture-9.sqlite", &mut |_| Ok(()))
        .unwrap();
    publication.commit(&run, 9.into(), &mut |_| Ok(())).unwrap();
    let identity = loop {
        match publication.advance(&mut |_| Ok(())).unwrap() {
            Progress::Published(identity) => break identity,
            Progress::Renamed | Progress::Verified => (),
            Progress::Complete(_) => panic!("capture cannot complete before Left"),
        }
    };
    let report = SavePointReport {
        event_run: run,
        event_sequence: 12.into(),
        capture_id: 9.into(),
        start: identity,
    };
    if mode != "guard" {
        publication
            .finish(
                if mode == "wrong-cause" {
                    FinalOutcome::SavePointTaken(&report)
                } else {
                    FinalOutcome::Left(&report)
                },
                &mut |_| Ok(()),
            )
            .unwrap();
    }
    report
}

fn scenario(mode: &str, verb: &str) {
    assert!(
        nix::unistd::geteuid().is_root(),
        "private root delegated scope required"
    );
    let mut f = fixture_with_member(inventory::MemberAccount {
        uid: 65533,
        gid: 65533,
    });
    let loaded = crate::serving_load::tests::setup(&f);
    let directory = crate::prepare_run_directory(&f.config, 0).unwrap();
    start::prepare_runtime_directory(&f.config.coordination_root, &f.config.agent, 0, 0).unwrap();
    let invocation =
        start::take_invocation_lock(&directory, crate::verb_table::HolderVerb::Load).unwrap();
    let report = report(&f.config, &loaded, mode);
    let saves = f.config.territory.join("save-points");
    let room = f.config.territory.join("state");
    let saved_before = super::recovery::files(&saves);
    let room_before = super::recovery::files(&room);
    save_points::write_marker(
        &f.config.root,
        Some(&save_points::Marker::Open { run: RUN.into() }),
    )
    .unwrap();
    if mode == "late-marker-foreign" {
        fs::write(
            f.config.root.join(save_points::MARKER),
            b"unreadable marker",
        )
        .unwrap();
    }
    // A joined conclusion must not read a freshly edited declaration/selector.
    fs::write(f.config.territory.join("agent.toml"), "invalid fresh draft").unwrap();
    fs::write(f.config.root.join("restore-artifact"), "invalid selector").unwrap();
    let run_lock = start::take_run_lock(&directory).unwrap().unwrap();
    let dir = agent_cgroup::open_run_directory(&directory).unwrap();
    let made = agent_cgroup::make(dir.as_fd(), &f.config.agent, RUN, (0, 0)).unwrap();
    let data = json!({"mode":mode,"verb":verb,"run":RUN,"report":report,
        "socket":f.config.coordination_socket(),"lock":directory.join("admin.lock"),
        "run_lock":directory.join("run.lock"),
        "marker":f.config.root.join(save_points::MARKER),
        "ready":f.config.root.join("worker-ready"),"holder_ready":f.config.root.join("holder-ready"),
        "holder_done":f.config.root.join("holder-done"),"joined":f.config.root.join("joined"),
        "returned":f.config.root.join("returned"),"wire":f.config.root.join("wire.json")});
    let inputs = f.config.root.join("join-fixture.json");
    fs::write(&inputs, serde_json::to_vec(&data).unwrap()).unwrap();
    fs::write(&f.config.worker, JOIN_WORKER).unwrap();
    fs::set_permissions(&f.config.worker, fs::Permissions::from_mode(0o700)).unwrap();
    let log_path = f.config.root.join("worker.log");
    let log = start::open_log(&log_path, None).unwrap();
    if mode == "unjudged" {
        f.config.territory_unjudged = true;
        f.config.territory_fd = None;
        f.config.save_points = None;
    }
    let worker = start::spawn_worker(start::WorkerStart {
        binary: &f.config.worker,
        arguments: vec![inputs.to_str().unwrap().into()],
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
    let mut children = Children {
        config: &f.config,
        record: made.record,
        worker,
        watcher: None,
    };
    wait(&f.config.root.join("worker-ready"));
    let holder_log = fs::File::create(f.config.root.join("holder.log")).unwrap();
    let mut holder = Holder(
        Command::new("/usr/bin/python3")
            .args(["-c", HOLDER])
            .arg(&inputs)
            .stdout(Stdio::from(holder_log.try_clone().unwrap()))
            .stderr(Stdio::from(holder_log))
            .spawn()
            .unwrap(),
    );
    wait(&f.config.root.join("holder-ready"));
    let result = run_authorized(
        &f.config,
        &AgentName(f.config.agent.clone()),
        Cause { uid: 73 },
        UnloadBounds {
            leave: Duration::from_millis(400),
            after_left: Duration::from_millis(60),
            term: Duration::from_millis(100),
            kill: Duration::from_secs(1),
        },
        |_| panic!("a join may neither inspect loaded capture nor take over its publisher"),
    );
    fs::write(f.config.root.join("returned"), b"returned").unwrap();
    assert!(
        holder.0.wait().unwrap().success(),
        "holder failed: {}",
        fs::read_to_string(f.config.root.join("holder.log")).unwrap()
    );
    if verb != "unload" || mode == "retry-holder" {
        assert!(
            children.worker.wait().unwrap().success(),
            "non-unload holder was dialed"
        );
    }
    let log = fs::read_to_string(&log_path).unwrap();
    assert!(!log.contains("Traceback"), "{mode}: {log}");
    assert_eq!(
        super::recovery::files(&saves),
        saved_before,
        "{mode}: join changed publication files"
    );
    assert_eq!(
        super::recovery::files(&room),
        room_before,
        "{mode}: join changed room files"
    );
    let marker = save_points::load_marker(&f.config.root).unwrap();
    let mut expected_marker = save_points::Marker::Open { run: RUN.into() };
    if verb != "unload" || mode == "retry-holder" {
        assert_eq!(
            result.as_ref().unwrap_err().refusal(),
            CaptureRefusal::Lifecycle(LifecycleRefusal::InvocationInFlight)
        );
        assert_eq!(
            f.config.root.join("joined").exists(),
            mode == "retry-holder"
        );
    } else if matches!(
        mode,
        "closed" | "open" | "late" | "unjudged" | "retry" | "sealed" | "silent"
    ) {
        let concluded = result
            .as_ref()
            .unwrap_or_else(|error| panic!("{mode}: {error:?}"));
        assert_eq!(concluded.answer, crate::unloaded_answer());
        assert!(concluded.evidence.is_none(), "a join has no capture owner");
        let joined = concluded
            .joined
            .as_ref()
            .expect("joined invocation evidence");
        assert_eq!(joined.noted_run, Some(Some(RUN.into())));
        if !matches!(mode, "sealed" | "silent") {
            assert_eq!(
                joined.received.as_ref().unwrap().wire,
                fs::read(f.config.root.join("wire.json")).unwrap()
            );
        }
        if mode != "sealed" {
            expected_marker = if matches!(mode, "unjudged" | "silent") {
                save_points::Marker::Forced { run: RUN.into() }
            } else {
                save_points::Marker::Closed { run: RUN.into() }
            };
            assert!(!directory.join(crate::containment::RECORD).exists());
        }
        assert!(!start::run_lock_held(&directory).unwrap());
    } else {
        let failure = result.as_ref().unwrap_err();
        if mode == "foreign-marker" {
            assert_eq!(
                failure.refusal(),
                CaptureRefusal::Lifecycle(LifecycleRefusal::AgentRunning)
            );
            expected_marker = save_points::Marker::Open {
                run: "unrelated".into(),
            };
            assert!(
                children.worker.try_wait().unwrap().is_none(),
                "foreign run was ended"
            );
        } else if matches!(
            mode,
            "guard" | "wrong-capture" | "wrong-hash" | "wrong-cause"
        ) {
            assert!(
                matches!(
                    failure.refusal(),
                    CaptureRefusal::Lifecycle(LifecycleRefusal::StatePersistenceInvalid { .. })
                ),
                "{failure:?}"
            );
        } else if matches!(mode, "eof" | "failed-run") {
            assert_eq!(
                failure.refusal(),
                CaptureRefusal::Lifecycle(LifecycleRefusal::Unanswered)
            );
            assert!(
                !crate::surface::render_serving_refusal(&failure.refusal())
                    .contains("worker-supplied")
            );
        } else if mode == "refused" {
            assert_eq!(
                failure.refusal(),
                CaptureRefusal::Lifecycle(LifecycleRefusal::DescriptorsUnusable)
            );
        } else {
            assert_eq!(
                failure.refusal(),
                CaptureRefusal::Lifecycle(LifecycleRefusal::Malformed)
            );
        }
        if mode != "eof" {
            assert_eq!(
                failure
                    .joined
                    .as_ref()
                    .unwrap()
                    .received
                    .as_ref()
                    .unwrap()
                    .wire,
                fs::read(f.config.root.join("wire.json")).unwrap()
            );
        }
    }
    assert_eq!(
        marker,
        Some(expected_marker),
        "{mode}: marker changed incorrectly"
    );
    if mode == "unjudged" {
        assert!(!f.config.territory.join("admin.log").exists());
    }
    if let Some(destination) = std::env::var_os("WEAVER_LEAVE_JOIN_EVIDENCE") {
        let destination = PathBuf::from(destination).join(format!("{mode}-{verb}"));
        fs::create_dir_all(&destination).unwrap();
        fs::write(
            destination.join("result.json"),
            serde_json::to_vec_pretty(&json!({
            "mode":mode,"verb":verb,"result":format!("{result:?}"),"marker":format!("{marker:?}"),
            "publication_before":saved_before,"publication_after":super::recovery::files(&saves),
            "room_before":room_before,"room_after":super::recovery::files(&room),
            "run_lock_held":start::run_lock_held(&directory).unwrap(),
            "cgroup_record_retained":directory.join(crate::containment::RECORD).exists(),
            "worker_log":log }))
            .unwrap(),
        )
        .unwrap();
    }
}

#[test]
#[ignore = "root-owned delegated scope; actual join invocation and private children"]
fn forced_leave_join_concludes_behind_the_holder_without_a_second_publisher() {
    for mode in ["closed", "open", "late", "unjudged", "retry", "sealed"] {
        scenario(mode, "unload");
    }
}

#[test]
#[ignore = "root-owned delegated scope; actual join invocation and private children"]
fn forced_leave_join_retains_guards_and_rejects_foreign_terminal_evidence() {
    for mode in [
        "guard",
        "foreign-exchange",
        "foreign-run",
        "beginning",
        "wrong-capture",
        "wrong-hash",
        "wrong-cause",
        "late-marker-foreign",
    ] {
        scenario(mode, "unload");
    }
}

#[test]
#[ignore = "root-owned delegated scope; actual join invocation and private children"]
fn forced_leave_join_refuses_every_other_invocation_holder() {
    for verb in ["load", "force-unload", "save-point", "restore", "stop"] {
        scenario("other-holder", verb);
    }
    scenario("retry-holder", "unload");
}

#[test]
#[ignore = "root-owned delegated scope; actual join invocation and private children"]
fn forced_leave_join_distinguishes_silence_worker_loss_and_another_run() {
    for mode in ["silent", "eof", "foreign-marker", "refused", "failed-run"] {
        scenario(mode, "unload");
    }
}
