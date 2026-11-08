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

Amend SPU Spec sections 1.2, 3, 4.1 and 10, and `weaver-harness-Spec` section
2.2, as mapped below.
The grounds are SPU PRD sections 4.1, 7 and 13.6.
These clauses authorize code after merge. They do not claim that the mechanic
exists at the source baseline. Until the PAIR-1 to PAIR-4 code lands, a
multi-device load keeps today's behaviour: no visibility mask, no identity
judgment and the engine's free-memory split. A deployment that needs these
properties waits for that code.

| Clause | Destination | Addition |
| --- | --- | --- |
| PAIR-1 | Spec 3, and `weaver-harness-Spec` 2.2 | PCI bus numbering and a visibility mask set by the harness from the binding |
| PAIR-2 | Spec 3 | The members of a multi-device set must be identical |
| PAIR-3 | Spec 4.1 | A deterministic even layer split for the GGUF backend |
| PAIR-4 | Spec 3 | Room judged per device from its actual share, for GGUF |
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

## 1. Device numbering and visibility

**PAIR-1.** A device ordinal in a binding means the device at that index in
PCI bus order. That is the order the operator's device tools list. A device
outside the binding is not visible to the organ at all.

Naming devices to the engine does not keep other devices out of the process.
On 2026-11-08, a llama.cpp server given two of three devices (`--device`)
still held a context on the third. Only the visibility mask kept the third
device out. On a box that
runs two agents, each SPU would otherwise hold a context on the other's
device.

**The harness sets the environment when it launches a device organ.** When
the worker forks the SPU, it sets two variables in the child's environment,
replacing any inherited value:

| Variable | Value |
| --- | --- |
| `CUDA_DEVICE_ORDER` | `PCI_BUS_ID` |
| `CUDA_VISIBLE_DEVICES` | The decoder binding's ordinals, in binding order, comma-separated |

The worker holds the decoder binding before the fork, in the load payload's
SPU instruction, so no new channel is needed. The classify organ's launch does
the same with its single device. No other organ gets these variables set. The
harness reads only the ordinals, which it already reads for the trace mirror
and the classify launch. It interprets nothing else in the binding. This
amends `weaver-harness-Spec` section 2.2 and `spawn.rs`'s environment
construction.

**Inside the organ, ordinals are positions.** Under that mask, the process sees
exactly the binding's devices, renumbered from zero in binding order. The SPU
uses position i for the device at binding index i, in every driver and engine
call. Every refusal, account and evidence record names the binding's own
ordinal, never the position, so the operator reads the numbers the device tools
show.

**The SPU checks the mask at admission, before any device query.** If
`CUDA_DEVICE_ORDER` is not `PCI_BUS_ID`, or `CUDA_VISIBLE_DEVICES` is not exactly
the instruction's ordinals in order, written in decimal and comma-separated
with no spaces (`0,1`), admission refuses. The refusal names both
values. This catches a launch path that does not go through the harness, such
as a test or a fixture driver, so the SPU never judges or loads under a
numbering the binding does not mean.

Admin does not change. Its start step clears the environment, and the harness
sets these two variables after that. A kernel-level guarantee, denying the
agent's processes the device nodes outside its binding, is a separate act
(PAIR-6, #479).

## 2. Identical members

**PAIR-2.** When a binding names more than one device, every member must be
identical in these three facts, read from the driver:

| Fact | Source |
| --- | --- |
| Device name | The driver's device name string |
| Compute capability | Major and minor version |
| Total memory | The driver's total-memory figure, in bytes |

Admission judges this after `judge_distinct` and before the room judgment.
It costs a name query and a compute-capability query per device, beside the
total-memory figure the room judgment already reads. A set that differs in any fact refuses as `DeviceCannotAdmit`.
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
first L mod N devices take the larger count. Under the split values below,
the output layer (slot L) falls in the last device's interval, and the split
test shows it. The input layer (`token_embd`) stays in host
memory under every split, because the engine never offloads it.

For Granite 4.2 30B (L = 64, N = 2) the division is 32 and 32, with the output
layer on the second device.

The engine maps a tensor split to layers by cumulative fraction over L + 1
slots, the output layer counted as slot L. A proportional split such as
`[0.5, 0.5]` therefore gives 33 and 31 for L = 64, not 32 and 32. The
implementation converts the counts to split values whose boundaries fall
strictly inside each interval, never on its edge. Let C(i) be the cumulative
repeating-layer count through device i, over L + 1 slots. The cumulative
boundary for device i < N - 1 is C(i) - 0.5, and the last device's is L + 1.
Each device's split value is its boundary minus the previous one:
v(0) = C(0) - 0.5, v(i) = C(i) - C(i-1) for 0 < i < N - 1, and
v(N-1) = L + 1 - (C(N-2) - 0.5). For L = 64 and N = 2 this gives 31.5 and 33.5,
which the 2026-11-08 load check observed as 32 and 32 with the output on the
second device. The mapping holds only with every layer offloaded (n_gpu_layers
at least L + 1). A smaller value shifts the engine's first offloaded layer, and
the parameters set it so. The implementation must show by test that the values
reproduce the counts under the engine's own mapping. It must not assume it.

The binding order fixes which device takes the first block. The pinned
`llama-cpp-2` exposes no tensor split setter. Upstream `utilityai/llama-cpp-rs`
commit `e7af607ebd5ec2932709ef918c17cca2f9b0fce4` adds `with_tensor_split`. It
copies the values into storage owned by the parameters and zero-pads to the
engine's maximum device count. Cherry-pick that commit onto the fork at the
current pin, so the vendored llama.cpp does not move. A rebase onto upstream
moves the engine pin, which requalifies the LORA-3 observer and every engine
citation here, and is out of scope. An empty slice restores the free-memory
default in that setter, so the SPU never passes one for a multi-device set.

Cherry-pick upstream commit `ef551b0563` with it. On the pin,
`append_kv_override` and `add_cpu_buft_override` always write slot 0, so a
second call fails an assertion. That commit appends instead. The SPU calls
neither today, so the fix is carried, not relied on.

One defect stays, upstream and on the fork. `add_cpu_buft_override` stores the
pattern's pointer with no lifetime tied to the parameters, so a pattern freed
before load leaves a dangling pointer. The SPU does not call it. Any later use
of buffer overrides must fix it first. No other fork change is needed, on the
operator's word of 2026-11-08 that no issue is filed for these.

`with_main_gpu` has no effect in layer split mode. The code may keep or drop
the call, but no comment may claim it selects a device.

This rule binds the GGUF backend only. The native (candle) pair keeps its own
tensor-parallel design (Spec 4.1, `native_pair.rs`).

## 4. Room per device

**PAIR-4.** For a GGUF artifact, admission judges each device against its own
share, not an even fraction of the file. Replace the even shard
(`residency.rs`, file length divided by device count) with a share computed
from the artifact's tensor directories.

Today the header read walks only the metadata of the first pinned file
(`artifact.rs::read_gguf_header`). Extend it to walk the tensor directory of
every pinned file, in split order: each tensor's name, shape and type, from
which its byte size follows. This still reads no tensor data and touches no
device, so it stays among the free steps before the room judgment. A tensor
directory that does not parse refuses as the header read refuses today.

| Term | Content |
| --- | --- |
| Layer weights | The bytes of every `blk.N.*` tensor, from every shard, for the layers PAIR-3 gives the device |
| Output | The output and output-norm tensors, on the last device only. Where the artifact has no separate `output.weight`, the engine duplicates `token_embd.weight` onto the output device, so its bytes are charged to the last device |
| KV cache | For each layer on the device: 2 x context positions x KV head count x head dimension x 2 bytes (the engine's default F16 cache), from the header's `attention.head_count_kv`, key and value lengths, and the effective context |
| Adapter | Under the LoRA fixture, the adapter tensors whose base layer is on the device |
| Headroom | The existing staged headroom parameter, unchanged. It now covers compute buffers and the engine's own overhead, not the cache |

For Granite at 32768 positions, the KV term is 4096 MiB per device, which the
2026-11-08 load check measured. The default 512 MiB headroom left that cache
uncharged, so the room judgment under-asked by about 4 GiB per device.

The KV formula describes full-attention layers. Other cache kinds are charged
by a bound that is never lower than what the engine allocates:

| Layer kind | Charge |
| --- | --- |
| Sliding-window attention | As a full-attention layer at the full context. The engine allocates no more than this, so the charge over-asks and never under-asks |
| Recurrent (state-space) | The layer's fixed state size for one sequence, from the header's state-space keys |
| No cache (the header gives the layer zero KV heads and no state-space role, as for a Nemotron-H feed-forward layer) | Nothing. The engine allocates no cache for the layer, and its weights are charged as for any layer |
| A kind the header does not identify | No bound exists, so admission refuses the binding and names the layer |

The fixture report names which charge each layer took.

The native (candle) pair keeps the existing even-fraction judgment. Its
tensor-parallel path shards the projections of every layer across both
devices (`native_pair.rs::ShardedModel::load`), so no device holds whole
`blk.N.*` layers and the table above does not describe its residency.

The input layer is not otherwise charged to any device. It is host memory, and
the SPU makes no host-memory judgment. The fixture report records its size.

Room is still judged against the driver's current free figure, and the room
refusal still carries the device's capacity.

## 5. Instruments and evidence

**PAIR-5.** The implementation supplies these instruments.

| Claim | Instrument | Deliberate change that must fail |
| --- | --- | --- |
| The harness sets the mask from the binding | The environment built for an SPU fork, with an inherited value of each variable present | Keep an inherited value, or set the mask out of binding order |
| A wrong mask refuses | Admission under a missing order variable and under a mask that differs from the instruction | Admit anyway, or query a device first |
| Positions map back to ordinals | A binding of non-zero ordinals, whose refusal names the binding's ordinals | Report the position |
| No context on a non-member device | On a box with a device outside the binding: the device lists no SPU process after load | Launch without the mask |
| Mixed members refuse | A recording device query returning two different names, capabilities or totals | Judge only the name, or skip the judgment for pairs |
| The split is deterministic | Hardware-free arithmetic over L and N, including odd L and N = 3, checked against the engine's cumulative mapping | Pass proportional halves, or put a value on a boundary edge |
| The split reaches the engine | The parameters built for a pair carry the computed tensor split | Leave the split unset |
| Room is per device for GGUF | Directory fixtures with unequal layer sizes, a split artifact whose later shard holds assigned tensors, a tied-embedding artifact with no `output.weight`, a KV term checked against the 4096 MiB Granite measurement, a sliding-window and a recurrent layer charged by their bounds, a cacheless layer charged no cache, an unidentified layer kind that refuses, and a native artifact that keeps the even fraction | Read only the first shard, omit the duplicated embedding or the KV term, or apply the GGUF table to a native artifact |
| The observed division matches | On a device pair, under the LORA-3 log capture at debug level: exactly one `load_tensors: layer N assigned to device` record for each layer and for the output (`layer L`), and exactly one cache record for each layer that holds a cache: the KV cache's `layer N: dev =` for an attention layer, and the recurrent cache's `layer N: dev =` (`llama-memory-recurrent.cpp`) for a recurrent layer, all matching PAIR-3. For an attention layer, a KV record reading "does not have KV cache" or "filtered" counts as missing. For a recurrent layer, a recurrent record reading "skipped" counts as missing. A layer that holds no cache (a Nemotron-H feed-forward layer, for example) has no `dev =` cache record, and none is expected. Its `load_tensors` record alone judges its device. A record of the other cache kind is expected to be absent for every layer | Accept a missing or duplicated record, or a layer on the wrong device |

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
| Row split | A fork change so the split buffer reads the model's device list, not every visible device by ordinal (possibly moot where the PAIR-1 mask makes the visible set the binding, to be confirmed), then a GPU proof with an adapter whose rank is below the engine's row rounding |
| Tensor split | A measurement on hardware that represents the targeted deployments, and the operator's ruling |
| Tighter cache bounds | A sliding-window charge at the window size, once measured against the engine, in place of the full-context bound |
| Host memory for the input layer | A host-room election, if the operator wants one |
| Host memory at load | Measure the SPU's own load. A llama.cpp server with mmap off peaked near the model's size in host memory (55 GiB for Granite), then fell to under 2 GiB |
| KV cache precision | The engine's default KV cache is F16, where the Python reference runs bf16. A known source of drift for the operator's ruling, not a defect here |
| Kernel device wall | Admin's start step denies the agent's processes the device nodes outside its binding, by cgroup device control. #479 |

Row and tensor split depend on a fast interconnect, because they synchronize
every layer. The targeted workstation devices have no such link, so a result on
a box that has one does not represent them.

## 7. Source index

Read at the baseline in section 0. These locate the work and are not claims
that the mechanic exists.

| Source | Relevant fact |
| --- | --- |
| `decoder/gguf.rs::ResidentModel::load` | Sets layer mode and the device list, and no tensor split |
| `residency.rs`, the admission room step | Even file fraction per device, plus the whole adapter on each |
| `gpu/mod.rs::room_and_reach` | One needed figure for every device, and peer access checked |
| `lora/observation.rs::judge` | Accepts a buffer on any device of the set |
| `tests/two_card.rs` | Two-device admission and serving under the free-memory default |
| `weaver-admin` `start.rs` and `weaver-harness` `spawn.rs` | Environment cleared by admin, passed unchanged to the SPU |
| `weaver-harness` `lifecycle.rs`, the SPU and classify forks | The decoder binding is in hand before the SPU fork, and classify already reads its device |
| Pinned `llama-model.cpp`, the split step | Free-memory default, cumulative fractions over L + 1 slots, input layer on the host |
| Pinned `llama-kv-cache.cpp` | The KV cache follows the layer's assigned device |
| Pinned `llama.cpp`, device list | Layer mode is confined to the listed devices |
| Pinned `llama-cpp-2` `params.rs` | No tensor split setter |
| Upstream `llama-cpp-rs` `e7af607` | The `with_tensor_split` setter to cherry-pick |
| Upstream `llama-cpp-rs` `ef551b0` | Overrides appended instead of written to slot 0 |
