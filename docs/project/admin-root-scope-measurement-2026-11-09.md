# The agent's own scope: the measurement before the Spec act

Date: 2026-11-09.

## What this note is

This note records the two measurements that gate the #166 Spec act, on the operator's
rulings of 2026-11-07 (every load runs in a root-owned transient scope of its own, made
by admin's start step, and the order is measure, then amend the Spec, then write the
code) and of 2026-11-09 (Parts A and B gate the code, Part C follows it, and a transient
stand-in unit is acceptable for the measurement). `weaver-admin-Spec` section 6.1 cites
it for its measured figures. The scripts and every run's console output are kept with
the operator's evidence, outside the repository.

- **Part A** reproduces today's refusal of a connector load, through the real connector
  account and its exact granted line, so the amendment answers a measured refusal and
  not a read one.
- **Part B0** makes the request the amended start step makes, from root stand-in units,
  and reads what PID1 does with it.

Both ran on one host, against the stack installed from `main` at f75b290d, whose admin
crate, its documents and the operator contract are byte-identical to the tree this act
amends. The agent was `granite1`, and neither part needed the GPU.

## Part A: today's refusal, through the connector's line

**The rule.** `sudo -l -U` for the connector's account granted the eight fixed lines of
`deploy/create-agent.sh`, `load granite1` among them, beside `!pam_session` and a
`secure_path` of `/usr/local/sbin:/usr/local/bin:/usr/bin`.

**The invoker.** Each case ran a transient stand-in unit under the connector's account,
whose command ran `sudo -n /opt/weaver/bin/weaver-admin load granite1`, the granted line
whole.
- **a1**, the shape WeaverWeb's Spec section 8 installs: the unit's cgroup
  `/system.slice/weaver-166-a1-admincon.service`, root's, with no delegate mark.
- **a2**, the same unit with `Delegate=yes`: its directory and its three delegated files
  owned by the connector's account, and both `trusted.delegate` and `user.delegate`
  reading `1`.

In both cases `system.slice` and the mount's root were root's.

**The first run stopped earlier than the guard, and it measured the order.** Both cases
refused `{"kind":"config_invalid","field":"restore"}`. The declaration's `[restore]`
named a save point at ordinal 13, and the manifest's latest was the previous batch's
leave save point at ordinal 14, so N6 (the operator's ruling of 2026-10-08 on #99)
refused in the publication and selection of the start step. That step runs before the
agent's cgroup is made, so a load whose `[restore]` is stale refuses in the invoker's
containment, and after the amendment it asks PID1 for nothing.

**The rerun.** A preflight read `[restore]` against the manifest's latest, by name or
digest and then by ordinal, as N6 does, and ran `weaver-admin restore granite1` first,
which answered `restore_named` and made the named save point the latest again. Each case
read it again before its unit started.
- **a1** refused `{"kind":"boundary_unverified"}`, sudo exiting 1: "the invoker's cgroup
  /system.slice/weaver-166-a1-admincon.service is not delegated (no user.delegate or
  trusted.delegate mark)".
- **a2** refused `{"kind":"boundary_unverified"}`, sudo exiting 1: "the invoker's cgroup
  /system.slice/weaver-166-a2-admincon.service, a file delegated with it, or a cgroup
  above it is not root's, so a process of the agent could be moved out of its cgroup
  (#166)".

**What admin did before it refused.** `admin.log` held, per case: the publication ("the
room holds 0 finished names"), the restore it was about to make, the refusal, "rolled
back: sink closed", and the dispatch line. Under each invoker strace saw four tasks: the
shell, `sudo`, `unix_chkpwd` (sudo's PAM account check, before its exec of admin) and
`weaver-admin`. Admin forked nothing and cloned no thread, `pids.peak` read 3, and admin
ran about 18 ms from its exec to its exit.

**What moved.** Between the before and after snapshots: `admin.log`, `admin.lock`'s
holder and modification time, and the runtime directory, which lost a stale `gate.sock`
(the runtime directory is prepared before the guard). The trace (175 lines) and
`worker.log` did not change.

**The execution context the connector's unit handed down.** `NoNewPrivileges=no`, the
full capability bounding set, no system call or address family filter, no private or
protected view, and every namespace and mount the same as PID 1's. The open-files limit
was soft 2048 and hard 2097152, systemd's manager defaults (`DefaultLimitNOFILESoft` and
`DefaultLimitNOFILE`), and the locked-memory limit was 8 MiB.

**A shell load read the same limits.** On 2026-11-10 (UTC) a batch of loads started
from a shell, through the same `systemd-run --scope -p Delegate=yes` wrapper, ran every
process in its chain at soft 2048 and hard 2097152, each with an 8 MiB locked-memory
limit: the batch's own script, `sudo`, `verify-load.sh`, and the loaded agent's worker,
SPU, trace relay and state member. Neither `sudo`, admin nor the worker raised the limit
along that chain. The operator's interactive shell read soft 1048576 the same day, and
the batch did not descend from it. So neither measured path raises the limit, and the
operator contract's "resource limits sized for the agent, model loading included, or
unset" is an obligation on whoever lays the install, this repository's
`deploy/update-stack.sh` included, since WeaverWeb's install builds on it (the
operator's ruling of 2026-11-09). The connector's invoker was measured here, and its
agent's constituents are Part C's to read.

## Part B0: the request, from root stand-in units

**The request.** Each case ran a probe as root in a stand-in service unit. The probe
called PID1 as the amended start step would, by systemd's own `busctl`:

```text
busctl call --system org.freedesktop.systemd1 /org/freedesktop/systemd1 \
  org.freedesktop.systemd1.Manager StartTransientUnit 'ssa(sv)a(sa(sv))' \
  <scope> fail 4 PIDs au 1 <own pid> Delegate b true \
  Slice s weaver.slice CollectMode s inactive-or-failed 0
```

It then read its own `/proc/self/cgroup` every 5 ms until it named the scope, within a
bound of 10 seconds.

| Case | The caller's bounding set | The call | The cgroup at its return | The move read |
|---|---|---|---|---|
| move | full (`000001ffffffffff`) | 3.0 ms, a job | the stand-in's service | 9.6 ms after the call began, after one poll |
| capbnd | `00000000000000ef`, no `CAP_SYS_ADMIN` | 2.6 ms, a job | the scope | 3.2 ms after the call began, no poll |

The 9.6 ms figure is an upper bound, set by the 5 ms poll. **The move lands before the
call returns or after it**, so the start step's own reading, not the call's answer, says
the move happened.

**The scope.** `/weaver.slice/weaver-166-probe.scope` was root's and `0755`, its
`cgroup.procs`, `cgroup.threads` and `cgroup.subtree_control` root's, both
`trusted.delegate` and `user.delegate` read `1`, and `weaver.slice` and the mount's root
were root's. Every input #166's guard reads passes. The unit carried `Delegate=yes`,
`KillMode=control-group`, `MemoryMax=infinity` and systemd's default `TasksMax`, and
nothing set any other limit.

**A second request of the same name**, while the scope stood, refused: "Unit
weaver-166-probe.scope was already loaded or has a fragment file.", `busctl` exiting 1.

**The invoker's stop.** The stand-in unit was stopped under `KillMode=control-group`
with the probe moved, and the probe lived on in the scope.

**The scope's end.** The probe made one empty child cgroup, as the start step's `mkdir`
would, then exited. PID1 collected the scope, and the scope's directory and the child
were both gone. The journal read only that the scope started and was deactivated
successfully.

**A dead pid.** A request naming a pid that had exited refused, "Failed to set unit
properties: No such process", and left no unit.

**The narrowed bounding set.** The capbnd caller held only `CAP_CHOWN`,
`CAP_DAC_OVERRIDE`, `CAP_DAC_READ_SEARCH`, `CAP_FOWNER`, `CAP_KILL`, `CAP_SETGID` and
`CAP_SETUID`, and PID1 admitted it. Which path admitted a root caller, its uid or
polkit's answer for root, was not read.

**The slice.** Before any use, PID1 read `weaver.slice` as loaded with no fragment, and
its cgroup was absent. The first scope made the slice's cgroup, root's and `0755`, and
after both probe scopes ended the slice stayed loaded and active, its directory
standing. So no install lays a slice unit, and the slice outlives its scopes.

**A dry run under the operator's user manager**, made before B0, agreed in kind: the
call returned a job, and the move followed it by about 18 ms.

## What follows the code

Parts B1 to B5 emulate the placement with `systemd-run --scope -p Delegate=yes
--slice=weaver.slice` under the GPU lease, reading the placement, a connector's refused
write to the scope's `cgroup.procs`, a connector's restart and the run end. Part C reads
the ruling's facts through the real granted line once the code act lands, with a
constituent's inherited context (`NoNewPrivs`, the bounding set, the limits and its
mount namespace against PID 1's).
