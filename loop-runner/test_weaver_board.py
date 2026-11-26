"""test_weaver_board: the board client against weaver-harness-loop-contract sections 2 to
4, standard library only. Run as python3 -I -B test_weaver_board.py; exits 1 on the first
run with a failure."""
import importlib.util
import json
import os
import socket
import sys
import threading

HERE = os.path.dirname(os.path.abspath(__file__))
spec = importlib.util.spec_from_file_location("weaver_board", os.path.join(HERE, "weaver_board.py"))
wb = importlib.util.module_from_spec(spec)
sys.modules["weaver_board"] = wb
spec.loader.exec_module(wb)

FRAMES = os.path.join(HERE, "..", "crates", "weaver-harness", "tests", "fixtures", "loop-frames.json")
DIGEST = "a" * 64
failed = []
passed = 0


def check(name, ok):
    global passed
    if ok:
        passed += 1
        print("ok   " + name)
    else:
        failed.append(name)
        print("FAIL " + name)


def refused(raw):
    try:
        wb.read_frame(raw)
    except wb.Refused:
        return True
    return False


def raises(kind, f, *args, **kwargs):
    """True where f raises kind. Any other exception is a failed check, not a crash."""
    try:
        f(*args, **kwargs)
    except kind:
        return True
    except Exception as e:
        print("     raised %s: %s" % (type(e).__name__, e))
    return False


def pair():
    a, b = socket.socketpair(socket.AF_UNIX, socket.SOCK_SEQPACKET)
    a.settimeout(5)
    b.settimeout(5)
    return a, b


def recv_all(sock, timeout=0.3):
    """Every message standing on sock within timeout."""
    sock.settimeout(timeout)
    got = []
    try:
        while True:
            data = sock.recv(2 * wb.ENVELOPE)
            if not data:
                break
            got.append(data)
    except socket.timeout:
        pass
    sock.settimeout(5)
    return got


# 1. The frames file: the board judges each frame as the relay does.
cases = json.load(open(FRAMES))["cases"]


def verdict(case):
    raw = bytes.fromhex(case["bytes_hex"]) if case.get("text") is None else case["text"].encode("utf-8", "surrogatepass")
    try:
        frame = wb.read_frame(raw)
        if case["phase"] == "hello":
            wb.check_hello(frame, DIGEST)
        else:
            wb.check_loop_frame(frame, 1)
    except wb.Refused:
        return "refused"
    return "valid"


wrong = [c["name"] for c in cases if verdict(c) != ("valid" if c["verdict"] == "valid" else "refused")]
check("every frame of the frames file meets its verdict (%d cases; wrong: %s)" % (len(cases), wrong),
      len(cases) >= 80 and not wrong)

# 2. Strict reading, by the board's own code.
check("a member named twice is refused", refused(b'{"kind":"ask","kind":"ask"}'))
check("a member named twice deep inside is refused", refused(b'{"kind":"x","a":[{"b":1,"b":2}]}'))
check("-0 is no integer", type(wb.read_frame(b'{"kind":"x","n":-0}')["n"]) is wb.Number)
check("-1, 1.0, 1e2 and 2^64 are no integers",
      all(type(wb.read_frame(('{"kind":"x","n":%s}' % n).encode())["n"]) is wb.Number
          for n in ("-1", "1.0", "1e2", "18446744073709551616")))
check("2^64 - 1 is an integer", wb.read_frame(b'{"kind":"x","n":18446744073709551615}')["n"] == (1 << 64) - 1)
check("true is no integer", not wb.is_int(wb.read_frame(b'{"kind":"x","n":true}')["n"]))
check("NaN, Infinity and -Infinity are refused",
      all(refused(('{"kind":"x","n":%s}' % n).encode()) for n in ("NaN", "Infinity", "-Infinity")))
check("a lone surrogate is refused, in a value and in a key",
      refused(b'{"kind":"x","t":"\\ud800"}') and refused(b'{"kind":"x","\\udc00":1}'))
check("a surrogate pair is taken", wb.read_frame(b'{"kind":"x","t":"\\ud83d\\ude00"}')["t"] == "\U0001F600")
check("bytes that are not UTF-8 are refused", refused(b'{"kind":"\xff"}'))
check("a list is refused", refused(b'[1]'))
check("nesting past the interpreter's depth is refused, not raised", refused(b"[" * 100000))

# 3. The segment series (section 2.4).
def sent(data):
    """The messages send_frame puts on the pair for data, read while it sends."""
    a, b = pair()
    worker = threading.Thread(target=wb.send_frame, args=(a, data))
    worker.start()
    got = []
    b.settimeout(1)
    try:
        while True:
            got.append(b.recv(2 * wb.ENVELOPE))
    except socket.timeout:
        pass
    worker.join()
    return got


got = sent(b"x" * wb.ENVELOPE)
check("a frame of exactly 64 KiB is one message", [len(m) for m in got] == [wb.ENVELOPE])
body = b'{"kind":"x","p":"' + b"y" * (2 * wb.ENVELOPE - 19) + b'"}'
check("the fixture is exactly two envelopes", len(body) == 2 * wb.ENVELOPE)
got = sent(body)
check("a frame of exactly two envelopes is a preamble naming two slices, then two slices",
      got[0] == b'{"segments":2,"bytes":131072}' and [len(m) for m in got[1:]] == [wb.ENVELOPE, wb.ENVELOPE])
got = sent(body + b" ")
check("one byte more is three slices", json.loads(got[0]) == {"segments": 3, "bytes": 131073} and len(got) == 4)
a, b = pair()
check("a frame past 8 MiB is not sent", raises(wb.Limit, wb.send_frame, a, b"z" * (wb.TOTAL + 1)) and recv_all(b) == [])
worker = threading.Thread(target=wb.send_frame, args=(a, body))
worker.start()
check("a series reassembles whole", wb.recv_frame(b) == body)
worker.join()


def series_refused(messages):
    a, b = pair()

    def send():
        try:
            for m in messages:
                a.sendmsg([m])
        except OSError:
            pass
        a.close()

    worker = threading.Thread(target=send)
    worker.start()
    result = raises((wb.Refused, wb.Closed), wb.recv_frame, b)
    b.close()
    worker.join()
    return result


check("a preamble with a wrong count is refused",
      series_refused([b'{"segments":3,"bytes":131072}', b"y" * wb.ENVELOPE, b"y" * wb.ENVELOPE]))
check("a preamble under the envelope is refused", series_refused([b'{"segments":1,"bytes":100}', b"y" * 100]))
check("a kindless frame that is no preamble is refused", series_refused([b'{"pass":1}']))
check("a short series is refused", series_refused([b'{"segments":2,"bytes":131072}', b"y" * wb.ENVELOPE]))
a, b = pair()
a.sendmsg([b"q" * (wb.ENVELOPE + 1)])
check("a message past the envelope is refused", raises(wb.Refused, wb.recv_frame, b))


# 4. The board: four outstanding, turn and refuse alone, returns in arrival order.
def opened():
    loop_end, harness = pair()
    board = wb.Board(loop_end)
    board._open(1)
    return board, harness


def frames(harness):
    return [json.loads(m) for m in recv_all(harness)]


def answer(harness, tag, value, pass_=1):
    harness.sendmsg([json.dumps({"kind": "return", "pass": pass_, "tag": tag, "value": value}).encode()])


board, harness = opened()
tags = [board.ask("seated") for _ in range(4)]
check("four asks may be outstanding, tagged 0 to 3", tags == [0, 1, 2, 3] and board.outstanding == (0, 1, 2, 3))
check("a fifth outstanding ask is refused, and nothing is sent", raises(wb.Limit, board.ask, "seated")
      and len(frames(harness)) == 4)
check("turn with asks outstanding is refused, and nothing is sent",
      raises(wb.Limit, board.turn, [{"role": "user", "content": [{"type": "text", "text": "x"}]}]) and frames(harness) == [])
check("refuse with asks outstanding is refused, and nothing is sent", raises(wb.Limit, board.refuse) and frames(harness) == [])
for tag in range(4):
    answer(harness, tag, tag * 10)
check("a later tag's wait reads and keeps the earlier returns", board.wait(2) == 20 and board.outstanding == (3,))
check("the kept returns are given after", board.wait(0) == 0 and board.wait(1) == 10 and board.wait(3) == 30)
check("with nothing outstanding the pass ends by refuse", board.refuse() is None
      and frames(harness) == [{"kind": "refuse", "pass": 1}] and board.ended)
check("an ask after the pass ended is refused", raises(wb.Limit, board.ask, "seated"))

board, harness = opened()
board.ask("seated")
board.ask("seated")
answer(harness, 1, True)
check("a return out of arrival order breaks the seam", raises(wb.Refused, board.wait, 1))
board, harness = opened()
board.ask("seated")
answer(harness, 0, True, pass_=2)
check("a return for another pass breaks the seam", raises(wb.Refused, board.wait, 0))
board, harness = opened()
board.ask("seated")
harness.sendmsg([b'{"kind":"return","pass":1,"tag":0,"value":1,"value":2}'])
check("a return with a member named twice breaks the seam", raises(wb.Refused, board.wait, 0))
board, harness = opened()
board.ask("seated")
harness.sendmsg([b'{"kind":"return","pass":1,"tag":-0,"value":true}'])
check("a return whose tag is -0 breaks the seam", raises(wb.Refused, board.wait, 0))
board, harness = opened()
board.ask("seated")
harness.sendmsg([b'{"kind":"pass","pass":2,"text":"x"}'])
check("a pass frame inside a pass breaks the seam", raises(wb.Refused, board.wait, 0))
board, harness = opened()
board.ask("seated")
harness.close()
check("a closed pair while waiting is Closed", raises(wb.Closed, board.wait, 0))

board, harness = opened()
check("an ask outside the closed list is refused before it is sent",
      raises(wb.Refused, board.ask, "seed") and frames(harness) == [])
check("flush with keep -0 is refused before it is sent", raises(wb.Refused, board.ask, "flush", keep=wb.Number("-0")))
check("flush with keep True is refused before it is sent", raises(wb.Refused, board.ask, "flush", keep=True))
check("recall with no last_turns is refused before it is sent", raises(wb.Refused, board.ask, "recall"))
for name, delta in [("the role assistant", [{"role": "assistant", "content": [{"type": "text", "text": "x"}]}]),
                    ("an empty content", [{"role": "user", "content": []}]),
                    ("an empty delta", []),
                    ("a tool block", [{"role": "user", "content": [{"type": "tool_call"}]}])]:
    check("a turn with %s is refused before it is sent, the pass still open" % name,
          raises(wb.Refused, board.turn, delta) and frames(harness) == [] and not board.ended)
board.turn([{"role": "user", "content": [{"type": "text", "text": ""}]}])
sent = frames(harness)
check("a turn with an empty text ends the pass", board.ended and sent == [
    {"kind": "ask", "pass": 1, "tag": 0, "ask": "turn", "delta": [{"role": "user", "content": [{"type": "text", "text": ""}]}]}])

board, harness = opened()
tag = board.ask("recall", last_turns=None)
check("recall with last_turns null sends the member as null",
      frames(harness) == [{"kind": "ask", "pass": 1, "tag": 0, "ask": "recall", "last_turns": None}])
big = [{"kind": "message.user", "run": "r", "turn": None, "sequence": "1", "pairs": [["content", "w" * 200000]]}]
data = json.dumps({"kind": "return", "pass": 1, "tag": tag, "value": big}).encode()
worker = threading.Thread(target=wb.send_frame, args=(harness, data))
worker.start()
check("a return past the envelope reaches the wires whole", board.wait(tag) == big)
worker.join()
text = "v" * 150000


def turn_big():
    board.turn([{"role": "user", "content": [{"type": "text", "text": text}]}])


worker = threading.Thread(target=turn_big)
worker.start()
received = wb.read_frame(wb.recv_frame(harness))
worker.join()
check("a turn past the envelope crosses to the harness as a series, whole",
      received["delta"][0]["content"][0]["text"] == text)

check("clamp saturates at 2^64 - 1 as Rust's saturating_mul does",
      wb.clamp((1 << 62) * 5) == (1 << 64) - 1 and wb.clamp(12) == 12)

print("== %d passed, %d failed" % (passed, len(failed)))
sys.exit(1 if failed else 0)
