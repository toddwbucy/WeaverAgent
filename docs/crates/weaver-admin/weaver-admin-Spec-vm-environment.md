# weaver-admin - Spec: the VM environment

**Status:** DRAFT. Not merged. It authorizes no code until the operator merges it.
**Date filed:** 2026-11-15
**Document ID:** `weaver-admin-Spec-vm-environment`
**Parent:** `weaver-admin-PRD`
**Editorial:** Per the Working Rules.
**Landing PR:** #561

---

## 0. What this document is

The specification of how admin represents its own share of the environment of
`weaver-admin-PRD` sections 7.1 to 7.5: the environment a WeaverAgent runs in when it
runs in a virtual machine that installed a carry set, per that charter's section 7.2.
Admin's share is the one the charter's section 7.1 names: the firewall table of its
section 2, the data-mount check of its section 4.1, the refusal of state a version cannot
read, the load and the unload as the agent's start and stop, and the trace's handoff. It
is a child of `weaver-admin-PRD`.

Level discipline. The charter carries each requirement and its reason. This document
elects how admin represents each, against the ground the charter's section states, and
cites that section. Where a ruling or a measurement fixes a representation, this
document states it. Where neither does, the cell stays open and names what settles it.
This document states no requirement of its own and invents no value. Its grounds come
from charters, contracts and the operator's rulings, and other documents are named only
for where a representation already stands.

**Only admin's share is here.** The carry set, its build and its install are the
deploy scripts', the machine the operator presents is the operator's, and the connectors
are WeaverWeb's. Section 7 names where each is represented, and this document restates
none of them.

The terms of `weaver-admin-PRD` section 7.1 hold here: the carry set, an appliance, the
console, enrollment, the WeaverAgent system and the agent.

## 1. The data mounts

Per `weaver-admin-PRD` sections 7.5 and 4.1.

**The two mount points are `/mnt/weaver/model` and `/mnt/weaver/state`**, fixed by the
operator's ruling of 2026-11-14.

**The check is made in the load**, between load steps 3 and 4 as the charter's section
4.1 places it, and `validate` runs it with the rest of the inventory, per the charter's
section 4.3. It reads the declaration's election of state from step 2.

**What refuses**, on the operator's ruling of 2026-11-15:

| Mount | Required | Refuses the load where |
|---|---|---|
| `/mnt/weaver/model` | always | the mount point is not a separate filesystem |
| `/mnt/weaver/state` | where the declaration elects state | the declaration elects state and the mount point is not a separate filesystem |

An agent whose declaration elects no state runs on the model mount alone, and its load
does not judge the state mount. The check judges the filesystem and not what lies on it,
and it mounts and repairs nothing. The sizes of the required mounts are reported.

How admin knows it runs on an appliance, how it represents a separate filesystem, the
refusal's case and what it names, and the form of the report with the surface it crosses
are cells of section 9. The judgment of a size is the console's, per the charter's
section 7.5. How the state mount meets the agent's territory is a later item, per the
charter's section 7.5.

## 2. State a version cannot read

Per `weaver-admin-PRD` sections 4.1 and 7.2.

**The refusal of state this version cannot read stands for save points.** The inventory
of `weaver-admin-Spec` section 4 judges a save point's stamp, its check and its digest,
and a save point that fails refuses the load. The member answers the enter's `restored`
ask with a refusal naming a schema mismatch, so the harness refuses the enter before it
authors `load`, per `weaver-state-Spec` section 3.

**State is transferable, and reproducibility is not claimed across versions**, per the
charter's section 7.2: an upgrade moves the state and the model to a new version of the
harness. A version that reads the state loads it. A version that cannot read it refuses
it as above. The record of an upgrade is a cell of section 9.

## 3. The firewall table

Per `weaver-admin-PRD` sections 2 and 7.3.

**The mechanism is nftables**, the operator's substrate ruling of 2026-11-14.

**Admin writes one named table and nothing else of the firewall.** **Each agent's table
is its own**, one table per agent on the operator's ruling of 2026-11-15, per the
charter's section 2. The table never carries the machine's whole policy. On an appliance
the base policy, input dropped but SSH, stands outside admin's table, and what carries
it is a later item of the charter's section 7.3.

**Every load replaces the table before it opens anything, and the table is not
persisted**, on the operator's ruling of 2026-11-15. Admin writes the table to the
kernel and to no file that a boot reads, so a reboot removes it, and a table that a run
left open stands until the next load replaces it. Nothing else recovers it. The unload
closes the ports the load opened, per the charter's section 2.

**No declaration member names a port as of 2026-11-15**, so no load has a port to open.

**What this document does not elect.** The table's name, the shape of its rules, how a
declaration names a port, the table's place in the load and the unload, what the
rollback does with a table that a refused load opened, and how the run card records the
open ports are fixed by no ruling or measurement as of 2026-11-15. `weaver-admin-PRD`
section 11 names the documents that settle them, and section 9 carries each. **How a
port that a declaration names gets through the appliance's base policy** is open in the
charter's section 10. In nftables each base chain gives its own verdict and a drop in
any one is final, so an accept in admin's table alone does not open a port there. This
document does not write that admin's table opens a declared port on an appliance.

## 4. Start, stop, and a reboot

Per `weaver-admin-PRD` sections 7 and 7.1.

**Starting and stopping the agent are its load and unload through admin.** The console
asks for either through admin-con, per `weaver-admin-operator-contract`.

**Nothing of this framework starts the agent after a reboot.** Admin runs per verb and
is not resident, and it lays no persistent unit, timer or other start of its own. An
operator's own packaging that starts the agent is the operator's, per the charter's
section 7.

## 5. The trace's handoff

Per `weaver-admin-PRD` sections 2, 4.1 and 7.5.

**Admin opens the sink and hands the descriptor to the worker**, under root, per the
charter's section 2 and load step 4. `weaver-admin-Spec` section 5 represents the open
for each kind of sink.

**The trace goes where it is declared**, on the operator's ruling of 2026-11-15, per the
charter's section 7.5. The operator chooses the destination on Weaver Web's declaration
page when the agent is created, and it may be `/dev/null`. A file sink's directory stays
one that no principal of the agent can write or replace, and the open follows no link.

This document elects nothing new for the handoff. `weaver-admin-Spec` sections 5 and 9,
`weaver-types-Spec` section 2 and `weaver-admin-operator-contract` sections 3 and 5
still hold the sink to the agent's territory, and the act that changes them, "the trace
goes where it is declared", is registered in the charter's section 11.

## 6. What is enforced, and by which instrument

Per apex section 11. Each election above is listed with what catches its failure.

- **The refusal of a save point** (section 2): the workspace's save-point corpus
  (`crates/weaver-types/tests/fixtures/save-points/`), read by both crates' tests, per
  `weaver-admin-Spec` section 4.
- **The data-mount check, its refusal and its report** (section 1): no instrument exists
  as of 2026-11-15.
- **Admin's table, its replacement at every load and its absence after a reboot**
  (section 3): no instrument exists as of 2026-11-15.
- The act that adds the check and the act that adds the table each owe walks and
  perturbation tests in the manner of `weaver-admin-Spec` section 10, before any code,
  per `weaver-admin-PRD` section 11's entry for the load additions.

## 7. Held elsewhere

- **The carry set**: its build from a named commit, its manifest and identity, and its
  install. `deploy/REDEPLOY.md` section 3a and the deploy scripts represent it, and the
  requirements are `weaver-admin-PRD` section 7.2's. The operating system and the driver
  are the run box's, per that section, the operator attaches and mounts the disks at the
  two mount points, per its section 7.5, and what carries section 7.3's base policy is a
  later item there.
- **The console's side**, in `weaver-web-PRD` and its Spec in `toddwbucy/WeaverWeb`:
  enrollment's console half, the connectors' keys and certificates, the one-time secret,
  what the console does with a clone, the connectors' units and how they come up after a
  reboot, the model store and the model's pull, and the display of
  the storage report.
- **The build's two settings**: the compiler's flag and the list of architectures that
  are the two settings of `weaver-admin-PRD` section 7.2. The deploy scripts
  (`deploy/bootstrap-stack.sh`, `deploy/update-stack.sh`) hold them as #570 sets them.
- **The install record and the hardware read at each load**, per `deploy/REDEPLOY.md`
  section 3 and `deploy/verify-load.sh`. The guest's processor joining that read is an
  edit owed, per `weaver-admin-PRD` section 11.
- **The evidence, and the measurements still owed** before a Spec relies on the claim of
  `weaver-admin-PRD` section 7.1. They stay with the experiments of
  `toddwbucy/WeaverTools` and its issue 28.

## 8. Open rulings

None of this document's own. The operator's one open ruling that bears on it, what the
run card is, is carried in `weaver-admin-PRD` section 10. A stopped agent against a
machine that is down is a cell of the charter's section 10, settled by an act. State and
save points on an appliance, with how the state mount meets the agent's territory, are a
later item, per the charter's section 7.5.

## 9. Open cells, each with what settles it

- **How admin knows it runs on an appliance.** Settled by the act that adds the check,
  per `weaver-admin-PRD` section 11.
- **How admin represents a separate filesystem, the refusal's case and what it names.**
  Settled by the act that adds the check.
- **The form of the storage report, and the surface it crosses to the console.** Settled
  by the act that adds the check, with the edit to `weaver-admin-operator-contract` that
  `weaver-admin-PRD` section 11 owes.
- **The table's name, the shape of its rules, how a declaration names a port, the
  table's place in the load and the unload, what the rollback does with a table that a
  refused load opened, and the run card's record of the open ports.** Settled by the
  act that adds the table and the act that adds the declaration's port member, per
  `weaver-admin-PRD` sections 10 and 11.
- **How a port that a declaration names gets through the appliance's base policy.**
  Settled by the act that adds the declaration's port member, in `weaver-admin-Spec` and
  the document of whatever carries the base policy together, per `weaver-admin-PRD`
  section 10.
- **The record of an upgrade.** Which record carries an upgrade, and how state made
  under an older version is marked as used under the newer one. Settled by the act that
  writes it, per `weaver-admin-PRD` section 11.
