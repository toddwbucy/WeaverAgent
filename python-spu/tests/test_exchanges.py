"""The decode seam's mid-generation exchanges walked against the Rust SPU, per
python-spu-Spec section 3 and weaver-harness-spu-decode-contract sections 2 and 3: a
cancel arriving during a generation closes at rest after the generation's stopped
answer, and any other ask arriving during one is refused, typed, and not queued."""
import json
import socket
import threading

from python_spu import server
from python_spu.transport import Channel
from python_spu.wire import dump


def serving(tiny_model, instruction):
    """The real Service in a thread over two socket pairs, admitted and opened.
    Answers the harness's two ends and the thread."""
    pairs = [socket.socketpair(socket.AF_UNIX, socket.SOCK_SEQPACKET) for _ in range(2)]
    service_life, service_decode = Channel(pairs[0][1]), Channel(pairs[1][1], True)
    life, decode = Channel(pairs[0][0]), Channel(pairs[1][0], True)
    for channel in (life, decode):
        channel.sock.settimeout(20)
    thread = threading.Thread(
        target=server.Service(cpu=True).serve, args=(service_life, service_decode), daemon=True)
    thread.start()
    body = dump(instruction)
    body["decoder"]["model-binding"]["artifact"] = str(tiny_model)
    life.send({"exchange": {"opener": "harness", "ordinal": 0}, "position": "open",
               "payload": {"kind": "directive", "body": {"kind": "admit", "instruction": body}}})
    assert life.receive()["payload"] == {"kind": "answer", "body": {"kind": "admitted"}}
    decode.send({"kind": "open", "session": "s", "messages": [
        {"role": "system", "content": [{"type": "text", "text": "be precise"}]}]})
    assert decode.receive() == {"kind": "opened"}
    return life, decode, thread, pairs


def generate(decode):
    decode.send({"kind": "append_and_generate", "turn": "t", "delta": [
        {"role": "user", "content": [{"type": "text", "text": "hello world"}]}]})


def until_generated(decode):
    frames = []
    while not frames or frames[-1]["kind"] != "generated":
        frames.append(decode.receive())
    return frames


def close(life, decode, thread, pairs):
    decode.sock.shutdown(socket.SHUT_RDWR)
    life.send({"exchange": {"opener": "harness", "ordinal": 1}, "position": "open",
               "payload": {"kind": "directive", "body": {"kind": "release"}}})
    assert life.receive()["payload"] == {"kind": "answer", "body": {"kind": "released"}}
    life.sock.shutdown(socket.SHUT_RDWR)
    thread.join(20)
    for pair in pairs:
        for end in pair:
            end.close()


def test_a_cancel_in_flight_closes_at_rest_after_the_stopped_generation(
        tiny_model, instruction, oracle):
    """weaver-spu main.rs, after the generation's answer: `if cancel.cancelled` it
    sends `TokenAnswer::AtRest`. The cancel is queued behind the generate before the
    service reads either, so the first poll finds it. The generation answers stopped,
    the cancel then closes at rest, and nothing else is owed: a cancel at rest after it
    answers at rest at once. Perturbation: drop the at_rest send and the receive after
    the generation times out, nothing owed arriving."""
    life, decode, thread, pairs = serving(tiny_model, instruction)
    generate(decode)
    decode.send({"kind": "cancel", "turn": "t"})
    frames = until_generated(decode)
    assert '"finish": "stopped"' in json.dumps(frames[-1]), frames[-1]
    closed = decode.receive()
    assert closed == {"kind": "at_rest"}
    assert oracle(op="round", type="TokenAnswer", body=closed) == {"ok": closed}
    decode.send({"kind": "cancel", "turn": "t"})
    assert decode.receive() == {"kind": "at_rest"}, "the cancel exchange closed once"
    close(life, decode, thread, pairs)


def test_an_ask_in_flight_is_refused_typed_and_not_queued(tiny_model, instruction, oracle):
    """weaver-spu main.rs `SeamCancel`: a directive other than cancel polled up
    mid-flight answers `TokenRefusal::OutOfOrder` at once and the generation runs on.
    The refusal is the Rust type's own serialization, round-tripped through the oracle,
    and the flush it refused is not served later. Perturbations: drop the refusal, or
    queue the polled ask in place of refusing it, and no refusal crosses."""
    life, decode, thread, pairs = serving(tiny_model, instruction)
    generate(decode)
    decode.send({"kind": "flush", "keep": 0})
    frames = until_generated(decode)
    refusals = [frame for frame in frames if frame == {"kind": "out_of_order"}]
    assert len(refusals) == 1, frames
    assert oracle(op="round", type="TokenRefusal", body=refusals[0]) == {"ok": refusals[0]}
    assert '"finish": "stopped"' not in json.dumps(frames[-1]), "the generation ran on"
    decode.send({"kind": "cancel", "turn": "t"})
    assert decode.receive() == {"kind": "at_rest"}, "nothing refused was queued"
    close(life, decode, thread, pairs)
