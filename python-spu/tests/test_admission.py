"""Admission walked against the Rust decode SPU, per python-spu-Spec sections 1 and 3:
the instruction's classify member accepted and left unread, and context-capacity held
to the ceiling weaver-spu sampling.rs resolves it against."""
import pytest

from python_spu import server
from python_spu.client import LocalProcess
from python_spu.wire import dump


def with_classify(instruction, artifact):
    body = dump(instruction)
    body["decoder"]["model-binding"]["artifact"] = str(artifact)
    body["classify"] = {"model-binding": {"artifact": "/nonexistent/classifier", "devices": [0]}}
    return body


def admit(tmp_path, body):
    process = LocalProcess(tmp_path / "stderr.txt")
    try:
        return process.ask({"kind": "admit", "instruction": body})
    finally:
        assert process.close() == 0, (tmp_path / "stderr.txt").read_text()


def test_the_classify_member_crosses_the_wire_as_the_rust_reads_it(oracle, instruction):
    """weaver-types serde takes the member and gives it back whole, so the Rust decode
    SPU receives it at admission and its admit arm (main.rs, reading only
    `instruction.decoder`) leaves it unread."""
    body = with_classify(instruction, "fixture")
    assert oracle(op="round", type="SpuInstruction", body=body) == {"ok": body}


def test_the_classify_member_is_accepted_and_left_unread(tmp_path, tiny_model, instruction):
    """An instruction carrying a classify member admits, its binding naming an artifact
    that does not exist, so nothing of it was read. Perturbation: put back the refusal
    of a classify member, and this answers artifact_unreadable."""
    answer = admit(tmp_path, with_classify(instruction, tiny_model))
    assert answer["payload"] == {"kind": "answer", "body": {"kind": "admitted"}}, (
        (tmp_path / "stderr.txt").read_text())


def test_context_capacity_past_a_u32_refuses_naming_the_field(tmp_path, tiny_model,
                                                              instruction, oracle):
    """2**32 is the first value sampling.rs refuses as NotACount, and main.rs answers it
    config_invalid with the field `tunable-values.context-capacity`. The refusal's whole
    envelope round-trips through the Rust OrganEnvelope unchanged, so its shape is the
    Rust's. Perturbation: put back the 2**64 ceiling, and this admits or refuses as
    device_cannot_admit instead."""
    body = dump(instruction)
    body["decoder"]["model-binding"]["artifact"] = str(tiny_model)
    body["decoder"]["tunable-values"]["context-capacity"] = 2**32
    answer = admit(tmp_path, body)
    assert answer["payload"] == {"kind": "refusal", "body": {
        "kind": "config_invalid", "field": "tunable-values.context-capacity"}}
    assert oracle(op="round", type="OrganEnvelope", body=answer) == {"ok": answer}


class Engine:
    max_context = 1 << 40

    def __init__(self, artifact, devices, cpu=False, readout=False, headroom=None):
        pass

    def close(self):
        pass


@pytest.mark.parametrize("capacity,admits", [(2**32 - 1, True), (2**32, False),
                                             (2**33, False), (2**64 - 1, False)])
def test_the_context_capacity_ceiling_is_exclusive_at_two_to_the_32(monkeypatch, instruction,
                                                                     capacity, admits):
    """The last u32 passes the bound and the first value past it refuses, judged before
    the engine is built, as sampling.rs judges before the cast."""
    sessions = []
    monkeypatch.setattr(server, "Session", lambda *args: sessions.append(args[2]))
    instruction.decoder.tunable_values["context-capacity"] = float(capacity)
    service = server.Service(cpu=True, engine_factory=Engine)
    if admits:
        service.admit(instruction)
        assert sessions == [capacity]
    else:
        with pytest.raises(server.AdmissionError) as caught:
            service.admit(instruction)
        assert (caught.value.kind, caught.value.fields) == (
            "config_invalid", {"field": "tunable-values.context-capacity"})
        assert sessions == []
