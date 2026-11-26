#!/usr/bin/env python3
"""`run-arm.sh`'s plan, read against act 2's brief section 8. The plan acts on nothing and
needs no root, so it is what can be measured here: the steps in the script's order, the
loop laid by the wire arms alone, and the refusals before any step. The acts themselves
are the appliance's (`PROCEDURE.md`).

Run: `python3 tools/identity-run/test_run_arm.py`. Standard library alone.
"""

import os
import re
import subprocess
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
SCRIPT = os.path.join(HERE, "run-arm.sh")


def plan(*args):
    return subprocess.run(["bash", SCRIPT, *args], capture_output=True, text=True)


def steps(out):
    return re.findall(r"^== step \d+: (.*)$", out, re.M)


THE_SCRIPT = [
    "load 1: continuous, nothing seated",
    "load 1: turns until fullness passes four-fifths of capacity, then one under pressure (at most 40)",
    "unload 1",
    "load 2: a first turn with an earlier run",
    "unload 2",
    "the declaration set to independent",
    "load 3: independent, nothing seated",
    "unload 3",
    "the declaration set back to continuous",
    "load 4: the seeding turn",
    "unload 4",
    "load 5: a first turn with the prompt seated",
    "unload 5",
    "the declaration set to independent",
    "load 6: independent, the prompt seated",
    "unload 6",
]


class ThePlan(unittest.TestCase):
    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory(prefix="weaver-identity-plan-")
        self.addCleanup(self.scratch.cleanup)
        self.wires = os.path.join(self.scratch.name, "wires.py")
        with open(self.wires, "w") as f:
            f.write("# a wire file\n")
        self.common = ["--agent", "a1", "--artifact", "/models/model.gguf",
                       "--out", os.path.join(self.scratch.name, "out")]

    def test_the_bash_parses(self):
        self.assertEqual(subprocess.run(["bash", "-n", SCRIPT]).returncode, 0)

    def test_a_compiled_arm_runs_the_six_loads_in_the_briefs_order_and_lays_no_loop(self):
        ran = plan("--arm", "compiled", *self.common)
        self.assertEqual(ran.returncode, 0, ran.stdout + ran.stderr)
        self.assertEqual(steps(ran.stdout), ["create the agent",
                                             "admit the operator's uid at the gate, for the seeding turn"]
                         + THE_SCRIPT + ["the record, worker.log and the save points copied to %s, with arm.json"
                                         % self.common[-1], "take every agent off the appliance"])
        self.assertIn("plan only", ran.stdout)
        self.assertNotIn("/loop", ran.stdout)

    def test_a_wires_arm_lays_the_loop_and_has_it_judged_before_the_first_load(self):
        ran = plan("--arm", "wires", "--wires", self.wires, *self.common)
        self.assertEqual(ran.returncode, 0, ran.stdout + ran.stderr)
        got = steps(ran.stdout)
        self.assertEqual(got[2:4], ["lay the loop by hand", "admin judges the loop"])
        self.assertEqual(got[4:4 + len(THE_SCRIPT)], THE_SCRIPT)
        self.assertIn("install -d -o root -g weaver-a1 -m 0750 /etc/weaver/a1/loop", ran.stdout)
        self.assertIn("-m 0640 %s /etc/weaver/a1/loop/wires.py" % self.wires, ran.stdout)
        import hashlib
        self.assertIn(hashlib.sha256(b"# a wire file\n").hexdigest(), ran.stdout.splitlines()[0])

    def test_every_turn_is_the_gate_connectors_and_the_seeding_turn_the_operators(self):
        ran = plan("--arm", "compiled", *self.common)
        turns = re.findall(r"^\+ timeout 900 runuser -u weaver-gatecon -- python3 -I - a1 .* --raw$", ran.stdout, re.M)
        self.assertEqual(len(turns), 6, "the turns the plan can count: two, then one a load but the seeding one")
        self.assertEqual(len(re.findall(r"turn\.py a1 --system --raw$", ran.stdout, re.M)), 1)
        self.assertEqual(re.findall(r"must answer kind answered, turn (t-\d+)", ran.stdout),
                         ["t-1", "t-2", "t-1", "t-1", "t-1", "t-1", "t-1"])
        self.assertEqual(re.findall(r"declaration-edit \S+ (\S+)", ran.stdout),
                         ["operator-admitted", "turn-context=independent", "turn-context=continuous",
                          "turn-context=independent"])

    def test_the_create_step_names_the_shared_declarations(self):
        ran = plan("--arm", "compiled", *self.common, "--context-capacity", "4096")
        self.assertRegex(ran.stdout, r"create-agent\.sh a1 --engine sqlite --artifact /models/model\.gguf "
                                     r"--context-capacity 4096 --turn-context continuous --root-operator --apply")

    def test_a_wrong_arm_refuses_before_any_step(self):
        rows = {
            "an arm unnamed": ["--arm", "other", *self.common],
            "a compiled arm with a wire file": ["--arm", "compiled", "--wires", self.wires, *self.common],
            "a wires arm with no file": ["--arm", "wires", "--wires", self.wires + ".absent", *self.common],
            "an unknown argument": ["--arm", "compiled", "--loud", *self.common],
            "a name admin would refuse": ["--arm", "compiled", "--agent", "A/1", *self.common[2:]],
        }
        for row, args in rows.items():
            with self.subTest(row=row):
                ran = plan(*args)
                self.assertEqual(ran.returncode, 2, ran.stdout + ran.stderr)
                self.assertNotIn("== step", ran.stdout)

    def test_apply_is_roots_and_refused_to_anyone_else_before_it_touches_anything(self):
        if os.geteuid() == 0:
            self.skipTest("run as root, where --apply would act")
        ran = plan("--arm", "compiled", *self.common, "--apply")
        self.assertEqual(ran.returncode, 2)
        self.assertIn("--apply is root's", ran.stderr)
        self.assertFalse(os.path.exists(self.common[-1]))


if __name__ == "__main__":
    unittest.main(verbosity=1)
