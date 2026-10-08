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

def privileged(paths, run):
    """Run `run` with every closed directory above the mapped `paths` opened
    to this user for the call, as root's privilege would pass it: the
    territory a test locks (LOCK_TERRITORY) is closed to the test process and
    open to the fake sudo alone, which is the difference between a read made
    under sudo and one made as the operator."""
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
    elif op == 'stat':
        # The migration's look before it changes anything (Codex on #94,
        # rounds 9 and 12): the territory's group as the fixture names it and
        # the directory's own mode; a file's uid:gid as the fixture owns it,
        # the operator's under the operator's home and root's elsewhere, and
        # the file's own mode.
        target = pathlib.Path(mapped(rest[-1]))
        assert target.is_relative_to(root), target
        st = privileged([target], lambda: os.lstat(target))
        if rest[:2] == ['-c', '%u:%g %a']:
            operator_home = root / 'home' / os.environ.get('USER', '')
            uid = os.environ.get('FIXTURE_UID', '12345') if target.is_relative_to(operator_home) else '0'
            print(f"{uid}:{uid}", format(st.st_mode & 0o7777, 'o'))
        else:
            print(os.environ.get('TERRITORY_GROUP_AS', 'fixture-group'), format(st.st_mode & 0o7777, 'o'))
    elif op in ('cat', 'tail', 'wc', 'getfacl') or (op == 'test' and not identity):
        # The install's reads under a territory, made as root (Codex on #94,
        # round 8): run on the scratch file with the privilege wrapper, so a
        # territory the test locked reads here and nowhere else.
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
    elif op == 'chmod' and os.environ.get('LOCK_TERRITORY') and rest[:1] == ['00710'] \
            and pathlib.Path(mapped(rest[-1])) == pathlib.Path(os.environ['LOCK_TERRITORY']):
        # **The migration's regroup closes the territory to this process**, as
        # the real one does to a shell that has not taken the new login: the
        # fake makes it 0000, which only the privilege wrapper above reopens.
        os.chmod(pathlib.Path(mapped(rest[-1])), 0)
    elif op == 'python3' and rest[:1] == ['-c']:
        # **The no-follow chmod** (Codex on #94, round 14): the helper's own
        # code run on the scratch file, as root would run it, through the
        # privilege wrapper; a link under the name refuses as it would.
        target = pathlib.Path(mapped(rest[-1]))
        assert target.is_relative_to(root), target
        sys.exit(privileged([target], lambda: subprocess.run(
            [sys.executable, '-c', rest[1], *rest[2:-1], str(target)]).returncode))
    elif op == 'mktemp':
        template = rest[-1]
        made = pathlib.Path(mapped(template.replace('XXXXXX', 'fixture')))
        assert made.is_relative_to(root), made
        if '-d' in rest: made.mkdir()
        else: made.touch()
        print(template.replace('XXXXXX', 'fixture'))
    elif op == 'visudo': sys.exit(1 if os.environ.get('VISUDO_FAIL') else 0)
    elif op == 'rmdir':
        # The restore's removal of a save-points/ the run made (#99 area 2
        # review, K7): only an empty directory goes, as rmdir does.
        target = pathlib.Path(mapped(rest[-1]))
        assert target.is_relative_to(root), target
        def remove():
            try: target.rmdir()
            except OSError: return 1
            return 0
        sys.exit(privileged([target], remove))
    elif op == 'python3' and rest[:1] != ['-c']:
        # A script run as root (the identity move of a declaration in its
        # territory): its file arguments are opened to it for the call, as
        # root's privilege passes a file's mode.
        files = [pathlib.Path(a) for a in rest[1:] if a.startswith('/') and pathlib.Path(a).is_file()]
        assert all(f.is_relative_to(root) for f in files), files
        modes = [(f, f.stat().st_mode) for f in files]
        for f, m in modes: f.chmod(m | 0o600)
        try: code = privileged(files, lambda: subprocess.run([sys.executable, *rest]).returncode)
        finally:
            for f, m in modes:
                if f.exists(): f.chmod(m & 0o7777)
        sys.exit(code)
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

# The stand-in sudo the migration test runs `migrate_layout` against: the
# file verbs run as this user, the account and ownership verbs are recorded.
STAND_IN_SUDO = (
    "#!/bin/sh\n"
    "printf '%s\\n' \"$*\" >> {recorded}\n"
    "[ \"$1\" = -n ] && shift\n"
    "case \"$1\" in\n"
    "  chown|chgrp|usermod) exit 0 ;;\n"
    # install runs for real, its owner and group dropped, so the mode it
    # leaves is the one the script asked for (#99 area 2 review, N1).
    "  install) shift; n=$#; skip=0\n"
    "    for a do\n"
    "      if [ \"$skip\" = 1 ]; then skip=0\n"
    "      elif [ \"$a\" = -o ] || [ \"$a\" = -g ]; then skip=1\n"
    "      else set -- \"$@\" \"$a\"; fi\n"
    "    done\n"
    "    shift \"$n\"; exec install \"$@\" ;;\n"
    "  *) exec \"$@\" ;;\n"
    "esac\n"
)


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
                     "COLLISION_GROUP", "KEEP_ALIVE", "TRACE_GROUP_AS", "GROUP_WRITES", "LOCK_TERRITORY",
                     "TERRITORY_GROUP_AS", "LAYOUT_GROUPS", "FIXTURE_ADMIN"):
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

    def test_update_stack_patches_a_declaration_as_root(self):
        # The declaration is root's in root's territory (the operator's ruling
        # of 2026-10-07 on #1), so the patch, its backup and its restore are
        # root's writes of root's own file. The reconcile patch that declared
        # `engine = "none"` could never run, `weaver-state` being in every
        # install, and is gone (#99 area 2 review, K11): no step writes a
        # declaration but the identity move. Perturbation: append as the
        # operator again, or put the patch back, and this fails.
        text = (self.repo / "deploy" / "update-stack.sh").read_text()
        self.assertIn('sudo cp -a "$decl" "$decl.pre-$AFTER-bak"', text)
        self.assertIn('sudo cp -a "${entry##*|}" "${entry%%|*}"', text)
        self.assertNotIn('sudo tee "$decl"', text)
        self.assertNotIn("STATE_BINARY", text)
        self.assertNotIn('>> "$decl"', text)
        self.assertNotIn('cat "$decl" |', text)

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

    def migrate(self, decl, *args):
        return subprocess.run([sys.executable, str(self.repo / "deploy" / "migrate-identity.py"), str(decl), *args],
                              env=self.env, text=True, capture_output=True, timeout=20)

    def test_migrate_identity_moves_one_system_text_into_the_draft(self):
        # The one shape that moves losslessly: one system message of one text
        # block. The check names the move and changes nothing; the apply
        # writes the draft 0600 with the text byte for byte (no newline
        # added: the seeding sends the file verbatim, Codex on #92), removes
        # the tables, and the declaration re-parses as itself minus the
        # identity. A second run finds nothing to do. Perturbations: drop the
        # strip of the content header and the re-parse refuses; append a
        # newline to the text and the draft assertion fails.
        import tomllib
        self.decl.mkdir(parents=True)
        decl = self.decl / "agent.toml"
        draft = self.decl / "system-prompt.md"
        for form in ("tables", "inline"):
            with self.subTest(form=form):
                text = self.KARL
                if form == "inline":
                    text = text.replace(
                        '[[spu-instruction.decoder.identity]]\nrole = "system"\n\n'
                        '[[spu-instruction.decoder.identity.content]]\ntype = "text"\ntext = """\n'
                        'You are Karl, a small local agent.\nAnswer plainly."""\n\n', "").replace(
                        "surprisal-election = true\n",
                        'surprisal-election = true\nidentity = [{ role = "system", content = [{ type = "text", text = "You are Karl, a small local agent.\\nAnswer plainly." }] }]\n')
                decl.write_text(text)
                draft.unlink(missing_ok=True)
                before = tomllib.loads(text)
                result = self.migrate(decl)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertIn("moves into " + str(draft), result.stdout)
                self.assertEqual(decl.read_text(), text, "the check changes nothing")
                self.assertFalse(draft.exists())
                result = self.migrate(decl, "--apply")
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(draft.read_text(), "You are Karl, a small local agent.\nAnswer plainly.")
                self.assertEqual(draft.stat().st_mode & 0o077, 0)
                after = tomllib.loads(decl.read_text())
                del before["spu-instruction"]["decoder"]["identity"]
                self.assertEqual(after, before)
                self.assertNotIn("identity", decl.read_text())
                result = self.migrate(decl, "--apply")
                self.assertEqual((result.returncode, result.stdout), (0, ""), "nothing left to move")
        # The identity's line endings round-trip byte for byte: a text the
        # declaration wrote with CRLF escapes lands in the draft as CRLF, and
        # the standing draft then compares equal on a second run (Codex on
        # #92, round 4). Perturbation: read the standing draft in text mode
        # and the second run refuses, CRLF having read as LF; the write's
        # newline="" is symmetry, which Linux's text mode does not need.
        decl.write_text(self.KARL.replace(
            'text = """\nYou are Karl, a small local agent.\nAnswer plainly."""',
            'text = "one\\r\\ntwo\\rthree"'))
        draft.unlink(missing_ok=True)
        result = self.migrate(decl, "--apply")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(draft.read_bytes(), b"one\r\ntwo\rthree")
        decl.write_text(self.KARL.replace(
            'text = """\nYou are Karl, a small local agent.\nAnswer plainly."""',
            'text = "one\\r\\ntwo\\rthree"'))
        result = self.migrate(decl, "--apply")
        self.assertEqual(result.returncode, 0, "the standing draft compares equal as bytes: " + result.stderr)
        self.assertEqual(draft.read_bytes(), b"one\r\ntwo\rthree")

    def test_migrate_identity_refuses_what_it_cannot_move_losslessly(self):
        # Two messages, a non-text block, a draft already standing with other
        # text: each refuses naming the runbook, and nothing is written. An
        # empty identity is removed with no draft to write.
        self.decl.mkdir(parents=True)
        decl = self.decl / "agent.toml"
        draft = self.decl / "system-prompt.md"
        two = self.KARL.replace('[gate-instruction', '[[spu-instruction.decoder.identity]]\nrole = "system"\n\n'
                                '[[spu-instruction.decoder.identity.content]]\ntype = "text"\ntext = "second"\n\n[gate-instruction')
        for text, standing in ((two, None), (self.KARL, "another prompt\n")):
            with self.subTest(text=text[:40], standing=standing):
                decl.write_text(text)
                draft.unlink(missing_ok=True)
                if standing is not None:
                    draft.write_text(standing)
                for args in ((), ("--apply",)):
                    result = self.migrate(decl, *args)
                    self.assertEqual(result.returncode, 2, result.stdout)
                    self.assertIn("HowToDeployANewAgent.md section 3", result.stderr)
                    self.assertEqual(decl.read_text(), text, "nothing was written")
                    if standing is not None:
                        self.assertEqual(draft.read_text(), standing)
        # A prompt whose seeding line would pass the gate's bound cannot be
        # seeded, so it does not move (Codex on #92, round 3). Perturbation:
        # drop the measure and the declaration is rewritten.
        draft.unlink(missing_ok=True)
        decl.write_text(self.KARL.replace("You are Karl, a small local agent.", "x" * (32 * 1024)))
        for args in ((), ("--apply",)):
            result = self.migrate(decl, *args)
            self.assertEqual(result.returncode, 2, result.stdout)
            self.assertIn("past the gate's bound of 32768 octets", result.stderr)
            self.assertIn("identity", decl.read_text(), "nothing was written")
            self.assertFalse(draft.exists())
        draft.unlink(missing_ok=True)
        decl.write_text(self.KARL.replace(
            '[[spu-instruction.decoder.identity]]\nrole = "system"\n\n'
            '[[spu-instruction.decoder.identity.content]]\ntype = "text"\ntext = """\n'
            'You are Karl, a small local agent.\nAnswer plainly."""\n\n', "").replace(
            "surprisal-election = true\n", "surprisal-election = true\nidentity = []\n"))
        result = self.migrate(decl, "--apply")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("empty identity is removed", result.stdout)
        self.assertFalse(draft.exists())
        self.assertNotIn("identity", decl.read_text())

    def test_stack_plan_names_the_identity_migration_and_refuses_what_cannot_move(self):
        # The plan names each declaration whose identity would move, before
        # the build; one that cannot move losslessly refuses before the build,
        # naming the runbook step. Perturbation: drop the preflight and the
        # plan runs to the build in both cases.
        decl = self.existing_territory / "agent.toml"
        decl.write_text(self.KARL)
        result = self.run_script("update-stack.sh")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("existing     the identity's text moves into " + str(decl.parent / "system-prompt.md"), result.stdout)
        self.assertNotIn("no declaration carries the inline identity", result.stdout)
        self.assertEqual(decl.read_text(), self.KARL, "a plan moves nothing")
        self.assertFalse((decl.parent / "system-prompt.md").exists())
        decl.write_text(self.KARL.replace('[gate-instruction', '[[spu-instruction.decoder.identity]]\nrole = "system"\n\n'
                                          '[[spu-instruction.decoder.identity.content]]\ntype = "text"\ntext = "second"\n\n[gate-instruction'))
        self.log.unlink(missing_ok=True)
        result = self.run_script("update-stack.sh")
        self.assertEqual(result.returncode, 1, result.stdout)
        self.assertIn("deploy/REDEPLOY.md section 8, step 2", result.stderr)
        self.assertFalse(any(c[:2] == ["cargo", "build"] for c in self.calls()))

    def test_stack_refuses_a_trace_the_old_admin_recreated(self):
        # Codex on #82: an admin before #62 recreated a lost trace root:root,
        # which the new admin refuses at load and validate never sees, so the
        # plan refuses it before the build, naming the re-lay. Perturbation:
        # drop the preflight, and the run reaches the build.
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
        # what is required, and the runbook step. Perturbation: print a
        # command again, and a line starts with sudo.
        for fact in ("uid 0", "group nobody-group", "mode 640", "regular file",
                     "requires uid 0, group weaver-existing-trace, mode 640, a regular file and not a link",
                     "deploy/REDEPLOY.md section 8, step 3"):
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

    def test_stack_migrates_a_root_naming_a_declaration_directory(self):
        # The operator's ruling of 2026-10-07 on #1: a root still naming a
        # declaration-directory is an agent of the layout before it. The plan
        # names the move and changes nothing; a territory already holding an
        # agent.toml refuses by name; the install's move, run against a
        # stand-in sudo, moves the four files into the territory, makes
        # save-points/, joins the member and the operator to the access group,
        # regroups the territory, writes the root's territory key and removes
        # the old one. Perturbations: skip the refusal and both declarations
        # stand; leave the old key and the next run migrates again.
        old_root = self.config / "old"
        old_root.mkdir()
        (old_root / "worker-binary").write_text(str(self.root / "installed" / "pyworker"))
        old_dir = self.operator_home / ".weaveragent" / "old"
        old_dir.mkdir(parents=True)
        for name, text in (("agent.toml", "[state-store]\nengine = \"none\"\n"), ("system-prompt.md", "You are old.\n"),
                           ("admin.log", "{}\n"), ("worker.log", "w\n")):
            (old_dir / name).write_text(text)
        (old_root / "declaration-directory").write_text(str(old_dir) + "\n")
        territory = self.root / "agents" / "weaver-old"
        territory.mkdir()
        result = self.run_script("update-stack.sh")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn(f"old          layout: agent.toml, system-prompt.md, admin.log and worker.log move from {old_dir} into {territory}",
                      result.stdout)
        self.assertTrue((old_dir / "agent.toml").exists(), "a plan moves nothing")
        self.assertFalse((old_root / "territory").exists())
        self.assert_unprivileged()
        (territory / "agent.toml").write_text("")
        result = self.run_script("update-stack.sh")
        self.assertEqual(result.returncode, 1, result.stdout)
        self.assertIn("already holds an agent.toml", result.stderr)
        self.assertIn("REDEPLOY.md section 8, step 7", result.stderr)
        (territory / "agent.toml").unlink()
        # **Every occupied destination refuses, naming it, its bytes kept**
        # (Codex on #94): a territory already holding a log would have it
        # replaced by the move. Perturbation: judge agent.toml alone again and
        # this run plans the migration over the standing log.
        (territory / "admin.log").write_text("kept\n")
        result = self.run_script("update-stack.sh")
        self.assertEqual(result.returncode, 1, result.stdout)
        self.assertIn("already holds an admin.log", result.stderr)
        self.assertEqual((territory / "admin.log").read_text(), "kept\n")
        (territory / "admin.log").unlink()
        # The move itself, as the install runs it: the stand-in sudo runs the
        # file verbs and records the account and ownership verbs.
        stand_in = self.root / "stand-in"
        stand_in.mkdir()
        recorded = self.root / "recorded"
        (stand_in / "sudo").write_text(STAND_IN_SUDO.replace("{recorded}", str(recorded)))
        (stand_in / "sudo").chmod(0o755)
        text = (self.repo / "deploy" / "update-stack.sh").read_text()
        program = (lay_functions(text) + shell_function(text, "migrate_layout")
                   + 'MOVED=(); declare -A MOVED_FILES=(); rollback() { echo "ROLLBACK: $1" >&2; exit 1; }\n'
                   + 'migrate_layout "$1"; printf \'%s\\n\' "${MOVED[@]}" "${LAID[@]}"')
        env = {**os.environ, "PATH": f"{stand_in}{os.pathsep}{os.environ['PATH']}",
               "ADMIN_BASE": str(self.config), "OPERATOR_NAME": "fixture-no-home"}
        env.pop("BASH_ENV", None)
        ran = subprocess.run(["bash", "-c", program, "bash", f"old|{old_dir}|{territory}"],
                             env=env, text=True, capture_output=True, timeout=20)
        self.assertEqual(ran.returncode, 0, ran.stderr)
        moved, laid = ran.stdout.splitlines()[-2:]
        self.assertEqual(moved, f"old|{old_dir}|{territory}")
        self.assertTrue(laid.startswith(f"old|{territory}|"), laid)
        group, mode, save_points = laid.split("|")[2:]
        self.assertTrue(group and mode.isdigit(), "registered with the territory's group and mode for the rollback")
        self.assertEqual(save_points, "made", "a save-points/ the run made is registered as made")
        for name in ("agent.toml", "system-prompt.md", "admin.log", "worker.log"):
            self.assertTrue((territory / name).exists(), name)
            self.assertFalse((old_dir / name).exists(), name)
        self.assertEqual((territory / "system-prompt.md").read_text(), "You are old.\n")
        self.assertTrue((territory / "save-points").is_dir())
        self.assertEqual((old_root / "territory").read_text(), str(territory) + "\n")
        self.assertFalse((old_root / "declaration-directory").exists())
        said = recorded.read_text()
        for name in ("agent.toml", "system-prompt.md"):
            self.assertIn(f"chown -h root:weaver-old-admin {territory}/{name}", said)
            self.assertIn(f"0640 {territory}/{name}", said)
            self.assertEqual((territory / name).stat().st_mode & 0o7777, 0o640)
        self.assertIn(f"chown -h root:weaver-old-admin {territory}/admin.log", said)
        self.assertIn(f"0640 {territory}/admin.log", said)
        self.assertEqual((territory / "admin.log").stat().st_mode & 0o7777, 0o640)
        # The member is not joined (the custody audit's G11); the operator is.
        self.assertNotIn("usermod -aG weaver-old-admin weaver-old-state", said)
        self.assertIn("usermod -aG weaver-old-state,weaver-old-admin fixture-no-home", said)
        self.assertIn(f"chgrp weaver-old-state {territory}", said)
        self.assertIn(f"chmod 00710 {territory}", said)
        self.assertEqual(territory.stat().st_mode & 0o7777, 0o710)
        self.assertEqual((territory / "save-points").stat().st_mode & 0o7777, 0o750)
        # After the move the plan reads the declaration from the territory
        # and names no migration (the fixture's build directory cleared, as a
        # second build in one fixture needs).
        self.log.unlink(missing_ok=True)
        shutil.rmtree(self.root / 'target with "quotes"', ignore_errors=True)
        result = self.run_script("update-stack.sh")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("every declaration stands in its territory", result.stdout)

    def test_the_install_reads_a_migrated_territory_through_privilege(self):
        # Codex on #94, round 8: the migration joins the operator to the
        # access group, a login fact this running shell does not acquire, and
        # regroups the territory, so every read of a declaration or a sink
        # after it goes through sudo and never through the operator's groups.
        # The fake sudo locks the territory at the migration's chmod 00710,
        # harder than the real 0710 does, which passes nothing to an operator
        # shell that has not taken the new login
        # (LOCK_TERRITORY) and reopens it for its own reads alone, so the
        # reconcile and verify steps reach the declaration and the stand-in
        # admin's refused load rolls the install back; made as the operator, the same reads find no declaration
        # and the run ends at "no agent root ... could be verified".
        # Perturbation: make any read in the install path unprivileged again
        # (`[ -f "$decl" ]`, `cat "$decl"`, `declared` opening the path) and
        # "no declaration at" appears.
        old_root = self.config / "old"
        old_root.mkdir()
        (old_root / "worker-binary").write_text(str(self.root / "installed" / "pyworker"))
        old_dir = self.operator_home / ".weaveragent" / "old"
        old_dir.mkdir(parents=True)
        sink = self.root / "agents" / "weaver-old" / "trace.ndjson"
        (old_dir / "agent.toml").write_text(f'[state-store]\nengine = "none"\n\n[trace-sink]\nkind = "file"\npath = "{sink}"\n')
        (old_dir / "system-prompt.md").write_text("You are old.\n")
        # As create-agent wrote them before the ruling: the operator's, 0600.
        for name in ("agent.toml", "system-prompt.md"):
            (old_dir / name).chmod(0o600)
        (old_root / "declaration-directory").write_text(str(old_dir) + "\n")
        territory = self.root / "agents" / "weaver-old"
        territory.mkdir()
        # Laid out before the ruling, at a mode the law does not hold, so the
        # restore's chmod is told apart from the lay's.
        territory.chmod(0o750)
        shutil.rmtree(self.config / "existing")
        (self.root / "installed").mkdir(exist_ok=True)
        # An admin that validates and refuses the load, so the run reaches
        # the verify step's reads of the declaration and then rolls back.
        admin = self.root / "fixture-admin"
        admin.write_text("#!/bin/sh\n"
                         "case \"$1\" in\n"
                         "  validate) echo '{\"kind\":\"validated\"}' ;;\n"
                         "  unload) echo '{\"kind\":\"state\",\"state\":\"unloaded\"}' ;;\n"
                         "  *) echo '{\"kind\":\"refused\",\"reason\":\"no_residency\"}' ;;\n"
                         "esac\n")
        admin.chmod(0o755)
        self.env.update(ALLOW_APPLY_CHECKS="1", LOCK_TERRITORY=str(territory), TERRITORY_GROUP_AS="weaver-old-state",
                        FIXTURE_ADMIN=str(admin))
        try:
            result = self.run_script("update-stack.sh", "--install")
        finally:
            os.chmod(territory, 0o755)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("declaration, draft and logs moved from", result.stdout)
        self.assertNotIn("no declaration at", result.stdout, result.stdout + result.stderr)
        self.assertNotIn("could be verified", result.stderr)
        self.assertIn("old: the verify load answered", result.stderr)
        calls = self.calls()
        after = calls.index(["sudo", "chmod", "00710", str(territory)])
        self.assertIn(["sudo", "-n", "test", "-f", str(territory / "agent.toml")], calls[after:])
        self.assertIn(["sudo", "-n", "cat", "--", str(territory / "agent.toml")], calls[after:])
        # The rollback put the files back through the same privilege.
        self.assertTrue((old_dir / "agent.toml").exists(), "the rollback moved the declaration back")
        self.assertEqual((old_root / "declaration-directory").read_text(), str(old_dir) + "\n")
        self.assertFalse((old_root / "territory").exists())
        # **And the territory's group and mode as the fixture laid them out**
        # (Codex on #94, round 9): read before the move, put back after the
        # regroup, so the pre-ruling admin's member passes to its room again.
        # Perturbation: drop the chgrp from the restore and the group's call
        # is missing after the regroup.
        self.assertIn(["sudo", "stat", "-c", "%G %a", "--", str(territory)], calls[:after])
        self.assertIn(["sudo", "chgrp", "weaver-old-state", str(territory)], calls[after:])
        self.assertIn(["sudo", "chmod", "00750", str(territory)], calls[after:])
        # The migration lays the territory before it moves a file into it
        # (the security review of ec08f69). Perturbation: lay after the moves.
        first_move = next(i for i, c in enumerate(calls) if c[:2] == ["sudo", "mv"])
        self.assertLess(after, first_move)
        regroups = [i for i, c in enumerate(calls) if c == ["sudo", "chgrp", "weaver-old-state", str(territory)]]
        self.assertEqual(len(regroups), 2, "the migration's regroup, then the restore's")
        self.assertLess(regroups[0], after)
        # **And each moved file as the operator owned it** (Codex on #94,
        # round 12): its uid:gid and mode read before the move, put back
        # after the move back, so the operator reads and edits it as before
        # the install. Perturbation: drop the chown from the restore and the
        # owner's call is missing after the regroup.
        for name in ("agent.toml", "system-prompt.md"):
            self.assertIn(["sudo", "stat", "-c", "%u:%g %a", "--", str(old_dir / name)], calls[:after])
            self.assertIn(["sudo", "chown", "-h", "12345:12345", "--", str(old_dir / name)], calls[after:])
            # The mode through the no-follow helper (Codex on #94, round 14).
            self.assertTrue(any(c[:3] == ["sudo", "python3", "-c"] and c[-2:] == ["600", str(old_dir / name)]
                                for c in calls[after:]), name)
            self.assertEqual((old_dir / name).stat().st_mode & 0o7777, 0o600, name)

    def test_the_rollback_keeps_what_stands_in_the_old_directory(self):
        """**The layout rollback replaces nothing in the operator's directory**
        (the custody audit's G10): a file the operator's tooling made in the
        old directory during the install stands when the rollback moves the
        territory's copy back, and the move back declines, keeps both, names
        both and answers failure, as the move in does. Perturbation: move back
        with a plain `mv -T` again and the operator's file is replaced and the
        rollback answers success."""
        text = (DEPLOY / "update-stack.sh").read_text()
        old = self.root / "old-directory"
        territory = self.root / "territory"
        old.mkdir()
        territory.mkdir()
        (territory / "agent.toml").write_text("moved\n")
        (old / "agent.toml").write_text("the operator's\n")
        stand_in = self.root / "stand-in"
        stand_in.mkdir()
        recorded = self.root / "recorded"
        (stand_in / "sudo").write_text(STAND_IN_SUDO.replace("{recorded}", str(recorded)))
        (stand_in / "sudo").chmod(0o755)
        program = (shell_function(text, "chmod_nofollow") + shell_function(text, "move_no_clobber")
                   + shell_function(text, "move_files_back")
                   + 'declare -A MOVED_FILES=(["old|agent.toml"]="0:0 644")\n'
                   + 'move_files_back old "$1" "$2"')
        env = {**os.environ, "PATH": f"{stand_in}{os.pathsep}{os.environ['PATH']}"}
        env.pop("BASH_ENV", None)
        ran = subprocess.run(["bash", "-c", program, "bash", str(old), str(territory)],
                             env=env, text=True, capture_output=True, timeout=20)
        self.assertNotEqual(ran.returncode, 0, "a declined move back is a failure")
        self.assertEqual((old / "agent.toml").read_text(), "the operator's\n")
        self.assertEqual((territory / "agent.toml").read_text(), "moved\n")
        self.assertIn(str(territory / "agent.toml"), ran.stderr)
        self.assertIn(str(old / "agent.toml"), ran.stderr)

    def test_a_move_that_would_replace_fails_and_keeps_the_destination(self):
        """**A move onto an occupied destination fails, loudly, and replaces
        nothing** (Codex on #94): `move_no_clobber` declines with `mv -n` and
        reads the source still standing as the failure, so a destination made
        after the preflight keeps its bytes and the migration rolls back.
        Perturbation: move with `mv -T` again and the destination's bytes are
        replaced and the move answers success."""
        text = (DEPLOY / "update-stack.sh").read_text()
        source = self.root / "admin.log.source"
        source.write_text("moved\n")
        destination = self.root / "admin.log"
        destination.write_text("kept\n")
        program = 'sudo() { "$@"; }\n' + shell_function(text, "move_no_clobber") + 'move_no_clobber "$1" "$2"'
        env = {k: v for k, v in os.environ.items() if k != "BASH_ENV"}
        ran = subprocess.run(["bash", "-c", program, "bash", str(source), str(destination)],
                             env=env, text=True, capture_output=True, timeout=20)
        self.assertNotEqual(ran.returncode, 0, "a declined move is a failure")
        self.assertEqual(destination.read_text(), "kept\n")
        self.assertTrue(source.exists())
        destination.unlink()
        ran = subprocess.run(["bash", "-c", program, "bash", str(source), str(destination)],
                             env=env, text=True, capture_output=True, timeout=20)
        self.assertEqual(ran.returncode, 0, ran.stderr)
        self.assertEqual(destination.read_text(), "moved\n")
        self.assertFalse(source.exists())

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

    def test_the_no_follow_chmod_refuses_a_link_and_sets_a_regular_file(self):
        # Codex on #94, round 14: root sets a moved file's mode through a
        # descriptor opened O_NOFOLLOW, so a name made a link between any
        # check and the chmod changes no target. The helper is run as the
        # script defines it, with sudo standing in as the caller. Perturbation:
        # open without O_NOFOLLOW and the planted link's target is made 0644.
        text = (self.repo / "deploy" / "update-stack.sh").read_text()
        target = self.root / "elsewhere"
        target.write_text("sensitive\n")
        target.chmod(0o600)
        link = self.root / "system-prompt.md"
        link.symlink_to(target)
        regular = self.root / "agent.toml"
        regular.write_text("")
        regular.chmod(0o600)
        program = 'sudo() { "$@"; }\n' + shell_function(text, "chmod_nofollow") + 'chmod_nofollow "$1" "$2"'
        env = {k: v for k, v in os.environ.items() if k != "BASH_ENV"}
        ran = subprocess.run(["bash", "-c", program, "bash", "644", str(link)],
                             env=env, text=True, capture_output=True, timeout=20)
        self.assertNotEqual(ran.returncode, 0)
        self.assertIn("is a link", ran.stderr)
        self.assertEqual(target.stat().st_mode & 0o7777, 0o600, "the link's target keeps its mode")
        ran = subprocess.run(["bash", "-c", program, "bash", "644", str(regular)],
                             env=env, text=True, capture_output=True, timeout=20)
        self.assertEqual(ran.returncode, 0, ran.stderr)
        self.assertEqual(regular.stat().st_mode & 0o7777, 0o644)
        self.assertNotIn("test ! -L", text, "the separate link check is gone, the open being the check")

    def test_the_migration_refuses_a_link_in_the_old_directory(self):
        # Codex on #94, round 13: chmod has no no-dereference form, so a link
        # among the four entries would have root set its target's mode at the
        # move or the rollback. Every entry is judged a regular file and no
        # link before anything moves, in the plan as in the install, and the
        # target keeps its mode. Perturbation: drop the link judgment from the
        # preflight and the plan names the move.
        old_root = self.config / "old"
        old_root.mkdir()
        (old_root / "worker-binary").write_text(str(self.root / "installed" / "pyworker"))
        old_dir = self.operator_home / ".weaveragent" / "old"
        old_dir.mkdir(parents=True)
        (old_dir / "agent.toml").write_text("[state-store]\nengine = \"none\"\n")
        secret = self.root / "elsewhere-secret"
        secret.write_text("not the draft\n")
        secret.chmod(0o600)
        (old_dir / "system-prompt.md").symlink_to(secret)
        (old_root / "declaration-directory").write_text(str(old_dir) + "\n")
        territory = self.root / "agents" / "weaver-old"
        territory.mkdir()
        (self.root / "installed").mkdir(exist_ok=True)
        for mode in ((), ("--install",)):
            with self.subTest(mode=mode):
                self.log.unlink(missing_ok=True)
                self.env["ALLOW_APPLY_CHECKS"] = "1"
                result = self.run_script("update-stack.sh", *mode)
                self.assertEqual(result.returncode, 1, result.stdout)
                self.assertIn(str(old_dir / "system-prompt.md") + " is a link", result.stderr)
                self.assertNotIn("layout: agent.toml", result.stdout)
                self.assertFalse([c for c in self.calls() if c[0] in ("cargo", "sudo")], self.calls())
                self.assertTrue((old_dir / "system-prompt.md").is_symlink())
                self.assertTrue((old_dir / "agent.toml").exists())
                self.assertFalse((territory / "agent.toml").exists())
                self.assertEqual(secret.stat().st_mode & 0o777, 0o600, "the target keeps its mode")
        # A regular file that is not one (a directory under the name) refuses
        # the same way, and a dangling link is a link.
        (old_dir / "system-prompt.md").unlink()
        (old_dir / "system-prompt.md").mkdir()
        result = self.run_script("update-stack.sh")
        self.assertEqual(result.returncode, 1, result.stdout)
        self.assertIn(str(old_dir / "system-prompt.md") + " is not a regular file", result.stderr)
        (old_dir / "system-prompt.md").rmdir()
        (old_dir / "worker.log").symlink_to(self.root / "absent")
        result = self.run_script("update-stack.sh")
        self.assertEqual(result.returncode, 1, result.stdout)
        self.assertIn(str(old_dir / "worker.log") + " is a link", result.stderr)

    def test_stack_refuses_an_unprovided_engine_and_names_the_migration(self):
        # A pre-#85 declaration electing postgres refuses before the build,
        # as an engine this build does not provide, and points to the
        # migration step rather than to a feature the workspace lacks.
        # Perturbation: restore the old remedy and the step goes unnamed.
        decl = self.existing_territory / "agent.toml"
        decl.write_text('[state-store]\nengine = "postgres"\ndatabase = "d"\nrole = "r"\n')
        result = self.run_script("update-stack.sh")
        self.assertEqual(result.returncode, 1, result.stdout)
        self.assertIn("postgres store, which is not an engine this build provides", result.stderr)
        self.assertIn("REDEPLOY.md section 8 step 2", result.stderr)
        self.assertNotIn("Name weaver-state/postgres", result.stderr)

    def test_stack_agents_are_the_roots_under_the_base(self):
        # A staged root under a dot-name, a plain file, and a root naming no
        # territory are not agents. Perturbation: drop the territory check and
        # `undeclared` is listed.
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

    def test_the_lay_brings_a_setgid_territory_with_access_entries_to_the_law(self):
        """**The lay leaves exactly what admin judges** (#99 area 2 review,
        N1): a setgid territory carrying access and default entries comes out
        0710 with none, and `save-points/` 0750 with none, whether the run
        makes it or finds it setgid with entries of its own; the lay is
        registered with what stood, and the restore puts that back, the setgid
        bit and the entries with it. Run on real directories through the
        stand-in sudo. Perturbations: `chmod 0710` again and the territory
        reads 2710; `install -d ... -m 0750` again and a found save-points/
        reads 2750; drop either `setfacl -b` and an entry survives."""
        setfacl = shutil.which("setfacl", path="/usr/bin:/bin")
        if not setfacl:
            self.skipTest("no setfacl on this box")
        text = (DEPLOY / "update-stack.sh").read_text()
        stand_in = self.root / "stand-in"
        stand_in.mkdir()
        recorded = self.root / "recorded"
        (stand_in / "sudo").write_text(STAND_IN_SUDO.replace("{recorded}", str(recorded)))
        (stand_in / "sudo").chmod(0o755)
        env = {**os.environ, "PATH": f"{stand_in}{os.pathsep}{os.environ['PATH']}"}
        env.pop("BASH_ENV", None)
        me = str(os.getuid())
        look = ('python3 -c \'import os, sys\n'
                'for p in sys.argv[1:]:\n'
                '    acl = [n for n in os.listxattr(p, follow_symlinks=False) if n.startswith("system.posix_acl")]\n'
                '    print("LOOK", p, format(os.lstat(p).st_mode & 0o7777, "o"), ",".join(acl) or "-")\' "$@"\n')

        def entries(path):
            return [n for n in os.listxattr(path, follow_symlinks=False) if n.startswith("system.posix_acl")]

        for found in (False, True):
            with self.subTest(save_points_found=found):
                territory = self.root / f"lay-{found}" / "weaver-old"
                territory.mkdir(parents=True)
                territory.chmod(0o2770)
                if territory.stat().st_mode & 0o7777 != 0o2770:
                    self.skipTest("this filesystem keeps no setgid bit for this user")
                save_points = territory / "save-points"
                if found:
                    save_points.mkdir()
                    save_points.chmod(0o2750)
                    subprocess.run([setfacl, "-m", f"u:{me}:r", str(save_points)], check=True)
                subprocess.run([setfacl, "-m", f"u:{me}:x", "-m", f"d:u:{me}:rx", str(territory)], check=True)
                program = (lay_functions(text)
                           + 'rollback() { echo "ROLLBACK: $1" >&2; exit 1; }\n'
                           + 'lay_territory old "$1"\n'
                           + 'look() { ' + look + '}\n'
                           + 'look "$1" "$1/save-points"\n'
                           + 'printf \'LAID %s\\n\' "${LAID[@]}"\n'
                           + 'printf \'ACL %s\\n\' "${!LAID_ACL[@]}"\n'
                           + 'IFS="|" read -r a t g m sp <<< "${LAID[0]}"\n'
                           + 'restore_directory "$t" "$g" "$m"\n'
                           + '[ "$sp" = made ] || restore_directory "$t/save-points" "${sp%%:*}" "${sp##*:}"\n')
                ran = subprocess.run(["bash", "-c", program, "bash", str(territory)],
                                     env=env, text=True, capture_output=True, timeout=20)
                self.assertEqual(ran.returncode, 0, ran.stderr)
                looks = {line.split()[1]: line.split()[2:] for line in ran.stdout.splitlines() if line.startswith("LOOK ")}
                self.assertEqual(looks[str(territory)], ["710", "-"], ran.stdout)
                self.assertEqual(looks[str(save_points)], ["750", "-"], ran.stdout)
                laid = next(line for line in ran.stdout.splitlines() if line.startswith("LAID "))
                fields = laid.removeprefix("LAID ").split("|")
                self.assertEqual(fields[:2], ["old", str(territory)])
                self.assertEqual(fields[3], "2770", "the mode as found, setgid and all")
                self.assertEqual(fields[4].split(":")[-1] if found else fields[4], "2750" if found else "made")
                acls = sorted(line.removeprefix("ACL ") for line in ran.stdout.splitlines() if line.startswith("ACL "))
                self.assertEqual(acls, sorted([str(territory)] + ([str(save_points)] if found else [])))
                # The restore put back what stood: the setgid bit and the entries.
                self.assertEqual(territory.stat().st_mode & 0o7777, 0o2770)
                self.assertTrue(entries(territory), "the territory's entries are set again")
                if found:
                    self.assertEqual(save_points.stat().st_mode & 0o7777, 0o2750)
                    self.assertTrue(entries(save_points))

    def test_the_plan_judges_every_territory_and_sink_as_admin_does(self):
        """**Every territory a root names is judged before the build as admin
        judges it** (#99 area 2 review, N1): a setgid mode, an access entry,
        a missing `save-points/` are named in the plan as what the install
        lays; a lawful territory plans nothing; a link at the territory or at
        `save-points/`, and a sink whose directory is not the canonical
        territory, refuse by name before anything is built. Perturbations:
        compare the mode masked to three digits and 2710 goes unnamed; drop
        the access look and the entry goes unnamed; drop the sink comparison
        and the plan runs on."""
        setfacl = shutil.which("setfacl", path="/usr/bin:/bin")
        territory = self.existing_territory
        territory.chmod(0o2710)
        if territory.stat().st_mode & 0o7777 != 0o2710:
            self.skipTest("this filesystem keeps no setgid bit for this user")
        result = self.run_script("update-stack.sh")
        self.assertEqual(result.returncode, 0, result.stderr)
        line = next(l for l in result.stdout.splitlines() if l.startswith("  existing     territory:"))
        self.assertIn("mode 2710", line)
        self.assertIn("no save-points/", line)
        self.assertIn("the install lays it root:weaver-existing-state 0710", line)
        # Lawful: nothing planned.
        territory.chmod(0o710)
        (territory / "save-points").mkdir()
        (territory / "save-points").chmod(0o750)
        shutil.rmtree(self.root / 'target with "quotes"', ignore_errors=True)
        result = self.run_script("update-stack.sh")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("every territory a root names stands as admin judges it", result.stdout)
        if setfacl:
            subprocess.run([setfacl, "-m", f"u:{os.getuid()}:x", str(territory)], check=True)
            shutil.rmtree(self.root / 'target with "quotes"', ignore_errors=True)
            result = self.run_script("update-stack.sh")
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("carrying access entries", "".join(
                l for l in result.stdout.splitlines() if l.startswith("  existing     territory:")))
            subprocess.run([setfacl, "-b", str(territory)], check=True)
        # A sink elsewhere, and one under a link to the agent directory, refuse
        # before the build, naming the canonical territory.
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
                self.assertFalse([c for c in self.calls() if c[:2] == ["cargo", "build"]], self.calls())
        decl.write_text('[state-store]\nengine = "none"\n')
        # A link at save-points/, and a territory that is a link, refuse.
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

    def test_the_install_lays_a_root_naming_its_territory_and_the_rollback_puts_it_back(self):
        """**A root already naming its territory is laid when it is off the
        law, and the lay is registered for the restore** (#99 area 2 review,
        N1): the install regroups it, sets 00710, drops its entries and lays
        `save-points/` 00750, having read its group and mode first; the
        fixture's admin answers nothing, so the run rolls back and the
        restore sets the group and the mode, setgid included, as found.
        Perturbations: drop the lay step and no 00710 is set; drop the
        registration and the restore never sets 02750."""
        territory = self.existing_territory
        territory.chmod(0o2750)
        if territory.stat().st_mode & 0o7777 != 0o2750:
            self.skipTest("this filesystem keeps no setgid bit for this user")
        (self.root / "installed").mkdir(exist_ok=True)
        self.env.update(ALLOW_APPLY_CHECKS="1", TERRITORY_GROUP_AS="weaver-existing-old")
        result = self.run_script("update-stack.sh", "--install")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("existing refuses and this script will not guess the fix", result.stderr)
        self.assertIn("== lay territories", result.stdout)
        calls = self.calls()
        lay = calls.index(["sudo", "chmod", "00710", str(territory)])
        self.assertIn(["sudo", "stat", "-c", "%G %a", "--", str(territory)], calls[:lay])
        # **Closed before anything in it is looked at** (the security review
        # of ec08f69): the territory takes 00710 and drops its entries before
        # root looks at `save-points` or acts on it, so a member who could
        # write the old territory cannot swap a link in between.
        # Perturbation: look at save-points before the chmod and this fails.
        look = calls.index(["sudo", "test", "-L", str(territory / "save-points")])
        self.assertLess(calls.index(["sudo", "setfacl", "-b", "--", str(territory)]), look)
        self.assertLess(lay, look)
        self.assertIn(["sudo", "chgrp", "weaver-existing-state", str(territory)], calls[:lay])
        self.assertIn(["sudo", "setfacl", "-b", "--", str(territory)], calls[lay:])
        self.assertIn(["sudo", "install", "-d", "-o", "root", "-g", "weaver-existing-admin", "-m", "00750",
                       str(territory / "save-points")], calls[lay:])
        self.assertIn(["sudo", "chgrp", "weaver-existing-old", str(territory)], calls[lay:])
        self.assertIn(["sudo", "chmod", "02750", str(territory)], calls[lay:])

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

    def test_the_migration_rollback_gives_back_only_what_it_moved(self):
        """**The rollback undoes the run and nothing more** (#99 area 2
        review, K7): the migrated files go back to the operator's directory,
        a log admin made in the territory during the run stays where admin
        made it, the `save-points/` the run made is removed, and no failure
        is reported where the restore is whole. Perturbations: move back the
        fixed four names again and the run's log lands in the operator's
        directory with RESTORE INCOMPLETE; drop the removal and save-points/
        stands."""
        old_root = self.config / "old"
        old_root.mkdir()
        (old_root / "worker-binary").write_text(str(self.root / "installed" / "pyworker"))
        old_dir = self.operator_home / ".weaveragent" / "old"
        old_dir.mkdir(parents=True)
        territory = (self.root / "agents" / "weaver-old").resolve()
        territory.mkdir()
        (old_dir / "agent.toml").write_text('[state-store]\nengine = "none"\n')
        (old_dir / "system-prompt.md").write_text("You are old.\n")
        (old_root / "declaration-directory").write_text(str(old_dir) + "\n")
        shutil.rmtree(self.config / "existing")
        (self.root / "installed").mkdir(exist_ok=True)
        admin = self.root / "fixture-admin"
        admin.write_text("#!/bin/sh\n"
                         f"echo 'made by this run' >> '{territory}/admin.log'\n"
                         "echo 'weaver-admin: boundary unverified: a fixture refusal' >&2\n"
                         "echo '{\"kind\":\"boundary_unverified\"}'\n")
        admin.chmod(0o755)
        self.env.update(ALLOW_APPLY_CHECKS="1", FIXTURE_ADMIN=str(admin), TERRITORY_GROUP_AS="weaver-old-state")
        result = self.run_script("update-stack.sh", "--install")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("old refuses and this script will not guess the fix", result.stderr)
        self.assertNotIn("RESTORE INCOMPLETE", result.stderr)
        self.assertNotIn("FAILED", result.stderr)
        self.assertEqual((old_dir / "agent.toml").read_text(), '[state-store]\nengine = "none"\n')
        self.assertEqual((old_dir / "system-prompt.md").read_text(), "You are old.\n")
        self.assertFalse((old_dir / "admin.log").exists(), "the run's own log is not the operator's")
        self.assertEqual((territory / "admin.log").read_text(), "made by this run\n")
        self.assertFalse((territory / "save-points").exists(), "the save-points/ the run made is removed")

    def test_the_identity_move_in_a_territory_sets_only_what_stands_and_its_draft_goes_back(self):
        """**The identity move as root sets the owner of what stands, and the
        rollback removes the draft it made** (#99 area 2 review, K11 and K7):
        an empty `identity = []` writes no draft and is moved without
        aborting the install; a moved text's draft is removed by the
        restore, the declaration put back from its backup; and the
        territory is laid before the move reaches it (the security review
        of ec08f69). Perturbations: chown the draft unconditionally again and
        the empty identity aborts before the reconcile; drop the draft
        removal and the draft outlives the rollback."""
        territory = self.existing_territory
        decl = territory / "agent.toml"
        empty = ('session = "s"\n\n[spu-instruction.decoder]\nidentity = []\nsurprisal-election = true\n\n'
                 '[state-store]\nengine = "none"\n')
        (self.root / "installed").mkdir(exist_ok=True)
        for label, text in (("empty identity", empty), ("one system text", self.KARL)):
            with self.subTest(label):
                decl.chmod(0o644)
                decl.write_text(text)
                decl.chmod(0o444)
                (territory / "system-prompt.md").unlink(missing_ok=True)
                self.log.unlink(missing_ok=True)
                shutil.rmtree(self.root / 'target with "quotes"', ignore_errors=True)
                self.env.update(ALLOW_APPLY_CHECKS="1")
                try:
                    result = self.run_script("update-stack.sh", "--install")
                finally:
                    decl.chmod(0o644)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("existing refuses and this script will not guess the fix", result.stderr)
                self.assertNotIn("RESTORE INCOMPLETE", result.stderr)
                calls = self.calls()
                self.assertIn(["sudo", "chown", "-h", "root:weaver-existing-admin", "--", str(decl)], calls)
                self.assertLess(calls.index(["sudo", "chmod", "00710", str(territory)]),
                                calls.index(["sudo", "cp", "-a", str(decl), f"{decl}.pre-abcdef0-bak"]))
                self.assertEqual(decl.read_text(), text, "the declaration is put back")
                self.assertFalse((territory / "system-prompt.md").exists(), "no draft outlives the rollback")
                if label == "one system text":
                    self.assertIn(["sudo", "rm", "-f", "--", str(territory / "system-prompt.md")], calls)

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


def lay_functions(script):
    """update-stack.sh's territory lay and what it calls, as the script
    defines them, with root's reads through the stand-in sudo
    (`PRIVILEGED=1`) and the registries the restore reads declared."""
    as_root = next(line for line in script.splitlines() if line.startswith("as_root() {"))
    return ("PRIVILEGED=1\n" + as_root + "\n"
            + shell_function(script, "carries_access_entries")
            + shell_function(script, "chmod_nofollow") + shell_function(script, "move_no_clobber")
            + shell_function(script, "lay_territory") + shell_function(script, "restore_directory")
            + "LAID=(); declare -A LAID_ACL=()\n")


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
                                 str(roots / "a"), str(roots / "b"), str(roots / "c"))
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
                                 str(dotted / "b"), str(dotted / "a"))
            self.assertEqual(listed.splitlines(), [str(custom.resolve())])
        self.assertIn('for d in "${OWN_TERRITORIES[@]}"; do archive_path "$(archive_name territory "$d")" "$d"; done',
                      self.script)
        self.assertIn('mapfile -t OWN_TERRITORIES < <(territories_outside', self.script)

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
                                 text=True, capture_output=True, timeout=20)
            self.assertEqual(run.stdout, "", "neither key names its own agent's territory")
            self.assertIn(f"{roots / 'a'}/territory names {wrong.resolve()}, not weaver-a", run.stderr)
            self.assertIn(f"{roots / 'c'}/territory names {other.resolve()}, not weaver-c", run.stderr)

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
