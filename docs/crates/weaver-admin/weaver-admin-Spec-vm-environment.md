# weaver-admin - Spec: the VM environment

**Status:** DRAFT. Not merged. It authorizes no code until the operator merges it.
**Date filed:** 2026-11-15
**Document ID:** `weaver-admin-Spec-vm-environment`
**Parent:** `weaver-admin-PRD`
**Editorial:** Per the Working Rules.

---

## 0. What this document is

The specification of how the environment of `weaver-admin-PRD` sections 7.1 to 7.5 is
represented, with the firewall table of that charter's section 2 and the checks of its
section 4.1: the environment a WeaverAgent runs in when it runs in a virtual machine
made from the image of that charter's section 7.2. On the operator's ruling of
2026-11-15 it is a child of `weaver-admin-PRD`.

Level discipline. The charter carries each requirement and its reason. This document
elects how each is represented, against the ground the charter's section states, and
cites that section. Where a ruling or a measurement fixes a representation, this
document states it. Where neither does, the cell stays open and names what settles it.
This document states no requirement of its own and invents no value.

The terms of `weaver-admin-PRD` section 7.1 hold here: the image, an appliance, the
console, enrollment, the WeaverAgent system and the agent.

**Most of what represents the image is its build**, `template-clone-Spec` section 10 in
`toddwbucy/WeaverTools`. That text stands on the repository's pull request 31 and is not
on its main as of 2026-11-15. This document cites it as "the build (WeaverTools pull
request 31)" until it merges, and restates none of it.

**A measurement here names the build that took it.** Four builds ran on 2026-11-14 and a
fifth on 2026-11-15. Their records are kept with the experiments, and the result notes
of the fourth and the fifth are owed to `toddwbucy/WeaverTools`. A sentence that says
observed names its build. A sentence that says unmeasured means no build has recorded
it.

## 1. The image

Per `weaver-admin-PRD` section 7.2.

### 1.1 Operating system and build

The build takes the Ubuntu 26.04.1 cloud image by its digest, compiles the binaries in
its own build guest from two commits named in full (`WA_COMMIT` and `WW_COMMIT`),
cleans the guest by a checked list and seals it (WeaverTools pull request 31). Nothing
built on the host enters the image.

### 1.2 Identity

The build writes the manifest at `/etc/weaver-appliance/MANIFEST`, over `/usr`, `/etc`,
`/opt`, `/boot` and `/boot/efi`, each regular file by its sha256 and each symbolic link
by its target. It names the image's identity the manifest's sha256, and it takes the
sealed disk's own sha256 at the seal (WeaverTools pull request 31). Which of the two is
the hash the charter's section 7.2 means, and which is published with each version, is
a cell of section 10.

**What a commit does not name.** Repeated builds from the same commits give
byte-identical files, with two exceptions: the SPU and the CUDA engine library, which
are the two files that the CUDA compiler produces. A measurement on 2026-11-14,
recorded on `toddwbucy/WeaverTools` issue 28, found two causes. The compiler puts its
own process id into the names of its temporary files, and those names reach the symbol
tables of both files. The compiler also emits intermediate code that differs from run to
run, in 9 of the 264 intermediate code images of the engine library. The compiled
kernels were identical in every build, for every card architecture that the library
targets. Observed again on the fifth build: two guests built from one commit agreed on
every member and library digest of the install record but those two files. Section 9
carries the election that would close this.

### 1.3 Where the image states its facts

**The commits are in the stamp**, `/etc/weaver-appliance/stamp`, as `weaveragent.commit`
and `weaverweb.commit` with their origins, inside the manifest's scope. The seal refuses
a stamp that does not name both commits, each equal to its input (WeaverTools pull
request 31). Observed on the third and fourth builds: the proof read both commits back
from the stamp of the booted image.

**The driver is in the same stamp**: each driver package with its version and the pocket
and component it came from, and the module's version, vermagic and signer (WeaverTools
pull request 31).

**The image's own version has no representation as of 2026-11-15.** It waits on the
version scheme, open ruling 2 of section 9. How WeaverAgent records the commit of each
released version is open ruling 1.

**The stack's install record holds build facts only**, per #559 and `deploy/REDEPLOY.md`
section 3. An image built with no card therefore states no driver and no card in
`/etc/weaver/stack/install-tuple`, and `verify-load.sh` reads the driver, `libcuda` and
the cards at each load and stores none of them. Observed on the fifth build: the record
held build facts only, and its digest equals the stamp's `stack.install-tuple`. Its
`cccl` line read `unknown`, and section 10 carries that cell.

### 1.4 The driver

The build's one route is `ubuntu-prebuilt`: Ubuntu's own 615 branch, its module
prebuilt and signed by the distribution for the image's one kernel, its userspace at the
same version, each package named, and every image and module package for that kernel
held. No DKMS, kernel source or compiler touches the driver (WeaverTools pull request
31). Observed on the fifth build: the four packages for the kernel read held, and one
kernel tree stood. The module's load under Secure Boot is unmeasured, because every
build ran with no card, and section 10 carries it.

### 1.5 Sealed

**Updates.** The build masks the update units, `fwupd-refresh.timer` and its service
among them, turns apt's periodic jobs off, disables the package sources, and removes
`unattended-upgrades` and `snapd`. Its probes refuse the seal where any of those stands
(WeaverTools pull request 31). `fwupd` itself stays installed, with its refresh masked.
Observed on the fourth build's first boot: both stanzas of the package sources still
read `Enabled: no`.

**No key or secret.** The seal removes the SSH host keys, the machine identity,
systemd's random seed and its credential secret, and its final probe looks for each. The
proof then reads the sealed image without booting it for the seed, the secret and any
non-empty log (WeaverTools pull request 31). Observed on the fifth build: the seed and
the secret were absent, and no file under `/var/log` held anything.

### 1.6 Versions

No representation stands as of 2026-11-15. How an upgrade is recorded, and whether a
respin recompiles, are cells of section 10.

### 1.7 Formats

The build makes one sealed qcow2, for KVM (WeaverTools pull request 31). The size of
the build guest is the build's, and the domain an appliance runs under, its virtual
processors and memory among them, is the operator's. The thread counts of the build and
of the sealed image, formats after KVM and where images are published are cells of
section 10.

## 2. Getting in

Per `weaver-admin-PRD` section 7.3.

**SSH listens on TCP port 22 through `ssh.socket`.** At the seal SSH is enabled with no
host key and no account, and `sshd-keygen.service` is masked, so cloud-init alone makes
the host keys at first boot (WeaverTools pull request 31). Observed on the fourth
build's first boot: `ssh.socket` was enabled, `ssh.service` was disabled and ran once a
connection came, and `sshd-keygen.service` was masked. The proof's listener check fails
on any socket bound beyond loopback but SSH on TCP 22. Observed on the fifth build: of
eight listening sockets, the only one beyond loopback was sshd on port 22.

**Cloud-init's limit.** The build writes
`/etc/cloud/cloud.cfg.d/99-weaver-appliance.cfg` (WeaverTools pull request 31):

- User-data is not allowed and vendor-data is off. User-data is discarded before any
  module runs, so a seed cannot name modules back.
- The module lists hold `growpart`, `resizefs`, `set_hostname`, `update_hostname`,
  `update_etc_hosts`, `users_groups` and `ssh` at init, none at config, and
  `keys_to_console` at final. `growpart` and `resizefs` take no setting from the seed
  and grow the root filesystem to the disk the operator gave.
- The seed gives the SSH key in its meta-data (`public-keys`, for the image's default
  account), the hostname in its meta-data (`local-hostname`) and the network in its
  `network-config`. A key given in user-data is not read.

Observed on the fourth build's first boot: cloud-init discarded the user-data, ran no
module outside those lists, and carried out none of the seed's asks for a boot command,
a run command, a package and an extra account. With no `network-config`, cloud-init's
documented fallback is DHCP on the first link, and that is unmeasured: every build's
seed named a static address.

**The receiving account.** The seed's key goes to the image's default account, `ubuntu`
on every build. What that account may do is a cell of section 10.

**The fingerprints.** `keys_to_console` prints the SSH host key fingerprints on the
machine's serial console at first boot. Observed on the third and fourth builds' first
boots: the printed block named exactly the three keys that SSH presented, and SSH
presented each key the block named. Observed on the fourth build's later proof runs:
the guest's SSH server counts each unauthenticated connection against its source, so
repeated `ssh-keyscan` runs against a fresh appliance came back short. One `ssh` is
unaffected.

**The boot wait.** netplan waits for a routable network with a resolver
(`systemd-networkd-wait-online --any --dns -o routable`). Observed once, on the fourth
build's first proof run: on a network with no default route or no resolver, the boot
waited 120 s for the network before it went on. That every such boot waits follows from
the setting and is unmeasured beyond that boot. Open ruling 5 of section 9 carries the
wait.

## 3. Pairing and models: the environment's half

Per `weaver-admin-PRD` section 7.4.

**The appliance listens for nothing but SSH**, by section 2's listener check. The image
configures no network of its own: no netplan file, no networkd or NetworkManager
configuration and no resolver address, and the initramfs's catch-all network is masked,
so the first boot's seed alone gives it a network (WeaverTools pull request 31).
Observed on the fifth build: a boot with no seed sent no frame in 180 s. Whether the
image bounds its outbound traffic past that is open ruling 6 of section 9, and the time
source is open ruling 4.

**The image ships no connector key, configuration or unit**, and its probes refuse the
seal on any (WeaverTools pull request 31). The connectors' keys, the pairing line with
its secret, the access tokens and the model's pull are WeaverWeb's to represent, per
section 8. Nothing of the image represents them as of 2026-11-15.

**Trust on first use** rests on section 2's fingerprints.

## 4. Storage

Per `weaver-admin-PRD` sections 7.5 and 4.1.

**The two mount points are `/mnt/weaver/model` and `/mnt/weaver/state`**, fixed by the
operator's ruling of 2026-11-14.

**The check is made in the load**, between load steps 3 and 4 as the charter's section
4.1 places it, and `validate` runs it with the rest of the inventory, per section 4.3 of
the charter. How admin knows it runs on an appliance, the check's representation, the
form of its report with the surface the report crosses, and the sizes that count as
sufficient are cells of section 10. Whether a failed check refuses the load is open in
the charter's section 10.

**The refusal of state this version cannot read stands for save points.** The inventory
of `weaver-admin-Spec` section 4 judges a save point's stamp, its check and its digest
and refuses `ConfigInvalid` naming the save point, and the member answers the enter's
`restored` ask with a refusal naming a schema mismatch, so the harness refuses the enter
before it authors `load`, per `weaver-state-Spec` section 3.

How the state mount meets the agent's territory is open ruling 8 of section 9. What the
image lays at the two paths is a cell of section 10: the build names the data disks'
mount points a later act (WeaverTools pull request 31).

## 5. The firewall

Per `weaver-admin-PRD` section 2.

**The mechanism is nftables**, the operator's substrate ruling of 2026-11-14. The name
of admin's table, the shape of its rules, how a declaration names ports, and the member
of the `load` event that records them are fixed by no ruling or measurement as of
2026-11-15, and `weaver-admin-PRD` section 11 names the documents that settle them.
Which table holds the default policy on an appliance is open in that charter's section
10.

**The image as built.** The build says the sealed image carries no guard of its own, and
for that reason it boots its proof's no-seed guest on a restricted network (WeaverTools
pull request 31). The second, third and fourth builds ran that boot unrestricted, and
the second reached public time servers. The fifth build ran it restricted. No listing
of the sealed image's nftables ruleset is recorded, so what the image ships in the
firewall is unmeasured.

## 6. Behaviour

Per `weaver-admin-PRD` sections 7 and 7.2.

**Nothing of the image starts the agent.** The build lays no agent configuration and no
connector unit, and its probes refuse the seal on either (WeaverTools pull request 31).
The connectors' units are WeaverWeb's. The console asks for a load or an unload through
admin-con, per `weaver-admin-operator-contract`.

**The record of an upgrade** is a cell of section 10.

## 7. What is enforced, and by which instrument

Per apex section 11. Each election above is listed with what catches its failure. Where
the only instrument is the image build's own proof, it is named as such: no test of this
repository reaches a sealed image.

- **The image's contents** (sections 1.1, 1.4 and 1.5, and the image's half of sections
  3 and 6): compilers, toolchains, sources and caches absent, update units masked and
  sources disabled, the kernel held, cloud-init limited, SSH enabled, no key, no
  connector configuration and no agent configured. The build's probes refuse the seal on
  any hit, in a pass before the seal's removals and a final pass after them. The only
  instrument is the image build's.
- **The first boot** (section 2): the fingerprints against the keys SSH presents, the
  user-data asks discarded, cloud-init done, the listener check, the stamp's two
  commits, and the manifest against the booted guest beyond the first boot's expected
  changes. The build's proof fails on any finding. The only instrument is the image
  build's proof. Observed on the fifth build: no proof refused, so its refusal side ran
  only in the build's offline mock.
- **What the seal's shutdown wrote** (section 1.5): the proof's offline read of the
  sealed image. The only instrument is the image build's proof.
- **No network without a seed** (section 3): the proof's no-seed boot, which fails on
  any frame the guest sends. The only instrument is the image build's proof.
- **The install record's build facts** (section 1.3): `deploy/test_plans.py`, whose
  install-tuple tests state their perturbations, and on the image the build's stack
  step, which refuses a record whose commit is not `WA_COMMIT`.
- **The refusal of a save point** (section 4): the workspace's save-point corpus
  (`crates/weaver-types/tests/fixtures/save-points/`), read by both crates' tests, per
  `weaver-admin-Spec` section 4.
- **Admin's table, the data-mount check and the storage report** (sections 4 and 5): no
  instrument exists as of 2026-11-15. The act that adds them owes walks and perturbation
  tests in the manner of `weaver-admin-Spec` section 10, before any code, per
  `weaver-admin-PRD` section 11.
- **No instrument at all**: the driver's load under Secure Boot, cloud-init's DHCP
  fallback, the time source and the sizes. Each is a cell of section 10 or an open
  ruling of section 9.

## 8. Held elsewhere

- **The console's side**, in `weaver-web-PRD` and its Spec in `toddwbucy/WeaverWeb`:
  that Weaver Web never touches a hypervisor, enrollment's console half with the link
  port and its default, the one-time secret's lifetime and form, the console's refusal
  of a second live holder of a certificate it issued with how it notices one and what it
  does with the appliance that connected second, the enrollment connection as the one
  unauthenticated surface of the console, the model store with its sources and its
  access tokens, the model's pull, the image download with its hash check, and the
  display of the storage report.
- **The evidence, and the measurements still owed** before a Spec relies on the claim of
  `weaver-admin-PRD` section 7.1. They stay with the experiments of
  `toddwbucy/WeaverTools` and its issue 28.
- **The build and its proof**, the build (WeaverTools pull request 31).

## 9. Open rulings

These are the operator's to rule. Each states the proposal on the table, where one
exists.

1. **How WeaverAgent records the commit of each released version.** Proposed by the
   WeaverAgent Executor: one annotated tag for each version, on the commit that the
   version was compiled from, pushed by the operator. The tag message carries the
   version, the digest of the install record of the image, and the test count.
2. **The version scheme of the image.**
3. **Whether the image is built to be reproducible from its commits.** Two build
   settings, measured together on 2026-11-14, make every shipped file byte-identical
   across builds. The first is a compiler flag that fixes the names of the temporary
   files. The second is a list of card architectures that carries compiled kernels only
   and no intermediate code. The cost of the second: a card whose architecture is not in
   the list has no fallback and cannot run the engine. The list that was measured holds
   the architectures sm_86, sm_89, sm_120 and sm_121. Proposed: adopt both settings for
   the image, because a version and the first cell of section 10 both need a rebuild
   that can be compared with its parent. The election also sets the architectures the
   first image is compiled for. Further architectures are later, and named as later.
4. **The time source.** The appliance never reaches the internet, per
   `weaver-admin-PRD` section 7.4. The build leaves chrony on Ubuntu's NTS pool as its
   servers. Which source an appliance takes its time from is not ruled.
5. **The boot wait.** On a network with no default route or no resolver, a boot waits
   120 s, per section 2. Whether the image keeps that wait is not ruled.
6. **The egress rule.** The image ships no guard on its outbound traffic, per sections 3
   and 5. Whether it bounds that traffic, and where the bound would live beside admin's
   one table, is not ruled. A guard that flushed the whole ruleset would also remove
   admin's table.
7. **Which table holds the default policy on an appliance.** Carried in
   `weaver-admin-PRD` section 10, per section 5.
8. **How the state mount meets the agent's territory.** The state disk is mounted at
   `/mnt/weaver/state` (section 4, the operator's ruling of 2026-11-14). The whole agent
   lives in its territory, `/var/lib/weaver-agent/weaver-<agent>/` (the operator's
   ruling of 2026-10-07 on #1), and admin refuses a territory reached through a link
   above it, per `weaver-admin-Spec` section 9. The two rulings meet at the trace, the
   state store and the save points, and no ruling says how.

## 10. Open cells, each with what settles it

- **Build once, respin many.** If every security respin recompiles, every respin changes
  the binary hashes, and a patch cannot be told from a change of behaviour. The
  candidate rule is to build once for each release and to carry the same binaries into
  its respins. The operating system layer of a respin is not inert by construction: a
  fix to the C library replaces the math library, and a fix to the kernel brings a new
  build of the driver module. Settled by one respin: compare the binaries by digest and
  the output by the probe set.
- **The CUDA objects across builds.** Section 1.2 states what was measured. One cause
  stays open: why the intermediate code differs from run to run where it is kept. No
  setting removed it. Settled by the election of section 9, item 3: one that keeps no
  intermediate code closes the cell, and one that keeps it leaves the cell to a
  measurement that finds the cause.
- **The CCCL version in the install record.** `deploy/install-tuple.sh` reads it from
  the Arch package manager alone, so an Ubuntu image records `cccl = unknown`. Settled
  by a WeaverAgent act that reads it from the package system the box has.
- **Which hash is published.** The build forms the manifest's sha256 and the sealed
  disk's, per section 1.2. Settled by the act that publishes the first version, which
  names the hash it publishes and the check the console's download makes against it.
- **The thread counts of the build and of the sealed image.** Settled by the build
  (WeaverTools pull request 31), which states the build's, and by the operator's ruling
  for the sealed image.
- **What the default account may do.** Observed on the fifth build's first boot:
  cloud-init added `/etc/sudoers.d/90-cloud-init-users` for the default account, as its
  documentation says. What that rule grants is unread. The operator mounts the disks and
  runs the pairing line through that account. Settled by a read of the rule on a first
  boot, and by the operator's ruling where it reaches further than those acts need.
- **What the image lays at the two mount points.** Settled by the build's act that adds
  the data disks' mount points (WeaverTools pull request 31).
- **How admin knows it runs on an appliance, and which mounts it checks.** Settled by
  the act that adds the check, per `weaver-admin-PRD` section 11.
- **The form of the storage report, and the surface it crosses to the console.** Settled
  by the act that adds the check, per `weaver-admin-PRD` section 11.
- **The sizes that count as sufficient.** No run has measured what an agent writes to
  the state disk. Settled by that measurement, which the act that adds the check owes
  before it states a size.
- **The record of an upgrade.** Which record carries an upgrade, and how state made
  under an older version is marked as used under the newer one. Settled by the act that
  writes it, per `weaver-admin-PRD` section 11.
- **A stopped service against a machine that is down.** What the appliance exposes so
  that the operator can tell the two apart. Settled when the first stop is run from the
  console.
- **The driver module under Secure Boot.** `weaver-admin-PRD` section 7.2 requires the
  module to load under Secure Boot, and every build ran with no card. Settled by a boot
  of the sealed image under Secure Boot with the module loaded.
- **Hypervisor formats after KVM, and where images are published for the console to
  find.** Settled by the operator's ruling that names the next format and the place.
