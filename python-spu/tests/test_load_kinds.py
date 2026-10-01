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
    regular file, which the Rust SPU resolves and this build does not serve, crosses as
    `BackendNotBuilt` does. Perturbation: judge the path with `is_dir()` alone, as the
    prototype did, and the regular file reads unresolvable."""
    assert refused(tmp_path / "absent") == "artifact_unresolvable"
    assert oracle(op="artifact", path=str(tmp_path / "absent")) == {
        "error": "ArtifactUnresolvable"}
    (tmp_path / "file").write_bytes(b"x")
    assert refused(tmp_path / "file" / "below") == "artifact_unresolvable"
    os.mkfifo(tmp_path / "fifo")
    assert refused(tmp_path / "fifo") == "artifact_unresolvable"
    assert refused(tmp_path / "file") == "device_cannot_admit"
    # The Rust SPU resolves and pins the same file, which is what makes its refusal
    # here a backend this build lacks and not an artifact that failed to resolve.
    assert "ok" in oracle(op="artifact", path=str(tmp_path / "file"))


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
