#!/usr/bin/env python3
"""`take-down.sh`'s plan and its refusals. The plan acts on nothing and needs no root, so
it is what can be measured here: the runbook's order with the archive first, every path
and account the agent's own and none the machine's, and the refusals before any step.
The acts themselves are the appliance's (`PROCEDURE.md`).

Run: `python3 tools/identity-run/test_take_down.py`. Standard library alone.
"""

import os
import re
import subprocess
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
SCRIPT = os.path.join(HERE, "take-down.sh")

THE_ORDER = [
    "read the agent's state and its paths, before anything is touched",
    "archive the agent's pieces into %s, as root, before anything is removed",
    "remove the sudo rule, so the connector can run nothing more on the agent",
    "remove the root, which ends the admission",
    "remove the agent's own folder, with any loop folder in it",
    "remove the run and runtime directories",
    "remove the territory",
    "remove the agent's /tmp entries, chosen by owner among its accounts",
    "the home's quota limit, where the agent's folder carries its uid as its project",
    "remove the agent's accounts, the agent's own with its home",
    "remove the agent's groups",
    "remove the agent's folder, which the steps above emptied",
]


def steps(out):
    return re.findall(r"^== step \d+: (.*)$", out, re.M)


class ThePlan(unittest.TestCase):
    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory(prefix="weaver-take-down-plan-")
        self.addCleanup(self.scratch.cleanup)
        self.archive = os.path.join(self.scratch.name, "arm", "archive")
        self.env = dict(os.environ)
        # A record and an admin base of the test's own, so the plan reads real values.
        self.stack = os.path.join(self.scratch.name, "stack")
        self.base = os.path.join(self.scratch.name, "admin")
        self.env["WEAVER_STACK_RECORD"] = self.stack
        self.env["WEAVER_ADMIN_CONFIG"] = self.base

    def lay(self, territory=None):
        os.makedirs(self.stack)
        with open(os.path.join(self.stack, "agent-directory"), "w") as f:
            f.write("/var/lib/weaver-agent\n")
        with open(os.path.join(self.stack, "coordination-root"), "w") as f:
            f.write("/run\n")
        os.makedirs(os.path.join(self.base, "a1"))
        with open(os.path.join(self.base, "a1", "territory"), "w") as f:
            f.write((territory or "/var/lib/weaver-agent/a1/weaver-a1") + "\n")

    def plan(self, *args, env=None):
        return subprocess.run(["bash", SCRIPT, *args], capture_output=True, text=True, env=env or self.env)

    def test_the_bash_parses(self):
        self.assertEqual(subprocess.run(["bash", "-n", SCRIPT]).returncode, 0)

    def test_the_plan_runs_the_runbooks_order_with_the_archive_before_any_removal(self):
        ran = self.plan("--agent", "a1", "--archive", self.archive)
        self.assertEqual(ran.returncode, 0, ran.stdout + ran.stderr)
        self.assertEqual(steps(ran.stdout), [s % self.archive if "%s" in s else s for s in THE_ORDER])
        self.assertIn("plan only", ran.stdout)
        commands = [l for l in ran.stdout.splitlines() if l.startswith("+ ")]
        first_removal = next(i for i, l in enumerate(commands) if re.match(r"\+ (rm|userdel|groupdel|rmdir) ", l))
        tar = next(i for i, l in enumerate(commands) if l.startswith("+ tar --create"))
        self.assertLess(tar, first_removal)

    def test_with_the_record_standing_the_plan_names_the_agents_own_paths(self):
        self.lay()
        ran = self.plan("--agent", "a1", "--archive", self.archive)
        self.assertEqual(ran.returncode, 0, ran.stdout + ran.stderr)
        out = ran.stdout
        self.assertIn("+ rm -rf --one-file-system -- %s/a1\n" % self.base, out)
        self.assertIn("+ rm -rf --one-file-system -- /etc/weaver/a1\n", out)
        self.assertIn("+ rm -rf --one-file-system -- /run/weaver.run/a1 /run/weaver-a1\n", out)
        self.assertIn("+ rm -rf --one-file-system -- /var/lib/weaver-agent/a1/weaver-a1\n", out)
        self.assertIn("+ rmdir -- /var/lib/weaver-agent/a1\n", out)
        tar = next(l for l in out.splitlines() if l.startswith("+ tar"))
        self.assertIn("var/lib/weaver-agent/a1/weaver-a1", tar)
        self.assertTrue(tar.endswith(" var/lib/weaver-agent/a1/weaver-a1 var/lib/weaver-agent/a1/home"), tar)

    def test_every_path_and_account_is_the_agents_own_and_no_connector_is_touched(self):
        self.lay()
        ran = self.plan("--agent", "a1", "--archive", self.archive)
        acting = [l for l in ran.stdout.splitlines() if re.match(r"\+ (rm|rmdir|userdel|groupdel|find) ", l)]
        self.assertEqual(len(acting), 16, acting)
        for line in acting:
            with self.subTest(line=line):
                self.assertRegex(line, r"weaver-a1\b|/a1\b")
                self.assertNotRegex(line, r"weaver-(gatecon|admincon)\b|weaver-a1-admincon")
        self.assertEqual(re.findall(r"^\+ userdel (?:-r )?(\S+)$", ran.stdout, re.M),
                         ["weaver-a1", "weaver-a1-state", "weaver-a1-relay"])
        self.assertEqual(re.findall(r"^\+ groupdel (\S+)$", ran.stdout, re.M),
                         ["weaver-a1", "weaver-a1-state", "weaver-a1-trace", "weaver-a1-admin"])

    def test_a_territory_that_is_not_the_agents_own_refuses_before_any_step_past_the_reads(self):
        for row, territory in (("another agent's", "/var/lib/weaver-agent/b1/weaver-b1"),
                               ("the base itself", "/var/lib/weaver-agent"),
                               ("a path beside", "/var/lib/weaver-agent/a1/weaver-a1/../../b1/weaver-b1")):
            with self.subTest(row=row):
                self.scratch.cleanup()
                os.makedirs(self.scratch.name)
                self.lay(territory)
                ran = self.plan("--agent", "a1", "--archive", self.archive)
                self.assertEqual(ran.returncode, 2, ran.stdout + ran.stderr)
                self.assertIn("is not the agent's own", ran.stderr)
                self.assertNotIn("== step 2", ran.stdout)

    def test_another_agents_territory_inside_this_agents_folder_refuses_before_anything(self):
        self.lay()
        os.makedirs(os.path.join(self.base, "b1"))
        with open(os.path.join(self.base, "b1", "territory"), "w") as f:
            f.write("/var/lib/weaver-agent/a1/weaver-b1\n")
        ran = self.plan("--agent", "a1", "--archive", self.archive)
        self.assertEqual(ran.returncode, 2, ran.stdout + ran.stderr)
        self.assertIn("names the territory /var/lib/weaver-agent/a1/weaver-b1, inside /var/lib/weaver-agent/a1,", ran.stderr)
        self.assertNotIn("== step 2", ran.stdout)
        # The control: b1's territory beside a1's folder plans.
        with open(os.path.join(self.base, "b1", "territory"), "w") as f:
            f.write("/var/lib/weaver-agent/b1/weaver-b1\n")
        ran = self.plan("--agent", "a1", "--archive", self.archive)
        self.assertEqual(ran.returncode, 0, ran.stdout + ran.stderr)

    def test_an_archive_inside_a_folder_the_take_down_removes_refuses_before_anything(self):
        # Review of #655: the archive is written at step 2 and the folders removed at
        # steps 3 to 7, so an archive inside one would be taken with it. Judged by
        # canonical path, a link or a `..` reaching inside refuses as a plain path does.
        # Perturbation: drop the judgment, and every row plans on.
        self.lay()
        link = os.path.join(self.scratch.name, "to-root")
        os.symlink(os.path.join(self.base, "a1"), link)
        for row, archive in (("the agent's folder", "/var/lib/weaver-agent/a1"),
                             ("inside the agent's folder", "/var/lib/weaver-agent/a1/kept"),
                             ("inside the territory", "/var/lib/weaver-agent/a1/weaver-a1/kept"),
                             ("inside the root", os.path.join(self.base, "a1", "kept")),
                             ("inside its own folder", "/etc/weaver/a1/kept"),
                             ("inside the run directory", "/run/weaver.run/a1/kept"),
                             ("inside the runtime directory", "/run/weaver-a1/kept"),
                             ("through a link", os.path.join(link, "kept")),
                             ("through a ..", "/var/lib/weaver-agent/b1/../a1/kept")):
            with self.subTest(row=row):
                ran = self.plan("--agent", "a1", "--archive", archive)
                self.assertEqual(ran.returncode, 2, ran.stdout + ran.stderr)
                self.assertIn("which this take-down removes", ran.stderr)
                self.assertNotIn("== step 2", ran.stdout)
        # The control: an archive beside the agent's folder plans.
        ran = self.plan("--agent", "a1", "--archive", "/var/lib/weaver-agent/a1-kept")
        self.assertEqual(ran.returncode, 0, ran.stdout + ran.stderr)

    def test_every_folder_is_removed_on_its_own_file_system(self):
        self.lay()
        ran = self.plan("--agent", "a1", "--archive", self.archive)
        removals = [l for l in ran.stdout.splitlines() if re.match(r"\+ (rm -rf|find) ", l)]
        self.assertEqual(len(removals), 7, removals)
        for line in removals:
            with self.subTest(line=line):
                self.assertIn("rm -rf --one-file-system --", line)

    def test_the_territory_is_compared_with_the_canonical_agent_directory(self):
        real = os.path.join(self.scratch.name, "agents")
        os.makedirs(real)
        link = os.path.join(self.scratch.name, "agents-link")
        os.symlink(real, link)
        territory = os.path.join(real, "a1", "weaver-a1")
        for row, written in (("a trailing slash", real + "/"), ("through a link", link)):
            with self.subTest(row=row):
                for d in (self.stack, self.base):
                    subprocess.run(["rm", "-rf", "--", d], check=True)
                self.lay(territory)
                with open(os.path.join(self.stack, "agent-directory"), "w") as f:
                    f.write(written + "\n")
                ran = self.plan("--agent", "a1", "--archive", self.archive)
                self.assertEqual(ran.returncode, 0, ran.stdout + ran.stderr)
                self.assertIn("+ rm -rf --one-file-system -- %s\n" % territory, ran.stdout)

    def test_an_account_whose_home_is_not_the_agents_own_refuses_before_anything(self):
        self.lay()
        stubs = os.path.join(self.scratch.name, "stubs")
        os.makedirs(stubs)
        for row, home, code in (("elsewhere", "/", 2), ("the agent's own", "/var/lib/weaver-agent/a1/home", 0)):
            with self.subTest(row=row):
                with open(os.path.join(stubs, "getent"), "w") as f:
                    f.write('#!/bin/sh\n[ "$1 $2" = "passwd weaver-a1" ] && { echo "weaver-a1:x:5000:5000::%s:/sbin/nologin"; exit 0; }\nexit 2\n' % home)
                os.chmod(os.path.join(stubs, "getent"), 0o755)
                env = dict(self.env, PATH=stubs + os.pathsep + self.env["PATH"])
                ran = self.plan("--agent", "a1", "--archive", self.archive, env=env)
                self.assertEqual(ran.returncode, code, ran.stdout + ran.stderr)
                if code:
                    self.assertIn("userdel -r would remove it", ran.stderr)
                    self.assertNotIn("== step 2", ran.stdout)

    def test_a_name_that_is_not_an_agents_refuses_before_any_step(self):
        for name in ("gatecon", "admincon", "adminx", "stack", "A1", "a", "a-1", "a1/b", "a" * 17, ""):
            with self.subTest(name=name):
                ran = self.plan("--agent", name, "--archive", self.archive)
                self.assertEqual(ran.returncode, 2, ran.stdout + ran.stderr)
                self.assertNotIn("== step", ran.stdout)

    def test_the_archive_must_be_a_new_absolute_folder(self):
        os.makedirs(self.archive)
        for row, archive in (("standing", self.archive), ("relative", "arm/archive")):
            with self.subTest(row=row):
                ran = self.plan("--agent", "a1", "--archive", archive)
                self.assertEqual(ran.returncode, 2, ran.stdout + ran.stderr)
                self.assertNotIn("== step 2", ran.stdout)

    def test_a_per_agent_connector_standing_refuses(self):
        stubs = os.path.join(self.scratch.name, "stubs")
        os.makedirs(stubs)
        with open(os.path.join(stubs, "getent"), "w") as f:
            f.write('#!/bin/sh\n[ "$1 $2" = "passwd weaver-a1-admincon" ] && { echo "weaver-a1-admincon:x:5000:5000::/:/sbin/nologin"; exit 0; }\nexit 2\n')
        os.chmod(os.path.join(stubs, "getent"), 0o755)
        env = dict(self.env, PATH=stubs + os.pathsep + self.env["PATH"])
        ran = self.plan("--agent", "a1", "--archive", self.archive, env=env)
        self.assertEqual(ran.returncode, 2, ran.stdout + ran.stderr)
        self.assertIn("removes no connector account", ran.stderr)
        self.assertNotIn("== step 2", ran.stdout)

    def test_apply_is_roots_and_refused_to_anyone_else_before_it_touches_anything(self):
        if os.geteuid() == 0:
            self.skipTest("run as root, where --apply would act")
        ran = self.plan("--agent", "a1", "--archive", self.archive, "--apply")
        self.assertEqual(ran.returncode, 2)
        self.assertIn("--apply is root's", ran.stderr)
        self.assertFalse(os.path.exists(self.archive))


if __name__ == "__main__":
    unittest.main(verbosity=1)
