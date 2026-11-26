#!/usr/bin/env python3
"""The comparison's own tests, and the first of the identity run's two negative controls,
over recorded fixtures (act 2's brief section 8).

`fixtures/two-loads.ndjson` is cut from a real record of 2026-11-18, two loads of the
compiled loop under `independent`: each run's load, its first turn, its save point and
its unload, every member and its order as the writer wrote them, long strings
shortened, the agent's name and the model's path replaced. The shapes that record lacks
are added here from `weaver-trace` `event.rs`: a recall's `returned`, and a load's
`reset` and `lineage.built_from`. Each arm is minted afresh, so the renaming is what
makes two arms equal, and its clocks moved, so setting them aside is.

Run: `python3 tools/identity-run/test_compare.py`. Standard library alone.
"""

import copy
import hashlib
import json
import os
import subprocess
import sys
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
sys.dont_write_bytecode = True  # the import leaves no bytecode beside the program
import compare  # noqa: E402

FIXTURE = os.path.join(HERE, "fixtures", "two-loads.ndjson")
WIRES_SHA = "1" * 64
NEGATIVE_SHA = "2" * 64


def fixture():
    """The real record, then a third load restored from an offline build over run 1
    after run 2 did not end clean, and a recall in it."""
    events = [json.loads(line) for line in open(FIXTURE, encoding="utf-8")]
    run1, run2 = events[0]["run"], events[12]["run"]
    session = events[0]["session"]
    load3 = copy.deepcopy(events[12])
    load3["run"] = "2026-11-18T22:01:00.000Z-a1-0123456789abcdef"
    load3["payload"]["lineage"] = dict(load3["payload"]["lineage"],
                                       built_from={"parent": session, "run": run1, "through": 1})
    load3["payload"]["reset"] = {"prior_run": run2, "reason": "the prior run ended unclean"}
    recall = {"session": session, "run": load3["run"], "turn": "t-1", "sequence": "3",
              "kind": "recall", "subsystem": "harness", "wall_ms": 1795039300000,
              "monotonic_ns": "99", "payload": {
                  "ask": {"verb": "recall", "last_turns": 4},
                  "returned": [{"run": run1, "turn": "t-1", "sequence": "4", "kind": "message.user"},
                               {"run": run2, "turn": "t-1", "sequence": "8", "kind": "message.assistant"}]}}
    return events + [load3, recall]


def remint(events, tag):
    """Every minted identifier given a fresh value for this arm, the same value always to
    the same new one, and every clock and both of a measurement's timings moved."""
    fresh = {}

    def mint(value):
        return fresh.setdefault(value, "%s-%d-%s" % (tag, len(fresh) + 1, hashlib.sha256(value.encode()).hexdigest()[:8]))

    def walk(node, path, paths):
        if isinstance(node, dict):
            return {k: walk(v, path + (k,), paths) for k, v in node.items()}
        if isinstance(node, list):
            return [walk(v, path + ("*",), paths) for v in node]
        return mint(node) if path in paths else node

    out = []
    for i, event in enumerate(events):
        event = walk(event, (), compare.ENVELOPE_RENAMED | compare.RENAMED.get(event["kind"], frozenset()))
        event["wall_ms"] = 1 + i + len(tag) * 1000
        event["monotonic_ns"] = str(7 * i + len(tag))
        timings = event.get("payload", {}).get("timings") if isinstance(event.get("payload"), dict) else None
        if event["kind"] == "model.measurement" and isinstance(timings, dict):
            timings["prefill_ns"] = str(1000 + 3 * i + len(tag))
            timings["decode_ns"] = str(2000 + 5 * i + len(tag))
        out.append(event)
    return out


def set_composer(events, composer):
    for event in events:
        if event["kind"] == "load":
            event["payload"]["composer"] = dict(composer)
    return events


class Arms:
    """The arm folders of one test, under a scratch folder removed at its end."""

    def __init__(self, case):
        self.scratch = tempfile.TemporaryDirectory(prefix="weaver-identity-")
        case.addCleanup(self.scratch.cleanup)

    def write(self, name, role, events, *, sha=None, turns=None, log=None, image=lambda i: b"",
              loads=None, agent="a1"):
        folder = os.path.join(self.scratch.name, name)
        os.makedirs(os.path.join(folder, "save-points"))
        events = copy.deepcopy(events)
        renamed = {}
        index = 0
        for event in events:
            if event["kind"] == "save_point":
                stamp = json.dumps({"weaver-save-point": 1, "arm": name, "index": index}).encode() + b"\n"
                body = b"the image of save point %d" % index + image(index)
                check = b'{"check":"' + hashlib.sha256(stamp + body).hexdigest().encode() + b'"}\n'
                data = stamp + check + body
                digest = hashlib.sha256(data).hexdigest()
                renamed[event["payload"]["save_point"]] = digest
                event["payload"]["save_point"] = digest
                event["payload"]["name"] = digest + ".save-point"
                # The territory's published name: the stamp's clock, then the digest.
                published = "20261118T2046%02dZ-%s.save-point" % (index, digest)
                with open(os.path.join(folder, "save-points", published), "wb") as f:
                    f.write(data)
                index += 1
            if event["kind"] == "load" and event["payload"].get("lineage"):
                old = event["payload"]["lineage"]["save_point"]
                event["payload"]["lineage"]["save_point"] = renamed.get(old, old)
        with open(os.path.join(folder, "trace.ndjson"), "w", encoding="utf-8") as f:
            for event in events:
                f.write(json.dumps(event, separators=(",", ":")) + "\n")
        n_loads = sum(e["kind"] == "load" for e in events)
        if turns is None:
            turns = sum(e["kind"] == "turn.started" for e in events)
        if log is None:
            log = ['{"depth":1024,"mark":768,"organ":"harness","peak":5,"queue":"trace"}']
            if role != "compiled":
                log += ['{"loop":"start","reason":"noise before the end"}', "a plain line"]
                log += ['{"loop":"end","passes":%d,"turns":%d,"refused":0}' % (p, p) for p in self.split(turns, n_loads)]
        with open(os.path.join(folder, "worker.log"), "w", encoding="utf-8") as f:
            f.write("".join(line + "\n" for line in log))
        meta = {"arm": role, "agent": agent, "loads": n_loads if loads is None else loads, "turns_to_loop": turns}
        if role != "compiled":
            meta["wires_sha256"] = sha
        with open(os.path.join(folder, "arm.json"), "w", encoding="utf-8") as f:
            json.dump(meta, f)
        return folder

    @staticmethod
    def split(turns, loads):
        return [turns // loads + (1 if i < turns % loads else 0) for i in range(loads)]


def wires_composer(sha, agent="a1"):
    return {"binary": "worker", "file": "/etc/weaver/%s/loop/wires.py" % agent, "sha256": sha}


def change_system_prompt(events):
    """One byte of the prompt the wire file composes, as a changed wire file would make."""
    for event in events:
        if event["kind"] == "message.system":
            text = event["payload"]["content"][0]["text"]
            event["payload"]["content"][0]["text"] = text[:-1] + ("X" if text[-1] != "X" else "Y")
            return events
    raise AssertionError("no system message")


class TheRun(unittest.TestCase):
    """`compare.py run` over four arms made from the fixture."""

    def arms(self, *, c1=None, c2=None, wires=None, negative=None, write=None):
        write = write or {}
        base = fixture()
        self.made = Arms(self)
        c1 = c1 if c1 is not None else remint(base, "c1")
        c2 = c2 if c2 is not None else remint(base, "c2")
        wires = wires if wires is not None else set_composer(remint(base, "w"), wires_composer(WIRES_SHA))
        negative = negative if negative is not None else set_composer(
            change_system_prompt(remint(base, "n")), wires_composer(NEGATIVE_SHA))
        return [
            self.made.write("c1", "compiled", c1, **write.get("c1", {})),
            self.made.write("c2", "compiled", c2, **write.get("c2", {})),
            self.made.write("w", "wires", wires, sha=WIRES_SHA, **write.get("w", {})),
            self.made.write("n", "negative", negative, sha=NEGATIVE_SHA, **write.get("n", {})),
        ]

    def run_on(self, folders):
        code, body = compare.run(folders)
        self.assertEqual(body["verdict"], compare.VERDICTS[code])
        return code, body

    def test_four_arms_made_alike_are_identical_and_the_negative_turns_red(self):
        # Every arm minted afresh and its clocks moved: the three steps make the three
        # arms equal, the images compare, and the changed prompt is the negative's
        # difference. Perturbation: drop any step, or rename one field fewer, and this
        # answers fails or control-differs.
        code, body = self.run_on(self.arms())
        self.assertEqual(code, compare.IDENTICAL, body)
        self.assertEqual(body["passes"], {"served": 2, "script": 2, "end_lines": 3})
        self.assertEqual(body["images"], {"compared": [0, 1], "not compared": []}, body)
        self.assertEqual(body["negative"]["kind"], ["message.system", "message.system"])
        self.assertEqual(body["negative"]["at"], "payload.content.0.text")
        prompt = next(e for e in fixture() if e["kind"] == "message.system")["payload"]["content"][0]["text"]
        self.assertEqual(body["negative"]["offset"], len(prompt) - 1)
        self.assertEqual(body["negative"]["window"][0][-1], prompt[-1])
        self.assertEqual(body["negative"]["window"][1][-1], "X" if prompt[-1] != "X" else "Y")

    def test_a_string_differing_past_what_values_prints_names_its_offset_and_a_window(self):
        left = "a" * 700 + "careful" + "b" * 300
        right = "a" * 700 + "cAreful" + "b" * 300
        difference = compare.first_difference([{"kind": "x", "v": left}], [{"kind": "x", "v": right}])
        self.assertEqual(difference["values"][0], difference["values"][1], "two equal beginnings")
        self.assertEqual(difference["offset"], 701)
        self.assertEqual(difference["window"], [left[661:741], right[661:741]])
        self.assertIn("cAreful", difference["window"][1])

    def test_the_command_line_answers_its_verdict_as_its_status(self):
        folders = self.arms()
        ran = subprocess.run([sys.executable, "-I", os.path.join(HERE, "compare.py"), "run"] + folders,
                             capture_output=True, text=True)
        self.assertEqual(ran.returncode, 0, ran.stdout + ran.stderr)
        self.assertEqual(json.loads(ran.stdout)["verdict"], "identical")
        ran = subprocess.run([sys.executable, "-I", os.path.join(HERE, "compare.py"), "run"] + folders[:3],
                             capture_output=True, text=True)
        self.assertEqual(ran.returncode, compare.UNREADABLE)
        self.assertIn("compare.py run", ran.stderr)

    # -- the first negative control: the comparison turns red ---------------------------

    def test_a_changed_byte_in_the_wires_arm_fails_the_run_naming_where(self):
        wires = set_composer(remint(fixture(), "w"), wires_composer(WIRES_SHA))
        wires[8]["payload"]["content"][0]["text"] += "!"  # the first answer
        code, body = self.run_on(self.arms(wires=wires))
        self.assertEqual(code, compare.FAILS, body)
        self.assertEqual(body["difference"]["event"], 8)
        self.assertEqual(body["difference"]["at"], "payload.content.0.text")

    def test_a_lineage_naming_another_save_point_fails_though_every_name_is_minted(self):
        # The renaming keeps the map one to one: run 2's lineage naming a save point other
        # than the one the control's names is a difference, though every name is minted.
        wires = set_composer(remint(fixture(), "w"), wires_composer(WIRES_SHA))
        wires[12]["payload"]["lineage"]["save_point"] = wires[22]["payload"]["save_point"]
        code, body = self.run_on(self.arms(wires=wires))
        self.assertEqual(code, compare.FAILS, body)
        self.assertEqual(body["difference"]["at"], "payload.lineage.save_point")

    def test_a_field_outside_the_renaming_is_compared_as_it_stands(self):
        # The declaration's digest is not set aside (contract section 6).
        for member, value in (("declaration", "0" * 64), ("turn_context", "continuous")):
            with self.subTest(member=member):
                wires = set_composer(remint(fixture(), "w"), wires_composer(WIRES_SHA))
                wires[0]["payload"][member] = value
                code, body = self.run_on(self.arms(wires=wires))
                self.assertEqual(code, compare.FAILS, body)
                self.assertEqual(body["difference"]["at"], "payload." + member)

    def test_an_envelope_member_beside_the_clocks_is_compared(self):
        wires = set_composer(remint(fixture(), "w"), wires_composer(WIRES_SHA))
        wires[3]["sequence"] = "30"
        code, body = self.run_on(self.arms(wires=wires))
        self.assertEqual(code, compare.FAILS, body)
        self.assertEqual(body["difference"]["at"], "sequence")

    def test_a_timing_beside_the_two_named_is_compared(self):
        wires = set_composer(remint(fixture(), "w"), wires_composer(WIRES_SHA))
        at = next(i for i, e in enumerate(wires) if e["kind"] == "model.measurement")
        wires[at]["payload"]["timings"]["sample_ns"] = "5"
        code, body = self.run_on(self.arms(wires=wires))
        self.assertEqual(code, compare.FAILS, body)
        self.assertEqual(body["difference"]["at"], "payload.timings.sample_ns")

    def test_a_named_timing_is_set_aside_by_value_and_compared_by_presence(self):
        wires = set_composer(remint(fixture(), "w"), wires_composer(WIRES_SHA))
        at = next(i for i, e in enumerate(wires) if e["kind"] == "model.measurement")
        wires[at]["payload"]["timings"]["decode_ns"] = "999"
        code, body = self.run_on(self.arms(wires=wires))
        self.assertEqual(code, compare.IDENTICAL, body)
        del wires[at]["payload"]["timings"]["decode_ns"]
        code, body = self.run_on(self.arms(wires=wires))
        self.assertEqual(code, compare.FAILS, body)
        self.assertEqual(body["difference"]["at"], "payload.timings.decode_ns")

    def test_an_event_more_or_fewer_fails_at_the_records_length(self):
        wires = set_composer(remint(fixture(), "w"), wires_composer(WIRES_SHA))
        code, body = self.run_on(self.arms(wires=wires[:-1]))
        self.assertEqual(code, compare.FAILS, body)
        self.assertEqual(body["difference"]["at"], "(the record's length)")

    # -- step 2: the composer, checked then set aside ------------------------------------

    def test_each_load_of_each_arm_must_name_its_composer(self):
        rows = {
            "a wires load naming another digest": ("w", 12, wires_composer("3" * 64)),
            "a wires load naming another agent's file": ("w", 0, wires_composer(WIRES_SHA, "a2")),
            "a wires load with a member more": ("w", 24, dict(wires_composer(WIRES_SHA), extra=1)),
            "a wires load naming the compiled loop": ("w", 24, compare.COMPILED_COMPOSER),
            "a compiled load naming a file": ("c2", 12, wires_composer(WIRES_SHA)),
        }
        for row, (arm, index, composer) in rows.items():
            with self.subTest(row=row):
                made = {"c1": remint(fixture(), "c1"), "c2": remint(fixture(), "c2"),
                        "w": set_composer(remint(fixture(), "w"), wires_composer(WIRES_SHA))}
                made[arm][index]["payload"]["composer"] = composer
                code, body = self.run_on(self.arms(c1=made["c1"], c2=made["c2"], wires=made["w"]))
                self.assertEqual(code, compare.FAILS, body)
                self.assertIn("event %d, a load, names the composer" % index, body["reason"])

    # -- the control decides -------------------------------------------------------------

    def test_control_arms_that_differ_answer_so_and_compare_no_wires(self):
        c2 = remint(fixture(), "c2")
        c2[6]["payload"]["resident"] += 1  # the model's output, as a run that did not repeat
        wires = set_composer(remint(fixture(), "w"), wires_composer(WIRES_SHA))
        wires[8]["payload"]["content"][0]["text"] += "!"
        code, body = self.run_on(self.arms(c2=c2, wires=wires))
        self.assertEqual(code, compare.CONTROL_DIFFERS, body)
        self.assertEqual(body["difference"]["at"], "payload.resident")

    def test_images_equal_in_the_control_must_be_equal_in_the_wires(self):
        code, body = self.run_on(self.arms(write={"w": {"image": lambda i: b"!" if i == 1 else b""}}))
        self.assertEqual(code, compare.FAILS, body)
        self.assertIn("save point 1:", body["reason"])

    def test_images_are_read_one_save_point_at_a_time(self):
        # The control differs at save point 0 alone: save point 1 is still compared, and a
        # wires arm that differs there fails; one that differs at 0 alone does not.
        c2 = {"image": lambda i: b"?" if i == 0 else b""}
        code, body = self.run_on(self.arms(write={"c2": c2, "w": {"image": lambda i: b"!" if i == 1 else b""}}))
        self.assertEqual(code, compare.FAILS, body)
        self.assertIn("save point 1:", body["reason"])
        code, body = self.run_on(self.arms(write={"c2": c2, "w": {"image": lambda i: b"!" if i == 0 else b""}}))
        self.assertEqual(code, compare.IDENTICAL, body)
        self.assertEqual(body["images"], {"compared": [1], "not compared": [0]})
        self.assertNotIn("reason", body)

    def test_images_the_control_holds_unequal_are_not_compared_and_the_result_says_so(self):
        code, body = self.run_on(self.arms(write={"c2": {"image": lambda i: b"?"}, "w": {"image": lambda i: b"!"}}))
        self.assertEqual(code, compare.IDENTICAL, body)
        self.assertEqual(body["images"], {"compared": [], "not compared": [0, 1]})
        self.assertIn("no save point's image was compared", body["reason"])

    def test_a_save_point_file_that_is_not_the_events_is_unreadable(self):
        folders = self.arms()
        name = sorted(os.listdir(os.path.join(folders[2], "save-points")))[0]
        with open(os.path.join(folders[2], "save-points", name), "ab") as f:
            f.write(b"x")
        code, body = self.run_on(folders)
        self.assertEqual(code, compare.UNREADABLE, body)
        self.assertIn("no file holds the save point", body["reason"])

    # -- the passes the runner served -----------------------------------------------------

    def test_the_wires_runner_must_serve_the_scripts_passes_one_end_line_a_load(self):
        rows = {
            "a pass short": {"w": {"log": ['{"loop":"end","passes":1,"turns":1,"refused":0}']
                                   + ['{"loop":"end","passes":0,"turns":0,"refused":0}'] * 2}},
            "an end line missing": {"w": {"log": ['{"loop":"end","passes":1,"turns":1,"refused":0}'] * 2}},
            "a compiled arm holding an end line": {"c1": {"log": ['{"loop":"end","passes":3,"turns":3,"refused":0}']}},
        }
        for row, write in rows.items():
            with self.subTest(row=row):
                code, body = self.run_on(self.arms(write=write))
                self.assertEqual(code, compare.FAILS, body)

    # -- the negative control's own checks ------------------------------------------------

    def test_a_negative_control_that_stays_green_fails_the_run(self):
        negative = set_composer(remint(fixture(), "n"), wires_composer(NEGATIVE_SHA))
        code, body = self.run_on(self.arms(negative=negative))
        self.assertEqual(code, compare.FAILS, body)
        self.assertIn("the negative control stayed green", body["reason"])

    def test_a_negative_control_on_the_wires_arms_own_file_fails_the_run(self):
        base = fixture()
        negative = set_composer(change_system_prompt(remint(base, "n")), wires_composer(WIRES_SHA))
        made = Arms(self)
        folders = [made.write("c1", "compiled", remint(base, "c1")),
                   made.write("c2", "compiled", remint(base, "c2")),
                   made.write("w", "wires", set_composer(remint(base, "w"), wires_composer(WIRES_SHA)), sha=WIRES_SHA),
                   made.write("n", "negative", negative, sha=WIRES_SHA)]
        code, body = self.run_on(folders)
        self.assertEqual(code, compare.FAILS, body)
        self.assertIn("no byte was changed", body["reason"])

    # -- the timing paths outside the comparison ------------------------------------------

    def test_an_arm_holding_a_timing_path_is_unclean_and_named(self):
        pressure = {"session": "s", "run": "r", "sequence": "40", "kind": "fault", "subsystem": "harness",
                    "wall_ms": 1, "monotonic_ns": "1", "payload": {"case": "recorder_commit_pressure"}}
        rows = {
            "the pressure fault": ("c2", lambda e: e.insert(10, pressure), {}, "commit pressure fault"),
            "a dead state seam": ("w", lambda e: e[11]["payload"]["release"].update(member="unconfirmed"), {}, "state seam had died"),
            "the queue at its mark": ("c1", lambda e: None,
                                      {"log": ['{"depth":1024,"mark":768,"organ":"harness","peak":768,"queue":"trace"}']},
                                      "queue reached its mark (768 of 768)"),
        }
        for row, (arm, spoil, write, said) in rows.items():
            with self.subTest(row=row):
                made = {"c1": remint(fixture(), "c1"), "c2": remint(fixture(), "c2"),
                        "w": set_composer(remint(fixture(), "w"), wires_composer(WIRES_SHA))}
                spoil(made[arm])
                code, body = self.run_on(self.arms(c1=made["c1"], c2=made["c2"], wires=made["w"], write={arm: write}))
                self.assertEqual(code, compare.UNCLEAN, body)
                self.assertEqual(len(body["arms"]), 1, body)
                self.assertIn(said, json.dumps(body["arms"]))

    # -- what the run refuses to read ------------------------------------------------------

    def test_unreadable_arms_are_named(self):
        rows = {
            "no record": lambda f: os.remove(os.path.join(f[2], "trace.ndjson")),
            "a member named twice": lambda f: self.append(f[0], '{"kind":"flush","kind":"flush"}'),
            "NaN": lambda f: self.append(f[1], '{"kind":"flush","payload":{"x":NaN}}'),
            "not an event": lambda f: self.append(f[2], "[1]"),
            "a stray arm.json": lambda f: self.rewrite(f[3], "arm.json", '{"arm":"other"}'),
        }
        for row, spoil in rows.items():
            with self.subTest(row=row):
                folders = self.arms()
                spoil(folders)
                code, body = self.run_on(folders)
                self.assertEqual(code, compare.UNREADABLE, body)

    def test_the_arms_come_in_their_order_under_one_name_and_one_script(self):
        folders = self.arms()
        code, body = self.run_on([folders[2], folders[0], folders[1], folders[3]])
        self.assertEqual(code, compare.UNREADABLE, body)
        code, body = self.run_on(self.arms(write={"n": {"agent": "a2"}}))
        self.assertEqual(code, compare.FAILS, body)
        self.assertIn("one agent name", body["reason"])
        code, body = self.run_on(self.arms(write={arm: {"loads": 6} for arm in ("c1", "c2", "w", "n")}))
        self.assertEqual(code, compare.FAILS, body)
        self.assertIn("holds 3 loads where the script makes 6", body["reason"])

    def test_a_run_over_nothing_fails(self):
        for row, keep in (("no load and no save point", lambda e: e["kind"] not in ("load", "save_point")),
                          ("no save point", lambda e: e["kind"] != "save_point")):
            with self.subTest(row=row):
                events = {}
                for arm, tag in (("c1", "a"), ("c2", "b"), ("w", "w"), ("n", "n")):
                    made = remint(fixture(), tag)
                    if arm in ("w", "n"):
                        made = set_composer(made, wires_composer(WIRES_SHA if arm == "w" else NEGATIVE_SHA))
                    if arm == "n":
                        made = change_system_prompt(made)
                    events[arm] = [e for e in made if keep(e)]
                loads = sum(1 for e in events["c1"] if e["kind"] == "load")
                code, body = self.run_on(self.arms(c1=events["c1"], c2=events["c2"], wires=events["w"],
                                                   negative=events["n"],
                                                   write={a: {"loads": loads} for a in ("c1", "c2", "w", "n")}))
                self.assertEqual(code, compare.FAILS, body)
                self.assertIn("holds no load or no save point", body["reason"])
        # A record with no event at all is no record.
        code, body = self.run_on(self.arms(c1=[], c2=[], wires=[], negative=[]))
        self.assertEqual(code, compare.UNREADABLE, body)

    def test_the_compiled_arms_and_the_wires_arm_must_send_one_count_of_turns(self):
        for arm in ("c1", "c2", "w"):
            with self.subTest(arm=arm):
                code, body = self.run_on(self.arms(write={arm: {"turns": 9}}))
                self.assertEqual(code, compare.FAILS, body)
                self.assertIn("did not run one script", body["reason"])
        # The negative's prompt differs, so pressure may come a turn sooner or later there.
        code, body = self.run_on(self.arms(write={"n": {"turns": 9}}))
        self.assertEqual(code, compare.IDENTICAL, body)

    @staticmethod
    def append(folder, line):
        with open(os.path.join(folder, "trace.ndjson"), "a", encoding="utf-8") as f:
            f.write(line + "\n")

    @staticmethod
    def rewrite(folder, name, text):
        with open(os.path.join(folder, name), "w", encoding="utf-8") as f:
            f.write(text)


class TheSteps(unittest.TestCase):
    """The three steps on the real record's own shapes."""

    def test_the_renaming_names_each_minted_value_once_in_order_and_nothing_else(self):
        # Graded against the live shape: the session, three runs, two save points by
        # digest and by name, and the lineage, the build's branch, the reset and the
        # recall naming ids already met.
        events = fixture()
        normal = compare.normalize(events, compare.COMPILED_COMPOSER)
        self.assertEqual(normal[0]["session"], "id-1")
        self.assertEqual(normal[0]["run"], "id-2")
        self.assertEqual(normal[10]["payload"]["save_point"], "id-3")
        self.assertEqual(normal[10]["payload"]["run"], "id-2")
        self.assertEqual(normal[10]["payload"]["name"], "id-4")
        self.assertEqual(normal[12]["run"], "id-5")
        self.assertEqual(normal[12]["payload"]["lineage"]["save_point"], "id-3")
        self.assertEqual(normal[12]["payload"]["lineage"]["run"], "id-2")
        self.assertEqual(normal[24]["payload"]["lineage"]["built_from"], {"parent": "id-1", "run": "id-2", "through": 1})
        self.assertEqual(normal[24]["payload"]["reset"]["prior_run"], "id-5")
        self.assertEqual([r["run"] for r in normal[25]["payload"]["returned"]], ["id-2", "id-5"])
        # Nothing else moved: the digests of the declaration and the stack, the turn keys
        # and the sequences stand as the record wrote them.
        self.assertEqual(normal[0]["payload"]["declaration"], events[0]["payload"]["declaration"])
        self.assertEqual(normal[0]["payload"]["stack"], events[0]["payload"]["stack"])
        self.assertEqual(normal[3]["turn"], "t-1")
        self.assertEqual(normal[25]["payload"]["returned"][0]["sequence"], "4")
        ids = {v for e in normal for v in json.dumps(e).split('"') if v.startswith("id-")}
        self.assertEqual(ids, {"id-%d" % n for n in range(1, 9)})

    def test_the_clocks_the_timings_and_the_composer_alone_are_set_aside(self):
        events = fixture()
        normal = compare.normalize(events, compare.COMPILED_COMPOSER)
        measured = 0
        for before, after in zip(events, normal):
            self.assertEqual(set(before) - set(after), {"wall_ms", "monotonic_ns"})
            if before["kind"] == "model.measurement":
                measured += 1
                self.assertEqual(set(before["payload"]["timings"]), set(after["payload"]["timings"]))
                self.assertEqual({k for k, v in after["payload"]["timings"].items() if v == compare.SET_ASIDE},
                                 {"prefill_ns", "decode_ns"})
                self.assertEqual(set(before["payload"]) - set(after["payload"]), set())
        self.assertEqual(measured, 2, "the real record's two measurements")
        self.assertEqual(set(events[0]["payload"]) - set(normal[0]["payload"]), {"composer"})
        self.assertEqual(events[0]["payload"]["composer"], {"binary": "worker"}, "the real record's composer")

    def test_the_real_records_two_measurements_differ_by_their_timings_alone(self):
        # Graded against the live shape: two loads of one agent, each one turn, whose
        # measurements differ in the two elapsed times and are equal once step 1 holds.
        events = fixture()
        first, second = (i for i, e in enumerate(events) if e["kind"] == "model.measurement")
        self.assertNotEqual(events[first]["payload"]["timings"], events[second]["payload"]["timings"])
        normal = compare.normalize(events, compare.COMPILED_COMPOSER)
        self.assertEqual(normal[first]["payload"], normal[second]["payload"])

    def test_values_compare_by_json_type_as_well_as_value(self):
        rows = (("true against 1", True, 1), ("false against 0", False, 0),
                ("1 against 1.0", 1, 1.0), ("0.0 against -0.0", 0.0, -0.0),
                ("1 in a list against 1.0", [1], [1.0]), ("null against false", None, False))
        for row, left, right in rows:
            with self.subTest(row=row):
                a = [{"kind": "x", "payload": {"a": left}}]
                b = [{"kind": "x", "payload": {"a": right}}]
                difference = compare.first_difference(a, b)
                self.assertIsNotNone(difference, row)
                self.assertTrue(difference["at"].startswith("payload.a"), difference)
                self.assertIsNotNone(compare.first_difference(b, a), row)
        for row, value in (("members in another order", ({"a": 1, "b": [2.5, True]}, {"b": [2.5, True], "a": 1})),
                           ("equal floats", (0.1, 0.1)), ("equal strings", ("t-1", "t-1"))):
            with self.subTest(row=row):
                self.assertIsNone(compare.first_difference([{"kind": "x", "v": value[0]}],
                                                           [{"kind": "x", "v": value[1]}]))

    def test_pair_answers_equal_or_the_first_difference(self):
        made = Arms(self)
        a = made.write("a", "compiled", remint(fixture(), "a"))
        b = made.write("b", "wires", set_composer(remint(fixture(), "b"), wires_composer(WIRES_SHA)), sha=WIRES_SHA)
        self.assertEqual(compare.pair([a, b]), (compare.IDENTICAL, {"verdict": "identical"}))
        c = made.write("c", "wires", set_composer(change_system_prompt(remint(fixture(), "c")), wires_composer(WIRES_SHA)), sha=WIRES_SHA)
        code, body = compare.pair([a, c])
        self.assertEqual(code, compare.FAILS)
        self.assertEqual(body["difference"]["at"], "payload.content.0.text")


if __name__ == "__main__":
    unittest.main(verbosity=1)
