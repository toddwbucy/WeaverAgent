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
runs in a virtual machine made from the image of that charter's section 7.2. Admin's
share is the one the charter's section 7.1 names: the firewall table of its section 2,
the data-mount check of its section 4.1, the refusal of state a version cannot read, the
load and the unload as the agent's start and stop, and the trace's handoff. It is a
child of `weaver-admin-PRD`.

Level discipline. The charter carries each requirement and its reason. This document
elects how admin represents each, against the ground the charter's section states, and
cites that section. Where a ruling or a measurement fixes a representation, this
document states it. Where neither does, the cell stays open and names what settles it.
This document states no requirement of its own and invents no value. It cites charters
and the operator's rulings only.

**Only admin's share is here.** The image, its build, its boot rules and its first boot
are the image's, and the connectors are WeaverWeb's. Each is represented by its owner's
documents, which section 7 names, and this document restates none of them.

The terms of `weaver-admin-PRD` section 7.1 hold here: the image, an appliance, the
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
refusal's case and what it names, the form of the report with the surface it crosses,
and the sizes that count as sufficient are cells of section 9. How the state mount meets
the agent's territory is open in the charter's section 10.

## 2. State a version cannot read

Per `weaver-admin-PRD` sections 4.1 and 7.2.

**The refusal of state this version cannot read stands for save points.** The inventory
of `weaver-admin-Spec` section 4 judges a save point's stamp, its check and its digest
and refuses `ConfigInvalid` naming the save point. The member answers the enter's
`restored` ask with a refusal naming a schema mismatch, so the harness refuses the enter
before it authors `load`, per `weaver-state-Spec` section 3.

**State is transferable, and nothing is claimed across versions**, per the charter's
section 7.2: an upgrade moves the state and the model to a new version of the harness.
A version that reads the state loads it. A version that cannot read it refuses it as
above. The record of an upgrade is a cell of section 9.

## 3. The firewall table

Per `weaver-admin-PRD` sections 2 and 7.3.

**The mechanism is nftables**, the operator's substrate ruling of 2026-11-14.

**Admin writes one named table and nothing else of the firewall.** The table never
carries the machine's whole policy. On an appliance the image's own boot rules carry the
base policy, input dropped but SSH, per the charter's section 7.3.

**Every load replaces the table before it opens anything, and the table is not
persisted**, on the operator's ruling of 2026-11-15. Admin writes the table to the
kernel and to no file that a boot reads, so a reboot removes it, and a table that a run
left open stands until the next load replaces it. Nothing else recovers it. The unload
closes the ports the load opened, per the charter's section 2.

**No declaration member names a port as of 2026-11-15**, so no load has a port to open.

**What this document does not elect.** The table's name, the shape of its rules, how a
declaration names a port, the table's place in the load and the unload, and how the run
card records the open ports are fixed by no ruling or measurement as of 2026-11-15.
`weaver-admin-PRD` section 11 names the documents that settle them, and section 9
carries each. Whether the one table is the agent's own, on a machine that runs several
agents, is open in the charter's section 10. **How a port that a declaration names gets
through the image's boot rules** is open in the charter's section 10. In nftables each
base chain gives its own verdict and a drop in any one is final, so an accept in admin's
table alone does not open a port there. This document does not write that admin's table
opens a declared port on an appliance.

## 4. Start, stop, and a reboot

Per `weaver-admin-PRD` sections 7 and 7.1.

**Starting and stopping the agent are its load and unload through admin.** The console
asks for either through admin-con, per `weaver-admin-operator-contract`.

**Nothing of this framework starts the agent after a reboot.** Admin runs per verb and
is not resident, and it lays no unit, timer or other start of its own. An operator's own
packaging that starts the agent is the operator's, per the charter's section 7.

## 5. The trace's handoff

Per `weaver-admin-PRD` sections 2, 4.1 and 7.5.

**Admin opens the sink and hands the descriptor to the worker**, under root, per the
charter's section 2 and load step 4. `weaver-admin-Spec` section 5 represents the open
for each kind of sink.

**On the operator's ruling of 2026-11-15 the trace's destination is the operator's
declaration in Weaver Web**: a path on the state mount, a sink off the machine, or
nowhere. A null sink is legitimate, and the run card records the choice. This document
elects nothing new for the handoff, for two reasons:

- `weaver-admin-Spec` section 5 opens a file sink beneath the territory's descriptor
  ("The trace's directory is the territory"), so a file sink on the state mount does not
  meet it as written.
- Where the sink is named, where it stands, and whether a sink that cannot be opened
  refuses a trace that goes nowhere are open in the charter's section 10, each against
  `weaver-admin-operator-contract`, for the operator to settle.

`weaver-admin-Spec` section 5 follows the operator's settlement.

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
  per `weaver-admin-PRD` section 11.

## 7. Held elsewhere

- **The image**: its operating system, its build from named commits, its identity and
  stamp, its driver, its seal, its boot rules, its time source, its versions and
  formats, and what it lays at the two mount points. Its build's documents in
  `toddwbucy/WeaverTools` represent it. The requirements are `weaver-admin-PRD` section
  7.2's and 7.3's.
- **The console's side**, in `weaver-web-PRD` and its Spec in `toddwbucy/WeaverWeb`:
  enrollment's console half, the connectors' keys and certificates, the one-time secret,
  the console's refusal of a second live holder of a certificate it issued, the
  connectors' units and how they come up after a reboot, the model store and the model's
  pull, the image download, and the display of the storage report.
- **The install record and the hardware read at each load**, per `deploy/REDEPLOY.md`
  section 3 and `deploy/verify-load.sh`. The guest's processor joining that read is an
  edit owed, per `weaver-admin-PRD` section 11.
- **The evidence, and the measurements still owed** before a Spec relies on the claim of
  `weaver-admin-PRD` section 7.1. They stay with the experiments of
  `toddwbucy/WeaverTools` and its issue 28.

## 8. Open rulings

None of this document's own. The operator's open rulings that bear on it are carried in
`weaver-admin-PRD` section 10: what the run card is, whether the one table is the
agent's own, where the trace's sink is named, where it stands, a sink that cannot be
opened, and how the state mount meets the agent's territory.

## 9. Open cells, each with what settles it

- **How admin knows it runs on an appliance.** Settled by the act that adds the check,
  per `weaver-admin-PRD` section 11.
- **How admin represents a separate filesystem, the refusal's case and what it names.**
  Settled by the act that adds the check.
- **The form of the storage report, and the surface it crosses to the console.** Settled
  by the act that adds the check, with the edit to `weaver-admin-operator-contract` that
  `weaver-admin-PRD` section 11 owes.
- **The sizes that count as sufficient.** No run has measured what an agent writes to
  the state mount. Settled by that measurement, which the act that adds the check owes
  before it states a size.
- **The table's name, the shape of its rules, how a declaration names a port, the
  table's place in the load and the unload, and the run card's record of the open
  ports.** Settled by the act that adds the table and the act that adds the
  declaration's port member, per `weaver-admin-PRD` sections 10 and 11.
- **How a port that a declaration names gets through the image's boot rules.** Settled
  by the act that adds the declaration's port member, in `weaver-admin-Spec` and the
  Spec of the image's build together, per `weaver-admin-PRD` section 10.
- **The record of an upgrade.** Which record carries an upgrade, and how state made
  under an older version is marked as used under the newer one. Settled by the act that
  writes it, per `weaver-admin-PRD` section 11.
- **A stopped agent against a machine that is down.** What the operator reads to tell
  the two apart. Settled when the first stop is run from the console.
