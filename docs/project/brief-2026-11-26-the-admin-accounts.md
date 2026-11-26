# Brief: the accounts, act one's code

**Filed:** 2026-11-26. **Base:** `act/the-admin-accounts`, on the head of
`act/the-loop-process` (#655), which already changes the name rules this act changes.
**Authority:** the operator's ruling on accounts of 2026-11-26, his answers of
2026-11-26, and the planning seat's decisions of the same day. Each is marked below.
**Contract:** the service's socket is `weaver-admin-operator-contract` section 8 (#658).
No code here is built against it until it reaches `main`.

## Which weaver-admin

The name means three things, and every document of this act says which at each site:

- **the account** `weaver-admin`, the third account of the machine beside the two
  connectors' accounts, `weaver-gatecon` and `weaver-admincon`;
- **the service** `weaver-admin-service`, which runs as that account and holds the
  socket;
- **the root program** `weaver-admin`, run per verb, which keeps its name until act
  two.

## Decisions that stand

| # | Decision | Whose |
| --- | --- | --- |
| 1 | The service is a third binary of the admin crate, `weaver-admin-service`. | the planning seat's |
| 2 | A socket unit and a service unit, both enabled, so the service stands from boot. | the operator's words ("It is a standing systemd service under its own account, not root."), read by the planning seat |
| 3 | The service runs without no-new-privs, because sudo gains root by its set-user-id bit. `weaver-admin-Spec` section 2 gives the reason. | the planning seat's |
| 4 | Every document says at each site which weaver-admin is meant. | the planning seat's |
| 5 | **The gate's door** is a folder of its own, setgid, grouped `weaver-<name>-gate`, and `-gate` becomes a reserved suffix. Not the variant grouped to the gate connector's own group. The world contract admits the seeding line from the operator's uid alone where the rule names it, and such a second dialer should join the door alone. | the planning seat's |
| 6 | Admin-con keeps the link to Weaver Web and asks the service a small protocol on the local socket. The service reads the grant itself on every verb, and the contract leaves room for a fifth message that carries the loop's text for loop install. | the operator's (77 (f)) |
| 7 | The name `stack` is refused. Built in #655. | the operator's (75 (e)) |
| 8 | Each connector's private key is in a file of its own, which the weaver-admin account cannot read. It lands with the later children. | the operator's (77 (c)) |
| 9 | The per-part read access of `/etc/weaver/<name>/`, and his Layout paragraph, are act two's. | the operator's (77 (e), 77 (d)) |

## The children

### In this stack now

1. **This brief.** No test.
2. **The names.** On top of what #655 holds:
   - **The account `weaver-admin`** is the machine's third account. `decommission.sh`
     knows it as one, so it never reads as an agent named `admin`. This covers
     `is_box_account`, the role it names, and every site that lists the two
     connectors.
   - **The suffix `-gate`** is refused wherever the other suffixes are:
     - `create-agent.sh`'s `agent_name_ok`;
     - admin's `well_formed`;
     - `verify-lifecycle.sh`'s mirror;
     - `decommission.sh`'s suffix list.
   - **Tests:**
     - a row for `x-gate` in admin's suffix rows;
     - `test_plans.py`'s box-account test with `weaver-admin`;
     - decommission's strip test with `weaver-admin` and `-gate`;
     - a new test of the name pattern at `create-agent.sh`'s first name check, which no
       test makes today.
   - This child creates no account and moves no rule.
3. **The gate's door.**
   - **Admin's load** lays `<coordination-root>/weaver.run/<name>/gate/` as
     `weaver-<name>:weaver-<name>-gate 2750`, beside the trace door in the root-owned run
     folder. It clears a stale `gate.sock` there.
   - **The gate** binds `gate.sock` there, under its umask, as it binds today. The socket
     takes the folder's group, so a uid in the gate group connects and the agent's own
     uid stays denied by the gate's check. The harness and the two clients
     (`deploy/turn.py`, `verify-lifecycle.sh`) read the new path. Admin hands the worker
     the path, and neither derives it from the coordination socket.
   - **Admin's check** of an allowed uid moves from the agent's group to the gate group.
   - **`create-agent.sh`** makes the group and refuses it where it stands. The gate
     connector joins it in place of the agent's group, with the allow-list and the
     check to match. `decommission.sh` removes it.
   - **The documents:** `weaver-gate-Spec` section 3, `weaver-admin-Spec` section 6,
     the harness Spec, and `weaver-gate-world-contract` say the same, in the same child.
   - **Test:** a binder that holds neither the door's group nor root, and a uid in the
     gate group but not in the agent's group is admitted. The agent's own uid stays
     denied.
   - **The gate connector's config** takes the new path at install. That is the other
     seat's, carried by the planning seat.

### Later, and what each waits on

The children below wait on the contract of #658 reaching `main`, since no code is built
against a contract under review. They land together with the other seat's connector
change, on one word from the planning seat.

4. **The rule's caller and the reader.** The sudo rule names `weaver-admin` in place of
   `weaver-admincon`. The weaver-admin account takes the agents' state and access groups
   and the trace reader's role. The admin connector keeps its account, with no rule, no
   group and no reader role. Every `test_plans.py` site, the runbooks' account lines and
   the Specs move with it.
5. **The service.** `weaver-admin-service`, built to section 8 of the contract, with the
   manifest pin at three binaries. Its tests reach no sudo: the sudo path is set by a
   test-only constructor to a stand-in that records its argv.
6. **The stack lays the units.** `bootstrap-stack.sh` and `update-stack.sh` lay the
   binary and the two units and enable both. The install window's stop and start of the
   service is measured under a stand-in before it is claimed.
7. **The connectors' key files** (decision 8), in the form the other seat brings. Each
   key is laid by root where the weaver-admin account cannot read it.
8. **The runbooks and `docs/technical/`.**

Outside this repository, on the same later word:

- **the appliance's documents and the setup script:** the pair account's move to
  `weaver-admin`, with the points already owed on the script;
- **the two sentences of this repository's `CLAUDE.md`** that call admin non-resident
  and say no supervisor is part of the agent. Their candidate text is written when the
  service's child is built, and nothing is edited there.

## Acceptance

| Point | Child | Shown by |
| --- | --- | --- |
| `weaver-admin` is a box account to decommission, never an agent named `admin` | 2 | decommission's strip test and its box-account listing |
| `-gate` is refused as a suffix by every name rule | 2 | admin's suffix row, `agent_name_ok`, the lifecycle verifier's mirror, decommission's list |
| `create-agent.sh`'s name pattern is tested | 2 | a new `test_plans.py` test |
| the door is laid `weaver-<name>:weaver-<name>-gate 2750` and a stale socket cleared | 3 | admin's tests |
| a uid in the gate group, outside the agent's group, is admitted, and the agent's uid denied | 3 | a namespace test whose binder holds neither the group nor root |
| admin refuses an allowed uid outside the gate group | 3 | the inventory's test, moved |
| the gate connector holds the gate group, not the agent's | 3 | `test_plans.py` rows for the group, the allow-list and the check |
| the Specs and the world contract name the door | 3 | review against the sections the pull request names |
| every new refusal and check goes red when removed | 2, 3 | a perturbation row each |

## Not in this stack

- Nothing here creates the weaver-admin account, moves the sudo rule, or builds the
  service.
- Nothing lays a loop over the management plane. That is the loop's act 3.
- Act two's fixed root programs, the per-part access, the trace on the state drive and
  the relay's fold.
