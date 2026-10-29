//! STATE-CIRCUIT-INSTRUMENT through the public Foundation surface. The normal
//! SQLite member, recorder, coordinator and codecs are real. Classifier, model
//! and admin publication replies are synthetic, never production qualification.
//! The fixture uses a delegated user scope and mapped-root user namespace; it
//! fails with the missing host prerequisite rather than skipping the circuit.
#![cfg(target_os = "linux")]

#[path = "state_flow/host.rs"]
mod host;
#[allow(dead_code)]
#[path = "state_flow/member.rs"]
mod member;
#[path = "state_flow/organs.rs"]
mod organs;

use nix::sys::socket::{
    AddressFamily, ControlMessage, MsgFlags, SockFlag, SockType, UnixAddr, connect, recv, send,
    sendmsg, socket,
};
use std::io::{IoSlice, Read};
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, Instant};
use weaver_harness::{
    FoundationAdmission, FoundationAdmissionProgress, FoundationEnterProgress,
    FoundationExitProgress, FoundationForcedLeaveProgress, FoundationHaltProgress,
    FoundationInputProgress, FoundationManagementProgress, FoundationProgram, LoopIdentity,
    OrganBinaries, OrganParameters, bind_coordination,
};
use weaver_types::state_flow::{self as s, capture::*, enter::*};
use weaver_types::{ExchangeId, LifecycleDirective, Opener, OrganEnvelope, Payload, Position};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Case {
    Idle,
    Held,
    Fragment,
    WrongIdentity,
    Discard,
    Loss,
}
impl Case {
    fn has_input(self) -> bool {
        self != Self::Idle
    }

    fn saves(self) -> bool {
        matches!(self, Self::Idle | Self::Held | Self::Fragment)
    }

    fn treatment(self) -> Option<member::ReceiptTreatment> {
        use member::ReceiptTreatment as R;
        match self {
            Self::Idle => None,
            Self::Held => Some(R::Hold),
            Self::Fragment => Some(R::Fragment),
            Self::WrongIdentity => Some(R::WrongIdentity),
            Self::Discard => Some(R::Discard),
            Self::Loss => Some(R::Kill),
        }
    }
}

fn now() -> u64 {
    let mut time = nix::libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: the kernel initializes a live timespec; no pointer is retained.
    assert_eq!(
        unsafe { nix::libc::clock_gettime(nix::libc::CLOCK_MONOTONIC, &mut time) },
        0
    );
    time.tv_sec as u64 * 1_000_000_000 + time.tv_nsec as u64
}

fn dial(path: &Path) -> OwnedFd {
    let fd = socket(
        AddressFamily::Unix,
        SockType::SeqPacket,
        SockFlag::SOCK_CLOEXEC,
        None,
    )
    .unwrap();
    connect(fd.as_raw_fd(), &UnixAddr::new(path).unwrap()).unwrap();
    fd
}

fn packet(fd: &OwnedFd) -> Option<Vec<u8>> {
    let mut bytes = vec![0; weaver_types::MAX_ENVELOPE_BYTES];
    match recv(fd.as_raw_fd(), &mut bytes, MsgFlags::MSG_DONTWAIT) {
        Ok(0) | Err(nix::errno::Errno::EAGAIN) => None,
        Ok(n) => {
            bytes.truncate(n);
            Some(bytes)
        }
        Err(error) => panic!("admin receive: {error}"),
    }
}

fn reply(peer: &OwnedFd, exchange: &ExchangeId, payload: CapturePayload) {
    let bytes = CaptureEnvelope {
        exchange: exchange.clone(),
        position: Position::Continue,
        payload,
    }
    .encode(CaptureSender::Admin)
    .unwrap();
    assert_eq!(
        send(peer.as_raw_fd(), &bytes, MsgFlags::MSG_NOSIGNAL),
        Ok(bytes.len())
    );
}

fn open_leave(path: &Path) -> OwnedFd {
    let peer = dial(path);
    let leave = OrganEnvelope {
        exchange: ExchangeId {
            opener: Opener::Admin,
            ordinal: 99,
        },
        position: Position::Open,
        payload: Payload::Directive(LifecycleDirective::Leave {
            cause: weaver_types::Cause { uid: 74 },
            forced: true,
            rollback: false,
        }),
    };
    let bytes = serde_json::to_vec(&leave).unwrap();
    assert_eq!(
        send(peer.as_raw_fd(), &bytes, MsgFlags::MSG_NOSIGNAL),
        Ok(bytes.len())
    );
    peer
}

struct StopResponder<'a>(&'a std::sync::atomic::AtomicBool);
impl Drop for StopResponder<'_> {
    fn drop(&mut self) {
        self.0.store(true, std::sync::atomic::Ordering::SeqCst);
    }
}

fn publication(
    peer: OwnedFd,
    root: &Path,
    done: &std::sync::atomic::AtomicBool,
) -> Option<s::SavePointReport> {
    let mut artifact = None;
    let mut complete = false;
    let end = Instant::now() + Duration::from_secs(25);
    while !done.load(std::sync::atomic::Ordering::SeqCst) {
        assert!(Instant::now() < end, "admin capture reply watchdog");
        if let Some(bytes) = packet(&peer) {
            let answer = CaptureEnvelope::decode(&bytes, CaptureSender::Harness).unwrap();
            assert_eq!(
                answer.exchange,
                ExchangeId {
                    opener: Opener::Admin,
                    ordinal: 99
                }
            );
            match answer.payload {
                CapturePayload::SavePointBeginning(_) => {
                    assert!(root.join("ready-rollback-lower.json").exists());
                }
                CapturePayload::SavePointImage(image) => {
                    let bytes =
                        std::fs::read(root.join("normal-member/room").join(image.room_name))
                            .unwrap();
                    assert_eq!(bytes.len() as u64, image.size_bytes.0);
                    std::fs::write(root.join("retained-capture.sqlite"), &bytes).unwrap();
                    let value = CaptureArtifact {
                        capture_id: image.capture_id,
                        artifact_id: format!("{:020}-{}", 2, "d".repeat(64)),
                        dump_sha256: member::digest(&bytes),
                        config_sha256: s::Digest::new("d".repeat(64)).unwrap(),
                    };
                    reply(
                        &peer,
                        &answer.exchange,
                        CapturePayload::SavePointImageChecked(CaptureImageChecked {
                            capture_id: value.capture_id,
                            dump_sha256: value.dump_sha256.clone(),
                        }),
                    );
                    artifact = Some(value);
                }
                CapturePayload::SavePointImageFinished(_) => reply(
                    &peer,
                    &answer.exchange,
                    CapturePayload::SavePointPrepared(artifact.as_ref().unwrap().clone()),
                ),
                CapturePayload::SavePointCommit(_) => reply(
                    &peer,
                    &answer.exchange,
                    CapturePayload::SavePointPublished(artifact.as_ref().unwrap().clone()),
                ),
                CapturePayload::SavePointCaptureComplete(_) => {
                    assert!(!complete);
                    complete = true;
                }
                CapturePayload::Answer(CaptureAnswer::Left {
                    save_point: Some(value),
                    forced: true,
                    no_state: false,
                }) => {
                    assert!(complete);
                    return Some(value);
                }
                other => panic!("unexpected capture reply {other:?}"),
            }
        }

        std::thread::sleep(Duration::from_millis(1));
    }
    None
}

struct Circuit {
    root: std::path::PathBuf,
    member: member::NormalMember,
    peer: UnixStream,
    reader: UnixStream,
    trace: Vec<u8>,
    requests: Vec<String>,
    filings: Vec<s::StateFrame>,
    end: Instant,
}

impl Drop for Circuit {
    fn drop(&mut self) {
        let _ = std::fs::write(self.root.join("trace.jsonl"), &self.trace);
        let _ = std::fs::write(
            self.root.join("request-kinds.json"),
            serde_json::to_vec(&self.requests).unwrap(),
        );
        let _ = std::fs::write(
            self.root.join("filings.json"),
            serde_json::to_vec(&self.filings).unwrap(),
        );
    }
}

impl Circuit {
    fn step(&mut self) {
        assert!(Instant::now() < self.end, "external state circuit watchdog");
        self.member
            .step(&mut self.peer, &mut self.requests, &mut self.filings);
        match self.reader.read_to_end(&mut self.trace) {
            Ok(_) => (),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => (),
            Err(error) => panic!("trace reader: {error}"),
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn circuit(root: &Path, case: Case) {
    let program = FoundationProgram {
        organs: OrganBinaries {
            classify: Some(host::organ(root, "checker")),
            spu: host::organ(root, "spu"),
            gate: host::organ(root, "gate"),
        },
        parameters: OrganParameters::default(),
        composer: LoopIdentity::compiled("external-state-circuit"),
        templates: organs::templates(),
    };
    let mut payload: EnterPayload =
        serde_json::from_str(include_str!("state_flow/enter.json")).unwrap();
    let EnterBinding::Serving {
        ste100_scheme,
        start,
        state_management,
        enter_window,
        ..
    } = &mut payload.binding
    else {
        panic!("serving fixture")
    };
    let mut normal = member::NormalMember::seeded(root, ste100_scheme, false);
    if let Some(treatment) = case.treatment() {
        normal.treat_first_filing(treatment);
    }
    let digest = member::digest(&normal.image);
    start.dump_sha256 = digest.clone();
    state_management.start.dump_sha256 = digest;
    *enter_window = s::EnterWindow {
        began_ns: now().into(),
        deadline_ns: (now() + 10_000_000_000).into(),
    };
    let envelope = EnterEnvelope {
        exchange: ExchangeId {
            opener: Opener::Admin,
            ordinal: 7,
        },
        position: Position::Open,
        payload: EnterContent::Directive(EnterDirective::Enter {
            payload: Box::new(payload),
        }),
    };
    let path = root.join("admin.sock");
    let mut admission = FoundationAdmission::listen(bind_coordination(&path).unwrap()).unwrap();
    let admin = dial(&path);
    let (sink, reader) = UnixStream::pair().unwrap();
    let (state, peer) = UnixStream::pair().unwrap();
    let bytes = envelope.encode().unwrap();
    assert_eq!(
        sendmsg::<()>(
            admin.as_raw_fd(),
            &[IoSlice::new(&bytes)],
            &[ControlMessage::ScmRights(&[
                sink.as_raw_fd(),
                state.as_raw_fd()
            ])],
            MsgFlags::MSG_NOSIGNAL,
            None,
        )
        .unwrap(),
        bytes.len()
    );
    drop((sink, state));
    peer.set_nonblocking(true).unwrap();
    reader.set_nonblocking(true).unwrap();
    let mut io = Circuit {
        root: root.into(),
        member: normal,
        peer,
        reader,
        trace: vec![],
        requests: vec![],
        filings: vec![],
        end: Instant::now() + Duration::from_secs(30),
    };
    let done = std::sync::atomic::AtomicBool::new(false);
    let (sender, receiver) = std::sync::mpsc::channel();
    let end = io.end;
    let tick = || {
        assert!(Instant::now() < end, "external owner watchdog");
        std::thread::sleep(Duration::from_millis(1));
    };
    let (report, sequence) = std::thread::scope(|scope| {
        let responder = scope.spawn(|| {
            let mut held_ticks = 0;
            let mut sent = false;
            while !done.load(std::sync::atomic::Ordering::SeqCst) {
                io.step();
                if case.has_input() && io.member.filing_held() && !sent {
                    held_ticks += 1;
                    assert_eq!(io.filings.len(), 1, "no resend of uncertain filing");
                    assert!(!root.join("ready-rollback-lower.json").exists());
                    if held_ticks == 150 {
                        let rows: Vec<serde_json::Value> = io
                            .trace
                            .split_inclusive(|b| *b == b'\n')
                            .filter(|line| line.last() == Some(&b'\n'))
                            .map(|line| serde_json::from_slice(line).unwrap())
                            .collect();
                        let samples: Vec<_> = rows
                            .iter()
                            .filter(|row| row["kind"] == "state.flow")
                            .collect();
                        assert!(
                            samples
                                .iter()
                                .any(|row| row["payload"]["pending_count"] == "1")
                        );
                        assert!(
                            samples
                                .iter()
                                .all(|row| row["payload"]["filed_count"] == "0"),
                            "a held real receipt cannot become filed"
                        );
                        std::fs::write(root.join("held-trace.jsonl"), &io.trace).unwrap();
                        if case.saves() {
                            sender.send(open_leave(&path)).unwrap();
                        } else {
                            io.member.apply_filing_fault();
                        }
                        sent = true;
                    }
                }
            }
        });
        let _stop = StopResponder(&done);
        loop {
            tick();
            match admission.advance() {
                FoundationAdmissionProgress::Pending => (),
                FoundationAdmissionProgress::Received => break,
                other => panic!("admission {other:?}: {:?}", admission.detail()),
            }
        }
        let mut run = Box::new(
            admission
                .adopt()
                .map_err(|a| a.detail().map(str::to_owned))
                .unwrap(),
        );
        let mut enter = run.begin(program).unwrap();
        loop {
            tick();
            // SAFETY: the isolated worker process retains this thread and every
            // owner through actual release/reap, or exits failed to its watchdog.
            match unsafe { enter.advance() } {
                FoundationEnterProgress::Pending => (),
                FoundationEnterProgress::Ready => break,
                other => panic!("startup {other:?}: {:?}", enter.detail()),
            }
        }
        let ready: OrganEnvelope =
            serde_json::from_slice(&packet(&admin).expect("Ready sent")).unwrap();
        assert!(matches!(
            ready.payload,
            Payload::Answer(weaver_types::LifecycleAnswer::Ready)
        ));
        drop(admin);
        let serving = enter.finish().ok().unwrap();
        let mut input = serving.input();
        if case.has_input() {
            while input.progress() != FoundationInputProgress::Accepted {
                tick();
                assert_ne!(
                    input.advance(),
                    FoundationInputProgress::Stopped,
                    "{:?}",
                    input.detail()
                );
            }
            input
                .start_task(weaver_types::TurnKey("external-input".into()))
                .unwrap();
        }
        if !case.saves() {
            while input.progress() != FoundationInputProgress::Stopped {
                tick();
                match input.advance() {
                    FoundationInputProgress::Pending => {
                        input.wait_for_wake();
                    }
                    FoundationInputProgress::Stopped => break,
                    other => panic!("filing fault must stop the input: {other:?}"),
                }
            }
            let mut halt = input
                .halt()
                .ok()
                .expect("failed input retains its halt owner");
            loop {
                tick();
                match halt.advance() {
                    FoundationHaltProgress::Pending => {
                        halt.wait_for_progress();
                    }
                    FoundationHaltProgress::Progress => (),
                    FoundationHaltProgress::Prepared => break,
                    other => panic!("unexpected halt {other:?}: {:?}", halt.detail()),
                }
            }
            let handoff = halt.handoff().ok().expect("terminal halt handoff");
            let mut exit = run.failed_exit(handoff).ok().expect("same failed run");
            loop {
                tick();
                match exit.advance() {
                    FoundationExitProgress::Pending => {
                        exit.wait_for_progress();
                    }
                    FoundationExitProgress::Progress => (),
                    FoundationExitProgress::Closed => break,
                }
            }
            assert!(!exit.abandoned());
            assert!(matches!(
                exit.channel_fault(),
                Some(weaver_harness::ChannelFault::Closed)
            ));
            let sequence = exit
                .unload_sequence()
                .expect("failed run's recorded Unload");
            drop(exit);
            done.store(true, std::sync::atomic::Ordering::SeqCst);
            responder.join().unwrap();
            return (None, sequence);
        }
        if !case.has_input() {
            sender.send(open_leave(&path)).unwrap();
        }
        while input.progress() != FoundationInputProgress::ManagementPending {
            tick();
            assert_ne!(
                input.advance(),
                FoundationInputProgress::Stopped,
                "{:?}",
                input.detail()
            );
        }
        let peer = receiver.recv_timeout(Duration::from_secs(5)).unwrap();
        let mut management = input.management().ok().unwrap();
        while management.progress() != FoundationManagementProgress::DirectivePending {
            tick();
            assert_ne!(
                management.advance(),
                FoundationManagementProgress::Stopped,
                "{:?}",
                management.detail()
            );
        }
        let handoff = management.forced_leave_handoff().ok().unwrap();
        let mut owner = run.forced_leave(handoff).ok().unwrap();
        let admin = scope.spawn(|| publication(peer, root, &done));
        loop {
            tick();
            let progress = owner.advance();
            if progress == FoundationForcedLeaveProgress::Closed {
                break;
            }
            if progress == FoundationForcedLeaveProgress::Pending {
                owner.wait_for_progress();
            }
        }
        assert!(!owner.abandoned(), "{:?}", owner.detail());
        let sequence = owner.unload_sequence().expect("actual recorded Unload");
        drop(owner);
        let report = admin
            .join()
            .unwrap()
            .expect("successful Left carries the save report");
        done.store(true, std::sync::atomic::Ordering::SeqCst);
        responder.join().unwrap();
        (Some(report), sequence)
    });
    io.member
        .finish(&mut io.peer, &mut io.requests, &mut io.filings);
    io.step();
    if let Some(report) = &report {
        io.member.assert_capture(report, &io.filings);
    } else {
        assert_eq!(io.filings.len(), 1, "no resend after filing failure");
        assert!(
            io.member.filing_held(),
            "original committed receipt retained"
        );
        assert!(!root.join("retained-capture.sqlite").exists());
        assert_eq!(io.requests.last().unwrap(), "file", "no work after failure");
    }
    std::fs::write(root.join("trace.jsonl"), &io.trace).unwrap();
    let rows: Vec<serde_json::Value> = io
        .trace
        .split(|b| *b == b'\n')
        .filter(|b| !b.is_empty())
        .map(|b| serde_json::from_slice(b).unwrap())
        .collect();
    assert_eq!(rows.last().unwrap()["kind"], "unload");
    assert_eq!(rows.last().unwrap()["sequence"], sequence.to_string());
    assert_eq!(
        rows.iter().filter(|r| r["kind"] == "message.user").count(),
        usize::from(case.has_input())
    );
    let faults: Vec<_> = rows.iter().filter(|r| r["kind"] == "fault").collect();
    if case.saves() {
        assert!(faults.is_empty());
    } else {
        assert_eq!(faults.len(), 1, "one first fatal cause");
        assert_failed_account(case, faults[0], rows.last().unwrap(), &io.filings[0]);
        assert!(
            rows.iter()
                .filter(|r| r["kind"] == "state.flow")
                .all(|r| r["payload"]["filed_count"] == "0"),
            "an unavailable or corrupt receipt never grants filed acknowledgement"
        );
        assert!(root.join("world-close.json").exists());
        assert!(root.join("ready-rollback-lower.json").exists());
        let closes: Vec<_> = rows.iter().filter(|r| r["kind"] == "turn.closed").collect();
        assert_eq!(closes.len(), 1);
        assert_eq!(
            closes[0]["payload"],
            serde_json::json!({"close":"stopped", "reason":"fault"})
        );
    }
    if case.has_input() {
        let original = std::fs::read(root.join("original-filing.jsonl")).unwrap();
        let response = s::decode_state_line(&original, s::FrameDirection::Response).unwrap();
        assert!(matches!(response, s::StateFrame::Filed { .. }));
        response
            .validate_response_to(&io.filings[0], response.observation().unwrap().operation_id)
            .unwrap();
        assert!(
            std::fs::read(root.join("member-responses.jsonl"))
                .unwrap()
                .windows(original.len())
                .any(|bytes| bytes == original)
        );
        if case == Case::Fragment {
            let prefix = std::fs::read(root.join("filing-prefix.bin")).unwrap();
            assert!(!prefix.is_empty() && prefix.len() < original.len());
            assert_eq!(prefix, original[..prefix.len()]);
            assert!(!prefix.contains(&b'\n'));
        }
        if case == Case::WrongIdentity {
            let mut expected: serde_json::Value = serde_json::from_slice(&original).unwrap();
            expected["request_id"] = (response.common().request_id.0 + 1).to_string().into();
            let damaged: serde_json::Value =
                serde_json::from_slice(&std::fs::read(root.join("damaged-filing.jsonl")).unwrap())
                    .unwrap();
            assert_eq!(
                damaged, expected,
                "only the proxy's request identity changes"
            );
        }
        if case == Case::Loss {
            assert_eq!(
                std::fs::read(root.join("normal-member/killed-and-reaped")).unwrap(),
                b"SIGKILL\n"
            );
        }
    }
}

fn assert_failed_account(
    case: Case,
    fault: &serde_json::Value,
    unload: &serde_json::Value,
    request: &s::StateFrame,
) {
    let s::StateFrame::File { common, filing } = request else {
        panic!("actual statement filing");
    };
    assert_eq!(fault["payload"]["case"], "state_flow_failed");
    let account = &fault["payload"]["account"];
    let expected = match case {
        Case::WrongIdentity => ("protocol_failed", "state_reply", "unconfirmed"),
        Case::Discard => ("decision_bound", "barrier", "closed"),
        Case::Loss => ("carry_failed", "state_reply", "unconfirmed"),
        other => panic!("not a failure case: {other:?}"),
    };
    assert_eq!(account["reason"], expected.0);
    assert_eq!(account["point"], expected.1);
    assert_eq!(account["flow_sample"]["pending_count"], "1");
    assert_eq!(account["flow_sample"]["filed_count"], "0");
    assert_eq!(account["outstanding_blocks"].as_array().unwrap().len(), 1);
    let held = &account["outstanding_blocks"][0];
    assert_eq!(
        held["block_id"],
        serde_json::to_value(&filing.block_id).unwrap()
    );
    assert_eq!(held["state"], "awaiting_receipt");
    assert_eq!(
        held["commit"], "unknown",
        "the proxy's knowledge is not the harness's"
    );
    assert_eq!(held["request_id"], common.request_id.0.to_string());
    assert_eq!(
        held["bytes_sent"],
        s::encode_state_line(request, s::FrameDirection::Request)
            .unwrap()
            .len()
            .to_string()
    );
    assert_eq!(held["frame_bytes"], held["bytes_sent"]);
    if case == Case::Discard {
        let wait = &account["wait"];
        let number = |v: &serde_json::Value| v.as_str().unwrap().parse::<u64>().unwrap();
        assert_eq!(wait["stage"], "decision");
        assert_eq!(
            number(&wait["deadline_ns"]) - number(&wait["began_ns"]),
            5_000_000_000
        );
        assert!(number(&account["observed_ns"]) >= number(&wait["deadline_ns"]));
    } else {
        assert_eq!(account["source_id"], held["block_id"]);
        assert_eq!(account["request"]["id"], common.request_id.0.to_string());
        if case == Case::WrongIdentity {
            assert_eq!(
                account["received_request"]["id"],
                (common.request_id.0 + 1).to_string()
            );
        } else {
            assert!(account["received_request"].is_null());
        }
    }
    assert_eq!(unload["payload"]["end_reason"], "component_failure");
    assert_eq!(unload["payload"]["release"]["spu"], "confirmed");
    assert_eq!(unload["payload"]["release"]["member"], expected.2);
    assert_eq!(
        unload["payload"]["state_flow_failure"]["fault_sequence"],
        fault["sequence"]
    );
    assert_eq!(&unload["payload"]["state_flow_failure"]["account"], account);
}

#[test]
fn state_flow_idle_capture_uses_normal_member() {
    host::scenario(
        "state_flow_idle_capture_uses_normal_member",
        Case::Idle,
        circuit,
    );
}

#[test]
fn state_flow_held_filing_is_not_resent_and_survives_capture() {
    host::scenario(
        "state_flow_held_filing_is_not_resent_and_survives_capture",
        Case::Held,
        circuit,
    );
}

#[test]
fn state_flow_fragmented_receipt_retains_custody_until_complete() {
    host::scenario(
        "state_flow_fragmented_receipt_retains_custody_until_complete",
        Case::Fragment,
        circuit,
    );
}

#[test]
fn state_flow_wrong_receipt_identity_halts_without_filing() {
    host::scenario(
        "state_flow_wrong_receipt_identity_halts_without_filing",
        Case::WrongIdentity,
        circuit,
    );
}

#[test]
fn state_flow_discarded_receipt_expires_without_resend() {
    host::scenario(
        "state_flow_discarded_receipt_expires_without_resend",
        Case::Discard,
        circuit,
    );
}

#[test]
fn state_flow_member_loss_runs_failed_cleanup() {
    host::scenario(
        "state_flow_member_loss_runs_failed_cleanup",
        Case::Loss,
        circuit,
    );
}

#[test]
fn state_flow_fixture_organ() {
    if let Ok(kind) = std::env::var("WEAVER_EXTERNAL_STATE_ORGAN") {
        organs::run(&kind);
    }
}
