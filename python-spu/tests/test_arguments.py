"""The worker's argument vector, the room judgment and the determinism environment, per
python-spu-Spec sections 3 and 8, each ported from the Rust SPU."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

import pytest

from python_spu import engine, server
from python_spu.client import LocalProcess
from python_spu.engine import AdmissionError, HEADROOM_BYTES, U64_MAX
from python_spu.server import BadParameter, parameters
from python_spu.wire import dump

ROOT = Path(__file__).resolve().parents[1]
LIFECYCLE = ROOT.parent / "crates" / "weaver-harness" / "src" / "lifecycle.rs"


def spu_arguments(headroom_bytes):
    """weaver-harness lifecycle.rs `OrganParameters::spu_arguments`, mirrored: a stated
    headroom travels as the named flag and its value, and nothing stated is nothing."""
    return [] if headroom_bytes is None else ["--headroom-bytes", headroom_bytes]


def test_the_mirror_is_the_workers_rule():
    """The mirror above is checked against the Rust it copies, so a renamed flag on the
    worker's side fails here rather than at an agent's first load."""
    source = LIFECYCLE.read_text()
    assert 'arguments.push("--headroom-bytes".to_string());' in source
    assert "arguments.push(headroom.clone());" in source


def test_the_workers_vector_is_accepted():
    assert parameters(spu_arguments(None)) == (HEADROOM_BYTES, False, None)
    assert parameters(spu_arguments("268435456")) == (268435456, False, None)
    assert HEADROOM_BYTES == 512 * 1024 * 1024


def test_the_whole_vector_is_judged():
    """weaver-spu main.rs `the_whole_vector_is_judged`, case for case, with the refusals'
    own words. Perturbations: take the first `--headroom-bytes` and return, and the
    trailing unknown passes. Keep the first of two, and stated twice passes. Accept an
    unknown parameter, and `--bogus` passes."""
    def refused(vector, detail):
        with pytest.raises(BadParameter) as caught:
            parameters(vector)
        assert str(caught.value) == detail
    assert parameters([]) == (HEADROOM_BYTES, False, None)
    assert parameters(["--headroom-bytes", "1024"])[0] == 1024
    refused(["--headroom-bytes", "1024", "--bogus"], "unknown parameter --bogus")
    refused(["--headroom-bytes", "1024", "--headroom-bytes", "2048"],
            "--headroom-bytes is stated twice")
    refused(["--headroom-bytes"], "--headroom-bytes takes a value")
    refused(["--headroom-bytes", "many"], "--headroom-bytes wants a byte count, got many")
    refused(["--help"], "unknown parameter --help")


def test_a_byte_count_is_what_rusts_u64_parse_accepts():
    """Rust's `u64` parse takes ASCII digits and an optional leading plus sign, up to
    2^64 - 1. Python's `int` takes more, so the port does not use it alone."""
    assert parameters(["--headroom-bytes", "+5"])[0] == 5
    assert parameters(["--headroom-bytes", "007"])[0] == 7
    assert parameters(["--headroom-bytes", str(U64_MAX)])[0] == U64_MAX
    for value in (str(U64_MAX + 1), "-1", " 5", "5 ", "5_0", "\u0665", "", "+", "0x10"):
        with pytest.raises(BadParameter):
            parameters(["--headroom-bytes", value])


# Rust's `u64::from_str` on each value, measured by running it on 2026-09-29.
RUST_U64 = [("+", None), ("+0", 0), ("+0005", 5), ("-0", None), ("0", 0), ("", None),
            (" 5", None), ("5_0", None), (str(U64_MAX), U64_MAX), (str(U64_MAX + 1), None),
            ("0" * 5000 + "1", 1), ("+" + "0" * 5000 + "7", 7), ("9" * 5000, None)]


@pytest.mark.parametrize("value,rust", RUST_U64, ids=[v[:12] for v, _ in RUST_U64])
def test_the_byte_count_is_rusts_u64_parse_case_for_case(value, rust):
    """CPython's int refuses more than 4300 digits, where Rust reads 5000 zeros and a 1 as
    1. Perturbation: judge the value with int() alone, and the long cases raise instead."""
    assert server.byte_count(value) == rust


def test_an_overlong_value_is_refused_in_the_one_line_form():
    done = run_entry(["--headroom-bytes", "9" * 5000])
    assert (done.returncode, done.stdout) == (1, "")
    assert json.loads(done.stderr)["refusal"] == "bad_parameter"


def test_python_spus_own_flags_follow_the_same_rules():
    assert parameters(["--cpu-experiment", "--headroom-bytes", "1"]) == (1, True, None)
    assert parameters(["--declare-imports", "x", "--cpu-experiment"]) == (HEADROOM_BYTES, True, "x")
    for vector in (["--cpu-experiment", "--cpu-experiment"], ["--declare-imports"],
                   ["--declare-imports", "a", "--declare-imports", "b"]):
        with pytest.raises(BadParameter):
            parameters(vector)


def run_entry(arguments, environment=None):
    """The server's entry with no channels inherited: a refusal must come before it
    adopts any, so none are needed to see it."""
    env = dict(os.environ, PYTHONPATH=str(ROOT / "src"))
    env.update(environment or {})
    return subprocess.run([sys.executable, "-m", "python_spu.server", *arguments],
                          capture_output=True, text=True, timeout=120, env=env)


@pytest.mark.parametrize("vector,detail", [
    (["--headroom-bytes"], "--headroom-bytes takes a value"),
    (["--headroom-bytes", "many"], "--headroom-bytes wants a byte count, got many"),
    (["--headroom-bytes", "1", "--headroom-bytes", "2"], "--headroom-bytes is stated twice"),
    (["--bogus"], "unknown parameter --bogus"),
])
def test_a_refused_vector_is_one_line_and_exit_1_before_anything(vector, detail):
    """As the Rust SPU's main refuses: one JSON line on stderr, nothing on stdout, exit
    1, before the channels are adopted. The entry here has no descriptors 3 and 4, so
    reaching adoption would answer a different line."""
    done = run_entry(vector)
    assert (done.returncode, done.stdout) == (1, "")
    assert json.loads(done.stderr) == {"refusal": "bad_parameter", "detail": detail}


def test_the_workers_vector_admits_through_the_process(tmp_path, tiny_model, instruction):
    """The worker's exact vector, with a headroom stated, reaches a served admission."""
    process = LocalProcess(tmp_path / "stderr.txt",
                           arguments=["--cpu-experiment", *spu_arguments("268435456")])
    try:
        body = dump(instruction)
        body["decoder"]["model-binding"]["artifact"] = str(tiny_model)
        answer = process.ask({"kind": "admit", "instruction": body})
        assert answer["payload"] == {"kind": "answer", "body": {"kind": "admitted"}}, (
            (tmp_path / "stderr.txt").read_text())
    finally:
        assert process.close() == 0, (tmp_path / "stderr.txt").read_text()


def test_the_headroom_reaches_the_engine(tiny_model, instruction):
    seen = {}

    class Engine:
        max_context = 1 << 20

        def __init__(self, artifact, devices, cpu=False, readout=False, headroom=None):
            seen["headroom"] = headroom

        def close(self):
            pass

    service = server.Service(cpu=True, engine_factory=Engine, headroom=12345)
    try:
        service.admit(instruction)
    except Exception:
        pass
    assert seen == {"headroom": 12345}


# The room judgment, weaver-spu gpu/mod.rs, ported.

def test_room_admits_at_exactly_the_shard_and_headroom_and_refuses_one_byte_under():
    """Perturbations: `free <= needed` refuses the exact fit, and dropping the headroom
    admits the byte-short device."""
    engine.judge_room(0, free=1000 + 24, total=4096, shard_bytes=1000, headroom=24)
    with pytest.raises(AdmissionError) as caught:
        engine.judge_room(1, free=1000 + 23, total=4096, shard_bytes=1000, headroom=24)
    assert caught.value.kind == "device_cannot_admit" and caught.value.fields == {}
    assert str(caught.value) == "no room on device 1: free 1023, needed 1024, total 4096"


def test_the_needed_sum_saturates_as_rusts_does():
    with pytest.raises(AdmissionError) as caught:
        engine.judge_room(0, free=U64_MAX - 1, total=U64_MAX, shard_bytes=U64_MAX, headroom=5)
    assert f"needed {U64_MAX}," in str(caught.value)


@pytest.fixture
def a_device(monkeypatch):
    """One CUDA device as the engine sees it, with the free memory the test sets and the
    load replaced by a stop at its first step, the read through the pins, so no test here
    touches a card however the judgment is perturbed."""
    import torch
    state = {"free": 0}
    monkeypatch.setattr(torch.cuda, "is_available", lambda: True)
    monkeypatch.setattr(torch.cuda, "device_count", lambda: 1)
    monkeypatch.setattr(torch.cuda, "mem_get_info", lambda device=None: (state["free"], 1 << 40))

    def stop(*args, **kwargs):
        raise RuntimeError("reached the load")
    monkeypatch.setattr(engine, "pinned_tensors", stop)
    return state


def test_the_engine_judges_room_before_the_load_with_the_headroom_it_was_given(tiny_model, a_device):
    """The shard is the container's size, one device. Perturbations: judge after the
    load, and the refusal never comes, or ignore the headroom, and the short device
    reaches the load."""
    shard = os.stat(Path(tiny_model) / "model.safetensors").st_size
    a_device["free"] = shard + 4096 - 1
    with pytest.raises(AdmissionError) as caught:
        engine.HFEngine(tiny_model, [0], headroom=4096)
    assert caught.value.kind == "device_cannot_admit"
    assert f"needed {shard + 4096}," in str(caught.value)
    a_device["free"] = shard + 4096
    with pytest.raises(AdmissionError) as caught:
        engine.HFEngine(tiny_model, [0], headroom=4096)
    assert (caught.value.kind, str(caught.value)) == ("device_cannot_admit", "load: reached the load")


def test_a_refusal_before_the_load_touches_no_device(tiny_model, a_device, monkeypatch):
    """Nothing was placed, so the refusal's cleanup asks the driver nothing: with the
    driver's context and cache calls made to raise, the room refusal is still the answer.
    Perturbation: free the cache whenever the device string names CUDA, and the raise
    replaces the refusal, which is how a card-hidden run failed at 01c46735."""
    import torch

    def touched(*args, **kwargs):
        raise AssertionError("a refusal before the load touched the device")
    monkeypatch.setattr(torch.cuda, "device", touched)
    monkeypatch.setattr(torch.cuda, "empty_cache", touched)
    a_device["free"] = 0
    with pytest.raises(AdmissionError) as caught:
        engine.HFEngine(tiny_model, [0], headroom=4096)
    assert caught.value.kind == "device_cannot_admit"


def test_a_cpu_experiment_judges_no_room(tiny_model, monkeypatch):
    """A CPU experiment has no device, so the driver is never asked. Perturbation: judge
    in CPU mode too, and the refusing driver refuses it."""
    import torch

    def refuse(device=None):
        raise AssertionError("a CPU experiment asked the driver for room")
    monkeypatch.setattr(torch.cuda, "mem_get_info", refuse)
    engine.HFEngine(tiny_model, [0], cpu=True, headroom=U64_MAX).close()


# The container resolution, weaver-spu artifact.rs, ported.

def directory(tmp_path, names, sizes=None):
    root = tmp_path / "artifact"
    root.mkdir()
    for name in names:
        (root / name).write_bytes(b"x" * (sizes or {}).get(name, 1))
    return root


def test_one_container_resolves_and_its_size_is_the_shard(tmp_path):
    root = directory(tmp_path, ["model.safetensors", "config.json", "tokenizer.json"],
                     {"model.safetensors": 77})
    assert engine.containers(root) == [root / "model.safetensors"]


def test_a_whole_split_resolves_in_shard_order(tmp_path):
    names = ["model-00002-of-00003.safetensors", "model-00001-of-00003.safetensors",
             "model-00003-of-00003.safetensors", "model.safetensors.index.json"]
    root = directory(tmp_path, names)
    assert [p.name for p in engine.containers(root)] == [
        "model-00001-of-00003.safetensors", "model-00002-of-00003.safetensors",
        "model-00003-of-00003.safetensors"]


@pytest.mark.parametrize("names,kind", [
    ([], "artifact_unresolvable"),
    (["config.json"], "artifact_unresolvable"),
    (["a.safetensors", "b.safetensors"], "artifact_unresolvable"),
    (["model.safetensors", "model.gguf"], "artifact_unresolvable"),
    (["model-00001-of-00003.safetensors", "model-00002-of-00003.safetensors"],
     "artifact_unresolvable"),
    (["model-00001-of-00002.safetensors"], "artifact_unresolvable"),
    (["model-00001-of-00002.safetensors", "other-00002-of-00002.safetensors"],
     "artifact_unresolvable"),
    (["model-00000-of-00001.safetensors", "x.safetensors"], "artifact_unresolvable"),
])
def test_what_is_not_one_artifact_refuses(tmp_path, names, kind):
    """Perturbations: take the first container of several, and two artifacts in one
    directory resolve. Skip the pin's check that every shard is present, and a lone
    first shard resolves."""
    root = directory(tmp_path, names)
    with pytest.raises(AdmissionError) as caught:
        resolve_and_pin(root)
    assert caught.value.kind == kind


def resolve_and_pin(root):
    """The Rust admit's first two steps, resolve and pin, as python-spu runs them. Answers
    the pinned names and their summed size, closing the descriptors."""
    pinned = engine.pin(engine.containers(root))
    try:
        return [name for name, _ in pinned], engine.pinned_size(pinned)
    finally:
        for _, fd in pinned:
            os.close(fd)


def test_a_linked_container_counts_by_its_target_and_a_named_directory_does_not(tmp_path):
    target = tmp_path / "blob"
    target.write_bytes(b"x" * 9)
    root = tmp_path / "artifact"
    root.mkdir()
    (root / "model.safetensors").symlink_to(target)
    (root / "decoy.gguf").mkdir()
    assert engine.containers(root) == [root / "model.safetensors"]


def test_the_split_pattern_is_rusts(tmp_path):
    split = engine._split
    assert split("model-00001-of-00002.safetensors") == ("model", 2, ".safetensors")
    assert split("m-00003-of-00003.gguf") == ("m", 3, ".gguf")
    for name in ("model.safetensors", "model-00003-of-00002.safetensors",
                 "model-00000-of-00002.safetensors", "model-0001-of-00002.safetensors",
                 "-00001-of-00002.safetensors", "model-00001-of-00000.safetensors",
                 "model_00001-of-00002.safetensors", "model-0000a-of-00002.safetensors"):
        assert split(name) is None, name


CONTAINER_CASES = [
    ["model.safetensors", "config.json"],
    ["model.gguf"],
    ["model-00002-of-00003.safetensors", "model-00001-of-00003.safetensors",
     "model-00003-of-00003.safetensors", "model.safetensors.index.json"],
    ["model-00001-of-00002.safetensors"],
    ["model-00001-of-00003.safetensors", "model-00002-of-00003.safetensors"],
    ["model-00001-of-00002.safetensors", "other-00002-of-00002.safetensors"],
    ["model-00000-of-00001.safetensors", "x.safetensors"],
    ["a.safetensors", "b.safetensors"],
    ["model.safetensors", "model.gguf"],
    [".safetensors"],
    ["config.json"],
    [],
]


@pytest.mark.parametrize("names", CONTAINER_CASES)
def test_the_container_rule_is_the_rust_codes(tmp_path, oracle, names):
    """The port against the Rust code itself, by execution: weaver-spu's resolve and pin
    at the oracle's pinned commit, over the same directory. Each file has its own size,
    so an equal pinned length means the same members. Perturbations of the port, in
    test_what_is_not_one_artifact_refuses, fail here too."""
    root = directory(tmp_path, names, {name: 10 * (i + 1) for i, name in enumerate(names)})
    rust = oracle(op="artifact", path=str(root))
    try:
        names, size = resolve_and_pin(root)
    except AdmissionError as refused:
        assert rust == {"error": {"artifact_unresolvable": "ArtifactUnresolvable",
                                  "artifact_unreadable": "ArtifactUnreadable"}[refused.kind]}, rust
        return
    assert rust == {"ok": {"resolved": names[0], "len": size}}, (rust, names)


def test_a_gguf_resolves_and_python_spu_refuses_it(tmp_path):
    root = directory(tmp_path, ["model.gguf"])
    assert engine.containers(root) == [root / "model.gguf"]
    with pytest.raises(AdmissionError) as caught:
        engine.HFEngine(root, [0], cpu=True)
    # The fixture's bytes are no GGUF header, which the Rust SPU refuses unreadable at
    # step two before any backend answers; tests/test_load_kinds.py walks the header.
    assert caught.value.kind == "artifact_unreadable"


# The determinism environment, python-spu-Spec section 8.

OWN = [("TORCH_DISABLE_NATIVE_JIT", "1", "0"),
       ("HF_HUB_DISABLE_PROGRESS_BARS", "1", "0"),
       ("CUBLAS_WORKSPACE_CONFIG", ":4096:8", ":16:8")]


@pytest.mark.parametrize("name,value,other", OWN, ids=[n for n, _, _ in OWN])
def test_the_package_sets_its_own_environment_where_none_is_set(name, value, other):
    env = {k: v for k, v in os.environ.items() if k != name}
    env["PYTHONPATH"] = str(ROOT / "src")
    done = subprocess.run([sys.executable, "-c", "import os, sys, python_spu; "
                           f"print(os.environ[{name!r}])"],
                          capture_output=True, text=True, timeout=120, env=env)
    assert done.stdout.strip() == value, done.stderr


@pytest.mark.parametrize("name,value,other", OWN, ids=[n for n, _, _ in OWN])
def test_a_differing_value_is_refused_by_name_not_overwritten(name, value, other):
    """python-spu-Spec section 8's rule for the environment the process sets for itself.
    Perturbations: overwrite the value, as the progress-bar line first did, or accept any
    value, and the entry goes on to adopt."""
    done = run_entry([], {name: other})
    assert (done.returncode, done.stdout) == (1, "")
    refusal = json.loads(done.stderr)
    assert refusal["refusal"] == "bad_environment"
    assert f"{name} is {other!r}" in refusal["detail"]
    agreed = run_entry([], {name: value})
    assert "bad_environment" not in agreed.stderr and "python_spu_fault" in agreed.stderr


@pytest.mark.parametrize("name,value,other", OWN, ids=[n for n, _, _ in OWN])
def test_a_carried_value_is_never_overwritten(name, value, other):
    """The environment itself keeps the deployment's value after the package's import,
    so a library importer, the smoke, declare_imports or a test, sees what the deployment
    set. Perturbation: record the carried value and then overwrite it, and the refusal
    still reads the record while this fails."""
    env = dict(os.environ, PYTHONPATH=str(ROOT / "src"), **{name: other})
    done = subprocess.run([sys.executable, "-c", "import os, python_spu; "
                           f"print(os.environ[{name!r}])"],
                          capture_output=True, text=True, timeout=120, env=env)
    assert done.stdout.strip() == other, done.stderr


def test_every_differing_value_is_named_in_the_one_line():
    done = run_entry([], {name: other for name, _, other in OWN})
    detail = json.loads(done.stderr)["detail"]
    assert all(f"{name} is {other!r}" in detail for name, _, other in OWN)


def test_a_foreign_cublas_on_the_library_path_is_not_mapped(tmp_path):
    """karl's /opt/cuda/lib64 holds another cuBLAS. torch preloads its own from the
    prefix, so a library of that name earlier on LD_LIBRARY_PATH is never mapped. The
    impostor here is a copy of blake3's extension, a shared object the lock carries on
    every box, named as cuBLAS: if the loader took it, torch would
    fail to resolve cuBLAS's symbols, and its path would show in the maps. Perturbation:
    LD_PRELOAD the impostor, and the maps rule names it foreign."""
    lib = tmp_path / "lib"
    lib.mkdir()
    import blake3
    impostor = next(Path(blake3.__file__).parent.glob("*.so"))
    for name in ("libcublas.so.13", "libcublasLt.so.13"):
        shutil.copy(impostor, lib / name)
    probe = ("import python_spu, torch\n"
             "from python_spu.loaded_code import foreign_now\n"
             "maps = {l.split()[-1] for l in open('/proc/self/maps') if l.split()[-1].startswith('/')}\n"
             "print(sorted(p for p in maps if 'cublas' in p))\n"
             "print(foreign_now())\n")
    env = dict(os.environ, PYTHONPATH=str(ROOT / "src"), LD_LIBRARY_PATH=str(lib),
               CUDA_VISIBLE_DEVICES="")
    done = subprocess.run([sys.executable, "-c", probe], capture_output=True, text=True,
                          timeout=300, env=env)
    assert done.returncode == 0, done.stderr
    mapped, foreign = done.stdout.strip().splitlines()
    assert foreign == "[]", foreign
    assert str(lib) not in mapped and "nvidia/cu13/lib/libcublas.so.13" in mapped, mapped


# The pin: the size, the load and the hash read through the descriptors, weaver-spu
# artifact.rs's PinnedArtifact, so they cannot observe different files.

@pytest.fixture
def artifact(tmp_path, tiny_model):
    """A copy of the tiny model the test may change, and a second weights file of the
    same shapes and different values, as a swap would bring."""
    import torch
    from safetensors.torch import load_file, save_file
    root = tmp_path / "artifact"
    shutil.copytree(tiny_model, root)
    tensors = load_file(root / "model.safetensors")
    other = {k: (v + 1).contiguous() if v.is_floating_point() else v for k, v in tensors.items()}
    swap = tmp_path / "swap.safetensors"
    save_file(other, swap, metadata={"format": "pt"})
    return root, swap


def swapping_after_the_pin(monkeypatch, root, replacement):
    """Replaces the container's name right after the pin, before anything reads it."""
    real = engine.pin

    def pin_then_swap(members):
        pinned = real(members)
        replacement(root / "model.safetensors")
        return pinned
    monkeypatch.setattr(engine, "pin", pin_then_swap)


def parameters_of(model):
    return {k: v.clone() for k, v in model.state_dict().items()}


def test_the_pinned_load_is_the_path_load_bitwise(tiny_model):
    """The route changes where the bytes come from and nothing else: every parameter and
    buffer, and the tied embedding, equal a path load's."""
    import torch
    from transformers import AutoModelForCausalLM
    by_path = AutoModelForCausalLM.from_pretrained(tiny_model, local_files_only=True,
                                                   dtype=torch.bfloat16, attn_implementation="eager")
    served = engine.HFEngine(tiny_model, [0], cpu=True)
    try:
        a, b = by_path.state_dict(), served.model.state_dict()
        assert a.keys() == b.keys()
        assert all(a[k].dtype == b[k].dtype and torch.equal(a[k], b[k]) for k in a)
        assert all(torch.equal(x, y) for (_, x), (_, y) in
                   zip(sorted(by_path.named_buffers()), sorted(served.model.named_buffers())))
        tied = lambda m: m.lm_head.weight.data_ptr() == m.model.embed_tokens.weight.data_ptr()
        assert tied(served.model) == tied(by_path)
    finally:
        served.close()


def test_the_digest_is_the_rust_codes_where_nothing_is_swapped(tiny_model, oracle):
    served = engine.HFEngine(tiny_model, [0], cpu=True)
    try:
        assert oracle(op="weights_hash", path=str(tiny_model)) == {"ok": served.weights_hash}
    finally:
        served.close()


def test_a_swap_after_the_pin_is_never_served_or_hashed(artifact, monkeypatch, oracle):
    """Perturbations: load by name, and the served parameters are the swapped ones. Hash
    by name, and the digest is the swapped directory's."""
    import torch
    root, swap = artifact
    before = oracle(op="weights_hash", path=str(root))["ok"]
    pinned_model = engine.HFEngine(root, [0], cpu=True)
    expected = parameters_of(pinned_model.model)
    pinned_model.close()
    swapping_after_the_pin(monkeypatch, root, lambda name: os.replace(swap, name))
    served = engine.HFEngine(root, [0], cpu=True)
    try:
        got = served.model.state_dict()
        assert all(torch.equal(expected[k], got[k]) for k in expected)
        assert served.weights_hash == before
        assert served.weights_hash != oracle(op="weights_hash", path=str(root))["ok"]
        assert served.pinned == []
    finally:
        served.close()


def test_the_size_is_the_pinned_files_not_the_names(artifact, a_device, monkeypatch):
    """A larger file swapped in under the name after the pin: room is judged on the pinned
    size, so a device with exactly that room reaches the load. Perturbation: size by
    name, and it refuses."""
    root, _ = artifact
    pinned_bytes = os.stat(root / "model.safetensors").st_size
    larger = root.parent / "larger"
    larger.write_bytes(b"x" * (pinned_bytes + 4096))
    # A new file renamed over the name, as a swap is: the pinned inode is untouched.
    swapping_after_the_pin(monkeypatch, root, lambda name: os.replace(larger, name))
    a_device["free"] = pinned_bytes + 1024
    with pytest.raises(AdmissionError) as caught:
        engine.HFEngine(root, [0], headroom=1024)
    assert (caught.value.kind, str(caught.value)) == ("device_cannot_admit", "load: reached the load")


def test_a_pinned_container_the_walk_does_not_meet_is_refused(artifact, monkeypatch):
    """The name removed after the pin: the load reads the pin and succeeds, and the digest
    refuses rather than name an identity without the container."""
    root, _ = artifact
    swapping_after_the_pin(monkeypatch, root, lambda name: name.unlink())
    with pytest.raises(AdmissionError) as caught:
        engine.HFEngine(root, [0], cpu=True)
    assert caught.value.kind == "artifact_unreadable"
    assert "model.safetensors" in str(caught.value)


def test_the_pins_are_closed_on_every_path(artifact, monkeypatch):
    """Held for the admit and closed after it, whether it is served or refused."""
    root, _ = artifact
    before = set(os.listdir("/proc/self/fd"))
    engine.HFEngine(root, [0], cpu=True).close()
    swapping_after_the_pin(monkeypatch, root, lambda name: name.unlink())
    with pytest.raises(AdmissionError):
        engine.HFEngine(root, [0], cpu=True)
    assert set(os.listdir("/proc/self/fd")) == before


def test_a_symlinked_container_admits_and_hashes_as_the_rust_does(artifact, oracle):
    """hash_canonical does not follow links, so a container reached through one is left
    out of the digest, the pin still serving its load, which is #25's symlinked-member
    item. Perturbation: refuse every pinned container the walk does not meet, and this
    refuses."""
    root, _ = artifact
    blob = root.parent / "blob.safetensors"
    os.replace(root / "model.safetensors", blob)
    (root / "model.safetensors").symlink_to(blob)
    served = engine.HFEngine(root, [0], cpu=True)
    try:
        assert oracle(op="weights_hash", path=str(root)) == {"ok": served.weights_hash}
    finally:
        served.close()


def test_a_move_that_fails_part_way_is_unreachable_before_the_cache_is_freed(tiny_model, monkeypatch):
    """The cache is freed only once nothing holds the part-moved model: not the local,
    not the exception's frames. Freeing it inside the except block, with the model still
    reachable, frees nothing, 444,596,224 bytes staying reserved on the card in #754's
    measurement. Here the move fails on the host and the driver's calls are stubs, so no
    card is touched. Perturbation: free inside the except block, and the model is still
    alive when the cache is emptied."""
    import contextlib
    import gc
    import weakref
    import torch
    monkeypatch.setattr(torch.cuda, "is_available", lambda: True)
    monkeypatch.setattr(torch.cuda, "device_count", lambda: 1)
    monkeypatch.setattr(torch.cuda, "mem_get_info", lambda device=None: (1 << 40, 1 << 40))
    held = {}

    def fail_the_move(self, *args, **kwargs):
        held["model"] = weakref.ref(self)
        raise RuntimeError("forced failure part-way through the move")
    monkeypatch.setattr(torch.nn.Module, "to", fail_the_move)
    seen = []

    def empty_cache():
        gc.collect()
        seen.append(held["model"]() is None)
    monkeypatch.setattr(torch.cuda, "empty_cache", empty_cache)
    monkeypatch.setattr(torch.cuda, "device", lambda device: contextlib.nullcontext())
    with pytest.raises(AdmissionError) as caught:
        engine.HFEngine(tiny_model, [0])
    # The move is placement, step four, which crosses as the Rust `LoadFailed` does.
    assert caught.value.kind == "device_cannot_admit"
    assert caught.value.__cause__ is None and caught.value.__context__ is None
    assert seen == [True]
