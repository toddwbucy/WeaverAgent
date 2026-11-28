//! The agent name rule, `weaver-admin-Spec` section 4: one file the root
//! program and the service both compile, so the two judge a name alike.

/// The name's shape, judged before any path is built from it: non-empty
/// ASCII letters, digits, `-` and `_`, so `.`, `..` and anything carrying `/`
/// are refused. **A name ending in a reserved suffix is refused too**, per
/// Spec section 4, since the agent's derived accounts and groups append
/// `-state`, `-trace`, `-relay`, `-admin`, `-admincon` and `-gate` to
/// `weaver-<name>`, and an agent named `x-relay` would collide with agent
/// `x`'s relay.
/// **The names `gatecon` and `admincon` are refused whole**, on the operator's
/// ruling of 2026-11-10: `weaver-gatecon` and `weaver-admincon` are the box's
/// two connector accounts, so an agent of either name would own one of them as
/// its own identity. **Nor is a name beginning `admin`, or `stack`**: the
/// agent's own folder `/etc/weaver/<agent>/`, where the load looks for the
/// loop folder, stands beside the config bases, every `/etc/weaver/admin*`,
/// and the stack's record, `/etc/weaver/stack`.
pub(crate) fn well_formed(agent: &str) -> bool {
    const RESERVED: &[&str] = &["-state", "-trace", "-relay", "-admin", "-admincon", "-gate"];
    const BOX_ACCOUNTS: &[&str] = &["gatecon", "admincon"];
    const BESIDE: &[&str] = &["stack"];
    !agent.is_empty()
        && agent
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        && !RESERVED.iter().any(|suffix| agent.ends_with(suffix))
        && !BOX_ACCOUNTS.contains(&agent)
        && !agent.starts_with("admin")
        && !BESIDE.contains(&agent)
}
