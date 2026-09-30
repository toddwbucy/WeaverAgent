#!/usr/bin/env python3
"""Deploy plan boundaries, using subprocess fixtures (no real sudo or build).

Run: python3 deploy/test_plans.py
The command doubles record every privileged/build invocation. They never
forward sudo, systemctl, package-manager or Cargo calls to the host.
"""

import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


DEPLOY = Path(__file__).resolve().parent
DOUBLE = r'''#!/usr/bin/env python3
import json, os, pathlib, shutil, subprocess, sys
name = pathlib.Path(sys.argv[0]).name
args = sys.argv[1:]
root = pathlib.Path(os.environ['FIXTURE_ROOT'])
with open(os.environ['CALLS'], 'a') as log:
    log.write(json.dumps([name, *args]) + '\n')
def mapped(value):
    if value.startswith('/home/'):
        return str(root / value.lstrip('/'))
    return value

def shell_read(arguments):
    if os.environ.get('READ_FAIL') == 'allow-list' and any(a.endswith('/allow-list') for a in arguments):
        sys.exit(2)
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
elif name == 'id': print('12345')
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
        self.config = self.root / "config"
        self.config.mkdir()
        self.agents = self.root / "agents"
        self.agents.mkdir()
        (self.config / "agent-config-directory").write_text(str(self.agents))
        (self.config / "worker-binary").write_text(str(self.root / "installed" / "pyworker"))
        (self.config / "allow-list").write_text("existing\n")
        (self.agents / "existing.toml").write_text("[state-store]\nengine = \"none\"\n")
        self.home = self.root / "home"
        (self.home / "fixture-no-home" / ".weaveragents").mkdir(parents=True)
        self.hba = self.root / "pg_hba.conf"
        self.hba.write_text("local all all peer\n")
        self.ident = self.root / "pg_ident.conf"
        self.ident.touch()
        self.artifact = self.root / "model.gguf"
        self.artifact.touch()
        self.bin = self.root / "bin"
        self.bin.mkdir()
        for name in ("sudo", "systemctl", "psql", "mktemp", "setfacl", "getent", "git", "cargo", "hostname", "nvidia-smi", "pacman", "sh", "id"):
            command = self.bin / name
            command.write_text(DOUBLE)
            command.chmod(0o755)
        self.log = self.root / "calls"
        self.env = {**os.environ, "PATH": str(self.bin) + os.pathsep + os.environ["PATH"],
                    "WEAVER_ADMIN_CONFIG": str(self.config), "CALLS": str(self.log),
                    "CARGO_TARGET_DIR": str(self.root / 'target with "quotes"'),
                    "USER": "fixture-no-home", "PROBE": str(self.root / "probe"),
                    "FIXTURE_ROOT": str(self.root)}
        for name in ("BASH_ENV", "SUDO_USER", "COLLISION", "ALLOW_APPLY_CHECKS", "ROLE_COLLISION", "BUILD_FAIL", "SUDO_FAIL", "READ_FAIL", "EMPTY_PATH", "ACL_FAIL", "PATH_FAIL", "ACCOUNT_FAIL"):
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
        return self.run_script("create-agent.sh", "m1", "--artifact", str(self.artifact), *args)

    def assert_unprivileged(self):
        forbidden = {"sudo", "systemctl", "psql", "mktemp", "setfacl"}
        self.assertFalse([c for c in self.calls() if c[0] in forbidden], self.calls())

    def test_agent_plan_defers_privilege_and_preserves_fixture_files(self):
        before = {p: p.read_bytes() for p in self.root.rglob("*") if p.is_file()}
        result = self.create()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn(str(self.agents / "m1.toml"), result.stdout)
        self.assertIn("PENDING --apply", result.stdout)
        self.assertNotIn("nothing of this agent exists", result.stdout)
        self.assert_unprivileged()
        after = {p: p.read_bytes() for p in self.root.rglob("*") if p.is_file() and p != self.log}
        self.assertEqual(before, after)

    def test_agent_plan_refuses_missing_or_empty_configuration(self):
        field = self.config / "agent-config-directory"
        for value in (None, ""):
            with self.subTest(value=value):
                if value is None: field.unlink()
                else: field.write_text(value)
                result = self.create()
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("agent-config-directory", result.stderr)
        self.assert_unprivileged()

    def test_missing_allow_list_is_empty_in_both_modes(self):
        (self.config / "allow-list").unlink()
        result = self.create()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assert_unprivileged()
        self.env.update(ALLOW_APPLY_CHECKS="1", ACL_FAIL="1")
        result = self.create("--apply")
        self.assertIn("refuses access entries", result.stderr)

    def test_agent_plan_refuses_visible_collisions(self):
        for collision in ("account", "declaration", "allow-list"):
            with self.subTest(collision=collision):
                self.env.pop("COLLISION", None)
                (self.agents / "m1.toml").unlink(missing_ok=True)
                (self.config / "allow-list").write_text("existing\n")
                if collision == "account": self.env["COLLISION"] = "weaver-m1-state"
                elif collision == "declaration": (self.agents / "m1.toml").touch()
                else: (self.config / "allow-list").write_text("m1\n")
                result = self.create()
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("already exists" if collision != "allow-list" else "already in", result.stderr)
        self.assert_unprivileged()

    def test_apply_still_refuses_catalogue_collision_before_creation(self):
        self.env.update(ALLOW_APPLY_CHECKS="1", ROLE_COLLISION="1")
        result = self.create("--apply")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("role weaver_m1 already exists", result.stderr)
        self.assertFalse(any("useradd" in c for c in self.calls()))

    def test_apply_still_probes_acl_before_creation(self):
        self.env.update(ALLOW_APPLY_CHECKS="1", ACL_FAIL="1")
        result = self.create("--apply")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("refuses access entries", result.stderr)
        self.assertFalse(any("systemctl" in c for c in self.calls()))
        self.assertTrue(any(c[0] == "setfacl" for c in self.calls()))
        self.assertFalse(any("useradd" in c for c in self.calls()))
        self.assertFalse(Path(self.env["PROBE"]).exists())

    def test_apply_requires_sudo_before_any_other_privileged_call(self):
        self.env.update(ALLOW_APPLY_CHECKS="1", SUDO_FAIL="1")
        result = self.create("--apply")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("--apply needs sudo", result.stderr)
        self.assertEqual([c for c in self.calls() if c[0] == "sudo"], [["sudo", "-v"]])

    def test_apply_read_failures_refuse_before_creating_accounts(self):
        for fault, cause in (("allow-list", "allow-list"), ("pg_roles", "role catalog"),
                             ("pg_database", "database catalog"), ("hba_file", "hba_file"),
                             ("ident_file", "ident_file"), ("start", "start PostgreSQL"),
                             ("is-active", "confirm PostgreSQL")):
            with self.subTest(fault=fault):
                self.log.unlink(missing_ok=True)
                self.env.update(ALLOW_APPLY_CHECKS="1", READ_FAIL=fault)
                result = self.create("--apply")
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(cause, result.stderr)
                self.assertFalse(any("useradd" in c for c in self.calls()))

    def test_apply_empty_authentication_paths_refuse_before_creation(self):
        for path in ("hba_file", "ident_file"):
            with self.subTest(path=path):
                self.log.unlink(missing_ok=True)
                self.env.update(ALLOW_APPLY_CHECKS="1", EMPTY_PATH=path)
                result = self.create("--apply")
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
                result = self.create("--apply")
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("anchor" if fault == "no-peer" else "authentication file", result.stderr)
                self.assert_no_provisioning()

    def test_missing_declaration_directory_refuses_both_modes(self):
        (self.config / "agent-config-directory").write_text(str(self.root / "missing"))
        for apply in (False, True):
            with self.subTest(apply=apply):
                self.env["ALLOW_APPLY_CHECKS"] = "1"
                result = self.create(*(["--apply"] if apply else []))
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("declaration directory", result.stderr)
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
        result = self.create("--session", "s-m1-1")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotIn("does not parse as TOML", result.stderr)

    def test_invalid_engine_does_not_prompt_for_sudo(self):
        result = self.create("--apply", "--engine", "invalid")
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(any(c[0] == "sudo" for c in self.calls()))

    def test_apply_uses_privileged_collision_reads(self):
        for path in (self.agents / "m1.toml", self.home / "weaver-m1",
                     self.home / "fixture-no-home" / ".weaveragents" / "weaver-m1"):
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

    def test_configuration_and_allow_list_are_trimmed_in_both_modes(self):
        (self.config / "agent-config-directory").write_text("  " + str(self.agents) + " \r\n")
        (self.config / "allow-list").write_text("existing\n  m1 \r\n")
        for apply in (False, True):
            with self.subTest(apply=apply):
                self.env["ALLOW_APPLY_CHECKS"] = "1"
                result = self.create(*(["--apply"] if apply else []))
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("m1 is already in", result.stderr)
                self.assertIn(str(self.agents / "m1.toml"), result.stdout)
                self.assert_no_provisioning()

    def test_present_unreadable_allow_list_refuses_in_both_modes(self):
        # Inject an I/O refusal; chmod alone is ineffective under root test runners.
        self.env["READ_FAIL"] = "allow-list"
        for apply in (False, True):
            with self.subTest(apply=apply):
                self.env["ALLOW_APPLY_CHECKS"] = "1"
                result = self.create(*(["--apply"] if apply else []))
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("cannot read", result.stderr)
                self.assert_no_provisioning()

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
        self.env.update(ALLOW_APPLY_CHECKS="1", PATH_FAIL=str(self.agents / "m1.toml"))
        result = self.create("--apply")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("cannot inspect", result.stderr)
        self.assert_no_provisioning()

    def test_apply_fixture_reaches_the_end_using_only_configured_directory(self):
        (self.config / "allow-list").unlink()
        self.env["ALLOW_APPLY_CHECKS"] = "1"
        result = self.create("--apply")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("== made", result.stdout)
        self.assertTrue((self.agents / "m1.toml").is_file())
        self.assertIn("m1", (self.config / "allow-list").read_text().splitlines())
        self.assertIn("local   weaver_m1", self.hba.read_text())
        self.assertIn("weaver-m1-state", self.ident.read_text())
        calls = self.calls()
        sql = [c for c in calls if "psql" in c]
        self.assertTrue(any("CREATE ROLE" in c[-1] for c in sql))
        self.assertTrue(any("CREATE DATABASE" in c[-1] for c in sql))
        self.assertTrue(all("-X" in c for c in sql))
        self.assertFalse(any("/etc/weaver/agents" in c for c in calls))

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

    def test_stack_refuses_an_agent_whose_declaration_is_still_yaml(self):
        # The admin this installs reads `<agent>.toml`, so an agent with only
        # `<agent>.yaml` refuses by name before cargo runs. Perturbation:
        # remove the check and the run plans, reaching the build.
        (self.agents / "existing.toml").unlink(missing_ok=True)
        (self.agents / "existing.yaml").write_text("state-store:\n  engine: none\n")
        result = self.run_script("update-stack.sh")
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertIn("only a YAML declaration stands", result.stderr)
        self.assertIn("existing", result.stderr)
        self.assertIn("Install each agent's TOML declaration", result.stderr)
        self.assertFalse(any(c[0] == "cargo" for c in self.calls()))
        # Beside its TOML, the YAML is inert and the run plans.
        (self.agents / "existing.toml").write_text("[state-store]\nengine = \"none\"\n")
        result = self.run_script("update-stack.sh")
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_stack_build_failure_cannot_claim_a_plan(self):
        self.env["BUILD_FAIL"] = "1"
        result = self.run_script("update-stack.sh")
        self.assertEqual(result.returncode, 42, result.stderr)
        self.assertIn("fixture build refusal", result.stderr)
        self.assertNotIn("== plan", result.stdout)
        self.assertNotIn("box is current", result.stdout)
        self.assert_unprivileged()


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
               "BIN_DIR": str(self.bin_dir), "ADMIN_CONFIG": "/nonexistent"}
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
