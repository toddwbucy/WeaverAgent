use super::*;
use crate::save_points::serving::repository::tests::verified_enter_fixture;
use crate::seed::preparation::tests::fixture;
use std::fs;

fn facts() -> RuntimeFacts {
    RuntimeFacts {
        enter_window: weaver_types::state_flow::EnterWindow {
            began_ns: 0.into(),
            deadline_ns: 1_000_000_000_000_000_000.into(),
        },
        run: RunId("composed-run".into()),
        reset: None,
        stack: BTreeMap::new(),
        cause: Cause { uid: 0 },
    }
}
#[test]
fn replacement_enter_producer_uses_verified_settings_and_exact_checked_root_bytes() {
    let fixture = fixture();
    let source = fs::read_to_string(fixture.config.territory.join("agent.toml")).unwrap();
    let draft = weaver_types::parse_serving_draft(&source).unwrap();
    let verified = verified_enter_fixture(draft.bind_start(hash(b"placeholder")));
    let expected = verified.declaration().clone();
    let identity = verified.identity();
    let original_checker = fs::read_to_string(fixture.config.root.join("ste100-binary")).unwrap();
    let renamed_checker =
        PathBuf::from(original_checker.trim()).with_file_name("checked-language-program");
    fs::rename(original_checker.trim(), &renamed_checker).unwrap();
    fs::write(
        fixture.config.root.join("ste100-binary"),
        format!("{}\n", renamed_checker.display()),
    )
    .unwrap();
    let original_scheme =
        fs::read_to_string(fixture.config.root.join("ste100-scheme.json")).unwrap();
    // Load preparation must never consult the draft, prompt or certificate.
    for name in [
        "agent.toml",
        "system-prompt.md",
        "system-prompt-certificate.json",
    ] {
        fs::write(
            fixture.config.territory.join(name),
            b"changed after publication",
        )
        .unwrap();
    }
    let prepared = PreparedEnter::prepare_against(
        &fixture.config,
        &AgentName(fixture.config.agent.clone()),
        verified,
        facts(),
        7,
        &fixture.boundary,
    )
    .map_err(|e| e.to_string())
    .unwrap();
    let payload = prepared.envelope().enter();
    let EnterBinding::Serving {
        state_management,
        start,
        ste100_scheme,
        ..
    } = &payload.binding
    else {
        panic!()
    };
    assert_eq!(state_management, expected.state_management());
    assert_eq!(start, &identity);
    assert_eq!(payload.declaration, identity.config_sha256.to_string());
    assert_eq!(ste100_scheme, &original_scheme);
    assert_eq!(payload.spu_instruction, expected.agent().spu_instruction);
    assert_eq!(prepared.inventory().config, *expected.agent());
    let executable_hash = hash(&fs::read(prepared.checker_binary()).unwrap());
    assert_eq!(
        payload.stack["checked-language-program"],
        executable_hash.to_string()
    );
    assert_ne!(
        executable_hash,
        state_management.ste100.model_artifact_sha256
    );
    let LoadedStateFacts::Serving {
        ste100_binary_sha256,
        ..
    } = prepared.loaded_state()
    else {
        panic!()
    };
    assert_eq!(ste100_binary_sha256, executable_hash);
    // Send from the producer after the root scheme changes: no later read may
    // replace the checked instance, and the descriptor pair stays in order.
    fs::write(
        fixture.config.root.join("ste100-scheme.json"),
        b"later edit",
    )
    .unwrap();
    use nix::sys::socket::{
        AddressFamily, ControlMessageOwned, MsgFlags, SockFlag, SockType, recvmsg, socketpair,
    };
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
    let (end, peer) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .unwrap();
    let (sink, _sink_peer) = std::os::unix::net::UnixStream::pair().unwrap();
    let (state, _state_peer) = std::os::unix::net::UnixStream::pair().unwrap();
    prepared
        .send(&Coordination::adopt(end), sink.as_fd(), state.as_fd())
        .unwrap();
    let mut bytes = vec![0; weaver_types::MAX_ENVELOPE_BYTES];
    let mut control = nix::cmsg_space!([i32; 2]);
    let (count, handles) = {
        let mut slices = [std::io::IoSliceMut::new(&mut bytes)];
        let message = recvmsg::<()>(
            peer.as_raw_fd(),
            &mut slices,
            Some(&mut control),
            MsgFlags::MSG_CMSG_CLOEXEC,
        )
        .unwrap();
        let mut handles = Vec::new();
        for cmsg in message.cmsgs().unwrap() {
            if let ControlMessageOwned::ScmRights(raw) = cmsg {
                for fd in raw {
                    // SAFETY: recvmsg installed each fresh descriptor exactly once.
                    handles.push(unsafe { OwnedFd::from_raw_fd(fd) });
                }
            }
        }
        (message.bytes, handles)
    };
    assert_eq!(
        EnterEnvelope::decode(&bytes[..count]).unwrap(),
        *prepared.envelope()
    );
    assert_eq!(handles.len(), 2);
    for (actual, expected) in handles.iter().zip([sink.as_fd(), state.as_fd()]) {
        assert_eq!(
            nix::sys::stat::fstat(actual).unwrap().st_ino,
            nix::sys::stat::fstat(expected).unwrap().st_ino
        );
    }
    // Verification's held descriptor survives path removal by the fixture.
    let mut bytes = [0; 128];
    let count = nix::unistd::read(prepared.dump(), &mut bytes).unwrap();
    assert_eq!(
        &bytes[..count],
        b"synthetic member image, not SQLite qualification"
    );
}
#[test]
fn replacement_enter_producer_refuses_changed_scheme_identity_and_custody() {
    for case in ["digest", "verbs", "mode", "binary-conflict"] {
        let fixture = fixture();
        let source = fs::read_to_string(fixture.config.territory.join("agent.toml")).unwrap();
        let mut draft_source = source.clone();
        if case == "verbs" {
            draft_source = source.replace("verbs = [\"read\"]", "verbs = [\"hold\"]");
            assert_ne!(source, draft_source);
        }
        let verified = verified_enter_fixture(
            weaver_types::parse_serving_draft(&draft_source)
                .unwrap()
                .bind_start(hash(b"placeholder")),
        );
        let path = fixture.config.root.join("ste100-scheme.json");
        if case == "digest" {
            let mut bytes = fs::read(&path).unwrap();
            bytes.push(b' ');
            fs::write(&path, bytes).unwrap();
        }
        if case == "mode" {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        }
        let mut runtime = facts();
        if case == "binary-conflict" {
            runtime
                .stack
                .insert("weaver-spu-ste100".into(), "conflicting digest".into());
        }
        assert!(
            PreparedEnter::prepare_against(
                &fixture.config,
                &AgentName(fixture.config.agent.clone()),
                verified,
                runtime,
                7,
                &fixture.boundary
            )
            .is_err(),
            "{case}"
        );
    }
}

const _: fn() = || {
    trait NoClone<M> {
        fn check() {}
    }
    impl<T: ?Sized> NoClone<()> for T {}
    impl<T: Clone> NoClone<u8> for T {}
    let _ = <PreparedEnter as NoClone<_>>::check;
};
