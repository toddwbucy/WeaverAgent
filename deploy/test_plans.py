#!/usr/bin/env python3
"""Deploy plan boundaries, using subprocess fixtures (no real sudo or build).

Run: python3 deploy/test_plans.py
The command doubles record every privileged/build invocation. They never
forward sudo, systemctl, package-manager or Cargo calls to the host.
"""

import json
import os
from pathlib import Path
import shlex
import re
import shutil
import subprocess
import sys
import tempfile
import unittest


DEPLOY = Path(__file__).resolve().parent
DOUBLE = r'''#!/usr/bin/env python3
import json, os, pathlib, shutil, subprocess, sys
name = pathlib.Path(sys.argv[0]).name
args = sys.argv[1:]
root = pathlib.Path(os.environ['FIXTURE_ROOT'])
if name == 'stat':
    # **The fixture's files are root's.** Every file the tests make is the
    # tester's, so the ownership `held_closed` judges is answered as uid 0
    # for anything under the fixture and read from the file itself elsewhere;
    # the mode is always the file's own. Not logged: it is a look, not a call.
    if args[:1] == ['-c'] and args[1] == '%u %G %a':
        # The trace's look: root under the fixture, its group what the test
        # names, its mode the file's own.
        path = pathlib.Path(args[-1])
        st = os.lstat(path)
        print(0 if path.is_relative_to(root) else st.st_uid, os.environ.get('TRACE_GROUP_AS', 'nobody-group'),
              format(st.st_mode & 0o7777, 'o'))
        sys.exit(0)
    if args[:1] == ['-c'] and args[1] == '%u:%G:%a':
        # The territory's look (#99 area 2 review, N1): root under the
        # fixture; the group LAYOUT_GROUPS names for the path, or else the
        # one the law names (a territory `weaver-<a>` the state group's, its
        # `save-points` the access group's); the mode the directory's own.
        path = pathlib.Path(args[-1])
        st = os.lstat(path)
        groups = json.loads(os.environ.get('LAYOUT_GROUPS', '{}'))
        if str(path) in groups: group = groups[str(path)]
        elif path.name == 'save-points': group = path.parent.name + '-admin'
        else: group = path.name + '-state'
        print(f"{0 if path.is_relative_to(root) else st.st_uid}:{group}:{format(st.st_mode & 0o7777, 'o')}")
        sys.exit(0)
    if args[:1] == ['-c'] and args[1] == '%u %a':
        path = pathlib.Path(args[-1])
        st = os.lstat(path)
        print(0 if path.is_relative_to(root) else st.st_uid, format(st.st_mode & 0o7777, 'o'))
        sys.exit(0)
    os.execv('/usr/bin/stat', ['stat', *args])
with open(os.environ['CALLS'], 'a') as log:
    log.write(json.dumps([name, *args]) + '\n')
def mapped(value):
    if value.startswith('/home/') or value.startswith('/etc/sudoers'):
        return str(root / value.lstrip('/'))
    return value

def privileged(paths, run):
    """Run `run` with every closed directory above the mapped `paths` opened
    to this user for the call, as root's privilege would pass it."""
    opened = []
    for p in paths:
        p = pathlib.Path(p)
        if not p.is_absolute() or not p.is_relative_to(root): continue
        for d in p.parents:
            if not d.is_relative_to(root) or d == root: break
            try: st = os.lstat(d)
            except OSError: continue
            if (st.st_mode & 0o700) != 0o700 and os.path.isdir(d) and not os.path.islink(d):
                opened.append((d, st.st_mode & 0o7777)); os.chmod(d, 0o700)
    try: return run()
    finally:
        for d, mode in reversed(opened):
            try: os.chmod(d, mode)
            except OSError: pass

def shell_read(arguments):
    if os.environ.get('PATH_FAIL') and os.environ['PATH_FAIL'] in arguments: sys.exit(1)
    rewritten = list(map(mapped, arguments))
    for argument in rewritten:
        if argument.startswith('/'):
            assert pathlib.Path(argument).is_relative_to(root), argument
    sys.exit(subprocess.run(['/bin/sh', *rewritten]).returncode)

if name == 'sh': shell_read(args)
elif name == 'getent':
    user = os.environ.get('USER', '')
    if args == ['passwd', user]:
        uid = os.environ.get('FIXTURE_UID', '12345')
        print(f"{user}:x:{uid}:{uid}::{root / 'home' / user}:/bin/bash")
        sys.exit(0)
    if os.environ.get('ACCOUNT_FAIL'): sys.exit(1)
    if args[:1] == ['group'] and os.environ.get('COLLISION_GROUP') == args[-1]: sys.exit(0)
    sys.exit(0 if os.environ.get('COLLISION') == args[-1] else 2)
elif name == 'id':
    if args == ['-un']: print(os.environ.get('USER', ''))
    elif args[:1] == ['-Gn']:
        who = args[1]
        if who.endswith('-relay'): print(os.environ.get('RELAY_GROUPS', who.removesuffix('-relay') + '-trace'))
        elif who.endswith('-admincon'): print(os.environ.get('CONNECTOR_GROUPS', who + ' ' + who.removesuffix('admincon') + 'state ' + who.removesuffix('con')))
        else: print(who)
    elif args[:1] == ['-u'] and len(args) > 1 and 'FIXTURE_ACCOUNT_UID' in os.environ:
        print(os.environ['FIXTURE_ACCOUNT_UID'])
    else: print(os.environ.get('FIXTURE_UID', '12345'))
elif name == 'systemctl':
    if args[:1] == ['list-units']:
        if os.environ.get('UNITS_FAIL'): sys.exit(1)
        print(os.environ.get('UNITS', ''), end='')
        sys.exit(0)
    sys.exit(99)
elif name == 'git':
    if args[0] == 'rev-parse': print('abcdef0')
    elif args[0] == 'branch': print('fixture-branch')
elif name == 'hostname': print('fixture-box')
elif name == 'nvidia-smi': print('fixture-driver')
elif name == 'pacman': print('cccl 3.3.4-1')
elif name == 'cargo':
    target = pathlib.Path(os.environ['CARGO_TARGET_DIR'])
    if args[0] == 'metadata': print(json.dumps({'target_directory': str(target)}))
    elif args[0] == 'test': print('test result: ok. 1 passed; 0 failed; 0 ignored')
    elif args[0] == 'build':
        if os.environ.get('BUILD_FAIL'):
            print('fixture build refusal', file=sys.stderr)
            sys.exit(42)
        target.joinpath('release').mkdir(parents=True)
        for member in ('pyworker', 'worker', 'weaver-admin', 'weaver-trace-relay', 'weaver-gate', 'weaver-spu', 'weaver-state'):
            target.joinpath('release', member).write_text('fixture artifact ' + member)
    else: sys.exit(99)
elif name == 'sudo':
    if not os.environ.get('ALLOW_APPLY_CHECKS'): sys.exit(99)
    if args == ['-v']: sys.exit(1 if os.environ.get('SUDO_FAIL') else 0)
    command = args[:]
    if command[0] == '-n': command.pop(0)
    identity = ''
    if command[0] == '-u':
        command.pop(0)
        identity = command.pop(0)
    op, *rest = command
    if op == 'sh': shell_read(rest)
    elif op == 'systemctl': sys.exit(2 if os.environ.get('READ_FAIL') == rest[0] else 0)
    elif op in ('grep', 'sed'):
        # Execute only the text operation on scratch files, never via sudo.
        file = pathlib.Path(mapped(rest[-1]))
        assert file.is_relative_to(root), file
        sys.exit(subprocess.run(['/usr/bin/' + op, *rest[:-1], str(file)]).returncode)
    elif op == 'tail' or (op == 'test' and not identity):
        # A look made as root, or the state log read as the member: run on
        # the scratch file.
        file = pathlib.Path(mapped(rest[-1]))
        assert file.is_relative_to(root), file
        args = [a for a in rest[:-1] if a != '--']
        sys.exit(privileged([file], lambda: subprocess.run(['/usr/bin/' + op, *args, str(file)]).returncode))
    elif op == 'tee':
        file = pathlib.Path(mapped(rest[-1]))
        assert file.is_relative_to(root), file
        text = sys.stdin.read()
        def write():
            with file.open('a' if '-a' in rest else 'w') as output: output.write(text)
        privileged([file], write)
    elif op == 'cp':
        source, destination = (pathlib.Path(mapped(a)) for a in rest[-2:])
        assert source.is_relative_to(root) and destination.is_relative_to(root)
        privileged([source, destination], lambda: shutil.copyfile(source, destination))
    elif op == 'install':
        target = pathlib.Path(mapped(rest[-1]))
        assert target.is_relative_to(root), target
        if '-d' in rest: privileged([target], lambda: target.mkdir(parents=True, exist_ok=True))
        else: privileged([target], target.touch)
    elif op == 'mv':
        source, destination = (pathlib.Path(mapped(a)) for a in rest[-2:])
        assert source.is_relative_to(root) and destination.is_relative_to(root)
        def move():
            assert not destination.exists(), destination
            source.rename(destination)
        privileged([source, destination], move)
    elif op == 'mktemp':
        template = rest[-1]
        made = pathlib.Path(mapped(template.replace('XXXXXX', 'fixture')))
        assert made.is_relative_to(root), made
        if '-d' in rest: made.mkdir()
        else: made.touch()
        print(template.replace('XXXXXX', 'fixture'))
    elif op == 'visudo': sys.exit(1 if os.environ.get('VISUDO_FAIL') else 0)
    elif op.startswith('WEAVER_ADMIN_CONFIG=') and os.environ.get('FIXTURE_ADMIN'):
        # **Admin as a test stands it in** (#99 area 2 review, N5): the
        # verb and the agent handed to the script FIXTURE_ADMIN names.
        assert rest[0].endswith('/weaver-admin'), rest
        sys.exit(subprocess.run([os.environ['FIXTURE_ADMIN'], *rest[1:]]).returncode)
    elif op == 'rm':
        target = pathlib.Path(mapped(rest[-1]))
        assert target.is_relative_to(root), target
        privileged([target], lambda: target.unlink(missing_ok=True))
    elif op == '-l':
        print('User ' + rest[-1] + ' may run the following commands on fixture-box:')
        for rule in sorted((root / 'etc' / 'sudoers.d').glob('weaver-*')):
            print('    ' + rule.read_text().splitlines()[-1])
    elif op == 'test':
        # The probes of a sqlite agent's state room, asked as an account: the
        # member passes, the agent's own uid is refused, unless the fixture
        # opens the wall.
        if identity == 'weaver-m1': sys.exit(0 if os.environ.get('WALL_OPEN') else 1)
        # The member's read of the trace: refused, unless the fixture opens it.
        if identity == 'weaver-m1-state' and '-r' in rest: sys.exit(0 if os.environ.get('TRACE_OPEN') else 1)
        # The connector's write of the territory's files: refused, unless the
        # fixture opens the group.
        if identity == 'weaver-m1-admincon' and '-w' in rest: sys.exit(0 if os.environ.get('GROUP_WRITES') else 1)
        sys.exit(0)
    elif op in ('chown', 'chgrp'):
        # **An owner is set only on what stands** (#99 area 2 review, K11):
        # a path under the fixture that does not exist fails, as chown does.
        for a in rest:
            p = pathlib.Path(mapped(a))
            if a.startswith('/') and p.is_relative_to(root) and not privileged([p], lambda: os.path.lexists(p)):
                sys.exit(1)
    elif op in ('useradd', 'usermod', 'groupadd', 'chmod', 'setfacl'): pass
    else: sys.exit(99)
elif name == 'mktemp':
    if not os.environ.get('ALLOW_APPLY_CHECKS'): sys.exit(99)
    probe = pathlib.Path(os.environ['PROBE'])
    probe.mkdir()
    print(probe)
elif name == 'setfacl': sys.exit(1 if os.environ.get('ACL_FAIL') else 0)
else: sys.exit(99)
'''

class PlanTests(unittest.TestCase):
    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory(prefix="weaver-deploy-test-")
        self.addCleanup(self.scratch.cleanup)
        self.root = Path(self.scratch.name)
        self.repo = self.root / "repo"
        shutil.copytree(DEPLOY, self.repo / "deploy")
        # The admin base, holding one agent's root, and the stack record.
        self.config = self.root / "config"
        self.config.mkdir()
        existing = self.config / "existing"
        existing.mkdir()
        (existing / "worker-binary").write_text(str(self.root / "installed" / "pyworker"))
        # The operator's home, which no script writes into since the
        # operator's ruling of 2026-10-07 on #1: the whole agent lives in its
        # territory under the stack record's agent-directory.
        self.home = self.root / "home"
        self.operator_home = self.home / "fixture-no-home"
        self.operator_home.mkdir(parents=True)
        (self.root / "agents").mkdir()
        self.existing_territory = self.root / "agents" / "weaver-existing"
        self.existing_territory.mkdir()
        (self.existing_territory / "agent.toml").write_text("[state-store]\nengine = \"none\"\n")
        # Laid to the law admin judges (weaver-admin-Spec section 9), as
        # create-agent.sh lays it: the territory 0710, its save-points/ 0750,
        # the groups the stand-in stat answers by name.
        self.existing_territory.chmod(0o710)
        (self.existing_territory / "save-points").mkdir(mode=0o750)
        (self.existing_territory / "save-points").chmod(0o750)
        (existing / "territory").write_text(str(self.existing_territory) + "\n")
        # m1's territory, where its declaration and draft land.
        self.decl = self.root / "agents" / "weaver-m1"
        # The box's sudoers, which includes sudoers.d, mapped into the fixture.
        (self.root / "etc" / "sudoers.d").mkdir(parents=True)
        (self.root / "etc" / "sudoers").write_text("@includedir /etc/sudoers.d\n")
        self.rule = self.root / "etc" / "sudoers.d" / "weaver-m1"
        # The box runs systemd as its manager, unless a test removes the mark.
        (self.root / "run" / "systemd" / "system").mkdir(parents=True)
        self.stack = self.root / "stack"
        self.stack.mkdir()
        self.logs = self.root / "log"
        self.logs.mkdir()
        self.stack_keys = {
            "worker-binary": str(self.root / "installed" / "pyworker"),
            "spu-binary": str(self.root / "installed" / "weaver-spu"),
            "gate-binary": str(self.root / "installed" / "weaver-gate"),
            "coordination-root": "/run",
            "library-path": str(self.root / "installed" / "lib"),
            "agent-directory": str(self.root / "agents"),
            "prefix": str(self.root / "installed"),
        }
        for key, value in self.stack_keys.items():
            (self.stack / key).write_text(value + ("" if value.endswith("\n") else "\n"))
        self.hba = self.root / "pg_hba.conf"
        self.hba.write_text("local all all peer\n")
        self.ident = self.root / "pg_ident.conf"
        self.ident.touch()
        self.artifact = self.root / "model.gguf"
        self.artifact.touch()
        self.bin = self.root / "bin"
        self.bin.mkdir()
        for name in ("sudo", "systemctl", "psql", "mktemp", "setfacl", "getent", "git", "cargo", "hostname", "nvidia-smi", "pacman", "sh", "id", "stat"):
            command = self.bin / name
            command.write_text(DOUBLE)
            command.chmod(0o755)
        self.log = self.root / "calls"
        self.env = {**os.environ, "PATH": str(self.bin) + os.pathsep + os.environ["PATH"],
                    "WEAVER_ADMIN_CONFIG": str(self.config), "CALLS": str(self.log),
                    "WEAVER_STACK_RECORD": str(self.stack),
                    "CARGO_TARGET_DIR": str(self.root / 'target with "quotes"'),
                    "USER": "fixture-no-home", "PROBE": str(self.root / "probe"),
                    "FIXTURE_ROOT": str(self.root)}
        for name in ("BASH_ENV", "SUDO_USER", "COLLISION", "ALLOW_APPLY_CHECKS", "TRACE_OPEN", "BUILD_FAIL", "SUDO_FAIL", "READ_FAIL", "EMPTY_PATH", "ACL_FAIL", "PATH_FAIL", "ACCOUNT_FAIL", "WALL_OPEN",
                     "VISUDO_FAIL", "RELAY_GROUPS", "CONNECTOR_GROUPS", "UNITS", "UNITS_FAIL", "FIXTURE_ACCOUNT_UID",
                     "COLLISION_GROUP", "KEEP_ALIVE", "TRACE_GROUP_AS", "GROUP_WRITES",
                     "LAYOUT_GROUPS", "FIXTURE_ADMIN", "ADMIN_CALLS", "UNLOAD_ANSWER"):
            self.env.pop(name, None)
        # Redirect even shell builtin /home probes into the fixture. The
        # production scripts have no test-only path switches and never read
        # the host's real agent homes during these tests.
        preamble = self.root / "fixture.bash"
        preamble.write_text("""fixture_args() {
  local arg
  fixture_mapped=()
  for arg in "$@"; do
    case "$arg" in /home/*|/run/systemd/system) arg="$FIXTURE_ROOT$arg";; esac
    fixture_mapped+=("$arg")
  done
}
[() { fixture_args "$@"; builtin [ "${fixture_mapped[@]}"; }
test() { fixture_args "$@"; builtin test "${fixture_mapped[@]}"; }
""")
        self.env["BASH_ENV"] = str(preamble)

    def run_script(self, name, *args):
        return subprocess.run(["bash", str(self.repo / "deploy" / name), *args],
                              env=self.env, text=True, capture_output=True, timeout=20)

    def calls(self):
        return [json.loads(line) for line in self.log.read_text().splitlines()] if self.log.exists() else []

    def create(self, *args):
        # sqlite is the default, named here anyway so a case reads the
        # election it runs under rather than assuming it.
        engine = [] if "--engine" in args else ["--engine", "sqlite"]
        return self.create_naming(*engine, *args)

    def install_stack(self):
        # The installed admin the sudo rule names, and the relay admin starts
        # from beside the worker, as bootstrap-stack.sh leaves them.
        (self.root / "installed" / "bin").mkdir(parents=True, exist_ok=True)
        for binary in (self.root / "installed" / "bin" / "weaver-admin",
                       self.root / "installed" / "weaver-trace-relay"):
            if not binary.exists():
                binary.write_text("#!/bin/sh\nexit 0\n")
                binary.chmod(0o755)

    def create_naming(self, *args):
        self.install_stack()
        return self.run_script("create-agent.sh", "m1", "--artifact", str(self.artifact), *args)

    def assert_no_printed_root_command(self, result):
        # No script prints a root command for the operator to copy, every
        # remedy pointing to the runbook instead (the Planner's call on #82).
        # A command is sudo followed by an option, a path, an assignment or a
        # file verb; the plan's "sudo rule" label is none of these.
        import re as _re
        command = _re.compile(r"(^|[\s:])sudo\s+(-|/|[A-Z_]+=|deploy/|chown|chmod|chgrp|rm\b|mv\b)")
        printed = [l for l in (result.stdout + result.stderr).splitlines() if command.search(l)]
        self.assertEqual(printed, [], printed)

    def assert_unprivileged(self):
        # `systemctl list-units` is a look any user may take, and the only
        # systemctl call a plan makes.
        forbidden = {"sudo", "systemctl", "psql", "mktemp", "setfacl"}
        self.assertFalse([c for c in self.calls() if c[0] in forbidden
                          and c[:2] != ["systemctl", "list-units"]], self.calls())

    def test_agent_plan_defers_privilege_and_preserves_fixture_files(self):
        self.install_stack()
        before = {p: p.read_bytes() for p in self.root.rglob("*") if p.is_file()}
        result = self.create()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn(str(self.decl / "agent.toml"), result.stdout)
        self.assertIn("/etc/sudoers.d/weaver-m1", result.stdout)
        self.assertIn("weaver-m1-admincon", result.stdout)
        self.assertFalse(self.decl.exists(), "the plan writes no declaration")
        self.assertIn("PENDING --apply", result.stdout)
        self.assertNotIn("nothing of this agent exists", result.stdout)
        self.assert_unprivileged()
        after = {p: p.read_bytes() for p in self.root.rglob("*") if p.is_file() and p != self.log}
        self.assertEqual(before, after)

    def test_agent_plan_refuses_missing_or_empty_stack_keys(self):
        # Every required key of the stack record refuses by name, absent or
        # empty, before anything privileged. Perturbation: drop the check and
        # an agent root is written without the key admin requires.
        for key in ("worker-binary", "coordination-root", "agent-directory", "prefix"):
            for value in (None, "  \n"):
                with self.subTest(key=key, value=value):
                    if value is None: (self.stack / key).unlink()
                    else: (self.stack / key).write_text(value)
                    result = self.create()
                    self.assertNotEqual(result.returncode, 0)
                    self.assertIn(key, result.stderr)
                    (self.stack / key).write_text(self.stack_keys[key] + "\n")
        self.assert_unprivileged()

    def test_missing_stack_record_refuses_in_both_modes(self):
        shutil.rmtree(self.stack)
        for apply in (False, True):
            with self.subTest(apply=apply):
                self.env["ALLOW_APPLY_CHECKS"] = "1"
                result = self.create(*(["--apply"] if apply else []))
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("no stack record", result.stderr)
                self.assertFalse(any(c[0] == "sudo" for c in self.calls()))

    def test_agent_plan_refuses_visible_collisions(self):
        for collision in ("account", "relay account", "connector account", "access group",
                          "agent root", "staged root", "sudo rule", "territory"):
            with self.subTest(collision=collision):
                self.env.pop("COLLISION", None)
                for path in (self.config / "m1", self.config / ".m1.partial"):
                    if path.exists(): path.rmdir()
                self.rule.unlink(missing_ok=True)
                shutil.rmtree(self.decl, ignore_errors=True)
                if collision == "account": self.env["COLLISION"] = "weaver-m1-state"
                elif collision == "relay account": self.env["COLLISION"] = "weaver-m1-relay"
                elif collision == "connector account": self.env["COLLISION"] = "weaver-m1-admincon"
                elif collision == "access group": self.env["COLLISION"] = "weaver-m1-admin"
                elif collision == "agent root": (self.config / "m1").mkdir()
                elif collision == "staged root": (self.config / ".m1.partial").mkdir()
                elif collision == "sudo rule": self.rule.write_text("")
                else:
                    self.decl.mkdir(parents=True)
                    (self.decl / "agent.toml").write_text("")
                result = self.create()
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("already", result.stderr)
        self.assert_unprivileged()

    def test_apply_requires_sudo_before_any_other_privileged_call(self):
        self.env.update(ALLOW_APPLY_CHECKS="1", SUDO_FAIL="1")
        result = self.create("--apply")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("--apply needs sudo", result.stderr)
        self.assertEqual([c for c in self.calls() if c[0] == "sudo"], [["sudo", "-v"]])

    def assert_no_provisioning(self):
        self.assertFalse(any("useradd" in c or any("CREATE ROLE" in a or "CREATE DATABASE" in a for a in c)
                             for c in self.calls()), self.calls())

    def test_missing_admin_base_refuses_both_modes(self):
        self.env["WEAVER_ADMIN_CONFIG"] = str(self.root / "missing")
        for apply in (False, True):
            with self.subTest(apply=apply):
                self.env["ALLOW_APPLY_CHECKS"] = "1"
                result = self.create(*(["--apply"] if apply else []))
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("admin base", result.stderr)
                self.assert_no_provisioning()

    def test_a_value_the_toml_string_cannot_carry_refuses_before_anything(self):
        # `--session 'trial"2'` would write `session = "trial"2"`. Each value
        # refuses at argument parsing, in both modes, before any command.
        # Perturbation: drop the character check and the plan runs.
        for flag, value in (("--session", 'trial"2'), ("--session", "a\\b"),
                            ("--session", "two\nlines"), ("--artifact", '/m/x"y.gguf'),
                            ("--artifact", "/m/x\\y.gguf"), ("--artifact", "/m/x\ty.gguf")):
            for mode in ((), ("--apply",)):
                with self.subTest(flag=flag, value=value, mode=mode):
                    self.log.unlink(missing_ok=True)
                    args = ["m1", "--artifact", str(self.artifact)] if flag == "--session" else ["m1"]
                    result = self.run_script("create-agent.sh", *args, flag, value, *mode)
                    self.assertEqual(result.returncode, 1, result.stderr)
                    self.assertIn("cannot hold as written", result.stderr)
                    self.assertEqual(self.calls(), [])

    def test_the_rendered_declaration_is_toml_before_anything_is_made(self):
        # The plan renders and parse-checks the declaration it would write.
        # Perturbation: break the heredoc's quoting and the plan refuses here.
        for engine in ("sqlite",):
            with self.subTest(engine=engine):
                result = self.create("--session", "s-m1-1", "--engine", engine)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertNotIn("does not parse as TOML", result.stderr)

    def test_invalid_engine_does_not_prompt_for_sudo(self):
        for engine in ("invalid", "none"):
            with self.subTest(engine=engine):
                result = self.create("--apply", "--engine", engine)
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse(any(c[0] == "sudo" for c in self.calls()))

    def test_apply_uses_privileged_collision_reads(self):
        for path in (self.config / "m1", self.home / "weaver-m1",
                     self.root / "agents" / "weaver-m1"):
            with self.subTest(path=path):
                self.log.unlink(missing_ok=True)
                path.touch()
                self.env["ALLOW_APPLY_CHECKS"] = "1"
                result = self.create("--apply")
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("already exists", result.stderr)
                logical = str(path).removeprefix(str(self.root)) if path.is_relative_to(self.home) else str(path)
                self.assertTrue(any(c[0] == "sudo" and logical in c for c in self.calls()))
                self.assert_no_provisioning()
                path.unlink()

    def test_stack_values_are_trimmed(self):
        (self.stack / "agent-directory").write_text(f"  {self.root / 'agents'} \r\n")
        result = self.create()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn(f"territory       {self.root / 'agents' / 'weaver-m1'} ", result.stdout)

    def test_the_territory_is_named_by_its_canonical_path(self):
        # The #94 survey's S23: admin compares the declaration's sink
        # directory with the canonical territory, so a base named through a
        # link is resolved before the territory, the sink and the root's
        # `territory` key are written from it. Perturbation: drop the
        # realpath and the linked path is planned.
        linked = self.root / "linked-agents"
        linked.symlink_to(self.root / "agents")
        (self.stack / "agent-directory").write_text(f"{linked}\n")
        result = self.create()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn(f"territory       {self.root / 'agents' / 'weaver-m1'} ", result.stdout)
        self.assertNotIn(str(linked), result.stdout)

    def test_a_territory_base_another_principal_could_write_refuses(self):
        # The operator's ruling of 2026-10-02 (#28): the territories stand under
        # a root-held base no other principal can make a name in. A base that
        # does not stand, or one a group may write, refuses before anything is
        # asked or made. Perturbation: drop the creatable_in judgment of the
        # base, and the open base plans on.
        open_base = self.root / "open-agents"
        open_base.mkdir()
        open_base.chmod(0o775)
        for base, said in ((self.root / "absent-agents", "does not stand"),
                           (open_base, "no other principal can make a name in")):
            (self.stack / "agent-directory").write_text(f"{base}\n")
            result = self.create()
            self.assertNotEqual(result.returncode, 0, base)
            self.assertIn(said, result.stderr)
            self.assert_unprivileged()

    def test_unreadable_stack_key_refuses(self):
        # A directory where a key file should be: cat fails, as an unreadable
        # file does, and chmod alone is ineffective under root test runners.
        (self.stack / "coordination-root").unlink()
        (self.stack / "coordination-root").mkdir()
        result = self.create()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("cannot read", result.stderr)
        self.assert_unprivileged()

    def test_failed_account_lookup_is_not_absence(self):
        self.env["ACCOUNT_FAIL"] = "1"
        for apply in (False, True):
            with self.subTest(apply=apply):
                self.env["ALLOW_APPLY_CHECKS"] = "1"
                result = self.create(*(["--apply"] if apply else []))
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("cannot read account", result.stderr)
                self.assert_no_provisioning()

    def test_failed_privileged_path_inspection_is_not_absence(self):
        self.env.update(ALLOW_APPLY_CHECKS="1", PATH_FAIL=str(self.config / "m1"))
        result = self.create("--apply")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("cannot inspect", result.stderr)
        self.assert_no_provisioning()

    def assert_agent_root(self, engine, spu=None):
        root = self.config / "m1"
        self.assertTrue(root.is_dir())
        self.assertFalse((self.config / ".m1.partial").exists())
        for key, value in self.stack_keys.items():
            if key in ("agent-directory", "prefix"):
                self.assertFalse((root / key).exists(), key)
            elif key == "spu-binary" and spu:
                self.assertEqual((root / key).read_text(), spu + "\n")
            else:
                self.assertEqual((root / key).read_text(), (self.stack / key).read_text(), key)
        # The agent's own keys (weaver-admin-Spec section 9), the territory
        # among them since the operator's ruling of 2026-10-07 on #1.
        self.assertEqual((root / "territory").read_text(), str(self.decl) + "\n")
        self.assertEqual((root / "operator").read_text(), "12345\n")
        self.assertEqual((root / "roles.toml").read_text(), 'trace-reader = "weaver-m1-admincon"\n')
        for retired in ("allow-list", "agent-config-directory", "spu-implementations", "agent-spu",
                        "agent.toml", "log-path", "run-tool", "control-tool", "unit-properties",
                        "declaration-directory"):
            self.assertFalse((root / retired).exists(), retired)
            self.assertFalse((self.config / retired).exists(), retired)
        # The declaration and the draft are root:weaver-m1-admin 0640 in the
        # territory, read by the access group alone, and nothing landed in
        # the operator's home. Perturbation: write them 0644 and the chmod
        # is not found.
        calls = self.calls()
        decl, draft = str(self.decl / "agent.toml"), str(self.decl / "system-prompt.md")
        self.assertIn(["sudo", "chown", "root:weaver-m1-admin", decl, draft], calls)
        self.assertIn(["sudo", "chmod", "0640", decl, draft], calls)
        self.assertEqual(sorted(p.name for p in self.operator_home.iterdir()), [], "nothing in the operator's home")
        import tomllib
        declaration = tomllib.loads((self.decl / "agent.toml").read_text())
        # The declaration carries no identity in either of its earlier forms
        # (weaver-types-Spec section 2, the ruling of 2026-10-06 that the
        # system prompt is state), and the draft the seeding turn reads stands
        # beside it, the operator's alone. Perturbation: write either key
        # again and the parse of a made agent refuses it by name.
        decoder = declaration["spu-instruction"]["decoder"]
        self.assertNotIn("identity", decoder)
        self.assertNotIn("identity-file", decoder)
        prompt = self.decl / "system-prompt.md"
        self.assertTrue(prompt.read_text().startswith("You are a careful assistant."))
        self.assertTrue(prompt.read_text().endswith("question allows.\n"))
        store = declaration["state-store"]
        self.assertEqual(store["engine"], engine)
        return store

    def assert_rule(self, verbs):
        admin = str((self.root / "installed" / "bin" / "weaver-admin").resolve())
        lines = ", ".join(f"{admin} {verb} m1" for verb in verbs)
        text = self.rule.read_text()
        self.assertIn("Defaults:weaver-m1-admincon !pam_session\n", text)
        self.assertIn(f"weaver-m1-admincon ALL=(root) NOPASSWD: {lines}\n", text)
        self.assertNotIn("SETENV", text)
        self.assertNotIn("env_keep", text)

    def test_the_embedded_engine_is_the_default_and_others_are_unprovided(self):
        # sqlite is the one engine this build provides (#1, #86), so an absent
        # --engine is sqlite and any other name refuses as unprovided before
        # any call, postgres one case among them. Perturbations: default to
        # nothing again, and the absent case refuses; accept any engine, and
        # the plan runs.
        result = self.create_naming()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("store engine    sqlite", result.stdout)
        for engine in ("postgres", "mysql"):
            result = self.create_naming("--engine", engine, "--apply")
            self.assertNotEqual(result.returncode, 0)
            self.assertIn(f"{engine} is not an engine this build provides",
                          result.stderr)
        self.assert_unprivileged()

    def test_apply_fixture_reaches_the_end_with_sqlite(self):
        # Perturbation: let the sqlite path fall into the
        # store half and psql or systemctl appear in the calls.
        self.env["ALLOW_APPLY_CHECKS"] = "1"
        result = self.create("--apply")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("== made", result.stdout)
        store = self.assert_agent_root("sqlite")
        self.assertNotIn("database", store)
        self.assertNotIn("role", store)
        self.assertEqual(self.hba.read_text(), "local all all peer\n")
        calls = self.calls()
        self.assertFalse(any("psql" in c or "systemctl" in c for c in calls), calls)
        self.assertTrue(any(c[:4] == ["sudo", "-u", "weaver-m1", "test"] for c in calls))
        self.assert_rule(["show", "validate", "load", "unload", "stop", "save-point", "restore", "force-unload"])
        self.assertIn("validate it before loading, with the validate verb of", result.stdout)
        # The seeding is named as the operator's next step and never done
        # here: create-agent makes an agent that is not loaded, and the
        # seeding is a turn against a loaded one. Perturbation: dial the gate
        # from the script and the second assertion fails.
        self.assertIn("deploy/turn.py m1 --system reads " + str(self.decl / "system-prompt.md"),
                      result.stdout)
        script = (self.repo / "deploy" / "create-agent.sh").read_text()
        dialing = [line for line in script.splitlines()
                   if not line.lstrip().startswith(("#", "printf", "plan "))
                   and any(needle in line for needle in ("turn.py", "gate.sock", "socat", "/dev/tcp", "python3 -c 'import socket"))]
        self.assertEqual(dialing, [], "create-agent.sh never opens the gate")
        self.assertFalse(any("turn.py" in " ".join(c) or "gate.sock" in " ".join(c) for c in calls), calls)
        self.assert_no_printed_root_command(result)

    def test_the_connector_role_chooses_the_rules_lines(self):
        # weaver-admin-Spec section 2: the observer's rule grants `show`, the
        # operator's adds the four that change the agent, each a fixed line.
        # Perturbation: ignore --connector-role and the observer gets all five.
        self.env["ALLOW_APPLY_CHECKS"] = "1"
        result = self.create("--connector-role", "observer", "--apply")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assert_rule(["show"])
        self.assertNotIn(" load m1", self.rule.read_text())
        result = self.create("--connector-role", "root")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("operator or observer", result.stderr)

    def test_a_rule_visudo_refuses_is_never_installed_and_admits_nothing(self):
        # The rule is checked before it is placed, and a refusal leaves no rule
        # and no admitted root. Perturbation: install before the check, and the
        # refused rule stands.
        self.env.update(ALLOW_APPLY_CHECKS="1", VISUDO_FAIL="1")
        result = self.create("--apply")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("visudo refuses the rendered rule", result.stderr)
        self.assertFalse(self.rule.exists())
        self.assertFalse(list((self.root / "etc" / "sudoers.d").iterdir()))
        self.assertFalse((self.config / "m1").exists())

    def test_a_sudoers_without_the_include_refuses_before_admission(self):
        # A rule in sudoers.d that sudo never reads would leave the connector
        # with no line at all. Perturbation: drop the include check, and the
        # agent is admitted with a dead rule.
        (self.root / "etc" / "sudoers").write_text("root ALL=(ALL) ALL\n")
        self.env["ALLOW_APPLY_CHECKS"] = "1"
        result = self.create("--apply")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("does not include /etc/sudoers.d", result.stderr)
        self.assertFalse((self.config / "m1").exists())

    def test_the_relay_and_connector_groups_are_checked_before_admission(self):
        # The relay holds the trace group alone, and the connector the state
        # group, the access group and nothing of the agent's. Perturbations:
        # drop any check, and a mis-grouped account is admitted.
        for env, said in ((dict(RELAY_GROUPS="weaver-m1-trace weaver-m1-state"), "not the trace group alone"),
                          (dict(CONNECTOR_GROUPS="weaver-m1-admincon weaver-m1-state"), "does not hold weaver-m1-admin"),
                          (dict(CONNECTOR_GROUPS="weaver-m1-admincon weaver-m1-admin"), "does not hold weaver-m1-state"),
                          (dict(CONNECTOR_GROUPS="weaver-m1-admincon weaver-m1-state weaver-m1-admin weaver-m1-trace"),
                           "holds weaver-m1-trace")):
            with self.subTest(env=env):
                shutil.rmtree(self.config / ".m1.partial", ignore_errors=True)
                shutil.rmtree(self.root / "agents" / "weaver-m1", ignore_errors=True)
                shutil.rmtree(self.decl, ignore_errors=True)
                for name in ("RELAY_GROUPS", "CONNECTOR_GROUPS"): self.env.pop(name, None)
                self.env.update(ALLOW_APPLY_CHECKS="1", **env)
                result = self.create("--apply")
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(said, result.stderr)
                self.assertFalse((self.config / "m1").exists())
                self.assertFalse(self.rule.exists())

    def test_the_declaration_directory_flag_is_retired(self):
        # The operator's ruling of 2026-10-07 on #1: the declaration lives in
        # the territory and nothing is written into the operator's home, so
        # the flag refuses naming the ruling, before any call. Perturbation:
        # take the flag again and the plan runs.
        result = self.create("--declaration-directory", "/elsewhere")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("retired on the operator's ruling of 2026-10-07", result.stderr)
        self.assertEqual(self.calls(), [])

    def test_the_territory_holds_the_declaration_the_draft_and_the_save_points(self):
        # The operator's rulings of 2026-10-07 and 2026-10-08 on #1: the
        # territory is root:weaver-<name>-state 0710, the declaration and the
        # draft root:weaver-<name>-admin 0640 in it, save-points/ root:weaver-<name>-admin 0750
        # beside the state room, the operator in the state and access groups
        # and the member in the access group never (the custody audit's G11), and the root names the
        # territory. Perturbations: write the draft into the operator's home
        # again, group the territory to the member, join the member to the
        # access group again, or drop the save-points directory, and this
        # fails.
        self.env["ALLOW_APPLY_CHECKS"] = "1"
        result = self.create("--apply")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assert_agent_root("sqlite")
        calls = self.calls()
        territory = str(self.decl)
        self.assertIn(["sudo", "install", "-d", "-o", "root", "-g", "weaver-m1-state", "-m", "00710", territory], calls)
        self.assertIn(["sudo", "install", "-d", "-o", "root", "-g", "weaver-m1-admin", "-m", "00750",
                       territory + "/save-points"], calls)
        self.assertIn(["sudo", "tee", territory + "/agent.toml"], calls)
        self.assertIn(["sudo", "tee", territory + "/system-prompt.md"], calls)
        self.assertNotIn(["sudo", "usermod", "-aG", "weaver-m1-admin", "weaver-m1-state"], calls)
        self.assertIn(["sudo", "usermod", "-aG", "weaver-m1,weaver-m1-trace,weaver-m1-state,weaver-m1-admin", "fixture-no-home"], calls)
        self.assertFalse([c for c in calls if any(a.startswith(str(self.operator_home)) for a in c)],
                         "no call reaches the operator's home")
        self.assertNotIn("admin.log", "".join(a for c in calls for a in c), "admin makes the logs, not the script")
        self.assertIn("sudoedit", result.stdout)

    def test_the_access_group_reads_and_never_writes(self):
        # The connector, which holds the access group, reads the declaration
        # and can write neither it, the draft nor the save-points directory;
        # a group that could write refuses before the admission. Perturbation:
        # drop the write probes, and an open group is admitted.
        self.env["ALLOW_APPLY_CHECKS"] = "1"
        result = self.create("--apply")
        self.assertEqual(result.returncode, 0, result.stderr)
        calls = self.calls()
        territory = str(self.decl)
        self.assertIn(["sudo", "-u", "weaver-m1-admincon", "test", "-r", territory + "/agent.toml"], calls)
        for f in ("agent.toml", "system-prompt.md", "save-points"):
            self.assertIn(["sudo", "-u", "weaver-m1-admincon", "test", "-w", f"{territory}/{f}"], calls)
        # Every mode the script gives the territory's files grants the group no write.
        for c in calls:
            if c[:2] == ["sudo", "install"] and "-m" in c and c[-1].startswith(territory):
                self.assertEqual(int(c[c.index("-m") + 1], 8) & 0o022, 0, c)
            if c[:2] == ["sudo", "chmod"] and any(a.startswith(territory) for a in c):
                self.assertEqual(int(c[2], 8) & 0o022, 0, c)
        self.log.unlink(missing_ok=True)
        shutil.rmtree(self.config / "m1", ignore_errors=True)
        shutil.rmtree(self.config / ".m1.partial", ignore_errors=True)
        shutil.rmtree(self.decl, ignore_errors=True)
        self.rule.unlink(missing_ok=True)
        self.env["GROUP_WRITES"] = "1"
        result = self.create("--apply")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("CAN WRITE", result.stderr)
        self.assertFalse((self.config / "m1").exists())

    def test_sqlite_wall_open_refuses_before_admission(self):
        self.env.update(ALLOW_APPLY_CHECKS="1", WALL_OPEN="1")
        result = self.create("--apply")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("CAN ENTER THE STATE ROOM", result.stderr)
        self.assertFalse((self.config / "m1").exists())
        self.assertTrue((self.config / ".m1.partial").is_dir())

    def test_spu_override_is_this_agents_spu_binary(self):
        spu = "/opt/elsewhere/python-spu.pyz"
        result = self.create("--spu", spu)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("spu-binary      " + spu, result.stdout)
        self.env["ALLOW_APPLY_CHECKS"] = "1"
        result = self.create("--spu", spu, "--apply")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assert_agent_root("sqlite", spu=spu)
        self.assertEqual((self.stack / "spu-binary").read_text(), self.stack_keys["spu-binary"] + "\n")

    def test_an_spu_named_like_another_stack_binary_refuses_before_provisioning(self):
        # Codex on #45: admin's stack::judge_names refuses every verb on a root
        # whose worker, state member, gate and SPU share a file name, so an
        # --spu named like one of them refuses here, before any call. Perturbation:
        # drop the judge_names call, and each provisions.
        self.env["ALLOW_APPLY_CHECKS"] = "1"
        for name in ("pyworker", "weaver-state", "weaver-gate"):
            result = self.create("--spu", "/opt/elsewhere/" + name, "--apply")
            self.assertNotEqual(result.returncode, 0, name)
            self.assertIn("share the file name " + name, result.stderr)
            # Only the operator's own identity was looked up.
            self.assertEqual([c for c in self.calls() if c[0] not in ("id", "getent")], [], name)
            self.assertFalse((self.config / ".m1.partial").exists(), name)

    def held_closed(self, path):
        """Each deploy script's `held_closed`, run on `path` under the fixture's
        `stat`. Answers each script's exit status and what it printed."""
        answers = {}
        for script in ("verify-load.sh", "create-agent.sh", "bootstrap-stack.sh", "update-stack.sh",
                       "verify-lifecycle.sh"):
            text = (self.repo / "deploy" / script).read_text()
            start = text.index("held_closed() {")
            body = text[start:text.index("\n}\n", start) + 3]
            ran = subprocess.run(["bash", "-c", body + 'held_closed "$1"', "bash", str(path)],
                                 env=self.env, text=True, capture_output=True, timeout=20)
            answers[script] = (ran.returncode, ran.stdout)
        return answers

    def test_every_deploy_script_holds_admins_ancestor_rule(self):
        # weaver-admin-Spec section 9: a path and every directory above it held
        # by root and writable by no group or other, a sticky directory
        # excepted. Every script's copy is the same text and answers alike.
        # Perturbation: drop the mode test from one copy, and its answer differs.
        texts = set()
        for script in ("verify-load.sh", "create-agent.sh", "bootstrap-stack.sh", "update-stack.sh",
                       "decommission.sh", "verify-lifecycle.sh"):
            text = (self.repo / "deploy" / script).read_text()
            start = text.index("held_closed() {")
            texts.add(text[start:text.index("\n}\n", start)])
        self.assertEqual(len(texts), 1, "every copy of held_closed is one text")
        parent = self.root / "parent"
        leaf = parent / "leaf"
        leaf.mkdir(parents=True)
        for mode, held in ((0o755, True), (0o1777, True), (0o775, False), (0o757, False), (0o777, False)):
            parent.chmod(mode)
            for script, (code, said) in self.held_closed(leaf).items():
                self.assertEqual(code == 0, held, (script, oct(mode), said))
                if not held:
                    self.assertEqual(said, str(parent), script)
        parent.chmod(0o755)
        for script, (code, said) in self.held_closed(self.root / "absent").items():
            self.assertNotEqual(code, 0, script)

    def test_create_agent_refuses_a_base_admin_would_refuse(self):
        # Codex on #45, round 10: a base under a directory another principal
        # may write passed create-agent and was provisioned whole, then admin's
        # judge_ancestors refused every verb. It refuses before any call.
        # Perturbation: drop the held_closed judgment of the base, and it provisions.
        self.env["ALLOW_APPLY_CHECKS"] = "1"
        self.config.chmod(0o777)
        try:
            result = self.create("--apply")
        finally:
            self.config.chmod(0o755)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("is not held closed by root at " + str(self.config), result.stderr)
        # Only the credential check and the read-only path looks ran.
        made = [c for c in self.calls()
                if c[0] in ("psql", "systemctl", "mktemp", "setfacl")
                or (c[0] == "sudo" and c[1:2] != ["-v"] and c[1:3] != ["-n", "sh"])]
        self.assertEqual(made, [], self.calls())
        self.assertFalse((self.config / ".m1.partial").exists())

    def test_create_agent_refuses_a_base_whose_inheritance_admin_would_refuse(self):
        """**A setgid base, or one carrying a default access entry, refuses
        before anything is made** (#99 area 2 review, K4): a territory made in
        it would inherit the bit or the entry, which admin refuses at every
        verb. Perturbations: drop the setgid judgment, or the default-entry
        look, and the agent is provisioned."""
        base = self.root / "agents"
        setfacl = shutil.which("setfacl", path="/usr/bin:/bin")
        cases = [("setgid", lambda: base.chmod(0o2755), lambda: base.chmod(0o755), "is setgid (mode 2755)")]
        if setfacl:
            cases.append(("default entry",
                          lambda: subprocess.run([setfacl, "-d", "-m", f"u:{os.getuid()}:rx", str(base)], check=True),
                          lambda: subprocess.run([setfacl, "-b", str(base)], check=True),
                          "carries a default access entry"))
        for label, make, undo, said in cases:
            with self.subTest(label):
                make()
                if label == "setgid" and base.stat().st_mode & 0o7777 != 0o2755:
                    undo()
                    continue
                self.log.unlink(missing_ok=True)
                self.env["ALLOW_APPLY_CHECKS"] = "1"
                try:
                    result = self.create("--apply")
                finally:
                    undo()
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(said, result.stderr)
                made = [c for c in self.calls()
                        if c[0] in ("psql", "systemctl", "mktemp", "setfacl")
                        or (c[0] == "sudo" and c[1:2] != ["-v"] and c[1:3] != ["-n", "sh"])]
                self.assertEqual(made, [], self.calls())

    def test_verify_load_runs_only_the_records_admin_held_closed(self):
        # Codex on #45, round 10: verify-load ran `$(dirname worker-binary)/
        # weaver-admin`, a key of a root admin had not judged, as root. It
        # takes admin from the record's prefix, judged closed, and never from
        # the root. Perturbations: restore the root-derived path, and the
        # planted admin runs; drop the binary's judgment, and the open
        # install's admin is accepted.
        self.env["FIXTURE_UID"] = "0"
        root = self.config / "m1"
        root.mkdir()
        (root / "agent.toml").write_text('[trace-sink]\npath = "/x"\n')
        planted = self.root / "planted"
        planted.mkdir()
        marker = self.root / "planted-ran"
        (planted / "weaver-admin").write_text(f"#!/bin/sh\ntouch {marker}\n")
        (planted / "weaver-admin").chmod(0o755)
        (root / "worker-binary").write_text(str(planted / "worker") + "\n")
        installed = self.root / "installed" / "bin"
        installed.mkdir(parents=True)
        result = self.run_script("verify-load.sh", "m1")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("no weaver-admin at " + str(installed / "weaver-admin"), result.stderr)
        self.assertFalse(marker.exists(), "the root's worker directory chose the admin run")
        (installed / "weaver-admin").write_text("#!/bin/sh\nexit 7\n")
        (installed / "weaver-admin").chmod(0o755)
        installed.chmod(0o777)
        try:
            result = self.run_script("verify-load.sh", "m1")
        finally:
            installed.chmod(0o755)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("is not held closed by root: " + str(installed), result.stderr)

    def verify_fixture(self):
        """An admitted agent whose admin is a stub: `load` starts a real
        process standing in for the run's constituents and writes a load
        event, `show` names it, `unload` ends it unless KEEP_ALIVE is set."""
        self.env.update(FIXTURE_UID="0", FIXTURE_ACCOUNT_UID=str(os.getuid()))
        root = self.config / "m1"
        root.mkdir()
        self.decl.mkdir(parents=True)
        sink = self.root / "agents" / "trace.ndjson"
        sink.write_text("")
        (self.decl / "agent.toml").write_text(f'[trace-sink]\nkind = "file"\npath = "{sink}"\n')
        (root / "territory").write_text(str(self.decl) + "\n")
        state = self.root / "stub-state"
        state.mkdir()
        installed = self.root / "installed" / "bin"
        installed.mkdir(parents=True)
        stub = installed / "weaver-admin"
        stub.write_text(f"""#!/bin/sh
case "$1" in
  validate) echo '{{"kind":"validated"}}' ;;
  load) sleep 300 >/dev/null 2>&1 &
        echo $! > {state}/pid
        echo '{{"kind":"load"}}' >> {sink}
        echo '{{"kind":"state","state":"idle"}}' ;;
  show) if [ -f {state}/pid ]; then
          echo "{{\\"kind\\":\\"state\\",\\"state\\":\\"idle\\",\\"constituents\\":[$(cat {state}/pid)]}}"
        else echo '{{"kind":"state","state":"unloaded"}}'; fi ;;
  unload) if [ -f {state}/pid ]; then
            [ -n "$KEEP_ALIVE" ] || kill $(cat {state}/pid)
            cp {state}/pid {state}/last; rm {state}/pid
          fi
          echo '{{"kind":"state","state":"unloaded"}}' ;;
esac
""")
        stub.chmod(0o755)
        return state

    def end_stand_in(self, state):
        for name in ("pid", "last"):
            path = state / name
            if path.exists():
                try:
                    os.kill(int(path.read_text()), 9)
                except (ProcessLookupError, ValueError):
                    pass

    def test_verify_load_checks_each_constituent_and_that_none_is_left(self):
        # The operator contract section 2: every constituent `show` names runs
        # as one of the agent's accounts and sits in the invoker's cgroup, a
        # relay among them for a file sink, and after the unload none is left.
        state = self.verify_fixture()
        try:
            result = self.run_script("verify-load.sh", "m1")
        finally:
            self.end_stand_in(state)
        self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
        self.assertIn("in the invoker's cgroup", result.stdout)
        self.assertIn("every constituent gone, show unloaded", result.stdout)

    def test_verify_load_reads_its_unload_and_takes_only_a_load_event(self):
        """**verify-load reads its first unload's answer, and only `load` is a
        load event** (#99 area 2 review, K8): a refused unload, the run still
        standing, refuses before the load, naming the answer; a load that
        writes only an `unload` event refuses as one that wrote no load
        event; a refused load refuses naming its answer and shows the state
        member's log, read as the member (N5's class, K12). Perturbations:
        ignore the unload's answer again and the run is verified; match
        "load" as a substring again and the `unload` event passes; drop the
        load's answer check and the refused load is verified."""
        stub = self.root / "installed" / "bin" / "weaver-admin"
        for case in ("refused unload", "unload event", "refused load"):
            with self.subTest(case):
                shutil.rmtree(self.config / "m1", ignore_errors=True)
                shutil.rmtree(self.decl, ignore_errors=True)
                shutil.rmtree(self.root / "stub-state", ignore_errors=True)
                shutil.rmtree(self.root / "installed", ignore_errors=True)
                state = self.verify_fixture()
                text = stub.read_text()
                if case == "refused unload":
                    text = text.replace(
                        """          echo '{"kind":"state","state":"unloaded"}' ;;""",
                        """          echo '{"kind":"refused","reason":"activity_not_at_rest"}' ;;""")
                elif case == "unload event":
                    text = text.replace("""echo '{"kind":"load"}' >>""", """echo '{"kind":"unload"}' >>""")
                else:
                    text = text.replace("""        echo '{"kind":"state","state":"idle"}' ;;""",
                                        """        echo '{"kind":"refused","reason":"descriptors_unusable"}' ;;""")
                    # The member's log, which admin makes root:<member's group>
                    # 0640 for the member to read (K12), read as the member.
                    room = self.root / "agents" / "state"
                    room.mkdir(exist_ok=True)
                    self.env["ALLOW_APPLY_CHECKS"] = "1"
                    (room / "state.log").write_text("no engine named \"x\" in this binary\n")
                stub.write_text(text)
                try:
                    result = self.run_script("verify-load.sh", "m1")
                finally:
                    self.end_stand_in(state)
                self.assertNotEqual(result.returncode, 0, result.stdout)
                if case == "refused unload":
                    self.assertIn('the unload before the load answered {"kind":"refused"', result.stderr)
                    self.assertNotIn("load        ", result.stdout)
                elif case == "unload event":
                    self.assertIn("the new events carry no load event", result.stderr)
                else:
                    self.assertIn("the load answered {\"kind\":\"refused\"", result.stderr)
                    self.assertIn("the state member last said:", result.stdout)
                    self.assertIn('no engine named "x" in this binary', result.stdout)

    def test_verify_load_counts_no_sink_that_is_not_a_file(self):
        """**A pipe sink is never opened for a count** (#99 area 2 review,
        K6): root opening a FIFO with no writer blocks for ever, so a non-file
        sink skips the line checks, says so, and is verified by the load's
        answer and its constituents. Perturbation: count whatever stands at
        the path again and the run hangs past the harness's timeout."""
        state = self.verify_fixture()
        fifo = self.root / "agents" / "trace.pipe"
        os.mkfifo(fifo)
        (self.decl / "agent.toml").write_text(f'[trace-sink]\nkind = "pipe"\npath = "{fifo}"\n')
        try:
            result = self.run_script("verify-load.sh", "m1")
        finally:
            self.end_stand_in(state)
        self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
        self.assertIn("not counted: a pipe is not a file a line count reads", result.stdout)

    def test_verify_load_refuses_a_constituent_of_another_account(self):
        # Perturbation: drop the account check, and a stranger's process passes.
        state = self.verify_fixture()
        self.env["FIXTURE_ACCOUNT_UID"] = str(os.getuid() + 1)
        try:
            result = self.run_script("verify-load.sh", "m1")
        finally:
            self.end_stand_in(state)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("none of the agent's, its member's or its relay's accounts", result.stderr)

    def test_a_failed_keep_verification_unloads_the_run(self):
        # Codex on #79: --keep keeps only a run every check passed. A stranger's
        # constituent under --keep still unloads. Perturbation: skip the unload
        # under --keep, and the run is left serving.
        state = self.verify_fixture()
        self.env["FIXTURE_ACCOUNT_UID"] = str(os.getuid() + 1)
        try:
            result = self.run_script("verify-load.sh", "m1", "--keep")
        finally:
            self.end_stand_in(state)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("so the run is unloaded", result.stderr)
        self.assertTrue((state / "last").exists(), "the stub admin was asked to unload")
        self.assertFalse((state / "pid").exists(), "and no run is left")

    def test_verify_load_refuses_a_constituent_that_outlives_the_unload(self):
        # Perturbation: drop the after-unload look, and the survivor passes.
        state = self.verify_fixture()
        self.env["KEEP_ALIVE"] = "1"
        try:
            result = self.run_script("verify-load.sh", "m1")
        finally:
            self.end_stand_in(state)
            self.env.pop("KEEP_ALIVE", None)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("outlived the unload", result.stderr)

    def test_the_territory_is_passage_for_the_member_and_the_trace_is_not_its_to_read(self):
        # The operator's ruling of 2026-10-02 (#28) as refined on #56 and by the
        # rulings of 2026-10-07 and 2026-10-08 on #1: the member passes through
        # a root:weaver-<name>-state 0710 territory (no setgid, no listing) to
        # its 0700 room by its own primary group, and the trace is made before
        # the first load as root:weaver-<name>-trace 0640, so the member, outside
        # that group, cannot read it. The operator joins the agent's group, the
        # trace group, the state group and the access group, and no access entry is set or
        # probed. Perturbations: restore setgid (2710 or 2750), group the trace
        # to the member, or drop the trace group from the operator, and this
        # fails.
        self.env["ALLOW_APPLY_CHECKS"] = "1"
        result = self.create("--apply")
        self.assertEqual(result.returncode, 0, result.stderr)
        calls = self.calls()
        territory = str(self.root / "agents" / "weaver-m1")
        self.assertIn(["sudo", "install", "-d", "-o", "root", "-g", "weaver-m1-state", "-m", "00710", territory], calls)
        self.assertIn(["sudo", "install", "-o", "root", "-g", "weaver-m1-trace", "-m", "0640", "/dev/null",
                       territory + "/trace.ndjson"], calls)
        self.assertIn(["sudo", "install", "-d", "-o", "weaver-m1-state", "-g", "weaver-m1-state", "-m", "0700",
                       territory + "/state"], calls)
        self.assertIn(["sudo", "groupadd", "--system", "weaver-m1-trace"], calls)
        self.assertIn(["sudo", "groupadd", "--system", "weaver-m1-admin"], calls)
        self.assertIn(["sudo", "useradd", "--system", "--shell", "/usr/sbin/nologin", "--no-create-home",
                       "--no-user-group", "--gid", "weaver-m1-trace", "weaver-m1-relay"], calls)
        self.assertIn(["sudo", "useradd", "--system", "--shell", "/usr/sbin/nologin", "--no-create-home",
                       "--user-group", "--groups", "weaver-m1-state,weaver-m1-admin", "weaver-m1-admincon"], calls)
        self.assertIn(["sudo", "usermod", "-aG", "weaver-m1,weaver-m1-trace,weaver-m1-state,weaver-m1-admin", "fixture-no-home"], calls)
        self.assertIn(["sudo", "-u", "weaver-m1-state", "test", "-r", territory + "/trace.ndjson"], calls)
        # The agent's own uid is asked to pass the territory and must not
        # (the #94 survey's S1), and the member to read the declaration.
        self.assertIn(["sudo", "-u", "weaver-m1", "test", "-x", territory], calls)
        self.assertIn(["sudo", "-u", "weaver-m1-state", "test", "-r", territory + "/agent.toml"], calls)
        self.assertFalse([c for c in calls if "setfacl" in c or c[0] == "mktemp"], calls)

    def test_a_trace_the_member_can_read_refuses_before_admission(self):
        # The probe is what holds the boundary on the box: a member that can read
        # the trace refuses before the root is moved into place, for either
        # engine. Perturbation: drop the probe, and the agent is admitted.
        for engine in ("sqlite",):
            with self.subTest(engine=engine):
                self.log.unlink(missing_ok=True)
                shutil.rmtree(self.config / ".m1.partial", ignore_errors=True)
                shutil.rmtree(self.root / "agents" / "weaver-m1", ignore_errors=True)
                shutil.rmtree(self.decl, ignore_errors=True)
                self.env.update(ALLOW_APPLY_CHECKS="1", TRACE_OPEN="1")
                result = self.create("--engine", engine, "--apply")
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("CAN READ THE TRACE", result.stderr)
                self.assertFalse((self.config / "m1").exists())

    def test_verify_load_reads_no_root_admin_does_not_validate(self):
        # Codex on #45, round 11: verify-load parsed the declaration, and counted
        # the sink it names, as root before admin had judged the root. It asks
        # admin's `validate` first and reads nothing from the root until it
        # answers validated. Perturbation: drop the gate, and the declaration
        # is read (the planted sink's marker is touched by the count).
        self.env["FIXTURE_UID"] = "0"
        root = self.config / "m1"
        root.mkdir()
        sink = self.root / "planted-sink"
        (root / "agent.toml").write_text(f'[trace-sink]\npath = "{sink}"\n')
        installed = self.root / "installed" / "bin"
        installed.mkdir(parents=True)
        (installed / "weaver-admin").write_text(
            '#!/bin/sh\necho \'{"kind":"refused","refusal":"boundary_unverified"}\'\n')
        (installed / "weaver-admin").chmod(0o755)
        result = self.run_script("verify-load.sh", "m1")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("admin does not validate m1", result.stderr)
        self.assertNotIn(str(sink), result.stdout + result.stderr, "the declaration was read")

    def test_a_stack_record_another_principal_may_write_refuses(self):
        # The walk of #45 round 11: create-agent and update-stack act as root on
        # paths the record names, so the record and each entry are held closed
        # first. A group-writable entry refuses both, naming it, before any
        # privileged step. Perturbation: drop the entry loop, and both go on.
        entry = self.stack / "coordination-root"
        entry.chmod(0o664)
        try:
            for result in (self.create(), self.run_script("update-stack.sh")):
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("the stack record's " + str(entry) + " is not held closed", result.stderr)
        finally:
            entry.chmod(0o644)
        self.assert_unprivileged()

    def test_the_members_state_log_is_read_as_the_member(self):
        # The walk of #45 round 11: the state log sits in the member's own
        # territory, where the member can put a link, so root never reads it;
        # both scripts read it as the member. Perturbation: restore a root read
        # in either, and its text no longer holds.
        for script in ("verify-load.sh", "update-stack.sh"):
            text = (self.repo / "deploy" / script).read_text()
            reads = [l for l in text.splitlines()
                     if "tail" in l and ("state.log" in l or '"$st"' in l)]
            self.assertTrue(reads, script)
            for line in reads:
                self.assertIn('sudo -n -u "weaver-$AGENT-state" tail', line, (script, line))

    def test_update_stack_writes_no_declaration(self):
        # The declaration is root's in root's territory (the operator's ruling
        # of 2026-10-07 on #1), and no step of update-stack writes it: the
        # reconcile patch that declared `engine = "none"` could never run
        # (#99 area 2 review, K11), and the identity move is gone with every
        # migration (the operator's ruling of 2026-10-08 on #1).
        # Perturbation: put back a copy or a write of the declaration as root
        # or as the operator, and this fails.
        import re as _re
        text = (self.repo / "deploy" / "update-stack.sh").read_text()
        self.assertFalse([l for l in text.splitlines() if _re.search(r"\bsudo\s.*\$decl\b", l)])
        self.assertNotIn('>> "$decl"', text)
        self.assertNotIn('cat "$decl" |', text)
        self.assertNotIn("STATE_BINARY", text)
        self.assertFalse((self.repo / "deploy" / "migrate-identity.py").exists())

    def test_verify_load_execs_admin_by_its_judged_canonical_path(self):
        # The walk of #45 round 11: admin was judged by its resolved path and
        # run by its written one, so a link on the written path could be
        # re-pointed between the two. It runs the canonical path it judged.
        # Perturbation: run `$PREFIX/bin/weaver-admin` as written, and $0 is the link.
        self.env["FIXTURE_UID"] = "0"
        root = self.config / "m1"
        root.mkdir()
        (root / "agent.toml").write_text('[trace-sink]\npath = "/x"\n')
        real = self.root / "real-admin"
        real.mkdir()
        (real / "weaver-admin").write_text('#!/bin/sh\necho "$0"\n')
        (real / "weaver-admin").chmod(0o755)
        installed = self.root / "installed" / "bin"
        installed.mkdir(parents=True)
        (installed / "weaver-admin").symlink_to(real / "weaver-admin")
        result = self.run_script("verify-load.sh", "m1")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("admin does not validate m1, so its root is not read: " + str(real / "weaver-admin"),
                      result.stderr)

    def test_turn_system_reads_the_draft_and_measures_it_against_the_gates_bound(self):
        # --system reads the draft from the territory the root's `territory`
        # key names and sends it as the system role; a draft whose line would
        # pass the gate's 32 KiB bound refuses before dialing, naming the size
        # (Codex on #92, round 3), and an empty draft refuses too. A draft
        # that fits reaches the dial, which finds no gate. Perturbation: drop
        # the measure and the oversize draft reaches the dial.
        root = self.config / "m1"
        root.mkdir()
        (root / "territory").write_text(str(self.decl) + "\n")
        (root / "coordination-root").write_text(str(self.root / "nowhere") + "\n")
        self.decl.mkdir(parents=True)
        draft = self.decl / "system-prompt.md"
        turn = [sys.executable, str(self.repo / "deploy" / "turn.py"), "m1", "--system"]
        draft.write_text("x" * (32 * 1024))
        result = subprocess.run(turn, env=self.env, text=True, capture_output=True, timeout=20)
        self.assertEqual(result.returncode, 1)
        self.assertIn("past the gate's bound of 32768 octets", result.stderr)
        self.assertNotIn("no gate at", result.stderr)
        # The empty draft refuses by the identity door's own rule, and a
        # draft of whitespace alone is a prompt the door admits, so it reaches
        # the dial (Codex on #92, round 7). Perturbation: strip before the
        # check and the blank draft refuses as empty.
        draft.write_text("")
        result = subprocess.run(turn, env=self.env, text=True, capture_output=True, timeout=20)
        self.assertEqual(result.returncode, 1)
        self.assertIn("is empty", result.stderr)
        self.assertNotIn("no gate at", result.stderr)
        draft.write_text("   \n")
        result = subprocess.run(turn, env=self.env, text=True, capture_output=True, timeout=20)
        self.assertEqual(result.returncode, 1)
        self.assertIn("no gate at", result.stderr)
        draft.write_text("You are Karl.\n")
        result = subprocess.run(turn, env=self.env, text=True, capture_output=True, timeout=20)
        self.assertEqual(result.returncode, 1)
        self.assertIn("no gate at", result.stderr)
        result = subprocess.run(turn + ["extra text"], env=self.env, text=True, capture_output=True, timeout=20)
        self.assertEqual(result.returncode, 2)
        self.assertIn("takes no text", result.stderr)
        # A draft that is not UTF-8 refuses before the dial, naming the file
        # (Codex on #92, round 4): the harness's identity door judges UTF-8
        # and a replacement character would be a silent change of the bytes.
        draft.write_bytes(b"You are \xff.\n")
        result = subprocess.run(turn, env=self.env, text=True, capture_output=True, timeout=20)
        self.assertEqual(result.returncode, 1)
        self.assertIn("is not UTF-8", result.stderr)
        self.assertIn(str(draft), result.stderr)
        self.assertNotIn("no gate at", result.stderr)
        # A draft this session cannot read names the access group it is read
        # through, the draft being root:weaver-<agent>-admin 0640, here a
        # group the test box does not hold. Perturbation: drop the note and
        # the refusal names no group.
        if os.geteuid() != 0:
            draft.chmod(0o000)
            try:
                result = subprocess.run(turn, env=self.env, text=True, capture_output=True, timeout=20)
            finally:
                draft.chmod(0o640)
            self.assertEqual(result.returncode, 1)
            self.assertIn("weaver-m1-admin", result.stderr)
            self.assertNotIn("no gate at", result.stderr)

    def test_turn_system_sends_the_drafts_bytes_verbatim(self):
        # The draft's line endings reach the gate as they stand: a CRLF draft
        # and a lone-CR draft each arrive with their bytes, read binary and
        # not through text mode's newline translation (Codex on #92, round
        # 4). A stand-in gate accepts the dial, keeps the line, and answers.
        # Perturbation: read the draft in text mode and the CRLF text arrives
        # with LF alone.
        import socket, threading
        root = self.config / "m1"
        root.mkdir()
        (root / "territory").write_text(str(self.decl) + "\n")
        coordination = self.root / "coordination"
        (coordination / "weaver-m1").mkdir(parents=True)
        (root / "coordination-root").write_text(str(coordination) + "\n")
        self.decl.mkdir(parents=True)
        draft = self.decl / "system-prompt.md"
        turn = [sys.executable, str(self.repo / "deploy" / "turn.py"), "m1", "--system", "--raw"]
        for text in ("one\r\ntwo\r\n", "one\rtwo", "one\ntwo\n"):
            with self.subTest(text=text):
                draft.write_bytes(text.encode())
                path = coordination / "weaver-m1" / "gate.sock"
                path.unlink(missing_ok=True)
                gate = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
                gate.bind(str(path))
                gate.listen(1)
                received = {}

                def serve():
                    conn, _ = gate.accept()
                    with conn:
                        line = b""
                        while not line.endswith(b"\n"):
                            chunk = conn.recv(65536)
                            if not chunk:
                                break
                            line += chunk
                        received["line"] = line
                        conn.sendall(b'{"kind":"answered","run":"r-1","text":"ok","turn":"t-1"}\n')
                server = threading.Thread(target=serve)
                server.start()
                try:
                    result = subprocess.run(turn, env=self.env, text=True, capture_output=True, timeout=20)
                finally:
                    server.join(timeout=10)
                    gate.close()
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(json.loads(received["line"]), {"role": "system", "text": text})
                self.assertIn('"kind":"answered"', result.stdout)

    def test_turn_names_a_coordination_root_it_cannot_read(self):
        # Codex on #45, round 12: an unreadable `coordination-root` fell back to
        # /run, so the turn dialled a gate that is not this agent's and reported
        # none standing. Absent and unreadable each refuse, naming the key.
        # Perturbation: restore the fallback, and the turn dials /run.
        root = self.config / "m1"
        root.mkdir()
        turn = [sys.executable, str(self.repo / "deploy" / "turn.py"), "m1", "hello"]
        result = subprocess.run(turn, env=self.env, text=True, capture_output=True, timeout=20)
        self.assertEqual(result.returncode, 1)
        self.assertIn("cannot read " + str(root / "coordination-root"), result.stderr)
        key = root / "coordination-root"
        key.write_text(str(self.root / "elsewhere") + "\n")
        key.chmod(0o000)
        try:
            if os.access(key, os.R_OK):
                self.skipTest("no mode closes a file to this user")
            result = subprocess.run(turn, env=self.env, text=True, capture_output=True, timeout=20)
        finally:
            key.chmod(0o644)
        self.assertEqual(result.returncode, 1)
        self.assertIn("cannot read " + str(key), result.stderr)
        self.assertNotIn("/run/", result.stderr)

    def test_verify_load_keeps_an_interior_space_in_the_prefix(self):
        # Codex on #45, round 12: the prefix was stripped of every space, so
        # `/opt/Weaver Stack` read as `/opt/WeaverStack`. Only its ends are
        # trimmed. Perturbation: delete every space, and admin is not found.
        self.env["FIXTURE_UID"] = "0"
        (self.config / "m1").mkdir()
        spaced = self.root / "Weaver Stack"
        (spaced / "bin").mkdir(parents=True)
        (spaced / "bin" / "weaver-admin").write_text('#!/bin/sh\necho refused\n')
        (spaced / "bin" / "weaver-admin").chmod(0o755)
        (self.stack / "prefix").write_text("  " + str(spaced) + " \n")
        result = self.run_script("verify-load.sh", "m1")
        self.assertNotIn("no weaver-admin", result.stderr)
        self.assertIn("admin does not validate m1", result.stderr)

    def test_stack_refuses_an_agent_key_it_cannot_read(self):
        # The sweep of Codex's round-12 class (unreadable read as absent): a
        # root key the operator cannot read was reported as absent, "this run
        # does not update it". It refuses, naming the key. Perturbation: drop
        # the check, and the plan reads it as absent.
        key = self.config / "existing" / "worker-binary"
        key.chmod(0o000)
        try:
            if os.access(key, os.R_OK):
                self.skipTest("no mode closes a file to this user")
            result = self.run_script("update-stack.sh")
        finally:
            key.chmod(0o644)
        self.assertEqual(result.returncode, 1, result.stdout)
        self.assertIn(str(key) + " stands and cannot be read", result.stderr)

    def test_create_agent_refuses_a_record_without_a_prefix_before_provisioning(self):
        # Codex on #45, round 13: the prefix was read only in the closing
        # message, inside a substitution whose refusal ended the subshell
        # alone, so an absent prefix provisioned everything and exited 0. It is
        # required at preflight. Perturbation: read it late again, and the
        # apply provisions and succeeds.
        (self.stack / "prefix").unlink()
        self.env["ALLOW_APPLY_CHECKS"] = "1"
        result = self.create("--apply")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("no prefix in the stack record", result.stderr)
        self.assertFalse((self.config / "m1").exists())
        self.assertFalse((self.config / ".m1.partial").exists())

    def test_a_new_name_is_made_only_where_no_other_principal_can_claim_it(self):
        # Codex on #45, round 14: a sticky directory keeps another principal from
        # renaming an entry, not from claiming a missing name, which a later
        # root `install -d` or `tee` would follow. `creatable_in` refuses a
        # sticky open directory that held_closed admits, in both scripts, and
        # create-agent refuses a sticky base. Perturbation: drop the mode test.
        sticky = self.root / "sticky"
        sticky.mkdir()
        sticky.chmod(0o1777)
        closed = self.root / "closed"
        closed.mkdir()
        for script in ("create-agent.sh", "bootstrap-stack.sh"):
            text = (self.repo / "deploy" / script).read_text()
            body = "".join(text[text.index(f"{name}() {{"):text.index("\n}\n", text.index(f"{name}() {{")) + 3]
                           for name in ("held_closed", "creatable_in"))
            run = lambda d: subprocess.run(["bash", "-c", body + 'creatable_in "$1"', "bash", str(d)],
                                           env=self.env, text=True, capture_output=True, timeout=20)
            self.assertNotEqual(run(sticky).returncode, 0, script)
            self.assertEqual(run(closed).returncode, 0, script)
        self.env["ALLOW_APPLY_CHECKS"] = "1"
        self.config.chmod(0o1777)
        try:
            result = self.create("--apply")
        finally:
            self.config.chmod(0o755)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("lets another principal make names in it", result.stderr)
        self.assertFalse((self.config / ".m1.partial").exists())

    def test_bootstrap_makes_every_record_entry_readable_whatever_the_umask(self):
        # Codex on #45, round 14: under a 0077 umask `sudo tee` made the record's
        # keys 0600, and the scripts that read the record without privilege
        # then refused. Every file bootstrap writes with tee is made 0644.
        # Perturbation: drop the chmod from `w`.
        lines = (self.repo / "deploy" / "bootstrap-stack.sh").read_text().splitlines()
        writes = [i for i, l in enumerate(lines) if "sudo tee " in l and not l.lstrip().startswith("#")]
        self.assertTrue(writes)
        for i in writes:
            target = lines[i].split("sudo tee ")[1].split()[0]
            self.assertIn(f"sudo chmod 0644 {target}", "\n".join(lines[i:i + 2]), lines[i])

    def test_spu_override_must_be_absolute(self):
        result = self.create("--spu", "relative/spu", "--apply")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("absolute path", result.stderr)
        self.assertEqual(self.calls(), [])

    def test_stack_plan_builds_the_whole_workspace_and_compares_all_members(self):
        result = self.run_script("update-stack.sh")
        self.assertEqual(result.returncode, 0, result.stderr)
        build = next(c for c in self.calls() if c[:2] == ["cargo", "build"])
        self.assertIn("--locked", build)
        self.assertIn("--workspace", build)
        self.assertNotIn("--exclude", build)
        self.assertIn("weaver-spu/cuda", build[-1])
        self.assertEqual(result.stdout.count("NEW"), 7)
        self.assertIn("plan only. rerun with --install", result.stdout)
        self.assert_unprivileged()
        cargo_actions = [c[1] for c in self.calls() if c[0] == "cargo"]
        self.assertEqual(cargo_actions, ["metadata", "test", "build"])

    def test_stack_test_step_selects_no_package_that_left(self):
        # #33: weaver-analysis left the workspace and cargo refuses a package
        # it does not hold. Perturbation: select it again and this fails.
        result = self.run_script("update-stack.sh")
        self.assertEqual(result.returncode, 0, result.stderr)
        test = next(c for c in self.calls() if c[:2] == ["cargo", "test"])
        self.assertNotIn("weaver-analysis", test)
        self.assertEqual([test[i + 1] for i, a in enumerate(test) if a == "-p"],
                         ["weaver-trace", "weaver-harness", "weaver-state"])

    def test_stack_refuses_a_box_on_the_box_wide_layout(self):
        # Admin reads only `<base>/<agent>/`, so a base still holding the
        # box-wide keys refuses by name before cargo runs, the box taken down
        # and stood up again rather than migrated. Perturbation: remove the
        # check and the run plans, reaching the build.
        for retired in ("allow-list", "agent-config-directory", "spu-implementations", "agent-spu"):
            with self.subTest(retired=retired):
                self.log.unlink(missing_ok=True)
                (self.config / retired).write_text("existing\n")
                result = self.run_script("update-stack.sh")
                self.assertEqual(result.returncode, 1, result.stderr)
                self.assertIn("box-wide layout", result.stderr)
                self.assertIn("deploy/REDEPLOY.md sections 1 to 4", result.stderr)
                self.assertFalse(any(c[0] == "cargo" for c in self.calls()))
                (self.config / retired).unlink()

    def test_stack_refuses_a_root_of_the_layout_before_50(self):
        # A root holding its declaration or a retired key is recreated, since
        # the admin this installs reads none of them. Refused by name before
        # cargo runs. Perturbation: drop the check, and the run plans on.
        # Each key admin refuses, as a file and as a dangling link, which
        # admin's look counts too (Codex on #105).
        for retired in admin_retired_root_keys() - {"declaration-directory"}:
            for form in ("file", "dangling link"):
                with self.subTest(retired=retired, form=form):
                    self.log.unlink(missing_ok=True)
                    at = self.config / "existing" / retired
                    if form == "file":
                        at.write_text("x\n")
                    else:
                        at.symlink_to(self.root / "nowhere")
                    result = self.run_script("update-stack.sh")
                    self.assertEqual(result.returncode, 1, result.stderr)
                    self.assertIn(f"existing: its root {self.config / 'existing'} holds {retired}, the layout before #50",
                                  result.stderr)
                    self.assertIn("Recreate existing with deploy/create-agent.sh after its take-down by "
                                  "deploy/HowToDeployANewAgent.md section 7", result.stderr)
                    self.assertFalse(any(c[0] == "cargo" for c in self.calls()))
                    at.unlink()

    def test_stack_and_admin_refuse_the_same_retired_root_keys(self):
        # **The two lists of retired root keys are one** (Codex on #105):
        # update-stack's preflight and admin's `RETIRED_ROOT_KEYS` judge the
        # same root, so a key admin refuses and the preflight passes would
        # build and install before reconcile met the refusal. Perturbation:
        # drop a key from either list, and they differ.
        script = (DEPLOY / "update-stack.sh").read_text()
        line = next(l for l in script.splitlines() if l.startswith("RETIRED_ROOT_KEYS="))
        stack_keys = set(line.split("=", 1)[1].strip('"').split()) | {"declaration-directory"}
        self.assertEqual(stack_keys, admin_retired_root_keys())
        self.assertIn('[ -e "$root/declaration-directory" ] || [ -L "$root/declaration-directory" ]', script)

    def test_stack_refuses_while_a_unit_of_the_old_layout_serves(self):
        # The admin this installs ends a run by its run lock, which a unit's
        # worker never took, so a unit still serving refuses, and so does a
        # systemctl that cannot answer. Perturbations: drop the check, or read
        # a failed look as no unit, and the run plans on.
        for env, said in ((dict(UNITS="weaver-worker@karl.service loaded active running x\n"),
                           "Unload each such agent with the admin that started it, "
                           "take it down by deploy/HowToDeployANewAgent.md section 7"),
                          (dict(UNITS_FAIL="1"), "cannot ask systemd")):
            with self.subTest(env=env):
                self.log.unlink(missing_ok=True)
                for name in ("UNITS", "UNITS_FAIL"): self.env.pop(name, None)
                self.env.update(env)
                result = self.run_script("update-stack.sh")
                self.assertEqual(result.returncode, 1, result.stderr)
                self.assertIn(said, result.stderr)
                if "UNITS" in env:
                    self.assertIn("weaver-worker@karl.service", result.stderr)
                self.assertFalse(any(c[0] == "cargo" for c in self.calls()))

    def test_a_stray_user_group_refuses_before_provisioning(self):
        # useradd --user-group makes a group named for each of the agent, the
        # member and the connector, so a stray one is a collision found before
        # anything is made. Perturbation: check only the trace and access groups,
        # and a stray connector group lets the plan run.
        for group in ("weaver-m1", "weaver-m1-state", "weaver-m1-admincon"):
            with self.subTest(group=group):
                self.env["COLLISION_GROUP"] = group
                result = self.create()
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(f"the group {group} already exists", result.stderr)
        self.assert_unprivileged()

    def test_stack_refuses_every_state_a_unit_can_hold_a_worker_in_and_looks_again(self):
        # Codex on #79: activating, reloading and deactivating are states apart
        # from active, and the look repeats just before the first binary moves.
        # Perturbations: narrow the states, or drop the second look, and this
        # fails.
        text = (self.repo / "deploy" / "update-stack.sh").read_text()
        self.assertIn("--state=active,activating,reloading,deactivating", text)
        calls = [i for i in range(len(text)) if text.startswith("\nrefuse_legacy_units\n", i)]
        self.assertEqual(len(calls), 2)
        self.assertLess(calls[0], text.index('cargo test --release --locked'))
        self.assertLess(text.index('sudo -v || die "--install needs sudo'), calls[1])
        self.assertLess(calls[1], text.index('if [ ${#CHANGED[@]} -gt 0 ]; then\n  say "install"'))

    def test_stack_plans_on_a_box_whose_systemd_is_not_running(self):
        # A systemctl client beside another init reaches no manager, so the box
        # has no unit of the old layout and the plan goes on. Perturbation: drop
        # the sd_booted mark's check, and the failed look refuses the run.
        (self.root / "run" / "systemd" / "system").rmdir()
        self.env["UNITS_FAIL"] = "1"
        result = self.run_script("update-stack.sh")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotIn("cannot ask systemd", result.stderr)
        self.assertFalse(any(c[:2] == ["systemctl", "list-units"] for c in self.calls()))

    KARL = (
        'session = "s-karl-1"\ntool-set = []\npermission-mode = "ask"\n\n'
        '[spu-instruction.decoder]\nresidual-readout-election = false\nsurprisal-election = true\n'
        'tunable-values = { seed = 1, context-capacity = 16384, max-tokens-per-turn = 1024 }\n\n'
        '[spu-instruction.decoder.model-binding]\nartifact = "/opt/weaver/models/x.gguf"\ndevices = [0]\n\n'
        '[[spu-instruction.decoder.identity]]\nrole = "system"\n\n'
        '[[spu-instruction.decoder.identity.content]]\ntype = "text"\ntext = """\n'
        'You are Karl, a small local agent.\nAnswer plainly."""\n\n'
        '[gate-instruction.access-rule]\nallowed-uids = [1000]\nallowed-gids = []\ndenied-uids = []\n\n'
        '[state-store]\nengine = "none"\n'
    )

    def test_stack_refuses_a_declaration_carrying_an_identity(self):
        # The inline identity of before 2026-10-06, as tables or inline, and
        # the identity-file key of #57 refuse by name before the build, naming
        # the agent and the remedy, the declaration unchanged and no draft
        # written (the operator's ruling of 2026-10-08 on #1). Perturbation:
        # drop the identity refusal and the plan runs on to the build.
        decl = self.existing_territory / "agent.toml"
        inline = self.KARL.replace(
            '[[spu-instruction.decoder.identity]]\nrole = "system"\n\n'
            '[[spu-instruction.decoder.identity.content]]\ntype = "text"\ntext = """\n'
            'You are Karl, a small local agent.\nAnswer plainly."""\n\n', "").replace(
            "surprisal-election = true\n",
            'surprisal-election = true\nidentity = [{ role = "system", content = [{ type = "text", text = "x" }] }]\n')
        empty = inline.replace('identity = [{ role = "system", content = [{ type = "text", text = "x" }] }]', "identity = []")
        file_key = inline.replace('identity = [{ role = "system", content = [{ type = "text", text = "x" }] }]',
                                  'identity-file = "system-prompt.md"')
        for label, text, key, form in (
                ("tables", self.KARL, "identity", "the inline identity of before 2026-10-06"),
                ("inline", inline, "identity", "the inline identity of before 2026-10-06"),
                ("empty", empty, "identity", "the inline identity of before 2026-10-06"),
                ("identity-file", file_key, "identity-file", "the identity-file key of #57")):
            with self.subTest(label):
                decl.write_text(text)
                self.log.unlink(missing_ok=True)
                result = self.run_script("update-stack.sh")
                self.assertEqual(result.returncode, 1, result.stdout)
                self.assertIn(f"existing: its declaration {decl} carries spu-instruction.decoder.{key}, {form}",
                              result.stderr)
                self.assertIn("Recreate existing with deploy/create-agent.sh after its take-down by "
                              "deploy/HowToDeployANewAgent.md section 7", result.stderr)
                self.assertEqual(decl.read_text(), text)
                self.assertFalse((decl.parent / "system-prompt.md").exists())
                self.assertFalse(any(c[0] == "cargo" for c in self.calls()))

    def test_stack_refuses_a_trace_the_old_admin_recreated(self):
        # Codex on #82: an admin before #62 recreated a lost trace root:root,
        # which the new admin refuses at load and validate never sees, so the
        # plan refuses it before the build, and the agent is recreated (the
        # operator's ruling of 2026-10-08 on #1). Perturbation: drop the
        # preflight, and the run reaches the build.
        decl = self.existing_territory / "agent.toml"
        # A path holding a space and a substitution, which a TOML string
        # carries and the printed command must quote, in the territory, where
        # admin requires the sink (#99 area 2 review, N1).
        trace = self.existing_territory / "a trace $(id).ndjson"
        trace.write_text("")
        trace.chmod(0o640)
        decl.write_text(f'[state-store]\nengine = "none"\n\n[trace-sink]\nkind = "file"\npath = "{trace}"\ncreate = true\n')
        result = self.run_script("update-stack.sh")
        self.assertEqual(result.returncode, 1, result.stdout)
        # The facts and no command (the Planner's call on #82): what stands,
        # what is required, and the remedy. Perturbation: print a command
        # again, and a line starts with sudo.
        for fact in ("uid 0", "group nobody-group", "mode 640", "regular file",
                     "requires uid 0, group weaver-existing-trace, mode 640, a regular file and not a link",
                     "Recreate existing with deploy/create-agent.sh after its take-down by "
                     "deploy/HowToDeployANewAgent.md section 7"):
            self.assertIn(fact, result.stderr)
        self.assert_no_printed_root_command(result)
        self.assertFalse(any(c[:2] == ["cargo", "build"] for c in self.calls()))
        self.log.unlink(missing_ok=True)
        self.env["TRACE_GROUP_AS"] = "weaver-existing-trace"
        result = self.run_script("update-stack.sh")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("every file sink stands as the territory lays it out", result.stdout)
        # The trace is empty, as a freshly provisioned one is, and still passes:
        # its type is asked by predicate, never by stat's words for it.
        self.assertEqual(trace.stat().st_size, 0)
        # A link at the trace's path is given no command, chown and chmod
        # following a link to whatever it names. Perturbation: give it the
        # re-lay command, and this fails.
        target = self.root / "agents" / "elsewhere"
        target.write_text("")
        trace.unlink()
        trace.symlink_to(target)
        result = self.run_script("update-stack.sh")
        self.assertEqual(result.returncode, 1, result.stdout)
        self.assertIn("not a regular file", result.stderr)
        self.assert_no_printed_root_command(result)

    def test_stack_runs_the_fetched_copy_after_its_fast_forward(self):
        # Codex on #82: the checks run from the copy that started, so a
        # fast-forward that moved HEAD re-executes the fetched script once.
        # Perturbation: drop the re-execution, or its guard, and this fails.
        text = (self.repo / "deploy" / "update-stack.sh").read_text()
        after = text.index('AFTER=$(git rev-parse --short HEAD)')
        reexec = text.index('exec env WEAVER_UPDATE_REEXECUTED=1 bash "$REPO/deploy/update-stack.sh" "${ORIGINAL_ARGS[@]}"')
        self.assertLess(after, reexec)
        self.assertIn('[ "$BEFORE" != "$AFTER" ] && [ -z "${WEAVER_UPDATE_REEXECUTED:-}" ]', text)
        self.assertLess(text.index('ORIGINAL_ARGS=("$@")'), text.index('INSTALL=0'))

    def test_stack_reads_each_declaration_from_its_territory(self):
        # The declaration lives in the territory the root names. One the
        # operator cannot read refuses by name, pointing at the access group,
        # and is never left out. Perturbation: read `<root>/agent.toml` again,
        # and the agent is skipped.
        decl = self.existing_territory / "agent.toml"
        decl.chmod(0o000)
        try:
            if os.access(decl, os.R_OK):
                self.skipTest("no mode closes a file to this user")
            result = self.run_script("update-stack.sh")
        finally:
            decl.chmod(0o644)
        self.assertEqual(result.returncode, 1, result.stdout)
        self.assertIn(str(decl) + " cannot be read", result.stderr)
        self.assertIn("weaver-existing-admin", result.stderr)

    def test_stack_refuses_a_root_naming_a_declaration_directory(self):
        # The operator's ruling of 2026-10-08 on #1: a root naming a
        # declaration-directory, the layout before 2026-10-07, is recreated,
        # never migrated, so the plan and the install refuse it by name before
        # anything is built or asked of sudo, with a territory key beside it
        # or without, and change nothing. Perturbation: drop the refusal and
        # the root without a territory key refuses on another ground, the one
        # beside it plans on.
        old_root = self.config / "old"
        old_root.mkdir()
        (old_root / "worker-binary").write_text(str(self.root / "installed" / "pyworker"))
        old_dir = self.operator_home / ".weaveragent" / "old"
        old_dir.mkdir(parents=True)
        (old_dir / "agent.toml").write_text("[state-store]\nengine = \"none\"\n")
        (old_root / "declaration-directory").write_text(str(old_dir) + "\n")
        (self.root / "installed").mkdir(exist_ok=True)
        for territory_key in (False, True):
            if territory_key:
                (old_root / "territory").write_text(str(self.existing_territory) + "\n")
            for mode in ((), ("--install",)):
                with self.subTest(territory_key=territory_key, mode=mode):
                    self.log.unlink(missing_ok=True)
                    self.env["ALLOW_APPLY_CHECKS"] = "1"
                    result = self.run_script("update-stack.sh", *mode)
                    self.assertEqual(result.returncode, 1, result.stdout)
                    self.assertIn(f"old: its root {old_root} names a declaration-directory, the layout before 2026-10-07",
                                  result.stderr)
                    self.assertIn("Recreate old with deploy/create-agent.sh after its take-down by "
                                  "deploy/HowToDeployANewAgent.md section 7", result.stderr)
                    self.assertFalse([c for c in self.calls() if c[0] in ("cargo", "sudo")], self.calls())
                    self.assertTrue((old_dir / "agent.toml").exists())
                    self.assertTrue((old_root / "declaration-directory").exists())

    def test_an_unload_is_read_from_admins_answer(self):
        """**An unload is verified by its answer** (the #94 survey's S22):
        `unload_verified` answers success only where admin's last line is the
        unloaded state, so a refused unload, the run still standing, fails
        the verify step and rolls the install back; and admin's cause on
        stderr is relayed beside it (#99 area 2 review, K9). Perturbations:
        answer success whatever admin said and the refusal passes; send
        admin's stderr to /dev/null again and the cause is lost."""
        text = (DEPLOY / "update-stack.sh").read_text()
        admin = self.root / "bin" / "weaver-admin"
        admin.parent.mkdir(parents=True, exist_ok=True)
        program = ('sudo() { [ "$1" = -n ] && shift; shift; "$@"; }\n'
                   + shell_function(text, "unload_verified") + shell_function(text, "answered_state")
                   + 'unload_verified m1')
        env = {k: v for k, v in os.environ.items() if k != "BASH_ENV"}
        env.update(BIN_DIR=str(admin.parent), ADMIN_BASE=str(self.config))
        for answer, verified in (('{"kind":"state","state":"unloaded"}', True),
                                 ('{"kind":"refused","refusal":"activity_not_at_rest"}', False),
                                 ('', False)):
            with self.subTest(answer=answer):
                admin.write_text(f"#!/bin/sh\necho 'weaver-admin: the cause' >&2\necho '{answer}'\nexit 0\n")
                admin.chmod(0o755)
                ran = subprocess.run(["bash", "-c", program], env=env, text=True,
                                     capture_output=True, timeout=20)
                self.assertEqual(ran.returncode == 0, verified, ran.stderr)
                if not verified:
                    self.assertIn("admin answered the unload of m1", ran.stderr)
                if answer:
                    self.assertIn("  admin: weaver-admin: the cause", ran.stderr)

    def test_stack_refuses_an_unprovided_engine_and_names_the_recreation(self):
        # A pre-#85 declaration electing postgres refuses before the build,
        # as an engine this build does not provide, and is recreated rather
        # than edited (the operator's ruling of 2026-10-08 on #1).
        # Perturbation: restore the old remedy and the recreation goes unnamed.
        decl = self.existing_territory / "agent.toml"
        decl.write_text('[state-store]\nengine = "postgres"\ndatabase = "d"\nrole = "r"\n')
        result = self.run_script("update-stack.sh")
        self.assertEqual(result.returncode, 1, result.stdout)
        self.assertIn("postgres store, which is not an engine this build provides", result.stderr)
        self.assertIn("Recreate existing with deploy/create-agent.sh after its take-down by "
                      "deploy/HowToDeployANewAgent.md section 7", result.stderr)
        self.assertNotIn("REDEPLOY.md", result.stderr)
        self.assertNotIn("Name weaver-state/postgres", result.stderr)

    def test_stack_agents_are_the_roots_under_the_base(self):
        # A staged root under a dot-name and a plain file are not agents.
        # Perturbation: drop the name check and `.m2.partial` refuses.
        (self.config / ".m2.partial").mkdir()
        (self.config / "stray-file").write_text("x")
        result = self.run_script("update-stack.sh")
        self.assertEqual(result.returncode, 0, result.stderr)
        agents_line = next(l for l in result.stdout.splitlines() if l.startswith("  agents"))
        self.assertEqual(agents_line.split(), ["agents", "existing"])
        # With no root at all the plan goes on (#39), its agents none.
        shutil.rmtree(self.config / "existing")
        shutil.rmtree(self.root / 'target with "quotes"', ignore_errors=True)
        result = self.run_script("update-stack.sh")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotIn("no agent root under", result.stderr)

    def test_stack_refuses_a_root_naming_no_territory(self):
        # A root naming neither a territory nor a declaration-directory is
        # one admin refuses at every verb, so the plan refuses it by name
        # before the build rather than leaving it out (the operator's ruling
        # of 2026-10-08 on #1). Perturbation: skip such a root again, and the
        # plan runs on with `existing` alone.
        (self.config / "undeclared").mkdir()
        (self.config / "undeclared" / "worker-binary").write_text("/x")
        result = self.run_script("update-stack.sh")
        self.assertEqual(result.returncode, 1, result.stdout)
        self.assertIn(f"undeclared: its root {self.config / 'undeclared'} names no territory as a regular "
                      "file, which admin refuses at every verb", result.stderr)
        self.assertIn("Recreate undeclared with deploy/create-agent.sh after its take-down by "
                      "deploy/HowToDeployANewAgent.md section 7", result.stderr)
        self.assertFalse(any(c[0] == "cargo" for c in self.calls()))
        # **A territory key that is a link refuses too**, as admin refuses
        # any root entry that is not a regular file (Codex on #105).
        # Perturbation: look at the key with -f alone, and it reads.
        shutil.rmtree(self.config / "undeclared")
        key = self.config / "existing" / "territory"
        aside = self.root / "territory.key"
        key.rename(aside)
        key.symlink_to(aside)
        try:
            result = self.run_script("update-stack.sh")
        finally:
            key.unlink()
            aside.rename(key)
        self.assertEqual(result.returncode, 1, result.stdout)
        self.assertIn("names no territory as a regular file", result.stderr)

    def test_stack_refuses_a_territory_named_through_a_link(self):
        # **The territory key is its canonical path**, as admin's
        # `judge_territory` requires (#99 area 2, H3; Codex on #105). A key
        # reaching it through a link above it refuses by name before the
        # build. Perturbation: drop the comparison, and the plan runs on.
        key = self.config / "existing" / "territory"
        real = self.existing_territory
        via = self.root / "via"
        via.symlink_to(real.parent)
        saved = key.read_text()
        key.write_text(str(via / real.name) + "\n")
        try:
            result = self.run_script("update-stack.sh")
        finally:
            key.write_text(saved)
        self.assertEqual(result.returncode, 1, result.stdout)
        self.assertIn(f"which resolves to {real.resolve()}, and admin requires the canonical path", result.stderr)
        self.assertFalse(any(c[0] == "cargo" for c in self.calls()))

    def test_stack_refuses_a_root_closed_to_the_operator(self):
        # Codex on #45, round 9: a root this user cannot read answered
        # `-f agent.toml` false and was silently left out of the agents, so an
        # install verified only the readable ones. It refuses naming the root,
        # before the build. Perturbation: drop the readability check, and the
        # run plans on without it.
        closed = self.config / "closed"
        closed.mkdir()
        (closed / "agent.toml").write_text("")
        closed.chmod(0o000)
        try:
            if os.access(closed, os.R_OK | os.X_OK):
                self.skipTest("no mode closes a directory to this user")
            result = self.run_script("update-stack.sh")
        finally:
            closed.chmod(0o755)
        self.assertEqual(result.returncode, 1, result.stdout)
        self.assertIn(str(closed) + " is closed to", result.stderr)
        self.assertFalse([c for c in self.calls() if c[:2] == ["cargo", "build"]], self.calls())

    def test_the_plan_refuses_a_territory_off_the_law_naming_what_is_off(self):
        """**A territory off the law admin judges refuses by name before the
        build** (#99 area 2 review, N1; the Planner's ruling of 2026-10-08 on
        #1 that a lay is a migration): its group, its mode, a setgid bit, an
        access entry, and a `save-points/` missing or off are each named, with
        the remedy, and nothing is laid. Perturbations: plan a lay again
        instead of refusing and the plan exits 0; compare the mode masked to
        three digits and 2710 goes unnamed; drop the access look and the entry
        goes unnamed."""
        territory = self.existing_territory
        sp = territory / "save-points"
        setfacl = shutil.which("setfacl", path="/usr/bin:/bin")

        def reset():
            if not sp.exists():
                sp.mkdir()
            sp.chmod(0o750)
            territory.chmod(0o710)
            self.env.pop("LAYOUT_GROUPS", None)
            if setfacl:
                subprocess.run([setfacl, "-b", str(territory)], check=True)

        def groups(path, group):
            self.env["LAYOUT_GROUPS"] = json.dumps({str(path): group})

        cases = [("setgid", lambda: territory.chmod(0o2710), territory, 0o2710, "stands setgid,"),
                 ("mode", lambda: territory.chmod(0o750), None, None, "stands mode 750,"),
                 ("group", lambda: groups(territory, "weaver-existing-old"), None, None,
                  "stands grouped weaver-existing-old,"),
                 ("no save-points/", sp.rmdir, None, None, "stands no save-points/,"),
                 ("save-points/ mode", lambda: sp.chmod(0o755), None, None, "stands save-points/ mode 755,"),
                 ("save-points/ setgid", lambda: sp.chmod(0o2750), sp, 0o2750, "stands save-points/ setgid,"),
                 ("save-points/ group", lambda: groups(sp, "weaver-existing-state"), None, None,
                  "stands save-points/ grouped weaver-existing-state,")]
        if setfacl:
            cases.append(("access entries",
                          lambda: subprocess.run([setfacl, "-m", f"u:{os.getuid()}:x", str(territory)], check=True),
                          None, None, "stands carrying access entries,"))
        for label, change, path, mode, said in cases:
            with self.subTest(label):
                reset()
                change()
                if path is not None and path.stat().st_mode & 0o7777 != mode:
                    continue  # this filesystem keeps no setgid bit for this user
                self.log.unlink(missing_ok=True)
                try:
                    result = self.run_script("update-stack.sh")
                finally:
                    reset()
                self.assertEqual(result.returncode, 1, result.stdout)
                self.assertIn(f"existing: its territory {territory} {said}", result.stderr)
                self.assertIn("which admin refuses at every verb: it requires root:weaver-existing-state 0710",
                              result.stderr)
                self.assertIn("Recreate existing with deploy/create-agent.sh after its take-down by "
                              "deploy/HowToDeployANewAgent.md section 7", result.stderr)
                self.assertFalse(any(c[0] == "cargo" for c in self.calls()))
        # Lawful: the plan runs on.
        self.log.unlink(missing_ok=True)
        result = self.run_script("update-stack.sh")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("every territory a root names stands as admin judges it", result.stdout)

    def test_the_plan_refuses_a_sink_or_a_link_admin_would_refuse(self):
        """A sink whose directory is not the canonical territory, and a link
        at the territory or at `save-points/`, refuse by name before anything
        is built (#99 area 2 review, N1); the sink is drift, rewritten by hand
        with sudoedit. Perturbation: drop the sink comparison and the plan
        runs on."""
        territory = self.existing_territory
        decl = territory / "agent.toml"
        link = self.root / "agents-link"
        link.symlink_to(self.root / "agents")
        for sink in (self.root / "elsewhere" / "trace.ndjson", link / "weaver-existing" / "trace.ndjson"):
            with self.subTest(sink=str(sink)):
                decl.write_text(f'[state-store]\nengine = "none"\n\n[trace-sink]\nkind = "file"\npath = "{sink}"\n')
                self.log.unlink(missing_ok=True)
                result = self.run_script("update-stack.sh")
                self.assertEqual(result.returncode, 1, result.stdout)
                self.assertIn(f"its trace sink {sink} does not stand in its canonical territory {territory.resolve()}",
                              result.stderr)
                self.assertIn(f"with sudoedit {decl}", result.stderr)
                self.assertFalse([c for c in self.calls() if c[:2] == ["cargo", "build"]], self.calls())
        decl.write_text('[state-store]\nengine = "none"\n')
        (territory / "save-points").rmdir()
        (territory / "save-points").symlink_to(self.root)
        result = self.run_script("update-stack.sh")
        self.assertEqual(result.returncode, 1, result.stdout)
        self.assertIn(f"{territory}/save-points is a link", result.stderr)
        (territory / "save-points").unlink()
        moved = self.root / "agents" / "weaver-existing-real"
        territory.rename(moved)
        territory.symlink_to(moved)
        result = self.run_script("update-stack.sh")
        self.assertEqual(result.returncode, 1, result.stdout)
        self.assertIn(f"its territory {territory} does not stand as a directory, or is a link", result.stderr)
        self.assertIn("Recreate existing with deploy/create-agent.sh", result.stderr)

    def test_the_verify_load_must_answer_the_idle_state(self):
        """**The verify step reads the load's answer** (#99 area 2 review,
        N5): a load admin refuses after the harness wrote `load` naming its
        composer leaves the sink grown and the unload answering unloaded, and
        still rolls the install back, naming the answer; a load answering
        idle verifies. Perturbation: call `admin_answer load` without reading
        its answer again and the refused load reports the box current."""
        territory = self.existing_territory.resolve()
        sink = territory / "trace.ndjson"
        (territory / "agent.toml").write_text(
            f'[state-store]\nengine = "none"\n\n[trace-sink]\nkind = "file"\npath = "{sink}"\n')
        (self.root / "installed").mkdir(exist_ok=True)
        admin = self.root / "fixture-admin"
        for answer, verified in (('{"kind":"refused","reason":"no_residency"}', False),
                                 ('{"kind":"state","state":"idle"}', True)):
            with self.subTest(answer=answer):
                admin.write_text(
                    "#!/bin/sh\n"
                    "case \"$1\" in\n"
                    "  validate) echo '{\"kind\":\"validated\"}' ;;\n"
                    "  unload) echo '{\"kind\":\"state\",\"state\":\"unloaded\"}' ;;\n"
                    f"  load) echo '{{\"kind\":\"load\",\"payload\":{{\"composer\":\"x\"}}}}' >> '{sink}'\n"
                    f"        echo '{answer}' ;;\n"
                    "  *) exit 2 ;;\n"
                    "esac\n")
                admin.chmod(0o755)
                sink.unlink(missing_ok=True)
                self.log.unlink(missing_ok=True)
                shutil.rmtree(self.root / 'target with "quotes"', ignore_errors=True)
                self.env.update(ALLOW_APPLY_CHECKS="1", FIXTURE_ADMIN=str(admin))
                result = self.run_script("update-stack.sh", "--install")
                if verified:
                    self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                    self.assertIn("the box is at", result.stdout)
                else:
                    self.assertNotEqual(result.returncode, 0, result.stdout)
                    self.assertIn(f"the verify load answered {answer}, not the idle state", result.stderr)
                    self.assertNotIn("the box is at", result.stdout)

    def test_the_verify_step_reads_no_line_of_a_pipe_sink(self):
        """**A pipe sink is verified by the load's answer and named as such**
        (#99 area 2 review, K6): neither a line count nor the load event is
        read from it, so a box holding one is not rolled back at every
        install. Perturbation: verify every sink as a file again and the
        install rolls back on "is not a regular file"."""
        territory = self.existing_territory.resolve()
        fifo = territory / "trace.pipe"
        os.mkfifo(fifo)
        (territory / "agent.toml").write_text(
            f'[state-store]\nengine = "none"\n\n[trace-sink]\nkind = "pipe"\npath = "{fifo}"\n')
        (self.root / "installed").mkdir(exist_ok=True)
        admin = self.root / "fixture-admin"
        admin.write_text("#!/bin/sh\n"
                         "case \"$1\" in\n"
                         "  validate) echo '{\"kind\":\"validated\"}' ;;\n"
                         "  unload) echo '{\"kind\":\"state\",\"state\":\"unloaded\"}' ;;\n"
                         "  load) echo '{\"kind\":\"state\",\"state\":\"idle\"}' ;;\n"
                         "  *) exit 2 ;;\n"
                         "esac\n")
        admin.chmod(0o755)
        self.env.update(ALLOW_APPLY_CHECKS="1", FIXTURE_ADMIN=str(admin))
        result = self.run_script("update-stack.sh", "--install")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("its pipe sink is not a file a line count reads", result.stdout)
        self.assertIn("the box is at", result.stdout)

    def test_stack_updates_a_box_with_no_agent(self):
        """**A stack with no agent root is updated all the same** (#39): the
        plan builds and plans, and --install installs, holds no rule,
        unloads, reconciles and verifies nothing, saying so, and reports the
        box at its commit. Perturbations: put back the "no agent root"
        refusal and the plan exits 1; put back the bare `VERIFIED -gt 0`
        rollback and the install rolls back."""
        shutil.rmtree(self.config / "existing")
        result = self.run_script("update-stack.sh")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("agents        (none: nothing to unload, reconcile or verify)", result.stdout)
        self.assertIn("no agents: --install holds no connector rule and unloads none", result.stdout)
        self.assertIn("== plan", result.stdout)
        self.assertNotIn("REFUSED", result.stderr)
        self.assert_unprivileged()
        (self.root / "installed").mkdir(exist_ok=True)
        admin = self.root / "fixture-admin"
        admin.write_text("#!/bin/sh\necho \"$1\" >> \"$ADMIN_CALLS\"\nexit 2\n")
        admin.chmod(0o755)
        shutil.rmtree(self.root / 'target with "quotes"', ignore_errors=True)
        self.env.update(ALLOW_APPLY_CHECKS="1", FIXTURE_ADMIN=str(admin),
                        ADMIN_CALLS=str(self.root / "admin-calls"))
        result = self.run_script("update-stack.sh", "--install")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        for said in ("no agents: no connector rule to hold, none to unload", "no agents to reconcile",
                     "no agents to verify", "the box is at"):
            self.assertIn(said, result.stdout)
        self.assertTrue([c for c in self.calls() if c[:2] == ["sudo", "install"]], self.calls())
        self.assertFalse((self.root / "admin-calls").exists(), "no admin verb is asked")

    def window_admin(self, extra=""):
        """A stand-in admin for the install window: each verb logged with
        whether the connector's rule stands at its name, unload and load
        answering their states, validate validated. EXTRA runs first."""
        rule = self.root / "etc" / "sudoers.d" / "weaver-existing"
        admin = self.root / "fixture-admin"
        admin.write_text(
            "#!/bin/sh\n" + extra +
            f"if [ -e '{rule}' ]; then at=in; else at=out; fi\n"
            "echo \"$1 $at\" >> \"$ADMIN_CALLS\"\n"
            "case \"$1\" in\n"
            "  validate) echo '{\"kind\":\"validated\"}' ;;\n"
            "  unload) echo \"${UNLOAD_ANSWER:-{\\\"kind\\\":\\\"state\\\",\\\"state\\\":\\\"unloaded\\\"}}\" ;;\n"
            "  load) echo '{\"kind\":\"state\",\"state\":\"idle\"}' ;;\n"
            "  *) exit 2 ;;\n"
            "esac\n")
        admin.chmod(0o755)
        territory = self.existing_territory.resolve()
        fifo = territory / "trace.pipe"
        if not fifo.exists():
            os.mkfifo(fifo)
        (territory / "agent.toml").write_text(
            f'[state-store]\nengine = "none"\n\n[trace-sink]\nkind = "pipe"\npath = "{fifo}"\n')
        (self.root / "installed").mkdir(exist_ok=True)
        rule.write_text("weaver-existing-admincon ALL=(root) NOPASSWD: /fixture\n")
        self.admin_calls = self.root / "admin-calls"
        self.admin_calls.unlink(missing_ok=True)
        self.log.unlink(missing_ok=True)
        shutil.rmtree(self.root / 'target with "quotes"', ignore_errors=True)
        self.env.update(ALLOW_APPLY_CHECKS="1", FIXTURE_ADMIN=str(admin), ADMIN_CALLS=str(self.admin_calls))
        self.env.pop("UNLOAD_ANSWER", None)
        return rule, rule.with_name(".weaver-existing.updating")

    def test_the_install_window_holds_the_rules_and_unloads_every_agent_first(self):
        """**--install holds each connector rule and unloads every agent before
        any binary moves** (#99 area 2 review, R2): the rule is renamed to its
        dot-name `.updating`, which sudo's includedir skips, before the first
        unload; every agent is unloaded before the first binary is installed;
        admin is asked nothing while the rule stands; the rule comes back
        after the verify. The plan names the agent and moves nothing.
        Perturbations: drop the hold and admin sees the rule in place; move
        the unload after the install and the order fails; drop the restore
        after the verify and the rule stays held."""
        rule, held = self.window_admin()
        self.env.pop("ALLOW_APPLY_CHECKS")
        plan = self.run_script("update-stack.sh")
        self.assertEqual(plan.returncode, 0, plan.stdout + plan.stderr)
        self.assertIn("existing     --install holds /etc/sudoers.d/weaver-existing as .weaver-existing.updating "
                      "where it stands, then unloads existing", plan.stdout)
        self.assertIn("plan only: no rule is moved and no agent unloaded", plan.stdout)
        self.assertTrue(rule.exists())
        self.assertFalse(held.exists())
        self.assert_unprivileged()
        self.env["ALLOW_APPLY_CHECKS"] = "1"
        shutil.rmtree(self.root / 'target with "quotes"', ignore_errors=True)
        self.log.unlink(missing_ok=True)
        result = self.run_script("update-stack.sh", "--install")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn(f"held /etc/sudoers.d/weaver-existing as /etc/sudoers.d/.weaver-existing.updating", result.stdout)
        self.assertIn("restored /etc/sudoers.d/weaver-existing", result.stdout)
        self.assertTrue(rule.exists())
        self.assertFalse(held.exists())
        verbs = self.admin_calls.read_text().split("\n")
        self.assertEqual(verbs[0], "unload out", verbs)
        self.assertFalse([v for v in verbs if v.endswith(" in")], verbs)
        calls = self.calls()
        def at(pred):
            return next(i for i, c in enumerate(calls) if pred(c))
        hold = at(lambda c: c[:2] == ["sudo", "mv"] and c[-1].endswith(".updating"))
        unload = at(lambda c: c[0] == "sudo" and "unload" in c)
        install = at(lambda c: c[:2] == ["sudo", "install"])
        back = at(lambda c: c[:2] == ["sudo", "mv"] and c[-1].endswith("/weaver-existing"))
        load = at(lambda c: c[0] == "sudo" and "load" in c)
        self.assertLess(hold, unload)
        self.assertLess(unload, install)
        self.assertLess(load, back)
        self.assertEqual(calls[hold][2:4], ["-T", "--"])

    def test_a_refused_unload_rolls_back_with_the_rule_restored(self):
        """**An unload admin refuses before the install rolls back** with no
        binary moved and the held rule put back. Perturbation: drop the
        rule's restore from the exit trap and it stays held."""
        rule, held = self.window_admin()
        self.env["UNLOAD_ANSWER"] = '{"kind":"refused","refusal":"activity_not_at_rest"}'
        result = self.run_script("update-stack.sh", "--install")
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertIn("the unload before the install did not answer unloaded", result.stderr)
        self.assertIn("restored /etc/sudoers.d/weaver-existing", result.stderr)
        self.assertTrue(rule.exists())
        self.assertFalse(held.exists())
        self.assertFalse([c for c in self.calls() if c[:2] == ["sudo", "install"]], self.calls())

    def test_a_held_name_already_standing_refuses_before_any_rule_moves(self):
        """**A stale `.updating` refuses by name** (a run killed by SIGKILL or
        a host stop leaves one): nothing is moved and admin is asked nothing.
        Perturbation: drop the look, and `mv -T` meets the held name."""
        rule, held = self.window_admin()
        held.write_text("an earlier run's held rule\n")
        result = self.run_script("update-stack.sh", "--install")
        self.assertEqual(result.returncode, 1, result.stdout)
        self.assertIn(f"/etc/sudoers.d/.weaver-existing.updating already stands", result.stderr)
        self.assertEqual(held.read_text(), "an earlier run's held rule\n")
        self.assertTrue(rule.exists())
        self.assertFalse([c for c in self.calls() if c[:2] == ["sudo", "mv"]], self.calls())
        self.assertFalse(self.admin_calls.exists())

    def test_a_signal_in_the_window_puts_the_rule_back(self):
        """**HUP, INT and TERM put the held rule back** through the exit
        trap: the stand-in admin signals the run's whole process group at the
        verify load, with the rule held and the binaries installed.
        Perturbation: drop the three signal traps and the rule stays held."""
        for sig, code in (("HUP", 129), ("INT", 130), ("TERM", 143)):
            with self.subTest(signal=sig):
                once = self.root / "signalled"
                once.unlink(missing_ok=True)
                rule, held = self.window_admin(
                    f"if [ \"$1\" = load ] && [ ! -e '{once}' ]; then : > '{once}'; "
                    f"kill -{sig} -- -$(ps -o pgid= -p $$ | tr -d ' '); sleep 5; fi\n")
                run = subprocess.run(["bash", str(self.repo / "deploy" / "update-stack.sh"), "--install"],
                                     env=self.env, text=True, capture_output=True, timeout=60,
                                     start_new_session=True)
                self.assertEqual(run.returncode, code, run.stdout + run.stderr)
                self.assertTrue(rule.exists(), run.stdout + run.stderr)
                self.assertFalse(held.exists())
                self.assertIn("restored /etc/sudoers.d/weaver-existing", run.stderr)

    def test_stack_build_failure_cannot_claim_a_plan(self):
        self.env["BUILD_FAIL"] = "1"
        result = self.run_script("update-stack.sh")
        self.assertEqual(result.returncode, 42, result.stderr)
        self.assertIn("fixture build refusal", result.stderr)
        self.assertNotIn("== plan", result.stdout)
        self.assertNotIn("box is current", result.stdout)
        self.assert_unprivileged()

    # ---- verify-lifecycle.sh: the live end-to-end check of one agent's
    # lifecycle. Its --apply needs a real box (owners and groups by name,
    # admin's answers in sequence, the gate's socket, kill, userdel), which
    # the fixture's doubles do not carry, so its plan, its refusals and its
    # near misses' restore are what run here.

    LIFECYCLE_STEPS = ("0", "1", "1b", "2", "3", "4", "5", "6", "6b", "7", "8", "8b", "9")

    def lifecycle(self, *args, agent="m1"):
        self.install_stack()
        return self.run_script("verify-lifecycle.sh", "--agent", agent, *args)

    def test_lifecycle_plan_prints_every_step_in_order_and_acts_on_nothing(self):
        # Plan by default: every step with its commands and its expected
        # answers, in the run's order, the three checks with no command, no
        # sudo and no file changed. Perturbation: drop step 6b from the run
        # list, or let the plan print a PASS, and this fails.
        self.install_stack()
        before = {p: p.read_bytes() for p in self.root.rglob("*") if p.is_file()}
        result = self.lifecycle()
        self.assertEqual(result.returncode, 0, result.stderr)
        at = [result.stdout.find(f"== step {s} (") for s in self.LIFECYCLE_STEPS]
        self.assertNotIn(-1, at, dict(zip(self.LIFECYCLE_STEPS, at)))
        self.assertEqual(at, sorted(at), "the steps print in the run's order")
        self.assertGreaterEqual(result.stdout.count("     expect: "), len(self.LIFECYCLE_STEPS))
        self.assertIn("   $ deploy/create-agent.sh m1 --artifact", result.stdout)
        for manual in ("against the KV cache", "stamp position", "descriptor audit"):
            self.assertIn(manual, result.stdout)
        self.assertIn("== plan only", result.stdout)
        self.assertNotIn("PASS", result.stdout)
        self.assert_unprivileged()
        after = {p: p.read_bytes() for p in self.root.rglob("*") if p.is_file() and p != self.log}
        self.assertEqual(before, after)

    def test_lifecycle_steps_name_what_they_check(self):
        # Each step names its Spec section or its #99 finding, in the plan and
        # so in every PASS and FAIL line. Perturbation: drop a finding id from
        # a step's label, and this fails.
        result = self.lifecycle()
        self.assertEqual(result.returncode, 0, result.stderr)
        for label in ("step 0 (deploy/REDEPLOY.md section 0)",
                      "step 1b (weaver-admin-Spec section 9; #99 H3)",
                      "step 2 (weaver-admin-Spec section 6; #99 K12)",
                      "step 5 (weaver-admin-Spec section 4; #99 N6)",
                      "step 6b (weaver-admin-Spec section 9; #99 K5)",
                      "step 8 (#99 N1, N5)", "step 8b (#99 H5, measured)",
                      "step 9 (HowToDeployANewAgent.md section 7)"):
            self.assertIn(f"== {label}:", result.stdout)

    def test_lifecycle_step_8_is_a_plan_only_near_miss_and_8b_always_runs(self):
        # Step 8 stages a declaration-directory key on the throwaway root and
        # expects admin's validate and update-stack.sh's plan to refuse it by
        # name, then removes it; no step runs update-stack.sh --install, and
        # the migration's switches are gone (the operator's ruling of
        # 2026-10-08 on #1). Perturbation: put back the --install line, or
        # skip step 8 or 8b, and this fails.
        result = self.lifecycle()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("== step 8 (#99 N1, N5): a root naming a declaration-directory", result.stdout)
        self.assertIn("== step 8b (#99 H5, measured):", result.stdout)
        self.assertIn("validate refuses config_invalid naming declaration-directory", result.stdout)
        self.assertIn("the plan exits 1 before any build, refusing m1 by name for its declaration-directory",
                      result.stdout)
        self.assertNotIn("SKIPPED", result.stdout)
        self.assertNotIn("--install", result.stdout)
        self.assert_unprivileged()
        script = (self.repo / "deploy" / "verify-lifecycle.sh").read_text()
        self.assertNotIn("update-stack.sh --install )", script)
        self.assertIn('RESTORE=(restore_absent "$R/declaration-directory")', script)
        for retired in ("--with-migration", "--allow-other-agents"):
            usage = self.run_script("verify-lifecycle.sh", "--help")
            self.assertNotIn(retired, usage.stdout)
            refused = self.lifecycle(retired)
            self.assertEqual(refused.returncode, 2, refused.stdout)
            self.assertIn(f"unknown argument: {retired}", refused.stderr)

    def test_lifecycle_keep_stops_before_the_take_down_with_the_checks(self):
        # **The checks with no command are made against a loaded agent**
        # (Codex on #104, round 1): with --keep the plan stops before step 9,
        # prints the checks and the exact --cleanup line; without it the
        # checks come before step 9 and the run says they were not made. The
        # usage recommends --keep for a first live run, and --keep with
        # --cleanup refuses. Perturbations: print the checks after step 9
        # again; run step 9 under --keep.
        kept = self.lifecycle("--keep")
        self.assertEqual(kept.returncode, 0, kept.stderr)
        self.assertNotIn("== step 9 (", kept.stdout)
        checks = kept.stdout.index("checks with no command today")
        self.assertLess(kept.stdout.index("== step 7 ("), checks)
        self.assertIn("kept: m1 stands loaded for the checks above", kept.stdout)
        self.assertIn("deploy/verify-lifecycle.sh --agent m1 --cleanup --apply", kept.stdout)
        self.assert_unprivileged()
        plain = self.lifecycle()
        self.assertEqual(plain.returncode, 0, plain.stderr)
        self.assertLess(plain.stdout.index("checks with no command today"),
                        plain.stdout.index("== step 9 ("))
        self.assertIn("the checks above were not made", plain.stdout)
        usage = self.run_script("verify-lifecycle.sh", "--help")
        self.assertIn("RECOMMENDED for a first live run", usage.stdout)
        both = self.lifecycle("--keep", "--cleanup")
        self.assertNotEqual(both.returncode, 0)
        self.assertIn("pass one", both.stderr)

    def test_lifecycle_refuses_before_acting(self):
        # Each refusal fires with its message before sudo is asked for: a name
        # create-agent would refuse, an agent that stands, a run as root or
        # under sudo, no stack record, an archive directory another user
        # writes, and an earlier run's marker.
        # Perturbation: drop any one refusal, and its case fails.
        archive = self.root / "archive"
        archive.mkdir()
        apply = ["--apply", "--archive", str(archive), "--artifact", str(self.artifact)]
        # **An archive directory another principal writes refuses** (the
        # commit security review of 36f1374): root's tar would follow a link
        # planted at the archive's name. Perturbation: drop the closed walk.
        open_archive = self.root / "open-archive"
        open_archive.mkdir()
        open_archive.chmod(0o777)
        open_apply = ["--apply", "--archive", str(open_archive), "--artifact", str(self.artifact)]
        # **A sticky one refuses too**, /tmp's shape: sticky stops another
        # user removing a name, never creating one. A sticky directory above
        # a closed one is allowed (the suite's own scratch sits under /tmp).
        # Perturbation: allow a sticky archive directory again.
        sticky_archive = self.root / "sticky-archive"
        sticky_archive.mkdir()
        sticky_archive.chmod(0o1777)
        sticky_apply = ["--apply", "--archive", str(sticky_archive), "--artifact", str(self.artifact)]
        marker = self.stack / "verify-lifecycle-m1"
        cases = (
            ("bad name", {}, None, ["--agent", "M1"], "lowercase letters and digits"),
            ("existing root", {}, "root", [], "already exists"),
            ("existing account", {"COLLISION": "weaver-m1-relay"}, None, [], "the account weaver-m1-relay already exists"),
            ("existing group", {"COLLISION_GROUP": "weaver-m1-admin"}, None, [], "the group weaver-m1-admin already exists"),
            ("existing territory", {}, "territory", [], "already exists"),
            ("root", {"FIXTURE_UID": "0"}, None, [], "not as root"),
            ("sudo", {"SUDO_USER": "someone"}, None, [], "not under sudo"),
            ("no stack record", {}, "stack", [], "no stack record"),
            ("open archive", {}, None, open_apply, "is writable by another principal"),
            ("sticky archive", {}, None, sticky_apply, "is writable by another principal"),
            ("earlier marker", {}, "marker", [], "take that agent down first with --cleanup"),
        )
        self.install_stack()
        for name, env, make, args, said in cases:
            with self.subTest(case=name):
                self.log.unlink(missing_ok=True)
                saved = dict(self.env)
                self.env.update(ALLOW_APPLY_CHECKS="1", **env)
                if make == "root": (self.config / "m1").mkdir()
                elif make == "territory": self.decl.mkdir()
                elif make == "stack": shutil.move(self.stack, self.root / "stack.aside")
                elif make == "marker": marker.write_text('agent = "m1"\n')
                try:
                    if args[:1] == ["--agent"]:
                        result = self.run_script("verify-lifecycle.sh", *args)
                    else:
                        result = self.lifecycle(*args)
                finally:
                    self.env = saved
                    if make == "root": (self.config / "m1").rmdir()
                    elif make == "territory": self.decl.rmdir()
                    elif make == "stack": shutil.move(self.root / "stack.aside", self.stack)
                    elif make == "marker": marker.unlink()
                self.assertNotEqual(result.returncode, 0, result.stdout)
                self.assertIn("REFUSED: ", result.stderr)
                self.assertIn(said, result.stderr)
                self.assertFalse([c for c in self.calls() if c[0] == "sudo"], self.calls())
        # **Every archive root writes is written under umask 077**, so the
        # territory's custodied files are not exposed by their archive (the
        # commit security review of 36f1374). Perturbation: drop the umask
        # from either tar.
        script = (self.repo / "deploy" / "verify-lifecycle.sh").read_text()
        writes = [l for l in script.splitlines() if "sudo -n" in l and " -cpf " in l]
        self.assertEqual(len(writes), 2, writes)
        for line in writes:
            self.assertIn("umask 077 && exec tar", line)
        # Beside another agent (the fixture's `existing`) the run passes every
        # refusal, no step touching another agent, and asks for sudo first,
        # and nothing else privileged.
        self.env.update(ALLOW_APPLY_CHECKS="1", SUDO_FAIL="1")
        self.log.unlink(missing_ok=True)
        result = self.lifecycle(*apply)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("--apply needs sudo", result.stderr)
        self.assertEqual([c for c in self.calls() if c[0] == "sudo"], [["sudo", "-v"]])

    def test_lifecycle_cleanup_takes_down_only_an_agent_it_made(self):
        # --cleanup is step 9 alone, plan by default, and refuses an agent
        # without the marker step 1 writes, or whose root names a territory
        # not its own; it never runs decommission.sh and names no other
        # agent. Perturbation: drop the marker check, and the first case
        # fails.
        result = self.lifecycle("--cleanup")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("verify-lifecycle.sh did not create m1", result.stderr)
        marker = self.stack / "verify-lifecycle-m1"
        marker.write_text('agent = "other"\n')
        self.assertIn("did not create m1", self.lifecycle("--cleanup").stderr)
        marker.write_text('agent = "m1"\nmade-by = "deploy/verify-lifecycle.sh"\n')
        result = self.lifecycle("--cleanup")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("== step 9 (HowToDeployANewAgent.md section 7):", result.stdout)
        self.assertNotIn("== step 1 (", result.stdout)
        commands = [l for l in result.stdout.splitlines() if l.startswith("   $ ")]
        self.assertTrue(commands)
        self.assertFalse([l for l in commands if "decommission" in l or "existing" in l], commands)
        self.assertIn("userdel -r weaver-m1;", result.stdout)
        self.assert_unprivileged()
        (self.config / "m1").mkdir()
        (self.config / "m1" / "territory").write_text(str(self.existing_territory) + "\n")
        result = self.lifecycle("--cleanup")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("removes only the territory named for its agent", result.stderr)


    def test_lifecycle_cleanup_removes_only_step_8s_staged_key(self):
        """**--cleanup removes a `declaration-directory` key only where it is
        step 8's** (#99, from #105's Codex round 2): a regular file naming
        exactly the territory, trimmed, on a root the marker names, lets the
        take-down's plan go on and names the removal; any other content, or
        a key that is not a regular file, refuses by name. Perturbation: drop
        the content check and the foreign key is planned for removal."""
        marker = self.stack / "verify-lifecycle-m1"
        marker.write_text('agent = "m1"\nmade-by = "deploy/verify-lifecycle.sh"\n')
        root = self.config / "m1"
        root.mkdir()
        key = root / "declaration-directory"
        territory = (self.root / "agents").resolve() / "weaver-m1"
        key.write_text(f"  {territory}\n")
        result = self.lifecycle("--cleanup")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn(f"   $ rm -f {root}/declaration-directory (step 8's staged key naming {territory}",
                      result.stdout)
        self.assertLess(result.stdout.index("declaration-directory (step 8's"),
                        result.stdout.index("weaver-admin unload m1"))
        self.assert_unprivileged()
        key.write_text("/somewhere/else\n")
        result = self.lifecycle("--cleanup")
        self.assertEqual(result.returncode, 1, result.stdout)
        self.assertIn(f"{key} names /somewhere/else, not {territory}, the value step 8 stages", result.stderr)
        key.unlink()
        key.mkdir()
        result = self.lifecycle("--cleanup")
        self.assertEqual(result.returncode, 1, result.stdout)
        self.assertIn("is not a regular file, so it is not the key step 8 stages", result.stderr)
        key.rmdir()
        result = self.lifecycle("--cleanup")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotIn("step 8's staged key", result.stdout)

    def test_the_driver_line_prints_no_stray_none(self):
        """**The driver's fallback is applied after the pipeline** (#39): a
        `head` that closes the pipe on a card list fails the pipeline under
        `pipefail`, and the old `|| echo none` inside the substitution
        printed the card and then `none`. Each script's line is run with a
        stand-in nvidia-smi that writes after `head` has gone, and with none.
        Perturbation: put `|| echo none` back inside any of the three."""
        lines = {}
        for name, var in (("bootstrap-stack.sh", "DRIVER"), ("update-stack.sh", "DRIVER"),
                          ("decommission.sh", "driver")):
            text = (DEPLOY / name).read_text()
            line = next(l.strip() for l in text.splitlines() if l.strip().startswith(f"{var}=$(nvidia-smi"))
            self.assertNotIn("echo none", line)
            lines[name] = (line, var)
        with tempfile.TemporaryDirectory() as scratch:
            bin_dir = Path(scratch)
            smi = bin_dir / "nvidia-smi"
            smi.write_text("#!/bin/sh\necho 'card0, 1'\nsleep 0.3\necho 'card1, 1'\necho 'card2, 1'\n")
            smi.chmod(0o755)
            # The box's own nvidia-smi is never run: the second case puts one
            # first on the path that answers nothing, as a box without a card.
            silent = bin_dir / "silent"
            silent.mkdir()
            (silent / "nvidia-smi").write_text("#!/bin/sh\nexit 9\n")
            (silent / "nvidia-smi").chmod(0o755)
            for name, (line, var) in lines.items():
                for path, want in ((f"{bin_dir}{os.pathsep}/usr/bin:/bin", "card0, 1"),
                                   (f"{silent}{os.pathsep}/usr/bin:/bin", "none")):
                    with self.subTest(script=name, want=want):
                        run = subprocess.run(["bash", "-c", f'set -euo pipefail\n{line}\nprintf "%s\\n" "${{{var}:-none}}"'],
                                             env={"PATH": path}, text=True, capture_output=True, timeout=20)
                        self.assertEqual(run.stdout, want + "\n", run.stderr)

    def test_turn_py_runs_as_the_runbooks_write_it(self):
        """`deploy/turn.py <agent>` is how CLAUDE.md and the runbooks run it
        (#39), so it is executable on disk and in the index. Perturbation:
        `chmod -x`, or the index's mode back to 100644."""
        self.assertTrue(os.access(DEPLOY / "turn.py", os.X_OK))
        self.assertTrue((DEPLOY / "turn.py").read_text().startswith("#!/usr/bin/env python3\n"))
        if (DEPLOY.parent / ".git").exists():
            staged = subprocess.run(["git", "-C", str(DEPLOY.parent), "ls-files", "-s", "deploy/turn.py"],
                                    text=True, capture_output=True, timeout=20).stdout
            self.assertTrue(staged.startswith("100755 "), staged)

    def test_the_runbook_takes_a_fresh_login_and_names_the_turn(self):
        """REDEPLOY.md, as #39 asks: a fresh login after the purge and before
        the build, and a line that verify-load proves a load while turn.py
        proves a turn. Perturbation: drop either and this fails."""
        text = (DEPLOY / "REDEPLOY.md").read_text()
        purge, build, agents = (text.index(h) for h in ("## 2. Purge", "## 3. Build", "## 4. Agents"))
        login = text.index("**Then log out and log in again**")
        self.assertLess(purge, login)
        self.assertLess(login, build)
        turn = text.index('`verify-load.sh` proves a load; `deploy/turn.py <name> "<text>"` proves a turn')
        self.assertLess(agents, turn)
        self.assertNotIn("bulk-store/dev-archive", text)


def admin_retired_root_keys():
    """The root keys weaver-admin refuses by name, read from its
    `RETIRED_ROOT_KEYS` so test_plans compares the scripts with admin's own
    list."""
    main = (DEPLOY.parent / "crates" / "weaver-admin" / "src" / "main.rs").read_text()
    block = main[main.index("const RETIRED_ROOT_KEYS"):]
    block = block[:block.index("];")]
    keys = set(re.findall(r'\(\s*"([a-z.-]+)",', block))
    assert len(keys) == 7, keys
    return keys


def lifecycle_functions(script, *names):
    """verify-lifecycle.sh's functions as the script defines them, a
    one-line definition as its line and any other to its closing brace."""
    lines = script.splitlines()
    out = []
    for name in names:
        start = next(i for i, l in enumerate(lines) if l.startswith(f"{name}()"))
        if lines[start].rstrip().endswith("}"):
            out.append(lines[start])
        else:
            end = next(i for i in range(start, len(lines)) if lines[i] == "}")
            out.extend(lines[start:end + 1])
    return "\n".join(out) + "\n"


class LifecycleTeardownTests(unittest.TestCase):
    """**The take-down's unload is admin's answer, unload else force-unload**:
    a root admin refuses `config_invalid` at every verb, even one holding the
    `declaration-directory` of an older layout with no process of the agent
    running, stops the take-down, there being no staged layout this script
    tolerates any more (the operator's ruling of 2026-10-08 on #1).
    `unload_for_teardown` is run alone with admin and pgrep as stand-ins.
    Perturbation: put back the branch that went on past `config_invalid` where
    the root stood staged and no process ran, and the take-down goes on."""

    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory(prefix="weaver-teardown-")
        self.addCleanup(self.scratch.cleanup)
        root = Path(self.scratch.name)
        self.bin_dir = root / "bin"
        self.bin_dir.mkdir()
        sudo = self.bin_dir / "sudo"
        sudo.write_text('#!/bin/sh\n[ "$1" = "-n" ] && shift\nexec env "$@"\n')
        sudo.chmod(0o755)
        admin = self.bin_dir / "weaver-admin"
        admin.write_text("#!/bin/sh\nprintf '%s\\n' '{\"kind\":\"config_invalid\"}'\n")
        admin.chmod(0o755)
        pgrep = self.bin_dir / "pgrep"
        pgrep.write_text("#!/bin/sh\nexit 1\n")
        pgrep.chmod(0o755)
        self.root_dir = root / "m1"
        self.root_dir.mkdir()
        (self.root_dir / "declaration-directory").write_text("/old\n")
        script = (DEPLOY / "verify-lifecycle.sh").read_text()
        self.program = ("set -euo pipefail\n"
                        + 'STEP_LABEL="step 9 (test)"; A=m1; AU=weaver-m1\n'
                        + f'R={shlex.quote(str(self.root_dir))}; ADMIN_BASE=/nonexistent; ADMIN={shlex.quote(str(admin))}\n'
                        + lifecycle_functions(script, "fail", "pass", "measured", "expect_eq", "json_at",
                                              "answered_state", "ask", "expect_state", "expect_kind",
                                              "expect_unloaded_alone", "unload_for_teardown")
                        + "unload_for_teardown\necho GOES-ON\n")

    def test_a_refused_force_unload_stops_the_take_down(self):
        env = {**os.environ, "PATH": str(self.bin_dir) + os.pathsep + os.environ["PATH"]}
        env.pop("BASH_ENV", None)
        result = subprocess.run(["bash", "-c", self.program], env=env, text=True,
                                capture_output=True, timeout=20)
        self.assertEqual(result.returncode, 1, result.stdout)
        self.assertIn("force-unload answers the unloaded state", result.stderr)
        self.assertIn("found:    {\"kind\":\"config_invalid\"}", result.stderr)
        self.assertNotIn("GOES-ON", result.stdout)


class LifecycleNearMissTests(unittest.TestCase):
    """**verify-lifecycle.sh's near misses put back what they changed**, the
    exit trap included, whatever admin answers: a near miss is run alone on a
    scratch file, with sudo and admin as stand-ins. Perturbations: drop the
    restore from `on_exit` and the interrupted case leaves 0711; drop it from
    both `near_miss` and `on_exit` and the failed case does too."""

    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory(prefix="weaver-lifecycle-")
        self.addCleanup(self.scratch.cleanup)
        root = Path(self.scratch.name)
        self.bin_dir = root / "bin"
        self.bin_dir.mkdir()
        sudo = self.bin_dir / "sudo"
        sudo.write_text('#!/bin/sh\n[ "$1" = "-n" ] && shift\nexec env "$@"\n')
        sudo.chmod(0o755)
        self.admin = self.bin_dir / "weaver-admin"
        self.target = root / "territory"
        self.target.mkdir()
        self.target.chmod(0o710)
        script = (DEPLOY / "verify-lifecycle.sh").read_text()
        traps = [l for l in script.splitlines() if l.startswith("trap ")]
        self.program = ("set -euo pipefail\n"
                        + 'STEP_LABEL="step 1b (test)"; RESTORE=(); CREATED=0; LOGDIR=""; A=m1\n'
                        + f'ADMIN_BASE=/nonexistent; ADMIN={shlex.quote(str(self.admin))}\n'
                        + "export MAIN_PID=$$\n"
                        + lifecycle_functions(script, "fail", "pass", "expect_eq", "json_at", "ask",
                                              "expect_kind", "restore_mode", "restore_now",
                                              "near_miss", "on_exit")
                        + "\n".join(traps) + "\n"
                        + 'near_miss "the territory at 0711" restore_mode "$1" 710 -- sudo -n chmod 0711 -- "$1"\n'
                        + 'echo FINISHED\n')

    def run_near_miss(self, admin_body):
        self.admin.write_text("#!/bin/sh\n" + admin_body)
        self.admin.chmod(0o755)
        env = {**os.environ, "PATH": str(self.bin_dir) + os.pathsep + os.environ["PATH"]}
        env.pop("BASH_ENV", None)
        return subprocess.run(["bash", "-c", self.program, "x", str(self.target)], env=env,
                              text=True, capture_output=True, timeout=20)

    def mode(self):
        return self.target.stat().st_mode & 0o7777

    def test_the_answer_owed_passes_and_the_mode_is_back(self):
        result = self.run_near_miss("""printf '%s\\n' '{"kind":"boundary_unverified"}'\n""")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("PASS step 1b (test): the territory at 0711: validate answers boundary_unverified", result.stdout)
        self.assertIn("FINISHED", result.stdout)
        self.assertEqual(self.mode(), 0o710)

    def test_a_failed_check_still_puts_the_change_back(self):
        result = self.run_near_miss("""printf '%s\\n' '{"kind":"validated"}'\n""")
        self.assertEqual(result.returncode, 1, result.stdout)
        self.assertIn("FAIL step 1b (test)", result.stderr)
        self.assertIn("expected: boundary_unverified", result.stderr)
        self.assertIn("found:    validated", result.stderr)
        self.assertEqual(self.mode(), 0o710)

    def test_a_run_stopped_before_the_answer_is_put_back_by_the_trap(self):
        # The run is interrupted while admin holds the verb: the change was
        # made and the step's own restore never reached.
        result = self.run_near_miss('kill -TERM "$MAIN_PID"\nsleep 1\n')
        self.assertEqual(result.returncode, 130, result.stdout + result.stderr)
        self.assertNotIn("FINISHED", result.stdout)
        self.assertEqual(self.mode(), 0o710)


def shell_function(script, name):
    """One function exactly as a deploy script defines it, read out of the
    script's own text so a test runs the code that ships."""
    lines = script.splitlines()
    start = lines.index(f"{name}() {{")
    end = next(i for i in range(start, len(lines)) if lines[i] == "}")
    return "\n".join(lines[start:end + 1]) + "\n"


class DecommissionTests(unittest.TestCase):
    """decommission.sh's reading of the per-agent layout, run as the script
    defines it. It needs root to run whole, so its pieces are run alone."""

    def setUp(self):
        self.script = (DEPLOY / "decommission.sh").read_text()
        self.bin = tempfile.TemporaryDirectory(prefix="weaver-decommission-")
        self.addCleanup(self.bin.cleanup)
        stat = Path(self.bin.name) / "stat"
        stat.write_text(DOUBLE)
        stat.chmod(0o755)

    def stat_env(self, fixture, **extra):
        """The territory's look as root takes it (#99 area 2 review, H4): the
        stat stand-in answers uid 0 under FIXTURE and, unless LAYOUT_GROUPS
        says otherwise, the group the law names."""
        env = {**os.environ, "PATH": self.bin.name + os.pathsep + os.environ["PATH"],
               "FIXTURE_ROOT": str(fixture), "CALLS": os.devnull, **extra}
        for name in ("BASH_ENV", "LAYOUT_GROUPS", "OP_HOME"):
            if name not in extra:
                env.pop(name, None)
        return env

    def run_fn(self, name, *args, env=None):
        run = subprocess.run(["bash", "-c", shell_function(self.script, name) + f'{name} "$@"', "x", *args],
                             env=env, text=True, capture_output=True, timeout=20)
        return run.stdout.strip()

    def test_the_sudo_rules_found_include_a_held_install_rule(self):
        # **A rule an install held and never put back is found** (#107): a
        # SIGKILLed update-stack leaves `.weaver-<agent>.updating`, which the
        # archive and the purge then take as they take a disabled rule, and
        # never a file of another name. Perturbation: drop the `.updating`
        # name from the find, and the held rule is left behind.
        scratch = tempfile.TemporaryDirectory(prefix="weaver-sudo-rules-")
        self.addCleanup(scratch.cleanup)
        rules = Path(scratch.name) / "sudoers.d"
        rules.mkdir()
        for name in ("weaver-a", ".weaver-b.decommissioning", ".weaver-c.updating",
                     "other-rule", ".weaver-d.staged"):
            (rules / name).write_text("x\n")
        found = self.run_fn("sudo_rules", str(rules)).splitlines()
        self.assertEqual(sorted(Path(f).name for f in found),
                         [".weaver-b.decommissioning", ".weaver-c.updating", "weaver-a"])
        script = (DEPLOY / "decommission.sh").read_text()
        self.assertIn("mapfile -t SUDO_RULES < <(sudo_rules /etc/sudoers.d)", script)

    def test_a_run_is_stopped_only_where_show_says_so(self):
        # A running, transitioning or unreadable agent refuses the archive and
        # the purge, never read as stopped. Perturbation: read a refusal, or an
        # unloaded state with constituents, as stopped, and this fails.
        for answer, verdict in (
                ('{"kind":"state","state":"unloaded"}', "stopped"),
                ('{"kind":"state","state":"absent"}', "stopped"),
                ('{"kind":"no_such_agent"}', "stopped"),
                ('{"kind":"state","state":"idle","constituents":[7]}', "running"),
                ('{"kind":"state","state":"unloaded","constituents":[7]}', "running"),
                ('{"kind":"state","state":"active"}', "running"),
                ('{"kind":"in_transition"}', "running"),
                ('{"kind":"boundary_unverified"}', "unknown"),
                ("", "unknown"),
                ("not json", "unknown")):
            with self.subTest(answer=answer):
                self.assertEqual(self.run_fn("run_verdict", answer), verdict)

    def test_every_provisioned_name_strips_to_its_agent(self):
        # Every account and group create-agent makes carries a reserved suffix,
        # so each names its agent. Perturbation: drop a suffix, and its account
        # is taken for an agent of its own.
        for name in ("weaver-m1", "weaver-m1-state", "weaver-m1-relay", "weaver-m1-admincon",
                     "weaver-m1-trace", "weaver-m1-admin"):
            with self.subTest(name=name):
                self.assertEqual(self.run_fn("strip_suffix", name), "m1")

    def test_the_agent_is_archived_whole_with_its_territory_and_the_home_untouched(self):
        # The operator's ruling of 2026-10-07 on #1: the declaration, the
        # draft, the logs and the save points live in the territory, which
        # the territories' archive takes whole, and the operator's home is
        # never read, archived or purged. Perturbation: archive or purge a
        # path under the home again, and this fails.
        self.assertIn('archive_path "$(archive_name territories "$d")" "$d"', self.script)
        self.assertNotIn("declaration-directories", self.script)
        self.assertNotIn("DECL_DIRS", self.script)
        self.assertFalse([l for l in self.script.splitlines()
                          if ".weaveragent" in l and not l.lstrip().startswith("#")],
                         "no line but a comment names the operator's directory")
        self.assertIn('read_key "$r" territory', self.script)

    def test_each_roots_own_territory_outside_the_bases_is_archived(self):
        # A root names its territory; one outside every base is archived by
        # that path, one under a base rides the base's archive, and a root
        # without the key (the pre-ruling layout) is covered by the bases as
        # before (Codex on #94, round 8). Perturbation: drop the base check
        # and the covered territory prints twice over; drop the key read and
        # the custom one is never archived.
        with tempfile.TemporaryDirectory() as scratch:
            base = Path(scratch) / "var-lib-weaver-agent"
            under = base / "weaver-a"
            custom = Path(scratch) / "srv-elsewhere" / "weaver-b"
            for d in (under, custom):
                d.mkdir(parents=True)
            roots = Path(scratch) / "admin"
            for name, territory in (("a", under), ("b", custom), ("c", None)):
                root = roots / name
                root.mkdir(parents=True)
                if territory is not None:
                    (root / "territory").write_text(f"{territory}\n")
            listed = self.run_fn("territories_outside", str(base), "--",
                                 str(roots / "a"), str(roots / "b"), str(roots / "c"),
                                 env=self.stat_env(scratch))
            self.assertEqual(listed.splitlines(), [str(custom)])
            # Canonical, not lexical (Codex on #94, round 12): a territory
            # written as `<base>/../elsewhere` is outside the base, and one
            # written with a dotted path inside it is covered. Perturbation:
            # compare the strings as written and the first lists nothing.
            dotted_out = f"{base}/../srv-elsewhere/weaver-b"
            dotted_in = f"{base}/../{base.name}/weaver-a"
            dotted = Path(scratch) / "admin-dotted"
            for name, territory in (("b", dotted_out), ("a", dotted_in)):
                root = dotted / name
                root.mkdir(parents=True)
                (root / "territory").write_text(f"{territory}\n")
            listed = self.run_fn("territories_outside", str(base), "--",
                                 str(dotted / "b"), str(dotted / "a"), env=self.stat_env(scratch))
            self.assertEqual(listed.splitlines(), [str(custom.resolve())])
        self.assertIn('for d in "${OWN_TERRITORIES[@]}"; do archive_path "$(archive_name territory "$d")" "$d"; done',
                      self.script)
        self.assertIn('OWN_LISTED=$(territories_outside', self.script)
        self.assertIn('mapfile -t OWN_TERRITORIES <<< "$OWN_LISTED"', self.script)

    def test_a_territory_key_not_naming_its_agent_is_never_purged(self):
        """**A root's `territory` key is listed for the archive and the purge
        only where it names `weaver-<agent>`** (#99 area 2 review, H4): a key
        mistyped as a parent directory (`/var/lib`) is said and skipped, never
        put on the purge list. Perturbation: drop the name check and the
        stand-in `/var/lib` is listed."""
        with tempfile.TemporaryDirectory() as scratch:
            base = Path(scratch) / "var-lib-weaver-agent"
            base.mkdir()
            wrong = Path(scratch) / "var-lib"
            wrong.mkdir()
            other = Path(scratch) / "srv" / "weaver-b"
            other.mkdir(parents=True)
            roots = Path(scratch) / "admin"
            for name, territory in (("a", wrong), ("c", other)):
                (roots / name).mkdir(parents=True)
                (roots / name / "territory").write_text(f"{territory}\n")
            run = subprocess.run(["bash", "-c", shell_function(self.script, "territories_outside")
                                  + 'territories_outside "$@"', "x", str(base), "--",
                                  str(roots / "a"), str(roots / "c")],
                                 env=self.stat_env(scratch), text=True, capture_output=True, timeout=20)
            self.assertEqual(run.stdout, "", "neither key names its own agent's territory")
            self.assertIn(f"{roots / 'a'}/territory names {wrong.resolve()}, not weaver-a", run.stderr)
            self.assertIn(f"{roots / 'c'}/territory names {other.resolve()}, not weaver-c", run.stderr)

    def test_a_territory_not_roots_and_its_state_groups_refuses(self):
        """**Every root's territory is judged by owner and group before the
        archive and the purge** (#99 area 2 review, H4 remainder): one not
        uid 0's, or not grouped `weaver-<agent>-state`, is named with what
        was found and the function answers 1, which refuses the archive and
        the purge; one standing so is listed. A territory under the
        operator's home is skipped before it is judged. The look is the
        stat stand-in's, root for anything under the fixture.
        Perturbation: drop the owner and group look and the misgrouped and
        foreign territories are listed with an answer of 0."""
        with tempfile.TemporaryDirectory() as scratch:
            scratch = Path(scratch).resolve()
            fixture = scratch / "fixture"
            good = fixture / "srv" / "weaver-a"
            grouped = fixture / "srv" / "weaver-b"
            foreign = scratch / "elsewhere" / "weaver-c"
            home = scratch / "home-op"
            homed = home / ".weaveragent" / "weaver-d"
            for d in (good, grouped, foreign, homed):
                d.mkdir(parents=True)
            base = fixture / "var-lib-weaver-agent"
            base.mkdir()
            roots = fixture / "admin"
            for name, territory in (("a", good), ("b", grouped), ("c", foreign), ("d", homed)):
                (roots / name).mkdir(parents=True)
                (roots / name / "territory").write_text(f"{territory}\n")
            env = self.stat_env(fixture, LAYOUT_GROUPS=json.dumps({str(grouped): "weaver-b-admin"}),
                                OP_HOME=str(home))
            run = subprocess.run(["bash", "-c", shell_function(self.script, "territories_outside")
                                  + 'territories_outside "$@"', "x", str(base), "--",
                                  *(str(roots / n) for n in "abcd")],
                                 env=env, text=True, capture_output=True, timeout=20)
            self.assertEqual(run.returncode, 1, run.stderr)
            self.assertEqual(run.stdout.splitlines(), [str(good)], run.stderr)
            self.assertIn(f"refused: {roots / 'b'}/territory names {grouped}, which stands uid 0, "
                          f"group weaver-b-admin, mode 755, not root:weaver-b-state", run.stderr)
            self.assertIn(f"refused: {roots / 'c'}/territory names {foreign}, which stands uid {os.getuid()}",
                          run.stderr)
            self.assertIn(f"skipped: {roots / 'd'}/territory names {homed}, under the operator's home",
                          run.stderr)
            ok = subprocess.run(["bash", "-c", shell_function(self.script, "territories_outside")
                                 + 'territories_outside "$@"', "x", str(base), "--", str(roots / "a")],
                                env=env, text=True, capture_output=True, timeout=20)
            self.assertEqual((ok.returncode, ok.stdout.splitlines()), (0, [str(good)]), ok.stderr)
        # The judgment refuses both modes, after the plan and before either acts.
        refusal = self.script.index('[ "$TERRITORIES_JUDGED" = 1 ] || die ')
        self.assertLess(self.script.index('[ "$MODE" = plan ] && { say "plan only.'), refusal)
        self.assertLess(refusal, self.script.index('if [ "$MODE" = archive ]; then'))
        self.assertIn("territories_outside \"${!TERRITORY_BASES[@]}\" -- \"${AGENT_ROOTS[@]}\") || TERRITORIES_JUDGED=0",
                      self.script)

    def test_the_archive_and_the_purge_name_their_directory(self):
        """**No archive directory is assumed** (#39): `--archive` or `--purge`
        without a directory refuses, asking for one, before anything is read,
        and the script names no box-specific default. Perturbation: put back
        the default and `--archive` alone goes on to the root check."""
        for mode, said in (("--archive", "--archive needs a directory to write the archive to"),
                           ("--purge", "--purge needs the directory --archive wrote")):
            with self.subTest(mode=mode):
                env = {k: v for k, v in os.environ.items() if k != "BASH_ENV"}
                run = subprocess.run(["bash", str(DEPLOY / "decommission.sh"), mode], env=env,
                                     text=True, capture_output=True, timeout=20)
                self.assertEqual(run.returncode, 1, run.stdout)
                self.assertIn(said, run.stderr)
        self.assertNotIn("bulk-store/dev-archive", self.script)
        self.assertIn("#   sudo deploy/decommission.sh --archive DIR ", self.script)

    def test_nothing_under_the_operators_home_is_listed(self):
        """**The operator's home is never listed, archived or purged** (the
        operator's ruling of 2026-10-07 on #1): the box-wide layout's
        `agent-config-directory` is not read, and a territory base or a log
        directory under the home (the stack record's `agent-directory`
        defaulted to `~/.weaveragent` from 2026-10-02 to 2026-10-07) is said
        and dropped. Perturbations: read the key again; drop the base's
        filter and a base under the home is kept."""
        code = [l for l in self.script.splitlines() if not l.lstrip().startswith("#")]
        self.assertFalse([l for l in code if "agent-config-directory" in l or "AGENT_DIRS" in l])
        self.assertNotIn('archive_name agent-config', self.script)
        with tempfile.TemporaryDirectory() as scratch:
            home = Path(scratch).resolve() / "home-op"
            (home / ".weaveragent").mkdir(parents=True)
            other = Path(scratch).resolve() / "var-lib-weaver-agent"
            other.mkdir()
            program = (shell_function(self.script, "in_operator_home")
                       + "plan() { printf '%s\\n' \"$*\"; }\n"
                       + f'OP_HOME={shlex.quote(str(home))}\n'
                       + f'declare -A TERRITORY_BASES=([{shlex.quote(str(home / ".weaveragent"))}]=1 '
                       + f'[{shlex.quote(str(other))}]=1)\ndeclare -A LOG_PATHS=()\n'
                       + self.script[self.script.index('for d in "${!TERRITORY_BASES[@]}"; do\n  in_operator_home'):
                                     self.script.index('for d in /var/lib/weaver "${!TERRITORY_BASES[@]}"')]
                       + 'printf "kept %s\\n" "${!TERRITORY_BASES[@]}"')
            run = subprocess.run(["bash", "-c", program], text=True, capture_output=True, timeout=20)
            self.assertIn(f"{home / '.weaveragent'}  under the operator's home: not touched", run.stdout, run.stderr)
            self.assertEqual([l for l in run.stdout.splitlines() if l.startswith("kept")], [f"kept {other}"])

    def test_a_running_agent_refuses_the_archive_and_the_purge(self):
        guard = '[ ${#RUNNING[@]} -eq 0 ] || die "agents still run or cannot be read'
        self.assertEqual(self.script.count(guard), 2)
        archive, purge = (self.script.index(m) for m in ('if [ "$MODE" = archive ]; then', " 3. purge\n"))
        first, second = (i for i in range(len(self.script)) if self.script.startswith(guard, i))
        self.assertLess(archive, first)
        self.assertLess(purge, second)


class RoundOneOf79Tests(unittest.TestCase):
    """Codex's first pass on #79: the connector probe, the purge's fresh look,
    and the credential before every install path, each run or read as the
    scripts define them."""

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="weaver-79-")
        self.addCleanup(self.tmp.cleanup)
        self.dir = Path(self.tmp.name)

    def test_the_connector_probe_succeeds_only_on_the_owed_answer(self):
        # A denied nested sudo, a refusal, or a wrong kind fails the probe, and
        # create-agent then refuses. Perturbation: restore `|| true` as the
        # probe's verdict, and the denied case passes.
        script = (DEPLOY / "create-agent.sh").read_text()
        sudo = self.dir / "sudo"
        sudo.write_text('#!/bin/sh\nprintf "%s\\n" "$PROBE_ANSWER"\n')
        sudo.chmod(0o755)
        program = shell_function(script, "probe_connector") + 'probe_connector "$1"'
        env = {**os.environ, "PATH": f"{self.dir}{os.pathsep}{os.environ['PATH']}",
               "CONNECTOR_USER": "weaver-m1-admincon", "ADMIN_BINARY": "/opt/weaver/bin/weaver-admin",
               "NAME": "m1"}
        env.pop("BASH_ENV", None)
        for verb, answer, passes in (("validate", '{"kind":"validated"}', True),
                                     ("show", '{"kind":"state","state":"unloaded"}', True),
                                     ("validate", "sudo: a password is required", False),
                                     ("validate", '{"kind":"boundary_unverified"}', False),
                                     ("show", '{"kind":"validated"}', False)):
            with self.subTest(verb=verb, answer=answer):
                run = subprocess.run(["bash", "-c", program, "x", verb], env={**env, "PROBE_ANSWER": answer},
                                     text=True, capture_output=True, timeout=20)
                self.assertEqual(run.returncode == 0, passes, run.stdout + run.stderr)
                self.assertEqual(run.stdout, answer)
        self.assertIn("if answer=$(probe_connector", script)
        self.assertNotIn('"$check_verb" "$NAME" 2>&1) || true', script)

    def test_the_purge_shuts_the_delegated_door_and_asks_again(self):
        # The purge removes the sudo rules and queries every agent afresh before
        # its first destructive step, never trusting the discovery-time answer.
        # Perturbations: drop the second query, or move it after the units
        # step, and this fails.
        script = (DEPLOY / "decommission.sh").read_text()
        purge = script[script.index(" 3. purge\n"):]
        door = purge.index('for f in "${SUDO_RULES[@]}"; do rm -f')
        again = purge.index("\nquery_runs\n")
        refusal = purge.index('die "agents run again or cannot be read')
        first_destructive = purge.index('say "units"')
        self.assertLess(door, again)
        self.assertLess(again, refusal)
        self.assertLess(refusal, first_destructive)

    def test_the_archive_shuts_the_delegated_door_and_asks_again(self):
        # The snapshot is taken with every rule disabled and every agent asked
        # afresh, and a run found puts the rules back and refuses before the
        # archive directory is touched. A disabled rule is found again by the
        # purge. Perturbations: drop the archive's query, or archive before it,
        # and this fails.
        script = (DEPLOY / "decommission.sh").read_text()
        archive = script[script.index('if [ "$MODE" = archive ]; then'):script.index(" 3. purge\n")]
        door = archive.index('held="$(dirname "$f")/.${f##*/}.decommissioning"')
        again = archive.index("\n  query_runs\n")
        restore = archive.index('mv -T -- "${entry%%|*}" "${entry##*|}"')
        refusal = archive.index('die "agents run or cannot be read')
        first_write = archive.index('as_op mkdir -p "$DEST"')
        self.assertLess(door, again)
        self.assertLess(again, restore)
        self.assertLess(restore, refusal)
        self.assertLess(refusal, first_write)
        self.assertIn("-name '.weaver-*.decommissioning'", script)

    def test_decommission_runs_only_an_admin_held_closed(self):
        # Codex on #79: the admin run as root is resolved and held closed, never
        # merely found executable. Perturbation: drop the judgment, and the
        # open directory's admin is picked.
        script = (DEPLOY / "decommission.sh").read_text()
        open_dir = self.dir / "open"
        closed_dir = self.dir / "closed"
        for d in (open_dir, closed_dir):
            d.mkdir()
            (d / "weaver-admin").write_text("#!/bin/sh\n")
            (d / "weaver-admin").chmod(0o755)
        open_dir.chmod(0o777)
        stat = self.dir / "stat"
        stat.write_text(DOUBLE)
        stat.chmod(0o755)
        program = ("plan() { :; }\n" + shell_function(script, "held_closed") + shell_function(script, "pick_admin")
                   + 'pick_admin "$@"')
        env = {**os.environ, "PATH": f"{self.dir}{os.pathsep}{os.environ['PATH']}", "FIXTURE_ROOT": str(self.dir),
               "CALLS": str(self.dir / "calls")}
        env.pop("BASH_ENV", None)
        try:
            run = subprocess.run(["bash", "-c", program, "x", str(open_dir / "weaver-admin"),
                                  str(closed_dir / "weaver-admin")], env=env, text=True, capture_output=True,
                                 timeout=20)
        finally:
            open_dir.chmod(0o755)
        self.assertEqual(run.stdout, str(closed_dir / "weaver-admin"), run.stderr)

    def test_decommission_archives_and_purges_every_territory_base(self):
        # Codex on #79: the per-agent layout's territories under the stack
        # record's agent-directory are archived, so they reach the purge list.
        # Perturbation: drop the loop, and they are left behind.
        script = (DEPLOY / "decommission.sh").read_text()
        archive = script[script.index('if [ "$MODE" = archive ]; then'):script.index(" 3. purge\n")]
        self.assertIn('archive_path "$(archive_name territories "$d")" "$d"', archive)
        self.assertIn("read_key /etc/weaver/stack agent-directory", script)

    def test_archive_names_never_collide(self):
        # Codex on #79: two sources sharing a last component name two archives.
        # Perturbation: name by the basename again, and the two collide.
        script = (DEPLOY / "decommission.sh").read_text()
        line = next(l.strip() for l in script.splitlines() if l.strip().startswith("archive_name() {"))
        names = []
        for path in ("/srv/weaver-agent", "/var/lib/weaver-agent"):
            run = subprocess.run(["bash", "-c", line + '\narchive_name territories "$1"', "x", path],
                                 text=True, capture_output=True, timeout=20)
            names.append(run.stdout)
        self.assertEqual(names, ["territories-srv-weaver-agent", "territories-var-lib-weaver-agent"])
        for sink in ("opt", "territories", "log"):
            self.assertIn(f'archive_path "$(archive_name {sink} ', script)
        self.assertNotIn('$(basename "$d")" "$d"', script)

    def test_no_archive_is_written_over_another(self):
        # Codex on #79: flattening is not injective, `/var/lib/weaver-agent` and
        # `/var/lib/weaver/agent` meeting at one name, so each archive takes a
        # name no archive in DEST holds yet. Perturbation: return the proposed
        # name unchecked, and the second source overwrites the first.
        script = (DEPLOY / "decommission.sh").read_text()
        line = next(l.strip() for l in script.splitlines() if l.strip().startswith("archive_name() {"))
        dest = self.dir / "dest"
        dest.mkdir()
        program = (line + "\n" + shell_function(script, "free_name")
                   + 'for p in "$@"; do n=$(free_name "$(archive_name territories "$p")"); touch "$DEST/$n.tar.zst"; echo "$n"; done')
        run = subprocess.run(["bash", "-c", program, "x", "/var/lib/weaver-agent", "/var/lib/weaver/agent",
                              "/var/lib/weaver-agent"], env={**os.environ, "DEST": str(dest)},
                             text=True, capture_output=True, timeout=20)
        self.assertEqual(run.stdout.split(), ["territories-var-lib-weaver-agent",
                                              "territories-var-lib-weaver-agent-2",
                                              "territories-var-lib-weaver-agent-3"], run.stderr)
        self.assertIn('name=$(free_name "$1"); shift', script)

    def test_query_runs_finds_a_run_by_its_accounts(self):
        # Codex on #79: a run whose root was lost is found by processes under
        # an agent's account. Perturbation: ask only the roots, and it is missed.
        script = (DEPLOY / "decommission.sh").read_text()
        pgrep = self.dir / "pgrep"
        pgrep.write_text('#!/bin/sh\n[ "$2" = "$LIVE_USER" ]\n')
        pgrep.chmod(0o755)
        program = ("plan() { :; }\n" + shell_function(script, "run_verdict") + shell_function(script, "query_runs")
                   + 'AGENT_ROOTS=(); ADMIN_BIN=""; WEAVER_USERS=(weaver-gone weaver-gone-state)\n'
                   + 'query_runs; echo "${#RUNNING[@]} ${RUNNING[*]}"')
        env = {**os.environ, "PATH": f"{self.dir}{os.pathsep}{os.environ['PATH']}", "LIVE_USER": "weaver-gone-state"}
        env.pop("BASH_ENV", None)
        run = subprocess.run(["bash", "-c", program], env=env, text=True, capture_output=True, timeout=20)
        self.assertEqual(run.stdout.strip(), "1 weaver-gone-state (processes)", run.stderr)

    def test_query_runs_reads_each_agent_now(self):
        # Stopped, then running, then stopped again, each read as it is now.
        # Perturbation: keep the last query's entries, and the third reads one.
        script = (DEPLOY / "decommission.sh").read_text()
        admin = self.dir / "weaver-admin"
        admin.write_text('#!/bin/sh\ncat "$STATE_FILE"\n')
        admin.chmod(0o755)
        state = self.dir / "state"
        program = ("plan() { :; }\n" + shell_function(script, "run_verdict") + shell_function(script, "query_runs")
                   + f'AGENT_ROOTS=({self.dir}/base/m1); ADMIN_BIN={admin}; WEAVER_USERS=()\n'
                   + 'query_runs; echo "${#RUNNING[@]}"; cp "$STATE_FILE" "$STATE_FILE.first"; cp "$NEXT" "$STATE_FILE"; '
                   + 'query_runs; echo "${#RUNNING[@]} ${RUNNING[*]}"; cp "$STATE_FILE.first" "$STATE_FILE"; '
                   + 'query_runs; echo "${#RUNNING[@]}"')
        state.write_text('{"kind":"state","state":"unloaded"}\n')
        nxt = self.dir / "next"
        nxt.write_text('{"kind":"state","state":"idle","constituents":[9]}\n')
        env = {**os.environ, "STATE_FILE": str(state), "NEXT": str(nxt)}
        env.pop("BASH_ENV", None)
        run = subprocess.run(["bash", "-c", program], env=env, text=True, capture_output=True, timeout=20)
        self.assertEqual(run.stdout.split("\n")[:3], ["0", "1 m1 (running)", "0"], run.stderr)

    def test_every_install_path_takes_the_credential_first(self):
        # A repair install with nothing changed still runs admin with `sudo -n`,
        # so the credential is taken before the install branch. Perturbation:
        # move `sudo -v` back inside the changed branch, and this fails.
        script = (DEPLOY / "update-stack.sh").read_text()
        take = script.index('sudo -v || die "--install needs sudo')
        self.assertLess(script.index('say "plan only. rerun with --install"; exit 0; }'), take)
        self.assertLess(take, script.index('if [ ${#CHANGED[@]} -gt 0 ]; then\n  say "install"'))
        self.assertEqual(script.count("sudo -v"), 1)


class BootstrapStandingTests(unittest.TestCase):
    """bootstrap-stack.sh never reads what it cannot see as absent or empty: its checks
    for a standing stack record, admin base and install, run as the script runs them,
    refuse where the look fails. Perturbation: read the base's listing with its error
    discarded, as before, and the unreadable base reads empty."""

    def check(self, base, stack):
        script = (DEPLOY / "bootstrap-stack.sh").read_text()
        start = script.index("standing() {")
        end = script.index('if standing "$PREFIX/bin"', start)
        body = 'die() { printf "REFUSED: %s\\n" "$*" >&2; exit 1; }\n'
        body += f"STACK={shlex.quote(str(stack))}; ADMIN_BASE={shlex.quote(str(base))}\n"
        body += script[start:end] + 'echo "passed"\n'
        return subprocess.run(["bash", "-c", body], capture_output=True, text=True)

    def test_an_unreadable_base_or_path_refuses(self):
        if os.geteuid() == 0:
            self.skipTest("no mode denies root; rerun as an unprivileged uid")
        tmp = Path(tempfile.mkdtemp())
        try:
            self.assertIn("passed", self.check(tmp / "admin", tmp / "stack").stdout)
            (tmp / "admin").mkdir()
            self.assertIn("passed", self.check(tmp / "admin", tmp / "stack").stdout)
            (tmp / "admin").chmod(0)
            result = self.check(tmp / "admin", tmp / "stack")
            self.assertIn("cannot list", result.stderr, result.stdout)
            (tmp / "admin").chmod(0o755)
            (tmp / "shut").mkdir()
            (tmp / "shut").chmod(0)
            result = self.check(tmp / "admin", tmp / "shut" / "stack")
            self.assertIn("cannot tell whether", result.stderr, result.stdout)
        finally:
            for p in (tmp / "admin", tmp / "shut"):
                if p.exists():
                    p.chmod(0o755)
            shutil.rmtree(tmp, ignore_errors=True)


STUB_ADMIN = """#!/bin/sh
printf '%s\\n' "boundary unverified: no weaver-state binary beside the worker's" >&2
printf '%s\\n' '{"kind":"refused","reason":"boundary_unverified"}'
exit 1
"""


def admin_answer_definition(script):
    """The `admin_answer` function exactly as the deploy script defines it.

    The function is read out of the script's own text and run alone, so the
    test exercises the code that ships rather than a copy of it, and the
    install steps before the call sites stay out of the fixture.
    """
    lines = script.splitlines()
    start = lines.index("admin_answer() {")
    end = next(i for i in range(start, len(lines)) if lines[i] == "}")
    return "\n".join(lines[start:end + 1]) + "\n"


def declared_definition(script):
    """The `declared` reader exactly as the deploy script defines it, read out
    of the script's own text so the test runs the code that ships."""
    lines = script.splitlines()
    start = lines.index("declared() {")
    end = next(i for i in range(start, len(lines)) if lines[i] == "}")
    return "\n".join(lines[start:end + 1]) + "\n"


class DeclaredTests(unittest.TestCase):
    """The one reader update-stack.sh takes a declaration's values through.
    Perturbation: put back the line-matching sed readers and the literal and
    escaped sink paths, the dotted engine and the inline store fail here."""

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.script = (Path(__file__).resolve().parent / "update-stack.sh").read_text()

    def tearDown(self):
        self.tmp.cleanup()

    def read(self, text, key, want):
        # The reader takes the document on stdin and the path only as a name
        # (Codex on #94, round 8): who opens the file is the caller's question.
        decl = Path(self.tmp.name) / "a.toml"
        decl.write_text(text)
        run = subprocess.run(["bash", "-c", declared_definition(self.script) + 'declared "$2" "$3" "$1" < "$1"', "x",
                              str(decl), key, want], text=True, capture_output=True)
        return run.returncode, run.stdout.rstrip("\n"), run.stderr

    def test_the_sink_path_is_the_decoded_string(self):
        for text, path in (('[trace-sink]\npath = \'/srv/a\\b "c".ndjson\'\n', '/srv/a\\b "c".ndjson'),
                           ('[trace-sink]\npath = "/srv/x\\u0041y.ndjson"\n', "/srv/xAy.ndjson"),
                           ('[trace-sink]\npath = "/srv/t.ndjson" # the sink\n', "/srv/t.ndjson"),
                           ('trace-sink = { kind = "file", path = "/srv/i.ndjson", create = true }\n',
                            "/srv/i.ndjson")):
            with self.subTest(text=text):
                self.assertEqual(self.read(text, "trace-sink.path", "string")[:2], (0, path))

    def test_the_store_election_reads_in_every_spelling(self):
        for text in ('[state-store]\nengine = "sqlite"\n', "[state-store]\nengine = 'sqlite'\n",
                     'state-store.engine = "sqlite"\n', 'state-store = { engine = "sqlite" }\n'):
            with self.subTest(text=text):
                self.assertEqual(self.read(text, "state-store.engine", "string")[:2], (0, "sqlite"))
                self.assertEqual(self.read(text, "state-store", "table")[0], 0)

    def test_absence_and_a_bad_file_answer_apart(self):
        self.assertEqual(self.read('session = "s"\n', "state-store.engine", "string")[0], 3)
        self.assertEqual(self.read('session = "s"\n', "state-store", "table")[0], 3)
        code, _, err = self.read('session = "s\n', "state-store", "table")
        self.assertEqual(code, 1)
        self.assertIn("is not a TOML 1.0 document", err)
        code, _, err = self.read('[state-store]\nengine = 3\n', "state-store.engine", "string")
        self.assertEqual(code, 1)
        self.assertIn("is not a string", err)


class AdminAnswerTests(unittest.TestCase):
    """**Admin's refusal cause survives the deploy script** (#673, was #676).

    Perturbation: restore `2>&1 | tail -1` in `admin_answer`, or route either
    call site back to it, and these fail, the cause no longer reaching
    stderr or the call site no longer reaching the helper. Watched under
    exactly those changes.
    """

    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory(prefix="weaver-admin-answer-")
        self.addCleanup(self.scratch.cleanup)
        root = Path(self.scratch.name)
        self.bin_dir = root / "bin"
        self.bin_dir.mkdir()
        admin = self.bin_dir / "weaver-admin"
        admin.write_text(STUB_ADMIN)
        admin.chmod(0o755)
        # `sudo -n VAR=value command args` runs the command with the
        # variable set, which `env` does without privilege.
        sudo = self.bin_dir / "sudo"
        sudo.write_text('#!/bin/sh\n[ "$1" = "-n" ] && shift\nexec env "$@"\n')
        sudo.chmod(0o755)
        self.script = (DEPLOY / "update-stack.sh").read_text()

    def answer(self, verb):
        program = ("set -euo pipefail\n" + admin_answer_definition(self.script)
                   + f'admin_answer {verb} m1\n')
        env = {**os.environ, "PATH": str(self.bin_dir) + os.pathsep + os.environ["PATH"],
               "BIN_DIR": str(self.bin_dir), "ADMIN_BASE": "/nonexistent"}
        env.pop("BASH_ENV", None)
        return subprocess.run(["bash", "-c", program], env=env, text=True,
                              capture_output=True, timeout=20)

    def test_a_refusal_keeps_its_cause_and_its_answer(self):
        for verb in ("validate", "load"):
            result = self.answer(verb)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(result.stdout,
                             '{"kind":"refused","reason":"boundary_unverified"}\n')
            self.assertIn("admin: boundary unverified: no weaver-state binary beside the worker's",
                          result.stderr)

    def test_both_call_sites_reach_the_helper(self):
        self.assertIn('validate() {\n  admin_answer validate "$1"\n}', self.script)
        self.assertIn('  verdict=$(admin_answer load "$AGENT")\n  if ! answered_state "$verdict" idle; then\n', self.script)
        admin_lines = [line for line in self.script.splitlines()
                       if "/weaver-admin\"" in line and not line.lstrip().startswith("#")]
        self.assertFalse([line for line in admin_lines if "tail -1" in line], admin_lines)


class AccessNoteTests(unittest.TestCase):
    """**turn.py names both groups reading the draft takes** (#99 area 2
    review, K10): the state group passes the territory and the access group
    reads the draft, so a session lacking either is told which. Perturbation:
    judge the access group alone again and a session holding it but not the
    state group is told its file's mode is wrong."""

    def note(self, held):
        import grp
        import importlib.util
        from unittest import mock
        spec = importlib.util.spec_from_file_location("turn_under_test", DEPLOY / "turn.py")
        turn = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(turn)
        gids = {"weaver-m1-state": 9001, "weaver-m1-admin": 9002}
        with mock.patch.object(grp, "getgrnam", lambda n: mock.Mock(gr_gid=gids[n])), \
                mock.patch.object(os, "getgroups", lambda: [gids[g] for g in held]), \
                mock.patch.object(os, "getegid", lambda: 1):
            return turn.access_note("m1")

    def test_each_missing_group_is_named(self):
        said = self.note(["weaver-m1-admin"])
        self.assertIn("does not hold weaver-m1-state, which passes the territory", said)
        self.assertNotIn("weaver-m1-admin, through", said)
        said = self.note(["weaver-m1-state"])
        self.assertIn("does not hold weaver-m1-admin, through which the draft is read", said)
        said = self.note([])
        self.assertIn("weaver-m1-state, which passes the territory or weaver-m1-admin", said)
        said = self.note(["weaver-m1-state", "weaver-m1-admin"])
        self.assertIn("holds weaver-m1-state and weaver-m1-admin", said)


class ShStandInTests(unittest.TestCase):
    """**Every `#!/bin/sh` stand-in this file writes is POSIX sh** (Codex on
    #94): a box whose `/bin/sh` is dash refuses a Bash-only form before the
    test it serves runs. No dash stands on olympus, so the stand-ins are
    judged here by their text: every string literal of this file that opens
    with the sh shebang, an f-string's literal parts joined, carries none of
    the Bash forms named. The count keeps the check from passing on nothing.
    Perturbation: put `${@: -1}` back in the sudo stand-in and this fails."""

    BASH_ONLY = ("${@:", "[[", "function ", "$(<")

    def test_every_sh_stand_in_is_posix(self):
        import ast
        tree = ast.parse(Path(__file__).read_text())
        inner = set()
        scripts = []
        for node in ast.walk(tree):
            if isinstance(node, ast.JoinedStr):
                parts = []
                for value in node.values:
                    if isinstance(value, ast.Constant):
                        inner.add(id(value))
                        parts.append(value.value)
                    else:
                        parts.append("{}")
                scripts.append((node.lineno, "".join(parts)))
        for node in ast.walk(tree):
            if (isinstance(node, ast.Constant) and isinstance(node.value, str)
                    and id(node) not in inner):
                scripts.append((node.lineno, node.value))
        # The needle is built from two parts so this check's own source holds
        # no literal it would count.
        shebang = "#!/bin/" + "sh\n"
        stand_ins = [(line, text) for line, text in scripts if text.startswith(shebang)]
        self.assertGreaterEqual(len(stand_ins), 14, [line for line, _ in stand_ins])
        for line, text in stand_ins:
            for form in self.BASH_ONLY:
                self.assertNotIn(form, text, f"the sh stand-in at line {line} carries {form!r}")


if __name__ == "__main__":
    unittest.main()
