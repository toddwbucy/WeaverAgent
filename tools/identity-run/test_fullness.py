#!/usr/bin/env python3
"""`fullness.py`, the reading after a turn: it waits for the turn's close, reads only
ended lines, and takes the current run's turn alone.

Run: `python3 tools/identity-run/test_fullness.py`. Standard library alone.
"""

import json
import os
import subprocess
import sys
import tempfile
import threading
import time
import unittest

sys.dont_write_bytecode = True
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import fullness  # noqa: E402

SCRIPT = os.path.join(HERE, "fullness.py")
CAPACITY = 100


def event(kind, run, turn=None, **payload):
    line = {"session": "s", "run": run, "kind": kind, "payload": payload or None}
    if turn:
        line["turn"] = turn
    return json.dumps(line, separators=(",", ":"))


def a_turn(run, turn, resident):
    return [event("turn.started", run, turn), event("model.output", run, turn, resident=resident, capacity=CAPACITY),
            event("turn.closed", run, turn)]


class TheReading(unittest.TestCase):
    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory(prefix="weaver-fullness-")
        self.addCleanup(self.scratch.cleanup)
        self.path = os.path.join(self.scratch.name, "trace.ndjson")

    def write(self, lines, tail=""):
        with open(self.path, "w", encoding="utf-8") as f:
            f.write("".join(l + "\n" for l in lines) + tail)

    def later(self, seconds, text):
        def append():
            time.sleep(seconds)
            with open(self.path, "a", encoding="utf-8") as f:
                f.write(text)
        thread = threading.Thread(target=append)
        thread.start()
        self.addCleanup(thread.join)

    def read(self, turn, bound="5"):
        ran = subprocess.run([sys.executable, "-I", SCRIPT, self.path, turn, bound], capture_output=True, text=True)
        return ran.returncode, ran.stdout

    def test_full_at_four_fifths_and_not_below(self):
        for resident, code in ((80, fullness.FULL), (100, fullness.FULL), (79, fullness.NOT_FULL)):
            with self.subTest(resident=resident):
                self.write([event("load", "r1")] + a_turn("r1", "t-1", resident))
                self.assertEqual(self.read("t-1")[0], code)

    def test_a_record_one_turn_behind_is_waited_for_and_the_answered_turn_is_read(self):
        # t-1 stands full in the record; t-2, just answered, is not there yet and is not full.
        self.write([event("load", "r1")] + a_turn("r1", "t-1", 90) + a_turn("r1", "t-2", 10)[:2])
        self.later(0.4, a_turn("r1", "t-2", 10)[2] + "\n")
        code, said = self.read("t-2")
        self.assertEqual(code, fullness.NOT_FULL, said)
        self.assertIn("fullness 10 of 100 after t-2", said)

    def test_a_last_line_cut_short_is_not_taken_and_the_reading_waits_for_its_end(self):
        close = a_turn("r1", "t-2", 90)[2]
        self.write([event("load", "r1")] + a_turn("r1", "t-1", 10) + a_turn("r1", "t-2", 90)[:2], tail=close[:20])
        self.later(0.4, close[20:] + "\n")
        code, said = self.read("t-2")
        self.assertEqual(code, fullness.FULL, said)

    def test_a_close_that_never_comes_spends_the_bound_and_says_so(self):
        self.write([event("load", "r1")] + a_turn("r1", "t-1", 90)[:2], tail='{"kind":"turn.clo')
        started = time.monotonic()
        code, said = self.read("t-1", bound="0.5")
        self.assertEqual(code, fullness.BOUND_SPENT, said)
        self.assertIn("did not hold the close of t-1", said)
        self.assertGreaterEqual(time.monotonic() - started, 0.5)

    def test_an_earlier_runs_turn_of_the_same_key_is_not_taken(self):
        self.write([event("load", "r1")] + a_turn("r1", "t-1", 90) + [event("unload", "r1"), event("load", "r2")]
                   + a_turn("r2", "t-1", 10)[:2])
        code, said = self.read("t-1", bound="0.3")
        self.assertEqual(code, fullness.BOUND_SPENT, said)

    def test_an_unreadable_record_is_named(self):
        rows = {
            "an ended line that is not JSON": ([event("load", "r1"), "{not json"], ""),
            "no load": (a_turn("r1", "t-1", 90), ""),
            "a closed turn with no output": ([event("load", "r1"), event("turn.started", "r1", "t-1"),
                                              event("turn.closed", "r1", "t-1")], ""),
        }
        for row, (lines, tail) in rows.items():
            with self.subTest(row=row):
                self.write(lines, tail)
                code, said = self.read("t-1", bound="0.3")
                self.assertEqual(code, fullness.UNREADABLE, said)
                self.assertIn("unreadable", said)


if __name__ == "__main__":
    unittest.main(verbosity=1)
