"""The admission's step order, python-spu-Spec section 3.1's step table: a failure
injected at each step of weaver-spu residency.rs `admit` and decoder/native.rs
`ResidentModel::load` answers that step's kind, whatever later step would also fail,
so the order itself is what these tests hold rather than a list of instances. Found
as a class by Codex's #47 rounds 1 and 4."""
import json
import shutil
import struct

import pytest

from python_spu import engine
from python_spu.engine import ADMISSION_STEPS, AdmissionError


@pytest.fixture
def copy(tmp_path, tiny_model):
    root = tmp_path / "artifact"
    shutil.copytree(tiny_model, root)
    return root


def refusal(path, devices=(0,), readout=False):
    with pytest.raises(AdmissionError) as caught:
        engine.HFEngine(path, list(devices), cpu=True, readout=readout)
    return caught.value


def unforeseen(*args, **kwargs):
    raise RuntimeError("unforeseen")


# Each step's own function, raising what the step never anticipated.
INJECTED = [
    ("resolve", "resolve_directory"),
    ("pin", "pin"),
    ("header", "read_header"),
    ("select", "select"),
    ("size", "pinned_size"),
    ("room", "judge_room"),
    ("hash", "weights_digest"),
    ("load", "pinned_tensors"),
]


@pytest.mark.parametrize("step,target", INJECTED)
def test_a_failure_at_each_step_answers_that_steps_kind(copy, monkeypatch, step, target):
    """An unforeseen failure inside a step crosses as the step's kind from the table, and
    names the step. Perturbation: answer every unforeseen failure as one kind, as the
    prototype's catch-all did, and the steps of the other kind differ."""
    if step == "room":
        import torch
        monkeypatch.setattr(torch.cuda, "is_available", lambda: True)
        monkeypatch.setattr(torch.cuda, "device_count", lambda: 1)
        monkeypatch.setattr(torch.cuda, "mem_get_info", lambda d=None: (1 << 40, 1 << 40))
        monkeypatch.setattr(engine, target, unforeseen)
        with pytest.raises(AdmissionError) as caught:
            engine.HFEngine(copy, [0], cpu=False)
        got = caught.value
    else:
        monkeypatch.setattr(engine, target, unforeseen)
        got = refusal(copy)
    assert (got.kind, str(got)) == (ADMISSION_STEPS[step], f"{step}: unforeseen")


def metadata_header(root, family):
    """Replaces the container's header with one naming `family`, its tensors gone."""
    body = json.dumps({"__metadata__": {"model_type": family}}).encode()
    (root / "model.safetensors").write_bytes(struct.pack("<Q", len(body)) + body)


def test_the_judgments_between_selection_and_the_load_answer_in_order(copy):
    """Selection, width, readout and distinct devices, each before the next and all
    before the hash and the load: an unknown family is unreadable whatever its devices;
    a family the registry selects at a width it does not declare is a device refusal
    before the load's own judgment of the family would answer unreadable; a selected
    family that does not tap refuses an elected readout as a device condition; a device
    named twice refuses before the room. Perturbation: judge the family at the load
    only, as the prototype did, and qwen3moe answers unreadable."""
    metadata_header(copy, "mystery")
    assert refusal(copy, devices=(0, 1, 2)).kind == "artifact_unreadable"
    metadata_header(copy, "qwen3moe")
    assert str(refusal(copy, devices=(0, 1, 2))).startswith("3 devices")
    assert refusal(copy, devices=(0, 1, 2)).kind == "device_cannot_admit"
    assert refusal(copy, readout=True).kind == "device_cannot_admit"
    assert "twice" in str(refusal(copy, devices=(0, 0)))
    # At the load, the declaration's own family is judged, not the header's: a
    # config.json declaring another family is the native backend's
    # BackendDoesNotServe, unreadable.
    config = json.loads((copy / "config.json").read_text())
    (copy / "config.json").write_text(json.dumps({**config, "model_type": "qwen3moe"}))
    assert refusal(copy).kind == "artifact_unreadable"


def test_a_weight_the_artifact_lacks_refuses_the_load(copy):
    """candle's VarBuilder refuses a tensor the model needs and the artifact lacks,
    `LoadFailed`; transformers would initialise it at random and serve. Found by the
    self-walk of #47's round 4. Perturbation: drop the missing-keys check, and the
    artifact with no tensors admits."""
    metadata_header(copy, "qwen2")
    got = refusal(copy)
    assert (got.kind, str(got).split(":")[0]) == ("device_cannot_admit", "missing tensors")


def test_the_hash_answers_before_the_load(copy, monkeypatch):
    """weaver-spu-Spec section 3 places the hash after the room and before any device
    is taken, so an artifact whose hash and whose load would both fail is unreadable.
    Perturbation: hash after the load, as the prototype did, and this is a device
    refusal."""
    monkeypatch.setattr(engine, "pinned_tensors", unforeseen)
    monkeypatch.setattr(engine, "weights_digest", unforeseen)
    assert refusal(copy).kind == "artifact_unreadable"


@pytest.mark.parametrize("name,damage,kind", [
    ("declaration absent", lambda r: (r / "config.json").unlink(), "device_cannot_admit"),
    ("tokenizer absent", lambda r: (r / "tokenizer.json").unlink(), "device_cannot_admit"),
])
def test_the_native_loads_reads_are_step_four(copy, oracle, name, damage, kind):
    """decoder/native.rs reads `config.json` (`read_declaration`, `read_config`,
    `read_eos`) and `tokenizer.json` inside the load, each failure `LoadFailed`. With the
    family declared in the header's `__metadata__`, the header step passes without a
    config, as the Rust's does through the oracle, and the load refuses. Found by
    Codex on #47. Perturbation: read the config before the header, as the prototype
    did, and the absent declaration is unreadable."""
    body = (copy / "model.safetensors").read_bytes()
    length = struct.unpack("<Q", body[:8])[0]
    header = json.loads(body[8:8 + length])
    header["__metadata__"] = {**header.get("__metadata__", {}), "model_type": "qwen2"}
    encoded = json.dumps(header).encode()
    (copy / "model.safetensors").write_bytes(
        struct.pack("<Q", len(encoded)) + encoded + body[8 + length:])
    damage(copy)
    assert "ok" in oracle(op="header", path=str(copy))
    assert refusal(copy).kind == kind


def test_a_short_header_read_is_read_on(tmp_path, monkeypatch):
    """artifact.rs reads with `read_exact`, which reads on after a short read until the
    buffer is full or the file ends. Found by Codex on #47. Perturbation: one `pread`
    per take, as before, and the halved reads refuse a sound header."""
    real = engine.os.pread
    monkeypatch.setattr(engine.os, "pread", lambda fd, n, at: real(fd, max(1, n // 2), at))
    path = tmp_path / "model.gguf"
    data = b"GGUF" + struct.pack("<IQQ", 3, 0, 1)
    key, value = b"general.architecture", b"qwen2"
    data += struct.pack("<Q", len(key)) + key + struct.pack("<I", 8)
    data += struct.pack("<Q", len(value)) + value
    path.write_bytes(data)
    pinned = engine.pin([path])
    try:
        assert engine.read_header(pinned[0][1], tmp_path)["family"] == "qwen2"
    finally:
        for _, fd in pinned:
            engine.os.close(fd)
