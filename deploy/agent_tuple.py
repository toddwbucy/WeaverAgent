#!/usr/bin/env python3
"""The agent's deployment tuple, as verify-load.sh prints it after a load.

Each row is one field of the tuple survey (the report of 2026-11-08, rows 1,
2, 3, 6, 7, 8 and 9), with the state engine and the restore save point. Each
line ends in one status word from a fixed set:

- recorded: a member of a live record carries it, named;
- derivable: it follows from facts read here by a rule the line names;
- absent: neither, with the reason;
- FAULT: two sources that must agree do not.

Nothing else is a status, so a reader can grep the printout for each word.

Two doors:

    python3 -I - header <artifact> < agent_tuple.py
        Reads the GGUF header and prints its facts as one JSON line. verify-load
        runs it as the agent's own account (sudo -n -u weaver-<agent>), the
        account admission reads the artifact as, so root never parses the
        artifact. The source arrives on standard input because the agent's
        account cannot reach the checkout.

    python3 -I agent_tuple.py rows --declaration <toml> [--header <json>] ...
        Prints the rows, as root, from files verify-load has already read or
        judged. It parses no artifact.

Standard library only: the deploy scripts take no Python dependency.
"""

import argparse
import hashlib
import json
import os
import struct
import sys

STATUSES = ("recorded", "derivable", "absent", "FAULT")

# --- The GGUF header -------------------------------------------------------

GGUF_MAGIC = b"GGUF"
# Versions 2 and 3 share the layout read below (64-bit counts and lengths).
GGUF_VERSIONS = (2, 3)

# **Every length is bounded before it is used**: against the bytes left in the
# file, and against a cap no sane header reaches. A header that cannot be
# bounded is refused, never read on.
KEY_CAP = 64 * 1024
STRING_CAP = 16 * 1024 * 1024
ARRAY_CAP = 16 * 1024 * 1024
TENSOR_CAP = 1024 * 1024
KV_CAP = 1024 * 1024
DIMS_CAP = 8

# The value types of gguf.h, with the fixed width of each scalar.
SCALAR_WIDTH = {0: 1, 1: 1, 2: 2, 3: 2, 4: 4, 5: 4, 6: 4, 7: 1, 10: 8, 11: 8, 12: 8}
UNSIGNED = {0: "<B", 2: "<H", 4: "<I", 10: "<Q"}
SIGNED = {1: "<b", 3: "<h", 5: "<i", 11: "<q"}
STRING, ARRAY = 8, 9

# ggml.h's `ggml_type`, at the vendored llama.cpp of both fork pins (ecce255
# and bebe55d carry the same list).
GGML_TYPES = {
    0: "F32", 1: "F16", 2: "Q4_0", 3: "Q4_1", 6: "Q5_0", 7: "Q5_1", 8: "Q8_0",
    9: "Q8_1", 10: "Q2_K", 11: "Q3_K", 12: "Q4_K", 13: "Q5_K", 14: "Q6_K",
    15: "Q8_K", 16: "IQ2_XXS", 17: "IQ2_XS", 18: "IQ3_XXS", 19: "IQ1_S",
    20: "IQ4_NL", 21: "IQ3_S", 22: "IQ2_S", 23: "IQ4_XS", 24: "I8", 25: "I16",
    26: "I32", 27: "I64", 28: "F64", 29: "IQ1_M", 30: "BF16", 34: "TQ1_0",
    35: "TQ2_0", 39: "MXFP4", 40: "NVFP4", 41: "Q1_0",
}

# llama.h's `llama_ftype`, the value `general.file_type` carries.
LLAMA_FTYPES = {
    0: "ALL_F32", 1: "MOSTLY_F16", 2: "MOSTLY_Q4_0", 3: "MOSTLY_Q4_1",
    7: "MOSTLY_Q8_0", 8: "MOSTLY_Q5_0", 9: "MOSTLY_Q5_1", 10: "MOSTLY_Q2_K",
    11: "MOSTLY_Q3_K_S", 12: "MOSTLY_Q3_K_M", 13: "MOSTLY_Q3_K_L",
    14: "MOSTLY_Q4_K_S", 15: "MOSTLY_Q4_K_M", 16: "MOSTLY_Q5_K_S",
    17: "MOSTLY_Q5_K_M", 18: "MOSTLY_Q6_K", 19: "MOSTLY_IQ2_XXS",
    20: "MOSTLY_IQ2_XS", 21: "MOSTLY_Q2_K_S", 22: "MOSTLY_IQ3_XS",
    23: "MOSTLY_IQ3_XXS", 24: "MOSTLY_IQ1_S", 25: "MOSTLY_IQ4_NL",
    26: "MOSTLY_IQ3_S", 27: "MOSTLY_IQ3_M", 28: "MOSTLY_IQ2_S",
    29: "MOSTLY_IQ2_M", 30: "MOSTLY_IQ4_XS", 31: "MOSTLY_IQ1_M",
    32: "MOSTLY_BF16", 36: "MOSTLY_TQ1_0", 37: "MOSTLY_TQ2_0",
    38: "MOSTLY_MXFP4_MOE", 39: "MOSTLY_NVFP4", 40: "MOSTLY_Q1_0",
    1024: "GUESSED",
}


class HeaderRefused(Exception):
    """A header this reader cannot bound or does not understand."""


class _Reader:
    def __init__(self, handle, size):
        self.handle = handle
        self.size = size

    def left(self):
        return self.size - self.handle.tell()

    def take(self, count, what):
        if count < 0 or count > self.left():
            raise HeaderRefused(f"{what}: {count} octets past the file's end")
        data = self.handle.read(count)
        if len(data) != count:
            raise HeaderRefused(f"{what}: short read")
        return data

    def skip(self, count, what):
        if count < 0 or count > self.left():
            raise HeaderRefused(f"{what}: {count} octets past the file's end")
        self.handle.seek(count, os.SEEK_CUR)

    def u32(self, what):
        return struct.unpack("<I", self.take(4, what))[0]

    def u64(self, what):
        return struct.unpack("<Q", self.take(8, what))[0]

    def string(self, what, cap):
        length = self.u64(what)
        if length > cap:
            raise HeaderRefused(f"{what}: a length of {length} is past the cap of {cap}")
        return self.take(length, what)

    def count(self, what, cap, width=1):
        value = self.u64(what)
        if value > cap:
            raise HeaderRefused(f"{what}: a count of {value} is past the cap of {cap}")
        if value * width > self.left():
            raise HeaderRefused(f"{what}: {value} items of {width} octets run past the file's end")
        return value


def _value(reader, kind, key, depth=0):
    """One value: a scalar or string is returned, an array is skipped."""
    if kind in SCALAR_WIDTH:
        raw = reader.take(SCALAR_WIDTH[kind], key)
        if kind in UNSIGNED:
            return struct.unpack(UNSIGNED[kind], raw)[0]
        if kind in SIGNED:
            return struct.unpack(SIGNED[kind], raw)[0]
        return None
    if kind == STRING:
        return reader.string(key, STRING_CAP)
    if kind == ARRAY:
        if depth > 0:
            raise HeaderRefused(f"{key}: a nested array")
        element = reader.u32(key)
        if element in SCALAR_WIDTH:
            width = SCALAR_WIDTH[element]
            items = reader.count(key, ARRAY_CAP, width)
            reader.skip(items * width, key)
        elif element == STRING:
            items = reader.count(key, ARRAY_CAP, 8)
            for _ in range(items):
                reader.skip(reader.count(key, STRING_CAP), key)
        else:
            raise HeaderRefused(f"{key}: an array of element type {element}")
        return None
    raise HeaderRefused(f"{key}: value type {kind}")


def read_header(path):
    """The header facts the tuple rows read, as a dict.

    Raises HeaderRefused for a file that is not a GGUF this reader bounds.
    """
    with open(path, "rb") as handle:
        size = os.fstat(handle.fileno()).st_size
        reader = _Reader(handle, size)
        if reader.take(4, "magic") != GGUF_MAGIC:
            raise HeaderRefused("not a GGUF file (magic)")
        version = reader.u32("version")
        if version not in GGUF_VERSIONS:
            raise HeaderRefused(f"GGUF version {version}")
        # Each tensor info takes at least 28 octets (an empty name, one dim,
        # type and offset); each pair at least 12.
        tensors = reader.count("tensor_count", TENSOR_CAP, 28)
        pairs = reader.count("kv_count", KV_CAP, 12)
        keys = {}
        for _ in range(pairs):
            key = reader.string("key", KEY_CAP).decode("utf-8", "replace")
            keys[key] = _value(reader, reader.u32(key), key)
        types = {}
        for _ in range(tensors):
            reader.skip(reader.count("tensor name", KEY_CAP), "tensor name")
            dims = reader.u32("n_dims")
            if dims > DIMS_CAP:
                raise HeaderRefused(f"n_dims {dims} is past the cap of {DIMS_CAP}")
            reader.skip(8 * dims, "dims")
            kind = reader.u32("tensor type")
            reader.skip(8, "tensor offset")
            name = GGML_TYPES.get(kind, f"type {kind}")
            types[name] = types.get(name, 0) + 1
    architecture = keys.get("general.architecture")
    architecture = architecture.decode("utf-8", "replace") if isinstance(architecture, bytes) else None
    block_count = keys.get(f"{architecture}.block_count") if architecture else None
    template = keys.get("tokenizer.chat_template")
    file_type = keys.get("general.file_type")
    split = keys.get("split.count")
    return {
        "version": version,
        "architecture": architecture,
        "block_count": block_count if isinstance(block_count, int) else None,
        "file_type": file_type if isinstance(file_type, int) else None,
        "chat_template_sha256": hashlib.sha256(template).hexdigest() if isinstance(template, bytes) else None,
        "split_count": split if isinstance(split, int) else None,
        "tensor_count": tensors,
        "tensor_types": dict(sorted(types.items())),
    }


# --- PAIR-3, the even division (weaver-spu/src/division.rs) ----------------

def even_division(layers, devices):
    """`Division::even`, case for case: None for no device or no layer; else
    floor(L/N) each, the first L mod N devices taking one more."""
    if devices <= 0 or layers <= 0:
        return None
    base, larger = divmod(layers, devices)
    return [base + (1 if index < larger else 0) for index in range(devices)]


def index_of(counts, slot):
    """`Division::index_of`: the binding index holding engine slot `slot`, the
    output layer (slot L) on the last device, None past it."""
    layers = sum(counts)
    if slot == layers:
        return len(counts) - 1
    end = 0
    for index, count in enumerate(counts):
        end += count
        if slot < end:
            return index
    return None


# --- The family key (weaver-spu/src/family/mod.rs, `same_key`) -------------

# The registry's family keys at this checkout, one per entry, in table order.
# A key listed more than once is contested: the SPU selects among its entries
# by rendering the artifact's template (`family::select`), which this script
# cannot do. Pinned to the registry by FamilyRegistryMirrorTests.
REGISTRY_FAMILIES = (
    "llama", "llama", "llama", "qwen2", "qwen3", "qwen3moe", "qwen35",
    "qwen35moe", "gemma4", "nemotron_h_moe", "mistral3", "phi3", "phi3",
    "gpt-oss", "granite",
)


def family_key(spelling):
    """`same_key`'s fold: `_`, `-` and `.` ignored, ASCII letters lowered."""
    return "".join(c.lower() if c.isascii() else c for c in spelling if c not in "_-.")


def registry_entries(architecture):
    key = family_key(architecture)
    return sum(1 for family in REGISTRY_FAMILIES if family_key(family) == key)


# --- The sink's latest measurement -----------------------------------------

def latest_measurement(sink, artifact):
    """The weights_hash of the sink's last `model.measurement` line, with its
    line number, where that measurement names the declared artifact.

    A load serves no generation, so its own events carry no measurement, and
    the Open evidence that carries the hash at load is dropped on the live
    path. An earlier generation's measurement is the only record of the hash a
    base-only declaration has. One naming another artifact is not this
    binding's, so it is not used. Returns None for no such line.
    """
    found = None
    with open(sink, "rb") as handle:
        for number, raw in enumerate(handle, start=1):
            if b'"model.measurement"' not in raw:
                continue
            try:
                event = json.loads(raw)
            except ValueError:
                continue
            if not isinstance(event, dict) or event.get("kind") != "model.measurement":
                continue
            payload = event.get("payload")
            if isinstance(payload, str):
                try:
                    payload = json.loads(payload)
                except ValueError:
                    continue
            if isinstance(payload, dict) and isinstance(payload.get("weights_hash"), str):
                found = (number, payload)
    if found is None:
        return None
    number, payload = found
    if payload.get("model") != artifact:
        return None
    return {"weights_hash": payload["weights_hash"], "line": number}


def load_payload(path, from_line=1):
    """The payload of the first `load` event at or after line `from_line` of
    `path` (the load's own new events), or None. The trace's `Event` flattens
    its envelope and carries the payload under `payload`."""
    with open(path, "rb") as handle:
        for number, raw in enumerate(handle, start=1):
            if number < from_line:
                continue
            try:
                event = json.loads(raw)
            except ValueError:
                continue
            if isinstance(event, dict) and event.get("kind") == "load":
                payload = event.get("payload")
                return payload if isinstance(payload, dict) else None
    return None


def live_gpus(text):
    """nvidia-smi's `pci.bus_id, name, memory.total` CSV lines, put in PCI bus
    order, each rendered as `<name> <memory>`. None for no such line."""
    rows = []
    for line in text.splitlines():
        parts = [part.strip() for part in line.split(",")]
        if len(parts) == 3 and all(parts):
            rows.append((parts[0].lower(), f"{parts[1]} {parts[2]}"))
    return [label for _, label in sorted(rows)] or None


# --- The rows ---------------------------------------------------------------

def row(number, field, value, status, note):
    assert status in STATUSES, status
    label = f"row {number}" if isinstance(number, int) else number
    return f"   {label:<7} {field:<22} {value}  [{status}: {note}]"


def binding_of(declaration):
    return declaration.get("spu-instruction", {}).get("decoder", {}).get("model-binding", {})


def artifact_rows(declaration, header, header_refusal, measurement):
    """Rows 1, 2 and 3: the artifact, its hash, its family and template, its
    formats."""
    binding = binding_of(declaration)
    adapter = binding.get("adapter")
    lines = [row(1, "artifact", binding.get("artifact", "<none declared>"), "recorded",
                 "model.measurement's model, on each generation")]
    if adapter is not None:
        lines.append(row(1, "weights_hash", adapter["base-blake3"], "derivable",
                         "BLAKE3 by SPU Spec 3's rule; equal to the declared base-blake3, "
                         "since admission refuses a load whose base differs (LORA-2)"))
    elif measurement is not None:
        lines.append(row(1, "weights_hash", measurement["weights_hash"], "recorded",
                         f"the latest model.measurement in the sink, line {measurement['line']}"))
    else:
        lines.append(row(1, "weights_hash", "-", "absent",
                         "the load records no weights_hash and the sink holds no generation; "
                         "each generation's model.measurement carries it"))
    if header is None:
        why = header_refusal or "no GGUF header was read"
        for number, field in ((2, "family"), (2, "chat template"), (3, "file type"),
                              (3, "tensor types")):
            lines.append(row(number, field, "-", "absent", why))
        lines.append(row(2, "template selection", "-", "absent", "the TID code act is parked"))
        return lines
    architecture = header.get("architecture")
    if architecture is None:
        lines.append(row(2, "family", "-", "absent", "the header carries no general.architecture"))
    else:
        entries = registry_entries(architecture)
        if entries == 1:
            lines.append(row(2, "family", architecture, "derivable",
                             "general.architecture names one registry entry (family::lookup)"))
        elif entries > 1:
            lines.append(row(2, "family", architecture, "absent",
                             f"the key is contested by {entries} registry entries; the SPU "
                             "selects one by rendering the template (family::select)"))
        else:
            lines.append(row(2, "family", architecture, "absent",
                             "no registry entry at this checkout carries the key"))
    template = header.get("chat_template_sha256")
    if template is None:
        lines.append(row(2, "chat template", "-", "absent", "the header carries no tokenizer.chat_template"))
    else:
        lines.append(row(2, "chat template", f"sha256 {template}", "derivable",
                         "the header's tokenizer.chat_template bytes, inside weights_hash"))
    lines.append(row(2, "template selection", "-", "absent", "the TID code act is parked"))
    file_type = header.get("file_type")
    if file_type is None:
        lines.append(row(3, "file type", "-", "absent", "the header carries no general.file_type"))
    else:
        lines.append(row(3, "file type", f"{LLAMA_FTYPES.get(file_type, 'unnamed')} ({file_type})",
                         "derivable", "general.file_type; no member renders it"))
    types = ", ".join(f"{name} {count}" for name, count in header.get("tensor_types", {}).items())
    split = header.get("split_count")
    where = f"shard 1 of {split}'s tensor infos" if split and split > 1 else "the header's tensor infos"
    lines.append(row(3, "tensor types", types or "-", "derivable" if types else "absent",
                     f"{where}; no member renders them"))
    return lines


def adapter_rows(declaration):
    adapter = binding_of(declaration).get("adapter")
    if adapter is None:
        return [row(6, "adapter", "none", "recorded",
                    "declared none; each generation's model.measurement carries adapter null (ADP-3)")]
    value = "{} sha256 {} scale {}".format(adapter["artifact"], adapter["sha256"], adapter["scale"])
    return [row(6, "adapter", value, "recorded",
                "declared; each generation's model.measurement carries it (ADP-3); "
                "the Open half is dropped on the live path")]


def device_rows(declaration, header, box_gpus, live_gpus):
    """Rows 7, 8 and 9: the devices, the placement and the offload."""
    devices = binding_of(declaration).get("devices", [])
    lines = [row(7, "devices", json.dumps(devices), "absent",
                 "in the record only inside the declaration's sha256")]
    premise = ("binding ordinal into the PCI-order list, under PAIR-1's "
               "CUDA_DEVICE_ORDER=PCI_BUS_ID mask")
    for ordinal in devices:
        if box_gpus is None:
            lines.append(row(7, f"device {ordinal}", "-", "absent", "no install tuple was read"))
        elif not 0 <= ordinal < len(box_gpus):
            lines.append(row(7, f"device {ordinal}", "-", "FAULT",
                             f"the install tuple lists {len(box_gpus)} devices"))
        else:
            lines.append(row(7, f"device {ordinal}", box_gpus[ordinal], "derivable",
                             f"the install tuple's {premise}"))
    if live_gpus is None:
        lines.append(row(7, "live", "-", "absent", "nvidia-smi answered nothing"))
    elif box_gpus is not None and live_gpus != box_gpus:
        lines.append(row(7, "live", "; ".join(live_gpus), "FAULT",
                         "nvidia-smi's PCI-order list differs from the install tuple's, "
                         "so the install tuple is stale"))
    else:
        for ordinal in devices:
            if 0 <= ordinal < len(live_gpus):
                lines.append(row(7, f"live {ordinal}", live_gpus[ordinal], "derivable",
                                 f"nvidia-smi now, {premise}"))
    layers = header.get("block_count") if header else None
    counts = even_division(layers, len(devices)) if layers else None
    if counts is None:
        lines.append(row(8, "placement", "-", "absent",
                         "no block_count in the header, or no device declared"))
    elif len(counts) == 1:
        lines.append(row(8, "placement", f"all {layers} layers and the output on device {devices[0]}",
                         "derivable", "one device loads with split mode none (PAIR-3, gguf.rs)"))
    else:
        split = ", ".join(f"device {d}: {c}" for d, c in zip(devices, counts))
        lines.append(row(8, "placement", f"{split}; output on device {devices[index_of(counts, layers)]}",
                         "derivable",
                         f"PAIR-3's even split of {layers} repeating layers in binding order "
                         "(division.rs); PAIR-0, no member"))
    lines.append(row(9, "cpu offload", "every layer offloaded; input embedding and sampling on the host",
                     "derivable",
                     "n_gpu_layers u32::MAX (gguf.rs placement_params); the engine keeps "
                     "token_embd in host memory by rule, at the install tuple's engine revision"))
    return lines


def state_rows(declaration, load_event):
    lines = []
    engine = declaration.get("state-store", {}).get("engine")
    recorded = (load_event or {}).get("state_store", {}).get("engine") if load_event else None
    if engine is None:
        lines.append(row("state", "engine", "-", "absent", "the declaration names no state-store engine"))
    elif load_event is None:
        lines.append(row("state", "engine", engine, "absent", "no load event was read"))
    elif recorded == engine:
        lines.append(row("state", "engine", engine, "recorded", "the load event's state_store"))
    else:
        lines.append(row("state", "engine", engine, "FAULT",
                         f"the load event's state_store names {recorded!r}"))
    point = declaration.get("restore", {}).get("save-point")
    if point is None:
        lines.append(row("restore", "save point", "none declared", "recorded",
                         "the declaration has no [restore]; the load event carries no lineage"))
    else:
        lines.append(row("restore", "save point", point, "recorded",
                         "the declaration's [restore]; the load event's lineage on a restoring load"))
    return lines


def main(argv):
    parser = argparse.ArgumentParser(prog="agent_tuple.py")
    sub = parser.add_subparsers(dest="door", required=True)
    header = sub.add_parser("header")
    header.add_argument("artifact")
    rows = sub.add_parser("rows")
    rows.add_argument("--declaration", required=True)
    rows.add_argument("--header-json")
    rows.add_argument("--sink")
    rows.add_argument("--from-line", type=int, default=1)
    rows.add_argument("--live-gpus-csv")
    args = parser.parse_args(argv)
    if args.door == "header":
        try:
            print(json.dumps(read_header(args.artifact), sort_keys=True))
        except HeaderRefused as refusal:
            print(json.dumps({"refused": str(refusal)}))
            return 1
        except OSError as error:
            print(json.dumps({"refused": f"unreadable: {error.strerror}"}))
            return 1
        return 0
    import tomllib
    with open(args.declaration, "rb") as handle:
        declaration = tomllib.load(handle)
    facts, refusal = None, None
    if args.header_json is not None:
        try:
            answer = json.loads(args.header_json)
        except ValueError:
            answer = {"refused": "the header read answered no JSON"}
        if not isinstance(answer, dict):
            answer = {"refused": "the header read answered no object"}
        if "refused" in answer:
            refusal = f"the header reader refused: {answer['refused']}"
        else:
            facts = answer
    load_event, measurement = None, None
    if args.sink:
        load_event = load_payload(args.sink, args.from_line)
        measurement = latest_measurement(args.sink, binding_of(declaration).get("artifact"))
    live = live_gpus(args.live_gpus_csv) if args.live_gpus_csv else None
    lines = (artifact_rows(declaration, facts, refusal, measurement)
             + adapter_rows(declaration)
             + device_rows(declaration, facts, None, live)
             + state_rows(declaration, load_event))
    print("\n".join(lines))
    return 3 if any("[FAULT:" in line for line in lines) else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
