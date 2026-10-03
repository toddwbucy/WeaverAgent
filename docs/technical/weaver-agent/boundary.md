# The agent's boundary

**Status: a stub, holding notes until the documentation pass (issue #67).** Written
2026-10-03 from the operator's rulings of 2026-10-02 and 2026-10-03.

## What the boundary is

The boundary is who and what may reach the agent from outside. It is distinct from the
agent's constitution, which is what shapes what happens inside it: the model, the system
prompt, the state-management settings, the elections and the save point it came from.

- **The constitution** lives in the declaration and at the top of the trace, on the load
  event, and it is part of the deployment tuple.
- **The boundary** lives in the agent's root and in admin's log. It is separate from the
  constitution, so changing who may reach the agent does not make it a different agent.
- **The boundary must be declared.** It is explicit and visible, so it never fails
  silently: a missing or malformed boundary file refuses by name, and nothing is ever
  admitted because a file was absent.

## Notes to expand

- **One agent, not a fungible instance.** A WeaverAgent is built as itself, with its own
  identity, state, trace and trust, even when several share a box (#63's rationale).
- **The agent's OS identity:** its users, groups, territory and modes.
- **The gate's allow-list:** today it sits in the declaration, where admitting a
  connector moves the digest. It is proposed to move to the root as boundary (#61).
- **Admin's two sockets,** lifecycle and trace, admitted by peer credential against the
  per-agent role list `roles.toml` (#50, #63).
- **Trace custody:** the trace group, the trace socket, and opening without following
  links (#56, #62).
- **The connectors:** each is one agent's own appendage and runs as its own user
  (toddwbucy/WeaverTools#6).
- **The operator's directory and the `operator` uid** (#57).
- **Host tools and OS-level grants** made by the operator (#22).
