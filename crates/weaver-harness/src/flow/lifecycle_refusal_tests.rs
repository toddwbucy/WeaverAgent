use super::*;
use crate::flow::pump_transport::{OrganPort, SendProgress, Stage};
use nix::errno::Errno;
use nix::sys::socket::{AddressFamily, MsgFlags, SockFlag, SockType, recv, socketpair};
use std::os::fd::{AsRawFd, OwnedFd};
use weaver_types::state_flow::STE100_PROTOCOL;
use weaver_types::{Opener, RefusalRecord};

fn exchange() -> ExchangeId {
    ExchangeId {
        opener: Opener::Admin,
        ordinal: u64::MAX,
    }
}
fn source(code: Ste100AdmissionCode, detail: &str) -> Ste100AdmissionRefused {
    Ste100AdmissionRefused {
        protocol: STE100_PROTOCOL.into(),
        code,
        detail: detail.into(),
    }
}
fn port() -> (OrganPort, OwnedFd) {
    let (near, peer) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .unwrap();
    (OrganPort::from_owned(near).unwrap(), peer)
}
fn receive(peer: &OwnedFd) -> Option<Vec<u8>> {
    let mut bytes = vec![0; MAX_ENVELOPE_BYTES + 1];
    match recv(
        peer.as_raw_fd(),
        &mut bytes,
        MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_TRUNC,
    ) {
        Ok(count) => {
            assert!(count <= bytes.len());
            bytes.truncate(count);
            Some(bytes)
        }
        Err(Errno::EAGAIN) => None,
        other => panic!("actual packet receive: {other:?}"),
    }
}
fn send(port: &mut OrganPort) {
    for _ in 0..128 {
        if port.try_send().unwrap() == SendProgress::Complete {
            return;
        }
    }
    panic!("bounded fixture did not send the original packet")
}
fn whole(source: &Ste100AdmissionRefused) -> OrganEnvelope {
    match prepare_admission_refusal(&exchange(), source) {
        Ok(envelope) => envelope,
        Err(AdmissionRefusalError::Envelope {
            envelope,
            fault: ChannelFault::Truncated { .. },
        }) => envelope,
        other => panic!("unexpected mapping outcome: {other:?}"),
    }
}
fn at_size(size: usize, prefix: &str) -> Ste100AdmissionRefused {
    let mut original = source(Ste100AdmissionCode::UnsupportedArtifact, prefix);
    let initial = serde_json::to_vec(&whole(&original)).unwrap().len();
    assert!(initial <= size);
    original.detail.push_str(&"x".repeat(size - initial));
    assert_eq!(serde_json::to_vec(&whole(&original)).unwrap().len(), size);
    original
}

#[test]
fn admission_refusal_mapping_keeps_all_five_checker_codes_exact_detail_and_correlation() {
    for (checker, expected) in [
        (
            Ste100AdmissionCode::SchemeMismatch,
            StateManagementCode::SchemeMismatch,
        ),
        (
            Ste100AdmissionCode::UnsupportedArtifact,
            StateManagementCode::TemplateFailed,
        ),
        (
            Ste100AdmissionCode::InvalidGrammar,
            StateManagementCode::TemplateFailed,
        ),
        (
            Ste100AdmissionCode::InsufficientCapacity,
            StateManagementCode::TemplateFailed,
        ),
        (
            Ste100AdmissionCode::InvalidInstruction,
            StateManagementCode::TemplateFailed,
        ),
    ] {
        let detail = "  failed reference /tmp/model\\weights: \"exact\"\t\n\u{e9}  ";
        let source = source(checker, detail);
        let before = serde_json::to_vec(&source).unwrap();
        let envelope = prepare_admission_refusal(&exchange(), &source).unwrap();
        assert_eq!(envelope.exchange, exchange());
        assert_eq!(envelope.position, Position::Close);
        let Payload::Refusal(refusal) = envelope.payload else {
            panic!("no answer or Ready is licensed")
        };
        let LifecycleRefusal::StateManagementInvalid {
            stage,
            code,
            rule_id,
            path,
            detail: mapped,
        } = &refusal
        else {
            panic!("wrong floor refusal")
        };
        assert_eq!(*stage, StateManagementStage::Template);
        assert_eq!(*code, expected);
        assert_eq!(rule_id, &None);
        assert_eq!(path, &None, "checker prose is not structured path evidence");
        let spelling = serde_json::to_value(checker).unwrap();
        assert_eq!(mapped, &format!("{}: {detail}", spelling.as_str().unwrap()));
        assert!(mapped.ends_with(detail));
        assert_eq!(serde_json::to_vec(&source).unwrap(), before);
        // The existing trace carrier embeds this same floor refusal; this value
        // roundtrip is not claimed to perform a Recorded author admission.
        let record = RefusalRecord::Lifecycle {
            asked: weaver_types::LifecycleAsk::Enter,
            refusal,
        };
        assert_eq!(
            serde_json::from_slice::<RefusalRecord>(&serde_json::to_vec(&record).unwrap()).unwrap(),
            record
        );
    }
}

#[test]
fn admission_refusal_mapping_preserves_real_no_compiled_adapter_reason_and_rejects_invalid_sources()
{
    let detail = "no compatible STE100 adapter is compiled into this executable";
    let original = source(Ste100AdmissionCode::UnsupportedArtifact, detail);
    let envelope = prepare_admission_refusal(&exchange(), &original).unwrap();
    let Payload::Refusal(LifecycleRefusal::StateManagementInvalid { detail: mapped, .. }) =
        envelope.payload
    else {
        panic!("wrong mapping")
    };
    assert_eq!(mapped, format!("unsupported_artifact: {detail}"));
    // The exact real producer wording is pinned here as a value control. This
    // is not a fork/admission or complete healthy rollback qualification test.
    for bad in [
        Ste100AdmissionRefused {
            protocol: "ste100/other".into(),
            ..original.clone()
        },
        source(Ste100AdmissionCode::InvalidGrammar, ""),
    ] {
        let held = serde_json::to_vec(&bad).unwrap();
        assert!(matches!(
            prepare_admission_refusal(&exchange(), &bad),
            Err(AdmissionRefusalError::InvalidSource(_))
        ));
        assert_eq!(serde_json::to_vec(&bad).unwrap(), held);
    }
}

#[test]
fn admission_refusal_mapping_exact_complete_envelope_crosses_actual_packet_and_one_over_returns_intact()
 {
    let exact = at_size(MAX_ENVELOPE_BYTES, "preserve /exact/path ");
    let envelope = prepare_admission_refusal(&exchange(), &exact).unwrap();
    let bytes = serde_json::to_vec(&envelope).unwrap();
    assert_eq!(bytes.len(), MAX_ENVELOPE_BYTES);
    let (mut transport, peer) = port();
    assert_eq!(transport.stage(envelope.clone()).unwrap(), Stage::Queued);
    send(&mut transport);
    assert_eq!(receive(&peer).unwrap(), bytes);
    assert!(receive(&peer).is_none());
    let too_long = at_size(MAX_ENVELOPE_BYTES + 1, "preserve /exact/path ");
    let before = serde_json::to_vec(&too_long).unwrap();
    let Err(AdmissionRefusalError::Envelope {
        envelope: returned,
        fault,
    }) = prepare_admission_refusal(&exchange(), &too_long)
    else {
        panic!("whole envelope must refuse one byte over")
    };
    assert_eq!(
        fault,
        ChannelFault::Truncated {
            bound: MAX_ENVELOPE_BYTES
        }
    );
    assert_eq!(
        serde_json::to_vec(&returned).unwrap().len(),
        MAX_ENVELOPE_BYTES + 1
    );
    assert_eq!(serde_json::to_vec(&too_long).unwrap(), before);
    assert_eq!(returned.exchange, exchange());
    assert_eq!(returned, whole(&too_long));
    let (mut rejecting, untouched) = port();
    let rejected = rejecting.stage(returned.clone()).unwrap_err();
    assert_eq!(rejected.input, returned);
    assert_eq!(rejected.failure.fault, fault);
    assert!(!rejecting.send_pending());
    assert!(
        receive(&untouched).is_none_or(|bytes| bytes.is_empty()),
        "no shortened or substitute refusal may be sent"
    );
}

#[test]
fn admission_refusal_mapping_counts_wrapper_escaping_and_utf8_bytes_not_source_characters() {
    for detail in [
        "\"".repeat(MAX_ENVELOPE_BYTES / 2),
        "\u{e9}".repeat(MAX_ENVELOPE_BYTES / 2),
    ] {
        let original = source(Ste100AdmissionCode::InvalidInstruction, &detail);
        assert!(original.detail.chars().count() < MAX_ENVELOPE_BYTES);
        let Err(AdmissionRefusalError::Envelope { envelope, fault }) =
            prepare_admission_refusal(&exchange(), &original)
        else {
            panic!("escaped complete envelope must exceed limit")
        };
        assert_eq!(
            fault,
            ChannelFault::Truncated {
                bound: MAX_ENVELOPE_BYTES
            }
        );
        assert!(serde_json::to_vec(&envelope).unwrap().len() > MAX_ENVELOPE_BYTES);
        let Payload::Refusal(LifecycleRefusal::StateManagementInvalid {
            detail: held, path, ..
        }) = envelope.payload
        else {
            panic!("original refusal retained")
        };
        assert_eq!(held, format!("invalid_instruction: {detail}"));
        assert!(path.is_none());
    }
    // At one byte over, the raw and mapped detail are both below the limit;
    // only sizing the complete correlated wrapper detects the excess.
    let original = at_size(MAX_ENVELOPE_BYTES + 1, "wrapper test ");
    assert!(original.detail.len() + "unsupported_artifact: ".len() < MAX_ENVELOPE_BYTES);
    assert!(matches!(
        prepare_admission_refusal(&exchange(), &original),
        Err(AdmissionRefusalError::Envelope {
            fault: ChannelFault::Truncated { .. },
            ..
        })
    ));
}

#[test]
fn admission_refusal_mapping_actual_busy_transport_keeps_first_and_returns_second_envelope() {
    let first = prepare_admission_refusal(
        &exchange(),
        &source(Ste100AdmissionCode::InvalidGrammar, "first exact refusal"),
    )
    .unwrap();
    let next_exchange = ExchangeId {
        opener: Opener::Admin,
        ordinal: 7,
    };
    let second = prepare_admission_refusal(
        &next_exchange,
        &source(Ste100AdmissionCode::SchemeMismatch, "second exact refusal"),
    )
    .unwrap();
    let (mut transport, peer) = port();
    assert_eq!(transport.stage(first.clone()).unwrap(), Stage::Queued);
    let Stage::Busy(returned) = transport.stage(second.clone()).unwrap() else {
        panic!("second original must return")
    };
    assert_eq!(returned, second);
    send(&mut transport);
    let observed: OrganEnvelope = serde_json::from_slice(&receive(&peer).unwrap()).unwrap();
    assert_eq!(observed, first);
    assert!(receive(&peer).is_none());
    assert_eq!(transport.stage(returned).unwrap(), Stage::Queued);
    send(&mut transport);
    let observed: OrganEnvelope = serde_json::from_slice(&receive(&peer).unwrap()).unwrap();
    assert_eq!(observed, second);
    assert!(receive(&peer).is_none());
}
