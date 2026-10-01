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


def test_the_registry_is_the_rust_registrys(oracle):
    assert oracle(op="registry_entries") == {"ok": [
        {"family": f, "widths": list(w), "taps_readout": t} for f, w, t in engine.REGISTRY]}


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


M = b'{"__metadata__":{"model_type":"qwen2"},"x":'
CORPUS = {
    "depth 126": M + b"[" * 125 + b"]" * 125 + b"}",
    "depth 127": M + b"[" * 126 + b"]" * 126 + b"}",
    "depth 128": M + b"[" * 127 + b"]" * 127 + b"}",
    "depth 129": M + b"[" * 128 + b"]" * 128 + b"}",
    "bracket in string": M + b'"' + b"[" * 200 + b'"}',
    "1e400": M + b"1e400}", "-1e400": M + b"-1e400}", "1e308": M + b"1e308}",
    "f64 max": M + b"1.7976931348623157e308}", "1e-400": M + b"1e-400}",
    "big int": M + b"1" + b"0" * 400 + b"}", "u64 max": M + b"18446744073709551615}",
    "u64 + 1": M + b"18446744073709551616}", "i64 min - 1": M + b"-9223372036854775809}",
    "-0": M + b"-0}", "1E5": M + b"1E5}", "01": M + b"01}", "1.": M + b"1.}",
    ".5": M + b".5}", "+1": M + b"+1}", "NaN": M + b"NaN}", "Infinity": M + b"Infinity}",
    "true": M + b"true}", "control char": M + b'"a\x01b"}', "tab": M + b'"a\tb"}',
    "nul escape": M + b'"\\u0000"}', "bad escape": M + b'"\\x"}',
    "paired surrogate": M + b'"\\ud83d\\ude00"}', "lone high": M + b'"\\ud800"}',
    "lone low": M + b'"\\ude00"}', "invalid utf8": M + b'"\xff"}',
    "bom": b"\xef\xbb\xbf" + M + b"1}", "utf16": (M + b"1}").decode().encode("utf-16"),
    "empty": b"", "whitespace": b"   ", "trailing data": M + b"1} x",
    "trailing whitespace": M + b"1} \n", "trailing comma": M + b"1,}", "comment": M + b"1}//c",
    "single quote": M + b"'a'}", "duplicate keys":
        b'{"__metadata__":{"model_type":"llama"},"__metadata__":{"model_type":"qwen2"}}',
    "duplicate inner": b'{"__metadata__":{"model_type":"llama","model_type":"qwen2"}}',
    "array": b"[1]", "scalar": b"1", "metadata not an object": b'{"__metadata__":1}',
    "architecture not a string": b'{"__metadata__":{"architecture":1,"model_type":"qwen2"}}',
    "architecture first": b'{"__metadata__":{"architecture":"llama","model_type":"qwen2"}}',
}


@pytest.mark.parametrize("name", list(CORPUS))
def test_the_header_reads_as_serde_json_reads_it(tmp_path, oracle, name):
    """The whole class of Codex's #47 findings, closed by difference rather than by
    reading: every edge of JSON this corpus names is read by the Rust `read_header`
    through the oracle and by `engine.read_header`, and the two agree on the family or
    on the refusal. Perturbation: drop any one guard of `strict_json` (the depth scan,
    the number range, the constants, the surrogates, the UTF-8 decode), and a case
    here differs."""
    import struct
    data = CORPUS[name]
    path = tmp_path / "model.safetensors"
    path.write_bytes(struct.pack("<Q", len(data)) + data)
    rust = oracle(op="header", path=str(path))
    pinned = engine.pin([path])
    try:
        python = {"ok": engine.read_header(pinned[0][1])["family"]}
    except AdmissionError as refusal:
        assert refusal.kind == "artifact_unreadable"
        python = "refused"
    finally:
        for _, fd in pinned:
            os.close(fd)
    assert python == ({"ok": rust["ok"]["family"]} if "ok" in rust else "refused"), rust


def test_the_sidecars_are_read_beside_the_pinned_container(tmp_path, oracle):
    """weaver-spu artifact.rs `sidecar_dir_of`: the header reads `config.json` beside
    the file the pin holds, recovered through `/proc/self/fd`, not beside the name the
    reference gave. A container reached through a link reads its target's sidecar, as
    the Rust `read_header` does through the oracle on the same link. Found by Codex on
    #47. Perturbation: read the sidecars beside the name, and the link's own
    `config.json` names another family."""
    import struct
    header = json.dumps({"__metadata__": {"format": "pt"}}).encode()
    real, beside = tmp_path / "real", tmp_path / "beside"
    real.mkdir()
    beside.mkdir()
    (real / "model.safetensors").write_bytes(struct.pack("<Q", len(header)) + header)
    (real / "config.json").write_text(json.dumps({"model_type": "qwen2"}))
    (beside / "config.json").write_text(json.dumps({"model_type": "llama"}))
    (beside / "model.safetensors").symlink_to(real / "model.safetensors")
    rust = oracle(op="header", path=str(beside / "model.safetensors"))
    pinned = engine.pin([beside / "model.safetensors"])
    try:
        python = engine.read_header(pinned[0][1])["family"]
    finally:
        for _, fd in pinned:
            os.close(fd)
    assert rust["ok"]["family"] == "qwen2", rust
    assert python == rust["ok"]["family"], (python, rust)


def test_the_load_reads_its_sidecars_beside_the_pinned_container(tiny_model, tmp_path):
    """weaver-spu decoder/native.rs `sidecar_dir`: the load's `config.json`,
    `AutoConfig` and `tokenizer.json` are read beside the pinned container, as the
    header's are. A directory whose container links into another model's directory
    loads that model's sidecars, never the ones sitting beside the link. Perturbation:
    read them from the reference's directory, and the link's own `config.json`, a
    family this build does not serve, refuses the load."""
    linked = tmp_path / "linked"
    linked.mkdir()
    (linked / "model.safetensors").symlink_to(tiny_model / "model.safetensors")
    for name in ("config.json", "tokenizer_config.json"):
        shutil.copy(tiny_model / name, linked / name)
    declared = json.loads((tiny_model / "config.json").read_text())
    (linked / "config.json").write_text(json.dumps(dict(declared, model_type="llama")))
    loaded = engine.HFEngine(linked, [0], cpu=True)
    try:
        assert loaded.layers == 2 and loaded.terminator == 2
    finally:
        loaded.close()
