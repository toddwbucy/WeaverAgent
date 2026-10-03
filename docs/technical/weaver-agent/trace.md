# The trace

**Status: a stub, holding notes until the documentation pass (issue #68).** Written
2026-10-03 from the operator's rulings. This is the concept page. The crate that writes
the trace has its own page, `weaver-trace.md`.

## What the trace is

The trace is the source of truth for what happened inside the agent. It is append-only
and lossless for every event it admits, and it is the floor under the other two layers:
state management and memory are built from it and rebuild from it.

- **At its top is the agent's constitution:** the load event, with the declaration's and
  the system prompt's digests, the state-management settings, the elections and the
  lineage of the save point it came from.
- **It records what the agent does:** every judgment the loop makes, every save point
  taken, restored or reset, and every tool call with its return.
- **It is about what is inside the agent.** The boundary (`boundary.md`) is not part of
  it, beyond a declared digest.

## Notes to expand

- **The three layers:** trace, state management and memory, with authority running
  downward.
- **Custody:** the agent never reaches its own trace. Root opens the sink, and readers
  reach it only through the trace group or admin's trace socket.
- **Save-point events,** and how a rebuild honours a reset.
- **Reproducibility:** what the deployment-tuple run proved (WeaverTools#9).
- **What the trace does not record:** side effects beyond the agent's seams, such as
  what a shell command did on the network (#22).
