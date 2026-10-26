//! ADMIN-RECOVERY through the same sole invocation as loaded forced capture.
//! These root fixtures own every child and cgroup and never use a deployed run.
use super::*;
use std::collections::BTreeMap;
use std::os::unix::fs::{MetadataExt, symlink};
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Case {
    Down,
    RoomCandidate,
    Orphan,
    MemberOnly,
    UnjudgedDown,
    UnjudgedMember,
    BadRecord,
}

const LEFTOVER: &str = r#"#!/usr/bin/python3
import errno,fcntl,os,signal,sys,time
signal.alarm(15)
def term(sig,frame):
    with open(sys.argv[4],'r+') as lock:
        try: fcntl.lockf(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
        except OSError as e: assert e.errno in (errno.EAGAIN,errno.EACCES)
        else: raise AssertionError('force escaped its invocation lock')
    with open(sys.argv[3],'w') as out: out.write('locked\n')
signal.signal(signal.SIGTERM,term)
if sys.argv[1]=='unlocked': os.close(9)
open(sys.argv[2],'w').close()
while True: time.sleep(1)
"#;

pub(super) type Files = BTreeMap<PathBuf, (u32, u32, u32, Vec<u8>)>;

/// Substitute only the retired publisher's account lookup, so restoring that
/// caller cannot silently pass because the fixture has no installed account.
struct PublicationAccount(Option<(u32, save_points::Owner)>);
impl Drop for PublicationAccount {
    fn drop(&mut self) {
        crate::TEST_PUBLICATION.with(|cell| cell.set(self.0));
    }
}

pub(super) fn files(path: &Path) -> Files {
    fn walk(base: &Path, path: &Path, result: &mut Files) {
        for entry in fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            let stat = fs::symlink_metadata(&path).unwrap();
            let bytes = if stat.file_type().is_symlink() {
                fs::read_link(&path)
                    .unwrap()
                    .as_os_str()
                    .as_encoded_bytes()
                    .to_vec()
            } else if stat.is_file() {
                fs::read(&path).unwrap()
            } else {
                vec![]
            };
            result.insert(
                path.strip_prefix(base).unwrap().into(),
                (stat.mode(), stat.uid(), stat.gid(), bytes),
            );
            if stat.is_dir() {
                walk(base, &path, result);
            }
        }
    }
    let mut result = BTreeMap::new();
    walk(path, path, &mut result);
    result
}

fn recovery(case: Case, marker: Option<save_points::Marker>) {
    assert!(
        nix::unistd::geteuid().is_root(),
        "requires a private root delegated scope"
    );
    let mut f = fixture_with_member(inventory::MemberAccount {
        uid: 65533,
        gid: 65533,
    });
    let _publication_account = PublicationAccount(
        crate::TEST_PUBLICATION.with(|cell| cell.replace(Some((65533, save_points::ROOT)))),
    );
    fs::create_dir(&f.config.coordination_root).unwrap();
    fs::set_permissions(
        &f.config.coordination_root,
        fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    let run_dir = crate::prepare_run_directory(&f.config, 0).unwrap();
    let term_seen = f.config.root.join("term-seen");
    let room = f.config.territory.join("state");
    fs::create_dir(&room).unwrap();
    fs::set_permissions(&room, fs::Permissions::from_mode(0o700)).unwrap();
    nix::unistd::chown(
        &room,
        Some(nix::unistd::Uid::from_raw(65533)),
        Some(nix::unistd::Gid::from_raw(65533)),
    )
    .unwrap();
    fs::write(room.join("unreported.sqlite"), b"unconfirmed room image").unwrap();
    // Valid under the retired format, but never a completed foundation capture.
    // A regression to room recovery would actually publish and remove this file.
    let legacy = save_points::tests::save_point(RUN, 1, 0, 1_000_000_000, b"room");
    let digest = save_points::judge(&legacy).unwrap().digest;
    let candidate = room.join(format!("{digest}.save-point"));
    fs::write(&candidate, legacy).unwrap();
    fs::set_permissions(&candidate, fs::Permissions::from_mode(0o600)).unwrap();
    nix::unistd::chown(
        &candidate,
        Some(nix::unistd::Uid::from_raw(65533)),
        Some(nix::unistd::Gid::from_raw(65533)),
    )
    .unwrap();
    fs::write(
        room.join("unfinished.sqlite.part"),
        b"unconfirmed partial image",
    )
    .unwrap();
    symlink(
        f.config.root.join("must-not-follow"),
        room.join("foreign.sqlite"),
    )
    .unwrap();
    let saves = f.config.territory.join("save-points");
    // Even an invalid index/guard belongs to a later verified load or manual
    // repair. Ending a workerless run may neither scan nor repair this state.
    if case != Case::RoomCandidate {
        fs::write(
            saves.join(save_points::MANIFEST),
            b"invalid latest index, retained\n",
        )
        .unwrap();
    }
    fs::write(
        saves.join(".capture.pending"),
        b"broken publication guard, retained\n",
    )
    .unwrap();
    fs::create_dir(saves.join("prepared.part")).unwrap();
    fs::write(
        saves.join("prepared.part/state.sqlite"),
        b"unconfirmed prepared image",
    )
    .unwrap();
    let before_room = files(&room);
    let before_saves = files(&saves);
    save_points::write_marker(&f.config.root, marker.as_ref()).unwrap();

    let has_child = matches!(
        case,
        Case::Orphan | Case::MemberOnly | Case::UnjudgedMember | Case::BadRecord
    );
    let held = matches!(case, Case::MemberOnly | Case::UnjudgedMember);
    if matches!(case, Case::UnjudgedDown | Case::UnjudgedMember) {
        f.config.territory_unjudged = true;
        f.config.territory_fd = None;
        f.config.save_points = None;
    }
    let mut children = has_child.then(|| {
        let invocation =
            start::take_invocation_lock(&run_dir, crate::verb_table::HolderVerb::Load).unwrap();
        let lock = start::take_run_lock(&run_dir).unwrap().unwrap();
        let dir = agent_cgroup::open_run_directory(&run_dir).unwrap();
        let made = agent_cgroup::make(dir.as_fd(), &f.config.agent, RUN, (0, 0)).unwrap();
        fs::write(&f.config.worker, LEFTOVER).unwrap();
        fs::set_permissions(&f.config.worker, fs::Permissions::from_mode(0o700)).unwrap();
        let log = start::open_log(&f.config.root.join("leftover.log"), None).unwrap();
        let ready = f.config.root.join("leftover-ready");
        let worker = start::spawn_worker(start::WorkerStart {
            binary: &f.config.worker,
            arguments: vec![
                if held { "held" } else { "unlocked" }.into(),
                ready.to_str().unwrap().into(),
                term_seen.to_str().unwrap().into(),
                run_dir.join("admin.lock").to_str().unwrap().into(),
            ],
            uid: 0,
            gid: 0,
            home: &f.boundary.home,
            library_path: None,
            log: &log,
            run_lock: &lock,
            relay_write: None,
            placement: made.run_procs().as_raw_fd(),
        })
        .unwrap();
        let children = Children {
            config: &f.config,
            record: made.record,
            worker,
            watcher: None,
        };
        drop(lock);
        drop(invocation);
        let end = Instant::now() + Duration::from_secs(3);
        while !ready.exists() {
            assert!(Instant::now() < end, "leftover fixture did not start");
            std::thread::sleep(Duration::from_millis(1));
        }
        children
    });
    assert_eq!(start::run_lock_held(&run_dir).unwrap(), held);
    assert!(!f.config.coordination_socket().exists());
    if case == Case::BadRecord {
        fs::write(
            run_dir.join(crate::containment::RECORD),
            b"invalid cgroup identity",
        )
        .unwrap();
    }
    let outcome = run_authorized(
        &f.config,
        &AgentName(f.config.agent.clone()),
        Cause { uid: 73 },
        UnloadBounds {
            leave: Duration::from_secs(3),
            after_left: Duration::from_millis(100),
            term: Duration::from_millis(100),
            kill: Duration::from_secs(1),
        },
        |_| panic!("workerless recovery must not look up a member or request capture"),
    );
    let outcome_text = format!("{outcome:?}");
    assert_eq!(files(&room), before_room, "{case:?}: room evidence changed");
    assert_eq!(
        files(&saves),
        before_saves,
        "{case:?}: indexed or guarded state changed"
    );
    assert_eq!(
        save_points::load_marker(&f.config.root).unwrap(),
        marker,
        "{case:?}: no Left grants no marker conclusion"
    );
    if case == Case::BadRecord {
        let failure = outcome.unwrap_err();
        assert_eq!(
            failure.refusal(),
            weaver_types::state_flow::capture::CaptureRefusal::Lifecycle(
                LifecycleRefusal::BoundaryUnverified
            )
        );
        assert!(
            children
                .as_mut()
                .unwrap()
                .worker
                .try_wait()
                .unwrap()
                .is_none(),
            "an unverified cgroup must not be killed"
        );
        assert!(run_dir.join(crate::containment::RECORD).exists());
    } else {
        let concluded = outcome.unwrap_or_else(|e| panic!("{case:?}: {e:?}"));
        assert_eq!(concluded.answer, crate::unloaded_answer());
        assert!(
            concluded.evidence.is_none(),
            "workerless recovery invents no Left"
        );
        assert!(!start::run_lock_held(&run_dir).unwrap());
        assert!(!run_dir.join(crate::containment::RECORD).exists());
        if let Some(children) = &mut children {
            assert_eq!(
                children.worker.wait().unwrap().signal(),
                Some(nix::libc::SIGKILL)
            );
            let cgroup =
                Path::new("/sys/fs/cgroup").join(children.record.path.trim_start_matches('/'));
            assert!(!cgroup.exists(), "actual cgroup removal precedes Unloaded");
        }
        if held {
            assert_eq!(
                fs::read(&term_seen).unwrap(),
                b"locked\n",
                "actual escalation precedes cgroup end under the invocation lock"
            );
        } else {
            assert!(
                !term_seen.exists(),
                "an unlocked orphan is the cgroup end's"
            );
        }
    }
    if matches!(case, Case::UnjudgedDown | Case::UnjudgedMember) {
        assert!(!f.config.territory.join("admin.log").exists());
    }
    if let Some(destination) = std::env::var_os("WEAVER_LEAVE_RECOVERY_EVIDENCE") {
        let destination = PathBuf::from(destination);
        fs::create_dir_all(&destination).unwrap();
        let marker_name = match &marker {
            None => "absent",
            Some(save_points::Marker::Open { .. }) => "open",
            Some(save_points::Marker::Closed { .. }) => "closed",
            Some(save_points::Marker::Forced { .. }) => "forced",
        };
        let result = json!({"case":format!("{case:?}"), "outcome":outcome_text,
            "prior_marker":format!("{marker:?}"),
            "final_marker":format!("{:?}",save_points::load_marker(&f.config.root).unwrap()),
            "room_before":before_room,"room_after":files(&room),
            "saves_before":before_saves,"saves_after":files(&saves),
            "record_retained":run_dir.join(crate::containment::RECORD).exists(),
            "run_lock_held":start::run_lock_held(&run_dir).unwrap(),
            "term_observed":term_seen.exists(), "member_lookup_called":false,
            "cgroup":children.as_ref().map(|c| &c.record.path)});
        fs::write(
            destination.join(format!("{case:?}-{marker_name}.json")),
            serde_json::to_vec_pretty(&result).unwrap(),
        )
        .unwrap();
    }
}

#[test]
#[ignore = "root-owned delegated cgroup scope; isolated actual recovery invocation"]
fn forced_leave_invocation_recovery_preserves_all_prior_marker_states() {
    recovery(
        Case::RoomCandidate,
        Some(save_points::Marker::Open { run: RUN.into() }),
    );
    for marker in [
        None,
        Some(save_points::Marker::Open { run: RUN.into() }),
        Some(save_points::Marker::Closed { run: RUN.into() }),
        Some(save_points::Marker::Forced { run: RUN.into() }),
    ] {
        recovery(Case::Down, marker);
    }
}

#[test]
#[ignore = "root-owned delegated cgroup scope; isolated actual recovery invocation"]
fn forced_leave_invocation_recovery_ends_unlocked_descendants_and_workerless_holders() {
    for case in [Case::Orphan, Case::MemberOnly] {
        recovery(case, Some(save_points::Marker::Open { run: RUN.into() }));
    }
}

#[test]
#[ignore = "root-owned delegated cgroup scope; isolated actual recovery invocation"]
fn forced_leave_invocation_recovery_ignores_unjudged_territory() {
    for case in [Case::UnjudgedDown, Case::UnjudgedMember] {
        recovery(case, Some(save_points::Marker::Open { run: RUN.into() }));
    }
}

#[test]
#[ignore = "root-owned delegated cgroup scope; isolated actual recovery invocation"]
fn forced_leave_invocation_recovery_refuses_unverified_containment() {
    recovery(
        Case::BadRecord,
        Some(save_points::Marker::Open { run: RUN.into() }),
    );
}
