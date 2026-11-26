#!/usr/bin/env python3
"""The identity run's comparison, per `weaver-harness-loop-contract` section 6 and act 2's
brief (`docs/project/brief-2026-11-26-the-loop-process.md`) section 8.

Standard library alone, outside the agent's runtime. It reads the arms `run-arm.sh`
copied out and answers one JSON object on standard output:

    compare.py run COMPILED_1 COMPILED_2 WIRES NEGATIVE
    compare.py pair ARM_A ARM_B

Each arm is a folder holding `trace.ndjson` (the record), `worker.log`, `save-points/`
(every save point the record names, under any name: each is found by its digest, the
sha256 of the file, which the territory's published name carries beside the clock) and
`arm.json`, which `run-arm.sh`
writes: `{"arm": "compiled" | "wires" | "negative", "agent": <name>, "loads": <n>,
"turns_to_loop": <n>, "wires_sha256": <hex, wires and negative arms alone>}`.

**The comparison** is over each event's canonical line parsed as JSON, event by event,
after exactly the contract's three steps and no others:

1. every envelope's `wall_ms` and `monotonic_ns` set aside
2. each load's `composer` checked against what its arm must hold, then set aside
3. every distinct value of the minted identifiers (`RENAMED`) replaced by `id-<n>` in the
   order it first appears, one map for the arm.

**`run` answers, by its exit status:**

- 0 `identical`: the control arms are equal, the wires arm equals them, its six loads
  name the laid digest, its runner served the script's passes, the saved images agree
  where the control's do, and the negative control turned red
- 1 `fails`: any of those does not hold. The answer names the first that does not
- 2 `control-differs`: the two compiled arms differ, so the wires are not compared
- 3 `unclean`: an arm holds a timing path outside the comparison (a recorder pressure
  fault, a state seam that died, the recorder's queue at its mark). That arm is run again,
  at most three times, then the run fails
- 4 `unreadable`: an arm's files are missing or are not what this program reads.

`pair` compares two arms by the three steps alone, each checked against its own
`arm.json`, and answers 0 equal or 1 with the first difference.
"""

import hashlib
import json
import math
import os
import sys

IDENTICAL, FAILS, CONTROL_DIFFERS, UNCLEAN, UNREADABLE = 0, 1, 2, 3, 4
USAGE = """usage:
    compare.py run COMPILED_1 COMPILED_2 WIRES NEGATIVE
    compare.py pair ARM_A ARM_B
"""
VERDICTS = {IDENTICAL: "identical", FAILS: "fails", CONTROL_DIFFERS: "control-differs",
            UNCLEAN: "unclean", UNREADABLE: "unreadable"}

# Step 1: the clocks, which differ between any two runs, and the two elapsed times the
# SPU renders into every `model.measurement` (`weaver-spu` `main.rs`, taken in
# `decoder/session.rs`), elapsed times as the clocks are. These and no others: a timing
# added later fails the comparison until the contract names it.
CLOCKS = ("wall_ms", "monotonic_ns")
TIMINGS = {"model.measurement": (("payload", "timings", "prefill_ns"),
                                 ("payload", "timings", "decode_ns"))}

# Step 3: the minted identifiers, by kind, as paths from the event's top. `*` is any
# element of a list. The envelope's are every event's. These and no others
# (contract section 6, `weaver-trace` `event.rs` `SavePointTaken`, `RecalledIdentity`,
# `Lineage`, `Branch`, `Reset`).
ENVELOPE_RENAMED = frozenset({("session",), ("run",)})
RENAMED = {
    "save_point": frozenset({("payload", "save_point"), ("payload", "run"), ("payload", "name")}),
    "recall": frozenset({("payload", "returned", "*", "run")}),
    "load": frozenset({
        ("payload", "lineage", "save_point"),
        ("payload", "lineage", "run"),
        ("payload", "lineage", "built_from", "parent"),
        ("payload", "lineage", "built_from", "run"),
        ("payload", "reset", "prior_run"),
    }),
}

# Step 2: what each arm's loads must name. The worker names itself `worker`.
COMPILED_COMPOSER = {"binary": "worker"}
WIRE_FILE = "/etc/weaver/{agent}/loop/wires.py"

PRESSURE_FAULT = "recorder_commit_pressure"
SHOWN = 240  # the most characters of a value an answer prints


class Unreadable(Exception):
    """An arm's files are missing, or are not what this program reads."""


# ------------------------------------------------------------------ reading an arm

def _strict_object(pairs):
    seen = {}
    for key, value in pairs:
        if key in seen:
            raise ValueError("the member %r is named twice" % key)
        seen[key] = value
    return seen


def _no_constant(name):
    raise ValueError("%s is not a JSON value" % name)


def parse_line(text):
    """One canonical line, parsed as strictly as the writer writes it: no member named
    twice, no NaN or Infinity, one object."""
    value = json.loads(text, object_pairs_hook=_strict_object, parse_constant=_no_constant)
    if not isinstance(value, dict) or not isinstance(value.get("kind"), str):
        raise ValueError("not an event: an object with a string kind")
    return value


def read_record(path):
    events = []
    try:
        with open(path, encoding="utf-8") as record:
            for number, line in enumerate(record, 1):
                if not line.strip():
                    continue
                try:
                    events.append(parse_line(line))
                except ValueError as why:
                    raise Unreadable("%s:%d: %s" % (path, number, why))
    except OSError as why:
        raise Unreadable("%s: %s" % (path, why.strerror or why))
    if not events:
        raise Unreadable("%s: no event" % path)
    return events


def read_worker_log(path):
    """The worker log's lines that parse as JSON objects. The rest are let go."""
    lines = []
    try:
        with open(path, encoding="utf-8", errors="replace") as log:
            for line in log:
                try:
                    value = json.loads(line)
                except ValueError:
                    continue
                if isinstance(value, dict):
                    lines.append(value)
    except OSError as why:
        raise Unreadable("%s: %s" % (path, why.strerror or why))
    return lines


def read_arm(folder):
    try:
        with open(os.path.join(folder, "arm.json"), encoding="utf-8") as f:
            meta = parse_line_meta(f.read())
    except OSError as why:
        raise Unreadable("%s/arm.json: %s" % (folder, why.strerror or why))
    except ValueError as why:
        raise Unreadable("%s/arm.json: %s" % (folder, why))
    return {
        "folder": folder,
        "meta": meta,
        "events": read_record(os.path.join(folder, "trace.ndjson")),
        "log": read_worker_log(os.path.join(folder, "worker.log")),
    }


def parse_line_meta(text):
    meta = json.loads(text, object_pairs_hook=_strict_object, parse_constant=_no_constant)
    if not isinstance(meta, dict):
        raise ValueError("not an object")
    if meta.get("arm") not in ("compiled", "wires", "negative"):
        raise ValueError("arm is compiled, wires or negative: %r" % meta.get("arm"))
    for key in ("loads", "turns_to_loop"):
        if not isinstance(meta.get(key), int) or isinstance(meta.get(key), bool) or meta[key] < 0:
            raise ValueError("%s is a count: %r" % (key, meta.get(key)))
    if not isinstance(meta.get("agent"), str) or not meta["agent"]:
        raise ValueError("agent is the agent's name")
    digest = meta.get("wires_sha256")
    if meta["arm"] == "compiled":
        if digest is not None:
            raise ValueError("a compiled arm lays no wire file")
    elif not (isinstance(digest, str) and len(digest) == 64
              and all(c in "0123456789abcdef" for c in digest)):
        raise ValueError("wires_sha256 is the laid file's sha256, 64 lowercase hex digits")
    return meta


# ------------------------------------------------------------------ the three steps

def expected_composer(meta):
    if meta["arm"] == "compiled":
        return dict(COMPILED_COMPOSER)
    return {"binary": "worker", "file": WIRE_FILE.format(agent=meta["agent"]),
            "sha256": meta["wires_sha256"]}


def normalize(events, composer):
    """The three steps, in order. Answers the renamed events, or raises `Mismatch` naming
    the first load whose composer is not `composer`."""
    names = {}

    def name(value):
        if value not in names:
            names[value] = "id-%d" % (len(names) + 1)
        return names[value]

    out = []
    for index, event in enumerate(events):
        event = {k: v for k, v in event.items() if k not in CLOCKS}
        for timing in TIMINGS.get(event["kind"], ()):
            event = _set_aside(event, timing)
        if event["kind"] == "load":
            payload = event.get("payload")
            if not isinstance(payload, dict) or not same(payload.get("composer"), composer):
                held = payload.get("composer") if isinstance(payload, dict) else None
                raise Mismatch("event %d, a load, names the composer %s where its arm must name %s"
                               % (index, shown(held), shown(composer)))
            event["payload"] = {k: v for k, v in payload.items() if k != "composer"}
        paths = ENVELOPE_RENAMED | RENAMED.get(event["kind"], frozenset())
        out.append(_rename(event, (), paths, name))
    return out


def _set_aside(node, path):
    """`node` less the member at `path`, where it stands; `node` itself is not changed."""
    if not isinstance(node, dict) or path[0] not in node:
        return node
    if len(path) == 1:
        return {k: v for k, v in node.items() if k != path[0]}
    return {k: (_set_aside(v, path[1:]) if k == path[0] else v) for k, v in node.items()}


def _rename(node, path, paths, name):
    if isinstance(node, dict):
        return {k: _rename(v, path + (k,), paths, name) for k, v in node.items()}
    if isinstance(node, list):
        return [_rename(v, path + ("*",), paths, name) for v in node]
    if path in paths:
        if not isinstance(node, str):
            raise Mismatch("the identifier at %s is not a string: %s" % (".".join(path), shown(node)))
        return name(node)
    return node


class Mismatch(Exception):
    """A check of the comparison does not hold."""


def same(x, y):
    """Two parsed JSON values are the same value of the same JSON type: a boolean never
    equals a number, an integer never equals a float, and 0.0 is not -0.0. Members in
    another order are the same object (the contract's parsed values)."""
    if isinstance(x, dict) or isinstance(y, dict):
        return (isinstance(x, dict) and isinstance(y, dict) and x.keys() == y.keys()
                and all(same(x[k], y[k]) for k in x))
    if isinstance(x, list) or isinstance(y, list):
        return (isinstance(x, list) and isinstance(y, list) and len(x) == len(y)
                and all(same(u, v) for u, v in zip(x, y)))
    if type(x) is not type(y):
        return False
    if isinstance(x, float):
        return x == y and math.copysign(1.0, x) == math.copysign(1.0, y)
    return x == y


def first_difference(a, b):
    """The first event at which two normalized arms differ, and where in it, or None."""
    for index, (x, y) in enumerate(zip(a, b)):
        if not same(x, y):
            path, xv, yv = _where(x, y, ())
            return {"event": index, "kind": [x.get("kind"), y.get("kind")],
                    "at": ".".join(map(str, path)) or "(the event)",
                    "values": [shown(xv), shown(yv)]}
    if len(a) != len(b):
        return {"event": min(len(a), len(b)), "kind": None, "at": "(the record's length)",
                "values": ["%d events" % len(a), "%d events" % len(b)]}
    return None


def _where(x, y, path):
    if isinstance(x, dict) and isinstance(y, dict):
        for key in list(x) + [k for k in y if k not in x]:
            if key not in x or key not in y:
                return path + (key,), x.get(key, "(absent)"), y.get(key, "(absent)")
            if not same(x[key], y[key]):
                return _where(x[key], y[key], path + (key,))
    if isinstance(x, list) and isinstance(y, list):
        for i, (u, v) in enumerate(zip(x, y)):
            if not same(u, v):
                return _where(u, v, path + (i,))
        return path, "%d elements" % len(x), "%d elements" % len(y)
    return path, x, y


def shown(value):
    text = json.dumps(value, ensure_ascii=True, separators=(",", ":"))
    return text if len(text) <= SHOWN else text[:SHOWN] + "..."


# ------------------------------------------------------------------ what the record also holds

def unclean(arm):
    """The timing paths outside the comparison an arm holds (contract section 6)."""
    found = []
    for index, event in enumerate(arm["events"]):
        payload = event.get("payload") if isinstance(event.get("payload"), dict) else {}
        if event["kind"] == "fault" and payload.get("case") == PRESSURE_FAULT:
            found.append("event %d: the recorder's commit pressure fault" % index)
        release = payload.get("release") if isinstance(payload.get("release"), dict) else {}
        if event["kind"] == "unload" and release.get("member") == "unconfirmed":
            found.append("event %d: an unload whose state seam had died, a timed-out answer among the causes" % index)
    for line in arm["log"]:
        if line.get("queue") == "trace" and isinstance(line.get("peak"), int) \
                and isinstance(line.get("mark"), int) and line["peak"] >= line["mark"]:
            found.append("worker.log: the recorder's queue reached its mark (%d of %d)" % (line["peak"], line["mark"]))
    return found


def loads(arm):
    return [e for e in arm["events"] if e["kind"] == "load"]


def runner_ends(arm):
    """The runner's end-of-file lines in the worker's log, one per load a runner served."""
    return [line for line in arm["log"] if line.get("loop") == "end"]


def images(arm):
    """Each save point's image, after its stamp line and its check line, in record order,
    each file found in `save-points/` by its sha256, the event's `save_point`."""
    folder = os.path.join(arm["folder"], "save-points")
    held = {}
    try:
        for entry in sorted(os.listdir(folder)):
            path = os.path.join(folder, entry)
            if os.path.isfile(path) and not os.path.islink(path):
                with open(path, "rb") as f:
                    data = f.read()
                held[hashlib.sha256(data).hexdigest()] = (path, data)
    except OSError as why:
        raise Unreadable("%s: %s" % (folder, why.strerror or why))
    out = []
    for event in arm["events"]:
        if event["kind"] != "save_point":
            continue
        digest = (event.get("payload") or {}).get("save_point")
        if digest not in held:
            raise Unreadable("%s: no file holds the save point %s" % (folder, shown(digest)))
        path, data = held[digest]
        stamp_end = data.find(b"\n")
        check_end = data.find(b"\n", stamp_end + 1) if stamp_end >= 0 else -1
        if check_end < 0 or not data[stamp_end + 1:check_end].startswith(b'{"check":'):
            raise Unreadable("%s: not a stamp line, a check line and an image" % path)
        out.append(data[check_end + 1:])
    return out


# ------------------------------------------------------------------ the run

def answer(code, **members):
    return code, dict({"verdict": VERDICTS[code]}, **members)


def run(folders):
    try:
        arms = [read_arm(f) for f in folders]
    except Unreadable as why:
        return answer(UNREADABLE, reason=str(why))
    roles = [a["meta"]["arm"] for a in arms]
    if roles != ["compiled", "compiled", "wires", "negative"]:
        return answer(UNREADABLE, reason="the arms in order are compiled, compiled, wires and negative: %s" % roles)
    if len({a["meta"]["agent"] for a in arms}) != 1:
        return answer(FAILS, reason="the four arms do not run under one agent name")
    if len({a["meta"]["loads"] for a in arms}) != 1:
        return answer(FAILS, reason="the four arms do not declare one script of loads")
    sent = [a["meta"]["turns_to_loop"] for a in arms[:3]]
    if len(set(sent)) != 1:
        return answer(FAILS, reason="the compiled arms and the wires arm sent %s turns to the loop, so they did "
                      "not run one script: the pressure reading counted the turns before pressure "
                      "differently in each" % sent)
    dirty = {a["folder"]: unclean(a) for a in arms if unclean(a)}
    if dirty:
        return answer(UNCLEAN, arms=dirty,
                      reason="an arm holds a timing path outside the comparison: run it again, at most three times")
    for arm in arms:
        if len(loads(arm)) != arm["meta"]["loads"]:
            return answer(FAILS, reason="%s holds %d loads where the script makes %d"
                          % (arm["folder"], len(loads(arm)), arm["meta"]["loads"]))
    try:
        normal = [normalize(a["events"], expected_composer(a["meta"])) for a in arms]
    except Mismatch as why:
        return answer(FAILS, reason=str(why))
    c1, c2, wires, negative = normal
    result = {}

    difference = first_difference(c1, c2)
    if difference:
        return answer(CONTROL_DIFFERS, difference=difference,
                      reason="the two compiled arms differ, so the wires are not compared")
    difference = first_difference(c1, wires)
    if difference:
        return answer(FAILS, difference=difference, reason="the wires arm differs from the compiled arms")

    for arm in arms[:2]:
        if runner_ends(arm):
            return answer(FAILS, reason="%s, a compiled arm, holds a runner's end line" % arm["folder"])
    ends = runner_ends(arms[2])
    passes = sum(line.get("passes") for line in ends if isinstance(line.get("passes"), int))
    result["passes"] = {"served": passes, "script": arms[2]["meta"]["turns_to_loop"], "end_lines": len(ends)}
    if len(ends) != arms[2]["meta"]["loads"] or passes != arms[2]["meta"]["turns_to_loop"]:
        return answer(FAILS, reason="the wires arm's runner did not serve the script's passes, one end line a load", **result)

    try:
        saved = [images(a) for a in arms[:3]]
    except Unreadable as why:
        return answer(UNREADABLE, reason=str(why), **result)
    if saved[0] == saved[1]:
        if saved[2] != saved[0]:
            at = next((i for i, (x, y) in enumerate(zip(saved[0], saved[2])) if x != y), min(len(saved[0]), len(saved[2])))
            return answer(FAILS, reason="the control arms' saved images are equal and the wires arm's differ, at save point %d" % at, **result)
        result["images"] = "compared: equal in all three arms (%d save points)" % len(saved[0])
    else:
        at = next((i for i, (x, y) in enumerate(zip(saved[0], saved[1])) if x != y), min(len(saved[0]), len(saved[1])))
        result["images"] = "not compared: the control arms' images differ at save point %d" % at

    if arms[3]["meta"]["wires_sha256"] == arms[2]["meta"]["wires_sha256"]:
        return answer(FAILS, reason="the negative control's wire file is the wires arm's: no byte was changed", **result)
    difference = first_difference(c1, negative)
    if difference is None:
        return answer(FAILS, reason="the negative control stayed green: a changed wire file compares equal", **result)
    result["negative"] = difference
    return answer(IDENTICAL, **result)


def pair(folders):
    try:
        arms = [read_arm(f) for f in folders]
        normal = [normalize(a["events"], expected_composer(a["meta"])) for a in arms]
    except Unreadable as why:
        return answer(UNREADABLE, reason=str(why))
    except Mismatch as why:
        return answer(FAILS, reason=str(why))
    difference = first_difference(*normal)
    if difference:
        return answer(FAILS, difference=difference)
    return answer(IDENTICAL)


def main(argv):
    if len(argv) == 5 and argv[0] == "run":
        code, body = run(argv[1:])
    elif len(argv) == 3 and argv[0] == "pair":
        code, body = pair(argv[1:])
    else:
        sys.stderr.write(USAGE)
        return UNREADABLE
    sys.stdout.write(json.dumps(body, indent=2, ensure_ascii=True) + "\n")
    return code


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
