use super::*;

/// **`state.log` is root's and the member's group** (#99 area 2, K12):
/// verify-load and update-stack read it as the member, and a log
/// `root:root` reads empty to them. Perturbation: answer root's group
/// and this fails.
/// **The territory's two groups are two, and neither is gid 0** (#99
/// area 2, H2). Perturbation: drop either half of the comparison and
/// this fails.
#[test]
fn the_territory_groups_are_two_and_not_roots() {
    let groups = |access, state| TerritoryGroups { access, state }.distinct();
    assert!(groups(1001, 1002).is_ok());
    assert!(groups(1001, 1001).is_err(), "one gid for both refuses");
    assert!(groups(0, 1002).is_err(), "an access group of gid 0 refuses");
    assert!(groups(1001, 0).is_err(), "a state group of gid 0 refuses");
}

#[test]
fn the_state_log_is_grouped_to_the_member() {
    let member = inventory::MemberAccount {
        uid: 4242,
        gid: 4343,
    };
    assert_eq!(state_log_owner(member), (0, 4343));
}

/// **The cause is the uid sudo reports, parsed strictly**, per
/// `weaver-admin-Spec` section 2: absent is a root shell and uid 0, a
/// decimal uid is that uid, and anything else refuses `Malformed` rather
/// than falling back. Perturbation: answer uid 0 for a value that does
/// not parse and the malformed cases record a cause nobody gave.
#[test]
fn the_cause_is_sudo_uid_parsed_strictly() {
    use std::ffi::OsStr;
    assert_eq!(cause_from(None), Ok(weaver_types::Cause { uid: 0 }));
    assert_eq!(
        cause_from(Some(OsStr::new("1000"))),
        Ok(weaver_types::Cause { uid: 1000 })
    );
    assert_eq!(
        cause_from(Some(OsStr::new("0"))),
        Ok(weaver_types::Cause { uid: 0 })
    );
    for malformed in [
        "",
        "-1",
        "+5",
        " 1000",
        "1000 ",
        "01000",
        "1e3",
        "4294967296",
        "x",
    ] {
        assert_eq!(
            cause_from(Some(OsStr::new(malformed))),
            Err(LifecycleRefusal::Malformed),
            "{malformed:?} refuses"
        );
    }
}

/// Spawn `readlink` over the named child descriptors with the given
/// pre-exec arming, and return what each resolved to - the socket's
/// inode identity, or an empty string where the number held nothing.
/// Identity rather than existence, because a child's own bookkeeping
/// can land a descriptor on a number this test is watching.
fn probe_child_fds<F>(arming: F, numbers: &[i32]) -> Vec<String>
where
    F: FnMut() -> std::io::Result<()> + Send + Sync + 'static,
{
    let mut probe = std::process::Command::new("/usr/bin/readlink");
    for number in numbers {
        probe.arg(format!("/proc/self/fd/{number}"));
    }
    probe
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    unsafe {
        use std::os::unix::process::CommandExt;
        probe.pre_exec(arming);
    }
    let held = probe.output().expect("the probe runs");
    std::str::from_utf8(&held.stdout)
        .expect("fd targets are ascii")
        .lines()
        .map(str::to_string)
        .collect()
}

/// This process's own reading of a descriptor's identity, the string
/// the child's probe must match for the same open file description.
fn own_fd_identity(raw: i32) -> String {
    std::fs::read_link(format!("/proc/self/fd/{raw}"))
        .expect("own descriptor resolves")
        .to_string_lossy()
        .into_owned()
}

/// **The third walk's enumerate half, on the real arming path**, per
/// `weaver-admin-Spec` section 10 as amended 2026-08-26: spawn a
/// subprocess with the member's spawn arming, enumerate its descriptors,
/// and confirm none of this process's crossed but the one the spawn
/// deliberately arms. Two layouts, because the arming has two paths:
/// the ordinary one, the end above the target and `dup2` moving it, and
/// the equal-number corner, the end already at three with close-on-exec
/// set, where `dup2` is a no-op and the unconditional flag clear is the
/// whole repair, per `weaver-harness-Spec` section 2.2.
///
/// Perturbation: remove the flag clear from `arm_member_end` and the
/// corner layout fails, the armed number closing at exec. Remove the
/// `dup2` and the ordinary layout fails.
#[test]
fn the_spawn_arms_the_gift_and_nothing_else_crosses() {
    // Layout one, ordinary: the end relocated above the child's own
    // low numbers, so the listing below cannot alias it.
    let (harness_end, member_end) = nix::sys::socket::socketpair(
        nix::sys::socket::AddressFamily::Unix,
        nix::sys::socket::SockType::Stream,
        None,
        nix::sys::socket::SockFlag::SOCK_CLOEXEC,
    )
    .expect("the pair");
    let relocate = |end: std::os::fd::OwnedFd| -> std::os::fd::OwnedFd {
        let raw =
            nix::fcntl::fcntl(&end, nix::fcntl::FcntlArg::F_DUPFD_CLOEXEC(16)).expect("relocates");
        // SAFETY: a fresh descriptor this process owns, adopted once,
        // the low original dropping with `end`.
        unsafe {
            use std::os::fd::FromRawFd;
            std::os::fd::OwnedFd::from_raw_fd(raw)
        }
    };
    let harness_end = relocate(harness_end);
    let member_end = relocate(member_end);
    let harness_raw = {
        use std::os::fd::AsRawFd;
        harness_end.as_raw_fd()
    };
    let member_raw = {
        use std::os::fd::AsRawFd;
        member_end.as_raw_fd()
    };
    let member_identity = own_fd_identity(member_raw);
    let listed = probe_child_fds(move || arm_member_end(member_raw), &[3, harness_raw]);
    assert_eq!(
        listed.first().map(String::as_str),
        Some(member_identity.as_str()),
        "the gift stands at the fixed number as the member's own end: {listed:?}"
    );
    assert_eq!(
        listed.len(),
        1,
        "this process's other end did not cross: {listed:?}"
    );
    drop(harness_end);
    drop(member_end);

    // Layout two, the corner: the end seated at the target with the
    // flag set before the arming runs, so `dup2` is a no-op and only
    // the unconditional clear keeps the gift alive across exec.
    let (own_end, corner_end) = nix::sys::socket::socketpair(
        nix::sys::socket::AddressFamily::Unix,
        nix::sys::socket::SockType::Stream,
        None,
        nix::sys::socket::SockFlag::SOCK_CLOEXEC,
    )
    .expect("the corner pair");
    let corner_raw = {
        use std::os::fd::AsRawFd;
        corner_end.as_raw_fd()
    };
    let corner_identity = own_fd_identity(corner_raw);
    let listed = probe_child_fds(
        move || {
            // Seat the end at three with close-on-exec set, which is
            // the layout an unlucky descriptor table hands the real
            // spawn.
            if unsafe { nix::libc::dup2(corner_raw, 3) } < 0 {
                return Err(std::io::Error::last_os_error());
            }
            if unsafe { nix::libc::fcntl(3, nix::libc::F_SETFD, nix::libc::FD_CLOEXEC) } < 0 {
                return Err(std::io::Error::last_os_error());
            }
            arm_member_end(3)
        },
        &[3],
    );
    assert_eq!(
        listed.first().map(String::as_str),
        Some(corner_identity.as_str()),
        "the corner's gift survives exec on the flag clear alone: {listed:?}"
    );
    drop(own_end);
    drop(corner_end);
}

/// **The vector this crate composes, in both directions**, per
/// `weaver-admin-Spec` section 6 as ruled 2026-08-26: a serving
/// inventory puts one value on it and a diagnostic inventory puts two,
/// the preload path the territory with a fixed leaf. Two watches rather
/// than one, because one would not fail on both directions.
///
/// Perturbation: make the arm that appends the preload path
/// unconditional and the serving half fails on a load carrying two.
/// Remove the arm and the diagnostic half fails on a load carrying one.
#[test]
fn the_vector_follows_the_kind_in_both_directions() {
    let territory = std::path::Path::new("/dbpool/agents/alpha/state");
    let serving_binding = weaver_types::EnterBinding::Serving {
        gate_instruction: weaver_types::GateInstruction {
            access_rule: weaver_types::AccessRule {
                allowed_uids: Default::default(),
                allowed_gids: Default::default(),
                denied_uids: Default::default(),
            },
        },
    };
    let serving = member_vector(territory, &serving_binding);
    assert_eq!(
        serving.len(),
        1,
        "a serving load carries the territory alone"
    );
    assert_eq!(serving[0], territory.as_os_str());
    let diagnostic = member_vector(territory, &weaver_types::EnterBinding::Diagnostic);
    assert_eq!(
        diagnostic.len(),
        2,
        "a diagnostic load carries the preload path"
    );
    assert_eq!(diagnostic[0], territory.as_os_str());
    assert_eq!(
        diagnostic[1],
        territory.join("preload.sock").into_os_string(),
        "the territory with the fixed leaf, no invocation input composing it"
    );
    // **A serving load that elects a restore names no door**, per Spec
    // section 6 as of A3.2: it restores through descriptor 4, so the
    // vector is the territory alone whatever the lineage.
    assert_eq!(member_vector(territory, &serving_binding).len(), 1);
}

/// **The stack names the binaries this crate started and handed the
/// worker**, per `weaver-admin-harness-contract` section 3 and Spec
/// section 9: the worker on every load, the member only where it stood, and
/// the root's SPU and the gate always.
///
/// Perturbations, each failing an assertion here: name the member whatever
/// stood; leave the SPU out; leave the gate out.
#[test]
fn the_stack_names_what_was_started_and_handed() {
    let config = unread_config();
    let without = stack_digests(&config, false, None);
    let names: Vec<&str> = without.keys().map(String::as_str).collect();
    assert_eq!(names, ["python-spu.pyz", "weaver-gate", "worker"]);
    let with = stack_digests(&config, true, None);
    assert_eq!(
        with.len(),
        4,
        "the worker, the member, the SPU and the gate"
    );
    assert!(with.contains_key("weaver-state"));
    // The classify arm's binary is named where it is handed.
    // Perturbation: drop it from the set and the key is absent.
    let classified = stack_digests(
        &config,
        false,
        Some(std::path::Path::new("/nonexistent/bin/weaver-spu-classify")),
    );
    assert!(classified.contains_key("weaver-spu-classify"));
}

/// **The vector carries the root's values and the declaration's loop
/// file, and nothing composed from the invocation's input**, per Spec
/// section 6: the socket, the SPU the root names and the gate, then each
/// named flag only where its value stands. Perturbation: substitute a
/// fixed SPU path in `worker_arguments` and the first assertion fails;
/// emit `--classify-binary` unconditionally and the bare vector carries it.
#[test]
fn the_roots_spu_reaches_the_vector() {
    let config = unread_config();
    let socket = config.coordination_socket();
    let bare = start::worker_arguments(&socket, &config.spu, &config.gate, None, None, None);
    assert_eq!(
        bare,
        vec![
            socket.display().to_string(),
            "/nonexistent/python/python-spu.pyz".to_string(),
            "/nonexistent/bin/weaver-gate".to_string(),
        ]
    );
    let full = start::worker_arguments(
        &socket,
        &config.spu,
        &config.gate,
        Some("4096"),
        Some(std::path::Path::new("/home/op/loop.py")),
        Some(std::path::Path::new("/opt/weaver/bin/weaver-spu-classify")),
    );
    for (flag, value) in [
        ("--headroom-bytes", "4096"),
        ("--loop-file", "/home/op/loop.py"),
        ("--classify-binary", "/opt/weaver/bin/weaver-spu-classify"),
    ] {
        let at = full.iter().position(|a| a == flag).expect(flag);
        assert_eq!(full[at + 1], value);
    }
}

/// The configuration as `validate`, `load` and `restore` read it: the
/// territory judged, then the declaration read through it and judged
/// where it is read (the #94 survey's S8).
fn load_and_read(
    base: &std::path::Path,
    agent: &str,
    owner: u32,
    access_gid: u32,
) -> Result<ServiceConfig, LifecycleRefusal> {
    let config = load_service_config_at(base, agent, owner, access_gid)?;
    read_declaration(&config)?;
    Ok(config)
}

/// **The Scratch guard removes its root's territory sibling** (#113):
/// `write_root` lays the territory out beside the root, as
/// `<root>.territory`, and a guard that removed only the root left one
/// behind on every run. Perturbation: remove the root alone, and the
/// sibling stays.
#[test]
fn scratch_removes_its_territory_sibling() {
    let path = std::env::temp_dir().join(format!(
        "weaver-admin-scratch-sibling-{}",
        std::process::id()
    ));
    let sibling = path.with_extension("territory");
    {
        let root = crate::scratch::Scratch(path.clone());
        write_root(&root);
        assert!(sibling.is_dir(), "write_root lays the sibling out");
    }
    assert!(!path.exists(), "the root is removed");
    assert!(!sibling.exists(), "the territory sibling is removed");
}

/// The values every root carries, written into a scratch root, with the
/// agent's territory beside it, `<root>.territory`, laid out as the
/// judgment asks with this test's uid in root's place: mode 0710, its
/// `save-points/` 0750, and an empty `agent.toml` 0640. Answers the
/// territory.
fn write_root(root: &std::path::Path) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(root).unwrap();
    let territory = root.with_extension("territory");
    let _ = std::fs::remove_dir_all(&territory);
    std::fs::create_dir_all(territory.join("save-points")).unwrap();
    std::fs::set_permissions(
        territory.join("save-points"),
        std::fs::Permissions::from_mode(0o750),
    )
    .unwrap();
    std::fs::write(territory.join("agent.toml"), "").unwrap();
    std::fs::set_permissions(
        territory.join("agent.toml"),
        std::fs::Permissions::from_mode(0o640),
    )
    .unwrap();
    std::fs::set_permissions(&territory, std::fs::Permissions::from_mode(0o710)).unwrap();
    let operator = nix::unistd::getuid().as_raw().to_string();
    let territory_path = territory.display().to_string();
    for (name, text) in [
        ("coordination-root", "/run/weaver"),
        ("worker-binary", "/opt/weaver/bin/worker"),
        ("spu-binary", "/opt/weaver/bin/weaver-spu"),
        ("gate-binary", "/opt/weaver/bin/weaver-gate"),
        ("territory", territory_path.as_str()),
        ("operator", operator.as_str()),
        ("roles.toml", "trace-reader = \"weaver-alpha-admincon\"\n"),
    ] {
        std::fs::write(root.join(name), text).unwrap();
    }
    territory
}

/// **A root naming two binaries under one file name fails the read**,
/// before any verb, per Spec section 9, and a sound root reads as this
/// agent's, its declaration beside it. Perturbation: drop the
/// `judge_names` call from the read and the second assertion fails.
#[test]
fn a_root_naming_two_binaries_under_one_name_fails_the_read() {
    // Held by the guard, so its territory sibling goes with it (#113).
    let root = crate::scratch::Scratch(
        std::env::temp_dir().join(format!("weaver-admin-root-read-{}", std::process::id())),
    );
    let _ = std::fs::remove_dir_all(&root);
    write_root(&root);
    let config = load_service_config_from(&root, "alpha").expect("a sound root reads");
    assert_eq!(config.agent, "alpha");
    assert_eq!(
        config.boundary.as_ref().unwrap().digest.len(),
        64,
        "the boundary file's sha256 hex"
    );
    std::fs::write(root.join("gate-binary"), "/opt/other/worker").unwrap();
    let failure = load_service_config_from(&root, "alpha")
        .err()
        .unwrap_or_default();
    assert!(failure.contains("share the file name"), "{failure:?}");
    let _ = std::fs::remove_dir_all(&root);
}

/// **The agent's root is admitted only as a directory root holds that no
/// other principal may write**, per Spec section 9. Judged here against
/// this test's own uid, the one parameter production fixes at root's.
///
/// Perturbations: drop the owner comparison and the first refusal
/// admits; drop the mode test and the group- and world-writable cases
/// admit.
#[test]
fn the_root_is_admitted_only_owned_and_closed() {
    use std::os::unix::fs::PermissionsExt;
    let me = nix::unistd::getuid().as_raw();
    let base = std::env::temp_dir().join(format!("weaver-admin-root-judge-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let root = base.join("alpha");
    std::fs::create_dir_all(&root).unwrap();
    let mode = |bits| std::fs::set_permissions(&root, std::fs::Permissions::from_mode(bits));
    mode(0o755).unwrap();
    assert_eq!(judge_root(&root, me), Ok(()));
    if me != 0 {
        assert_eq!(
            judge_root(&root, 0),
            Err(LifecycleRefusal::BoundaryUnverified),
            "a root another uid holds"
        );
    }
    for open in [0o775, 0o757, 0o777] {
        mode(open).unwrap();
        assert_eq!(
            judge_root(&root, me),
            Err(LifecycleRefusal::BoundaryUnverified),
            "{open:o} lets another principal write"
        );
    }
    mode(0o755).unwrap();
    assert_eq!(
        judge_root(&base.join("beta"), me),
        Err(LifecycleRefusal::NoSuchAgent),
        "no root is no agent"
    );
    std::fs::write(base.join("gamma"), "a file").unwrap();
    assert_eq!(
        judge_root(&base.join("gamma"), me),
        Err(LifecycleRefusal::NoSuchAgent),
        "a file is not a root"
    );
    std::os::unix::fs::symlink(&root, base.join("delta")).unwrap();
    assert_eq!(
        judge_root(&base.join("delta"), me),
        Err(LifecycleRefusal::NoSuchAgent),
        "a link at the root is not followed"
    );
    let _ = std::fs::remove_dir_all(&base);
}

/// **Every directory above the root is closed as the root is**, per Spec
/// section 9: a directory a group or the world may write refuses
/// `BoundaryUnverified` unless it is sticky, and a closed or sticky one
/// admits. Codex on #45, round 9: a writable ancestor let another principal
/// swap the judged root before the reads. Perturbation: drop the
/// `judge_ancestors` call, and the open base reads.
#[test]
fn every_directory_above_the_root_is_closed() {
    use std::os::unix::fs::PermissionsExt;
    let me = nix::unistd::getuid().as_raw();
    let base = std::env::temp_dir().join(format!("weaver-admin-ancestors-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let root = base.join("alpha");
    write_root(&root);
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).unwrap();
    let base_mode =
        |bits| std::fs::set_permissions(&base, std::fs::Permissions::from_mode(bits)).unwrap();
    for open in [0o775, 0o757, 0o777] {
        base_mode(open);
        assert_eq!(
            load_service_config_at(&base, "alpha", me, nix::unistd::getegid().as_raw()).err(),
            Some(LifecycleRefusal::BoundaryUnverified),
            "a base of mode {open:o} lets another principal swap the root"
        );
    }
    for closed in [0o755, 0o1777] {
        base_mode(closed);
        assert!(
            load_service_config_at(&base, "alpha", me, nix::unistd::getegid().as_raw()).is_ok(),
            "a base of mode {closed:o} admits"
        );
    }
    // A writable directory two levels up refuses as the parent does.
    let deeper = base.join("deeper");
    write_root(&deeper.join("beta"));
    std::fs::set_permissions(deeper.join("beta"), std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::set_permissions(&deeper, std::fs::Permissions::from_mode(0o755)).unwrap();
    base_mode(0o777);
    assert_eq!(
        load_service_config_at(&deeper, "beta", me, nix::unistd::getegid().as_raw()).err(),
        Some(LifecycleRefusal::BoundaryUnverified),
        "a writable grandparent"
    );
    base_mode(0o755);
    let _ = std::fs::remove_dir_all(&base);
}

/// **The territory and `save-points/` are grouped by name** (Codex on
/// #94), per Spec section 9: the territory's gid is judged against the
/// state group and `save-points/`'s against the access group, each
/// resolved by name in production and never taken as it stands.
/// The territory stands under this test's own gid and the judgment is
/// given a gid one off, so the case needs no supplementary group and
/// never skips: one off refuses, its own reads. Perturbation: take the
/// territory's gid as it stands again and the one-off case reads.
#[test]
fn a_territory_under_another_group_than_the_access_group_refuses() {
    use std::os::unix::fs::PermissionsExt;
    let me = nix::unistd::getuid().as_raw();
    let mine = nix::unistd::getegid().as_raw();
    let base =
        std::env::temp_dir().join(format!("weaver-admin-territory-gid-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let root = base.join("alpha");
    write_root(&root);
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(
        load_service_config_at(&base, "alpha", me, mine.wrapping_add(1)).err(),
        Some(LifecycleRefusal::BoundaryUnverified),
        "a territory under another group than the access group refuses"
    );
    let config = load_service_config_at(&base, "alpha", me, mine).expect("its own group reads");
    assert_eq!(config.access_gid, mine);
    // **The territory is the state group's, `save-points/` and the
    // declaration the access group's** (the operator's ruling of
    // 2026-10-08 on #1): judged against two groups apart, the territory
    // under the access group alone refuses. Perturbation: judge the
    // territory against the access group and it reads.
    let apart = TerritoryGroups {
        access: mine,
        state: mine.wrapping_add(1),
    };
    assert_eq!(
        load_service_config_with(&base, "alpha", me, apart).err(),
        Some(LifecycleRefusal::BoundaryUnverified),
        "a territory under the access group rather than the state group refuses"
    );
    let _ = std::fs::remove_dir_all(&base);
    let _ = std::fs::remove_dir_all(base.join("alpha").with_extension("territory"));
}

/// **The territory `deploy/create-agent.sh` lays out passes both of
/// admin's judgments of it** (the #94 survey's S1): `judge_territory`,
/// which asks the ruled mode and group, and section 4's denial walk,
/// `agent_can_traverse` on the sink's directory, which asks that the
/// agent's uid cannot pass. At `0711` the two could never both hold, and
/// every load refused. The mode and the group are read from the script's
/// own line, so the script, the judgment and the walk cannot drift apart
/// again. Perturbation: lay the territory out, or judge it, at `0711`.
#[test]
fn the_territory_create_agent_lays_out_passes_both_judgments() {
    use std::os::unix::fs::PermissionsExt;
    let script = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../deploy/create-agent.sh"),
    )
    .unwrap();
    let line = script
        .lines()
        .find(|line| line.starts_with("sudo install -d") && line.ends_with("\"$HOME_DIR\""))
        .expect("create-agent.sh lays the territory out with one install line");
    let words: Vec<&str> = line.split_whitespace().collect();
    let after = |flag: &str| words[words.iter().position(|w| *w == flag).unwrap() + 1];
    assert_eq!(after("-o"), "root", "{line}");
    assert_eq!(after("-g"), "\"$STATE_GROUP\"", "{line}");
    let mode = u32::from_str_radix(after("-m"), 8).unwrap();
    assert!(
        script.contains("STATE_GROUP=\"$MEMBER_USER\""),
        "the state group is the member's own primary group"
    );
    let me = nix::unistd::getuid().as_raw();
    let mine = nix::unistd::getegid().as_raw();
    let base = std::env::temp_dir().join(format!("weaver-admin-laid-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let territory = write_root(&base.join("alpha"));
    std::fs::set_permissions(&territory, std::fs::Permissions::from_mode(mode)).unwrap();
    let groups = TerritoryGroups {
        access: mine,
        state: mine,
    };
    assert!(
        judge_territory(&territory, groups).is_ok(),
        "create-agent's mode {mode:o} passes the territory's judgment"
    );
    // The agent's uid is not the territory's owner and holds neither
    // the state group nor the access group.
    let boundary = inventory::Boundary {
        agent_uid: me.wrapping_add(1),
        admin_uid: me,
        agent_gids: vec![mine.wrapping_add(1)],
        home: territory.clone(),
        member_binary: None,
        member_account: None,
        member_group: None,
    };
    assert!(
        !inventory::agent_can_traverse(&territory, &boundary),
        "create-agent's mode {mode:o} lets the agent's uid pass to the trace"
    );
    assert!(inventory::admin_holds_custody(&territory, &boundary));
    let _ = std::fs::remove_dir_all(&base);
}

/// **A territory with no `agent.toml` is no agent, and one
/// that is not a closed regular file refuses**, per Spec section 9: the
/// root's keys standing do not make an agent. Perturbations: drop the
/// declaration check, or judge presence alone, and a case here reads.
#[test]
fn a_territory_without_a_declaration_is_no_agent() {
    use std::os::unix::fs::PermissionsExt;
    let me = nix::unistd::getuid().as_raw();
    let base = std::env::temp_dir().join(format!("weaver-admin-no-decl-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let root = base.join("alpha");
    let declarations = write_root(&root);
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).unwrap();
    let config = load_and_read(&base, "alpha", me, nix::unistd::getegid().as_raw())
        .expect("a declared agent reads");
    assert_eq!(config.agent, "alpha");
    assert_eq!(
        config.territory,
        std::fs::canonicalize(&declarations).unwrap()
    );
    std::fs::remove_file(declarations.join("agent.toml")).unwrap();
    assert_eq!(
        load_and_read(&base, "alpha", me, nix::unistd::getegid().as_raw()).err(),
        Some(LifecycleRefusal::NoSuchAgent),
        "no agent.toml is no agent"
    );
    std::fs::create_dir(declarations.join("agent.toml")).unwrap();
    assert_eq!(
        load_and_read(&base, "alpha", me, nix::unistd::getegid().as_raw()).err(),
        Some(LifecycleRefusal::BoundaryUnverified),
        "a directory named agent.toml"
    );
    std::fs::remove_dir(declarations.join("agent.toml")).unwrap();
    std::os::unix::fs::symlink(declarations.join("gone"), declarations.join("agent.toml")).unwrap();
    assert_eq!(
        load_and_read(&base, "alpha", me, nix::unistd::getegid().as_raw()).err(),
        Some(LifecycleRefusal::BoundaryUnverified),
        "a link named agent.toml"
    );
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn a_root_holding_a_retired_key_refuses_by_its_name() {
    // **Nothing of an older layout is read or tolerated** (the operator's
    // ruling of 2026-10-08 on #1): each retired key refuses
    // `ConfigInvalid` naming it, whatever else the root holds, and the
    // force with it. Perturbation: drop `refuse_retired_keys` from the
    // judgment and a root holding one loads.
    use std::os::unix::fs::PermissionsExt;
    let me = nix::unistd::getuid().as_raw();
    let base = std::env::temp_dir().join(format!("weaver-admin-retired-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let root = base.join("alpha");
    write_root(&root);
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).unwrap();
    let gid = nix::unistd::getegid().as_raw();
    assert!(load_and_read(&base, "alpha", me, gid).is_ok());
    // A retired name refuses by its name whatever stands there: a file,
    // a directory, a dangling link (#107). Perturbation: judge the
    // entries before the retired names, and the directory and the link
    // refuse `BoundaryUnverified` unnamed.
    for (key, form) in RETIRED_ROOT_KEYS
        .iter()
        .flat_map(|(key, _)| ["directory", "dangling link"].map(|form| (*key, form)))
    {
        let stale = root.join(key);
        if form == "directory" {
            std::fs::create_dir(&stale).unwrap();
        } else {
            std::os::unix::fs::symlink(base.join("nowhere"), &stale).unwrap();
        }
        assert_eq!(
            load_and_read(&base, "alpha", me, gid).err(),
            Some(LifecycleRefusal::ConfigInvalid {
                field: Some(FieldName(key.to_string())),
            }),
            "{key} as a {form} refuses by its name"
        );
        if form == "directory" {
            std::fs::remove_dir(&stale).unwrap();
        } else {
            std::fs::remove_file(&stale).unwrap();
        }
    }
    for (key, _) in RETIRED_ROOT_KEYS {
        let stale = root.join(key);
        std::fs::write(&stale, "/old\n").unwrap();
        std::fs::set_permissions(&stale, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(
            load_and_read(&base, "alpha", me, gid).err(),
            Some(LifecycleRefusal::ConfigInvalid {
                field: Some(FieldName(key.to_string())),
            }),
            "{key} refuses by its name"
        );
        assert_eq!(
            load_service_config_judged(
                &base,
                "alpha",
                me,
                Some(TerritoryGroups {
                    access: gid,
                    state: gid
                }),
                true
            )
            .err(),
            Some(LifecycleRefusal::ConfigInvalid {
                field: Some(FieldName(key.to_string())),
            }),
            "{key} refuses the force too"
        );
        std::fs::remove_file(&stale).unwrap();
    }
    assert!(load_and_read(&base, "alpha", me, gid).is_ok());
    let _ = std::fs::remove_dir_all(&base);
}

/// **The territory is root's at mode 0710, its `save-points/` 0750 and
/// its declaration the access group's at 0640, and never a link at its
/// own name**, per Spec section 9 on the operator's rulings of 2026-10-07
/// and 2026-10-08 on #1, with this test's uid in root's place: the `0711`
/// of the first 2026-10-08 ruling refuses now, as does every other mode.
/// Perturbations: test only the write bits and the 0750 territory reads;
/// judge 0711 again and the 0710 territory refuses; drop the save-points
/// judgment and the 0770 one reads; judge the declaration's write bits
/// alone and the 0644 one reads; drop its group comparison and one under
/// another group reads.
#[test]
fn the_territory_is_roots_at_0710_and_never_a_link() {
    use std::os::unix::fs::PermissionsExt;
    let me = nix::unistd::getuid().as_raw();
    let base = std::env::temp_dir().join(format!("weaver-admin-decl-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let root = base.join("alpha");
    let territory = write_root(&root);
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(load_and_read(&base, "alpha", me, nix::unistd::getegid().as_raw()).is_ok());
    for open in [0o750, 0o711, 0o700, 0o770, 0o1710] {
        std::fs::set_permissions(&territory, std::fs::Permissions::from_mode(open)).unwrap();
        assert_eq!(
            load_and_read(&base, "alpha", me, nix::unistd::getegid().as_raw()).err(),
            Some(LifecycleRefusal::BoundaryUnverified),
            "{open:o} is not the territory's 0710"
        );
    }
    std::fs::set_permissions(&territory, std::fs::Permissions::from_mode(0o710)).unwrap();
    let save_points = territory.join("save-points");
    for open in [0o770, 0o755, 0o700] {
        std::fs::set_permissions(&save_points, std::fs::Permissions::from_mode(open)).unwrap();
        assert_eq!(
            load_and_read(&base, "alpha", me, nix::unistd::getegid().as_raw()).err(),
            Some(LifecycleRefusal::BoundaryUnverified),
            "{open:o} is not save-points' 0750"
        );
    }
    std::fs::set_permissions(&save_points, std::fs::Permissions::from_mode(0o750)).unwrap();
    std::fs::rename(&save_points, territory.join("aside")).unwrap();
    assert_eq!(
        load_and_read(&base, "alpha", me, nix::unistd::getegid().as_raw()).err(),
        Some(LifecycleRefusal::BoundaryUnverified),
        "no save-points directory is the provisioning incomplete"
    );
    std::fs::rename(territory.join("aside"), &save_points).unwrap();
    let declaration = territory.join("agent.toml");
    std::fs::set_permissions(&declaration, std::fs::Permissions::from_mode(0o664)).unwrap();
    assert_eq!(
        load_and_read(&base, "alpha", me, nix::unistd::getegid().as_raw()).err(),
        Some(LifecycleRefusal::BoundaryUnverified),
        "a declaration the group could write"
    );
    // **Root's and the access group's, 0640 exactly** (the operator's
    // ruling of 2026-10-08 on #1): the territory's passage is the state
    // group's, whose member must not read the declaration, so one any uid
    // could read refuses, and one read by the access group alone reads. Perturbation: judge the write
    // bits alone again and the 0644 declaration reads.
    std::fs::set_permissions(&declaration, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(
        load_and_read(&base, "alpha", me, nix::unistd::getegid().as_raw()).err(),
        Some(LifecycleRefusal::BoundaryUnverified),
        "a declaration every uid could read"
    );
    std::fs::set_permissions(&declaration, std::fs::Permissions::from_mode(0o640)).unwrap();
    assert!(load_and_read(&base, "alpha", me, nix::unistd::getegid().as_raw()).is_ok());
    // **Grouped to the access group**: a declaration at 0640 under any
    // other group is read by that group, so it refuses. Set to another
    // of this uid's own groups, and skipped, naming why, where it holds
    // none. Perturbation: drop the group comparison and it reads.
    let mine = nix::unistd::getegid();
    match nix::unistd::getgroups()
        .unwrap_or_default()
        .into_iter()
        .find(|g| *g != mine)
    {
        Some(other) => {
            nix::unistd::chown(&declaration, None, Some(other)).unwrap();
            assert_eq!(
                load_and_read(&base, "alpha", me, mine.as_raw()).err(),
                Some(LifecycleRefusal::BoundaryUnverified),
                "a declaration grouped to another group than the access group"
            );
            nix::unistd::chown(&declaration, None, Some(mine)).unwrap();
        }
        None => eprintln!("skipped the declaration's group case: this uid holds one group"),
    }
    let link = base.join("linked.territory");
    std::os::unix::fs::symlink(&territory, &link).unwrap();
    std::fs::write(root.join("territory"), link.display().to_string()).unwrap();
    assert_eq!(
        load_and_read(&base, "alpha", me, nix::unistd::getegid().as_raw()).err(),
        Some(LifecycleRefusal::BoundaryUnverified),
        "a link at the territory's own name"
    );
    // **A link above the territory's name** (#99 area 2, H3): the open
    // follows it and the logs would be made at the canonical path, so
    // a key that is not canonical refuses. Perturbation: drop the
    // canonical comparison and this loads.
    let via = base.join("via");
    std::os::unix::fs::symlink(territory.parent().unwrap(), &via).unwrap();
    let through = via.join(territory.file_name().unwrap());
    std::fs::write(root.join("territory"), through.display().to_string()).unwrap();
    assert_eq!(
        load_and_read(&base, "alpha", me, nix::unistd::getegid().as_raw()).err(),
        Some(LifecycleRefusal::BoundaryUnverified),
        "a territory named through a link above it"
    );
    let _ = std::fs::remove_dir_all(&base);
}

/// **`save-points/` is judged for access-control entries as the
/// territory is** (the custody audit's G7): a default entry granting a
/// principal outside the access group read refuses the territory. Skips,
/// naming why, where `setfacl` cannot set the entry. Perturbation: drop
/// the look and the territory reads.
#[test]
fn a_save_points_carrying_an_access_entry_refuses() {
    use std::os::unix::fs::PermissionsExt;
    let me = nix::unistd::getuid().as_raw();
    let base = std::env::temp_dir().join(format!("weaver-admin-acl-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let root = base.join("alpha");
    write_root(&root);
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).unwrap();
    let save_points = root.with_extension("territory").join("save-points");
    let set = std::process::Command::new("setfacl")
        .args(["-d", "-m", "u:nobody:r"])
        .arg(&save_points)
        .status();
    if !set.is_ok_and(|status| status.success()) {
        eprintln!("SKIP: setfacl could not set a default entry here");
        let _ = std::fs::remove_dir_all(&base);
        return;
    }
    assert_eq!(
        load_service_config_at(&base, "alpha", me, nix::unistd::getegid().as_raw()).err(),
        Some(LifecycleRefusal::BoundaryUnverified),
        "an access-control entry on save-points refuses"
    );
    let _ = std::fs::remove_dir_all(&base);
    let _ = std::fs::remove_dir_all(root.with_extension("territory"));
}

/// **The declaration and a log carrying an access entry beyond their mode
/// are refused** (#99 area 2, H5; #107): an entry granting the member read
/// hides under the mask, which still shows 0640. Skips, naming why, where
/// setfacl cannot set the entry. Perturbations: drop the look from
/// `open_declaration` and the declaration reads; from `log::open_append`
/// and the log opens.
#[test]
fn a_declaration_or_a_log_carrying_an_access_entry_refuses() {
    use std::os::unix::fs::PermissionsExt;
    let me = nix::unistd::getuid().as_raw();
    let gid = nix::unistd::getegid().as_raw();
    let base = std::env::temp_dir().join(format!("weaver-admin-file-acl-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let root = base.join("alpha");
    let territory = write_root(&root);
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).unwrap();
    let entry = |path: &std::path::Path, args: &[&str]| {
        std::process::Command::new("setfacl")
            .args(args)
            .arg(path)
            .status()
            .is_ok_and(|status| status.success())
    };
    let declaration = territory.join("agent.toml");
    if !entry(&declaration, &["-m", "u:nobody:r"]) {
        eprintln!("SKIP: setfacl could not set an entry here");
        let _ = std::fs::remove_dir_all(&base);
        let _ = std::fs::remove_dir_all(&territory);
        return;
    }
    assert_eq!(
        load_and_read(&base, "alpha", me, gid).err(),
        Some(LifecycleRefusal::BoundaryUnverified),
        "a declaration the member could read through an entry"
    );
    assert!(entry(&declaration, &["-b"]));
    std::fs::set_permissions(&declaration, std::fs::Permissions::from_mode(0o640)).unwrap();
    assert!(
        load_and_read(&base, "alpha", me, gid).is_ok(),
        "and without it it reads"
    );
    let log_path = base.join("worker.log");
    std::fs::write(&log_path, "").unwrap();
    assert!(entry(&log_path, &["-m", "u:nobody:r"]));
    assert!(
        log::open_append(&log_path, None).is_err(),
        "a log the member could read through an entry"
    );
    assert!(entry(&log_path, &["-b"]));
    assert!(
        log::open_append(&log_path, None).is_ok(),
        "and without it it opens"
    );
    let _ = std::fs::remove_dir_all(&base);
    let _ = std::fs::remove_dir_all(&territory);
}

/// **A load with save points still waiting in the room refuses before it
/// selects** (Codex on #94, at ab8acef, and the #99 area 1 review, T2):
/// the load's publication, its gate and its selection run in that order
/// in `publish_then_select`, and a room file the copy cannot publish, its
/// stamp sound and its body not, defers, so the load refuses
/// `BoundaryUnverified` and never reaches the selection; the helper says
/// nothing deferred goes on. Perturbations: drop the gate from
/// `publish_then_select` and the deferred load selects nothing and goes
/// on; ignore the flag in `load_may_select` and the same.
#[test]
fn a_load_with_save_points_waiting_in_the_room_refuses() {
    let (config, report, base) = territory_with_one_room_file("load-deferred");
    // The room's file keeps its sound stamp and its name, and its body
    // no longer digests to the name: the scan admits it, every copy
    // refuses it, and the publication defers.
    let room = config.territory.join(save_points::ROOM).join(&report.name);
    let mut bytes = std::fs::read(&room).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 0xff;
    std::fs::write(&room, &bytes).unwrap();
    let declaration = weaver_types::parse(&format!(
        concat!(
            "session = \"s-1\"\n",
            "tool-set = []\n",
            "permission-mode = \"ask\"\n",
            "\n",
            "[spu-instruction.decoder]\n",
            "residual-readout-election = false\n",
            "tunable-values = {{}}\n",
            "\n",
            "[spu-instruction.decoder.model-binding]\n",
            "artifact = \"qwen3-4b-instruct\"\n",
            "devices = [0]\n",
            "\n",
            "[gate-instruction.access-rule]\n",
            "allowed-uids = [0]\n",
            "allowed-gids = []\n",
            "denied-uids = [1701]\n",
            "\n",
            "[trace-sink]\n",
            "kind = \"file\"\n",
            "path = \"{}/trace.ndjson\"\n",
            "create = true\n",
            "\n",
            "[state-store]\n",
            "engine = \"sqlite\"\n",
        ),
        config.territory.display()
    ))
    .expect("the declaration parses");
    let gate_instruction = declaration
        .gate_instruction
        .clone()
        .expect("a serving declaration");
    let me = nix::unistd::getuid().as_raw();
    let inventory = inventory::Inventory {
        config: declaration,
        identity: "weaver-alpha".into(),
        declaration: String::new(),
        binding: weaver_types::EnterBinding::Serving { gate_instruction },
        lineage: None,
        member_account: Some(inventory::MemberAccount {
            uid: me,
            gid: nix::unistd::getgid().as_raw(),
        }),
    };
    assert_eq!(
        publish_then_select(&config, &AgentName("alpha".into()), &inventory).err(),
        Some(LifecycleRefusal::BoundaryUnverified),
        "the deferred load refuses before it selects"
    );
    assert!(room.exists(), "the file stays for the operator");
    assert_eq!(load_may_select(&config, false), Ok(()));
    let _ = std::fs::remove_dir_all(&base);
}

/// **The publication does not ask the boundary file** (Codex on #94, at
/// 9fa18b4), per Spec section 9, which asks it of `validate` and `load`
/// alone: with `roles.toml` gone, the load's inventory refuses naming it,
/// and the publication an unload runs over a run already gone, and
/// `restore`, derive the room without it. This box holds no agent
/// accounts, so the publication stops at the account lookup either way;
/// what it must never answer is the boundary's refusal. Perturbation:
/// derive the publication's inventory through `take_inventory` again and
/// it refuses naming `roles.toml`.
#[test]
fn the_publication_does_not_ask_the_boundary_file() {
    use std::os::unix::fs::PermissionsExt;
    let me = nix::unistd::getuid().as_raw();
    let base = std::env::temp_dir().join(format!("weaver-admin-room-only-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let root = base.join("alpha");
    write_root(&root);
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::remove_file(root.join("roles.toml")).unwrap();
    let config = load_service_config_at(&base, "alpha", me, nix::unistd::getegid().as_raw())
        .expect("the root reads without its boundary file");
    let agent = AgentName("alpha".into());
    let boundary = LifecycleRefusal::ConfigInvalid {
        field: Some(weaver_types::FieldName("roles.toml".into())),
    };
    assert_eq!(
        take_inventory(&config, &agent).err(),
        Some(boundary.clone()),
        "the load's inventory asks the boundary"
    );
    assert_ne!(
        room_inventory(&config, &agent).err(),
        Some(boundary.clone())
    );
    assert_ne!(
        publish_from_room(&config, &agent, &[]).err(),
        Some(boundary.clone()),
        "the publication never refuses for the boundary"
    );
    assert_ne!(restore(&config, &agent).err(), Some(boundary));
    let _ = std::fs::remove_dir_all(&base);
    let _ = std::fs::remove_dir_all(root.with_extension("territory"));
}

/// **The publication never reads the declaration** (the #94 survey's
/// S8): it runs after a run ended, so a declaration saved unparsable
/// while the run stood must not strand the leave's save point. With
/// `agent.toml` holding no TOML, a sound room file publishes through
/// `publish_from_room`, the entry point the unload and the `save-point`
/// verb call, its test seam standing in only for the account lookup.
/// Perturbation: derive the room through `room_inventory` again and it
/// refuses.
#[test]
fn the_publication_does_not_read_the_declaration() {
    use std::os::unix::fs::PermissionsExt;
    let me = nix::unistd::getuid().as_raw();
    let base =
        std::env::temp_dir().join(format!("weaver-admin-no-decl-read-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let root = base.join("alpha");
    let territory = write_root(&root);
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::write(territory.join("agent.toml"), "this is [ not toml").unwrap();
    let room = territory.join(save_points::ROOM);
    std::fs::create_dir(&room).unwrap();
    let bytes = save_points::tests::save_point("r-1", 1, 0, 1_000_000_000, b"room");
    let digest = save_points::judge(&bytes).unwrap().digest;
    std::fs::write(room.join(format!("{digest}.save-point")), &bytes).unwrap();
    let config = load_service_config_at(&base, "alpha", me, nix::unistd::getegid().as_raw())
        .expect("the root reads");
    let mine = save_points::Owner {
        uid: me,
        gid: nix::unistd::getgid().as_raw(),
    };
    TEST_PUBLICATION.with(|cell| cell.set(Some((me, mine))));
    let lines =
        publish_from_room(&config, &AgentName("alpha".into()), &[]).expect("the room publishes");
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].digest, digest);
    assert!(!room.join(format!("{digest}.save-point")).exists());
    let _ = std::fs::remove_dir_all(&base);
}

/// **A missing or malformed boundary file refuses naming it, at the verbs
/// that require it and no others**, per Spec section 9: the root still
/// reads, so `unload`, `stop` and `show` keep a running agent's recovery
/// path with no digest in their log lines, and `require_boundary`, which
/// `validate` and `load` call, refuses naming `roles.toml`. Every other
/// value's failure names nothing. Perturbation: refuse the read itself on
/// a damaged boundary file and the first assertion fails.
#[test]
fn the_boundary_file_is_required_only_where_named() {
    use std::os::unix::fs::PermissionsExt;
    let me = nix::unistd::getuid().as_raw();
    let base = std::env::temp_dir().join(format!("weaver-admin-roles-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let root = base.join("alpha");
    write_root(&root);
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).unwrap();
    let named = Err(LifecycleRefusal::ConfigInvalid {
        field: Some(weaver_types::FieldName("roles.toml".into())),
    });
    for (damage, why) in [
        (None, "missing"),
        (Some("trace-reader = \"x\"\nroles = 1\n"), "an unknown key"),
    ] {
        match damage {
            None => std::fs::remove_file(root.join("roles.toml")).unwrap(),
            Some(text) => std::fs::write(root.join("roles.toml"), text).unwrap(),
        }
        let config = load_service_config_at(&base, "alpha", me, nix::unistd::getegid().as_raw())
            .unwrap_or_else(|e| panic!("{why}: the root still reads, got {e:?}"));
        assert_eq!(config.boundary_digest(), None, "{why}: no digest to log");
        assert_eq!(config.require_boundary().map(|_| ()), named, "{why}");
    }
    std::fs::write(root.join("roles.toml"), "trace-reader = \"x\"\n").unwrap();
    std::fs::remove_file(root.join("spu-binary")).unwrap();
    assert_eq!(
        load_service_config_at(&base, "alpha", me, nix::unistd::getegid().as_raw()).err(),
        Some(LifecycleRefusal::ConfigInvalid { field: None }),
        "another value names nothing"
    );
    let _ = std::fs::remove_dir_all(&base);
}

/// **Every key is closed as the root is.** A key a group or the world may
/// write, a key that is a link, and a directory inside the root each refuse
/// `BoundaryUnverified`; a root of closed files reads. Codex on #45, round 7:
/// the directory's own mode let a writable key name the program admin runs
/// as root. Perturbation: drop `judge_entries`, and each case reads.
#[test]
fn every_key_of_the_root_is_closed() {
    use std::os::unix::fs::PermissionsExt;
    let me = nix::unistd::getuid().as_raw();
    let base = std::env::temp_dir().join(format!("weaver-admin-keys-{}", std::process::id()));
    let fresh = || {
        let _ = std::fs::remove_dir_all(&base);
        let root = base.join("alpha");
        write_root(&root);
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).unwrap();
        for entry in std::fs::read_dir(&root).unwrap() {
            let path = entry.unwrap().path();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        }
        root
    };
    let root = fresh();
    assert!(
        load_service_config_at(&base, "alpha", me, nix::unistd::getegid().as_raw()).is_ok(),
        "a closed root reads"
    );
    std::fs::set_permissions(
        root.join("worker-binary"),
        std::fs::Permissions::from_mode(0o666),
    )
    .unwrap();
    assert_eq!(
        load_service_config_at(&base, "alpha", me, nix::unistd::getegid().as_raw()).err(),
        Some(LifecycleRefusal::BoundaryUnverified),
        "a world-writable key"
    );
    let root = fresh();
    std::fs::remove_file(root.join("gate-binary")).unwrap();
    std::fs::write(base.join("elsewhere"), "/opt/weaver/bin/weaver-gate").unwrap();
    std::os::unix::fs::symlink(base.join("elsewhere"), root.join("gate-binary")).unwrap();
    assert_eq!(
        load_service_config_at(&base, "alpha", me, nix::unistd::getegid().as_raw()).err(),
        Some(LifecycleRefusal::BoundaryUnverified),
        "a key that is a link"
    );
    let root = fresh();
    std::fs::create_dir(root.join("stray")).unwrap();
    assert_eq!(
        load_service_config_at(&base, "alpha", me, nix::unistd::getegid().as_raw()).err(),
        Some(LifecycleRefusal::BoundaryUnverified),
        "a directory inside the root"
    );
    let _ = std::fs::remove_dir_all(&base);
}

/// **A malformed name refuses before any path is built from it**, so
/// `.`, `..` and a name carrying `/` never reach the base.
#[test]
fn a_malformed_name_refuses_before_the_base_is_read() {
    for bad in [".", "..", "a/b", "../alpha", "", "al pha"] {
        assert_eq!(
            load_service_config(&AgentName(bad.into()), false).err(),
            Some(LifecycleRefusal::NoSuchAgent),
            "{bad:?}"
        );
    }
}

/// **An optional value is absent only where nothing stands at its path**,
/// **Every path a key names is absolute, and the enter's bound forms a
/// deadline**, per Spec sections 9 and 2: a relative value at each path key
/// fails the read naming the key, as does a `load-bound-seconds` too large
/// to add to the clock. Perturbations: accept a relative path and the
/// root reads with its paths resolved against the working directory;
/// accept any positive count and `u64::MAX` reads, to panic at the
/// enter's wait.
#[test]
fn a_relative_path_or_an_unreachable_bound_fails_the_read() {
    // Held by the guard, so its territory sibling goes with it (#113).
    let root = crate::scratch::Scratch(
        std::env::temp_dir().join(format!("weaver-admin-relative-{}", std::process::id())),
    );
    let fresh = || {
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        write_root(&root);
    };
    for name in [
        "worker-binary",
        "spu-binary",
        "gate-binary",
        "coordination-root",
        "territory",
        "library-path",
    ] {
        fresh();
        std::fs::write(root.join(name), "relative/path").unwrap();
        let failure = load_service_config_from(&root, "alpha")
            .err()
            .unwrap_or_default();
        assert!(
            failure.contains(name) && failure.contains("not an absolute path"),
            "{name}: {failure:?}"
        );
    }
    fresh();
    std::fs::write(root.join("load-bound-seconds"), u64::MAX.to_string()).unwrap();
    let failure = load_service_config_from(&root, "alpha")
        .err()
        .unwrap_or_default();
    assert!(failure.contains("load-bound-seconds"), "{failure:?}");
    fresh();
    std::fs::write(root.join("load-bound-seconds"), "1800").unwrap();
    assert_eq!(
        load_service_config_from(&root, "alpha").unwrap().load_bound,
        std::time::Duration::from_secs(1800)
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// per Spec section 9: at each optional path a directory and bytes that are
/// not UTF-8 and a dangling link fail the read, naming the value, and an
/// absent file leaves the read standing. Perturbations: read every error as
/// absent, as the first form of this loader did, or decide presence by
/// following the link, as the second did, and the failing cases are accepted.
#[test]
fn an_optional_value_that_does_not_read_fails_the_read() {
    // Held by the guard, so its territory sibling goes with it (#113).
    let root = crate::scratch::Scratch(
        std::env::temp_dir().join(format!("weaver-admin-optional-{}", std::process::id())),
    );
    let fresh = || {
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        write_root(&root);
    };
    for name in ["headroom-bytes", "library-path", "load-bound-seconds"] {
        fresh();
        assert!(
            load_service_config_from(&root, "alpha").is_ok(),
            "{name} absent"
        );
        std::fs::create_dir(root.join(name)).unwrap();
        let failure = load_service_config_from(&root, "alpha")
            .err()
            .unwrap_or_default();
        assert!(failure.contains(name), "{name} as a directory: {failure:?}");
        fresh();
        std::fs::write(root.join(name), [0xff, 0xfe, 0x00]).unwrap();
        let failure = load_service_config_from(&root, "alpha")
            .err()
            .unwrap_or_default();
        assert!(failure.contains(name), "{name} not UTF-8: {failure:?}");
        fresh();
        std::os::unix::fs::symlink(root.join("gone"), root.join(name)).unwrap();
        let failure = load_service_config_from(&root, "alpha")
            .err()
            .unwrap_or_default();
        assert!(
            failure.contains(name),
            "{name} a dangling link: {failure:?}"
        );
    }
    let _ = std::fs::remove_dir_all(&root);
}

/// A configuration whose values are never read by the arm under test.
/// `dispatch`'s observation arm touches no field, which is what lets this
/// stand without a filesystem.
fn unread_config() -> ServiceConfig {
    ServiceConfig {
        agent: "alpha".into(),
        coordination_root: PathBuf::from("/nonexistent"),
        worker: PathBuf::from("/nonexistent/bin/worker"),
        spu: PathBuf::from("/nonexistent/python/python-spu.pyz"),
        gate: PathBuf::from("/nonexistent/bin/weaver-gate"),
        headroom_bytes: None,
        library_path: None,
        load_bound: DEFAULT_LOAD_BOUND,
        territory: PathBuf::from("/nonexistent/territory"),
        territory_fd: None,
        save_points: None,
        territory_unjudged: false,
        operator: 1000,
        access_gid: 1000,
        boundary: Ok(BoundaryRead {
            digest: "0".repeat(64),
            reader: "weaver-alpha-admincon".into(),
        }),
        root: PathBuf::from("/nonexistent/root"),
    }
}

#[test]
fn stop_checks_the_name_before_building_a_path() {
    let config = unread_config();
    for hostile in ["../../etc", "alpha/../beta", "a/b", "..", ""] {
        let refused = dispatch(
            &config,
            surface::Request::Stop(AgentName(hostile.to_string())),
        );
        assert_eq!(
            refused,
            Err(LifecycleRefusal::NoSuchAgent),
            "{hostile:?} is refused before any path is built"
        );
    }
    // And a well-formed name that is not this root's agent is refused by
    // the same check, so what a verb may name has one answer.
    let refused = dispatch(&config, surface::Request::Stop(AgentName("beta".into())));
    assert_eq!(refused, Err(LifecycleRefusal::NoSuchAgent));
}

/// The shared check admits exactly the agent whose root was read and
/// refuses every shape that could leave the directory.
#[test]
fn the_name_check_is_one_answer_for_every_verb() {
    let config = unread_config();
    assert_eq!(admissible(&config, &AgentName("alpha".into())), Ok(()));
    for bad in ["alpha/", "/alpha", "al pha", "alpha\u{0}", "ALPHA/../x"] {
        assert_eq!(
            admissible(&config, &AgentName(bad.to_string())),
            Err(LifecycleRefusal::NoSuchAgent),
            "{bad:?} is not admissible"
        );
    }
}

/// **`show` reads the run lock before it dials, and names the run's
/// constituents beside the harness's word**, per Spec section 3 and
/// toddwbucy/WeaverWeb#15. A free lock answers `Unloaded` without
/// dialing, so no state is invented from a missing socket. A lock held by
/// a stand-in constituent with no worker listening answers `Unloaded`
/// with that constituent's pid, a run that never entered, for the caller
/// to end with `unload`. Perturbation: answer the constituents empty and
/// the second case fails; read a held lock as `Idle` and it fails too.
#[test]
fn show_reads_the_lock_and_names_the_constituents() {
    use std::os::unix::process::CommandExt;
    let base = std::env::temp_dir().join(format!("weaver-admin-show-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let mut config = unread_config();
    config.coordination_root = base.clone();
    let run_directory = config.run_directory();
    std::fs::create_dir_all(&run_directory).unwrap();
    assert_eq!(
        show(&config),
        Ok(LifecycleAnswer::State {
            state: weaver_types::AgentState::Unloaded,
            load: None,
            constituents: Vec::new(),
        }),
        "a free lock is no run"
    );
    let lock = start::take_run_lock(&run_directory).unwrap().unwrap();
    let high = start::high(lock.raw()).unwrap();
    let raw = std::os::fd::AsRawFd::as_raw_fd(&high);
    let mut command = std::process::Command::new("/bin/sleep");
    command.arg("30");
    // SAFETY: dup2 is async-signal-safe.
    unsafe {
        command.pre_exec(move || start::place(raw, start::RUN_LOCK_FD));
    }
    let mut child = command.spawn().unwrap();
    drop((high, lock));
    assert_eq!(
        show(&config),
        Ok(LifecycleAnswer::InTransition {
            constituents: vec![child.id()],
        }),
        "a held lock with no worker is no Unloaded: InTransition, naming who holds it"
    );
    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_dir_all(&base);
}

/// **A save point asked of a worker that is gone refuses `OutOfOrder`**
/// (`weaver-admin-Spec` 3.0, S8b x save-point: no worker): the run lock is
/// held by a constituent and no worker listens, so the verb table answers
/// `OutOfOrder` rather than the dial's `Unanswered`. Perturbation: dial the
/// harness without observing, and the verb answers `Unanswered`.
#[test]
fn s8b_save_point_refuses_out_of_order() {
    let (config, _scratch) = scratch_config("s8b-save-point");
    let agent = AgentName("alpha".into());
    let mut holder = stand_in_holder(&config);
    let answered = save_point(&config, &agent);
    let _ = holder.kill();
    let _ = holder.wait();
    assert_eq!(answered, Err(LifecycleRefusal::OutOfOrder));
}

/// **`show` answers by the ladder** (`weaver-admin-Spec` section 3, the
/// ladder; 3.0, the verb table's `show` column), the rungs the tests above
/// leave: (i) the worker's word, `Idle` with its facts, `InTransition` while
/// a leave is pending, and `InTransition` naming the constituents in place
/// of an `Unloaded` while the run lock is held; (ii) a worker that accepts
/// and does not answer, `Unanswered`. Rung (0) is
/// `show_answers_a_transition_in_flight`'s, and (iii) and (iv)
/// `show_reads_the_lock_and_names_the_constituents`'. Perturbation: answer a
/// held run's `Unloaded` word as `Unloaded`, and the third case fails.
#[test]
fn show_ladder_rung_0_to_iv() {
    let state = |state| {
        weaver_types::Payload::Answer(LifecycleAnswer::State {
            state,
            load: None,
            constituents: Vec::new(),
        })
    };
    // (case, the worker's answer or none for a wedged one, the answer for
    // the run lock's holder `pid`)
    type Want = fn(u32) -> Result<LifecycleAnswer, LifecycleRefusal>;
    let cases: [(&str, Option<weaver_types::Payload>, Want); 4] = [
        (
            "(i) the worker's word, the constituents named",
            Some(state(weaver_types::AgentState::Idle)),
            |pid| {
                Ok(LifecycleAnswer::State {
                    state: weaver_types::AgentState::Idle,
                    load: None,
                    constituents: vec![pid],
                })
            },
        ),
        (
            "(i) a leave pending",
            Some(weaver_types::Payload::Answer(
                LifecycleAnswer::InTransition {
                    constituents: Vec::new(),
                },
            )),
            |pid| {
                Ok(LifecycleAnswer::InTransition {
                    constituents: vec![pid],
                })
            },
        ),
        (
            "(i) an Unloaded word under a held run lock",
            Some(state(weaver_types::AgentState::Unloaded)),
            |pid| {
                Ok(LifecycleAnswer::InTransition {
                    constituents: vec![pid],
                })
            },
        ),
        ("(ii) a wedged worker", None, |_| {
            Err(LifecycleRefusal::Unanswered)
        }),
    ];
    for (case, worker, want) in cases {
        let (config, _scratch) = scratch_config("show-ladder");
        let mut holder = stand_in_holder(&config);
        let worker = match worker {
            Some(answer) => {
                answering_worker(&config, vec![answer]);
                None
            }
            None => Some(silent_worker(&config)),
        };
        let answered = show(&config);
        let pid = holder.id();
        let _ = holder.kill();
        let _ = holder.wait();
        drop(worker);
        assert_eq!(answered, want(pid), "{case}");
    }
}

/// A scratch coordination root for one test, with the agent's run
/// directory made inside it as this test's user.
fn scratch_config(tag: &str) -> (ServiceConfig, crate::scratch::Scratch) {
    let base = std::env::temp_dir().join(format!("weaver-admin-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let mut config = unread_config();
    config.coordination_root = base.clone();
    config.root = base.join("root");
    std::fs::create_dir_all(&config.root).unwrap();
    std::fs::create_dir_all(config.run_directory()).unwrap();
    (config, crate::scratch::Scratch(base))
}

/// A stand-in constituent: `sleep` holding the run lock's description at
/// its fixed number, as a member or a worker does.
fn stand_in_holder(config: &ServiceConfig) -> std::process::Child {
    use std::os::unix::process::CommandExt;
    let lock = start::take_run_lock(&config.run_directory())
        .unwrap()
        .expect("a free run lock");
    let high = start::high(lock.raw()).unwrap();
    let raw = std::os::fd::AsRawFd::as_raw_fd(&high);
    let mut command = std::process::Command::new("/bin/sleep");
    command.arg("30");
    // SAFETY: dup2 is async-signal-safe.
    unsafe {
        command.pre_exec(move || start::place(raw, start::RUN_LOCK_FD));
    }
    command.spawn().unwrap()
}

/// A stand-in worker that binds the coordination socket and accepts, and
/// then answers nothing: a silent run.
fn silent_worker(config: &ServiceConfig) -> std::os::fd::OwnedFd {
    let socket = config.coordination_socket();
    std::fs::create_dir_all(socket.parent().unwrap()).unwrap();
    let listener = nix::sys::socket::socket(
        nix::sys::socket::AddressFamily::Unix,
        nix::sys::socket::SockType::SeqPacket,
        nix::sys::socket::SockFlag::SOCK_CLOEXEC,
        None,
    )
    .unwrap();
    let address = nix::sys::socket::UnixAddr::new(&socket).unwrap();
    nix::sys::socket::bind(std::os::fd::AsRawFd::as_raw_fd(&listener), &address).unwrap();
    nix::sys::socket::listen(&listener, nix::sys::socket::Backlog::new(8).unwrap()).unwrap();
    listener
}

/// **The coordination root and every directory above it are held closed
/// before the run directory is made**, per Spec section 3: an open root,
/// sticky or not, or an open directory above it, refuses
/// `BoundaryUnverified` and makes nothing, and a closed one admits and
/// makes the run directory.
/// Judged against this test's uid, production's owner being uid 0.
/// Perturbation: drop the judgment from `prepare_run_directory` and the
/// open cases make the run directory.
#[test]
fn the_coordination_root_is_judged_before_any_lock() {
    use std::os::unix::fs::PermissionsExt;
    let me = nix::unistd::getuid().as_raw();
    let base = std::env::temp_dir().join(format!("weaver-admin-coord-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let root = base.join("run");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::set_permissions(&base, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut config = unread_config();
    config.coordination_root = root.clone();
    let mode = |path: &std::path::Path, bits| {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(bits)).unwrap()
    };
    for open in [0o775, 0o757, 0o777, 0o1777] {
        mode(&root, open);
        assert_eq!(
            prepare_run_directory(&config, me).err(),
            Some(LifecycleRefusal::BoundaryUnverified),
            "a coordination root of mode {open:o}, sticky or not"
        );
        assert!(
            !root.join("weaver.run").exists(),
            "and nothing is made in it"
        );
    }
    mode(&root, 0o755);
    assert!(
        prepare_run_directory(&config, me).is_ok(),
        "a closed root admits"
    );
    assert!(
        config.run_directory().is_dir(),
        "and the run directory stands"
    );
    mode(&base, 0o777);
    assert_eq!(
        prepare_run_directory(&config, me).err(),
        Some(LifecycleRefusal::BoundaryUnverified),
        "an open directory above the root"
    );
    mode(&base, 0o755);
    let _ = std::fs::remove_dir_all(&base);
}

/// **The trace reader is judged at `validate` and `load`**, per Spec
/// section 9: the agent's own account or its member's refuses naming
/// `roles.toml`, a reader the box does not carry refuses the same way, a
/// reader outside the access group refuses too, and a box with no access
/// group refuses `BoundaryUnverified`. A reader holding the group by its
/// primary gid or by membership is admitted. Perturbation: drop the
/// membership test and the outsider is admitted.
#[test]
fn the_trace_reader_is_judged() {
    let agent = AgentName("alpha".into());
    let named = Err(LifecycleRefusal::ConfigInvalid {
        field: Some(weaver_types::FieldName("roles.toml".into())),
    });
    let group = || Some((5000, vec!["weaver-alpha-admincon".to_string()]));
    assert_eq!(
        reader_verdict("weaver-alpha", &agent, Some(5000), group()),
        named
    );
    assert_eq!(
        reader_verdict("weaver-alpha-state", &agent, Some(5000), group()),
        named
    );
    assert_eq!(
        reader_verdict("ghost", &agent, None, group()),
        named,
        "no such user"
    );
    assert_eq!(
        reader_verdict("outsider", &agent, Some(100), group()),
        named,
        "not in the group"
    );
    assert_eq!(
        reader_verdict("weaver-alpha-admincon", &agent, Some(100), None),
        Err(LifecycleRefusal::BoundaryUnverified),
        "the box carries no access group"
    );
    assert_eq!(
        reader_verdict("weaver-alpha-admincon", &agent, Some(100), group()),
        Ok(())
    );
    assert_eq!(
        reader_verdict("primary", &agent, Some(5000), group()),
        Ok(()),
        "by primary gid"
    );
}

/// A stand-in worker that answers each dial with the next scripted payload:
/// it accepts, reads the directive, and closes the exchange with the
/// answer, one connection per answer, as admin dials once per exchange.
fn answering_worker(
    config: &ServiceConfig,
    answers: Vec<weaver_types::Payload>,
) -> std::thread::JoinHandle<()> {
    let listener = silent_worker(config);
    std::thread::spawn(move || {
        for payload in answers {
            let Ok(raw) = nix::sys::socket::accept(std::os::fd::AsRawFd::as_raw_fd(&listener))
            else {
                return;
            };
            // SAFETY: accept answered a fresh descriptor this thread owns.
            let fd = unsafe { <std::os::fd::OwnedFd as std::os::fd::FromRawFd>::from_raw_fd(raw) };
            let peer = channel::Coordination::adopt(fd);
            let Ok(request) = peer.recv() else { return };
            let _ = peer.send(&weaver_types::OrganEnvelope {
                exchange: request.exchange,
                position: weaver_types::Position::Close,
                payload,
            });
        }
    })
}

/// Short bounds for the unload path, the production values being fixed.
/// A worker that answers as `answering_worker` does and keeps every
/// directive it was sent, so a test reads what admin directed.
fn recording_worker(
    config: &ServiceConfig,
    answers: Vec<weaver_types::Payload>,
) -> std::thread::JoinHandle<Vec<weaver_types::Payload>> {
    let listener = silent_worker(config);
    std::thread::spawn(move || {
        let mut directed = Vec::new();
        for payload in answers {
            let Ok(raw) = nix::sys::socket::accept(std::os::fd::AsRawFd::as_raw_fd(&listener))
            else {
                break;
            };
            let fd = unsafe { <std::os::fd::OwnedFd as std::os::fd::FromRawFd>::from_raw_fd(raw) };
            let peer = channel::Coordination::adopt(fd);
            let Ok(request) = peer.recv() else { break };
            directed.push(request.payload);
            let _ = peer.send(&weaver_types::OrganEnvelope {
                exchange: request.exchange,
                position: weaver_types::Position::Close,
                payload,
            });
        }
        directed
    })
}

fn report() -> weaver_types::SavePointReport {
    weaver_types::SavePointReport {
        save_point: "ab".into(),
        name: "ab.save-point".into(),
        run: weaver_types::RunId("r-1".into()),
        sequence: 5,
        turn: 1,
        event_run: weaver_types::RunId("r-1".into()),
        position: 7,
    }
}

/// **An unload whose leave save point is not taken does not complete**,
/// per Spec section 3 on the operator's ruling of 2026-10-06 on #1 (A3.0
/// item 6): the harness's `SavePointNotTaken` returns as the verb's
/// refusal, no escalation ends the run, and the holder keeps the run
/// lock; **`force-unload` directs the leave with `forced`** and completes,
/// the escalation ending what still holds the lock. Perturbations: treat
/// the refusal as silence and the first case escalates, the holder
/// ending; send `forced: false` from `force_unload` and the second
/// assertion fails.
#[test]
fn an_unload_without_its_save_point_stops_and_a_forced_one_completes() {
    let (config, _scratch) = scratch_config("unload-stops");
    let mut holder = stand_in_holder(&config);
    let worker = recording_worker(
        &config,
        vec![
            weaver_types::Payload::Answer(LifecycleAnswer::State {
                state: weaver_types::AgentState::Idle,
                load: None,
                constituents: Vec::new(),
            }),
            weaver_types::Payload::Refusal(LifecycleRefusal::SavePointNotTaken {
                missed: weaver_types::SavePointLeg::Finished,
            }),
        ],
    );
    assert_eq!(
        unload_within(&config, TEST_UNLOAD_BOUNDS, false),
        Err(LifecycleRefusal::SavePointNotTaken {
            missed: weaver_types::SavePointLeg::Finished,
        })
    );
    let directed = worker.join().unwrap();
    assert!(matches!(
        directed[1],
        weaver_types::Payload::Directive(LifecycleDirective::Leave { forced: false, .. })
    ));
    assert!(
        start::run_lock_held(&config.run_directory()).unwrap(),
        "the run stays open, its lock held"
    );
    assert!(
        holder.try_wait().unwrap().is_none(),
        "the holder was not ended"
    );

    // The first worker's name goes before the second binds it, and an
    // open marker stands, which the forced unload closes as forced even
    // through the escalation (Codex on #94, round 1).
    let _ = std::fs::remove_file(config.coordination_socket());
    save_points::write_marker(
        &config.root,
        Some(&save_points::Marker::Open { run: "r-1".into() }),
    )
    .unwrap();
    let worker = recording_worker(
        &config,
        vec![
            weaver_types::Payload::Answer(LifecycleAnswer::State {
                state: weaver_types::AgentState::Idle,
                load: None,
                constituents: Vec::new(),
            }),
            weaver_types::Payload::Answer(LifecycleAnswer::Left {
                save_point: None,
                forced: true,
                no_state: false,
            }),
        ],
    );
    assert_eq!(
        unload_within(&config, TEST_UNLOAD_BOUNDS, true),
        Ok(LifecycleAnswer::State {
            state: weaver_types::AgentState::Unloaded,
            load: None,
            constituents: Vec::new(),
        })
    );
    let directed = worker.join().unwrap();
    assert!(
        matches!(
            directed[1],
            weaver_types::Payload::Directive(LifecycleDirective::Leave { forced: true, .. })
        ),
        "the forced unload directs a forced leave: {directed:?}"
    );
    let _ = holder.wait();
    assert!(!start::run_lock_held(&config.run_directory()).unwrap());
    assert_eq!(
        save_points::read_marker(&config.root),
        Some(save_points::Marker::Forced { run: "r-1".into() }),
        "the escalated forced unload still closes the marker as forced"
    );

    // **A marker that cannot be written refuses the unload** (Codex on
    // #94, round 2, the load's class at the other end): the root made
    // unwritable, an open marker standing, the forced unload ends the
    // run and answers `BoundaryUnverified` rather than unloaded.
    // Perturbation: log the failed write and answer unloaded again.
    let _ = std::fs::remove_file(config.coordination_socket());
    save_points::write_marker(
        &config.root,
        Some(&save_points::Marker::Open { run: "r-2".into() }),
    )
    .unwrap();
    let mut holder = stand_in_holder(&config);
    let worker = recording_worker(
        &config,
        vec![
            weaver_types::Payload::Answer(LifecycleAnswer::State {
                state: weaver_types::AgentState::Idle,
                load: None,
                constituents: Vec::new(),
            }),
            weaver_types::Payload::Answer(LifecycleAnswer::Left {
                save_point: None,
                forced: true,
                no_state: false,
            }),
        ],
    );
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&config.root, std::fs::Permissions::from_mode(0o500)).unwrap();
    }
    let answered = unload_within(&config, TEST_UNLOAD_BOUNDS, true);
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&config.root, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let _ = worker.join();
    let _ = holder.wait();
    if nix::unistd::getuid().is_root() {
        // Root writes through the mode; the case cannot stand as root.
        assert!(answered.is_ok());
    } else {
        assert_eq!(answered, Err(LifecycleRefusal::BoundaryUnverified));
        assert_eq!(
            save_points::read_marker(&config.root),
            Some(save_points::Marker::Open { run: "r-2".into() }),
            "the marker stands as it was"
        );
    }
}

/// **The `save-point` verb asks the running worker and answers the
/// report only once it is published**, per Spec sections 2 and 6: one
/// directive, one answer, out of order where no run stands, a refusal
/// where the publication cannot be made, and the report where it is: with
/// the reported file in a scratch territory's room, the verb answers
/// `SavePointTaken` and the manifest holds its line as arrived on demand
/// at the event's position. Perturbations: drop the run-lock check and
/// the second case dials an absent worker and answers `Unanswered`
/// instead; answer the report whatever the publication did and the third
/// assertion sees `SavePointTaken`; refuse whatever the publication did
/// and the last case refuses.
#[test]
fn the_save_point_verb_answers_the_report_and_is_out_of_order_without_a_run() {
    let (config, _scratch) = scratch_config("save-point-verb");
    let agent = AgentName("alpha".into());
    assert_eq!(
        save_point(&config, &agent),
        Err(LifecycleRefusal::OutOfOrder),
        "no run, no save point"
    );
    let mut holder = stand_in_holder(&config);
    let worker = recording_worker(
        &config,
        vec![
            weaver_types::Payload::Answer(LifecycleAnswer::State {
                state: weaver_types::AgentState::Idle,
                load: None,
                constituents: Vec::new(),
            }),
            weaver_types::Payload::Answer(LifecycleAnswer::SavePointTaken { report: report() }),
        ],
    );
    // This box holds no member account and the fixture no room, so
    // nothing is published and the verb refuses rather than answering a
    // report of a save point that stands in the room alone (Codex on
    // #94, round 1); the directive reached the worker all the same.
    // A save point taken and not published is named so (the #94
    // survey's S12), as the unload names it.
    let answered = save_point(&config, &agent);
    assert_eq!(
        answered,
        Err(LifecycleRefusal::SavePointNotTaken {
            missed: weaver_types::SavePointLeg::Published,
        }),
        "answers only a published save point"
    );
    let directed = worker.join().unwrap();
    // The verb observes first, by the verb table (3.0), then directs.
    assert!(matches!(
        directed[0],
        weaver_types::Payload::Directive(LifecycleDirective::Observe)
    ));
    assert!(matches!(
        directed[1],
        weaver_types::Payload::Directive(LifecycleDirective::SavePoint { .. })
    ));
    let _ = holder.kill();
    let _ = holder.wait();
    // The reported file stands in the territory's room: it publishes,
    // and the verb answers the report.
    let (config, report, base) = territory_with_one_room_file("save-point-verb-published");
    let mut holder = stand_in_holder(&config);
    let worker = recording_worker(
        &config,
        vec![
            weaver_types::Payload::Answer(LifecycleAnswer::State {
                state: weaver_types::AgentState::Idle,
                load: None,
                constituents: Vec::new(),
            }),
            weaver_types::Payload::Answer(LifecycleAnswer::SavePointTaken {
                report: report.clone(),
            }),
        ],
    );
    let answered = save_point(&config, &agent);
    let _ = worker.join();
    let _ = holder.kill();
    let _ = holder.wait();
    assert_eq!(
        answered,
        Ok(LifecycleAnswer::SavePointTaken {
            report: report.clone()
        })
    );
    let lines = manifest_lines(&config);
    assert_eq!(lines.len(), 1);
    assert_eq!(
        (
            lines[0].digest.as_str(),
            lines[0].arrived,
            lines[0].position.clone()
        ),
        (
            report.save_point.as_str(),
            save_points::Arrival::Demand,
            Some((report.event_run.0.clone(), report.position))
        )
    );
    let _ = std::fs::remove_dir_all(&base);
}

/// **A force refused before `Left` answers at once** (the #99 area 1
/// review, K3): a worker refusing the forced leave `ActivityNotAtRest`,
/// its run plainly standing, is answered without the after-left wait.
/// Perturbation: wait on every refusal again and the verb takes the
/// whole wait.
#[test]
fn a_force_refused_before_left_answers_without_the_wait() {
    let (config, _scratch) = scratch_config("forced-not-at-rest");
    let mut holder = stand_in_holder(&config);
    let worker = recording_worker(
        &config,
        vec![
            weaver_types::Payload::Answer(LifecycleAnswer::State {
                state: weaver_types::AgentState::Active,
                load: None,
                constituents: Vec::new(),
            }),
            weaver_types::Payload::Refusal(LifecycleRefusal::ActivityNotAtRest),
        ],
    );
    let bounds = UnloadBounds {
        after_left: std::time::Duration::from_secs(5),
        ..TEST_UNLOAD_BOUNDS
    };
    let started = std::time::Instant::now();
    let answered = unload_within(&config, bounds, true);
    let took = started.elapsed();
    let _ = worker.join();
    let _ = holder.kill();
    let _ = holder.wait();
    assert_eq!(answered, Err(LifecycleRefusal::ActivityNotAtRest));
    assert!(
        took < std::time::Duration::from_secs(3),
        "answered in {took:?}, inside the five-second wait"
    );
}

/// **A force does not depend on the territory** (the operator's ruling of
/// 2026-10-08 on #99, K5): with the territory at a mode the judgment
/// refuses, every other verb refuses at the configuration, while the
/// force's configuration stands marked unjudged; the force over a run
/// already ended publishes nothing from the room, writes no `admin.log`
/// line, and closes the marker forced. Perturbation: judge the territory
/// for the force too and its configuration refuses.
#[test]
fn a_force_ends_the_run_where_the_territory_does_not_judge() {
    use std::os::unix::fs::PermissionsExt;
    let me = nix::unistd::getuid().as_raw();
    let mine = nix::unistd::getegid().as_raw();
    let base = std::env::temp_dir().join(format!(
        "weaver-admin-force-unjudged-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&base);
    let root = base.join("alpha");
    let territory = write_root(&root);
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).unwrap();
    let room = territory.join(save_points::ROOM);
    std::fs::create_dir(&room).unwrap();
    let bytes = save_points::tests::save_point("r-1", 1, 0, 1_000_000_000, b"room");
    let digest = save_points::judge(&bytes).unwrap().digest;
    std::fs::write(room.join(format!("{digest}.save-point")), &bytes).unwrap();
    std::fs::set_permissions(&territory, std::fs::Permissions::from_mode(0o711)).unwrap();
    let groups = TerritoryGroups {
        access: mine,
        state: mine,
    };
    assert_eq!(
        load_service_config_judged(&base, "alpha", me, Some(groups), false).err(),
        Some(LifecycleRefusal::BoundaryUnverified),
        "every other verb refuses the drifted territory"
    );
    let mut config = load_service_config_judged(&base, "alpha", me, Some(groups), true)
        .expect("the force's configuration stands");
    assert!(config.territory_unjudged);
    config.coordination_root = base.join("coordination");
    std::fs::create_dir_all(config.run_directory()).unwrap();
    save_points::write_marker(
        &config.root,
        Some(&save_points::Marker::Open { run: "r-1".into() }),
    )
    .unwrap();
    // The publication runs as this uid, so only the unjudged mark keeps
    // it from the room.
    TEST_PUBLICATION.with(|cell| {
        cell.set(Some((
            me,
            save_points::Owner {
                uid: me,
                gid: nix::unistd::getgid().as_raw(),
            },
        )))
    });
    assert_eq!(
        unload_within(&config, TEST_UNLOAD_BOUNDS, true),
        Ok(unloaded_answer())
    );
    record(&config, "force-unload", "unloaded");
    assert_eq!(
        save_points::read_marker(&config.root),
        Some(save_points::Marker::Forced { run: "r-1".into() })
    );
    assert!(
        room.join(format!("{digest}.save-point")).exists(),
        "nothing is published from the unjudged territory"
    );
    assert!(
        !territory.join("admin.log").exists(),
        "nothing is written there"
    );
    // Without its groups the force still stands.
    assert!(
        load_service_config_judged(&base, "alpha", me, None, true)
            .expect("stands")
            .territory_unjudged
    );
    let _ = std::fs::remove_dir_all(&base);
}

/// **A forced verb closes the marker where the run already ended**, per
/// Spec section 3 (Codex on #94, round 6): with the lock free and an open
/// marker standing, `force-unload` answers unloaded and the marker reads
/// forced, while `unload` leaves it open as the unclean stop it is; and a
/// forced leave refused by a harness that then goes down closes the marker
/// as forced inside the after-left wait, the refusal still returned.
/// Perturbations: skip the early branch's close and the first marker
/// stays open; return the refusal without the wait and the third does.
#[test]
fn a_forced_verb_closes_the_marker_where_the_run_already_ended() {
    let (config, _scratch) = scratch_config("forced-ended");
    let open = |run: &str| save_points::Marker::Open { run: run.into() };
    save_points::write_marker(&config.root, Some(&open("r-1"))).unwrap();
    let answered = unload_within(&config, TEST_UNLOAD_BOUNDS, false);
    assert!(matches!(
        answered,
        Ok(LifecycleAnswer::State {
            state: weaver_types::AgentState::Unloaded,
            ..
        })
    ));
    assert_eq!(
        save_points::read_marker(&config.root),
        Some(open("r-1")),
        "an unforced verb leaves the unclean stop for the next load"
    );
    let answered = unload_within(&config, TEST_UNLOAD_BOUNDS, true);
    assert!(matches!(
        answered,
        Ok(LifecycleAnswer::State {
            state: weaver_types::AgentState::Unloaded,
            ..
        })
    ));
    assert_eq!(
        save_points::read_marker(&config.root),
        Some(save_points::Marker::Forced { run: "r-1".into() }),
        "the forced verb records the operator's choice"
    );
    // The refusal arm: the holder keeps the lock while the worker
    // refuses the leave past `Left`, a drain that failed being one such
    // refusal, then goes down inside the after-left wait. A refusal
    // answered before `Left` is never waited on (K3).
    save_points::write_marker(&config.root, Some(&open("r-2"))).unwrap();
    let _ = std::fs::remove_file(config.coordination_socket());
    let mut holder = stand_in_holder(&config);
    let worker = recording_worker(
        &config,
        vec![
            weaver_types::Payload::Answer(LifecycleAnswer::State {
                state: weaver_types::AgentState::Idle,
                load: None,
                constituents: Vec::new(),
            }),
            weaver_types::Payload::Refusal(LifecycleRefusal::DescriptorsUnusable),
        ],
    );
    // **The holder goes down once the leave is answered** (#113, section
    // 1): it was a 100 ms sleep, which under load could outrun the 300 ms
    // after-left wait. The worker's thread ends once it has answered the
    // observation and the leave, so the kill follows the refusal; the wait
    // on it is bounded, so a regression fails rather than hangs.
    let (answered_leave, leave_answered) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        let directed = worker.join();
        let _ = answered_leave.send(());
        directed
    });
    let ender = std::thread::spawn(move || {
        let in_time = leave_answered
            .recv_timeout(std::time::Duration::from_secs(5))
            .is_ok();
        let _ = holder.kill();
        let _ = holder.wait();
        in_time
    });
    let answered = unload_within(&config, TEST_UNLOAD_BOUNDS, true);
    assert!(
        ender.join().unwrap(),
        "the worker answered the leave inside the bound"
    );
    let _ = worker.join();
    assert!(matches!(
        answered,
        Err(LifecycleRefusal::DescriptorsUnusable)
    ));
    assert_eq!(
        save_points::read_marker(&config.root),
        Some(save_points::Marker::Forced { run: "r-2".into() }),
        "the refusal after the run ended still closed the marker as forced"
    );
}

/// **The member's group set is its own group alone**, per Spec section 6
/// on the operator's ruling of 2026-10-08 on #1 (the custody audit's
/// G11): the territory is `0710` under the state group, the member's own
/// primary group, so the member needs no other group to reach its room,
/// and the access group, which reads the logs and the
/// published save points, is not the member's. Perturbation: put the
/// access group back beside the member's own and the assertion fails.
#[test]
fn the_members_group_set_carries_no_access_group() {
    let member = inventory::MemberAccount {
        uid: 1501,
        gid: 1501,
    };
    assert_eq!(member_groups(member), [1501]);
}

/// **A clean unload whose leave save point did not publish does not
/// complete**, per Spec section 3 on A3.0 item 6 (Codex on #94, round 9):
/// the worker answers `Left` naming a save point, the publication refuses
/// (here the scratch inventory's refusal; on a box, a room file past the
/// bound or any publication that does not land), the verb refuses
/// `SavePointNotTaken` naming the publication and the marker stays open,
/// so the next load records `NoCleanUnload`. The forced unload's case,
/// no save point reported, is `a_forced_verb_closes_the_marker_where_the_run_already_ended`.
/// Perturbation: discard the publication's result again and the verb
/// answers unloaded with the marker closed.
#[test]
fn a_clean_unload_whose_save_point_did_not_publish_stops() {
    let (config, _scratch) = scratch_config("unload-unpublished");
    save_points::write_marker(
        &config.root,
        Some(&save_points::Marker::Open { run: "r-1".into() }),
    )
    .unwrap();
    let mut holder = stand_in_holder(&config);
    let worker = recording_worker(
        &config,
        vec![
            weaver_types::Payload::Answer(LifecycleAnswer::State {
                state: weaver_types::AgentState::Idle,
                load: None,
                constituents: Vec::new(),
            }),
            weaver_types::Payload::Answer(LifecycleAnswer::Left {
                save_point: Some(report()),
                forced: false,
                no_state: false,
            }),
        ],
    );
    let ender = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(100));
        let _ = holder.kill();
        let _ = holder.wait();
    });
    let answered = unload_within(&config, TEST_UNLOAD_BOUNDS, false);
    ender.join().unwrap();
    let _ = worker.join();
    assert_eq!(
        answered,
        Err(LifecycleRefusal::SavePointNotTaken {
            missed: weaver_types::SavePointLeg::Published,
        })
    );
    assert_eq!(
        save_points::read_marker(&config.root),
        Some(save_points::Marker::Open { run: "r-1".into() }),
        "the marker stays open for the next load's reset"
    );
    // The helper's own cases: a reported digest among the lines passes,
    // one missing or a refused publication names it, none reported passes.
    let line = save_points::ManifestLine {
        ordinal: 1,
        digest: "ab".into(),
        name: "x".into(),
        stamp: save_points::Stamp {
            run: "r-1".into(),
            sequence: 5,
            turn: 1,
            schema: String::new(),
            wall_ns: 0,
        },
        position: None,
        arrived: save_points::Arrival::Leave,
    };
    let reports = vec![(report(), save_points::Arrival::Leave)];
    assert_eq!(unpublished_leave(&reports, &Ok(vec![line])), None);
    assert_eq!(unpublished_leave(&reports, &Ok(vec![])), Some("ab".into()));
    assert_eq!(
        unpublished_leave(&reports, &Err(LifecycleRefusal::BoundaryUnverified)),
        Some("ab".into())
    );
    assert_eq!(
        unpublished_leave(&[], &Err(LifecycleRefusal::BoundaryUnverified)),
        None
    );
}

/// A configuration over a judged scratch territory whose room holds one
/// sound finished save point, with a run directory beside it and the
/// publication run as this test's uid; answers the config, the room
/// file's report and the base to remove.
fn territory_with_one_room_file(
    tag: &str,
) -> (
    ServiceConfig,
    weaver_types::SavePointReport,
    std::path::PathBuf,
) {
    use std::os::unix::fs::PermissionsExt;
    let me = nix::unistd::getuid().as_raw();
    let base = std::env::temp_dir().join(format!("weaver-admin-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let root = base.join("alpha");
    let territory = write_root(&root);
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).unwrap();
    let room = territory.join(save_points::ROOM);
    std::fs::create_dir(&room).unwrap();
    let bytes = save_points::tests::save_point("r-1", 5, 1, 1_000_000_000, b"room");
    let digest = save_points::judge(&bytes).unwrap().digest;
    std::fs::write(room.join(format!("{digest}.save-point")), &bytes).unwrap();
    let mut config =
        load_service_config_at(&base, "alpha", me, nix::unistd::getegid().as_raw()).unwrap();
    config.coordination_root = base.join("coordination");
    std::fs::create_dir_all(config.run_directory()).unwrap();
    TEST_PUBLICATION.with(|cell| {
        cell.set(Some((
            me,
            save_points::Owner {
                uid: me,
                gid: nix::unistd::getgid().as_raw(),
            },
        )))
    });
    let report = weaver_types::SavePointReport {
        save_point: digest.clone(),
        name: format!("{digest}.save-point"),
        ..report()
    };
    (config, report, base)
}

/// The lines the scratch territory's manifest holds.
fn manifest_lines(config: &ServiceConfig) -> Vec<save_points::ManifestLine> {
    let me = nix::unistd::getuid().as_raw();
    save_points::read_manifest(
        config.save_points_fd().unwrap(),
        save_points::Owner {
            uid: me,
            gid: nix::unistd::getgid().as_raw(),
        },
    )
    .unwrap()
}

/// The digests the scratch territory's manifest names.
fn manifest_digests(config: &ServiceConfig) -> Vec<String> {
    manifest_lines(config)
        .into_iter()
        .map(|line| line.digest)
        .collect()
}

/// **A leave whose lock outlives the after-left wait keeps its save
/// point** (the #94 survey's S7): the worker answers `Left` naming the
/// room's file, a holder outlives the wait and is ended by the
/// escalation, and the report is published before `Unloaded`, its line
/// arrived at the leave with the event's position, the marker closing
/// clean; and where the reported file is not in the room, the unload
/// refuses `SavePointNotTaken` naming `published` and the marker stays
/// open. Perturbation: drop the report on the escalation's path again
/// and the first file publishes as recovered with no position, and the
/// second unload answers unloaded over a marker closed clean.
#[test]
fn a_leave_ended_by_the_escalation_still_publishes_its_save_point() {
    let escalated_leave = |config: &ServiceConfig, report: &weaver_types::SavePointReport| {
        save_points::write_marker(
            &config.root,
            Some(&save_points::Marker::Open { run: "r-1".into() }),
        )
        .unwrap();
        let mut holder = stand_in_holder(config);
        let worker = recording_worker(
            config,
            vec![
                weaver_types::Payload::Answer(LifecycleAnswer::State {
                    state: weaver_types::AgentState::Idle,
                    load: None,
                    constituents: Vec::new(),
                }),
                weaver_types::Payload::Answer(LifecycleAnswer::Left {
                    save_point: Some(report.clone()),
                    forced: false,
                    no_state: false,
                }),
            ],
        );
        let answered = unload_within(config, TEST_UNLOAD_BOUNDS, false);
        let _ = holder.kill();
        let _ = holder.wait();
        let _ = worker.join();
        answered
    };
    let (config, report, base) = territory_with_one_room_file("left-escalated");
    assert_eq!(escalated_leave(&config, &report), Ok(unloaded_answer()));
    let lines = manifest_lines(&config);
    assert_eq!(lines.len(), 1);
    assert_eq!(
        (
            lines[0].digest.as_str(),
            lines[0].arrived,
            lines[0].position.clone()
        ),
        (
            report.save_point.as_str(),
            save_points::Arrival::Leave,
            Some((report.event_run.0.clone(), report.position))
        ),
        "published as the leave's, at the event's position"
    );
    assert_eq!(
        save_points::read_marker(&config.root),
        Some(save_points::Marker::Closed { run: "r-1".into() })
    );
    let _ = std::fs::remove_dir_all(&base);
    // The reported file absent from the room: the leave's save point
    // did not publish, so the unload does not complete.
    let (config, report, base) = territory_with_one_room_file("left-escalated-absent");
    let room = config.territory.join(save_points::ROOM);
    std::fs::remove_file(room.join(&report.name)).unwrap();
    assert_eq!(
        escalated_leave(&config, &report),
        Err(LifecycleRefusal::SavePointNotTaken {
            missed: weaver_types::SavePointLeg::Published,
        })
    );
    assert!(manifest_lines(&config).is_empty());
    assert_eq!(
        save_points::read_marker(&config.root),
        Some(save_points::Marker::Open { run: "r-1".into() }),
        "the marker stays open for the next load's reset"
    );
    let _ = std::fs::remove_dir_all(&base);
}

/// **A retry on a run already ended publishes the room first** (the #94
/// survey's S11): after an unload refused `published`, the leave's save
/// point stands in the room; a `force-unload` with the run lock free
/// publishes it and closes the marker as forced, never clean, since a
/// published file proves no leave (Codex on #94 at 197e80b).
/// Perturbation: drop the publication from the lock-free branch and the
/// file stays in the room.
#[test]
fn a_retry_on_an_ended_run_publishes_the_room_first() {
    let (config, report, base) = territory_with_one_room_file("ended-retry");
    save_points::write_marker(
        &config.root,
        Some(&save_points::Marker::Open { run: "r-1".into() }),
    )
    .unwrap();
    assert_eq!(
        unload_within(&config, TEST_UNLOAD_BOUNDS, true),
        Ok(unloaded_answer())
    );
    assert_eq!(manifest_digests(&config), [report.save_point]);
    assert_eq!(
        save_points::read_marker(&config.root),
        Some(save_points::Marker::Forced { run: "r-1".into() })
    );
    let _ = std::fs::remove_dir_all(&base);
}

/// **A run ended with its own on-demand save point in the room
/// concludes open** (Codex on #94 at 197e80b): the file is
/// this run's, its stamp naming the run the marker stands open on, but
/// no leave answered, so a plain unload publishes it and the marker
/// stays open for the next load's `NoCleanUnload`. Perturbation: close
/// the marker clean on this run's own file and it reads closed.
#[test]
fn a_run_ended_with_its_own_demand_file_concludes_open() {
    let (config, report, base) = territory_with_one_room_file("own-demand");
    save_points::write_marker(
        &config.root,
        Some(&save_points::Marker::Open { run: "r-1".into() }),
    )
    .unwrap();
    assert_eq!(
        unload_within(&config, TEST_UNLOAD_BOUNDS, false),
        Ok(unloaded_answer())
    );
    assert_eq!(manifest_digests(&config), [report.save_point]);
    assert_eq!(
        save_points::read_marker(&config.root),
        Some(save_points::Marker::Open { run: "r-1".into() })
    );
    let _ = std::fs::remove_dir_all(&base);
}

/// **A file of an older run makes no run clean** (the push review of
/// 197e80b): a run that crashed with only an older run's file in its
/// room publishes it as recovered, and the marker stays open on the
/// crashed run, so the next load records `NoCleanUnload`. Perturbation:
/// close the marker on any publication and it reads closed.
#[test]
fn a_recovered_file_of_an_older_run_leaves_the_marker_open() {
    let (config, report, base) = territory_with_one_room_file("older-run");
    save_points::write_marker(
        &config.root,
        Some(&save_points::Marker::Open { run: "r-2".into() }),
    )
    .unwrap();
    assert_eq!(
        unload_within(&config, TEST_UNLOAD_BOUNDS, false),
        Ok(unloaded_answer())
    );
    assert_eq!(manifest_digests(&config), [report.save_point]);
    assert_eq!(
        save_points::read_marker(&config.root),
        Some(save_points::Marker::Open { run: "r-2".into() })
    );
    let _ = std::fs::remove_dir_all(&base);
}

/// **A leave's save point of another run makes no run clean** (the push
/// review of 197e80b): the worker reports a save point whose event run
/// is not the run the marker stands open on; it publishes, the verb
/// answers unloaded, and the marker stays open for the next load's
/// reset. The run compared is the event's, never the covered run: a run
/// restored and left with no turn covers the prior run's position, so a
/// report covering `r-0` whose event is `r-1`, with the marker open on
/// `r-1`, closes it clean. Perturbations: close the marker whatever the
/// report's run and the first reads closed; compare the covered run and
/// the second stays open.
#[test]
fn a_leave_reporting_another_runs_save_point_leaves_the_marker_open() {
    let leave =
        |config: &ServiceConfig, report: &weaver_types::SavePointReport, marker_run: &str| {
            save_points::write_marker(
                &config.root,
                Some(&save_points::Marker::Open {
                    run: marker_run.into(),
                }),
            )
            .unwrap();
            let mut holder = stand_in_holder(config);
            let worker = recording_worker(
                config,
                vec![
                    weaver_types::Payload::Answer(LifecycleAnswer::State {
                        state: weaver_types::AgentState::Idle,
                        load: None,
                        constituents: Vec::new(),
                    }),
                    weaver_types::Payload::Answer(LifecycleAnswer::Left {
                        save_point: Some(report.clone()),
                        forced: false,
                        no_state: false,
                    }),
                ],
            );
            let ender = std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(100));
                let _ = holder.kill();
                let _ = holder.wait();
            });
            let answered = unload_within(config, TEST_UNLOAD_BOUNDS, false);
            ender.join().unwrap();
            let _ = worker.join();
            answered
        };
    let (config, report, base) = territory_with_one_room_file("other-run-leave");
    assert_eq!(leave(&config, &report, "r-2"), Ok(unloaded_answer()));
    assert_eq!(manifest_digests(&config), [report.save_point]);
    assert_eq!(
        save_points::read_marker(&config.root),
        Some(save_points::Marker::Open { run: "r-2".into() })
    );
    let _ = std::fs::remove_dir_all(&base);
    // Covering the prior run, the event this run's: the marker closes.
    let (config, report, base) = territory_with_one_room_file("restored-run-leave");
    let report = weaver_types::SavePointReport {
        run: weaver_types::RunId("r-0".into()),
        event_run: weaver_types::RunId("r-1".into()),
        ..report
    };
    assert_eq!(leave(&config, &report, "r-1"), Ok(unloaded_answer()));
    assert_eq!(manifest_digests(&config), [report.save_point]);
    assert_eq!(
        save_points::read_marker(&config.root),
        Some(save_points::Marker::Closed { run: "r-1".into() }),
        "the event's run is compared, not the covered run"
    );
    let _ = std::fs::remove_dir_all(&base);
}

/// **A sink outside the territory refuses naming `trace-sink`**, per Spec
/// section 9 (Codex on #94, round 10): the sink's directory must be the
/// judged territory itself, a file, a pipe or a socket alike.
/// Perturbation: compare the sink's path prefix instead and a sink in a
/// subdirectory of the territory passes.
#[test]
fn a_sink_outside_the_territory_refuses() {
    let territory = std::path::Path::new("/var/lib/weaver-agent/weaver-alpha");
    let file = |path: &str| weaver_types::TraceSink::File {
        path: path.into(),
        create: false,
    };
    assert!(
        sink_within_territory(
            &file("/var/lib/weaver-agent/weaver-alpha/trace.ndjson"),
            territory
        )
        .is_ok()
    );
    for elsewhere in [
        "/var/lib/weaver-agent/weaver-beta/trace.ndjson",
        "/var/lib/weaver-agent/weaver-alpha/state/trace.ndjson",
        "/srv/trace.ndjson",
    ] {
        assert_eq!(
            sink_within_territory(&file(elsewhere), territory).err(),
            Some(LifecycleRefusal::ConfigInvalid {
                field: Some(FieldName("trace-sink".into()))
            }),
            "{elsewhere}"
        );
    }
}

/// **The marker is restored by the rollback**, per Spec section 4 on
/// A3.0 item 5: a load that wrote the marker open and then failed puts
/// back what stood before, a closed marker or none. Perturbation: skip
/// the marker in `roll_back` and the open marker survives the failure.
#[test]
fn the_rollback_restores_the_marker_it_found() {
    let (config, _scratch) = scratch_config("marker-rollback");
    let closed = save_points::Marker::Closed { run: "r-0".into() };
    save_points::write_marker(&config.root, Some(&closed)).unwrap();
    let mut standing = Standing {
        marker_before: Some(Some(closed.clone())),
        ..Standing::default()
    };
    save_points::write_marker(
        &config.root,
        Some(&save_points::Marker::Open { run: "r-1".into() }),
    )
    .unwrap();
    let account = roll_back(&config, &mut standing);
    assert!(account.contains("marker restored"), "{account}");
    assert_eq!(save_points::read_marker(&config.root), Some(closed.clone()));
    let mut standing = Standing {
        marker_before: Some(None),
        ..Standing::default()
    };
    save_points::write_marker(
        &config.root,
        Some(&save_points::Marker::Open { run: "r-1".into() }),
    )
    .unwrap();
    roll_back(&config, &mut standing);
    assert_eq!(save_points::read_marker(&config.root), None);
    // **What stood before is recorded ahead of the write** (Codex on
    // #94, round 4): the root made unwritable, the open marker does not
    // write, the load refuses, and the standing still carries the closed
    // marker it found, so the rollback puts it back once the root
    // writes. Perturbation: record `marker_before` after the write and
    // the refused write records nothing.
    save_points::write_marker(&config.root, Some(&closed)).unwrap();
    let mut standing = Standing::default();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&config.root, std::fs::Permissions::from_mode(0o500)).unwrap();
    }
    let refused = open_marker(&config.root, &mut standing, "r-1");
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&config.root, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    assert!(matches!(refused, Err(LifecycleRefusal::BoundaryUnverified)));
    assert_eq!(standing.marker_before, Some(Some(closed.clone())));
    save_points::write_marker(
        &config.root,
        Some(&save_points::Marker::Open { run: "r-1".into() }),
    )
    .unwrap();
    roll_back(&config, &mut standing);
    assert_eq!(save_points::read_marker(&config.root), Some(closed.clone()));
    // **The run being undone leaves forced, and one that does not leave
    // keeps the marker open on it** (the #94 survey's S10): the worker
    // refuses the leave, so the trace holds this run's `load` and no
    // `unload`, and the marker names the run for the next load's reset.
    // Perturbations: direct an unforced leave and the directive says so;
    // restore the prior marker whatever the leave did and it reads
    // closed.
    let mut standing = Standing {
        entered: true,
        marker_before: Some(Some(closed)),
        run_reference: Some("r-1".into()),
        ..Standing::default()
    };
    save_points::write_marker(
        &config.root,
        Some(&save_points::Marker::Open { run: "r-1".into() }),
    )
    .unwrap();
    let worker = recording_worker(
        &config,
        vec![weaver_types::Payload::Refusal(
            LifecycleRefusal::ActivityNotAtRest,
        )],
    );
    roll_back(&config, &mut standing);
    let directed = worker.join().unwrap();
    assert!(matches!(
        directed[0],
        weaver_types::Payload::Directive(LifecycleDirective::Leave { forced: true, .. })
    ));
    assert_eq!(
        save_points::read_marker(&config.root),
        Some(save_points::Marker::Open { run: "r-1".into() })
    );
    let _ = std::fs::remove_file(config.coordination_socket());
    // **A run whose `load` is on the trace, its `Ready` never read,
    // leaves the marker open on it** (the operator's ruling of 2026-10-08
    // on #99, K1): an enter refused after `load`, or a `Ready` past the
    // load bound, leaves `marker_before` unset; the rollback's forced
    // leave answers `Left`, so the run was entered, and the marker names
    // it, never the closed marker found. Perturbation: write the marker
    // only where `marker_before` was set and it stays closed.
    let closed = save_points::Marker::Closed { run: "r-0".into() };
    save_points::write_marker(&config.root, Some(&closed)).unwrap();
    let mut standing = Standing {
        entered: true,
        run_reference: Some("r-2".into()),
        ..Standing::default()
    };
    let worker = recording_worker(
        &config,
        vec![weaver_types::Payload::Answer(LifecycleAnswer::Left {
            save_point: None,
            forced: true,
            no_state: false,
        })],
    );
    let account = roll_back(&config, &mut standing);
    let _ = worker.join();
    assert!(account.contains("left open on the run"), "{account}");
    assert_eq!(
        save_points::read_marker(&config.root),
        Some(save_points::Marker::Open { run: "r-2".into() })
    );
    // A harness that never entered answers the forced leave
    // `OutOfOrder`: no `load` is on the trace, so the marker found is
    // put back. Perturbation: name the run whatever the leave answered
    // and this reads open.
    let _ = std::fs::remove_file(config.coordination_socket());
    let mut standing = Standing {
        entered: true,
        run_reference: Some("r-3".into()),
        marker_before: Some(Some(closed.clone())),
        ..Standing::default()
    };
    let worker = recording_worker(
        &config,
        vec![weaver_types::Payload::Refusal(LifecycleRefusal::OutOfOrder)],
    );
    roll_back(&config, &mut standing);
    let _ = worker.join();
    assert_eq!(save_points::read_marker(&config.root), Some(closed));
}

const TEST_UNLOAD_BOUNDS: UnloadBounds = UnloadBounds {
    leave: std::time::Duration::from_secs(2),
    after_left: std::time::Duration::from_millis(300),
    term: std::time::Duration::from_secs(2),
    kill: std::time::Duration::from_secs(2),
};

/// **`unload`'s leave, the wait after `left`, then the escalation**, per
/// Spec section 3: a worker observed `Idle` is directed to leave and
/// answers `Left`, but a constituent keeps the run lock past the wait, so
/// the escalation ends it and the verb answers once the lock is free. The
/// same path runs where the observation is refused, a refusal being
/// silence to `unload`. Perturbation: answer on `Left` alone and the holder
/// survives; propagate the observation's refusal and the second run
/// refuses `Malformed` with the holder standing.
#[test]
fn unload_leaves_waits_and_escalates() {
    for (tag, observed) in [
        (
            "idle",
            weaver_types::Payload::Answer(LifecycleAnswer::State {
                state: weaver_types::AgentState::Idle,
                load: None,
                constituents: Vec::new(),
            }),
        ),
        (
            "refused",
            weaver_types::Payload::Refusal(LifecycleRefusal::Malformed),
        ),
    ] {
        let (config, _scratch) = scratch_config(&format!("unload-leave-{tag}"));
        let mut holder = stand_in_holder(&config);
        let worker = answering_worker(
            &config,
            vec![
                observed,
                weaver_types::Payload::Answer(LifecycleAnswer::Left {
                    save_point: None,
                    forced: false,
                    no_state: true,
                }),
            ],
        );
        assert_eq!(
            unload_within(&config, TEST_UNLOAD_BOUNDS, false),
            Ok(LifecycleAnswer::State {
                state: weaver_types::AgentState::Unloaded,
                load: None,
                constituents: Vec::new(),
            }),
            "{tag}"
        );
        assert!(holder.wait().is_ok(), "{tag}: the holder ended");
        assert!(
            !start::run_lock_held(&config.run_directory()).unwrap(),
            "{tag}"
        );
        let _ = worker.join();
    }
}

/// **A load meeting a run whose worker refuses the observation answers
/// `AgentRunning`**, per Spec section 3: a refusal is an answer, so a run
/// stands, and the load touches nothing. Perturbation: propagate the
/// observation's refusal and the load answers `Malformed`.
#[test]
fn a_refused_observation_is_a_running_agent_to_a_load() {
    let (config, _scratch) = scratch_config("load-refused-observe");
    let mut holder = stand_in_holder(&config);
    let worker = answering_worker(
        &config,
        vec![weaver_types::Payload::Refusal(LifecycleRefusal::Malformed)],
    );
    let mut standing = Standing::default();
    assert_eq!(
        run_load(&config, &AgentName("alpha".into()), &mut standing),
        Err(LifecycleRefusal::AgentRunning)
    );
    assert!(holder.try_wait().unwrap().is_none(), "the run still stands");
    let _ = holder.kill();
    let _ = holder.wait();
    let _ = worker.join();
}

/// **A refused leave returns to the operator unchanged**, per Spec
/// section 3: `ActivityNotAtRest` answers and nothing is ended.
/// Perturbation: escalate on any leave failure and the busy run dies.
#[test]
fn a_refused_leave_ends_nothing() {
    let (config, _scratch) = scratch_config("unload-busy");
    let mut holder = stand_in_holder(&config);
    let worker = answering_worker(
        &config,
        vec![
            weaver_types::Payload::Answer(LifecycleAnswer::State {
                state: weaver_types::AgentState::Active,
                load: None,
                constituents: Vec::new(),
            }),
            weaver_types::Payload::Refusal(LifecycleRefusal::ActivityNotAtRest),
        ],
    );
    assert_eq!(
        unload_within(&config, TEST_UNLOAD_BOUNDS, false),
        Err(LifecycleRefusal::ActivityNotAtRest)
    );
    assert!(holder.try_wait().unwrap().is_none(), "the busy run stands");
    let _ = holder.kill();
    let _ = holder.wait();
    let _ = worker.join();
}

/// **A failed dial is answered from the worker's own exit**, per Spec
/// section 6: a worker that already exited refuses `NoResidency`, its
/// status named on standard error, and one still running with no socket
/// is a bind that failed. Perturbation: drop the status read and the exited
/// worker reads as `BindFailed`.
#[test]
fn a_failed_dial_reads_the_worker_exit() {
    let mut exited = std::process::Command::new("/bin/sh")
        .args(["-c", "exit 3"])
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while std::fs::read_to_string(format!("/proc/{}/stat", exited.id()))
        .map(|stat| {
            !stat
                .rsplit(')')
                .next()
                .unwrap_or("")
                .trim_start()
                .starts_with('Z')
        })
        .unwrap_or(false)
        && std::time::Instant::now() < deadline
    {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(
        refusal_from_worker(&mut exited),
        LifecycleRefusal::NoResidency
    );
    let mut running = std::process::Command::new("/bin/sleep")
        .arg("30")
        .spawn()
        .unwrap();
    assert_eq!(
        refusal_from_worker(&mut running),
        LifecycleRefusal::BindFailed
    );
    let _ = running.kill();
    let _ = running.wait();
}

/// **The reserved suffixes refuse at the name check**, per Spec section
/// 4: an agent named `x-relay` would share agent `x`'s relay account.
/// Perturbation: drop the suffix check and each name passes.
#[test]
fn a_reserved_suffix_refuses() {
    for reserved in ["x-state", "x-trace", "x-relay", "x-admin", "x-admincon"] {
        assert!(!well_formed(reserved), "{reserved}");
        assert_eq!(
            load_service_config(&AgentName(reserved.into()), false).err(),
            Some(LifecycleRefusal::NoSuchAgent),
            "{reserved}"
        );
    }
    assert!(well_formed("x-relays"), "a suffix is matched whole");
}

/// **A load never ends an existing run**, per Spec section 3: a held run
/// lock with no worker listening refuses `AgentRunning`, and the holder
/// still runs. Perturbation: let the load end what holds the lock and the
/// holder dies under it.
#[test]
fn a_load_never_ends_an_existing_run() {
    let (config, _scratch) = scratch_config("load-held");
    let mut holder = stand_in_holder(&config);
    let mut standing = Standing::default();
    assert_eq!(
        run_load(&config, &AgentName("alpha".into()), &mut standing),
        Err(LifecycleRefusal::AgentRunning)
    );
    assert!(holder.try_wait().unwrap().is_none(), "the run still stands");
    let _ = holder.kill();
    let _ = holder.wait();
}

/// **A silent worker is never reaped, and the observation is bounded**,
/// per Spec section 3: a held lock whose worker accepts and answers
/// nothing refuses the load `Unanswered` and `show` `Unanswered`, each
/// within the observation's bound, and the holder runs on. Perturbation:
/// drop the bound from the observation's read and the verb never returns;
/// read silence as no listener and the load answers `AgentRunning`.
#[test]
fn a_silent_run_is_bounded_and_never_reaped() {
    let (config, _scratch) = scratch_config("silent");
    let mut holder = stand_in_holder(&config);
    let _listener = silent_worker(&config);
    let started = std::time::Instant::now();
    let mut standing = Standing::default();
    assert_eq!(
        run_load(&config, &AgentName("alpha".into()), &mut standing),
        Err(LifecycleRefusal::Unanswered)
    );
    assert_eq!(show(&config), Err(LifecycleRefusal::Unanswered));
    assert!(
        started.elapsed() < OBSERVE_BOUND * 2 + std::time::Duration::from_secs(4),
        "both answered within their bounds"
    );
    assert!(
        holder.try_wait().unwrap().is_none(),
        "the silent run still stands"
    );
    let _ = holder.kill();
    let _ = holder.wait();
}

/// **`unload` ends a run that never entered**, per Spec section 3: a held
/// lock with no worker listening goes straight to the escalation, and the
/// verb answers once the lock is free. Perturbation: direct leave at the
/// unentered run and refuse on its answer, and the holder survives.
#[test]
fn unload_ends_a_run_that_never_entered() {
    let (config, _scratch) = scratch_config("unload-unentered");
    let mut holder = stand_in_holder(&config);
    assert_eq!(
        unload(&config),
        Ok(LifecycleAnswer::State {
            state: weaver_types::AgentState::Unloaded,
            load: None,
            constituents: Vec::new(),
        })
    );
    assert!(holder.wait().is_ok());
    assert!(!start::run_lock_held(&config.run_directory()).unwrap());
}

/// **`show` answers a transition in flight**, per Spec section 3: another
/// process holding the invocation lock exclusively makes `show` answer
/// `InTransition` at once, without dialing. Perturbation: dial instead and
/// the answer is the observation's.
#[test]
fn show_answers_a_transition_in_flight() {
    let (config, _scratch) = scratch_config("in-transition");
    let path = config.run_directory().join("admin.lock");
    let (ready_read, ready_write) = nix::unistd::pipe().unwrap();
    // Built before the fork, so the child only calls what a fork of a
    // threaded process may.
    let c_path = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
    // SAFETY: the child calls only async-signal-safe functions.
    match unsafe { nix::unistd::fork() }.unwrap() {
        nix::unistd::ForkResult::Child => {
            start::close_inherited_except(Some(std::os::fd::AsRawFd::as_raw_fd(&ready_write)));
            // SAFETY: open, fcntl, write and pause are async-signal-safe.
            unsafe {
                let fd = nix::libc::open(
                    c_path.as_ptr(),
                    nix::libc::O_RDWR | nix::libc::O_CREAT,
                    0o600 as nix::libc::c_uint,
                );
                let lock = nix::libc::flock {
                    l_type: nix::libc::F_WRLCK as i16,
                    l_whence: nix::libc::SEEK_SET as i16,
                    l_start: 0,
                    l_len: 0,
                    l_pid: 0,
                };
                nix::libc::fcntl(fd, nix::libc::F_SETLK, &lock);
                nix::libc::write(
                    std::os::fd::AsRawFd::as_raw_fd(&ready_write),
                    [1u8].as_ptr().cast(),
                    1,
                );
                nix::libc::pause();
                nix::libc::_exit(0);
            }
        }
        nix::unistd::ForkResult::Parent { child } => {
            drop(ready_write);
            let mut byte = [0u8; 1];
            nix::unistd::read(&ready_read, &mut byte).unwrap();
            assert_eq!(
                show(&config),
                Ok(LifecycleAnswer::InTransition {
                    constituents: Vec::new(),
                })
            );
            // SAFETY: kill on the child this test forked.
            unsafe { nix::libc::kill(child.as_raw(), nix::libc::SIGKILL) };
            let _ = nix::sys::wait::waitpid(child, None);
        }
    }
}

/// **The member's territory is the member's own, and a load closes a
/// room that was left open rather than opening a closed one.**
///
/// Both halves matter and the second is the one issue #545 found: the
/// preparation ran on every load, so whatever the operator's
/// provisioning made the room, one load rewrote it to this crate's uid,
/// the parent's group, and `0750`. A member-owned `0700` room did not
/// survive a single load, which is why the charter's uid could not be
/// held by provisioning alone.
///
/// The account stood up is this process's own, which is what lets the
/// chown run without privilege: a test that named another account would
/// assert nothing off a root box.
///
/// Perturbation: restore `mode(0o750)`, the `set_permissions(0o750)`,
/// and the chown of the parent's group, and the fresh room reads `0750`
/// where `0700` is asserted. Watched failing 2026-09-15, at the first
/// assertion, which is where the run stops.
///
/// conforms: admin-member-territory-is-the-members-own
#[test]
fn the_territory_is_owned_by_the_member_and_closed_on_every_load() {
    use std::os::fd::AsFd;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let root = crate::scratch::Scratch(
        std::env::temp_dir().join(format!("wt-territory-{}", std::process::id())),
    );
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("the operator-side directory");
    let member = inventory::MemberAccount {
        uid: nix::unistd::getuid().as_raw(),
        gid: nix::unistd::getgid().as_raw(),
    };

    let root_fd = save_points::open_directory(&root).unwrap();
    let territory =
        prepare_territory(root_fd.as_fd(), &root, member).expect("the territory is made");
    let made = std::fs::metadata(&territory).expect("it stands");
    assert_eq!(
        made.mode() & 0o777,
        0o700,
        "the room the member is handed grants nobody else anything"
    );
    assert_eq!(made.uid(), member.uid, "and the member owns it");

    // A room widened between loads, which is the case the repair exists
    // for: the operator, another tool, or an earlier build of this crate.
    std::fs::set_permissions(&territory, std::fs::Permissions::from_mode(0o755)).expect("widen it");
    let again = prepare_territory(root_fd.as_fd(), &root, member).expect("the second load");
    assert_eq!(again, territory, "the same room, not a second one");
    assert_eq!(
        std::fs::metadata(&again).expect("it stands").mode() & 0o777,
        0o700,
        "a load closes a room that was left open"
    );
    // **A link at the room's name is refused and its target untouched**
    // (the custody audit's G13). Perturbation: make the room by path
    // again and the link's target is chowned and narrowed to 0700.
    std::fs::remove_dir(&territory).unwrap();
    let elsewhere = root.0.join("elsewhere");
    std::fs::create_dir(&elsewhere).unwrap();
    std::fs::set_permissions(&elsewhere, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::os::unix::fs::symlink(&elsewhere, &territory).unwrap();
    assert!(
        prepare_territory(root_fd.as_fd(), &root, member).is_none(),
        "a link at the room's name refuses the leg"
    );
    assert_eq!(
        std::fs::metadata(&elsewhere).unwrap().mode() & 0o777,
        0o755,
        "the link's target keeps its mode"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// **The member spawn drops to the member's account, driven through
/// `stand_state_member` itself** (#673 item 6). Needs euid 0, and runs
/// through `the_member_spawn_is_watched_inside_a_user_namespace` on a box
/// where that is not the invoking uid. A stand-in `weaver-state` beside
/// the worker records the identity it runs under into the territory the
/// real path prepared, and the reading is the kernel's own status after
/// exec: every uid the member's, every gid its group's, and the
/// supplementary set that group and the territory's access group alone,
/// none of root's, the access group riding the drop from the territory as
/// judged (Codex on #94, round 8). The member's end is read too, a socket
/// at the fixed number.
///
/// `drop_to`'s own instrument, in the inventory module, watches the order
/// of the three calls and the saved ids. This
/// one watches that the member's spawn takes the drop at all, which no
/// reading of `drop_to` can see.
///
/// Perturbations, each watched failing 2026-09-26 through the namespace
/// watch: remove `become_member` from the spawn's pre-exec and the member
/// runs as uid 0; hand `drop_to` root's group beside the member's and the
/// supplementary set carries 0.
///
/// conforms: admin-member-spawn-drops-to-its-account
#[test]
#[ignore = "needs euid 0: run by the_member_spawn_is_watched_inside_a_user_namespace"]
fn the_member_spawn_lands_the_members_identity_as_root() {
    use std::os::unix::fs::PermissionsExt;
    assert!(
        nix::unistd::geteuid().is_root(),
        "this instrument needs euid 0"
    );
    /// Removes the tree however the reading ends. It runs as root in
    /// the namespace, the only party that can enter the member's room.
    struct Yard(std::path::PathBuf);
    impl Drop for Yard {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let yard = Yard(std::env::temp_dir().join(format!("wt-member-spawn-{}", std::process::id())));
    let _ = std::fs::remove_dir_all(&yard.0);
    let bin = yard.0.join("bin");
    let sink = yard.0.join("sink");
    for dir in [&bin, &sink] {
        std::fs::create_dir_all(dir).expect("the yard's directories");
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o755))
            .expect("the member can traverse them");
    }
    std::fs::set_permissions(&yard.0, std::fs::Permissions::from_mode(0o755))
        .expect("and the yard");
    // The stand-in writes its reading whole and then renames it, so a
    // partial file is never what the poll below reads.
    let stand_in = bin.join("weaver-state");
    std::fs::write(
        &stand_in,
        concat!(
            "#!/bin/sh
",
            "for a; do t=$a; done
",
            "/usr/bin/readlink /proc/self/fd/3 > \"$t/fd3\"\n",
            "/usr/bin/readlink /proc/self/fd/9 > \"$t/fd9\"\n",
            "/bin/cat /proc/self/status > \"$t/status.part\" && ",
            "/bin/mv \"$t/status.part\" \"$t/status\"\n",
        ),
    )
    .expect("the stand-in is written");
    std::fs::set_permissions(&stand_in, std::fs::Permissions::from_mode(0o755))
        .expect("and made executable");

    let source = format!(
        concat!(
            "session = \"s-1\"\n",
            "tool-set = []\n",
            "permission-mode = \"ask\"\n",
            "\n",
            "[spu-instruction.decoder]\n",
            "residual-readout-election = false\n",
            "tunable-values = {{}}\n",
            "\n",
            "[spu-instruction.decoder.model-binding]\n",
            "artifact = \"qwen3-4b-instruct\"\n",
            "devices = [0]\n",
            "\n",
            "[gate-instruction.access-rule]\n",
            "allowed-uids = [0]\n",
            "allowed-gids = []\n",
            "denied-uids = [1701]\n",
            "\n",
            "[trace-sink]\n",
            "kind = \"file\"\n",
            "path = \"{}/trace.ndjson\"\n",
            "create = true\n",
            "\n",
            "[state-store]\n",
            "engine = \"sqlite\"\n",
        ),
        sink.display()
    );
    let config = weaver_types::parse(&source).expect("the declaration parses");
    let gate_instruction = config
        .gate_instruction
        .clone()
        .expect("a serving declaration carries its instruction");
    let member = inventory::MemberAccount {
        uid: 4242,
        gid: 4243,
    };
    let inventory = inventory::Inventory {
        config,
        identity: "weaver-alpha".into(),
        declaration: String::new(),
        binding: weaver_types::EnterBinding::Serving { gate_instruction },
        lineage: None,
        member_account: Some(member),
    };
    let mut service = unread_config();
    // The territory's access group as judged, which no longer rides the
    // drop (the custody audit's G11), and the territory's descriptor, as
    // the judgment opens it, through which the room is made.
    service.access_gid = 4244;
    service.territory_fd = Some(save_points::open_directory(&sink).unwrap());
    service.worker = bin.join("weaver-worker");
    let run_directory = sink.join("run");
    std::fs::create_dir_all(&run_directory).unwrap();
    let run_lock = start::take_run_lock(&run_directory)
        .unwrap()
        .expect("a free run lock");

    let harness_end = stand_state_member(&service, &inventory, &run_lock, None);
    assert!(harness_end.is_some(), "the member stands");
    let territory = sink.join("state");
    let status = territory.join("status");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !status.exists() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let read = std::fs::read_to_string(&status).expect("the member recorded itself");
    let line = |key: &str| {
        read.lines()
            .find_map(|l| l.strip_prefix(key))
            .map(|rest| rest.split_whitespace().collect::<Vec<_>>().join(" "))
            .unwrap_or_default()
    };
    assert_eq!(
        line("Uid:"),
        "4242 4242 4242 4242",
        "every uid the member's"
    );
    assert_eq!(line("Gid:"), "4243 4243 4243 4243", "every gid its group's");
    assert_eq!(
        line("Groups:"),
        "4243",
        "its own group alone: not the access group, none of root's"
    );
    let fd3 = std::fs::read_to_string(territory.join("fd3")).expect("the end was read");
    assert!(
        fd3.starts_with("socket:"),
        "the member's end is a socket at the fixed number: {fd3}"
    );
    // **The member holds the run lock's description at 9**, per Spec
    // section 3. Perturbation: drop the placement from the member's
    // spawn and nothing stands at 9.
    let fd9 = std::fs::read_to_string(territory.join("fd9")).expect("the lock was read");
    assert_eq!(
        fd9.trim(),
        run_directory.join("run.lock").display().to_string(),
        "the member holds the run lock at its fixed number"
    );
}

/// **The worker's spawn, as root inside a user namespace**, per Spec
/// sections 6 and 10's third walk: the worker runs as the agent and holds
/// no group root left it, leads its own session with no signal ignored and
/// no new privileges, carries the fixed environment and nothing else, and
/// crosses the exec with exactly its allowlist of descriptors: standard
/// input at `/dev/null`, output and error at the worker log, the run lock
/// at 9 and the relay's write end at 8. A stand-in worker, `sh -c`,
/// records itself and sleeps. Run by the watch below.
///
/// Perturbations: skip the supplementary narrowing and `Groups:` holds
/// root's; drop the run lock's placement and 9 is absent from the list;
/// let the caller's environment through and the environment carries more.
#[test]
#[ignore = "needs root; run inside a user namespace by the watch below"]
fn the_worker_spawn_lands_its_identity_and_allowlist_as_root() {
    use std::os::unix::fs::PermissionsExt;
    let base = std::env::temp_dir().join(format!("weaver-admin-worker-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    std::fs::set_permissions(&base, std::fs::Permissions::from_mode(0o755)).unwrap();
    let run_directory = base.join("run");
    std::fs::create_dir_all(&run_directory).unwrap();
    let run_lock = start::take_run_lock(&run_directory).unwrap().unwrap();
    let (relay_read, relay_write) = nix::unistd::pipe2(nix::fcntl::OFlag::O_CLOEXEC).unwrap();
    let log = start::open_log(&base.join("worker.log"), None).unwrap();
    // **A descriptor the caller left inheritable**, as a root shell may:
    // without the seal it would cross the worker's exec.
    // SAFETY: F_DUPFD without the flag on a descriptor this test owns.
    let leaked = unsafe {
        nix::libc::fcntl(
            std::os::fd::AsRawFd::as_raw_fd(&relay_read),
            nix::libc::F_DUPFD,
            50,
        )
    };
    assert!(leaked >= 50);
    let mut child = start::spawn_worker(start::WorkerStart {
        binary: std::path::Path::new("/bin/sleep"),
        arguments: vec!["30".to_string()],
        uid: 4242,
        gid: 4243,
        home: std::path::Path::new("/nonexistent/home/alpha"),
        library_path: Some(std::path::Path::new("/opt/weaver/lib")),
        log: &log,
        run_lock: &run_lock,
        relay_write: Some(std::os::fd::AsRawFd::as_raw_fd(&relay_write)),
    })
    .expect("the worker spawns");
    // The stand-in is `sleep` itself, which changes nothing it inherited,
    // read from outside once its exec has landed.
    let pid = child.id();
    let proc = std::path::PathBuf::from(format!("/proc/{pid}"));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while std::fs::read_to_string(proc.join("comm"))
        .map(|c| c.trim() != "sleep")
        .unwrap_or(true)
        && std::time::Instant::now() < deadline
    {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let status = std::fs::read_to_string(proc.join("status")).unwrap();
    let line = |key: &str| {
        status
            .lines()
            .find_map(|l| l.strip_prefix(key))
            .map(|rest| rest.split_whitespace().collect::<Vec<_>>().join(" "))
            .unwrap_or_default()
    };
    assert_eq!(line("Uid:"), "4242 4242 4242 4242", "every uid the agent's");
    assert_eq!(line("Gid:"), "4243 4243 4243 4243", "every gid its group's");
    assert_eq!(line("Groups:"), "4243", "its group alone, none of root's");
    assert_eq!(line("SigIgn:"), "0000000000000000", "no signal ignored");
    assert_eq!(line("NoNewPrivs:"), "1", "no new privileges");
    assert_eq!(line("Umask:"), "0027", "a fixed file-creation mask");
    assert_eq!(
        std::fs::read_link(proc.join("cwd")).unwrap(),
        std::path::Path::new("/"),
        "a fixed working directory"
    );
    let stat = std::fs::read_to_string(proc.join("stat")).unwrap();
    let fields: Vec<&str> = stat
        .rsplit(')')
        .next()
        .unwrap()
        .split_whitespace()
        .collect();
    assert_eq!(fields[3], pid.to_string(), "it leads its own session");
    // **Read once the stand-in is actually sleeping**: `sleep` opens its
    // locale files for an instant after the exec, so the table is read
    // until it settles, within the bound, and the last reading is judged.
    let table = || {
        let mut fds: Vec<u32> = std::fs::read_dir(proc.join("fd"))
            .unwrap()
            .filter_map(|entry| entry.ok()?.file_name().to_str()?.parse().ok())
            .collect();
        fds.sort_unstable();
        fds
    };
    let settle = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let mut fds = table();
    while fds != [0, 1, 2, 8, 9] && std::time::Instant::now() < settle {
        std::thread::sleep(std::time::Duration::from_millis(20));
        fds = table();
    }
    assert_eq!(
        fds,
        vec![0, 1, 2, 8, 9],
        "exactly the allowlist crosses the exec"
    );
    assert_eq!(
        std::fs::read_link(proc.join("fd/0")).unwrap(),
        std::path::Path::new("/dev/null")
    );
    let lock_meta = std::fs::metadata(run_directory.join("run.lock")).unwrap();
    let held = std::fs::metadata(proc.join("fd/9")).unwrap();
    {
        use std::os::unix::fs::MetadataExt;
        assert_eq!((held.dev(), held.ino()), (lock_meta.dev(), lock_meta.ino()));
    }
    let environ = std::fs::read(proc.join("environ")).unwrap();
    let mut environment: Vec<String> = environ
        .split(|b| *b == 0)
        .filter(|v| !v.is_empty())
        .map(|v| String::from_utf8_lossy(v).into_owned())
        .collect();
    environment.sort();
    assert_eq!(
        environment,
        vec![
            "HOME=/nonexistent/home/alpha".to_string(),
            "LANG=C.UTF-8".to_string(),
            "LD_LIBRARY_PATH=/opt/weaver/lib".to_string(),
            "PATH=/usr/bin:/bin".to_string(),
        ],
        "the fixed environment and nothing else"
    );
    let _ = child.kill();
    let _ = child.wait();
    drop(relay_read);
    let _ = std::fs::remove_dir_all(&base);
}

/// **The relay's spawn, as root inside a user namespace**, per Spec
/// sections 6 and 10's third walk: the relay runs as its account and its
/// one group, the trace group, holds no group root left it, leads its own
/// session with no signal ignored and no new privileges, and crosses the
/// exec with exactly its allowlist: its standard streams at `/dev/null`,
/// the listener at 3, the sink at 4, the log at 5, the lifetime pipe at 6
/// and the run lock at 9. Its environment is the fixed set, so the relay's
/// test-only wait knob, set in this process, never reaches it. The
/// stand-in is a script that becomes `sleep`. Run by the watch below.
/// Perturbations: drop the log's placement and 5 is absent from the list,
/// and pass the knob through and the environment carries it.
#[test]
#[ignore = "needs root; run inside a user namespace by the watch below"]
fn the_relay_spawn_lands_its_identity_and_allowlist_as_root() {
    use std::os::unix::fs::PermissionsExt;
    let base = std::env::temp_dir().join(format!("weaver-admin-relay-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    std::fs::set_permissions(&base, std::fs::Permissions::from_mode(0o755)).unwrap();
    let stand_in = base.join("weaver-trace-relay");
    std::fs::write(&stand_in, "#!/bin/sh\nexec /bin/sleep 30\n").unwrap();
    std::fs::set_permissions(&stand_in, std::fs::Permissions::from_mode(0o755)).unwrap();
    let run_directory = base.join("run");
    std::fs::create_dir_all(&run_directory).unwrap();
    let run_lock = start::take_run_lock(&run_directory).unwrap().unwrap();
    let listener = start::bind_trace_door(&run_directory, 0).unwrap();
    {
        use std::os::unix::fs::MetadataExt;
        let door = std::fs::symlink_metadata(run_directory.join("trace.sock")).unwrap();
        assert_eq!(door.mode() & 0o777, 0o660, "the door is 0660");
        assert_eq!((door.uid(), door.gid()), (0, 0), "root's, grouped as asked");
    }
    let sink_path = base.join("trace.ndjson");
    std::fs::write(&sink_path, "").unwrap();
    let sink: std::os::fd::OwnedFd = std::fs::File::open(&sink_path).unwrap().into();
    let log = start::open_log(&base.join("admin.log"), None).unwrap();
    let (lifetime_read, lifetime_write) = nix::unistd::pipe2(nix::fcntl::OFlag::O_CLOEXEC).unwrap();
    // SAFETY: the watch runs this instrument alone, one thread.
    unsafe { std::env::set_var("WEAVER_TRACE_RELAY_TEST_MS", "1") };
    let mut child = start::spawn_relay(start::RelayStart {
        binary: &stand_in,
        reader_uid: 4246,
        agent: "alpha",
        boundary_digest: "b0b0",
        uid: 4244,
        gid: 4245,
        listener: &listener,
        sink: &sink,
        log: &log,
        lifetime_read: &lifetime_read,
        run_lock: &run_lock,
    })
    .expect("the relay spawns");
    let pid = child.id();
    let proc = std::path::PathBuf::from(format!("/proc/{pid}"));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while std::fs::read_to_string(proc.join("comm"))
        .map(|c| c.trim() != "sleep")
        .unwrap_or(true)
        && std::time::Instant::now() < deadline
    {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let status = std::fs::read_to_string(proc.join("status")).unwrap();
    let line = |key: &str| {
        status
            .lines()
            .find_map(|l| l.strip_prefix(key))
            .map(|rest| rest.split_whitespace().collect::<Vec<_>>().join(" "))
            .unwrap_or_default()
    };
    assert_eq!(
        line("Uid:"),
        "4244 4244 4244 4244",
        "every uid the relay account's"
    );
    assert_eq!(
        line("Gid:"),
        "4245 4245 4245 4245",
        "every gid the trace group's"
    );
    assert_eq!(line("Groups:"), "4245", "the trace group alone");
    assert_eq!(line("SigIgn:"), "0000000000000000");
    assert_eq!(line("NoNewPrivs:"), "1");
    let environ = std::fs::read(proc.join("environ")).unwrap();
    let names: Vec<String> = environ
        .split(|b| *b == 0)
        .filter(|entry| !entry.is_empty())
        .map(|entry| {
            let name = entry.split(|b| *b == b'=').next().unwrap_or_default();
            String::from_utf8_lossy(name).into_owned()
        })
        .collect();
    assert!(
        !names
            .iter()
            .any(|name| name == "WEAVER_TRACE_RELAY_TEST_MS"),
        "the test-only knob never reaches a started relay: {names:?}"
    );
    assert!(names.iter().any(|name| name == "PATH"), "{names:?}");
    let table = || {
        let mut fds: Vec<u32> = std::fs::read_dir(proc.join("fd"))
            .unwrap()
            .filter_map(|entry| entry.ok()?.file_name().to_str()?.parse().ok())
            .collect();
        fds.sort_unstable();
        fds
    };
    let settle = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let mut fds = table();
    while fds != [0, 1, 2, 3, 4, 5, 6, 9] && std::time::Instant::now() < settle {
        std::thread::sleep(std::time::Duration::from_millis(20));
        fds = table();
    }
    assert_eq!(
        fds,
        vec![0, 1, 2, 3, 4, 5, 6, 9],
        "exactly the relay's allowlist"
    );
    let _ = child.kill();
    let _ = child.wait();
    drop(lifetime_write);
    let _ = std::fs::remove_dir_all(&base);
}

/// The watch for the relay spawn, the worker's pattern.
#[test]
fn the_relay_spawn_is_watched_inside_a_user_namespace() {
    if nix::unistd::geteuid().is_root() {
        return the_relay_spawn_lands_its_identity_and_allowlist_as_root();
    }
    let exe = std::env::current_exe().expect("the test binary names itself");
    let ran = std::process::Command::new("unshare")
        .args(["--map-auto", "--map-root-user"])
        .arg(&exe)
        .args([
            "--exact",
            "tests::the_relay_spawn_lands_its_identity_and_allowlist_as_root",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .stdin(std::process::Stdio::null())
        .output();
    let output = match ran {
        Ok(output) => output,
        Err(e) => {
            eprintln!("SKIP relay spawn watch: unshare could not run: {e}");
            return;
        }
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.starts_with("unshare:") {
        eprintln!(
            "SKIP relay spawn watch: no user namespace here: {}",
            stderr.trim()
        );
        return;
    }
    assert!(
        output.status.success() && stdout.contains("test result: ok. 1 passed"),
        "the relay spawn failed inside the namespace\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

/// The watch for the worker spawn, the member's pattern: re-executes this
/// test binary inside `unshare --map-auto --map-root-user` and requires
/// the instrument above to report exactly one test passed. A box where the
/// namespace cannot be entered prints a SKIP naming why and passes.
#[test]
fn the_worker_spawn_is_watched_inside_a_user_namespace() {
    if nix::unistd::geteuid().is_root() {
        return the_worker_spawn_lands_its_identity_and_allowlist_as_root();
    }
    let exe = std::env::current_exe().expect("the test binary names itself");
    let ran = std::process::Command::new("unshare")
        .args(["--map-auto", "--map-root-user"])
        .arg(&exe)
        .args([
            "--exact",
            "tests::the_worker_spawn_lands_its_identity_and_allowlist_as_root",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .stdin(std::process::Stdio::null())
        .output();
    let output = match ran {
        Ok(output) => output,
        Err(e) => {
            diag!("SKIP worker spawn watch: unshare could not run: {e}");
            return;
        }
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.starts_with("unshare:") {
        diag!(
            "SKIP worker spawn watch: no user namespace here: {}",
            stderr.trim()
        );
        return;
    }
    assert!(
        output.status.success() && stdout.contains("test result: ok. 1 passed"),
        "the worker spawn failed inside the namespace\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

/// **The watch that runs on an ordinary `cargo test`**, the store
/// probe's pattern: re-executes this test binary inside `unshare
/// --map-auto --map-root-user`, where the process is uid 0 over the
/// invoking user's subordinate ids, and requires the instrument above to
/// report exactly one test passed, so a filter matching nothing cannot
/// read as green. A box where the namespace cannot be entered prints a
/// SKIP naming why and passes, and has no watch.
///
/// Perturbation: as the instrument's, watched through this test.
#[test]
fn the_member_spawn_is_watched_inside_a_user_namespace() {
    if nix::unistd::geteuid().is_root() {
        return the_member_spawn_lands_the_members_identity_as_root();
    }
    let exe = std::env::current_exe().expect("the test binary names itself");
    let ran = std::process::Command::new("unshare")
        .args(["--map-auto", "--map-root-user"])
        .arg(&exe)
        .args([
            "--exact",
            "tests::the_member_spawn_lands_the_members_identity_as_root",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .stdin(std::process::Stdio::null())
        .output();
    let output = match ran {
        Ok(output) => output,
        Err(e) => {
            diag!("SKIP member spawn watch: unshare could not run: {e}");
            return;
        }
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.starts_with("unshare:") {
        diag!(
            "SKIP member spawn watch: no user namespace here: {}",
            stderr.trim()
        );
        return;
    }
    assert!(
        output.status.success() && stdout.contains("test result: ok. 1 passed"),
        "the member spawn failed inside the namespace\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}
