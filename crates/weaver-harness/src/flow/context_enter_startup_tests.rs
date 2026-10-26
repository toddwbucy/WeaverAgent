//! The same original SCM_RIGHTS Enter drives the startup circuit below.
use super::*;
use crate::flow::pump::{
    LifecyclePeer,
    enter::{EnterReceiver, ReceivedEnter},
};
use nix::sys::socket::{
    AddressFamily, ControlMessage, MsgFlags, SockFlag, SockType, sendmsg, socketpair,
};
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::net::UnixStream;
use weaver_types::state_flow::enter::{EnterBinding, EnterEnvelope};

pub(in crate::flow::context) fn envelope(
    declaration: &s::FlowConfig,
    load: &weaver_trace::serving::payloads::LoadRecord,
    bootstrap: &s::Ste100Bootstrap,
    binary: &std::path::Path,
) -> EnterEnvelope {
    let mut value: serde_json::Value = serde_json::from_slice(include_bytes!(
        "../../../weaver-types/tests/fixtures/state-enter.json"
    ))
    .unwrap();
    let payload = &mut value["payload"]["body"]["payload"];
    payload["spu_instruction"]["decoder"]["model-binding"]["artifact"] =
        serde_json::json!("fixture-spu");
    payload["binding"]["gate_instruction"]["access-rule"] =
        serde_json::json!({"allowed-uids":[111],"allowed-gids":[222],"denied-uids":[333]});
    payload["session"] = serde_json::json!("session");
    payload["run"] = serde_json::json!("run");
    payload["operator"] = serde_json::json!(42);
    payload["declaration"] = serde_json::json!(load.state_start.config_sha256);
    payload["boundary"] = serde_json::json!("actual-enter-boundary");
    payload["cause"] = serde_json::json!({"uid":73});
    payload["library_path"] = serde_json::json!("/synthetic/engine-libraries");
    payload["reset"] = serde_json::json!({"prior_run":"prior-run","reason":"forced-unload"});
    payload["stack"] =
        serde_json::json!({binary.file_name().unwrap().to_str().unwrap():"e".repeat(64)});
    payload["binding"]["start"] = serde_json::to_value(&load.state_start).unwrap();
    payload["binding"]["ste100_scheme"] = serde_json::json!(bootstrap.scheme_text);
    payload["binding"]["state_management"] = serde_json::json!({
        "flow":declaration,"start":{"dump_sha256":load.state_start.dump_sha256},
        "persistence":load.state_flow.persistence,"reads":load.state_flow.reads,
        "ste100":bootstrap.instruction,"tools":load.state_flow.tools,"placement":load.state_flow.placement,
    });
    serde_json::from_value(value).unwrap()
}

pub(super) fn adopt(
    run: &mut Fixture,
    load: &weaver_trace::serving::payloads::LoadRecord,
    bootstrap: &s::Ste100Bootstrap,
    binary: &std::path::Path,
) -> (ReceivedEnter, OwnedFd) {
    let mut envelope = envelope(&run.context.declaration, load, bootstrap, binary);
    let bound = run.context.deadlines.get(PumpDeadline::Enter).unwrap();
    let weaver_types::state_flow::enter::EnterContent::Directive(
        weaver_types::state_flow::enter::EnterDirective::Enter { payload },
    ) = &mut envelope.payload;
    let EnterBinding::Serving { enter_window, .. } = &mut payload.binding else {
        panic!()
    };
    *enter_window = s::EnterWindow {
        began_ns: bound.began_ns().into(),
        deadline_ns: bound.deadline_ns().into(),
    };
    let bytes = envelope.encode().unwrap();
    let (admin, peer) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .unwrap();
    let (sink, reader) = UnixStream::pair().unwrap();
    let (state, member) = UnixStream::pair().unwrap();
    sendmsg::<()>(
        peer.as_raw_fd(),
        &[std::io::IoSlice::new(&bytes)],
        &[ControlMessage::ScmRights(&[
            sink.as_raw_fd(),
            state.as_raw_fd(),
        ])],
        MsgFlags::MSG_NOSIGNAL,
        None,
    )
    .unwrap();
    drop((sink, state));
    let mut receiver = EnterReceiver::from_peer(LifecyclePeer::from_owned(admin).unwrap())
        .ok()
        .unwrap();
    receiver.advance(&mut run.clock).unwrap();
    let original = receiver.into_received().ok().unwrap();
    assert_eq!(original.wire(), bytes);
    let (context, original) = original
        .adopt(&mut run.clock)
        .map_err(|f| f.detail)
        .unwrap()
        .into_parts();
    run.context = context;
    run.state = member;
    run._reader = reader;
    ORIGINAL_ADMIN.with(|slot| slot.set(peer.as_raw_fd()));
    (original, peer)
}

pub(super) fn assert_record(
    context: &RecordingContext,
    original: &ReceivedEnter,
    reader: &mut UnixStream,
) {
    reader.set_nonblocking(false).unwrap();
    reader
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let mut bytes = Vec::new();
    let mut byte = [0];
    while bytes.last() != Some(&b'\n') {
        reader.read_exact(&mut byte).unwrap();
        bytes.push(byte[0]);
    }
    let row: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(row["kind"], "load");
    let load = &row["payload"];
    let payload = original.envelope().enter();
    let EnterBinding::Serving {
        state_management,
        start,
        ste100_scheme,
        ..
    } = &payload.binding
    else {
        panic!()
    };
    assert_eq!(row["session"], payload.session.0);
    assert_eq!(row["run"], payload.run.0);
    assert_eq!(load["declaration"], payload.declaration);
    assert_eq!(load["boundary"], payload.boundary);
    assert_eq!(load["cause"], serde_json::to_value(payload.cause).unwrap());
    assert_eq!(
        load["library_path"],
        serde_json::to_value(&payload.library_path).unwrap()
    );
    assert_eq!(load["stack"], serde_json::to_value(&payload.stack).unwrap());
    assert_eq!(load["reset"], serde_json::to_value(&payload.reset).unwrap());
    assert_eq!(load["state_member"], true);
    assert_eq!(load["state_store"]["engine"], "sqlite");
    assert_eq!(load["state_start"], serde_json::to_value(start).unwrap());
    assert_eq!(load["ste100_scheme"], *ste100_scheme);
    assert_eq!(load["ste100_binary_sha256"], "e".repeat(64));
    assert_eq!(load["origin_ns"], context.origin_ns().to_string());
    assert_eq!(load["state_member_observation"]["pid"], 7);
    let mut flow = load["state_flow"].clone();
    assert_eq!(
        flow.as_object_mut().unwrap().remove("registry_id").unwrap(),
        "state-copy/1"
    );
    let expected = serde_json::to_value(state_management).unwrap();
    for key in ["persistence", "reads", "ste100", "tools", "placement"] {
        assert_eq!(
            flow.as_object_mut().unwrap().remove(key).unwrap(),
            expected[key],
            "{key}"
        );
    }
    assert_eq!(flow, expected["flow"]);
    assert_eq!(
        load["template_inventory"]["inventory_sha256"],
        inventory().digest().as_str()
    );
    reader.set_nonblocking(true).unwrap();
}

thread_local! {static ORIGINAL_ADMIN:std::cell::Cell<i32>=const {std::cell::Cell::new(-1)};}
pub(in crate::flow::context) fn close_original_peer() {
    ORIGINAL_ADMIN.with(|slot| {
        let fd = slot.get();
        assert!(fd >= 0);
        // The synchronous circuit retains this far-end owner above the child driver.
        nix::sys::socket::shutdown(fd, nix::sys::socket::Shutdown::Both).unwrap();
    });
}

pub(in crate::flow::context) fn original_peer_fd() -> i32 {
    ORIGINAL_ADMIN.with(|slot| {
        let fd = slot.get();
        assert!(fd >= 0);
        fd
    })
}
