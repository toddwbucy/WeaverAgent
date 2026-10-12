//! The second walk, run from inside the crate because this crate publishes
//! no library surface.

use super::*;

/// **A rule admitting a peer the socket's mode turns away is named.**
///
/// The gate binds `0770` owned by the agent's group, so a uid outside it
/// is refused at `connect(2)` - before `accept`, so the credential check
/// never runs and the gate records nothing. Two locks on one door may
/// narrow the same set and may not contradict.
///
/// **Judged against a constructed table, so this runs everywhere.**
/// Reading the host's made it vacuous on a development box, where the
/// operator is in every agent group, and on CI, where no agent group
/// exists - a watch that ran in neither place while its record claimed
/// otherwise.
///
/// Perturbation: return `None` unconditionally from
/// `unreachable_peer_against` and this fails. The call site is watched
/// separately below.
///
/// conforms: admin-access-rule-reaches-the-socket
#[test]
fn a_rule_the_mode_would_defeat_is_named_rather_than_left_to_connect() {
    let outsider = 4242;
    let rule = weaver_types::AccessRule {
        allowed_uids: [outsider].into_iter().collect(),
        allowed_gids: Default::default(),
        denied_uids: Default::default(),
    };
    let excludes = ResolvedGroup::Present {
        gid: 6000,
        members: vec!["someone-else".to_string()],
    };
    // A uid the host does resolve, so this case is about the group and
    // not about the passwd entry: this process's own, which is in
    // neither the gid nor the member list above.
    let me = nix::unistd::Uid::current().as_raw();
    let mine = weaver_types::AccessRule {
        allowed_uids: [me].into_iter().collect(),
        allowed_gids: Default::default(),
        denied_uids: Default::default(),
    };
    if me != 0 {
        assert_eq!(
            unreachable_peer_against("weaver-x", &mine, &excludes),
            Some(Unreachable::UidOutsideGroup(me)),
            "a uid outside the group is named by its field"
        );
    }

    // Root is skipped: `CAP_DAC_OVERRIDE` reaches a `0770` socket
    // whatever group it holds.
    let root = weaver_types::AccessRule {
        allowed_uids: [0].into_iter().collect(),
        allowed_gids: Default::default(),
        denied_uids: Default::default(),
    };
    assert_eq!(unreachable_peer_against("weaver-x", &root, &excludes), None);

    // A gid is not judged at all, whether it is the group's or not.
    let by_gid = weaver_types::AccessRule {
        allowed_uids: Default::default(),
        allowed_gids: [1000, 6000].into_iter().collect(),
        denied_uids: Default::default(),
    };
    assert_eq!(
        unreachable_peer_against("weaver-x", &by_gid, &excludes),
        None
    );

    // **A uid the host cannot resolve is named rather than skipped**, and
    // this assertion said the opposite until 2026-08-29. Skipping it was
    // an undeclared third exemption beside `allowed-gids` and uid 0: a
    // uid with no passwd entry is in no group, membership being recorded
    // by name, so it reaches a `0770` socket it does not own by no route.
    assert_eq!(
        unreachable_peer_against("weaver-x", &rule, &excludes),
        Some(Unreachable::UidOutsideGroup(4242)),
        "a uid with no passwd entry is unreachable, not exempt"
    );

    // Membership by the group's own gid, and by the member list.
    let includes = ResolvedGroup::Present {
        gid: 6000,
        members: vec![
            nix::unistd::User::from_uid(nix::unistd::Uid::from_raw(me))
                .expect("this uid resolves")
                .expect("this uid has a passwd entry")
                .name,
        ],
    };
    assert_eq!(unreachable_peer_against("weaver-x", &mine, &includes), None);
}

/// **A half-provisioned box is named**, the user existing without its
/// group being the state that fails the unit start with an opaque
/// credential error. A box carrying neither is not refused on a fact
/// about an agent it does not have.
#[test]
fn a_user_without_its_group_is_named_and_a_bare_box_is_not() {
    let rule = weaver_types::AccessRule {
        allowed_uids: Default::default(),
        allowed_gids: Default::default(),
        denied_uids: Default::default(),
    };
    assert_eq!(
        unreachable_peer_against("weaver-x", &rule, &ResolvedGroup::UserWithoutGroup),
        Some(Unreachable::GroupMissing),
        "the missing group is its own case and not a uid's"
    );
    // **The remedy is the one that works on that box.** A single sentence
    // covering both cases told this operator to run `gpasswd`, which is
    // what fails when the group is what is absent.
    let remedy = Unreachable::GroupMissing.remedy("weaver-x");
    assert!(
        remedy.contains("groupadd") && !remedy.contains("gpasswd"),
        "the missing-group remedy creates the group: {remedy}"
    );
    assert!(
        Unreachable::UidOutsideGroup(7)
            .remedy("weaver-x")
            .contains("gpasswd"),
        "the outside-uid remedy adds the member"
    );
    // And the field slot carries a field rather than a sentence.
    assert_eq!(
        Unreachable::UidOutsideGroup(7).field("weaver-x"),
        "gate-instruction.access-rule.allowed-uids.7"
    );
    assert_eq!(
        unreachable_peer_against("weaver-x", &rule, &ResolvedGroup::NoAgent),
        None
    );

    // A group this caller is not in, so the walk actually runs.
    let outside = ResolvedGroup::Present {
        gid: u32::MAX - 1,
        members: Vec::new(),
    };

    // **A uid with no passwd entry is unreachable, not exempt.** Group
    // membership is recorded by name, so a uid carrying no name holds no
    // group and reaches a `0770` socket it does not own by no route.
    // Continuing past it was an undeclared third exemption beside
    // `allowed-gids` and uid 0, and it passed exactly the declaration
    // this check exists to catch.
    let nameless = 4242;
    assert!(
        nix::unistd::User::from_uid(nix::unistd::Uid::from_raw(nameless))
            .ok()
            .flatten()
            .is_none(),
        "the fixture uid must carry no passwd entry on this box"
    );
    let orphan = weaver_types::AccessRule {
        allowed_uids: [nameless].into_iter().collect(),
        allowed_gids: Default::default(),
        denied_uids: Default::default(),
    };
    assert_eq!(
        unreachable_peer_against("weaver-x", &orphan, &outside),
        Some(Unreachable::UidOutsideGroup(nameless)),
        "a uid with no passwd entry is named rather than skipped"
    );

    // **And a uid the rule itself denies is not asked about.**
    // `weaver_types::authorized` gives `denied_uids` precedence, so a uid
    // in both sets never reaches `accept` whatever the mode does, and
    // naming it would refuse a declaration over a peer already refused.
    let both = weaver_types::AccessRule {
        allowed_uids: [nameless].into_iter().collect(),
        allowed_gids: Default::default(),
        denied_uids: [nameless].into_iter().collect(),
    };
    assert_eq!(
        unreachable_peer_against("weaver-x", &both, &outside),
        None,
        "a denied uid is not a peer the mode is asked about"
    );
}

/// A group this cannot read is not a contradiction it may assert, so the
/// check answers `None` rather than refusing on a fact it never
/// established. The load then fails later and loudly.
#[test]
fn an_unreadable_group_is_not_read_as_a_contradiction() {
    let rule = weaver_types::AccessRule {
        allowed_uids: [1000].into_iter().collect(),
        allowed_gids: Default::default(),
        denied_uids: Default::default(),
    };
    assert_eq!(unreachable_peer("weaver-no-such-agent-here", &rule), None);
}

/// **The group half of the check covers every binding, not the serving
/// one.**
///
/// The start step drops every worker to the group named `{identity}`, so
/// a box carrying the agent user and not its group fails the start
/// whatever the binding is.
/// Gating the whole check on `EnterBinding::Serving` let a diagnostic
/// declaration pass validate clean and fail at load, which is exactly the
/// failure the `GroupMissing` arm exists to preempt.
///
/// The other half of the rule still holds: a box carrying **neither** the
/// user nor the group has provisioned no agent and is refused on nothing,
/// which is what lets a bare checkout and CI run this at all.
///
/// Perturbation: returning the group check to inside the `Serving` arm
/// leaves the diagnostic half `Ok`. Watched failing 2026-08-29.
///
/// conforms: admin-access-rule-reaches-the-socket
#[test]
fn a_diagnostic_declaration_meets_the_group_check_too() {
    let root = scratch("diagnostic-group");
    let sink_dir = root.join("sink");
    std::fs::create_dir_all(&sink_dir).expect("sink dir");
    let home = root.join("home");
    std::fs::create_dir_all(&home).expect("home");
    std::fs::set_permissions(&sink_dir, std::fs::Permissions::from_mode(0o700)).expect("mode");
    let name = AgentName("karl".into());
    let bound = boundary(&home, 65533);

    // Diagnostic: the kind its binding requires, with the gate
    // instruction its kind excludes removed. No access rule is present,
    // so nothing but the group can refuse it.
    let source = format!(
        "binding-kind = \"diagnostic\"\n{}",
        config_source(&sink_dir).replace(
            concat!(
                "[gate-instruction.access-rule]\n",
                "allowed-uids = [0]\n",
                "allowed-gids = []\n",
                "denied-uids = [1701]\n"
            ),
            ""
        )
    );

    let refused = take_inventory_against(
        &name,
        &source,
        &bound,
        Some(&ResolvedGroup::UserWithoutGroup),
    );
    assert!(
        matches!(refused, Err(LifecycleRefusal::BoundaryUnverified)),
        "a diagnostic declaration meets the missing group: {refused:?}"
    );

    // And a box that provisioned no agent at all is refused on nothing.
    assert!(
        take_inventory_against(&name, &source, &bound, Some(&ResolvedGroup::NoAgent)).is_ok(),
        "an unprovisioned box is refused on no fact about an agent"
    );
}

/// The wiring: `take_inventory` calls the reachability check and refuses
/// on what it says.
///
/// **Runs everywhere, which the form it replaced did not.** That form
/// searched the box for a provisioned agent group, so it skipped on a
/// developer box with no `weaver-*` group and skipped on CI for the same
/// reason, while its own record claimed a perturbation was watched. The
/// group is supplied here instead of found, so the only host fact left is
/// the caller's uid.
///
/// Perturbation: dropping the `unreachable_peer` block from
/// `take_inventory_against` leaves this returning `Ok`, watched failing
/// 2026-08-29 - and now on this box rather than on a hypothetical one.
///
/// conforms: admin-access-rule-reaches-the-socket
#[test]
fn a_rule_the_mode_would_defeat_refuses_at_the_inventory() {
    let me = nix::unistd::Uid::current().as_raw();
    if me == 0 {
        diag!("SKIP: the check skips root by design");
        return;
    }
    let root = scratch("reach");
    let sink_dir = root.join("sink");
    std::fs::create_dir_all(&sink_dir).expect("sink dir");
    let home = root.join("home");
    std::fs::create_dir_all(&home).expect("home");
    std::fs::set_permissions(&sink_dir, std::fs::Permissions::from_mode(0o700)).expect("mode");
    let name = AgentName("karl".into());
    let bound = boundary(&home, 65533);

    // A group this caller is not in and cannot be: no members, and a gid
    // no real group carries. The rule then admits a uid the socket's
    // `0770` turns away at `connect`, which is the contradiction.
    let group = ResolvedGroup::Present {
        gid: u32::MAX - 1,
        members: Vec::new(),
    };
    let source = config_source(&sink_dir)
        .replace("allowed-uids = [0]\n", &format!("allowed-uids = [{me}]\n"));
    let refused = take_inventory_against(&name, &source, &bound, Some(&group));
    assert!(
        matches!(refused, Err(LifecycleRefusal::BoundaryUnverified)),
        "the unreachable uid refuses as a boundary fault: {refused:?}"
    );

    // And the same walk admits the rule the group does reach, so the
    // refusal is the contradiction and not the walk running at all.
    let reachable = ResolvedGroup::Present {
        gid: u32::MAX - 1,
        members: vec![
            nix::unistd::User::from_uid(nix::unistd::Uid::from_raw(me))
                .ok()
                .flatten()
                .map(|user| user.name)
                .unwrap_or_default(),
        ],
    };
    assert!(
        take_inventory_against(&name, &source, &bound, Some(&reachable)).is_ok(),
        "a uid inside the group passes the same walk"
    );
    let _ = std::fs::remove_dir_all(&root);
}

use std::os::unix::fs::PermissionsExt;

fn scratch(tag: &str) -> crate::scratch::Scratch {
    let dir = crate::scratch::Scratch(std::env::temp_dir().join(format!(
        "weaver-admin-inv-{tag}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    )));
    let _ = std::fs::remove_dir_all(&dir.0);
    std::fs::create_dir_all(&dir.0).expect("scratch");
    dir
}

fn boundary(home: &std::path::Path, agent_uid: u32) -> Boundary {
    Boundary {
        agent_uid,
        admin_uid: nix::unistd::getuid().as_raw(),
        // The agent is deliberately NOT in the directory's group: an agent
        // that were a member would be admitted by the group search bit,
        // and correctly so - the boundary the operator draws is exactly
        // that the agent holds no membership reaching this directory.
        agent_gids: vec![65533],
        home: home.to_path_buf(),
        // The fixtures elect no store, which resolves to the embedded
        // engine and so requires a member binary: this process's own
        // stands in for it, present on every box the suite runs on.
        member_binary: Some(std::path::PathBuf::from("/proc/self/exe")),
        // And an account for the member, which the same election
        // requires: this process's own credentials stand in for it,
        // being the one account every box the suite runs on carries.
        member_account: Some(MemberAccount {
            uid: nix::unistd::getuid().as_raw(),
            gid: nix::unistd::getgid().as_raw(),
        }),
        member_group: Some(nix::unistd::getgid().as_raw()),
    }
}

/// **The fixture's rule names uid 0**, which the reachability check
/// skips for root's `CAP_DAC_OVERRIDE`, so no test drawing this fixture
/// depends on the host's group table. It named 1000 until 2026-08-28,
/// which made three boundary tests refuse for the new reason instead of
/// the one they assert wherever the box carried the agent's group without
/// that uid in it - the suite reading one box's provisioning as a
/// property of the code. A test that wants the check judging something
/// writes its own rule.
fn config_source(sink_dir: &std::path::Path) -> String {
    format!(
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
            "create = true\n"
        ),
        sink_dir.display()
    )
}

/// The fixture with a store election appended, the rest unchanged.
fn config_source_electing(sink_dir: &std::path::Path, store: &str) -> String {
    format!("{}\n[state-store]\n{store}", config_source(sink_dir))
}

/// **The store election's declaration half**, per `weaver-admin-Spec`
/// section 4 as of 2026-09-04: `none` beside a state election refuses
/// naming `state-election`, the retired service engine's `database` and
/// `role` refuse at the parse as unknown fields, and each refusal is
/// `ConfigInvalid` naming the field. Perturbation: remove the match's
/// `state-election` arm and its case passes the inventory; give either
/// retired key back to the grammar and its case reads.
#[test]
fn the_store_election_is_judged_before_the_box() {
    let name = AgentName("alpha".into());
    let root = crate::scratch::Scratch(
        std::env::temp_dir().join(format!("wt-store-decl-{}", std::process::id())),
    );
    let _ = std::fs::remove_dir_all(&root);
    let home = root.join("home");
    std::fs::create_dir_all(&home).expect("home");
    let boundary = boundary(&home, 65533);
    let cases: [(&str, &str); 4] = [
        (
            "engine = \"none\"\n\n[state-election]\nall-kinds = true\nkeys = []\n",
            "state-election",
        ),
        // The service engine's keys left the grammar (the operator's
        // ruling of 2026-10-08 on #1): each refuses at the parse as an
        // unknown field, named by its key.
        ("engine = \"none\"\ndatabase = \"d\"\n", "database"),
        ("engine = \"sqlite\"\nrole = \"r\"\n", "role"),
        // An engine this build does not provide refuses at the parse.
        ("engine = \"postgres\"\n", "state-store.engine"),
    ];
    for (store, field) in cases {
        let source = config_source_electing(&home, store);
        let refused = take_inventory(&name, &source, &boundary);
        match refused {
            Err(LifecycleRefusal::ConfigInvalid { field: Some(ref f) }) if f.0 == field => {}
            other => panic!("{store:?} should refuse naming {field}, got {other:?}"),
        }
    }
    let _ = std::fs::remove_dir_all(&root);
}

/// **Every election but `none` requires the member's binary**,
/// `BoundaryUnverified`. Perturbation: drop the binary check and the first
/// case passes the inventory or fails later on the sink instead.
#[test]
fn every_election_but_none_requires_the_member() {
    let name = AgentName("alpha".into());
    let root = crate::scratch::Scratch(
        std::env::temp_dir().join(format!("wt-store-box-{}", std::process::id())),
    );
    let _ = std::fs::remove_dir_all(&root);
    let home = root.join("home");
    std::fs::create_dir_all(&home).expect("home");
    let sink_dir = root.join("sink");
    std::fs::create_dir_all(&sink_dir).expect("sink");
    // The sink is admin's, mode-locked, so the walks before the store's
    // pass and the store's own is what refuses.
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&sink_dir, std::fs::Permissions::from_mode(0o700)).expect("mode");

    let mut without_binary = boundary(&home, 65533);
    without_binary.member_binary = None;
    let embedded = config_source(&sink_dir);
    assert!(
        matches!(
            take_inventory(&name, &embedded, &without_binary),
            Err(LifecycleRefusal::BoundaryUnverified)
        ),
        "an absent election is the embedded engine and requires the member"
    );
    let declined = config_source_electing(&sink_dir, "engine = \"none\"\n");
    assert!(
        take_inventory(&name, &declined, &without_binary).is_ok(),
        "none declines the member and requires nothing"
    );

    let _ = std::fs::remove_dir_all(&root);
}

/// **Every election but `none` requires the member's own account**, the
/// way it requires the member's binary, and for the same reason: the
/// member holds its territory by owning it, per `weaver-state-PRD`
/// section 4, so a member with no account has nothing to own and would
/// run as this crate does. The name is derived rather than declared, so
/// the refusal can say which account the box is missing.
///
/// Perturbation: remove the `member_account` arm from `take_inventory`
/// and the first case passes the inventory, the load then standing a
/// member under admin's own identity - which is the whole of what issue
/// #545 found. Watched failing 2026-09-15.
///
/// conforms: admin-member-account-required-at-inventory
#[test]
fn every_election_but_none_requires_the_members_own_account() {
    let name = AgentName("alpha".into());
    assert_eq!(
        member_identity_for(&name),
        "weaver-alpha-state",
        "the account deploy/create-agent.sh makes, derived from the same name"
    );
    let root = crate::scratch::Scratch(
        std::env::temp_dir().join(format!("wt-member-account-{}", std::process::id())),
    );
    let _ = std::fs::remove_dir_all(&root);
    let sink_dir = root.join("sink");
    std::fs::create_dir_all(&sink_dir).expect("sink");
    // Admin's own, mode-locked, so the walks ahead of the store's pass
    // and the store's own is what refuses.
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&sink_dir, std::fs::Permissions::from_mode(0o700)).expect("mode");

    let mut unprovisioned = boundary(&sink_dir, 65533);
    unprovisioned.member_account = None;
    let embedded = config_source(&sink_dir);
    assert!(
        matches!(
            take_inventory(&name, &embedded, &unprovisioned),
            Err(LifecycleRefusal::BoundaryUnverified)
        ),
        "an absent election is the embedded engine and requires the account"
    );
    let declined = config_source_electing(&sink_dir, "engine = \"none\"\n");
    assert!(
        take_inventory(&name, &declined, &unprovisioned).is_ok(),
        "none declines the member and requires nothing"
    );
    assert!(
        take_inventory(&name, &embedded, &boundary(&sink_dir, 65533)).is_ok(),
        "and a box carrying the account passes, so the refusal is the account's"
    );
    // **A `none` agent reports no member though the box carries the
    // account** (the #94 survey's S2): every consumer reads the report's
    // account as "a member stands". Perturbation: answer the box's
    // account whatever the engine and this reads it.
    let provisioned = boundary(&sink_dir, 65533);
    assert!(provisioned.member_account.is_some());
    let report = take_inventory(&name, &declined, &provisioned).expect("none reads");
    assert_eq!(report.member_account, None, "none stands no member");
    let report = take_inventory(&name, &embedded, &provisioned).expect("sqlite reads");
    assert_eq!(report.member_account, provisioned.member_account);
    // **The account's primary group must be the state group, and not
    // gid 0** (#99 area 2, K1). Perturbation: drop the comparison and
    // both pass the inventory.
    let mut apart = boundary(&sink_dir, 65533);
    apart.member_group = apart.member_group.map(|gid| gid.wrapping_add(1));
    assert!(
        matches!(
            take_inventory(&name, &embedded, &apart),
            Err(LifecycleRefusal::BoundaryUnverified)
        ),
        "a primary group other than the state group refuses"
    );
    let mut unresolved = boundary(&sink_dir, 65533);
    unresolved.member_group = None;
    assert!(
        take_inventory(&name, &embedded, &unresolved).is_err(),
        "a state group that does not resolve refuses"
    );
    let mut rooted = boundary(&sink_dir, 65533);
    rooted.member_account = Some(MemberAccount { uid: 65533, gid: 0 });
    rooted.member_group = Some(0);
    assert!(
        take_inventory(&name, &embedded, &rooted).is_err(),
        "a member grouped to gid 0 refuses even where the names agree"
    );
    assert!(
        take_inventory(&name, &declined, &apart).is_ok(),
        "none stands no member, so its group is not asked"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// The uid, gid and group lines of a `/proc/<pid>/status` text, each
/// with its fields joined by single spaces.
fn status_identity(status: &str) -> (String, String, String) {
    let line = |key: &str| {
        status
            .lines()
            .find_map(|l| l.strip_prefix(key))
            .map(|rest| rest.split_whitespace().collect::<Vec<_>>().join(" "))
            .unwrap_or_default()
    };
    (line("Uid:"), line("Gid:"), line("Groups:"))
}

/// **The saved ids, checked where they can still be seen.** `execve`
/// copies the effective ids into the saved ones, so a status read after
/// exec shows saved equal to effective whatever the drop left: that read
/// cannot watch this. Registered after [`identity_command`]'s own
/// pre-exec, this runs in the child after `drop_to` and before exec, and
/// fails the spawn with `ENOTRECOVERABLE` unless all three uids and all
/// three gids are the target. Two syscalls and no allocation.
fn and_no_saved_id_survives(command: &mut std::process::Command, uid: u32, gid: u32) {
    use std::os::unix::process::CommandExt;
    // SAFETY: `getresuid` and `getresgid` are async-signal-safe and the
    // closure allocates nothing.
    unsafe {
        command.pre_exec(move || {
            let (mut r, mut e, mut s) = (0, 0, 0);
            let (mut rg, mut eg, mut sg) = (0, 0, 0);
            if nix::libc::getresuid(&mut r, &mut e, &mut s) < 0
                || nix::libc::getresgid(&mut rg, &mut eg, &mut sg) < 0
            {
                return Err(std::io::Error::last_os_error());
            }
            if [r, e, s] != [uid; 3] || [rg, eg, sg] != [gid; 3] {
                return Err(std::io::Error::from_raw_os_error(
                    nix::libc::ENOTRECOVERABLE,
                ));
            }
            Ok(())
        });
    }
}

/// **The identity drop, measured in the kernel.** Needs euid 0, and runs
/// through `the_identity_drop_is_watched_inside_a_user_namespace` on a box
/// where that is not the invoking uid. Two readings, each for a different
/// half of the property (a third, the store probe's own construction,
/// retired with the service engine on the ruling of 2026-10-02 on #1):
///
/// 1. Before exec, through [`and_no_saved_id_survives`]: the real,
///    effective and saved uids are all 4242 and the gids all 4243.
/// 2. After exec, from the child's own status: the uid and gid lines, and
///    the supplementary set exactly 4243 and 4244, none of root's.
///
/// Perturbations, each watched failing 2026-09-24 through the namespace
/// watch:
///
/// - the form issue #675 measured, `CommandExt::uid` and `::gid` with
///   `setgroups` alone in the pre-exec: the spawn fails `EPERM`;
/// - `setgroups` removed from `drop_to`: root's supplementary set remains;
/// - `setresuid(user, user, 0)` or `setresgid(p, p, 0)`: reading 1;
///
#[test]
#[ignore = "needs euid 0: run by the_identity_drop_is_watched_inside_a_user_namespace"]
fn identity_command_lands_the_whole_identity_as_root() {
    assert_eq!(
        nix::unistd::geteuid().as_raw(),
        0,
        "this instrument needs euid 0"
    );

    // Readings 1 and 2: identity_command, checked before and after exec.
    let mut command = identity_command(std::path::Path::new("/bin/cat"), 4242, &[4243, 4244])
        .expect("an identity with a primary group builds");
    and_no_saved_id_survives(&mut command, 4242, 4243);
    command
        .arg("/proc/self/status")
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let output = command
        .output()
        .expect("the dropped child spawns with every uid and gid dropped, saved ones included");
    assert!(
        output.status.success(),
        "the dropped child reads its own status"
    );
    let (uids, gids, groups) =
        status_identity(&String::from_utf8(output.stdout).expect("status is ascii"));
    assert_eq!(uids, "4242 4242 4242 4242", "the uid line after the drop");
    assert_eq!(gids, "4243 4243 4243 4243", "the primary group, all four");
    assert_eq!(
        groups, "4243 4244",
        "exactly the identity's set, none of root's"
    );

    let empty = identity_command(std::path::Path::new("/bin/cat"), 4242, &[]);
    assert!(
        empty.is_err_and(|e| e.kind() == std::io::ErrorKind::InvalidInput),
        "an identity with no primary group is refused before any spawn"
    );
}

/// **The watch that runs on an ordinary `cargo test`.** Re-executes this
/// test binary inside `unshare --map-auto --map-root-user`, where the
/// process is uid 0 over the invoking user's subordinate ids, and requires
/// the instrument above to report exactly one test passed, so a filter
/// that matched nothing cannot read as green.
///
/// **Where it runs.** On thinkpad, measured 2026-09-24: `newuidmap` and
/// `newgidmap` present, the operator's user holding `100000:65536` in
/// `/etc/subuid` and `/etc/subgid`. A box where the namespace cannot be
/// entered prints a SKIP naming why and passes. That box has no watch, and
/// says so rather than claiming one. A box where the namespace enters and
/// the instrument fails, fails here.
///
/// Perturbation: as the instrument's, watched through this test.
#[test]
fn the_identity_drop_is_watched_inside_a_user_namespace() {
    if nix::unistd::geteuid().is_root() {
        return identity_command_lands_the_whole_identity_as_root();
    }
    let exe = std::env::current_exe().expect("the test binary names itself");
    let ran = std::process::Command::new("unshare")
        .args(["--map-auto", "--map-root-user"])
        .arg(&exe)
        .args([
            "--exact",
            "inventory::tests::identity_command_lands_the_whole_identity_as_root",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .stdin(std::process::Stdio::null())
        .output();
    let output = match ran {
        Ok(output) => output,
        Err(e) => {
            diag!("SKIP identity drop watch: unshare could not run: {e}");
            return;
        }
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.starts_with("unshare:") {
        diag!(
            "SKIP identity drop watch: no user namespace here: {}",
            stderr.trim()
        );
        return;
    }
    assert!(
        output.status.success() && stdout.contains("test result: ok. 1 passed"),
        "the identity drop failed inside the namespace\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

/// **A declaration whose gate instruction disagrees with its kind is
/// refused at the inventory, before any look at the filesystem.** The
/// refusal names the field, so the operator learns which member to move.
///
/// Perturbation: remove the cross-field check from `take_inventory` and
/// the diagnostic-with-instruction case sails through to a load whose
/// binding admin cannot construct honestly. Watched under exactly that
/// removal. The boundary handed in is deliberately broken - a missing
/// home would refuse later - so a pass through to a boundary refusal is
/// the perturbation showing.
#[test]
fn a_kind_gate_disagreement_refuses_at_the_inventory() {
    let root = scratch("kind");
    let name = AgentName("alpha".to_string());
    let bound = boundary(&root.join("absent-home"), 65533);

    // Diagnostic, carrying the instruction its kind excludes.
    let source = format!("binding-kind = \"diagnostic\"\n{}", config_source(&root));
    let refused = take_inventory(&name, &source, &bound);
    assert!(
        matches!(
            refused,
            Err(LifecycleRefusal::ConfigInvalid { field: Some(ref f) }) if f.0 == "gate-instruction"
        ),
        "the diagnostic instruction refuses naming the field: {refused:?}"
    );

    // Serving, with the instruction its kind requires removed.
    let source = config_source(&root).replace(
        concat!(
            "[gate-instruction.access-rule]\n",
            "allowed-uids = [0]\n",
            "allowed-gids = []\n",
            "denied-uids = [1701]\n"
        ),
        "",
    );
    let refused = take_inventory(&name, &source, &bound);
    assert!(
        matches!(
            refused,
            Err(LifecycleRefusal::ConfigInvalid { field: Some(ref f) }) if f.0 == "gate-instruction"
        ),
        "the serving omission refuses naming the field: {refused:?}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// **A declaration granting the re-feed permission is refused.** The
/// member is this crate's to set from the resolved kind at the
/// construction, per `weaver-admin-Spec` section 7, and it parses with
/// `default` because one type serves the declaration and the seam, so
/// this refusal is the declaration's whole guard.
///
/// Perturbation: remove the permission check from the inventory and the
/// granting declaration loads. Watched under exactly that removal.
///
/// conforms: admin-granted-permission-refused-at-inventory
#[test]
fn a_declaration_granting_the_permission_refuses_at_the_inventory() {
    let root = scratch("permission");
    let name = AgentName("alpha".to_string());
    let bound = boundary(&root.join("absent-home"), 65533);

    let source = config_source(&root).replace(
        "residual-readout-election = false\n",
        "residual-readout-election = false\nrefeed-permission = true\n",
    );
    assert_ne!(
        source,
        config_source(&root),
        "the grant landed in the source"
    );
    let refused = take_inventory(&name, &source, &bound);
    assert!(
        matches!(
            refused,
            Err(LifecycleRefusal::ConfigInvalid { field: Some(ref f) })
                if f.0 == "spu-instruction.decoder.refeed-permission"
        ),
        "the granting declaration refuses naming the field: {refused:?}"
    );
    // The sibling, on the same terms.
    let source = config_source(&root).replace(
        "residual-readout-election = false\n",
        "residual-readout-election = false\ncolumn-permission = true\n",
    );
    let refused = take_inventory(&name, &source, &bound);
    assert!(
        matches!(
            refused,
            Err(LifecycleRefusal::ConfigInvalid { field: Some(ref f) })
                if f.0 == "spu-instruction.decoder.column-permission"
        ),
        "the granting declaration refuses naming the sibling: {refused:?}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// **The second walk: the agent reaches the sink by path.** The
/// containing directory must deny the agent uid the search bit, so the
/// kernel refuses the lookup before any mode on the file is consulted.
///
/// Perturbation: remove the traversal check from the inventory and the
/// world-searchable boundary loads. Watched under exactly that removal.
#[test]
fn a_traversable_sink_directory_refuses_the_load() {
    let root = scratch("traverse");
    let sink_dir = root.join("sink");
    std::fs::create_dir_all(&sink_dir).expect("sink dir");
    let home = root.join("home");
    std::fs::create_dir_all(&home).expect("home");
    let name = AgentName("alpha".to_string());
    // An agent uid that is nobody here, so ownership is not the route.
    let bound = boundary(&home, 65533);

    // World-searchable: the kernel would let the agent traverse.
    std::fs::set_permissions(&sink_dir, std::fs::Permissions::from_mode(0o755)).expect("mode");
    let refused = take_inventory(&name, &config_source(&sink_dir), &bound);
    assert!(
        matches!(refused, Err(LifecycleRefusal::BoundaryUnverified)),
        "a traversable sink directory refuses, got {refused:?}"
    );

    // Admin-owned and unsearchable by anyone else: the boundary the
    // operator is required to draw.
    std::fs::set_permissions(&sink_dir, std::fs::Permissions::from_mode(0o750)).expect("mode");
    let admitted = take_inventory(&name, &config_source(&sink_dir), &bound);
    assert!(
        admitted.is_ok(),
        "an unsearchable boundary admits: {admitted:?}"
    );
}

/// A path this crate cannot resolve yields no boundary evidence, and an
/// agent that owns the directory can restore the search bit itself.
///
/// Perturbation: read an unresolvable parent as not-traversable, or admit
/// an agent-owned directory whose mode currently denies search, and both
/// assertions below fail.
#[test]
fn unresolvable_and_agent_owned_directories_are_not_boundaries() {
    let root = scratch("evidence");
    let bound = boundary(&root, nix::unistd::getuid().as_raw());
    assert!(
        agent_can_traverse(std::path::Path::new(""), &bound),
        "a relative sink path's empty parent is no evidence of a boundary"
    );
    // This process owns the scratch directory, so presenting our own uid as
    // the agent's is how the test presents the ownership case.
    // The search bit is cleared first, which is what makes the watch
    // reachable: with it set, a check that also required the bit would
    // pass and the test could not tell the two apart.
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o600))
        .expect("clear the search bit");
    assert!(
        agent_can_traverse(&root, &bound),
        "an agent that owns the directory can restore the search bit itself"
    );
    let _ = std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700));
}

/// **The sink boundary has two halves and neither implies the other.** A
/// directory owned by a third principal at mode 0700 denies the agent its
/// traversal and still defeats custody, because that owner controls the
/// file admin opened.
///
/// Perturbation: drop the custody check from the inventory and the
/// third-party case loads. Watched under exactly that removal.
#[test]
fn a_third_party_owned_sink_directory_refuses_the_load() {
    // **Unreachable as root, and skipped rather than asserted anyway.**
    // A directory this test creates is owned by whoever runs it, and the
    // custody check admits root by name, so under root the third-party
    // case cannot be constructed without a second real uid to chown to.
    // Asserting through it would be a watch that reports on the runner
    // rather than on the check.
    if nix::unistd::getuid().is_root() {
        diag!(
            "SKIP a_third_party_owned_sink_directory_refuses_the_load: run as root, \
                 so a created directory is root-owned and holds custody by name"
        );
        return;
    }
    let root = scratch("custody");
    let sink_dir = root.join("sink");
    std::fs::create_dir_all(&sink_dir).expect("sink dir");
    std::fs::set_permissions(&sink_dir, std::fs::Permissions::from_mode(0o750)).expect("mode");
    let home = root.join("home");
    std::fs::create_dir_all(&home).expect("home");
    let name = AgentName("alpha".to_string());

    // This process owns the directory, so it holds custody and the load
    // is admitted: the denial half already passes at 0750.
    let held = boundary(&home, 65533);
    assert!(
        take_inventory(&name, &config_source(&sink_dir), &held).is_ok(),
        "an admin-owned directory holds custody"
    );

    // Now present the same directory to a boundary whose admin principal
    // is somebody else. The agent still cannot traverse it - nothing about
    // the directory changed - and custody is now a third party's, which is
    // exactly the case the traversal check alone admits.
    let third_party = Boundary {
        agent_uid: 65533,
        admin_uid: 65532,
        agent_gids: vec![65531],
        home: home.clone(),
        member_binary: Some(std::path::PathBuf::from("/proc/self/exe")),
        member_account: Some(MemberAccount {
            uid: nix::unistd::getuid().as_raw(),
            gid: nix::unistd::getgid().as_raw(),
        }),
        member_group: Some(nix::unistd::getgid().as_raw()),
    };
    assert!(
        !agent_can_traverse(&sink_dir, &third_party),
        "the denial half still passes, which is what makes this case reachable"
    );
    let refused = take_inventory(&name, &config_source(&sink_dir), &third_party);
    assert!(
        matches!(refused, Err(LifecycleRefusal::BoundaryUnverified)),
        "a directory a third principal owns refuses the load, got {refused:?}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// **A declaration naming `restore` parses here and resolves no lineage
/// here**, per `weaver-admin-Spec` section 4 as of A3.2: the selection,
/// the judgment of the bytes and the lineage are the load's, through the
/// manifest, so the inventory carries the name parsed and nothing more;
/// the record form of before A3.2 refuses by name at the parse.
///
/// Perturbation: resolve a lineage here from the name and the first
/// assertion fails, a lineage nobody judged.
#[test]
fn a_restore_parses_at_the_inventory_and_the_load_selects_it() {
    let root = scratch("restore");
    let sink_dir = root.join("sink");
    std::fs::create_dir_all(&sink_dir).expect("sink dir");
    std::fs::set_permissions(&sink_dir, std::fs::Permissions::from_mode(0o750)).expect("mode");
    let home = root.join("home");
    std::fs::create_dir_all(&home).expect("home");
    let name = AgentName("alpha".into());
    let bound = boundary(&home, 65533);
    let source = format!(
        "{}\n[restore]\nsave-point = \"20261006T000000Z-ab.save-point\"\n",
        config_source(&sink_dir)
    );
    let taken = take_inventory(&name, &source, &bound).expect("admits");
    assert!(
        taken.lineage.is_none(),
        "the lineage is the load's to resolve"
    );
    assert_eq!(
        taken.config.restore.map(|r| r.save_point).as_deref(),
        Some("20261006T000000Z-ab.save-point")
    );
    let record = format!(
        "{}\n[restore]\nrecord = \"{}\"\n",
        config_source(&sink_dir),
        root.join("s-1.ndjson").display()
    );
    let refused = take_inventory(&name, &record, &bound);
    assert!(
        matches!(refused, Err(LifecycleRefusal::ConfigInvalid { field: Some(ref f) }) if f.0 == "record"),
        "the record form refuses by name, got {refused:?}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// **A missing home refuses and the walk builds nothing**, per
/// `weaver-admin-Spec` section 4 as of 2026-09-07 and issue #481: the
/// refusal is `BoundaryUnverified` and the directory is still absent
/// after it. This test stood from 2026-08-05 and lost its attribute on
/// 2026-09-06 when the restore test above was inserted between the
/// attribute and this function.
///
/// Perturbation: create the home on the miss instead of refusing and the
/// second assertion fails on the directory it finds. Watched under
/// exactly that change.
#[test]
fn the_inventory_repairs_nothing() {
    let root = scratch("repair");
    let sink_dir = root.join("sink");
    std::fs::create_dir_all(&sink_dir).expect("sink dir");
    std::fs::set_permissions(&sink_dir, std::fs::Permissions::from_mode(0o750)).expect("mode");
    let absent_home = root.join("no-such-home");
    let bound = boundary(&absent_home, 65533);
    let refused = take_inventory(
        &AgentName("alpha".into()),
        &config_source(&sink_dir),
        &bound,
    );
    assert!(matches!(refused, Err(LifecycleRefusal::BoundaryUnverified)));
    assert!(!absent_home.exists(), "nothing was built");
}

/// **The artifact is named here and resolved at admission**, per
/// `weaver-admin-Spec` section 4 as ruled 2026-09-05 on issue #456. A
/// binding naming a path this box does not hold passes the inventory,
/// because resolution is the SPU's under the agent's identity and a
/// root-side look would pass what the agent is denied. An empty member is
/// the declaration's omission and refuses naming the field.
///
/// Perturbation: restore a resolution check on the artifact and the first
/// assertion refuses on the absent path, or drop the presence check and
/// the second admits an unnamed binding. Watched under both.
#[test]
fn the_artifact_is_named_here_and_resolved_at_admission() {
    let root = scratch("artifact");
    let sink_dir = root.join("sink");
    std::fs::create_dir_all(&sink_dir).expect("sink dir");
    std::fs::set_permissions(&sink_dir, std::fs::Permissions::from_mode(0o750)).expect("mode");
    let home = root.join("home");
    std::fs::create_dir_all(&home).expect("home");
    let name = AgentName("alpha".into());
    let bound = boundary(&home, 65533);

    let absent = config_source(&sink_dir).replace(
        "artifact = \"qwen3-4b-instruct\"",
        "artifact = \"/no/such/directory/model.gguf\"",
    );
    let admitted = take_inventory(&name, &absent, &bound);
    assert!(
        admitted.is_ok(),
        "an absent artifact path is the SPU's to refuse, got {admitted:?}"
    );

    let unnamed =
        config_source(&sink_dir).replace("artifact = \"qwen3-4b-instruct\"", "artifact = \"\"");
    let refused = take_inventory(&name, &unnamed, &bound);
    match refused {
        Err(LifecycleRefusal::ConfigInvalid { field: Some(ref f) })
            if f.0 == "spu-instruction.decoder.model-binding.artifact" => {}
        other => panic!("an unnamed artifact refuses naming the field, got {other:?}"),
    }
}
