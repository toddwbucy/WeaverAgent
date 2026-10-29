use super::*;
use crate::flow::plan::AcceptedPlanUpdate;
use nix::sys::socket::{AddressFamily, MsgFlags, SockType, send, socketpair};
use sha2::{Digest as _, Sha256};

#[test]
fn refused_plan_packet_preserves_exact_wire_after_peer_eof_without_granting_admission() {
    let (mut owner, other, wire) = received(" { \"unparsed\": true }\n", 42, false);
    let envelope = owner.held().unwrap().envelope.clone();
    drop(other);
    let refused = owner.take_refused_plan_update().unwrap().unwrap();
    assert_eq!(refused.wire(), wire);
    assert_eq!(refused.envelope(), &envelope);
    assert!(owner.held().is_none());
    assert!(owner.wire.is_none());
    assert!(owner.take_root_plan_update().ok().flatten().is_none());
}

#[test]
fn refused_plan_packet_cannot_take_an_opener_with_descriptors() {
    let (mut owner, _other, wire) = received(&document(), 0, false);
    owner.held.as_mut().unwrap().sink = Some(std::fs::File::open("/dev/null").unwrap().into());
    assert!(owner.take_refused_plan_update().is_err());
    assert_eq!(owner.wire.as_deref(), Some(wire.as_slice()));
    assert!(owner.held().unwrap().sink.is_some());
}

pub(in crate::flow) fn document() -> String {
    serde_json::json!({"format":"state-plan-update/1","expected":null,"operation":{
        "create":{"task":"Read records.","steps":[{"id":null,"instruction":"Read input."},{"id":null,"instruction":"Send output."}],
        "categories":[],"groups":[],"review":{"setup_id":"review","sha256":"a".repeat(64)}}
    }}).to_string()
}
pub(in crate::flow) fn received(
    document: &str,
    cause: u32,
    bad_digest: bool,
) -> (LifecyclePeer, OwnedFd, Vec<u8>) {
    let (fd, other) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_NONBLOCK | SockFlag::SOCK_CLOEXEC,
    )
    .unwrap();
    let mut owner = LifecyclePeer::from_owned(fd).unwrap();
    owner.enable_serving_openers().unwrap();
    let digest = if bad_digest {
        "0".repeat(64)
    } else {
        format!("{:x}", Sha256::digest(document.as_bytes()))
    };
    let packet = OrganEnvelope {
        exchange: weaver_types::ExchangeId {
            opener: Opener::Admin,
            ordinal: 2,
        },
        position: Position::Open,
        payload: Payload::Directive(LifecycleDirective::PlanUpdate {
            cause: weaver_types::Cause { uid: cause },
            document_sha256: weaver_types::state_flow::Digest::new(digest).unwrap(),
            document: document.into(),
        }),
    };
    let wire = serde_json::to_vec_pretty(&packet).unwrap();
    assert_eq!(
        send(other.as_raw_fd(), &wire, MsgFlags::MSG_NOSIGNAL).unwrap(),
        wire.len()
    );
    assert_eq!(owner.try_receive().unwrap(), ReceiveProgress::Held);
    (owner, other, wire)
}
pub(in crate::flow) fn packet(document: &str) -> RootPlanPacket {
    assert_eq!(
        nix::unistd::geteuid().as_raw(),
        0,
        "actual root peer instrument"
    );
    let (mut owner, _other, _) = received(document, 0, false);
    owner.take_root_plan_update().unwrap().unwrap()
}

#[test]
fn root_plan_packet_checks_real_peer_credentials_before_transferring() {
    let (mut owner, _other, wire) = received(&document(), 0, false);
    if nix::unistd::geteuid().as_raw() == 0 {
        let packet = owner.take_root_plan_update().unwrap().unwrap();
        assert_eq!(packet.wire(), wire);
    } else {
        assert!(matches!(
            owner.take_root_plan_update(),
            Err(PlanReceiveError::Refused(LifecycleRefusal::Unauthorized))
        ));
        assert_eq!(owner.wire.as_deref(), Some(wire.as_slice()));
        assert!(owner.held().is_some());
    }
}

#[test]
#[ignore = "actual root coordination peer"]
fn root_plan_packet_retains_exact_bytes_and_refuses_nonroot_cause_without_minting() {
    assert_eq!(nix::unistd::geteuid().as_raw(), 0);
    let raw = format!("\n {} \n", document());
    let (mut owner, other, wire) = received(&raw, 0, false);
    drop(other);
    let original = owner.take_root_plan_update().unwrap().unwrap();
    assert_eq!(original.wire(), wire);
    let accepted = AcceptedPlanUpdate::from_received(original).ok().unwrap();
    assert_eq!(accepted.packet().input().2, raw);
    assert_eq!(accepted.context().cause.uid, 0);
    assert_eq!(accepted.source().cause().uid, 0);
    assert_eq!(
        serde_json::to_value(accepted.message()).unwrap(),
        serde_json::json!({"role":"user","content":[
            {"type":"text","text":"Read records."},{"type":"text","text":"Read input."},{"type":"text","text":"Send output."}
        ]})
    );
    let weaver_types::state_flow::PlanInputOperation::Create { task, steps, .. } =
        &accepted.context().operation
    else {
        panic!("create projection")
    };
    assert_eq!(task.0, 0);
    assert_eq!(steps.iter().map(|s| s.block.0).collect::<Vec<_>>(), [1, 2]);
    let (mut owner, _other, wire) = received(&raw, 1000, false);
    assert!(matches!(
        owner.take_root_plan_update(),
        Err(PlanReceiveError::Refused(LifecycleRefusal::Unauthorized))
    ));
    assert_eq!(owner.wire.as_deref(), Some(wire.as_slice()));
}

#[test]
#[ignore = "actual root coordination peer"]
fn root_plan_packet_digest_and_strict_shape_refusals_return_original_custody() {
    for (raw, bad_digest) in [
        (document(), true),
        ("[]".into(), false),
        (
            document().replace("\"format\":", "\"extra\":0,\"format\":"),
            false,
        ),
    ] {
        let (mut owner, _other, wire) = received(&raw, 0, bad_digest);
        let original = owner.take_root_plan_update().unwrap().unwrap();
        let (original, refusal) = AcceptedPlanUpdate::from_received(original)
            .err()
            .expect("malformed input must not grant");
        assert_eq!(original.wire(), wire);
        assert_eq!(original.input().2, raw);
        assert_eq!(
            refusal.case,
            weaver_types::state_flow::PlanUpdateCase::MalformedPlan
        );
    }
}

const _: fn() = || {
    trait NoClone<Marker> {
        fn check() {}
    }
    impl<T: ?Sized> NoClone<()> for T {}
    impl<T: Clone> NoClone<u8> for T {}
    let _ = <RootPlanPacket as NoClone<_>>::check;
    let _ = <AcceptedPlanUpdate as NoClone<_>>::check;
};

#[test]
#[ignore = "actual root coordination peer"]
fn root_plan_packet_projection_preserves_replacement_ids_and_has_no_unused_or_aliased_blocks() {
    use weaver_types::state_flow as s;
    let seed = "a".repeat(32);
    for answer_break in [false, true] {
        for task in [None, Some("Keep the task.")] {
            let id = serde_json::json!({"seed_id":seed,"local_id":"7"});
            let replacement = serde_json::json!({"task":task,"steps":[
                {"id":id,"instruction":"Read input."},{"id":null,"instruction":"Send output."}],
                "categories":[],"groups":[],"review":{"setup_id":"review","sha256":"a".repeat(64)},"preserved_closures":[id]});
            let operation = if answer_break {
                serde_json::json!({"answer_break":{"replacement":replacement}})
            } else {
                serde_json::json!({"replace":replacement})
            };
            let raw = serde_json::json!({"format":"state-plan-update/1","expected":{
                "plan_id":{"seed_id":seed,"local_id":"2"},"revision":"3"},"operation":operation})
            .to_string();
            let accepted = AcceptedPlanUpdate::from_received(packet(&raw))
                .ok()
                .unwrap();
            let context = match &accepted.context().operation {
                s::PlanInputOperation::Replace(value)
                | s::PlanInputOperation::AnswerBreak { replacement: value } => value,
                _ => panic!("replacement projection"),
            };
            assert_eq!(context.task.map(|n| n.0), task.map(|_| 0));
            let start = u64::from(task.is_some());
            assert_eq!(
                context.steps.iter().map(|s| s.block.0).collect::<Vec<_>>(),
                [start, start + 1]
            );
            assert_eq!(context.steps[0].id.as_ref().unwrap().local_id.0, 7);
            assert!(context.steps[1].id.is_none());
            assert_eq!(context.preserved_closures[0].local_id.0, 7);
            assert_eq!(accepted.message().content.len() as u64, start + 2);
            assert_eq!(accepted.packet().input().2, raw);
            assert_eq!(
                serde_json::to_value(crate::flow::mirror::plan_input(accepted.context())).unwrap(),
                serde_json::to_value(accepted.context()).unwrap()
            );
        }
    }
}
