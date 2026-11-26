#!/usr/bin/env python3
"""The fullness reading `run-arm.sh` takes after a turn, to know when the next turn runs
under pressure. Standard library alone.

    fullness.py TRACE TURN BOUND_SECONDS

The record is read as the writer leaves it, which is behind the turn's answer: nothing on
a turn's path waits on the sink (`weaver-harness-Spec`, the recorder). So the reading
waits, up to BOUND_SECONDS, until the current run holds TURN's `turn.closed`, the last
`load` naming the current run. It reads only lines the writer has ended, so a last line
still being written is never taken. Then it reads TURN's last `model.output`.

Exit status: 0 full enough (resident at or past four-fifths of capacity), 1 not yet, 3
the bound ran out before the record held TURN's close, 4 the record is unreadable (an
ended line that is not one JSON object, no load, or a closed turn with no output). It
prints one line saying which.
"""

import json
import sys
import time

FULL, NOT_FULL, BOUND_SPENT, UNREADABLE = 0, 1, 3, 4
POLL = 0.1


class Unreadable(Exception):
    pass


def ended_events(path):
    """Every event on a line the writer has ended, in order. A last line with no newline
    is still being written and is left for the next read."""
    with open(path, "rb") as f:
        data = f.read()
    lines = data.split(b"\n")[:-1]
    events = []
    for number, line in enumerate(lines, 1):
        try:
            event = json.loads(line)
        except ValueError:
            raise Unreadable("line %d is ended and is not JSON" % number)
        if not isinstance(event, dict):
            raise Unreadable("line %d is not one object" % number)
        events.append(event)
    return events


def reading(events, turn):
    """(resident, capacity) of `turn` in the current run, or None while its close is not
    in the record."""
    loads = [i for i, e in enumerate(events) if e.get("kind") == "load"]
    if not loads:
        raise Unreadable("the record holds no load")
    run = events[loads[-1]].get("run")
    current = [e for e in events[loads[-1]:] if e.get("run") == run and e.get("turn") == turn]
    if not any(e.get("kind") == "turn.closed" for e in current):
        return None
    outputs = [e.get("payload") for e in current if e.get("kind") == "model.output"]
    if not outputs or not isinstance(outputs[-1], dict):
        raise Unreadable("turn %s is closed and holds no model.output" % turn)
    return outputs[-1].get("resident", 0), outputs[-1].get("capacity", 0)


def main(argv):
    if len(argv) != 4:
        print("usage: fullness.py TRACE TURN BOUND_SECONDS")
        return UNREADABLE
    path, turn = argv[1], argv[2]
    try:
        bound = float(argv[3])
    except ValueError:
        print("the bound is a number of seconds: %r" % argv[3])
        return UNREADABLE
    deadline = time.monotonic() + bound
    while True:
        try:
            got = reading(ended_events(path), turn)
        except (OSError, Unreadable) as why:
            print("   the record is unreadable: %s" % why)
            return UNREADABLE
        if got is not None:
            resident, capacity = got
            print("   fullness %s of %s after %s" % (resident, capacity, turn))
            ok = isinstance(resident, int) and isinstance(capacity, int) and capacity > 0
            return FULL if ok and resident * 5 >= capacity * 4 else NOT_FULL
        if time.monotonic() >= deadline:
            print("   the record did not hold the close of %s within %s seconds" % (turn, argv[3]))
            return BOUND_SPENT
        time.sleep(POLL)


if __name__ == "__main__":
    sys.exit(main(sys.argv))
