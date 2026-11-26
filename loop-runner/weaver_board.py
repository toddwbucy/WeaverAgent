"""weaver_board: the loop's end of the loop seam, per weaver-harness-loop-contract
(docs/crates/contracts/ in this repository). Standard library only.

A wire file's on_turn(board, text) asks the harness through the Board it is handed, one
wire at a time or up to four outstanding:

    board.seated()          board.turn_context()     board.first_turn()
    board.fullness()        board.flush(keep)        board.recall(last_turns)
    board.shape()           board.turn(delta)        board.refuse()

or board.ask(name, **arguments), which sends the ask and gives its tag, and
board.wait(tag), which gives its value. turn and refuse end the pass, and each is sent
only with no other ask outstanding (the contract's section 3).

The board checks each frame before it sends it, with the relay's own rules. A frame that
breaks them raises Mistake and is not sent: the runner refuses that pass, and the seam
stands for the next. Limit is the same for the board's own rules. Refused and Closed are
the harness's side: a frame it sent that breaks the contract, or its end gone. The board
keeps the first it meets as board.broken, and after it sends and waits for nothing. The
runner reads board.broken, not what on_turn raised, so a wire file can neither break the
seam by raising Refused nor keep it by catching one.

The frames are read strictly (the contract's section 2): a member named twice, an
integer written with a sign, a fraction or an exponent, NaN and Infinity, and a lone
surrogate are refused, by this module's own code, since Python's json takes each of
them. A frame over 64 KiB crosses as the segment series of section 2.4."""

import json
import socket
import types

VERSION = "weaver-loop/1"
ENVELOPE = 64 * 1024
TOTAL = 8 * 1024 * 1024
OUTSTANDING = 4
ASKS_PER_PASS = 64
INT_MAX = (1 << 64) - 1
ASKS = ("seated", "turn_context", "first_turn", "fullness", "flush", "recall", "shape", "turn")
# Read only, so no wire file can loosen the board's own checks between passes.
ARGUMENTS = types.MappingProxyType({"flush": ("keep",), "recall": ("last_turns",), "turn": ("delta",)})


class Refused(Exception):
    """A frame outside the contract. On a frame the harness sent, the seam is broken."""


class Mistake(Exception):
    """A frame the wires would send that breaks the contract, refused before the send.
    The seam is not broken: the runner refuses the pass."""


class Limit(Exception):
    """The wires broke a rule of the loop's own: a fifth outstanding ask, an ask after
    the pass ended, or turn or refuse sent with another ask outstanding."""


class Closed(Exception):
    """The harness's end of the pair closed."""


class Number:
    """A JSON number that is no integer of 0 to 2^64 - 1 written as digits: -0, -1, 1.0,
    1e2, or one past 2^64 - 1. It is no integer wherever the contract names one."""

    def __init__(self, written):
        self.written = written

    def __repr__(self):
        return "Number(%r)" % self.written


def _no_twice(pairs):
    seen = set()
    for key, _ in pairs:
        if key in seen:
            raise Refused("a member named twice in one object: %r" % key)
        seen.add(key)
    return dict(pairs)


def _integer(written):
    # Read from the literal text: json hands "-0" here, where int() would give 0.
    if written.startswith("-"):
        return Number(written)
    value = int(written)
    return value if value <= INT_MAX else Number(written)


def _constant(written):
    raise Refused("NaN or Infinity, which JSON does not have: %s" % written)


def _no_surrogates(value):
    # A walk with its own stack: nesting deeper than the interpreter's recursion limit
    # parses in json and must not raise here.
    stack = [value]
    while stack:
        item = stack.pop()
        if isinstance(item, str):
            if any(0xD800 <= ord(c) <= 0xDFFF for c in item):
                raise Refused("a string holding a lone surrogate")
        elif isinstance(item, dict):
            stack.extend(item.keys())
            stack.extend(item.values())
        elif isinstance(item, list):
            stack.extend(item)


def read_frame(raw):
    """One frame's bytes as a dict, or Refused."""
    try:
        text = raw.decode("utf-8")
    except UnicodeDecodeError:
        raise Refused("a frame that is not UTF-8")
    try:
        value = json.loads(text, object_pairs_hook=_no_twice, parse_int=_integer,
                           parse_float=Number, parse_constant=_constant)
        _no_surrogates(value)
    except (ValueError, RecursionError) as e:
        raise Refused("a frame that does not parse as JSON: %s" % e)
    if not isinstance(value, dict):
        raise Refused("a frame that is not one JSON object")
    return value


def is_int(value):
    return type(value) is int


def _exactly(frame, names):
    extra = [k for k in frame if k not in names]
    if extra:
        raise Refused("a member outside the %s frame's: %r" % (frame.get("kind"), extra[0]))
    missing = [n for n in names if n not in frame]
    if missing:
        raise Refused("the %s frame lacks its member %r" % (frame.get("kind"), missing[0]))


def _int(frame, name):
    # Read off the wire, a value past the range is already a Number. Built by the wires,
    # an int may be negative or past 2^64 - 1, so the range is checked here too.
    if not (is_int(frame[name]) and 0 <= frame[name] <= INT_MAX):
        raise Refused("the %s frame's %r is no integer of 0 to 2^64 - 1" % (frame["kind"], name))
    return frame[name]


def check_delta(delta):
    """The contract's section 2.2: at least one message of exactly role and content, the
    role system or user, and at least one block of exactly the type text and a string
    text. An empty text is legal."""
    if not isinstance(delta, list) or not delta:
        raise Refused("a delta that is not a list of at least one message")
    for message in delta:
        if not isinstance(message, dict) or set(message) != {"role", "content"}:
            raise Refused("a delta message that is not exactly role and content")
        if message["role"] not in ("system", "user"):
            raise Refused("a delta role other than system or user: %r" % (message["role"],))
        content = message["content"]
        if not isinstance(content, list) or not content:
            raise Refused("a delta message whose content is not a list of at least one block")
        for block in content:
            if not (isinstance(block, dict) and set(block) == {"type", "text"}
                    and block["type"] == "text" and isinstance(block["text"], str)):
                raise Refused("a delta block that is not exactly the type text and a string text")


def check_hello(frame, wires_sha256):
    """The loop's first frame, as the relay reads it."""
    if not isinstance(frame.get("kind"), str):
        raise Refused("a frame with no kind, or a kind that is no string")
    if frame["kind"] != "hello":
        raise Refused("the %s frame came before the hello" % frame["kind"])
    _exactly(frame, ("kind", "version", "wires_sha256"))
    if frame["version"] != VERSION:
        raise Refused("a hello whose version is not %r" % VERSION)
    if frame["wires_sha256"] != wires_sha256:
        raise Refused("a hello whose digest is not the wire file's")


def check_loop_frame(frame, open_pass):
    """A frame the loop sends inside open_pass, as the relay reads it: the board checks
    each frame it sends, so a wire file's mistake is named here and never breaks the
    seam."""
    kind = frame.get("kind")
    if not isinstance(kind, str):
        raise Refused("a frame with no kind, or a kind that is no string")
    if kind == "refuse":
        _exactly(frame, ("kind", "pass"))
    elif kind == "ask":
        if "ask" not in frame:
            raise Refused("the ask frame lacks its member 'ask'")
        name = frame["ask"]
        if not isinstance(name, str):
            raise Refused("the ask frame's 'ask' is not a string")
        if name not in ASKS:
            raise Refused("an ask outside the closed list: %r" % name)
        _exactly(frame, ("kind", "pass", "tag", "ask") + ARGUMENTS.get(name, ()))
        _int(frame, "tag")
        if name == "flush":
            _int(frame, "keep")
        elif name == "recall" and frame["last_turns"] is not None:
            _int(frame, "last_turns")
        elif name == "turn":
            check_delta(frame["delta"])
    elif kind == "hello":
        raise Refused("a second hello")
    elif kind in ("pass", "return"):
        raise Refused("the %s frame came from the loop, and only the harness sends it" % kind)
    else:
        raise Refused("an unknown kind %r" % kind)
    if _int(frame, "pass") != open_pass:
        raise Refused("a frame naming pass %r, not the open pass %d" % (frame["pass"], open_pass))


# ---- The pair: one message, and the segment series (the contract's section 2.4).

def send_message(sock, data):
    try:
        sent = sock.sendmsg([data])
    except OSError as e:
        # EPIPE or ECONNRESET: the harness's end is gone.
        raise Closed("a send to the harness failed: %s" % e)
    if sent != len(data):
        raise Closed("a send that did not finish")


def send_frame(sock, data):
    """One frame's bytes, as one message or as the segment series."""
    if len(data) <= ENVELOPE:
        send_message(sock, data)
        return
    if len(data) > TOTAL:
        raise Limit("a frame of %d bytes, past the total bound of %d" % (len(data), TOTAL))
    count = -(-len(data) // ENVELOPE)
    send_message(sock, json.dumps({"segments": count, "bytes": len(data)},
                                  separators=(",", ":")).encode("ascii"))
    for start in range(0, len(data), ENVELOPE):
        send_message(sock, data[start:start + ENVELOPE])


def recv_message(sock):
    try:
        data, _, flags, _ = sock.recvmsg(ENVELOPE)
    except OSError as e:
        # A peer that closes with frames unread gives ECONNRESET on this pair.
        raise Closed("a read from the harness failed: %s" % e)
    if flags & socket.MSG_TRUNC:
        raise Refused("a message past the envelope's 64 KiB")
    if not data:
        raise Closed("the harness's end of the pair closed")
    return data


def recv_frame(sock):
    """One frame's bytes, reassembled where it crosses as a segment series."""
    first = recv_message(sock)
    try:
        head = read_frame(first)
    except Refused:
        return first
    if "kind" in head:
        return first
    if set(head) != {"segments", "bytes"} or not (is_int(head["segments"]) and is_int(head["bytes"])):
        raise Refused("a frame with no kind that is not a segment preamble")
    total, count = head["bytes"], head["segments"]
    if not (ENVELOPE < total <= TOTAL) or count != -(-total // ENVELOPE):
        raise Refused("a segment preamble that breaks its rules: %r" % head)
    whole = b""
    for _ in range(count):
        whole += recv_message(sock)
        if len(whole) > total:
            raise Refused("a series longer than its preamble")
    if len(whole) != total:
        raise Refused("a series shorter than its preamble")
    return whole


def encode(frame):
    return json.dumps(frame, ensure_ascii=False, separators=(",", ":"),
                      allow_nan=False).encode("utf-8")


# ---- The board.

class Board:
    """The loop's handle on its end of the pair, through which every wire runs."""

    def __init__(self, sock):
        self._sock = sock
        self._pass = None
        self._ended = True
        self._next_tag = 0
        self._outstanding = []
        self._returned = {}
        self._asked = 0
        self._broken = None

    def _open(self, number):
        self._pass, self._ended, self._next_tag = number, False, 0
        self._outstanding, self._returned, self._asked = [], {}, 0

    @property
    def ended(self):
        return self._ended

    @property
    def broken(self):
        """The harness's breach the board itself met, Refused or Closed, or None."""
        return self._broken

    def _whole(self):
        if self._broken is not None:
            raise Limit("the seam broke: %s" % (self._broken,))

    @property
    def outstanding(self):
        return tuple(self._outstanding)

    def _send(self, frame):
        self._whole()
        # The relay counts every ask of a pass, the turn among them, and breaks at the
        # sixty-fifth (the contract's section 3).
        if frame.get("kind") == "ask" and self._asked >= ASKS_PER_PASS:
            raise Mistake("a sixty-fifth ask in one pass")
        try:
            check_loop_frame(frame, self._pass)
            _no_surrogates(frame)
        except Refused as e:
            raise Mistake(str(e)) from None
        try:
            send_frame(self._sock, encode(frame))
        except Closed as e:
            self._broken = e
            raise
        if frame["kind"] == "ask":
            self._asked += 1

    def ask(self, name, **arguments):
        """Send one ask of the closed list and give its tag."""
        if self._ended:
            raise Limit("an ask after the pass ended")
        if name == "turn":
            raise Limit("turn ends the pass: call board.turn(delta)")
        if len(self._outstanding) >= OUTSTANDING:
            raise Limit("a fifth outstanding ask: wait for one first")
        own = sorted(set(arguments) & {"kind", "pass", "tag", "ask"})
        if own:
            raise Mistake("the ask's own member %r given as an argument" % own[0])
        tag = self._next_tag
        frame = {"kind": "ask", "pass": self._pass, "tag": tag, "ask": name}
        frame.update(arguments)
        self._send(frame)
        self._next_tag += 1
        self._outstanding.append(tag)
        return tag

    def wait(self, tag):
        """The value of an outstanding ask. The harness answers in arrival order, so the
        returns of the asks sent before it are read and kept on the way."""
        self._whole()
        while tag not in self._returned:
            if tag not in self._outstanding:
                raise Limit("the tag %r is not outstanding" % (tag,))
            try:
                frame = read_frame(recv_frame(self._sock))
                if frame.get("kind") != "return":
                    raise Refused("a frame of the kind %r inside a pass" % (frame.get("kind"),))
                _exactly(frame, ("kind", "pass", "tag", "value"))
                if _int(frame, "pass") != self._pass or _int(frame, "tag") != self._outstanding[0]:
                    raise Refused("a return for pass %r and tag %r out of order" % (frame["pass"], frame["tag"]))
            except (Refused, Closed) as e:
                self._broken = e
                raise
            self._returned[self._outstanding.pop(0)] = frame["value"]
        return self._returned.pop(tag)

    def _end(self, frame):
        if self._ended:
            raise Limit("the pass has ended")
        if self._outstanding:
            ended_by = "refuse" if frame["kind"] == "refuse" else "turn"
            raise Limit("%s sent with %d asks outstanding" % (ended_by, len(self._outstanding)))
        self._send(frame)
        self._ended = True

    def seated(self):
        return self.wait(self.ask("seated"))

    def turn_context(self):
        return self.wait(self.ask("turn_context"))

    def first_turn(self):
        return self.wait(self.ask("first_turn"))

    def fullness(self):
        return self.wait(self.ask("fullness"))

    def flush(self, keep):
        return self.wait(self.ask("flush", keep=keep))

    def recall(self, last_turns):
        return self.wait(self.ask("recall", last_turns=last_turns))

    def shape(self):
        return self.wait(self.ask("shape"))

    def turn(self, delta):
        """End the pass with the turn: the harness runs it and answers no return."""
        self._end({"kind": "ask", "pass": self._pass, "tag": self._next_tag, "ask": "turn", "delta": delta})
        self._next_tag += 1

    def refuse(self):
        """End the pass with no turn: the harness answers ChannelLost."""
        self._end({"kind": "refuse", "pass": self._pass})


def clamp(product):
    """A product saturating at 2^64 - 1, as Rust's saturating_mul does."""
    return min(product, INT_MAX)

