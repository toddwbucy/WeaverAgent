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
    if args[:1] == ['-c'] and args[1] == '%u %a':
        path = pathlib.Path(args[-1])
        st = os.lstat(path)
        # The operator's home is the operator's, as the declaration
        # directory's judgment needs it to be.
        operator_home = root / 'home' / os.environ.get('USER', '')
        if path.is_relative_to(operator_home):
            owner = int(os.environ.get('FIXTURE_UID', '12345'))
        else:
            owner = 0 if path.is_relative_to(root) else st.st_uid
        print(owner, format(st.st_mode & 0o7777, 'o'))
        sys.exit(0)
    os.execv('/usr/bin/stat', ['stat', *args])
with open(os.environ['CALLS'], 'a') as log:
    log.write(json.dumps([name, *args]) + '\n')
def mapped(value):
    if value.startswith('/home/') or value.startswith('/etc/sudoers'):
        return str(root / value.lstrip('/'))
    return value

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
        elif who.endswith('-admincon'): print(os.environ.get('CONNECTOR_GROUPS', who + ' ' + who.removesuffix('con')))
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
    elif op == 'psql':
        query = rest[-1]
        if os.environ.get('READ_FAIL') and os.environ['READ_FAIL'] in query: sys.exit(2)
        if os.environ.get('EMPTY_PATH') and os.environ['EMPTY_PATH'] in query: sys.exit(0)
        if 'pg_roles' in query and os.environ.get('ROLE_COLLISION'): print('1')
        elif 'show hba_file' in query: print(root / 'pg_hba.conf')
        elif 'show ident_file' in query: print(root / 'pg_ident.conf')
        elif query == 'select 1': sys.exit(1 if identity == 'weaver-m1' else 0)
    elif op in ('grep', 'sed'):
        # Execute only the text operation on scratch files, never via sudo.
        file = pathlib.Path(mapped(rest[-1]))
        assert file.is_relative_to(root), file
        sys.exit(subprocess.run(['/usr/bin/' + op, *rest[:-1], str(file)]).returncode)
    elif op == 'tee':
        file = pathlib.Path(mapped(rest[-1]))
        assert file.is_relative_to(root), file
        with file.open('a' if '-a' in rest else 'w') as output: output.write(sys.stdin.read())
    elif op == 'cp':
        source, destination = (pathlib.Path(mapped(a)) for a in rest[-2:])
        assert source.is_relative_to(root) and destination.is_relative_to(root)
        shutil.copyfile(source, destination)
    elif op == 'install':
        target = pathlib.Path(mapped(rest[-1]))
        assert target.is_relative_to(root), target
        if '-d' in rest: target.mkdir(parents=True, exist_ok=True)
        else: target.touch()
    elif op == 'mv':
        source, destination = (pathlib.Path(mapped(a)) for a in rest[-2:])
        assert source.is_relative_to(root) and destination.is_relative_to(root)
        assert not destination.exists(), destination
        source.rename(destination)
    elif op == 'mktemp':
        template = rest[-1]
        made = pathlib.Path(mapped(template.replace('XXXXXX', 'fixture')))
        assert made.is_relative_to(root), made
        if '-d' in rest: made.mkdir()
        else: made.touch()
        print(template.replace('XXXXXX', 'fixture'))
    elif op == 'visudo': sys.exit(1 if os.environ.get('VISUDO_FAIL') else 0)
    elif op == 'rm':
        target = pathlib.Path(mapped(rest[-1]))
        assert target.is_relative_to(root), target
        target.unlink(missing_ok=True)
    elif op == '-l':
        print('User ' + rest[-1] + ' may run the following commands on fixture-box:')
        for rule in sorted((root / 'etc' / 'sudoers.d').glob('weaver-*')):
            print('    ' + rule.read_text().splitlines()[-1])
    elif op == 'test':
        # The probes of a sqlite agent's state room: the member passes, the
        # agent's own uid is refused, unless the fixture opens the wall.
        if identity == 'weaver-m1': sys.exit(0 if os.environ.get('WALL_OPEN') else 1)
        # The member's read of the trace: refused, unless the fixture opens it.
        if identity == 'weaver-m1-state' and '-r' in rest: sys.exit(0 if os.environ.get('TRACE_OPEN') else 1)
        sys.exit(0)
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
        # The operator's home, holding each agent's declaration directory.
        self.home = self.root / "home"
        self.operator_home = self.home / "fixture-no-home"
        self.operator_home.mkdir(parents=True)
        existing_decl = self.operator_home / ".weaveragent" / "existing"
        existing_decl.mkdir(parents=True)
        (existing_decl / "agent.toml").write_text("[state-store]\nengine = \"none\"\n")
        (existing / "declaration-directory").write_text(str(existing_decl) + "\n")
        self.decl = self.operator_home / ".weaveragent" / "m1"
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
        (self.root / "agents").mkdir()
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
        for name in ("BASH_ENV", "SUDO_USER", "COLLISION", "ALLOW_APPLY_CHECKS", "ROLE_COLLISION", "TRACE_OPEN", "BUILD_FAIL", "SUDO_FAIL", "READ_FAIL", "EMPTY_PATH", "ACL_FAIL", "PATH_FAIL", "ACCOUNT_FAIL", "WALL_OPEN",
                     "VISUDO_FAIL", "RELAY_GROUPS", "CONNECTOR_GROUPS", "UNITS", "UNITS_FAIL", "FIXTURE_ACCOUNT_UID",
                     "COLLISION_GROUP", "KEEP_ALIVE", "TRACE_GROUP_AS"):
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
        # No engine is the default, so a case that names none runs as sqlite,
        # named here rather than assumed by the script.
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

    def assert_unprivileged(self):
        # `systemctl list-units` is a look any user may take, and the only
        # systemctl call a plan makes.
        forbidden = {"sudo", "systemctl", "psql", "mktemp", "setfacl"}
        self.assertFalse([c for c in self.calls() if c[0] in forbidden
                          and c[:2] != ["systemctl", "list-units"]], self.calls())

    def postgres(self, *args):
        return self.create("--engine", "postgres", *args)

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
                          "agent root", "staged root", "sudo rule", "declaration"):
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
                    self.decl.chmod(0o700)
                    (self.decl / "agent.toml").write_text("")
                result = self.create()
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("already", result.stderr)
        self.assert_unprivileged()

    def test_apply_still_refuses_catalogue_collision_before_creation(self):
        self.env.update(ALLOW_APPLY_CHECKS="1", ROLE_COLLISION="1")
        result = self.postgres("--apply")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("role weaver_m1 already exists", result.stderr)
        self.assertFalse(any("useradd" in c for c in self.calls()))

    def test_apply_requires_sudo_before_any_other_privileged_call(self):
        self.env.update(ALLOW_APPLY_CHECKS="1", SUDO_FAIL="1")
        result = self.create("--apply")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("--apply needs sudo", result.stderr)
        self.assertEqual([c for c in self.calls() if c[0] == "sudo"], [["sudo", "-v"]])

    def test_apply_read_failures_refuse_before_creating_accounts(self):
        for fault, cause in (("pg_roles", "role catalog"),
                             ("pg_database", "database catalog"), ("hba_file", "hba_file"),
                             ("ident_file", "ident_file"), ("start", "start PostgreSQL"),
                             ("is-active", "confirm PostgreSQL")):
            with self.subTest(fault=fault):
                self.log.unlink(missing_ok=True)
                self.env.update(ALLOW_APPLY_CHECKS="1", READ_FAIL=fault)
                result = self.postgres("--apply")
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(cause, result.stderr)
                self.assertFalse(any("useradd" in c for c in self.calls()))

    def test_apply_empty_authentication_paths_refuse_before_creation(self):
        for path in ("hba_file", "ident_file"):
            with self.subTest(path=path):
                self.log.unlink(missing_ok=True)
                self.env.update(ALLOW_APPLY_CHECKS="1", EMPTY_PATH=path)
                result = self.postgres("--apply")
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("empty " + path, result.stderr)
                self.assertFalse(any("useradd" in c for c in self.calls()))

    def assert_no_provisioning(self):
        self.assertFalse(any("useradd" in c or any("CREATE ROLE" in a or "CREATE DATABASE" in a for a in c)
                             for c in self.calls()), self.calls())

    def test_authentication_preconditions_refuse_before_provisioning(self):
        for fault in ("no-peer", "missing-hba", "missing-ident"):
            with self.subTest(fault=fault):
                self.log.unlink(missing_ok=True)
                self.hba.write_text("local all all peer\n")
                self.ident.touch()
                if fault == "no-peer": self.hba.write_text("local all all trust\n")
                elif fault == "missing-hba": self.hba.unlink()
                else: self.ident.unlink()
                self.env["ALLOW_APPLY_CHECKS"] = "1"
                result = self.postgres("--apply")
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("anchor" if fault == "no-peer" else "authentication file", result.stderr)
                self.assert_no_provisioning()

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
        for engine in ("sqlite", "postgres"):
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
        self.assertIn(f"directory       {self.root / 'agents' / 'weaver-m1'} ", result.stdout)

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
        # The agent's own keys (weaver-admin-Spec section 9).
        self.assertEqual((root / "declaration-directory").read_text(), str(self.decl) + "\n")
        self.assertEqual((root / "operator").read_text(), "12345\n")
        self.assertEqual((root / "roles.toml").read_text(), 'trace-reader = "weaver-m1-admincon"\n')
        for retired in ("allow-list", "agent-config-directory", "spu-implementations", "agent-spu",
                        "agent.toml", "log-path", "run-tool", "control-tool", "unit-properties"):
            self.assertFalse((root / retired).exists(), retired)
            self.assertFalse((self.config / retired).exists(), retired)
        # The declaration is the operator's, in a directory closed to all others.
        self.assertEqual(self.decl.stat().st_mode & 0o777, 0o700)
        self.assertEqual((self.decl / "agent.toml").stat().st_mode & 0o077, 0)
        import tomllib
        store = tomllib.loads((self.decl / "agent.toml").read_text())["state-store"]
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

    def test_apply_fixture_reaches_the_end_with_postgres(self):
        self.env["ALLOW_APPLY_CHECKS"] = "1"
        result = self.postgres("--apply")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("== made", result.stdout)
        store = self.assert_agent_root("postgres")
        self.assertEqual((store["database"], store["role"]), ("weaver_m1", "weaver_m1"))
        self.assertIn("local   weaver_m1", self.hba.read_text())
        self.assertIn("weaver-m1-state", self.ident.read_text())
        calls = self.calls()
        sql = [c for c in calls if "psql" in c]
        self.assertTrue(any("CREATE ROLE" in c[-1] for c in sql))
        self.assertTrue(any("CREATE DATABASE" in c[-1] for c in sql))
        self.assertTrue(all("-X" in c for c in sql))
        self.assertFalse(any("/etc/weaver/agents" in c for c in calls))
        self.assert_rule(["show", "validate", "load", "unload", "stop"])

    def test_an_unnamed_engine_refuses_naming_both(self):
        # Neither engine is the default, per the operator's ruling on #38.
        # Perturbation: default ENGINE to either and this case runs on.
        result = self.create_naming("--apply")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("--engine sqlite or --engine postgres", result.stderr + result.stdout)
        self.assertIn("Neither is the default", result.stderr + result.stdout)
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
        self.assert_rule(["show", "validate", "load", "unload", "stop"])

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
        # The relay holds the trace group alone, and the connector the access
        # group and nothing of the agent's. Perturbations: drop either check,
        # and a mis-grouped account is admitted.
        for env, said in ((dict(RELAY_GROUPS="weaver-m1-trace weaver-m1-state"), "not the trace group alone"),
                          (dict(CONNECTOR_GROUPS="weaver-m1-admincon"), "does not hold weaver-m1-admin"),
                          (dict(CONNECTOR_GROUPS="weaver-m1-admincon weaver-m1-admin weaver-m1-trace"),
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

    def test_a_declaration_directory_another_principal_could_reach_refuses(self):
        # weaver-admin-Spec section 9: the directory is the operator's and closed,
        # and every directory above it the operator's or root's and closed.
        # Perturbations: drop either judgment, and the plan runs on.
        self.decl.mkdir(parents=True)
        self.decl.chmod(0o750)
        result = self.create()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("grants group or other access", result.stderr)
        self.decl.chmod(0o700)
        (self.operator_home / ".weaveragent").chmod(0o777)
        try:
            result = self.create()
        finally:
            (self.operator_home / ".weaveragent").chmod(0o755)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("which another principal could write", result.stderr)
        result = self.create("--declaration-directory", "relative/dir")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("absolute path", result.stderr)
        self.assert_unprivileged()

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
        for script in ("verify-load.sh", "create-agent.sh", "bootstrap-stack.sh", "update-stack.sh"):
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
                       "decommission.sh"):
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
        (root / "declaration-directory").write_text(str(self.decl) + "\n")
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
        # The operator's ruling of 2026-10-02 (#28) as refined on #56: the member
        # passes through a root:member 0710 territory (no setgid, no listing) to
        # its 0700 room, and the trace is made before the first load as
        # root:weaver-<name>-trace 0640, so the member, outside that group, cannot
        # read it. The operator joins all three groups, and no access entry is set
        # or probed. Perturbations: restore setgid (2710 or 2750), group the
        # trace to the member, or drop the trace group from the operator, and
        # this fails.
        self.env["ALLOW_APPLY_CHECKS"] = "1"
        result = self.create("--apply")
        self.assertEqual(result.returncode, 0, result.stderr)
        calls = self.calls()
        territory = str(self.root / "agents" / "weaver-m1")
        self.assertIn(["sudo", "install", "-d", "-o", "root", "-g", "weaver-m1-state", "-m", "0710", territory], calls)
        self.assertIn(["sudo", "install", "-o", "root", "-g", "weaver-m1-trace", "-m", "0640", "/dev/null",
                       territory + "/trace.ndjson"], calls)
        self.assertIn(["sudo", "install", "-d", "-o", "weaver-m1-state", "-g", "weaver-m1-state", "-m", "0700",
                       territory + "/state"], calls)
        self.assertIn(["sudo", "groupadd", "--system", "weaver-m1-trace"], calls)
        self.assertIn(["sudo", "groupadd", "--system", "weaver-m1-admin"], calls)
        self.assertIn(["sudo", "useradd", "--system", "--shell", "/usr/sbin/nologin", "--no-create-home",
                       "--no-user-group", "--gid", "weaver-m1-trace", "weaver-m1-relay"], calls)
        self.assertIn(["sudo", "useradd", "--system", "--shell", "/usr/sbin/nologin", "--no-create-home",
                       "--user-group", "--groups", "weaver-m1-admin", "weaver-m1-admincon"], calls)
        self.assertIn(["sudo", "usermod", "-aG", "weaver-m1,weaver-m1-state,weaver-m1-trace", "fixture-no-home"], calls)
        self.assertIn(["sudo", "-u", "weaver-m1-state", "test", "-r", territory + "/trace.ndjson"], calls)
        self.assertFalse([c for c in calls if "setfacl" in c or c[0] == "mktemp"], calls)

    def test_a_trace_the_member_can_read_refuses_before_admission(self):
        # The probe is what holds the boundary on the box: a member that can read
        # the trace refuses before the root is moved into place, for either
        # engine. Perturbation: drop the probe, and the agent is admitted.
        for engine in ("sqlite", "postgres"):
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

    def test_update_stack_patches_a_declaration_as_its_owner(self):
        # The declaration is the operator's, in the operator's closed directory
        # (weaver-admin-Spec section 9), so the patch and its restore are the
        # operator's own writes and no root step touches a file another
        # principal could choose. Perturbation: restore a root copy or append,
        # and this fails.
        text = (self.repo / "deploy" / "update-stack.sh").read_text()
        for root_write in ('sudo cp -a "$decl"', 'sudo tee -a "$decl"', 'sudo cp -a "${entry##*|}"'):
            self.assertNotIn(root_write, text)
        self.assertIn('cp -a "$decl" "$decl.pre-$AFTER-bak"', text)
        self.assertIn('engine = "none"\\n\' >> "$decl"', text)

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
        # box-wide keys refuses by name before cargo runs. Perturbation:
        # remove the check and the run plans, reaching the build.
        for retired in ("allow-list", "agent-config-directory", "spu-implementations", "agent-spu"):
            with self.subTest(retired=retired):
                self.log.unlink(missing_ok=True)
                (self.config / retired).write_text("existing\n")
                result = self.run_script("update-stack.sh")
                self.assertEqual(result.returncode, 1, result.stderr)
                self.assertIn("box-wide layout", result.stderr)
                self.assertIn("REDEPLOY.md", result.stderr)
                self.assertFalse(any(c[0] == "cargo" for c in self.calls()))
                (self.config / retired).unlink()

    def test_stack_refuses_a_root_of_the_layout_before_50(self):
        # A root holding its declaration or a retired key is migrated by hand
        # first, since the admin this installs reads none of them. Refused by
        # name before cargo runs. Perturbation: drop the check, and the run
        # plans on.
        for retired in ("agent.toml", "run-tool", "control-tool", "unit-properties", "log-path"):
            with self.subTest(retired=retired):
                self.log.unlink(missing_ok=True)
                (self.config / "existing" / retired).write_text("x\n")
                result = self.run_script("update-stack.sh")
                self.assertEqual(result.returncode, 1, result.stderr)
                self.assertIn("layout before #50", result.stderr)
                self.assertIn("REDEPLOY.md section 8", result.stderr)
                self.assertFalse(any(c[0] == "cargo" for c in self.calls()))
                (self.config / "existing" / retired).unlink()

    def test_stack_refuses_while_a_unit_of_the_old_layout_serves(self):
        # The admin this installs ends a run by its run lock, which a unit's
        # worker never took, so a unit still serving refuses, and so does a
        # systemctl that cannot answer. Perturbations: drop the check, or read
        # a failed look as no unit, and the run plans on.
        for env, said in ((dict(UNITS="weaver-worker@karl.service loaded active running x\n"),
                           "weaver-worker@karl.service"),
                          (dict(UNITS_FAIL="1"), "cannot ask systemd")):
            with self.subTest(env=env):
                self.log.unlink(missing_ok=True)
                for name in ("UNITS", "UNITS_FAIL"): self.env.pop(name, None)
                self.env.update(env)
                result = self.run_script("update-stack.sh")
                self.assertEqual(result.returncode, 1, result.stderr)
                self.assertIn(said, result.stderr)
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

    def test_stack_refuses_a_trace_the_old_admin_recreated(self):
        # Codex on #82: an admin before #62 recreated a lost trace root:root,
        # which the new admin refuses at load and validate never sees, so the
        # plan refuses it before the build, naming the re-lay. Perturbation:
        # drop the preflight, and the run reaches the build.
        decl = self.operator_home / ".weaveragent" / "existing" / "agent.toml"
        trace = self.root / "agents" / "trace.ndjson"
        trace.write_text("")
        trace.chmod(0o640)
        decl.write_text(f'[state-store]\nengine = "none"\n\n[trace-sink]\nkind = "file"\npath = "{trace}"\ncreate = true\n')
        result = self.run_script("update-stack.sh")
        self.assertEqual(result.returncode, 1, result.stdout)
        self.assertIn("Re-lay it first: sudo chgrp weaver-existing-trace", result.stderr)
        self.assertFalse(any(c[:2] == ["cargo", "build"] for c in self.calls()))
        self.log.unlink(missing_ok=True)
        self.env["TRACE_GROUP_AS"] = "weaver-existing-trace"
        result = self.run_script("update-stack.sh")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("every file sink stands as the territory lays it out", result.stdout)
        # The trace is empty, as a freshly provisioned one is, and still passes:
        # its type is asked by predicate, never by stat's words for it.
        self.assertEqual(trace.stat().st_size, 0)

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

    def test_stack_reads_each_declaration_from_its_directory(self):
        # The declaration lives in the directory the root names. One the
        # operator cannot read refuses by name and is never left out.
        # Perturbation: read `<root>/agent.toml` again, and the agent is skipped.
        decl = self.operator_home / ".weaveragent" / "existing" / "agent.toml"
        decl.chmod(0o000)
        try:
            if os.access(decl, os.R_OK):
                self.skipTest("no mode closes a file to this user")
            result = self.run_script("update-stack.sh")
        finally:
            decl.chmod(0o644)
        self.assertEqual(result.returncode, 1, result.stdout)
        self.assertIn(str(decl) + " cannot be read", result.stderr)

    def test_stack_agents_are_the_roots_under_the_base(self):
        # A staged root under a dot-name, a plain file, and a root naming no
        # declaration directory are not agents. Perturbation: drop the
        # declaration-directory check and `undeclared` is listed.
        (self.config / ".m2.partial").mkdir()
        (self.config / "stray-file").write_text("x")
        (self.config / "undeclared").mkdir()
        (self.config / "undeclared" / "worker-binary").write_text("/x")
        result = self.run_script("update-stack.sh")
        self.assertEqual(result.returncode, 0, result.stderr)
        agents_line = next(l for l in result.stdout.splitlines() if l.startswith("  agents"))
        self.assertEqual(agents_line.split(), ["agents", "existing"])
        shutil.rmtree(self.config / "existing")
        result = self.run_script("update-stack.sh")
        self.assertEqual(result.returncode, 1)
        self.assertIn("no agent root under", result.stderr)

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

    def test_stack_build_failure_cannot_claim_a_plan(self):
        self.env["BUILD_FAIL"] = "1"
        result = self.run_script("update-stack.sh")
        self.assertEqual(result.returncode, 42, result.stderr)
        self.assertIn("fixture build refusal", result.stderr)
        self.assertNotIn("== plan", result.stdout)
        self.assertNotIn("box is current", result.stdout)
        self.assert_unprivileged()


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

    def run_fn(self, name, *args):
        run = subprocess.run(["bash", "-c", shell_function(self.script, name) + f'{name} "$@"', "x", *args],
                             text=True, capture_output=True, timeout=20)
        return run.stdout.strip()

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

    def test_the_operators_declaration_directories_are_never_purged(self):
        # They are archived and dropped from the purge list. Perturbation: drop
        # the slice back, and the purge would remove the operator's files.
        self.assertIn('archive_path declaration-directories "${!DECL_DIRS[@]}"', self.script)
        self.assertIn('PURGE=("${PURGE[@]:0:$kept}")', self.script)
        self.assertLess(self.script.index("kept=${#PURGE[@]}"),
                        self.script.index('archive_path declaration-directories'))

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
        for sink in ("opt", "territories", "log", "agent-config"):
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
printf '%s\\n' "boundary unverified: no store socket at /run/weaver/fixture" >&2
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
        decl = Path(self.tmp.name) / "a.toml"
        decl.write_text(text)
        run = subprocess.run(["bash", "-c", declared_definition(self.script) + 'declared "$@"', "x",
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
        for text in ('[state-store]\nengine = "postgres"\n', "[state-store]\nengine = 'postgres'\n",
                     'state-store.engine = "postgres"\n', 'state-store = { engine = "postgres" }\n'):
            with self.subTest(text=text):
                self.assertEqual(self.read(text, "state-store.engine", "string")[:2], (0, "postgres"))
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
            self.assertIn("admin: boundary unverified: no store socket at /run/weaver/fixture",
                          result.stderr)

    def test_both_call_sites_reach_the_helper(self):
        self.assertIn('validate() {\n  admin_answer validate "$1"\n}', self.script)
        self.assertIn('  admin_answer load "$AGENT"\n', self.script)
        admin_lines = [line for line in self.script.splitlines()
                       if "/weaver-admin\"" in line and not line.lstrip().startswith("#")]
        self.assertFalse([line for line in admin_lines if "tail -1" in line], admin_lines)


if __name__ == "__main__":
    unittest.main()
