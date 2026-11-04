use super::*;

use crate::seed::preparation::tests::fixture;
use std::fs;

fn verified_fixture(
    fixture: &crate::seed::preparation::tests::Fixture,
    bound: weaver_types::ServingDeclaration,
) -> VerifiedArtifact {
    let scheme = fs::read_to_string(fixture.config.root.join("ste100-scheme.json")).unwrap();
    let thesaurus =
        bound.state_management().flow.thesaurus.as_ref().map(|_| {
            fs::read_to_string(fixture.config.root.join("ste100-thesaurus.json")).unwrap()
        });
    crate::save_points::serving::repository::tests::verified_enter_fixture(bound, scheme, thesaurus)
}

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
fn classifier_free_binding_does_not_activate_classifier_only_startup() {
    let mut fixture = fixture();
    fixture.config.operator = 0;
    let source = fs::read_to_string(fixture.config.territory.join("agent.toml")).unwrap();
    let source = source
        .lines()
        .filter(|line| !line.starts_with("state-management.ste100."))
        .collect::<Vec<_>>()
        .join("\n");
    let draft = weaver_types::parse_serving_draft(&source).unwrap();
    let verified = verified_fixture(&fixture, draft.bind_start(hash(b"placeholder")));
    fs::remove_file(fixture.config.root.join("ste100-binary")).unwrap();
    let error = PreparedEnter::prepare_against(
        &fixture.config,
        &AgentName(fixture.config.agent.clone()),
        verified,
        facts(),
        7,
        &fixture.boundary,
    )
    .err()
    .expect("classifier-only startup refuses");
    assert!(matches!(error, PreparationFailure::Inventory {
        refusal: weaver_types::LifecycleRefusal::ConfigInvalid { field: Some(ref name) }, ..
    } if name.0 == "state-management.ste100"));
}

#[test]
fn state_content_1_load_refuses_a_thesaurus_before_root_artifact_reads() {
    let mut fixture = fixture();
    fixture.config.operator = 0;
    let source = fs::read_to_string(fixture.config.territory.join("agent.toml")).unwrap();
    let draft = weaver_types::parse_serving_draft(&source).unwrap();
    let thesaurus = serde_json::json!({
        "format": "ste100-thesaurus/1", "scheme_id": draft.state_management().flow.scheme.scheme_id,
        "source": {"repository":"synthetic", "commit":"test", "path":"test", "sha256":hash(b"source")},
        "entries": []
    }).to_string();
    fs::write(
        fixture.config.root.join("ste100-thesaurus.json"),
        &thesaurus,
    )
    .unwrap();
    let source = format!(
        "{source}\nstate-management.flow.thesaurus = \
         {{artifact-sha256 = \"{}\", scheme-id = \"{}\"}}\n",
        hash(thesaurus.as_bytes()),
        draft.state_management().flow.scheme.scheme_id
    );
    let draft = weaver_types::parse_serving_draft(&source).unwrap();
    let verified = verified_fixture(&fixture, draft.bind_start(hash(b"placeholder")));
    fs::remove_file(fixture.config.root.join("ste100-binary")).unwrap();
    let error = PreparedEnter::prepare_against(
        &fixture.config,
        &AgentName(fixture.config.agent.clone()),
        verified,
        facts(),
        7,
        &fixture.boundary,
    )
    .err()
    .expect("legacy load must not lose an election");
    assert!(matches!(error, PreparationFailure::Inventory {
        refusal: weaver_types::LifecycleRefusal::ConfigInvalid { field: Some(ref name) }, ..
    } if name.0 == "state-management.flow.thesaurus"));
}
#[test]
fn replacement_enter_producer_uses_verified_settings_and_exact_checked_root_bytes() {
    let fixture = fixture();
    let source = fs::read_to_string(fixture.config.territory.join("agent.toml")).unwrap();
    let draft = weaver_types::parse_serving_draft(&source).unwrap();
    let verified = verified_fixture(&fixture, draft.bind_start(hash(b"placeholder")));
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
        state_management
            .ste100
            .as_ref()
            .unwrap()
            .model_artifact_sha256
    );
    let LoadedStateFacts::Serving {
        ste100_binary_sha256,
        ..
    } = prepared.loaded_state()
    else {
        panic!()
    };
    assert_eq!(ste100_binary_sha256, Some(executable_hash));
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
    // GoverningFiles now refuses changed verb keys before an artifact is verified.
    for case in ["digest", "mode", "binary-conflict"] {
        let fixture = fixture();
        let source = fs::read_to_string(fixture.config.territory.join("agent.toml")).unwrap();
        let verified = verified_fixture(
            &fixture,
            weaver_types::parse_serving_draft(&source)
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
