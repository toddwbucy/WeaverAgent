# weaver-spu Spec: device pair amendment

**Version:** 0.1
**Date filed:** 2026-11-08
**Document ID:** `weaver-spu-device-pair-Spec-amendment`
**Parent:** `weaver-spu-PRD`
**Editorial:** Per the Working Rules.

## 0. Authority and scope

**PAIR-0.** This amendment fixes how the SPU divides one model across a set of
more than one device. The operator ruled its content on 2026-11-08. The ground
is reproducibility: a different division of layers is a different numerical
path, so the division must not depend on what a device holds at load time.

Amend SPU Spec sections 1.2, 3, 4.1 and 10 as mapped below.
The grounds are SPU PRD sections 4.1, 7 and 13.6.
These clauses authorize code after merge. They do not claim that the mechanic
exists at the source baseline.

| Clause | Destination | Addition |
| --- | --- | --- |
| PAIR-1 | Spec 3 | Device numbering by PCI bus, and refusal of a renumbered device space |
| PAIR-2 | Spec 3 | The members of a multi-device set must be identical |
| PAIR-3 | Spec 4.1 | A deterministic even layer split for the GGUF backend |
| PAIR-4 | Spec 3 | Room judged per device from that device's actual share |
| PAIR-5 | Spec 1.2 and 10 | Instruments, including the observed division |
| PAIR-6 | Spec 10 | Open cells: row and tensor split |

The baseline is main at `9d76e1765af25084cb77e3b4bb38bc20c9211d46`.
The dependency pin is `toddwbucy/llama-cpp-rs` at
`ecce255bcb14dd6d88f184cc8776c23a85afafeb`, as recorded by Cargo.lock.
The vendored llama.cpp in that pin is the engine every engine citation names.

**What this amendment does not do.** It adds no declaration key and does not
change the binding grammar. `ModelBinding` stays `{artifact, devices}`
(`weaver-types` `config.rs`). It adds no interleaved placement: on two devices
an interleave only adds crossings, and a clean one needs an engine change
(section 6 of the source survey). It builds no row or tensor split (PAIR-6).
It changes no production measurement or trace member. The division is a pure
function of the artifact and the ordered device set, and both are already in
the record, so a replay can derive the division without a new member.

The source survey is the hub report "SPU device placement: step-0 survey"
of 2026-11-08. It is evidence for this document. It is not normative.

## 1. Device numbering

**PAIR-1.** A device ordinal in a binding means the device at that index in
PCI bus order. That is the order the operator's device tools list.

Each SPU binary sets `CUDA_DEVICE_ORDER=PCI_BUS_ID` in its own process
environment at entry, before any driver, runtime or backend call. This applies
to `weaver-spu`, `weaver-spu-classify` and every other binary the crate builds
that can open a device. An inherited value is replaced, not trusted. Without
this setting, the driver numbers devices fastest first, which need not match
bus order. On a box with mixed devices, a binding ordinal could then name a
device the operator did not choose.

Admin and the harness do not change. Admin's start step clears the environment
(`weaver-admin` `start.rs`), and the harness passes the worker's environment to
the SPU unchanged (`weaver-harness` `spawn.rs`). The SPU's own entry is the one
place that controls the value for every launch path.

The agent does not set `CUDA_VISIBLE_DEVICES`. If the SPU's environment carries
that variable at entry, admission refuses before any device query. A visible
set renumbers ordinals, so the binding's ordinals would no longer name the
devices the operator listed. The refusal names the variable and its value.

## 2. Identical members

**PAIR-2.** When a binding names more than one device, every member must be
identical in these three facts, read from the driver:

| Fact | Source |
| --- | --- |
| Device name | The driver's device name string |
| Compute capability | Major and minor version |
| Total memory | The driver's total-memory figure, in bytes |

Admission judges this after `judge_distinct` and before the room judgment.
It costs one driver query per device, the same queries the room judgment
already makes. A set that differs in any fact refuses as `DeviceCannotAdmit`.
The account names each device's ordinal and its three facts, so the operator
sees which member differs.

The ground is reproducibility. The engine chooses kernels and tile sizes per
architecture. A model split across two architectures runs part of its layers
on a different arithmetic path. Two devices of one model that differ in total
memory (for example, by error-correction mode) are also refused: a different
configuration is not treated as the same device.

This judgment applies to every backend, because admission is shared. A single
device binding is unaffected. Peer reachability and width (Spec 3 and 5) are
unchanged and still apply.

## 3. The even layer split

**PAIR-3.** For a binding with more than one device, the GGUF backend loads the
model with layer split mode and an explicit tensor split computed by this rule.
Never leave the tensor split unset. Unset, the engine divides by each device's
free memory at load (`llama-model.cpp`, the default split), which can move from
run to run.

Let L be the artifact's repeating layer count, read from its header, and let
N be the number of devices. Assign contiguous blocks of repeating layers in
binding order. Each device takes either floor(L/N) or ceil(L/N) layers. The
first L mod N devices take the larger count. The engine always places the
output layer on the last device. The input layer (`token_embd`) stays in host
memory under every split, because the engine never offloads it.

For Granite 4.2 30B (L = 64, N = 2) the division is 32 and 32, with the output
layer on the second device.

The engine maps a tensor split to layers by cumulative fraction over L + 1
slots, the output layer counted as slot L. A proportional split such as
`[0.5, 0.5]` therefore gives 33 and 31 for L = 64, not 32 and 32. The
implementation converts the counts to split values that fall strictly inside
each boundary interval, never on its edge. For device i with cumulative count
C(i), the boundary value is C(i) - 0.5. The implementation must show by test
that the values reproduce the counts under the engine's own mapping. It must
not assume it.

The binding order fixes which device takes the first block. The pinned
`llama-cpp-2` exposes no tensor split setter. Upstream `utilityai/llama-cpp-rs`
commit `e7af607ebd5ec2932709ef918c17cca2f9b0fce4` adds `with_tensor_split`. It
copies the values into storage owned by the parameters and zero-pads to the
engine's maximum device count. Cherry-pick that commit onto the fork at the
current pin, so the vendored llama.cpp does not move. A rebase onto upstream
moves the engine pin, which requalifies the LORA-3 observer and every engine
citation here, and is out of scope. An empty slice restores the free-memory
default in that setter, so the SPU never passes one for a multi-device set.
No other fork change is needed. `with_main_gpu` has no effect in layer split mode; the code
may keep or drop the call, but no comment may claim it selects a device.

This rule binds the GGUF backend only. The native (candle) pair keeps its own
tensor-parallel design (Spec 4.1, `native_pair.rs`).

## 4. Room per device

**PAIR-4.** Admission judges each device against its own share, not an even
fraction of the file. Replace the even shard (`residency.rs`, file length
divided by device count) with a share computed from the header's tensor
directory, which admission already reads at no device cost:

| Term | Content |
| --- | --- |
| Layer weights | The bytes of every `blk.N.*` tensor for the layers PAIR-3 gives the device |
| Output | The output and output-norm tensors, on the last device only |
| Adapter | Under the LoRA fixture, the adapter tensors whose base layer is on the device |
| Headroom | The existing staged headroom parameter, unchanged |

The input layer is not charged to any device. It is host memory, and the SPU
makes no host-memory judgment. The fixture report records its size.

The KV cache stays inside the headroom term, as today. Charging it per layer is
a later measurement, not a zero cost. Room is still judged against the driver's
current free figure, and the room refusal still carries the device's capacity.

## 5. Instruments and evidence

**PAIR-5.** The implementation supplies these instruments.

| Claim | Instrument | Deliberate change that must fail |
| --- | --- | --- |
| Bus numbering is set before any device call | A test that reads the SPU's entry order, plus a run whose inherited value differs | Set the variable after a driver call, or keep an inherited value |
| A visible set refuses | Admission with `CUDA_VISIBLE_DEVICES` present | Admit anyway |
| Mixed members refuse | A recording device query returning two different names, capabilities or totals | Judge only the name, or skip the judgment for pairs |
| The split is deterministic | Hardware-free arithmetic over L and N, including odd L and N = 3, checked against the engine's cumulative mapping | Pass proportional halves, or put a value on a boundary edge |
| The split reaches the engine | The parameters built for a pair carry the computed tensor split | Leave the split unset |
| Room is per device | A directory fixture with unequal layer sizes and an output tensor | Judge the even file fraction |
| The observed division matches | On a device pair, under the LORA-3 log capture: every `load_tensors: layer N assigned to device` record and every KV `layer N: dev =` record matches PAIR-3 | Accept a missing record or a layer on the wrong device |

The observed-division instrument is an explicit accelerator test, like the
LORA-7 fixture. It runs only in an isolated process with the LORA-3 observer.
The production path does not install a log callback. A missing device or
artifact fails the test by name and is not a pass.

The existing two-device tests (`tests/two_card.rs`) rest on the free-memory
default. They change to the even split, and their room floors are restated
against the per-device share.

The fixture report of LORA-6 gains three facts: the computed division, the
observed division records, and the interconnect between the members (for
example, NVLink with its link count, or PCIe). The interconnect is evidence
only. Admission does not judge it.

The LORA-3 observer matches each adapter target's buffer to the device PAIR-3
gives that target's layer, as amended by #478. This amendment supplies the
declared division that clause waited for.

## 6. Open cells

**PAIR-6.** These remain open. They do not authorize substitutes.

| Cell | What closes it |
| --- | --- |
| Row split | A fork change so the split buffer reads the model's device list, not every visible device by ordinal; then a GPU proof with an adapter whose rank is below the engine's row rounding |
| Tensor split | A measurement on hardware that represents the targeted deployments, and the operator's ruling |
| KV cache in the room judgment | A per-layer measurement of the cache on each device |
| Host memory for the input layer | A host-room election, if the operator wants one |

Row and tensor split depend on a fast interconnect, because they synchronize
every layer. The targeted workstation devices have no such link, so a result on
a box that has one does not represent them.

## 7. Source index

Read at the baseline in section 0. These locate the work and are not claims
that the mechanic exists.

| Source | Relevant fact |
| --- | --- |
| `decoder/gguf.rs::ResidentModel::load` | Sets layer mode and the device list; sets no tensor split |
| `residency.rs`, the admission room step | Even file fraction per device, plus the whole adapter on each |
| `gpu/mod.rs::room_and_reach` | One needed figure for every device; peer access checked |
| `lora/observation.rs::judge` | Accepts a buffer on any device of the set |
| `tests/two_card.rs` | Two-device admission and serving under the free-memory default |
| `weaver-admin` `start.rs` and `weaver-harness` `spawn.rs` | Environment cleared by admin, passed unchanged to the SPU |
| Pinned `llama-model.cpp`, the split step | Free-memory default, cumulative fractions over L + 1 slots, input layer on the host |
| Pinned `llama-kv-cache.cpp` | The KV cache follows the layer's assigned device |
| Pinned `llama.cpp`, device list | Layer mode is confined to the listed devices |
| Pinned `llama-cpp-2` `params.rs` | No tensor split setter |
| Upstream `llama-cpp-rs` `e7af607` | The `with_tensor_split` setter to cherry-pick |
