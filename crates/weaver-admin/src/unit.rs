//! conforms: admin-init-system-over-command-line
//! conforms: admin-unit-declares-no-open
//!
//! The unit, per `weaver-admin-Spec` section 6: the init system is asked over
//! its command-line interface, one invocation per load with the unit's
//! properties declared on the invocation, one invocation of the stop verb to
//! stop it, and the same interface answers the residency query of section 3.
//!
//! The alternative is a bus library, and it loses on the tree: `zbus` carries
//! async machinery into the resolved tree whatever its surface API, which this
//! crate's own manifest assertion forbids, and `dbus-rs` trades that for a C
//! library dependency in a binary that otherwise links none. What the command
//! line costs instead is failure discrimination, named at
//! `weaver-admin-systemd-contract` section 3 and not defended here.

use std::process::Command;

/// The template's fixed part and the one variable. The template lives in
/// admin's own service configuration, and **the only value interpolated is the
/// validated agent name**, so a name reaching a unit has one origin.
#[derive(Debug, Clone)]
pub struct UnitTemplate {
    /// The run tool the operator's init system provides.
    pub run_tool: String,
    /// The control tool that answers the residency query and stops a unit.
    pub control_tool: String,
    /// The properties the fixed template carries beyond `User=`. Which ones is
    /// the operator's policy surface, deliberately not enumerated by the Spec,
    /// because a hardening list frozen in a Spec is a posture that cannot
    /// track its host.
    pub properties: Vec<String>,
    /// The worker binary the unit starts.
    pub worker: std::path::PathBuf,
    /// The SPU binary the worker forks at enter, and the gate binary beside
    /// it. **Operator-installed values rather than anything this invocation
    /// composed**, per `weaver-admin-Spec` section 9, which is why they sit
    /// here and not in the agent's declaration. The gate's is one
    /// installation's fact. The SPU's here is `spu-binary`, the installation's
    /// default, and a load replaces it with the agent's own where section 9's
    /// map names the agent. They reach the worker in the argument vector
    /// because a process that does not yet exist has no other way to learn
    /// them.
    pub spu: std::path::PathBuf,
    pub gate: std::path::PathBuf,
    /// The admission headroom this installation wants its organs to run with,
    /// if it states one.
    ///
    /// **Optional where the binaries are required, and the difference is what
    /// each is.** A worker cannot start without knowing which binaries to fork,
    /// so a missing one refuses. A headroom is a number the organ already has a
    /// compiled default for, so a missing one leaves that default standing and
    /// an installation that never thought about it behaves as it always did.
    ///
    /// Carried as the operator's string rather than parsed here. This crate
    /// hands it on and the organ's composition root judges it, so a bad value
    /// is refused once, by the crate that knows what the parameter means.
    pub headroom_bytes: Option<String>,
}

/// What the init system answers about a unit, per
/// `weaver-admin-systemd-contract` section 3.
///
/// **The init system answers three values and this enum carries four.**
/// `Active`, `Failed`, and `Inactive` are the manager's own, carried under its
/// own names because a translation is where the invention would enter.
/// `Unknown` is this crate's and never the manager's: it is what an ask that
/// itself failed yields, kept distinct so a query that could not run is not
/// read as an answer that came back.
///
/// **The three separate less than they look.** `Active` and `Failed` each mean
/// one thing, a running unit and one whose process exited non-zero. `Inactive`
/// means at least three: a unit that stopped cleanly, one that never existed,
/// and one whose exec never succeeded. Measured against a live manager on
/// 2026-08-05. A party reading this learns that a worker is not running and
/// does not learn why.
///
/// **This is residency and not lifecycle state.** Constructing an `AgentState`
/// from it would be inventing a fact, per Spec section 3.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Residency {
    Active,
    Failed,
    Inactive,
    /// The ask itself failed. Reported as unknown rather than guessed at,
    /// because a state this boundary could not answer is not a state the
    /// program may invent.
    Unknown,
}

impl Residency {
    /// The manager's own name for the value, carried rather than translated,
    /// because a translation is where the invention would enter.
    pub fn as_str(self) -> &'static str {
        match self {
            Residency::Active => "active",
            Residency::Failed => "failed",
            Residency::Inactive => "inactive",
            Residency::Unknown => "unknown",
        }
    }
}

/// The per-agent unit name, from the validated agent name and nothing else.
pub fn unit_name(agent: &str) -> String {
    format!("weaver-worker@{agent}.service")
}

/// The runtime directory the coordination socket is bound inside, named
/// relative to the manager's runtime root because the manager creates and
/// removes it.
pub fn runtime_directory_name(agent: &str) -> String {
    format!("weaver-{agent}")
}

/// Starts the worker's transient unit.
///
/// **The unit declares no descriptor-bearing open.** The worker starts bare
/// and binds its own coordination socket inside the runtime directory asked
/// for here, per `weaver-harness-Spec` section 2.3, so nothing is placed into
/// the unit at start and no descriptor crosses the init system at all. The
/// sink's descriptor crosses later and elsewhere, inside the enter directive
/// over the connection admin dialed.
///
/// **Bare states what no descriptor crosses and says nothing about
/// arguments**, per `weaver-admin-Spec` section 6. A descriptor is a
/// capability the manager would have to hold and pass, and an argument is a
/// value the worker reads and then resolves under its own identity, so the
/// socket path this carries grants nothing the agent uid did not already
/// have.
///
/// **The runtime directory is asked for because its removal is the answer to a
/// stale socket.** A Unix socket's pathname outlives the process that bound
/// it, and the program does not solve that by unlinking, which races a live
/// successor. The manager creates the directory at start and destroys it with
/// the unit, so the pathname cannot outlive the worker.
pub fn start(
    unit: &AgentUnit,
    identity: &str,
    coordination_socket: &std::path::Path,
    loop_file: Option<&std::path::Path>,
) -> std::io::Result<std::process::ExitStatus> {
    Command::new(&unit.template.run_tool)
        .args(unit.arguments(identity, coordination_socket, loop_file))
        .status()
}

/// **The unit one agent's load starts, and the only thing `start` takes**, per
/// `weaver-admin-Spec` sections 6 and 9. It pairs the installation's template,
/// its SPU replaced by the one section 9's map chose for the agent, with that
/// agent's name, and its one constructor is `for_agent`, which applies the map.
/// So the installation's own template, whose SPU is only the default, has no
/// route to a start, and a template chosen for one agent cannot be started under
/// another's name. A type property, and the compiler holds it: `start` taking a
/// `UnitTemplate` does not type-check, and the field is private to this module.
#[derive(Debug, Clone)]
pub struct AgentUnit {
    template: UnitTemplate,
    agent: String,
}

impl AgentUnit {
    /// The agent's unit: the installation's template with the agent's SPU.
    pub fn for_agent(
        template: &UnitTemplate,
        choice: &crate::spu_choice::SpuChoice,
        agent: &str,
    ) -> Self {
        let mut template = template.clone();
        template.spu = choice.for_agent(agent, &template.spu).path;
        AgentUnit {
            template,
            agent: agent.to_string(),
        }
    }

    /// The argument vector `start` runs for this agent.
    pub(crate) fn arguments(
        &self,
        identity: &str,
        coordination_socket: &std::path::Path,
        loop_file: Option<&std::path::Path>,
    ) -> Vec<String> {
        start_arguments(
            &self.template,
            identity,
            &self.agent,
            coordination_socket,
            loop_file,
        )
    }
}

/// The argument vector `start` runs, built here so nothing reads a copy of it.
/// The test that asserts no descriptor-bearing property is declared reads this
/// function, and a builder who added such a property to `start` would have to
/// add it here, which is what keeps that test from passing over a drifted
/// invocation.
///
/// **The worker's own arguments follow the binary and take no value the
/// invocation's own input composes**, per `weaver-admin-Spec` section 6. The
/// socket path is derived by the caller from the validated agent name, the
/// two binaries are the operator's installed values, and the loop file is
/// the vector's one declaration-sourced value, the operator's file validated
/// at inventory, resolved by the worker under the agent's own identity. So
/// the vector reads the allow-listed name and the operator's files and reads
/// nothing else. A builder who let any of these be composed from the
/// invocation's own input would widen the delegated authority by the route
/// the name check closes.
/// Every unit key this crate elects, and therefore every key an installed
/// template may not set.
///
/// **The list is the closure of what the boundary rests on, not the keys one
/// round happened to find.** `unreachable_peer`'s premise is that the
/// socket's group is `weaver-<agent>` and the worker runs as the agent, and
/// `agent_gids` computes the credential set from exactly these. A template
/// moving any of them breaks the premise, and admin then validates a rule
/// against an identity the worker does not hold.
///
/// **Two of them do their damage without looking like an override**, which
/// is why the list is derived rather than enumerated from the keys this
/// crate happens to write. `SupplementaryGroups=` is additive: it grants
/// gids without displacing anything, so `User=` and `Group=` still read
/// correctly while the worker holds a gid the denial walk never computed.
/// And **an empty `RuntimeDirectory=` resets the list rather than setting
/// it**, so a template carrying one leaves systemd creating no runtime
/// directory at all, and the coordination socket has nowhere to bind - a
/// start failure rather than a boundary one, and no less this crate's to
/// refuse. `DynamicUser` is here because it discards the provisioned
/// identity outright.
///
/// conforms: admin-runtime-directory-mode-is-stated
const ELECTED_KEYS: &[&str] = &[
    "User",
    "Group",
    "SupplementaryGroups",
    "DynamicUser",
    "RuntimeDirectory",
    "RuntimeDirectoryMode",
];

fn start_arguments(
    template: &UnitTemplate,
    identity: &str,
    agent: &str,
    coordination_socket: &std::path::Path,
    loop_file: Option<&std::path::Path>,
) -> Vec<String> {
    let mut args = vec![
        "--unit".to_string(),
        format!("weaver-worker@{agent}"),
        format!("--property=User={identity}"),
        // **The group is named beside the user.** `0750` on the runtime
        // directory puts the operator's reach on membership in the agent's
        // group, and without this systemd takes whatever primary group the
        // operator happened to provision the agent user with. Where that is
        // shared - `users`, or `nogroup` - the mode grants traversal to every
        // member of it and the boundary is not the one section 6 describes.
        format!("--property=Group={identity}"),
        format!(
            "--property=RuntimeDirectory={}",
            runtime_directory_name(agent)
        ),
        // **The runtime directory states its mode**, per `weaver-admin-Spec`
        // section 6. systemd's default is `0755`, so without this every uid
        // on the box may traverse to the agent's sockets, and the gate's own
        // mode is then the only thing between a stranger and the front door.
        // `0750` puts the group there too: the operator reaches the socket by
        // membership in the agent's group, which is the provisioning already
        // documented, and no one else reaches it at all.
        //
        // conforms: admin-runtime-directory-mode-is-stated
        "--property=RuntimeDirectoryMode=0750".to_string(),
    ];
    // **The template's own properties are refused where they name a key this
    // crate elects**, rather than emitted and allowed to win.
    //
    // `systemd-run` takes the last assignment, so an installed hardening
    // template carrying one of [`ELECTED_KEYS`] would revert the boundary
    // with no diagnostic - and `unreachable_peer`'s whole premise, that the
    // socket's group is `weaver-<agent>` and the worker runs as the agent,
    // would break with it, so admin would validate a rule against an identity
    // the worker does not hold. `SupplementaryGroups=` does it without
    // displacing anything and an empty `RuntimeDirectory=` resets rather than
    // sets, which is why the list is the closure of what the boundary rests
    // on rather than the keys this crate writes.
    //
    // Emitting these after the template would let the boundary win silently
    // instead, which trades one silent override for another. A template
    // naming either key is a configuration this crate cannot honour and says
    // so.
    //
    // **Says so out loud, because dropping it quietly is the same failure in
    // the other direction.** An operator who installs a hardening template
    // with `RuntimeDirectoryMode=0700` and reads a clean start has no way to
    // learn the boundary on disk and the boundary in force disagree. This
    // function returns a `Vec<String>` and has no error channel to refuse
    // through, so the diagnostic is the refusal an operator gets, and it
    // names the property rather than the key alone so the line in the
    // template is findable.
    //
    // conforms: admin-runtime-directory-mode-is-stated
    for property in &template.properties {
        let names_boundary = property
            .split('=')
            .next()
            .is_some_and(|key| ELECTED_KEYS.contains(&key.trim()));
        if names_boundary {
            eprintln!(
                "template property dropped: {property}: this crate sets \
                 Group= and RuntimeDirectoryMode= itself, the socket's group \
                 being the boundary admin validates access rules against. \
                 Remove it from the unit template rather than expecting it to \
                 apply."
            );
            continue;
        }
        args.push(format!("--property={property}"));
    }
    args.push(template.worker.display().to_string());
    args.push(coordination_socket.display().to_string());
    args.push(template.spu.display().to_string());
    args.push(template.gate.display().to_string());
    // The construction parameters follow the positional arguments as named
    // flags, so an installation supplying one need not know the order.
    if let Some(headroom) = &template.headroom_bytes {
        args.push("--headroom-bytes".to_string());
        args.push(headroom.clone());
    }
    // The declaration's loop file, per `weaver-admin-Spec` section 6: the
    // vector's one declaration-sourced value, absent from the vector where
    // absent from the declaration so the worker's own default stands.
    if let Some(loop_file) = loop_file {
        args.push("--loop-file".to_string());
        args.push(loop_file.display().to_string());
    }
    args
}

/// Stops the unit and waits for it to have stopped.
///
/// **A stop that is accepted is not a stop that has happened.**
/// `weaver-admin-systemd-contract` section 4 promises the ask is answered when
/// the unit has stopped rather than when the stop was accepted, so this call
/// is the confirmation and this crate elects no timer beside it.
pub fn stop(template: &UnitTemplate, agent: &str) -> std::io::Result<std::process::ExitStatus> {
    Command::new(&template.control_tool)
        .arg("stop")
        .arg(unit_name(agent))
        .status()
}

/// Asks the init system what state the unit is in.
///
/// The value is carried and nothing is claimed beyond it. An ask that fails is
/// `Unknown` rather than a guess.
pub fn residency(template: &UnitTemplate, agent: &str) -> Residency {
    let out = Command::new(&template.control_tool)
        .arg("is-active")
        .arg(unit_name(agent))
        .output();
    match out {
        Ok(out) => residency_of(String::from_utf8_lossy(&out.stdout).trim()),
        Err(_) => Residency::Unknown,
    }
}

/// The manager's word for a state, folded into the four this crate carries.
/// Split from the ask so the fold is reachable by a test rather than only by
/// catching a unit mid-transition.
///
/// **`inactive` is the only word that reads as at rest.** The manager names
/// more states than this enum carries, and the fold has a safe direction and
/// an unsafe one: reading a unit that is still moving as at rest is what lets
/// an unload answer over a worker that has not stopped, which is the report
/// `weaver-admin-Spec` section 6 says this crate must never produce. Reading a
/// stopped unit as running costs a refusal the operator retries.
///
/// So the known transitional words fold to `Active`, and `deactivating` folds
/// with `activating` and `reloading` rather than with `inactive`: a unit on its
/// way down has not arrived. `failed` keeps its own case, the manager's own
/// word for a unit whose process exited non-zero. A word this crate does not
/// recognise folds to `Unknown` rather than to `Active`, because an answer
/// nobody can read is not the same fact as a unit known to be running, and
/// both refuse an unload anyway, so the safe direction is kept without
/// claiming a state the manager did not report.
fn residency_of(word: &str) -> Residency {
    match word {
        "active" | "activating" | "reloading" | "deactivating" => Residency::Active,
        "failed" => Residency::Failed,
        "inactive" => Residency::Inactive,
        _ => Residency::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **The start invocation carries no descriptor-bearing property.** The
    /// route that placed the trace's far end on the unit's standard output was
    /// declined, because standard output is inherited across fork and exec and
    /// would hand every organ the harness forks a writable handle to the
    /// agent's own record.
    ///
    /// Perturbation: add a `StandardOutput=` or `ListenStream=` property to
    /// `start` and this test fails. Watched under exactly that addition.
    #[test]
    fn the_start_invocation_declares_no_descriptor() {
        let template = UnitTemplate {
            run_tool: "/bin/true".into(),
            control_tool: "/bin/true".into(),
            properties: vec!["PrivateTmp=yes".into(), "NoNewPrivileges=yes".into()],
            worker: "/usr/libexec/weaver-worker".into(),
            spu: "/usr/libexec/weaver-spu".into(),
            gate: "/usr/libexec/weaver-gate".into(),
            headroom_bytes: None,
        };
        let rendered = start_arguments(
            &template,
            "weaver-alpha",
            "alpha",
            std::path::Path::new("/run/weaver-alpha/coordination.sock"),
            None,
        );
        for banned in [
            "StandardOutput=",
            "StandardError=",
            "StandardInput=",
            "ListenStream=",
            "Sockets=",
            "FileDescriptorName=",
        ] {
            assert!(
                !rendered.iter().any(|a| a.contains(banned)),
                "the start invocation carries {banned}, which places a descriptor: {rendered:?}"
            );
        }
        assert!(
            rendered.iter().any(|a| a.contains("RuntimeDirectory=")),
            "the runtime directory is asked for: {rendered:?}"
        );
        // **And its mode is stated rather than left to systemd's 0755**, per
        // section 6. At the default every uid on the box may traverse to the
        // agent's sockets, leaving the gate's own mode as the only thing
        // between a stranger and the front door.
        //
        // Perturbation: drop the `RuntimeDirectoryMode` property and this
        // fails, the directory then arriving world-traversable. Watched
        // under exactly that removal.
        //
        // conforms: admin-runtime-directory-mode-is-stated
        assert!(
            rendered
                .iter()
                .any(|a| a == "--property=RuntimeDirectoryMode=0750"),
            "the runtime directory's mode is the unit's election: {rendered:?}"
        );
        // **And the group the mode rests on.** `0750` puts the operator's
        // reach on membership in the agent's group, and without `Group=`
        // systemd takes whatever primary group the agent user was
        // provisioned with - where that is shared, the mode grants traversal
        // to every member of it and the boundary is not the one section 6
        // describes. Unwatched, a later tidy dropping this line leaves the
        // suite green while the mode silently reverts to that.
        assert!(
            rendered
                .iter()
                .any(|a| a == "--property=Group=weaver-alpha"),
            "the runtime directory's group is named rather than inherited: {rendered:?}"
        );
        // **And a template naming either boundary key is dropped**, not
        // emitted after them: `systemd-run` takes the last assignment, so an
        // installed hardening template could otherwise revert the mode or the
        // group with no diagnostic, and admin would then validate an access
        // rule against a group the socket does not carry.
        let hostile = UnitTemplate {
            properties: vec![
                "Group=users".into(),
                "RuntimeDirectoryMode=0755".into(),
                "MemoryMax=4G".into(),
            ],
            ..template.clone()
        };
        let rendered = start_arguments(
            &hostile,
            "weaver-alpha",
            "alpha",
            std::path::Path::new("/run/weaver-alpha/coordination.sock"),
            None,
        );
        assert!(
            rendered
                .iter()
                .any(|a| a == "--property=Group=weaver-alpha")
                && !rendered.iter().any(|a| a == "--property=Group=users"),
            "the template's Group is dropped: {rendered:?}"
        );
        assert!(
            rendered
                .iter()
                .any(|a| a == "--property=RuntimeDirectoryMode=0750")
                && !rendered
                    .iter()
                    .any(|a| a == "--property=RuntimeDirectoryMode=0755"),
            "the template's mode is dropped: {rendered:?}"
        );
        assert!(
            rendered.iter().any(|a| a == "--property=MemoryMax=4G"),
            "a property naming no boundary key still rides: {rendered:?}"
        );
    }

    /// The one interpolated value is the validated agent name, so a name
    /// reaching a unit has one origin.
    #[test]
    fn the_agent_name_is_the_one_variable() {
        let template = UnitTemplate {
            run_tool: "/bin/true".into(),
            control_tool: "/bin/true".into(),
            properties: vec![],
            worker: "/usr/libexec/weaver-worker".into(),
            spu: "/usr/libexec/weaver-spu".into(),
            gate: "/usr/libexec/weaver-gate".into(),
            headroom_bytes: None,
        };
        let rendered = start_arguments(
            &template,
            "weaver-alpha",
            "alpha",
            std::path::Path::new("/run/weaver-alpha/coordination.sock"),
            None,
        );
        assert!(rendered.iter().any(|a| a == "--property=User=weaver-alpha"));
        assert!(
            rendered
                .iter()
                .any(|a| a == "--property=RuntimeDirectory=weaver-alpha")
        );
        assert_eq!(unit_name("alpha"), "weaver-worker@alpha.service");
    }

    /// **The ask carries the worker's provisioning, after the binary and in
    /// the order the composition root reads.** The defect this pins is the one
    /// the act of 2026-08-13 landed on: an ask that started a worker with no
    /// arguments started a worker that refused its own start, and no load
    /// could reach a dial.
    ///
    /// Perturbation: drop any of the three pushes in `start_arguments` and
    /// this fails. The order matters as much as the presence, the composition
    /// root reading them positionally, so the assertion is on the tail as a
    /// sequence rather than on membership.
    /// **A unit on its way down is not at rest.** The fold has a safe
    /// direction and an unsafe one, and `deactivating` is the word that could
    /// take the unsafe one: read as inactive it lets an unload answer over a
    /// worker that has not stopped, which is the report section 6 forbids.
    ///
    /// Perturbation: fold `deactivating` with `inactive` and this fails.
    /// Watched under exactly that fold, which is where it came from.
    #[test]
    fn a_unit_in_motion_is_not_at_rest() {
        for word in ["active", "activating", "reloading", "deactivating"] {
            assert_eq!(residency_of(word), Residency::Active, "word {word}");
        }
        assert_eq!(residency_of("inactive"), Residency::Inactive);
        assert_eq!(residency_of("failed"), Residency::Failed);
        assert_eq!(residency_of("something-new"), Residency::Unknown);
    }

    #[test]
    fn the_start_ask_carries_the_workers_provisioning() {
        let template = UnitTemplate {
            run_tool: "/bin/true".into(),
            control_tool: "/bin/true".into(),
            properties: vec!["PrivateTmp=yes".into()],
            worker: "/usr/libexec/weaver-worker".into(),
            spu: "/usr/libexec/weaver-spu".into(),
            gate: "/usr/libexec/weaver-gate".into(),
            headroom_bytes: None,
        };
        let rendered = start_arguments(
            &template,
            "weaver-alpha",
            "alpha",
            std::path::Path::new("/run/weaver-alpha/coordination.sock"),
            None,
        );
        assert_eq!(
            rendered[rendered.len() - 4..],
            [
                "/usr/libexec/weaver-worker".to_string(),
                "/run/weaver-alpha/coordination.sock".to_string(),
                "/usr/libexec/weaver-spu".to_string(),
                "/usr/libexec/weaver-gate".to_string(),
            ],
            "the binary and its three arguments close the vector: {rendered:?}"
        );
    }

    /// **No template property moves what this crate elects**, per
    /// `weaver-admin-Spec` section 6.
    ///
    /// `systemd-run` takes the last assignment, so every one of these would
    /// otherwise win over what this crate sets, and `unreachable_peer` would
    /// then validate an access rule against an identity the worker does not
    /// hold. **`SupplementaryGroups=` is the one worth naming**: it is
    /// additive, so it leaves `User=` and `Group=` reading correctly while
    /// handing the worker a gid `agent_gids` never computes, which reopens
    /// the sink traversal along a second route after the first was closed.
    /// **An empty `RuntimeDirectory=` is the other**: it resets the list
    /// rather than setting it, so systemd creates no runtime directory and
    /// the coordination socket has nowhere to bind. Both are covered here in
    /// the shape that bites rather than in the shape that looks like an
    /// override.
    ///
    /// Perturbation: dropping any key from [`ELECTED_KEYS`] fails this.
    /// Watched failing 2026-08-29 with `SupplementaryGroups` removed and
    /// again with `RuntimeDirectory` removed.
    ///
    /// conforms: admin-runtime-directory-mode-is-stated
    #[test]
    fn no_template_property_moves_the_workers_identity() {
        let template = UnitTemplate {
            control_tool: "/bin/true".into(),
            run_tool: "/bin/true".into(),
            properties: vec![
                "User=root".into(),
                "Group=users".into(),
                "SupplementaryGroups=wheel docker".into(),
                "DynamicUser=yes".into(),
                "RuntimeDirectoryMode=0777".into(),
                // **Empty resets rather than sets**, which is the shape that
                // costs the runtime directory and with it the socket.
                "RuntimeDirectory=".into(),
                // Whitespace around the key is the same key.
                " Group =nogroup".into(),
                // And a hardening property that moves nothing rides through.
                "PrivateTmp=yes".into(),
            ],
            worker: "/usr/libexec/weaver-worker".into(),
            spu: "/usr/libexec/weaver-spu".into(),
            gate: "/usr/libexec/weaver-gate".into(),
            headroom_bytes: None,
        };
        let rendered = start_arguments(
            &template,
            "weaver-alpha",
            "alpha",
            std::path::Path::new("/run/weaver-alpha/coordination.sock"),
            None,
        );
        for dropped in [
            "User=root",
            "Group=users",
            "SupplementaryGroups=wheel docker",
            "DynamicUser=yes",
            "RuntimeDirectoryMode=0777",
            "RuntimeDirectory=",
            "Group =nogroup",
        ] {
            // **Matched whole rather than by substring**: this crate's own
            // `RuntimeDirectory=weaver-alpha` contains the reset form
            // `RuntimeDirectory=`, so a `contains` check here would report
            // the election as the template's leftover.
            let emitted = format!("--property={dropped}");
            assert!(
                !rendered.contains(&emitted),
                "the template may not set {dropped}: {rendered:?}"
            );
        }
        assert!(
            rendered.contains(&"--property=PrivateTmp=yes".to_string()),
            "a property that moves no identity rides through: {rendered:?}"
        );
        // And what this crate sets is still what the vector carries - the
        // runtime directory included, an empty reset having been the way to
        // lose it.
        assert!(
            rendered.contains(&"--property=User=weaver-alpha".to_string())
                && rendered.contains(&"--property=Group=weaver-alpha".to_string())
                && rendered.contains(&"--property=RuntimeDirectory=weaver-alpha".to_string())
                && rendered.contains(&"--property=RuntimeDirectoryMode=0750".to_string()),
            "the crate's own election survives the filter: {rendered:?}"
        );
    }

    /// **The declared loop rides the vector, and only where declared**, per
    /// `weaver-admin-Spec` section 6: the vector's one declaration-sourced
    /// value crosses as a named flag after the positional arguments, and an
    /// absent member puts no flag on the vector so the worker's own default
    /// stands.
    ///
    /// Perturbation: push the flag unconditionally and the absent half fails.
    /// Drop the push and the declared half fails.
    #[test]
    fn the_declared_loop_rides_the_vector_and_only_where_declared() {
        let template = UnitTemplate {
            run_tool: "/bin/true".into(),
            control_tool: "/bin/true".into(),
            properties: vec![],
            worker: "/usr/libexec/weaver-worker".into(),
            spu: "/usr/libexec/weaver-spu".into(),
            gate: "/usr/libexec/weaver-gate".into(),
            headroom_bytes: None,
        };
        let declared = start_arguments(
            &template,
            "weaver-alpha",
            "alpha",
            std::path::Path::new("/run/weaver-alpha/coordination.sock"),
            Some(std::path::Path::new("/etc/weaver/agents/alpha.loop.py")),
        );
        assert_eq!(
            declared[declared.len() - 2..],
            [
                "--loop-file".to_string(),
                "/etc/weaver/agents/alpha.loop.py".to_string(),
            ],
            "the declared loop closes the vector as a named flag: {declared:?}"
        );
        let absent = start_arguments(
            &template,
            "weaver-alpha",
            "alpha",
            std::path::Path::new("/run/weaver-alpha/coordination.sock"),
            None,
        );
        assert!(
            !absent.iter().any(|a| a == "--loop-file"),
            "an absent member puts no flag on the vector: {absent:?}"
        );
    }
}
