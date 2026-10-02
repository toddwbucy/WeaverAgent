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
    if args[:1] == ['-c'] and args[1] == '%u %a':
        path = pathlib.Path(args[-1])
        st = os.lstat(path)
        owner = 0 if path.is_relative_to(root) else st.st_uid
        print(owner, format(st.st_mode & 0o7777, 'o'))
        sys.exit(0)
    os.execv('/usr/bin/stat', ['stat', *args])
with open(os.environ['CALLS'], 'a') as log:
    log.write(json.dumps([name, *args]) + '\n')
def mapped(value):
    if value.startswith('/home/'):
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
    if os.environ.get('ACCOUNT_FAIL'): sys.exit(1)
    sys.exit(0 if os.environ.get('COLLISION') == args[-1] else 2)
elif name == 'id': print(os.environ.get('FIXTURE_UID', '12345'))
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
        for member in ('pyworker', 'worker', 'weaver-admin', 'weaver-gate', 'weaver-spu', 'weaver-state'):
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
        directory = pathlib.Path(mapped(rest[-1]))
        assert directory.is_relative_to(root), directory
        directory.mkdir(parents=True, exist_ok=True)
    elif op == 'mv':
        source, destination = (pathlib.Path(mapped(a)) for a in rest[-2:])
        assert source.is_relative_to(root) and destination.is_relative_to(root)
        assert not destination.exists(), destination
        source.rename(destination)
    elif op == 'test':
        # The probes of a sqlite agent's state room: the member passes, the
        # agent's own uid is refused, unless the fixture opens the wall.
        if identity == 'weaver-m1': sys.exit(0 if os.environ.get('WALL_OPEN') else 1)
        sys.exit(0)
    elif op in ('useradd', 'usermod', 'chmod', 'setfacl'): pass
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
        (existing / "agent.toml").write_text("[state-store]\nengine = \"none\"\n")
        self.stack = self.root / "stack"
        self.stack.mkdir()
        self.logs = self.root / "log"
        self.logs.mkdir()
        self.stack_keys = {
            "worker-binary": str(self.root / "installed" / "pyworker"),
            "spu-binary": str(self.root / "installed" / "weaver-spu"),
            "gate-binary": str(self.root / "installed" / "weaver-gate"),
            "run-tool": "/usr/bin/systemd-run", "control-tool": "/usr/bin/systemctl",
            "coordination-root": "/run",
            "unit-properties": "UMask=0000\nEnvironment=LD_LIBRARY_PATH=/fixture/lib\n",
            "log-directory": str(self.logs),
            "agent-directory": str(self.root / "agents"),
            "prefix": str(self.root / "installed"),
        }
        for key, value in self.stack_keys.items():
            (self.stack / key).write_text(value + ("" if value.endswith("\n") else "\n"))
        self.home = self.root / "home"
        (self.home / "fixture-no-home").mkdir(parents=True)
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
        for name in ("BASH_ENV", "SUDO_USER", "COLLISION", "ALLOW_APPLY_CHECKS", "ROLE_COLLISION", "BUILD_FAIL", "SUDO_FAIL", "READ_FAIL", "EMPTY_PATH", "ACL_FAIL", "PATH_FAIL", "ACCOUNT_FAIL", "WALL_OPEN"):
            self.env.pop(name, None)
        # Redirect even shell builtin /home probes into the fixture. The
        # production scripts have no test-only path switches and never read
        # the host's real agent homes during these tests.
        preamble = self.root / "fixture.bash"
        preamble.write_text("""fixture_args() {
  local arg
  fixture_mapped=()
  for arg in "$@"; do
    case "$arg" in /home/*) arg="$FIXTURE_ROOT$arg";; esac
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

    def create_naming(self, *args):
        return self.run_script("create-agent.sh", "m1", "--artifact", str(self.artifact), *args)

    def assert_unprivileged(self):
        forbidden = {"sudo", "systemctl", "psql", "mktemp", "setfacl"}
        self.assertFalse([c for c in self.calls() if c[0] in forbidden], self.calls())

    def postgres(self, *args):
        return self.create("--engine", "postgres", *args)

    def test_agent_plan_defers_privilege_and_preserves_fixture_files(self):
        before = {p: p.read_bytes() for p in self.root.rglob("*") if p.is_file()}
        result = self.create()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn(str(self.config / "m1" / "agent.toml"), result.stdout)
        self.assertIn(str(self.logs / "m1" / "admin.log"), result.stdout)
        self.assertIn("PENDING --apply", result.stdout)
        self.assertNotIn("nothing of this agent exists", result.stdout)
        self.assert_unprivileged()
        after = {p: p.read_bytes() for p in self.root.rglob("*") if p.is_file() and p != self.log}
        self.assertEqual(before, after)

    def test_agent_plan_refuses_missing_or_empty_stack_keys(self):
        # Every required key of the stack record refuses by name, absent or
        # empty, before anything privileged. Perturbation: drop the check and
        # an agent root is written without the key admin requires.
        for key in ("worker-binary", "unit-properties", "log-directory", "agent-directory"):
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
        for collision in ("account", "agent root", "staged root", "log directory"):
            with self.subTest(collision=collision):
                self.env.pop("COLLISION", None)
                for path in (self.config / "m1", self.config / ".m1.partial", self.logs / "m1"):
                    if path.exists(): path.rmdir()
                if collision == "account": self.env["COLLISION"] = "weaver-m1-state"
                elif collision == "agent root": (self.config / "m1").mkdir()
                elif collision == "staged root": (self.config / ".m1.partial").mkdir()
                else: (self.logs / "m1").mkdir()
                result = self.create()
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("already exists", result.stderr)
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
        (self.stack / "log-directory").unlink()
        (self.stack / "log-directory").mkdir()
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
            if key in ("log-directory", "agent-directory", "prefix"):
                self.assertFalse((root / key).exists(), key)
            elif key == "spu-binary" and spu:
                self.assertEqual((root / key).read_text(), spu + "\n")
            else:
                self.assertEqual((root / key).read_text(), (self.stack / key).read_text(), key)
        self.assertEqual((root / "log-path").read_text(), str(self.logs / "m1" / "admin.log") + "\n")
        self.assertTrue((self.logs / "m1").is_dir())
        for retired in ("allow-list", "agent-config-directory", "spu-implementations", "agent-spu"):
            self.assertFalse((root / retired).exists())
            self.assertFalse((self.config / retired).exists())
        import tomllib
        store = tomllib.loads((root / "agent.toml").read_text())["state-store"]
        self.assertEqual(store["engine"], engine)
        return store

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
            self.assertEqual(self.calls(), [], name)
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
        for script in ("verify-load.sh", "create-agent.sh", "bootstrap-stack.sh", "update-stack.sh"):
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

    def test_the_territory_is_reached_by_group_and_never_by_access_entries(self):
        # The operator's ruling of 2026-10-02 (#28): the territory is root's,
        # grouped to the state member, 2750, under the record's base; the
        # member's room is its own, 0700; the operator joins both of the agent's
        # groups; and no access entry is set or probed anywhere, so a box whose
        # datasets carry none deploys. Perturbations: group the territory to the
        # operator, or restore the setfacl chain, and this fails.
        self.env["ALLOW_APPLY_CHECKS"] = "1"
        result = self.create("--apply")
        self.assertEqual(result.returncode, 0, result.stderr)
        calls = self.calls()
        territory = str(self.root / "agents" / "weaver-m1")
        self.assertIn(["sudo", "install", "-d", "-o", "root", "-g", "weaver-m1-state", "-m", "2750", territory], calls)
        self.assertIn(["sudo", "install", "-d", "-o", "weaver-m1-state", "-g", "weaver-m1-state", "-m", "0700",
                       territory + "/state"], calls)
        self.assertIn(["sudo", "usermod", "-aG", "weaver-m1,weaver-m1-state", "fixture-no-home"], calls)
        self.assertFalse([c for c in calls if "setfacl" in c or c[0] == "mktemp"], calls)

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
        entry = self.stack / "log-directory"
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

    def test_update_stack_patches_only_a_declaration_held_closed(self):
        # The walk of #45 round 11: the patch runs exactly when admin has refused
        # the root, so its judgment does not stand behind the write; the
        # declaration is held closed before the root copy and append.
        # Perturbation: drop the judgment, and the patch is unguarded.
        text = (self.repo / "deploy" / "update-stack.sh").read_text()
        guard = text.index('bad=$(held_closed "$decl")')
        self.assertLess(guard, text.index('sudo cp -a "$decl"'))
        self.assertLess(guard, text.index('sudo tee -a "$decl"'))

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
        self.assertEqual(result.stdout.count("NEW"), 6)
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

    def test_stack_agents_are_the_roots_under_the_base(self):
        # A staged root under a dot-name, a plain file, and a root with no
        # declaration are not agents, as admin refuses the last for every verb.
        # Perturbation: drop the declaration check and `undeclared` is listed.
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
