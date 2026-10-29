//! Real root listener, entered worker and recorder; the member remains scripted.
use super::*;
use crate::flow::context::worker_startup::input::management::FoundationManagementProgress;
use crate::flow::context::worker_startup::input::management::tests::{
    Guard, dial, read_packet, trace,
};
use crate::flow::context::worker_startup::tests::Member;
use nix::sys::socket::{MsgFlags, send};
use sha2::{Digest as _, Sha256};
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use weaver_types::state_flow as s;
use weaver_types::{Cause, LifecycleDirective, LifecycleRefusal};

#[path = "context_worker_plan_cleanup_tests.rs"]
pub(in crate::flow::context::worker_startup) mod cleanup;

#[path = "context_worker_flags_tests.rs"]
pub(in crate::flow::context::worker_startup) mod flags;

#[path = "context_worker_plan_admission_tests.rs"]
mod admission;

#[path = "context_worker_plan_pressure_tests.rs"]
mod pressure;

#[path = "context_worker_plan_commit_cancel_tests.rs"]
mod commit_cancel;

pub(in crate::flow::context) fn templates() -> &'static [s::TemplateSpec] {
    static ALL: std::sync::OnceLock<Vec<s::TemplateSpec>> = std::sync::OnceLock::new();
    ALL.get_or_init(|| {
        let mut all = crate::flow::context::template_tests::templates().to_vec();
        all.extend(
            ["plan.authorize", "plan.lock", "plan.replace"]
                .into_iter()
                .map(|id| s::TemplateSpec {
                    template_id: id.into(),
                    kind: "plan.event".into(),
                    parts: vec![s::TemplatePart::Literal("Read plan.".into())],
                }),
        );
        all
    })
}

pub(in crate::flow::context) fn scheme(
    bootstrap: &mut s::Ste100Bootstrap,
    declaration: &mut s::SchemeDeclaration,
) {
    let mut artifact: serde_json::Value = serde_json::from_str(&bootstrap.scheme_text).unwrap();
    artifact["technical_names"] = serde_json::json!([
        {"text":"review","authority":{"rule_id":"test-rule","disposition":"technical_use"}}
    ]);
    bootstrap.scheme_text = serde_json::to_string(&artifact).unwrap();
    let digest = s::Digest::new(format!(
        "{:x}",
        Sha256::digest(bootstrap.scheme_text.as_bytes())
    ))
    .unwrap();
    *declaration = s::SchemeDeclaration {
        scheme_id: format!("ste100:{digest}"),
        artifact_sha256: digest,
        verbs: vec!["read".into()],
    };
}

fn document() -> String {
    let mut value: serde_json::Value =
        serde_json::from_str(&crate::flow::pump::plan::tests::document()).unwrap();
    let create = &mut value["operation"]["create"];
    create["task"] = "Read plan.".into();
    create["categories"] = serde_json::json!([{"id":"review","verbs":["read"],"acts":["statement"],"review_required":false}]);
    for step in create["steps"].as_array_mut().unwrap() {
        step["instruction"] = "Read plan.".into();
    }
    format!("\n {} \n", serde_json::to_string_pretty(&value).unwrap())
}

fn management_answer(guard: &mut Guard<'_>, peer: &OwnedFd) -> Vec<u8> {
    let end = Instant::now() + Duration::from_secs(5);
    while guard.input.as_mut().unwrap().advance() != FoundationInputProgress::ManagementPending {
        assert!(Instant::now() < end, "secondary dial was not admitted");
    }
    guard.management = Some(guard.input.take().unwrap().management().ok().unwrap());
    loop {
        let owner = guard.management.as_mut().unwrap();
        let progress = owner.advance();
        assert_ne!(
            progress,
            FoundationManagementProgress::Stopped,
            "{:?}",
            owner.detail()
        );
        if progress == FoundationManagementProgress::Answered {
            break;
        }
        assert!(Instant::now() < end, "secondary answer did not finish");
    }
    let answer = read_packet(peer).unwrap();
    guard.input = Some(guard.management.take().unwrap().finish().ok().unwrap());
    answer
}

fn observe(guard: &mut Guard<'_>, socket: &Path) -> s::enter::ObservationContent {
    let peer = dial(socket);
    let message = OrganEnvelope {
        exchange: ExchangeId {
            opener: Opener::Admin,
            ordinal: 23,
        },
        position: Position::Open,
        payload: Payload::Directive(LifecycleDirective::Observe),
    };
    let raw = serde_json::to_vec(&message).unwrap();
    assert_eq!(
        send(peer.as_raw_fd(), &raw, MsgFlags::MSG_NOSIGNAL).unwrap(),
        raw.len()
    );
    let answer: s::enter::ObservationEnvelope =
        serde_json::from_slice(&management_answer(guard, &peer)).unwrap();
    assert_eq!(answer.exchange, message.exchange);
    assert_eq!(answer.position, Position::Close);
    answer.payload
}

fn sqlite_update<'a>(
    serving: FoundationServing<'a>,
    member: &mut Member,
    reader: &mut UnixStream,
    socket: &Path,
    lost: bool,
) -> Guard<'a> {
    let mut guard = Guard {
        input: Some(serving.input()),
        management: None,
    };
    let document = document();
    let digest = s::Digest::new(format!("{:x}", Sha256::digest(document.as_bytes()))).unwrap();
    let peer = request(socket, &document);
    accepted(&mut guard);
    // The member has not answered the accepted operation. Observe must not
    // expose the prior cache as a completed update or claim to be at rest.
    assert_eq!(
        observe(&mut guard, socket),
        s::enter::ObservationContent::Answer(s::enter::ObservationAnswer::InTransition {
            constituents: vec![]
        })
    );
    let competitor = request(socket, &document);
    let answer: OrganEnvelope =
        serde_json::from_slice(&management_answer(&mut guard, &competitor)).unwrap();
    assert_eq!(
        answer.payload,
        Payload::Refusal(LifecycleRefusal::OutOfOrder)
    );
    let peer = if lost {
        drop(peer);
        None
    } else {
        Some(peer)
    };
    let done = AtomicBool::new(false);
    std::thread::scope(|scope| {
        let responder = scope.spawn(|| {
            let end = Instant::now() + Duration::from_secs(15);
            while !done.load(Ordering::SeqCst) && Instant::now() < end {
                member.step_live("ready");
                std::thread::sleep(Duration::from_millis(1));
            }
        });
        let end = Instant::now() + Duration::from_secs(5);
        while guard.input.as_ref().unwrap().plan_pending() {
            let input = guard.input.as_mut().unwrap();
            let progress = input.advance();
            assert_ne!(
                progress,
                FoundationInputProgress::Stopped,
                "{:?}",
                input.detail()
            );
            assert!(Instant::now() < end, "normal member update did not settle");
        }
        done.store(true, Ordering::SeqCst);
        responder.join().unwrap();
    });
    let input = guard.input.as_mut().unwrap();
    let completion = input
        .serving
        .plan
        .as_ref()
        .unwrap()
        .completion
        .as_ref()
        .unwrap();
    let RootPlanCompletion::Updated(updated) = completion else {
        panic!("normal member update must succeed");
    };
    assert_eq!(updated.account().packet().input().2, document);
    let summary = updated
        .account()
        .work()
        .summary()
        .unwrap()
        .summary()
        .unwrap()
        .clone();
    assert_eq!(summary.document_sha256, digest);
    assert_eq!(summary.status, s::PlanStatus::Locked);
    assert_eq!(summary.last_source.turn, None);
    assert_eq!(
        summary.last_source.block_id.sequence.0,
        updated
            .account()
            .work()
            .effect_filing()
            .unwrap()
            .sequence()
            .0
    );
    if let Some(peer) = peer {
        let answer: OrganEnvelope = serde_json::from_slice(&read_packet(&peer).unwrap()).unwrap();
        assert_eq!(answer.exchange.ordinal, 2);
        assert_eq!(answer.position, Position::Close);
        assert_eq!(answer.payload, Payload::Answer(updated.answer()));
        assert!(read_packet(&peer).is_none(), "exactly one root answer");
    }
    assert!(input.serving.owner.context.root_plan_work().is_none());
    assert_eq!(
        input
            .serving
            .owner
            .context
            .committed_plan_summary()
            .unwrap(),
        Some(&summary)
    );
    assert!(
        input
            .serving
            .owner
            .context
            .deadlines
            .get(PumpDeadline::Decision)
            .is_none()
    );
    assert!(
        input
            .serving
            .owner
            .context
            .deadlines
            .get(PumpDeadline::PlanAnswer)
            .is_none()
    );
    let observed = observe(&mut guard, socket);
    let s::enter::ObservationContent::Answer(s::enter::ObservationAnswer::State { plan, .. }) =
        observed
    else {
        panic!("completed update must be observable: {observed:?}");
    };
    assert_eq!(plan, Some(summary));
    let rows = trace(reader);
    let plan_rows: Vec<_> = rows
        .iter()
        .filter(|row| row["kind"] == "plan.event")
        .collect();
    assert_eq!(
        plan_rows.len(),
        2,
        "one input and one effect, never a replay"
    );
    assert_eq!(plan_rows[0]["origin"], "operator");
    assert_eq!(plan_rows[1]["origin"], "loop");
    assert!(rows.iter().all(|row| row["kind"] != "turn.started"));
    guard
}

fn admin_update(
    serving: FoundationServing<'_>,
    member: &mut Member,
    reader: &mut UnixStream,
    socket: &Path,
) {
    let root = socket.parent().unwrap();
    let run = root.join("coord/weaver.run/plan-fixture");
    std::fs::create_dir_all(&run).unwrap();
    std::fs::create_dir(root.join("admin")).unwrap();
    std::fs::write(root.join("admin/plan-update.json"), document()).unwrap();
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(run.join("run.lock"))
        .unwrap();
    let request = nix::libc::flock {
        l_type: nix::libc::F_WRLCK as i16,
        l_whence: nix::libc::SEEK_SET as i16,
        l_start: 0,
        l_len: 0,
        l_pid: 0,
    };
    // SAFETY: this fixture retains the live file and passes its valid lock layout.
    assert_eq!(
        unsafe { nix::libc::fcntl(lock.as_raw_fd(), nix::libc::F_OFD_SETLK, &request) },
        0
    );
    struct Child(std::process::Child);
    impl Drop for Child {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let mut child = Child(
        std::process::Command::new(
            std::env::var("WEAVER_ROOT_PLAN_ADMIN_TEST_BIN")
                .expect("independently built admin test binary"),
        )
        .args([
            "--exact",
            "serving_plan::tests::root_plan_actual_admin_process",
            "--nocapture",
        ])
        .env("WEAVER_ROOT_PLAN_CIRCUIT", root)
        .env_remove("SUDO_UID")
        .spawn()
        .unwrap(),
    );
    let mut guard = Guard {
        input: Some(serving.input()),
        management: None,
    };
    let done = AtomicBool::new(false);
    std::thread::scope(|scope| {
        let responder = scope.spawn(|| {
            let end = Instant::now() + Duration::from_secs(15);
            while !done.load(Ordering::SeqCst) && Instant::now() < end {
                member.step_live("ready");
                std::thread::sleep(Duration::from_millis(1));
            }
        });
        let end = Instant::now() + Duration::from_secs(10);
        let mut observations = 0;
        let mut updates = 0;
        while observations < 2 {
            assert!(
                Instant::now() < end,
                "actual admin did not finish: {:?}",
                child.0.try_wait().unwrap()
            );
            if let Some(owner) = &mut guard.management {
                match owner.advance() {
                    FoundationManagementProgress::DirectivePending => {
                        updates += 1;
                        assert_eq!(updates, 1, "no replay after accepted root update");
                        guard.input = Some(
                            guard
                                .management
                                .take()
                                .unwrap()
                                .queue_plan_update()
                                .ok()
                                .unwrap(),
                        );
                    }
                    FoundationManagementProgress::Answered => {
                        observations += 1;
                        guard.input = Some(guard.management.take().unwrap().finish().ok().unwrap());
                    }
                    FoundationManagementProgress::Stopped
                    | FoundationManagementProgress::Dropped => {
                        panic!("actual admin exchange: {:?}", owner.detail())
                    }
                    FoundationManagementProgress::Pending => (),
                }
            } else {
                let input = guard.input.as_mut().unwrap();
                match input.advance() {
                    FoundationInputProgress::ManagementPending => {
                        guard.management =
                            Some(guard.input.take().unwrap().management().ok().unwrap())
                    }
                    FoundationInputProgress::Stopped => {
                        panic!("actual admin root update: {:?}", input.detail())
                    }
                    _ => (),
                }
            }
        }
        assert_eq!(updates, 1);
        done.store(true, Ordering::SeqCst);
        responder.join().unwrap();
    });
    let end = Instant::now() + Duration::from_secs(5);
    let status = loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            break status;
        }
        assert!(Instant::now() < end, "actual admin child did not exit");
        std::thread::sleep(Duration::from_millis(1));
    };
    assert!(status.success());
    let answer: weaver_types::LifecycleAnswer =
        serde_json::from_slice(&std::fs::read(root.join("admin-plan-answer.json")).unwrap())
            .unwrap();
    let Some(RootPlanCompletion::Updated(updated)) = &guard
        .input
        .as_ref()
        .unwrap()
        .serving
        .plan
        .as_ref()
        .unwrap()
        .completion
    else {
        panic!("actual admin update needs committed custody");
    };
    assert_eq!(answer, updated.answer());
    assert_eq!(updated.account().packet().input().2, document());
    assert_eq!(
        trace(reader)
            .iter()
            .filter(|row| row["kind"] == "plan.event")
            .count(),
        2
    );
}

fn thread_cpu_ns() -> u64 {
    let mut time = nix::libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: the clock initializes this valid stack timespec on success.
    assert_eq!(
        unsafe { nix::libc::clock_gettime(nix::libc::CLOCK_THREAD_CPUTIME_ID, &mut time) },
        0
    );
    u64::try_from(time.tv_sec).unwrap() * 1_000_000_000 + u64::try_from(time.tv_nsec).unwrap()
}

pub(in crate::flow::context::worker_startup) fn request(socket: &Path, document: &str) -> OwnedFd {
    let peer = dial(socket);
    let envelope = OrganEnvelope {
        exchange: ExchangeId {
            opener: Opener::Admin,
            ordinal: 2,
        },
        position: Position::Open,
        payload: Payload::Directive(LifecycleDirective::PlanUpdate {
            cause: Cause { uid: 0 },
            document_sha256: s::Digest::new(format!("{:x}", Sha256::digest(document.as_bytes())))
                .unwrap(),
            document: document.into(),
        }),
    };
    let raw = serde_json::to_vec(&envelope).unwrap();
    assert_eq!(
        send(peer.as_raw_fd(), &raw, MsgFlags::MSG_NOSIGNAL).unwrap(),
        raw.len()
    );
    peer
}

pub(in crate::flow::context::worker_startup) fn accepted(guard: &mut Guard<'_>) {
    let end = Instant::now() + Duration::from_secs(5);
    while guard.input.as_mut().unwrap().advance() != FoundationInputProgress::ManagementPending {
        assert!(Instant::now() < end);
    }
    guard.management = Some(guard.input.take().unwrap().management().ok().unwrap());
    while guard.management.as_mut().unwrap().advance() == FoundationManagementProgress::Pending {
        assert!(Instant::now() < end);
    }
    assert_eq!(
        guard.management.as_ref().unwrap().progress(),
        FoundationManagementProgress::DirectivePending
    );
    match guard.management.take().unwrap().queue_plan_update() {
        Ok(input) => guard.input = Some(input),
        Err(owner) => {
            let detail = owner.detail().map(str::to_owned);
            guard.management = Some(owner);
            panic!("root handoff: {detail:?}");
        }
    }
    assert!(guard.input.as_ref().unwrap().plan_pending());
    assert!(
        guard
            .input
            .as_ref()
            .unwrap()
            .serving
            .owner
            .context
            .root_plan_work()
            .is_some()
    );
}

fn stopped(
    serving: FoundationServing<'_>,
    member: &mut Member,
    reader: &mut UnixStream,
    socket: &Path,
) {
    let mut guard = Guard {
        input: Some(serving.input()),
        management: None,
    };
    let original = request(socket, &crate::flow::pump::plan::tests::document());
    accepted(&mut guard);
    // The real first read crosses the socket before its member is permitted
    // to answer. Stop then arrives on a separate authenticated connection.
    assert_eq!(
        guard.input.as_mut().unwrap().advance(),
        FoundationInputProgress::Pending
    );
    assert!(
        guard
            .input
            .as_ref()
            .unwrap()
            .serving
            .owner
            .context
            .read
            .is_some()
    );
    let stop = dial(socket);
    let request = OrganEnvelope {
        exchange: ExchangeId {
            opener: Opener::Admin,
            ordinal: 9,
        },
        position: Position::Open,
        payload: Payload::Directive(LifecycleDirective::Stop {
            cause: Cause { uid: 0 },
        }),
    };
    let raw = serde_json::to_vec(&request).unwrap();
    assert_eq!(
        send(stop.as_raw_fd(), &raw, MsgFlags::MSG_NOSIGNAL).unwrap(),
        raw.len()
    );
    let end = Instant::now() + Duration::from_secs(5);
    while guard.input.as_mut().unwrap().advance() != FoundationInputProgress::ManagementPending {
        assert!(Instant::now() < end);
    }
    guard.management = Some(guard.input.take().unwrap().management().ok().unwrap());
    let done = AtomicBool::new(false);
    std::thread::scope(|scope| {
        let responder = scope.spawn(|| {
            let until = Instant::now() + Duration::from_secs(10);
            while !done.load(Ordering::SeqCst) && Instant::now() < until {
                member.step_live("ready");
                std::thread::sleep(Duration::from_millis(1));
            }
        });
        loop {
            let owner = guard.management.as_mut().unwrap();
            let progress = owner.advance();
            assert_ne!(
                progress,
                FoundationManagementProgress::Stopped,
                "{:?}",
                owner.detail()
            );
            if progress == FoundationManagementProgress::Answered {
                break;
            }
            assert!(Instant::now() < end, "Stop did not settle root work");
        }
        done.store(true, Ordering::SeqCst);
        responder.join().unwrap();
    });
    let answer: OrganEnvelope = serde_json::from_slice(&read_packet(&stop).unwrap()).unwrap();
    assert_eq!(answer.exchange, request.exchange);
    assert_eq!(
        answer.payload,
        Payload::Answer(weaver_types::LifecycleAnswer::AtRest)
    );
    guard.input = Some(guard.management.take().unwrap().finish().ok().unwrap());
    let input = guard.input.as_ref().unwrap();
    let plan = input.serving.plan.as_ref().unwrap();
    assert!(!plan.active());
    let Some(RootPlanCompletion::Cancelled(account)) = &plan.completion else {
        panic!("Stop needs cancelled root custody");
    };
    assert_eq!(
        account.packet().input().2,
        crate::flow::pump::plan::tests::document()
    );
    assert!(account.work().input_filing().is_none());
    assert!(account.work().effect().is_none());
    assert!(input.serving.owner.context.read.is_none());
    assert!(input.serving.owner.context.root_plan_work().is_none());
    assert!(
        input
            .serving
            .owner
            .context
            .deadlines
            .get(PumpDeadline::Decision)
            .is_none()
    );
    assert!(
        input
            .serving
            .owner
            .context
            .deadlines
            .get(PumpDeadline::StateControl)
            .is_none()
    );
    assert!(read_packet(&original).is_none());
    let rows = trace(reader);
    assert!(!rows.iter().any(|row| row["kind"] == "plan.event"));
    assert!(
        member
            .requests
            .iter()
            .filter(|name| *name == "release")
            .count()
            >= 5
    );
}

pub(in crate::flow::context::worker_startup) fn exercise(
    serving: FoundationServing<'_>,
    member: &mut Member,
    reader: &mut UnixStream,
    socket: &Path,
    mode: &str,
) {
    if mode.ends_with("-open-turn") {
        return admission::open_turn(serving, member, reader, socket);
    }
    if mode.ends_with("-answer-pressure") || mode.ends_with("-answer-expiry") {
        return pressure::exercise(serving, reader, socket, mode.ends_with("-answer-expiry"));
    }
    if mode.ends_with("-save-sqlite-admin") {
        return admin_update(serving, member, reader, socket);
    }
    if mode.ends_with("-save-sqlite-cancel-input") || mode.ends_with("-save-sqlite-cancel-effect") {
        return commit_cancel::exercise(
            serving,
            member,
            reader,
            socket,
            mode.ends_with("-cancel-effect"),
        );
    }
    if mode.contains("-save-sqlite-") {
        let _guard = sqlite_update(
            serving,
            member,
            reader,
            socket,
            mode.ends_with("lost-answer"),
        );
        return;
    }
    if mode.ends_with("stop") {
        return stopped(serving, member, reader, socket);
    }
    let mut guard = Guard {
        input: Some(serving.input()),
        management: None,
    };
    let cpu_before = thread_cpu_ns();
    let done = AtomicBool::new(false);
    std::thread::scope(|scope| {
        let responder = scope.spawn(|| {
            if mode.ends_with("read-refused") {
                std::thread::sleep(Duration::from_millis(200));
            }
            let until = Instant::now() + Duration::from_secs(15);
            while !done.load(Ordering::SeqCst) && Instant::now() < until {
                member.step_live(if mode.ends_with("read-refused") {
                    "summary-refused"
                } else {
                    "ready"
                });
                std::thread::sleep(Duration::from_millis(1));
            }
        });
        let document = if mode.ends_with("read-refused") {
            crate::flow::pump::plan::tests::document()
        } else {
            "{ malformed root document".into()
        };
        for _ in 0..2 {
            let peer = request(socket, &document);
            accepted(&mut guard);
            let peer = if mode.ends_with("disconnected") {
                drop(peer);
                None
            } else {
                Some(peer)
            };
            let end = Instant::now() + Duration::from_secs(5);
            while guard.input.as_ref().unwrap().plan_pending() {
                let input = guard.input.as_mut().unwrap();
                let progress = input.advance();
                if mode.ends_with("read-refused") {
                    assert!(
                        thread_cpu_ns() - cpu_before < 120_000_000,
                        "root driver spun while the actual member delayed its answer"
                    );
                }
                assert_ne!(
                    progress,
                    FoundationInputProgress::Stopped,
                    "{:?}",
                    input.detail()
                );
                assert!(Instant::now() < end, "root update did not settle");
            }
            let input = guard.input.as_mut().unwrap();
            assert!(input.serving.owner.context.root_plan_work().is_none());
            assert!(
                input
                    .serving
                    .owner
                    .context
                    .deadlines
                    .get(PumpDeadline::Decision)
                    .is_none()
            );
            assert!(
                input
                    .serving
                    .owner
                    .context
                    .deadlines
                    .get(PumpDeadline::PlanAnswer)
                    .is_none()
            );
            let completed = input
                .serving
                .plan
                .as_ref()
                .unwrap()
                .completion
                .as_ref()
                .unwrap();
            let RootPlanCompletion::Refused { refusal, account } = completed else {
                panic!("typed refusal required");
            };
            assert_eq!(account.packet().input().2, document);
            assert!(account.work().refusal_sequence().is_some());
            assert!(
                input
                    .serving
                    .owner
                    .context
                    .committed_plan_summary()
                    .unwrap()
                    .is_none()
            );
            let LifecycleRefusal::PlanUpdateRefused {
                case, read_code, ..
            } = refusal
            else {
                panic!();
            };
            assert_eq!(
                *case,
                if mode.ends_with("read-refused") {
                    s::PlanUpdateCase::ReadRefused
                } else {
                    s::PlanUpdateCase::MalformedPlan
                }
            );
            assert_eq!(
                *read_code,
                mode.ends_with("read-refused")
                    .then_some(s::ReadRefusalCode::ResultTooLarge)
            );
            if let Some(peer) = &peer {
                let answer: OrganEnvelope =
                    serde_json::from_slice(&read_packet(peer).expect("root answer")).unwrap();
                assert_eq!(answer.exchange.ordinal, 2);
                assert_eq!(answer.position, Position::Close);
                assert_eq!(answer.payload, Payload::Refusal(refusal.clone()));
                assert!(read_packet(peer).is_none());
            }
        }
        done.store(true, Ordering::SeqCst);
        responder.join().unwrap();
    });
    let rows = trace(reader);
    let root_refusals: Vec<_> = rows
        .iter()
        .filter(|row| row["kind"] == "refusal")
        .filter_map(|row| {
            serde_json::from_value::<weaver_types::RefusalRecord>(row["payload"].clone()).ok()
        })
        .filter(|record| {
            matches!(
                record,
                weaver_types::RefusalRecord::Lifecycle {
                    asked: weaver_types::LifecycleAsk::PlanUpdate,
                    ..
                }
            )
        })
        .collect();
    assert_eq!(root_refusals.len(), 2, "{root_refusals:?}");
    if mode.ends_with("read-refused") {
        assert!(
            thread_cpu_ns() - cpu_before < 120_000_000,
            "root driver spun while the actual member delayed its answer"
        );
    }
    assert!(!rows.iter().any(|row| row["kind"] == "plan.event"));
}
