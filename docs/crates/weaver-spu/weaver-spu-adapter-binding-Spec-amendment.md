# weaver-spu Spec: adapter binding amendment

**Version:** 0.1
**Date filed:** 2026-11-08
**Document ID:** `weaver-spu-adapter-binding-Spec-amendment`
**Parent:** `weaver-spu-PRD`
**Editorial:** Per the Working Rules.

## 0. Authority and scope

**ADP-0.** This amendment puts one LoRA adapter on the production path: the
operator declares it beside the base in the model binding, admission loads it
through the mechanic the LoRA amendment built for the fixture, and the record
carries its identity wherever it carries the base's. It closes the LORA-8 cell
"Production adapter serving" for one adapter, and it is #473 item 3's first
half. The operator ruled on 2026-11-08 that the adapter is the point: a model
served without its adapter is only the first step to one served with it.

Amend SPU Spec sections 3 and 6, `weaver-types-Spec` section 2,
`weaver-admin-Spec` section 7, the `weaver-harness-spu` contract (the
`model-binding` draw), the harness replay and Open evidence, the diagnostic
identity carrier, the state member's replay projection and the deploy
runbooks, as mapped below. The grounds are SPU PRD sections 4.1, 7 and 14 and
the LoRA amendment LORA-1 to LORA-5. These clauses authorize code after merge.
Until the ADP-1 to ADP-3 code lands, a declaration carries no adapter and
every production load is base-only.

| Clause | Destination | Addition |
| --- | --- | --- |
| ADP-1 | `weaver-types-Spec` 2, `weaver-admin-Spec` 7, the harness-spu contract | The optional `adapter` table in the model binding |
| ADP-2 | SPU Spec 3 | Admission builds the adapter request from the declaration, on the fixture's path |
| ADP-3 | SPU Spec 6, the harness replay and Open evidence, diagnostic, state | The adapter identity in every record that carries the base identity |
| ADP-4 | `weaver-harness` dev_loop (`Loops/`) | The loop's compiled prompt yields to a seated one |
| ADP-5 | Deploy runbooks | `create-agent.sh` declares the adapter |
| ADP-6 | SPU Spec 10 | Instruments |
| ADP-7 | SPU Spec 10 | Open cells |

The baseline is main at `dc5f439d60865c418b951704eefb645839684ce3`. The
dependency pin is `toddwbucy/llama-cpp-rs` at the revision Cargo.lock records
(`ecce255`, moving to the placement child's `bebe55d` with the same vendored
engine).

**What this amendment does not do.** One adapter per decoder binding. It
declares no set, selects nothing per request, and serves no role other than
decode: the declared set, per-request selection and the classifier as a role
(#473 items 3 and 4) stay open (ADP-7). It changes no renderer and no family
entry. It does not rule the accepted Python/SPU drift. It does not change the
gate's line bound (ADP-7).

## 1. The declaration

**ADP-1.** `ModelBinding` gains an optional `adapter` table. Absent, the
binding is what it is today and the load is base-only. Present, it carries
exactly the four members LORA-1 names for the fixture request, in the
declaration's grammar:

```toml
[spu-instruction.decoder.model-binding]
artifact = "<base GGUF>"
devices = [0, 1]

[spu-instruction.decoder.model-binding.adapter]
artifact = "<adapter GGUF>"
sha256 = "<64 lowercase hex>"
base-blake3 = "<64 lowercase hex>"
scale = 1.0
```

| Member | Meaning and parse rule |
| --- | --- |
| `artifact` | One GGUF LoRA file, named explicitly. The same `ArtifactRef` grammar as the base |
| `sha256` | The adapter file's complete-bytes SHA-256, 64 lowercase hexadecimal characters. Any other shape is a parse error |
| `base-blake3` | The base's SPU Spec 3 BLAKE3 identity, 64 lowercase hexadecimal characters. It pins the adapter to the exact runtime base it was converted against |
| `scale` | A finite f32. No default. A missing or non-finite value is a parse error |

The table is closed: an unknown key is a parse error, as everywhere in the
declaration. A present table with any member missing is a parse error, never a
base-only fallback (LORA-1: do not replace presence with absence after an
error). The `ModelBinding` type stays one type, with `adapter:
Option<AdapterBinding>`.

Admin checks the adapter's `artifact` for presence the way it checks the
base's (`weaver-admin-Spec` section 7): an empty member refuses
`ConfigInvalid` naming
`spu-instruction.decoder.model-binding.adapter.artifact`, and admin resolves
neither path, because resolution runs under the agent's identity at admission.
Admin does not read, hash or open the adapter. The digests are the
declaration's claims, and the SPU is what checks them.

The harness carries the binding opaque to the SPU as today. The
`weaver-harness-spu` contract's `model-binding` draw names the new table and
changes nothing else.

## 2. Admission

**ADP-2.** The SPU builds the LORA-1 adapter request from the declared table,
member for member, and admits it on the one code path LORA-1 already requires
the fixture and the production caller to share. Nothing in LORA-2 to LORA-5
changes: the same custody and ownership checks, the same pre-load judgment,
the same digest and base-identity comparison before any device work, the same
load order, scale attachment and residency ownership, and the same refusal
mapping.

| Declared fact | Admission check (LORA-2) |
| --- | --- |
| `sha256` | Equal to the pinned adapter file's digest, or `ArtifactUnreadable` with the digest-mismatch account |
| `base-blake3` | Equal to the base's computed BLAKE3, or `DeviceCannotAdmit` with the compatibility reason |
| `scale` | Passed to the backend as declared (LORA-4) |

The LORA-2 sentence "the ordinary production caller cannot exercise an adapter
request under this amendment" is retired. The caller is the declaration. The
admit account line names the adapter stage where an adapter fails, so an
operator reading `admin.log` sees which artifact and which check refused.

Room is judged with the adapter's tensors in the need, per LORA-2 and, on a
device pair, per the device-pair amendment PAIR-4, which already charges the
adapter per device.

A restore (a reload of the agent) re-admits the binding as declared, adapter
included. A save point records no adapter fact of its own: the declaration is
the authority for what is loaded, and the record (ADP-3) is the authority for
what was.

## 3. Identity in the record

**ADP-3.** Wherever the record carries the base identity (`model` and
`weights_hash`), it carries the adapter identity beside it, as one member
`adapter`, always present. The member is `null` for a base-only load, so the
absence is stated, and an object for an adapted one:

```json
"adapter": {"artifact": "<display path>", "sha256": "<hex>", "scale": 1.0}
```

The LORA-8 consumer list moves together, in one act, because the Open
measurement is a closed shape and a member added at one site is a refusal at
the next:

| Site | Change |
| --- | --- |
| `placement.rs::open_evidence` | The Open measurement carries `adapter`. The closed shape becomes `model`, `weights_hash`, `adapter`, `prefix_token_ids`, `blocks` |
| `main.rs::render_measurement` | Every generation's measurement carries `adapter`, the actual loaded identity, `null` for base-only |
| `weaver-harness/src/open_evidence.rs` | Validates the five-member shape and refuses a measurement without `adapter` |
| `weaver-harness/src/replay.rs` | `SourceGeneration` and `ReplayIdentity` carry the adapter. The comparison treats an adapter mismatch as `IdentityBroken`, as it treats the weights hash |
| `weaver-diagnostic/src/event.rs::ReplayIdentity` | Carries the adapter |
| `weaver-state/src/preload_door.rs` | The replay projection's paths include `adapter` |
| `residency.rs` and `artifact.rs::weights_hash` | The base identity is unchanged. The adapter identity is the resident's, from LORA-5's ownership |

The state-management Spec amendment that defines SPU-OPEN's closed measurement
changes with this clause, not by a member added from the fixture (the LoRA
amendment's section 6 rule).

## 4. The loop's compiled prompt yields to a seated one

**ADP-4.** The system prompt is state (the ruling of 2026-10-06): the seeding
turn enters it, and every later load seats what the state member holds as the
open's identity prefix. The dev_loop still carries a compiled prompt
(`dev_loop/mod.rs:44-46`, `SYSTEM_PROMPT`) and pushes it as a `Role::System`
message on the first turn of every load (`contribution`, lines 242-252, from
`drive` at 100-105) and again at the head of every re-entry after a flush
(`reentry_text`, lines 127-128, pushed at 253-266). The first-turn test is
"the assembled history is empty" (lines 60-62), and the seated prompt never
counts as history because assembly reads only the user, assistant and
tool-result kinds (`assembly.rs:18-22`).

On every load after the seeding run, the model therefore reads the seated
prompt as its prefix and then the loop's "You are a careful assistant..." as a
second system turn. For the R1 adapter, whose prompt is the seated one, that
is a corruption of the prompt the adapter was trained against, on the path
this amendment opens. The seeding run itself does not show it, because the
seed turn leaves an assistant message.

The rule: **where the open seated a non-empty identity prefix, the loop pushes
no compiled prompt**, on the first turn or at re-entry. The re-entry after a
flush is the recall text alone, the seated prefix having been kept by the
flush. Where nothing is seated (an agent before its seeding turn), the loop
pushes its compiled prompt as today. The continuity line rides with whichever
opening is pushed.

The comment at `dev_loop/mod.rs:81-83`, which still describes the system
prompt as message content rather than the open's prefix, predates the ruling
and goes with this act. The test
`the_prompt_leads_the_first_turn_and_no_other` (`dev_loop/tests.rs:25`) moves
to the unseated case, and a seated case joins it.

This clause lands as its own pull request, since it is a loop change in the
harness and not an SPU change, and it is the operator's to confirm: the loop
is a workflow document under `Loops/`, and this amendment proposes its rule
rather than owning it.

## 5. Deploying an adapted agent

**ADP-5.** `create-agent.sh` takes `--adapter <path> --adapter-sha256 <hex>
--adapter-scale <f32> --base-blake3 <hex>`, all four or none, and writes the
ADP-1 table. It computes nothing: the digests are the operator's declared
claims, taken from the conversion record (#435) and the fixture report
(LORA-6). The adapter file takes the same custody as the base (owned outside
the agent uid, readable by it, not writable by it), and the runbook says so.
`verify-load.sh` reads the adapter member of the Open measurement and prints
it beside the base identity. `HowToDeployANewAgent.md` gains the adapted
agent's steps.

## 6. Instruments

**ADP-6.** The code act carries these tests. Each is perturbation-verified.

| Behaviour | Test | Perturbation that fails it |
| --- | --- | --- |
| The table parses | A declaration with the four members parses to `Some(AdapterBinding)`, and one without the table to `None` | Make the table required |
| Presence is all or nothing | A table missing any one member, with a bad digest shape, with a non-finite scale, or with an unknown key is a parse error | Default any member |
| Admin checks presence only | An empty adapter `artifact` refuses `ConfigInvalid` naming the path, and a nonexistent one passes admin | Resolve the path in admin |
| One path | The request admission builds from the declaration is equal to the one the LORA-7 fixture builds for the same artifacts | Build a second request type |
| The declaration's claims are checked | A wrong `sha256` refuses `ArtifactUnreadable`, and a wrong `base-blake3` refuses `DeviceCannotAdmit`, each with its account | Trust the declared digest |
| The identity is recorded | A base-only Open measurement carries `"adapter": null`, and an adapted one (the LORA-7 fixture) carries the object | Omit the member for base-only |
| The shape stays closed | The harness refuses an Open measurement without `adapter` | Accept the four-member shape |
| Replay judges the adapter | A replay against a record whose adapter differs from the loaded one returns `IdentityBroken` | Compare the weights hash alone |
| The projection follows | The state member's replay projection includes `adapter` | Leave the path list unchanged |
| The loop yields (ADP-4) | With a seated prefix, the first turn's delta carries no compiled prompt, and neither does a re-entry. Without one, it does | Push the compiled prompt unconditionally |
| The script declares it | `create-agent.sh --adapter ...` renders the table and parses, and three of the four flags refuse | Default a flag |

The live proof is the agent turn check of the Granite proof plan: the same
agent restored to the same save point, loaded base-only and then adapted, with
the declaration the only difference, read from the trace.

## 7. Open cells

**ADP-7.** These are outside this amendment:

- **The declared set and per-request selection** (#473 item 3's second half),
  and **the classifier as an SPU role** (item 4).
- **The accepted Python/SPU drift**, the operator's ruling after the LORA-7
  fixture and the 100-source comparison.
- **The gate's line bound.** The R1 system prompt is 47,214 bytes and the gate
  refuses a line over 32 KiB (`weaver-gate-Spec` section 4, `relay.rs:42`),
  so the adapter's own prompt cannot be seeded through the gate today. Seeding
  it in two lines renders two system turns. Raising the bound is a gate Spec
  act and a gate-world contract change, cross-repository, and the operator's
  ruling.
- **The trace floor** (#473 item 1).
- **A save point's record of the adapter.** Whether a save point names the
  adapter it was taken under, so a restore under a different declaration can
  refuse, is a state-management question.
