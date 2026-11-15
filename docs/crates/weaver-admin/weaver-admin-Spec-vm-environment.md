# weaver-admin - Spec: the VM environment

**Status:** DRAFT. Not merged. It authorizes no code until the operator merges it.
**Date filed:** 2026-11-15
**Document ID:** `weaver-admin-Spec-vm-environment`
**Parent:** `weaver-admin-PRD`
**Editorial:** Per the Working Rules.

---

## 0. What this document is

The specification of the environment a WeaverAgent runs in when it runs in a virtual
machine made from the agent image. It covers what the image holds, how an operator gets
in, the machine's half of pairing, the two data mounts, admin's one firewall table, and
how the machine behaves after a reboot and at an upgrade. It specifies the environment
and nothing else. On the operator's ruling of 2026-11-15 it is a child of
`weaver-admin-PRD` and not a charter of its own: the appliance charter drafted on
2026-11-14 is recast as this Spec, and its requirement-level statements moved into
`weaver-admin-PRD` sections 2, 4.1, 7 and 10 in the same act.

Level discipline. `weaver-admin-PRD` carries what admin needs and why, and the
requirements of this environment that bind admin are among them. This document says how
the environment represents them. Where a ruling or a measurement fixes a representation,
this document states it. Where neither does, the cell stays open and names what settles
it. This document invents no value.

Authority. Every requirement here is the operator's ruling of 2026-11-14 unless the
sentence says otherwise. Three sources carry those rulings, in descending order: the
operator's written brief of that date on the appliance path, the operator's rulings
later that day, and the appliance sketch (v0.3, 2026-11-14), whose sections the operator
ratified. A sentence that states a proposal says that it is a proposal. Section 10 lists
what the operator has not ruled. A measurement is named by the build that took it, among
the builds of 2026-11-14 that `template-clone-Spec` section 10 and pull request 31 in
`toddwbucy/WeaverTools` describe.

**Five terms carry one meaning each in this document.**

- The **image** is the sealed disk image. One image exists for each version, and it is
  published with its hash.
- An **appliance** is a virtual machine that an operator made from the image.
- The **console** is the Weaver Web server, the operator's management surface for many
  agents.
- The **agent** is the bounded thing: the processes of one WeaverAgent that run under
  their own UIDs.
- The **WeaverAgent system** is everything installed to run and to bound that agent:
  admin, the gate, the harness, the SPU, state management, and the two connectors, which
  are WeaverWeb's.

This document never uses "the agent" for a component that holds a privilege. It names
the component.

## 1. What the image is

**A sealed image that carries the WeaverAgent system for one agent and nothing else.**
An operator installs Weaver Web, takes the image, loads it into a hypervisor they
already run, and makes a virtual machine from it. From then on the operator manages the
agent service through the console and manages the machine through the hypervisor. The
console knows the agent. The hypervisor knows the machine. Neither reaches into the side
of the other.

**It exists to hand an operator a held deployment tuple.** The output of a turn depends
on what computes it: the binaries, the libraries they load, the driver, the model bytes
and the declaration. An image holds the first three as one hashed object. An operator
who runs a version of the image then runs the same bytes as every other operator who
runs that version. What has been measured about this claim, and what is still owed, is
`toddwbucy/WeaverTools` issue 28's, per section 9.

**The agent is bounded by the operating system under a UID.** The image does not give
the agent an operating system. It gives the WeaverAgent system a machine with nothing
else on it. The agent stays bounded inside that machine by its accounts and permissions,
as it is on any install. A privilege that an operator gives the agent beyond what the
WeaverAgent system needs is the operator's choice, and it is outside this document.

## 2. What it is not

**Not a hypervisor driver.** Weaver Web never touches a hypervisor. It does not create,
clone, start, stop, snapshot, network or destroy a virtual machine. Those acts belong to
the operator and their hypervisor, and the suite holds no opinion about them. To start
and stop the agent is to load and unload it through admin, inside a running appliance.
The earlier direction, in which the console drove the lifecycle of a virtual machine, is
withdrawn.

**Not an installer.** Nothing is installed into an appliance after its image is sealed.
No site hook exists. A site condition, such as a limit that one operator sets on their
own cards, is a unit that the operator adds to their own machines over SSH. The image
never learns of it.

**Not a rule about deployment.** A machine given to one agent is the recommendation, and
`weaver-admin-PRD` section 7 states it with its reasons. The appliance is one half of
it, and a dedicated host is the other.

**Not a tutorial.** The entry condition is that the operator already runs a hypervisor
with GPU passthrough working. It is a condition. The suite does not teach it.

## 3. The image

### 3.1 Operating system and build

**The operating system is Ubuntu 26.04.**

**The binaries are compiled inside the image from named commits.** After the build, the
build tooling and the source trees are removed and the image is sealed. Two reasons
hold. The image then states by commit what code it carries. The binaries are built
against the libraries they run with. The build, its cleanup list, its probes, its seal
and its proof are `template-clone-Spec` section 10's, and this document restates none of
them.

### 3.2 What a commit names

**A commit does not yet name every byte of an image.** Repeated builds from the same
commits give byte-identical files, with two exceptions: the SPU and the CUDA engine
library, which are the two files that the CUDA compiler produces. A measurement on
2026-11-14 found two causes. The compiler puts its own process id into the names of its
temporary files, and those names reach the symbol tables of both files. The compiler
also emits intermediate code that differs from run to run, in 9 of the 264 intermediate
code images of the engine library. The compiled kernels are identical in every build,
for every card architecture that the library targets. So the code that a targeted card
runs does not differ between builds.

**Until the build changes, the identity of an image is its own manifest and that
manifest's hash**, as `template-clone-Spec` section 10 forms them
(`/etc/weaver-appliance/MANIFEST`), and a rebuild from the same commits does not confirm
an image. Section 10 carries the election that would close this.

### 3.3 Where the image states its facts

**The image states the commits it was compiled from, where a reader can find them
without the build log.** They are the keys `weaveragent.commit` and `weaverweb.commit`,
each with its origin, in the stamp `/etc/weaver-appliance/stamp`, which lies inside the
manifest's scope. The third and fourth builds of 2026-11-14 read both commits back from
the stamp.

**The image states its driver in the same stamp**: each driver package with its version
and the pocket and component it came from, and the module's version, vermagic and
signer, per `template-clone-Spec` section 10.

**The image's own version has no representation yet.** It waits on the version scheme,
an open ruling of section 10. WeaverAgent also records, for each released version of the
image, the commit that the version was compiled from. How it records that is the other
open ruling of section 10 that bears on this section.

**The stack's install record holds build facts only**, per #559 and `deploy/REDEPLOY.md`
section 3. An image built with no card therefore states no driver and no card in
`/etc/weaver/stack/install-tuple`, and `verify-load.sh` reads the driver, `libcuda` and
the cards at each load and stores none of them. One line of that record cannot be stated
on this image as built, and section 11 carries it.

### 3.4 The driver

**The NVIDIA driver is the distribution's own prebuilt and signed kernel module of the
615 branch**, with the kernel held to match it. No compiler runs near the driver, and
the module loads under Secure Boot. No build has measured that load yet, and section 11
carries the cell. The driver version is a member of the deployment tuple, and the image
states it, per section 3.3.

The build's one route, `ubuntu-prebuilt` in `template-clone-Spec` section 10, takes
Ubuntu's own branch: its module is prebuilt and signed by the distribution for the
image's one kernel, its userspace is at the same version, each package is named, and the
kernel packages are held. No DKMS, kernel source or compiler touches the driver.

### 3.5 Sealed

**Nothing changes an image after its seal.** Automatic package updates and firmware
updates are off. The build masks the update units, turns apt's periodic jobs off,
disables the package sources and removes `unattended-upgrades` and `snapd`, per
`template-clone-Spec` section 10. After the fourth build's first boot, both stanzas of
the package sources still read `Enabled: no`.

**No shared key or secret ships in the image.** An image key was once proposed and is
dropped. The pairing line and the console's fingerprint do its work without a secret
that every copy of the image carries. The seal removes the SSH host keys, the machine
identity, systemd's random seed and its credential secret, and its final probe looks for
each. The proof reads the sealed image again for the seed and the secret, per
`template-clone-Spec` section 10.

### 3.6 Versions

**One image exists for each version.** A security fix is a new version. Each version is
tested and evaluated before its release, which is one reason the appliance model was
chosen.

**Every published version stays published with its hash.** The deployment tuple of an
agent names its image version, and a replay needs that image.

**An upgrade of an appliance is a change of the deployment tuple and is recorded as
one.** State that was made under an older version is recorded as made under it and, from
the load onward, as used under the newer version. Which record carries the upgrade is a
cell of section 11.

### 3.7 Thin

**The image carries no model and no state.** Both live on disks that the operator
attaches, per section 6.

### 3.8 Formats

**One image comes first, for KVM**, as one sealed qcow2, per `template-clone-Spec`
section 10. Proxmox is out of scope for now, and it is the first format named as later.
`toddwbucy/WeaverTools` issue 29 records one Proxmox condition to settle then. Other
hypervisor formats are later and are named as later.

**The image fixes no machine size.** The build guest's size is `template-clone-Spec`
section 10's. The domain an appliance runs under, its virtual processors and memory
among them, is the operator's own.

## 4. Getting in

**SSH is the one way in, and no fallback exists.**

**The appliance listens on SSH alone**, on TCP port 22 through `ssh.socket`. No other
port is open unless a declaration names it, per section 7. The operator's rule: where a
port does not have to be open, it is not open. On the fourth build's first boot,
`ssh.socket` was enabled, `ssh.service` was disabled and ran once a connection came, and
`sshd-keygen.service` was masked, so cloud-init alone makes the host keys. The proof's
listener check, which fails on any socket bound beyond loopback but SSH on TCP 22, is
`template-clone-Spec` section 10's.

**First-boot settings come through cloud-init, and cloud-init is limited to three
settings:** the operator's SSH key, the network settings and the hostname. Cloud-init
runs no command and installs no package. Either act would make the sealed claim false at
the first boot. The image holds this limit itself and does not rely on a setting of the
hypervisor.

The representation is the build's, measured on the fourth build of 2026-11-14.

- User-data is not allowed and vendor-data is off. User-data is discarded before any
  module runs, so a seed cannot name modules back.
- The module lists hold `growpart`, `resizefs`, `set_hostname`, `update_hostname`,
  `update_etc_hosts`, `users_groups` and `ssh` at init, none at config, and
  `keys_to_console` at final.
- The seed gives the SSH key in its meta-data (`public-keys`, for the image's default
  account), the hostname in its meta-data (`local-hostname`) and the network in its
  `network-config`. A key given in user-data is not read.
- With no `network-config`, cloud-init falls back to DHCP on the first link.

`growpart` and `resizefs` take no setting from the seed. They grow the root filesystem
to the disk the operator gave. On the fourth build's first boot, a seed that asked for a
boot command, a run command, a package and an extra account had every ask discarded.

**The operator's key goes to the image's default account** (`ubuntu` on the builds of
2026-11-14). What that account may do is a cell of section 11.

**Cloud-init prints the SSH host key fingerprints on the console of the machine at first
boot, and that print is kept.** It gives the operator a value to compare at the first
SSH, per section 5.5. On the third and fourth builds' first boots, the printed block
named exactly the three keys that SSH presented, and SSH presented each key the block
named. One effect is measured and stated for operators: the guest's SSH server counts
each unauthenticated connection against its source, so repeated `ssh-keyscan` runs
against a fresh appliance can come back short. One `ssh` is unaffected.

**A seed's network decides one wait at every boot.** On a network with no default route
or no resolver, every boot waits 120 s for the network before it goes on (the fourth
build's first proof run). Section 10 carries that wait as an open ruling.

## 5. Pairing: the environment's half

The console's half of pairing is held elsewhere, per section 9. This section states what
the appliance does.

### 5.1 No listening port but SSH

**The console begins pairing, and the appliance makes the connection.** The appliance
never listens for the console. Its connectors dial the link port of the console. The
console is a network service, and to listen is its work. **The appliance never reaches
the internet**: the console fetches models, and the appliance only dials out.

### 5.2 The pairing line

**The console shows one line for the operator to run in the appliance over SSH.** The
line carries the address of the console, a one-time secret and the certificate
fingerprint of the console. The appliance dials the link port, checks the fingerprint,
presents the secret, hands over the public halves of keys that it made itself, and
receives its certificates. From then on every connection between the two is mutual TLS,
and the secret is spent.

### 5.3 Keys made inside the machine

**Each connector makes its own private key inside the appliance, and the key never
leaves it.** The console signs the public half. Each appliance is meant to hold its own
certificate. A machine cloned after enrollment breaks this, and the console's answer to
a second live holder is held elsewhere, per section 9.

### 5.4 The one exception

**The one-time secret is the only secret that travels on a command line.** It is the
exception because no other way to begin exists. Three bounds hold it. It is single-use.
It is short-lived, on the order of ten minutes. It is bound to one pending enrollment
that the operator opened, so it cannot enroll an appliance that the operator did not
expect. No other secret travels on a command line. Its exact lifetime and its form are
the console's, per section 9.

### 5.5 Trust on first use

**The first SSH of the operator into an appliance trusts the host key that it meets.** A
wrong machine at the given address could be enrolled. The operator compares the host key
with the fingerprints that cloud-init printed at first boot, per section 4.

## 6. Storage

**Two mount points are fixed: `/mnt/weaver/model` and `/mnt/weaver/state`.**

**The state disk is always required.** Every agent writes a trace. The state disk holds
the trace, the state store and the save points.

**The operator attaches the disks in the hypervisor and mounts them over SSH.** The
WeaverAgent system only checks and reports. It checks that each mount point is a
separate filesystem, and it reports the sizes to the console. Where storage is not
sufficient, the console says so and asks the operator to add storage.

**The check runs at every load, and admin makes it**, per `weaver-admin-PRD` section
4.1, which states the requirement and its reason. Whether the model resolves stays the
SPU's to answer at admission, per `weaver-admin-PRD` section 4.3, so the check judges
the filesystem and not the model on it.

**State that this version cannot read refuses the load, and the refusal says why**, per
`weaver-admin-PRD` section 4.1. For save points the representation stands already: the
inventory of `weaver-admin-Spec` section 4 refuses a save point whose stamp it cannot
judge, `ConfigInvalid` naming it, and the state member refuses the enter where the
schema a save point stands under is not the one it runs, before `load` is authored.

Four questions of this section stay open. How the state mount meets the agent's
territory is an open ruling of section 10. What the image lays at the two paths, the
form of the check's report with the surface it crosses, and the sizes that count as
sufficient are cells of section 11.

## 7. The firewall

**The firewall mechanism is nftables**, the operator's substrate ruling. Admin owns one
named nftables table and writes only that table. It never owns the whole firewall. At a
load admin opens in that table the ports the declaration names, and at the unload it
closes them. `weaver-admin-PRD` section 2 states the requirement and its reason.

**A declaration that names a tool or exposes the gate names the ports.** **The open
ports and the rules of the table are recorded with the run**, on the run card, as a
condition of the arrangement. **The gate is a door that the declaration names and admin
opens at load.** It is not a surface that the image exposes.

**One property of the mechanism binds the image.** In nftables an accept in one table
does not override a drop in another table. So the table of admin opens a port only where
nothing else on the machine drops it. On an appliance the default policy of the image
must therefore sit where the table of admin can open a declared port. Which table holds
that policy is an open ruling, carried in `weaver-admin-PRD` section 10 because one of
its shapes gives admin part of a table it does not own. On a machine where the operator
runs other rules, those rules must also allow the port, and that is the operator's act.

**The image as built ships no firewall table of its own.** The builds' proof boots run
the guest on a restricted network because the sealed image carries no guard, per
`template-clone-Spec` section 10. A guard that flushed the whole ruleset would also
remove admin's table. Whether the image bounds its outbound traffic is the egress rule,
an open ruling of section 10.

The name of the table, the shape of its rules, how a declaration names ports and the
member of the run record that carries them are representations no ruling or measurement
fixes yet. `weaver-admin-PRD` section 11 names the documents that settle them.

## 8. Behaviour

**After a reboot the connectors come up and the console sees the appliance. The agent
stays unloaded until the console loads it.** No autostart exists, per `weaver-admin-PRD`
section 7. The connectors' units are WeaverWeb's, and the build lays none of them, per
`template-clone-Spec` section 10.

**Load and unload are the verbs of admin, as built**, per `weaver-admin-PRD` section 4.
The console asks for them through the admin connector.

**An upgrade is recorded as section 3.6 states.**

## 9. Held elsewhere

Three bodies of the appliance charter of 2026-11-14 are not this document's. Each is
named here and cited from where it lives.

- **The console's side.** Enrollment's console half holds the link port and its default,
  the one-time secret's exact lifetime and form, and the console's answer to a second
  live holder of a certificate it issued. The rest is the model store and its sources,
  the image download with its hash check, and the display of the storage report. They
  are `weaver-web-PRD`'s and its Spec's, in `toddwbucy/WeaverWeb`.
- **The evidence, and the measurements still owed** before a Spec relies on the claim of
  section 1. They stay with the experiments of `toddwbucy/WeaverTools` and its issue 28,
  which hold the measured counts, what they rest on, what they do not cover, the cells
  still to run, and the falsifier.
- **The build and its proof.** They are `template-clone-Spec` section 10's, in
  `toddwbucy/WeaverTools`.

The asks the charter made of WeaverAgent are carried by `weaver-admin-PRD` sections 10
and 11. Its asks of WeaverWeb and of WeaverTools are those repositories' own.

## 10. Open rulings

These are the operator's to rule. Each states the proposal on the table, where one
exists.

1. **How WeaverAgent records the commit of each released version.** Proposed by the
   WeaverAgent Executor on 2026-11-14: one annotated tag for each version, on the commit
   that the version was compiled from, pushed by the operator. The tag message carries
   the version, the digest of the install record of the image, and the test count.
2. **The version scheme of the image.**
3. **Whether the image is built to be reproducible from its commits.** Two build
   settings, measured together on 2026-11-14, make every shipped file byte-identical
   across builds. The first is a compiler flag that fixes the names of the temporary
   files. The second is a list of card architectures that carries compiled kernels only
   and no intermediate code. The cost of the second: a card whose architecture is not in
   the list has no fallback and cannot run the engine. The list that was measured holds
   the architectures sm_86, sm_89, sm_120 and sm_121. Proposed: adopt both settings for
   the image, because section 3.6 and the first cell of section 11 both need a rebuild
   that can be compared with its parent. The election also sets the architectures the
   first image is compiled for. Further architectures are later, and named as later.
4. **The time source.** The appliance never reaches the internet, per section 5.1. The
   stock time service of the operating system points at a public time pool, and the
   builds left it there. Which source an appliance takes its time from is not ruled.
5. **The boot wait.** On a network with no default route or no resolver, every boot
   waits 120 s, per section 4. Whether the image keeps that wait is not ruled.
6. **The egress rule.** The image ships no guard on its outbound traffic, per section 7.
   Whether it bounds that traffic, and where the bound would live beside admin's one
   table, is not ruled.
7. **Which table holds the default policy on an appliance.** Carried in
   `weaver-admin-PRD` section 10, per section 7.
8. **How the state mount meets the agent's territory**, found by this act. The state
   disk is mounted at `/mnt/weaver/state` (section 6, the operator's ruling of
   2026-11-14). The whole agent lives in its territory,
   `/var/lib/weaver-agent/weaver-<agent>/` (the operator's ruling of 2026-10-07 on #1),
   and admin refuses a territory reached through a link above it, per
   `weaver-admin-Spec` section 9. The two rulings meet at the trace, the state store and
   the save points, and no ruling says how.

## 11. Open cells, each with what settles it

- **Build once, respin many.** If every security respin recompiles, every respin changes
  the binary hashes, and a patch cannot be told from a change of behaviour. The
  candidate rule is to build once for each release and to carry the same binaries into
  its respins. The operating system layer of a respin is not inert by construction: a
  fix to the C library replaces the math library, and a fix to the kernel brings a new
  build of the driver module. Settled by one respin: compare the binaries by digest and
  the output by the probe set.
- **The CUDA objects across builds.** Section 3.2 states what was measured. One cause
  stays open: why the intermediate code differs from run to run where it is kept. No
  setting removed it. It matters only if the election of section 10, item 3, keeps that
  code.
- **The CCCL version in the install record.** `deploy/install-tuple.sh` reads it from
  the Arch package manager alone, so an Ubuntu image records `cccl = unknown`. Settled
  by a WeaverAgent act that reads it from the package system the box has.
- **What the default account may do.** Cloud-init's documented behaviour gives the
  default account a sudoers rule, which the build first measures at the fifth build. The
  operator mounts the disks and runs the pairing line through that account. Settled by
  the fifth build's first boot, and by the operator's ruling where the measured rule
  reaches further than those acts need.
- **What the image lays at the two mount points.** `template-clone-Spec` section 10
  names the data disks' mount points a later act. Settled by that act.
- **The form of the storage report, and the surface it crosses to the console.** Settled
  by the act that adds the check, per `weaver-admin-PRD` section 11.
- **The sizes that count as sufficient.** No run has measured what an agent writes to
  the state disk. Settled by that measurement.
- **The record of an upgrade.** Which record carries an upgrade, and how state made
  under an older version is marked as used under the newer one. Settled by the act that
  writes it, per `weaver-admin-PRD` section 11.
- **A stopped service against a machine that is down.** What the appliance exposes so
  that the operator can tell the two apart. Settled when the first stop is run from the
  console.
- **The driver module under Secure Boot.** Section 3.4 requires the module to load under
  Secure Boot, and no build has measured it: the builds of 2026-11-14 ran with no card.
  Settled by a boot of the sealed image under Secure Boot with the module loaded.
- **Hypervisor formats after KVM.** Later, and named as later.
