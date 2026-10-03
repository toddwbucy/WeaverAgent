# Redeploy log: olympus, 2026-10-03, the start step of #50

The end-to-end run C4 of #50 owes before #79 leaves draft. It stands the stack built from
#79's tree (C2, C3 and C4, revision `c225971`) on olympus. A throwaway agent, `cfour`,
runs through its connector's sudo rule as admin-con will, and is then taken down by hand.
Times are local (CDT). The box kept the agents of the tuple run (`rusty`, `rustbase`,
`pyra`), which were not touched: not their accounts, territories, trace group or
declarations. Their old install was moved aside, reversibly, on the operator's ruling of
2026-10-03.

## 0. Box facts, measured 15:40

    host        olympus, Linux 7.2.8-1-cachyos
    cards       0, 1: NVIDIA RTX A6000 (49140 MiB); 2: NVIDIA RTX 2000 Ada Generation (16380 MiB); driver 615.71.09
    nvcc        cuda_13.4.r13.4 (pacman cuda 13.4.2-1), cccl 3.3.4-1 (inside the 3.1.4-3.3.4 window)
    g++         16.2.1, and /usr/bin/g++-15, which the build used as NVCC_CCBIN (update-stack.sh's default)
    postgresql  18.4-3.1
    toolchain   nightly-2026-02-13 via rust-toolchain.toml
    sudoers     includes /etc/sudoers.d

What stood: the install of 2026-09-30 and 2026-10-01, on the box-wide layout from before
2026-10-01. `/etc/weaver/admin` held the base-level keys (`allow-list`,
`agent-config-directory`, `run-tool`, `control-tool`, `unit-properties`, `log-path`
and the rest), no stack record stood, `/opt/weaver/bin` and `/opt/weaver/lib` held the
old members and engine, and nothing ran: no worker, no `weaver-worker@` unit active.
Nothing held a file under `/opt/weaver/bin` or `/opt/weaver/lib` open.

## 1. The old install, moved aside (15:43)

    sudo install -d -o root -g root -m 0755 /etc/weaver/admin.pre-c4
    sudo sh -c 'mv /etc/weaver/admin/* /etc/weaver/admin.pre-c4/'     # the 13 base-level files
    sudo mv /opt/weaver/bin /opt/weaver/bin.pre-c4
    sudo mv /opt/weaver/lib /opt/weaver/lib.pre-c4

`/etc/weaver/admin` was left standing and empty, which `bootstrap-stack.sh` accepts.
`/etc/ld.so.conf.d/weaver.conf` held `/opt/weaver/lib` and `/opt/cuda/lib64` before
the run, and bootstrap rewrote it with the same two lines. `/opt/weaver/models`,
`python-spu`, `loops` and the `backup-*` directory were not touched.

**To undo**, returning the box to the old install, with no agent of the new layout
standing:

    sudo rm -r /opt/weaver/bin /opt/weaver/lib /etc/weaver/stack
    sudo mv /opt/weaver/bin.pre-c4 /opt/weaver/bin
    sudo mv /opt/weaver/lib.pre-c4 /opt/weaver/lib
    sudo ldconfig
    sudo sh -c 'mv /etc/weaver/admin.pre-c4/* /etc/weaver/admin/' && sudo rmdir /etc/weaver/admin.pre-c4
    sudo rmdir /run/weaver.run          # made by the new admin, empty once cfour is gone

## 2. Build and install (15:44 to 15:49)

`deploy/bootstrap-stack.sh`, with its own `CARGO_TARGET_DIR` and `NVCC_CCBIN=g++-15`:
the three crates' release tests passed (236 passed, 0 failed), and the release build of
the workspace with every engine took 5 min 35 s. `--install` placed the seven members
(`weaver-trace-relay` beside the worker) and the 15 engine objects. It wrote the stack
record with `library-path` and none of the retired keys, and `weaver-spu` resolved its
five engine objects. `/var/lib/weaver-agent` was already root 0755, holding the tuple
agents' territories, and `install -d` changed nothing there.

## 3. The agent (15:49)

    deploy/create-agent.sh cfour --engine sqlite --artifact /opt/weaver/models/qwen2.5-0.5b-instruct-q6_k.gguf [--apply]

The plan needed no sudo. Apply made the four accounts and two groups, the territory, the
trace and the state room, and wrote `~/.weaveragent/cfour/agent.toml` as the operator,
the directory 0700. The root held `declaration-directory`, `operator` and `roles.toml`.
The boundary probes passed: the member writes its room, the agent's uid cannot enter it,
the member cannot read the trace, the relay holds the trace group alone, and the
connector holds `weaver-cfour-admin` and its own group. The rule passed `visudo` and
`sudo -l -U` printed its five lines. The admission followed, and **the connector ran
`validate` through its rule**, answering `{"kind":"validated"}`.

## 4. verify-load (15:50)

`sudo deploy/verify-load.sh cfour`, 1.6 s in all: validated, loaded `idle`, three new
events (`load`, `recall`, `message.system`). The load event names the session, the run,
`state_member: true` and the sqlite store. The three constituents `show` named, the
relay, the state member and the worker, each ran as its own account (`-relay`, `-state`,
the agent's) in the invoker's cgroup. After the unload every constituent was gone and
`show` read unloaded.

## 5. The stand-in admin-con (15:50 to 15:51)

The connector's own containment was a transient scope, `systemd-run --scope
--uid=weaver-cfour-admincon`, standing in for WeaverWeb's packaging, and every verb ran
as that user with `sudo -n` through the rule.

- **Containment, with `!pam_session` in effect.** `load` answered `idle`, and `show`
  named three constituents, all three in `/system.slice/c4-admincon.scope`, the
  connector's own cgroup. The scope listed exactly the relay, the state member and the
  worker.
- **The trace door.** As the connector, `{"offset":0}` streamed the `TraceLine` header
  (device, inode, birth time) and then the record's lines, eight in all. The relay
  logged `connect` and `disconnect` with the connector's uid. The operator, who holds
  the trace group and not the access group, was refused at the socket by the kernel
  (`EACCES`). `trace.sock` stood root:weaver-cfour-admin 0660.
- **A turn.** `deploy/turn.py cfour "<question>"` as the operator answered in 0.1 s, and
  the trace carries `turn.started` through `turn.closed` for it.
- **A logged verb with its output gone** (C2's broken-stdio item, the "and logs" half).
  `validate` with standard output and standard error closed, and `show` into a pipe
  whose reader had exited, both exited 0. Each appended its line to `admin.log`, which
  went from 12 to 13 to 14 lines.
- **The relay dies with the worker** (C3's held item: the start step drops its copy of
  the lifetime pipe's write end). `SIGKILL` to the worker, and the relay and the state
  member were gone within 50 ms. The relay logged `ended`, "the worker exited", `show`
  read unloaded, and the scope ended with its last process.
- **`unload` through the rule.** A second load in a fresh scope, then `unload`: 0.28 s,
  every constituent gone, `show` unloaded.
- **Fail closed.** A third load in a fresh scope, then `systemctl stop` on that scope:
  every constituent gone and `show` unloaded. Stopping the connector stops its agent,
  per toddwbucy/WeaverWeb#15.

`admin.log` and `worker.log` stood in `~/.weaveragent/cfour/`, the operator's, 0640.
The runs ended by `SIGKILL` and by stopping the scope wrote no `unload` event: the worker
died before it could, which is the forfeit `weaver-trace-PRD` names for an abrupt exit.
The next load opened a new run.

## 6. decommission's guard (15:51)

With cfour loaded, `sudo deploy/decommission.sh --archive <dir>` refused before making
anything: "agents still run or cannot be read: cfour (running)", and the directory was
never created. After the unload, the plan read `cfour stopped`. It listed cfour's root,
its rule and its declaration directory (marked kept). It also lists
`/etc/weaver/admin.pre-c4` as a base of the old layout, which a whole-box decommission
would archive and purge with the rest.

## 7. cfour taken down by hand (15:52)

Per `HowToDeployANewAgent.md` section 7, after `show` read unloaded:

    sudo rm /etc/sudoers.d/weaver-cfour
    sudo rm -r /etc/weaver/admin/cfour /run/weaver.run/cfour
    sudo rm -rf /run/weaver-cfour
    sudo userdel -r weaver-cfour
    sudo userdel weaver-cfour-state; sudo userdel weaver-cfour-relay; sudo userdel weaver-cfour-admincon
    for g in weaver-cfour weaver-cfour-state weaver-cfour-trace weaver-cfour-admin; do sudo groupdel "$g"; done
    sudo rm -r /var/lib/weaver-agent/weaver-cfour

No account or group named for cfour remains, and `/etc/weaver/admin` is empty again. The
declaration directory `~/.weaveragent/cfour/` stays, as the runbook says, holding the
declaration and the two logs. The box now holds the new stack with no agent, beside the
tuple agents' untouched remains, and section 1's undo returns it to the old install.

## 8. What the run found

Nothing refused that should not have, and no script needed a fix. Two notes for the next
box:

- `sg` is absent on olympus. A shell that predates create-agent's group joins reaches
  the gate with `sudo -u <operator> ...`, which takes the account's groups afresh, or
  with a new login.
- `devices = [0]` bound CUDA0, an RTX A6000, per `worker.log`, and the whole load of the
  0.5b model answered within the 1.6 s verify-load took.
