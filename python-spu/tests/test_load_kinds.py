"""Each load failure crosses as the Rust SPU's refusal for the same step, per
python-spu-Spec section 3.1: weaver-spu artifact.rs `resolve` for the first look,
residency.rs `From<AdmitRefusal> for LifecycleRefusal` for the rest, where an artifact
read is `Unreadable`, a backend this build does not carry is `BackendNotBuilt`, and the
engine taking the weights is `LoadFailed`, the last two crossing `device_cannot_admit`."""
import json
import os
import shutil

import pytest

from python_spu import engine
from python_spu.engine import AdmissionError


def refused(path):
    with pytest.raises(AdmissionError) as caught:
        engine.HFEngine(path, [0], cpu=True)
    return caught.value.kind


@pytest.fixture
def copy(tmp_path, tiny_model):
    root = tmp_path / "artifact"
    shutil.copytree(tiny_model, root)
    return root


def test_the_first_look_is_the_rust_resolve(tmp_path, oracle):
    """Absent and a path through a file are unresolvable, a FIFO is unresolvable, and a
    regular file resolves and is judged by its header, as the Rust SPU judges it.
    Perturbation: judge the path with `is_dir()` alone, as the prototype did, and the
    regular file reads unresolvable."""
    assert refused(tmp_path / "absent") == "artifact_unresolvable"
    assert oracle(op="artifact", path=str(tmp_path / "absent")) == {
        "error": "ArtifactUnresolvable"}
    (tmp_path / "file").write_bytes(b"x")
    assert refused(tmp_path / "file" / "below") == "artifact_unresolvable"
    os.mkfifo(tmp_path / "fifo")
    assert refused(tmp_path / "fifo") == "artifact_unresolvable"
    # The Rust SPU resolves and pins the same file, and reads its header next, so a
    # file of no container's bytes is unreadable there, as here.
    assert "ok" in oracle(op="artifact", path=str(tmp_path / "file"))
    assert refused(tmp_path / "file") == "artifact_unreadable"


def test_a_denied_lookup_is_unreadable(tmp_path, oracle):
    """artifact.rs `resolve`: a lookup the kernel refuses is a present artifact this
    identity cannot reach. Perturbation: answer every failed look unresolvable, and
    this reads unresolvable."""
    if os.geteuid() == 0:
        pytest.skip("no mode denies root; rerun as an unprivileged uid")
    shut = tmp_path / "shut"
    (shut / "artifact").mkdir(parents=True)
    shut.chmod(0)
    try:
        assert refused(shut / "artifact") == "artifact_unreadable"
        assert oracle(op="artifact", path=str(shut / "artifact")) == {
            "error": "ArtifactUnreadable"}
    finally:
        shut.chmod(0o755)


def test_a_sidecar_that_does_not_read_is_unreadable(copy):
    """A config the engine cannot parse is a read of the artifact before any load, the
    Rust header step's `Unreadable`. Perturbation: map every exception to
    `device_cannot_admit`, and this crosses as a device condition."""
    (copy / "config.json").write_text("{ not json")
    assert refused(copy) == "artifact_unreadable"


def test_a_tokenizer_that_does_not_read_is_load_failed(copy):
    """The Rust SPU reads `tokenizer.json` inside the native load, decoder/native.rs
    `ResidentModel::load`, mapping its failure to `LoadFailed`; only `config.json` and
    `tokenizer_config.json` are read at the header step. Perturbation: let the
    tokenizer read fall to the pre-load catch-all, and this crosses as unreadable."""
    (copy / "tokenizer.json").write_text("{ not json")
    assert refused(copy) == "device_cannot_admit"


def test_a_failure_taking_the_weights_is_load_failed(copy, monkeypatch):
    """The engine taking the weights is step four, whose failure the Rust SPU maps
    `LoadFailed` -> `DeviceCannotAdmit`. Perturbation: map every exception to
    `artifact_unreadable`, as the prototype did, and this crosses as unreadable."""
    def broken(*args, **kwargs):
        raise RuntimeError("the engine could not take the weights")
    monkeypatch.setattr(engine, "pinned_tensors", broken)
    assert refused(copy) == "device_cannot_admit"


def test_a_quantized_artifact_is_one_the_engine_cannot_take(copy):
    """A quantized safetensors artifact fails in the Rust SPU inside the native load,
    step four, so it crosses as `LoadFailed` does."""
    config = json.loads((copy / "config.json").read_text())
    config["quantization_config"] = {"quant_method": "bitsandbytes", "load_in_8bit": True}
    (copy / "config.json").write_text(json.dumps(config))
    assert refused(copy) == "device_cannot_admit"


def gguf(architecture=None, template=None, truncate=False):
    """A GGUF header with no tensors and the key-value pairs named."""
    import struct

    def text(value):
        data = value.encode()
        return struct.pack("<Q", len(data)) + data
    pairs = []
    if architecture is not None:
        pairs.append(text("general.architecture") + struct.pack("<I", 8) + text(architecture))
    if template is not None:
        pairs.append(text("tokenizer.chat_template") + struct.pack("<I", 8) + text(template))
    data = b"GGUF" + struct.pack("<IQQ", 3, 0, len(pairs)) + b"".join(pairs)
    return data[:-3] if truncate else data


def safetensors(header):
    import struct
    body = json.dumps(header).encode()
    return struct.pack("<Q", len(body)) + body


@pytest.mark.parametrize("name,data,kind", [
    ("malformed", gguf("qwen2", truncate=True), "artifact_unreadable"),
    ("no-architecture", gguf(), "artifact_unreadable"),
    ("unknown-family", gguf("mystery"), "artifact_unreadable"),
    ("contested-without-template", gguf("llama"), "artifact_unreadable"),
    ("served-elsewhere", gguf("qwen3"), "device_cannot_admit"),
    ("qwen2-gguf", gguf("qwen2"), "device_cannot_admit"),
])
def test_a_gguf_is_judged_by_its_header_first(tmp_path, oracle, name, data, kind):
    """weaver-spu reads the header at step two, before `BackendNotBuilt` can answer at
    step four: malformed bytes and a family the registry does not hold are unreadable,
    a contested family with no template is `TemplateAbsent`, and a header the free
    steps pass meets the missing backend. Found by Codex on #47. The Rust header read is
    executed on the same bytes. Perturbation: refuse every GGUF `device_cannot_admit`
    before the header, as the act's first commit did, and the unreadable cases fail."""
    root = tmp_path / name
    root.mkdir()
    (root / "model.gguf").write_bytes(data)
    rust = oracle(op="header", path=str(root))
    if kind == "artifact_unreadable" and name in ("malformed", "no-architecture"):
        assert rust == {"error": "ArtifactUnreadable"}, rust
    else:
        assert rust["ok"]["container"] == "Gguf", rust
    assert refused(root) == kind


def test_a_file_reference_is_judged_by_its_header_first(tmp_path, oracle):
    """A file reference resolves in the Rust SPU and its header is read before any
    backend answers: malformed bytes are unreadable, a safetensors file naming qwen2
    beside its sidecar meets the backend this build lacks."""
    bad = tmp_path / "bad.safetensors"
    bad.write_bytes(b"\x05\x00")
    assert oracle(op="header", path=str(bad)) == {"error": "ArtifactUnreadable"}
    assert refused(bad) == "artifact_unreadable"
    good = tmp_path / "good" / "model.safetensors"
    good.parent.mkdir()
    good.write_bytes(safetensors({"__metadata__": {"format": "pt"}}))
    (good.parent / "config.json").write_text(json.dumps({"model_type": "qwen2"}))
    assert oracle(op="header", path=str(good))["ok"]["family"] == "qwen2"
    assert refused(good) == "device_cannot_admit"


def test_a_directorys_header_and_sidecars_refuse_before_the_load(copy, oracle):
    """The safetensors header and its sidecars, `config.json` and
    `tokenizer_config.json`, are read at step two in the Rust SPU, a present one that
    does not parse refusing unreadable, before any device or load. Perturbation: drop
    the header read, and the corrupt tokenizer_config.json admits."""
    (copy / "tokenizer_config.json").write_text("{ not json")
    assert oracle(op="header", path=str(copy)) == {"error": "ArtifactUnreadable"}
    assert refused(copy) == "artifact_unreadable"


def test_the_registry_families_are_the_rust_registrys(oracle):
    assert oracle(op="registry_families") == {"ok": engine.REGISTRY_FAMILIES}


@pytest.mark.parametrize("name,header", [
    ("nan", b'{"__metadata__": {"model_type": "qwen2", "x": NaN}}'),
    ("infinity", b'{"__metadata__": {"model_type": "qwen2", "x": -Infinity}}'),
    ("utf16", '{"__metadata__": {"model_type": "qwen2"}}'.encode("utf-16")),
    ("surrogate", b'{"__metadata__": {"model_type": "qwen2", "x": "\\ud800"}}'),
])
def test_a_header_serde_refuses_is_unreadable(tmp_path, oracle, name, header):
    """The Rust header is read by serde_json, which refuses JSON's non-standard
    constants, any encoding but UTF-8 and an unpaired surrogate escape, each of which
    CPython's `json` accepts. A file reference naming qwen2 would otherwise pass its
    header and meet the backend. Found by Codex on #47. Perturbation: parse with
    `json.loads` as before, and each case reaches `device_cannot_admit`."""
    import struct
    path = tmp_path / f"{name}.safetensors"
    path.write_bytes(struct.pack("<Q", len(header)) + header)
    assert oracle(op="header", path=str(path)) == {"error": "ArtifactUnreadable"}
    assert refused(path) == "artifact_unreadable"


def test_a_header_read_that_fails_is_unreadable(tmp_path, monkeypatch):
    """artifact.rs maps every `read_exact` failure to `ArtifactUnreadable`, so a read
    the kernel fails after the pin is a refusal and never a fault. Found by Codex on
    #47. Perturbation: let `pread`'s OSError escape, and this raises it."""
    import errno
    path = tmp_path / "model.safetensors"
    path.write_bytes(b"\x00" * 16)
    def failing(fd, n, at):
        raise OSError(errno.EIO, "I/O error")
    monkeypatch.setattr(engine.os, "pread", failing)
    assert refused(path) == "artifact_unreadable"
