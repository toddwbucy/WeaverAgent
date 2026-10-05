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
    // `weaver-types-PRD` section 2.1, so a declaration written before the
    // member existed resolves as it always meant.
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
    // rather than surplus, and `database` and `role` belong to the service
    // engine exactly, absent under every other engine and present under it.
    let store = config.state_store.clone().unwrap_or_default();
    let store_invalid = |field: &str| LifecycleRefusal::ConfigInvalid {
        field: Some(FieldName(field.into())),
    };
    match store.engine {
        StoreEngine::None if config.state_election.is_some() => {
            return Err(store_invalid("state-election"));
        }
        StoreEngine::None | StoreEngine::Sqlite => {
            if store.database.is_some() {
                return Err(store_invalid("state-store.database"));
            }
            if store.role.is_some() {
                return Err(store_invalid("state-store.role"));
            }
        }
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
    // custody, so both are checked.
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

    // **A restore is judged here too**, per `weaver-admin-Spec` section 4 as
    // of 2026-09-04 and issue #432. The record is read under this crate's
    // own custody and never handed on: a record that cannot be read refuses
    // `BoundaryUnverified`, a cut the record does not hold refuses
    // `ConfigInvalid` naming `restore.through`, and the session name decides
    // resume from branch, a cut under the record's own name refusing because
    // a session cannot rewind under its own name while its record carries
    // the turns the cut would drop.
    let lineage = match config.restore.as_ref() {
        None => None,
        Some(restore) => Some(judge_restore(restore, &config.session)?),
    };

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
        member_account: boundary.member_account,
    })
}

/// The restore's judgment, per `weaver-admin-Spec` section 4 as of
/// 2026-09-04: read the record, find what it holds, and resolve the lineage
/// the enter carries.
///
/// **The record is the one fact that can say whether the cut exists**, so
/// the walk reads its envelopes and nothing else: the session every event
/// carries, the runs in landing order, and each run's last turn number. A
/// turn key spells `t-<n>` per `weaver-trace-Spec` section 2, and a cut names
/// the turn by that number beside its run.
fn judge_restore(
    restore: &weaver_types::Restore,
    session: &weaver_types::SessionId,
) -> Result<weaver_types::Lineage, LifecycleRefusal> {
    let text = std::fs::read_to_string(&restore.record).map_err(|error| {
        diag!(
            "boundary unverified: the restore's record {} does not read: {error}",
            restore.record.display()
        );
        LifecycleRefusal::BoundaryUnverified
    })?;
    let held = RecordHoldings::read(&text).ok_or_else(|| {
        diag!(
            "boundary unverified: the restore's record {} holds no event",
            restore.record.display()
        );
        LifecycleRefusal::BoundaryUnverified
    })?;
    let through_refusal = || LifecycleRefusal::ConfigInvalid {
        field: Some(FieldName("restore.through".into())),
    };
    // **The session name decides what the restore is.** The declaration's
    // own name with the record whole is a resume, and a cut under it refuses.
    if session.0 == held.session {
        if restore.through.is_some() {
            diag!("config invalid: a session cannot rewind under its own name");
            return Err(through_refusal());
        }
        return Ok(held.whole());
    }
    // A new name is a branch, at the cut where one is named and at the
    // record's end where none is.
    match restore.through.as_ref() {
        None => Ok(held.whole()),
        Some(cut) => {
            let turns = held
                .runs
                .iter()
                .find(|(run, _)| *run == cut.run.0)
                .map(|(_, turns)| turns)
                .ok_or_else(|| {
                    diag!("config invalid: the record holds no run {:?}", cut.run.0);
                    through_refusal()
                })?;
            // Membership and never a bound: a run holding turns one and
            // three holds no turn two, and a cut there names nothing.
            if !turns.contains(&cut.turn) {
                diag!(
                    "config invalid: run {:?} holds no turn {}",
                    cut.run.0,
                    cut.turn
                );
                return Err(through_refusal());
            }
            Ok(weaver_types::Lineage {
                parent: weaver_types::SessionId(held.session),
                run: cut.run.clone(),
                through: cut.turn,
            })
        }
    }
}

/// What a record holds that a cut is judged against: its session, and its
/// runs in landing order with the turn numbers each run holds.
struct RecordHoldings {
    session: String,
    runs: Vec<(String, std::collections::BTreeSet<u64>)>,
}

impl RecordHoldings {
    /// **The record's session is the first event's, and a line of another
    /// session is not the record's**: a trace holds one session by
    /// `weaver-trace-PRD` section 2, so a foreign line is skipped rather
    /// than read as a run this session holds.
    fn read(text: &str) -> Option<RecordHoldings> {
        let mut session: Option<String> = None;
        let mut runs: Vec<(String, std::collections::BTreeSet<u64>)> = Vec::new();
        for line in text.lines() {
            let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            let Some(run) = value.get("run").and_then(|r| r.as_str()) else {
                continue;
            };
            let line_session = value.get("session").and_then(|s| s.as_str());
            match (&session, line_session) {
                (None, Some(found)) => session = Some(found.to_string()),
                (Some(held), Some(found)) if held != found => continue,
                (Some(_), None) | (None, None) => continue,
                _ => {}
            }
            let turn = value
                .get("turn")
                .and_then(|t| t.as_str())
                .and_then(|t| t.strip_prefix("t-"))
                .and_then(|n| n.parse::<u64>().ok());
            let turns = match runs.iter_mut().find(|(held, _)| held == run) {
                Some((_, turns)) => turns,
                None => {
                    runs.push((run.to_string(), std::collections::BTreeSet::new()));
                    &mut runs.last_mut().expect("just pushed").1
                }
            };
            if let Some(turn) = turn {
                turns.insert(turn);
            }
        }
        Some(RecordHoldings {
            session: session?,
            runs,
        })
    }

    /// The whole record resolved to its last run's last turn, zero where
    /// that run holds no turn.
    fn whole(&self) -> weaver_types::Lineage {
        let (run, through) = self
            .runs
            .last()
            .map(|(run, turns)| (run.clone(), turns.iter().next_back().copied().unwrap_or(0)))
            .unwrap_or_else(|| (String::new(), 0));
        weaver_types::Lineage {
            parent: weaver_types::SessionId(self.session.clone()),
            run: weaver_types::RunId(run),
            through,
        }
    }
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
/// **The window between this check and the sink's open is not closed here, and
/// the reason is the trust model rather than an oversight.** The path is
/// resolved twice, once for this validation and once when the sink is opened,
/// so a party able to write in the directory could replace the target or
/// redirect it by symlink between them. Every such party is root or the admin
/// principal, because the check below is what refuses any other owner, and the
/// charter secures the agent against reaching its own record while securing
/// nothing against the operator, who is trusted by construction. Descriptor
/// relative resolution would close the window against an untrusted writer that
/// this check has already excluded.
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
mod tests {
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
                "identity = []\n",
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
    /// naming `state-election`, `database` and `role` belong to the service
    /// engine exactly, and each refusal is `ConfigInvalid` naming the field.
    /// Perturbation: remove any one arm of the match and its case below
    /// passes the inventory, each case naming the arm that catches it.
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
            (
                "engine = \"none\"\ndatabase = \"d\"\n",
                "state-store.database",
            ),
            ("engine = \"sqlite\"\nrole = \"r\"\n", "state-store.role"),
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

    /// **The restore is judged here, and the session name decides what it
    /// is**, per `weaver-admin-Spec` section 4 as of 2026-09-04. The record's
    /// own session name with the record whole is a resume resolved to the
    /// last run's last turn, a cut under that name refuses, a new name is a
    /// branch at the cut or at the end, a cut the record does not hold
    /// refuses naming the field, and a record that does not read refuses
    /// the boundary.
    ///
    /// Perturbation: drop the run check and the absent-run case resolves to
    /// a lineage, or drop the session rule and a rewind under the record's
    /// own name resolves. Watched under both.
    #[test]
    fn the_restore_is_judged_at_the_inventory() {
        let root = scratch("restore");
        let sink_dir = root.join("sink");
        std::fs::create_dir_all(&sink_dir).expect("sink dir");
        std::fs::set_permissions(&sink_dir, std::fs::Permissions::from_mode(0o750)).expect("mode");
        let home = root.join("home");
        std::fs::create_dir_all(&home).expect("home");
        let record = root.join("s-1.ndjson");
        std::fs::write(
            &record,
            concat!(
                "{\"session\":\"s-1\",\"run\":\"r-a\",\"sequence\":\"0\",\"kind\":\"load\"}\n",
                "{\"session\":\"s-1\",\"run\":\"r-a\",\"turn\":\"t-1\",\"sequence\":\"1\",\"kind\":\"turn.opened\"}\n",
                "{\"session\":\"s-1\",\"run\":\"r-a\",\"turn\":\"t-2\",\"sequence\":\"2\",\"kind\":\"turn.opened\"}\n",
                "{\"session\":\"s-1\",\"run\":\"r-b\",\"sequence\":\"3\",\"kind\":\"load\"}\n",
                "{\"session\":\"s-1\",\"run\":\"r-b\",\"turn\":\"t-1\",\"sequence\":\"4\",\"kind\":\"turn.opened\"}\n",
                "{\"session\":\"s-1\",\"run\":\"r-b\",\"turn\":\"t-3\",\"sequence\":\"5\",\"kind\":\"turn.closed\"}\n",
                // A line of another session is not this record's: a run it
                // names is not held, and a turn it names is not either.
                "{\"session\":\"s-other\",\"run\":\"r-x\",\"turn\":\"t-9\",\"sequence\":\"6\",\"kind\":\"turn.closed\"}\n",
            ),
        )
        .expect("the record writes");
        let name = AgentName("alpha".into());
        let bound = boundary(&home, 65533);
        let restore = |cut: &str| format!("\n[restore]\nrecord = \"{}\"\n{cut}", record.display());

        // A resume: the record's own session, whole, resolves to r-b's turn 3.
        let source = format!("{}{}", config_source(&sink_dir), restore(""));
        let taken = take_inventory(&name, &source, &bound).expect("a resume admits");
        let lineage = taken.lineage.expect("a resume carries its lineage");
        assert_eq!(lineage.parent.0, "s-1");
        assert_eq!(lineage.run.0, "r-b");
        assert_eq!(lineage.through, 3, "the last run's last turn");

        // A cut under the record's own name refuses: no rewind under one name.
        let source = format!(
            "{}{}",
            config_source(&sink_dir),
            restore("through = { run = \"r-b\", turn = 1 }\n")
        );
        let refused = take_inventory(&name, &source, &bound);
        assert!(
            matches!(refused, Err(LifecycleRefusal::ConfigInvalid { field: Some(ref f) }) if f.0 == "restore.through"),
            "a rewind under the record's own name refuses, got {refused:?}"
        );

        // A branch: a new session name at a named cut.
        let branched = config_source(&sink_dir).replace("session = \"s-1\"", "session = \"s-2\"");
        let source = format!(
            "{branched}{}",
            restore("through = { run = \"r-a\", turn = 2 }\n")
        );
        let taken = take_inventory(&name, &source, &bound).expect("a branch admits");
        let lineage = taken.lineage.expect("a branch carries its lineage");
        assert_eq!(
            (
                lineage.parent.0.as_str(),
                lineage.run.0.as_str(),
                lineage.through
            ),
            ("s-1", "r-a", 2)
        );

        // A cut the record does not hold refuses naming the field: a run it
        // lacks, a turn past the run's last, a turn the run skips where it
        // holds one and three, and a run a foreign session's line named.
        for cut in [
            "through = { run = \"r-zz\", turn = 1 }\n",
            "through = { run = \"r-a\", turn = 9 }\n",
            "through = { run = \"r-b\", turn = 2 }\n",
            "through = { run = \"r-x\", turn = 9 }\n",
        ] {
            let source = format!("{branched}{}", restore(cut));
            let refused = take_inventory(&name, &source, &bound);
            assert!(
                matches!(refused, Err(LifecycleRefusal::ConfigInvalid { field: Some(ref f) }) if f.0 == "restore.through"),
                "an absent cut refuses naming the field, got {refused:?}"
            );
        }

        // A record that does not read refuses the boundary.
        let source = format!(
            "{branched}\n[restore]\nrecord = \"{}\"\n",
            root.join("no-such-record.ndjson").display()
        );
        let refused = take_inventory(&name, &source, &bound);
        assert!(
            matches!(refused, Err(LifecycleRefusal::BoundaryUnverified)),
            "got {refused:?}"
        );
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
}
