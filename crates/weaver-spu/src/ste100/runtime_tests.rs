//! Actual runtime/socket ownership with sealed test-only work. No language claim.
use super::*;
use crate::ste100::admission::{AdapterKind, SealedAdapter, VerifiedArtifacts, VerifiedFiles};
use crate::ste100::channel::{Message, Receive, SendProgress, Tag};
use nix::sys::socket::{
    AddressFamily, MsgFlags, SockFlag, SockType, send as send_packet, socketpair,
};
use std::rc::Rc;
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};
use weaver_types::state_flow::{
    Binding, DecimalU64, NounBinding, Span, Ste100Bootstrap, Ste100DefectCode, Ste100Ready,
    Ste100Reply, Ste100Target, decode_strict,
};

#[path = "../../tests/common/ste100_fixture.rs"]
mod fixture;

#[derive(Debug)]
enum Event {
    Admission {
        bootstrap: Ste100Bootstrap,
        thread: thread::ThreadId,
        files: Vec<Weak<Vec<u8>>>,
    },
    Check {
        request: Ste100Request,
        thread: thread::ThreadId,
        files_alive: bool,
    },
    Dropped {
        thread: thread::ThreadId,
        files_alive: bool,
    },
}
enum AdmissionStep {
    Proceed,
    Refuse,
    Panic,
}
enum CheckStep {
    Result(Ste100Result),
    Panic,
}
struct Adapter {
    admission: Mutex<Receiver<AdmissionStep>>,
    checks: Mutex<Option<Receiver<CheckStep>>>,
    events: Sender<Event>,
}
impl SealedAdapter for Adapter {}
impl AdapterAdmission for Adapter {
    type Backend = Backend;
    fn kind(&self) -> AdapterKind {
        AdapterKind::TestOnly
    }
    fn admit(
        &self,
        artifacts: &VerifiedArtifacts,
        files: &VerifiedFiles,
    ) -> Result<Backend, Ste100AdmissionRefused> {
        let files = fixture::INPUTS
            .iter()
            .map(|(name, bytes)| {
                let held = files
                    .retain(name)
                    .expect("declared bytes acquired before adapter");
                assert_eq!(held.as_slice(), *bytes);
                Arc::downgrade(&held)
            })
            .collect::<Vec<_>>();
        self.events
            .send(Event::Admission {
                bootstrap: artifacts.bootstrap().clone(),
                thread: thread::current().id(),
                files: files.clone(),
            })
            .unwrap();
        match self.admission.lock().unwrap().recv() {
            Ok(AdmissionStep::Proceed) => Ok(Backend {
                files,
                checks: self.checks.lock().unwrap().take().unwrap(),
                events: self.events.clone(),
                _not_send: Rc::new(()),
            }),
            Ok(AdmissionStep::Refuse) | Err(_) => Err(Ste100AdmissionRefused {
                protocol: weaver_types::state_flow::STE100_PROTOCOL.into(),
                code: Ste100AdmissionCode::UnsupportedArtifact,
                detail: "test-only adapter refused the acquired candidate".into(),
            }),
            Ok(AdmissionStep::Panic) => panic!("test-only admission panic"),
        }
    }
}
struct Backend {
    files: Vec<Weak<Vec<u8>>>,
    checks: Receiver<CheckStep>,
    events: Sender<Event>,
    // A future device adapter may be thread-affine. The runtime must construct,
    // use and destroy it on its own worker rather than demand Backend: Send.
    _not_send: Rc<()>,
}
impl Backend {
    fn inputs_alive(&self) -> bool {
        self.files
            .iter()
            .zip(fixture::INPUTS)
            .all(|(file, (_, expected))| {
                file.upgrade()
                    .is_some_and(|bytes| bytes.as_slice() == expected)
            })
    }
}
impl CheckBackend for Backend {
    fn check(&mut self, request: &Ste100Request) -> Ste100Result {
        self.events
            .send(Event::Check {
                request: request.clone(),
                thread: thread::current().id(),
                files_alive: self.inputs_alive(),
            })
            .unwrap();
        match self.checks.recv() {
            Ok(CheckStep::Result(result)) => {
                assert!(
                    self.inputs_alive(),
                    "deferred last read lost checked input ownership"
                );
                result
            }
            Ok(CheckStep::Panic) => panic!("test-only checker panic"),
            Err(_) => component(Ste100ComponentCode::NoResult, "test controller retired"),
        }
    }
}
impl Drop for Backend {
    fn drop(&mut self) {
        let _ = self.events.send(Event::Dropped {
            thread: thread::current().id(),
            files_alive: self.inputs_alive(),
        });
    }
}
struct Control {
    admission: Option<Sender<AdmissionStep>>,
    checks: Option<Sender<CheckStep>>,
    events: Receiver<Event>,
}
impl Control {
    fn new() -> (Self, Adapter) {
        let (admission, admission_rx) = channel();
        let (checks, checks_rx) = channel();
        let (events_tx, events) = channel();
        (
            Self {
                admission: Some(admission),
                checks: Some(checks),
                events,
            },
            Adapter {
                admission: Mutex::new(admission_rx),
                checks: Mutex::new(Some(checks_rx)),
                events: events_tx,
            },
        )
    }
    fn event(&self) -> Event {
        self.events
            .recv_timeout(Duration::from_secs(30))
            .expect("actual worker checkpoint")
    }
    fn admit(&self, step: AdmissionStep) {
        self.admission.as_ref().unwrap().send(step).unwrap();
    }
    fn result(&self, result: Ste100Result) {
        self.checks
            .as_ref()
            .unwrap()
            .send(CheckStep::Result(result))
            .unwrap();
    }
}
impl Drop for Control {
    fn drop(&mut self) {
        self.admission.take();
        self.checks.take();
    }
}
fn component(code: Ste100ComponentCode, detail: &str) -> Ste100Result {
    Ste100Result::ComponentFailed {
        code,
        detail: detail.into(),
    }
}
fn span(start: u64, end: u64) -> Span {
    Span {
        start: DecimalU64(start),
        end: DecimalU64(end),
    }
}
fn accepted() -> Ste100Result {
    Ste100Result::Accepted {
        bindings: vec![Binding {
            statement_span: span(0, 10),
            verb: "read".into(),
            verb_span: span(0, 4),
            condition: None,
            condition_span: None,
            nouns: vec![NounBinding {
                value: "item".into(),
                span: span(5, 9),
            }],
        }],
    }
}
fn request(bootstrap: &Ste100Bootstrap, id: u64) -> Ste100Request {
    Ste100Request {
        request_id: DecimalU64(id),
        target: Ste100Target::Input {
            input_id: DecimalU64(77),
            block: DecimalU64(3),
        },
        text: "Read item.".into(),
        scheme_id: format!("ste100:{}", fixture::sha(bootstrap.scheme_text.as_bytes())),
    }
}
fn pair(limit: usize) -> (Channel, Channel) {
    let (left, right) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .unwrap();
    (
        Channel::from_owned(left, limit).unwrap(),
        Channel::from_owned(right, limit).unwrap(),
    )
}
fn send(peer: &mut Channel, tag: Tag, id: u64, bytes: Vec<u8>) {
    peer.begin_send(tag, id, bytes).unwrap();
    while peer.has_pending_send() {
        assert_ne!(peer.try_send().unwrap(), SendProgress::Pending);
    }
}
fn fragment(peer: &Channel, tag: Tag, id: u64) {
    let mut bytes = vec![1, tag as u8];
    bytes.extend_from_slice(&id.to_be_bytes());
    bytes.extend_from_slice(&10_u64.to_be_bytes());
    bytes.extend_from_slice(&0_u64.to_be_bytes());
    bytes.push(b'{');
    assert_eq!(
        send_packet(peer.readiness().as_raw_fd(), &bytes, MsgFlags::MSG_NOSIGNAL).unwrap(),
        bytes.len()
    );
}
fn message(runtime: &mut Runtime<Adapter>, peer: &mut Channel) -> Message {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        runtime.advance().unwrap();
        match peer.try_receive().unwrap() {
            Receive::Complete(message) => return message,
            Receive::Pending | Receive::Fragment => {}
        }
        assert!(
            Instant::now() < deadline,
            "whole runtime response stalled at {:?}",
            runtime.position()
        );
        thread::yield_now();
    }
}
type ReadyRuntime = (
    fixture::Fixture,
    Channel,
    Runtime<Adapter>,
    Control,
    thread::ThreadId,
    Vec<Weak<Vec<u8>>>,
);
fn ready() -> ReadyRuntime {
    ready_with_limit(fixture::LIMIT)
}
fn ready_with_limit(limit: usize) -> ReadyRuntime {
    let mut fixture = fixture::Fixture::new(true);
    fixture.bootstrap.instruction.max_wire_bytes = DecimalU64(limit as u64);
    let (mut peer, channel) = pair(limit);
    let (control, adapter) = Control::new();
    let mut runtime = Runtime::new_fixture(channel, adapter);
    send(
        &mut peer,
        Tag::Bootstrap,
        0,
        serde_json::to_vec(&fixture.bootstrap).unwrap(),
    );
    runtime.advance().unwrap();
    let Event::Admission {
        bootstrap,
        thread,
        files,
    } = control.event()
    else {
        panic!("expected actual admission");
    };
    assert_eq!(bootstrap, fixture.bootstrap);
    assert_ne!(thread, thread::current().id());
    assert_eq!(runtime.position(), Position::Admitting);
    assert_eq!(runtime.advance().unwrap(), RuntimeProgress::Pending);
    assert_eq!(peer.try_receive().unwrap(), Receive::Pending);
    control.admit(AdmissionStep::Proceed);
    let message = message(&mut runtime, &mut peer);
    assert_eq!(message.tag, Tag::Ready);
    assert_eq!(message.id, 0);
    let ready: Ste100Ready = decode_strict(std::str::from_utf8(&message.payload).unwrap()).unwrap();
    assert_eq!(ready.scheme_id, request(&fixture.bootstrap, 1).scheme_id);
    assert_eq!(ready.instances, fixture.bootstrap.instruction.instances);
    assert_eq!(
        ready.checker_artifact_sha256,
        fixture.bootstrap.instruction.model_artifact_sha256
    );
    assert_eq!(
        ready.max_wire_bytes,
        fixture.bootstrap.instruction.max_wire_bytes
    );
    (fixture, peer, runtime, control, thread, files)
}
fn submit(
    runtime: &mut Runtime<Adapter>,
    peer: &mut Channel,
    control: &Control,
    request: &Ste100Request,
    owner: thread::ThreadId,
) {
    send(
        peer,
        Tag::Request,
        request.request_id.0,
        serde_json::to_vec(request).unwrap(),
    );
    runtime.advance().unwrap();
    let Event::Check {
        request: actual,
        thread,
        files_alive,
    } = control.event()
    else {
        panic!("actual whole-block job expected");
    };
    assert_eq!(&actual, request);
    assert_eq!(thread, owner);
    assert!(files_alive);
    assert_eq!(runtime.position(), Position::Checking);
    assert_eq!(runtime.advance().unwrap(), RuntimeProgress::Pending);
    assert_eq!(peer.try_receive().unwrap(), Receive::Pending);
}
fn reply(
    runtime: &mut Runtime<Adapter>,
    peer: &mut Channel,
    request: &Ste100Request,
) -> Ste100Reply {
    let message = message(runtime, peer);
    assert_eq!(message.tag, Tag::Reply);
    assert_eq!(message.id, request.request_id.0);
    let reply: Ste100Reply = decode_strict(std::str::from_utf8(&message.payload).unwrap()).unwrap();
    assert_eq!(reply.request_id, request.request_id);
    assert_eq!(reply.target, request.target);
    assert_eq!(reply.scheme_id, request.scheme_id);
    reply
}
fn wait_worker_ready(runtime: &Runtime<Adapter>) {
    let worker = runtime.worker.as_ref().expect("actual worker is owned");
    let mut fd = libc::pollfd {
        fd: worker.wake.as_raw_fd(),
        events: libc::POLLIN,
        revents: 0,
    };
    assert!(
        unsafe { libc::poll(&mut fd, 1, 30_000) } > 0,
        "actual completion wake never arrived"
    );
    assert_ne!(fd.revents & libc::POLLIN, 0);
}
fn assert_dropped(control: &Control, owner: thread::ThreadId, files: &[Weak<Vec<u8>>]) {
    let Event::Dropped {
        thread,
        files_alive,
    } = control.event()
    else {
        panic!("actual backend destructor expected");
    };
    assert_eq!(thread, owner);
    assert!(files_alive, "all checked bytes outlive backend destructor");
    let deadline = Instant::now() + Duration::from_secs(30);
    while files.iter().any(|file| file.strong_count() != 0) {
        assert!(Instant::now() < deadline, "retired file ownership leaked");
        thread::yield_now();
    }
}

#[test]
fn ste100_runtime_retains_declared_inputs_through_thread_affine_check_and_destruction() {
    let (fixture, mut peer, mut runtime, control, owner, files) = ready();
    let bootstrap = fixture.bootstrap.clone();
    for (name, _) in fixture::INPUTS {
        std::fs::write(fixture.root.join(name), b"replacement unchecked bytes").unwrap();
    }
    std::fs::write(
        &bootstrap.instruction.model_binding.artifact.0,
        b"replacement manifest",
    )
    .unwrap();
    drop(fixture);
    let request = request(&bootstrap, 1);
    submit(&mut runtime, &mut peer, &control, &request, owner);
    control.result(accepted());
    assert_eq!(reply(&mut runtime, &mut peer, &request).result, accepted());
    assert_eq!(runtime.position(), Position::Idle);
    drop(peer);
    assert_eq!(
        runtime.advance().unwrap(),
        RuntimeProgress::Stopped(Stop::PeerClosed)
    );
    assert_dropped(&control, owner, &files);
}

#[test]
fn ste100_runtime_correct_defect_keeps_the_same_owner_usable_and_preserves_target_identity() {
    let (fixture, mut peer, mut runtime, control, owner, files) = ready();
    for (index, result) in [
        accepted(),
        Ste100Result::NotSte100 {
            code: Ste100DefectCode::NonSte100,
            detail: "actual test-only defect".into(),
            spans: vec![span(0, 4)],
        },
        accepted(),
    ]
    .into_iter()
    .enumerate()
    {
        let request = request(&fixture.bootstrap, 1 + index as u64);
        submit(&mut runtime, &mut peer, &control, &request, owner);
        control.result(result.clone());
        assert_eq!(reply(&mut runtime, &mut peer, &request).result, result);
        assert_eq!(runtime.position(), Position::Idle);
    }
    drop(peer);
    assert_eq!(
        runtime.advance().unwrap(),
        RuntimeProgress::Stopped(Stop::PeerClosed)
    );
    assert_dropped(&control, owner, &files);
}

#[test]
fn ste100_runtime_detects_first_overlap_fragment_while_admission_is_held_and_discards_late_ready() {
    let fixture = fixture::Fixture::new(true);
    let (mut peer, channel) = pair(fixture::LIMIT);
    let (control, adapter) = Control::new();
    let mut runtime = Runtime::new_fixture(channel, adapter);
    send(
        &mut peer,
        Tag::Bootstrap,
        0,
        serde_json::to_vec(&fixture.bootstrap).unwrap(),
    );
    runtime.advance().unwrap();
    let Event::Admission {
        thread: owner,
        files,
        ..
    } = control.event()
    else {
        panic!();
    };
    fragment(&peer, Tag::Request, 1);
    assert!(runtime.advance().is_err());
    assert_eq!(runtime.position(), Position::Failed);
    control.admit(AdmissionStep::Proceed);
    assert_dropped(&control, owner, &files);
    assert!(runtime.advance().is_err());
    assert!(
        peer.try_receive().is_err(),
        "late admission cannot send Ready"
    );
}

#[test]
fn ste100_runtime_peer_close_during_held_admission_revokes_late_ready_without_waiting_for_work() {
    let fixture = fixture::Fixture::new(true);
    let (mut peer, channel) = pair(fixture::LIMIT);
    let (control, adapter) = Control::new();
    let mut runtime = Runtime::new_fixture(channel, adapter);
    send(
        &mut peer,
        Tag::Bootstrap,
        0,
        serde_json::to_vec(&fixture.bootstrap).unwrap(),
    );
    runtime.advance().unwrap();
    let Event::Admission {
        thread: owner,
        files,
        ..
    } = control.event()
    else {
        panic!();
    };
    drop(peer);
    assert!(runtime.advance().is_err());
    assert_eq!(runtime.position(), Position::Failed);
    assert!(
        files.iter().all(|file| file.strong_count() > 0),
        "detached acquisition still owns its verified files"
    );
    control.admit(AdmissionStep::Proceed);
    assert_dropped(&control, owner, &files);
    assert!(runtime.advance().is_err());
}

#[test]
fn ste100_runtime_overlap_wins_even_when_a_complete_check_is_already_available() {
    for complete_first in [false, true] {
        let (fixture, mut peer, mut runtime, control, owner, files) = ready();
        let first = request(&fixture.bootstrap, 1);
        submit(&mut runtime, &mut peer, &control, &first, owner);
        if complete_first {
            control.result(accepted());
            wait_worker_ready(&runtime);
        }
        fragment(&peer, Tag::Request, 2);
        assert!(runtime.advance().is_err());
        assert_eq!(runtime.position(), Position::Failed);
        if !complete_first {
            control.result(accepted());
        }
        assert_dropped(&control, owner, &files);
        assert!(runtime.advance().is_err());
        assert!(
            peer.try_receive().is_err(),
            "overlap cannot receive a completed prefix"
        );
    }
}

#[test]
fn ste100_runtime_peer_close_during_check_preserves_worker_custody_until_its_last_read() {
    let (fixture, mut peer, mut runtime, control, owner, files) = ready();
    let first = request(&fixture.bootstrap, 1);
    submit(&mut runtime, &mut peer, &control, &first, owner);
    drop(fixture);
    drop(peer);
    assert!(runtime.advance().is_err());
    assert!(files.iter().all(|file| file.strong_count() > 0));
    control.result(accepted());
    assert_dropped(&control, owner, &files);
    assert!(runtime.advance().is_err());
}

#[test]
fn ste100_runtime_component_result_and_invalid_output_end_after_one_correlated_terminal() {
    for result in [
        component(Ste100ComponentCode::NoResult, "artifact_capacity"),
        Ste100Result::Accepted { bindings: vec![] },
    ] {
        let expected = if matches!(result, Ste100Result::ComponentFailed { .. }) {
            Ste100ComponentCode::NoResult
        } else {
            Ste100ComponentCode::InvalidResult
        };
        let (fixture, mut peer, mut runtime, control, owner, files) = ready();
        let first = request(&fixture.bootstrap, 1);
        submit(&mut runtime, &mut peer, &control, &first, owner);
        control.result(result);
        let response = reply(&mut runtime, &mut peer, &first);
        assert!(
            matches!(response.result,Ste100Result::ComponentFailed {code,..} if code==expected)
        );
        assert_eq!(runtime.position(), Position::Closed);
        assert_eq!(
            runtime.advance().unwrap(),
            RuntimeProgress::Stopped(Stop::ComponentFailed)
        );
        assert_dropped(&control, owner, &files);
        assert!(
            peer.try_receive().is_err(),
            "terminal component reply is unique"
        );
    }
}

#[test]
fn ste100_runtime_worker_panic_is_one_terminal_no_result_and_never_ready_after_admission_loss() {
    let fixture = fixture::Fixture::new(true);
    let (mut peer, channel) = pair(fixture::LIMIT);
    let (control, adapter) = Control::new();
    let mut runtime = Runtime::new_fixture(channel, adapter);
    send(
        &mut peer,
        Tag::Bootstrap,
        0,
        serde_json::to_vec(&fixture.bootstrap).unwrap(),
    );
    runtime.advance().unwrap();
    assert!(matches!(control.event(), Event::Admission { .. }));
    control.admit(AdmissionStep::Panic);
    let refusal = message(&mut runtime, &mut peer);
    assert_eq!(refusal.tag, Tag::AdmissionRefused);
    let refusal: Ste100AdmissionRefused =
        decode_strict(std::str::from_utf8(&refusal.payload).unwrap()).unwrap();
    assert_eq!(refusal.code, Ste100AdmissionCode::UnsupportedArtifact);
    let (fixture, mut peer, mut runtime, control, owner, files) = ready();
    let first = request(&fixture.bootstrap, 1);
    submit(&mut runtime, &mut peer, &control, &first, owner);
    control
        .checks
        .as_ref()
        .unwrap()
        .send(CheckStep::Panic)
        .unwrap();
    assert!(matches!(
        reply(&mut runtime, &mut peer, &first).result,
        Ste100Result::ComponentFailed {
            code: Ste100ComponentCode::NoResult,
            ..
        }
    ));
    assert_dropped(&control, owner, &files);
    assert_eq!(runtime.position(), Position::Closed);
    assert!(peer.try_receive().is_err());
}

#[test]
fn ste100_runtime_admission_refusal_never_becomes_ready() {
    let fixture = fixture::Fixture::new(true);
    let (mut peer, channel) = pair(fixture::LIMIT);
    let (control, adapter) = Control::new();
    let mut runtime = Runtime::new_fixture(channel, adapter);
    send(
        &mut peer,
        Tag::Bootstrap,
        0,
        serde_json::to_vec(&fixture.bootstrap).unwrap(),
    );
    runtime.advance().unwrap();
    let Event::Admission { files, .. } = control.event() else {
        panic!();
    };
    control.admit(AdmissionStep::Refuse);
    let message = message(&mut runtime, &mut peer);
    assert_eq!(message.tag, Tag::AdmissionRefused);
    assert_eq!(runtime.position(), Position::Closed);
    assert!(peer.try_receive().is_err());
    let deadline = Instant::now() + Duration::from_secs(30);
    while files.iter().any(|file| file.strong_count() > 0) {
        assert!(Instant::now() < deadline);
        thread::yield_now();
    }
}

#[test]
fn ste100_runtime_pending_ready_and_reply_reject_new_traffic_before_any_send() {
    for ready_pending in [true, false] {
        let fixture = fixture::Fixture::new(true);
        let (mut peer, channel) = pair(fixture::LIMIT);
        let (control, adapter) = Control::new();
        let mut runtime = Runtime::new_fixture(channel, adapter);
        send(
            &mut peer,
            Tag::Bootstrap,
            0,
            serde_json::to_vec(&fixture.bootstrap).unwrap(),
        );
        runtime.advance().unwrap();
        let Event::Admission {
            thread: owner,
            files,
            ..
        } = control.event()
        else {
            panic!();
        };
        control.admit(AdmissionStep::Proceed);
        wait_worker_ready(&runtime);
        runtime.advance().unwrap();
        assert_eq!(runtime.position(), Position::ReadyPending);
        if !ready_pending {
            assert_eq!(message(&mut runtime, &mut peer).tag, Tag::Ready);
            let first = request(&fixture.bootstrap, 1);
            submit(&mut runtime, &mut peer, &control, &first, owner);
            control.result(accepted());
            wait_worker_ready(&runtime);
            runtime.advance().unwrap();
            assert_eq!(runtime.position(), Position::ReplyPending);
        }
        assert_eq!(peer.try_receive().unwrap(), Receive::Pending);
        fragment(&peer, Tag::Request, if ready_pending { 1 } else { 2 });
        assert!(runtime.advance().is_err());
        assert_eq!(runtime.position(), Position::Failed);
        assert_dropped(&control, owner, &files);
        assert!(peer.try_receive().is_err());
    }
}

#[test]
fn ste100_runtime_drop_does_not_join_a_held_check_but_keeps_inputs_until_worker_finishes() {
    let (fixture, mut peer, mut runtime, control, owner, files) = ready();
    let first = request(&fixture.bootstrap, 1);
    submit(&mut runtime, &mut peer, &control, &first, owner);
    drop(fixture);
    let (done, completed) = channel();
    let disposal = thread::spawn(move || {
        drop(runtime);
        done.send(()).unwrap();
    });
    let detached = completed.recv_timeout(Duration::from_secs(30)).is_ok();
    assert!(files.iter().all(|file| file.strong_count() > 0));
    // Release even on the negative branch so a regression fails an assertion,
    // instead of stranding the test process on its intentionally held worker.
    control.result(accepted());
    assert_dropped(&control, owner, &files);
    disposal.join().unwrap();
    assert!(
        detached,
        "reactor disposal joined a model call before its owner released it"
    );
    assert!(peer.try_receive().is_err());
}

fn defect_with_encoded_length(request: &Ste100Request, length: usize) -> Ste100Result {
    let mut reply = Ste100Reply {
        request_id: request.request_id,
        target: request.target.clone(),
        scheme_id: request.scheme_id.clone(),
        result: Ste100Result::NotSte100 {
            code: Ste100DefectCode::NonSte100,
            detail: "x".into(),
            spans: vec![],
        },
    };
    let base = serde_json::to_vec(&reply).unwrap().len();
    let Ste100Result::NotSte100 { detail, .. } = &mut reply.result else {
        unreachable!();
    };
    *detail = "x".repeat(length - base + 1);
    assert_eq!(serde_json::to_vec(&reply).unwrap().len(), length);
    reply.result
}

#[test]
fn ste100_runtime_whole_reply_exact_bound_succeeds_and_one_byte_over_closes_without_truncation() {
    for extra in [0, 1] {
        let (fixture, mut peer, mut runtime, control, owner, files) = ready();
        let first = request(&fixture.bootstrap, 1);
        submit(&mut runtime, &mut peer, &control, &first, owner);
        let result = defect_with_encoded_length(&first, fixture::LIMIT + extra);
        control.result(result.clone());
        if extra == 0 {
            assert_eq!(reply(&mut runtime, &mut peer, &first).result, result);
            assert_eq!(runtime.position(), Position::Idle);
            drop(peer);
            assert_eq!(
                runtime.advance().unwrap(),
                RuntimeProgress::Stopped(Stop::PeerClosed)
            );
        } else {
            wait_worker_ready(&runtime);
            assert!(runtime.advance().is_err());
            assert_eq!(runtime.position(), Position::Failed);
            assert!(
                peer.try_receive().is_err(),
                "over-bound output must not become a truncated answer"
            );
        }
        assert_dropped(&control, owner, &files);
    }
}

#[test]
fn ste100_runtime_complete_multi_binding_and_template_results_are_single_correlated_answers() {
    let (fixture, mut peer, mut runtime, control, owner, files) = ready();
    let mut first = request(&fixture.bootstrap, 1);
    first.text = "Read item. Read item.".into();
    let Ste100Result::Accepted { mut bindings } = accepted() else {
        unreachable!();
    };
    let mut second = bindings[0].clone();
    second.statement_span = span(11, 21);
    second.verb_span = span(11, 15);
    second.nouns[0].span = span(16, 20);
    bindings.push(second);
    let whole = Ste100Result::Accepted { bindings };
    submit(&mut runtime, &mut peer, &control, &first, owner);
    control.result(whole.clone());
    assert_eq!(reply(&mut runtime, &mut peer, &first).result, whole);
    let mut template = request(&fixture.bootstrap, 2);
    template.target = Ste100Target::Template {
        template_id: "test-only.whole-inclusion".into(),
    };
    template.text = serde_json::to_string(&weaver_types::state_flow::TemplateSpec {
        template_id: "test-only.whole-inclusion".into(),
        kind: "loop.instruction".into(),
        parts: vec![
            weaver_types::state_flow::TemplatePart::Literal("Read ".into()),
            weaver_types::state_flow::TemplatePart::Slot {
                name: "item".into(),
                class: weaver_types::state_flow::SlotClass::TechnicalName,
            },
            weaver_types::state_flow::TemplatePart::Literal(".".into()),
        ],
    })
    .unwrap();
    submit(&mut runtime, &mut peer, &control, &template, owner);
    control.result(Ste100Result::Accepted { bindings: vec![] });
    assert_eq!(
        reply(&mut runtime, &mut peer, &template).result,
        Ste100Result::Accepted { bindings: vec![] }
    );
    // These injected complete results only pin the runtime's transport boundary;
    // they do not run a grammar compiler or prove production inclusion.
    drop(peer);
    runtime.advance().unwrap();
    assert_dropped(&control, owner, &files);
}

fn wait_on_driver(
    mut runtime: Runtime<Adapter>,
    peer: &mut Option<Channel>,
    finish: impl FnOnce(),
) -> Runtime<Adapter> {
    let (started, checkpoint) = channel();
    let (finished, result) = channel();
    let driver = thread::spawn(move || {
        started.send(()).unwrap();
        let waited = runtime.wait();
        assert!(finished.send((runtime, waited)).is_ok());
    });
    checkpoint
        .recv_timeout(Duration::from_secs(30))
        .expect("reactor reaches its actual wait");
    finish();
    let timely = result.recv_timeout(Duration::from_secs(30));
    let was_timely = timely.is_ok();
    let (runtime, waited) = match timely {
        Ok(result) => result,
        Err(_) => {
            drop(peer.take());
            result
                .recv_timeout(Duration::from_secs(30))
                .expect("peer closure releases failed wake fixture")
        }
    };
    driver.join().unwrap();
    assert!(
        was_timely,
        "completed worker did not wake the actual reactor poll"
    );
    waited.unwrap();
    runtime
}

#[test]
fn ste100_runtime_actual_poll_wakes_for_admission_check_and_worker_disconnect() {
    let fixture = fixture::Fixture::new(true);
    let (mut peer, channel) = pair(fixture::LIMIT);
    let (control, adapter) = Control::new();
    let mut runtime = Runtime::new_fixture(channel, adapter);
    send(
        &mut peer,
        Tag::Bootstrap,
        0,
        serde_json::to_vec(&fixture.bootstrap).unwrap(),
    );
    runtime.advance().unwrap();
    let Event::Admission {
        thread: owner,
        files,
        ..
    } = control.event()
    else {
        panic!();
    };
    let mut peer = Some(peer);
    runtime = wait_on_driver(runtime, &mut peer, || control.admit(AdmissionStep::Proceed));
    assert_eq!(
        message(&mut runtime, peer.as_mut().unwrap()).tag,
        Tag::Ready
    );
    let first = request(&fixture.bootstrap, 1);
    submit(
        &mut runtime,
        peer.as_mut().unwrap(),
        &control,
        &first,
        owner,
    );
    runtime = wait_on_driver(runtime, &mut peer, || control.result(accepted()));
    assert_eq!(
        reply(&mut runtime, peer.as_mut().unwrap(), &first).result,
        accepted()
    );
    let second = request(&fixture.bootstrap, 2);
    submit(
        &mut runtime,
        peer.as_mut().unwrap(),
        &control,
        &second,
        owner,
    );
    runtime = wait_on_driver(runtime, &mut peer, || {
        control
            .checks
            .as_ref()
            .unwrap()
            .send(CheckStep::Panic)
            .unwrap()
    });
    assert!(matches!(
        reply(&mut runtime, peer.as_mut().unwrap(), &second).result,
        Ste100Result::ComponentFailed {
            code: Ste100ComponentCode::NoResult,
            ..
        }
    ));
    assert_dropped(&control, owner, &files);
    assert_eq!(runtime.position(), Position::Closed);
}

#[test]
fn ste100_runtime_fragmented_reply_backpressure_never_admits_an_overlapping_request_or_prefix() {
    let limit = 1_000_000;
    let (fixture, mut peer, mut runtime, control, owner, files) = ready_with_limit(limit);
    nix::sys::socket::setsockopt(
        &runtime.dispatcher.readiness(),
        nix::sys::socket::sockopt::SndBuf,
        &65_536_usize,
    )
    .unwrap();
    let first = request(&fixture.bootstrap, 1);
    submit(&mut runtime, &mut peer, &control, &first, owner);
    control.result(defect_with_encoded_length(&first, limit));
    wait_worker_ready(&runtime);
    runtime.advance().unwrap();
    assert_eq!(runtime.position(), Position::ReplyPending);
    let mut held = false;
    for _ in 0..64 {
        if runtime.advance().unwrap() == RuntimeProgress::Pending {
            held = true;
            break;
        }
        assert_eq!(
            runtime.position(),
            Position::ReplyPending,
            "fixture must create real outbound pressure"
        );
    }
    assert!(
        held && runtime.dispatcher.wants_write(),
        "an actual fragment remains unsent"
    );
    // The peer has received a prefix in kernel storage, never a whole result.
    assert_eq!(peer.try_receive().unwrap(), Receive::Fragment);
    fragment(&peer, Tag::Request, 2);
    assert!(runtime.advance().is_err());
    assert_eq!(runtime.position(), Position::Failed);
    loop {
        match peer.try_receive() {
            Ok(Receive::Fragment) => {}
            Err(_) => break,
            other => panic!("no whole answer after overlap: {other:?}"),
        }
    }
    assert_dropped(&control, owner, &files);
}

#[test]
fn ste100_runtime_production_constructor_cannot_select_a_test_adapter_even_for_valid_nonfixture_bytes()
 {
    for fixture_marker in [false, true] {
        let fixture = fixture::Fixture::new(fixture_marker);
        let (mut peer, channel) = pair(fixture::LIMIT);
        let (control, adapter) = Control::new();
        // If the production factory guard is lost, the valid nonfixture variant
        // really reaches this adapter and produces Ready, rather than hanging or
        // being masked by the separate fixture-artifact refusal.
        control.admit(AdmissionStep::Proceed);
        let mut runtime = Runtime::new(channel, adapter);
        send(
            &mut peer,
            Tag::Bootstrap,
            0,
            serde_json::to_vec(&fixture.bootstrap).unwrap(),
        );
        let response = message(&mut runtime, &mut peer);
        assert_eq!(response.tag, Tag::AdmissionRefused);
        let refusal: Ste100AdmissionRefused =
            decode_strict(std::str::from_utf8(&response.payload).unwrap()).unwrap();
        assert_eq!(refusal.code, Ste100AdmissionCode::UnsupportedArtifact);
        assert!(
            matches!(
                control.events.try_recv(),
                Err(TryRecvError::Empty | TryRecvError::Disconnected)
            ),
            "production must not invoke test-only admission"
        );
        assert_eq!(runtime.position(), Position::Closed);
        assert!(peer.try_receive().is_err());
    }
}
