use crate::flow::classifier::{PortFailure, PortFailureKind, ResultPoll, Ste100Port, Submit};
use crate::flow::ste100_channel::{Channel, DATAGRAM_BYTES, Fault, HEADER_BYTES, Receive, Tag};
use crate::flow::ste100_socket::*;
use nix::sys::socket::{
    AddressFamily, MsgFlags, SockFlag, SockType, recv, send, setsockopt, socketpair, sockopt,
};
use std::os::fd::{AsRawFd, OwnedFd};
use weaver_types::state_flow::{
    Binding, DecimalU64, Digest, NounBinding, RepairCap, STE100_PROTOCOL, SchemeDeclaration, Span,
    Ste100AdmissionCode, Ste100AdmissionRefused, Ste100Bootstrap, Ste100ComponentCode,
    Ste100DefectCode, Ste100Instruction, Ste100Ready, Ste100Reply, Ste100Request, Ste100Result,
    Ste100Target, decode_strict,
};
use weaver_types::{ArtifactRef, DeviceOrdinal, ModelBinding};

const LIMIT: u64 = 2_000_000;
const ATTEMPTS: usize = 256;

// Synthetic transport data, not a qualified language artifact or model claim.
fn fixture() -> (Ste100Bootstrap, SchemeDeclaration, Ste100Ready) {
    let seed: serde_json::Value = serde_json::from_str(include_str!("fixtures/seed.json")).unwrap();
    let scheme_id = seed["scheme_id"].as_str().unwrap().to_owned();
    let scheme = SchemeDeclaration {
        artifact_sha256: Digest::new(scheme_id.strip_prefix("ste100:").unwrap()).unwrap(),
        scheme_id,
        verbs: vec!["read".into()],
    };
    let instruction = Ste100Instruction {
        model_binding: ModelBinding {
            artifact: ArtifactRef("synthetic-checker-artifact".into()),
            devices: vec![DeviceOrdinal(0)],
        },
        model_artifact_sha256: Digest::new("1".repeat(64)).unwrap(),
        instances: 1,
        max_wire_bytes: DecimalU64(LIMIT),
        measurement_ref: "socket-fixture-only".into(),
        repair_cap: RepairCap {
            attempts: 2,
            measurement_ref: "socket-fixture-only".into(),
        },
        decision_wait: serde_json::from_str(
            r#"{"elapsed_ns":"1","measurement_ref":"socket-fixture-only"}"#,
        )
        .unwrap(),
    };
    let ready = Ste100Ready {
        protocol: STE100_PROTOCOL.into(),
        scheme_id: scheme.scheme_id.clone(),
        artifact_sha256: scheme.artifact_sha256.clone(),
        checker_artifact_sha256: instruction.model_artifact_sha256.clone(),
        instances: instruction.instances,
        max_wire_bytes: instruction.max_wire_bytes,
    };
    let bootstrap = Ste100Bootstrap {
        protocol: STE100_PROTOCOL.into(),
        scheme_text: seed["ste100_scheme"].as_str().unwrap().into(),
        instruction,
    };
    (bootstrap, scheme, ready)
}

fn ends() -> (OwnedFd, OwnedFd) {
    socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .unwrap()
}

fn starting() -> (SocketPort, Channel, Ste100Ready) {
    let (end, peer) = ends();
    let (bootstrap, scheme, ready) = fixture();
    let port = SocketPort::starting(end, bootstrap, scheme).unwrap();
    (
        port,
        Channel::from_owned(peer, LIMIT as usize).unwrap(),
        ready,
    )
}

fn receive_bootstrap(port: &mut SocketPort, peer: &mut Channel) -> Ste100Bootstrap {
    for _ in 0..ATTEMPTS {
        assert_eq!(port.advance_admission().unwrap(), AdmissionPoll::Pending);
        match peer.try_receive().unwrap() {
            Receive::Complete(message) => {
                assert_eq!((message.tag, message.id), (Tag::Bootstrap, 0));
                return decode_strict(std::str::from_utf8(&message.payload).unwrap()).unwrap();
            }
            Receive::Pending | Receive::Fragment => {}
        }
    }
    panic!("bootstrap was not completely sent");
}

fn send_payload(peer: &mut Channel, tag: Tag, id: u64, payload: Vec<u8>) {
    peer.begin_send(tag, id, payload).unwrap();
    for _ in 0..ATTEMPTS {
        peer.try_send().unwrap();
        if !peer.has_pending_send() {
            return;
        }
    }
    panic!("peer send failed to complete within fixture bound");
}

fn admit() -> (SocketPort, Channel) {
    let (mut port, mut peer, ready) = starting();
    let received = receive_bootstrap(&mut port, &mut peer);
    assert_eq!(received, fixture().0);
    send_payload(
        &mut peer,
        Tag::Ready,
        0,
        serde_json::to_vec(&ready).unwrap(),
    );
    assert_eq!(
        port.advance_admission().unwrap(),
        AdmissionPoll::Ready(ready)
    );
    assert!(!port.wants_write());
    (port, peer)
}

fn request(id: u64) -> Ste100Request {
    Ste100Request {
        request_id: id.into(),
        target: Ste100Target::Input {
            input_id: 5.into(),
            block: 0.into(),
        },
        text: "Read caf\u{e9}.".into(),
        scheme_id: fixture().1.scheme_id,
    }
}

fn accepted(request: &Ste100Request) -> Ste100Reply {
    let span = |start: u64, end: u64| Span {
        start: start.into(),
        end: end.into(),
    };
    Ste100Reply {
        request_id: request.request_id,
        target: request.target.clone(),
        scheme_id: request.scheme_id.clone(),
        result: Ste100Result::Accepted {
            bindings: vec![Binding {
                statement_span: span(0, 11),
                verb: "read".into(),
                verb_span: span(0, 4),
                condition: None,
                condition_span: None,
                nouns: vec![NounBinding {
                    value: "caf\u{e9}".into(),
                    span: span(5, 10),
                }],
            }],
        },
    }
}

fn rejected(request: &Ste100Request) -> Ste100Reply {
    Ste100Reply {
        result: Ste100Result::NotSte100 {
            code: Ste100DefectCode::NonSte100,
            detail: "synthetic rejection".into(),
            spans: vec![],
        },
        ..accepted(request)
    }
}

fn receive_request(port: &mut SocketPort, peer: &mut Channel) -> Ste100Request {
    for _ in 0..ATTEMPTS {
        assert_eq!(port.try_result().unwrap(), ResultPoll::Pending);
        match peer.try_receive().unwrap() {
            Receive::Complete(message) => {
                assert_eq!(message.tag, Tag::Request);
                let request: Ste100Request =
                    decode_strict(std::str::from_utf8(&message.payload).unwrap()).unwrap();
                assert_eq!(message.id, request.request_id.0);
                return request;
            }
            Receive::Pending | Receive::Fragment => {}
        }
    }
    panic!("request was not completely sent");
}

fn assert_component(failure: &PortFailure, code: Ste100ComponentCode) {
    assert_eq!(failure.kind, PortFailureKind::Component(code));
}

fn assert_frozen(port: &mut SocketPort, failure: &PortFailure) {
    assert!(!port.wants_write());
    assert!(!port.needs_progress());
    assert_eq!(port.try_result().unwrap_err(), *failure);
    assert_eq!(port.advance_admission().unwrap_err(), *failure);
    assert_eq!(port.submit(request(99)).unwrap_err(), *failure);
}

#[test]
fn bootstrap_is_exact_and_ready_transfers_once_on_the_actual_socket() {
    let (end, other) = ends();
    let original_fd = end.as_raw_fd();
    let (bootstrap, scheme, ready) = fixture();
    let mut port = SocketPort::starting(end, bootstrap, scheme).unwrap();
    let mut peer = Channel::from_owned(other, LIMIT as usize).unwrap();
    assert_eq!(port.readiness().as_raw_fd(), original_fd);
    assert!(port.wants_write());
    assert_eq!(receive_bootstrap(&mut port, &mut peer), fixture().0);
    send_payload(
        &mut peer,
        Tag::Ready,
        0,
        serde_json::to_vec(&ready).unwrap(),
    );
    assert_eq!(
        port.advance_admission().unwrap(),
        AdmissionPoll::Ready(ready)
    );
    assert_eq!(port.advance_admission().unwrap(), AdmissionPoll::Pending);
    assert_eq!(port.try_result().unwrap(), ResultPoll::Pending);
    assert!(!port.wants_write());
}

#[test]
fn invalid_local_bootstrap_and_declaration_never_send_bytes() {
    for case in 0..7 {
        let (end, peer) = ends();
        let (mut bootstrap, mut scheme, _) = fixture();
        match case {
            0 => bootstrap.protocol = "labels/1".into(),
            1 => bootstrap.instruction.instances = 0,
            2 => bootstrap.instruction.max_wire_bytes = 0.into(),
            3 => bootstrap.instruction.max_wire_bytes = 16.into(),
            4 => bootstrap.scheme_text.clear(),
            5 => scheme.scheme_id = "wrong digest identity".into(),
            6 => scheme.verbs.push("read".into()),
            _ => unreachable!(),
        }
        assert!(
            SocketPort::starting(end, bootstrap, scheme).is_err(),
            "case {case}"
        );
        let mut bytes = [0; DATAGRAM_BYTES];
        assert_eq!(
            recv(peer.as_raw_fd(), &mut bytes, MsgFlags::MSG_DONTWAIT),
            Ok(0)
        );
    }
    let (end, _) = std::os::unix::net::UnixStream::pair().unwrap();
    let (bootstrap, scheme, _) = fixture();
    let failure = SocketPort::starting(end.into(), bootstrap, scheme)
        .err()
        .unwrap();
    assert_component(&failure, Ste100ComponentCode::TransportFailed);
}

#[test]
fn every_ready_identity_echo_and_strict_shape_is_required() {
    for field in [
        "protocol",
        "scheme_id",
        "artifact_sha256",
        "checker_artifact_sha256",
        "instances",
        "max_wire_bytes",
        "unknown",
        "duplicate",
        "numeric_counter",
    ] {
        let (mut port, mut peer, ready) = starting();
        receive_bootstrap(&mut port, &mut peer);
        let mut value = serde_json::to_value(&ready).unwrap();
        match field {
            "protocol" | "scheme_id" => value[field] = "different".into(),
            "artifact_sha256" | "checker_artifact_sha256" => {
                value[field] = "9".repeat(64).into();
            }
            "instances" => value[field] = 2.into(),
            "max_wire_bytes" => value[field] = "1999999".into(),
            "unknown" => value["extra"] = true.into(),
            "numeric_counter" => value["max_wire_bytes"] = LIMIT.into(),
            "duplicate" => {}
            _ => unreachable!(),
        }
        let mut bytes = serde_json::to_vec(&value).unwrap();
        if field == "duplicate" {
            bytes.splice(1..1, b"\"instances\":1,".iter().copied());
        }
        send_payload(&mut peer, Tag::Ready, 0, bytes);
        let failure = port.advance_admission().unwrap_err();
        assert_component(&failure, Ste100ComponentCode::InvalidResult);
        assert_frozen(&mut port, &failure);
    }
}

#[test]
fn every_admission_refusal_transfers_once_without_becoming_a_ready() {
    for code in [
        Ste100AdmissionCode::SchemeMismatch,
        Ste100AdmissionCode::UnsupportedArtifact,
        Ste100AdmissionCode::InvalidGrammar,
        Ste100AdmissionCode::InsufficientCapacity,
        Ste100AdmissionCode::InvalidInstruction,
    ] {
        let (mut port, mut peer, _) = starting();
        receive_bootstrap(&mut port, &mut peer);
        let refusal = Ste100AdmissionRefused {
            protocol: STE100_PROTOCOL.into(),
            code,
            detail: "fixture admission refusal".into(),
        };
        send_payload(
            &mut peer,
            Tag::AdmissionRefused,
            0,
            serde_json::to_vec(&refusal).unwrap(),
        );
        assert_eq!(
            port.advance_admission().unwrap(),
            AdmissionPoll::Refused(refusal)
        );
        assert!(!port.wants_write());
        let failure = port.advance_admission().unwrap_err();
        assert_eq!(failure.kind, PortFailureKind::ProtocolFailed);
        assert_frozen(&mut port, &failure);
    }
}

#[test]
fn admission_and_service_cannot_cross_before_ready_or_before_complete_send() {
    for service_poll in [false, true] {
        let (mut port, _, _) = starting();
        let failure = if service_poll {
            port.try_result().unwrap_err()
        } else {
            port.submit(request(1)).unwrap_err()
        };
        assert_eq!(failure.kind, PortFailureKind::ProtocolFailed);
        assert_frozen(&mut port, &failure);
    }
    let (end, other) = ends();
    let (mut bootstrap, scheme, ready) = fixture();
    bootstrap.scheme_text = "x".repeat(DATAGRAM_BYTES * 4);
    let mut port = SocketPort::starting(end, bootstrap, scheme).unwrap();
    let mut peer = Channel::from_owned(other, LIMIT as usize).unwrap();
    assert_eq!(port.advance_admission().unwrap(), AdmissionPoll::Pending);
    assert_eq!(peer.try_receive().unwrap(), Receive::Fragment);
    send_payload(
        &mut peer,
        Tag::Ready,
        0,
        serde_json::to_vec(&ready).unwrap(),
    );
    let failure = port.advance_admission().unwrap_err();
    assert_component(&failure, Ste100ComponentCode::InvalidResult);
    assert_frozen(&mut port, &failure);
}

#[test]
fn queued_ready_before_first_send_is_not_made_timely_by_sending_bootstrap() {
    let (mut port, mut peer, ready) = starting();
    send_payload(
        &mut peer,
        Tag::Ready,
        0,
        serde_json::to_vec(&ready).unwrap(),
    );
    let failure = port.advance_admission().unwrap_err();
    assert_component(&failure, Ste100ComponentCode::InvalidResult);
    assert_frozen(&mut port, &failure);
    assert_eq!(peer.try_receive(), Err(Fault::Closed));
}

#[test]
fn queued_reply_before_final_send_cannot_become_a_timely_reply() {
    let (mut port, mut peer) = admit();
    let mut request = request(1);
    request.text = "x".repeat(DATAGRAM_BYTES);
    let encoded = serde_json::to_vec(&request).unwrap();
    let fragment_bytes = DATAGRAM_BYTES - HEADER_BYTES;
    assert!(encoded.len() > fragment_bytes && encoded.len() < fragment_bytes * 2);
    port.submit(request.clone()).unwrap();
    assert_eq!(port.try_result().unwrap(), ResultPoll::Pending);
    assert_eq!(peer.try_receive().unwrap(), Receive::Fragment);
    send_payload(
        &mut peer,
        Tag::Reply,
        1,
        serde_json::to_vec(&rejected(&request)).unwrap(),
    );
    let failure = port.try_result().unwrap_err();
    assert_component(&failure, Ste100ComponentCode::InvalidResult);
    assert_frozen(&mut port, &failure);
    assert_eq!(peer.try_receive(), Err(Fault::Closed));
}

#[test]
fn busy_returns_whole_request_and_each_language_result_releases_once() {
    let (mut port, mut peer) = admit();
    for id in 1..=2 {
        let request = request(id);
        assert_eq!(port.submit(request.clone()).unwrap(), Submit::Submitted);
        assert!(port.wants_write());
        let mut unsent = request.clone();
        unsent.text = "unmodified \r\n second input".into();
        unsent.request_id = 0.into();
        assert_eq!(port.submit(unsent.clone()).unwrap(), Submit::Busy(unsent));
        assert_eq!(receive_request(&mut port, &mut peer), request);
        assert!(!port.wants_write());
        let reply = if id == 1 {
            accepted(&request)
        } else {
            rejected(&request)
        };
        send_payload(
            &mut peer,
            Tag::Reply,
            id,
            serde_json::to_vec(&reply).unwrap(),
        );
        assert_eq!(port.try_result().unwrap(), ResultPoll::Completed(reply));
        assert_eq!(port.try_result().unwrap(), ResultPoll::Pending);
    }
}

#[test]
fn reply_identity_and_spans_use_shared_validation_and_retain_received_evidence() {
    for field in [
        "header",
        "request_id",
        "target",
        "scheme",
        "binding",
        "empty",
    ] {
        let (mut port, mut peer) = admit();
        let request = request(1);
        port.submit(request.clone()).unwrap();
        receive_request(&mut port, &mut peer);
        let mut reply = accepted(&request);
        let mut header_id = 1;
        match field {
            "header" => header_id = 2,
            "request_id" => {
                reply.request_id = 2.into();
                header_id = 2;
            }
            "target" => {
                reply.target = Ste100Target::Input {
                    input_id: 8.into(),
                    block: 0.into(),
                }
            }
            "scheme" => reply.scheme_id = "other".into(),
            "binding" => {
                let Ste100Result::Accepted { bindings } = &mut reply.result else {
                    unreachable!()
                };
                bindings[0].nouns[0].span.end = 9.into(); // Splits UTF-8.
            }
            "empty" => reply.result = Ste100Result::Accepted { bindings: vec![] },
            _ => unreachable!(),
        }
        send_payload(
            &mut peer,
            Tag::Reply,
            header_id,
            serde_json::to_vec(&reply).unwrap(),
        );
        let failure = port.try_result().unwrap_err();
        assert_component(&failure, Ste100ComponentCode::InvalidResult);
        assert_eq!(failure.received.as_deref(), Some(&reply), "{field}");
        assert_frozen(&mut port, &failure);
    }
}

#[test]
fn invalid_utf8_duplicate_fields_and_legacy_labels_are_not_results() {
    for case in 0..5 {
        let (mut port, mut peer) = admit();
        let request = request(1);
        port.submit(request.clone()).unwrap();
        receive_request(&mut port, &mut peer);
        let mut payload = serde_json::to_vec(&accepted(&request)).unwrap();
        match case {
            0 => payload = vec![0xff],
            1 => {
                payload.splice(1..1, b"\"request_id\":\"1\",".iter().copied());
            }
            2 => payload = br#"{"scores":[0.9]}"#.to_vec(),
            3 => payload = br#"{"label_refusal":{"detail":"not a language reply"}}"#.to_vec(),
            4 => payload = b"not JSON".to_vec(),
            _ => unreachable!(),
        }
        send_payload(&mut peer, Tag::Reply, 1, payload);
        let failure = port.try_result().unwrap_err();
        assert_component(&failure, Ste100ComponentCode::InvalidResult);
        assert_frozen(&mut port, &failure);
    }
}

#[test]
fn component_reply_survives_peer_exit_transfers_once_and_preserves_original_cause() {
    for code in [
        Ste100ComponentCode::Crashed,
        Ste100ComponentCode::NoResult,
        Ste100ComponentCode::InvalidResult,
        Ste100ComponentCode::TransportFailed,
    ] {
        let (mut port, mut peer) = admit();
        let request = request(1);
        port.submit(request.clone()).unwrap();
        receive_request(&mut port, &mut peer);
        let reply = Ste100Reply {
            result: Ste100Result::ComponentFailed {
                code,
                detail: "exact backend failure".into(),
            },
            ..accepted(&request)
        };
        send_payload(
            &mut peer,
            Tag::Reply,
            1,
            serde_json::to_vec(&reply).unwrap(),
        );
        drop(peer);
        assert_eq!(
            port.try_result().unwrap(),
            ResultPoll::Completed(reply.clone())
        );
        let failure = port.try_result().unwrap_err();
        assert_component(&failure, code);
        assert_eq!(failure.detail, "exact backend failure");
        assert_eq!(failure.received.as_deref(), Some(&reply));
        assert_frozen(&mut port, &failure);
    }
}

#[test]
fn unsolicited_and_duplicate_replies_cannot_attach_to_a_later_request() {
    for before_submit in [false, true] {
        let (mut port, mut peer) = admit();
        let request = request(1);
        let reply = accepted(&request);
        send_payload(
            &mut peer,
            Tag::Reply,
            1,
            serde_json::to_vec(&reply).unwrap(),
        );
        let failure = if before_submit {
            port.submit(request).unwrap_err()
        } else {
            port.try_result().unwrap_err()
        };
        assert_component(&failure, Ste100ComponentCode::InvalidResult);
        assert_eq!(failure.received.as_deref(), Some(&reply));
        assert_frozen(&mut port, &failure);
    }
    let (mut port, mut peer) = admit();
    let request = request(1);
    port.submit(request.clone()).unwrap();
    receive_request(&mut port, &mut peer);
    let payload = serde_json::to_vec(&accepted(&request)).unwrap();
    send_payload(&mut peer, Tag::Reply, 1, payload.clone());
    send_payload(&mut peer, Tag::Reply, 1, payload);
    assert!(matches!(
        port.try_result().unwrap(),
        ResultPoll::Completed(_)
    ));
    let failure = port.submit(self::request(2)).unwrap_err();
    assert_component(&failure, Ste100ComponentCode::InvalidResult);
}

#[test]
fn fragmented_send_retains_the_exact_whole_request_through_socket_pressure() {
    let (mut port, mut peer) = admit();
    setsockopt(&port.readiness(), sockopt::SndBuf, &131_072).unwrap();
    let mut request = request(1);
    request.text = "Read caf\u{e9}.\r\n".repeat(80_000);
    port.submit(request.clone()).unwrap();
    for _ in 0..32 {
        assert_eq!(port.try_result().unwrap(), ResultPoll::Pending);
    }
    assert!(
        port.wants_write(),
        "fixture must exercise a blocked partial send"
    );
    assert_eq!(receive_request(&mut port, &mut peer), request);
    let reply = rejected(&request);
    send_payload(
        &mut peer,
        Tag::Reply,
        1,
        serde_json::to_vec(&reply).unwrap(),
    );
    assert_eq!(port.try_result().unwrap(), ResultPoll::Completed(reply));
    assert!(!port.wants_write());
}

#[test]
fn a_reply_before_request_send_completes_is_invalid_not_a_partial_success() {
    let (mut port, mut peer) = admit();
    let mut request = request(1);
    request.text = "x".repeat(DATAGRAM_BYTES * 4);
    port.submit(request.clone()).unwrap();
    assert_eq!(port.try_result().unwrap(), ResultPoll::Pending);
    assert_eq!(peer.try_receive().unwrap(), Receive::Fragment);
    send_payload(
        &mut peer,
        Tag::Reply,
        1,
        serde_json::to_vec(&rejected(&request)).unwrap(),
    );
    let failure = port.try_result().unwrap_err();
    assert_component(&failure, Ste100ComponentCode::InvalidResult);
    assert_frozen(&mut port, &failure);
}

#[test]
fn eof_and_broken_fragment_framing_have_distinct_component_codes() {
    let (mut port, peer) = admit();
    drop(peer);
    let failure = port.try_result().unwrap_err();
    assert_component(&failure, Ste100ComponentCode::Crashed);
    assert_frozen(&mut port, &failure);

    let (mut port, peer) = admit();
    let datagram = [0_u8; HEADER_BYTES + 1];
    assert_eq!(
        send(
            peer.readiness().as_raw_fd(),
            &datagram,
            MsgFlags::MSG_NOSIGNAL
        ),
        Ok(datagram.len())
    );
    let failure = port.try_result().unwrap_err();
    assert_component(&failure, Ste100ComponentCode::TransportFailed);
    assert_frozen(&mut port, &failure);
}

#[test]
fn invalid_request_identity_fails_before_the_peer_receives_any_request() {
    for bad_scheme in [false, true] {
        let (mut port, mut peer) = admit();
        let mut request = request(1);
        if bad_scheme {
            request.scheme_id = "wrong".into();
        } else {
            request.request_id = 0.into();
        }
        let failure = port.submit(request).unwrap_err();
        assert_eq!(failure.kind, PortFailureKind::ProtocolFailed);
        assert!(matches!(peer.try_receive(), Err(Fault::Closed)));
        assert_frozen(&mut port, &failure);
    }
}

#[test]
fn complete_fragmented_reply_is_returned_only_after_its_last_fragment() {
    let (mut port, mut peer) = admit();
    let request = request(1);
    port.submit(request.clone()).unwrap();
    receive_request(&mut port, &mut peer);
    let mut reply = rejected(&request);
    let Ste100Result::NotSte100 { detail, .. } = &mut reply.result else {
        unreachable!()
    };
    *detail = "d".repeat(DATAGRAM_BYTES * 2);
    peer.begin_send(Tag::Reply, 1, serde_json::to_vec(&reply).unwrap())
        .unwrap();
    peer.try_send().unwrap();
    assert_eq!(port.try_result().unwrap(), ResultPoll::Pending);
    assert!(peer.has_pending_send());
    for _ in 0..ATTEMPTS {
        if peer.has_pending_send() {
            peer.try_send().unwrap();
        }
        match port.try_result().unwrap() {
            ResultPoll::Pending => {}
            ResultPoll::Completed(actual) => {
                assert_eq!(actual, reply);
                assert!(!peer.has_pending_send());
                assert_eq!(port.try_result().unwrap(), ResultPoll::Pending);
                return;
            }
        }
    }
    panic!("fragmented result never completed");
}
