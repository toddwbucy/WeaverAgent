use super::*;
use nix::sys::socket::{
    AddressFamily, ControlMessage, MsgFlags, SockFlag, SockType, send, sendmsg, socketpair,
};
use std::io::{IoSlice, Read};
use std::os::unix::net::UnixStream;
use std::time::Duration;
use weaver_types::state_flow::capture::*;

fn pair() -> (Coordination, std::os::fd::OwnedFd) {
    let (end, peer) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .unwrap();
    (Coordination::adopt(end), peer)
}
fn packet() -> CaptureEnvelope {
    CaptureEnvelope {
        exchange: weaver_types::ExchangeId {
            opener: weaver_types::Opener::Admin,
            ordinal: 5,
        },
        position: weaver_types::Position::Continue,
        payload: CapturePayload::SavePointCommit(CaptureIdentity {
            capture_id: 7.into(),
        }),
    }
}

#[test]
fn admin_capture_channel_retains_the_exact_exchange_and_original_bytes() {
    let (channel, peer) = pair();
    let expected = packet();
    let wire = expected.encode(CaptureSender::Harness).unwrap();
    send(peer.as_raw_fd(), &wire, MsgFlags::MSG_NOSIGNAL).unwrap();
    let windows = CaptureWindows::begin(Duration::from_secs(1), 100).unwrap();
    let received = channel.recv_capture(&windows).unwrap();
    assert_eq!(received.envelope, expected);
    assert_eq!(received.wire, wire);
    let mut reply = packet();
    reply.payload = CapturePayload::SavePointPublished(CaptureArtifact {
        capture_id: 7.into(),
        artifact_id: "fixture-artifact".into(),
        dump_sha256: weaver_types::state_flow::Digest::new("a".repeat(64)).unwrap(),
        config_sha256: weaver_types::state_flow::Digest::new("b".repeat(64)).unwrap(),
    });
    channel.send_capture(&reply, &windows).unwrap();
    let mut bytes = vec![0; MAX_ENVELOPE_BYTES];
    let count =
        nix::sys::socket::recv(peer.as_raw_fd(), &mut bytes, MsgFlags::MSG_DONTWAIT).unwrap();
    assert_eq!(&bytes[..count], reply.encode(CaptureSender::Admin).unwrap());
}

#[test]
fn admin_capture_channel_refuses_bad_direction_malformed_oversized_and_ancillary_packets() {
    for case in ["direction", "malformed", "oversized", "handles"] {
        let (channel, peer) = pair();
        let mut value = packet();
        if case == "direction" {
            value.payload = CapturePayload::SavePointImageChecked(CaptureImageChecked {
                capture_id: 7.into(),
                dump_sha256: weaver_types::state_flow::Digest::new("a".repeat(64)).unwrap(),
            });
        }
        let bytes = match case {
            "malformed" => b"{".to_vec(),
            "oversized" => vec![b'x'; MAX_ENVELOPE_BYTES + 1],
            _ => serde_json::to_vec(&value).unwrap(),
        };
        let (handle, mut watcher) = UnixStream::pair().unwrap();
        if case == "handles" {
            sendmsg::<()>(
                peer.as_raw_fd(),
                &[IoSlice::new(&bytes)],
                &[ControlMessage::ScmRights(&[handle.as_raw_fd()])],
                MsgFlags::MSG_NOSIGNAL,
                None,
            )
            .unwrap();
        } else {
            send(peer.as_raw_fd(), &bytes, MsgFlags::MSG_NOSIGNAL).unwrap();
        }
        drop(handle);
        let windows = CaptureWindows::begin(Duration::from_secs(1), 100).unwrap();
        assert!(channel.recv_capture(&windows).is_err(), "{case}");
        assert_eq!(watcher.read(&mut [0]).unwrap(), 0);
    }
}

#[test]
fn admin_capture_channel_never_renews_a_silent_or_pressured_wait() {
    for pressure in [false, true] {
        let (channel, _peer) = pair();
        if pressure {
            loop {
                match send(
                    channel.fd.as_raw_fd(),
                    b"pressure",
                    MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_NOSIGNAL,
                ) {
                    Ok(_) => (),
                    Err(nix::errno::Errno::EAGAIN) => break,
                    Err(error) => panic!("{error}"),
                }
            }
        }
        let windows = CaptureWindows::begin(Duration::from_millis(20), 100).unwrap();
        let fault = if pressure {
            let mut value = packet();
            value.payload = CapturePayload::SavePointAborted(CaptureAborted {
                capture_id: 7.into(),
                code: "fixture".into(),
                detail: "full socket".into(),
            });
            channel.send_capture(&value, &windows).unwrap_err()
        } else {
            channel.recv_capture(&windows).unwrap_err()
        };
        assert_eq!(fault, ChannelFault::Unanswered);
        assert_eq!(windows.check(), Err(ChannelFault::Unanswered));
    }
}

#[test]
fn admin_capture_channel_decode_time_spends_the_original_window() {
    let (channel, peer) = pair();
    send(
        peer.as_raw_fd(),
        &packet().encode(CaptureSender::Harness).unwrap(),
        MsgFlags::MSG_NOSIGNAL,
    )
    .unwrap();
    let windows = CaptureWindows::begin(Duration::from_millis(200), 100).unwrap();
    let decoded = std::cell::Cell::new(false);
    let answer = channel.recv_capture_with(&windows, |bytes| {
        let value = CaptureEnvelope::decode(bytes, CaptureSender::Harness)
            .map_err(|_| ChannelFault::Undecodable)?;
        decoded.set(true);
        std::thread::sleep(Duration::from_millis(250));
        Ok(value)
    });
    assert!(decoded.get());
    assert_eq!(answer.unwrap_err(), ChannelFault::Unanswered);
}

#[test]
fn graceful_capture_channel_waits_past_tail_and_save_before_beginning() {
    let (channel, peer) = pair();
    let windows = CaptureWindows::graceful(Duration::from_millis(10), None, None, 1).unwrap();
    let start = std::time::Instant::now();
    let worker = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(60));
        send(
            peer.as_raw_fd(),
            &packet().encode(CaptureSender::Harness).unwrap(),
            MsgFlags::MSG_NOSIGNAL,
        )
        .unwrap();
    });
    let received = channel.recv_capture(&windows);
    worker.join().unwrap();
    assert_eq!(received.unwrap().envelope, packet());
    assert!(start.elapsed() >= Duration::from_millis(60));
    assert!(windows.remaining().unwrap().is_none());
}

#[test]
fn capture_channel_matches_original_graceful_or_forced_direction() {
    for forced in [false, true] {
        let (channel, peer) = pair();
        let windows = if forced {
            CaptureWindows::begin(Duration::from_secs(1), 100).unwrap()
        } else {
            CaptureWindows::graceful(Duration::from_secs(1), None, None, 100).unwrap()
        };
        let mut request = weaver_types::OrganEnvelope {
            exchange: packet().exchange,
            position: weaver_types::Position::Open,
            payload: weaver_types::Payload::Directive(weaver_types::LifecycleDirective::Leave {
                cause: weaver_types::Cause { uid: 73 },
                forced: !forced,
                rollback: false,
            }),
        };
        assert_eq!(
            channel.send_capture_leave(&request, &windows),
            Err(ChannelFault::Undecodable)
        );
        let weaver_types::Payload::Directive(weaver_types::LifecycleDirective::Leave {
            forced: flag,
            ..
        }) = &mut request.payload
        else {
            unreachable!()
        };
        *flag = forced;
        channel.send_capture_leave(&request, &windows).unwrap();
        let mut bytes = vec![0; MAX_ENVELOPE_BYTES];
        let count =
            nix::sys::socket::recv(peer.as_raw_fd(), &mut bytes, MsgFlags::MSG_DONTWAIT).unwrap();
        let actual: weaver_types::OrganEnvelope = serde_json::from_slice(&bytes[..count]).unwrap();
        assert_eq!(actual, request);
    }
}

#[test]
fn capture_reader_retains_whole_commit_when_decode_crosses_the_deadline() {
    use crate::serving_capture_window::with_test_times;
    let (channel, peer) = pair();
    let expected = packet();
    let wire = expected.encode(CaptureSender::Harness).unwrap();
    send(peer.as_raw_fd(), &wire, MsgFlags::MSG_NOSIGNAL).unwrap();
    let windows = with_test_times(&[100], || {
        CaptureWindows::begin(Duration::from_nanos(10), 100).unwrap()
    });
    let read = with_test_times(&[101, 102, 110], || {
        channel.recv_capture_retained_with(&windows, |bytes| {
            let envelope = CaptureEnvelope::decode(bytes, CaptureSender::Harness).unwrap();
            // The scripted third reading expires after this actual packet decode.
            Ok(envelope)
        })
    });
    let read = read.unwrap();
    assert_eq!(read.received.envelope, expected);
    assert_eq!(read.received.wire, wire);
    assert_eq!(read.timing, Err(ChannelFault::Unanswered));
}

#[test]
fn capture_send_retains_physical_carry_when_the_post_send_deadline_expires() {
    use crate::serving_capture_window::with_test_times;
    let (channel, peer) = pair();
    let windows = with_test_times(&[100], || {
        CaptureWindows::begin(Duration::from_nanos(10), 100).unwrap()
    });
    let envelope = CaptureEnvelope {
        payload: CapturePayload::SavePointAborted(CaptureAborted {
            capture_id: 7.into(),
            code: "fixture".into(),
            detail: "bounded cancellation".into(),
        }),
        ..packet()
    };
    let mut sent = false;
    let result = with_test_times(&[101, 102, 110], || {
        channel.send_capture_tracked(&envelope, &windows, &mut sent)
    });
    assert_eq!(result, Err(ChannelFault::Unanswered));
    assert!(
        sent,
        "post-send expiry must retain the whole physical carry"
    );
    let mut bytes = vec![0; MAX_ENVELOPE_BYTES];
    let count =
        nix::sys::socket::recv(peer.as_raw_fd(), &mut bytes, MsgFlags::MSG_DONTWAIT).unwrap();
    assert_eq!(
        &bytes[..count],
        envelope.encode(CaptureSender::Admin).unwrap()
    );
}
