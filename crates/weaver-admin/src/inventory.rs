//! conforms: admin-inventory-one-function
//! conforms: admin-existence-checks-repair-nothing
//! conforms: admin-missing-home-refuses-and-builds-nothing
//! conforms: admin-boundary-denies-agent-traversal
//! conforms: admin-checks-no-device
//! conforms: admin-identity-from-validated-name
//! conforms: admin-kind-mismatch-refused-at-inventory
//! conforms: admin-restore-cut-judged-at-the-inventory
//!
//! The inventory, per `weaver-admin-Spec` section 4: **one function**, called
//! by `validate` and by `load`'s steps 2 and 3, refusing at the first failure
//! with the field or check named. The two callers cannot drift, which is the
//! charter's one-code-path-entered-two-ways rule made structural.

use std::path::Path;

use weaver_types::{
    AgentConfig, AgentName, BindingKind, ConfigErrorKind, EnterBinding, FieldName,
    LifecycleRefusal, StoreEngine, TraceSink,
};

/// What the boundary check needs to know about the host, supplied by the
/// caller rather than discovered here, so a test can present a boundary
/// without provisioning one.
#[derive(Debug, Clone)]
pub struct Boundary {
    /// The agent's uid, resolved from the validated name.
    pub agent_uid: u32,
    /// The admin principal's uid, for the custody half of the sink boundary.
    pub admin_uid: u32,
    /// The agent's groups, for the search-bit reasoning below.
    pub agent_gids: Vec<u32>,
    /// The agent's home, which must exist.
    pub home: std::path::PathBuf,
    /// The state member's binary where the box carries one beside the
    /// worker's, per `weaver-admin-Spec` section 4 as of 2026-09-04: every
    /// election but `none` requires it, so its absence is the box's fault
    /// and never an absent member.
    pub member_binary: Option<std::path::PathBuf>,
    /// The state member's own account where the box carries one, per
    /// `weaver-state-PRD` section 4: the member holds its territory by
    /// owning it, so every election but `none` requires the account the
    /// spawn drops to and the store admits. `None` is an unprovisioned box
    /// and never an absent member, the same reading `member_binary` takes.
    pub member_account: Option<MemberAccount>,
    /// The state group `weaver-<agent>-state` resolved by name, which the
    /// territory is judged against (`weaver-admin-Spec` section 9). The
    /// member passes the territory by its primary group, so the account's
    /// gid must be this one.
    pub member_group: Option<u32>,
}

/// The state member's own kernel identity: the uid the spawn drops to and
/// the uid that owns the territory, which are one uid because the charter's
/// custody argument rests on their being one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemberAccount {
    pub uid: u32,
    pub gid: u32,
}

/// The one identity-constructing site.
///
/// The constructed identity is `weaver-<name>` **from the validated name,
/// never from a caller-supplied string**, which is the argless-grant
/// discipline landing at the one site that constructs. It is the one site
/// because the same validated name is what the start step drops the worker to,
/// so the delegated authority has one origin.
pub fn identity_for(name: &AgentName) -> String {
    format!("weaver-{}", name.0)
}

/// The state member's own account name, `weaver-<name>-state`, constructed
/// from the same validated name and at the same one site, per
/// `weaver-admin-Spec` section 6.
///
/// **It derives rather than being declared.** An account named in the
/// agent's file would be a second place the member's identity is stated, and
/// the store's identity map, the territory's owner, and the spawn's uid would
/// then answer to a value the operator can move under a running agent. The
/// derivation makes `deploy/create-agent.sh`'s `weaver-<name>-state` and this
/// crate's lookup one fact.
pub fn member_identity_for(name: &AgentName) -> String {
    format!("{}-state", identity_for(name))
}

/// The report a completed inventory yields: what was read, for the caller that
/// proceeds to a load and for the verb that stops here.
#[derive(Debug, Clone)]
pub struct Inventory {
    pub config: AgentConfig,
    pub identity: String,
    /// The declaration file's digest as this inventory read it, per
    /// `weaver-admin-harness-contract` section 5 as of 2026-09-04: supplied
    /// on the enter so the run and the record name what they were built
    /// from, and this crate is the file's one reader.
    pub declaration: String,
    /// The binding kind resolved, per `weaver-admin-Spec` section 7: the one
    /// inventory function resolves it, so the verb and the load cannot
    /// resolve differently, and what the enter carries is this value.
    pub binding: EnterBinding,
    /// The restore's lineage where the declaration elects one, resolved
    /// here per `weaver-admin-Spec` section 4 as of 2026-09-04: the parent's
    /// session, the run the cut falls in, and the turn the holdings stop
    /// at, a whole record resolved to its last run's last turn. The record's
    /// path never leaves this crate.
    pub lineage: Option<weaver_types::Lineage>,
    /// The state member's account, carried from the boundary the walk
    /// verified so the spawn takes the account the store was asked about
    /// rather than resolving it a second time.
    pub member_account: Option<MemberAccount>,
}

/// The one inventory function.
///
/// The name was admitted by the agent's root before this runs, per
/// `weaver-admin-Spec` section 9. The parse is the floor's - `weaver_types::parse` yields a whole config or a typed error,
/// and this crate adds no partial reader. The existence checks are admin's,
/// and **each is a look rather than an ask**: nothing is repaired and nothing
/// is built.
///
/// **The devices the binding assigns are not checked here**, and the absence
/// is deliberate: whether they exist, have room, or can reach each other are
/// questions about hardware, and admin reasons about the device at no point.
/// An admin that verified the GPU would be a second arbitrator reintroduced as
/// a convenience.
pub fn take_inventory(
    name: &AgentName,
    source: &str,
    boundary: &Boundary,
) -> Result<Inventory, LifecycleRefusal> {
    take_inventory_against(name, source, boundary, None)
}

/// The whole of `take_inventory` with the host's answer about the agent's
/// group supplied rather than read.
///
/// **The seam exists so the reachability call site is watched.** The judgment
/// `unreachable_peer_against` makes was already testable this way and the
/// wiring, whether `take_inventory` calls it and refuses on what it says, was
/// not: its only watch first searched the box for a provisioned agent group
/// excluding the caller, found none on a developer box and none on CI, and
/// returned early - so it skipped in both environments the suite runs in
/// while its record claimed a perturbation. A watch that runs nowhere is the
/// thing this program calls representation.
///
/// `None` reads the host, which is every caller outside the tests.
fn take_inventory_against(
    name: &AgentName,
    source: &str,
    boundary: &Boundary,
    group: Option<&ResolvedGroup>,
) -> Result<Inventory, LifecycleRefusal> {
    let identity = identity_for(name);

    let config = weaver_types::parse(source).map_err(|e| match e.kind {
        ConfigErrorKind::MissingField
        | ConfigErrorKind::UnknownField
        | ConfigErrorKind::BadValue
        | ConfigErrorKind::Malformed => LifecycleRefusal::ConfigInvalid { field: e.field },
    })?;

    // The kind conditions the gate instruction's presence, per
    // `weaver-types-Spec` section 2: a serving declaration requires it and a
    // diagnostic declaration excludes it. The parse checks each field alone,
    // so the cross-field rule lands here, before any look at the filesystem
    // and before any unit starts. Absence of the kind means serving, per
    // `weaver-types-PRD` section 2.1, the ruled meaning of its absence.
    let binding = match (
        config.binding_kind.clone().unwrap_or(BindingKind::Serving),
        config.gate_instruction.clone(),
    ) {
        (BindingKind::Serving, Some(gate_instruction)) => {
            EnterBinding::Serving { gate_instruction }
        }
        (BindingKind::Diagnostic, None) => EnterBinding::Diagnostic,
        _ => {
            return Err(LifecycleRefusal::ConfigInvalid {
                field: Some(FieldName("gate-instruction".into())),
            });
        }
    };

    // **The store election is judged here**, per `weaver-admin-Spec` section
    // 4 as of 2026-09-04, the declaration's half first, before any look at
    // the box. The parse checks each field alone, so the cross-field rules
    // land here: `none` declines the member and so refuses a state election
    // beside it, an election with no member to receive it being malformed
    // rather than surplus. The service engine's `database` and `role` are no
    // longer in the grammar and refuse at the parse as unknown fields.
    let store = config.state_store.clone().unwrap_or_default();
    let store_invalid = |field: &str| LifecycleRefusal::ConfigInvalid {
        field: Some(FieldName(field.into())),
    };
    match store.engine {
        StoreEngine::None if config.state_election.is_some() => {
            return Err(store_invalid("state-election"));
        }
        StoreEngine::None | StoreEngine::Sqlite => {}
    }

    // A granted permission member is refused, per `weaver-admin-Spec`
    // section 7: the member is this crate's to set from the resolved kind at
    // the construction, and it parses with `default` because one type
    // serves the declaration and the seam, so the refusal lands here, where
    // the file is seen whole, rather than at a parse that would also refuse
    // the instruction this crate authors.
    if config.spu_instruction.decoder.refeed_permission {
        return Err(LifecycleRefusal::ConfigInvalid {
            field: Some(FieldName(
                "spu-instruction.decoder.refeed-permission".into(),
            )),
        });
    }
    if config.spu_instruction.decoder.column_permission {
        return Err(LifecycleRefusal::ConfigInvalid {
            field: Some(FieldName(
                "spu-instruction.decoder.column-permission".into(),
            )),
        });
    }

    // **The model binding's artifact is checked for presence and never
    // resolved here**, per `weaver-admin-Spec` section 4 as ruled 2026-09-05
    // on issue #456. Whether it resolves is the SPU's to answer at admission
    // under the agent's identity: a look from here runs as root and passes a
    // directory the agent uid is denied, which would document as enforced a
    // check that enforces nothing. An empty member is the declaration's own
    // omission and refuses as one.
    if config
        .spu_instruction
        .decoder
        .model_binding
        .artifact
        .0
        .is_empty()
    {
        return Err(LifecycleRefusal::ConfigInvalid {
            field: Some(FieldName(
                "spu-instruction.decoder.model-binding.artifact".into(),
            )),
        });
    }

    // The agent's home exists.
    if !boundary.home.is_dir() {
        return Err(LifecycleRefusal::BoundaryUnverified);
    }

    // The sink exists or its creation flag is set.
    if !sink_present_or_creatable(&config.trace_sink) {
        return Err(LifecycleRefusal::BoundaryUnverified);
    }

    // **The sink boundary has two halves and neither implies the other.**
    //
    // Denial: the containing directory denies the agent uid the search bit, so
    // the kernel refuses the lookup before any mode on the file is consulted.
    //
    // Custody: the directory is owned by root or by the admin principal. A
    // directory owned by some third principal at mode 0700 satisfies the
    // denial and defeats the custody, because that owner can chmod, replace,
    // or remove the file admin opened - and custody of where the record leaves
    // the system is admin's by charter. Ownership does not by itself deny a
    // traversal that mode grants, and denial does not by itself establish
    // custody, so both are checked. The directory is the territory, root's at
    // `0710` under the state group, which the agent's uid does not hold (the
    // operator's ruling of 2026-10-08 on #1).
    let directory = sink_directory(&config.trace_sink);
    if agent_can_traverse(directory, boundary) {
        return Err(LifecycleRefusal::BoundaryUnverified);
    }
    if !admin_holds_custody(directory, boundary) {
        return Err(LifecycleRefusal::BoundaryUnverified);
    }

    // **The store's box half**, per `weaver-admin-Spec` section 4 as of
    // 2026-09-04. Every election but `none` stands a member and so requires
    // the member's binary: the leg's standing is the declaration's fact and
    // not the directory's, per `weaver-state-PRD` section 4 and issue #381,
    // so a box lacking the binary refuses rather than running without a leg
    // the declaration never declined. The refusal is `BoundaryUnverified` and
    // never `ConfigInvalid`, for the reason the group case below gives: the
    // declaration is well formed and the fault is the provisioning's.
    if store.engine != StoreEngine::None && boundary.member_binary.is_none() {
        diag!("boundary unverified: no weaver-state binary beside the worker's");
        return Err(LifecycleRefusal::BoundaryUnverified);
    }
    // **And its account**, per `weaver-state-PRD` section 4: the member holds
    // its territory by owning it, so a member with no account of its own has
    // no territory to hold and would run as this crate does. A box lacking
    // the account refuses here for the reason a box lacking the binary does,
    // the provisioning being what is missing.
    //
    // conforms: admin-member-account-required-at-inventory
    if store.engine != StoreEngine::None && boundary.member_account.is_none() {
        diag!(
            "boundary unverified: no {} account for the state member to run as. \
             Run deploy/create-agent.sh, or useradd --system --no-create-home \
             --user-group it.",
            member_identity_for(name)
        );
        return Err(LifecycleRefusal::BoundaryUnverified);
    }
    // **And its primary group is the state group, and not root's** (#99
    // area 2, K1): the territory is judged against the state group by name
    // and the member passes it by its primary group, so a box where the two
    // differ would validate and fail every load with nothing naming why.
    if store.engine != StoreEngine::None
        && let Some(account) = boundary.member_account
        && (boundary.member_group != Some(account.gid) || account.gid == 0)
    {
        diag!(
            "boundary unverified: the state member's primary group (gid {}) is not the \
             group {} resolves to ({:?}), or is gid 0",
            account.gid,
            member_identity_for(name),
            boundary.member_group
        );
        return Err(LifecycleRefusal::BoundaryUnverified);
    }

    // **A restore names a save point, and the selection is the load's**, per
    // `weaver-admin-Spec` section 4 as of A3.2: this walk parses the name and
    // judges nothing of it; `main`'s load selects the save point through the
    // manifest, judges its bytes and resolves the lineage, so the inventory
    // carries none here and the load fills it.
    let lineage = None;

    // **The access rule is checked against the mode that will carry it**, per
    // `weaver-admin-Spec` section 6 and the operator's ruling of 2026-08-28.
    //
    // **Last of the walks**, so it preempts none of them: an artifact or a
    // boundary that refuses is the older and narrower fact, and a check
    // running ahead of them made three inventory tests depend on whether the
    // box happened to carry the agent's group.
    //
    // The gate's socket is `0770` owned by the agent's group, per
    // `weaver-gate-Spec` section 3, so reaching it takes membership in that
    // group. A rule admitting a uid outside it names a peer the filesystem
    // turns away at `connect(2)` before the credential check ever runs: the
    // dialer sees `Permission denied`, the driver reports the socket never
    // stood, and the gate logs nothing because the peer never reached
    // `accept`. That diagnosis cost two runs on 2026-08-27 while it was an
    // unprovisioned box, and the mode makes it the designed behaviour unless
    // the two are made to agree.
    //
    // **The two locks may narrow the same set and may not contradict.** So a
    // rule the mode would silently defeat refuses here, named, before any
    // worker starts, rather than at a `connect` no layer reports.
    //
    // **The rule half is the serving binding's and the group half is every
    // binding's.** The start step drops every worker to the group named
    // `{identity}`, so a box carrying the agent user and not its group fails
    // the start whatever the binding is, and gating the whole check on `Serving` let a diagnostic declaration
    // pass validate clean and fail at load. A diagnostic binding is asked
    // with an empty rule, which reaches the group arm and names no peer.
    let empty_rule = weaver_types::AccessRule {
        allowed_uids: Default::default(),
        allowed_gids: Default::default(),
        denied_uids: Default::default(),
    };
    let rule = match &binding {
        EnterBinding::Serving { gate_instruction } => &gate_instruction.access_rule,
        _ => &empty_rule,
    };
    if let Some(unreachable) = match group {
        Some(known) => unreachable_peer_against(&identity, rule, known),
        None => unreachable_peer(&identity, rule),
    } {
        // **`BoundaryUnverified` and not `ConfigInvalid`.** The declaration is
        // well formed and the fault is the box's: the operator wrote a uid
        // that ought to reach the socket and the provisioning has not put it
        // in the agent's group. `ConfigInvalid` names the TOML, and a
        // deployer following that reading deletes the uid from
        // `allowed-uids` - which makes validate pass and breaks the connector
        // for good, the credential check then denying it at `accept` with
        // nothing saying why. This is the same fault an unprovisioned home or
        // sink is, and it answers as they do.
        //
        // **The remedy comes from the case and not from this call site.** The
        // two refusals want different instructions, and a single sentence
        // here told a deployer whose group was missing to run `gpasswd`,
        // which is the command that fails on exactly that box.
        diag!(
            "boundary unverified: {}: {}",
            unreachable.field(&identity),
            unreachable.remedy(&identity)
        );
        return Err(LifecycleRefusal::BoundaryUnverified);
    }

    Ok(Inventory {
        config,
        identity,
        declaration: declaration_digest(source),
        binding,
        lineage,
        // **The member is the election's, not the box's** (the #94 survey's
        // S2): a box carries the member's account whatever the engine, and
        // every consumer reads this as "a member stands", so a `none` agent
        // answers none and never waits on, restores to or publishes for a
        // member it does not stand.
        member_account: (store.engine != StoreEngine::None)
            .then_some(boundary.member_account)
            .flatten(),
    })
}

/// A file's digest, sha256 of its bytes, hex, and the empty string where the
/// file does not read: a digest that cannot be computed reports that it
/// could not rather than a wrong value, the sentinel `weaver-spu-Spec`
/// section 3 keeps for the weights hash, kept here for the stack.
pub fn file_digest(path: &Path) -> String {
    use sha2::Digest;
    let Ok(bytes) = std::fs::read(path) else {
        return String::new();
    };
    let digest = sha2::Sha256::digest(&bytes);
    let mut hex = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write;
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

/// **A command that runs `program` as exactly `uid` with exactly `gids`**, the
/// first of them primary. The identity is taken in the pre-exec by
/// [`drop_to`] and nowhere else: `CommandExt::uid` and `CommandExt::gid` are
/// deliberately not used, because the standard library applies them before
/// it runs the pre-exec, so a `setgroups` placed there runs after root is
/// gone and fails `EPERM`. Measured on 2026-09-24, issue #675: the store
/// probe built that way could not be spawned as any member, and every
/// agent electing postgres refused `BoundaryUnverified`.
///
/// An empty `gids` is refused before any spawn: an identity with no primary
/// group is not one this crate can hand a process.
// #118: a test-only hook, kept beside the production code it observes.
#[cfg(test)]
pub fn identity_command(
    program: &Path,
    uid: u32,
    gids: &[u32],
) -> std::io::Result<std::process::Command> {
    use std::os::unix::process::CommandExt;
    if gids.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "an identity needs at least its primary group",
        ));
    }
    let set: Vec<nix::libc::gid_t> = gids.iter().map(|g| *g as nix::libc::gid_t).collect();
    let mut command = std::process::Command::new(program);
    // SAFETY: `drop_to` is three async-signal-safe syscalls and no
    // allocation. The group set is built above, before the fork, and only
    // read in the child.
    unsafe {
        command.pre_exec(move || drop_to(uid, &set));
    }
    Ok(command)
}

/// **The privilege drop, in the one order that works**: the supplementary set,
/// then the gids, then the uids, each of the three ids set so no saved id
/// survives to return to. `setgroups` and `setresgid` need the privilege
/// `setresuid` gives away, so the uid goes last. Called in a pre-exec while
/// the fork still holds root, by the member's spawn, and in the tests through
/// `identity_command`, the harness that measures it.
///
/// Async-signal-safe throughout, per the pre-exec contract. A failure returns
/// the error, which fails the spawn, so a process this crate could not
/// unprivilege does not run at all.
pub fn drop_to(uid: u32, gids: &[nix::libc::gid_t]) -> std::io::Result<()> {
    let Some(&primary) = gids.first() else {
        return Err(std::io::Error::from_raw_os_error(nix::libc::EINVAL));
    };
    if unsafe { nix::libc::setgroups(gids.len(), gids.as_ptr()) } < 0 {
        return Err(std::io::Error::last_os_error());
    }
    if unsafe { nix::libc::setresgid(primary, primary, primary) } < 0 {
        return Err(std::io::Error::last_os_error());
    }
    let user = uid as nix::libc::uid_t;
    if unsafe { nix::libc::setresuid(user, user, user) } < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

/// The declaration's digest, sha256 of the file's bytes as read, hex.
pub fn declaration_digest(source: &str) -> String {
    use sha2::Digest;
    let digest = sha2::Sha256::digest(source.as_bytes());
    let mut hex = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write;
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

/// The first peer an access rule admits that the socket's mode will turn
/// away, named by its field, or `None` where the two agree.
///
/// **The mode and the rule are two locks on one door and may not
/// contradict.** The gate binds at `0770` owned by the agent's group, so a
/// peer reaches `accept` only through that group. A rule admitting a uid
/// outside it is a rule that does nothing, and does nothing invisibly: the
/// refusal lands at `connect(2)` where the gate cannot see it and no layer
/// above reports it as what it is.
///
/// **Only `allowed_uids` is judged.** A gid names no particular peer, and
/// whether one holding it reaches the socket turns on that peer's own
/// memberships rather than on the gid, so a rule admitting a gid is left to
/// the credential check. Uid 0 is skipped for a different reason: root
/// reaches a `0770` socket whatever group it holds.
///
/// **Unresolvable is not the same as unreachable.** Where this cannot read
/// the group at all it answers `None` rather than refusing a declaration on
/// a fact it could not establish, the load then failing later and loudly
/// rather than here and wrongly.
///
/// conforms: admin-access-rule-reaches-the-socket
fn unreachable_peer(identity: &str, rule: &weaver_types::AccessRule) -> Option<Unreachable> {
    unreachable_peer_against(identity, rule, &resolved_group(identity))
}

/// Why a declaration's rule cannot be met by the socket it will meet.
///
/// **Two cases and not one string.** The field a refusal names and the
/// instruction an operator needs differ between them, and an earlier form
/// returned one sentence for both - which `take_inventory` then spliced into
/// a field-path slot, telling a deployer to run `gpasswd` for a missing
/// group, where `gpasswd` is what fails when the group is what is absent.
#[derive(Debug, PartialEq, Eq)]
enum Unreachable {
    /// The rule admits a uid outside the agent's group, so it is turned away
    /// at `connect` before the credential check.
    UidOutsideGroup(u32),
    /// The agent user exists and its group does not, so the unit's `Group=`
    /// cannot resolve and the start fails opaquely.
    GroupMissing,
}

impl Unreachable {
    /// The field a refusal names, where one applies.
    fn field(&self, identity: &str) -> String {
        match self {
            Unreachable::UidOutsideGroup(uid) => {
                format!("gate-instruction.access-rule.allowed-uids.{uid}")
            }
            Unreachable::GroupMissing => format!("the {identity} group"),
        }
    }

    /// What the operator does about it. The two remedies are different and
    /// naming the wrong one costs a deployer the diagnosis.
    fn remedy(&self, identity: &str) -> String {
        match self {
            Unreachable::UidOutsideGroup(uid) => format!(
                "uid {uid} is outside the {identity} group, which the gate's \
                 socket mode admits by. Run `gpasswd -a <user> {identity}` \
                 rather than deleting the uid from the rule, which would make \
                 validate pass and leave the credential check denying it."
            ),
            Unreachable::GroupMissing => format!(
                "the {identity} user exists and the {identity} group does \
                 not, so the unit's Group= cannot resolve and the start \
                 fails. Run `groupadd {identity} && usermod -g {identity} \
                 {identity}`."
            ),
        }
    }
}

/// What the host says about an agent's group: its gid and members, or which
/// of the two absences it is.
enum ResolvedGroup {
    /// The group exists, with this gid and these member names.
    Present { gid: u32, members: Vec<String> },
    /// Neither the group nor the user: no agent is provisioned here.
    NoAgent,
    /// The user exists and the group does not, which fails the unit start.
    UserWithoutGroup,
    /// The lookup itself failed, so nothing about the group is known.
    Unresolvable,
}

fn resolved_group(identity: &str) -> ResolvedGroup {
    match nix::unistd::Group::from_name(identity) {
        Ok(Some(group)) => ResolvedGroup::Present {
            gid: group.gid.as_raw(),
            members: group.mem,
        },
        // **A lookup that failed is not a group that is absent.** A transient
        // `getgrnam_r` error would otherwise read as `UserWithoutGroup` on a
        // box where the user resolves, and refuse a load on a fact never
        // established - which is the rule the rest of this function follows.
        Err(_) => ResolvedGroup::Unresolvable,
        Ok(None) => {
            // **A missing group is a fault only where the agent user
            // exists.** `Group={identity}` is a hard start-time requirement,
            // so a box that provisioned the user and not the group fails the
            // unit start with an opaque credential error. That state is
            // nameable and is named. A box carrying neither - a bare
            // checkout, or CI - has provisioned no agent at all and is not
            // refused on a fact about an agent it does not have.
            match nix::unistd::User::from_name(identity) {
                Ok(Some(_)) => ResolvedGroup::UserWithoutGroup,
                _ => ResolvedGroup::NoAgent,
            }
        }
    }
}

/// The judgment, against a group already resolved.
///
/// **Separated from the lookup so the watch can run anywhere.** Reading the
/// host's group table made the test that guards this vacuous on both a
/// development box, where the operator is in every agent group, and on CI,
/// where no agent group exists - so the record claimed a watch that ran in
/// neither place.
///
/// conforms: admin-access-rule-reaches-the-socket
fn unreachable_peer_against(
    _identity: &str,
    rule: &weaver_types::AccessRule,
    group: &ResolvedGroup,
) -> Option<Unreachable> {
    let (gid, members) = match group {
        ResolvedGroup::Present { gid, members } => (*gid, members),
        ResolvedGroup::NoAgent | ResolvedGroup::Unresolvable => return None,
        ResolvedGroup::UserWithoutGroup => return Some(Unreachable::GroupMissing),
    };
    let members: std::collections::BTreeSet<&String> = members.iter().collect();

    for uid in &rule.allowed_uids {
        // **Root is not bound by the mode.** `CAP_DAC_OVERRIDE` reaches a
        // `0770` socket whatever group it holds, so refusing `allowed-uids:
        // [0]` would refuse a rule that works, on a premise false for that
        // one uid.
        if *uid == 0 {
            continue;
        }
        // **A uid the rule denies is not a peer this asks about.**
        // `weaver_types::authorized` gives `denied_uids` precedence over
        // `allowed_uids`, so a uid in both is refused at `accept` whatever
        // the mode does, and naming it unreachable would refuse a
        // declaration over a peer that was never going to be admitted.
        if rule.denied_uids.contains(uid) {
            continue;
        }
        let user = match nix::unistd::User::from_uid(nix::unistd::Uid::from_raw(*uid)) {
            Ok(Some(user)) => user,
            // **A uid with no passwd entry is unreachable and not exempt.**
            // Group membership is recorded by name, so a uid carrying no
            // name is in no group and cannot hold the agent's: it reaches a
            // `0770` socket it does not own by no route. Continuing here was
            // an undeclared third exemption beside `allowed-gids` and uid 0,
            // and it passed exactly the declaration this check exists to
            // catch - the dialer turned away at `connect(2)` while the
            // driver reports the socket never stood.
            Ok(None) => return Some(Unreachable::UidOutsideGroup(*uid)),
            // A lookup that failed establishes nothing, which is the rule
            // the group resolution follows too.
            Err(_) => continue,
        };
        // The agent's own group reached either as a primary or a secondary.
        if user.gid.as_raw() == gid || members.contains(&user.name) {
            continue;
        }
        return Some(Unreachable::UidOutsideGroup(*uid));
    }
    // **`allowed_gids` is not judged here, and the omission is the finding.**
    // A gid names no particular peer, and whether a peer holding it reaches
    // the socket turns on that peer's own memberships: a rule admitting gid
    // 1000 works where the operator holding it is a supplementary member of
    // the agent's group, which is the shape one of these boxes has. An arm
    // refusing every gid but the agent's own refused that working
    // declaration. Deciding it properly means enumerating who holds the gid,
    // which this crate has no reason to do, so the rule's gid half rests on
    // the credential check alone and this says so rather than guessing.
    None
}

pub(crate) fn sink_directory(sink: &TraceSink) -> &Path {
    let path = match sink {
        TraceSink::File { path, .. }
        | TraceSink::Pipe { path, .. }
        | TraceSink::Socket { path } => path,
    };
    path.parent().unwrap_or(Path::new("/"))
}

fn sink_present_or_creatable(sink: &TraceSink) -> bool {
    match sink {
        TraceSink::File { path, create } | TraceSink::Pipe { path, create } => {
            *create || path.exists()
        }
        // No creation flag exists for a socket sink: something of the
        // operator's must already be listening.
        TraceSink::Socket { path } => path.exists(),
    }
}

/// Whether the sink directory is held by a principal whose custody the charter
/// recognizes: root, or the admin principal itself. Any other owner controls
/// the file admin opened, whatever the mode currently reads.
/// **This look is by path, and the sink's open is not**: a file or pipe sink
/// opens beneath the territory's judged descriptor (`sink::open`, #140), so
/// the directory judged at the open is the one held, whatever this look read.
pub fn admin_holds_custody(directory: &Path, boundary: &Boundary) -> bool {
    use std::os::unix::fs::MetadataExt;
    let Ok(meta) = std::fs::metadata(directory) else {
        // A directory this crate cannot resolve establishes no custody, the
        // same reading the traversal half takes for the same reason.
        return false;
    };
    meta.uid() == 0 || meta.uid() == boundary.admin_uid
}

/// Whether the agent uid could traverse into the directory: it is the owner,
/// holds a group search bit through some membership, or the other bits carry
/// search. Any of those is a boundary the operator has not drawn.
pub fn agent_can_traverse(directory: &Path, boundary: &Boundary) -> bool {
    use std::os::unix::fs::MetadataExt;
    // **A path this crate cannot resolve yields no boundary evidence**, so it
    // is treated as traversable rather than as safe. A relative sink path's
    // parent is the empty path, whose metadata lookup fails, and reading that
    // failure as "not traversable" would admit a configuration nothing
    // verified.
    if !directory.is_absolute() {
        return true;
    }
    let Ok(meta) = std::fs::metadata(directory) else {
        return true;
    };
    let mode = meta.mode();
    // **An agent that owns the directory can restore the search bit itself**,
    // so ownership defeats the boundary whatever the mode currently reads.
    if meta.uid() == boundary.agent_uid {
        return true;
    }
    if boundary.agent_gids.contains(&meta.gid()) && mode & 0o010 != 0 {
        return true;
    }
    mode & 0o001 != 0
}

#[cfg(test)]
mod tests;
