"""The model-free walk of python-spu-Spec section 3.1: `knobs`, `stops`, `registry` and
`partition`, each comparing python-spu's port with the weaver-spu item the oracle runs
at its pinned commit, per section 6."""
import json
import math
import re
from pathlib import Path

import pytest

from python_spu import engine, family, sampling
from python_spu.measurement import PartitionDefect, prompt_partition

MAIN = Path(__file__).resolve().parents[2] / "crates" / "weaver-spu" / "src" / "main.rs"


def wire(dispositions):
    return {name: "tunable" if d[0] == "tunable" else {"frozen": d[1]}
            for name, d in dispositions.items()}


ALL = {**sampling.KNOBS, **sampling.SESSION_PARAMETERS}
SUPPLIED = [
    {"seed": 11, "context-capacity": 256, "max-tokens-per-turn": 4},
    {"seed": 0, "context-capacity": 2**32 - 1, "max-tokens-per-turn": 2**63},
    {},
    {"seed": 11},
    {"seed": 11, "context-capacity": 256},
    {"seed": 1.5, "context-capacity": 256, "max-tokens-per-turn": 4},
    {"seed": -1, "context-capacity": 256, "max-tokens-per-turn": 4},
    {"seed": 2**64, "context-capacity": 256, "max-tokens-per-turn": 4},
    {"seed": 11, "context-capacity": 2**32, "max-tokens-per-turn": 4},
    {"seed": 11, "context-capacity": 256, "max-tokens-per-turn": 2**64},
    {"seed": 11, "context-capacity": 256, "max-tokens-per-turn": 4, "temperature": 9.0},
]
TUNED = {**ALL, "temperature": sampling.TUNABLE, "top-k": sampling.TUNABLE}


def python_knobs(dispositions, supplied):
    try:
        effective = sampling.resolve(dispositions, supplied)
    except sampling.KnobRefusal as refused:
        if refused.kind == "unsupplied":
            return {"refused": {"unsupplied": refused.knob}}
        return {"refused": {"not_a_count": {"knob": refused.knob, "supplied": refused.supplied}}}
    return {"effective": effective,
            "tunable": [name for name, d in dispositions.items() if d[0] == "tunable"]}


@pytest.mark.parametrize("dispositions", [ALL, TUNED])
@pytest.mark.parametrize("supplied", SUPPLIED)
def test_knobs_resolve_as_the_rust_resolves(oracle, dispositions, supplied):
    """sampling.rs `Knobs::resolve` then `SessionParameters::resolve`: the effective
    values, f32 knobs at single precision, the tunable names, and the first refusal.
    Perturbation: admit a count at its ceiling, and the at-the-bound cases differ."""
    supplied = {k: float(v) for k, v in supplied.items()}
    rust = oracle(op="knobs", dispositions=wire(dispositions), supplied=supplied)
    assert rust == {"ok": python_knobs(dispositions, supplied)}


def test_the_dispositions_are_the_binarys(oracle):
    """main.rs `KNOBS` and `SESSION_PARAMETERS` are the binary's, so no call reaches
    them: their literal is read, and python-spu's table is held to it. Perturbation:
    freeze a knob differently in `sampling.KNOBS`, and this fails."""
    source = MAIN.read_text()
    found = {}
    for block in ("KNOBS: Knobs", "SESSION_PARAMETERS: SessionParameters"):
        body = source[source.index(block):]
        body = body[:body.index("};")]
        for field, value in re.findall(r"(\w+): Disposition::(\w+(?:\([^)]*\))?)", body):
            name = field.replace("_", "-")
            frozen = re.fullmatch(r"Frozen\((.*)\)", value)
            found[name] = sampling.TUNABLE if value == "OperatorTunable" else \
                sampling.FROZEN(json.loads(frozen.group(1)))
    assert found == ALL


STOP_CASES = [
    (["<|im_end|>"], 2, {"<|im_end|>": [2]}),
    (["<|im_end|>"], 7, {"<|im_end|>": [2]}),
    (["<|im_end|>", "<|endoftext|>"], 9, {"<|im_end|>": [2], "<|endoftext|>": [9]}),
    (["<|eot_id|>", "<|eom_id|>"], 4, {"<|eot_id|>": [3], "<|eom_id|>": [5, 6]}),
    (["<turn|>"], 1, {}),
]


@pytest.mark.parametrize("declared,eos,vocabulary", STOP_CASES)
def test_stops_promote_as_the_rust_promotes(oracle, declared, eos, vocabulary):
    """residency.rs `promote_stop_conditions`: one-token conditions stop, the first is
    the terminator, a split one is named unpromoted, the artifact's end of sequence is
    added, and an unpromoted turn close faults."""
    rust = oracle(op="stops", declared=declared, eos=eos, vocabulary=vocabulary)["ok"]
    try:
        python = engine.promote_stop_conditions(declared, eos, lambda c: vocabulary.get(c, []))
    except engine.StopFault:
        assert "fault" in rust, rust
        return
    assert rust == python


def test_the_engine_stops_on_the_artifacts_end_of_sequence(tiny_model, tmp_path, oracle):
    """The engine's stop set is the family's promoted against the artifact, its own end
    of sequence beside the turn close, as the Rust residency's `stop_set`. The tiny
    model's eos is set apart from `<|im_end|>` here. Perturbation: keep the terminator
    as the only stop, as the prototype did, and the eos is missing."""
    import shutil
    root = tmp_path / "eos-apart"
    shutil.copytree(tiny_model, root)
    config = json.loads((root / "config.json").read_text())
    config["eos_token_id"] = 7
    (root / "config.json").write_text(json.dumps(config))
    loaded = engine.HFEngine(root, [0], cpu=True)
    try:
        rust = oracle(op="stops", declared=["<|im_end|>"], eos=7,
                      vocabulary={"<|im_end|>": loaded.tokenize("<|im_end|>")})["ok"]
        assert loaded.stop_tokens == rust["tokens"] == [2, 7]
        assert loaded.terminator == rust["terminator"]
    finally:
        loaded.close()


REGISTRY_CASES = [
    ("qwen2", None), ("Qwen-2", None), ("QWEN.2", None), ("qwen_2", None),
    ("qwen3", None), ("mystery", None), ("llama", None), ("phi3", None),
]


@pytest.mark.parametrize("name,template", REGISTRY_CASES)
@pytest.mark.parametrize("width", [1, 2, 3])
def test_this_build_refuses_exactly_where_rust_selects_another(oracle, name, template, width):
    """family/mod.rs `select` and `judge_width`, with the key's normalisation: this
    build admits exactly where the Rust registry selects qwen2 at a width it declares
    and the width is the one device this build serves. Where the Rust selects another
    family or refuses the name, it refuses unreadable; where the Rust refuses the width,
    or admits a width this build does not serve, it refuses `device_cannot_admit`.
    Perturbation: compare the family by spelling rather than by key, and the respelled
    qwen2 cases refuse."""
    rust = oracle(op="registry", family=name, template=template, width=width, readout=True)["ok"]
    assert rust["key"] == family.normalised_key(name)
    if "refused" in rust or rust["selected"] != "qwen2":
        expected = "artifact_unreadable"
    elif rust["width_refused"] is not None or width != 1:
        expected = "device_cannot_admit"
    else:
        expected = None
        assert rust["readout_refused"] is None, "the readout this build serves is the Rust's"
    assert family.judge(name, width) == expected


PARTITIONS = [
    ([], 0), ([], 3), ([4, 8, 11], 11), ([4, 8, 10], 11), ([4, 2, 11], 11),
    ([0, 0, 5], 5), ([5], 5), ([3, 3], 4),
]


@pytest.mark.parametrize("offsets,text_len", PARTITIONS)
def test_the_partition_is_the_rusts(oracle, offsets, text_len):
    """measurement.rs `PromptPartition::new`: accepted with its accessors, or its
    defect. Perturbation: refuse equal neighbours as not ascending, and `[0, 0, 5]`
    differs."""
    rust = oracle(op="partition", offsets=offsets, text_len=text_len)["ok"]
    try:
        python = prompt_partition(offsets, text_len)
    except PartitionDefect as defect:
        python = {"defect": {"kind": defect.kind, **defect.fields}}
    assert rust == python


def test_a_generation_stops_at_the_artifacts_end_of_sequence(instruction):
    """**A drift this walk found, not plumbing.** The Rust sampler loop stops on any
    token of the residency's `StopSet`, the artifact's end of sequence among them, where
    python-spu stopped on the turn close alone and ran past an eos to the token cap.
    The fake engine draws token 1 every time and declares it the end of sequence beside
    the terminator 2: the generation stops before emitting anything, completed, the
    terminator made resident. Perturbation: stop on the terminator alone, as the
    prototype did, and the generation runs to its cap emitting the eos."""
    from python_spu.session import Session

    class Engine:
        logits = [-1000., 0., -1000.]
        norms = []
        terminator = 2
        stop_tokens = [2, 1]
        artifact = weights_hash = "fake"

        def tokenize(self, _): return [0]
        def append(self, _): pass
        def rebuild(self, _): pass
        def detokenize(self, tokens): return "".join(map(str, tokens))

    session = Session(Engine(), instruction.decoder, 64, 8, 11)
    session.open([])
    body = session.generate("t", [], lambda _: None)["body"]
    assert body["measurement"]["output_tokens"] == []
    assert body["finish"] == "completed"
    assert session.resident[-1] == 2
