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
import re
import stat
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


ARCHITECTURE = re.compile(r"[A-Za-z0-9_.-]{1,64}")


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


CONTAINER_SUFFIXES = (".gguf", ".safetensors")


def split_pattern(name):
    """`<stem>-NNNNN-of-NNNNN.<gguf|safetensors>` as (stem, count, suffix), or
    None: artifact.rs `split_pattern`, case for case."""
    for suffix in CONTAINER_SUFFIXES:
        if name.endswith(suffix):
            rest = name[:-len(suffix)]
            break
    else:
        return None
    digits = set("0123456789")
    if len(rest) < 15:
        return None
    head, of_part = rest[:-9], rest[-9:]
    if not of_part.startswith("-of-"):
        return None
    count_digits = of_part[4:]
    if not set(count_digits) <= digits:
        return None
    count = int(count_digits)
    if len(head) < 6:
        return None
    stem_dash, index_digits = head[:-5], head[-5:]
    if not set(index_digits) <= digits or not stem_dash.endswith("-"):
        return None
    stem = stem_dash[:-1]
    index = int(index_digits)
    if not stem or count == 0 or index == 0 or index > count:
        return None
    return stem, count, suffix


def container_within(directory):
    """The one container a directory artifact resolves to, as admission's
    `container_within` (artifact.rs) chooses it: dot-entries skipped, regular
    files with a `.gguf` or `.safetensors` extension only, one such file, or
    several that are exactly one split's shards (one stem, suffix and count,
    as many as the count), which resolve to the first shard."""
    try:
        names = os.listdir(directory)
    except OSError as error:
        raise HeaderRefused(f"the artifact directory is unreadable: {error.strerror}") from None
    found = sorted(n for n in names
                   if not n.startswith(".")
                   and os.path.splitext(n)[1] in CONTAINER_SUFFIXES
                   and os.path.isfile(os.path.join(directory, n)))
    if not found:
        raise HeaderRefused("the artifact directory holds no container")
    if len(found) == 1:
        return os.path.join(directory, found[0])
    parsed = [split_pattern(n) for n in found]
    if None in parsed or len(set(parsed)) != 1 or len(found) != parsed[0][1]:
        raise HeaderRefused(f"the artifact directory holds {len(found)} containers "
                            "that are not one split's shards")
    stem, count, suffix = parsed[0]
    return os.path.join(directory, f"{stem}-{1:05}-of-{count:05}{suffix}")


def resolve_artifact(path):
    """The file a declared artifact resolves to, by admission's rule
    (artifact.rs `resolve_with_kind`): a directory to its one container, a
    regular file to itself, anything else refused."""
    try:
        mode = os.stat(path).st_mode
    except (FileNotFoundError, NotADirectoryError):
        raise HeaderRefused("the artifact does not resolve") from None
    except OSError as error:
        raise HeaderRefused(f"the artifact is unreadable: {error.strerror}") from None
    if stat.S_ISDIR(mode):
        return container_within(path)
    if not stat.S_ISREG(mode):
        raise HeaderRefused("the artifact is not a regular file or a directory")
    return path


def read_header(path):
    """The header facts the tuple rows read, as a dict, the declared path
    resolved first as admission resolves it, and `resolved` naming the
    container where a directory resolved.

    Raises HeaderRefused for an artifact that does not resolve, or a file
    that is not a GGUF this reader bounds.
    """
    resolved = resolve_artifact(path)
    fd = os.open(resolved, os.O_RDONLY | os.O_NONBLOCK | os.O_CLOEXEC)
    with os.fdopen(fd, "rb") as handle:
        if not stat.S_ISREG(os.fstat(handle.fileno()).st_mode):
            raise HeaderRefused("the resolved artifact is not a regular file")
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
    # A family key is a short name; anything else is not one this reader
    # reports, and is printed as absent rather than echoed.
    if architecture is not None and not ARCHITECTURE.fullmatch(architecture):
        architecture = None
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
        "resolved": resolved if resolved != path else None,
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

def open_sink(path):
    """The sink opened once, as a regular file, never through a link and never
    blocking on a FIFO: O_NOFOLLOW refuses a link at the name, O_NONBLOCK
    keeps a FIFO swapped in after the shell's look from holding root for
    ever, and fstat on the descriptor refuses anything not regular. Every
    read of the sink goes through the one descriptor this returns."""
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK | os.O_CLOEXEC)
    if not stat.S_ISREG(os.fstat(fd).st_mode):
        os.close(fd)
        raise OSError(f"{path} is not a regular file")
    return os.fdopen(fd, "rb")


HEX64 = re.compile(r"[0-9a-f]{64}")


def scan_sink(handle, from_line, artifact):
    """One pass over the sink: the payload of the first `load` event at or
    after `from_line` (the load's own new events), and the sink's last
    `model.measurement` when it names the declared artifact.

    The trace's `Event` flattens its envelope and carries the payload under
    `payload`; a measurement is spliced there whole. A load serves no
    generation, so its own events carry no measurement, and the Open evidence
    that carries the hash at load is dropped on the live path: an earlier
    generation's measurement is the only record of the hash a base-only
    declaration has. One naming another artifact is not this binding's, so it
    is not used, and a hash that is not 64 lowercase hex digits is not taken.
    """
    load, found = None, None
    for number, raw in enumerate(handle, start=1):
        is_load = load is None and number >= from_line and b'"load"' in raw
        if not is_load and b'"model.measurement"' not in raw:
            continue
        try:
            event = json.loads(raw)
        except ValueError:
            continue
        if not isinstance(event, dict):
            continue
        payload = event.get("payload")
        if is_load and event.get("kind") == "load":
            load = payload if isinstance(payload, dict) else {}
            continue
        if event.get("kind") != "model.measurement":
            continue
        if isinstance(payload, str):
            try:
                payload = json.loads(payload)
            except ValueError:
                continue
        if isinstance(payload, dict) and isinstance(payload.get("weights_hash"), str):
            found = (number, payload)
    measurement = None
    if found is not None:
        number, payload = found
        if payload.get("model") == artifact and HEX64.fullmatch(payload["weights_hash"]):
            measurement = {"weights_hash": payload["weights_hash"], "line": number}
    return load or None, measurement


def latest_measurement(sink, artifact):
    with open_sink(sink) as handle:
        return scan_sink(handle, 1, artifact)[1]


def load_payload(path, from_line=1):
    with open_sink(path) as handle:
        return scan_sink(handle, from_line, None)[0]


def live_gpus(text):
    """nvidia-smi's `pci.bus_id, name, memory.total` CSV lines, each rendered
    `<bus id>, <name>, <memory>` as the install tuple's `gpu.<n>` values are,
    and sorted in the C locale (code point order) as install-tuple.sh sorts
    them, which is PCI bus order. None for no such line."""
    rows = []
    for line in text.splitlines():
        parts = [part.strip() for part in line.split(",")]
        if len(parts) == 3 and all(parts):
            rows.append(", ".join(parts))
    return sorted(rows) or None


# --- The install tuple (deploy/install-tuple.sh) ---------------------------

INSTALL_TUPLE_CAP = 64 * 1024
INSTALL_SCALARS = ("commit", "toolchain", "features", "engine", "engine-source",
                   "engine-vendored", "nvcc", "driver", "cudart", "cublas", "cublaslt",
                   "cccl", "gpus")


# What install-tuple.sh writes for a fact the box cannot answer.
UNKNOWN = "unknown"


class InstallTupleRefused(Exception):
    """An install tuple this reader cannot take as written."""


def read_install_tuple(path):
    """The box's install tuple: its own sha256 (the box side's identity), the
    scalar facts, the member and library digests by name, and the devices in
    `gpu.<n>` order.

    One fact per line, `key = value`, split on the first " = ", no value
    empty, every scalar key present, no key twice, each member and library
    digest 64 hex digits or `unknown` (install-tuple.sh writes `unknown` for
    any fact the box cannot answer, a file it could not hash among them),
    `gpu.<n>` numbered from 0 without a gap and as many as `gpus` says.
    Anything else is refused.
    """
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK | os.O_CLOEXEC)
    with os.fdopen(fd, "rb") as handle:
        if not stat.S_ISREG(os.fstat(handle.fileno()).st_mode):
            raise InstallTupleRefused("not a regular file")
        raw = handle.read(INSTALL_TUPLE_CAP + 1)
    if len(raw) > INSTALL_TUPLE_CAP:
        raise InstallTupleRefused(f"past {INSTALL_TUPLE_CAP} octets")
    try:
        text = raw.decode("ascii")
    except UnicodeDecodeError:
        raise InstallTupleRefused("not ASCII") from None
    facts, members, libraries, gpus = {}, {}, {}, {}
    for number, line in enumerate(text.splitlines(), start=1):
        key, sep, value = line.partition(" = ")
        if not sep or not key or not value or not line.isprintable():
            raise InstallTupleRefused(f"line {number} is not `key = value`")
        for prefix, into in (("member.", members), ("library.", libraries), ("gpu.", gpus)):
            if key.startswith(prefix):
                name = key[len(prefix):]
                break
        else:
            into, name = facts, key
        if not name or name in into:
            raise InstallTupleRefused(f"line {number}: {key} is empty or repeated")
        into[name] = value
    missing = [key for key in INSTALL_SCALARS if key not in facts]
    if missing:
        raise InstallTupleRefused(f"no {', '.join(missing)}")
    for name, digest in list(members.items()) + list(libraries.items()):
        if digest != UNKNOWN and not HEX64.fullmatch(digest):
            raise InstallTupleRefused(f"{name}'s digest is not 64 hex digits or unknown")
    order = sorted(gpus, key=lambda n: int(n) if n.isdigit() else -1)
    if order != [str(n) for n in range(len(gpus))]:
        raise InstallTupleRefused("gpu.<n> is not numbered 0 to n-1")
    if facts["gpus"] != str(len(gpus)):
        raise InstallTupleRefused(f"gpus says {facts['gpus']} and {len(gpus)} gpu lines stand")
    return {"sha256": hashlib.sha256(raw).hexdigest(), "facts": facts, "members": members,
            "libraries": libraries, "gpus": [gpus[n] for n in order]}


def file_sha256(path):
    """A file's sha256, opened without following a link and judged regular."""
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK | os.O_CLOEXEC)
    with os.fdopen(fd, "rb") as handle:
        if not stat.S_ISREG(os.fstat(handle.fileno()).st_mode):
            raise OSError(f"{path} is not a regular file")
        digest = hashlib.sha256()
        for block in iter(lambda: handle.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


VERSIONED_LIBRARY = re.compile(r"lib(ggml|llama)[A-Za-z0-9_-]*\.so\.[0-9]+\.[0-9]+\.[0-9]+")


# --- The rows ---------------------------------------------------------------

def printable(text):
    """Every character a terminal would act on, escaped: the values come from
    the artifact's header, the sink and the declaration, and the line is
    printed to root's terminal."""
    return "".join(c if c.isprintable() else c.encode("unicode_escape").decode("ascii")
                   for c in str(text))


class Line(str):
    """A printed row that carries its status, so the exit reads the flag set
    where the row is made and never the printed text, which holds values
    from the header, the declaration and the sink."""
    fault = False


def row(number, field, value, status, note):
    assert status in STATUSES, status
    label = f"row {number}" if isinstance(number, int) else number
    line = Line(f"   {label:<7} {field:<22} {printable(value)}  [{status}: {printable(note)}]")
    line.fault = status == "FAULT"
    return line


def binding_of(declaration):
    return declaration.get("spu-instruction", {}).get("decoder", {}).get("model-binding", {})


def artifact_rows(declaration, header, header_refusal, measurement):
    """Rows 1, 2 and 3: the artifact, its hash, its family and template, its
    formats."""
    binding = binding_of(declaration)
    adapter = binding.get("adapter")
    lines = [row(1, "artifact", binding.get("artifact", "<none declared>"), "recorded",
                 "model.measurement's model, on each generation")]
    if header is not None and header.get("resolved"):
        lines.append(row(1, "container", header["resolved"], "derivable",
                         "the declared directory's one container, by admission's rule "
                         "(artifact.rs container_within), whose header rows 2, 3 and 8 read"))
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


def device_rows(declaration, header, box_gpus, live_gpus, engine=None):
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
                     "token_embd in host memory by rule, at the engine revision "
                     + (engine or "the install tuple would name")))
    return lines


def box_rows(install_path, install, refusal):
    """The box side: the install tuple's own digest, the identity a report
    cites for the box, and the engine it names."""
    if install is None:
        return [row("box", "install tuple", "-", "FAULT" if refusal else "absent",
                    refusal or f"no install tuple at {install_path}; the box side is unstated")]
    facts = install["facts"]
    fork = facts["engine-source"].rpartition("#")[2]
    return [
        row("box", "install tuple", f"sha256 {install['sha256']}", "recorded",
            f"{install_path}, the box tuple's identity; commit {facts['commit']}"),
        row("box", "engine", f"fork {fork}, llama.cpp {facts['engine-vendored']}", "recorded",
            "the install tuple's engine-source and engine-vendored"),
    ]


def binding_rows(declaration_sha256, load_event, install, stack_library_path):
    """The record bound to the box: the declaration's sha256 against the load
    event's, the load event's stack digests against the install tuple's
    members by name, and its library path against the install tuple's
    library set, each file hashed here."""
    lines = []
    if load_event is None:
        return [row("bind", "declaration", f"sha256 {declaration_sha256}", "absent",
                    "no load event was read, so no record is bound")]
    recorded = load_event.get("declaration")
    if recorded == declaration_sha256:
        lines.append(row("bind", "declaration", f"sha256 {declaration_sha256}", "recorded",
                         "equal to the load event's declaration"))
    else:
        lines.append(row("bind", "declaration", f"sha256 {declaration_sha256}", "FAULT",
                         f"the load event's declaration is {recorded!r}"))
    if install is None:
        lines.append(row("bind", "stack", "-", "absent", "no install tuple to bind the stack to"))
        return lines
    stack = load_event.get("stack") if isinstance(load_event.get("stack"), dict) else {}
    members = install["members"]
    for name in sorted(set(stack) | set(members)):
        if name in stack and members.get(name) == UNKNOWN:
            lines.append(row("bind", f"member {name}", stack[name], "absent",
                             "not bound: the install tuple could not hash it"))
        elif name in stack and name in members:
            if stack[name] == members[name]:
                lines.append(row("bind", f"member {name}", members[name], "recorded",
                                 "the load event's stack equals the install tuple's member"))
            else:
                lines.append(row("bind", f"member {name}", stack[name], "FAULT",
                                 f"the install tuple's member is {members[name]}"))
        else:
            side = "the load event's stack" if name in stack else "the install tuple"
            lines.append(row("bind", f"member {name}", (stack.get(name) or members.get(name)), "absent",
                             f"not bound: only {side} carries it"))
    path = load_event.get("library_path")
    if not path:
        lines.append(row("bind", "library path", "-", "absent",
                         "the load event names no library path"))
        return lines
    if os.path.normpath(path) != os.path.normpath(stack_library_path or ""):
        lines.append(row("bind", "library path", path, "absent",
                         f"not bound: the install tuple digests {stack_library_path or 'no directory'}"))
        return lines
    libraries = install["libraries"]
    try:
        present = sorted(n for n in os.listdir(path) if VERSIONED_LIBRARY.fullmatch(n))
    except OSError as error:
        return lines + [row("bind", "library path", path, "FAULT", f"unreadable: {error.strerror}")]
    for name in sorted(set(present) | set(libraries)):
        if name not in libraries:
            lines.append(row("bind", f"library {name}", "-", "FAULT",
                             "in the library path and not in the install tuple"))
            continue
        try:
            digest = file_sha256(os.path.join(path, name))
        except OSError as error:
            lines.append(row("bind", f"library {name}", "-", "FAULT",
                             f"the install tuple names it and it does not read: {error}"))
            continue
        if libraries[name] == UNKNOWN:
            lines.append(row("bind", f"library {name}", digest, "absent",
                             "not bound: the install tuple could not hash it"))
        elif digest == libraries[name]:
            lines.append(row("bind", f"library {name}", digest, "recorded",
                             f"the file in {path} equals the install tuple's digest"))
        else:
            lines.append(row("bind", f"library {name}", digest, "FAULT",
                             f"the install tuple's digest is {libraries[name]}"))
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
    return lines + restore_rows(declaration, load_event)


def names_the_save_point(declared, digest):
    """Whether `[restore] save-point` names the save point of `digest`, as
    admin's selection matches it (save_points.rs select_with): the digest
    itself, or the published name `<YYYYMMDDTHHMMSSZ>-<digest>.save-point`
    (published_name)."""
    return declared == digest or re.fullmatch(
        r"[0-9]{8}T[0-9]{6}Z-" + re.escape(digest) + r"\.save-point", declared) is not None


def restore_rows(declaration, load_event):
    """What the declaration asks to restore and what the load restored, two
    rows. Admin restores the manifest's latest save point where `[restore]`
    is absent, and the harness names what it restored in the load event's
    `lineage` (lifecycle.rs, weaver-trace event.rs `Lineage`). Where the
    declaration names a save point and the load restored another, or none,
    that is a FAULT."""
    declared = declaration.get("restore", {}).get("save-point")
    lines = [row("restore", "declared", declared or "none declared", "recorded",
                 "the declaration's [restore]" if declared else
                 "the declaration has no [restore], so admin restores the manifest's latest")]
    if load_event is None:
        return lines + [row("restore", "restored from", "-", "absent", "no load event was read")]
    lineage = load_event.get("lineage")
    if lineage is None:
        if declared:
            return lines + [row("restore", "restored from", "none", "FAULT",
                                "the declaration names a save point and the load event "
                                "carries no lineage")]
        return lines + [row("restore", "restored from", "none", "recorded",
                            "the load event carries no lineage, so this load restored no save point")]
    digest = lineage.get("save_point") if isinstance(lineage, dict) else None
    if not isinstance(digest, str) or not HEX64.fullmatch(digest):
        return lines + [row("restore", "restored from", "-", "FAULT",
                            "the load event's lineage names no save point digest")]
    value = (f"{digest} (run {lineage.get('run')}, sequence {lineage.get('sequence')}, "
             f"turn {lineage.get('turn')})")
    note = f"the load event's lineage, named_at_restore {lineage.get('named_at_restore')}"
    if lineage.get("built_from") is not None:
        note += f", built_from {json.dumps(lineage['built_from'], sort_keys=True)}"
    if declared and not names_the_save_point(declared, digest):
        return lines + [row("restore", "restored from", value, "FAULT",
                            f"the declaration's [restore] names {declared}")]
    return lines + [row("restore", "restored from", value, "recorded", note)]


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
    rows.add_argument("--install-tuple")
    rows.add_argument("--install-tuple-unheld")
    rows.add_argument("--stack-library-path")
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
    # One read: the bytes parsed are the bytes hashed.
    with open(args.declaration, "rb") as handle:
        declaration_bytes = handle.read()
    declaration = tomllib.loads(declaration_bytes.decode("utf-8"))
    declaration_sha256 = hashlib.sha256(declaration_bytes).hexdigest()
    install, install_refusal = None, None
    if args.install_tuple_unheld:
        install_refusal = f"the install tuple is not held closed by root at {args.install_tuple_unheld}"
    elif args.install_tuple and os.path.lexists(args.install_tuple):
        try:
            install = read_install_tuple(args.install_tuple)
        except (InstallTupleRefused, OSError) as error:
            install_refusal = f"the install tuple is refused: {error}"
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
        with open_sink(args.sink) as handle:
            load_event, measurement = scan_sink(handle, args.from_line,
                                                binding_of(declaration).get("artifact"))
    live = live_gpus(args.live_gpus_csv) if args.live_gpus_csv else None
    engine = None
    if install is not None:
        engine = (f"fork {install['facts']['engine-source'].rpartition('#')[2]}, "
                  f"llama.cpp {install['facts']['engine-vendored']}")
    lines = (box_rows(args.install_tuple or "-", install, install_refusal)
             + artifact_rows(declaration, facts, refusal, measurement)
             + adapter_rows(declaration)
             + device_rows(declaration, facts, install["gpus"] if install else None, live, engine)
             + state_rows(declaration, load_event)
             + binding_rows(declaration_sha256, load_event, install, args.stack_library_path))
    print("\n".join(lines))
    return 3 if any(line.fault for line in lines) else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
