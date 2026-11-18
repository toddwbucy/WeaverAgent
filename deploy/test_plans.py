#!/usr/bin/env python3
"""Deploy plan boundaries, using subprocess fixtures (no real sudo or build).

Run: python3 deploy/test_plans.py
The command doubles record every privileged/build invocation. They never
forward sudo, systemctl, package-manager or Cargo calls to the host.
"""

import atexit
import base64
import contextlib
import ctypes
import errno
import fcntl
import fnmatch
import hashlib
import json
import os
from pathlib import Path
import pwd
import shlex
import re
import resource
import shutil
import signal
import stat
import subprocess
import sys
import tempfile
import termios
import threading
import time
import unittest


DEPLOY = Path(__file__).resolve().parent
# The text create-agent.sh's `allows` hands `sh -c` for a look as an account
# (#577), and the call it makes.
ACCESS_LOOK = 'if [ "$1" "$2" ]; then printf yes; else printf no; fi'
# **The commands create-agent.sh runs as root steps** (#576), which the
# root-mode tests route to the double. A scan holds the script to this list,
# so a new root step cannot reach the real command in a root-mode test.
ROOT_STEPS = ("useradd", "groupadd", "usermod", "install", "chown", "chgrp", "chmod", "tee", "cp", "mv", "rm",
              "mktemp", "visudo", "chattr", "setquota", "repquota", "find")
# **The drop's clean start** (B3, #590): the PATH as_account hands a command
# run as another account, sudo's default secure_path, and the rule that tells
# a leading VAR=value from the command, as account-drop.sh and the double
# read it.
DROP_PATH = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"
DROP_NAMED = r"^[A-Za-z_][A-Za-z0-9_]*="
# **The programs no test reaches** (the Planner's rulings of 2026-11-18). Even
# as this account, each acts on the box through a privileged component, so
# the guard refuses and records each as it does sudo and runuser (make_guard),
# and no PATH a test hands over finds one outside the tests' temporary root
# (guard_path). Named, with what each reaches:
PRIVILEGED = (
    "systemctl",     # PID1's manager over the system bus, and the account's own manager
    "busctl",        # every service on the system bus, PID1 and logind among them
    "dbus-send",     # every service on the system bus, as busctl
    "gdbus",         # every service on the system bus, as busctl
    "systemd-run",   # a transient unit, from PID1 or from the account's own manager
    "loginctl",      # logind: sessions, seats, power off and reboot
    "machinectl",    # machined: machines and images, through PID1
    "virsh",         # libvirt's daemon: domains and the host's devices, a card's driver among them
    "virt-install",  # libvirt's daemon: new domains and the devices they take
    "nvidia-smi",    # the cards' driver: clocks, power limits, persistence and resets
    "modprobe",      # the kernel's modules
    "rmmod",         # the kernel's modules
    "insmod",        # the kernel's modules
)
# The families are read by pattern, never named: every program whose name
# starts with one of these, so a new libvirt, qemu or card program is covered
# without an edit. Of the cards' tools, nvidia-smi sets their clocks, power
# limits and persistence and resets them, and nvidia-modprobe loads their
# driver and makes their device files.
PRIVILEGED_FAMILIES = ("qemu-", "virt-", "nvidia-")


def privileged(name, path=None):
    """**The test of membership**, stated once: a named program, a name in a
    named family, or a name whose file at PATH is a named program's file on
    the box (PRIVILEGED_FILES), as poweroff, reboot, halt and shutdown are
    systemctl's. The file is read by resolving links, and a hard link is the
    same file, so no list of other names is kept. Where modprobe, rmmod and
    insmod are links to kmod, kmod's other names (kmod, lsmod, modinfo and
    depmod) join by the same rule, the read-only lsmod and modinfo among
    them."""
    return (name in PRIVILEGED or name.startswith(PRIVILEGED_FAMILIES)
            or (path is not None and _file_of(path) in PRIVILEGED_FILES))


def _file_of(path):
    try:
        st = os.stat(path)
    except (OSError, ValueError):
        return None
    return st.st_dev, st.st_ino


# **The box half of a PATH, as an image gives it** (#610): a directory of
# links to every program in /usr/bin but those named, never one that grants
# root, and the guard's copy of a privileged one, so a test sees what a
# deployed script does on a base that lacks a program the box has, and a
# call to a privileged program is refused and recorded.
ROOT_GRANTING = ("sudo", "sudoedit", "sudo-rs", "su", "runuser", "pkexec", "doas", "run0")
def box_view(where, without=()):
    where.mkdir()
    _refresh_files()
    for entry in os.scandir("/usr/bin"):
        if entry.name not in without and entry.name not in ROOT_GRANTING:
            os.symlink(view_link(entry.name, entry.path), where / entry.name)
    return where


def view_link(name, path):
    """**What a view links NAME to**, the one rule box_view, path_without's
    farm and IMAGE_VIEW share: the guard's refusing copy where NAME is
    privileged and PATH, if any, lies outside the tests' temporary root,
    made now if the start made none, else PATH."""
    if path is not None and _inside(path) or not privileged(name, path):
        return path
    return guard_copy(name)


_COPIES_MADE = set()


def guard_copy(name):
    """The guard's refusing copy of NAME, made now if the guard holds none.
    Whether a file stands under the name is read each time, since a test may
    remove a copy it made, and each copy made is kept in _COPIES_MADE
    (files_held). A stand-in's name never comes here: a stand-in that is a
    privileged program's file stops the run at its start."""
    copy = STAND_IN_GUARD / name
    if not copy.exists():
        refusing_copy(STAND_IN_GUARD, name, REACH_RECORD)
        _COPIES_MADE.add(copy)
    return copy


def look_as(account, flag, path):
    return ["sudo", "-n", "-u", account, "timeout", "0", "setsid", "-w", "env", "-i", "PATH=" + DROP_PATH,
            "sh", "-c", ACCESS_LOOK, "sh", flag, path]


# **No test reaches the box's own sudo** (B3, the Planner's ruling of
# 2026-11-17). Before this process starts a shell or a deploy script, the PATH
# it hands over, and any DROP_PATH, is read with `command -v`: sudo, runuser,
# setsid, env and timeout must each resolve inside the tests' temporary root,
# where every stand-in lives, or not at all, and the test is refused before
# anything runs otherwise. The suite's own PATH carries a guard directory
# ahead of the box's, whose sudo and runuser refuse and whose setsid, env and
# timeout are the box's, so a test that stands in neither meets the guard's
# first. A literal box tail is written GUARDED_USR_BIN. The privileged
# programs are held the same way: the guard refuses each, and each must
# resolve inside the temporary root, by its real path, or not at all.
BOX_PATH = os.environ.get("PATH", os.defpath)
STAND_IN_NAMES = ("sudo", "runuser", "setsid", "env", "timeout")
TEST_ROOT = os.path.realpath(tempfile.gettempdir())
_BOX_BINS = ("/usr/local/sbin", "/usr/local/bin", "/usr/sbin", "/usr/bin", "/sbin", "/bin")


def _inside(path):
    return os.path.commonpath([os.path.realpath(path), TEST_ROOT]) == TEST_ROOT


# The box's bin directories and the suite's PATH at its start, each once by
# its real path (/bin, /sbin and /usr/sbin are often /usr/bin).
_START_DIRS = tuple(dict.fromkeys(os.path.realpath(d) for d in _BOX_BINS + tuple(BOX_PATH.split(os.pathsep))
                                  if d and not _inside(d)))
# Each named program's file on the box, as device and inode, read again when
# a start directory changes, so an upgrade during a run that replaces one
# keeps its other names in the set.
PRIVILEGED_FILES = frozenset()
_FILES_READ = [None]


def _mtime(path):
    try:
        return os.stat(path).st_mtime_ns
    except OSError:
        return None


def _refresh_files():
    # The key is kept only once every start directory's last change is two
    # seconds old, as a directory's reading is (_members_of), so a file
    # replaced within one tick of the clock is never missed. Every
    # directory's reading is dropped when a start directory changes, since a
    # directory of links into one keeps its own time when a target changes.
    global PRIVILEGED_FILES
    key = tuple(_mtime(d) for d in _START_DIRS)
    if key != _FILES_READ[0]:
        PRIVILEGED_FILES = frozenset(f for d in _START_DIRS for n in PRIVILEGED
                                     for f in [_file_of(os.path.join(d, n))] if f)
        _MEMBERS_READ.clear()
        now = time.time_ns()
        _FILES_READ[0] = key if all(m is None or now - m > 2_000_000_000 for m in key) else None


# A directory's privileged programs, by its real path, read again whenever it
# changes, so a program installed during a run, or a stand-in a test adds, is
# covered. A reading is kept only once the directory's last change is two
# seconds old, so a change within one tick of the clock is never missed. A
# time ahead of the clock (a skewed file server, a clock stepped back) is
# not kept until the clock has passed it by two seconds, and is read on
# each child until then.
_MEMBERS_READ = {}


def _members_of(directory):
    real = os.path.realpath(directory)
    changed = _mtime(real)
    if changed is None:
        return ()
    held = _MEMBERS_READ.get(real)
    if held and held[0] == changed:
        return held[1]
    try:
        names = tuple(sorted(e.name for e in os.scandir(real) if privileged(e.name, e.path)))
    except OSError:
        names = ()
    if time.time_ns() - changed > 2_000_000_000:
        _MEMBERS_READ[real] = (changed, names)
    return names


def privileged_in(directories, cwd=None, guard_follows=False):
    """The named privileged programs, then each other privileged program that
    one of DIRECTORIES carries, each once, in the order found. An empty or
    relative directory is read against CWD, as a child started there reads
    it. With GUARD_FOLLOWS, each found in a directory outside the temporary
    root gets the guard's copy (guard_path)."""
    _refresh_files()
    base = os.path.abspath(cwd) if cwd is not None else os.getcwd()
    found = list(PRIVILEGED)
    for d in directories:
        members = _members_of(os.path.join(base, d or "."))
        if guard_follows and members and not _inside(os.path.join(base, d or ".")):
            for n in members:
                guard_copy(n)
        found += [n for n in members if n not in found]
    return tuple(found)


_IMAGE_LISTED = tuple(n.strip() for n in (DEPLOY / "image-programs.txt").read_text().splitlines()
                      if n.strip() and not n.strip().startswith("#"))
# The programs the guard refuses beside sudo and runuser, read at the start:
# each privileged program in a box bin directory or the suite's PATH, and each
# the image's list names, which the image view links to the guard's copy.
PRIVILEGED_NAMES = privileged_in(_START_DIRS)
# **A multi-call binary that is also a shell or a stand-in** (busybox, toybox)
# would make sh or env a privileged program by its file, so the guard could
# refuse neither without the other: the run stops at its start, named.
def shared_with_privileged(directories):
    return [n for n in ("sh", "bash") + STAND_IN_NAMES
            if privileged(n, shutil.which(n, path=os.pathsep.join(directories)))]


_SHARED = shared_with_privileged(_START_DIRS)
if _SHARED:
    sys.exit(f"test_plans: the box's {', '.join(_SHARED)} share a file with a named privileged program, so the "
             "guard cannot refuse one without the other, and no test runs")
PRIVILEGED_NAMES += tuple(n for n in _IMAGE_LISTED if privileged(n) and n not in PRIVILEGED_NAMES)
# **A stranger's uid is never the runner's** (Codex on #599): a stand-in that
# answers a path the tests made as not root's answers this uid, held by no
# fixture account, rather than the file's own owner, which is 0 when the suite
# runs as root.
STRANGER_UID = 54321


# **No test gains a privilege**: no_new_privs on this process before any
# test, which every child inherits and no exec clears. A setuid, setgid or
# file-capability program a test reaches, by PATH or by an absolute path,
# then runs with the runner's own credentials, so the box's sudo refuses at
# its start and acts as no one. No test needs such a program's privilege:
# the one setuid program a test runs is mount, in AccountDropTests, as root
# of its own user namespace already. Root holds every privilege before the
# flag is set, so a run as root gains nothing from it, and its end says so.
def _no_new_privs():
    libc = ctypes.CDLL(None, use_errno=True)
    if libc.prctl(38, 1, 0, 0, 0) != 0:  # PR_SET_NO_NEW_PRIVS
        sys.exit(f"test_plans: prctl(PR_SET_NO_NEW_PRIVS) failed: {os.strerror(ctypes.get_errno())}, so no test runs")
    if not no_new_privs_held():
        sys.exit("test_plans: /proc/self/status does not read NoNewPrivs 1, so no test runs")


def no_new_privs_held(status="/proc/self/status"):
    return re.search(r"^NoNewPrivs:\s*1$", Path(status).read_text(), re.M) is not None


_no_new_privs()
# **The suite keeps a record of every reach of a program the guard refuses**
# (sudo, runuser and PRIVILEGED_NAMES), under the tests' temporary root. Each
# appends its name, the running test's id and its arguments at a path written
# into its own text, so a drop that clears the environment still writes its
# line, with the test unknown. After the last test a line that stands fails
# the run even when every test passed, so no test can take exit 97 and hide a
# reach. The record's directory goes at exit when it holds no reach, and
# stays, named, when it holds one.
REACH_DIR = Path(tempfile.mkdtemp(prefix="weaver-test-reach-"))
REACH_RECORD = REACH_DIR / "reach-record"
REACH_RECORD.touch()


def reaches_in(record):
    """**The record's reaches**, as (name, test, arguments): one NUL-ended
    record each, so an argument's newline or tab stays inside its own. It is
    read as UTF-8 whatever the locale, and bytes that are not UTF-8 are shown
    escaped, so a reach is always counted and named the same way."""
    return [tuple((r.split("\t", 2) + ["", ""])[:3])
            for r in record.read_text(encoding="utf-8", errors="backslashreplace").split("\0") if r]


def say_reaches(reaches):
    for name, test, args in reaches:
        args = args.replace("\\", "\\\\").replace("\n", "\\n").replace("\t", "\\t")
        print(f"guard: {test} reached {name} {args}".rstrip(), file=sys.stderr)


# The count of reaches each finish() read, or None where the record was gone.
_FINISHED = []


def _fail_at_exit(said):
    print(said, file=sys.stderr)
    sys.stdout.flush()
    sys.stderr.flush()
    os._exit(1)


def _drop_an_empty_record():
    """At exit: the record's directory goes when it holds no reach. A reach
    finish() never read fails the process, named: under another runner
    (`python3 -m unittest`, pytest), which never calls it, or after it ran (an
    atexit callback). So does a record gone or unreadable that finish() did
    not report."""
    try:
        reaches = reaches_in(REACH_RECORD)
    except OSError as e:
        if _FINISHED[-1:] != [None]:
            _fail_at_exit(f"guard: the reach record {REACH_RECORD} cannot be read ({type(e).__name__}), so this "
                          "process cannot show that no test reached a program the guard refuses")
        return
    read = (_FINISHED[-1] or 0) if _FINISHED else 0
    if not reaches:
        shutil.rmtree(REACH_DIR, True)
    elif len(reaches) > read:
        say_reaches(reaches[read:])
        _fail_at_exit(f"guard: the record stays at {REACH_RECORD}, and "
                      + ("the run's end read it before these reaches" if _FINISHED else "this runner never read it"))


atexit.register(_drop_an_empty_record)


def refusing_copy(where, name, record):
    """NAME in WHERE: it appends its reach to RECORD and refuses with 97."""
    (where / name).write_text(
        "#!/bin/sh\n"
        '{ printf "%s\\t%s\\t" "${0##*/}" "${WEAVER_TEST_ID:-unknown}"; sep=; '
        'for a do printf "%s%s" "$sep" "$a"; sep=" "; done; printf "\\0"; } >> '
        + shlex.quote(str(record)) + "\n"
        'printf "a test reached %s outside its stand-ins\\n" "${0##*/}" >&2\n'
        "exit 97\n")
    (where / name).chmod(0o755)


def make_guard(where, record):
    """**A guard directory** in WHERE: a sudo, a runuser and each of
    PRIVILEGED_NAMES that append the reach to RECORD and refuse with 97, and
    the box's setsid, env and timeout."""
    for name in ("sudo", "runuser") + PRIVILEGED_NAMES:
        refusing_copy(where, name, record)
    for name in ("setsid", "env", "timeout"):
        if shutil.which(name, path=BOX_PATH):
            (where / name).symlink_to(shutil.which(name, path=BOX_PATH))
    return where


STAND_IN_GUARD = make_guard(Path(tempfile.mkdtemp(prefix="weaver-test-guard-")), REACH_RECORD)
atexit.register(shutil.rmtree, STAND_IN_GUARD, True)
os.environ["PATH"] = str(STAND_IN_GUARD) + os.pathsep + BOX_PATH
GUARDED_USR_BIN = str(STAND_IN_GUARD) + os.pathsep + "/usr/bin:/bin"
_GUARD_BASH = shutil.which("bash", path=BOX_PATH)
_REAL_POPEN = subprocess.Popen


class StandInGuardError(AssertionError):
    pass


def _resolved(path, names=STAND_IN_NAMES, cwd=None):
    # Read where the child starts, so a relative or empty entry finds what
    # the child finds, and each answer is made absolute against that place.
    probe = _REAL_POPEN([_GUARD_BASH, "--norc", "--noprofile", "-c",
                         'for n in "$@"; do command -v -- "$n" || echo; done', "guard", *names],
                        env={"PATH": path}, cwd=cwd, stdout=subprocess.PIPE, text=True)
    out, _ = probe.communicate(timeout=20)
    base = os.path.abspath(cwd) if cwd is not None else os.getcwd()
    return tuple((n, os.path.join(base, f) if f else f) for n, f in zip(names, out.split("\n")))


def guard_path(path, what, cwd=None):
    # A stand-in directory may link the box's setsid, env and timeout, as the
    # guard's own does, but never its sudo or runuser, nor a privileged
    # program: any that a directory of the PATH carries, by privileged()'s
    # test, read where the child starts. **The guard follows the box**: each
    # privileged program in a directory outside the temporary root gets the
    # guard's copy, made now if the start made none, so a program installed
    # during the run is refused where the guard comes first, and the PATH is
    # refused where it does not.
    names = STAND_IN_NAMES + privileged_in(path.split(os.pathsep), cwd, guard_follows=True)
    for name, found in _resolved(path, names, cwd):
        linked = name in ("setsid", "env", "timeout")
        if found and not (_inside(os.path.dirname(found)) and (linked or _inside(found))):
            raise StandInGuardError(f"the {what} a test hands over resolves {name} to {found}, outside the "
                                    f"stand-ins under {TEST_ROOT}: refused before it runs")


def guard_argv(argv, cwd=None, executable=None):
    """**No child is started on a privileged program outside the tests'
    temporary root by its path**, whether or not it runs a shell: each
    element of its command line, and its executable, that names a file is
    read by privileged()'s test, where the child starts. The executable is
    the program run, so it is read in argv[0]'s place. An `env` wrapper's
    own options are read as env reads them (env_hands), and the PATH it
    hands its command, the default PATH where it clears the environment or
    PATH and assigns none, is read by guard_path."""
    _refresh_files()
    base = os.path.abspath(cwd) if cwd is not None else os.getcwd()
    words = [os.fsdecode(a) for a in argv]
    if executable is not None:
        _guard_paths(words[:1], base)
        words = [os.fsdecode(executable)] + words[1:]
    _guard_line(words, base)


def _guard_line(words, base):
    # The line's paths, then the first `env` wrapper's, whose command line, a
    # nested `env` in it included, is read in turn.
    _guard_paths(words, base)
    for i, w in enumerate(words):
        if os.path.basename(w) == "env":
            path, command = env_hands(words[i + 1:])
            if path == os.defpath:
                guard_path(path, "PATH, the default one an `env` wrapper's -i, - or -u PATH leaves,", base)
            elif path is not None:
                guard_path(path, "PATH an `env` wrapper assigns", base)
            _guard_line(command, base)
            return


def _guard_paths(words, base):
    for a in words:
        found = os.path.join(base, a) if "/" in a else None
        if found and os.path.isfile(found) and privileged(os.path.basename(a), found) and not _inside(found):
            raise StandInGuardError(f"a child's command line names {a}, which is {found}, a privileged program "
                                    f"outside the stand-ins under {TEST_ROOT}: refused before it runs")


# **env's options as GNU env 9.12 reads them** (its --help), each long one
# by its short option, or its own name where it has none, and whether it
# takes an argument (True), takes none (False) or takes one only after "="
# ("="). A long option may be shortened to any prefix that names one
# alone. An option outside the table, or an ambiguous shortening, refuses
# the line, so an option a later env adds fails closed rather than moving
# the reading by a word, and a test holds the table to the box's env.
_ENV_LONG = {"--argv0": ("a", True), "--ignore-environment": ("i", False), "--env0-from": ("--env0-from", True),
             "--null": ("0", False), "--unset": ("u", True), "--chdir": ("C", True),
             "--split-string": ("S", True), "--block-signal": ("--block-signal", "="),
             "--default-signal": ("--default-signal", "="), "--ignore-signal": ("--ignore-signal", "="),
             "--list-signal-handling": ("--list-signal-handling", False), "--debug": ("v", False),
             "--help": ("--help", False), "--version": ("--version", False)}
_ENV_SHORT = {opt: takes for opt, takes in _ENV_LONG.values() if len(opt) == 1}
# The options no line may hand env, which this guard does not read: each
# would need env's own grammar or a file read, and no test hands one.
_ENV_REFUSED = {"S": "-S, which splits its string into words by env's own grammar",
                "C": "-C, which starts its command in another directory",
                "--env0-from": "--env0-from, which reads its environment from a file"}


def env_hands(words):
    """**What an `env` wrapper hands its command**, from WORDS, the words
    after `env`, read as env reads them: the PATH its command runs with
    (the default PATH where `-i`, `-` or `-u PATH` clears it and no PATH=
    follows, a PATH= it assigns, or None where it keeps the one it was
    handed), and the command line it runs. Short options group (`-iv`),
    and an argument may be the option's own word or the next. An option of
    _ENV_REFUSED, one outside the table, and an ambiguous shortening refuse
    the line."""
    todo, path = list(words), None

    def refuse(what):
        raise StandInGuardError(f"a child's command line hands env {what}, which this guard does not read as env "
                                "does: refused before it runs")

    def argument(rest):
        return rest if rest else todo.pop(0) if todo else ""

    while todo and todo[0].startswith("-") and todo[0] != "-":
        word = todo.pop(0)
        if word == "--":
            break
        if word.startswith("--"):
            name, eq, value = word.partition("=")
            named = [name] if name in _ENV_LONG else [k for k in _ENV_LONG if k.startswith(name)]
            if len(named) != 1:
                refuse(f"the option {name}, {'an ambiguous shortening' if named else 'not one of its options'}")
            opt, takes = _ENV_LONG[named[0]]
            if eq and takes is False:
                refuse(f"the option {named[0]} with an argument it does not take")
            opts = [(opt, value if eq else argument("") if takes is True else None)]
        else:
            opts, rest = [], word[1:]
            while rest:
                opt, rest = rest[0], rest[1:]
                if opt not in _ENV_SHORT:
                    refuse(f"the option -{opt}, not one of its options")
                if _ENV_SHORT[opt]:
                    opts.append((opt, argument(rest)))
                    break
                opts.append((opt, None))
        for opt, value in opts:
            if opt in _ENV_REFUSED:
                refuse(_ENV_REFUSED[opt])
            if opt == "i" or opt == "u" and value == "PATH":
                path = os.defpath
    if todo and todo[0] == "-":
        path = os.defpath
        todo.pop(0)
    while todo and "=" in todo[0]:
        word = todo.pop(0)
        if word.startswith("PATH="):
            path = word[len("PATH="):]
    return path, todo


SHELLS = ("bash", "sh", "dash", "ash", "ksh", "mksh", "zsh", "rbash", "csh", "tcsh", "fish")


def _runs_a_script(args):
    # A shell, by any of its names, or a deploy script anywhere in the command
    # line, so one started under a wrapper (unshare, timeout, env) is read as
    # well.
    argv = [str(a) for a in ([args] if isinstance(args, (str, os.PathLike)) else args)]
    return any(os.path.basename(a) in SHELLS or a.endswith(".sh") or a.startswith(str(DEPLOY))
               for a in argv)


def _socket_input(stdin):
    # The input a child starts with: the one named, else this process's own.
    if stdin is None:
        fd = 0
    elif isinstance(stdin, int):
        if stdin < 0:
            return False  # PIPE, DEVNULL, STDOUT
        fd = stdin
    else:
        try:
            fd = stdin.fileno()
        except (AttributeError, OSError, ValueError):
            return False
    try:
        return stat.S_ISSOCK(os.fstat(fd).st_mode)
    except OSError:
        return False


def guard_shell_files(stdin, env):
    # **Nothing a shell reads after this guard may rewrite its PATH**: bash
    # whose standard input is a socket reads ~/.bashrc, as one sshd started,
    # and a non-interactive bash reads BASH_ENV, so either could put the box's
    # sudo first after guard_path has read the PATH. A BASH_ENV a test writes
    # lies under the tests' temporary root.
    if _socket_input(stdin):
        raise StandInGuardError("a test hands a shell or a deploy script a socket as its standard input, and bash "
                                "then reads ~/.bashrc, which can rewrite the PATH this guard read: refused before "
                                "it runs")
    if env.get("BASH_ENV") and not _inside(env["BASH_ENV"]):
        raise StandInGuardError(f"a test hands a shell or a deploy script BASH_ENV {env['BASH_ENV']}, outside "
                                f"{TEST_ROOT}, which bash reads after this guard read the PATH: refused before it "
                                "runs")


FIXTURE_ENVIRON = "fixture-environ.json"


def _hand_settings(env):
    # **A drop clears the environment** (#590): as_account starts runuser and
    # sudo under `env -i`, so a stand-in for either reads the run's settings
    # from FIXTURE_ENVIRON beside it, written here for every directory of the
    # run's PATH under the tests' temporary root that holds one. A test runs
    # its scripts one at a time, so each run's file is its own.
    for d in env.get("PATH", "").split(os.pathsep):
        if d and _inside(d) and any(os.path.isfile(os.path.join(d, n)) for n in ("sudo", "runuser")):
            with open(os.path.join(d, FIXTURE_ENVIRON), "w") as settings:
                json.dump(dict(env), settings)


# **A copied script's child sees only the image's programs.** The files the
# appliance image copies (deploy/deployed-copy.txt, #607) run there on the
# image's PATH alone. So a child that runs one of them, by its path or name or
# as an excerpt of its text, has each box bin directory of its PATH, and of a
# DROP_PATH handed to it, replaced by IMAGE_VIEW: a link to each program
# deploy/image-programs.txt names that the box carries, never a root-granting
# one, and the guard's copy of each privileged one the list names, carried or
# not, so `command -v` finds it as on the image and a call is refused and
# recorded. Any other directory outside the tests' temporary root goes, and a
# stand-in directory stays where the test put it. A child drawing on any other
# deploy script, which runs on a build box, keeps its PATH. A call to a
# program the image lacks then fails as it would there, on a line a test runs.
# ProgramCheckTests reads the lines no test runs, and every call through
# as_account, whose DROP_PATH account-drop.sh assigns itself on purpose
# (#590), so no caller's environment chooses the PATH the drop runs with.
DEPLOYED_COPY = frozenset(n.strip() for n in (DEPLOY / "deployed-copy.txt").read_text().splitlines()
                          if n.strip() and not n.lstrip().startswith("#"))
IMAGE_VIEW = STAND_IN_GUARD / "image-view"
IMAGE_VIEW.mkdir()
_NO_PROGRAMS = STAND_IN_GUARD / "no-programs"
_NO_PROGRAMS.mkdir()


def image_link(name, bins=_BOX_BINS):
    """What IMAGE_VIEW links a listed program to: the guard's copy of a
    privileged one, whether BINS carry it or not, else the program BINS
    carry, else nothing."""
    return view_link(name, shutil.which(name, path=os.pathsep.join(bins)))


for _name in _IMAGE_LISTED:
    if _name not in ROOT_GRANTING and (_link := image_link(_name)):
        (IMAGE_VIEW / _name).symlink_to(_link)
_BOX_BIN_REAL = frozenset(os.path.realpath(d) for d in _BOX_BINS)
_DEPLOY_FILES = sorted(p.name for p in DEPLOY.iterdir()
                       if p.suffix in (".sh", ".py") and p.name not in ("test_plans.py", "program_check.py"))
_DEPLOY_PATHS = re.compile(r"/(" + "|".join(map(re.escape, _DEPLOY_FILES)) + r")(?![\w.-])")
_DEPLOY_NAMES = frozenset(_DEPLOY_FILES)
_DEPLOY_LINES = {}
for _name in _DEPLOY_FILES:
    for _line in (DEPLOY / _name).read_text().splitlines():
        if len(_line.strip()) >= 30 and not _line.lstrip().startswith("#"):
            _DEPLOY_LINES.setdefault(_line.strip(), set()).add(_name)


def drawn_from(args):
    """The deploy files a command line runs: each named by a path, and each
    whose text a code string carries a line of."""
    names = set()
    for a in ([args] if isinstance(args, (str, os.PathLike)) else args):
        a = str(a)
        names.update(_DEPLOY_PATHS.findall(a))
        if a in _DEPLOY_NAMES:
            names.add(a)  # run by its bare name, from deploy/ as the working directory
        for line in a.splitlines():
            # A line a copied script carries draws that script alone, though a
            # build script carries it too (the helpers say, plan and die).
            hit = _DEPLOY_LINES.get(line.strip(), set())
            names |= (hit & DEPLOYED_COPY) or hit
    return names


def image_path(path):
    out = []
    for entry in path.split(os.pathsep):
        if entry and os.path.realpath(entry) in _BOX_BIN_REAL:
            if str(IMAGE_VIEW) not in out:
                out.append(str(IMAGE_VIEW))
        elif entry and _inside(entry):
            out.append(entry)
    return os.pathsep.join(out) or str(_NO_PROGRAMS)


def _image_view(args, kwargs):
    drawn = drawn_from(args)
    if not drawn or not drawn <= DEPLOYED_COPY:
        return args, kwargs
    env = dict(os.environ if kwargs.get("env") is None else kwargs["env"])
    for name in ("PATH", "DROP_PATH"):
        if name in env or name == "PATH":
            env[name] = image_path(env.get(name, os.defpath))
    if not isinstance(args, (str, os.PathLike)):
        # A PATH an `env` wrapper hands on: a bare assignment, never a code
        # string that starts with one.
        args = [f"{str(a).split('=', 1)[0]}={image_path(str(a).split('=', 1)[1])}"
                if re.fullmatch(r"(DROP_)?PATH=\S*", str(a)) else a for a in args]
    return args, {**kwargs, "env": env}


class _GuardedPopen(_REAL_POPEN):
    def __init__(self, args, *rest, **kwargs):
        # **A run that names no input reads /dev/null.** bash whose standard
        # input is a socket reads ~/.bashrc as one sshd started, which can
        # rewrite the PATH a test hands it, and the runner's own input may be
        # one: a check of a PATH lacking runuser found the box's that way, now
        # and then, until this.
        if len(rest) < 3 and kwargs.get("stdin") is None:
            kwargs["stdin"] = subprocess.DEVNULL
        if _runs_a_script(args):
            args, kwargs = _image_view(args, kwargs)
        if kwargs.get("env") is not None:
            _hand_settings(kwargs["env"])
        # **Every child's PATH is read**, a shell's or not: a child that runs
        # code (python3 -c, make) starts its own children on the PATH it is
        # handed, out of this process's sight. A shell=True child is a shell.
        env = os.environ if kwargs.get("env") is None else kwargs["env"]
        cwd = None if kwargs.get("cwd") is None else os.fsdecode(kwargs["cwd"])
        guard_path(env.get("PATH", os.defpath), "PATH", cwd)
        if "DROP_PATH" in env:
            guard_path(env["DROP_PATH"], "DROP_PATH", cwd)
        # A PATH an `env` wrapper hands on in the command line itself.
        for a in ([] if isinstance(args, (str, bytes, os.PathLike)) else args):
            if os.fsdecode(a).startswith("PATH="):
                guard_path(os.fsdecode(a)[5:], "PATH", cwd)
        if _runs_a_script(args) or kwargs.get("shell", len(rest) > 7 and rest[7]):
            guard_shell_files(rest[2] if len(rest) > 2 else kwargs.get("stdin"), env)
        executable = kwargs.get("executable", rest[1] if len(rest) > 1 else None)
        guard_argv([args] if isinstance(args, (str, bytes, os.PathLike)) else list(args), cwd, executable)
        super().__init__(args, *rest, **kwargs)


subprocess.Popen = _GuardedPopen
DOUBLE = r'''#!/usr/bin/env python3
import json, os, pathlib, re, shutil, stat, subprocess, sys, time
name = pathlib.Path(sys.argv[0]).name
args = sys.argv[1:]
# **A drop starts the double clean** (#590): as_account hands runuser and sudo
# `env -i` with DROP_PATH alone, so the run's settings, which the suite writes
# beside the double, come back here, PATH among them.
if 'FIXTURE_ROOT' not in os.environ:
    settings = pathlib.Path(sys.argv[0]).parent / '@FIXTURE_ENVIRON@'
    if settings.exists(): os.environ.update(json.loads(settings.read_text()))
root = pathlib.Path(os.environ['FIXTURE_ROOT'])
def owner(path, st):
    """uid 0 under the fixture, STRANGER_UID for any other path the tests
    made, and the file's own owner elsewhere, so a stranger's file is never
    read as root's when the suite runs as root (Codex on #599)."""
    if path.is_relative_to(root): return 0
    if path != pathlib.Path('@TEST_ROOT@') and path.is_relative_to('@TEST_ROOT@'): return @STRANGER_UID@
    return st.st_uid
if name == 'stat':
    # **The fixture's files are root's.** Every file the tests make is the
    # tester's, so the ownership `held_closed` judges is answered as uid 0
    # for anything under the fixture, STRANGER_UID for any other file the
    # tests made, and read from the file itself elsewhere; the mode is always
    # the file's own. Not logged: it is a look, not a call.
    if args[:1] == ['-c'] and args[1] == '%u %G %a':
        # The trace's look: root under the fixture, its group what the test
        # names, its mode the file's own.
        path = pathlib.Path(args[-1])
        st = os.lstat(path)
        print(owner(path, st), os.environ.get('TRACE_GROUP_AS', 'nobody-group'),
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
        print(f"{owner(path, st)}:{group}:{format(st.st_mode & 0o7777, 'o')}")
        sys.exit(0)
    if args[:1] == ['-c'] and args[1] == '%u %a':
        path = pathlib.Path(args[-1])
        st = os.lstat(path)
        print(owner(path, st), format(st.st_mode & 0o7777, 'o'))
        sys.exit(0)
    os.execv('/usr/bin/stat', ['stat', *args])
if name == 'findmnt':
    # **The mount under a path** (#585), a look and not logged: FIXTURE_MOUNT
    # is "fstype|options|target", and without it a filesystem with no
    # project quota answers.
    fstype, options, target = os.environ.get('FIXTURE_MOUNT', 'xfs|rw,noquota|/').split('|')
    print({'FSTYPE': fstype, 'OPTIONS': options, 'TARGET': target}[args[args.index('-o') + 1]])
    sys.exit(0)
if name == 'mktemp' and args and '/weaver-deploy-messages-' in args[-1]:
    # The build's messages file, made as the scripts make it; not logged.
    os.execv('/usr/bin/mktemp', ['mktemp', *args])
if (name == 'mktemp' and args or name == 'as-root' and args[:1] == ['mktemp']) and '/weaver-drop.' in args[-1]:
    # **A drop's relays' directory** (#590), which account-drop.sh makes
    # under TMPDIR in either mode, root mode's routing included: made as the
    # helper makes it, and not logged, since it is no root step.
    os.execv('/usr/bin/mktemp', ['mktemp', *args[args.index('mktemp') + 1:]] if name == 'as-root' else ['mktemp', *args])
if (name == 'as-root' and args[:3] == ['rm', '-rf', '--'] and len(args) == 4
        and os.path.normpath(os.path.dirname(args[3])) == os.path.normpath(os.environ.get('TMPDIR') or '/tmp')
        and os.path.basename(args[3]).startswith('weaver-drop.')):
    # **And removed as it came** (the clean-up after #598): the helper
    # removes the directory once its pipes are open, which in root mode
    # reaches the double as `as-root rm -rf -- DIR`. Removed as the helper
    # removes it, and not logged, since it is no root step. Without this the
    # double's own rm refused it, and every root-mode drop left one in /tmp.
    os.execv('/usr/bin/rm', ['rm', *args[1:]])
with open(os.environ['CALLS'], 'a') as log:
    log.write(json.dumps([name, *args]) + '\n')
# **Root mode** (#576): a root step root runs directly reaches the double as
# `as-root <command>...`, through the root-mode tests' shell functions, and a
# look as an account as `runuser -u <account> -- <command>...`. Each is logged
# as it came, then answered as the same step through sudo would be.
if name == 'as-root':
    name = 'sudo'
via_runuser = False
# **Every drop as_account makes starts clean** (B3, #590): runuser is handed
# `env -i PATH=<DROP_PATH>` and the VAR=value pairs the script names, by the
# helper's own rule, and sudo the same after `timeout 0 setsid -w`, so the
# account's env starts with no terminal. Each is checked and set aside here,
# so the double answers the command the account would run.
DROP_PATH = '@DROP_PATH@'
DROP_NAMED = re.compile(r'@DROP_NAMED@')
with open('/proc/self/environ', 'rb') as _block:
    STARTED_WITH = [item for item in _block.read().split(b'\0') if item]
def clean_start(command, head):
    # The drop's own block: runuser or sudo started under `env -i` with
    # DROP_PATH alone (#590), read as it was handed, before the settings
    # above came back.
    assert STARTED_WITH == [b'PATH=' + DROP_PATH.encode()], ('a drop handed the caller\'s environment', STARTED_WITH)
    assert command[:len(head)] == head, ('a drop without its session and its relay', command)
    command = command[len(head):]
    want = ['env', '-i', 'PATH=' + DROP_PATH]
    assert command[:len(want)] == want, ('a drop that does not start clean', command)
    command = command[len(want):]
    while command and DROP_NAMED.match(command[0]): command = command[1:]
    return command
if name == 'runuser':
    assert args[0] == '-u' and args[2] == '--', ('runuser as create-agent.sh calls it', args)
    name, args, via_runuser = 'sudo', ['-n', '-u', args[1], *clean_start(args[3:], [])], True
def mapped(value):
    if value.startswith('/home/') or value.startswith('/etc/sudoers') or value.startswith('/etc/ld.so.conf.d/'):
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

# **chattr as ext4 answers it** (#600): flags and a project are set on a
# regular file or a directory alone, and the project inherit flag, `+P`, on a
# directory alone. Any other file, a link named outright among them, is
# refused with EOPNOTSUPP in chattr's words, and the run goes on to the next
# path and fails at the end, as chattr does. A path that does not stand fails
# its stat. `-R` walks what a directory holds without following a link. What
# each path takes is kept in ATTRS, the fixture's record of the drive's
# attributes, by path: its project and whether it carries the inherit flag.
ATTRS = root / 'attrs.json'
def chattr_model(argv):
    recursive, project, flags, paths, i = False, None, [], [], 0
    while i < len(argv):
        a = argv[i]
        if a == '-R': recursive = True
        elif a == '-p': project, i = argv[i + 1], i + 1
        elif a[:1] in '+-=' and len(a) > 1 and a[1:].isalpha(): flags.append(a)
        else: paths.append(a)
        i += 1
    attrs = json.loads(ATTRS.read_text()) if ATTRS.exists() else {}
    failed = False
    def one(path, named):
        nonlocal failed
        try: st = os.lstat(path)
        except OSError as e:
            print(f'chattr: {os.strerror(e.errno)} while trying to stat {path}', file=sys.stderr)
            failed = True
            return
        if stat.S_ISLNK(st.st_mode) and not named: return
        regular, directory = stat.S_ISREG(st.st_mode), stat.S_ISDIR(st.st_mode)
        if not (regular or directory) or ('+P' in flags and not directory):
            print(f'chattr: Operation not supported while setting {"flags" if flags else "project"} on {path}',
                  file=sys.stderr)
            failed = True
            return
        held = attrs.setdefault(path, {'project': 0, 'inherit': False})
        if '+P' in flags: held['inherit'] = True
        if '-P' in flags: held['inherit'] = False
        if project is not None: held['project'] = int(project)
        if recursive and directory:
            for entry in sorted(os.listdir(path)): one(os.path.join(path, entry), False)
    for path in paths:
        path = mapped(path)
        assert pathlib.Path(path).is_relative_to(root), ('chattr outside the fixture', path)
        privileged([path], lambda: one(path, True))
    ATTRS.write_text(json.dumps(attrs))
    return 1 if failed else 0

# **A look made as an account** (#577): create-agent.sh's `allows` hands
# exactly this text to `sh -c`, so the shell's builtin `[` answers it. It is
# the suite's ACCESS_LOOK, written in when the double is.
ACCESS_LOOK = @ACCESS_LOOK@
def account_answer(identity, flag):
    """What the kernel would answer for `identity` under the fixture: the
    agent's own uid enters neither its state room nor its territory unless
    WALL_OPEN, the member reads neither the trace nor the declaration unless
    TRACE_OPEN, and the admin-con account writes nothing of the territory
    unless GROUP_WRITES. Every other look passes."""
    if identity == 'weaver-m1': return bool(os.environ.get('WALL_OPEN'))
    if identity == 'weaver-m1-state' and flag == '-r': return bool(os.environ.get('TRACE_OPEN'))
    if identity == 'weaver-admincon' and flag == '-w': return bool(os.environ.get('GROUP_WRITES'))
    return True

def shell_read(arguments):
    if os.environ.get('PATH_FAIL') and os.environ['PATH_FAIL'] in arguments: sys.exit(1)
    rewritten = list(map(mapped, arguments))
    for argument in rewritten:
        if argument.startswith('/'):
            assert pathlib.Path(argument).is_relative_to(root), argument
    sys.exit(subprocess.run(['/bin/sh', *rewritten]).returncode)

# **The box's two connector accounts** (weaver-admin-Spec section 2): absent
# unless BOX_PASSWD names one (a JSON map of name to passwd line) or a sudo
# useradd in this fixture laid it, which is recorded beside the fixture.
BOX_ACCOUNTS = {'weaver-gatecon': 990, 'weaver-admincon': 991}
def laid_box_accounts():
    laid = root / 'laid-box-accounts'
    return laid.read_text().split() if laid.exists() else []
def box_passwd():
    lines = {n: f"{n}:x:{u}:{u}::/home/{n}:/usr/sbin/nologin" for n, u in BOX_ACCOUNTS.items() if n in laid_box_accounts()}
    lines.update(json.loads(os.environ.get('BOX_PASSWD', '{}')))
    return lines
def box_group(key):
    groups = {str(u): f"{n}:x:{u}:" for n, u in BOX_ACCOUNTS.items()}
    groups.update({n: f"{n}:x:{u}:" for n, u in BOX_ACCOUNTS.items()})
    groups.update(json.loads(os.environ.get('BOX_GROUP', '{}')))
    return groups.get(key)

if name == 'sh': shell_read(args)
elif name == 'getent':
    user = os.environ.get('USER', '')
    if args == ['passwd', user]:
        uid = os.environ.get('FIXTURE_UID', '12345')
        print(f"{user}:x:{uid}:{uid}::{root / 'home' / user}:/bin/bash")
        sys.exit(0)
    if args == ['passwd']:
        uid = os.environ.get('FIXTURE_UID', '12345')
        print(f"{user}:x:{uid}:{uid}::{root / 'home' / user}:/bin/bash")
        for line in box_passwd().values(): print(line)
        for line in os.environ.get('EXTRA_PASSWD', '').splitlines(): print(line)
        sys.exit(0)
    if args[:1] == ['passwd'] and args[-1] in BOX_ACCOUNTS:
        if os.environ.get('ACCOUNT_FAIL'): sys.exit(1)
        line = box_passwd().get(args[-1])
        if line is None: sys.exit(2)
        print(line); sys.exit(0)
    if args[:1] == ['group'] and box_group(args[-1]) and (args[-1] not in BOX_ACCOUNTS or args[-1] in box_passwd()):
        print(box_group(args[-1])); sys.exit(0)
    # **A standing agent's account** (#606): STANDING_PASSWD, a JSON map of
    # name to passwd line, which create-agent.sh's guard reads for a standing
    # agent's home.
    standing = json.loads(os.environ.get('STANDING_PASSWD', '{}'))
    if args[:1] == ['passwd'] and args[-1] in standing:
        print(standing[args[-1]]); sys.exit(0)
    if os.environ.get('ACCOUNT_FAIL'): sys.exit(1)
    if args[:1] == ['group'] and os.environ.get('COLLISION_GROUP') == args[-1]: sys.exit(0)
    sys.exit(0 if os.environ.get('COLLISION') == args[-1] else 2)
elif name == 'id':
    if args == ['-un']: print(os.environ.get('USER', ''))
    elif args[:1] == ['-Gn']:
        who = args[1]
        # A box account's groups, and, once a usermod joined it in this
        # fixture, the groups ADMINCON_JOINED or GATECON_JOINED name in their
        # place, so a check made after the join is judged on its own.
        joined = (root / 'joined-box-accounts').read_text().split() if (root / 'joined-box-accounts').exists() else []
        if who == 'weaver-gatecon':
            print(os.environ.get('GATECON_JOINED') if who in joined and os.environ.get('GATECON_JOINED')
                  else os.environ.get('GATECON_GROUPS', 'weaver-gatecon weaver-m1'))
        elif who == 'weaver-admincon':
            print(os.environ.get('ADMINCON_JOINED') if who in joined and os.environ.get('ADMINCON_JOINED')
                  else os.environ.get('ADMINCON_GROUPS', 'weaver-admincon weaver-m1-state weaver-m1-admin'))
        elif who.endswith('-relay'): print(os.environ.get('RELAY_GROUPS', who.removesuffix('-relay') + '-trace'))
        elif who.endswith('-admincon'): print(os.environ.get('CONNECTOR_GROUPS', who + ' ' + who.removesuffix('admincon') + 'state ' + who.removesuffix('con')))
        else: print(who)
    elif args[:1] == ['-u'] and len(args) > 1 and 'FIXTURE_ACCOUNT_UID' in os.environ:
        # FIXTURE_ACCOUNT_FOR names the one account the stand-in runs as.
        named = os.environ.get('FIXTURE_ACCOUNT_FOR', args[1])
        print(os.environ['FIXTURE_ACCOUNT_UID'] if named == args[1] else '54321')
    else: print(os.environ.get('FIXTURE_UID', '12345'))
elif name == 'systemctl':
    if args[:1] == ['list-units']:
        if os.environ.get('UNITS_FAIL'): sys.exit(1)
        print(os.environ.get('UNITS', ''), end='')
        sys.exit(0)
    sys.exit(99)
elif name == 'git':
    if args[0] == 'rev-parse': print('abcdef0')
    # FIXTURE_BRANCH set empty stands for a detached checkout, which
    # `git branch --show-current` answers with an empty line.
    elif args[0] == 'branch': print(os.environ.get('FIXTURE_BRANCH', 'fixture-branch'))
    elif args[0] == 'ls-tree': print('160000 commit e21cdc11fixturevendoredcommit\tllama.cpp')
elif name == 'nvidia-smi':
    # A box with no card: nvidia-smi's failure sentence on stdout, as it
    # prints it, and a failing status.
    if os.environ.get('NO_NVIDIA'):
        print("NVIDIA-SMI has failed because it couldn't communicate with the NVIDIA driver. "
              "Make sure that the latest NVIDIA driver is installed and running.")
        sys.exit(9)
    # Listed out of PCI bus order, so a reader that trusts the query's order
    # is caught.
    if '--query-gpu=pci.bus_id,name,memory.total' in args:
        print('00000000:41:00.0, Fixture GPU B, 2048 MiB')
        print('00000000:01:00.0, Fixture GPU A, 1024 MiB')
    # The driver's version, one line per card, as nvidia-smi answers it;
    # FIXTURE_DRIVERS names other lines.
    elif '--query-gpu=driver_version' in args:
        print(os.environ.get('FIXTURE_DRIVERS', '999.1.2\n999.1.2'))
    else: print('fixture-driver')
elif name == 'nvcc': print('nvcc: fixture\nCuda compilation tools, release 99.9, V99.9.1\nBuild fixture')
elif name == 'pacman': print('cccl 3.3.4-1')
elif name == 'dpkg-query':
    # The package database FIXTURE_DPKG names, as JSON: `owners` maps a path
    # to what dpkg-query -S prints before it (one package, or two that share
    # the file), `diverted` and `local_diverted` the paths a package's or a
    # local diversion names, whose two lines dpkg-query prints ahead of the
    # owner's, and `versions` each package's version. As dpkg's searchoutput
    # does, a diverted path counts as found though no package owns it, and
    # the diversion lines are translated: under a `de` message locale they
    # take a form the reader does not know (not dpkg's own wording). A path
    # or a package it does not hold is refused, as dpkg-query refuses one.
    db = json.loads(os.environ.get('FIXTURE_DPKG', '{}'))
    if args[:1] == ['-S'] and len(args) == 2:
        path = args[1]
        messages = (os.environ.get('LC_ALL') or os.environ.get('LC_MESSAGES')
                    or os.environ.get('LANG') or 'C')
        diverted = False
        for by, listed in (('diversion by fixture-diverter', 'diverted'), ('local diversion', 'local_diverted')):
            if path in db.get(listed, []):
                diverted = True
                if messages.startswith('de'):
                    print(f'Umleitung ({listed}) von {path} nach {path}.distrib')
                else:
                    print(f'{by} from: {path}')
                    print(f'{by} to: {path}.distrib')
        if path not in db.get('owners', {}):
            if diverted:
                sys.exit(0)
            print(f'dpkg-query: no path found matching pattern {path}', file=sys.stderr)
            sys.exit(1)
        print(f"{db['owners'][path]}: {path}")
    elif args[:1] == ['-W'] and len(args) == 3 and args[1].startswith('-f='):
        package = args[2].split(':', 1)[0]
        if package not in db.get('versions', {}):
            print(f'dpkg-query: no packages found matching {args[2]}', file=sys.stderr)
            sys.exit(1)
        print(args[1][3:].replace('${Package}', package).replace('${Version}', db['versions'][package])
              .replace('\\n', '\n'), end='')
    else: sys.exit(2)
elif name == 'cargo':
    target = pathlib.Path(os.environ['CARGO_TARGET_DIR'])
    # The engine's package as the lock names it, beside another crate with a
    # build script, so a reader that does not match the id exactly is caught.
    if args[0] == 'metadata': print(json.dumps({'target_directory': str(target), 'packages': [
        {'name': 'llama-cpp-sys-2', 'id': 'fixture-engine-source#llama-cpp-sys-2@0.1.0', 'version': '0.1.0',
         'source': 'git+https://fixture.invalid/llama-cpp-rs?rev=f00d#f00dfixtureforkcommit',
         'manifest_path': str(root / 'checkout' / 'llama-cpp-sys-2' / 'Cargo.toml')},
        {'name': 'fixture-other-sys', 'id': 'fixture-other-source#fixture-other-sys@0.1.0'}]}))
    elif args[0] == 'pkgid':
        # The lock's one package of the name, as `cargo pkgid` names it.
        if 'llama-cpp-sys-2' not in args: sys.exit(101)
        print('fixture-engine-source#llama-cpp-sys-2@0.1.0')
    elif args[0] == 'test': print('test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s')
    elif args[0] == 'build':
        if os.environ.get('BUILD_FAIL'):
            print('fixture build refusal', file=sys.stderr)
            sys.exit(42)
        target.joinpath('release').mkdir(parents=True)
        for member in ('worker', 'weaver-admin', 'weaver-trace-relay', 'weaver-gate', 'weaver-spu', 'weaver-state'):
            target.joinpath('release', member).write_text('fixture artifact ' + member)
        # The engine's out/lib as llama-cpp-sys-2 lays it out: each versioned
        # file beside its SONAME link and its bare link, and what the globs
        # leave behind. ENGINE_BUILD names the build, so a test can move it.
        out = target / 'release' / 'build' / 'llama-cpp-sys-2-fixture' / 'out'
        lib = out / 'lib'
        (lib / 'cmake').mkdir(parents=True)
        (lib / 'pkgconfig').mkdir()
        for stem, version in (('libllama', '0.0.1'), ('libggml', '0.9.1')):
            (lib / f'{stem}.so.{version}').write_text(f"fixture {stem} {os.environ.get('ENGINE_BUILD', 'a')}")
            (lib / f'{stem}.so.0').symlink_to(f'{stem}.so.{version}')
            (lib / f'{stem}.so').symlink_to(f'{stem}.so.0')
        # The configure cache cmake leaves in out/build, in the shape a live
        # build wrote on 2026-11-11: the OpenSSL lookup is recorded off only
        # where the build's environment carried the variable, as the fork's
        # build script passes every CMAKE_* variable to cmake. STALE_ENGINE
        # stands for an engine a build before the rule configured, which
        # cargo reuses without rerunning the build script, whatever the
        # environment now holds.
        (out / 'build').mkdir()
        cache = 'CMAKE_BUILD_TYPE:STRING=Release\nLLAMA_OPENSSL:BOOL=ON\n'
        if os.environ.get('CMAKE_DISABLE_FIND_PACKAGE_OpenSSL') == 'ON' and not os.environ.get('STALE_ENGINE'):
            cache += 'CMAKE_DISABLE_FIND_PACKAGE_OpenSSL:UNINITIALIZED=ON\n'
        (out / 'build' / 'CMakeCache.txt').write_text(cache)
        # A second engine directory, newer by mtime, as a shared target holds
        # one per engine pin it has built: the build did not use it.
        if os.environ.get('DECOY_ENGINE'):
            decoy = target / 'release' / 'build' / 'llama-cpp-sys-2-decoy' / 'out' / 'lib'
            decoy.mkdir(parents=True)
            for stem, version in (('libllama', '0.0.1'), ('libggml', '0.9.1')):
                (decoy / f'{stem}.so.{version}').write_text(f'decoy {stem}')
            later = time.time() + 3600
            os.utime(decoy, (later, later))
        if '--message-format=json-render-diagnostics' in args and not os.environ.get('NO_ENGINE_MESSAGE'):
            for package, at in (('fixture-other-source#fixture-other-sys@0.1.0', target / 'release' / 'build' / 'other' / 'out'),
                                ('fixture-engine-source#llama-cpp-sys-2@0.1.0', out)):
                print(json.dumps({'reason': 'build-script-executed', 'package_id': package, 'out_dir': str(at)}))
            print(json.dumps({'reason': 'build-finished', 'success': True}))
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
    if identity and command[:1] in (['env'], ['timeout']):
        command = clean_start(command, ['timeout', '0', 'setsid', '-w'])
    op, *rest = command
    # **A drop sudo refuses** (#577): the walk's operator rule named (root)
    # alone, so `sudo -u <account>` failed before any look was made.
    if identity and os.environ.get('DROP_FAIL') in (identity, 'all'):
        print(f'sudo: you are not permitted to run commands as {identity}', file=sys.stderr)
        sys.exit(1)
    if op == '--version':
        print(os.environ.get('SUDO_VERSION', 'Sudo version 1.9.16'))
    elif identity and op == 'sh':
        # **The look as an account through the shell's own `[`** (#577),
        # answered as the kernel would for that account under the fixture.
        # Any other shell run as an account fails loudly, so a look whose text
        # drifts is never run as the tester, past the fixture's walls.
        assert rest[:2] == ['-c', ACCESS_LOOK], ('a shell as an account the double does not know', rest)
        print('yes' if account_answer(identity, rest[3]) else 'no', end='')
        sys.exit(0)
    elif identity == 'nobody' and op == 'true':
        # The early ask that the operator's sudo runs commands as other
        # accounts (#577), refused only where DROP_FAIL names nobody or all.
        sys.exit(0)
    elif op == 'sh': shell_read(rest)
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
        if os.environ.get('TEE_FAIL') and os.environ['TEE_FAIL'] in rest[-1]: sys.exit(1)
        file = pathlib.Path(mapped(rest[-1]))
        assert file.is_relative_to(root), file
        text = sys.stdin.read()
        def write():
            with file.open('a' if '-a' in rest else 'w') as output: output.write(text)
        privileged([file], write)
    elif op == 'cp':
        source, destination = (pathlib.Path(mapped(a)) for a in rest[-2:])
        assert source.is_relative_to(root) and destination.is_relative_to(root)
        def copy():
            # `-a` copies a link as a link; `--remove-destination` unlinks the
            # name first, so the copy is a new file and never follows a link.
            if '--remove-destination' in rest and (destination.exists() or destination.is_symlink()):
                destination.unlink()
            if '-a' in rest and source.is_symlink():
                destination.symlink_to(os.readlink(source))
            else:
                shutil.copyfile(source, destination)
        privileged([source, destination], copy)
    elif op == 'install':
        target = pathlib.Path(mapped(rest[-1]))
        assert target.is_relative_to(root), target
        if '-d' in rest: privileged([target], lambda: target.mkdir(parents=True, exist_ok=True))
        else:
            # A file installed from a source is a new file holding its bytes,
            # as `install` unlinks the name first.
            source = pathlib.Path(mapped(rest[-2]))
            def place():
                if source.is_relative_to(root) and source.is_file():
                    if target.exists() or target.is_symlink(): target.unlink()
                    shutil.copyfile(source, target)
                else: target.touch()
            privileged([target], place)
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
        # verb and the agent handed to the script FIXTURE_ADMIN names. **Every
        # verb comes bare, a load included** (weaver-admin-Spec 6.1): admin's
        # start step asks PID1 for the agent's own scope, so a wrapper in front
        # of admin fails the fixture.
        assert rest[0].endswith('/weaver-admin'), ('admin wrapped', rest)
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
        # **The external `test` asked as an account**, which no script calls
        # since #577 and which stays so a script of before can be run against
        # it. UUTILS_TEST stands in for the walk's uutils `test`, which ignored
        # supplementary groups: every access of the admin-con account comes
        # through a group, so it answers no to all of them.
        if os.environ.get('UUTILS_TEST') and identity == 'weaver-admincon': sys.exit(1)
        sys.exit(0 if account_answer(identity, rest[0]) else 1)
    elif op in ('chown', 'chgrp'):
        # **An owner is set only on what stands** (#99 area 2 review, K11):
        # a path under the fixture that does not exist fails, as chown does.
        for a in rest:
            p = pathlib.Path(mapped(a))
            if a.startswith('/') and p.is_relative_to(root) and not privileged([p], lambda: os.path.lexists(p)):
                sys.exit(1)
    elif op == 'useradd':
        if os.environ.get('USERADD_FAIL') == rest[-1]:
            # **The account made and its home not** (#600): useradd's exit 12,
            # after the account's lines are written.
            with (root / 'made-then-failed').open('a') as made: made.write(rest[-1] + '\n')
            sys.exit(12)
        if '--create-home' in rest:
            # **useradd lays the home and copies the skeleton in** (#600): the
            # three regular files the appliance's /etc/skel holds, and a link
            # where FIXTURE_SKEL_LINK names a file for it to point at. A home
            # that stands already takes nothing, as useradd warns and goes on.
            home = pathlib.Path(mapped(rest[rest.index('--home-dir') + 1]))
            assert home.is_relative_to(root), home
            def lay():
                if home.exists(): return
                home.mkdir()
                for skel in ('.bashrc', '.bash_profile', '.bash_logout'): (home / skel).write_text('# skel\n')
                if os.environ.get('FIXTURE_SKEL_LINK'): (home / '.profile').symlink_to(os.environ['FIXTURE_SKEL_LINK'])
            privileged([home], lay)
        if rest[-1] in BOX_ACCOUNTS:
            with (root / 'laid-box-accounts').open('a') as laid: laid.write(rest[-1] + '\n')
    elif op == 'usermod':
        if rest[-1] in BOX_ACCOUNTS:
            with (root / 'joined-box-accounts').open('a') as joined: joined.write(rest[-1] + '\n')
    elif op == 'chattr': sys.exit(chattr_model(rest))
    elif op == 'find':
        # **Root's walk over the home** (#600): `find -P DIR <tests> -exec
        # chattr ARGS {} +` alone, the walk run on the fixture's tree in the
        # mode named (-P, or -L, which follows links) and its chattr answered
        # by the model above. Any other action is refused, so a walk that
        # would act otherwise never runs here.
        assert rest[0] in ('-P', '-L') and rest[-2:] == ['{}', '+'] and rest.count('-exec') == 1, ('a walk the double does not know', rest)
        at = rest.index('-exec')
        assert rest[at + 1] == 'chattr', rest
        top, tests = mapped(rest[1]), rest[2:at]
        assert pathlib.Path(top).is_relative_to(root), top
        assert not set(tests) & {'-execdir', '-delete', '-ok', '-okdir', '-fprint', '-fprint0', '-fprintf', '-fls'}, tests
        real_find = shutil.which('find', path='/usr/bin:/bin')
        walk = privileged([top], lambda: subprocess.run([real_find, rest[0], top, *tests, '-print0'],
                                                       capture_output=True))
        if walk.returncode:
            sys.stderr.buffer.write(walk.stderr)
            sys.exit(1)
        found = [p.decode() for p in walk.stdout.split(b'\0') if p]
        sys.exit(chattr_model(rest[at + 2:-2] + found) if found else 0)
    elif op in ('groupadd', 'chmod', 'setfacl', 'ldconfig'): pass
    elif op == 'setquota':
        # SETQUOTA_HOLD names a file this step waits for, up to 15 s, once it
        # has said it is under way beside it, so a test signals the run in
        # the middle of a step (#600).
        hold = os.environ.get('SETQUOTA_HOLD')
        if hold:
            pathlib.Path(hold + '.ready').write_text('')
            for _ in range(1500):
                if os.path.exists(hold): break
                time.sleep(0.01)
        sys.exit(1 if os.environ.get('SETQUOTA_FAIL') else 0)
    elif op == 'repquota':
        # **The drive's project quotas as CSV** (#585): FIXTURE_REPQUOTA is
        # the report's lines after the header, and FIXTURE_REPQUOTA_FAILS
        # makes the read fail. Without either, project 0 alone is in use.
        if os.environ.get('FIXTURE_REPQUOTA_FAILS'): sys.exit(1)
        print('Project,BlockStatus,FileStatus,BlockUsed,BlockSoftLimit,BlockHardLimit,BlockGrace,'
              'FileUsed,FileSoftLimit,FileHardLimit,FileGrace')
        print(os.environ.get('FIXTURE_REPQUOTA', '#0,ok,ok,20,0,0,,2,0,0,'))
    else: sys.exit(99)
elif name == 'mktemp':
    if not os.environ.get('ALLOW_APPLY_CHECKS'): sys.exit(99)
    probe = pathlib.Path(os.environ['PROBE'])
    probe.mkdir()
    print(probe)
elif name == 'setfacl': sys.exit(1 if os.environ.get('ACL_FAIL') else 0)
elif name == 'ldd':
    # What the loader resolves for the installed SPU: the engine's SONAMEs
    # from LD_LIBRARY_PATH, as the loader takes them, or from LDD_ELSEWHERE;
    # LDD_MISSING names a library it cannot find, LDD_STATIC answers as ldd
    # does for a file that is not a dynamic executable, and a file that is
    # not there fails as ldd does.
    if not os.path.exists(args[-1]):
        print(f'ldd: {args[-1]}: No such file or directory', file=sys.stderr)
        sys.exit(1)
    if os.environ.get('LDD_STATIC'):
        print('\tnot a dynamic executable')
        sys.exit(1)
    with open(os.environ['CALLS'], 'a') as seen:
        seen.write(json.dumps(['ldd-env', os.environ.get('LD_LIBRARY_PATH', ''), os.environ.get('LD_PRELOAD', '')]) + '\n')
    # The CUDA runtime and the driver's library resolve into the fixture's
    # CUDA directory, where setUp leaves each name a link to a versioned
    # file, or into LDD_RUNTIME.
    engine_dir = os.environ.get('LDD_ELSEWHERE') or os.environ.get('LD_LIBRARY_PATH', '/fixture/lib').split(':')[0]
    runtime_dir = os.environ.get('LDD_RUNTIME') or str(root / 'cuda' / 'lib64')
    for soname in ('libllama.so.0', 'libggml.so.0', 'libcudart.so.99', 'libcublas.so.99', 'libcublasLt.so.99',
                   'libcurand.so.99', 'libcuda.so.1', 'libc.so.6'):
        at = engine_dir if soname.startswith(('libllama', 'libggml')) else runtime_dir
        if soname == os.environ.get('LDD_MISSING'): print(f'\t{soname} => not found')
        else: print(f'\t{soname} => {at}/{soname} (0x0000)')
else: sys.exit(99)
'''
DOUBLE = DOUBLE.replace('@ACCESS_LOOK@', repr(ACCESS_LOOK)).replace('@DROP_PATH@', DROP_PATH)
DOUBLE = DOUBLE.replace('@DROP_NAMED@', DROP_NAMED).replace('@FIXTURE_ENVIRON@', FIXTURE_ENVIRON)
DOUBLE = DOUBLE.replace('@TEST_ROOT@', TEST_ROOT).replace('@STRANGER_UID@', str(STRANGER_UID))

# The six sampling defaults the harness reads from every serving declaration
# (weaver-harness-Spec section 6.1), at the values create-agent.sh writes. A
# standing agent's fixture declaration carries them, since update-stack.sh's
# --install refuses while any agent lacks one.
SAMPLING = ('\n[spu-instruction.decoder.tunable-values]\ntemperature = 0.7\ntop-k = 40\n'
            'top-p = 0.95\nrepetition-penalty = 1.1\nrepetition-window = 64\nseed = 11\n')


class PlanTests(unittest.TestCase):
    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory(prefix="weaver-deploy-test-")
        self.addCleanup(self.scratch.cleanup)
        self.root = Path(self.scratch.name)
        self.repo = self.root / "repo"
        shutil.copytree(DEPLOY, self.repo / "deploy")
        # **The shell's build is stood in for** (#29): the real one fetches and
        # compiles busybox, which BusyboxBuildTests judges apart. The stand-in
        # logs its call and writes the two files beside the members.
        stand_in = self.repo / "deploy" / "build-busybox.sh"
        stand_in.write_text(
            "#!/usr/bin/env python3\n"
            "import json, os, pathlib, sys\n"
            "with open(os.environ['CALLS'], 'a') as log:\n"
            "    log.write(json.dumps(['build-busybox.sh', *sys.argv[1:]]) + '\\n')\n"
            "if sys.argv[1] == '--preflight': sys.exit(0)\n"
            "out = pathlib.Path(sys.argv[1])\n"
            "out.joinpath('busybox').write_text('fixture busybox')\n"
            "out.joinpath('busybox.applets').write_text('ls\\ncat\\n')\n")
        stand_in.chmod(0o755)
        # The admin base, holding one agent's root, and the stack record.
        self.config = self.root / "config"
        self.config.mkdir()
        existing = self.config / "existing"
        existing.mkdir()
        (existing / "worker-binary").write_text(str(self.root / "installed" / "worker"))
        # The operator's home, which no script writes into since the
        # operator's ruling of 2026-10-07 on #1: the whole agent lives in its
        # territory under the stack record's agent-directory.
        self.home = self.root / "home"
        self.operator_home = self.home / "fixture-no-home"
        self.operator_home.mkdir(parents=True)
        (self.root / "agents").mkdir()
        self.existing_territory = self.root / "agents" / "weaver-existing"
        self.existing_territory.mkdir()
        (self.existing_territory / "agent.toml").write_text("[state-store]\nengine = \"none\"\n" + SAMPLING)
        # Laid to the law admin judges (weaver-admin-Spec section 9), as
        # create-agent.sh lays it: the territory 0710, its save-points/ 0750,
        # the groups the stand-in stat answers by name.
        self.existing_territory.chmod(0o710)
        (self.existing_territory / "save-points").mkdir(mode=0o750)
        (self.existing_territory / "save-points").chmod(0o750)
        (existing / "territory").write_text(str(self.existing_territory) + "\n")
        # m1's folder, and its territory inside it, where its declaration and
        # draft land (the operator's ruling of 2026-11-16 on the layout, #578).
        self.folder = self.root / "agents" / "m1"
        self.decl = self.folder / "weaver-m1"
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
            "worker-binary": str(self.root / "installed" / "worker"),
            "spu-binary": str(self.root / "installed" / "weaver-spu"),
            "gate-binary": str(self.root / "installed" / "weaver-gate"),
            "coordination-root": "/run",
            "library-path": str(self.root / "installed" / "lib"),
            "agent-directory": str(self.root / "agents"),
            "prefix": str(self.root / "installed"),
        }
        for key, value in self.stack_keys.items():
            (self.stack / key).write_text(value + ("" if value.endswith("\n") else "\n"))
        # The engine's libraries as bootstrap-stack.sh leaves them, equal to
        # what the cargo double builds by default, and the loader's entry it
        # writes, which the double and the preamble map into the fixture.
        self.lib_dir = self.root / "installed" / "lib"
        self.install_libraries("a")
        (self.root / "etc" / "ld.so.conf.d").mkdir(parents=True)
        self.ldso_conf = self.root / "etc" / "ld.so.conf.d" / "weaver.conf"
        # The CUDA directory the entry names, root's and closed as the stat
        # double answers for the fixture.
        self.cuda_lib = self.root / "cuda" / "lib64"
        self.cuda_lib.mkdir(parents=True)
        # The runtime files the ldd double resolves there, each name a link
        # to its versioned file, as a CUDA install lays them out.
        for name, version in (("libcudart.so.99", "99.1.2"), ("libcublas.so.99", "99.3.4"),
                              ("libcublasLt.so.99", "99.3.4"), ("libcurand.so.99", "99.5.6"),
                              ("libcuda.so.1", "999.1.2")):
            stem = name.rsplit(".so.", 1)[0]
            (self.cuda_lib / f"{stem}.so.{version}").write_text(f"fixture {stem}\n")
            (self.cuda_lib / name).symlink_to(f"{stem}.so.{version}")
        self.ldso_conf.write_text(f"{self.lib_dir}\n{self.cuda_lib}\n")
        # The engine's checkout, where the install tuple reads the vendored
        # engine's commit from the fork's tree.
        (self.root / "checkout" / "llama-cpp-sys-2").mkdir(parents=True)
        self.hba = self.root / "pg_hba.conf"
        self.hba.write_text("local all all peer\n")
        self.ident = self.root / "pg_ident.conf"
        self.ident.touch()
        self.artifact = self.root / "model.gguf"
        self.artifact.touch()
        self.bin = self.root / "bin"
        self.bin.mkdir()
        for name in ("sudo", "systemctl", "psql", "mktemp", "setfacl", "getent", "git", "cargo", "nvidia-smi", "pacman", "sh", "id", "stat", "ldd", "nvcc",
                     "runuser", "as-root", "findmnt", "setquota", "chattr", "repquota"):
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
        for name in ("BASH_ENV", "FIXTURE_MOUNT", "FIXTURE_REPQUOTA", "FIXTURE_REPQUOTA_FAILS", "SUDO_USER", "SUDO_UID", "COLLISION", "ALLOW_APPLY_CHECKS", "TRACE_OPEN", "BUILD_FAIL", "SUDO_FAIL", "READ_FAIL", "EMPTY_PATH", "ACL_FAIL", "PATH_FAIL", "ACCOUNT_FAIL", "WALL_OPEN",
                     "VISUDO_FAIL", "DROP_FAIL", "UUTILS_TEST", "SUDO_VERSION", "TEE_FAIL", "RELAY_GROUPS", "CONNECTOR_GROUPS", "UNITS", "UNITS_FAIL", "FIXTURE_ACCOUNT_UID",
                     "COLLISION_GROUP", "KEEP_ALIVE", "TRACE_GROUP_AS", "GROUP_WRITES",
                     "LAYOUT_GROUPS", "FIXTURE_ADMIN", "ADMIN_CALLS", "UNLOAD_ANSWER", "MISPLACED",
                     "KEEP_CGROUP", "FIXTURE_ACCOUNT_FOR", "FIXTURE_SKEL_LINK", "SETQUOTA_FAIL", "SETQUOTA_HOLD", "USERADD_FAIL", "STAND_IN_LEAF", "ENGINE_BUILD", "NO_ENGINE_MESSAGE",
                     "LDD_MISSING", "CUDA_LIB_DIR", "DECOY_ENGINE", "LDD_ELSEWHERE", "LDD_STATIC",
                     "CMAKE_DISABLE_FIND_PACKAGE_OpenSSL", "STALE_ENGINE",
                     "BOX_PASSWD", "BOX_GROUP", "EXTRA_PASSWD", "STANDING_PASSWD", "GATECON_GROUPS", "ADMINCON_GROUPS",
                     "GATECON_JOINED", "ADMINCON_JOINED",
                     "LD_LIBRARY_PATH", "LD_PRELOAD", "NO_NVIDIA", "LDD_RUNTIME", "FIXTURE_DRIVERS",
                     "NVCC_PREPEND_FLAGS", "NVCC_APPEND_FLAGS", "FIXTURE_DPKG"):
            self.env.pop(name, None)
        self.env["CUDA_LIB_DIR"] = str(self.cuda_lib)
        # Redirect even shell builtin /home probes, and the cgroup mount's
        # reads, into the fixture. The
        # production scripts have no test-only path switches and never read
        # the host's real agent homes during these tests.
        preamble = self.root / "fixture.bash"
        preamble.write_text("""fixture_args() {
  local arg
  fixture_mapped=()
  for arg in "$@"; do
    case "$arg" in
      /home/*|/run/systemd/system|/etc/ld.so.conf.d/*) arg="$FIXTURE_ROOT$arg";;
      /sys/fs/cgroup/*) arg="$FIXTURE_ROOT/cgroup-fs${arg#/sys/fs/cgroup}";;
    esac
    fixture_mapped+=("$arg")
  done
}
[() { fixture_args "$@"; builtin [ "${fixture_mapped[@]}"; }
test() { fixture_args "$@"; builtin test "${fixture_mapped[@]}"; }
grep() { fixture_args "$@"; command grep "${fixture_mapped[@]}"; }
""")
        self.env["BASH_ENV"] = str(preamble)

    def install_libraries(self, build):
        """Lay the library directory out as an install of the cargo double's
        engine build BUILD leaves it: the versioned files and their links."""
        self.lib_dir.mkdir(parents=True, exist_ok=True)
        for stem, version in (("libllama", "0.0.1"), ("libggml", "0.9.1")):
            for name in (f"{stem}.so", f"{stem}.so.0", f"{stem}.so.{version}"):
                (self.lib_dir / name).unlink(missing_ok=True)
            (self.lib_dir / f"{stem}.so.{version}").write_text(f"fixture {stem} {build}")
            (self.lib_dir / f"{stem}.so.0").symlink_to(f"{stem}.so.{version}")
            (self.lib_dir / f"{stem}.so").symlink_to(f"{stem}.so.0")

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
        # The box's two connector accounts, absent here, are named as laid at
        # --apply, and the access rule names the gate-con account's uid to come.
        self.assertIn("weaver-gatecon  the box's, absent: laid at --apply", result.stdout)
        self.assertIn("weaver-admincon  the box's, absent: laid at --apply", result.stdout)
        self.assertIn("allowed-uids = [the uid weaver-gatecon takes when --apply lays it]", result.stdout)
        self.assertNotIn("weaver-m1-admincon", result.stdout)
        self.assertFalse(self.decl.exists(), "the plan writes no declaration")
        self.assertIn("PENDING --apply", result.stdout)
        self.assertNotIn("nothing of this agent exists", result.stdout)
        self.assert_unprivileged()
        # The settings file the suite writes beside the stand-ins for each run
        # (#590) is the suite's, not the plan's.
        after = {p: p.read_bytes() for p in self.root.rglob("*") if p.is_file() and p != self.log
                 and p.name != FIXTURE_ENVIRON}
        before = {p: b for p, b in before.items() if p.name != FIXTURE_ENVIRON}
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
        for collision in ("account", "relay account", "access group",
                          "agent root", "staged root", "sudo rule", "territory"):
            with self.subTest(collision=collision):
                self.env.pop("COLLISION", None)
                for path in (self.config / "m1", self.config / ".m1.partial"):
                    if path.exists(): path.rmdir()
                self.rule.unlink(missing_ok=True)
                shutil.rmtree(self.folder, ignore_errors=True)
                if collision == "account": self.env["COLLISION"] = "weaver-m1-state"
                elif collision == "relay account": self.env["COLLISION"] = "weaver-m1-relay"
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

    ADAPTER_FLAGS = ("--adapter", "--adapter-sha256", "--adapter-scale", "--base-blake3")

    def adapter_values(self):
        return {"--adapter": str(self.root / "adapter.gguf"), "--adapter-sha256": "a" * 64,
                "--adapter-scale": "0.7", "--base-blake3": "b" * 64}

    def test_the_script_declares_the_adapter_table_whole(self):
        # ADP-5 and its ADP-6 row: the four flags write the binding's closed
        # `adapter` table, as declared and computed from nothing, and the
        # rendered declaration parses. Perturbations: drop a member from the
        # table, or quote the scale, and the read below fails.
        self.env["ALLOW_APPLY_CHECKS"] = "1"
        values = self.adapter_values()
        result = self.create(*[a for flag in self.ADAPTER_FLAGS for a in (flag, values[flag])], "--apply")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("adapter         " + values["--adapter"] + " at scale 0.7", result.stdout)
        import tomllib
        binding = tomllib.loads((self.decl / "agent.toml").read_text())["spu-instruction"]["decoder"]["model-binding"]
        self.assertEqual(binding["adapter"], {"artifact": values["--adapter"], "sha256": "a" * 64,
                                              "base-blake3": "b" * 64, "scale": 0.7})
        self.assertEqual(binding["artifact"], str(self.artifact))

    def test_a_partial_or_malformed_adapter_refuses_before_anything(self):
        # ADP-5's guards, in both modes, before any command: three of the
        # four flags refuse naming the fourth, a digest that is not 64
        # lowercase hexadecimal characters refuses, a scale that is not a
        # finite decimal within f32's range refuses (TOML would write `nan` or
        # `inf` bare), and the adapter path meets the TOML-string guard.
        # Perturbations: default a flag, or write a value unchecked, and the
        # plan runs.
        values = self.adapter_values()
        cases = []
        for absent in self.ADAPTER_FLAGS:
            cases.append(({k: v for k, v in values.items() if k != absent},
                          f"this run is missing: {absent}\n"))
        for flag, bad, said in (("--adapter-sha256", "a" * 63, "--adapter-sha256 is 64"),
                                ("--adapter-sha256", "A" * 64, "--adapter-sha256 is 64"),
                                ("--base-blake3", "g" * 64, "--base-blake3 is 64"),
                                ("--adapter-scale", "nan", "--adapter-scale is a finite"),
                                ("--adapter-scale", "inf", "--adapter-scale is a finite"),
                                ("--adapter-scale", "1e3", "--adapter-scale is a finite"),
                                ("--adapter-scale", "0x1", "--adapter-scale is a finite"),
                                ("--adapter-scale", "1" * 40 + ".0", "--adapter-scale is a finite"),
                                ("--adapter-scale", "00.7", "--adapter-scale is a finite"),
                                ("--adapter-scale", "007", "--adapter-scale is a finite"),
                                ("--adapter-scale", "1" + "0" * 19, "--adapter-scale is a finite"),
                                ("--adapter", '/m/x"y.gguf', "cannot hold as written"),
                                ("--adapter", "/m/x\ty.gguf", "cannot hold as written")):
            cases.append(({**values, flag: bad}, said))
        for given, said in cases:
            for mode in ((), ("--apply",)):
                with self.subTest(given=given, mode=mode):
                    self.log.unlink(missing_ok=True)
                    args = [a for flag, value in given.items() for a in (flag, value)]
                    result = self.run_script("create-agent.sh", "m1", "--artifact", str(self.artifact),
                                             *args, *mode)
                    self.assertEqual(result.returncode, 1, result.stderr)
                    self.assertIn(said, result.stderr)
                    self.assertEqual(self.calls(), [])

    def test_the_decoders_three_values_are_written_as_given_and_default_when_absent(self):
        # **#580**: the setup pages choose the devices, the context capacity
        # and the turn context, and the script writes them, so nobody edits
        # the declaration by hand. Given, each is written as given, the
        # devices in their given order as the shard order, and turn-context
        # as a top-level key. Absent, each keeps what the script wrote before:
        # device 0, 32768 tokens, and no turn-context, which reads as
        # continuous. The plan says all three. A TURN_CONTEXT the caller's
        # environment carries is not the flag. Perturbations: write the
        # devices sorted, a fixed capacity, or turn-context inside the
        # decoder's table, or keep the inherited variable, and a read below
        # fails.
        import tomllib
        self.env.update(ALLOW_APPLY_CHECKS="1", TURN_CONTEXT="independent")
        for given, devices, capacity, turn, shown in (
                ((), [0], 32768, None, "devices [0], context-capacity 32768, turn-context continuous (absent, so not written)"),
                (("--devices", "1,0", "--context-capacity", "131072", "--turn-context", "independent"),
                 [1, 0], 131072, "independent", "devices [1, 0], context-capacity 131072, turn-context independent"),
                (("--turn-context", "continuous", "--context-capacity", "4096"), [0], 4096, "continuous",
                 "devices [0], context-capacity 4096, turn-context continuous"),
                (("--devices", "4294967295", "--context-capacity", "4294967295"), [4294967295], 4294967295, None,
                 "devices [4294967295], context-capacity 4294967295, turn-context continuous (absent, so not written)")):
            with self.subTest(given=given):
                self.reset_made()
                result = self.create(*given, "--apply")
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertIn("decoder         " + shown, result.stdout)
                declaration = tomllib.loads((self.decl / "agent.toml").read_text())
                decoder = declaration["spu-instruction"]["decoder"]
                self.assertEqual(decoder["model-binding"]["devices"], devices)
                self.assertEqual(decoder["tunable-values"]["context-capacity"], capacity)
                self.assertEqual(decoder["tunable-values"]["max-tokens-per-turn"], 4096)
                self.assertEqual(declaration.get("turn-context"), turn)
                self.assertNotIn("turn-context", decoder)

    def test_a_malformed_decoder_value_refuses_before_anything(self):
        # **#580**: each value refuses in both modes before any command, as a
        # load would refuse it: an empty device set and a turn-context outside
        # its two words at the parse, a repeated ordinal and a capacity that
        # is not a count below 2^32 at the SPU's admission, and a capacity of
        # zero at either engine. A value TOML cannot write as an integer
        # (a leading zero, a sign, a fraction) refuses too, and so does a flag
        # given last with no value. Perturbations: drop any one check, and
        # its cases run on.
        cases = [(("--devices",), "--devices needs device ordinals"),
                 (("--context-capacity",), "--context-capacity needs a count"),
                 (("--turn-context",), "--turn-context needs continuous or independent")]
        for bad in ("", "0,", ",0", "0, 1", "0 ,1", "01", "-1", "+1", "a", "0x1", "1.0", "12345678901"):
            cases.append((("--devices", bad), "--devices is a comma-separated list"))
        for bad in ("4294967296", "9999999999"):
            cases.append((("--devices", bad), "--devices takes ordinals from 0 to 4294967295"))
        for bad, twice in (("0,0", 0), ("1,2,1", 1)):
            cases.append((("--devices", bad), f"--devices names ordinal {twice} twice"))
        for bad in ("", "0", "-1", "+1", "032768", "1.5", "1e5", "12345678901", "4294967296", "9999999999"):
            cases.append((("--context-capacity", bad), "--context-capacity is a count of tokens from 1 to 4294967295"))
        for bad in ("", "Continuous", "both", "independent "):
            cases.append((("--turn-context", bad), "--turn-context is continuous or independent"))
        # **A capacity below the turn's ceiling the script writes refuses**,
        # naming both values (the Planner's ruling of 2026-11-16 on #580): the
        # script's own consistency rule, which no load checks. Perturbation:
        # drop the floor, and both cases run on.
        for bad in ("1", "4095"):
            cases.append((("--context-capacity", bad),
                          f"--context-capacity {bad} is below the max-tokens-per-turn of 4096 this script writes"))
        for given, said in cases:
            for mode in ((), ("--apply",)):
                with self.subTest(given=given, mode=mode):
                    self.log.unlink(missing_ok=True)
                    result = self.run_script("create-agent.sh", "m1", "--artifact", str(self.artifact),
                                             *mode, *given)
                    self.assertEqual(result.returncode, 1, result.stderr)
                    self.assertIn(said, result.stderr)
                    self.assertEqual(self.calls(), [])

    QUOTA_MOUNT = "ext4|rw,relatime,prjquota|/state"

    def path_without(self, tool, box_path):
        """The stand-ins' directory, then one directory of links to every
        entry on the box's PATH but TOOL, the first of a name winning as the
        PATH lookup would. A privileged program from outside the tests'
        temporary root is linked as the guard's refusing copy, never the
        box's."""
        farm = self.root / f"path-without-{tool}"
        if not farm.exists():
            farm.mkdir()
            for d in box_path.split(os.pathsep):
                if d == str(self.bin) or not os.path.isdir(d):
                    continue
                for e in os.scandir(d):
                    link = farm / e.name
                    if e.name != tool and not link.is_symlink():
                        link.symlink_to(view_link(e.name, e.path))
        return os.pathsep.join([str(self.bin), str(farm)])

    def test_the_farm_links_the_guards_copy_of_each_privileged_program(self):
        """path_without's farm links a privileged program from outside the
        tests' temporary root as the guard's refusing copy, never the box's,
        so a call is refused and recorded rather than lost, with no guard on
        the PATH handed in. Read by the links alone, so nothing runs.
        Perturbations: link the box's privileged programs, or none, and this
        fails."""
        farm = Path(self.path_without("hostname", "/usr/bin").split(os.pathsep)[1])
        self.assertEqual([n for n in os.listdir(farm) if privileged(n, farm / n) and not _inside(farm / n)], [])
        for e in os.scandir("/usr/bin"):
            if e.name != "hostname" and privileged(e.name, e.path):
                with self.subTest(name=e.name):
                    self.assertEqual(os.readlink(farm / e.name), str(STAND_IN_GUARD / e.name))

    def test_the_home_is_capped_by_an_ext4_project_quota_when_asked(self):
        # **#585, the operator's ruling of 2026-11-16**: once the agent's
        # account stands, and before any other account of the agent's, the
        # drive's project quotas are read. After the home's mode is set and
        # before anything of the agent's runs, the folder takes the project
        # as the mark, the home and its directories and regular files take
        # their own project, the agent's uid, its directories the inherit
        # flag, and the project's limits are set on the drive's mount: the size in
        # kibibytes and one inode per 16 KiB of it (the Planner's ruling).
        # **What the drive holds after is read, not the calls alone** (#600):
        # the chattr stand-in answers as ext4 does, refusing the inherit flag
        # on a regular file, and keeps what each path took. The home and its
        # directories carry the project and the flag, the skeleton's files
        # the project alone, and the folder the project alone.
        # Without the flag nothing is capped and the plan says so.
        # Perturbations: set the flag with the project in one recursive call
        # (the line #600 found), set it on the files as well, leave the files
        # without the project, set the limit from the size as given, set no
        # inode limit, drop the mark, read the quotas after the other
        # accounts, or cap before the home stands, and a read below fails.
        self.env.update(ALLOW_APPLY_CHECKS="1", FIXTURE_MOUNT=self.QUOTA_MOUNT)
        folder = str(self.root / "agents" / "m1")
        home = folder + "/home"
        for size, kib in (("64G", 64 * 1024 * 1024), ("512M", 512 * 1024), ("2T", 2 * 1024 ** 3)):
            with self.subTest(size=size):
                self.reset_made()
                result = self.create("--home-quota", size, "--apply")
                self.assertEqual(result.returncode, 0, result.stderr)
                calls = self.calls()
                read = ["sudo", "repquota", "-P", "-n", "-v", "-O", "csv", "/state"]
                mark = ["sudo", "chattr", "-p", "12345", folder]
                project = ["sudo", "find", "-P", home, "(", "-type", "d", "-o", "-type", "f", ")",
                           "-exec", "chattr", "-p", "12345", "{}", "+"]
                inherit = ["sudo", "find", "-P", home, "-type", "d", "-exec", "chattr", "+P", "{}", "+"]
                limit = ["sudo", "setquota", "-P", "12345", "0", str(kib), "0", str(kib // 16), "/state"]
                for call in (read, mark, project, inherit, limit):
                    self.assertIn(call, calls)
                made = calls.index(["sudo", "useradd", "--system", "--shell", "/usr/sbin/nologin", "--create-home",
                                    "--home-dir", home, "--user-group", "weaver-m1"])
                member = calls.index(["sudo", "useradd", "--system", "--shell", "/usr/sbin/nologin",
                                      "--no-create-home", "--user-group", "weaver-m1-state"])
                moded = calls.index(["sudo", "chmod", "2750", home])
                self.assertLess(made, calls.index(read))
                self.assertLess(calls.index(read), member)
                self.assertLess(moded, calls.index(mark))
                self.assertLess(calls.index(mark), calls.index(project))
                self.assertLess(calls.index(project), calls.index(inherit))
                self.assertLess(calls.index(inherit), calls.index(limit))
                self.assertEqual(self.drive_attrs(home, alone=[folder]), {
                    folder: (12345, False), home: (12345, True), **{f"{home}/{skel}": (12345, False)
                                                                   for skel in (".bashrc", ".bash_profile", ".bash_logout")}})
                self.assertIn(f"home quota      {home}        capped at {size} and {kib // 16} files, an ext4 project "
                              "quota on /state", result.stdout)
        self.assertEqual(64 * 1024 * 1024 // 16, 4194304, "64G holds 4,194,304 files, the Planner's figure")
        self.reset_made()
        self.env.pop("FIXTURE_MOUNT")
        result = self.create("--apply")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse([c for c in self.calls() if c[:2] in (["sudo", "chattr"], ["sudo", "setquota"])])
        self.assertIn("home quota      none: no --home-quota, so the home is not capped", result.stdout)

    def drive_attrs(self, *trees, alone=()):
        """What the chattr stand-in left on each of TREES and everything
        under it, walked without following a link, and on each path ALONE,
        as {path: (project, inherit flag)}: a path the stand-in never set is
        (0, False)."""
        held = json.loads((self.root / "attrs.json").read_text()) if (self.root / "attrs.json").exists() else {}
        seen = {}
        for path in alone:
            a = held.get(path, {"project": 0, "inherit": False})
            seen[path] = (a["project"], a["inherit"])
        for top in trees:
            for d, dirs, files in os.walk(top):
                for path in [d, *(os.path.join(d, f) for f in files),
                             *(os.path.join(d, x) for x in dirs if os.path.islink(os.path.join(d, x)))]:
                    a = held.get(path, {"project": 0, "inherit": False})
                    seen[path] = (a["project"], a["inherit"])
        return seen

    def test_the_chattr_stand_in_holds_ext4s_rule(self):
        # **#600**: ext4 sets the project inherit flag on a directory alone
        # and refuses it on a regular file with EOPNOTSUPP, in chattr's words,
        # going on to the next path and failing at the end. A project is set
        # on a file or a directory, a link named outright is refused, and -R
        # walks without following one. The suite's stand-in answers so, or
        # the quota step's tests would pass a line the drive refuses, as they
        # passed the line the first appliance run met. Perturbation: let the
        # stand-in set the flag on a regular file, and the first case passes.
        self.env.update(ALLOW_APPLY_CHECKS="1")
        drive = self.root / "drive"
        (drive / "sub").mkdir(parents=True)
        (drive / ".bashrc").write_text("x\n")
        (drive / "sub" / "notes").write_text("x\n")
        outside = self.root / "outside"
        outside.write_text("not the drive's\n")
        (drive / "link").symlink_to(outside)

        def chattr(*args):
            return subprocess.run([str(self.bin / "sudo"), "chattr", *args], env=self.env, text=True,
                                  capture_output=True, timeout=20)

        run = chattr("-R", "+P", "-p", "7", str(drive))
        self.assertEqual(run.returncode, 1, run.stderr)
        self.assertEqual(sorted(run.stderr.splitlines()), [
            f"chattr: Operation not supported while setting flags on {drive / '.bashrc'}",
            f"chattr: Operation not supported while setting flags on {drive / 'sub' / 'notes'}"])
        self.assertEqual(self.drive_attrs(str(drive)), {
            str(drive): (7, True), str(drive / "sub"): (7, True), str(drive / ".bashrc"): (0, False),
            str(drive / "sub" / "notes"): (0, False), str(drive / "link"): (0, False)})
        (self.root / "attrs.json").unlink()
        run = chattr("-R", "-p", "7", str(drive))
        self.assertEqual(run.returncode, 0, run.stderr)
        self.assertEqual({p: a for p, a in self.drive_attrs(str(drive)).items() if a != (7, False)},
                         {str(drive / "link"): (0, False)}, "the walk follows no link")
        for args, said in ((("-p", "7", str(drive / "link")), "setting project"),
                           (("+P", str(drive / "link")), "setting flags"),
                           (("-p", "7", str(drive / "absent")), "No such file or directory while trying to stat")):
            with self.subTest(args=args):
                run = chattr(*args)
                self.assertEqual(run.returncode, 1, run.stderr)
                self.assertIn(said, run.stderr)
        self.assertNotIn(str(outside), json.loads((self.root / "attrs.json").read_text()))

    def test_the_home_s_walk_passes_over_a_link_the_skeleton_carries(self):
        # **#600**: the walk that sets the home's project follows no link, so
        # a link the skeleton carries is passed over and what it names takes
        # nothing, and chattr is handed regular files and directories alone.
        # Perturbation: walk with -L, or hand chattr every entry, and the run
        # is refused at the link.
        self.env.update(ALLOW_APPLY_CHECKS="1", FIXTURE_MOUNT=self.QUOTA_MOUNT)
        outside = self.root / "outside"
        outside.write_text("not the home's\n")
        self.env["FIXTURE_SKEL_LINK"] = str(outside)
        home = str(self.root / "agents" / "m1" / "home")
        result = self.create("--home-quota", "64G", "--apply")
        self.assertEqual(result.returncode, 0, result.stderr)
        held = self.drive_attrs(home)
        self.assertEqual(held[f"{home}/.profile"], (0, False))
        self.assertEqual(held[f"{home}/.bashrc"], (12345, False))
        self.assertEqual(held[home], (12345, True))
        self.assertNotIn(str(outside), json.loads((self.root / "attrs.json").read_text()))

    def test_a_run_that_stops_after_it_lays_anything_names_what_stands(self):
        """**#600**: a run that stops once it has laid anything, by a refusal
        or by a step that fails under set -e, names what it laid, in order
        (STANDS), the one thing in hand when it stopped (MAY STAND), and the
        take-down. A useradd that makes the account and fails on its home
        leaves the account named as one that may stand. A run that stops
        before it lays anything, or that succeeds, names nothing. After the
        admission the staged root is named as the admitted one.
        Perturbations: drop the exit trap, or one name, or the mark before a
        step, or the rename after the admission, and a case below fails."""
        self.env.update(ALLOW_APPLY_CHECKS="1", FIXTURE_MOUNT=self.QUOTA_MOUNT)
        folder = str(self.root / "agents" / "m1")
        home = f"the account weaver-m1 with its home {folder}/home"
        accounts = [f"the folder {folder}", home, "the account weaver-m1-state", "the group weaver-m1-trace",
                    "the group weaver-m1-admin", "the account weaver-m1-relay"]
        take_down = "Take m1 down by deploy/HowToDeployANewAgent.md section 7 before creating it again."
        limits = "project 12345's limits on /state"
        cases = {
            "a limit that is not set": ({"SETQUOTA_FAIL": "1"}, ("--home-quota", "64G"),
                                        "REFUSED: project 12345's limits could not be set on /state (setquota)",
                                        [*accounts, f"project 12345 on {folder}"], limits),
            "a write that fails": ({"TEE_FAIL": "/agent.toml"}, (), None,
                                   [*accounts, f"the territory {folder}/weaver-m1"], None),
            "an account made and its home not": ({"USERADD_FAIL": "weaver-m1"}, (), None,
                                                 [f"the folder {folder}"], home),
        }
        for case, (env, given, refused, laid, in_hand) in cases.items():
            with self.subTest(case=case):
                self.reset_made()
                (self.root / "made-then-failed").unlink(missing_ok=True)
                self.env.update(env)
                try:
                    result = self.create(*given, "--apply")
                finally:
                    for key in env: self.env.pop(key)
                self.assertNotEqual(result.returncode, 0, result.stdout)
                if refused: self.assertIn(refused, result.stderr)
                note = result.stderr[result.stderr.index("\nSTANDS: ") + 1:].splitlines()
                expected = [f"STANDS: this run stopped after it laid {', '.join(laid)}."]
                if in_hand: expected.append(f"MAY STAND: {in_hand}, which this run was laying when it stopped.")
                self.assertEqual(note, [*expected, take_down])
        self.assertEqual((self.root / "made-then-failed").read_text(), "weaver-m1\n",
                         "the double made the account it then failed on")
        with self.subTest(case="nothing laid, or nothing left"):
            self.reset_made()
            refused = self.create("--home-quota", "64g", "--apply")
            self.assertNotEqual(refused.returncode, 0)
            self.reset_made()
            made = self.create("--home-quota", "64G", "--apply")
            self.assertEqual(made.returncode, 0, made.stderr)
            self.assertNotIn("STAND", refused.stderr + made.stderr)
        with self.subTest(case="after the admission"):
            # No suite run reaches a step after the admission (the connector's
            # probe runs under the installed admin base alone), so the note and
            # the rename run as the script holds them.
            script = (self.repo / "deploy" / "create-agent.sh").read_text()
            note = script[script.index("STANDING=() LAYING=''"):script.index("trap 'rc=$?")]
            admit = script.index('root mv -T -- "$STAGE" "$AGENT_ROOT"\n')
            rename = script[script.index("\n", admit) + 1:script.index("\ndone\n", admit) + len("\ndone\n")]
            self.assertTrue(rename.startswith('for i in "${!STANDING[@]}"; do\n'), rename)
            program = ("set -euo pipefail\nNAME=m1 STAGE=/x/.m1.partial AGENT_ROOT=/x/m1\n" + note
                       + "trap 'rc=$?; [ \"$rc\" -eq 0 ] || standing_note' EXIT\n"
                       + 'STANDING+=("the folder /f" "the staged agent root $STAGE" "the sudo rule /r")\n'
                       + rename + "LAYING=''\nexit 1\n")
            run = subprocess.run(["bash", "-c", program], text=True, capture_output=True, timeout=20)
            self.assertEqual(run.stderr.strip().splitlines(), [
                "STANDS: this run stopped after it laid the folder /f, the agent root /x/m1, admitted, the sudo rule /r.",
                take_down])

    def test_a_run_stopped_by_a_signal_names_what_stands(self):
        """**#600**: bash runs the EXIT trap with the last command's status on
        a fatal signal, 0 after a step that passed, so a run hung up or
        terminated would name nothing. Each of HUP, INT, TERM and PIPE ends
        the run with 128 and its number, and the note names the signal, what
        stands and the step in hand. Cases: SIGTERM to the script alone while
        setquota is under way, which bash takes once the step returns, and
        SIGHUP to its whole process group, as a terminal's hang-up reaches
        it, which ends the step too. Perturbations: drop the signal traps, or
        exit 0 from them, and no note is written. Drop the signal's record
        and the note does not name it."""
        self.env.update(ALLOW_APPLY_CHECKS="1", FIXTURE_MOUNT=self.QUOTA_MOUNT)
        self.install_stack()
        folder = str(self.root / "agents" / "m1")
        laid = [f"the folder {folder}", f"the account weaver-m1 with its home {folder}/home",
                "the account weaver-m1-state", "the group weaver-m1-trace", "the group weaver-m1-admin",
                "the account weaver-m1-relay", f"project 12345 on {folder}"]
        for name, to_group in (("TERM", False), ("HUP", True)):
            with self.subTest(signal=name):
                self.reset_made()
                hold = self.root / f"hold-{name}"
                self.env["SETQUOTA_HOLD"] = str(hold)
                try:
                    run = subprocess.Popen(["bash", str(self.repo / "deploy" / "create-agent.sh"), "m1",
                                            "--artifact", str(self.artifact), "--engine", "sqlite",
                                            "--home-quota", "64G", "--apply"],
                                           env=self.env, text=True, stdin=subprocess.DEVNULL,
                                           stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True)
                finally:
                    self.env.pop("SETQUOTA_HOLD")
                try:
                    ready = Path(str(hold) + ".ready")
                    for _ in range(1500):
                        if ready.exists() or run.poll() is not None: break
                        time.sleep(0.01)
                    self.assertTrue(ready.exists(), "setquota was never reached")
                    sig = getattr(signal, "SIG" + name)
                    if to_group: os.killpg(run.pid, sig)
                    else: os.kill(run.pid, sig)
                    hold.write_text("")
                    out, err = run.communicate(timeout=20)
                finally:
                    if run.poll() is None:
                        os.killpg(run.pid, signal.SIGKILL)
                        run.wait()
                self.assertEqual(run.returncode, 128 + sig, err)
                self.assertEqual(err[err.index("\nSTANDS: ") + 1:].splitlines(), [
                    f"STANDS: this run stopped on SIG{name} after it laid {', '.join(laid)}.",
                    f"MAY STAND: project 12345's limits on /state, which this run was laying when it stopped "
                    f"on SIG{name}.",
                    "Take m1 down by deploy/HowToDeployANewAgent.md section 7 before creating it again."])
    def test_the_folder_sheds_an_inherited_project_flag_before_its_mark(self):
        # **CodeRabbit on #582**: a folder made in a base that carries the
        # project inherit flag takes the flag, and `chattr -p` sets the
        # project and leaves the flag, so the territory and the state room,
        # made in the folder after the mark, would join the home's project
        # and count against its cap. The flag is cleared in a call of its
        # own, after the folder is made and before its mark, and so before
        # the territory and the state room are made in it. Without the cap
        # the folder is left as the base made it. This pins the sequence:
        # what ext4 does with it is proved on an appliance's drive, since a
        # project quota needs a real ext4 mount, which no test here makes.
        # Perturbations: drop the clearing, fold it into the mark's call, or
        # move it after the territory, and an assertion below fails.
        self.env.update(ALLOW_APPLY_CHECKS="1", FIXTURE_MOUNT=self.QUOTA_MOUNT)
        folder = str(self.root / "agents" / "m1")
        result = self.create("--home-quota", "64G", "--apply")
        self.assertEqual(result.returncode, 0, result.stderr)
        calls = self.calls()
        made = calls.index(["sudo", "install", "-d", "-o", "root", "-g", "root", "-m", "00755", folder])
        shed = ["sudo", "chattr", "-P", folder]
        self.assertIn(shed, calls)
        mark = calls.index(["sudo", "chattr", "-p", "12345", folder])
        territory = next(i for i, c in enumerate(calls) if c[:3] == ["sudo", "install", "-d"]
                         and c[-1] == f"{folder}/weaver-m1")
        room = next(i for i, c in enumerate(calls) if c[:3] == ["sudo", "install", "-d"]
                    and c[-1] == f"{folder}/weaver-m1/state")
        self.assertLess(made, calls.index(shed))
        self.assertLess(calls.index(shed), mark)
        self.assertLess(mark, territory)
        self.assertLess(mark, room)
        self.reset_made()
        self.env.pop("FIXTURE_MOUNT")
        result = self.create("--apply")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotIn(shed, self.calls())

    def test_a_home_project_already_in_use_refuses_before_the_cap(self):
        # **#585, the Planner's ruling of 2026-11-16**: a project the drive
        # already carries, the agent's uid, with any usage or any limit,
        # refuses once the agent's account stands, naming the project and the
        # mount, before any other account of the agent's is made and before
        # any mark, project or limit is set. A read that fails refuses too.
        # A project of another number, or the agent's with every count zero,
        # passes. In both modes. Perturbations: drop the refusal, judge the
        # usage alone, match the project by prefix, or take a failed read as
        # free, and a case below fails.
        free = ("#1234,ok,ok,9,0,0,,1,0,0,", "#123456,ok,ok,9,0,9,,1,0,9,", "#12345,ok,ok,0,0,0,,0,0,0,")
        taken = {"blocks used": "#12345,ok,ok,8,0,0,,1,0,0,", "a block limit": "#12345,ok,ok,0,0,64,,0,0,0,",
                 "a soft block limit": "#12345,ok,ok,0,64,0,,0,0,0,", "files used": "#12345,ok,ok,0,0,0,,3,0,0,",
                 "an inode limit": "#12345,ok,ok,0,0,0,,0,0,99,", "a soft inode limit": "#12345,ok,ok,0,0,0,,0,99,0,"}
        for root_mode in (False, True):
            if root_mode:
                self.root_mode()
                self.env["FIXTURE_ACCOUNT_UID"] = "12345"
            flag = ("--root-operator",) if root_mode else ()
            step = (lambda name: ["as-root", name]) if root_mode else (lambda name: ["sudo", name])
            self.env.update(ALLOW_APPLY_CHECKS="1", FIXTURE_MOUNT=self.QUOTA_MOUNT)
            for why, line in [(w, l) for w, l in taken.items()] + [("a failed read", None)]:
                with self.subTest(root_mode=root_mode, why=why):
                    self.reset_made()
                    self.env.pop("FIXTURE_REPQUOTA", None)
                    self.env.pop("FIXTURE_REPQUOTA_FAILS", None)
                    if line: self.env["FIXTURE_REPQUOTA"] = "\n".join((*free[:2], line))
                    else: self.env["FIXTURE_REPQUOTA_FAILS"] = "1"
                    result = self.create(*flag, "--home-quota", "64G", "--apply")
                    self.assertEqual(result.returncode, 1, result.stdout)
                    if line:
                        self.assertIn(f"project 12345, weaver-m1's uid, is already in use on /state (repquota: {line})",
                                      result.stderr)
                    else:
                        self.assertIn("the project quotas on /state cannot be read (repquota)", result.stderr)
                    self.assertIn("section 7", result.stderr)
                    calls = self.calls()
                    self.assertIn(step("repquota"), [c[:2] for c in calls])
                    self.assertFalse([c for c in calls if c[:2] in (step("chattr"), step("setquota"))], calls)
                    self.assertFalse([c for c in calls if c[:2] == step("useradd") and c[-1] == "weaver-m1-state"], calls)
            with self.subTest(root_mode=root_mode, why="free"):
                self.reset_made()
                self.env.pop("FIXTURE_REPQUOTA_FAILS", None)
                self.env["FIXTURE_REPQUOTA"] = "\n".join(free)
                result = self.create(*flag, "--home-quota", "64G", "--apply")
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertIn([*step("setquota"), "-P", "12345", "0", str(64 * 1024 * 1024), "0", "4194304", "/state"],
                              self.calls())

    def test_a_home_quota_refuses_before_anything(self):
        # **#585**: in both modes, before any account or file is made, a size
        # that is not a whole count of M, G or T refuses, and so do a base on
        # another filesystem than ext4, an ext4 base not mounted with project
        # quotas, a box without setquota, and the flag with no value.
        # Perturbations: drop any one check, and its cases run on.
        cases = [((), self.QUOTA_MOUNT, ("--home-quota",), "--home-quota needs a size")]
        for bad in ("", "0G", "64", "64g", "64GB", "64K", "1.5G", "064G", "-1G", "+1G", "12345678G"):
            cases.append(((), self.QUOTA_MOUNT, ("--home-quota", bad), "--home-quota is a whole size"))
        cases.append(((), "xfs|rw,prjquota|/state", ("--home-quota", "64G"),
                      "--home-quota caps the home with an ext4 project quota, and "))
        cases.append(((), "ext4|rw,relatime,usrquota,grpquota|/state", ("--home-quota", "64G"),
                      "--home-quota needs /state mounted with project quotas (prjquota)"))
        # Each tool absent whatever the box carries: its stand-in moved aside
        # and the box's PATH replaced by links to everything on it but that
        # tool, so the case runs on a box with e2fsprogs and the quota tools.
        for tool in ("chattr", "setquota", "repquota", "findmnt"):
            cases.append(((tool,), self.QUOTA_MOUNT, ("--home-quota", "64G"), f"--home-quota needs {tool} on PATH"))
        box_path = self.env["PATH"]
        for absent, mount, given, said in cases:
            for mode in ((), ("--apply",)):
                with self.subTest(given=given, mount=mount, absent=absent, mode=mode):
                    self.log.unlink(missing_ok=True)
                    for name in absent: (self.bin / name).rename(self.root / f"aside-{name}")
                    try:
                        self.env["FIXTURE_MOUNT"] = mount
                        if absent:
                            self.env["PATH"] = self.path_without(absent[0], box_path)
                            self.assertIsNone(shutil.which(absent[0], path=self.env["PATH"]))
                        result = self.create(*mode, *given)
                    finally:
                        self.env["PATH"] = box_path
                        for name in absent: (self.root / f"aside-{name}").rename(self.bin / name)
                    self.assertEqual(result.returncode, 1, result.stderr)
                    self.assertIn(said, result.stderr)
                    self.assertFalse([c for c in self.calls() if c[0] in ("sudo", "runuser", "as-root")], self.calls())

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
        # The agent's folder, and an agent of the layout before #578 by its
        # territory or its home, each refuse (the operator's ruling of
        # 2026-10-08 on #1). Perturbation: drop either old-layout refusal, and
        # its case runs on.
        for path in (self.config / "m1", self.root / "agents" / "m1", self.root / "agents" / "weaver-m1",
                     self.home / "weaver-m1"):
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
        self.assertIn(f"territory       {self.root / 'agents' / 'm1' / 'weaver-m1'} ", result.stdout)

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
        self.assertIn(f"territory       {self.root / 'agents' / 'm1' / 'weaver-m1'} ", result.stdout)
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
        self.assertEqual((root / "roles.toml").read_text(), 'trace-reader = "weaver-admincon"\n')
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
        # The six sampling defaults the harness reads at every load
        # (weaver-harness-Spec section 6.1), the five at the constants of the
        # build before the carrier and the seed as it always was.
        # Perturbation: drop one from the written declaration and this names
        # it, as the made agent's first load would.
        tunables = decoder["tunable-values"]
        self.assertEqual({k: tunables.get(k) for k in ("temperature", "top-k", "top-p",
                                                        "repetition-penalty", "repetition-window")},
                         {"temperature": 0.7, "top-k": 40, "top-p": 0.95,
                          "repetition-penalty": 1.1, "repetition-window": 64})
        self.assertIsInstance(tunables.get("seed"), int)
        # A base-only agent omits the adapter table (ADP-1): absence is an
        # omitted key, never an empty table.
        self.assertNotIn("adapter", decoder["model-binding"])
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
        self.assertIn("Defaults:weaver-admincon !pam_session\n", text)
        self.assertIn(f"weaver-admincon ALL=(root) NOPASSWD: {lines}\n", text)
        self.assertNotIn("weaver-m1-admincon", text)
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
        self.assertTrue(any(c[:12] == look_as("weaver-m1", "-x", "")[:12] for c in calls))
        self.assert_rule(["show", "validate", "load", "unload", "stop", "save-point", "restore", "force-unload"])
        self.assertIn("validate it before loading, with the validate verb of", result.stdout)
        # **The agent has no seeding sender** (weaver-gate-world-contract
        # section 2): its rule names the box's gate-con account alone, so the
        # closing lines name no seeding step and say why, and the script never
        # dials the gate. Perturbations: print the seeding step again, or dial
        # the gate from the script, and this fails.
        self.assertIn("alone, so it has no seeding sender: " + str(self.decl / "system-prompt.md"), result.stdout)
        self.assertIn("(weaver-gate-world-contract section 2)", result.stdout)
        self.assertNotIn("--system", result.stdout)
        # The access rule names the gate-con account's uid alone, the uid it
        # took when laid (990 in the fixture), and never the operator's.
        declaration = (self.decl / "agent.toml").read_text()
        self.assertIn("allowed-uids = [990]\n", declaration)
        self.assertNotIn("12345", declaration)
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

    def reset_made(self):
        """Take away what an apply run made, so the next run starts clean."""
        self.log.unlink(missing_ok=True)
        (self.root / "attrs.json").unlink(missing_ok=True)
        for made in (self.config / "m1", self.config / ".m1.partial", self.folder):
            shutil.rmtree(made, ignore_errors=True)
        self.rule.unlink(missing_ok=True)

    def test_no_look_as_an_account_uses_the_external_test(self):
        # **#577**: on the by-hand appliance walk of 2026-11-16 the box's
        # `test` was uutils', whose `-r` ignored supplementary groups, so the
        # admin-con account's read of the declaration, which comes through a
        # group, was refused and the run stopped at the wall. Every look as
        # another account goes through the shell's builtin `[`. Perturbation:
        # put `sudo -u "$1" test "$2" "$3"` back in `allows`, and the scan
        # finds it.
        # A drop in any spelling (`-u`, `-nu`, after `-n` or before it,
        # runuser, setpriv, or a script's own wrapper), then `test` before the
        # command ends.
        external = re.compile(r"(\bsudo\b[^|;&#]*\s-n?u\b|\brunuser\b|\bsetpriv\b|\bas_(op|can)\b)[^|;&#]*\btest\b")
        for script in sorted(DEPLOY.glob("*.sh")):
            with self.subTest(script=script.name):
                code = [l for l in script.read_text().splitlines() if not l.lstrip().startswith("#")]
                self.assertEqual([l for l in code if external.search(l)], [], script.name)
        # The look's one text, in each script that looks as an account.
        for name in ("create-agent.sh", "decommission.sh", "verify-lifecycle.sh"):
            with self.subTest(pinned=name):
                self.assertIn(f"sh -c '{ACCESS_LOOK}'", (DEPLOY / name).read_text())

    def test_a_group_blind_test_changes_no_answer_of_the_wall(self):
        # **#577**: a group-blind external `test` (UUTILS_TEST answers no to
        # every look of the admin-con account, whose access is all through
        # groups) changes nothing, because no look calls it. Perturbation:
        # make `allows` call `sudo -n -u "$1" test "$2" "$3"`, and the run
        # stops at the admin-con account's read of the declaration, as the
        # walk's did. The script of 6b91747e stops there under this fixture.
        self.env.update(ALLOW_APPLY_CHECKS="1", UUTILS_TEST="1")
        result = self.create("--apply")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("weaver-admincon reads the declaration and writes nothing of the territory",
                      result.stdout)
        self.assertFalse([c for c in self.calls() if c[:1] == ["sudo"] and "test" in c[1:5]], self.calls())

    def test_a_drop_sudo_refuses_stops_the_run_naming_sudo(self):
        # **#577**: a drop sudo itself refuses is neither yes nor no. Before,
        # a refused `sudo -u weaver-m1 test -x` read as "the agent's uid
        # cannot enter", and the wall passed unchecked. Each account the wall
        # looks as refuses here, naming sudo, and nothing is admitted.
        # Perturbation: let `allows` return 1 on a refused drop, and the
        # agent's case admits the root.
        for account in ("weaver-m1-state", "weaver-m1", "weaver-admincon"):
            with self.subTest(account=account):
                self.reset_made()
                self.env.update(ALLOW_APPLY_CHECKS="1", DROP_FAIL=account)
                result = self.create("--apply")
                self.assertNotEqual(result.returncode, 0, result.stdout)
                self.assertIn(f"sudo would not run a look as {account}", result.stderr)
                self.assertIn("(ALL) and not (root) alone", result.stderr)
                self.assertNotIn("CAN ", result.stderr)
                self.assertFalse((self.config / "m1").exists())
                self.assertFalse(self.rule.exists())
                self.assert_no_printed_root_command(result)

    def root_mode(self):
        """**Root runs the script** (#576): uid 0, and a shell in which each
        command a root step runs reaches the double as `as-root <command>`,
        as the bare command would run as root on the box. The script calls
        none of these names outside a root step, so only root steps are
        routed. `grep` reaches the double only for `/etc/sudoers`, which the
        double maps into the fixture."""
        self.env["FIXTURE_UID"] = "0"
        preamble = self.root / "fixture-root.bash"
        steps = " ".join(ROOT_STEPS)
        preamble.write_text(f'. {shlex.quote(self.env["BASH_ENV"])}\n'
                            f'for c in {steps}; do eval "$c() {{ \\"\\$FIXTURE_ROOT/bin/as-root\\" $c \\"\\$@\\"; }}"; done\n'
                            'grep() { case " $* " in *" /etc/sudoers "*) "$FIXTURE_ROOT/bin/as-root" grep "$@" ;;\n'
                            '  *) fixture_args "$@"; command grep "${fixture_mapped[@]}" ;; esac; }\n')
        self.env["BASH_ENV"] = str(preamble)

    def test_root_alone_needs_the_flag_and_the_flag_needs_root(self):
        # **#576**: on a single-purpose appliance root is the operator, and
        # says so with --root-operator. Root without the flag refuses, naming
        # the flag, so a workstation run as root by accident still refuses,
        # and the flag under another uid refuses. Perturbations: drop either
        # uid check, and its case runs on.
        # **Through sudo it is a workstation's operator** (the self-check):
        # sudo with the flag refuses, and sudo without it is not told to add
        # the flag. Perturbation: drop the SUDO_USER check under the flag, and
        # the run goes on as root.
        for case, uid, sudo_user, args, said in (
                ("root, no flag", "0", None, (), "run it as root with --root-operator"),
                ("flag, not root", "12345", None, ("--root-operator",), "--root-operator is for root"),
                ("sudo, flag", "0", "someone", ("--root-operator",), "not sudo from someone"),
                ("sudo, no flag", "0", "someone", (), "not under sudo")):
            with self.subTest(case=case):
                self.log.unlink(missing_ok=True)
                self.env["FIXTURE_UID"] = uid
                self.env.pop("SUDO_USER", None)
                if sudo_user: self.env["SUDO_USER"] = sudo_user
                result = self.create(*args)
                self.assertNotEqual(result.returncode, 0, result.stdout)
                self.assertIn(said, result.stderr)
                if case == "sudo, no flag": self.assertNotIn("--root-operator", result.stderr)
                self.assertNotIn("== plan", result.stdout)
                self.assertFalse([c for c in self.calls() if c[0] in ("sudo", "runuser", "as-root")], self.calls())
        # **Root through its own sudo is root** (the Planner's grade of #583):
        # SUDO_USER=root takes the flag, and without it is told to add it.
        # Perturbation: refuse SUDO_USER=root again, and the plan never runs.
        self.env.update(FIXTURE_UID="0", SUDO_USER="root")
        result = self.create("--root-operator")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("== plan", result.stdout)
        result = self.create()
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertIn("run it as root with --root-operator", result.stderr)
        self.env.pop("SUDO_USER")

    def test_root_mode_makes_the_agent_without_sudo_and_writes_operator_zero(self):
        # **#576**: root runs every root step itself, looks as each account
        # through runuser, joins no group, and writes the root's `operator` key
        # as 0. The one sudo call left is the listing of the admin-con
        # account's rule, which root may read. Perturbations: run root steps
        # through sudo in root mode, and sudo install appears; join root to
        # the groups, and a usermod names root; look through sudo, and no
        # runuser look is made. **No drop's relay directory outlives the run**
        # (the clean-up after #598): each drop removes its own, root mode's
        # routing included, under a TMPDIR written with a trailing slash, and
        # nothing else is removed: a file named rm in the run's own directory
        # stands (CodeRabbit on #608). Perturbations: drop the double's
        # pass-through for that removal, or compare its directory as written,
        # and the directories stand. Hand rm its own name as an operand, and
        # the file named rm goes.
        self.root_mode()
        relays = self.root / "relays"
        relays.mkdir()
        decoy = self.root / "cwd"
        decoy.mkdir()
        (decoy / "rm").write_text("not the relay\n")
        self.addCleanup(os.chdir, os.getcwd())
        os.chdir(decoy)
        self.env.update(ALLOW_APPLY_CHECKS="1", TMPDIR=f"{relays}/")
        result = self.create("--root-operator", "--apply")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(sorted(p.name for p in relays.glob("weaver-drop.*")), [])
        self.assertTrue((decoy / "rm").exists(), "a file named rm in the run's directory stands")
        self.assertIn("== made", result.stdout)
        self.assertIn("operator        root (--root-operator), which reads everything already", result.stdout)
        calls = self.calls()
        sudo = [c for c in calls if c[0] == "sudo"]
        self.assertEqual(sudo, [["sudo", "-n", "-l", "-U", "weaver-admincon"]], sudo)
        territory = str(self.decl)
        self.assertIn(["runuser", "-u", "weaver-m1", "--", "env", "-i", "PATH=" + DROP_PATH, "sh", "-c", ACCESS_LOOK,
                       "sh", "-x", territory], calls)
        self.assertIn(["runuser", "-u", "weaver-admincon", "--", "env", "-i", "PATH=" + DROP_PATH, "sh", "-c",
                       ACCESS_LOOK, "sh", "-r",
                       territory + "/agent.toml"], calls)
        self.assertTrue([c for c in calls if c[:2] == ["as-root", "useradd"] and c[-1] == "weaver-m1"], calls)
        self.assertTrue([c for c in calls if c[:2] == ["as-root", "visudo"]], calls)
        self.assertFalse([c for c in calls if c[:2] == ["as-root", "usermod"] and c[-1] in ("root", "fixture-no-home")],
                         calls)
        self.assertEqual((self.config / "m1" / "operator").read_text(), "0\n")
        self.assertNotIn("joined groups", result.stdout)
        self.assertIn(["runuser", "-u", "nobody", "--", "env", "-i", "PATH=" + DROP_PATH, "true"], calls)
        self.assertNotIn("through weaver-m1-admin", result.stdout)
        self.assert_no_printed_root_command(result)

    def test_root_mode_checks_runuser_before_anything_is_made(self):
        # **A runuser that cannot drop stops the run before any account is
        # made** (the self-check), as the operator's sudo does in the
        # operator's mode (#577). Perturbation: drop the early check, and the
        # accounts are made before the wall refuses.
        self.root_mode()
        self.env.update(ALLOW_APPLY_CHECKS="1", DROP_FAIL="nobody")
        result = self.create("--root-operator", "--apply")
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertIn("runuser would not run a command as nobody", result.stderr)
        made = [c for c in self.calls() if c[0] == "as-root" and c[1] in ("useradd", "groupadd", "usermod", "install")]
        self.assertFalse(made, made)
        self.assertNotIn("== accounts", result.stdout)

    def test_every_root_step_is_one_the_root_mode_tests_route(self):
        # A root step whose command the root-mode preamble does not route
        # would run the real command in a root-mode test. Perturbation: add a
        # `root ln` to the script, and this names it.
        script = (self.repo / "deploy" / "create-agent.sh").read_text()
        used = set()
        for line in script.splitlines():
            if line.lstrip().startswith("#"): continue
            used.update(m.group(1) for m in re.finditer(r"(?:^\s*|[|;&(!{]\s*|\$\(\s*|\b(?:then|do|else)\s+)root (?:-n )?([a-z][a-z-]*)", line))
        # The scan sees an indented step (`cp` in the copy loop) and one under
        # `if !` (`visudo`), or it is blind to them.
        self.assertTrue({"cp", "visudo", "useradd"} <= used, used)
        self.assertEqual(used - set(ROOT_STEPS) - {"grep"}, set(), used)

    def test_root_mode_refuses_a_look_runuser_refuses_naming_runuser(self):
        # A drop runuser refuses is neither yes nor no in root mode either,
        # and the refusal names runuser, not sudo. Perturbation: keep sudo in
        # the message, and the case reads wrong.
        self.root_mode()
        self.env.update(ALLOW_APPLY_CHECKS="1", DROP_FAIL="weaver-m1")
        result = self.create("--root-operator", "--apply")
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertIn("runuser would not run a look as weaver-m1", result.stderr)
        self.assertIn("runuser must run commands as the agent's accounts", result.stderr)
        self.assertFalse((self.config / "m1").exists())

    def test_an_operator_sudo_that_runs_as_root_alone_stops_before_anything_is_made(self):
        # **#577, the review's finding 1**: the wall looks as the agent's
        # accounts, which stand only once made, so a `(root)`-only operator
        # rule is asked of before anything is made, as `nobody`. Perturbation:
        # drop the early ask, and the accounts and the territory stand before
        # the wall refuses.
        self.env.update(ALLOW_APPLY_CHECKS="1", DROP_FAIL="nobody")
        result = self.create("--apply")
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertIn("sudo would not run a command as nobody", result.stderr)
        self.assertIn("(ALL) and not (root) alone", result.stderr)
        made = [c for c in self.calls() if c[:2] in (["sudo", "useradd"], ["sudo", "groupadd"], ["sudo", "usermod"])]
        self.assertEqual(made, [], made)
        self.assertFalse(self.decl.exists())
        self.assertFalse(list((self.root / "etc" / "sudoers.d").iterdir()))
        self.assert_no_printed_root_command(result)

    def test_a_staged_rule_that_cannot_be_written_is_never_installed(self):
        # **The review's finding 2**: `stage_rule` runs inside `$(...)`, where
        # bash clears `set -e`, so its write is checked by hand. Perturbation:
        # leave the write unchecked, and an empty stage passes visudo and is
        # installed as the rule.
        self.env.update(ALLOW_APPLY_CHECKS="1", TEE_FAIL="/.weaver-m1.")
        result = self.create("--apply")
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertIn("cannot write the staged sudo rule", result.stderr)
        self.assertFalse(list((self.root / "etc" / "sudoers.d").iterdir()))
        self.assertFalse((self.config / "m1").exists())

    def test_a_rule_visudo_refuses_stops_before_anything_is_made_and_names_sudo_rs(self):
        # **#577 (and D4 of the plan)**: on the walk the box's sudo was
        # sudo-rs, whose visudo refuses `Defaults:<user> !pam_session`, and
        # the refusal came at the rule's install after every account and the
        # territory stood. The rule is now judged before the accounts, and
        # the refusal names sudo-rs where it is the box's sudo. Perturbation:
        # drop the early check, and useradd and groupadd run before the
        # refusal; drop the note, and sudo-rs goes unnamed.
        for version, named in (("sudo-rs 0.2.5", True), ("Sudo version 1.9.16", False)):
            with self.subTest(version=version):
                self.reset_made()
                self.env.update(ALLOW_APPLY_CHECKS="1", VISUDO_FAIL="1", SUDO_VERSION=version)
                result = self.create("--apply")
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("visudo refuses the rendered rule", result.stderr)
                if named:
                    self.assertIn("This box's sudo is sudo-rs 0.2.5", result.stderr)
                    self.assertIn("Defaults:weaver-admincon !pam_session", result.stderr)
                    self.assertIn("make the original sudo the box's sudo", result.stderr)
                else:
                    self.assertNotIn("sudo-rs", result.stderr)
                calls = self.calls()
                made = [c for c in calls if c[:2] in (["sudo", "useradd"], ["sudo", "groupadd"], ["sudo", "usermod"])
                        or (c[:2] == ["sudo", "install"] and str(self.decl) in " ".join(c))]
                self.assertEqual(made, [], made)
                self.assertNotIn("== accounts", result.stdout)
                self.assertFalse(list((self.root / "etc" / "sudoers.d").iterdir()))
                self.assertFalse(self.decl.exists())
                self.assert_no_printed_root_command(result)

    def test_the_rule_is_checked_before_the_accounts_and_again_at_its_install(self):
        # The early check removes its stage, and the install stages and
        # checks again, so a rule is never placed unchecked. Perturbation:
        # install the early stage without the second visudo, and one visudo
        # call is left.
        self.env["ALLOW_APPLY_CHECKS"] = "1"
        result = self.create("--apply")
        self.assertEqual(result.returncode, 0, result.stderr)
        calls = self.calls()
        visudo = [i for i, c in enumerate(calls) if c[:2] == ["sudo", "visudo"]]
        useradd = [i for i, c in enumerate(calls) if c[:2] == ["sudo", "useradd"]]
        self.assertEqual(len(visudo), 2, visudo)
        self.assertLess(visudo[0], min(useradd))
        self.assertGreater(visudo[1], max(useradd))
        self.assertIn("visudo accepts the rendered rule", result.stdout)
        self.assertTrue(self.rule.exists())

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
        # The relay holds the trace group alone; the admin-con account the
        # state and access groups and never the agent's own group or a trace
        # group; the gate-con account the agent's own group and no -state,
        # -admin or -trace group of any agent; neither the other's group
        # (weaver-admin-Spec section 2). Perturbations: drop any check, and a
        # mis-grouped account is admitted.
        for env, said in ((dict(RELAY_GROUPS="weaver-m1-trace weaver-m1-state"), "not the trace group alone"),
                          (dict(ADMINCON_GROUPS="weaver-admincon weaver-m1-state"), "does not hold weaver-m1-admin"),
                          (dict(ADMINCON_GROUPS="weaver-admincon weaver-m1-admin"), "does not hold weaver-m1-state"),
                          (dict(ADMINCON_GROUPS="weaver-admincon weaver-m1-state weaver-m1-admin weaver-m1-trace"),
                           "holds weaver-m1-trace"),
                          (dict(ADMINCON_GROUPS="weaver-admincon weaver-m1-state weaver-m1-admin weaver-m1"),
                           "holds weaver-m1, outside what the box's account may hold"),
                          (dict(ADMINCON_GROUPS="weaver-admincon weaver-m1-state weaver-m1-admin weaver-gatecon"),
                           "holds weaver-gatecon, outside what the box's account may hold"),
                          (dict(GATECON_GROUPS="weaver-gatecon"), "does not hold weaver-m1"),
                          (dict(GATECON_GROUPS="weaver-gatecon weaver-m1 weaver-other-admin"),
                           "holds weaver-other-admin, outside what the box's account may hold"),
                          (dict(GATECON_GROUPS="weaver-gatecon weaver-m1 weaver-other-state"),
                           "holds weaver-other-state, outside what the box's account may hold"),
                          (dict(GATECON_GROUPS="weaver-gatecon weaver-m1 weaver-other-trace"),
                           "holds weaver-other-trace, outside what the box's account may hold"),
                          # Clean when judged after laying, crossed once joined:
                          # the wall's own check after the join refuses.
                          (dict(ADMINCON_JOINED="weaver-admincon weaver-m1-state weaver-m1-admin weaver-m1-trace"),
                           "holds weaver-m1-trace, outside what the box's account may hold"),
                          (dict(GATECON_JOINED="weaver-gatecon weaver-m1 weaver-m1-state"),
                           "holds weaver-m1-state, outside what the box's account may hold"),
                          # The wall uses the same allow-list: a foreign group
                          # joined after the judge refuses there too.
                          (dict(GATECON_JOINED="weaver-gatecon weaver-m1 sudo"),
                           "holds sudo, outside what the box's account may hold"),
                          (dict(ADMINCON_JOINED="weaver-admincon weaver-m1-state weaver-m1-admin disk"),
                           "holds disk, outside what the box's account may hold")):
            with self.subTest(env=env):
                shutil.rmtree(self.config / ".m1.partial", ignore_errors=True)
                shutil.rmtree(self.folder, ignore_errors=True)
                for name in ("RELAY_GROUPS", "ADMINCON_GROUPS", "GATECON_GROUPS", "ADMINCON_JOINED", "GATECON_JOINED"):
                    self.env.pop(name, None)
                (self.root / "laid-box-accounts").unlink(missing_ok=True)
                (self.root / "joined-box-accounts").unlink(missing_ok=True)
                self.env.update(ALLOW_APPLY_CHECKS="1", **env)
                result = self.create("--apply")
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(said, result.stderr)
                self.assertFalse((self.config / "m1").exists())
                self.assertFalse(self.rule.exists())

    def test_a_standing_box_account_is_judged_by_its_shape(self):
        """**A box account that stands is adopted only in the shape this
        script would lay** (weaver-admin-Spec section 4): a system uid, no
        login shell, no home, a primary group of its own that no other
        account shares, and no group reaching the other account's door. Each
        case refuses by name in the plan, before anything privileged.
        Perturbation: drop any one judgment and its case plans through."""
        well = "weaver-gatecon:x:990:990::/home/weaver-gatecon:/usr/sbin/nologin"
        cases = (
            (dict(BOX_PASSWD={"weaver-gatecon": "weaver-gatecon:x:1500:1500::/home/weaver-gatecon:/usr/sbin/nologin"},
                  BOX_GROUP={"1500": "weaver-gatecon:x:1500:"}), None, "not a system account"),
            (dict(BOX_PASSWD={"weaver-gatecon": "weaver-gatecon:x:990:990::/home/weaver-gatecon:/bin/bash"}),
             None, "login shell '/bin/bash'"),
            (dict(BOX_PASSWD={"weaver-gatecon": well}), "home", "with a home at /home/weaver-gatecon"),
            (dict(BOX_PASSWD={"weaver-gatecon": well}, BOX_GROUP={"990": "weaver-gatecon:x:990:someone"}),
             None, "primary group weaver-gatecon is shared with someone"),
            (dict(BOX_PASSWD={"weaver-gatecon": well}, EXTRA_PASSWD="other:x:2000:990::/home/other:/bin/bash"),
             None, "is the primary group of other too"),
            (dict(BOX_PASSWD={"weaver-gatecon": well}, GATECON_GROUPS="weaver-gatecon weaver-existing-admin"),
             None, "holds weaver-existing-admin, outside what the box's account may hold"),
            (dict(BOX_PASSWD={"weaver-admincon": "weaver-admincon:x:991:991::/home/weaver-admincon:/usr/sbin/nologin"},
                  ADMINCON_GROUPS="weaver-admincon weaver-existing-state weaver-existing"),
             None, "holds weaver-existing, outside what the box's account may hold"),
            # An agent's own group whatever hyphens its name carries.
            (dict(BOX_PASSWD={"weaver-admincon": "weaver-admincon:x:991:991::/home/weaver-admincon:/usr/sbin/nologin"},
                  ADMINCON_GROUPS="weaver-admincon weaver-my-bot-state weaver-my-bot"),
             None, "holds weaver-my-bot, outside what the box's account may hold"),
            # The earlier layout's per-agent connector group, on gate-con.
            (dict(BOX_PASSWD={"weaver-gatecon": well}, GATECON_GROUPS="weaver-gatecon weaver-old-admincon"),
             None, "holds weaver-old-admincon, outside what the box's account may hold"),
            # **Outside the weaver- namespace alike** (the Planner's grade of
            # ac623d58): a foreign group refuses on either account, and a
            # relay group on gate-con.
            (dict(BOX_PASSWD={"weaver-admincon": "weaver-admincon:x:991:991::/home/weaver-admincon:/usr/sbin/nologin"},
                  ADMINCON_GROUPS="weaver-admincon weaver-existing-state wheel"),
             None, "holds wheel, outside what the box's account may hold"),
            (dict(BOX_PASSWD={"weaver-gatecon": well}, GATECON_GROUPS="weaver-gatecon weaver-existing docker"),
             None, "holds docker, outside what the box's account may hold"),
            (dict(BOX_PASSWD={"weaver-gatecon": well}, GATECON_GROUPS="weaver-gatecon weaver-existing-relay"),
             None, "holds weaver-existing-relay, outside what the box's account may hold"),
            (dict(BOX_PASSWD={"weaver-admincon": "weaver-admincon:x:991:991::/home/weaver-admincon:/usr/sbin/nologin"},
                  ADMINCON_GROUPS="weaver-admincon weaver-existing-relay"),
             None, "holds weaver-existing-relay, outside what the box's account may hold"),
            (dict(BOX_PASSWD={"weaver-admincon": "weaver-admincon:x:0:991::/home/weaver-admincon:/usr/sbin/nologin"},
                  BOX_GROUP={"991": "weaver-admincon:x:991:"}), None, "stands with uid 0, root's"),
            # **A primary group of its own is its own by name** (CodeRabbit on
            # #533): an agent's access group that no one else holds, and that
            # the allow-list admits, is still not the account's own.
            (dict(BOX_PASSWD={"weaver-admincon": "weaver-admincon:x:991:1700::/home/weaver-admincon:/usr/sbin/nologin"},
                  BOX_GROUP={"1700": "weaver-m1-admin:x:1700:"}, ADMINCON_GROUPS="weaver-m1-admin"),
             None, "primary group is weaver-m1-admin, not its own group weaver-admincon"),
        )
        home = self.root / "home" / "weaver-gatecon"
        for env, layout, said in cases:
            with self.subTest(said=said):
                for name in ("BOX_PASSWD", "BOX_GROUP", "EXTRA_PASSWD", "GATECON_GROUPS", "ADMINCON_GROUPS"):
                    self.env.pop(name, None)
                shutil.rmtree(home, ignore_errors=True)
                if layout == "home": home.mkdir(parents=True)
                self.env.update({k: json.dumps(v) if isinstance(v, dict) else v for k, v in env.items()})
                result = self.create()
                self.assertNotEqual(result.returncode, 0, result.stdout)
                self.assertIn(said, result.stderr)
                self.assertIn("so it is not adopted as the box's account", result.stderr)
        shutil.rmtree(home, ignore_errors=True)
        self.assert_unprivileged()
        # A standing account of the right shape is adopted: the plan says it
        # was found and judged, and the access rule names its uid.
        for name in ("BOX_GROUP", "EXTRA_PASSWD", "GATECON_GROUPS", "ADMINCON_GROUPS"):
            self.env.pop(name, None)
        self.env["BOX_PASSWD"] = json.dumps({"weaver-gatecon": well})
        result = self.create()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("weaver-gatecon  the box's, found standing: judged below", result.stdout)
        self.assertIn("weaver-admincon  the box's, absent: laid at --apply", result.stdout)
        self.assertIn("weaver-gatecon stands, and its shape is the box's gate-con account", result.stdout)
        self.assertIn("allowed-uids = [990] (weaver-gatecon alone)", result.stdout)

    def test_the_rule_written_names_the_gatecon_uid_alone(self):
        # **The rendered rule is asserted before it is written** (the
        # Planner's guard on ac623d58): the plan's placeholder 65534, a uid
        # that is not gate-con's, root's, or a rule with a second line each
        # refuse; gate-con's uid alone passes. Perturbation: drop the
        # assertion, or its placeholder test, and a case passes.
        script = (DEPLOY / "create-agent.sh").read_text()
        program = ('die() { printf "REFUSED: %s\\n" "$*" >&2; exit 1; }\n'
                   + shell_function(script, "rule_names_gatecon")
                   + 'GATECON_USER=weaver-gatecon; GATECON_UID=$1; DECLARATION_TEXT=$2; rule_names_gatecon; echo PASSED')
        for uid, text, passes in (("990", "x = 1\nallowed-uids = [990]\nallowed-gids = []", True),
                                  ("990", "allowed-uids = [65534]", False),
                                  ("65534", "allowed-uids = [65534]", False),
                                  ("0", "allowed-uids = [0]", False),
                                  ("", "allowed-uids = []", False),
                                  ("990", "allowed-uids = [990]\nallowed-uids = [1000]", False),
                                  ("990", "allowed-uids = [990, 1000]", False)):
            with self.subTest(uid=uid, text=text):
                run = subprocess.run(["bash", "-c", program, "x", uid, text], text=True,
                                     capture_output=True, timeout=20)
                self.assertEqual("PASSED" in run.stdout, passes, run.stdout + run.stderr)
                self.assertEqual(run.returncode == 0, passes, run.stderr)
        self.assertEqual(script.count("\nrule_names_gatecon\n"), 2,
                         "asserted after the re-render and again at the write")

    def test_a_box_group_without_its_account_refuses_in_the_plan(self):
        # **An absent box account with a group of its name standing refuses
        # before anything is made**: apply would lay the account with its own
        # group, which useradd refuses over a standing one, after the agent's
        # accounts. Perturbation: drop the plan's group check and the plan
        # passes.
        for box in ("weaver-gatecon", "weaver-admincon"):
            with self.subTest(box=box):
                self.env["COLLISION_GROUP"] = box
                result = self.create()
                self.assertNotEqual(result.returncode, 0, result.stdout)
                self.assertIn(f"the group {box} stands without its account", result.stderr)
        self.env.pop("COLLISION_GROUP", None)
        self.assert_unprivileged()
        script = (DEPLOY / "create-agent.sh").read_text()
        accounts = script.index('say "accounts"')
        self.assertLess(script.index('root useradd --system --shell /usr/sbin/nologin --no-create-home --user-group "$box_account"', accounts),
                        script.index('--create-home --home-dir "$AGENT_HOME" --user-group "$AGENT_USER"', accounts),
                        "the box's accounts are laid before any of the agent's")

    def test_the_box_accounts_names_are_not_agents(self):
        # weaver-admin-Spec section 4: an agent named gatecon or admincon would
        # own the box's account as its own identity, so create-agent refuses
        # both names before any call, as admin does. Perturbation: drop the
        # refusal and the plan runs.
        for reserved in ("gatecon", "admincon"):
            with self.subTest(reserved=reserved):
                self.install_stack()
                result = self.run_script("create-agent.sh", reserved, "--artifact", str(self.artifact))
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(f"'{reserved}' is reserved", result.stderr)
        self.assertEqual(self.calls(), [])

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
        # The operator joins the trace, state and access groups and never the
        # agent's own (it does not dial the gate); the gate-con account joins
        # the agent's own group alone, and the admin-con account the state and
        # access groups (weaver-admin-Spec section 2).
        self.assertIn(["sudo", "usermod", "-aG", "weaver-m1-trace,weaver-m1-state,weaver-m1-admin", "fixture-no-home"], calls)
        self.assertIn(["sudo", "usermod", "-aG", "weaver-m1", "weaver-gatecon"], calls)
        self.assertIn(["sudo", "usermod", "-aG", "weaver-m1-state,weaver-m1-admin", "weaver-admincon"], calls)
        self.assertFalse([c for c in calls if c[:3] == ["sudo", "usermod", "-aG"] and c[-1] == "fixture-no-home"
                          and "weaver-m1" in c[3].split(",")], calls)
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
        self.assertIn(look_as("weaver-admincon", "-r", territory + "/agent.toml"), calls)
        for f in ("agent.toml", "system-prompt.md", "save-points"):
            self.assertIn(look_as("weaver-admincon", "-w", f"{territory}/{f}"), calls)
        # Every mode the script gives the territory's files grants the group no write.
        for c in calls:
            if c[:2] == ["sudo", "install"] and "-m" in c and c[-1].startswith(territory):
                self.assertEqual(int(c[c.index("-m") + 1], 8) & 0o022, 0, c)
            if c[:2] == ["sudo", "chmod"] and any(a.startswith(territory) for a in c):
                self.assertEqual(int(c[2], 8) & 0o022, 0, c)
        self.log.unlink(missing_ok=True)
        shutil.rmtree(self.config / "m1", ignore_errors=True)
        shutil.rmtree(self.config / ".m1.partial", ignore_errors=True)
        shutil.rmtree(self.folder, ignore_errors=True)
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
        spu = "/opt/elsewhere/custom-spu"
        result = self.create("--spu", spu)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("spu-binary      " + spu, result.stdout)
        self.env["ALLOW_APPLY_CHECKS"] = "1"
        result = self.create("--spu", spu, "--apply")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assert_agent_root("sqlite", spu=spu)
        self.assertEqual((self.stack / "spu-binary").read_text(), self.stack_keys["spu-binary"] + "\n")

    def test_retired_python_binaries_refuse_before_provisioning_or_build(self):
        # The Rust fixture is the control. Restoring any retired election must
        # refuse before provisioning or a build, including per-agent overrides.
        control = self.run_script("update-stack.sh")
        self.assertEqual(control.returncode, 0, control.stderr)
        for path in ("/opt/weaver/bin/pyworker", "/opt/elsewhere/python-spu.pyz",
                     "/opt/weaver/python-spu/renamed.pyz"):
            for where, role in (("stack", "worker-binary"), ("stack", "spu-binary"),
                                ("agent", "worker-binary"), ("agent", "spu-binary"),
                                ("create", "worker-binary"), ("override", "spu-binary")):
                with self.subTest(path=path, where=where):
                    key = (self.stack / role if where in ("stack", "create")
                           else self.config / "existing" / role)
                    old = key.read_text() if key.exists() else None
                    try:
                        if where != "override":
                            key.write_text(path + "\n")
                        self.log.unlink(missing_ok=True)
                        if where == "override":
                            result = self.create("--spu", path)
                        elif where == "create":
                            result = self.create()
                        else:
                            result = self.run_script("update-stack.sh")
                        self.assertEqual(result.returncode, 1, result.stdout)
                        self.assertIn("retired Python executable", result.stderr)
                        self.assertIn(path, result.stderr)
                        self.assertFalse(any(c[0] in ("cargo", "sudo") for c in self.calls()))
                    finally:
                        if old is None:
                            key.unlink(missing_ok=True)
                        else:
                            key.write_text(old)

    def test_retired_python_installations_are_preserved_and_refused(self):
        # Removing a member from MEMBERS does not remove its installed file.
        # Ordinary files, directories and dangling links all require cleanup.
        prefix = Path(self.stack_keys["prefix"])
        self.env["WEAVER_PREFIX"] = str(prefix)
        for relative in ("pyworker", "python-spu", "bin/pyworker"):
            path = prefix / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            for form in ("file", "directory", "link"):
                with self.subTest(relative=relative, form=form):
                    if form == "file":
                        path.write_text("old installation")
                    elif form == "directory":
                        path.mkdir()
                    else:
                        path.symlink_to(prefix / "missing")
                    try:
                        # update/create use the recorded binary directory.
                        scripts = [("update-stack.sh", ()), ("create-agent.sh", (
                            "m1", "--artifact", str(self.artifact)))]
                        # bootstrap uses prefix/bin, as its layout requires.
                        if relative == "bin/pyworker":
                            scripts = [("bootstrap-stack.sh", ())]
                        elif relative == "python-spu":
                            scripts.append(("bootstrap-stack.sh", ()))
                        for script, args in scripts:
                            self.log.unlink(missing_ok=True)
                            result = self.run_script(script, *args)
                            self.assertEqual(result.returncode, 1, result.stdout)
                            self.assertIn("retired Python installation remains", result.stderr)
                            self.assertTrue(path.exists() or path.is_symlink())
                            self.assertFalse(any(c[0] in ("cargo", "sudo") for c in self.calls()))
                    finally:
                        if form == "directory":
                            path.rmdir()
                        else:
                            path.unlink()

    def test_an_spu_named_like_another_stack_binary_refuses_before_provisioning(self):
        # Codex on #45: admin's stack::judge_names refuses every verb on a root
        # whose worker, state member, gate and SPU share a file name, so an
        # --spu named like one of them refuses here, before any call. Perturbation:
        # drop the judge_names call, and each provisions.
        self.env["ALLOW_APPLY_CHECKS"] = "1"
        for name in ("worker", "weaver-state", "weaver-gate"):
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

    def standing_agents(self):
        """Five standing agents beside the fixture's base, as their roots and
        accounts name them: m9 of the layout of #578 (`<agents>/m9/weaver-m9`
        and its home beside it), m8 with a custom territory outside any folder,
        m7 of the layout before #578 (`<agents>/weaver-m7`) with its home
        under a home directory, m5 whose key names `<agents>/m5/weaver-m5`
        where only the folder stands, and m4 with a custom territory and its
        home in `<agents>/m4`."""
        agents = self.root / "agents"
        places = {"m9 folder": agents / "m9", "m9 territory": agents / "m9" / "weaver-m9",
                  "m9 home": agents / "m9" / "home", "m8 territory": self.root / "srv" / "weaver-m8",
                  "m7 territory": agents / "weaver-m7", "m7 home": self.root / "homes" / "weaver-m7",
                  "m5 folder": agents / "m5", "m4 territory": self.root / "srv" / "weaver-m4",
                  "m4 folder": agents / "m4", "m4 home": agents / "m4" / "home"}
        for path in places.values():
            path.mkdir(parents=True, exist_ok=True)
        for agent, territory in (("m9", places["m9 territory"]), ("m8", places["m8 territory"]),
                                 ("m7", places["m7 territory"]), ("m5", agents / "m5" / "weaver-m5"),
                                 ("m4", places["m4 territory"])):
            (self.config / agent).mkdir()
            (self.config / agent / "territory").write_text(f"{territory}\n")
        self.env["STANDING_PASSWD"] = json.dumps({
            "weaver-m9": f"weaver-m9:x:2009:2009::{places['m9 home']}:/usr/sbin/nologin",
            "weaver-m7": f"weaver-m7:x:2007:2007::{places['m7 home']}:/usr/sbin/nologin",
            "weaver-m4": f"weaver-m4:x:2004:2004::{places['m4 home']}:/usr/sbin/nologin"})
        return places

    def test_create_agent_refuses_a_base_inside_a_standing_agent(self):
        """**#606, the Planner's ruling of 2026-11-17**: an agent made inside
        another's folder, territory or home sits in what decommission.sh
        refuses to archive or purge, so a stack record whose agent-directory
        lies in a standing agent refuses before anything is made, in either
        mode, naming the agent and the place. The five shapes it can meet: the
        agent's folder, its territory there, its home there, a custom
        territory outside any folder, and a home outside its folder. The
        folder is seen as decommission.sh's folder_bases sees it, by its key
        or its home read lexically, so a folder whose territory is gone, and
        one only its home names, refuse too (the self-check on #606). A
        territory key that stands and cannot be read refuses, as do a root
        that cannot be searched and an admin base that cannot be listed or
        searched (CodeRabbit on #608). (A base in
        a weaver entry in /tmp is the agent's account's, which the base's own
        check refuses first, as it refuses any home on a box.) Perturbations:
        drop the territory, or the home, from a standing agent's places, and
        the custom territory, or the home outside the folder, is taken. Read
        an unreadable key as no agent, or skip the root's or the admin base's
        check, and that case is taken. Drop the folder the key names, or the
        one the home names, and the folder whose territory is gone, or the one
        only its home names, is taken."""
        places = self.standing_agents()
        cases = [("the agent's folder", places["m9 folder"], "agent m9's folder " + str(places["m9 folder"])),
                 ("its territory there", places["m9 territory"],
                  "agent m9's territory " + str(places["m9 territory"])),
                 ("its home there", places["m9 home"], "agent m9's folder " + str(places["m9 folder"])),
                 ("a custom territory outside any folder", places["m8 territory"] / "agents",
                  "agent m8's territory " + str(places["m8 territory"])),
                 ("a home outside its folder", places["m7 home"] / "agents",
                  "agent m7's home " + str(places["m7 home"])),
                 ("a folder whose territory is gone", places["m5 folder"] / "agents",
                  "agent m5's folder " + str(places["m5 folder"])),
                 ("a folder its home names, its territory elsewhere", places["m4 folder"] / "agents",
                  "agent m4's folder " + str(places["m4 folder"]))]
        for label, base, held in cases:
            base.mkdir(exist_ok=True)
            (self.stack / "agent-directory").write_text(f"{base}\n")
            for mode in ((), ("--apply",)):
                with self.subTest(label, mode=mode):
                    self.log.unlink(missing_ok=True)
                    self.env["ALLOW_APPLY_CHECKS"] = "1" if mode else ""
                    result = self.create(*mode)
                    self.assertNotEqual(result.returncode, 0, result.stdout)
                    self.assertIn(f"the stack record's agent-directory {base} lies in {held}, so an agent made "
                                  "there would sit inside another agent, which decommission.sh refuses to archive "
                                  "or purge (#606). Set the record's agent-directory back to the base the agents "
                                  "were made in, then rerun", result.stderr)
                    made = [c for c in self.calls()
                            if c[0] in ("psql", "systemctl", "mktemp", "setfacl", "as-root")
                            or (c[0] == "sudo" and c[1:2] != ["-v"] and c[1:3] != ["-n", "sh"])]
                    self.assertEqual(made, [], self.calls())
                    self.assertFalse((self.config / ".m1.partial").exists())
        (self.stack / "agent-directory").write_text(f"{self.root / 'agents'}\n")
        (self.config / "m6").mkdir()
        key = self.config / "m6" / "territory"
        key.write_text(f"{self.root / 'agents' / 'm6' / 'weaver-m6'}\n")
        for label, unread, mode, back, needs in (
                ("a territory key that cannot be read", key, 0, 0o644, os.R_OK),
                ("a root that cannot be searched", self.config / "m6", 0o644, 0o755, os.X_OK),
                ("an admin base that cannot be listed", self.config, 0o311, 0o755, os.R_OK),
                ("an admin base that cannot be searched", self.config, 0o644, 0o755, os.X_OK)):
            with self.subTest(label):
                unread.chmod(mode)
                try:
                    if os.access(unread, needs):
                        self.skipTest("this uid passes what its mode refuses")
                    result = self.create()
                finally:
                    unread.chmod(back)
                self.assertNotEqual(result.returncode, 0, result.stdout)
                self.assertIn(f"{unread} cannot be read, so whether the agent-directory {self.root / 'agents'} "
                              "lies inside a standing agent cannot be told. Read it by hand, then rerun",
                              result.stderr)

    def test_create_agent_takes_a_base_beside_or_above_a_standing_agent(self):
        """**#606**: the guard refuses only a base in a standing agent. The
        ruled layout (the base holding m9's folder and m7's territory of the
        layout before #578, whose directory is the base itself), and a base
        whose name only begins like m9's folder, plan as before.
        Perturbations: count any territory's directory as its agent's folder,
        and the layout before #578 refuses. Drop the slash, and the near name
        refuses."""
        self.standing_agents()
        near = self.root / "agents" / "m90"
        near.mkdir()
        for label, base in (("the ruled layout, with the layout before #578 beside it", self.root / "agents"),
                            ("a near name", near)):
            with self.subTest(label):
                (self.stack / "agent-directory").write_text(f"{base}\n")
                self.log.unlink(missing_ok=True)
                result = self.create()
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertNotIn("lies in agent", result.stderr)
                self.assertIn(f"folder          {base / 'm1'}", result.stdout)

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
        process standing in for the run's constituents, lists it in the agent's
        cgroup beneath the agent's scope under a stand-in cgroup mount, and writes
        a load event, `show` names it, `unload` ends it and removes the cgroup
        unless KEEP_ALIVE is set (or, for the cgroup, KEEP_CGROUP)."""
        # verify-load.sh probes its drop as nobody before anything loads
        # (#590), which the double answers only under ALLOW_APPLY_CHECKS.
        self.env.update(FIXTURE_UID="0", FIXTURE_ACCOUNT_UID=str(os.getuid()), ALLOW_APPLY_CHECKS="1")
        cgroup_fs = self.root / "cgroup-fs"
        agent_cgroup = cgroup_fs / "weaver.slice" / "weaver-m1.scope" / "weaver-m1"

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
        mkdir -p {agent_cgroup}/constituents {agent_cgroup}/worker/run
        [ -n "$MISPLACED" ] || echo $! > {agent_cgroup}/${{STAND_IN_LEAF:-constituents}}/cgroup.procs
        echo '{{"kind":"load"}}' >> {sink}
        echo '{{"kind":"state","state":"idle"}}' ;;
  show) if [ -f {state}/pid ]; then
          echo "{{\\"kind\\":\\"state\\",\\"state\\":\\"idle\\",\\"constituents\\":[$(cat {state}/pid)]}}"
        else echo '{{"kind":"state","state":"unloaded"}}'; fi ;;
  unload) if [ -f {state}/pid ]; then
            [ -n "$KEEP_ALIVE" ] || kill $(cat {state}/pid)
            cp {state}/pid {state}/last; rm {state}/pid
          fi
          [ -n "$KEEP_ALIVE$KEEP_CGROUP" ] || rm -rf {agent_cgroup}
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

    def test_verify_load_prints_the_adapter_beside_the_base(self):
        # ADP-5: the declared adapter beside the declared base, `none` where
        # the table is omitted, and the adapter member of an Open measurement
        # an event carries beside the base identity it measured. The live
        # record carries no Open measurement today, and the absence is said.
        # Perturbations: drop the adapter line, or the measurement scan, and
        # the reads below fail.
        a = "a" * 64
        for adapter, evidence in ((False, False), (True, False), (True, True)):
            with self.subTest(adapter=adapter, evidence=evidence):
                shutil.rmtree(self.config / "m1", ignore_errors=True)
                shutil.rmtree(self.folder, ignore_errors=True)
                shutil.rmtree(self.root / "installed", ignore_errors=True)
                shutil.rmtree(self.root / "stub-state", ignore_errors=True)
                state = self.verify_fixture()
                declaration = self.decl / "agent.toml"
                text = declaration.read_text() + (
                    '\n[spu-instruction.decoder.model-binding]\nartifact = "/m/base.gguf"\ndevices = [0]\n')
                if adapter:
                    text += (f'\n[spu-instruction.decoder.model-binding.adapter]\nartifact = "/m/a.gguf"\n'
                             f'sha256 = "{a}"\nbase-blake3 = "{"b" * 64}"\nscale = 0.7\n')
                declaration.write_text(text)
                if evidence:
                    stub = self.root / "installed" / "bin" / "weaver-admin"
                    sink = self.root / "agents" / "trace.ndjson"
                    placement = json.dumps({"kind": "state.placement", "payload": {"open_evidence": {
                        "request": {}, "measurement": {"model": "/m/base.gguf", "weights_hash": "h",
                                                       "adapter": {"artifact": "/m/a.gguf", "sha256": a,
                                                                   "scale": 0.7}}}}})
                    stub.write_text(stub.read_text().replace(
                        "echo '{\"kind\":\"load\"}' >>",
                        f"echo '{placement}' >> {sink}; echo '{{\"kind\":\"load\"}}' >>"))
                try:
                    result = self.run_script("verify-load.sh", "m1")
                finally:
                    self.end_stand_in(state)
                self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
                self.assertIn("base        /m/base.gguf (declared)", result.stdout)
                if adapter:
                    self.assertIn(f"adapter     /m/a.gguf sha256 {a} scale 0.7 over base blake3 {'b' * 64} (declared)",
                                  result.stdout)
                else:
                    self.assertIn("adapter     none (declared)", result.stdout)
                if evidence:
                    self.assertIn('open measurement: model /m/base.gguf weights_hash h adapter '
                                  f'{{"artifact": "/m/a.gguf", "sha256": "{a}", "scale": 0.7}}', result.stdout)
                else:
                    self.assertIn("open measurement: none in the new events", result.stdout)

    def test_verify_load_checks_each_constituent_and_that_none_is_left(self):
        # The operator contract section 2: every constituent `show` names runs
        # as one of the agent's accounts and sits in the agent's cgroup beneath
        # its own scope, a relay among them for a file sink, and after the
        # unload none is left.
        state = self.verify_fixture()
        try:
            result = self.run_script("verify-load.sh", "m1")
        finally:
            self.end_stand_in(state)
        self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
        self.assertIn("in constituents beneath the agent's scope", result.stdout)
        self.assertIn("every constituent gone, the agent's cgroup gone, show unloaded", result.stdout)

    def test_verify_load_refuses_a_constituent_outside_the_agents_cgroup(self):
        """**A constituent not in the agent's cgroup beneath its scope,
        and an agent's cgroup outliving the unload, each refuse**
        (weaver-admin-Spec 6.1): verify-load reads each constituent's leaf
        and the cgroup's absence after the unload. Perturbations: drop the
        leaf check, and the misplaced constituent is verified; drop the
        absence check, and the cgroup left standing is."""
        for case, variable, said in (
                ("misplaced", "MISPLACED", "is not in "),
                ("outlived", "KEEP_CGROUP", "outlived the unload")):
            with self.subTest(case):
                shutil.rmtree(self.config / "m1", ignore_errors=True)
                shutil.rmtree(self.folder, ignore_errors=True)
                shutil.rmtree(self.root / "stub-state", ignore_errors=True)
                shutil.rmtree(self.root / "installed", ignore_errors=True)
                shutil.rmtree(self.root / "cgroup-fs", ignore_errors=True)
                state = self.verify_fixture()
                self.env[variable] = "1"
                try:
                    result = self.run_script("verify-load.sh", "m1")
                finally:
                    del self.env[variable]
                    self.end_stand_in(state)
                self.assertNotEqual(result.returncode, 0, result.stdout)
                self.assertIn(said, result.stderr)

    def test_verify_load_checks_the_worker_in_worker_run(self):
        """**The agent's own account is checked in `worker/run/`** (weaver-admin-Spec
        6.1): a stand-in running as the agent's account passes the leaf check
        in `worker/run/` and refuses in `constituents/`. It stands alone, so
        the file sink's relay check refuses after it either way.
        Perturbation: check the worker in `constituents/`, and the first
        case refuses at the leaf."""
        for leaf, passes in (("worker/run", True), ("constituents", False)):
            with self.subTest(leaf):
                for d in (self.config / "m1", self.folder, self.root / "stub-state",
                          self.root / "installed", self.root / "cgroup-fs"):
                    shutil.rmtree(d, ignore_errors=True)
                state = self.verify_fixture()
                self.env.update(FIXTURE_ACCOUNT_FOR="weaver-m1", STAND_IN_LEAF=leaf)
                try:
                    result = self.run_script("verify-load.sh", "m1")
                finally:
                    for name in ("FIXTURE_ACCOUNT_FOR", "STAND_IN_LEAF"):
                        del self.env[name]
                    self.end_stand_in(state)
                self.assertNotEqual(result.returncode, 0, result.stdout)
                if passes:
                    self.assertIn("weaver-m1  in worker/run beneath the agent's scope", result.stdout)
                    self.assertIn("no relay stands among the constituents", result.stderr)
                else:
                    self.assertIn("is not in ", result.stderr)

    def test_verify_load_refuses_a_drop_that_cannot_run_before_its_first_unload(self):
        """**#590**: verify-load.sh's reads as the agent's accounts discard
        their own refusal, so a runuser that cannot drop is probed as nobody
        and refused by name before admin unloads or loads anything.
        Perturbation: drop the probe, and admin is asked to unload and load."""
        stub = self.root / "installed" / "bin" / "weaver-admin"
        verbs = self.root / "admin-verbs"
        state = self.verify_fixture()
        text = stub.read_text()
        stub.write_text(text.replace('#!/bin/sh\n', f'#!/bin/sh\necho "$1" >> {shlex.quote(str(verbs))}\n', 1))
        self.env["DROP_FAIL"] = "nobody"
        try:
            result = self.run_script("verify-load.sh", "m1")
        finally:
            self.env.pop("DROP_FAIL")
            self.end_stand_in(state)
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertIn("runuser would not run a command as nobody", result.stderr)
        self.assertEqual(verbs.read_text().split(), ["validate"])

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
                shutil.rmtree(self.folder, ignore_errors=True)
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

    def test_verify_load_prints_the_deployment_tuple(self):
        """**verify-load prints the tuple after the load event**, every row
        ending in one of the four status words, and the run verifies.
        Perturbation: drop the tuple block and the heading is missing."""
        state = self.verify_fixture()
        try:
            result = self.run_script("verify-load.sh", "m1")
        finally:
            self.end_stand_in(state)
        self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
        self.assertIn("== m1 deployment tuple", result.stdout)
        rows = [line for line in result.stdout.splitlines()
                if re.match(r"^   (row [0-9]|state|restore) ", line)]
        self.assertGreaterEqual(len(rows), 12, result.stdout)
        for line in rows:
            self.assertRegex(line, r"\[(recorded|derivable|absent|FAULT): ")
        self.assertIn("m1 verified", result.stdout)

    def test_a_tuple_fault_unloads_the_run(self):
        """**A FAULT line fails the verification and unloads**, `--keep`
        included: here the load event's state_store names another engine than
        the declaration. Perturbation: ignore the helper's status and the run
        verifies (and under --keep stays serving)."""
        state = self.verify_fixture()
        sink = self.root / "agents" / "trace.ndjson"
        (self.decl / "agent.toml").write_text(
            f'[trace-sink]\nkind = "file"\npath = "{sink}"\n\n[state-store]\nengine = "sqlite"\n')
        stub = self.root / "installed" / "bin" / "weaver-admin"
        stub.write_text(stub.read_text().replace(
            "echo '{\"kind\":\"load\"}'",
            "echo '{\"kind\":\"load\",\"payload\":{\"state_store\":{\"engine\":\"none\"}}}'"))
        try:
            result = self.run_script("verify-load.sh", "m1", "--keep")
        finally:
            self.end_stand_in(state)
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertIn("[FAULT: the load event's state_store names 'none'", result.stdout)
        self.assertIn("the deployment tuple carries a FAULT line, so the run is unloaded", result.stderr)
        self.assertFalse((state / "pid").exists(), "no run is left")

    def tuple_load(self, lines, spu=True):
        """A verify fixture whose stack record holds the install tuple
        `lines` (INSTALL_TUPLE_LINES or EARLIER_TUPLE_LINES, filled from the
        fixture's own files), whose load event binds the declaration and the
        worker, and whose agent root names an installed SPU and the library
        path, as create-agent.sh copies them, unless `spu` is False."""
        state = self.verify_fixture()
        decl = (self.decl / "agent.toml").read_bytes()
        runtime = {}
        for key, name in (("cudart", "libcudart.so.99"), ("cublas", "libcublas.so.99"),
                          ("cublaslt", "libcublasLt.so.99"), ("curand", "libcurand.so.99")):
            real = (self.cuda_lib / name).resolve()
            runtime[key] = f"{hashlib.sha256(real.read_bytes()).hexdigest()} {real}"
        filled = [line.format(llama="1" * 64, ggml="2" * 64, **runtime) for line in lines]
        (self.stack / "install-tuple").write_text("".join(l + "\n" for l in filled))
        if spu:
            (self.root / "installed" / "weaver-spu").write_text("fixture spu\n")
            root = self.config / "m1"
            (root / "spu-binary").write_text(self.stack_keys["spu-binary"] + "\n")
            (root / "library-path").write_text(self.stack_keys["library-path"] + "\n")
        payload = json.dumps({"declaration": hashlib.sha256(decl).hexdigest(),
                              "stack": {"worker": "1" * 64}})
        stub = self.root / "installed" / "bin" / "weaver-admin"
        stub.write_text(stub.read_text().replace(
            "echo '{\"kind\":\"load\"}'", "echo '{\"kind\":\"load\",\"payload\":" + payload + "}'"))
        return state

    def test_verify_load_binds_the_record_to_the_install_tuple(self):
        """**The box side and the record binding print from the stack record's
        install tuple**: its digest as the box's identity, the declaration's
        sha256 against the load event's, the stack against its members, and
        each runtime file the install recorded against the file the loader
        resolves for the agent's SPU now, with the agent's library path and
        none of the operator's loader variables. The driver and libcuda are
        read at this load. Perturbations: drop the install tuple's argument
        in verify-load and the box line reads absent; drop `--live-ldd` and
        every runtime row reads absent; drop the library path from ldd's
        environment and the double resolves the engine from elsewhere."""
        state = self.tuple_load(INSTALL_TUPLE_LINES)
        tuple_sha = hashlib.sha256((self.stack / "install-tuple").read_bytes()).hexdigest()
        # The operator's shell carries its own library path, which the
        # loader's read for the agent's SPU never takes.
        self.env["LD_LIBRARY_PATH"] = "/operator/out/lib"
        try:
            result = self.run_script("verify-load.sh", "m1")
        finally:
            self.end_stand_in(state)
        self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
        self.assertRegex(result.stdout, r"box +install tuple +sha256 " + tuple_sha + r" +\[recorded:")
        self.assertRegex(result.stdout, r"bind +declaration +sha256 [0-9a-f]{64} +\[recorded: equal")
        self.assertRegex(result.stdout, r"bind +member worker +1{64} +\[recorded:")
        for key in ("cudart", "cublas", "cublaslt", "curand"):
            self.assertRegex(result.stdout, r"bind +runtime " + key + r" +[0-9a-f]{64} /\S+ +\[recorded:")
        self.assertRegex(result.stdout, r"live +driver +999\.1\.2 +\[derivable:")
        self.assertRegex(result.stdout, r"live +libcuda +[0-9a-f]{64} \S+libcuda\.so\.999\.1\.2 +\[derivable:")
        self.assertNotIn("tuple form", result.stdout)
        self.assertNotIn("FAULT", result.stdout)
        self.assertIn("m1 verified", result.stdout)
        self.assertIn(["ldd-env", self.stack_keys["library-path"], ""],
                      [c for c in self.calls() if c[0] == "ldd-env"])

    def test_verify_load_reads_an_earlier_tuple_and_binds_no_runtime_file(self):
        """**A record of the earlier form verifies**, so the host's pinned
        stack keeps verifying until an install rewrites it: the form is
        named with the way to rewrite it, its card lines are never judged
        (the fixture's cards differ from them), and no runtime file is
        bound. Perturbation: refuse the earlier form in the helper and the
        load is unloaded."""
        state = self.tuple_load(EARLIER_TUPLE_LINES)
        try:
            result = self.run_script("verify-load.sh", "m1")
        finally:
            self.end_stand_in(state)
        self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
        self.assertRegex(result.stdout, r"box +tuple form +earlier: cublas, cublaslt, cudart, driver, gpu\.0, "
                                        r"gpu\.1, gpus +\[absent:")
        for key in ("cudart", "cublas", "cublaslt", "curand"):
            self.assertRegex(result.stdout, r"bind +runtime " + key + r" .*\[absent: not bound: the install "
                                            r"tuple is of the earlier form")
        self.assertNotIn("FAULT", result.stdout)
        self.assertIn("m1 verified", result.stdout)

    def test_verify_load_unloads_a_box_whose_runtime_drifted_from_its_install(self):
        """**A runtime file that changed since the install is a FAULT and the
        run is unloaded**: the install recorded one file and the loader now
        resolves another, which a sealed image never shows. Perturbation:
        bind by path alone and the patched file passes."""
        state = self.tuple_load(INSTALL_TUPLE_LINES)
        (self.cuda_lib / "libcurand.so.99.5.6").write_text("patched since the install\n")
        try:
            result = self.run_script("verify-load.sh", "m1", "--keep")
        finally:
            self.end_stand_in(state)
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertRegex(result.stdout, r"bind +runtime curand .*\[FAULT: drift from the install")
        self.assertRegex(result.stdout, r"bind +runtime cudart .*\[recorded:")
        self.assertIn("the deployment tuple carries a FAULT line, so the run is unloaded", result.stderr)
        self.assertFalse((state / "pid").exists(), "no run is left")

    def untrusted_loader(self, setup, said):
        """**Root runs ldd only on an SPU, and with a library directory, that
        admin's rule holds closed, and it fails closed**: where it cannot,
        each runtime file the install recorded is a FAULT naming why, and the
        run is unloaded. Admin refuses such an SPU at load, so a loaded agent
        showing one is not what was installed. `setup` breaks one premise
        after the fixture stands."""
        state = self.tuple_load(INSTALL_TUPLE_LINES)
        setup()
        try:
            result = self.run_script("verify-load.sh", "m1", "--keep")
        finally:
            self.end_stand_in(state)
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertRegex(result.stdout, r"live +libcuda .*\[absent: not read: .*" + re.escape(said))
        self.assertRegex(result.stdout, r"bind +runtime cudart .*\[FAULT: the install tuple records [0-9a-f]{64} "
                                        r"\S+, and the loader's read cannot be bound to it: .*" + re.escape(said))
        self.assertIn("the deployment tuple carries a FAULT line, so the run is unloaded", result.stderr)
        self.assertFalse([c for c in self.calls() if c[0] == "ldd-env"], "ldd never ran")

    def test_verify_load_faults_a_root_naming_no_spu(self):
        self.untrusted_loader(lambda: (self.config / "m1" / "spu-binary").unlink(),
                              "the agent's config root names no spu-binary")

    def test_verify_load_faults_an_spu_that_is_not_there(self):
        self.untrusted_loader(lambda: (self.root / "installed" / "weaver-spu").unlink(), "no SPU at")

    def test_verify_load_runs_no_loader_on_an_spu_another_principal_could_write(self):
        """Perturbation: drop the held_closed check on the SPU and ldd runs."""
        self.untrusted_loader(lambda: (self.root / "installed" / "weaver-spu").chmod(0o666),
                              "is not held closed by root at")

    def test_verify_load_runs_no_loader_with_a_library_path_another_principal_could_write(self):
        """Perturbation: drop the held_closed check on the library path and
        ldd runs with it."""
        self.untrusted_loader(lambda: (self.root / "installed" / "lib").chmod(0o777),
                              "the agent's library path")

    def test_verify_load_resolves_the_library_path_once_and_hands_ldd_the_resolved_path(self):
        """**The library path judged is the library path used**: the root
        names a link, the link's target is what is held closed, and ldd gets
        the target, so the link cannot be moved between the two.
        Perturbation: hand ldd the root's value as written and it gets the
        link."""
        state = self.tuple_load(INSTALL_TUPLE_LINES)
        link = self.root / "installed" / "lib-link"
        link.symlink_to("lib")
        for where in (self.config / "m1", self.stack):
            (where / "library-path").write_text(f"{link}\n")
        try:
            result = self.run_script("verify-load.sh", "m1")
        finally:
            self.end_stand_in(state)
        self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
        seen = [c[1] for c in self.calls() if c[0] == "ldd-env"]
        self.assertEqual(seen, [str(self.root / "installed" / "lib")])
        self.assertRegex(result.stdout, r"bind +runtime cudart .*\[recorded:")

    def test_verify_load_binds_no_runtime_for_an_agent_that_runs_another_spu(self):
        """**Like for like** (the self-check of 2026-11-14): an agent made with
        `create-agent.sh --spu` runs an SPU the install never read, so its
        runtime files are not bound, the rows say which SPU, the live
        libcuda is still read from it, and nothing faults. Perturbation:
        drop the comparison with the stack record's SPU and the custom SPU's
        runtime is judged against the installed one's."""
        state = self.tuple_load(INSTALL_TUPLE_LINES)
        custom = self.root / "installed" / "custom-spu"
        custom.write_text("another spu\n")
        (self.config / "m1" / "spu-binary").write_text(f"{custom}\n")
        # The custom SPU resolves its runtime elsewhere, as a build against
        # another CUDA would.
        elsewhere = self.root / "other-cuda"
        shutil.copytree(self.cuda_lib, elsewhere, symlinks=True)
        (elsewhere / "libcublas.so.99.3.4").write_text("another cublas\n")
        self.env["LDD_RUNTIME"] = str(elsewhere)
        try:
            result = self.run_script("verify-load.sh", "m1")
        finally:
            self.end_stand_in(state)
        self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
        self.assertRegex(result.stdout, r"bind +runtime cublas .*\[absent: not bound: this agent's SPU is "
                                        + re.escape(str(custom)) + r", not the installed ")
        self.assertRegex(result.stdout, r"live +libcuda +[0-9a-f]{64} .*\[derivable:")
        self.assertNotIn("FAULT", result.stdout)

    def test_verify_load_binds_no_runtime_for_a_root_naming_another_library_path(self):
        """**Like for like**: a root whose library path differs from the one the
        install read with resolves through another directory, so the rows
        are absent and say which. Perturbation: drop the comparison and the
        resolution through the root's directory is judged as drift."""
        state = self.tuple_load(INSTALL_TUPLE_LINES)
        other = self.root / "installed" / "other-lib"
        other.mkdir()
        (self.config / "m1" / "library-path").write_text(f"{other}\n")
        try:
            result = self.run_script("verify-load.sh", "m1")
        finally:
            self.end_stand_in(state)
        self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
        self.assertRegex(result.stdout, r"bind +runtime cudart .*\[absent: not bound: this agent's library path is "
                                        + re.escape(str(other)))

    def test_verify_load_faults_a_stack_record_key_it_cannot_trust(self):
        """**The record's SPU key decides whether a read is bound, so it is
        trusted only where admin's rule holds it closed**; an open key would
        let another principal turn every binding off. Perturbation: drop
        the held_closed check on the record's spu-binary and the runtime
        rows read recorded."""
        state = self.tuple_load(INSTALL_TUPLE_LINES)
        (self.stack / "spu-binary").chmod(0o666)
        try:
            result = self.run_script("verify-load.sh", "m1", "--keep")
        finally:
            self.end_stand_in(state)
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertRegex(result.stdout, r"bind +runtime cudart .*\[FAULT: .*the stack record's spu-binary is not "
                                        r"held closed by root")
        self.assertRegex(result.stdout, r"live +libcuda +[0-9a-f]{64} .*\[derivable:")

    def test_verify_load_prints_the_load_event_through_its_stack_map(self):
        """**The load event's line ends at a member boundary, after the stack
        map**, and names the members it leaves to the sink. The cut at 600
        characters fell inside the declaration digest on the live load of
        2026-11-09 and never reaches the stack once a lineage stands, while
        verify-lifecycle reads `stack` from this line. The payload below has
        the live event's members in the live order. Perturbation: put back
        `json.dumps(keep)[:600]` and the line neither parses nor holds the
        stack."""
        state = self.verify_fixture()
        stack = {name: "1" * 64 for name in ("weaver-gate", "weaver-spu", "weaver-state",
                                             "weaver-trace-relay", "worker")}
        declaration = hashlib.sha256((self.decl / "agent.toml").read_bytes()).hexdigest()
        payload = {"residual_readout": False, "surprisal": True, "state_member": True,
                   "state_store": {"engine": "sqlite"}, "declaration": declaration,
                   "composer": {"binary": "worker"},
                   "tee": {"all_kinds": True, "keys": [{"kind": "message.user", "paths": ["content"]}]},
                   "lineage": {"save_point": "5" * 64, "run": "r" * 40, "sequence": "7", "turn": "2",
                               "named_at_restore": True},
                   "stack": stack,
                   "boundary": "/fixture/cgroup/weaver-m1", "cause": {"uid": 0}, "library_path": "/fixture/lib"}
        event = {"session": "m1-001", "run": "r" * 40, "sequence": "0", "kind": "load",
                 "subsystem": "harness", "payload": payload}
        stub = self.root / "installed" / "bin" / "weaver-admin"
        stub.write_text(stub.read_text().replace(
            "echo '{\"kind\":\"load\"}'", "echo '" + json.dumps(event) + "'"))
        try:
            result = self.run_script("verify-load.sh", "m1")
        finally:
            self.end_stand_in(state)
        self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
        lines = result.stdout.splitlines()
        at = [i for i, line in enumerate(lines) if line.startswith("   load event: ")]
        self.assertEqual(len(at), 1, result.stdout)
        printed = json.loads(lines[at[0]].removeprefix("   load event: "))
        through = list(payload)[:list(payload).index("stack") + 1]
        self.assertEqual(printed, dict(event, payload={k: payload[k] for k in through}))
        self.assertGreater(len(lines[at[0]]), 600)
        self.assertEqual(lines[at[0] + 1],
                         "   load event cut after its stack map; boundary, cause, library_path are in the sink")

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
        # that group, cannot read it. The operator joins the trace group, the
        # state group and the access group, and no access entry is set or
        # probed. The box's two connector accounts are laid where absent, each
        # a system account with no login shell, no home and its own group, and
        # no per-agent connector account is made. Perturbations: restore setgid (2710 or 2750), group the trace
        # to the member, or drop the trace group from the operator, and this
        # fails.
        self.env["ALLOW_APPLY_CHECKS"] = "1"
        result = self.create("--apply")
        self.assertEqual(result.returncode, 0, result.stderr)
        calls = self.calls()
        territory = str(self.root / "agents" / "m1" / "weaver-m1")
        self.assertIn(["sudo", "install", "-d", "-o", "root", "-g", "weaver-m1-state", "-m", "00710", territory], calls)
        # **The agent's folder holds the territory and the home side by side**
        # (#578): root's 0755, made before the account whose home stands in
        # it, and the home the agent's 2750. Perturbations: make the home
        # under /home again, or the folder after the useradd, and this fails.
        folder = str(self.root / "agents" / "m1")
        make_folder = ["sudo", "install", "-d", "-o", "root", "-g", "root", "-m", "00755", folder]
        add_agent = ["sudo", "useradd", "--system", "--shell", "/usr/sbin/nologin", "--create-home",
                     "--home-dir", folder + "/home", "--user-group", "weaver-m1"]
        self.assertIn(make_folder, calls)
        self.assertIn(add_agent, calls)
        self.assertLess(calls.index(make_folder), calls.index(add_agent))
        self.assertIn(["sudo", "chmod", "2750", folder + "/home"], calls)
        self.assertLess(calls.index(make_folder), calls.index(
            ["sudo", "install", "-d", "-o", "root", "-g", "weaver-m1-state", "-m", "00710", territory]))
        self.assertIn(f"folder          {folder}        root:root 0755", result.stdout)
        self.assertIn(f"home            {folder}/home        weaver-m1 2750", result.stdout)
        self.assertIn(["sudo", "install", "-o", "root", "-g", "weaver-m1-trace", "-m", "0640", "/dev/null",
                       territory + "/trace.ndjson"], calls)
        self.assertIn(["sudo", "install", "-d", "-o", "weaver-m1-state", "-g", "weaver-m1-state", "-m", "0700",
                       territory + "/state"], calls)
        self.assertIn(["sudo", "groupadd", "--system", "weaver-m1-trace"], calls)
        self.assertIn(["sudo", "groupadd", "--system", "weaver-m1-admin"], calls)
        self.assertIn(["sudo", "useradd", "--system", "--shell", "/usr/sbin/nologin", "--no-create-home",
                       "--no-user-group", "--gid", "weaver-m1-trace", "weaver-m1-relay"], calls)
        for box_account in ("weaver-gatecon", "weaver-admincon"):
            self.assertIn(["sudo", "useradd", "--system", "--shell", "/usr/sbin/nologin", "--no-create-home",
                           "--user-group", box_account], calls)
        self.assertFalse([c for c in calls if c[:2] == ["sudo", "useradd"] and c[-1] == "weaver-m1-admincon"], calls)
        self.assertIn(["sudo", "usermod", "-aG", "weaver-m1-trace,weaver-m1-state,weaver-m1-admin", "fixture-no-home"], calls)
        self.assertIn(look_as("weaver-m1-state", "-r", territory + "/trace.ndjson"), calls)
        # The agent's own uid is asked to pass the territory and must not
        # (the #94 survey's S1), and the member to read the declaration.
        self.assertIn(look_as("weaver-m1", "-x", territory), calls)
        self.assertIn(look_as("weaver-m1-state", "-r", territory + "/agent.toml"), calls)
        self.assertFalse([c for c in calls if "setfacl" in c or c[0] == "mktemp"], calls)

    def test_a_trace_the_member_can_read_refuses_before_admission(self):
        # The probe is what holds the boundary on the box: a member that can read
        # the trace refuses before the root is moved into place, for either
        # engine. Perturbation: drop the probe, and the agent is admitted.
        for engine in ("sqlite",):
            with self.subTest(engine=engine):
                self.log.unlink(missing_ok=True)
                shutil.rmtree(self.config / ".m1.partial", ignore_errors=True)
                shutil.rmtree(self.folder, ignore_errors=True)
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
                self.assertIn('as_account "weaver-$AGENT-state" tail', line, (script, line))

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
        # pass the gate's bound of 49,041 octets refuses before dialing, naming
        # the size (Codex on #92, round 3), and an empty draft refuses too. A
        # draft that fits reaches the dial, which finds no gate; a draft whose
        # line is exactly the bound is the edge that fits. Perturbation: drop
        # the measure and the oversize draft reaches the dial; read the bound
        # exclusively and the bound-exact draft refuses.
        root = self.config / "m1"
        root.mkdir()
        (root / "territory").write_text(str(self.decl) + "\n")
        (root / "coordination-root").write_text(str(self.root / "nowhere") + "\n")
        self.decl.mkdir(parents=True)
        draft = self.decl / "system-prompt.md"
        turn = [sys.executable, str(self.repo / "deploy" / "turn.py"), "m1", "--system"]
        # {"role": "system", "text": "..."} takes 30 octets around the text.
        draft.write_text("x" * (49_041 - 30 + 1))
        result = subprocess.run(turn, env=self.env, text=True, capture_output=True, timeout=20)
        self.assertEqual(result.returncode, 1)
        self.assertIn("the seeding line is 49042 octets, past the gate's bound of 49041 octets",
                      result.stderr)
        self.assertNotIn("no gate at", result.stderr)
        draft.write_text("x" * (49_041 - 30))
        result = subprocess.run(turn, env=self.env, text=True, capture_output=True, timeout=20)
        self.assertEqual(result.returncode, 1)
        self.assertNotIn("past the gate's bound", result.stderr)
        self.assertIn("no gate at", result.stderr)
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
        # The six Rust members, and the shell's busybox and applet list (#29).
        self.assertEqual(result.stdout.count("NEW"), 8)
        for shell_file in ("busybox ", "busybox.applets "):
            self.assertIn("  " + shell_file, result.stdout)
        self.assertIn("plan only. rerun with --install", result.stdout)
        self.assert_unprivileged()
        # The lock's id of the engine's package, for the build's messages
        # (the library step).
        cargo_actions = [c[1] for c in self.calls() if c[0] == "cargo"]
        self.assertEqual(cargo_actions, ["metadata", "test", "build", "pkgid"])

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
        # useradd --user-group makes a group named for each of the agent and
        # the member, so a stray one is a collision found before anything is
        # made. Perturbation: check only the trace and access groups, and a
        # stray member group lets the plan run.
        for group in ("weaver-m1", "weaver-m1-state"):
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
        self.assertLess(calls[1], text.index('if [ ${#CHANGED[@]} -gt 0 ] || [ ${#LIB_CHANGED[@]} -gt 0 ]'))

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

    def test_stack_refuses_a_declaration_carrying_permission_mode(self):
        # #29 retired permission-mode, and the admin this installs refuses it
        # as an unknown field, so the plan refuses it by name before the
        # build, naming the agent and the remedy, the declaration unchanged.
        # The control: the same declaration without the key is not refused
        # for it. Perturbation: drop the refusal and the plan runs on to the
        # build.
        decl = self.existing_territory / "agent.toml"
        bare = self.KARL.replace(
            '[[spu-instruction.decoder.identity]]\nrole = "system"\n\n'
            '[[spu-instruction.decoder.identity.content]]\ntype = "text"\ntext = """\n'
            'You are Karl, a small local agent.\nAnswer plainly."""\n\n', "")
        self.assertIn('permission-mode = "ask"', bare)
        decl.write_text(bare)
        self.log.unlink(missing_ok=True)
        result = self.run_script("update-stack.sh")
        self.assertEqual(result.returncode, 1, result.stdout)
        self.assertIn(f"existing: its declaration {decl} carries permission-mode, retired by #29",
                      result.stderr)
        self.assertIn("Recreate existing with deploy/create-agent.sh after its take-down by "
                      "deploy/HowToDeployANewAgent.md section 7", result.stderr)
        self.assertEqual(decl.read_text(), bare)
        self.assertFalse(any(c[0] == "cargo" for c in self.calls()))
        decl.write_text(bare.replace('permission-mode = "ask"\n', ""))
        self.log.unlink(missing_ok=True)
        control = self.run_script("update-stack.sh")
        self.assertNotIn("permission-mode", control.stderr)

    def test_stack_refuses_the_retired_loop_file_before_build(self):
        decl = self.existing_territory / "agent.toml"
        bare = decl.read_text()
        for key, value in (("loop-file", '"/opt/loops/a.py"'),
                           ('"loop-file"', '""'),
                           ("loop-file", "false")):
            with self.subTest(key=key, value=value):
                source = f"{key} = {value}\n{bare}"
                decl.write_text(source)
                self.log.unlink(missing_ok=True)
                result = self.run_script("update-stack.sh")
                self.assertEqual(result.returncode, 1, result.stdout)
                self.assertIn("loop-file, retired by #180", result.stderr)
                self.assertIn("Recreate existing", result.stderr)
                self.assertEqual(decl.read_text(), source)
                self.assertFalse(any(c[0] in ("cargo", "sudo") for c in self.calls()))
        decl.write_text(bare)
        control = self.run_script("update-stack.sh")
        self.assertEqual(control.returncode, 0, control.stderr)

    def test_a_declaration_lacking_a_sampling_default_is_named_and_refuses_the_install(self):
        # The harness this installs reads six sampling names from every
        # serving declaration and refuses the load naming the first missing
        # one (weaver-harness-Spec section 6.1). The plan names each agent
        # whose declaration lacks any, with the keys it lacks and the values
        # that keep the build before's behaviour, and goes on. An --install
        # refuses before the build and before anything is replaced, the
        # declaration untouched. A declaration with no tunable-values table
        # names all six, and so does one naming serving outright. A
        # diagnostic declaration is passed, since its reader requires none.
        # The control: the six present, nothing named and the install not
        # refused for it. Perturbation: drop the --install refusal and the
        # install runs on to the build, and drop a name from SAMPLING_NAMES
        # and the plan stops naming it.
        decl = self.existing_territory / "agent.toml"
        full = decl.read_text()
        self.assertIn("top-p = 0.95\n", full)
        partial = full.replace("top-p = 0.95\n", "").replace("repetition-window = 64\n", "")
        bare = '[state-store]\nengine = "none"\n'
        six = "temperature, top-k, top-p, repetition-penalty, repetition-window, seed"

        def run(*args):
            # A plan runs the stand-in build, which leaves its target behind.
            shutil.rmtree(self.root / 'target with "quotes"', ignore_errors=True)
            self.log.unlink(missing_ok=True)
            return self.run_script("update-stack.sh", *args)

        for label, text, lacking in (("partial", partial, "top-p, repetition-window"),
                                     ("bare", bare, six),
                                     ("serving", 'binding-kind = "serving"\n' + bare, six)):
            with self.subTest(label):
                decl.write_text(text)
                plan = run()
                self.assertEqual(plan.returncode, 0, plan.stderr)
                self.assertIn("== agents whose declaration lacks a sampling default\n", plan.stdout)
                self.assertIn(f"  existing: its declaration {decl} lacks tunable-values {lacking}\n",
                              plan.stdout)
                self.assertIn("temperature = 0.7, top-k = 40, top-p = 0.95, repetition-penalty = 1.1, "
                              "repetition-window = 64", plan.stdout)
                install = run("--install")
                self.assertEqual(install.returncode, 1, install.stdout)
                self.assertIn("an --install refuses while an agent's declaration lacks a sampling "
                              "default, named above", install.stderr)
                self.assertIn(f"  existing: its declaration {decl} lacks tunable-values {lacking}\n",
                              install.stdout)
                self.assertFalse(any(c[0] in ("cargo", "sudo") for c in self.calls()))
                self.assertEqual(decl.read_text(), text)
        decl.write_text('binding-kind = "diagnostic"\n' + bare)
        diagnostic = run()
        self.assertNotIn("sampling default", diagnostic.stdout + diagnostic.stderr)
        decl.write_text(full)
        control = run()
        self.assertEqual(control.returncode, 0, control.stderr)
        self.assertNotIn("sampling default", control.stdout + control.stderr)
        install = run("--install")
        self.assertNotIn("sampling default", install.stdout + install.stderr)

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
        decl.write_text(f'[state-store]\nengine = "none"\n\n[trace-sink]\nkind = "file"\npath = "{trace}"\ncreate = true\n' + SAMPLING)
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

    def test_the_tree_step_names_a_detached_checkout_by_its_commit(self):
        """**A detached checkout is named by its commit**, never as an empty
        branch: a pinned install runs from a detached worktree, and the live
        plan of 2026-11-09 printed "on branch , not fast-forwarding". A
        branch other than main is still named. Perturbation: drop the
        detached arm and the first run prints "on branch , not
        fast-forwarding"."""
        for branch, said in (("", "detached at abcdef0, not fast-forwarding"),
                             ("fixture-branch", "on branch fixture-branch, not fast-forwarding")):
            with self.subTest(branch=branch):
                self.env["FIXTURE_BRANCH"] = branch
                shutil.rmtree(self.root / 'target with "quotes"', ignore_errors=True)
                self.log.unlink(missing_ok=True)
                result = self.run_script("update-stack.sh")
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                tree = result.stdout.split("\n== tree\n", 1)[1].split("\n\n", 1)[0].splitlines()
                self.assertEqual(tree, [f"  {said}", "  abcdef0 -> abcdef0"])
        self.env.pop("FIXTURE_BRANCH")

    def test_the_agents_line_lines_up_with_the_box_facts(self):
        """**The agents line sits in the box facts' column**: the list was
        built with a leading space, so the live plan of 2026-11-09 printed
        "agents         granite1", one column out. Perturbation: build the
        list as "$AGENTS $agent" again and the line carries two spaces more
        than its label's padding."""
        result = self.run_script("update-stack.sh")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("\n  agents        existing\n", result.stdout)

    def test_the_will_install_line_names_every_file_the_plan_lists(self):
        """**The box section's install line names what the install
        installs**: the members, the shell's two files and the engine
        libraries, as REDEPLOY says update-stack plans and installs them. The
        live plan of 2026-11-09 left the shell out of the line while its plan
        section listed busybox and busybox.applets. bootstrap-stack.sh, which
        installs the same three, has no plan fixture, so its line is read
        from its text. Perturbation: drop the shell from either script's
        line, or from REDEPLOY's sentence, and this fails."""
        result = self.run_script("update-stack.sh")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        line, = [l for l in result.stdout.splitlines() if l.startswith("  will install  ")]
        plan = result.stdout.split("\n== plan\n", 1)[1].split("\n\n", 1)[0].splitlines()
        named = set(re.findall(r"[\w.+-]+", line.split("engine libraries", 1)[0]))
        globs = re.search(r"engine libraries \(([^)]*)\)", line).group(1).split()
        listed = [row.split()[0] for row in plan if re.match(r"  \w", row)]
        self.assertIn("busybox.applets", listed, plan)
        self.assertTrue({"busybox", "busybox.applets"} <= named, line)
        self.assertTrue(globs, line)
        for name in listed:
            if name.startswith("lib"):
                self.assertTrue(any(fnmatch.fnmatch(name, g) for g in globs), name)
            else:
                self.assertIn(name, named)
        bootstrap = (self.repo / "deploy" / "bootstrap-stack.sh").read_text()
        planned, = [l for l in bootstrap.splitlines() if l.lstrip().startswith('plan "will install')]
        for variable in ("$MEMBERS", "$SHELL_FILES", "$LIB_GLOBS"):
            self.assertIn(variable, planned)
        self.assertIn('SHELL_FILES="busybox busybox.applets"', bootstrap)
        redeploy = re.sub(r"\s+", " ", (DEPLOY / "REDEPLOY.md").read_text())
        installs, = [s for s in redeploy.split(". ") if "plans and installs the members" in s]
        self.assertIn("the shell", installs)

    def test_the_install_window_prints_once_under_install(self):
        """**The install window is said once**: --install runs the plan
        first, so its preview says what the window will do before the
        credential prompt (#99 area 2 review, R2), and the act reports under
        a header of its own only where an agent stands to hold and unload.
        The live install of 2026-11-09, on a box with no agent, printed a
        second install window header over the same "no agents" body.
        Perturbation: put the act's old header and its "no agents" line back
        and two headers and two bodies stand; print the preview in a plan
        only and --install says nothing before it acts."""
        plan = self.run_script("update-stack.sh")
        self.assertEqual(plan.returncode, 0, plan.stdout + plan.stderr)
        self.assertEqual([l for l in plan.stdout.splitlines() if l.startswith("== the install window")],
                         ["== the install window"])
        self.assertIn("plan only: no rule is moved and no agent unloaded", plan.stdout)
        shutil.rmtree(self.root / 'target with "quotes"', ignore_errors=True)
        self.log.unlink(missing_ok=True)
        result = self.library_install()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual([l for l in result.stdout.splitlines() if l.startswith("== the install window")],
                         ["== the install window"])
        self.assertEqual(result.stdout.count("no agents:"), 1, result.stdout)
        self.assertIn("  no agents: --install holds no connector rule and unloads none\n", result.stdout)
        self.assertNotIn("== holding the connector rules", result.stdout)
        self.assertNotIn("plan only:", result.stdout)
        self.assertLess(result.stdout.index("== the install window"), result.stdout.index("== install"))

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
        (old_root / "worker-binary").write_text(str(self.root / "installed" / "worker"))
        old_dir = self.operator_home / ".weaveragent" / "old"
        old_dir.mkdir(parents=True)
        (old_dir / "agent.toml").write_text("[state-store]\nengine = \"none\"\n" + SAMPLING)
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
                   + shell_function(text, "unload_verified") + shell_function(text, "await_unload")
                   + shell_function(text, "answered_state")
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

    def test_a_rollback_removes_the_shell_key_its_run_wrote(self):
        """**A rolled-back run leaves no stack record naming a busybox the
        rollback took away** (#29): restore() removes `shell-binary` where
        this run wrote it, beside the busybox it added, and leaves a key the
        run found standing. Perturbation: drop the removal and the first case
        keeps the key."""
        text = (DEPLOY / "update-stack.sh").read_text()
        stack = self.root / "rollback-stack"
        bin_dir = self.root / "rollback-bin"
        backup = self.root / "rollback-backup"
        for d in (stack, bin_dir, backup):
            d.mkdir()
        program = ('sudo() { "$@"; }\n' + shell_function(text, "restore")
                   + 'RESTORED=0; LOADED_AGENT=""; INSTALL_DONE=1; ADDED=(busybox); CHANGED=()\n'
                   + 'restore\n')
        env = {k: v for k, v in os.environ.items() if k != "BASH_ENV"}
        env.update(STACK=str(stack), BIN_DIR=str(bin_dir), BACKUP=str(backup))
        for written, stays in ((1, False), (0, True)):
            with self.subTest(written=written):
                (stack / "shell-binary").write_text(str(bin_dir / "busybox") + "\n")
                (bin_dir / "busybox").write_text("x")
                ran = subprocess.run(["bash", "-c", f"SHELL_KEY_WRITTEN={written}\n" + program],
                                     env=env, text=True, capture_output=True, timeout=20)
                self.assertFalse((bin_dir / "busybox").exists(), ran.stderr)
                self.assertEqual((stack / "shell-binary").exists(), stays, ran.stderr)
        # **The repair path** (Codex on #177): binaries unchanged and nothing
        # installed, the key absent before this run and written by it, then a
        # validation fails. The rollback installs nothing back and still
        # removes the key. Perturbation: put the removal back under the
        # binaries' guard and the key stays.
        with self.subTest(path="repair"):
            (stack / "shell-binary").write_text(str(bin_dir / "busybox") + "\n")
            (bin_dir / "busybox").write_text("x")
            repair = ('sudo() { "$@"; }\n' + shell_function(text, "restore")
                      + 'RESTORED=0; LOADED_AGENT=""; INSTALL_DONE=0; ADDED=(); CHANGED=()\n'
                      + 'SHELL_KEY_WRITTEN=1\nrestore\n')
            ran = subprocess.run(["bash", "-c", repair], env={**env, "BACKUP": ""}, text=True,
                                 capture_output=True, timeout=20)
            self.assertFalse((stack / "shell-binary").exists(), ran.stderr)
            self.assertTrue((bin_dir / "busybox").exists(), "the unchanged busybox stays")

    def test_an_install_unload_past_its_bound_is_joined_by_a_force(self):
        """**The install's unload is bounded, the force its fallback** (the
        Planner's ruling of 2026-10-12 on #126): an unload that answers
        inside UNLOAD_TIMEOUT is read as before and no force is sent; one
        still running past it is joined by `force-unload`, whose answer is
        read, and the script prints which ended the run. Perturbations: wait
        the unload out with no bound, and the slow case is ended by the
        unload; force at once, and the quick case is ended by the force."""
        text = (DEPLOY / "update-stack.sh").read_text()
        admin = self.root / "bin" / "weaver-admin"
        admin.parent.mkdir(parents=True, exist_ok=True)
        calls = self.root / "admin-calls"
        program = ('sudo() { [ "$1" = -n ] && shift; shift; "$@"; }\n'
                   + shell_function(text, "unload_verified") + shell_function(text, "await_unload")
                   + shell_function(text, "answered_state")
                   + 'unload_verified m1')
        env = {k: v for k, v in os.environ.items() if k != "BASH_ENV"}
        env.update(BIN_DIR=str(admin.parent), ADMIN_BASE=str(self.config), UNLOAD_TIMEOUT="1")
        for case, delay, ended_by in (("quick", 0, "unload"), ("slow", 4, "force-unload")):
            with self.subTest(case=case):
                calls.unlink(missing_ok=True)
                admin.write_text(
                    "#!/bin/sh\n"
                    f"echo \"$1\" >> {calls}\n"
                    f"[ \"$1\" = unload ] && sleep {delay}\n"
                    "echo '{\"kind\":\"state\",\"state\":\"unloaded\"}'\n")
                admin.chmod(0o755)
                ran = subprocess.run(["bash", "-c", program], env=env, text=True,
                                     capture_output=True, timeout=30)
                self.assertEqual(ran.returncode, 0, ran.stderr)
                self.assertIn(f"  m1: unloaded by {ended_by}", ran.stdout)
                verbs = calls.read_text().split()
                if ended_by == "unload":
                    self.assertEqual(verbs, ["unload"])
                else:
                    self.assertEqual(sorted(verbs), ["force-unload", "unload"])
                    self.assertIn("m1: the unload passed 1s; force-unload joins it", ran.stderr)

    def test_an_exit_mid_unload_concludes_the_unload_before_it_restores(self):
        """**The exit trap concludes the unload in flight first** (Codex on
        #126, round 3): an INT arriving while an agent's unload runs in the
        background exits through `on_exit`, which waits that unload out, or
        past its bound has `force-unload` join it, and only then restores,
        so nothing is put back under an agent still leaving. Perturbation:
        drop `conclude_pending_unload` from `on_exit`, and the restore runs
        before the unload ends."""
        text = (DEPLOY / "update-stack.sh").read_text()
        admin = self.root / "bin" / "weaver-admin"
        admin.parent.mkdir(parents=True, exist_ok=True)
        calls = self.root / "trap-calls"
        program = ('sudo() { [ "$1" = -n ] && shift; shift; "$@"; }\n'
                   + shell_function(text, "unload_verified") + shell_function(text, "await_unload")
                   + shell_function(text, "conclude_pending_unload")
                   + shell_function(text, "answered_state") + shell_function(text, "on_exit")
                   + f'restore() {{ echo restore >> {calls}; }}\n'
                   + 'restore_rules() { :; }\n'
                   + 'COMPLETED=0 INSTALL_DONE=1 HELD_RULES=()\n'
                   + "trap on_exit EXIT\ntrap 'exit 130' INT\n"
                   + '( sleep 0.5; kill -INT $$ ) &\n'
                   + 'unload_verified m1\n')
        env = {k: v for k, v in os.environ.items() if k != "BASH_ENV"}
        env.update(BIN_DIR=str(admin.parent), ADMIN_BASE=str(self.config))
        for case, bound, last in (("waited out", "10", "unload done"),
                                  ("joined past the bound", "2", "force-unload")):
            with self.subTest(case=case):
                calls.unlink(missing_ok=True)
                admin.write_text(
                    "#!/bin/sh\n"
                    f"[ \"$1\" = unload ] && {{ sleep 4; echo 'unload done' >> {calls}; }}\n"
                    f"[ \"$1\" = force-unload ] && echo force-unload >> {calls}\n"
                    "echo '{\"kind\":\"state\",\"state\":\"unloaded\"}'\n")
                admin.chmod(0o755)
                ran = subprocess.run(["bash", "-c", program], env={**env, "UNLOAD_TIMEOUT": bound},
                                     text=True, capture_output=True, timeout=30)
                self.assertEqual(ran.returncode, 130, ran.stderr)
                order = calls.read_text().splitlines()
                self.assertEqual(order[-1], "restore", order)
                self.assertIn(last, order[:-1], order)
                self.assertIn("m1: the run is ending with its unload in flight", ran.stderr)

    def test_a_signal_at_either_edge_of_the_unload_in_flight_is_concluded_once(self):
        """**The record and the unload stand together** (Codex on #126, round
        5). A signal raised inside the window before the runner is recorded
        (here, at the `mktemp`) is held and raised again once the record is
        set, so the trap waits that unload out before it restores; and a
        signal landing as the finished unload's directory is removed finds
        the record already cleared, so no `force-unload` follows an unload
        already answered. Perturbations: drop the held window, and the
        restore runs with no unload concluded; clear the record after the
        `rm` again, and the trap waits out the bound and forces."""
        text = (DEPLOY / "update-stack.sh").read_text()
        admin = self.root / "bin" / "weaver-admin"
        admin.parent.mkdir(parents=True, exist_ok=True)
        calls = self.root / "edge-calls"
        admin.write_text(
            "#!/bin/sh\n"
            f"[ \"$1\" = unload ] && {{ sleep 1; echo 'unload done' >> {calls}; }}\n"
            f"[ \"$1\" = force-unload ] && echo force-unload >> {calls}\n"
            "echo '{\"kind\":\"state\",\"state\":\"unloaded\"}'\n")
        admin.chmod(0o755)
        env = {k: v for k, v in os.environ.items() if k != "BASH_ENV"}
        env.update(BIN_DIR=str(admin.parent), ADMIN_BASE=str(self.config), UNLOAD_TIMEOUT="3")
        for edge, injected, expected in (
                ("the fork", 'mktemp() { command mktemp "$@"; kill -INT $$; }\n', ["unload done", "restore"]),
                ("the removal", 'rm() { command rm "$@"; kill -INT $$; }\n', ["unload done", "restore"])):
            with self.subTest(edge=edge):
                calls.unlink(missing_ok=True)
                program = ('sudo() { [ "$1" = -n ] && shift; shift; "$@"; }\n'
                           + shell_function(text, "unload_verified") + shell_function(text, "await_unload")
                           + shell_function(text, "conclude_pending_unload")
                           + shell_function(text, "answered_state") + shell_function(text, "on_exit")
                           + f'restore() {{ echo restore >> {calls}; }}\n'
                           + 'restore_rules() { :; }\n'
                           + 'COMPLETED=0 INSTALL_DONE=1 HELD_RULES=()\n'
                           + "trap on_exit EXIT\ntrap 'exit 130' INT\n"
                           + injected
                           + 'unload_verified m1\n')
                ran = subprocess.run(["bash", "-c", program], env=env,
                                     text=True, capture_output=True, timeout=30)
                self.assertEqual(ran.returncode, 130, ran.stderr)
                self.assertEqual(calls.read_text().splitlines(), expected, ran.stderr)

    def test_a_second_signal_during_the_conclusion_is_held_until_the_restore(self):
        """**A signal during the exit trap's conclusion is held** (Codex on
        #126, round 6): after a first INT has entered `on_exit`, a second
        arriving while it waits the unload out prints one line and the
        conclusion goes on to the restore, the exit code the first signal's.
        Here the first INT is raised at the `mktemp`, and every `sleep` of the
        wait raises another and then ends as a Ctrl-C ends it, with 130,
        under the script's own `set -euo pipefail`. Perturbations: drop the
        holding trap, or the trap's `set +e`, and the trap ends with no
        restore."""
        text = (DEPLOY / "update-stack.sh").read_text()
        admin = self.root / "bin" / "weaver-admin"
        admin.parent.mkdir(parents=True, exist_ok=True)
        calls = self.root / "second-calls"
        admin.write_text(
            "#!/bin/sh\n"
            f"[ \"$1\" = unload ] && {{ sleep 2; echo 'unload done' >> {calls}; }}\n"
            f"[ \"$1\" = force-unload ] && echo force-unload >> {calls}\n"
            "echo '{\"kind\":\"state\",\"state\":\"unloaded\"}'\n")
        admin.chmod(0o755)
        program = ('set -euo pipefail\n'
                   + 'sudo() { [ "$1" = -n ] && shift; shift; "$@"; }\n'
                   + shell_function(text, "unload_verified") + shell_function(text, "await_unload")
                   + shell_function(text, "conclude_pending_unload")
                   + shell_function(text, "answered_state") + shell_function(text, "on_exit")
                   + f'restore() {{ echo restore >> {calls}; }}\n'
                   + 'restore_rules() { :; }\n'
                   + 'COMPLETED=0 INSTALL_DONE=1 HELD_RULES=()\n'
                   + "trap on_exit EXIT\ntrap 'exit 130' INT\n"
                   + 'mktemp() { command mktemp "$@"; kill -INT $$; }\n'
                   + 'sleep() { kill -INT $$; command sleep "$@"; return 130; }\n'
                   + 'unload_verified m1\n')
        env = {k: v for k, v in os.environ.items() if k != "BASH_ENV"}
        env.update(BIN_DIR=str(admin.parent), ADMIN_BASE=str(self.config), UNLOAD_TIMEOUT="10")
        ran = subprocess.run(["bash", "-c", program], env=env,
                             text=True, capture_output=True, timeout=30)
        self.assertEqual(ran.returncode, 130, ran.stderr)
        self.assertEqual(calls.read_text().splitlines(), ["unload done", "restore"], ran.stderr)
        self.assertIn("the run is concluding; a signal now is held until the restore is done", ran.stderr)

    def unload_program(self, admin_text, prelude="", call="unload_verified m1"):
        """`unload_verified` and `await_unload` as the script defines them,
        with a stand-in `weaver-admin` and a `sudo` that records its
        arguments, for the once-over's tests of the unload in flight."""
        text = (DEPLOY / "update-stack.sh").read_text()
        admin = self.root / "bin" / "weaver-admin"
        admin.parent.mkdir(parents=True, exist_ok=True)
        admin.write_text("#!/bin/sh\n" + admin_text
                         + "echo '{\"kind\":\"state\",\"state\":\"unloaded\"}'\n")
        admin.chmod(0o755)
        sudo_calls = self.root / "sudo-calls"
        sudo_calls.unlink(missing_ok=True)
        program = ('set -euo pipefail\n'
                   + f'sudo() {{ echo "$*" >> {sudo_calls}; [ "$1" = -n ] && shift; '
                   + '[ "$1" = -v ] && return 0; shift; "$@"; }\n'
                   + shell_function(text, "unload_verified") + shell_function(text, "await_unload")
                   + shell_function(text, "answered_state")
                   + prelude + call + '\n')
        env = {k: v for k, v in os.environ.items() if k != "BASH_ENV"}
        env.update(BIN_DIR=str(admin.parent), ADMIN_BASE=str(self.config))
        return program, env, sudo_calls

    def test_an_unload_whose_runner_dies_unanswered_is_joined_at_once(self):
        """**A runner gone without `done` is joined at once** (the once-over
        of 2026-10-12): a TERM that kills the background unload before it
        writes `done` has `force-unload` join at once, well inside the
        bound. Perturbation: drop the `kill -0` from the wait, and the run
        sits out the bound."""
        calls = self.root / "dead-calls"
        program, env, _ = self.unload_program(
            f'[ "$1" = unload ] && {{ kill -TERM $PPID; sleep 1; exit 0; }}\n'
            f'[ "$1" = force-unload ] && echo force-unload >> {calls}\n')
        started = time.monotonic()
        ran = subprocess.run(["bash", "-c", program], env={**env, "UNLOAD_TIMEOUT": "8"},
                             text=True, capture_output=True, timeout=30)
        elapsed = time.monotonic() - started
        self.assertEqual(ran.returncode, 0, ran.stderr)
        self.assertEqual(calls.read_text().splitlines(), ["force-unload"])
        self.assertIn("m1: the unload ended without answering; force-unload joins it", ran.stderr)
        self.assertLess(elapsed, 5, ran.stderr)

    def test_the_unload_wait_refreshes_sudos_timestamp(self):
        """**sudo's timestamp is refreshed through the wait** (the once-over
        of 2026-10-12), so the joining force's `sudo -n` is not refused for a
        password the bound outlived. Perturbation: drop the refresh, and no
        `-n -v` is asked."""
        program, env, sudo_calls = self.unload_program('[ "$1" = unload ] && sleep 3\n')
        ran = subprocess.run(["bash", "-c", program], env={**env, "SUDO_REFRESH": "1"},
                             text=True, capture_output=True, timeout=30)
        self.assertEqual(ran.returncode, 0, ran.stderr)
        self.assertIn("-n -v", sudo_calls.read_text().splitlines())

    def test_an_unload_with_no_directory_is_never_asked_and_fails(self):
        """**A failed `mktemp` fails the unload at once** (the once-over of
        2026-10-12) rather than waiting out the bound and forcing an agent
        whose graceful unload was never asked. Perturbation: drop the check,
        and the force is asked."""
        program, env, sudo_calls = self.unload_program(
            "", prelude="mktemp() { return 1; }\n",
            call="unload_verified m1 && echo answered || echo failed")
        ran = subprocess.run(["bash", "-c", program], env={**env, "UNLOAD_TIMEOUT": "3"},
                             text=True, capture_output=True, timeout=30)
        self.assertEqual(ran.stdout.strip(), "failed", ran.stderr)
        self.assertIn("m1: no directory for the unload could be made, so it was never asked", ran.stderr)
        self.assertFalse(sudo_calls.exists(), "admin was asked")

    def test_update_stack_reads_its_unload_timeout_and_refuses_what_it_does_not_know(self):
        # --unload-timeout takes whole seconds above zero, and an argument the
        # script does not know refuses rather than being ignored, so a
        # misspelt bound never runs the default unseen. Perturbation: accept
        # any value, and 0 passes.
        for args in (["--unload-timeout", "0"], ["--unload-timeout", "x"],
                     ["--unload-timeout"], ["--unload-timout", "60"]):
            with self.subTest(args=args):
                result = self.run_script("update-stack.sh", *args)
                self.assertEqual(result.returncode, 2, result.stdout)
                self.assertIn("update-stack.sh:", result.stderr)
        text = (DEPLOY / "update-stack.sh").read_text()
        self.assertIn("UNLOAD_TIMEOUT=300", text)

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
                decl.write_text(f'[state-store]\nengine = "none"\n\n[trace-sink]\nkind = "file"\npath = "{sink}"\n' + SAMPLING)
                self.log.unlink(missing_ok=True)
                result = self.run_script("update-stack.sh")
                self.assertEqual(result.returncode, 1, result.stdout)
                self.assertIn(f"its trace sink {sink} does not stand in its canonical territory {territory.resolve()}",
                              result.stderr)
                self.assertIn(f"with sudoedit {decl}", result.stderr)
                self.assertFalse([c for c in self.calls() if c[:2] == ["cargo", "build"]], self.calls())
        decl.write_text('[state-store]\nengine = "none"\n' + SAMPLING)
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
            f'[state-store]\nengine = "none"\n\n[trace-sink]\nkind = "file"\npath = "{sink}"\n' + SAMPLING)
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
            f'[state-store]\nengine = "none"\n\n[trace-sink]\nkind = "pipe"\npath = "{fifo}"\n' + SAMPLING)
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
        # The window is said once, by the plan --install runs first
        # (test_the_install_window_prints_once_under_install).
        for said in ("no agents: --install holds no connector rule and unloads none", "no agents to reconcile",
                     "no agents to verify", "the box is at"):
            self.assertIn(said, result.stdout)
        self.assertTrue([c for c in self.calls() if c[:2] == ["sudo", "install"]], self.calls())
        self.assertFalse((self.root / "admin-calls").exists(), "no admin verb is asked")

    def test_stack_installs_the_shell_and_records_it(self):
        """**The update builds the shell, installs it beside the members, and
        names it in the stack record** (#29, weaver-admin-Spec section 9):
        build-busybox.sh is called with the build directory, busybox goes in
        at 0755 and its applet list at 0644, and the record gains
        shell-binary. Perturbations: drop the build call and no busybox is
        installed. Install the list at 0755 and the mode assertion fails."""
        shutil.rmtree(self.config / "existing")
        (self.root / "installed").mkdir(exist_ok=True)
        admin = self.root / "fixture-admin"
        admin.write_text("#!/bin/sh\nexit 2\n")
        admin.chmod(0o755)
        shutil.rmtree(self.root / 'target with "quotes"', ignore_errors=True)
        self.env.update(ALLOW_APPLY_CHECKS="1", FIXTURE_ADMIN=str(admin))
        result = self.run_script("update-stack.sh", "--install")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        calls = self.calls()
        self.assertTrue([c for c in calls if c[0] == "build-busybox.sh"], calls)
        installs = [c for c in calls if c[:2] == ["sudo", "install"]]
        busybox = [c for c in installs if c[-1].endswith("/busybox")]
        applets = [c for c in installs if c[-1].endswith("/busybox.applets")]
        self.assertTrue(busybox and "0755" in busybox[0], installs)
        self.assertTrue(applets and "0644" in applets[0], installs)
        self.assertIn("the stack record gains shell-binary", result.stdout)
        self.assertTrue([c for c in calls if c[:2] == ["sudo", "tee"] and c[-1].endswith("/shell-binary")],
                        calls)

    def engine_digest(self, stem, build):
        return hashlib.sha256(f"fixture {stem} {build}".encode()).hexdigest()[:12]

    def test_the_plan_lists_each_engine_library_from_the_build_cargo_names(self):
        """**The engine's libraries are planned as the members are** (the
        operator's ruling of 2026-11-09): each versioned file of the out/lib
        cargo reports for the engine's package is CHANGED, unchanged or NEW
        by its digest against the library directory, with the 12-character
        prefixes; the links are counted and not listed; the loader's entry
        that stands is left. A newer engine directory beside it, as a shared
        target holds one per pin, is not read. Perturbations: select the
        out/lib by `ls -td` and the decoy's digest is planned; drop the link
        comparison and no link is counted."""
        self.env.update(ENGINE_BUILD="b", DECOY_ENGINE="1")
        (self.lib_dir / "libggml.so.0").unlink()
        (self.lib_dir / "libggml.so.0").symlink_to("libggml.so.0.9.0")
        result = self.run_script("update-stack.sh")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        out = self.root / 'target with "quotes"' / "release" / "build" / "llama-cpp-sys-2-fixture" / "out" / "lib"
        self.assertIn(f"  engine out    {out}\n", result.stdout)
        self.assertNotIn("decoy", result.stdout)
        self.assertIn(f"  {'libllama.so.0.0.1':<22} CHANGED  {self.engine_digest('libllama', 'a')} -> "
                      f"{self.engine_digest('libllama', 'b')}\n", result.stdout)
        self.assertIn(f"  {'libggml.so.0.9.1':<22} CHANGED  {self.engine_digest('libggml', 'a')} -> "
                      f"{self.engine_digest('libggml', 'b')}\n", result.stdout)
        for link in ("libllama.so ", "libllama.so.0 ", "libggml.so ", "libggml.so.0 "):
            self.assertNotIn("  " + link, result.stdout)
        self.assertIn(f"  {'(library links)':<22} 1 to carry with cp -a\n", result.stdout)
        self.assertIn("/etc/ld.so.conf.d/weaver.conf stands; left as it is", result.stdout)
        self.assertIn(f"  library dir   {self.lib_dir}\n", result.stdout)
        self.assert_unprivileged()
        # An unchanged engine plans unchanged and the box is current.
        self.install_libraries("a")
        installed = self.root / "installed"
        for member in ("worker", "weaver-admin", "weaver-trace-relay", "weaver-gate", "weaver-spu", "weaver-state"):
            installed.joinpath(member).write_text("fixture artifact " + member)
        installed.joinpath("busybox").write_text("fixture busybox")
        installed.joinpath("busybox.applets").write_text("ls\ncat\n")
        shutil.rmtree(self.root / 'target with "quotes"')
        self.env["ENGINE_BUILD"] = "a"
        result = self.run_script("update-stack.sh")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn(f"  {'libllama.so.0.0.1':<22} unchanged\n", result.stdout)
        self.assertNotIn("(library links)", result.stdout)
        self.assertIn("every deployed binary and engine library already matches the build", result.stdout)
        self.assertIn("the box is current at", result.stdout)

    def test_a_build_path_holding_a_backslash_plans_by_the_bare_digest(self):
        """**A digest is the file's, never its name's** (CodeRabbit on #559): GNU
        sha256sum escapes a file name holding a backslash and marks its
        digest field with a leading backslash, so a build or installed
        directory with one planned every equal member and library as
        CHANGED. Every digest is now taken from standard input.
        Perturbation: hash any of the four compared files by its path again
        and the box is not current."""
        self.install_libraries("a")
        installed = self.root / "installed"
        for member in ("worker", "weaver-admin", "weaver-trace-relay", "weaver-gate", "weaver-spu", "weaver-state"):
            installed.joinpath(member).write_text("fixture artifact " + member)
        installed.joinpath("busybox").write_text("fixture busybox")
        installed.joinpath("busybox.applets").write_text("ls\ncat\n")
        # The installed side as well: the stack record names a prefix whose
        # directory holds a backslash.
        odd = self.root / "installed \\ box"
        installed.rename(odd)
        for key in ("worker-binary", "spu-binary", "gate-binary", "library-path", "prefix"):
            (self.stack / key).write_text(self.stack_keys[key].replace(str(installed), str(odd)) + "\n")
        (self.config / "existing" / "worker-binary").write_text(str(odd / "worker"))
        self.env.update(ENGINE_BUILD="a", CARGO_TARGET_DIR=str(self.root / "target with \\ backslash"))
        result = self.run_script("update-stack.sh")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn(f"  {'worker':<22} unchanged\n", result.stdout)
        self.assertIn(f"  {'libllama.so.0.0.1':<22} unchanged\n", result.stdout)
        self.assertIn("every deployed binary and engine library already matches the build", result.stdout)

    def test_the_plan_refuses_where_the_library_step_cannot_be_named(self):
        """**What cannot be named refuses before anything is planned**: a
        build whose messages report no output directory for the engine's
        package, and a stack record whose library-path is missing, relative,
        or not a directory. Perturbation: fall back to the newest out/lib
        when no message names one and the first case plans."""
        self.env["NO_ENGINE_MESSAGE"] = "1"
        result = self.run_script("update-stack.sh")
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        target = self.root / 'target with "quotes"'
        self.assertIn(f"the build reported no single output directory under {target}/release/build for llama-cpp-sys-2",
                      result.stderr)
        self.assertNotIn("== plan", result.stdout)
        self.assertEqual(list(target.glob("weaver-deploy-messages-*")), [], "the messages file is removed")
        self.env.pop("NO_ENGINE_MESSAGE")
        for value, said in (("", "no absolute library-path"), ("relative/lib", "no absolute library-path"),
                            (str(self.root / "no-such-lib"), "does not stand as a directory")):
            with self.subTest(value=value):
                self.log.unlink(missing_ok=True)
                (self.stack / "library-path").write_text(value + "\n")
                result = self.run_script("update-stack.sh")
                self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
                self.assertIn(said, result.stderr)
                self.assertFalse(any(c[0] == "cargo" for c in self.calls()), self.calls())

    def engine_cache(self):
        """The configure cache the cargo double leaves in the engine's
        out_dir, resolved as `engine_out` resolves that directory."""
        out = self.root / 'target with "quotes"' / "release" / "build" / "llama-cpp-sys-2-fixture" / "out"
        return Path(os.path.realpath(out)) / "build" / "CMakeCache.txt"

    def bootstrap(self, *args):
        """bootstrap-stack.sh on a box that holds no stack: its record, admin
        base, prefix and agent directory named under the fixture and absent.
        Its plan stops in this fixture at the member check that follows the
        engine's output directory, because the cargo double's members are
        not executable."""
        self.env.update(WEAVER_STACK_RECORD=str(self.root / "new-stack"),
                        WEAVER_ADMIN_CONFIG=str(self.root / "new-admin"),
                        WEAVER_PREFIX=str(self.root / "new-prefix"),
                        WEAVER_AGENT_DIR=str(self.root / "new-agents"))
        return self.run_script("bootstrap-stack.sh", *args)

    def test_both_scripts_build_the_engine_with_its_openssl_lookup_off(self):
        """**Both stack scripts build the engine without HTTPS**
        (weaver-spu-Spec section 1.1, deploy/engine-libraries.sh): every
        cargo run either script makes carries
        CMAKE_DISABLE_FIND_PACKAGE_OpenSSL=ON, over an OFF in the operator's
        shell. The cargo double, as the fork's build script does, records
        the lookup off in the engine's configure cache only where the
        build's environment carried it, the cache line a live build wrote on
        2026-11-11, whose cmake then printed "OpenSSL not found, HTTPS
        support disabled". Bootstrap passes its engine check and stops at
        the member check after it (`bootstrap`). Perturbation: drop the
        export and both scripts refuse the engine they have just built."""
        self.env["CMAKE_DISABLE_FIND_PACKAGE_OpenSSL"] = "OFF"
        result = self.run_script("update-stack.sh")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("CMAKE_DISABLE_FIND_PACKAGE_OpenSSL:UNINITIALIZED=ON\n", self.engine_cache().read_text())
        self.assertIn("plan only. rerun with --install", result.stdout)
        shutil.rmtree(self.root / 'target with "quotes"')
        result = self.bootstrap()
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("CMAKE_DISABLE_FIND_PACKAGE_OpenSSL:UNINITIALIZED=ON\n", self.engine_cache().read_text())
        self.assertNotIn("configure cache", result.stderr)
        self.assertIn('release/worker was not built', result.stderr)

    def test_an_engine_configured_before_the_rule_refuses_before_anything_is_installed(self):
        """**An engine a build before the rule configured refuses** in both
        scripts, right after the build names its output directory and before
        any plan or install. Cargo reruns no build script when the variable
        changes, so a warm target hands back an engine whose configure cache
        still has the OpenSSL lookup on. The refusal names the cache, a fresh
        target first, and then the clean, which `cargo clean --dry-run` showed
        on 2026-11-11 to remove every build directory and fingerprint of the
        engine in the target. An --install refuses before any sudo call that
        places a file. Perturbation: drop either script's call and its stale engine
        plans (update-stack) or reaches the member check (bootstrap)."""
        self.env["STALE_ENGINE"] = "1"
        said = f"the engine's configure cache at {self.engine_cache()} does not turn its OpenSSL lookup off"
        clean = ("Build into a fresh CARGO_TARGET_DIR, or remove it with 'cargo clean --release -p "
                 "llama-cpp-sys-2 --locked', which removes every build of the engine in this target, those "
                 "of every checkout sharing it included")
        target = self.root / 'target with "quotes"'
        result = self.run_script("update-stack.sh")
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn(said, result.stderr)
        self.assertIn(clean, result.stderr)
        self.assertNotIn("engine out", result.stdout)
        self.assertNotIn("== plan", result.stdout)
        self.assertEqual(list(target.glob("weaver-deploy-messages-*")), [], "the messages file is removed")
        self.assert_unprivileged()
        shutil.rmtree(target)
        self.log.unlink(missing_ok=True)
        result = self.library_install()
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn(said, result.stderr)
        placed = [c for c in self.calls() if c[:2] in (["sudo", "install"], ["sudo", "cp"], ["sudo", "tee"],
                                                       ["sudo", "mv"], ["sudo", "ldconfig"])]
        self.assertEqual(placed, [], self.calls())
        self.assertEqual((self.lib_dir / "libllama.so.0.0.1").read_text(), "fixture libllama a")
        shutil.rmtree(target)
        self.log.unlink(missing_ok=True)
        result = self.bootstrap()
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn(said, result.stderr)
        self.assertIn(clean, result.stderr)
        self.assertNotIn("was not built", result.stderr)
        self.assert_unprivileged()

    def test_a_loader_entry_refuses_a_cuda_directory_root_does_not_hold_closed(self):
        """**Where the plan would write the loader's entry, the CUDA directory
        it names is judged first** (deploy/engine-libraries.sh): one absolute
        path, held closed by root, or the plan refuses before any sudo. Where
        the entry stands, the variable is not read. Perturbation: drop the
        `loader_dir` call and each refused case plans the entry."""
        open_dir = self.root / "open-cuda"
        open_dir.mkdir()
        open_dir.chmod(0o777)
        self.ldso_conf.unlink()
        for value, said in (("relative/lib64", "is not one absolute path"),
                            (str(self.root / "no-such-cuda"), "is not held closed by root"),
                            (str(open_dir), f"is not held closed by root at {open_dir}")):
            with self.subTest(value=value):
                self.log.unlink(missing_ok=True)
                shutil.rmtree(self.root / 'target with "quotes"', ignore_errors=True)
                self.env["CUDA_LIB_DIR"] = value
                result = self.run_script("update-stack.sh")
                self.assertEqual(result.returncode, 1, result.stdout)
                self.assertIn(f"CUDA_LIB_DIR {value} {said}", result.stderr)
                self.assertNotIn("NEW      " + str(self.lib_dir), result.stdout)
                self.assertFalse(any(c[0] == "sudo" for c in self.calls()), self.calls())
        self.ldso_conf.write_text(f"{self.lib_dir}\n{self.cuda_lib}\n")
        shutil.rmtree(self.root / 'target with "quotes"', ignore_errors=True)
        self.env["CUDA_LIB_DIR"] = str(open_dir)
        result = self.run_script("update-stack.sh")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("stands; left as it is", result.stdout)

    def library_install(self, **env):
        """An --install on a box with no agent, the engine moved to build b,
        libggml's SONAME link at another target and libllama's bare link
        absent, so one file is replaced, one link replaced and one added."""
        shutil.rmtree(self.config / "existing")
        (self.lib_dir / "libggml.so.0").unlink()
        (self.lib_dir / "libggml.so.0").symlink_to("libggml.so.0.9.0")
        (self.lib_dir / "libllama.so").unlink()
        admin = self.root / "fixture-admin"
        admin.write_text("#!/bin/sh\nexit 2\n")
        admin.chmod(0o755)
        self.env.update(ALLOW_APPLY_CHECKS="1", FIXTURE_ADMIN=str(admin), ENGINE_BUILD="b", **env)
        return self.run_script("update-stack.sh", "--install")

    def test_a_name_the_build_no_longer_makes_is_removed_and_put_back_by_a_rollback(self):
        """**The library directory ends holding the build's engine and no
        other**: a versioned file of a higher build number, which `ldconfig`
        would point the SONAME link at, and a link the build does not make
        are planned REMOVED, removed under the window into the backup's lib/,
        and put back where the install rolls back. Perturbations: drop the
        removal and the stale file stays; drop it from the restore and the
        rollback leaves it gone."""
        (self.lib_dir / "libllama.so.0.0.9").write_text("fixture libllama stale")
        (self.lib_dir / "libllama.so.1").symlink_to("libllama.so.0.0.9")
        result = self.library_install()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        digest = hashlib.sha256(b"fixture libllama stale").hexdigest()[:12]
        self.assertIn(f"  {'libllama.so.0.0.9':<22} REMOVED  {digest}\n", result.stdout)
        self.assertIn(f"  {'(stale links)':<22} 1 to remove\n", result.stdout)
        self.assertFalse((self.lib_dir / "libllama.so.0.0.9").exists())
        self.assertFalse((self.lib_dir / "libllama.so.1").is_symlink())
        backup, = self.root.glob("backup-*")
        self.assertEqual((backup / "lib" / "libllama.so.0.0.9").read_text(), "fixture libllama stale")
        self.assertEqual(os.readlink(backup / "lib" / "libllama.so.1"), "libllama.so.0.0.9")
        # The same box, rolled back by an unresolved library: a fresh fixture.
        self.setUp()
        (self.lib_dir / "libllama.so.0.0.9").write_text("fixture libllama stale")
        (self.lib_dir / "libllama.so.1").symlink_to("libllama.so.0.0.9")
        result = self.library_install(LDD_MISSING="libggml.so.0")
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertEqual((self.lib_dir / "libllama.so.0.0.9").read_text(), "fixture libllama stale")
        self.assertEqual(os.readlink(self.lib_dir / "libllama.so.1"), "libllama.so.0.0.9")

    def test_the_install_takes_the_engine_only_from_the_library_directory(self):
        """**The resolution check is the agent's, not the operator's shell's**
        (deploy/engine-libraries.sh): ldd runs with LD_LIBRARY_PATH the
        library directory and no LD_PRELOAD, whatever the operator carries;
        an engine resolved outside the directory, and an SPU ldd cannot read,
        each roll the install back. Perturbations: run ldd in the operator's
        environment and the outside directory passes; drop ldd's status and
        the unreadable SPU passes."""
        # A preload every process can load, so only its absence at ldd counts.
        preload = "/usr/lib/libm.so.6" if os.path.exists("/usr/lib/libm.so.6") else ""
        self.env.update(LD_LIBRARY_PATH="/operator/out/lib", LD_PRELOAD=preload)
        result = self.library_install()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn(["ldd-env", str(self.lib_dir), ""], self.calls())
        self.assertIn(f"of them the engine's from {self.lib_dir}", result.stdout)
        for env, said in (({"LDD_ELSEWHERE": "/operator/out/lib"}, "resolves its engine outside"),
                          ({"LDD_STATIC": "1"}, "ldd could not read the installed weaver-spu (it exited 1)")):
            with self.subTest(env=env):
                self.setUp()
                result = self.library_install(**env)
                self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
                self.assertIn(said, result.stderr)
                self.assertEqual((self.lib_dir / "libllama.so.0.0.1").read_text(), "fixture libllama a")

    def test_the_install_carries_the_engine_libraries_under_the_window(self):
        """**The install carries each changed library and link, keeping the
        old ones in the backup's lib/** and the loader's entry where it is
        missing: a new file, root's without following a link, ldconfig, then
        the SPU's every library resolved. Perturbations: drop the library
        loop and libllama stays build a; copy without `-a` and the link
        becomes a file; drop the backup and lib/ holds nothing."""
        self.ldso_conf.unlink()
        result = self.library_install()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn(f"  {'/etc/ld.so.conf.d/weaver.conf':<22} NEW      {self.lib_dir} and {self.cuda_lib}, then ldconfig",
                      result.stdout)
        self.assertEqual((self.lib_dir / "libllama.so.0.0.1").read_text(), "fixture libllama b")
        self.assertEqual((self.lib_dir / "libggml.so.0.9.1").read_text(), "fixture libggml b")
        self.assertEqual(os.readlink(self.lib_dir / "libggml.so.0"), "libggml.so.0.9.1")
        self.assertEqual(os.readlink(self.lib_dir / "libllama.so"), "libllama.so.0")
        backup, = self.root.glob("backup-*")
        self.assertEqual((backup / "lib" / "libllama.so.0.0.1").read_text(), "fixture libllama a")
        self.assertEqual(os.readlink(backup / "lib" / "libggml.so.0"), "libggml.so.0.9.0")
        self.assertFalse((backup / "lib" / "libllama.so").exists() or (backup / "lib" / "libllama.so").is_symlink())
        self.assertEqual(self.ldso_conf.read_text(), f"{self.lib_dir}\n{self.cuda_lib}\n")
        calls = self.calls()
        # A file is installed root's from the moment it has a name; a link is
        # carried as a link and made root's without following it.
        out = self.root / 'target with "quotes"' / "release" / "build" / "llama-cpp-sys-2-fixture" / "out" / "lib"
        for name in ("libllama.so.0.0.1", "libggml.so.0.9.1"):
            self.assertIn(["sudo", "install", "-o", "root", "-g", "root", "-m", "0755", "--",
                           str(out / name), str(self.lib_dir / name)], calls)
        for name in ("libggml.so.0", "libllama.so"):
            self.assertIn(["sudo", "chown", "-h", "root:root", "--", str(self.lib_dir / name)], calls)
        placed = [c for c in calls if c[:2] in (["sudo", "cp"], ["sudo", "install"])
                  and c[-1].startswith(str(self.lib_dir)) and "-d" not in c]
        copies = [c for c in placed if c[1] == "cp"]
        self.assertTrue(copies and all(c[2:4] == ["-a", "--remove-destination"] for c in copies), copies)
        # The versioned files are carried before any link.
        order = [Path(c[-1]).name for c in placed]
        self.assertLess(max(order.index(n) for n in ("libllama.so.0.0.1", "libggml.so.0.9.1")),
                        min(order.index(n) for n in ("libggml.so.0", "libllama.so")), order)
        self.assertIn(["sudo", "ldconfig"], calls)
        self.assertLess(calls.index(["sudo", "ldconfig"]), next(i for i, c in enumerate(calls) if c[0] == "ldd"))
        self.assertIn(f"weaver-spu resolves every library it names, 2 of them the engine's from {self.lib_dir}",
                      result.stdout)
        self.assertIn("the box is at", result.stdout)

    def test_an_unresolved_library_rolls_the_libraries_back(self):
        """**A library the SPU cannot resolve after the install rolls back**,
        the libraries with the binaries: the replaced file and link come back
        from the backup, the added link goes, the loader's entry this run
        wrote goes, and ldconfig runs over what stands. Perturbations: drop
        the library restore and libllama stays build b; drop the removal and
        the added link stays; drop the entry's removal and it stays."""
        self.ldso_conf.unlink()
        result = self.library_install(LDD_MISSING="libllama.so.0")
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("libllama.so.0 => not found", result.stderr)
        self.assertIn("weaver-spu has unresolved libraries after the install", result.stderr)
        self.assertEqual((self.lib_dir / "libllama.so.0.0.1").read_text(), "fixture libllama a")
        self.assertEqual((self.lib_dir / "libggml.so.0.9.1").read_text(), "fixture libggml a")
        self.assertEqual(os.readlink(self.lib_dir / "libggml.so.0"), "libggml.so.0.9.0")
        self.assertFalse((self.lib_dir / "libllama.so").is_symlink() or (self.lib_dir / "libllama.so").exists())
        self.assertFalse(self.ldso_conf.exists())
        self.assertEqual(self.calls().count(["sudo", "ldconfig"]), 2, self.calls())
        self.assertNotIn("RESTORE INCOMPLETE", result.stderr)

    TUPLE_KEYS = ["commit", "toolchain", "features", "engine", "engine-source", "engine-vendored", "nvcc",
                  "member.worker", "member.weaver-admin", "member.weaver-trace-relay", "member.weaver-gate",
                  "member.weaver-spu", "member.weaver-state", "member.busybox", "member.busybox.applets",
                  "library.libggml.so.0.9.1", "library.libllama.so.0.0.1",
                  "runtime.cudart", "runtime.cublas", "runtime.cublaslt", "runtime.curand", "cccl"]

    def test_a_verified_install_prints_and_records_the_install_tuple(self):
        """**A verified install ends with the install tuple** (the Planner's
        brief of 2026-11-09): printed under `== tuple` before the closing
        line and written whole to the stack record's install-tuple, one
        `key = value` line per fact in a fixed order: the members and the
        libraries by their installed sha256 in full, the engine as the lock
        pins it, and the runtime files the installed SPU resolves, each by
        its sha256 and the versioned file its name leads to. **Build facts
        only** (the operator's rulings of 2026-11-14): no driver, card count
        or card, though the box's nvidia-smi answers all three. A plan writes
        none. Perturbations: drop the write and the record holds nothing;
        collect no library names and the keys differ; print the tuple after
        the closing line and the order fails; read the runtime in the
        operator's environment and its library path reaches ldd; read the
        lock unfiltered and the metadata call carries no platform; record
        the name ldd prints rather than the file it leads to and the path is
        the link's; put back the driver or card lines and the keys differ."""
        # The operator's shell carries its own library path, which neither
        # the install's check nor the tuple's runtime read may take.
        result = self.library_install(LD_LIBRARY_PATH="/operator/out/lib")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        seen = [c[1] for c in self.calls() if c[0] == "ldd-env"]
        self.assertEqual(seen, [str(self.lib_dir)] * 2, "the check and the tuple each read as admin runs the SPU")
        self.assertLess(result.stdout.index("== tuple"), result.stdout.index("== the box is at"))
        block = result.stdout.split("== tuple\n", 1)[1].split("\n\n", 1)[0].splitlines()
        self.assertEqual(block[-1], f"  written to {self.stack / 'install-tuple'}")
        printed = [line[2:] for line in block[:-1]]
        record = (self.stack / "install-tuple").read_text().splitlines()
        self.assertEqual(record, printed)
        facts = dict(line.split(" = ", 1) for line in record)
        self.assertEqual([line.split(" = ", 1)[0] for line in record], self.TUPLE_KEYS)
        installed = self.root / "installed"
        for member in ("worker", "weaver-spu", "busybox.applets"):
            self.assertEqual(facts[f"member.{member}"], hashlib.sha256((installed / member).read_bytes()).hexdigest())
        self.assertEqual(facts["library.libllama.so.0.0.1"],
                         hashlib.sha256(b"fixture libllama b").hexdigest())
        self.assertEqual(facts["commit"], "abcdef0")
        self.assertEqual(facts["features"], "weaver-spu/cuda,weaver-state/sqlite")
        self.assertEqual(facts["engine"], "llama-cpp-sys-2 0.1.0")
        self.assertEqual(facts["engine-source"], "git+https://fixture.invalid/llama-cpp-rs?rev=f00d#f00dfixtureforkcommit")
        self.assertEqual(facts["engine-vendored"], "e21cdc11fixturevendoredcommit")
        self.assertEqual(facts["nvcc"], "99.9, V99.9.1")
        for key, file in (("cudart", "libcudart.so.99.1.2"), ("cublas", "libcublas.so.99.3.4"),
                          ("cublaslt", "libcublasLt.so.99.3.4"), ("curand", "libcurand.so.99.5.6")):
            path = (self.cuda_lib / file).resolve()
            self.assertEqual(facts[f"runtime.{key}"], f"{hashlib.sha256(path.read_bytes()).hexdigest()} {path}")
        self.assertEqual(facts["cccl"], "3.3.4-1")
        self.assertFalse([key for key in facts if key in ("driver", "gpus") or key.startswith(("gpu.", "runtime.libcuda"))],
                         "the hardware half is read at each load, never stored")
        # The record reads as the helper takes it, a tuple of this form.
        install = _agent_tuple().read_install_tuple(str(self.stack / "install-tuple"))
        self.assertEqual(install["earlier"], [])
        self.assertEqual(sorted(install["runtime"]), ["cublas", "cublaslt", "cudart", "curand"])
        # The lock read filtered to the host, so an offline cache a build
        # filled answers it.
        metadata = [c for c in self.calls() if c[:2] == ["cargo", "metadata"] and "--no-deps" not in c]
        self.assertTrue(metadata and all("--filter-platform" in c for c in metadata), metadata)
        tee = [c for c in self.calls() if c[:2] == ["sudo", "tee"] and c[-1].endswith("/install-tuple")]
        self.assertEqual(len(tee), 1, self.calls())
        self.assertIn(["sudo", "chmod", "0644", str(self.stack / "install-tuple")], self.calls())

    def test_a_plan_writes_no_install_tuple_and_says_the_install_will(self):
        result = self.run_script("update-stack.sh")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn(f"--install ends, once verified, by writing the install tuple to {self.stack / 'install-tuple'}",
                      result.stdout)
        self.assertNotIn("== tuple", result.stdout)
        self.assertFalse((self.stack / "install-tuple").exists())
        self.assert_unprivileged()

    def test_the_install_tuple_reads_unknown_where_the_box_cannot_answer(self):
        """**A fact the box cannot answer reads `unknown`, and no key is left
        out**: no SPU to resolve, no lock to read and no toolchain file, on a
        box with no card, where nvidia-smi answers its failure sentence. The
        sentence is never a fact, since the tuple reads no hardware.
        Perturbation: print nothing for an unanswered fact and the keys
        differ."""
        self.env["NO_NVIDIA"] = "1"
        empty = self.root / "empty-repo"
        empty.mkdir()
        script = (f"source {shlex.quote(str(self.repo / 'deploy' / 'engine-libraries.sh'))}; "
                  f"source {shlex.quote(str(self.repo / 'deploy' / 'install-tuple.sh'))}; "
                  f"install_tuple {shlex.quote(str(empty))} fixture-commit {shlex.quote(str(self.root / 'no-bin'))} "
                  f"{shlex.quote(str(self.root / 'no-lib'))} fixture-features 'worker' 'libllama.so.0.0.1'")
        env = {**self.env, "PATH": str(self.bin) + os.pathsep + GUARDED_USR_BIN}
        (self.bin / "cargo").write_text("#!/bin/sh\nexit 101\n")
        (self.bin / "git").write_text("#!/bin/sh\nexit 128\n")
        ran = subprocess.run(["bash", "-c", script], env=env, text=True, capture_output=True, timeout=20)
        self.assertEqual(ran.returncode, 0, ran.stderr)
        facts = dict(line.split(" = ", 1) for line in ran.stdout.splitlines())
        self.assertEqual(list(facts), ["commit", "toolchain", "features", "engine", "engine-source", "engine-vendored",
                                       "nvcc", "member.worker", "library.libllama.so.0.0.1", "runtime.cudart",
                                       "runtime.cublas", "runtime.cublaslt", "runtime.curand", "cccl"])
        for key in ("toolchain", "engine-source", "engine-vendored", "member.worker", "library.libllama.so.0.0.1",
                    "runtime.cudart", "runtime.cublas", "runtime.cublaslt", "runtime.curand"):
            self.assertEqual(facts[key], "unknown", key)
        self.assertEqual(facts["engine"], "llama-cpp-sys-2 unknown")
        self.assertEqual(facts["commit"], "fixture-commit", "the commit is the caller's, never read again")
        self.assertNotIn("NVIDIA-SMI", ran.stdout)

    def ubuntu_box(self):
        """Lay out an Ubuntu box's CUDA as NVIDIA's repository installs it:
        two toolkit releases, each with its CCCL under
        `targets/x86_64-linux/include/cccl` from its own package, each
        `include` a link to its target's, and `cuda` a link to the newer,
        whose nvcc is the one on PATH. Answers the CCCL directory of each
        release, by release."""
        local = self.root / "usr-local"
        cccl = {}
        for release in ("99.3", "99.4"):
            top = local / f"cuda-{release}"
            include = top / "targets" / "x86_64-linux" / "include"
            (include / "cccl" / "cuda" / "std" / "__cccl").mkdir(parents=True)
            (include / "cccl" / "cuda" / "std" / "__cccl" / "version.h").write_text(
                f"#define CCCL_VERSION fixture-{release}\n")
            (top / "include").symlink_to("targets/x86_64-linux/include")
            cccl[release] = include / "cccl"
        (local / "cuda-99.4" / "bin").mkdir()
        nvcc = local / "cuda-99.4" / "bin" / "nvcc"
        nvcc.write_text(DOUBLE)
        nvcc.chmod(0o755)
        (local / "cuda").symlink_to("cuda-99.4")
        return cccl

    def ubuntu_tuple(self, pacman=False, dpkg=True, **env):
        """The install tuple as bootstrap writes it, `TUPLE=$(install_tuple
        ...)` under `set -euo pipefail`, on a box whose PATH holds no pacman
        (or, with PACMAN, the double's), the dpkg-query double unless DPKG
        is false, the fixture toolkit's nvcc and the system's tools the
        function names. Answers the facts and the dpkg-query calls."""
        bin_dir = self.root / "ubuntu-bin"
        if not bin_dir.exists():
            bin_dir.mkdir()
            for tool in ("bash", "python3", "sed", "grep", "awk", "cut", "sha256sum", "tr", "readlink",
                         "dirname", "env"):
                (bin_dir / tool).symlink_to(shutil.which(tool))
            for name, body in (("cargo", "#!/bin/sh\nexit 101\n"), ("git", "#!/bin/sh\nexit 128\n"),
                               ("ldd", "#!/bin/sh\nexit 1\n")):
                (bin_dir / name).write_text(body)
                (bin_dir / name).chmod(0o755)
        # Absent, not merely not executable: bash's `command -v` answers a
        # file it cannot run when PATH holds no other of the name.
        for name, keep in (("pacman", pacman), ("dpkg-query", dpkg)):
            (bin_dir / name).unlink(missing_ok=True)
            if keep:
                (bin_dir / name).write_text(DOUBLE)
                (bin_dir / name).chmod(0o755)
        empty = self.root / "empty-repo"
        empty.mkdir(exist_ok=True)
        script = (f"set -euo pipefail; "
                  f"source {shlex.quote(str(self.repo / 'deploy' / 'engine-libraries.sh'))}; "
                  f"source {shlex.quote(str(self.repo / 'deploy' / 'install-tuple.sh'))}; "
                  f"TUPLE=$(install_tuple {shlex.quote(str(empty))} fixture-commit "
                  f"{shlex.quote(str(self.root / 'no-bin'))} {shlex.quote(str(self.root / 'no-lib'))} "
                  f"fixture-features 'worker' 'libllama.so.0.0.1'); printf '%s\\n' \"$TUPLE\"")
        path = os.pathsep.join([str(bin_dir), str(self.root / "usr-local" / "cuda" / "bin")])
        self.log.unlink(missing_ok=True)
        # weaver-spu's build script reads CUDA_PATH, else CUDA_ROOT, as the
        # build guest sets the first to its toolkit.
        base = {k: v for k, v in self.env.items() if k not in ("CUDA_PATH", "CUDA_ROOT")}
        base["CUDA_PATH"] = str(self.root / "usr-local" / "cuda")
        ran = subprocess.run([str(bin_dir / "bash"), "-c", script], env={**base, "PATH": path, **env},
                             text=True, capture_output=True, timeout=20)
        self.assertEqual(ran.returncode, 0, ran.stderr)
        facts = dict(line.split(" = ", 1) for line in ran.stdout.splitlines())
        return facts, [c for c in self.calls() if c[0] == "dpkg-query"]

    def dpkg_database(self, cccl, **changes):
        """The fixture's package database: each release's CCCL header owned
        by its own package, by the path dpkg records, the resolved one."""
        header = "cuda/std/__cccl/version.h"
        db = {"owners": {str((cccl[r] / header).resolve()): f"cccl-{r.replace('.', '-')}" for r in cccl},
              "versions": {"cccl-99-3": "99.3.3.4.1-1", "cccl-99-4": "99.3.4.3.1-1"}}
        db.update(changes)
        return json.dumps(db)

    def test_without_pacman_the_tuple_records_the_cccl_package_nvcc_reads_first(self):
        """**Where pacman is absent, the cccl fact is the dpkg package that
        owns the CCCL header nvcc reads first**, as `<package>=<version>`
        (the Planner's brief of 2026-11-15): an Ubuntu box holds one CCCL
        package per toolkit release, as the appliance image holds
        `cccl-13-3` and `cccl-13-4`, and builds against the one its
        NVCC_PREPEND_FLAGS names. The first include directory of those flags
        that holds the header wins, by any spelling nvcc reads and across
        lines, and the path is asked for as found and then resolved, since
        dpkg records the path a package unpacks. With none, the header is
        the toolkit's own CCCL, beside the nvcc on PATH, and the SPU
        build's own include may hold that same file. Where pacman stands
        it answers, and dpkg-query is never asked. Perturbations: read no
        prepended flag and the first case reads cccl-99-4; ask only as
        found and the linked spelling reads unknown; ask only as resolved
        and the linked ancestor reads unknown; read one directory per flag
        and the comma list reads cccl-99-4; read the first line of the
        flags and the two-line case reads cccl-99-4; compare the build's
        include as spelled and its link to the toolkit reads unknown; pass
        over pacman and the last case asks dpkg-query."""
        cccl = self.ubuntu_box()
        self.env["FIXTURE_DPKG"] = self.dpkg_database(cccl)
        bare = self.root / "no-cccl-here"
        bare.mkdir()
        linked = self.root / "usr-local" / "cuda-99.3" / "include" / "cccl"
        header = str((cccl["99.3"] / "cuda/std/__cccl/version.h").resolve())
        for case, flags in (("separate", f"-O2 -I {bare} -I {cccl['99.3']}"),
                            ("joined", f"-I{bare} -I{cccl['99.3']}"),
                            ("long", f"--include-path={bare} --include-path {cccl['99.3']}"),
                            ("comma list", f"-I{bare},{cccl['99.3']}"),
                            ("two lines", f"-O2\n-I{cccl['99.3']}"),
                            ("linked spelling", f"-I{linked}")):
            with self.subTest(case=case):
                facts, asked = self.ubuntu_tuple(NVCC_PREPEND_FLAGS=flags)
                self.assertEqual(facts["cccl"], "cccl-99-3=99.3.3.4.1-1")
                self.assertIn(["dpkg-query", "-S", header], asked)
        with self.subTest(case="a linked ancestor"):
            # The path dpkg records runs through a link above the toolkit, as
            # where /usr/local is itself a link: asked for as found it is
            # owned, and resolved it is not.
            (self.root / "usr-link").symlink_to("usr-local")
            spelled = self.root / "usr-link" / "cuda-99.3" / "targets" / "x86_64-linux" / "include" / "cccl"
            owned = str(spelled / "cuda/std/__cccl/version.h")
            facts, _ = self.ubuntu_tuple(NVCC_PREPEND_FLAGS=f"-I{spelled}",
                                         FIXTURE_DPKG=self.dpkg_database(cccl, owners={owned: "cccl-99-3"}))
            self.assertEqual(facts["cccl"], "cccl-99-3=99.3.3.4.1-1")
        with self.subTest(case="the build's include links to the toolkit's"):
            same = self.root / "cuda-same"
            same.mkdir()
            (same / "include").symlink_to(cccl["99.4"])
            facts, _ = self.ubuntu_tuple(CUDA_PATH=str(same))
            self.assertEqual(facts["cccl"], "cccl-99-4=99.3.4.3.1-1")
        with self.subTest(case="the toolkit's own"):
            facts, asked = self.ubuntu_tuple()
            self.assertEqual(facts["cccl"], "cccl-99-4=99.3.4.3.1-1")
            self.assertEqual(asked[0], ["dpkg-query", "-S",
                                        str((cccl["99.4"] / "cuda/std/__cccl/version.h").resolve())])
        with self.subTest(case="an include directory appended that holds no CCCL"):
            facts, _ = self.ubuntu_tuple(NVCC_APPEND_FLAGS=f"-I{bare}")
            self.assertEqual(facts["cccl"], "cccl-99-4=99.3.4.3.1-1")
        with self.subTest(case="pacman stands"):
            facts, asked = self.ubuntu_tuple(pacman=True, NVCC_PREPEND_FLAGS=f"-I{cccl['99.3']}")
            self.assertEqual(facts["cccl"], "3.3.4-1")
            self.assertEqual(asked, [])

    def test_without_pacman_a_cccl_the_box_cannot_place_reads_unknown(self):
        """**A cccl package the box cannot place reads `unknown`**, never a
        guess: a relative include directory ahead of the rest (relative to
        a compiler's directory this script never sees), an appended include
        directory that holds CCCL (the build's own flags stand between it
        and the toolkit's), an include form the reader does not rank (a
        system include, options for the host compiler, an options file, a
        pre-included header, a quote), an SPU build include that holds a
        CCCL of its own (the engine's compile does not read it), a toolkit
        of two targets, a header no package owns (a toolkit from NVIDIA's
        installer) or two packages own, a header path dpkg-query would read
        as a pattern, a header a diversion names, a package's or a local
        one, by either spelling and in any message locale (the file there
        may be the replacement, while the original package's line still
        names the path), and a box with no dpkg-query. Perturbations: skip
        relative directories, drop the appended flags' read, rank no other
        include form, rank an options file or a pre-included header as no
        flag, drop the build include's check, take the first target, take
        the first of two owners, drop the pattern refusal, pass over a
        package's or a local diversion's lines, ask the resolved spelling
        after a diversion of the spelling as found, or ask dpkg-query in
        the box's locale, and that case reads a package."""
        cccl = self.ubuntu_box()
        database = self.dpkg_database(cccl)
        header = str((cccl["99.3"] / "cuda/std/__cccl/version.h").resolve())
        odd = self.root / "odd[dir]"
        (odd / "cuda" / "std" / "__cccl").mkdir(parents=True)
        (odd / "cuda" / "std" / "__cccl" / "version.h").write_text("#define CCCL_VERSION fixture-odd\n")
        oddheader = str((odd / "cuda/std/__cccl/version.h").resolve())
        # A CUDA 12 layout, its CCCL directly under include, owned.
        twelve = self.root / "cuda-99.2"
        (twelve / "include" / "cuda" / "std" / "__cccl").mkdir(parents=True)
        (twelve / "include" / "cuda" / "std" / "__cccl" / "version.h").write_text("#define CCCL_VERSION fixture-12\n")
        twelveheader = str((twelve / "include" / "cuda/std/__cccl/version.h").resolve())
        # An options file naming the older toolkit's CCCL, which nvcc reads.
        optf = self.root / "cccl.flags"
        optf.write_text(f"-I{cccl['99.3']}\n")
        for case, env in (
                ("relative first", {"NVCC_PREPEND_FLAGS": f"-Irelative/cccl -I{cccl['99.3']}"}),
                ("appended CCCL", {"NVCC_APPEND_FLAGS": f"-I{cccl['99.3']}"}),
                ("a system include", {"NVCC_PREPEND_FLAGS": f"-isystem {cccl['99.3']}"}),
                ("host compiler options", {"NVCC_PREPEND_FLAGS": f"-Xcompiler -I{cccl['99.3']}"}),
                ("an options file", {"NVCC_PREPEND_FLAGS": f"--options-file {optf}"}),
                ("an options file, short", {"NVCC_PREPEND_FLAGS": f"-optf {optf}"}),
                ("a pre-included header", {"NVCC_PREPEND_FLAGS": f"--pre-include {header}"}),
                ("a pre-included header, short", {"NVCC_PREPEND_FLAGS": f"-include {header}"}),
                ("a quote", {"NVCC_PREPEND_FLAGS": f'-I{cccl["99.3"]} -DFIXTURE="a b"'}),
                ("the build's own CCCL", {"CUDA_PATH": str(twelve),
                                          "FIXTURE_DPKG": self.dpkg_database(cccl, owners={
                                              **json.loads(database)["owners"], twelveheader: "cccl-99-3"})}),
                ("unowned", {"NVCC_PREPEND_FLAGS": f"-I{cccl['99.3']}",
                             "FIXTURE_DPKG": self.dpkg_database(cccl, owners={})}),
                ("two owners", {"NVCC_PREPEND_FLAGS": f"-I{cccl['99.3']}",
                                "FIXTURE_DPKG": self.dpkg_database(cccl, owners={header: "cccl-99-3, cccl-99-4"})}),
                ("a pattern", {"NVCC_PREPEND_FLAGS": f"-I{odd}",
                               "FIXTURE_DPKG": self.dpkg_database(cccl, owners={oddheader: "cccl-99-3"})})):
            with self.subTest(case=case):
                facts, _ = self.ubuntu_tuple(**{"FIXTURE_DPKG": database, **env})
                self.assertEqual(facts["cccl"], "unknown")
        with self.subTest(case="no dpkg-query"):
            facts, _ = self.ubuntu_tuple(dpkg=False, NVCC_PREPEND_FLAGS=f"-I{cccl['99.3']}", FIXTURE_DPKG=database)
            self.assertEqual(facts["cccl"], "unknown")
        with self.subTest(case="a diversion names the header"):
            facts, _ = self.ubuntu_tuple(NVCC_PREPEND_FLAGS=f"-I{cccl['99.3']}",
                                         FIXTURE_DPKG=self.dpkg_database(cccl, diverted=[header]))
            self.assertEqual(facts["cccl"], "unknown")
        with self.subTest(case="a local diversion names the header"):
            facts, _ = self.ubuntu_tuple(NVCC_PREPEND_FLAGS=f"-I{cccl['99.3']}",
                                         FIXTURE_DPKG=self.dpkg_database(cccl, local_diverted=[header]))
            self.assertEqual(facts["cccl"], "unknown")
        # Through a linked ancestor: the diversion names the spelling the
        # flags give, which no package owns, and the resolved spelling's
        # owner line still names the package, so the asking stops at the
        # diversion.
        (self.root / "usr-divert").symlink_to("usr-local")
        spelled = self.root / "usr-divert" / "cuda-99.3" / "targets" / "x86_64-linux" / "include" / "cccl"
        for kind in ("diverted", "local_diverted"):
            with self.subTest(case=f"{kind} names the spelling as found"):
                facts, asked = self.ubuntu_tuple(
                    NVCC_PREPEND_FLAGS=f"-I{spelled}",
                    FIXTURE_DPKG=self.dpkg_database(cccl, **{kind: [str(spelled / "cuda/std/__cccl/version.h")]}))
                self.assertEqual(facts["cccl"], "unknown")
                self.assertNotIn(["dpkg-query", "-S", header], asked)
        with self.subTest(case="a diversion in a translated locale"):
            facts, _ = self.ubuntu_tuple(NVCC_PREPEND_FLAGS=f"-I{cccl['99.3']}", LC_ALL="", LC_MESSAGES="",
                                         LANG="de_DE.UTF-8",
                                         FIXTURE_DPKG=self.dpkg_database(cccl, diverted=[header]))
            self.assertEqual(facts["cccl"], "unknown")
        with self.subTest(case="two targets"):
            # The other target holds its own CCCL, owned, and sorts first.
            other = self.root / "usr-local" / "cuda-99.4" / "targets" / "sbsa-linux" / "include" / "cccl"
            (other / "cuda" / "std" / "__cccl").mkdir(parents=True)
            (other / "cuda" / "std" / "__cccl" / "version.h").write_text("#define CCCL_VERSION fixture-sbsa\n")
            owners = json.loads(database)["owners"]
            owners[str((other / "cuda/std/__cccl/version.h").resolve())] = "cccl-99-4"
            facts, _ = self.ubuntu_tuple(FIXTURE_DPKG=self.dpkg_database(cccl, owners=owners))
            self.assertEqual(facts["cccl"], "unknown")

    def test_bootstrap_records_the_same_install_tuple_after_its_install(self):
        """**The two scripts write one record from one function**: bootstrap
        sources deploy/install-tuple.sh, as update-stack does, calls it after
        its install and stack record, before its closing line, and writes it
        to the record's install-tuple, 0644; neither script defines the
        function itself. Perturbation: inline a second copy in either script
        and this fails."""
        for name in ("bootstrap-stack.sh", "update-stack.sh"):
            text = (self.repo / "deploy" / name).read_text()
            with self.subTest(script=name):
                self.assertIn('source "$REPO/deploy/install-tuple.sh"', text)
                self.assertNotIn("install_tuple() {", text)
                self.assertEqual(text.count("TUPLE=$(install_tuple "), 1)
                self.assertIn('sudo tee "$STACK/install-tuple"', text)
                self.assertIn('sudo chmod 0644 "$STACK/install-tuple"', text)
        text = (self.repo / "deploy" / "bootstrap-stack.sh").read_text()
        self.assertLess(text.index("w agent-directory"), text.index("TUPLE=$(install_tuple "))
        self.assertLess(text.index("TUPLE=$(install_tuple "), text.index('say "installed at $REV"'))

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
            f'[state-store]\nengine = "none"\n\n[trace-sink]\nkind = "pipe"\npath = "{fifo}"\n' + SAMPLING)
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
        # The preview before the credential prompt, then the act under its
        # own header (test_the_install_window_prints_once_under_install).
        self.assertEqual([l for l in result.stdout.splitlines() if l.startswith("== the install window")],
                         ["== the install window"])
        self.assertLess(result.stdout.index("existing     --install holds /etc/sudoers.d/weaver-existing"),
                        result.stdout.index("\n== holding the connector rules, unloading the agents\n"))
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

    def test_a_signal_after_the_shell_key_on_a_repair_run_removes_it(self):
        """**An interrupted repair run removes the shell key it wrote** (#29,
        Codex on #177), the real signal source modelled: the box already
        holds every binary the build makes, so nothing installs, the stack
        record has no shell-binary, the run writes it, and the stand-in admin
        signals the run's whole process group at the reconcile's validate.
        The exit trap restores: no key remains, and the unchanged busybox
        stands. Perturbation: drop SHELL_KEY_WRITTEN from the trap's condition
        and the key stays."""
        rule, held = self.window_admin(
            f"if [ \"$1\" = validate ] && [ ! -e '{self.root / 'signalled'}' ]; then "
            f": > '{self.root / 'signalled'}'; "
            "kill -TERM -- -$(ps -o pgid= -p $$ | tr -d ' '); sleep 5; fi\n")
        installed = self.root / "installed"
        for member in ("worker", "weaver-admin", "weaver-trace-relay", "weaver-gate",
                       "weaver-spu", "weaver-state"):
            installed.joinpath(member).write_text("fixture artifact " + member)
        installed.joinpath("busybox").write_text("fixture busybox")
        installed.joinpath("busybox.applets").write_text("ls\ncat\n")
        key = self.stack / "shell-binary"
        self.assertFalse(key.exists())
        run = subprocess.run(["bash", str(self.repo / "deploy" / "update-stack.sh"), "--install"],
                             env=self.env, text=True, capture_output=True, timeout=60,
                             start_new_session=True)
        self.assertEqual(run.returncode, 143, run.stdout + run.stderr)
        self.assertIn("nothing to install", run.stdout)
        self.assertIn("the stack record gains shell-binary", run.stdout)
        self.assertFalse(key.exists(), run.stdout + run.stderr)
        self.assertEqual(installed.joinpath("busybox").read_text(), "fixture busybox")
        self.assertTrue(rule.exists(), "the held rule is put back as before")

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

    LIFECYCLE_STEPS = ("0", "1", "1b", "2", "3", "4", "5", "5b", "6", "6b", "7", "8", "8b", "9")

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
        # The settings file the suite writes beside the stand-ins for each run
        # (#590) is the suite's, not the plan's.
        after = {p: p.read_bytes() for p in self.root.rglob("*") if p.is_file() and p != self.log
                 and p.name != FIXTURE_ENVIRON}
        before = {p: b for p, b in before.items() if p.name != FIXTURE_ENVIRON}
        self.assertEqual(before, after)

    def test_lifecycle_steps_name_what_they_check(self):
        # Each step names its Spec section or its #99 finding, in the plan and
        # so in every PASS and FAIL line. Perturbation: drop a finding id from
        # a step's label, and this fails.
        result = self.lifecycle()
        self.assertEqual(result.returncode, 0, result.stderr)
        for label in ("step 0 (deploy/REDEPLOY.md section 0)",
                      "step 1b (weaver-admin-Spec section 9; #99 H3)",
                      "step 2 (weaver-admin-Spec section 6, 3.0 S0 x load; #99 K12; weaver-gate-world-contract section 2)",
                      "step 5 (weaver-admin-Spec section 4, 3.0 S0 x restore and outcome row 6; #99 N6)",
                      "step 5b (weaver-admin-Spec section 9; weaver-gate-world-contract sections 1 and 2)",
                      "step 6b (weaver-admin-Spec section 9, 3.0 outcome row 2; #99 K5)",
                      "step 8 (#99 N1, N5)", "step 8b (#99 H5, closed by #108)",
                      "step 9 (HowToDeployANewAgent.md section 7)"):
            self.assertIn(f"== {label}:", result.stdout)

    def test_lifecycle_verb_steps_cite_their_3_0_row(self):
        # Every step that drives a lifecycle verb names its row of
        # weaver-admin-Spec 3.0 (the state, verb and outcome tables), so a
        # PASS or FAIL line says which row it checked; the steps that drive
        # none (the box, create, the near misses, the staged keys, the take
        # down) keep their own citations. Perturbation: drop the row from
        # step 6's label, and this fails.
        result = self.lifecycle()
        self.assertEqual(result.returncode, 0, result.stderr)
        for step, row in (("2", "3.0 S0 x load"), ("3", "3.0 S2 x save-point"),
                          ("4", "3.0 S2 x unload and outcome row 6"),
                          ("5", "3.0 S0 x restore and outcome row 6"),
                          ("6", "3.0 S2 x force-unload, sole, and outcome row 6"),
                          ("6b", "3.0 outcome row 2"), ("7", "3.0 S8b")):
            line = next((l for l in result.stdout.splitlines()
                         if l.startswith(f"== step {step} (")), "")
            self.assertIn(row, line, f"step {step}")

    def test_lifecycle_the_sole_force_concludes_by_its_row(self):
        # The healthy sole force (step 6) takes its leave save point, so by
        # weaver-admin-Spec 3.0's outcome row 6 it publishes a line arrived
        # at the leave, closes the marker Closed, and the next load records
        # no reset, its lineage the new line; the unload event is forced
        # (I2). Perturbation: put back step 6's "the marker forced; no new
        # manifest line", and this fails.
        result = self.lifecycle()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("unloaded; the marker closed; the unload event forced; "
                      "a manifest line arrived leave (SP_F)", result.stdout)
        self.assertIn("load idle; no reset; its lineage SP_F", result.stdout)
        self.assertNotIn("the marker forced; the unload event forced; no new manifest line",
                         result.stdout)
        script = (self.repo / "deploy" / "verify-lifecycle.sh").read_text()
        body = script[script.index("step_6() {"):script.index("step_6b() {")]
        for check in ('expect_marker "$RUN" closed',
                      'expect_eq "the line arrived at the leave" leave "$(manifest_field last 2)"',
                      'expect_eq "the load carries no reset" absent',
                      'expect_at "the lineage is the new line" "$spf"'):
            self.assertIn(check, body)
        self.assertNotIn("forced-unload", body)

    def test_lifecycle_step_7_reads_show_by_the_ladder(self):
        # After the worker's kill, show answers by weaver-admin-Spec section
        # 3's ladder: in_transition naming the constituents where any still
        # holds the run lock (rung iii) or the agent's cgroup holds them with
        # the lock free (rung iv), unloaded naming none where it is free and
        # the cgroup empty; step 7 accepts exactly those two (Codex on #126,
        # round 3), and asks again while in_transition names none, a process
        # mid-exit (rung iv; the Planner's grade of #165). Perturbation:
        # require unloaded after the kill again, or drop the asking again,
        # and this fails.
        result = self.lifecycle()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("in_transition naming the constituents where any still holds the run lock "
                      "(rung iii) or the agent's cgroup still holds them with the lock free (rung iv), "
                      "naming none while a process is mid-exit (rung iv, asked again), unloaded with "
                      "none where the lock is free and the cgroup empty", result.stdout)
        script = (self.repo / "deploy" / "verify-lifecycle.sh").read_text()
        body = script[script.index("step_7() {"):script.index("step_8() {")]
        self.assertIn("    in_transition/some)", body)
        self.assertIn("    state/none)", body)
        self.assertIn('fail "show after the kill answers by the ladder"', body)
        self.assertIn('[ "$after" = in_transition ] && { [ "$left" = absent ] || [ "$left" = "[]" ]; } || break', body)
        self.assertNotIn('ask show; expect_state "show after the kill" unloaded', body)

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
        self.assertIn("== step 8b (#99 H5, closed by #108):", result.stdout)
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
            ("existing folder", {}, "folder", [], "already exists"),
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
                elif make == "folder": self.folder.mkdir()
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
                    elif make == "folder": self.folder.rmdir()
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
        writes = [l for l in script.splitlines() if "root sh -c" in l and " -cpf " in l]
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

    def test_lifecycle_root_mode_plans_what_root_measures(self):
        # **#576, B2**: with --root-operator, run as root, the plan passes the
        # flag to create-agent.sh, names root the operator, and plans each
        # check root's capability voids in its root-mode form: no group
        # joined and none held, step 5b's rule half at uid 0 with root's
        # seeding line and its socket half on the admin-con account's uid,
        # step 9's show as the box's gate-con account, and update-stack.sh's
        # group reads said to be unable to refuse. The take-down lines carry
        # the flag. Nothing is acted on, and the operator's plan carries none
        # of it. Perturbations: drop the flag from step 1's line, or let the
        # plan print a gpasswd in root mode, and a read below fails.
        self.env["FIXTURE_UID"] = "0"
        result = self.lifecycle("--root-operator")
        self.assertEqual(result.returncode, 0, result.stderr)
        out = result.stdout
        # Both create-agent.sh lines, the plan's and the apply's, carry it.
        self.assertEqual(out.count("$ deploy/create-agent.sh m1 --root-operator --artifact"), 2, out)
        for said in ("operator        root, by --root-operator (#576)",
                     "root holds none of weaver-m1, weaver-m1-trace, weaver-m1-state and weaver-m1-admin",
                     "allowed-uids = [0] in", "root joins no group (#576)",
                     "deploy/turn.py m1 --system --raw as root: the seeding line, from uid 0 (#576)",
                     "allowed-uids = [<weaver-admincon's uid>] (the perturbation",
                     "validated, by admin's exemption of uid 0 from the reach judgment",
                     "root's seeding line is answered, and the record carries one turnless message.system",
                     "weaver-admin show m1, as weaver-gatecon through runuser and as root",
                     "unauthorized as weaver-gatecon, a caller that is not root",
                     "as root (#576): its reads through the operator's groups pass by root's capability",
                     "as root (#576): the refusal reads the root's key"):
            self.assertIn(said, out)
        self.assertNotIn("gpasswd", out)
        self.assertNotIn("holds weaver-m1-trace, weaver-m1-state and weaver-m1-admin and not", out)
        self.assertFalse([c for c in self.calls() if c[0] in ("sudo", "runuser", "as-root")], self.calls())
        kept = self.lifecycle("--root-operator", "--keep")
        self.assertEqual(kept.returncode, 0, kept.stderr)
        self.assertIn("deploy/verify-lifecycle.sh --agent m1 --cleanup --apply --root-operator", kept.stdout)
        self.env.pop("FIXTURE_UID")
        plain = self.lifecycle()
        self.assertEqual(plain.returncode, 0, plain.stderr)
        self.assertNotIn("--root-operator", plain.stdout)
        self.assertNotIn("as root (#576)", plain.stdout)
        self.assertIn("gpasswd -a", plain.stdout)
        usage = self.run_script("verify-lifecycle.sh", "--help")
        self.assertIn("--root-operator", usage.stdout)
        # Both create-agent.sh runs carry the flag, which is empty without it.
        script = (DEPLOY / "verify-lifecycle.sh").read_text()
        runs = [l for l in script.splitlines() if 'deploy/create-agent.sh" "$A"' in l]
        self.assertEqual(len(runs), 2, runs)
        self.assertTrue(all('"$A" $ROOT_FLAG --artifact' in l for l in runs), runs)

    def test_lifecycle_root_mode_refuses_what_is_not_root_s_own_login(self):
        # **#576, B2**: the flag is for root, root without the flag refuses
        # naming it, and a run through sudo from another account refuses in
        # root mode, by SUDO_USER or by SUDO_UID, since admin records the
        # invoker's SUDO_UID as every cause and the operator key is 0. Root
        # through its own sudo passes. Each refuses before anything privileged.
        # Perturbations: drop any one check, and its case runs on.
        cases = (
            ("the flag under another uid", {}, ["--root-operator"], "--root-operator is for root, on a single-purpose appliance"),
            ("root without the flag", {"FIXTURE_UID": "0"}, [], "run it as root with --root-operator"),
            ("sudo from another account", {"FIXTURE_UID": "0", "SUDO_USER": "someone"},
             ["--root-operator"], "not sudo from another account (SUDO_USER someone, SUDO_UID unset)"),
            ("SUDO_UID another uid's", {"FIXTURE_UID": "0", "SUDO_USER": "root", "SUDO_UID": "1000"},
             ["--root-operator"], "admin records the invoker's SUDO_UID as every cause"),
            ("SUDO_UID set and empty", {"FIXTURE_UID": "0", "SUDO_UID": ""},
             ["--root-operator"], "(SUDO_USER unset, SUDO_UID empty)"),
        )
        self.install_stack()
        for name, env, args, said in cases:
            with self.subTest(case=name):
                self.log.unlink(missing_ok=True)
                saved = dict(self.env)
                self.env.update(env)
                try:
                    result = self.lifecycle(*args)
                finally:
                    self.env = saved
                self.assertNotEqual(result.returncode, 0, result.stdout)
                self.assertIn("REFUSED: ", result.stderr)
                self.assertIn(said, result.stderr)
                self.assertFalse([c for c in self.calls() if c[0] in ("sudo", "runuser", "as-root")], self.calls())
        self.env.update(FIXTURE_UID="0", SUDO_USER="root", SUDO_UID="0")
        result = self.lifecycle("--root-operator")
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_lifecycle_root_mode_probes_runuser_and_asks_no_credential(self):
        # **#576, B2**: --apply in root mode asks for no sudo credential. The
        # drop probe, runuser as nobody, stands where the operator's sudo -v
        # stands, and a runuser that cannot drop refuses before anything acts.
        # verify-load.sh's drops are runuser's too (#590), so root's own sudo
        # is not probed. Perturbations: keep sudo -v in root mode, or put the
        # probe of root's sudo back, and the calls name it; drop the runuser
        # probe, and the case runs on.
        archive = self.root / "archive"
        archive.mkdir()
        self.env.update(FIXTURE_UID="0", ALLOW_APPLY_CHECKS="1", CARGO_TARGET_DIR=str(self.root / "t"))
        probe = ["runuser", "-u", "nobody", "--", "env", "-i", "PATH=" + DROP_PATH, "true"]
        for fail, said, made in (("DROP_FAIL", "runuser would not run a command as nobody", [probe]),):
            with self.subTest(fail=fail):
                self.log.unlink(missing_ok=True)
                self.env[fail] = "nobody"
                try:
                    result = self.lifecycle("--root-operator", "--apply", "--archive", str(archive),
                                            "--artifact", str(self.artifact))
                finally:
                    self.env.pop(fail)
                self.assertNotEqual(result.returncode, 0, result.stdout)
                self.assertIn(said, result.stderr)
                self.assertEqual([c for c in self.calls() if c[0] in ("sudo", "runuser", "as-root")], made)

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
        territory = (self.root / "agents").resolve() / "m1" / "weaver-m1"
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
        """**The driver's fallback is applied after the read** (#39): a `head`
        that closed the pipe on a card list failed the pipeline under
        `pipefail`, and the old `|| echo none` inside the substitution
        printed the card and then `none`. Each script's line is run with a
        stand-in nvidia-smi that writes its later cards after a pause; with
        one that answers nothing; with one that prints its failure sentence
        and fails, as on a box with no card, where an image is built; and
        with one where a card in error prints its own line before a healthy
        card's answer and the whole fails. Perturbations: put `|| echo none`
        back inside any of the three; keep the first line whatever it is and
        the sentence or the error is the driver; drop a failing nvidia-smi's
        answer whole and the healthy card's driver is lost."""
        lines = {}
        for name, var in (("bootstrap-stack.sh", "DRIVER"), ("update-stack.sh", "DRIVER"),
                          ("decommission.sh", "driver")):
            text = (DEPLOY / name).read_text()
            line = next(l.strip() for l in text.splitlines() if l.strip().startswith(f"{var}=$(nvidia-smi"))
            self.assertNotIn("echo none", line)
            lines[name] = (line, var)
        with tempfile.TemporaryDirectory() as scratch:
            bin_dir = Path(scratch)
            # Each stand-in answers the query it is asked: `<name>, <version>`
            # per card for name,driver_version, the version alone otherwise.
            answer = ('case "$*" in *name,driver_version*) p="Card %s, 620.70.06\\n";; '
                      '*) p="620.70.06\\n";; esac\n')
            smi = bin_dir / "nvidia-smi"
            smi.write_text("#!/bin/sh\n" + answer + 'printf "$p" 0\nsleep 0.3\nprintf "$p" 1\nprintf "$p" 2\n')
            smi.chmod(0o755)
            # The box's own nvidia-smi is never run: the second case puts one
            # first on the path that answers nothing, as a box without a card.
            silent = bin_dir / "silent"
            silent.mkdir()
            (silent / "nvidia-smi").write_text("#!/bin/sh\nexit 9\n")
            (silent / "nvidia-smi").chmod(0o755)
            cardless = bin_dir / "cardless"
            cardless.mkdir()
            (cardless / "nvidia-smi").write_text(
                "#!/bin/sh\necho \"NVIDIA-SMI has failed because it couldn't communicate with the NVIDIA "
                "driver. Make sure that the latest NVIDIA driver is installed and running.\"\nexit 9\n")
            (cardless / "nvidia-smi").chmod(0o755)
            errored = bin_dir / "errored"
            errored.mkdir()
            (errored / "nvidia-smi").write_text(
                "#!/bin/sh\n" + answer + "echo 'Unable to determine the device handle for GPU0000:41:00.0: "
                "Unknown Error'\nprintf \"$p\" 1\nexit 15\n")
            (errored / "nvidia-smi").chmod(0o755)
            for name, (line, var) in lines.items():
                card = "Card {}, 620.70.06" if "name,driver_version" in line else "620.70.06"
                for path, want in ((f"{bin_dir}{os.pathsep}{GUARDED_USR_BIN}", card.format(0)),
                                   (f"{silent}{os.pathsep}{GUARDED_USR_BIN}", "none"),
                                   (f"{cardless}{os.pathsep}{GUARDED_USR_BIN}", "none"),
                                   (f"{errored}{os.pathsep}{GUARDED_USR_BIN}", card.format(1))):
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
        proves a turn, run as the box's gate-con account, the one caller a
        new agent's rule names. Perturbation: drop either and this fails."""
        text = (DEPLOY / "REDEPLOY.md").read_text()
        purge, build, agents = (text.index(h) for h in ("## 2. Purge", "## 3. Build", "## 4. Agents"))
        login = text.index("**Then log out and log in again**")
        self.assertLess(purge, login)
        self.assertLess(login, build)
        turn = text.index("`verify-load.sh` proves a load; a turn proves a turn, run as the box's gate-con account")
        self.assertLess(agents, turn)
        self.assertIn('`sudo -n -u weaver-gatecon python3 -I - <name>\n"<text>" < deploy/turn.py`', text)
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


def account_drop_source():
    """account-drop.sh sourced as the scripts source it (#590), then the
    DROP_PATH the test hands in the environment, or an empty one (B3), so a
    bare name looked up on the account's side resolves nothing the test did
    not stand in, and the guard reads what the test hands."""
    return ('drop_path=${DROP_PATH-}\n' + f'source {shlex.quote(str(DEPLOY / "account-drop.sh"))}\n'
            + 'DROP_PATH=$drop_path\n')


def lifecycle_functions(script, *names):
    """verify-lifecycle.sh's functions as the script defines them, a
    one-line definition as its line and any other to its closing brace.
    **The root step and the drop come with them** (#576): `root`, which
    every root step runs through, and account-drop.sh's `as_account`, which
    every look as an account runs through, in the operator's mode unless
    ROOT_OPERATOR says otherwise."""
    lines = script.splitlines()
    out = ["ROOT_OPERATOR=${ROOT_OPERATOR:-0}", account_drop_source().rstrip("\n")]
    for name in [n for n in ("root",) if n not in names] + list(names):
        start = next(i for i, l in enumerate(lines) if l.startswith(f"{name}()"))
        if lines[start].rstrip().endswith("}"):
            out.append(lines[start])
        else:
            end = next(i for i in range(start, len(lines)) if lines[i] == "}")
            out.extend(lines[start:end + 1])
    return "\n".join(out) + "\n"


class LifecycleRootModeHelperTests(unittest.TestCase):
    """**#576, B2: every root step and every look as an account goes through
    one helper each.** `root CMD...` is `sudo -n CMD...` for the operator and
    the command itself for root, a leading VAR=value through `env`, and
    `as_account ACCOUNT CMD...` is `sudo -n -u` for the operator and `runuser
    -u ... --` for root, under `setsid` and `env -i` in either mode (B3), a
    leading VAR=value handed to env, and under sudo the env after setsid
    (#590). Each runs alone with sudo, runuser and
    the command as stand-ins that log their argv."""

    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory(prefix="weaver-lifecycle-helpers-")
        self.addCleanup(self.scratch.cleanup)
        root = Path(self.scratch.name)
        self.bin_dir = root / "bin"
        self.bin_dir.mkdir()
        self.calls = root / "calls"
        log = shlex.quote(str(self.calls))
        for tool in ("sudo", "runuser", "cmd"):
            stand_in = self.bin_dir / tool
            stand_in.write_text(f'#!/bin/sh\nprintf "%s\\n" "{tool} $* [${{WEAVER_ADMIN_CONFIG:-unset}}]" >> {log}\n')
            stand_in.chmod(0o755)
        (root / "deploy").mkdir()
        (root / "deploy" / "turn.py").write_text("")
        self.checkout = root
        self.script = (DEPLOY / "verify-lifecycle.sh").read_text()

    def run_fns(self, root_mode, body):
        program = ("set -euo pipefail\n" + f"ROOT_OPERATOR={1 if root_mode else 0}\n"
                   + f"A=m1; ADMIN_BASE=/base; CHECKOUT={shlex.quote(str(self.checkout))}; "
                   + f"LOGDIR={shlex.quote(str(self.checkout))}\n"
                   + lifecycle_functions(self.script, "turn_as") + body)
        self.calls.unlink(missing_ok=True)
        env = {**os.environ, "PATH": str(self.bin_dir) + os.pathsep + os.environ["PATH"]}
        for name in ("BASH_ENV", "WEAVER_ADMIN_CONFIG"):
            env.pop(name, None)
        run = subprocess.run(["bash", "-c", program], env=env, text=True, capture_output=True, timeout=20)
        self.assertEqual(run.returncode, 0, run.stderr)
        return self.calls.read_text().splitlines()

    def test_a_root_step_is_sudo_for_the_operator_and_the_command_for_root(self):
        # Perturbation: send root's step through sudo, or drop the env for a
        # leading VAR=value, and a line below fails.
        self.assertEqual(self.run_fns(False, 'root WEAVER_ADMIN_CONFIG=/x cmd a b\nroot cmd c\n'),
                         ["sudo -n WEAVER_ADMIN_CONFIG=/x cmd a b [unset]", "sudo -n cmd c [unset]"])
        self.assertEqual(self.run_fns(True, 'root WEAVER_ADMIN_CONFIG=/x cmd a b\nroot cmd c\n'),
                         ["cmd a b [/x]", "cmd c [unset]"])
        # A bare root step runs in this shell, so a function stands in as it
        # would for the root-mode tests' routing.
        self.assertEqual(self.run_fns(True, 'tee() { echo "tee in the shell $*" >> "$0"; }\nroot tee x\n'
                                      .replace('"$0"', shlex.quote(str(self.calls)))),
                         ["tee in the shell x"])

    def test_a_look_as_an_account_is_sudo_u_for_the_operator_and_runuser_for_root(self):
        # Perturbation: drop the env under runuser, and runuser is handed the
        # VAR=value as its command.
        self.assertEqual(self.run_fns(False, 'as_account weaver-m1 cmd x\nas_account weaver-m1 V=1 cmd y\n'),
                         ["sudo -n -u weaver-m1 timeout 0 setsid -w env -i PATH= cmd x [unset]",
                          "sudo -n -u weaver-m1 timeout 0 setsid -w env -i PATH= V=1 cmd y [unset]"])
        self.assertEqual(self.run_fns(True, 'as_account weaver-m1 cmd x\nas_account weaver-m1 V=1 cmd y\n'),
                         ["runuser -u weaver-m1 -- env -i PATH= cmd x [unset]",
                          "runuser -u weaver-m1 -- env -i PATH= V=1 cmd y [unset]"])

    def test_turn_py_runs_as_the_account_in_either_mode(self):
        # Both go through env -i (B3), the operator's under sudo and setsid,
        # root's under runuser. Perturbation: pass the VAR=value to runuser
        # bare, and the root line fails.
        self.assertEqual(self.run_fns(False, 'turn_as weaver-gatecon hello there\n'),
                         ["sudo -n -u weaver-gatecon timeout 0 setsid -w env -i PATH= WEAVER_ADMIN_CONFIG=/base "
                          "python3 -I - m1 hello there [unset]"])
        self.assertEqual(self.run_fns(True, 'turn_as weaver-gatecon --system --raw\n'),
                         ["runuser -u weaver-gatecon -- env -i PATH= WEAVER_ADMIN_CONFIG=/base python3 -I - m1 "
                          "--system --raw [unset]"])

    def test_sudo_is_the_root_step_s_the_listing_s_and_the_credential_s_alone(self):
        # Every other privileged call is a root step or a look as an account,
        # and the look's sudo is account-drop.sh's (#590), so root mode no
        # longer probes root's own sudo. Perturbation: put back a bare `sudo
        # -n` at any root step, or the probe, and this names it.
        code = [l for l in self.script.splitlines() if not l.lstrip().startswith("#")]
        found = sorted(m.group(0) for l in code for m in re.finditer(r"\bsudo -[nv]\b[^)|;&]*", l))
        self.assertEqual(found, ['sudo -n "$@"', 'sudo -n -l -U "$ADMINCON" 2>', 'sudo -v '], found)


class LifecycleStep5bRootModeTests(unittest.TestCase):
    """**#576, B2: step 5b in root mode.** The rule half stays at uid 0: the
    rule moves to 0, root joins no group, gate-con's turn is refused and
    root's is answered, then root's seeding line is answered, judged by the
    gate's raw close. The socket half moves to the admin-con account's uid,
    which no root's capability can stand in for, and the measurement is put
    back byte for byte. The step runs alone over a declaration file, with a
    stand-in gate that admits a turn only from the uid the rule names and a
    system line only from the operator key's uid, admin answering each verb,
    and the checks logged with what they expected and found. In the
    operator's mode the same step joins and leaves the group and moves the
    rule to the operator's uid, as before B2. Perturbations: let root mode
    join the group, perturb on the operator's uid, or drop the seeding line,
    and a read below fails."""

    GATE = r"""#!/usr/bin/python3 -I
import json, os, re, sys
name, args = os.path.basename(sys.argv[0]), sys.argv[1:]
if "CALLS" not in os.environ:
    with open(os.path.join(os.path.dirname(sys.argv[0]), "@FIXTURE_ENVIRON@")) as settings:
        os.environ.update(json.load(settings))
if name == "sudo":
    args = args[1:] if args[:1] == ["-n"] else args
    if args[:1] != ["-u"]:
        os.execvp("env", ["env", *args])
    who, cmd = args[1], args[2:]
else:
    assert args[0] == "-u" and args[2] == "--", args
    who, cmd = args[1], args[3:]
with open(os.environ["CALLS"], "a") as log:
    log.write(json.dumps([name, who, *cmd]) + "\n")
if name == "sudo":
    assert cmd[:4] == ["timeout", "0", "setsid", "-w"], cmd
    cmd = cmd[4:]
assert cmd[:2] == ["env", "-i"] and cmd[2].startswith("PATH="), cmd
cmd = cmd[3:]
while cmd and re.match(r"[A-Za-z_][A-Za-z0-9_]*=", cmd[0]):
    cmd = cmd[1:]
assert cmd[:3] == ["python3", "-I", "-"], cmd
uids = {"root": 0, "weaver-gatecon": 111, "weaver-admincon": 222, "op": 12345}
rule = re.search(r"^allowed-uids = \[(\d+)\]$", open(os.environ["DECL"]).read(), re.M)
if not rule or int(rule.group(1)) != uids[who]:
    sys.exit(1)
if "--system" in cmd:
    admitted = uids[who] == int(os.environ["OPERATOR_KEY"])
    print(json.dumps({"kind": "answered", "text": "seeded"} if admitted else
                     {"kind": "refused", "reason": "a line of role system is admitted from the operator alone"}))
else:
    print("Red, yellow and blue.")
""".replace("@FIXTURE_ENVIRON@", FIXTURE_ENVIRON)

    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory(prefix="weaver-lifecycle-5b-root-")
        self.addCleanup(self.scratch.cleanup)
        root = Path(self.scratch.name)
        self.bin_dir = root / "bin"
        self.bin_dir.mkdir()
        self.calls = root / "calls"
        self.decl = root / "agent.toml"
        self.original = 'name = "m1"\n\n[gate-instruction.access-rule]\nallowed-uids = [111]\n'
        log = shlex.quote(str(self.calls))
        for tool, body in (
                ("sudo", self.GATE), ("runuser", self.GATE),
                ("tee", f'#!/bin/sh\ncat > "$2"\nl=$(grep "^allowed-uids" "$2")\n'
                        f'grep -q "^\\[restore\\]" "$2" && l="$l +restore"\n'
                        f'printf \'["tee", "%s"]\\n\' "$l" >> {log}\n'),
                ("gpasswd", f'#!/bin/sh\nprintf \'["gpasswd", "%s", "%s", "%s"]\\n\' "$1" "$2" "$3" >> {log}\n'),
                ("id", '#!/bin/sh\ncase "$1 $2" in\n"-u weaver-gatecon") echo 111 ;;\n"-u weaver-admincon") echo 222 ;;\n'
                       '"-Gn root") echo root ;;\n"-Gn op") echo "op weaver-m1-trace" ;;\n*) exit 1 ;;\nesac\n'),
                ("weaver-admin", f'#!/bin/sh\nprintf \'["admin", "%s"]\\n\' "$1" >> {log}\n'
                                 f'n=$(grep -c \'"admin", "validate"\' {log})\n'
                                 'case "$1" in\n'
                                 '  validate) if [ "$n" = 2 ]; then echo "cause: allowed-uids" >&2; '
                                 'echo \'{"kind":"boundary_unverified"}\'; else echo \'{"kind":"validated"}\'; fi ;;\n'
                                 '  unload) echo \'{"kind":"state","state":"unloaded"}\' ;;\n'
                                 '  load) echo \'{"kind":"state","state":"idle"}\' ;;\n'
                                 '  restore) echo \'{"kind":"restore_named","save_point":"DIGA"}\' ;;\n'
                                 'esac\n')):
            stand_in = self.bin_dir / tool
            stand_in.write_text(body)
            stand_in.chmod(0o755)
        (root / "deploy").mkdir()
        (root / "deploy" / "turn.py").write_text("")
        (root / "deploy" / "agent_tuple.py").write_text("")
        self.root = root
        script = (DEPLOY / "verify-lifecycle.sh").read_text()
        self.functions = lifecycle_functions(
            script, "begin_step", "end_step", "fail", "pass", "measured", "json_at", "json_req", "expect_at",
            "ask", "expect_kind", "answered_state", "holds_group", "restore_rule", "restore_now", "identity_recalls",
            "turn_as", "turn", "step_5b")

    def run_step(self, root_mode, operator_key=None):
        self.decl.write_text(self.original)
        self.calls.unlink(missing_ok=True)
        q = lambda p: shlex.quote(str(p))
        program = ("set -euo pipefail\n"
                   + f"ROOT_OPERATOR={1 if root_mode else 0}; APPLY=1; A=m1; AU=weaver-m1; RESTORE=(); RUN=\"\"\n"
                   + "GATECON=weaver-gatecon; ADMINCON=weaver-admincon; SP_A_NAME=spa; SP_A_DIG=DIGA\n"
                   + (f"OPERATOR=root; OPERATOR_UID=0\n" if root_mode else "OPERATOR=op; OPERATOR_UID=12345\n")
                   + f"DECL={q(self.decl)}; TRACE=/none; CHECKOUT={q(self.root)}; LOGDIR={q(self.root)}\n"
                   + f"ADMIN={q(self.bin_dir / 'weaver-admin')}; ADMIN_BASE=/base\n"
                   + 'say() { :; }\nplan() { :; }\nrun_line() { :; }\nwant() { :; }\n'
                   + f'expect_eq() {{ printf \'["check", "%s", "%s", "%s"]\\n\' "$1" "$2" "$3" >> {q(self.calls)}; }}\n'
                   + 'expect_state() { :; }\nexpect_lay() { :; }\ntrace_lines() { echo 0; }\nmanifest_field() { echo x; }\n'
                   + "events_since() { echo '{\"kind\":\"message.system\",\"payload\":{}}'; }\n"
                   + "last_event() { echo '{\"kind\":\"load\",\"run\":\"r1\",\"payload\":{\"declaration\":\"d\","
                     "\"boundary\":\"b\",\"lineage\":{\"save_point\":\"x\",\"named_at_restore\":false}}}'; }\n"
                   + self.functions + "step_5b\necho FINISHED\n")
        env = {**os.environ, "PATH": str(self.bin_dir) + os.pathsep + os.environ["PATH"], "CALLS": str(self.calls),
               "DECL": str(self.decl),
               "OPERATOR_KEY": operator_key or ("0" if root_mode else "12345")}
        env.pop("BASH_ENV", None)
        run = subprocess.run(["bash", "-c", program], env=env, text=True, capture_output=True, timeout=30)
        self.assertEqual(run.returncode, 0, run.stdout + run.stderr)
        self.assertIn("FINISHED", run.stdout)
        return [json.loads(l) for l in self.calls.read_text().splitlines()]

    def test_root_mode_keeps_the_rule_half_at_0_and_moves_the_socket_half(self):
        calls = self.run_step(root_mode=True)
        self.assertFalse([c for c in calls if c[0] == "gpasswd"], calls)
        self.assertEqual([c[1] for c in calls if c[0] == "tee"],
                         ["allowed-uids = [0]", "allowed-uids = [0] +restore", "allowed-uids = [222]",
                          "allowed-uids = [111]"])
        drops = [c[:2] + c[-2:] for c in calls if c[0] in ("runuser", "sudo")]
        self.assertEqual(drops, [["runuser", "weaver-gatecon", "m1", "Name three primary colours."],
                                 ["runuser", "root", "m1", "Name three primary colours."],
                                 ["runuser", "root", "--system", "--raw"],
                                 ["runuser", "weaver-gatecon", "m1", "Name three primary colours."]])
        checks = {c[1]: c[2:] for c in calls if c[0] == "check"}
        self.assertEqual(checks["root's seeding line is answered"], ["answered", "answered"])
        self.assertEqual(checks["the record carries the seeding, one turnless message.system"], ["1", "1"])
        self.assertEqual(checks["the rule names weaver-admincon's uid alone for the perturbation"],
                         ["allowed-uids = [222]", "allowed-uids = [222]"])
        self.assertEqual(checks["a turn as weaver-gatecon is refused, the rule no longer naming it"], ["refused", "refused"])
        self.assertEqual(checks["a turn as root is answered: a rule naming the operator is admitted"],
                         ["answered", "answered"])
        self.assertEqual(self.decl.read_text(), self.original, "the measurement is put back byte for byte")

    def test_a_refused_seeding_close_is_not_taken_for_an_answer(self):
        # turn.py exits 0 and prints the gate's close whatever it says, so
        # the check reads the close's kind. Here the harness's operator is
        # another uid than the rule's. Perturbation: judge the seeding by
        # turn.py's exit alone, and this reads answered.
        calls = self.run_step(root_mode=True, operator_key="12345")
        checks = {c[1]: c[2:] for c in calls if c[0] == "check"}
        self.assertEqual(checks["root's seeding line is answered"], ["answered", "refused"])

    def test_the_operator_s_mode_is_as_before(self):
        calls = self.run_step(root_mode=False)
        self.assertEqual([c for c in calls if c[0] == "gpasswd"],
                         [["gpasswd", "-a", "op", "weaver-m1"], ["gpasswd", "-d", "op", "weaver-m1"]])
        self.assertEqual([c[1] for c in calls if c[0] == "tee"],
                         ["allowed-uids = [12345]", "allowed-uids = [12345] +restore", "allowed-uids = [12345]",
                          "allowed-uids = [111]"])
        self.assertFalse([c for c in calls if "--system" in c], calls)
        self.assertEqual(self.decl.read_text(), self.original)


class LifecycleRootModeGroupsAndShowTests(unittest.TestCase):
    """**#576, B2: step 1's groups and step 9's show in both modes.** In
    root mode root holds none of the agent's four groups, each read alone, and
    step 9 asks `show` as the box's gate-con account through runuser, answered
    unauthorized, and as root, answered no_such_agent. In the operator's mode
    the operator holds the three groups and not the agent's, and step 9 asks as
    the operator bare and under sudo. Each function runs alone with id, sudo,
    runuser and admin as stand-ins: admin answers by the caller the stand-ins
    record in AS, root's no_such_agent and anyone else's unauthorized."""

    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory(prefix="weaver-lifecycle-groups-show-")
        self.addCleanup(self.scratch.cleanup)
        root = Path(self.scratch.name)
        self.bin_dir = root / "bin"
        self.bin_dir.mkdir()
        self.calls = root / "calls"
        log = shlex.quote(str(self.calls))
        for tool, body in (
                ("id", '#!/usr/bin/env python3\nimport os, sys\n'
                       'held = os.environ.get("GROUPS_" + sys.argv[2]) if sys.argv[1:2] == ["-Gn"] else None\n'
                       'sys.exit(1) if held is None else print(held)\n'),
                ("sudo", f'#!/bin/sh\necho "sudo $*" >> {log}\n[ "$1" = -n ] && shift\nAS=root exec env "$@"\n'),
                ("runuser", f'#!/bin/sh\necho "runuser $*" >> {log}\nwho=$2\nshift 4\n'
                            f'AS=$who exec {shutil.which("env", path=BOX_PATH)} "$@"\n'),
                ("weaver-admin", '#!/bin/sh\ncase $AS in root) echo \'{"kind":"no_such_agent"}\' ;; '
                                 '*) echo \'{"kind":"unauthorized"}\' ;; esac\n')):
            stand_in = self.bin_dir / tool
            stand_in.write_text(body)
            stand_in.chmod(0o755)
        self.functions = lifecycle_functions((DEPLOY / "verify-lifecycle.sh").read_text(),
                                             "fail", "pass", "expect_eq", "json_at", "json_req", "expect_at",
                                             "ask", "expect_kind", "holds_group", "expect_operator_groups",
                                             "expect_show_closed")
        self.admin = self.bin_dir / "weaver-admin"

    def run_fn(self, root_mode, body, **groups):
        program = ("set -euo pipefail\n" + f"ROOT_OPERATOR={1 if root_mode else 0}; STEP_LABEL=test\n"
                   + f"A=m1; AU=weaver-m1; GATECON=weaver-gatecon; ADMIN={shlex.quote(str(self.admin))}; "
                   + "ADMIN_BASE=/base\n" + ("OPERATOR=root\n" if root_mode else "OPERATOR=op\n")
                   + self.functions + body + "\necho FINISHED\n")
        self.calls.unlink(missing_ok=True)
        env = {**os.environ, "PATH": str(self.bin_dir) + os.pathsep + os.environ["PATH"],
               "AS": "root" if root_mode else "op"}
        env.pop("BASH_ENV", None)
        for who, held in groups.items():
            env[f"GROUPS_{who}"] = held
        run = subprocess.run(["bash", "-c", program], env=env, text=True, capture_output=True, timeout=20)
        return run, self.calls.read_text().splitlines() if self.calls.exists() else []

    def test_root_holds_none_of_the_agent_s_groups(self):
        # Perturbations: read the operator's three groups in root mode, or
        # leave one of the four unread, and a case below fails.
        run, _ = self.run_fn(True, "expect_operator_groups", root="root")
        self.assertEqual(run.returncode, 0, run.stdout + run.stderr)
        for g in ("weaver-m1", "weaver-m1-trace", "weaver-m1-state", "weaver-m1-admin"):
            self.assertIn(f"PASS test: root does not hold {g}", run.stdout)
        for held in ("weaver-m1", "weaver-m1-trace", "weaver-m1-state", "weaver-m1-admin"):
            with self.subTest(held=held):
                run, _ = self.run_fn(True, "expect_operator_groups", root=f"root {held}")
                self.assertEqual(run.returncode, 1, run.stdout)
                self.assertIn(f"root does not hold {held}", run.stderr)
        run, _ = self.run_fn(True, "expect_operator_groups")
        self.assertEqual(run.returncode, 1, run.stdout)
        self.assertIn("found:    unreadable", run.stderr)

    def test_the_operator_holds_three_groups_and_not_the_agent_s(self):
        run, _ = self.run_fn(False, "expect_operator_groups", op="op weaver-m1-trace weaver-m1-state weaver-m1-admin")
        self.assertEqual(run.returncode, 0, run.stdout + run.stderr)
        run, _ = self.run_fn(False, "expect_operator_groups",
                             op="op weaver-m1 weaver-m1-trace weaver-m1-state weaver-m1-admin")
        self.assertEqual(run.returncode, 1, run.stdout)
        self.assertIn("op does not hold weaver-m1", run.stderr)

    def test_root_asks_show_as_gate_con_and_as_itself(self):
        # Perturbation: ask the refused show as root, and it answers
        # no_such_agent and fails.
        run, calls = self.run_fn(True, "expect_show_closed")
        self.assertEqual(run.returncode, 0, run.stdout + run.stderr)
        self.assertIn("show as weaver-gatecon answers unauthorized", run.stdout)
        self.assertIn("show as root answers no_such_agent", run.stdout)
        self.assertEqual(calls, [f"runuser -u weaver-gatecon -- env -i PATH= WEAVER_ADMIN_CONFIG=/base "
                                 f"{self.admin} show m1"])

    def test_the_operator_asks_show_bare_and_under_sudo(self):
        run, calls = self.run_fn(False, "expect_show_closed")
        self.assertEqual(run.returncode, 0, run.stdout + run.stderr)
        self.assertIn("show as the operator answers unauthorized", run.stdout)
        self.assertIn("show under sudo answers no_such_agent", run.stdout)
        self.assertEqual(calls, [f"sudo -n WEAVER_ADMIN_CONFIG=/base {self.admin} show m1"])


class AccountDropTests(unittest.TestCase):
    """**B3 and #590: a command run as another account is handed nothing of
    the caller's but what the call names** (the Planner's rulings of
    2026-11-17). While such a command runs, any process of the account's uid
    can read its environment and open its descriptors through /proc. So
    `as_account`, account-drop.sh's, which every deploy script that drops to
    an account sources, in both modes, starts runuser or sudo with DROP_PATH
    alone, and gives the command /dev/null or a named file on standard input,
    pipes the script holds on standard output and standard error, bounded
    past its exit, no other descriptor, a session of its own with no
    controlling terminal, and DROP_PATH with the named VAR=value pairs as its
    whole environment, passes an interrupt on to it, and returns its status,
    else a relay's. The helper runs sourced under bash whose controlling
    terminal is a real pseudo-terminal and which holds one more descriptor,
    with stand-ins for runuser and sudo that record what they were handed and
    run the rest, tripwires of the same names on the drop's PATH, and a
    reporter, the command, that records what it was handed. Perturbations:
    take away any one of these, in either mode, or either relay's status, and
    a case below fails."""

    # A stand-in for runuser or sudo: records its arguments, its own
    # environment block as it was handed and whether it holds a controlling
    # terminal, beside itself, then runs the rest, as runuser's `-u A --` and
    # sudo's `-n -u A` leave it.
    STAND_IN = """#!/usr/bin/python3 -I
import json, os, sys
r = {"argv": sys.argv[1:], "env": {}}
with open("/proc/self/environ", "rb") as block:
    for item in block.read().split(b"\\0"):
        if item:
            key, _, value = item.decode().partition("=")
            r["env"][key] = value
try:
    os.close(os.open("/dev/tty", os.O_RDWR))
    r["tty"] = "opened"
except OSError as e:
    r["tty"] = e.errno
with open(sys.argv[0] + ".json", "w") as record:
    json.dump(r, record)
os.execvp(sys.argv[4], sys.argv[4:])
"""
    REPORTER = """#!/usr/bin/python3 -I
import json, os, select, stat, sys
r = {"links": [os.readlink(f"/proc/self/fd/{n}") for n in (0, 1, 2)],
     "inodes": [os.fstat(n).st_ino for n in (0, 1, 2)],
     "fifo": [stat.S_ISFIFO(os.fstat(n).st_mode) for n in (0, 1, 2)], "extra": [], "env": {}}
for fd in range(3, 1024):
    try:
        os.fstat(fd)
        r["extra"].append(fd)
    except OSError:
        pass
with open("/proc/self/environ", "rb") as block:
    for item in block.read().split(b"\\0"):
        if item:
            key, _, value = item.decode().partition("=")
            r["env"][key] = value
try:
    os.close(os.open("/dev/tty", os.O_RDWR))
    r["tty"] = "opened"
except OSError as e:
    r["tty"] = e.errno
# What standard input holds, read until its end or a second of quiet, so a
# terminal left on it fails the case rather than hanging it.
data = b""
while select.select([0], [], [], 1.0)[0]:
    chunk = os.read(0, 4096)
    if not chunk:
        break
    data += chunk
r["stdin"] = data.decode(errors="replace")
with open(sys.argv[1], "w") as report:
    json.dump(r, report)
# A case that makes a relay fail (/dev/full) can see it close its pipe before
# these writes end. That broken pipe is the relay's failure, which the case
# reads in the status, so the reporter still exits with the status it was
# asked for, its outputs on /dev/null for the flush at exit.
for stream, word in ((sys.stdout, "out-a"), (sys.stderr, "err-b"), (sys.stdout, "out-c")):
    try:
        print(word, file=stream, flush=True)
    except BrokenPipeError:
        pass
devnull = os.open(os.devnull, os.O_WRONLY)
os.dup2(devnull, 1)
os.dup2(devnull, 2)
sys.exit(int(sys.argv[2]))
"""
    # A stand-in for the drop's privileged parent, as it treats signals:
    # runuser's watching parent, for `-u` in the caller's session, passes
    # SIGTERM alone on to its child, and sudo under use_pty relays TERM, INT,
    # QUIT and HUP to a child that leads its own process group.
    RELAY = """#!/usr/bin/python3 -I
import os, signal, sys
name, args = os.path.basename(sys.argv[0]), sys.argv[4:]
caught = (signal.SIGINT, signal.SIGQUIT, signal.SIGHUP, signal.SIGTERM)
for s in caught:
    signal.signal(s, signal.SIG_IGN)
child = os.fork()
if child == 0:
    for s in caught:
        signal.signal(s, signal.SIG_DFL)
    if name == "sudo":
        os.setpgid(0, 0)
    os.execvp(args[0], args)
for s in (signal.SIGTERM,) if name == "runuser" else (signal.SIGTERM, signal.SIGINT, signal.SIGQUIT, signal.SIGHUP):
    signal.signal(s, lambda sig, _: os.kill(child, sig))
_, status = os.waitpid(child, 0)
code = os.waitstatus_to_exitcode(status)
sys.exit(code if code >= 0 else 128 - code)
"""

    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory(prefix="weaver-account-drop-")
        self.addCleanup(self.scratch.cleanup)
        self.root = Path(self.scratch.name)
        self.bin_dir = self.root / "bin"
        self.bin_dir.mkdir()
        # The drop's PATH: the box's setsid, timeout and env, which the drop
        # runs under, and nothing else, so a bare name looked up on the
        # account's side finds nothing more. A runuser or sudo found there,
        # after the clear, is a tripwire (#590).
        self.drop_bin = self.root / "drop-bin"
        self.drop_bin.mkdir()
        for tool in ("setsid", "timeout", "env"):
            (self.drop_bin / tool).symlink_to(shutil.which(tool, path=BOX_PATH))
        self.tripped = self.root / "tripped"
        for tool in ("runuser", "sudo"):
            (self.drop_bin / tool).write_text(f'#!/bin/sh\necho "$0" >> {shlex.quote(str(self.tripped))}\nexit 97\n')
            (self.drop_bin / tool).chmod(0o755)
        self.drop_path = str(self.drop_bin)
        self.report = self.root / "report.json"
        self.feed = self.root / "feed"
        self.feed.write_text("the named file")
        self.extra = self.root / "extra"
        for tool in ("runuser", "sudo"):
            stand_in = self.bin_dir / tool
            stand_in.write_text(self.STAND_IN)
            stand_in.chmod(0o755)
        self.reporter = self.root / "reporter"
        self.reporter.write_text(self.REPORTER)
        self.reporter.chmod(0o755)

    def env(self, bin_dir):
        env = {**os.environ, "PATH": str(bin_dir) + os.pathsep + os.environ["PATH"], "DROP_PATH": self.drop_path,
               "SECRET": "root-only"}
        env.pop("BASH_ENV", None)
        return env

    def drop(self, root_mode, before="", after="", stdin="terminal", output="pipes", status=3):
        """`as_account BEFORE weaver-m1 WEAVER_X=1 <reporter> AFTER` from
        SCRIPT, run by bash whose controlling terminal is a pseudo-terminal
        and which holds one more descriptor, open on a file. STDIN is
        "terminal" or "pipe", the caller's own, holding a secret. OUTPUT is
        "pipes", the caller's two, "merged", one pipe for both as `2>&1`
        makes it, or "terminal". Returns the status, the report (None where
        the reporter did not run), the caller's standard output and standard
        error as they came (the terminal's in both for "terminal"), and the
        inodes of every descriptor the caller holds."""
        master, slave = os.openpty()
        held = os.open(str(self.extra), os.O_WRONLY | os.O_CREAT, 0o600)
        self.report.unlink(missing_ok=True)
        rc_file = Path(str(self.report) + ".rc")
        program = ("set -euo pipefail\n" + f"ROOT_OPERATOR={1 if root_mode else 0}\n"
                   + account_drop_source()
                   + f"rc=0; as_account {before} weaver-m1 WEAVER_X=1 {shlex.quote(str(self.reporter))} "
                   + f"{shlex.quote(str(self.report))} {status} {after} || rc=$?\n"
                   + f'echo "$rc" > {shlex.quote(str(rc_file))}\n')

        def controlling_terminal():
            os.setsid()
            fcntl.ioctl(slave, termios.TIOCSCTTY, 0)

        out = {"pipes": (subprocess.PIPE, subprocess.PIPE), "merged": (subprocess.PIPE, subprocess.STDOUT),
               "terminal": (slave, slave)}[output]
        try:
            proc = subprocess.Popen(["bash", "-c", program], env=self.env(self.bin_dir),
                                    stdin=slave if stdin == "terminal" else subprocess.PIPE,
                                    stdout=out[0], stderr=out[1], preexec_fn=controlling_terminal, pass_fds=(held,))
            caller = {os.fstat(slave).st_ino, os.fstat(held).st_ino}
            for stream in (proc.stdin, proc.stdout, proc.stderr):
                if stream is not None:
                    caller.add(os.fstat(stream.fileno()).st_ino)
            seen_out, seen_err = proc.communicate(
                input=b"the operator's secret\n" if proc.stdin is not None else None, timeout=30)
            seen_out, seen_err = seen_out or b"", seen_err or b""
        finally:
            os.close(slave)
            os.close(held)
        if output == "terminal":
            seen = b""
            while True:
                try:
                    chunk = os.read(master, 4096)
                except OSError:
                    break
                if not chunk:
                    break
                seen += chunk
            seen_out = seen_err = seen.replace(b"\r\n", b"\n")
        os.close(master)
        record = json.loads(self.report.read_text()) if self.report.exists() else None
        return int(rc_file.read_text()), record, seen_out.decode(), seen_err.decode(), caller

    def assert_handed_nothing(self, record, caller, stdin="/dev/null"):
        self.assertEqual(record["links"][0], stdin, "standard input")
        for n in (1, 2):
            # A relay's FIFO, unlinked before the command starts (#590), so
            # no name reaches it.
            self.assertTrue(record["fifo"][n], record["links"])
            self.assertTrue(record["links"][n].startswith("pipe:") or record["links"][n].endswith(" (deleted)"),
                            record["links"])
            self.assertNotIn(record["inodes"][n], caller, f"descriptor {n} is one the caller holds")
        self.assertEqual(record["extra"], [], "a descriptor above 2 reached the account")
        self.assertEqual(record["tty"], errno.ENXIO, "the command could open a controlling terminal")
        self.assertEqual(record["env"], {"PATH": self.drop_path, "WEAVER_X": "1"}, "its whole environment")
        self.assertFalse(self.tripped.exists(), "a runuser or sudo was found on the drop's PATH")

    def handed(self, tool):
        return json.loads((self.bin_dir / f"{tool}.json").read_text())

    def test_runuser_and_sudo_start_with_the_drops_path_alone(self):
        # **#590**: runuser or sudo itself is the first process of the
        # account's, and it is started under `env -i` with DROP_PATH alone, so
        # nothing of the caller's environment (here SECRET) is in its block,
        # and it is the one found on the caller's PATH before the clear, never
        # one on DROP_PATH. In root mode setsid runs before runuser, so
        # runuser holds no terminal. Under sudo the env runs after setsid, so
        # sudo keeps the terminal its credential is keyed by, and the account's
        # env starts with none. Perturbations: drop the outer `env -i`, look
        # runuser or sudo up after it, put setsid after runuser, or the env
        # back before timeout under sudo, and a case below fails.
        for root_mode, tool in ((True, "runuser"), (False, "sudo")):
            with self.subTest(root_mode=root_mode):
                rc, record, _, _, caller = self.drop(root_mode)
                self.assertEqual(rc, 3)
                self.assert_handed_nothing(record, caller)
                handed = self.handed(tool)
                self.assertEqual(handed["env"], {"PATH": self.drop_path}, "the drop's own block")
                command = ["env", "-i", "PATH=" + self.drop_path, "WEAVER_X=1", str(self.reporter), str(self.report), "3"]
                if root_mode:
                    self.assertEqual(handed["argv"], ["-u", "weaver-m1", "--", *command])
                    self.assertEqual(handed["tty"], errno.ENXIO, "runuser holds a terminal")
                else:
                    self.assertEqual(handed["argv"], ["-n", "-u", "weaver-m1", "timeout", "0", "setsid", "-w", *command])
                    self.assertEqual(handed["tty"], "opened", "sudo lost the terminal its credential is keyed by")

    def test_a_process_left_holding_the_output_is_cut_off(self):
        # **#590**: the relays drain for DROP_RELAY_WAIT seconds after the
        # command exits. A process it left behind still holding its output
        # then is cut off, and the call fails saying so, where before it hung
        # until that process let go. The bound is account-drop.sh's own, 5 s.
        # Perturbation: wait on the relays unbounded, and the case times out.
        pid_file = self.root / "left-pid"
        command = self.root / "leaves"
        command.write_text("#!/usr/bin/python3 -I\nimport os, sys, time\n"
                           "child = os.fork()\n"
                           "if child == 0:\n    time.sleep(60)\n    os._exit(0)\n"
                           f"open({str(pid_file)!r}, 'w').write(str(child))\n"
                           "print('started', flush=True)\n")
        command.chmod(0o755)
        for root_mode in (True, False):
            with self.subTest(root_mode=root_mode):
                pid_file.unlink(missing_ok=True)
                program = ("set -euo pipefail\n" + f"ROOT_OPERATOR={1 if root_mode else 0}\n" + account_drop_source()
                           + f'rc=0; as_account weaver-m1 {shlex.quote(str(command))} || rc=$?\necho "rc=$rc"\n')
                started = time.monotonic()
                try:
                    run = subprocess.run(["bash", "-c", program], env=self.env(self.bin_dir), stdin=subprocess.DEVNULL,
                                         text=True, capture_output=True, timeout=20)
                finally:
                    took = time.monotonic() - started
                    if pid_file.exists():
                        try:
                            os.kill(int(pid_file.read_text()), signal.SIGKILL)
                        except ProcessLookupError:
                            pass
                self.assertEqual(run.stdout, "started\nrc=1\n", run.stderr)
                self.assertIn("held open 5s past its exit by a process it left, so the call fails", run.stderr)
                self.assertGreater(took, 4.5)
                self.assertLess(took, 8)

    def test_a_relay_that_has_exited_is_not_held_open(self):
        # **#590**: bash can leave a relay that has exited unreaped through the
        # whole drain. Such a zombie is one `kill -0` answers for, so a relay
        # that had finished read as one held open and failed the call (the
        # quota test, about one run in ten, 2026-11-17). drop_relay_stands
        # reads /proc/PID/stat instead: a running child of the named parent
        # stands, and a zombie, a pid no longer there and a process of another
        # parent, as a reused number would be, do not. The state and the
        # parent are read after the last `)`, so a command name that holds
        # `) Z ` or `) S ` and spaces misleads nothing. Every relay check in
        # as_account is that one, with its own shell as the parent, and
        # `kill -0` is left for the drop alone, which `wait` reaps.
        # Perturbation: the check by `kill -0`, and the zombie and other
        # parent cases fail; a zombie that stands, the fields read after the
        # first `)` or by spaces from the start, or no parent check, and a case
        # fails; either relay check in as_account by `kill -0`, and the text
        # fails.
        def named(comm, then):
            # A child of this process that names itself COMM, then runs THEN.
            child = os.fork()
            if child == 0:
                try:
                    with open("/proc/self/comm", "w") as name:
                        name.write(comm)
                    then()
                finally:
                    os._exit(0)
            deadline = time.monotonic() + 5
            while (Path(f"/proc/{child}/comm").read_text() != comm + "\n"
                   and time.monotonic() < deadline):
                time.sleep(0.01)
            return child
        running = named("r) Z  (x", lambda: time.sleep(60))
        self.addCleanup(os.waitpid, running, 0)
        self.addCleanup(os.kill, running, signal.SIGKILL)
        zombie = named("z) S  (x", lambda: None)
        self.addCleanup(os.waitpid, zombie, 0)
        deadline = time.monotonic() + 5
        while self.alive(zombie) and time.monotonic() < deadline:
            time.sleep(0.01)
        self.assertTrue(Path(f"/proc/{zombie}").exists() and not self.alive(zombie), "the child is no zombie")
        gone = subprocess.Popen(["true"])
        gone.wait()
        me, other = str(os.getpid()), str(os.getppid())
        program = (account_drop_source()
                   + 'while [ "$#" -gt 0 ]; do\n'
                   + '  if drop_relay_stands "$1" "$2"; then said=stands; else said=gone; fi\n'
                   + '  if kill -0 "$1" 2>/dev/null; then signalled=yes; else signalled=no; fi\n'
                   + '  echo "$said $signalled"\n'
                   + '  shift 2\n'
                   + 'done\n')
        run = subprocess.run(["bash", "-c", program, "bash", str(running), me, str(running), other, str(zombie), me,
                              str(gone.pid), me],
                             env=self.env(self.bin_dir), stdin=subprocess.DEVNULL, text=True, capture_output=True,
                             timeout=20)
        # A process of another parent and the zombie answer `kill -0`, which is
        # the case's premise.
        self.assertEqual(run.stdout.splitlines(), ["stands yes", "gone yes", "gone yes", "gone no"], run.stderr)
        body = (DEPLOY / "account-drop.sh").read_text().split("\nas_account() {\n", 1)[1]
        self.assertEqual(set(re.findall(r'kill -0 \S+', body)), {'kill -0 "$drop"'})
        self.assertEqual(body.count("drop_relay_stands "), 3)
        self.assertEqual(re.findall(r'drop_relay_stands "\$(\w+)" "\$(\w+)"', body),
                         [("out_pid", "BASHPID"), ("err_pid", "BASHPID"), ("relay", "BASHPID")])

    @staticmethod
    def alive(pid):
        try:
            with open(f"/proc/{pid}/stat") as stat:
                return stat.read().rsplit(")", 1)[1].split()[0] != "Z"
        except FileNotFoundError:
            return False

    def test_nothing_of_the_caller_s_reaches_the_account(self):
        for root_mode in (True, False):
            for stdin in ("terminal", "pipe"):
                with self.subTest(root_mode=root_mode, stdin=stdin):
                    rc, record, out, err, caller = self.drop(root_mode, stdin=stdin)
                    self.assert_handed_nothing(record, caller)
                    self.assertEqual(record["stdin"], "")
                    self.assertNotEqual(record["inodes"][1], record["inodes"][2], "two pipes, two relays")
                    self.assertEqual((rc, out, err), (3, "out-a\nout-c\n", "err-b\n"))

    def test_a_named_file_is_the_one_input(self):
        for root_mode in (True, False):
            with self.subTest(root_mode=root_mode):
                rc, record, _, _, caller = self.drop(root_mode, before=f"--stdin {shlex.quote(str(self.feed))}")
                self.assert_handed_nothing(record, caller, stdin=str(self.feed))
                self.assertEqual((rc, record["stdin"]), (3, "the named file"))

    def test_one_file_for_both_outputs_is_one_pipe_in_its_order(self):
        # A caller's `2>&1`, or a terminal on both, is one relay, so what the
        # command wrote reaches it in the order written. Perturbation: drop
        # the one-pipe choice, and the two descriptors are two pipes.
        for root_mode in (True, False):
            for output in ("merged", "terminal"):
                with self.subTest(root_mode=root_mode, output=output):
                    rc, record, out, _, caller = self.drop(root_mode, output=output)
                    self.assert_handed_nothing(record, caller)
                    self.assertEqual(record["inodes"][1], record["inodes"][2], "one pipe for both")
                    self.assertEqual((rc, out), (3, "out-a\nerr-b\nout-c\n"))

    def test_the_status_is_the_command_s_else_a_relay_s(self):
        # A relay that cannot write fails the call where the command passed,
        # and never hides the command's own failure. Perturbation: return
        # either relay's status alone, or the command's alone, and a case
        # below fails.
        for root_mode in (True, False):
            for status, after, want in ((0, ">/dev/full", 1), (0, "2>/dev/full", 1), (3, ">/dev/full", 3),
                                        (0, ">/dev/full 2>&1", 1), (0, "", 0)):
                with self.subTest(root_mode=root_mode, status=status, after=after):
                    rc, record, _, _, _ = self.drop(root_mode, after=after, status=status)
                    self.assertIsNotNone(record, "the command ran")
                    self.assertEqual(rc, want)

    def test_an_interrupt_ends_the_script_and_the_account_s_command(self):
        # setsid moves the command out of the terminal's process group, so a
        # Ctrl-C or a Ctrl-\ reaches it only through as_account, which passes
        # the signal on as SIGTERM and then takes it itself: the script's own
        # trap runs, or, without one, the script ends by the signal, and the
        # command is gone. The stand-ins relay as runuser and sudo do. A
        # Ctrl-\ dumps the shells' cores, so the case keeps them off the disk.
        # Perturbations: drop the traps or the QUIT trap alone, drop
        # `timeout 0` in the operator's mode, or drop the signal taken again,
        # and a case below fails.
        relay_bin = self.root / "relay-bin"
        relay_bin.mkdir()
        relay = self.root / "relay"
        relay.write_text(self.RELAY)
        relay.chmod(0o755)
        for tool in ("runuser", "sudo"):
            (relay_bin / tool).symlink_to(relay)
        pid_file = self.root / "pid"
        command = self.root / "long"
        command.write_text("#!/usr/bin/python3 -I\nimport os, time\n"
                           f"open({str(pid_file)!r}, 'w').write(str(os.getpid()))\ntime.sleep(30)\n")
        command.chmod(0o755)
        cases = [(root_mode, sig, trap) for root_mode in (True, False)
                 for sig in (signal.SIGINT, signal.SIGQUIT) for trap in (False, True)]
        for root_mode, sig, trap in cases:
            with self.subTest(root_mode=root_mode, signal=sig.name, trap=trap):
                pid_file.unlink(missing_ok=True)
                program = ("set -euo pipefail\n" + f"ROOT_OPERATOR={1 if root_mode else 0}\n"
                           + (f"trap 'echo trapped >&2; exit {128 + sig}' {sig.name[3:]}\n" if trap else "")
                           + account_drop_source()
                           + f"as_account weaver-m1 {shlex.quote(str(command))} >/dev/null\necho AFTER\n")
                proc = subprocess.Popen(["bash", "-c", program], env=self.env(relay_bin),
                                        stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                                        stderr=subprocess.PIPE, start_new_session=True,
                                        preexec_fn=lambda: resource.setrlimit(resource.RLIMIT_CORE, (0, 0)))
                pid, took, out, err = None, None, b"", b""
                for _ in range(200):
                    if pid_file.exists() and pid_file.read_text():
                        pid = int(pid_file.read_text())
                        break
                    time.sleep(0.05)
                if pid is not None:
                    started = time.monotonic()
                    os.killpg(proc.pid, sig)
                    try:
                        out, err = proc.communicate(timeout=10)
                        took = time.monotonic() - started
                    except subprocess.TimeoutExpired:
                        pass
                    for _ in range(60):
                        if not self.alive(pid):
                            break
                        time.sleep(0.05)
                outlived = pid is not None and self.alive(pid)
                if outlived:
                    os.kill(pid, signal.SIGKILL)
                if proc.poll() is None:
                    os.killpg(proc.pid, signal.SIGKILL)
                    proc.communicate()
                self.assertIsNotNone(pid, "the command started")
                self.assertFalse(outlived, "the account's command outlived the interrupt")
                self.assertIsNotNone(took, "the script still ran 10 s after the interrupt")
                self.assertLess(took, 5)
                self.assertNotIn(b"AFTER", out, "the script ran on past the interrupt")
                # bash ignores SIGQUIT in every case, so without a trap a
                # Ctrl-\ ends the script by `set -e` on as_account's 131, as
                # when it reached the command directly.
                self.assertEqual(proc.returncode, 128 + sig if trap or sig == signal.SIGQUIT else -sig, err)
                if trap:
                    self.assertIn(b"trapped", err, "the script's own trap ran")

    def test_without_proc_nothing_runs(self):
        # **Fail closed** (the Planner's ruling of 2026-11-17): where
        # /proc/self/fd cannot be read, the descriptors this shell holds could
        # not be closed, so as_account refuses before anything runs. An empty
        # tmpfs covers /proc inside a user and mount namespace. Perturbation:
        # drop the check, and the command runs.
        if subprocess.run(["unshare", "--map-root-user", "true"], capture_output=True).returncode != 0:
            self.skipTest("unshare --map-root-user is not available")
        marker = self.root / "ran"
        for root_mode in (True, False):
            with self.subTest(root_mode=root_mode):
                marker.unlink(missing_ok=True)
                command = shlex.quote("echo ran > " + shlex.quote(str(marker)))
                program = ("set -euo pipefail\n" + f"ROOT_OPERATOR={1 if root_mode else 0}\n"
                           + account_drop_source()
                           + f'rc=0; as_account weaver-m1 /bin/sh -c {command} || rc=$?\necho "rc=$rc"\n')
                run = subprocess.run(["unshare", "--map-root-user", "--mount", "bash", "-c",
                                      'mount -t tmpfs none /proc && exec bash -c "$1"', "bash", program],
                                     env=self.env(self.bin_dir), stdin=subprocess.DEVNULL, text=True,
                                     capture_output=True, timeout=20)
                self.assertEqual((run.returncode, run.stdout), (0, "rc=1\n"), run.stderr)
                self.assertIn("as_account: no command runs as weaver-m1, since /proc/self/fd cannot be read",
                              run.stderr)
                self.assertFalse(marker.exists(), "the command ran")

    def test_turn_py_reaches_the_account_on_its_named_input(self):
        # turn_as hands turn.py over through --stdin, which this shell opens.
        # Perturbation: hand it on the call's own standard input, and the
        # account reads /dev/null.
        script = (DEPLOY / "verify-lifecycle.sh").read_text()
        (self.root / "deploy").mkdir()
        (self.root / "deploy" / "turn.py").write_text("the turn's source\n")
        (self.drop_bin / "python3").write_text("#!/bin/sh\nexec /bin/cat\n")
        (self.drop_bin / "python3").chmod(0o755)
        root = shlex.quote(str(self.root))
        for root_mode in (True, False):
            with self.subTest(root_mode=root_mode):
                program = ("set -euo pipefail\n" + f"ROOT_OPERATOR={1 if root_mode else 0}\n"
                           + f"A=m1; ADMIN_BASE=/base; CHECKOUT={root}; LOGDIR={root}\n"
                           + lifecycle_functions(script, "turn_as")
                           + 'turn_as weaver-gatecon hello\nprintf "%s|%s" "$TURN_RC" "$TURN_SAID"\n')
                run = subprocess.run(["bash", "-c", program], env=self.env(self.bin_dir), stdin=subprocess.DEVNULL,
                                     text=True, capture_output=True, timeout=20)
                self.assertEqual((run.returncode, run.stdout), (0, "0|the turn's source"), run.stderr)

    def test_every_other_drop_names_its_input_and_output(self):
        # **The class, swept** (B3): a drop to another account written outside
        # as_account, in any deploy script and in any spelling, reads its
        # standard input from /dev/null or a file and sends its output to a
        # capture, /dev/null or a file. The two `set --` lines are
        # as_account's own. Since #590 no such drop is left but
        # decommission.sh's as_op, which is root becoming the operator, whose
        # own account holds sudo, so it crosses to no account of the agent's.
        # Perturbation: write a drop outside the helper again, with no stdin
        # redirect, and this names it.
        drop = re.compile(r"\bsudo\b[^#]*\s(-[A-Za-z]*u|--user)\b|\brunuser\b[^#]*\s-u\b")
        exempt = {"decommission.sh": 'as_op() { sudo -u "$OPERATOR" -H "$@"; }'}
        found = []
        for script in sorted(DEPLOY.glob("*.sh")):
            for n, line in enumerate(script.read_text().splitlines(), 1):
                if line.lstrip().startswith("#") or not drop.search(line) or line.lstrip().startswith("set -- "):
                    continue
                if exempt.get(script.name) == line.strip():
                    found.append(f"{script.name}:{n} (exempt)")
                    continue
                found.append(f"{script.name}:{n}")
                self.assertRegex(line, r'<\s*(/dev/null|"\$)', f"{script.name}:{n} names no standard input")
                self.assertRegex(line, r'\$\(|>\s*/dev/null', f"{script.name}:{n} may hold a terminal on its output")
        self.assertEqual(len(found), 1, found)
        self.assertEqual(sum(f.endswith("(exempt)") for f in found), 1, found)

    def test_verify_load_refuses_without_runuser_or_setsid(self):
        # **#590**: verify-load.sh's reads as the agent's accounts go through
        # runuser and discard their own refusal, so the script refuses by name
        # where runuser or setsid is not on PATH. Perturbation: test for one
        # of the two alone, and a case below runs on.
        text = (DEPLOY / "verify-load.sh").read_text()
        start = text.index("command -v runuser >/dev/null && command -v setsid >/dev/null")
        check = text[start:text.index("\n", text.index("|| die", start)) + 1]
        for present in ((), ("runuser",), ("setsid",), ("runuser", "setsid")):
            with self.subTest(present=present):
                path = self.root / ("path-" + "-".join(present))
                path.mkdir()
                if "runuser" in present:
                    (path / "runuser").symlink_to(self.bin_dir / "runuser")
                if "setsid" in present:
                    (path / "setsid").symlink_to(shutil.which("setsid", path=BOX_PATH))
                program = 'die() { echo "REFUSED: $*" >&2; exit 1; }\n' + check + "echo RUNS-ON\n"
                run = subprocess.run([_GUARD_BASH, "-c", program], env={"PATH": str(path)}, text=True,
                                     capture_output=True, timeout=20)
                if len(present) == 2:
                    self.assertEqual((run.returncode, run.stdout), (0, "RUNS-ON\n"), run.stderr)
                else:
                    self.assertEqual(run.returncode, 1, run.stdout)
                    self.assertIn("REFUSED: runuser or setsid is not on PATH", run.stderr)

    def test_lifecycle_root_mode_refuses_without_sudo_for_step_1s_listing(self):
        # **#590**: root mode asks root's sudo for one read, step 1's listing
        # of the admin-con rule, and probes nothing of it, so --apply refuses
        # by name where sudo is not on PATH. Perturbation: drop the check, and
        # the case without sudo runs on.
        text = (DEPLOY / "verify-lifecycle.sh").read_text()
        start = text.index("    command -v sudo >/dev/null \\\n")
        end = text.index("the box's sudo is the one that rule is written for\"\n", start)
        check = text[start:text.index("\n", end) + 1]
        for present in (False, True):
            with self.subTest(sudo=present):
                path = self.root / f"path-sudo-{present}"
                path.mkdir()
                if present:
                    (path / "sudo").write_text("#!/bin/sh\nexit 97\n")
                    (path / "sudo").chmod(0o755)
                program = 'die() { echo "REFUSED: $*" >&2; exit 1; }\n' + check + "echo RUNS-ON\n"
                run = subprocess.run([_GUARD_BASH, "-c", program], env={"PATH": str(path)}, text=True,
                                     capture_output=True, timeout=20)
                if present:
                    self.assertEqual((run.returncode, run.stdout), (0, "RUNS-ON\n"), run.stderr)
                else:
                    self.assertEqual(run.returncode, 1, run.stdout)
                    self.assertIn("REFUSED: sudo is not on PATH", run.stderr)

    def test_every_script_that_drops_sources_the_one_helper(self):
        # **One helper** (#590, in place of two copies held equal): each
        # script that runs a command as an agent's account sources
        # account-drop.sh once and defines neither as_account nor DROP_PATH of
        # its own, and the helper's PATH and VAR=value rule are the ones the
        # double reads by. Perturbation: define either in one script, or
        # change either in the helper, and this fails.
        helper = (DEPLOY / "account-drop.sh").read_text()
        self.assertEqual([l for l in helper.splitlines() if l.startswith("DROP_PATH=")], [f"DROP_PATH={DROP_PATH}"])
        self.assertIn(f'while [[ "${{1:-}}" =~ {DROP_NAMED} ]]; do named+=("$1"); shift; done',
                      shell_function(helper, "as_account"))
        for name in ("create-agent.sh", "verify-lifecycle.sh", "verify-load.sh", "update-stack.sh"):
            text = (DEPLOY / name).read_text()
            self.assertEqual(len(re.findall(r'^source ".*/account-drop\.sh"$', text, re.M)), 1, name)
            self.assertNotRegex(text, r"(?m)^\s*(as_account\(\)|DROP_PATH=)", name)


def files_held(files):
    """PRIVILEGED_FILES held at FILES for a test's block: a start
    directory's change does not read them again, no directory's reading
    from before the block or during it is kept, and no copy guard_copy
    makes during the block outlives it, so a box program FILES names never
    stays refused. A copy that stood before the block stays, and any other
    file in the guard's directory is left as it is."""
    from unittest import mock
    module = sys.modules[__name__]
    before = set(os.listdir(STAND_IN_GUARD))

    def drop_copies():
        for copy in [c for c in _COPIES_MADE if c.parent == STAND_IN_GUARD and c.name not in before]:
            copy.unlink(missing_ok=True)
            _COPIES_MADE.discard(copy)

    held = contextlib.ExitStack()
    held.callback(_MEMBERS_READ.clear)
    held.callback(drop_copies)
    held.enter_context(mock.patch.object(module, "PRIVILEGED_FILES", files))
    held.enter_context(mock.patch.object(module, "_refresh_files", lambda: None))
    _MEMBERS_READ.clear()
    return held


class StandInGuardTests(unittest.TestCase):
    """**No test reaches the box's own sudo** (B3, the Planner's ruling of
    2026-11-17). Before this process starts a shell or a deploy script, the
    PATH it hands over, and any DROP_PATH, is read with `command -v`: sudo,
    runuser, setsid, env and timeout each resolve inside the tests'
    temporary root, where the stand-ins are, or not at all, and the test is
    refused before anything runs otherwise. The suite's own PATH carries a
    guard directory ahead of the box's. Perturbation: let the check pass
    everything, and the first case runs its command."""

    def test_a_path_that_reaches_the_box_first_is_refused_before_anything_runs(self):
        with tempfile.TemporaryDirectory(prefix="weaver-guard-") as scratch:
            marker = Path(scratch) / "ran"
            # A stand-in directory that links the box's own sudo.
            linked = Path(scratch) / "linked"
            linked.mkdir()
            (linked / "sudo").symlink_to(shutil.which("sudo", path=BOX_PATH) or "/usr/bin/sudo")
            program = f"echo ran > {shlex.quote(str(marker))}"
            for env in ({**os.environ, "PATH": "/usr/bin" + os.pathsep + os.environ["PATH"]},
                        {**os.environ, "DROP_PATH": "/usr/bin:/bin"},
                        {**os.environ, "PATH": BOX_PATH},
                        {**os.environ, "PATH": str(linked) + os.pathsep + os.environ["PATH"]}):
                with self.subTest(path=env["PATH"].split(os.pathsep)[0], drop=env.get("DROP_PATH")):
                    with self.assertRaises(StandInGuardError) as refused:
                        subprocess.run(["bash", "-c", program], env=env, timeout=20)
                    self.assertRegex(str(refused.exception), r"resolves (sudo|runuser|setsid|env|timeout) to /")
                    self.assertFalse(marker.exists(), "nothing ran")
            # A wrapper in front of the shell, or a PATH an `env` wrapper
            # hands on, is read the same way.
            for argv in (["timeout", "20", "bash", "-c", program],
                         ["env", "PATH=/usr/bin" + os.pathsep + os.environ["PATH"], "bash", "-c", program]):
                with self.subTest(argv=argv[:2]):
                    with self.assertRaises(StandInGuardError):
                        subprocess.run(argv, env={**os.environ, "PATH": BOX_PATH} if argv[0] == "timeout" else None,
                                       timeout=20)
                    self.assertFalse(marker.exists(), "nothing ran")
            subprocess.run(["bash", "-c", program], env=dict(os.environ), timeout=20, check=True)
            self.assertTrue(marker.exists(), "the suite's own PATH passes")

    def test_a_shell_handed_a_socket_or_a_foreign_bash_env_is_refused_before_anything_runs(self):
        """Perturbation: let guard_shell_files pass everything, and the socket
        cases run their command. Take out the /dev/null default, and a run
        that names no input, made while this process's own input is a socket,
        is refused."""
        import socket
        with tempfile.TemporaryDirectory(prefix="weaver-guard-") as scratch:
            marker = Path(scratch) / "ran"
            program = f"echo ran > {shlex.quote(str(marker))}"
            near, far = socket.socketpair()
            with near, far:
                for given in (near, near.fileno()):
                    with self.subTest(stdin=type(given).__name__):
                        with self.assertRaises(StandInGuardError) as refused:
                            subprocess.run(["bash", "-c", program], stdin=given, env=dict(os.environ), timeout=20)
                        self.assertIn("a socket as its standard input", str(refused.exception))
                        self.assertFalse(marker.exists(), "nothing ran")
                with self.subTest(bash_env="/etc/profile"):
                    with self.assertRaises(StandInGuardError) as refused:
                        subprocess.run(["bash", "-c", program], env={**os.environ, "BASH_ENV": "/etc/profile"},
                                       timeout=20)
                    self.assertIn("BASH_ENV /etc/profile", str(refused.exception))
                    self.assertFalse(marker.exists(), "nothing ran")
                # **A run that names no input reads /dev/null**, so this
                # process's own input being a socket reaches no shell.
                held = os.dup(0)
                try:
                    os.dup2(far.fileno(), 0)
                    run = subprocess.run(["bash", "-c", "readlink /proc/self/fd/0"], env=dict(os.environ),
                                         text=True, capture_output=True, timeout=20)
                finally:
                    os.dup2(held, 0)
                    os.close(held)
                self.assertEqual(run.stdout.strip(), "/dev/null", run.stderr)
            subprocess.run(["bash", "-c", program], env=dict(os.environ), timeout=20, check=True)
            self.assertTrue(marker.exists(), "a run that names no input passes")

    def test_the_guard_s_sudo_and_runuser_refuse(self):
        # **A guard of its own**, made as the suite's is, so these reaches
        # stand in its own record and never in the suite's, which fails the
        # run. Each reach refuses with 97 and appends its name, the running
        # test and its arguments. Perturbation: drop the record's write, and
        # the record stays empty.
        with tempfile.TemporaryDirectory() as scratch:
            guard = Path(scratch) / "guard"
            guard.mkdir()
            record = Path(scratch) / "record"
            make_guard(guard, record)
            for name in ("sudo", "runuser"):
                with self.subTest(name=name):
                    run = subprocess.run(["bash", "-c", f"{name} -n true"],
                                         env={**os.environ, "PATH": str(guard) + os.pathsep + BOX_PATH}, text=True,
                                         capture_output=True, timeout=20)
                    self.assertEqual(run.returncode, 97, run.stderr)
                    self.assertIn(f"a test reached {name} outside its stand-ins", run.stderr)
            test = os.environ.get("WEAVER_TEST_ID", "unknown")
            self.assertEqual(record.read_text(), f"sudo\t{test}\t-n true\0runuser\t{test}\t-n true\0")

    def test_the_guard_refuses_and_records_each_privileged_program(self):
        """**The programs no test reaches** (the Planner's rulings of
        2026-11-18): the guard holds a refusing copy of each named program,
        of each qemu-*, virt-* and nvidia-* program the box carries, and of
        each name whose file is a named program's (poweroff is systemctl's),
        read at the start, and the suite's own PATH finds the guard's first.
        The box's members are read here by resolving links, apart from the
        code's own test. Each copy refuses with 97 and records its reach. A
        copy is run by its own path, never by a name a PATH resolves, so a
        copy missing fails here and reaches nothing. Perturbations: drop the
        set from make_guard, read no directory at the start, or drop the
        file rule, and this fails."""
        box = os.pathsep.join(d for d in _BOX_BINS if os.path.isdir(d))
        files = {os.path.realpath(f) for n in PRIVILEGED for f in [shutil.which(n, path=box)] if f}
        carried = {e.name for d in box.split(os.pathsep) for e in os.scandir(d)
                   if e.name.startswith(("qemu-", "virt-", "nvidia-")) or os.path.realpath(e.path) in files}
        self.assertLessEqual(set(PRIVILEGED) | carried, set(PRIVILEGED_NAMES))
        for name in PRIVILEGED_NAMES:
            self.assertEqual(shutil.which(name, path=os.environ["PATH"]), str(STAND_IN_GUARD / name), name)
        with tempfile.TemporaryDirectory() as scratch:
            guard = Path(scratch) / "guard"
            guard.mkdir()
            record = Path(scratch) / "record"
            make_guard(guard, record)
            for name in PRIVILEGED_NAMES:
                with self.subTest(name=name):
                    run = subprocess.run([str(guard / name), "--probe", name], text=True, capture_output=True,
                                         timeout=20)
                    self.assertEqual(run.returncode, 97, run.stderr)
                    self.assertIn(f"a test reached {name} outside its stand-ins", run.stderr)
            test = os.environ.get("WEAVER_TEST_ID", "unknown")
            self.assertEqual(reaches_in(record), [(name, test, f"--probe {name}") for name in PRIVILEGED_NAMES])

    def test_a_path_that_finds_a_privileged_program_outside_is_refused_before_anything_runs(self):
        """A PATH a test hands over that finds a privileged program outside
        the tests' temporary root is refused before anything runs: one whose
        box tail carries the program, a stand-in directory that links one to
        a program outside, a made-up virt-newtool, qemu-newtool or
        nvidia-newtool that no list names, caught by its family's pattern, a
        name no list holds whose file is a named program's, and a relative
        entry, read where the child starts. A stand-in of a test's own making
        passes. The box cases are read with `command -v` alone, and the
        made-up programs link `true`, so nothing privileged runs if the check
        is gone. For the file rule, `true`'s file stands for a named
        program's. Perturbations: resolve the stand-in names alone, read no
        family or no file in the handed PATH, or read it where the runner
        stands, and a case passes."""
        true = shutil.which("true", path=os.pathsep.join(_BOX_BINS))
        with tempfile.TemporaryDirectory(prefix="weaver-guard-") as scratch:
            marker = Path(scratch) / "ran"
            program = f"echo ran > {shlex.quote(str(marker))}"

            def stand_ins(name, without=(), linked=()):
                where = Path(scratch) / name
                where.mkdir()
                for n in STAND_IN_NAMES + PRIVILEGED:
                    if n not in without and n not in linked:
                        (where / n).write_text("#!/bin/sh\nexit 0\n")
                        (where / n).chmod(0o755)
                for n in linked:
                    (where / n).symlink_to(true)
                return where

            box = "/usr/bin" + os.pathsep + "/bin"
            for name in PRIVILEGED:
                if shutil.which(name, path=box):
                    with self.subTest(box=name):
                        path = str(stand_ins(f"without-{name}", without=(name,))) + os.pathsep + box
                        with self.assertRaises(StandInGuardError) as refused:
                            guard_path(path, "PATH")
                        self.assertIn(f"resolves {name} to /", str(refused.exception))
            for name in ("systemctl", "virt-newtool", "qemu-newtool", "nvidia-newtool"):
                with self.subTest(linked=name):
                    path = str(stand_ins(f"linked-{name}", linked=(name,)))
                    with self.assertRaises(StandInGuardError) as refused:
                        subprocess.run([_GUARD_BASH, "-c", program], env={**os.environ, "PATH": path}, timeout=20)
                    self.assertIn(f"resolves {name} to {path}/{name}", str(refused.exception))
                    self.assertFalse(marker.exists(), "nothing ran")
            with self.subTest(by_its_file="an-other-name"), files_held(PRIVILEGED_FILES | {_file_of(true)}):
                path = str(stand_ins("by-its-file", linked=("an-other-name",)))
                with self.assertRaises(StandInGuardError) as refused:
                    subprocess.run([_GUARD_BASH, "-c", program], env={**os.environ, "PATH": path}, timeout=20)
                self.assertIn(f"resolves an-other-name to {path}/an-other-name", str(refused.exception))
                self.assertFalse(marker.exists(), "nothing ran")
            with self.subTest(relative="linked-systemctl"):
                with self.assertRaises(StandInGuardError) as refused:
                    subprocess.run([_GUARD_BASH, "-c", program], env={**os.environ, "PATH": "linked-systemctl"},
                                   cwd=scratch, timeout=20)
                self.assertIn(f"resolves systemctl to {scratch}/linked-systemctl/systemctl", str(refused.exception))
                self.assertFalse(marker.exists(), "nothing ran")
            path = str(stand_ins("own"))
            subprocess.run([_GUARD_BASH, "-c", program], env={**os.environ, "PATH": path}, timeout=20, check=True)
            self.assertTrue(marker.exists(), "a test's own stand-ins pass")

    def test_a_child_that_names_a_privileged_program_is_refused_before_it_runs(self):
        """A child that runs no shell is refused before it starts when it
        would run a privileged program outside the tests' temporary root: by
        a bare name its PATH finds, alone or under a wrapper, by an absolute
        or a relative path, as its executable, by a name whose file is a
        named program's, by a PATH= its own `env` wrapper assigns, or any
        wrapper that takes an assignment does (sudo does, and `true` stands
        in), by the default PATH an `env` wrapper leaves when it clears the
        environment or PATH in any spelling env reads, after an option that
        takes an argument too (the links stand in for that PATH, and the
        refusal names it), through a nested `env`, through `env` as the
        executable under another argv[0], as a child that runs code but no
        shell, and as a shell=True child. An `env` option this guard does
        not read (`-S`, `-C`, `--env0-from`, or one env lacks) refuses the
        line. Stand-ins of a test's own making, a box program
        that is not privileged, an `env -i` that assigns its own PATH, and a
        `-i` that is another program's option pass. The privileged programs
        here are made-up family members, and `true`'s file standing for a
        named program's, each a link to `true`, so nothing privileged runs
        if a check is gone. Perturbations: drop the command line's read, its
        reading of env's options, the read of a PATH= in the line, the read
        of a PATH handed to a child that is not a shell, the executable's
        read in argv[0]'s place, or the refusal of an option, and a case
        runs. Read a `-i` anywhere in the line, as an earlier guard
        did, and grep's is refused."""
        from unittest import mock
        true = shutil.which("true", path=os.pathsep.join(_BOX_BINS))
        timeout = shutil.which("timeout", path=os.pathsep.join(_BOX_BINS))
        with tempfile.TemporaryDirectory(prefix="weaver-guard-") as scratch:
            links = Path(scratch) / "links"
            links.mkdir()
            for n in ("virt-newtool", "qemu-newtool", "nvidia-newtool", "an-other-name"):
                (links / n).symlink_to(true)
            own = Path(scratch) / "own"
            own.mkdir()
            (own / "virt-own").write_text("#!/bin/sh\nexit 0\n")
            (own / "virt-own").chmod(0o755)
            cases = {"bare": (["virt-newtool"], {"env": {"PATH": str(links)}}),
                     "path": ([str(links / "qemu-newtool")], {}),
                     "relative": (["links/qemu-newtool"], {"cwd": scratch}),
                     "wrapped": ([timeout, "5", "nvidia-newtool"], {"env": {"PATH": str(links)}}),
                     "executable": (["anything"], {"executable": str(links / "virt-newtool")})}
            for case, (argv, kwargs) in cases.items():
                with self.subTest(case=case):
                    with self.assertRaises(StandInGuardError) as refused:
                        subprocess.run(argv, timeout=20, **kwargs)
                    self.assertIn("refused before it runs", str(refused.exception))
            with self.subTest(case="by-its-file"), files_held(PRIVILEGED_FILES | {_file_of(true)}):
                with self.assertRaises(StandInGuardError):
                    subprocess.run(["an-other-name"], env={"PATH": str(links)}, timeout=20)
            env_program = shutil.which("env", path=os.pathsep.join(_BOX_BINS))
            refused_lines = {
                "env-assigns-PATH": ([env_program, f"PATH={links}", "virt-newtool"], {}),
                "env-clears-the-environment": ([env_program, "-i", "virt-newtool"], {}),
                "env-unsets-PATH": ([env_program, "-u", "PATH", "virt-newtool"], {}),
                "env-dash": ([env_program, "-", "virt-newtool"], {}),
                "env-unsets-PATH-in-two-words": ([env_program, "--unset", "PATH", "virt-newtool"], {}),
                "env-groups-its-options": ([env_program, "-iv", "virt-newtool"], {}),
                "env-shortens-an-option": ([env_program, "--ignore-env", "virt-newtool"], {}),
                "env-nested": ([env_program, f"PATH={own}", env_program, "-u", "PATH", "virt-newtool"], {}),
                "env-takes-an-argument": ([env_program, "-a", "x", "-i", "virt-newtool"], {}),
                "env-as-the-executable": (["x", "-i", "virt-newtool"], {"executable": env_program}),
                "a-wrapper-assigns-PATH": ([true, f"PATH={links}", "virt-newtool"], {}),
                "a-child-that-runs-code": ([sys.executable, "-c", "pass"], {"env": {"PATH": str(links)}}),
                "shell-true": ("virt-newtool", {"shell": True, "env": {"PATH": str(links)}})}
            grep = shutil.which("grep", path=os.pathsep.join(_BOX_BINS))
            with mock.patch.object(os, "defpath", str(links)):
                for case, (argv, kwargs) in refused_lines.items():
                    with self.subTest(case=case):
                        with self.assertRaises(StandInGuardError) as refused:
                            subprocess.run(argv, timeout=20, **kwargs)
                        self.assertIn(str(links), str(refused.exception))
                        if case == "env-clears-the-environment":
                            self.assertIn("the default one an `env` wrapper's -i", str(refused.exception))
                refused_options = {
                    "env-splits-a-string": ([env_program, "-S", "-i virt-newtool"], "-S, which splits"),
                    "env-splits-a-string-shortened": ([env_program, "--split", "-i virt-newtool"], "-S, which splits"),
                    "env-changes-directory": ([env_program, "-C", scratch, "links/virt-newtool"], "-C, which starts"),
                    "env-reads-a-file": ([env_program, "--env0-from", os.devnull, "virt-newtool"],
                                         "--env0-from, which reads"),
                    "env-lacks-the-option": ([env_program, "--no-such-option", "-i", "virt-newtool"],
                                             "--no-such-option, not one of its options")}
                for case, (argv, reason) in refused_options.items():
                    with self.subTest(case=case):
                        with self.assertRaises(StandInGuardError) as refused:
                            subprocess.run(argv, timeout=20)
                        self.assertIn(reason, str(refused.exception))
                with self.subTest(passes="grep-ignores-case"):
                    self.assertEqual(subprocess.run([grep, "-i", "virt-newtool", os.devnull], timeout=20).returncode, 1)
                with self.subTest(passes="env-assigns-its-own-PATH"):
                    subprocess.run([env_program, "-i", f"PATH={own}", "virt-own"], timeout=20, check=True)
            subprocess.run(["virt-own"], env={"PATH": str(own)}, timeout=20, check=True)
            subprocess.run([str(own / "virt-own")], timeout=20, check=True)
            subprocess.run([true], timeout=20, check=True)

    def test_a_name_is_privileged_by_its_file_by_a_link_or_a_hard_link(self):
        """privileged()'s third clause: a name no list holds is privileged
        when its file is a named program's, reached by a symbolic link or
        by a hard link, and not by a copy of the same bytes. A file under
        the temporary root stands for the named program's. Perturbation:
        drop the file rule, and the links pass."""
        from unittest import mock
        with tempfile.TemporaryDirectory() as scratch:
            named = Path(scratch) / "named"
            named.write_text("#!/bin/sh\nexit 0\n")
            (Path(scratch) / "by-symlink").symlink_to(named)
            os.link(named, Path(scratch) / "by-hard-link")
            shutil.copyfile(named, Path(scratch) / "by-copy")
            with mock.patch.object(sys.modules[__name__], "PRIVILEGED_FILES", frozenset({_file_of(named)})):
                self.assertTrue(privileged("by-symlink", Path(scratch) / "by-symlink"))
                self.assertTrue(privileged("by-hard-link", Path(scratch) / "by-hard-link"))
                self.assertFalse(privileged("by-copy", Path(scratch) / "by-copy"))
                self.assertFalse(privileged("by-symlink"), "without a path, the name alone decides")

    def test_a_program_installed_during_the_run_is_covered(self):
        """A directory's reading is kept once its last change is two seconds
        old, and the directory is read again whenever it changes, so a
        program installed during a run joins the set. A change within one
        tick of the clock leaves the time as it was, which is why a recent
        reading is never kept: both are shown by stamping the directory's
        time. A directory dated ahead of the clock (a skewed file server) is
        not kept either, so a program installed in the same tick of that
        clock is read. Perturbations: keep the first reading, keep a recent
        one, or keep one dated ahead, and the new program is missed."""
        with tempfile.TemporaryDirectory() as scratch:
            box = Path(scratch) / "box"
            box.mkdir()
            real = os.path.realpath(box)
            try:
                old = time.time_ns() - 10_000_000_000
                os.utime(box, ns=(old, old))
                self.assertEqual(_members_of(str(box)), ())
                self.assertIn(real, _MEMBERS_READ, "a reading two seconds old is kept")
                (box / "virt-late").write_text("#!/bin/sh\nexit 0\n")
                os.utime(box, ns=(old, old + 1_000_000_000))
                self.assertEqual(_members_of(str(box)), ("virt-late",))
                _MEMBERS_READ.pop(real, None)
                now = time.time_ns()
                os.utime(box, ns=(now, now))
                self.assertEqual(_members_of(str(box)), ("virt-late",))
                self.assertNotIn(real, _MEMBERS_READ, "a recent reading is not kept")
                (box / "qemu-same-tick").write_text("#!/bin/sh\nexit 0\n")
                os.utime(box, ns=(now, now))
                self.assertEqual(_members_of(str(box)), ("qemu-same-tick", "virt-late"))
                ahead = time.time_ns() + 3_600_000_000_000
                os.utime(box, ns=(ahead, ahead))
                self.assertEqual(_members_of(str(box)), ("qemu-same-tick", "virt-late"))
                self.assertNotIn(real, _MEMBERS_READ, "a reading dated ahead of the clock is not kept")
                (box / "nvidia-ahead").write_text("#!/bin/sh\nexit 0\n")
                os.utime(box, ns=(ahead, ahead))
                self.assertEqual(_members_of(str(box)), ("nvidia-ahead", "qemu-same-tick", "virt-late"))
            finally:
                _MEMBERS_READ.pop(real, None)

    def test_a_named_programs_files_are_read_again_when_a_start_directory_changes(self):
        """An upgrade during a run replaces a named program's file, and its
        other names must stay in the set: the files are read again when a
        start directory changes. A start directory under the scratch stands
        in, with a systemctl replaced by a rename and an alias that names
        it. The key is kept only once the directory's last change is two
        seconds old, so a file replaced within one tick of the clock, which
        leaves the time as it was, is read too: shown by stamping the
        directory's time. A key dated ahead of the clock is not kept either.
        Perturbations: read the files once, keep a recent key, or keep one
        dated ahead, and the alias drops out."""
        from unittest import mock
        with tempfile.TemporaryDirectory() as scratch:
            box = Path(scratch) / "box"
            box.mkdir()
            (box / "systemctl").write_text("#!/bin/sh\nexit 0\n")
            (box / "an-alias").symlink_to("systemctl")
            module = sys.modules[__name__]
            with mock.patch.object(module, "_START_DIRS", (str(box),)), \
                    mock.patch.object(module, "PRIVILEGED_FILES", frozenset()), \
                    mock.patch.object(module, "_FILES_READ", [None]):
                old = time.time_ns() - 10_000_000_000
                os.utime(box, ns=(old, old))
                _refresh_files()
                self.assertTrue(privileged("an-alias", box / "an-alias"))
                self.assertEqual(module._FILES_READ[0], (old,), "a key two seconds old is kept")
                (box / "systemctl.new").write_text("#!/bin/sh\nexit 1\n")
                os.rename(box / "systemctl.new", box / "systemctl")
                os.utime(box, ns=(old + 1_000_000_000, old + 1_000_000_000))
                _refresh_files()
                self.assertTrue(privileged("an-alias", box / "an-alias"), "the alias of the new file")
                now = time.time_ns()
                os.utime(box, ns=(now, now))
                _refresh_files()
                self.assertIsNone(module._FILES_READ[0], "a recent key is not kept")
                (box / "systemctl.new").write_text("#!/bin/sh\nexit 2\n")
                os.rename(box / "systemctl.new", box / "systemctl")
                os.utime(box, ns=(now, now))
                _refresh_files()
                self.assertTrue(privileged("an-alias", box / "an-alias"), "the alias of a file replaced in one tick")
                ahead = time.time_ns() + 3_600_000_000_000
                os.utime(box, ns=(ahead, ahead))
                _refresh_files()
                self.assertIsNone(module._FILES_READ[0], "a key dated ahead of the clock is not kept")
                (box / "systemctl.new").write_text("#!/bin/sh\nexit 3\n")
                os.rename(box / "systemctl.new", box / "systemctl")
                os.utime(box, ns=(ahead, ahead))
                _refresh_files()
                self.assertTrue(privileged("an-alias", box / "an-alias"), "the alias of a file replaced ahead")
            _MEMBERS_READ.clear()
            _refresh_files()

    def test_a_directory_of_links_into_a_start_directory_is_read_again_when_it_changes(self):
        """A directory of links into a start directory, a view, keeps its own
        time when a target there is replaced, so every directory's reading
        is dropped when a start directory changes: a program replaced by a
        link to a named program's file joins the view's members though the
        named programs' files are as they were. Perturbation: drop the
        readings only when the files change, and the replaced program is
        missed."""
        from unittest import mock
        with tempfile.TemporaryDirectory() as scratch:
            box, view = Path(scratch) / "box", Path(scratch) / "view"
            box.mkdir()
            view.mkdir()
            (box / "systemctl").write_text("#!/bin/sh\nexit 0\n")
            (box / "a-program").write_text("#!/bin/sh\nexit 0\n")
            (view / "a-program").symlink_to(box / "a-program")
            old = time.time_ns() - 10_000_000_000
            for d in (box, view):
                os.utime(d, ns=(old, old))
            module = sys.modules[__name__]
            try:
                with mock.patch.object(module, "_START_DIRS", (str(box),)), \
                        mock.patch.object(module, "PRIVILEGED_FILES", frozenset()), \
                        mock.patch.object(module, "_FILES_READ", [None]):
                    _refresh_files()
                    self.assertEqual(_members_of(str(view)), ())
                    self.assertIn(os.path.realpath(view), _MEMBERS_READ, "the view's reading is kept")
                    (box / "a-program.new").symlink_to("systemctl")
                    os.rename(box / "a-program.new", box / "a-program")
                    os.utime(box, ns=(old + 1_000_000_000, old + 1_000_000_000))
                    _refresh_files()
                    self.assertEqual(_members_of(str(view)), ("a-program",))
            finally:
                _MEMBERS_READ.clear()

    def test_a_held_file_set_leaves_no_copy_behind(self):
        """files_held, which may hold a box program's file as a named
        program's, removes every copy the guard makes during its block, so
        that program is never left refused for the rest of the run, and
        leaves any other file there, as the settings a drop reads
        (FIXTURE_ENVIRON). A copy removed is made again when it is next
        asked for, and a copy that stood before a block stays after it, even
        when the block removed it and the guard made it again. A made-up name
        stands for the program. Perturbations: keep the copies, drop every
        new file, drop every copy made during the block, or read the copy's
        file once, and this fails."""
        true = shutil.which("true", path=os.pathsep.join(_BOX_BINS))
        copy = STAND_IN_GUARD / "virt-held-during-a-block"
        other = STAND_IN_GUARD / "a-file-the-block-writes"
        try:
            with files_held(PRIVILEGED_FILES | {_file_of(true)}):
                guard_copy(copy.name)
                other.write_text("{}")
                self.assertTrue(copy.exists())
            self.assertFalse(copy.exists())
            self.assertTrue(other.exists(), "a file that is not a copy stays")
            self.assertEqual(guard_copy(copy.name), copy)
            self.assertTrue(copy.exists(), "a copy removed is made again")
            with files_held(PRIVILEGED_FILES | {_file_of(true)}):
                copy.unlink()
                guard_copy(copy.name)
            self.assertTrue(copy.exists(), "a copy that stood before the block stays")
        finally:
            other.unlink(missing_ok=True)
            copy.unlink(missing_ok=True)
            _COPIES_MADE.discard(copy)

    def test_the_guard_makes_its_copy_of_a_program_installed_during_the_run(self):
        """**The guard follows the box**: a privileged program that appears
        during a run in a directory outside the temporary root gets the
        guard's refusing copy when a PATH that carries the directory is
        read, so a PATH with the guard's directory first, as the suite's own
        is, still passes and its call meets the copy, and a PATH that finds
        the program before the guard is still refused. A scratch directory
        stands for the box, outside a temporary root held under the scratch
        with a guard of its own. The copy is read, never run. Perturbation:
        make no copy, and the guard's PATH is refused."""
        from unittest import mock
        module = sys.modules[__name__]
        with tempfile.TemporaryDirectory() as scratch:
            root = Path(scratch) / "root"
            guard = root / "guard"
            guard.mkdir(parents=True)
            box = Path(scratch) / "box"
            box.mkdir()
            try:
                with mock.patch.object(module, "TEST_ROOT", os.path.realpath(root)), \
                        mock.patch.object(module, "STAND_IN_GUARD", guard):
                    path = f"{guard}{os.pathsep}{box}"
                    guard_path(path, "PATH")
                    self.assertFalse((guard / "virt-late").exists())
                    (box / "virt-late").write_text("#!/bin/sh\nexit 0\n")
                    (box / "virt-late").chmod(0o755)
                    guard_path(path, "PATH")
                    self.assertIn("exit 97", (guard / "virt-late").read_text())
                    with self.assertRaises(StandInGuardError) as refused:
                        guard_path(f"{box}{os.pathsep}{guard}", "PATH")
                    self.assertIn(f"resolves virt-late to {box}/virt-late", str(refused.exception))
            finally:
                _COPIES_MADE.discard(guard / "virt-late")
                _MEMBERS_READ.pop(os.path.realpath(box), None)

    def test_env_hands_reads_envs_own_options_as_env_does(self):
        """env_hands reads the words after `env` as GNU env does: the PATH its
        command runs with (the default PATH where it clears the environment
        or PATH, in any spelling, and assigns none) and the command line. An
        option that takes an argument takes it, and an option of the
        command's own is not env's. An option the table lacks, an ambiguous
        shortening, an argument handed to a long option that takes none, and
        the options no line may hand env (`-S`, `-C`, `--env0-from`, each in
        any spelling), are refused, each by its own reason. Each row was
        checked against the box's env 9.12. Perturbations: read `-i` alone,
        drop `-`, `-a`'s argument or the grouping of short options, take a
        long option only in full, read an unknown option as one that takes
        nothing, read `-S` as any option, or let a long option that takes
        nothing take "=VALUE", and a row fails."""
        from unittest import mock
        rows = {
            ("-i", "a", "-i"): ("/default", ["a", "-i"]),
            ("-", "a"): ("/default", ["a"]),
            ("-iv", "a"): ("/default", ["a"]),
            ("-viu", "PATH", "a"): ("/default", ["a"]),
            ("-uPATH", "a"): ("/default", ["a"]),
            ("-u", "HOME", "a"): (None, ["a"]),
            ("--unset", "PATH", "a"): ("/default", ["a"]),
            ("--unset=PATH", "a"): ("/default", ["a"]),
            ("--ignore-env", "a"): ("/default", ["a"]),
            ("-a", "x", "-i", "a"): ("/default", ["a"]),
            ("--argv0", "x", "-u", "PATH", "a"): ("/default", ["a"]),
            ("--argv0=x", "-i", "a"): ("/default", ["a"]),
            ("--default-signal", "-i", "a"): ("/default", ["a"]),
            ("-i", "PATH=/p", "X=1", "a", "PATH=/q"): ("/p", ["a", "PATH=/q"]),
            ("--", "-i"): (None, ["-i"]),
            ("a", "-i"): (None, ["a", "-i"]),
        }
        refused = {("--i", "a"): "--i, an ambiguous shortening", ("-x", "a"): "-x, not one of its options",
                   ("--no-such-option", "a"): "--no-such-option, not one of its options",
                   ("--env0-from", "f", "a"): "--env0-from, which reads",
                   ("--env0-f=f", "a"): "--env0-from, which reads", ("-S", "-i a"): "-S, which splits",
                   ("-vS-i a",): "-S, which splits", ("-0S", "-i a"): "-S, which splits",
                   ("--split-string=a",): "-S, which splits", ("-C", "/d", "a"): "-C, which starts",
                   ("--ch=/d", "a"): "-C, which starts", ("-iC/d", "a"): "-C, which starts",
                   ("--ignore-environment=x", "a"): "--ignore-environment with an argument it does not take",
                   ("--debug=1", "a"): "--debug with an argument it does not take"}
        with mock.patch.object(os, "defpath", "/default"):
            for words, want in rows.items():
                with self.subTest(words=words):
                    self.assertEqual(env_hands(words), want)
            for words, reason in refused.items():
                with self.subTest(refused=words), self.assertRaises(StandInGuardError) as raised:
                    env_hands(words)
                self.assertIn(reason, str(raised.exception))
                self.assertIn("this guard does not read as env does: refused before it runs", str(raised.exception))

    def test_envs_table_holds_every_option_the_boxs_env_names(self):
        """The table env_hands reads holds every option the box's env names in
        its --help, long and short, each with whether it takes an argument,
        and no other, so an option a later env adds is named here before a
        line meets the guard's refusal of it. Perturbations: drop -a from
        the table, or add a short option the box's env lacks, and this
        fails."""
        env_program = shutil.which("env", path=os.pathsep.join(_BOX_BINS))
        text = subprocess.run([env_program, "--help"], capture_output=True, text=True, timeout=20,
                              env={"LC_ALL": "C", "PATH": str(STAND_IN_GUARD)}, check=True).stdout
        text = re.sub(r"\x1b\]8;;[^\x1b]*\x1b\\|\x1b\[[0-9;]*m", "", text)
        longs, shorts = {}, set()
        for line in text.splitlines():
            if not re.match(r"\s+-", line):
                continue
            short, long, argument = re.match(r"\s+(?:-(\w)(?:,\s+|\s|$))?(?:(--[\w-]+)(\[=|=)?)?", line).groups()
            if long:
                longs[long] = (short or long, {None: False, "=": True, "[=": "="}[argument])
            if short:
                shorts.add(short)
        self.assertIn("--argv0", longs, "the help reads as the table expects")
        self.assertEqual(longs, _ENV_LONG)
        self.assertEqual(shorts, set(_ENV_SHORT))

    def test_a_multi_call_binary_that_is_a_shell_or_a_stand_in_is_named(self):
        """A file that is both a named program's and env's (a multi-call
        binary such as busybox) is named at the start, where the run stops,
        so the guard never refuses env or sh by its file. A file under the
        scratch stands for the binary. Perturbation: name nothing, and this
        fails."""
        from unittest import mock
        with tempfile.TemporaryDirectory() as scratch:
            binary = Path(scratch) / "multi"
            binary.write_text("#!/bin/sh\nexit 0\n")
            binary.chmod(0o755)
            (Path(scratch) / "env").symlink_to(binary)
            with mock.patch.object(sys.modules[__name__], "PRIVILEGED_FILES", frozenset({_file_of(binary)})):
                self.assertEqual(shared_with_privileged((scratch,)), ["env"])
            self.assertEqual(shared_with_privileged((scratch,)), [])

    def test_a_view_makes_the_guards_copy_of_a_program_the_start_did_not_read(self):
        """view_link, the rule every view uses, makes the guard's refusing
        copy of a privileged program the start made none of (one installed
        since, or in a directory the start did not read), so a view never
        skips a name, which would hand the next directory's program the name
        or lose the call with 127. The copy is read, never run, since a run
        would record a reach. Perturbation: make no copy, and the view links
        a file that is not there."""
        true = shutil.which("true", path=os.pathsep.join(_BOX_BINS))
        copy = STAND_IN_GUARD / "virt-made-on-demand"
        self.assertFalse(copy.exists())
        try:
            with tempfile.TemporaryDirectory() as scratch:
                (Path(scratch) / "virt-made-on-demand").symlink_to(true)
                self.assertEqual(view_link("virt-made-on-demand", str(Path(scratch) / "virt-made-on-demand")), copy)
                text = copy.read_text()
                self.assertIn(shlex.quote(str(REACH_RECORD)), text)
                self.assertIn("exit 97", text)
                self.assertTrue(os.access(copy, os.X_OK))
                self.assertEqual(view_link("sh", "/usr/bin/sh"), "/usr/bin/sh")
                inside = Path(scratch) / "systemctl"
                self.assertEqual(view_link("systemctl", inside), inside, "a stand-in inside stays")
        finally:
            copy.unlink(missing_ok=True)

    def test_box_view_links_the_guards_copy_of_each_privileged_program(self):
        """box_view (#610) links no root-granting program, and for a
        privileged one the guard's refusing copy, never the box's, so a call
        is refused and recorded rather than lost. Perturbations: link the
        box's privileged programs, or none, and this fails."""
        with tempfile.TemporaryDirectory() as scratch:
            view = box_view(Path(scratch) / "view")
            self.assertEqual([n for n in os.listdir(view) if n in ROOT_GRANTING], [])
            self.assertEqual([n for n in os.listdir(view) if privileged(n, view / n) and not _inside(view / n)], [])
            for e in os.scandir("/usr/bin"):
                if e.name not in ROOT_GRANTING and privileged(e.name, e.path):
                    with self.subTest(name=e.name):
                        self.assertEqual(os.readlink(view / e.name), str(STAND_IN_GUARD / e.name))
            self.assertTrue((view / "sh").exists())


def program_check():
    """deploy/program_check.py, imported."""
    if "program_check" not in sys.modules:
        sys.path.insert(0, str(DEPLOY))
        try:
            import program_check  # noqa: F401
        finally:
            sys.path.remove(str(DEPLOY))
    return sys.modules["program_check"]


class SuiteGuardTests(unittest.TestCase):
    """**The suite's guard holds for the whole run**: no test gains a
    privilege, and a reach of a program the guard refuses fails the run
    whatever its tests said. A run's own end is shown in a child, which
    imports this file and runs one planted test class through run_suite."""

    def suite_child(self, body, end="sys.exit(t.run_suite(argv=['planted']))\n", env=None):
        # The child is this suite, which puts its own guard ahead of the box's
        # PATH as it starts. It is handed the guarded PATH, which every child
        # is read by, and the box's beside it, which it takes back before the
        # suite starts.
        code = ("import os, subprocess, sys, unittest\n"
                "os.environ['PATH'] = os.environ.pop('WEAVER_TEST_BOX_PATH')\n"
                f"sys.path.insert(0, {str(DEPLOY)!r})\n"
                "import test_plans as t\n"
                "print(t.REACH_DIR, flush=True)\n"
                + body
                + end)
        run = subprocess.run([sys.executable, "-c", code], env={**os.environ, "WEAVER_TEST_BOX_PATH": BOX_PATH,
                                                             **(env or {})},
                             text=True, capture_output=True, timeout=120)
        record_dir = Path(run.stdout.split("\n", 1)[0])
        self.assertEqual(record_dir.parent, Path(TEST_ROOT), run.stdout + run.stderr)
        self.assertTrue(record_dir.name.startswith("weaver-test-reach-"), record_dir)
        self.addCleanup(shutil.rmtree, record_dir, True)
        return run, record_dir

    def test_no_test_gains_a_privilege(self):
        """no_new_privs holds in the runner and in a child two execs down, so
        the box's sudo, reached by any path, acts as no one. Perturbation:
        drop the prctl, and the flag reads 0 here and in the child."""
        self.assertTrue(no_new_privs_held())
        child = subprocess.run(["bash", "--norc", "--noprofile", "-c", "cat /proc/self/status"], text=True,
                               capture_output=True, timeout=20)
        self.assertRegex(child.stdout, re.compile(r"^NoNewPrivs:\s*1$", re.M))

    def test_a_copied_scripts_child_sees_only_the_images_programs(self):
        """**The image view**: a child running a copied script, by its path or
        name or as an excerpt of its text, finds none of the box's programs the
        image lacks, finds the rest in IMAGE_VIEW and sudo in the guard, and
        keeps no PATH or DROP_PATH entry outside the tests' temporary root,
        though its excerpt starts with a helper a build script carries too. A
        child drawing on a build box's own lines, or on none, keeps the box's.
        Perturbations: skip the rewrite, read no bare name, or let a shared
        helper draw the build script, and the box's program is found."""
        programs = set(program_check().read_names(DEPLOY / "image-programs.txt"))
        extra = next((n for n in sorted(os.listdir("/usr/bin")) if n not in programs and n not in ROOT_GRANTING
                      and not privileged(n, f"/usr/bin/{n}") and re.fullmatch(r"[a-z][a-z0-9_-]*", n)
                      and os.access(f"/usr/bin/{n}", os.X_OK)), None)
        if extra is None:
            self.skipTest("this box carries no program the image lacks")
        probe = ('for n in "$@"; do printf "%s=%s\\n" "$n" "$(command -v "$n")"; done\n'
                 'printf "PATH=%s\\nDROP_PATH=%s\\n" "$PATH" "$DROP_PATH"\n')
        copied_line = next(line for line in (DEPLOY / "create-agent.sh").read_text().splitlines()
                           if re.fullmatch(r'[A-Z_]+="[a-z -]{30,}"', line))
        build_line = next(line for line in (DEPLOY / "update-stack.sh").read_text().splitlines()
                          if re.fullmatch(r'[A-Z_]+="[a-z -]{30,}"', line))
        self.assertEqual((_DEPLOY_LINES[copied_line], _DEPLOY_LINES[build_line]),
                         ({"create-agent.sh"}, {"update-stack.sh"}))
        # A helper both kinds of script carry, which an excerpt often starts with.
        shared_line = next(line for line in (DEPLOY / "create-agent.sh").read_text().splitlines()
                           if re.fullmatch(r"[a-z_]+\(\) +\{.*\}", line)
                           and _DEPLOY_LINES.get(line.strip(), set()) - DEPLOYED_COPY)
        handed = os.pathsep.join((str(STAND_IN_GUARD), "/usr/bin", "/opt/elsewhere"))
        env = {**os.environ, "DROP_PATH": handed}

        def seen(zero, program, wrapper=()):
            run = subprocess.run([*wrapper, "bash", "--norc", "--noprofile", "-c", program, zero, extra, "sh", "sudo"],
                                 env=env, text=True, capture_output=True, timeout=20)
            self.assertEqual(run.returncode, 0, run.stderr)
            return dict(line.split("=", 1) for line in run.stdout.splitlines())

        viewed = os.pathsep.join((str(STAND_IN_GUARD), str(IMAGE_VIEW)))
        for zero, program, wrapper in ((str(DEPLOY / "account-drop.sh"), probe, ()),
                                       ("account-drop.sh", probe, ()),
                                       ("probe", copied_line + "\n" + probe, ()),
                                       ("probe", shared_line + "\n" + copied_line + "\n" + probe, ()),
                                       (str(DEPLOY / "verify-load.sh"), probe,
                                        ("env", f"PATH={STAND_IN_GUARD}:/usr/bin:/opt/elsewhere"))):
            with self.subTest(zero=zero, wrapper=wrapper):
                got = seen(zero, program, wrapper)
                self.assertEqual(got[extra], "")
                self.assertEqual(got["sh"], str(IMAGE_VIEW / "sh"))
                self.assertEqual(got["sudo"], str(STAND_IN_GUARD / "sudo"))
                self.assertEqual((got["DROP_PATH"], got["PATH"] if wrapper else viewed), (viewed, viewed))
                self.assertEqual([e for e in got["PATH"].split(os.pathsep) if not _inside(e)], [], got["PATH"])
        for zero, program, wrapper in (("probe", probe, ()),
                                       ("probe", copied_line + "\n" + build_line + "\n" + probe, ()),
                                       ("probe", probe, ("env", f"PATH={STAND_IN_GUARD}:/usr/bin"))):
            with self.subTest(zero=zero, build=build_line in program, wrapper=wrapper):
                got = seen(zero, program, wrapper)
                self.assertEqual(got[extra], f"/usr/bin/{extra}")
                self.assertEqual(got["DROP_PATH"], handed)

    def test_a_shell_by_any_name_is_read_as_one(self):
        """A child run by any shell's name (bash, sh, dash, ash, ksh, mksh,
        zsh, rbash, csh, tcsh, fish), by path or under a wrapper, gets the
        view and the guard, and a
        child running neither a shell nor a deploy script does not.
        Perturbation: name bash and sh alone, and dash goes unread."""
        for shell in SHELLS:
            with self.subTest(shell=shell):
                self.assertTrue(_runs_a_script([shell, "-c", "true"]))
                self.assertTrue(_runs_a_script(["timeout", "5", f"/usr/bin/{shell}", "-c", "true"]))
        self.assertIn("dash", SHELLS)
        self.assertFalse(_runs_a_script(["python3", "-c", "pass"]))

    def test_the_view_and_the_checker_read_one_set_of_lists(self):
        """The view reads the checker's copy list, program list and bin
        directories, so neither drifts from the other: IMAGE_VIEW holds a
        link to each listed program the box carries, never a root-granting
        one, and a link to the guard's refusing copy of each listed
        privileged one, carried or not, and nothing else, so `command -v`
        finds each as on the image whatever the box holds. Perturbations:
        link the box's privileged program, or only one the box carries, and
        this fails."""
        pc = program_check()
        self.assertEqual(DEPLOYED_COPY, frozenset(pc.read_names(DEPLOY / "deployed-copy.txt")))
        self.assertEqual(frozenset(_BOX_BINS), pc.BIN_DIRS)
        bins = os.pathsep.join(_BOX_BINS)
        listed = {n for n in pc.read_names(DEPLOY / "image-programs.txt") if n not in ROOT_GRANTING}
        guarded = {n for n in listed if privileged(n, shutil.which(n, path=bins))}
        self.assertLessEqual({"systemctl", "nvidia-smi", "nvidia-modprobe"} & listed, guarded)
        self.assertEqual(set(os.listdir(IMAGE_VIEW)), guarded | {n for n in listed if shutil.which(n, path=bins)})
        for name in sorted(os.listdir(IMAGE_VIEW)):
            with self.subTest(name=name):
                want = STAND_IN_GUARD / name if name in guarded else shutil.which(name, path=bins)
                self.assertEqual(os.readlink(IMAGE_VIEW / name), str(want))

    def test_the_view_links_a_privileged_program_the_box_lacks(self):
        """A listed program privileged by its name is the guard's copy in the
        view even on a box that lacks it (a build box with no card driver),
        so a call there is refused and recorded, not lost with 127. Read with
        bin directories that carry nothing, and every such name the list
        holds has its copy in the guard. Perturbation: link a privileged
        program only where the box carries it, and this fails."""
        with tempfile.TemporaryDirectory() as empty:
            self.assertEqual(image_link("nvidia-smi", (empty,)), STAND_IN_GUARD / "nvidia-smi")
            self.assertEqual(image_link("systemctl", (empty,)), STAND_IN_GUARD / "systemctl")
            self.assertIsNone(image_link("sh", (empty,)))
        for name in _IMAGE_LISTED:
            if privileged(name):
                with self.subTest(name=name):
                    self.assertTrue((STAND_IN_GUARD / name).is_file())

    def test_a_test_that_omits_a_privileged_stand_in_meets_the_guard(self):
        """**An omission is named, not silent** (the Planner's ruling of
        2026-11-18): a test whose stand-ins omit systemctl, running a copied
        script's line (the image view) or a line of no deploy script (the
        suite's PATH), meets the guard's refusing copy and never the box's
        program. The call is refused with 97, the reach names the test, and
        the run fails though the test passed. The planted test reads where
        systemctl resolves before it runs anything, so a view or a guard
        without the copy fails there and reaches nothing. Perturbations: link
        the box's program in the view, or drop the set from the guard, and no
        reach is recorded."""
        line = next(l.strip() for l in (DEPLOY / "decommission.sh").read_text().splitlines()
                    if l.strip().startswith("mapfile -t UNITS < <(systemctl list-units"))
        run, record_dir = self.suite_child(
            f"line = {line!r}\n"
            "import shutil, tempfile\n"
            "class Planted(unittest.TestCase):\n"
            "    def test_omitted(self):\n"
            "        with tempfile.TemporaryDirectory() as stand_ins:\n"
            "            copied = line + '\\nprintf %s ${#UNITS[@]}'\n"
            "            box = stand_ins + os.pathsep + '/usr/bin' + os.pathsep + '/bin'\n"
            "            inherited = stand_ins + os.pathsep + os.environ['PATH']\n"
            "            self.assertEqual(t.drawn_from(['bash', '-c', copied]), {'decommission.sh'})\n"
            "            guard = os.path.realpath(t.STAND_IN_GUARD / 'systemctl')\n"
            "            for path in (t.image_path(box), inherited):\n"
            "                found = shutil.which('systemctl', path=path)\n"
            "                self.assertEqual(found and os.path.realpath(found), guard, path)\n"
            "            took = subprocess.run(['bash', '-c', copied], env={**os.environ, 'PATH': box},\n"
            "                                  capture_output=True, text=True, timeout=20)\n"
            "            self.assertEqual((took.returncode, took.stdout), (0, '0'), took.stderr)\n"
            "            took = subprocess.run(['bash', '-c', 'systemctl is-active weaver-planted'],\n"
            "                                  env={**os.environ, 'PATH': inherited}, capture_output=True, text=True,\n"
            "                                  timeout=20)\n"
            "            self.assertEqual(took.returncode, 97, took.stderr)\n")
        self.assertEqual(run.returncode, 1, run.stderr)
        self.assertIn("\nOK\n", run.stderr)
        self.assertIn("guard: no_new_privs held, 2 reaches of a program the guard refuses\n", run.stderr)
        self.assertIn("guard: __main__.Planted.test_omitted reached systemctl list-units weaver-worker@* --all "
                      "--no-legend --plain\n", run.stderr)
        self.assertIn("guard: __main__.Planted.test_omitted reached systemctl is-active weaver-planted\n", run.stderr)
        self.assertIn(f"guard: the record stays at {record_dir / 'reach-record'}\n", run.stderr)

    def test_a_reach_fails_a_run_whose_tests_all_passed(self):
        """**A reach fails the run**, even when every test passed: a test that
        reaches the guard's sudo and takes its 97 leaves a line naming the
        test and the arguments, the run exits 1, and the record stays under
        the tests' temporary root, named. A drop that clears the environment,
        and a reach after the test, still write theirs, as unknown, and an
        argument's newline or tab stays inside its own reach. Perturbations: drop the record's
        write, or the end's check of it, and the run exits 0."""
        run, record_dir = self.suite_child(
            "class Planted(unittest.TestCase):\n"
            "    def test_reach(self):\n"
            "        took = subprocess.run([str(t.STAND_IN_GUARD / 'sudo'), '-n', 'true'], capture_output=True)\n"
            "        self.assertEqual(took.returncode, 97)\n"
            "        took = subprocess.run(['env', '-i', 'PATH=' + str(t.STAND_IN_GUARD),\n"
            "                               str(t.STAND_IN_GUARD / 'runuser'), '-u', 'nobody', 'id'],\n"
            "                              capture_output=True)\n"
            "        self.assertEqual(took.returncode, 97)\n"
            "        took = subprocess.run([str(t.STAND_IN_GUARD / 'sudo'), 'bash', '-c', 'a\\nb\\tc'],\n"
            "                              capture_output=True)\n"
            "        self.assertEqual(took.returncode, 97)\n"
            "    @classmethod\n"
            "    def tearDownClass(cls):\n"
            "        subprocess.run([str(t.STAND_IN_GUARD / 'sudo'), '-n', 'after'], capture_output=True)\n")
        self.assertEqual(run.returncode, 1, run.stderr)
        self.assertIn("\nOK\n", run.stderr)
        self.assertIn("guard: no_new_privs held, 4 reaches of a program the guard refuses\n", run.stderr)
        self.assertIn("guard: __main__.Planted.test_reach reached sudo bash -c a\\nb\\tc\n", run.stderr)
        self.assertIn("guard: unknown reached sudo -n after\n", run.stderr)
        self.assertIn("guard: __main__.Planted.test_reach reached sudo -n true\n", run.stderr)
        self.assertIn("guard: unknown reached runuser -u nobody id\n", run.stderr)
        self.assertIn(f"guard: the record stays at {record_dir / 'reach-record'}\n", run.stderr)
        self.assertTrue((record_dir / "reach-record").is_file())

    def test_a_reach_fails_a_run_under_another_runner(self):
        """**Another runner** (unittest's own main, as `python3 -m unittest`
        and pytest run it) never calls finish(), and a reach still fails the
        process at exit, named, with the record kept. Perturbation: drop the
        exit's check, and the run exits 0."""
        run, record_dir = self.suite_child(
            "class Planted(unittest.TestCase):\n"
            "    def test_reach(self):\n"
            "        took = subprocess.run([str(t.STAND_IN_GUARD / 'sudo'), '-n', 'true'], capture_output=True)\n"
            "        self.assertEqual(took.returncode, 97)\n",
            end="unittest.main(argv=['planted'])\n")
        self.assertEqual(run.returncode, 1, run.stderr)
        self.assertIn("\nOK\n", run.stderr)
        # The child's runner names no test, so the reach carries the id it inherited.
        self.assertRegex(run.stderr, r"\nguard: \S+ reached sudo -n true\n")
        self.assertIn(f"guard: the record stays at {record_dir / 'reach-record'}, and this runner never read it\n",
                      run.stderr)

    def test_a_reach_after_the_runs_end_or_a_record_gone_fails_the_process(self):
        """A reach after the run's end read the record (a test's atexit
        callback) fails the process at exit, named, and so does a record a
        test removed or made unreadable (a directory in its place), said
        once: by the run's end under run_suite, and by the exit under
        unittest's own main. Perturbations: let
        the exit pass a gone record, catch only a missing one, or read only
        whether finish() ran, and a case exits 0."""
        late = ("class Planted(unittest.TestCase):\n"
                "    def test_late(self):\n"
                "        import atexit\n"
                "        atexit.register(subprocess.run, [str(t.STAND_IN_GUARD / 'sudo'), '-n', 'late'],\n"
                "                        capture_output=True)\n")
        run, record_dir = self.suite_child(late)
        self.assertEqual(run.returncode, 1, run.stderr)
        self.assertIn("guard: no_new_privs held, 0 reaches of a program the guard refuses\n", run.stderr)
        self.assertIn("guard: unknown reached sudo -n late\n", run.stderr)
        self.assertIn(f"guard: the record stays at {record_dir / 'reach-record'}, and the run's end read it before "
                      "these reaches\n", run.stderr)
        for undo, error in (("t.REACH_RECORD.unlink()", "FileNotFoundError"),
                            ("t.REACH_RECORD.unlink(); t.REACH_RECORD.mkdir()", "IsADirectoryError")):
            gone = ("class Planted(unittest.TestCase):\n"
                    "    def test_gone(self):\n"
                    f"        {undo}\n")
            # The run's end says it under run_suite, and the exit under
            # unittest's own main, which has no end of its own.
            for end, who in (("sys.exit(t.run_suite(argv=['planted']))\n", "run"),
                             ("unittest.main(argv=['planted'])\n", "process")):
                with self.subTest(undo=undo, end=end):
                    run, record_dir = self.suite_child(gone, end=end)
                    self.assertEqual(run.returncode, 1, run.stderr)
                    self.assertIn("\nOK\n", run.stderr)
                    self.assertIn(f"guard: the reach record {record_dir / 'reach-record'} cannot be read ({error}), "
                                  f"so this {who} cannot show", run.stderr)
                    self.assertEqual(run.stderr.count("guard: the reach record"), 1, run.stderr)
                    self.assertNotIn("Traceback", run.stderr)

    def test_a_reach_whose_arguments_are_not_text_is_counted_and_named(self):
        """A reach whose argument is not valid text (a 0xff byte) is counted
        and named, escaped, in a test and after the run's end, so the run
        fails, and a UTF-8 argument is named as UTF-8 under a locale whose
        encoding is not (the C locale with UTF-8 mode and coercion off).
        Perturbations: read the record as strict text, and the run crashes
        before naming the reach, or the exit passes it. Read it in the
        locale's encoding, and the UTF-8 argument is named byte by byte."""
        reach = "subprocess.run([str(t.STAND_IN_GUARD / 'sudo'), '-n', os.fsdecode(b'\\xff')], capture_output=True)"
        for body, who in (("    def test_bytes(self):\n        " + reach + "\n", "test_bytes"),
                          ("    def test_late(self):\n        import atexit, functools\n"
                           "        atexit.register(functools.partial(lambda: " + reach + "))\n", "unknown")):
            with self.subTest(who=who):
                run, _ = self.suite_child("import os\nclass Planted(unittest.TestCase):\n" + body)
                self.assertEqual(run.returncode, 1, run.stderr)
                self.assertRegex(run.stderr, r"guard: \S*" + who + r"\S* reached sudo -n \\+xff\n")
                self.assertNotIn("Traceback", run.stderr)
        accent = "subprocess.run([str(t.STAND_IN_GUARD / 'sudo'), '-n', os.fsdecode(b'\\xc3\\xa9')], capture_output=True)"
        run, _ = self.suite_child("import os\nclass Planted(unittest.TestCase):\n    def test_accent(self):\n        "
                                  + accent + "\n", env={"LC_ALL": "C", "PYTHONUTF8": "0", "PYTHONCOERCECLOCALE": "0",
                                                   "PYTHONIOENCODING": "ascii:backslashreplace"})
        self.assertEqual(run.returncode, 1, run.stderr)
        self.assertRegex(run.stderr, r"guard: \S*test_accent\S* reached sudo -n \\xe9\n")

    def test_a_run_with_no_reach_ends_on_its_tests_and_leaves_no_record(self):
        """A run with no reach exits on its tests' verdict, 0 when they pass,
        1 when one fails and 5, as unittest's own main does, when none ran,
        and removes the record's directory at exit. Perturbations: keep the
        record at exit, and its directory stands. Read no count of the tests
        run, and a run of none exits 0."""
        for body, status in (("    def test_nothing(self):\n        pass\n", 0),
                             ("    def test_failing(self):\n        self.fail('planted')\n", 1),
                             ("    pass\n", 5)):
            with self.subTest(status=status):
                run, record_dir = self.suite_child("class Planted(unittest.TestCase):\n" + body)
                self.assertEqual(run.returncode, status, run.stderr)
                self.assertIn("guard: no_new_privs held, 0 reaches of a program the guard refuses\n", run.stderr)
                self.assertFalse(record_dir.exists(), "a run with no reach leaves no record")


class ProgramCheckTests(unittest.TestCase):
    """**The deployed copy calls only the image's programs** (#610): a live
    take-down stopped at decommission.sh's `HOST=$(hostname)`, a program the
    appliance image does not carry, while this suite passed throughout. The
    checker reads every command position of the copied shell scripts against
    deploy/image-programs.txt, the sealed image's program names, with no
    build and no run, so a call no test reaches is still read."""

    PROGRAMS = DEPLOY / "image-programs.txt"
    COPY = DEPLOY / "deployed-copy.txt"
    PLANT = "hostname"

    def checker(self, *args):
        return subprocess.run([sys.executable, str(DEPLOY / "program_check.py"), "--programs", str(self.PROGRAMS),
                               *map(str, args)], text=True, capture_output=True, timeout=120)

    def copied(self, scratch):
        """The deployed copy in SCRATCH, beside its list."""
        shutil.copy2(self.COPY, scratch / self.COPY.name)
        names = program_check().read_names(self.COPY)
        for name in names:
            shutil.copy2(DEPLOY / name, scratch / name)
        return scratch / self.COPY.name, names

    def options_used(self):
        """(program, spelling, script) for each row of OPTIONS a line of the
        copied scripts uses, read in process under the checker's wall-clock
        bound, as findings() reads."""
        pc = program_check()
        programs = set(pc.read_names(self.PROGRAMS))
        shells = [str(DEPLOY / n) for n in pc.read_names(self.COPY) if n.endswith(".sh")]
        used = set()
        with pc.bound():
            scopes = pc.sourcer_functions(shells)
            for path in shells:
                script, _ = pc.check_shell(path, programs, scopes[path])
                used |= {(p, sp, script.name) for p, sp, _ in script.options_used}
        return used

    def findings(self, copy, programs=None):
        """The checker's findings over the copy's shell scripts, in process,
        as {script name: [(line, word, why)]}, under its wall-clock bound."""
        pc = program_check()
        programs = set(pc.read_names(self.PROGRAMS)) if programs is None else programs
        here = copy.parent
        shells = [str(here / n) for n in pc.read_names(copy) if n.endswith(".sh")]
        with pc.bound():
            scopes = pc.sourcer_functions(shells)
            return {os.path.basename(p): pc.check_shell(p, programs, scopes[p])[1] for p in shells}

    def test_the_deployed_copy_calls_only_the_images_programs(self):
        run = self.checker("--copy", self.COPY)
        self.assertEqual((run.returncode, run.stdout, run.stderr), (0, "", ""))

    def test_a_call_outside_the_list_is_named_in_each_copied_script(self):
        """**Planted**: hostname, absent from the image, called in each copied
        shell script in each shape a script calls a program, at the top level
        and inside a function, is named with its script and line, and nothing
        else is: no option's argument (`sudo -u USER`, `time -p`) reads as a
        program, and a wrapper named by its path (`/usr/bin/env`) is read as
        by its name. Perturbation: drop the reading of a shape (the wrappers,
        the code strings, the substitutions, an option's argument, a heredoc
        a shell reads past a pipe), and its lines go unnamed."""
        pc = program_check()
        self.assertNotIn(self.PLANT, pc.read_names(self.PROGRAMS))
        h = self.PLANT
        shapes = [[f"HOST=$({h})"], [f"true | {h} >/dev/null"], [f"x=$(true && {h} -s)"], [f"sudo -n {h}"],
                  [f"env -u X PATH=/usr/bin {h}"], [f"sudo -u nobody -H {h}"], [f"sh -c '{h}'"], [f'x="$({h})"'],
                  [f"if ! {h} >/dev/null; then :; fi"], [f"{{ {h}; }} 2>/dev/null"],
                  [f"find . -maxdepth 0 -exec {h} {{}} \\;"], [f"trap '{h}' EXIT"], [f"while {h}; do break; done"],
                  [f"case x in x) {h} ;; esac"], [f"/usr/bin/{h}"], [f"command -v {h} >/dev/null"],
                  [f"command {h}"], [f"xargs {h} </dev/null"], [f"x=$(( 1 + $({h} | wc -c) ))"],
                  ["cat <<EOF", f"$({h})", "EOF"], [f"y=$(sh -c 'echo $({h})')"], [f"cat <({h}) /dev/null"],
                  [f"sudo -n sh -c '{h}'"], [f"trap -- '{h}' EXIT"], [f"exec {h} </dev/null"],
                  [f"find -P . -type f -exec {h} {{}} +"], [f"time -p {h}"], ["sh <<'X' | cat", h, "X"],
                  [f"/usr/bin/env {h}"], [f"tar -I /usr/bin/{h} -cf /dev/null ."]]
        own = {"create-agent.sh": [f"root {h}", f"root -n {h}", f"read_as_owner {h}",
                                   f"as_account --stdin /dev/null nobody A=1 {h}"],
               "decommission.sh": [f"as_op {h}", f"as_op sh -c '{h}'"],
               "verify-load.sh": [f"as_account nobody {h}"],
               "account-drop.sh": [f"as_account nobody {h}"],
               "retired-python.sh": [f"root {h}"]}
        with tempfile.TemporaryDirectory() as scratch:
            copy, names = self.copied(Path(scratch))
            shells = [n for n in names if n.endswith(".sh")]
            self.assertEqual(sorted(shells), sorted(own))
            planted = {}
            for name in shells:
                lines = (Path(scratch) / name).read_text().split("\n")
                plants = shapes + [[w] for w in own[name]]
                head = next(i for i, line in enumerate(lines) if re.match(r"^[a-z_]+\(\) \{\s*(#.*)?$", line))
                body = [line for plant in plants for line in plant]
                lines[head + 1:head + 1] = body
                lines += body
                # An array run as a command, assigned once, so planted once,
                # under a name of each script's own (the fence refuses a name
                # another script beside it mentions).
                run = "PLANTED_" + name.split(".")[0].replace("-", "_").upper()
                lines += [f"{run}=(env -u X)", f"{run}+=(A=1)", f'"${{{run}[@]}}" {h}']
                (Path(scratch) / name).write_text("\n".join(lines))
                want, at = [len(lines)], head + 2
                for start in (head + 2, len(lines) - 3 - len(body) + 1):
                    at = start
                    for plant in plants:
                        want.append(at + next(i for i, line in enumerate(plant) if h in line))
                        at += len(plant)
                planted[name] = sorted(want)
            found = self.findings(copy)
            for name in shells:
                with self.subTest(script=name):
                    self.assertEqual(sorted(line for line, word, _ in found[name] if word == h), planted[name],
                                     found[name])
                    self.assertEqual([f for f in found[name] if f[1] != h], [])
            run = self.checker("--copy", copy)
            self.assertEqual(run.returncode, 1)
            self.assertIn(f"{Path(scratch) / 'decommission.sh'}:{planted['decommission.sh'][0]}: {h}: called here, "
                          "and not a program the image carries (to pass: carry the program in the image's list, or stand "
                          "the call inside a guard in GUARDED)\n", run.stdout)

    def test_an_option_passes_only_by_its_row_in_options(self):
        """**OPTIONS, the option spellings the copied scripts use**: each
        wrapper, command, exec and each leaf whose options can run a program
        passes an option only by its row, read by the row's reading. An
        argument row's word is never the command (`sudo -u hostname true`),
        a code row's word (`tar -I`) and a code flag's string (`sh -c`) are
        read as code, a lookup's words are looked up (`command -v`), every
        -exec of a find runs its words to its ; or +, and find's -P stands
        before its starting point. Any other option or spelling is refused
        by its line with the remedy: a joined or grouped form of a row
        (`sudo -iu`, `tar -cIx`, `tar cIf`), one inside quotes (`tar '-Ix'`),
        one the shell expands (`sudo -u$X`), a row missing its argument, and
        any use of a wrapper no copied script names as a command (runuser,
        flock, timeout, bash), whatever its options. Perturbations: read an argument row's
        word as the command, skip a code row's string, pass a spelling
        outside its row, read an expanded word by its value, or let a
        missing argument pass, and a line goes unnamed or misnamed."""
        pc = program_check()
        h = self.PLANT
        named = [f"sh -c '{h}' name", "sh <<'X'", h, "X", f"find . -exec true {{}} \\; -exec {h} {{}} \\;",
                 f"find -P . -maxdepth 1 -type f -exec {h} {{}} +", f"sudo -u nobody -H {h}", f"sudo -n -u nobody {h}",
                 f"sudo -n A=1 {h}", f"env -u X -u Y {h}", f"env -u X A=1 {h}", f"xargs {h} </dev/null",
                 f"command {h}", f"command -v {h} >/dev/null", f"exec {h}", f"tar -I '{h} -9' -cf /dev/null .",
                 f"tar --numeric-owner -tf x -I {h}", f"sudo -u nobody sh -c '{h}'"]
        quiet = ["sudo -u hostname true", "env -u hostname true", "install -o hostname -g hostname -m 0644 a b",
                 "find . -name hostname -printf '%P\\n'", "awk -F: -v hostname=1 '{ print }' f",
                 "find -P . -type f -exec true {} \\; -exec true {} +", "sh -c 'true'", "command -v true >/dev/null",
                 "tar -I 'zstd -T0' -cf - .", "look_fn() { :; }", "command -v look_fn >/dev/null"]
        spelling = "a spelling OPTIONS does not hold"
        with tempfile.TemporaryDirectory() as scratch:
            copy, _ = self.copied(Path(scratch))
            target = Path(scratch) / "verify-load.sh"
            text = target.read_text()
            start = text.count("\n") + 2
            target.write_text(text + "\n" + "\n".join(named + quiet) + "\n")
            want = [start + i for i, line in enumerate(named) if h in line and not line.startswith("sh <<")]
            found = self.findings(copy)["verify-load.sh"]
            self.assertEqual([(line, w) for line, w, _ in found], [(n, h) for n in want], found)
            for planted, why in (
                    ("sudo -iu nobody hostname", f"sudo -iu, {spelling}"),
                    ("sudo -n -- hostname", f"sudo --, {spelling}"),
                    ("sudo --user=nobody hostname", f"sudo --user=nobody, {spelling}"),
                    ("sudo -u$X hostname", f"sudo -u$X, {spelling}"),
                    ("sudo -u", "sudo -u with its argument missing"),
                    ("env -i hostname", f"env -i, {spelling}"),
                    ("env -uX hostname", f"env -uX, {spelling}"),
                    ("env -S 'hostname -s'", f"env -S, {spelling}"),
                    ("sh -ec hostname", f"sh -ec, {spelling}"),
                    ("sh -o pipefail -c hostname", f"sh -o, {spelling}"),
                    ("xargs -r hostname", f"xargs -r, {spelling}"),
                    ("xargs -I{} hostname {}", f"xargs -I{{}}, {spelling}"),
                    ("command -p hostname", f"command -p, {spelling}"),
                    ("command -pv hostname", f"command -pv, {spelling}"),
                    ("exec -a nm hostname", f"exec -a, {spelling}"),
                    ("find . -delete", f"find -delete, {spelling}"),
                    ("find . -execdir hostname {} +", f"find -execdir, {spelling}"),
                    ("find . -ok hostname {} \\;", f"find -ok, {spelling}"),
                    ("find -L . -type f", f"find -L, {spelling}"),
                    ("find . -type f -P", "find -P past its starting points"),
                    ("find . $p", f"find $p, {spelling}"),
                    ("find . -name", "find -name with its argument missing"),
                    ("tar -cIx -f /dev/null .", f"tar -cIx, {spelling}"),
                    ("tar -cI x -f /dev/null .", f"tar -cI, {spelling}"),
                    ("tar cIf x y.tar .", f"tar cIf, {spelling}"),
                    ("tar -cFx -f y.tar .", f"tar -cFx, {spelling}"),
                    ("tar '-Ix' -cf y.tar .", f"tar '-Ix', {spelling}"),
                    ("tar -I'x -9' -cf y.tar .", f"tar -I'x -9', {spelling}"),
                    ("tar --use-compress-program=x -cf y .", f"tar --use-compress-program=x, {spelling}"),
                    ("tar --to-command=x -xf y", f"tar --to-command=x, {spelling}"),
                    ("tar --checkpoint-action=exec=x -cf y .", f"tar --checkpoint-action=exec=x, {spelling}"),
                    ("tar -cf y . -I", "tar -I with its argument missing"),
                    ("sort --compress-program=gzip x", f"sort --compress-program=gzip, {spelling}"),
                    ("install --strip-program=x -s a b", f"install --strip-program=x, {spelling}"),
                    ("visudo -f x", f"visudo -f, {spelling}"),
                    ("pacman -S x", f"pacman -S, {spelling}"),
                    ("awk -f p.awk f", f"awk -f, {spelling}"),
                    ("awk '--source=BEGIN { system(\"x\") }'", f"awk '--source=BEGIN {{ system(\"x\") }}', {spelling}"),
                    ("awk -eBEGIN f", f"awk -eBEGIN, {spelling}"),
                    ("awk -E p.awk f", f"awk -E, {spelling}"),
                    ("awk -v", "awk -v with its argument missing"),
                    ("sed -f s.sed f", f"sed -f, {spelling}"),
                    ("sed -e p f", f"sed -e, {spelling}"),
                    ("runuser --command=true --command=hostname root", "what runuser runs"),
                    ("runuser '--command=true' root", "what runuser runs"),
                    ("runuser -u root -- hostname", "what runuser runs"),
                    ("flock /tmp/l --command=true", "what flock runs"),
                    ("timeout -sKILL 5 hostname", "what timeout runs"),
                    ("bash -euo pipefail -c hostname", "what bash runs")):
                with self.subTest(planted=planted):
                    target.write_text(text + "\n" + planted + "\n")
                    found = self.findings(copy)["verify-load.sh"]
                    self.assertEqual([(w, why_) for _, w, why_ in found], [("-", f"this reader does not follow {why}")])
                    if spelling in why:
                        self.assertEqual(pc.remedy(found[0][2]), "add this spelling to OPTIONS with its reading, or "
                                                                 "rewrite to one OPTIONS holds")
                    elif why.endswith("with its argument missing"):
                        self.assertEqual(pc.remedy(found[0][2]), "give the option its argument, as the copied scripts "
                                                                 "do")
                    else:
                        self.assertIsNotNone(pc.remedy(found[0][2]))
            target.write_text(text + "\nenv -i hostname\n")
            run = self.checker("--copy", copy)
            self.assertIn(f"{target}:{start}: -: this reader does not follow env -i, a spelling OPTIONS does not hold "
                          "(to pass: add this spelling to OPTIONS with its reading, or rewrite to one OPTIONS holds)\n",
                          run.stdout)

    def test_each_row_of_options_is_used_by_its_script(self):
        """**OPTIONS checked both ways**: every row is used by the script it
        cites, in the copied scripts as they stand, and a row no line of its
        script uses is named, as a DYNAMIC entry is: a planted row, and a
        real row whose use is taken out. An edit that moves the cited line
        names nothing, so a change to a copied script does not fail the
        suite over a line number. A row citing a script the copy does not
        hold is named. Perturbations: drop the stale-row check, check the
        cited line rather than the script, or skip the rows whose script the
        copy does not hold, and a row goes unnamed or a moved line fails."""
        pc = program_check()
        used = self.options_used()
        self.assertEqual([k for k, (_, cited) in pc.OPTIONS.items() if (*k, cited.rsplit(":", 1)[0]) not in used], [])
        self.assertEqual(pc.unheld_rows(set(pc.read_names(self.COPY))), [])
        with tempfile.TemporaryDirectory() as scratch:
            copy, _ = self.copied(Path(scratch))
            pc.OPTIONS[("sort", "-r")] = ("flag", "verify-load.sh:1")
            try:
                found = self.findings(copy)["verify-load.sh"]
                beside = pc.unheld_rows({"create-agent.sh"})
            finally:
                del pc.OPTIONS[("sort", "-r")]
            self.assertEqual(found, [(1, "-r", "OPTIONS holds sort -r for this script, and no line of it uses that "
                                               "spelling")])
            self.assertEqual(pc.remedy(found[0][2]), "cite a line that uses the spelling, or remove the row")
            self.assertIn((0, "-r", "OPTIONS holds sort -r for verify-load.sh:1, a script the copy does not hold"),
                          beside)
            self.assertNotIn("create-agent.sh", " ".join(why for _, _, why in beside))
            (cited,) = [c for k, (_, c) in pc.OPTIONS.items() if k == ("env", "-u")]
            self.assertTrue(cited.startswith("verify-load.sh:"))
            at = int(cited.rsplit(":", 1)[1])
            target = Path(scratch) / "verify-load.sh"
            text = target.read_text()
            target.write_text("# a line above\n" + text)
            self.assertEqual(self.findings(copy)["verify-load.sh"], [])
            lines = text.split("\n")
            (use,) = [n for n, line in enumerate(lines) if line.startswith("LDD_ENV=(env -u ")]
            lines[use] = "LDD_ENV=(env)"
            target.write_text("\n".join(lines))
            self.assertEqual(self.findings(copy)["verify-load.sh"],
                             [(at, "-u", "OPTIONS holds env -u for this script, and no line of it uses that "
                                         "spelling")])
            run = self.checker("--copy", copy)
            self.assertEqual(run.returncode, 1)
            self.assertIn(f"{target}:{at}: -u: OPTIONS holds env -u for this script", run.stdout)

    def test_an_array_run_as_a_command_is_read_by_its_fence(self):
        """**An array run as a command**: a function's own local array, and
        an array in each state its appends can leave (an append later than
        the run, or skipped by its condition), are read, so hostname is named
        at each planted line and true names nothing. An array is refused, by
        its reason, where it is not one assignment and at most six appends,
        where its name is written in any other form or mentioned beside the
        script, where its entries stand in another scope or a loop, where it
        runs inside another array's run or in a new shell's code, and where
        its assignment is not established before the run (under a condition,
        in $( ), chained, piped, in the background, in a case branch or a
        group, after && and blank lines, or later in the text), though not
        after an arithmetic command ends a chain. Its elements are never
        assignments. Perturbations: read an array with all its appends
        alone, its entries across scopes or in a loop, its name's other
        mentions, an array inside another's run or in a new shell, no limit
        on its appends, its elements as assignments, an assignment that may
        not have run, or a continuation lost on a blank line or kept past an
        arithmetic command, and a line goes unnamed or misnamed."""
        h = self.PLANT
        named = ['f_true() { local -a F_TRUE_RUN=(true); "${F_TRUE_RUN[@]}"; }',
                 f'f_plant() {{ local -a F_PLANT_RUN=({h}); "${{F_PLANT_RUN[@]}}"; }}',
                 "STATE1_RUN=(env)", f'"${{STATE1_RUN[@]}}" {h}', "STATE1_RUN+=(-u)",
                 "STATE2_RUN=(env)", "if false; then STATE2_RUN+=(-u); fi", f'"${{STATE2_RUN[@]}}" {h}']
        with tempfile.TemporaryDirectory() as scratch:
            copy, _ = self.copied(Path(scratch))
            target = Path(scratch) / "verify-load.sh"
            text = target.read_text()
            start = text.count("\n") + 2
            target.write_text(text + "\n" + "\n".join(named) + "\n")
            want = [start + i for i, line in enumerate(named) if h in line]
            found = self.findings(copy)["verify-load.sh"]
            self.assertEqual([(line, w) for line, w, _ in found], [(n, h) for n in want], found)

            def unread(n, why):
                return f'"${{{n}[@]}}" run as a command, since it is {why}'

            def elsewhere(n):
                return unread(n, f"written elsewhere than {n}=( ), {n}+=( ) and ${{{n}[...]}}, or beside this script")

            count, scope = "not one assignment and at most 6 appends", \
                "assigned nowhere, or outside the run's function, or in more than one scope"
            unset = ("assigned where it may not run before the run: not a statement of its own at the top of its "
                     "scope, or later in the text")
            pc = program_check()
            for planted, why in (
                    ('TWICE_RUN=(true)\nTWICE_RUN=(hostname)\n"${TWICE_RUN[@]}"', unread("TWICE_RUN", count)),
                    ("SEVEN_RUN=(env)\n" + "SEVEN_RUN+=(A=1)\n" * 7 + '"${SEVEN_RUN[@]}" true', unread("SEVEN_RUN", count)),
                    ('mapfile -t MAPPED_RUN </dev/null\n"${MAPPED_RUN[@]}"', elsewhere("MAPPED_RUN")),
                    ('ELEMENT_RUN=(true)\nELEMENT_RUN[0]=hostname\n"${ELEMENT_RUN[@]}"', elsewhere("ELEMENT_RUN")),
                    ('SCALAR_RUN=(env)\nSCALAR_RUN=hostname\n"${SCALAR_RUN[@]}" true', elsewhere("SCALAR_RUN")),
                    ('UNSET_RUN=(true)\nunset UNSET_RUN\nUNSET_RUN+=(hostname)\n"${UNSET_RUN[@]}"', elsewhere("UNSET_RUN")),
                    ('g_set() { local LOCAL_RUN=hostname; f_run; }\nf_run() { "${LOCAL_RUN[@]}" true; }\nLOCAL_RUN=(env)',
                     elsewhere("LOCAL_RUN")),
                    ('ADDED_RUN=(env)\nadd_to() { ADDED_RUN+=(-u); }\nadd_to\n"${ADDED_RUN[@]}" echo X',
                     unread("ADDED_RUN", scope)),
                    ('f_one() { local -a TWO_RUN=(true); "${TWO_RUN[@]}"; }\nf_two() { local -a TWO_RUN=(env); }',
                     unread("TWO_RUN", scope)),
                    ('"${NEVER_RUN[@]}" true', unread("NEVER_RUN", scope)),
                    ('LOOP_RUN=(env)\nfor i in 1 2; do LOOP_RUN+=(-u); done\n"${LOOP_RUN[@]}" true',
                     unread("LOOP_RUN", "assigned or appended in a loop")),
                    ('WHILE_RUN=(env)\nwhile false; do WHILE_RUN+=(-u); done\n"${WHILE_RUN[@]}" true',
                     unread("WHILE_RUN", "assigned or appended in a loop")),
                    ('BRACE_RUN=(env)\nfor i in a; { BRACE_RUN+=(-u); }\n"${BRACE_RUN[@]}" true',
                     unread("BRACE_RUN", "assigned or appended in a loop")),
                    ('NESTED_RUN=(env)\n"${NESTED_RUN[@]}" "${NESTED_RUN[@]}" hostname',
                     unread("NESTED_RUN", "run inside another array's run")),
                    ("CHILD_RUN=(true)\nsh -c '\"${CHILD_RUN[@]}\" hostname'",
                     unread("CHILD_RUN", "run in a new shell's code, which inherits none of the script's arrays")),
                    *[(text, unread(n, unset)) for n, text in (
                        ("OPTIONAL_RUN", 'if false; then OPTIONAL_RUN=(true); fi\n"${OPTIONAL_RUN[@]}" hostname'),
                        ("SUB_RUN", 'y=$(SUB_RUN=(true))\n"${SUB_RUN[@]}" hostname'),
                        ("GROUP_RUN", '( GROUP_RUN=(true) )\n"${GROUP_RUN[@]}" hostname'),
                        ("AND_RUN", 'false && AND_RUN=(true)\n"${AND_RUN[@]}" hostname'),
                        ("NEWLINE_RUN", 'false &&\n  NEWLINE_RUN=(true)\n"${NEWLINE_RUN[@]}" hostname'),
                        ("BACK_RUN", 'BACK_RUN=(true) &\n"${BACK_RUN[@]}" hostname'),
                        ("PIPED_RUN", 'PIPED_RUN=(true) | cat\n"${PIPED_RUN[@]}" hostname'),
                        ("LATE_RUN", '"${LATE_RUN[@]}" hostname\nLATE_RUN=(true)'),
                        ("BLANK_RUN", 'false &&\n\nBLANK_RUN=(true)\n"${BLANK_RUN[@]}" hostname'),
                        ("NOTE_RUN", 'false |\n# a note\nNOTE_RUN=(true)\n"${NOTE_RUN[@]}" hostname'),
                        ("CASE_RUN", 'case x in\n  a)\n    CASE_RUN=(true)\n    ;;\nesac\n"${CASE_RUN[@]}" hostname'),
                        ("LINES_SUB_RUN", 'y=$(\nLINES_SUB_RUN=(true)\n)\n"${LINES_SUB_RUN[@]}" hostname'),
                        ("LINES_GROUP_RUN", '(\nLINES_GROUP_RUN=(true)\n)\n"${LINES_GROUP_RUN[@]}" hostname'))],
                    ):
                with self.subTest(planted=planted):
                    target.write_text(text + "\n" + planted + "\n")
                    found = self.findings(copy)["verify-load.sh"]
                    self.assertEqual([(w, why_) for _, w, why_ in found], [("-", f"this reader does not follow {why}")])
                    self.assertIsNotNone(pc.remedy(found[0][2]))
            # A name another script beside it mentions is fenced too.
            other = Path(scratch) / "create-agent.sh"
            kept = other.read_text()
            other.write_text(kept + "\nBESIDE_RUN+=(-u)\n")
            target.write_text(text + '\nBESIDE_RUN=(env)\n"${BESIDE_RUN[@]}" true\n')
            self.assertEqual([(w, why_) for _, w, why_ in self.findings(copy)["verify-load.sh"]],
                             [("-", f"this reader does not follow {elsewhere('BESIDE_RUN')}")])
            other.write_text(kept)
            # An established assignment past an unrelated chained line or an
            # arithmetic command ending a chain, and a multi-line function's
            # local array, pass.
            target.write_text(text + '\nPAST_RUN=(env -u X)\ntrue && echo x\nPAST_RUN+=(A=1)\n"${PAST_RUN[@]}" hostname\n'
                              'f_lines() {\n  local -a LINES_RUN=(true)\n  "${LINES_RUN[@]}" x\n}\n'
                              'false && (( 1 ))\nARITH_RUN=(true)\n"${ARITH_RUN[@]}"\n')
            self.assertEqual([w for _, w, _ in self.findings(copy)["verify-load.sh"]], ["hostname"])
            # An array's elements are never assignments: bash runs A=1 as a program.
            target.write_text(text + '\nELEMENTS_RUN=(A=1 true)\n"${ELEMENTS_RUN[@]}"\n')
            self.assertEqual([w for _, w, _ in self.findings(copy)["verify-load.sh"]], ["A=1"])

    def test_what_a_wrapper_execs_is_a_program_and_an_unread_runner_is_named(self):
        """**What a wrapper execs is a program**: a builtin, keyword or
        function after sudo, env, xargs, find -exec or an own wrapper is
        looked for on the list, so `xargs cd` and `sudo cd` are named. A
        quoted assignment after env, sudo or as_account is read as theirs, as
        they read it. A program by its path in a bin directory
        (`/usr/bin/env`, `/usr/bin/time`) is read as by its name. Each program
        in UNREAD_RUNNERS, which runs a command this reader does not read,
        the wrappers no copied script names as a command among them (runuser,
        timeout, flock, bash, the program time), is named, by its name or its
        path.
        Perturbations: let a wrapper run a builtin, read quoted assignments as
        commands, pass the unread runners or the program time, or stop at a
        wrapper's path, and a line goes unnamed."""
        pc = program_check()
        h = self.PLANT
        calls = ["xargs cd /d", "sudo -u x cd /d", "env wait", "as_account u read v",
                 f'env "PATH=$p" {h}', f'sudo "A=$x" {h}', f'as_account nobody "A=$x" {h}',
                 f"/usr/bin/env {h}", "/usr/bin/env cd /d", f"/usr/bin/time -f %e {h}", f"/bin/timeout 5 {h}",
                 "env -u X true", "xargs printf x", "find . -exec test -d {} \\;", "/usr/bin/echo x"]
        named = [("cd",), ("cd",), ("wait",), ("read",), (h,), (h,), (h,), (h,), ("cd",), ("-", "time"), ("-",)]
        with tempfile.TemporaryDirectory() as scratch:
            copy, _ = self.copied(Path(scratch))
            target = Path(scratch) / "verify-load.sh"
            text = target.read_text()
            start = text.count("\n") + 2
            target.write_text(text + "\n" + "\n".join(calls) + "\n")
            found = self.findings(copy)["verify-load.sh"]
            self.assertEqual([(line, w) for line, w, _ in found],
                             [(start + i, w) for i, ws in enumerate(named) for w in ws], found)
            runners = sorted(pc.UNREAD_RUNNERS)
            # Behind command, so time is the program, never the keyword.
            target.write_text(text + "\n" + "\n".join(f"command {r} true" for r in runners)
                              + "\n/usr/bin/unshare true\n")
            found = self.findings(copy)["verify-load.sh"]
            self.assertEqual(sorted((line, why) for line, w, why in found if w == "-"),
                             [(start + i, f"this reader does not follow what {r} runs") for i, r in enumerate(runners)]
                             + [(start + len(runners), "this reader does not follow what unshare runs")])

    def test_a_program_a_function_or_keyword_stands_for_is_read_as_the_program(self):
        """**What bash runs as a program**, though a function or the keyword
        time has its name: `time` past a command's start (after an assignment
        or a redirect, escaped, behind command, from an array) is the program
        time, which runs a command this reader does not read and is named,
        and `command`, `exec`, a wrapper and a path skip functions, while the
        keyword time at a command's start runs the function, its command
        read. A new shell's code (sh -c, tar -I) inherits no function, while
        eval and trap run in this shell, and builtin reads the builtin it
        names. Perturbations: read a later time as the keyword, let command,
        a path or a new shell reach a function, read eval in a new shell, or
        pass builtin unread, and a line goes unnamed."""
        h = self.PLANT
        lines = [("hostname() { command hostname -s; }", [h]), ("myf() { :; }", []),
                 ("X=1 time true", ["-", "time"]), (f">/dev/null time {h}", ["-", "time"]),
                 (f"\\time {h}", ["-", "time"]), (f"command time {h}", ["-", "time"]),
                 (f"TIME_RUN=(time {h})", ["-", "time"]), ('"${TIME_RUN[@]}"', []), ("exec myf", ["myf"]),
                 ("/usr/bin/env myf", ["myf"]), ("xargs myf", ["myf"]), ("myf", []),
                 ("time -p no_such_tool", ["no_such_tool"]), (f"time -p {h}", []), (f"sh -c {h}", [h]),
                 (f"sudo sh -c '{h} -s'", [h]), (f"eval {h}", []), (f"trap '{h}' EXIT", []),
                 ("builtin eval no_such_tool", ["no_such_tool"]), (f"/usr/bin/{h}", [h]),
                 (f"tar -I '{h} -9' -cf /dev/null .", [h])]
        with tempfile.TemporaryDirectory() as scratch:
            copy, _ = self.copied(Path(scratch))
            target = Path(scratch) / "retired-python.sh"
            text = target.read_text()
            start = text.count("\n") + 2
            target.write_text(text + "\n" + "\n".join(line for line, _ in lines) + "\n")
            found = self.findings(copy)["retired-python.sh"]
            self.assertEqual([(line, w) for line, w, _ in found],
                             [(start + i, w) for i, (_, ws) in enumerate(lines) for w in ws], found)

    def test_a_program_or_word_it_cannot_place_is_refused_with_its_remedy(self):
        """**The fences**: a program on the list that is neither in LEAVES nor
        a wrapper wrapped() reads, and a command word this reader cannot name
        outside DYNAMIC, are refused, and a DYNAMIC entry no line uses is
        named. The command line prints each finding's remedy after it.
        Perturbations: pass an unplaced program, pass an unnamed word, keep an
        unused entry, or print no remedy, and a case goes unnamed."""
        pc = program_check()
        lines = [("gzip -9 f", "gzip", "this reader does not know whether it runs a command it is handed"),
                 ('"$PROG" x', '"$PROG"', "this reader cannot name the program this runs"),
                 ("/opt/tool/bin/run x", "/opt/tool/bin/run", "this reader cannot name the program this runs"),
                 (f"{self.PLANT} -s", self.PLANT, "called here, and not a program the image carries")]
        with tempfile.TemporaryDirectory() as scratch:
            copy, _ = self.copied(Path(scratch))
            target = Path(scratch) / "verify-load.sh"
            text = target.read_text()
            start = text.count("\n") + 2
            target.write_text(text + "\n" + "\n".join(line for line, _, _ in lines) + "\n")
            found = self.findings(copy)["verify-load.sh"]
            self.assertEqual(found, [(start + i, w, why) for i, (_, w, why) in enumerate(lines)])
            run = self.checker("--copy", copy)
            self.assertEqual(run.returncode, 1)
            self.assertIn(f"{target}:{start}: gzip: this reader does not know whether it runs a command it is handed "
                          "(to pass: add it to LEAVES with the reason it runs nothing it is handed, or read it in "
                          "wrapped() with its spellings in OPTIONS)\n", run.stdout)
            self.assertIn(f"{target}:{start + 1}: \"$PROG\": this reader cannot name the program this runs (to pass: "
                          "name the program, or add (script, word) to DYNAMIC with what it runs)\n", run.stdout)
            self.assertEqual([line for line in run.stdout.splitlines() if " (to pass: " not in line], [])
            target.write_text(text)
            pc.DYNAMIC[("verify-load.sh", '"$GONE"')] = "planted"
            try:
                found = self.findings(copy)["verify-load.sh"]
            finally:
                del pc.DYNAMIC[("verify-load.sh", '"$GONE"')]
            self.assertEqual(found, [(0, '"$GONE"', "DYNAMIC allows it, and no line of the script runs that")])

    def test_no_input_takes_the_checker_more_than_a_few_seconds(self):
        """**A bound on cost**: a nested or pathological shape is refused, not
        explored, so no input takes the checker past ten seconds: arrays four
        deep with six appends each, code nested 200 deep, arrays inside
        arrays, an array read in its 64 states around a code string, a long
        script, a sed script holding a run of 60 backslashes, and an array
        whose element is an operator. Each is refused by its own reason, with
        its own remedy printed after it, and each exits as its findings say,
        so a crash or a reader's failure fails. Perturbations: read an array
        inside another's run, lift the depth bound, let sed's ordinary
        character take a backslash, read an operator as an array element,
        fail the depth bound as a reader's defect, or stand the operator
        element's remedy after the catch-all, and a case times out, passes
        unread, or ends by another reason or remedy."""
        h = self.PLANT
        no = "this reader does not follow "
        words = "write the array's elements as words"
        cases = [("".join(f'NEST{i}=("${{NEST{i + 1}[@]}}")\n' + f"NEST{i}+=(-x)\n" * 6 for i in range(4))
                  + 'NEST4=(true)\n"${NEST0[@]}"\n', no + '"${NEST1[@]}" run as a command, since it is run inside '
                                                          "another array's run",
                  "write the array as one NAME=( ), a statement of its own at the top of its scope before the run, "
                  f"and at most {program_check().MAX_APPENDS} NAME+=( ) outside loops, with no other write to the "
                  "name, and run it in the shell that assigned it"),
                 ("x=" + "$(" * 200 + "true" + ")" * 200 + "\n", no + "code nested deeper than 32",
                  "flatten the nesting"),
                 ("A1=(" * 200 + "x" + ")" * 200 + "\n", no + "an array inside an array", words),
                 ("WIDE_RUN=(sh -c 'true')\n" + "".join(f"WIDE_RUN+=(V{i}=1)\n" for i in range(6))
                  + f'"${{WIDE_RUN[@]}}" {h}\n', None, None),
                 ("true | cat\n" * 5000, None, None),
                 ("sed -n 's/" + "\\" * 60 + "/x/;p' f\n",
                  no + "sed past its s, p, d and q commands, whose e runs a program",
                  "write the sed script as s, p, d and q commands, in one literal word"),
                 ("x=(;)\n", no + "an array element that starts with ;", words + ", as bash requires")]
        with tempfile.TemporaryDirectory() as scratch:
            for text, refused, remedy in cases:
                with self.subTest(text=text[:40]):
                    target = Path(scratch) / "case.sh"
                    target.write_text("#!/usr/bin/env bash\n" + text)
                    try:
                        run = subprocess.run([sys.executable, str(DEPLOY / "program_check.py"), "--programs",
                                              str(self.PROGRAMS), str(target)], text=True, capture_output=True,
                                             timeout=10)
                    except subprocess.TimeoutExpired:
                        self.fail("the checker took more than ten seconds")
                    said = [tuple(line.split(": ", 2)[1:]) for line in run.stdout.splitlines()]
                    self.assertEqual(said, [("-", f"{refused} (to pass: {remedy})")] if refused else [],
                                     run.stdout + run.stderr)
                    self.assertEqual(run.returncode, 1 if refused else 0, run.stdout + run.stderr)

    def test_every_remedy_is_reached_by_its_own_refusal(self):
        """**REMEDIES in order**: the first match wins, so an entry an earlier
        one shadows is never printed. Every entry is reached by a refusal it
        is written for: an entry whose pattern is plain text by a refusal
        that is that text, and a pattern entry by a real refusal the checker
        prints, one for each, so a new pattern entry needs its own. No entry
        that starts with the catch-all's text stands after the catch-all.
        Perturbations: stand the operator element's remedy after the
        catch-all, where it was written first, or the heredoc-in-$( ) entry
        after the plain heredoc entry, and it is named."""
        pc = program_check()
        catch_all = "this reader does not follow"
        no = catch_all + " "
        printed = {r"this reader does not follow a heredoc inside \$\( \)":
                       no + "a heredoc inside $( ) whose body follows the )",
                   r"this reader does not follow .*, a spelling OPTIONS does not hold":
                       no + "sed -i, a spelling OPTIONS does not hold",
                   r"this reader does not follow .* with its argument missing": no + "tar -I with its argument missing",
                   r"this reader does not follow \w+, which changes what a name runs":
                       no + "enable, which changes what a name runs",
                   r'this reader does not follow "\$\{':
                       no + '"${NEST1[@]}" run as a command, since it is run inside another array\'s run',
                   r"(?:called|looked up) here, and not a program": "called here, and not a program the image carries"}
        keys = [k for k, _ in pc.REMEDIES]
        self.assertEqual(keys.count(catch_all), 1)
        self.assertEqual([k for k in keys[keys.index(catch_all) + 1:] if k.startswith(catch_all)], [])
        patterns = {k for k in keys if set(k) & set(".^$*+?{}[]\\|()")}
        self.assertEqual(patterns, set(printed))
        self.assertEqual([k for k, r in pc.REMEDIES
                          if pc.remedy(printed.get(k, k)) != r.replace("MAX_APPENDS", str(pc.MAX_APPENDS))], [])

    def test_the_checker_bounds_its_own_run(self):
        """**The checker's own bound**: a run that loops past BOUND_SECONDS,
        or allocates past BOUND_BYTES of address space, ends with a refusal
        naming the file it was checking and the limit it reached, with exit
        1 and no traceback. The loops are planted in place of the first of
        two files' check, after both were read for their functions, and the
        child runs under an address-space net of its own above the bound,
        so the box is never at risk. The wall-clock loop allocates nothing,
        and the child's standard error holds only the line it writes after
        main() returns. Perturbations: set no wall-clock limit, or no
        address-space limit, or name the file last read, and the run is
        killed by the test's timeout, names the net's limit, or names the
        other file."""
        pc = program_check()
        net = 1 << 30
        self.assertLess(pc.BOUND_BYTES, net)
        with tempfile.TemporaryDirectory() as scratch:
            first, second = Path(scratch) / "a.sh", Path(scratch) / "b.sh"
            for target in (first, second):
                target.write_text("#!/usr/bin/env bash\ntrue\n")
            for seconds, loop, limit in ((0.5, "pass", "the wall-clock limit of 0.5 s"),
                                         (5, "held.append(bytearray(1 << 20))",
                                          f"the address-space limit of {pc.BOUND_BYTES >> 20} MiB")):
                with self.subTest(limit=limit):
                    code = (f"import sys\nsys.path.insert(0, {str(DEPLOY)!r})\nimport program_check as pc\n"
                            f"pc.BOUND_SECONDS = {seconds}\nheld = []\ndef loop(*args):\n"
                            f"    while True:\n        {loop}\n"
                            f"pc.check_shell = loop\n"
                            f"rc = pc.main(['--programs', {str(self.PROGRAMS)!r}, {str(first)!r}, {str(second)!r}])\n"
                            "sys.stderr.write('returned')\n"
                            "sys.exit(rc)\n")
                    run = subprocess.run([sys.executable, "-c", code], text=True, capture_output=True, timeout=10,
                                         preexec_fn=lambda: resource.setrlimit(resource.RLIMIT_AS, (net, net)))
                    why = f"the checker's bound was reached ({limit}) while reading this file, so it is refused unread"
                    self.assertEqual(run.returncode, 1, run.stdout + run.stderr)
                    self.assertEqual(run.stderr, "returned")
                    self.assertEqual(run.stdout, f"{first}:0: -: {why} (to pass: {pc.remedy(why)})\n")

    def test_the_suites_own_reading_is_bounded_in_time(self):
        """The suite runs the reader over the copy in process in two helpers,
        findings() and options_used(), where an address-space limit would
        bind the suite and every child it starts, so each reads under the
        wall-clock limit alone: a reading that runs past it raises
        BoundReached. The loop is planted in place of the first reading
        inside each bound, and the test asserts it was entered, so the
        refusal is the loop's and not a slow real reading's. The suite's
        other in-process calls into the checker (check_python, siblings,
        unheld_rows, remedy) are regex or list work over a file's lines, not
        the reader. Perturbation: either helper reads unbounded, and the
        planted loop runs out its three seconds and returns."""
        pc = program_check()
        kept = pc.sourcer_functions, pc.BOUND_SECONDS
        entered = []

        def loop(paths):
            entered.append(len(paths))
            end = time.monotonic() + 3
            while time.monotonic() < end:
                pass
            return kept[0](paths)

        pc.sourcer_functions, pc.BOUND_SECONDS = loop, 0.3
        try:
            with tempfile.TemporaryDirectory() as scratch:
                copy, _ = self.copied(Path(scratch))
                for name, read in (("findings", lambda: self.findings(copy)), ("options_used", self.options_used)):
                    with self.subTest(read=name):
                        entered.clear()
                        with self.assertRaises(pc.BoundReached):
                            read()
                        self.assertEqual(len(entered), 1)
        finally:
            pc.sourcer_functions, pc.BOUND_SECONDS = kept

    def test_the_bound_leaves_its_caller_as_it_found_it(self):
        """bound() restores what its caller had: a timer the caller armed
        runs on from where it stood, and the caller's SIGALRM handler and
        address-space limit come back. Off the main thread, which alone
        receives the signal, it refuses to run rather than read unbounded.
        Perturbations: zero the caller's timer, or bound a thread's reading
        by nothing, and the test goes red."""
        pc = program_check()
        alarms = []

        def caller(signum, frame):
            alarms.append(signum)

        limits = resource.getrlimit(resource.RLIMIT_AS)
        before = signal.signal(signal.SIGALRM, caller)
        try:
            signal.setitimer(signal.ITIMER_REAL, 100)
            with pc.bound(1, 512 << 20):
                self.assertNotEqual(resource.getrlimit(resource.RLIMIT_AS), limits)
            left = signal.getitimer(signal.ITIMER_REAL)[0]
            self.assertTrue(90 < left <= 100, left)
            self.assertIs(signal.getsignal(signal.SIGALRM), caller)
            self.assertEqual(resource.getrlimit(resource.RLIMIT_AS), limits)
            self.assertEqual(alarms, [])
        finally:
            signal.setitimer(signal.ITIMER_REAL, 0)
            signal.signal(signal.SIGALRM, before)
        refused = []

        def off_main():
            try:
                with pc.bound():
                    refused.append(None)
            except RuntimeError as e:
                refused.append(str(e))

        thread = threading.Thread(target=off_main)
        thread.start()
        thread.join(10)
        self.assertEqual(refused, ["the checker's wall-clock bound needs the main thread, which alone receives its "
                                   "signal"])

    def test_a_guarded_program_passes_only_by_its_allowance_and_its_guard(self):
        """nvcc and rustup, absent from the image, pass in decommission.sh only
        by GUARDED, inside the guard its entry names. With GUARDED empty both
        are named. With a line's guard gone its program is named. The same call
        unguarded in decommission.sh, beside its guard on one line, or guarded
        in another script, is named. Perturbations: allow a program whatever
        its line holds, or anywhere on a line holding its guard, and the calls
        outside their guards pass."""
        pc = program_check()
        programs = set(pc.read_names(self.PROGRAMS))
        self.assertEqual(sorted(p for s, p, _, _ in pc.GUARDED), ["nvcc", "rustup"])
        self.assertFalse({"nvcc", "rustup"} & programs)
        with tempfile.TemporaryDirectory() as scratch:
            copy, _ = self.copied(Path(scratch))
            self.assertEqual(self.findings(copy)["decommission.sh"], [])
            saved = pc.GUARDED
            pc.GUARDED = ()
            try:
                bare = self.findings(copy)["decommission.sh"]
            finally:
                pc.GUARDED = saved
            self.assertEqual(sorted({w for _, w, _ in bare}), ["nvcc", "rustup"], bare)
            target = Path(scratch) / "decommission.sh"
            text = target.read_text()
            for program, guard in ((p, g) for s, p, g, _ in pc.GUARDED):
                with self.subTest(guard=guard):
                    lines = text.split("\n")
                    at = [i for i, line in enumerate(lines) if guard in line and program in line]
                    self.assertEqual(len(at), 1, at)
                    lines[at[0]] = lines[at[0]].replace(guard, guard.replace("||", "&&"))
                    target.write_text("\n".join(lines))
                    found = self.findings(copy)["decommission.sh"]
                    self.assertEqual([w for _, w, _ in found], [program], found)
                    self.assertIn("this use lies outside its guard", found[0][2])
            target.write_text(text + "\nrustup show\nnvcc --version\n")
            found = self.findings(copy)["decommission.sh"]
            self.assertEqual(sorted(w for _, w, _ in found), ["nvcc", "rustup"], found)
            beside = [g for s_, p, g, _ in pc.GUARDED]
            target.write_text(text + f"\nNVCC=$({beside[0]} true); nvcc --build\n"
                              f"x=$({beside[1]}; rustup self uninstall -y\n")
            found = self.findings(copy)["decommission.sh"]
            self.assertEqual(sorted((w, line) for line, w, _ in found),
                             [("nvcc", text.count("\n") + 2), ("rustup", text.count("\n") + 3)], found)
            target.write_text(text)
            other = Path(scratch) / "create-agent.sh"
            other.write_text(other.read_text() + "\nx=$(command -v nvcc || true)\n"
                             "y=$(root rustup show 2>/dev/null || echo unknown)\n")
            found = self.findings(copy)["create-agent.sh"]
            self.assertEqual(sorted(w for _, w, _ in found), ["nvcc", "rustup"], found)

    def test_a_function_that_runs_its_arguments_needs_its_wrapper_entry(self):
        """**A new wrapper cannot hide what it runs**: a function that runs a
        command it was handed, "$@", "$1" or a name it took from them, unnamed
        in OWN_WRAPPERS, is named itself, since the command handed to it would
        otherwise read as an argument. One that runs a fixed binary with its
        arguments (verify-load's admin) is not. Perturbations: drop the check,
        or the names taken from the arguments, and a wrapper passes."""
        pc = program_check()
        with tempfile.TemporaryDirectory() as scratch:
            copy, _ = self.copied(Path(scratch))
            target = Path(scratch) / "verify-load.sh"
            target.write_text(target.read_text() + '\nrun_it() {\n  sudo -n "$@"\n}\nrun_it hostname\n'
                              'run_by() { local c=$1; shift; "$c" "$@"; }\nrun_by hostname\n'
                              'run_one() { "${1:?}"; }\nrun_one hostname\n'
                              'run_fixed() { "$ADMIN" "$@"; }\nrun_fixed show\n')
            found = self.findings(copy)["verify-load.sh"]
            self.assertEqual([(w, why) for _, w, why in found],
                             [(fn, "a function that runs its arguments as a command, with no entry in "
                                   "OWN_WRAPPERS") for fn in ("run_it", "run_by", "run_one")])
            saved = dict(pc.OWN_WRAPPERS)
            del pc.OWN_WRAPPERS["as_op"]
            try:
                found = self.findings(copy)["decommission.sh"]
            finally:
                pc.OWN_WRAPPERS.clear()
                pc.OWN_WRAPPERS.update(saved)
            self.assertIn("as_op", [w for _, w, _ in found])

    def test_a_construct_the_reader_does_not_follow_is_named_never_passed(self):
        """A backtick, coproc, an unclosed quote ($' included), a shell reading
        a pipe, a heredoc whose $( ) closes before its body, code nested
        deeper than 32 and an array inside an array stop the reading and are
        named, so a file is never passed unread. So is each way a leaf on the
        list runs a program through a form its rows leave open (visudo's
        editor, pacman without -Q first, awk's system(), pipes and a program
        the shell expands, and sed's e and a script the shell expands), each
        builtin that changes what a name runs (enable, alias,
        hash -p, shopt expand_aliases) and each variable bash runs as code (a
        prompt, BASH_ENV, ENV, an exported function), while the leaves' plain
        forms pass. Each refusal has its remedy. A case inside $( ), whose
        pattern's ) a parenthesis count would take for the close, is read."""
        with tempfile.TemporaryDirectory() as scratch:
            copy, _ = self.copied(Path(scratch))
            target = Path(scratch) / "retired-python.sh"
            text = target.read_text()
            for planted, why in (("x=`hostname`", "a backtick substitution"), ("coproc hostname", "coproc"),
                                 ("echo 'open", "an unclosed single quote"),
                                 ("cat <<EOF | sh\nhostname\nEOF",
                                  "sh reading a standard input that is not its own heredoc"),
                                 ("x=$'abc\nhostname", "an unclosed $' quote"),
                                 ("x=$(cat <<EOF)\nbody\nEOF", "a heredoc inside $( ) whose body follows the )"),
                                 ("x=" + "$(" * 40 + "true" + ")" * 40, "code nested deeper than 32"),
                                 ("A9=(B9=(x))", "an array inside an array"),
                                 ("visudo", "visudo without -c, which opens an editor"),
                                 ("pacman $OP x", "pacman without -Q first, the query, which runs no hooks"),
                                 ("pacman x", "pacman without -Q first, the query, which runs no hooks"),
                                 ("awk '{ system(\"x\") }' f", "awk with system() or a pipe in its program, which runs a "
                                                             "program"),
                                 ("awk '{ print | \"x\" }' f", "awk with system() or a pipe in its program, which runs a "
                                                            "program"),
                                 ('awk "$p" f', "awk with a program the shell expands (a variable or a substitution in "
                                                "it), whose text this reader does not see"),
                                 ("sed 's/a/b/e' f", "sed past its s, p, d and q commands, whose e runs a program"),
                                 ("sed -n '1e x' f", "sed past its s, p, d and q commands, whose e runs a program"),
                                 ('sed -n "s/$A/b/p" f', "sed with a script the shell expands (a variable or a "
                                                         "substitution in it), whose text this reader does not see"),
                                 ("enable -n echo", "enable, which changes what a name runs"),
                                 ("alias ls=x", "alias, which changes what a name runs"),
                                 ("hash -p /opt/x ls", "hash, which changes what a name runs"),
                                 ("shopt -s expand_aliases", "shopt, which changes what a name runs"),
                                 ("PS4='$(x) '", "a variable bash runs as code: PS4"),
                                 ("env BASH_ENV=/opt/x sh s", "a variable bash runs as code: BASH_ENV")):
                with self.subTest(planted=planted):
                    target.write_text(text + "\n" + planted + "\n")
                    found = self.findings(copy)["retired-python.sh"]
                    self.assertEqual([why_ for _, _, why_ in found], [f"this reader does not follow {why}"])
                    self.assertIsNotNone(program_check().remedy(found[0][2]))
            target.write_text(text + "\nx=$(case a in a) hostname ;; b) true ;; esac)\n"
                              "case b in a) true ;; b) hostname\nesac\nSELF_RUN=(\"${SELF_RUN[@]}\")\n\"${SELF_RUN[@]}\"\n"
                              "awk -F: '$1 == 1 || $2 == 2 { print }' f\nsed -n 's/a/b/p' f\nsed 2d f\nvisudo -cqf x\n"
                              "sed -n 's/a\\/b/c\\\\/p' f\n"
                              "pacman -Q x\nsort -u x\n")
            found = self.findings(copy)["retired-python.sh"]
            self.assertEqual([(w, why) for _, w, why in found],
                             [("hostname", "called here, and not a program the image carries")] * 2
                             + [("-", 'this reader does not follow "${SELF_RUN[@]}" run as a command, since it is run '
                                      "inside another array's run")])

    def test_a_python_file_that_starts_a_program_is_named(self):
        """The checker reads no Python, so a copied Python file, and the Python
        a copied shell script carries, passes only while it starts no
        program, by any spelling the scan knows (an attribute, a from-import,
        an alias anywhere in an import list, pty, multiprocessing).
        Perturbations: drop either check, the from-import spelling, or the
        alias after another import, and a call passes."""
        with tempfile.TemporaryDirectory() as scratch:
            copy, _ = self.copied(Path(scratch))
            target = Path(scratch) / "turn.py"
            target.write_text(target.read_text() + "\nimport subprocess\n")
            run = self.checker("--copy", copy)
            self.assertEqual(run.returncode, 1)
            self.assertRegex(run.stdout, r"turn\.py:\d+: subprocess: starts a program")
            shell = Path(scratch) / "verify-load.sh"
            shell.write_text(shell.read_text() + "\npython3 -c 'import os\nos.system(\"hostname\")'\n")
            found = self.findings(copy)["verify-load.sh"]
            self.assertEqual([w for _, w, _ in found], ["os.system"], found)
            pc = program_check()
            text = (DEPLOY / "turn.py").read_text()
            for spelling in ("from os import system, execv", "from os import *", "from os import (\n    system,\n)",
                             "import os as o", "import sys, os as o", "import sys as s, os.path, os as o",
                             "import pty", "posix.posix_spawn(p, a, e)", "import multiprocessing"):
                with self.subTest(spelling=spelling):
                    target.write_text(text + "\n" + spelling + "\n")
                    self.assertEqual([n for n, _, _ in pc.check_python(target)], [text.count("\n") + 2])

    def test_the_program_list_names_its_seal(self):
        """The list is one sealed image's: its first line names the seal, and
        its names are plain, sorted and each once."""
        lines = self.PROGRAMS.read_text().splitlines()
        self.assertRegex(lines[0], r"^# seal [0-9a-f]{8,}$")
        names = program_check().read_names(self.PROGRAMS)
        self.assertEqual(names, sorted(set(names)))
        self.assertEqual([n for n in names if not re.fullmatch(r"[A-Za-z0-9_.+\[-]+", n)], [])

    def test_no_deployed_script_or_double_names_a_root_binary_by_path(self):
        """A root-granting program by an absolute path passes every PATH a test
        sets, so no deploy script, deploy Python file or the test double names
        one. Perturbation: plant /usr/bin/sudo in a script, and it is named."""
        pattern = re.compile(r"/(?:usr/)?(?:local/)?s?bin/(?:" + "|".join(map(re.escape, ROOT_GRANTING))
                             + r")(?![\w.-])")
        for planted in ("exec /usr/bin/sudo -n true", "/bin/su -c x", "/usr/local/sbin/runuser -u a b",
                        '"/usr/bin/sudo-rs" x', "/usr/bin/pkexec x"):
            self.assertRegex(planted, pattern)
        self.assertNotRegex("/usr/bin/sum /usr/bin/sudoku /usr/bin/subscribe", pattern)
        texts = {p.name: p.read_text() for p in sorted(DEPLOY.iterdir())
                 if p.suffix in (".sh", ".py") and p.name != "test_plans.py"}
        texts["test_plans.DOUBLE"] = DOUBLE
        for name, text in texts.items():
            with self.subTest(file=name):
                self.assertEqual([n for n, line in enumerate(text.splitlines(), 1) if pattern.search(line)], [])


class DeployedCopyTests(unittest.TestCase):
    """**deploy/ states its deployed copy** (#607): deploy/deployed-copy.txt
    names the files a copy of deploy/ installed elsewhere needs, so a consumer
    keeps no list of its own. Every listed file exists, each once, and every
    file a listed one sources or runs from its own directory is listed."""

    def test_the_list_holds_every_sibling_its_files_reach(self):
        pc = program_check()
        listed = pc.read_names(DEPLOY / "deployed-copy.txt")
        self.assertEqual(len(listed), len(set(listed)))
        for name in listed:
            with self.subTest(file=name):
                self.assertTrue((DEPLOY / name).is_file(), name)
                self.assertEqual([s for s in pc.siblings(DEPLOY / name) if s not in listed], [])
        self.assertEqual(pc.siblings(DEPLOY / "create-agent.sh"), ["account-drop.sh", "retired-python.sh"])
        self.assertEqual(pc.siblings(DEPLOY / "verify-load.sh"), ["account-drop.sh", "agent_tuple.py"])

    def test_a_new_sibling_is_read(self):
        """A source or a run from the script's own directory, in each spelling
        the scripts use and the usual others (a name assigned from the
        directory, a quoted name Python joins to it), is read as a sibling,
        and a comment or a message naming a file is not. Perturbations: read
        only `source` lines, or only a name after a /, and one is missed."""
        pc = program_check()
        with tempfile.TemporaryDirectory() as scratch:
            script = Path(scratch) / "x.sh"
            script.write_text('#!/usr/bin/env bash\n'
                              'source "$(dirname "${BASH_SOURCE[0]}")/one.sh"\n'
                              '. "$(dirname -- "$(realpath -e -- "${BASH_SOURCE[0]}")")/two.sh"\n'
                              'HELPER="$(dirname -- "$0")/three.py"\n'
                              '# source "$(dirname "${BASH_SOURCE[0]}")/four.sh"\n'
                              'HERE=$(cd "$(dirname "$0")" && pwd)\n'
                              'LIB="$HERE/lib"\n'
                              'source "$HERE/five.sh"\n'
                              '"${LIB}/six.sh" --check\n'
                              'echo "bootstrap-stack.sh writes the record"\n')
            self.assertEqual(pc.siblings(script), ["five.sh", "one.sh", "six.sh", "three.py", "two.sh"])
            module = Path(scratch) / "y.py"
            module.write_text('import os\nfrom pathlib import Path\n'
                              'HELPER = Path(__file__).parent / "seven.py"\n'
                              'here = os.path.dirname(os.path.abspath(__file__))\n'
                              'TOOL = os.path.join(here, "eight.sh")\n'
                              '# os.path.join(here, "nine.py")\n'
                              'print("run update-stack.sh first")\n')
            self.assertEqual(pc.siblings(module), ["eight.sh", "seven.py"])

    def test_a_change_to_the_scripts_own_directory_is_refused(self):
        """A cd or pushd to the script's own directory, by its anchor or a name
        assigned from it, or os.chdir to it in Python, is refused naming the
        line, since a relative path after it reaches a sibling this reader
        cannot name. The `HERE=$(cd ... && pwd)` idiom, which changes only the
        substitution's directory, and a cd elsewhere are read as before.
        Perturbation: read no change of directory, and each passes."""
        pc = program_check()
        with tempfile.TemporaryDirectory() as scratch:
            for name, text, line in (
                    ("a.sh", 'cd "$(dirname "$0")"\nbash ./x.sh\n', 'cd "$(dirname "$0")"'),
                    ("b.sh", 'HERE=$(dirname "$0")\ntrue && cd "$HERE" && bash ./x.sh\n', 'true && cd "$HERE"'),
                    ("c.sh", 'pushd "${BASH_SOURCE%/*}"\nsource ./x.sh\n', 'pushd "${BASH_SOURCE%/*}"'),
                    ("d.py", 'import os\nhere = os.path.dirname(__file__)\nos.chdir(here)\n', "os.chdir(here)")):
                with self.subTest(file=name):
                    path = Path(scratch) / name
                    path.write_text(text)
                    with self.assertRaises(ValueError) as refused:
                        pc.siblings(path)
                    self.assertIn("a change to the script's own directory", str(refused.exception))
                    self.assertIn(line, str(refused.exception))
            path = Path(scratch) / "e.sh"
            path.write_text('HERE=$(cd "$(dirname "$0")" && pwd)\nsource "$HERE/x.sh"\n'
                            "sh -c 'cd \"$1\" && ls' _ /tmp\n")
            self.assertEqual(pc.siblings(path), ["x.sh"])


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


class LifecycleAccountLookTests(unittest.TestCase):
    """**A look as an account is the shell's own `[`, and a refused drop is
    its own answer** (#577): verify-lifecycle.sh's `as_look` answers yes or
    no as the builtin answers for the account, so a group-blind external
    `test` (here one first on the drop's PATH that answers no to everything,
    reached as sudo reaches it, by exec) changes no answer, and a drop sudo refuses
    answers `refused`, so an expectation of `no` fails on it. Run as the
    script defines them, with a stand-in for sudo. Perturbations: make
    `as_look` run `test "$2" "$3"` as the account, and the readable file
    reads no; answer `no` on a refused drop, and the expectation passes."""

    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory(prefix="weaver-looks-")
        self.addCleanup(self.scratch.cleanup)
        root = Path(self.scratch.name)
        self.bin_dir = root / "bin"
        self.bin_dir.mkdir()
        for name, body in (
            ("sudo", '[ "$1" = "-n" ] && shift\n'
                     'if [ "$1" = "-u" ]; then [ ! -e "${0%/*}/drop-refused" ] || exit 1; shift 2; fi\n'
                     'exec env "$@"\n'),
            ("test", "exit 1\n"),
        ):
            stand_in = self.bin_dir / name
            stand_in.write_text("#!/bin/sh\n" + body)
            stand_in.chmod(0o755)
        # The drop's PATH is this directory (B3): the stand-in `test` first,
        # beside the box's sh, and the timeout, setsid and env the operator's
        # drop runs under.
        for tool in ("sh", "timeout", "setsid", "env"):
            (self.bin_dir / tool).symlink_to(shutil.which(tool, path=BOX_PATH))
        self.file = root / "agent.toml"
        self.file.write_text("declared\n")
        script = (DEPLOY / "verify-lifecycle.sh").read_text()
        self.functions = ("set -euo pipefail\n" + 'STEP_LABEL="step 2 (test)"\n'
                          + lifecycle_functions(script, "fail", "pass", "expect_eq", "as_look"))

    def run_program(self, body, refused=False):
        # The drop clears the environment (#590), so a refused drop is a file
        # beside the stand-in.
        flag = self.bin_dir / "drop-refused"
        if refused:
            flag.touch()
        else:
            flag.unlink(missing_ok=True)
        full = {**os.environ, "PATH": str(self.bin_dir) + os.pathsep + os.environ["PATH"],
                "DROP_PATH": str(self.bin_dir)}
        full.pop("BASH_ENV", None)
        return subprocess.run(["bash", "-c", self.functions + body], env=full, text=True,
                              capture_output=True, timeout=20)

    def test_the_answers_are_the_builtins(self):
        for flag, path, said in (("-r", self.file, "yes"), ("-e", self.file, "yes"),
                                 ("-e", self.file.with_name("none"), "no")):
            with self.subTest(flag=flag, path=path.name):
                result = self.run_program(f'as_look weaver-m1-state {flag} {shlex.quote(str(path))}\n')
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(result.stdout, said)

    def test_an_expected_no_fails_on_a_refused_drop(self):
        result = self.run_program(
            f'expect_eq "the member cannot read it" no "$(as_look weaver-m1-state -e {shlex.quote(str(self.file.with_name("none")))})"\n'
            'echo GOES-ON\n', refused=True)
        self.assertEqual(result.returncode, 1, result.stdout)
        self.assertIn("found:    refused", result.stderr)
        self.assertNotIn("GOES-ON", result.stdout)


class LifecycleGroupLookupTests(unittest.TestCase):
    """**A group lookup that fails is its own answer** (CodeRabbit on #533):
    `holds_group` answers `unreadable` where `id -Gn` fails, so an
    expectation of `no` fails on it, and step 5b's `restore_rule` attempts
    the operator's removal unless the answer is `no`, so gpasswd's refusal
    fails the restore rather than a skipped removal passing it. Run as the
    script defines them, with stand-ins for sudo, id and gpasswd.
    Perturbation: put back `$(id -Gn "$1" 2>/dev/null || true)` and the
    failed lookup answers `no`, the expectation passes and the restore
    skips the removal."""

    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory(prefix="weaver-groups-")
        self.addCleanup(self.scratch.cleanup)
        root = Path(self.scratch.name)
        self.bin_dir = root / "bin"
        self.bin_dir.mkdir()
        self.calls = root / "gpasswd-calls"
        for name, body in (
            ("sudo", '[ "$1" = "-n" ] && shift\nexec env "$@"\n'),
            ("id", '[ -n "$ID_GROUPS" ] || exit 1\nprintf "%s\\n" "$ID_GROUPS"\n'),
            ("gpasswd", 'printf "%s\\n" "$*" >>"$GPASSWD_CALLS"\nexit "${GPASSWD_RC:-0}"\n'),
        ):
            stand_in = self.bin_dir / name
            stand_in.write_text("#!/bin/sh\n" + body)
            stand_in.chmod(0o755)
        self.decl = root / "agent.toml"
        self.decl.write_text("moved\n")
        script = (DEPLOY / "verify-lifecycle.sh").read_text()
        self.functions = ("set -euo pipefail\n"
                          + f'STEP_LABEL="step 5b (test)"; OPERATOR=op; AU=weaver-m1; DECL={shlex.quote(str(self.decl))}\n'
                          + lifecycle_functions(script, "fail", "pass", "expect_eq", "holds_group", "restore_rule"))

    def run_program(self, body, **env):
        full = {**os.environ, "PATH": str(self.bin_dir) + os.pathsep + os.environ["PATH"],
                "GPASSWD_CALLS": str(self.calls), "ID_GROUPS": "", **env}
        full.pop("BASH_ENV", None)
        return subprocess.run(["bash", "-c", self.functions + body], env=full, text=True,
                              capture_output=True, timeout=20)

    def test_the_answers_read_the_group_database(self):
        for groups, said in (("op weaver-m1-trace weaver-m1", "yes"),
                             ("op weaver-m1-trace", "no"),
                             ("", "unreadable")):
            with self.subTest(groups=groups):
                result = self.run_program('holds_group op weaver-m1\n', ID_GROUPS=groups)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(result.stdout, said)

    def test_an_expected_no_fails_on_an_unreadable_lookup(self):
        result = self.run_program('expect_eq "op is out of weaver-m1" no "$(holds_group op weaver-m1)"\n'
                                  'echo GOES-ON\n')
        self.assertEqual(result.returncode, 1, result.stdout)
        self.assertIn("found:    unreadable", result.stderr)
        self.assertNotIn("GOES-ON", result.stdout)

    def test_the_restore_removes_unless_the_answer_is_no(self):
        original = base64.b64encode(b"original\n").decode()
        for groups, gpasswd_rc, rc, removed in (("op weaver-m1", "0", 0, True),
                                                ("op", "0", 0, False),
                                                ("", "0", 0, True),
                                                ("", "3", 1, True)):
            with self.subTest(groups=groups, gpasswd_rc=gpasswd_rc):
                self.calls.unlink(missing_ok=True)
                self.decl.write_text("moved\n")
                result = self.run_program(f'rc=0; restore_rule {original} || rc=$?; echo "rc=$rc"\n',
                                          ID_GROUPS=groups, GPASSWD_RC=gpasswd_rc)
                self.assertIn(f"rc={rc}", result.stdout, result.stderr)
                self.assertEqual(self.decl.read_text(), "original\n")
                self.assertEqual(self.calls.exists(), removed)
                if removed:
                    self.assertEqual(self.calls.read_text(), "-d op weaver-m1\n")


class LifecycleArchiveReadBackTests(unittest.TestCase):
    """**The take-down's archives are read back whole** (the first live run,
    2026-10-13): step 9's two read-back lines, lifted from the script as it
    ships, run under `set -euo pipefail` against a real tar of 600 entries,
    more than one write's worth of listing, through a stand-in sudo. A
    listing piped into `grep -q .` closed the pipe at the first entry, tar
    died on its next write and `pipefail` made the check "no" for a sound
    archive. Perturbation: put the `| grep -q .` shape back at either line,
    and it fails here; a missing archive still fails."""

    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory(prefix="weaver-readback-")
        self.addCleanup(self.scratch.cleanup)
        root = Path(self.scratch.name)
        self.bin_dir = root / "bin"
        self.bin_dir.mkdir()
        sudo = self.bin_dir / "sudo"
        sudo.write_text('#!/bin/sh\n[ "$1" = "-n" ] && shift\nexec env "$@"\n')
        sudo.chmod(0o755)
        entries = root / "weaver-m1"
        entries.mkdir()
        for i in range(600):
            (entries / f"entry-{i}").write_text("")
        self.tar = root / "m1.tar"
        subprocess.run(["tar", "-C", str(root), "-cf", str(self.tar), "weaver-m1"], check=True)
        script = (DEPLOY / "verify-lifecycle.sh").read_text()
        self.checks = [line.strip() for line in script.splitlines() if "archive reads back" in line
                       and line.strip().startswith("expect_eq")]
        self.assertEqual(len(self.checks), 2, self.checks)
        self.functions = lifecycle_functions(script, "fail", "pass", "expect_eq", "archive_reads_back")

    def run_checks(self, tar):
        program = ("set -euo pipefail\n" + 'STEP_LABEL="step 9 (test)"\n'
                   + f"TERRITORY_TAR={shlex.quote(str(tar))}; ROOT_TAR={shlex.quote(str(tar))}\n"
                   + self.functions + "\n".join(self.checks) + "\necho READ-BACK\n")
        env = {**os.environ, "PATH": str(self.bin_dir) + os.pathsep + os.environ["PATH"]}
        env.pop("BASH_ENV", None)
        return subprocess.run(["bash", "-c", program], env=env, text=True,
                              capture_output=True, timeout=20)

    def test_a_sound_archive_reads_back(self):
        result = self.run_checks(self.tar)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("READ-BACK", result.stdout)

    def test_a_missing_archive_does_not(self):
        result = self.run_checks(self.tar.with_name("absent.tar"))
        self.assertEqual(result.returncode, 1, result.stdout)
        self.assertIn("archive reads back", result.stderr)


class LifecycleOperatorShowTests(unittest.TestCase):
    """**The take-down's operator check expects `unauthorized`** (the cleanup
    run of 2026-10-13): admin run without root performs nothing and answers
    `unauthorized` for every name, existing or not (weaver-admin-Spec,
    "Authorization is the kernel's"), so step 9's check of `show` as the
    operator, lifted from the script as it ships, passes a stand-in admin
    answering `unauthorized` and fails one answering `no_such_agent`.
    Perturbation: put back the expectation of `no_such_agent`, and both cases
    fail here."""

    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory(prefix="weaver-operator-show-")
        self.addCleanup(self.scratch.cleanup)
        self.admin = Path(self.scratch.name) / "weaver-admin"
        script = (DEPLOY / "verify-lifecycle.sh").read_text()
        lines = script.splitlines()
        at = [i for i, line in enumerate(lines) if "show as the operator answers" in line]
        self.assertEqual(len(at), 1, at)
        self.assertTrue(lines[at[0] - 1].strip().startswith("said="), lines[at[0] - 1])
        self.check = "\n".join(line.strip() for line in lines[at[0] - 1:at[0] + 1])
        self.functions = lifecycle_functions(script, "fail", "pass", "expect_eq", "json_at")

    def run_check(self, kind):
        self.admin.write_text(f"#!/bin/sh\nprintf '%s\\n' '{{\"kind\":\"{kind}\"}}'\nexit 1\n")
        self.admin.chmod(0o755)
        program = ("set -euo pipefail\n" + 'STEP_LABEL="step 9 (test)"; A=m1; ADMIN_BASE=/nonexistent\n'
                   + f"ADMIN={shlex.quote(str(self.admin))}\n"
                   + self.functions + self.check + "\necho CHECKED\n")
        env = {**os.environ}
        env.pop("BASH_ENV", None)
        return subprocess.run(["bash", "-c", program], env=env, text=True,
                              capture_output=True, timeout=20)

    def test_unauthorized_passes(self):
        result = self.run_check("unauthorized")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("CHECKED", result.stdout)

    def test_no_such_agent_fails(self):
        result = self.run_check("no_such_agent")
        self.assertEqual(result.returncode, 1, result.stdout)
        self.assertIn("show as the operator answers", result.stderr)


class LifecycleEnvelopeFieldTests(unittest.TestCase):
    """**verify-lifecycle.sh reads an envelope field from the envelope** (the
    full run of 2026-10-13, stopped in step 2): weaver-trace-Spec section 3
    flattens `Envelope` into the event's top level, so `run`, `session` and
    the rest stand beside `payload`, never inside it. `json_at` answers
    `absent` for a missing path, so a read of `payload.run` handed the run
    marker's check the string `absent`. The envelope's fields are read from
    the Spec's `Envelope` struct, and every `json_at` read of a load event
    (`$load` or `$(last_event load)`) is checked for one of them under
    `payload.`. The pin is scoped to the load event because another kind's
    payload may carry a field of the same name as its own (a save point's
    payload carries `run`, `sequence` and `turn`), which a check may read
    rightly; the load's payload carries none of them. The first load's read
    of the run is lifted and run against an event of that shape.
    Perturbation: put `payload.run` back at the six `RUN=` lines and the
    static pin names them and the lifted read answers `absent`."""

    def setUp(self):
        self.script = (DEPLOY / "verify-lifecycle.sh").read_text()
        spec = (DEPLOY.parent / "docs/crates/weaver-trace/weaver-trace-Spec.md").read_text()
        body = re.search(r"pub struct Envelope \{(.*?)\n\}", spec, re.S)
        self.assertIsNotNone(body, "the Spec's Envelope struct")
        self.fields = re.findall(r"pub (\w+):", body.group(1))
        self.assertIn("run", self.fields)
        self.assertIn("monotonic_ns", self.fields)

    def test_no_read_of_a_load_event_takes_an_envelope_field_from_its_payload(self):
        reader = (r'(?:json_(?:at|req \w+)|expect_at "[^"]*" "[^"]*")'
                  r' "(?:\$load|\$\(last_event load\))" ')
        under = re.compile(reader + r'"?payload\.(' + "|".join(self.fields) + r")\b")
        reads = [line for line in self.script.splitlines() if re.search(reader, line)]
        self.assertGreaterEqual(len(reads), 6, "the load event's reads")
        found = [f"{n}: {line.strip()}" for n, line in enumerate(self.script.splitlines(), 1)
                 if under.search(line)]
        self.assertEqual(found, [], "an envelope field read under a load event's payload")

    def test_the_first_loads_run_is_read_from_the_envelope(self):
        lines = [line.strip() for line in self.script.splitlines()
                 if line.strip().startswith('json_req RUN "$load"')]
        self.assertTrue(lines, "the RUN reads")
        event = json.dumps({"kind": "load", "run": "r-envelope", "sequence": 0, "session": "s-1",
                            "subsystem": "admin", "wall_ms": 1, "monotonic_ns": 1,
                            "payload": {"boundary": "b", "cause": {"uid": 0}, "declaration": "d"}})
        program = (lifecycle_functions(self.script, "fail", "json_at", "json_req")
                   + f"load={shlex.quote(event)}\n" + lines[0] + '\nprintf "%s\\n" "$RUN"\n')
        result = subprocess.run(["bash", "-c", program], text=True, capture_output=True, timeout=20)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.strip(), "r-envelope")


class LifecycleRequiredReadTests(unittest.TestCase):
    """**A value a check compares is required** (#139): `json_at` answers
    `absent` for a missing path and `unparsed` for text that is not JSON,
    so two missing reads compared equal and the check passed. `json_req`,
    lifted with its helpers as the script ships them, answers a present
    value and fails the step naming the path on either. The event is a
    load event in the shape a real trace writes (the envelope flattened
    beside `payload`, weaver-trace-Spec section 3), and the unparsed text
    is what `marker()` answers when the marker cannot be read.
    Perturbation: let `json_req` pass `absent` through and the missing
    cases are answered rather than failed; read `RUN` or a compared value
    through `json_at` again and the static pins name the line."""

    EVENT = json.dumps({"kind": "load", "run": "r-2", "sequence": 0, "session": "s-1",
                        "subsystem": "admin", "wall_ms": 1, "monotonic_ns": 1,
                        "payload": {"boundary": "b", "cause": {"uid": 0}, "declaration": "d",
                                    "lineage": {"run": "r-1", "save_point": "ab",
                                                "named_at_restore": False}}})

    def setUp(self):
        self.script = (DEPLOY / "verify-lifecycle.sh").read_text()

    def read(self, text, path, check=""):
        program = (lifecycle_functions(self.script, "fail", "json_at", "json_req")
                   + 'STEP_LABEL="step 4 (fixture)"\n'
                   + f"json_req value {shlex.quote(text)} {shlex.quote(path)} {shlex.quote(check) if check else ''}\n"
                   + 'printf "%s\\n" "$value"\n')
        return subprocess.run(["bash", "-c", program], text=True, capture_output=True, timeout=20)

    def test_a_present_value_is_answered(self):
        result = self.read(self.EVENT, "payload.lineage.run")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.strip(), "r-1")

    def test_a_missing_path_or_unparsed_text_fails_the_step_naming_the_path(self):
        null = json.dumps({"kind": "load", "run": None, "payload": {"lineage": {"save_point": ""}}})
        for text, path, found in ((self.EVENT, "payload.reset.prior_run", "absent"),
                                  ("absent", "run", "unparsed"),
                                  (null, "run", "null"),
                                  (null, "payload.lineage.save_point", "empty")):
            with self.subTest(path=path):
                result = self.read(text, path)
                self.assertEqual(result.returncode, 1)
                self.assertNotIn(found + "\n", result.stdout.split("== FAIL")[-1],
                                 "nothing is answered past the failure")
                self.assertIn(f"a value at {path}", result.stderr)
                self.assertIn(f"found:    {found}", result.stderr)

    def test_a_failed_read_names_the_check_it_serves(self):
        result = self.read(self.EVENT, "payload.reset.prior_run", "the reset names the crashed run")
        self.assertEqual(result.returncode, 1)
        self.assertIn("check:    the reset names the crashed run", result.stderr)

    def test_no_run_or_compared_value_is_read_through_json_at(self):
        found = []
        for number, line in enumerate(self.script.splitlines(), 1):
            text = line.strip()
            if re.match(r"(?:local \w+; )?\w+=\$\(json_at\b", text) and re.match(r"(?:local \w+; )?(RUN|dig|SP_A_DIG)=", text):
                found.append(f"{number}: {text}")
            # A check whose expected side is a variable or the manifest's
            # field compares two reads, so its found side is required too.
            if re.match(r'expect_eq "[^"]*" "\$(?:\w+|\(manifest_field [^)]*\))" "\$\(json_at', text):
                found.append(f"{number}: {text}")
            # A test comparing a json_at read against a variable is the same
            # comparison written another way.
            if re.search(r'\[ "\$\(json_at [^]]*\)" !?= "\$', text) or re.search(r'\[ "\$\w+" !?= "\$\(json_at', text):
                found.append(f"{number}: {text}")
        self.assertEqual(found, [], "a compared value read through json_at")


class LifecycleRoomCountTests(unittest.TestCase):
    """**The run prints the largest room count admin.log records** (S16, the
    operator's ruling of 2026-10-14 on #140): `room_count_report`, lifted
    with its helpers as the script ships them, reads admin.log through a
    stand-in `sudo` and prints one MEASURED line, never a check. The lines
    are in admin.log's own shape (admin's `publish` lines, a load's, and the
    relay's `ended` line, which carries no `verb`). Perturbation: keep the
    first count rather than the largest and the line names 1; drop the
    past-the-bound reading and that case reads as a count; mark a scan past
    the bound before its count reads and a malformed past-the-bound line
    makes a later ordinary count read as past the bound (Codex on #155)."""

    def line(self, verb, outcome):
        return json.dumps({"agent": "m1", "command": f"weaver-admin {verb} m1", "outcome": outcome,
                           "uid": 1000, "verb": verb, "wall_ms": 1}, separators=(",", ":"))

    RELAY = ('{"actor":"relay","agent":"m1","boundary":"b","detail":"the worker exited",'
             '"event":"ended","wall_ms":2}')

    def report(self, lines):
        scratch = tempfile.TemporaryDirectory(prefix="weaver-room-count-")
        self.addCleanup(scratch.cleanup)
        root = Path(scratch.name)
        bin_dir = root / "bin"
        bin_dir.mkdir()
        sudo = bin_dir / "sudo"
        sudo.write_text('#!/bin/sh\n[ "$1" = "-n" ] && shift\nexec env "$@"\n')
        sudo.chmod(0o755)
        territory = root / "weaver-m1"
        territory.mkdir()
        (territory / "admin.log").write_text("".join(l + "\n" for l in lines))
        script = (DEPLOY / "verify-lifecycle.sh").read_text()
        program = ("set -euo pipefail\nAPPLY=1\n"
                   + f"T={shlex.quote(str(territory))}\n"
                   + "plan() { printf '   %s\\n' \"$*\"; }\n"
                   + lifecycle_functions(script, "fail", "measured", "json_at", "json_req",
                                         "room_count_report")
                   + "room_count_report\n")
        env = {**os.environ, "PATH": str(bin_dir) + os.pathsep + os.environ["PATH"]}
        env.pop("BASH_ENV", None)
        return subprocess.run(["bash", "-c", program], env=env, text=True,
                              capture_output=True, timeout=20)

    def test_the_largest_count_is_printed(self):
        result = self.report([
            self.line("load", "idle"),
            self.line("publish", "the room holds 1 finished names"),
            self.RELAY,
            self.line("publish", "the room holds 3 finished names"),
            self.line("publish", "<name> ordinal 2 at the leave"),
            self.line("publish", "the room holds 2 finished names"),
        ])
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("MEASURED the room's count: the largest room count over 3 scans: 3 finished names",
                      result.stdout)
        self.assertNotIn("FAIL", result.stdout)

    def test_a_scan_past_the_bound_and_a_log_with_none(self):
        result = self.report([self.line("publish", "the room holds more than 4096 finished names")])
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("more than 4096 finished names, past the bound", result.stdout)
        result = self.report([self.line("load", "idle"), self.RELAY])
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("admin.log records no room scan", result.stdout)

    def test_a_count_that_does_not_read_is_said_and_passed_over(self):
        result = self.report([
            self.line("publish", "the room holds  finished names"),
            self.line("publish", "the room holds 2 finished names"),
        ])
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("a publish line's count does not read", result.stdout)
        self.assertIn("over 1 scans: 2 finished names", result.stdout)

    def test_a_past_bound_line_that_does_not_read_marks_nothing_past(self):
        result = self.report([
            self.line("publish", "the room holds more than x finished names"),
            self.line("publish", "the room holds 2 finished names"),
        ])
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("a publish line's count does not read", result.stdout)
        self.assertIn("over 1 scans: 2 finished names", result.stdout)
        self.assertNotIn("past the bound", result.stdout)

    def test_the_report_runs_before_the_take_down_on_both_paths(self):
        script = (DEPLOY / "verify-lifecycle.sh").read_text()
        tail = script[script.index('if [ "$KEEP" -eq 1 ]; then\n  STEP_LABEL='):]
        self.assertEqual(tail.count("room_count_report\n"), 2)
        self.assertLess(tail.rindex("room_count_report\n"), tail.rindex("\nstep_9\n"))


class LifecycleForcedLogTests(unittest.TestCase):
    """**Step 6b reads admin's own writes to admin.log, not the file's
    growth** (the second full run, 2026-10-13): the force on the drifted
    territory writes no line of admin's (3.0 outcome row 2), but it ends the
    worker, and the trace relay, holding the descriptor admin handed it at
    the judged load, writes its `ended` line (weaver-admin-Spec section 8).
    The check, lifted from the script as it ships with its helper, runs
    against a log whose appended tail is (a) the relay's `ended` line alone,
    which passes, and (b) an admin verb line, which fails naming it.
    Perturbation: put back the file-size comparison and (a) fails."""

    RELAY = ('{"actor":"relay","agent":"m1","boundary":"b","detail":"the worker exited",'
             '"event":"ended","wall_ms":2}')
    ADMIN = ('{"agent":"m1","command":"weaver-admin force-unload m1","outcome":"unloaded",'
             '"uid":1000,"verb":"force-unload","wall_ms":3}')

    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory(prefix="weaver-forced-log-")
        self.addCleanup(self.scratch.cleanup)
        root = Path(self.scratch.name)
        self.bin_dir = root / "bin"
        self.bin_dir.mkdir()
        sudo = self.bin_dir / "sudo"
        sudo.write_text('#!/bin/sh\n[ "$1" = "-n" ] && shift\nexec env "$@"\n')
        sudo.chmod(0o755)
        self.territory = root / "weaver-m1"
        self.territory.mkdir()
        self.log = self.territory / "admin.log"
        self.before = ('{"agent":"m1","command":"weaver-admin load m1","outcome":"idle",'
                       '"uid":1000,"verb":"load","wall_ms":1}\n')
        script = (DEPLOY / "verify-lifecycle.sh").read_text()
        checks = [line.strip() for line in script.splitlines()
                  if "admin.log line" in line and line.strip().startswith("expect_eq")]
        self.assertEqual(len(checks), 1, checks)
        self.check = checks[0]
        self.functions = lifecycle_functions(script, "fail", "pass", "expect_eq", "file_size",
                                             "not_relay_lines_since")

    def run_check(self, appended):
        self.log.write_text(self.before)
        program = ("set -euo pipefail\n" + 'STEP_LABEL="step 6b (test)"\n'
                   + f"T={shlex.quote(str(self.territory))}\n" + self.functions
                   + 'size=$(file_size "$T/admin.log")\n'
                   + f"printf '%s\\n' {shlex.quote(appended)} >>\"$T/admin.log\"\n"
                   + self.check + "\necho CHECKED\n")
        env = {**os.environ, "PATH": str(self.bin_dir) + os.pathsep + os.environ["PATH"]}
        env.pop("BASH_ENV", None)
        return subprocess.run(["bash", "-c", program], env=env, text=True,
                              capture_output=True, timeout=20)

    def test_the_relays_ended_line_alone_passes(self):
        result = self.run_check(self.RELAY)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("CHECKED", result.stdout)

    def test_an_admin_verb_line_fails_naming_it(self):
        result = self.run_check(self.ADMIN)
        self.assertEqual(result.returncode, 1, result.stdout)
        self.assertIn("admin wrote no admin.log line", result.stderr)
        self.assertIn('"verb":"force-unload"', result.stderr)


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
                        + lifecycle_functions(script, "fail", "pass", "expect_eq", "json_at", "json_req", "expect_at", "ask",
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


class LifecycleBoxGroupsTests(unittest.TestCase):
    """**Step 1 judges the box accounts by create-agent.sh's allow-list**
    (the Planner's grade of ac623d58, the sweep): a group outside it, in the
    weaver- namespace or not, is named. Run alone with id as a stand-in.
    Perturbation: let a non-weaver group through and the foreign cases pass."""

    def test_groups_outside_the_allow_list_are_named(self):
        scratch = tempfile.TemporaryDirectory(prefix="weaver-box-groups-")
        self.addCleanup(scratch.cleanup)
        bin_dir = Path(scratch.name)
        (bin_dir / "id").write_text('#!/bin/sh\nprintf "%s\\n" "$HELD"\n')
        (bin_dir / "id").chmod(0o755)
        script = (DEPLOY / "verify-lifecycle.sh").read_text()
        program = ("GATECON=weaver-gatecon; ADMINCON=weaver-admincon\n"
                   + lifecycle_functions(script, "box_groups_outside") + 'box_groups_outside "$1"')
        env = {**os.environ, "PATH": str(bin_dir) + os.pathsep + os.environ["PATH"]}
        env.pop("BASH_ENV", None)
        for account, held, said in (
                ("weaver-gatecon", "weaver-gatecon weaver-m1 weaver-my-bot", "none"),
                ("weaver-gatecon", "weaver-gatecon weaver-m1 wheel", "wheel"),
                ("weaver-gatecon", "weaver-gatecon weaver-m1-relay weaver-m1-state", "weaver-m1-relay weaver-m1-state"),
                ("weaver-gatecon", "weaver-gatecon weaver-admincon weaver-old-admincon", "weaver-admincon weaver-old-admincon"),
                ("weaver-admincon", "weaver-admincon weaver-m1-state weaver-m1-admin", "none"),
                ("weaver-admincon", "weaver-admincon weaver-m1-state docker", "docker"),
                ("weaver-admincon", "weaver-admincon weaver-m1 weaver-m1-trace weaver-gatecon",
                 "weaver-m1 weaver-m1-trace weaver-gatecon")):
            with self.subTest(account=account, held=held):
                run = subprocess.run(["bash", "-c", program, "x", account], env={**env, "HELD": held},
                                     text=True, capture_output=True, timeout=20)
                self.assertEqual(run.stdout, said, run.stderr)
        self.assertIn('expect_eq "$u holds no group outside what the box\'s account may hold" none', script)


class LifecycleUnseededTurnTests(unittest.TestCase):
    """**An unseeded agent's turn is read by the seeding's mark, not by the
    kind** (the operator's live run of 2026-11-10 on a throwaway agent, which
    stopped in step 2): the check refused any `message.system`, and the run's
    one turn, answered, recorded `turn.started message.system message.user
    message.assistant turn.closed`, the system message the loop's compiled
    prompt, led into the first turn because nothing was seated
    (Loops/basic-inference-loop.md section 6, ADP-4). A seeding is the
    turnless `message.system` the identity door authors (weaver-harness-Spec
    section 6.1), so `unseeded_turn` refuses a turnless one and asks the
    bracket to be led by a `message.system` of the turn. The events are in
    the envelope shape of the recorded trace
    `crates/weaver-state/src/engine/recorded-karl.ndjson` (a turnless event
    carries no `turn` key, sequences as strings), the run's own kinds and
    order as the run printed them. Steps 4 and 5b asserted a recall after a
    load, which the harness writes only for an identity answer that returns
    events (weaver-trace-Spec section 3, `lifecycle.rs` skipping an empty
    answer), so they now want none, read beside the load event so a read that
    failed answers `unread` rather than an empty count (the self-check of
    2026-11-10, which also asked for a broken bracket, an open never closed
    and messages of a turn the bracket does not name to fail).
    Perturbation: put back `"message.system" not in ks` and the run's shape
    fails; drop the turnless test and the seeded shapes pass; drop the lead
    test and a seated turn passes; drop the open-at-the-end test, the stray
    close test or the bracket's own turn from the comparison and its case
    passes; drop the load witness and a failed read answers `none`."""

    def setUp(self):
        self.script = (DEPLOY / "verify-lifecycle.sh").read_text()

    @staticmethod
    def event(kind, sequence, turn="t-1", **payload):
        e = {"session": "s-vtest-1", "run": "2026-11-10T23:00:00.000Z-vtest1-0123456789abcdef",
             "sequence": str(sequence), "kind": kind, "subsystem": "harness",
             "wall_ms": 1794351600000 + sequence, "monotonic_ns": str(1000 * sequence)}
        if turn is not None:
            e["turn"] = turn
        if payload:
            e["payload"] = payload
        return json.dumps(e, separators=(",", ":"), sort_keys=True)

    def judge(self, name, events):
        program = lifecycle_functions(self.script, name) + name + "\n"
        run = subprocess.run(["bash", "-c", program], input="".join(e + "\n" for e in events),
                             text=True, capture_output=True, timeout=20)
        self.assertEqual(run.returncode, 0, run.stderr)
        return run.stdout.strip()

    def test_the_runs_unseeded_turn_reads_as_one(self):
        ev = self.event
        the_run = [ev("turn.started", 1), ev("message.system", 2), ev("message.user", 3),
                   ev("message.assistant", 4), ev("turn.closed", 5)]
        self.assertEqual(self.judge("unseeded_turn", the_run), "one unseeded turn")

    def test_a_seeding_or_a_wrong_bracket_is_named(self):
        ev = self.event
        turn = [ev("turn.started", 5, "t-2"), ev("message.system", 6, "t-2"),
                ev("message.user", 7, "t-2"), ev("message.assistant", 8, "t-2"),
                ev("turn.closed", 9, "t-2")]
        seeding = [ev("message.system", 1, None), ev("turn.started", 2),
                   ev("message.assistant", 3), ev("turn.closed", 4)]
        for name, events, found in (
                ("a seeding, then a turn", seeding + turn,
                 "message.system(turnless) turn.started message.assistant turn.closed turn.started"
                 " message.system message.user message.assistant turn.closed"),
                ("a turnless system message before the turn", [ev("message.system", 1, None)] + turn,
                 "message.system(turnless) turn.started message.system message.user"
                 " message.assistant turn.closed"),
                ("a seated turn, no loop prompt",
                 [ev("turn.started", 1), ev("message.user", 2), ev("message.assistant", 3),
                  ev("turn.closed", 4)],
                 "turn.started message.user message.assistant turn.closed"),
                ("the loop prompt after the user", [ev("turn.started", 1), ev("message.user", 2),
                                                    ev("message.system", 3), ev("message.assistant", 4),
                                                    ev("turn.closed", 5)],
                 "turn.started message.user message.system message.assistant turn.closed"),
                ("a system message of another turn",
                 [ev("turn.started", 1), ev("message.system", 2, "t-9"), ev("message.user", 3),
                  ev("message.assistant", 4), ev("turn.closed", 5)],
                 "turn.started message.system message.user message.assistant turn.closed"),
                ("no answer", [ev("turn.started", 1), ev("message.system", 2), ev("message.user", 3),
                               ev("turn.closed", 4)],
                 "turn.started message.system message.user turn.closed"),
                ("two turns", turn + turn,
                 " ".join(["turn.started message.system message.user message.assistant turn.closed"] * 2)),
                ("a turn, then one left open", turn + turn[:3],
                 "turn.started message.system message.user message.assistant turn.closed"
                 " turn.started message.system message.user"),
                ("a close with no open", [ev("turn.closed", 0, "t-0")] + turn,
                 "turn.closed turn.started message.system message.user message.assistant turn.closed"),
                ("an open inside an open", turn[:1] + turn,
                 "turn.started turn.started message.system message.user message.assistant turn.closed"),
                ("the messages of a turn the bracket does not name",
                 [ev("turn.started", 1), ev("message.system", 2, "t-9"), ev("message.user", 3, "t-9"),
                  ev("message.assistant", 4, "t-9"), ev("turn.closed", 5)],
                 "turn.started message.system message.user message.assistant turn.closed"),
                ("an event with no kind", turn + [json.dumps({"turn": "t-2"})],
                 "turn.started message.system message.user message.assistant turn.closed None")):
            with self.subTest(name):
                self.assertEqual(self.judge("unseeded_turn", events), found)

    def test_identity_recalls_need_the_load_read_and_count_only_identity(self):
        ev = self.event
        load = ev("load", 1, None, declaration="d", boundary="b", cause={"uid": 0})
        identity = ev("recall", 2, None, ask={"verb": "identity"},
                      returned=[{"run": "r", "sequence": "1", "kind": "message.system"}])
        recall = ev("recall", 3, None, ask={"verb": "recall", "last_turns": 4},
                    returned=[{"run": "r", "turn": "t-1", "sequence": "3", "kind": "message.user"}])
        for name, events, said in (
                ("a failed read", [], "unread"),
                ("recalls with no load read", [recall], "unread"),
                ("the load alone", [load], "none"),
                ("the load and a recall of the recall verb", [load, recall], "none"),
                ("the load and an identity recall", [load, identity, recall], "1"),
                ("a recall whose payload is null", [load, ev("recall", 4, None).replace(
                    '"kind":"recall"', '"kind":"recall","payload":null')], "malformed 2"),
                ("a recall whose ask is a string", [load, ev("recall", 4, None, ask="identity")],
                 "malformed 2"),
                ("a line that is not JSON", [load, "{not json"], "malformed 2")):
            with self.subTest(name):
                self.assertEqual(self.judge("identity_recalls", events), said)

    def test_the_steps_read_through_them(self):
        self.assertIn("message.assistant turn.closed | unseeded_turn)", self.script)
        site = ('expect_eq "the load is read and wrote no identity recall (nothing seated)" none \\\n'
                '    "$(events_since "$start" load recall | identity_recalls)"')
        self.assertEqual(self.script.count(site), 2, "steps 4 and 5b")
        self.assertNotIn('[ -n "$(events_since "$start" recall)" ]', self.script)
        self.assertNotIn('"message.system" not in ks', self.script)


class LifecycleStep5bRestoreTests(unittest.TestCase):
    """**Step 5b's measurement is put back on every exit, the trap
    included** (the Planner's condition on the code act of #533): the
    declaration byte for byte and the operator out of the agent's own group,
    each attempted whatever the other answers. Run alone on a scratch
    declaration, with sudo, id and gpasswd as stand-ins. Perturbations: drop
    `restore_now` from `on_exit` and the interrupted case leaves the moved
    rule; chain the two puts with `&&` and the failed-write case never takes
    the operator out."""

    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory(prefix="weaver-lifecycle-5b-")
        self.addCleanup(self.scratch.cleanup)
        root = Path(self.scratch.name)
        self.bin_dir = root / "bin"
        self.bin_dir.mkdir()
        self.groups = root / "groups"
        self.calls = root / "calls"
        self.decl = root / "agent.toml"
        self.original = b'session = "m1-001"\n\n[gate-instruction.access-rule]\nallowed-uids = [990]\n'
        self.decl.write_bytes(self.original)
        stand_ins = {
            "sudo": '#!/bin/sh\n[ "$1" = "-n" ] && shift\nexec env "$@"\n',
            "id": f'#!/bin/sh\n[ "$1" = "-Gn" ] || exit 2\ncat {shlex.quote(str(self.groups))}\n',
            "gpasswd": (f'#!/bin/sh\nprintf "%s\\n" "$*" >> {shlex.quote(str(self.calls))}\n'
                        '[ -z "$GPASSWD_FAILS" ] || exit 1\n'
                        f'[ "$1" = "-d" ] && printf "op\\n" > {shlex.quote(str(self.groups))}\n'
                        f'[ "$1" = "-a" ] && printf "op weaver-m1\\n" > {shlex.quote(str(self.groups))}\n'
                        'exit 0\n'),
        }
        for name, body in stand_ins.items():
            (self.bin_dir / name).write_text(body)
            (self.bin_dir / name).chmod(0o755)
        script = (DEPLOY / "verify-lifecycle.sh").read_text()
        traps = [l for l in script.splitlines() if l.startswith("trap ")]
        self.preamble = ("set -euo pipefail\n"
                         + 'STEP_LABEL="step 5b (test)"; RESTORE=(); CREATED=0; LOGDIR=""; A=m1; AU=weaver-m1\n'
                         + f'OPERATOR=op; DECL={shlex.quote(str(self.decl))}\n'
                         + "export MAIN_PID=$$\n"
                         + lifecycle_functions(script, "holds_group", "restore_rule", "restore_now", "on_exit")
                         + "\n".join(traps) + "\n"
                         + 'orig=$(base64 -w0 -- "$DECL")\n'
                         + 'RESTORE=(restore_rule "$orig")\n'
                         + 'sudo -n gpasswd -a "$OPERATOR" "$AU" >/dev/null\n'
                         + "printf 'allowed-uids = [1000]\\n' | sudo -n tee -- \"$DECL\" >/dev/null\n")

    def run_program(self, tail, **env_extra):
        self.groups.write_text("op\n")
        env = {**os.environ, "PATH": str(self.bin_dir) + os.pathsep + os.environ["PATH"], **env_extra}
        env.pop("BASH_ENV", None)
        return subprocess.run(["bash", "-c", self.preamble + tail], env=env,
                              text=True, capture_output=True, timeout=20)

    def test_a_run_stopped_mid_measurement_is_put_back_by_the_trap(self):
        result = self.run_program('kill -TERM "$MAIN_PID"\nsleep 1\necho FINISHED\n')
        self.assertEqual(result.returncode, 130, result.stdout + result.stderr)
        self.assertNotIn("FINISHED", result.stdout)
        self.assertEqual(self.decl.read_bytes(), self.original)
        self.assertEqual(self.groups.read_text(), "op\n")
        self.assertIn("restored: restore_rule", result.stdout)

    def test_a_failed_check_exits_and_the_trap_puts_it_back(self):
        result = self.run_program('exit 1\n')
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertEqual(self.decl.read_bytes(), self.original)
        self.assertEqual(self.groups.read_text(), "op\n")

    def test_the_step_s_own_restore_leaves_nothing_for_the_trap(self):
        result = self.run_program('restore_now\necho FINISHED\n')
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(result.stdout.count("restored: restore_rule"), 1, result.stdout)
        self.assertEqual(self.decl.read_bytes(), self.original)
        self.assertEqual(self.calls.read_text().splitlines(), ["-a op weaver-m1", "-d op weaver-m1"])

    def test_a_failed_write_still_takes_the_operator_out(self):
        # The declaration's write fails (the file made unwritable after the
        # move), and the group's removal is still attempted and the failure
        # named for the hand.
        result = self.run_program(f'chmod 0444 {shlex.quote(str(self.decl))}\nexit 1\n')
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertEqual(self.groups.read_text(), "op\n")
        self.assertIn("-d op weaver-m1", self.calls.read_text())
        self.assertIn("RESTORE FAILED: restore_rule", result.stderr)

    def test_an_operator_already_out_is_not_removed_again(self):
        result = self.run_program(f'printf "op\\n" > {shlex.quote(str(self.groups))}\nrestore_now\n')
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertNotIn("-d op weaver-m1", self.calls.read_text())


# Admin's refusal of a load over a read entry on the trace, as the first live run
# after #108 met it (2026-10-15, ba3eb4e, the lifecycle run's tee): the cause on
# stderr, naming the trace, then the answer on stdout.
H5_CAUSE = "weaver-admin: the trace {trace}: carries an access-control entry beyond its mode"
H5_ANSWER = '{"kind":"boundary_unverified"}'


class LifecycleStep8bTests(unittest.TestCase):
    """**Step 8b asserts H5 closed** (#99 H5, closed by #108): with a read
    entry for the member on the trace, a load answers `boundary_unverified`.
    With the entry removed, the same agent loads, and that control pins the
    refusal to the entry without reading admin's prose. Admin's cause is
    recorded as measured, stderr being outside its interface (Codex on #171,
    round 2). The step runs alone, with sudo, setfacl and admin as stand-ins,
    admin answering its first load and its second in turn, and calls admin
    bare (weaver-admin-Spec 6.1).
    Perturbations: answer anything else with the entry, and the step fails with
    the entry removed and then the agent force-unloaded, its answer checked;
    drop the control load, and a refusal that was not the entry's passes."""

    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory(prefix="weaver-lifecycle-8b-")
        self.addCleanup(self.scratch.cleanup)
        root = Path(self.scratch.name)
        self.bin_dir = root / "bin"
        self.bin_dir.mkdir()
        self.calls = root / "calls"
        log = shlex.quote(str(self.calls))
        for tool, body in (
                ("sudo", '#!/bin/sh\n[ "$1" = "-n" ] && shift\nexec env "$@"\n'),
                ("setfacl", f'#!/bin/sh\necho "setfacl $1" >> {log}\n')):
            stand_in = self.bin_dir / tool
            stand_in.write_text(body)
            stand_in.chmod(0o755)
        self.admin = self.bin_dir / "weaver-admin"
        self.trace = root / "trace.ndjson"
        self.trace.write_text("")
        script = (DEPLOY / "verify-lifecycle.sh").read_text()
        self.program = (
            "set -euo pipefail\n"
            + 'APPLY=1; A=m1; AU=weaver-m1; RESTORE=(); LOGDIR=""\n'
            + f'ADMIN_BASE=/nonexistent; ADMIN={shlex.quote(str(self.admin))}; TRACE={shlex.quote(str(self.trace))}\n'
            + 'say() { printf "\\n== %s\\n" "$*"; }\nplan() { :; }\nrun_line() { :; }\nwant() { :; }\n'
            + f'expect_unloaded_alone() {{ echo "show alone" >> {log}; }}\nexpect_no_acl() {{ :; }}\nexpect_lay() {{ :; }}\n'
            + lifecycle_functions(script, "begin_step", "end_step", "fail", "pass", "measured", "expect_eq",
                                  "json_at", "json_req", "expect_at", "ask", "expect_kind", "restore_acl",
                                  "restore_now", "answered_state", "step_8b")
            + "step_8b\necho FINISHED\n")

    def run_step(self, load_answer, load_cause="", control='{"kind":"state","state":"idle"}',
                 unload='{"kind":"state","state":"unloaded"}'):
        """Admin answers its first load (the entry on the trace) with
        LOAD_ANSWER and LOAD_CAUSE on stderr, its second (the control) with
        CONTROL, and every force-unload with UNLOAD."""
        log = shlex.quote(str(self.calls))
        self.admin.write_text(
            "#!/bin/sh\n"
            f'loads=$(grep -c "^admin load" {log} 2>/dev/null || true)\n'
            f'echo "admin $1" >> {log}\n'
            'case "$1" in\n'
            '  load)\n'
            '    if [ "${loads:-0}" = 0 ]; then\n'
            f"      [ -z {shlex.quote(load_cause)} ] || printf '%s\\n' {shlex.quote(load_cause)} >&2\n"
            f"      printf '%s\\n' {shlex.quote(load_answer)}\n"
            f"    else printf '%s\\n' {shlex.quote(control)}; fi ;;\n"
            f"  force-unload) printf '%s\\n' {shlex.quote(unload)} ;;\n"
            "esac\n")
        self.admin.chmod(0o755)
        self.calls.unlink(missing_ok=True)
        env = {**os.environ, "PATH": str(self.bin_dir) + os.pathsep + os.environ["PATH"]}
        env.pop("BASH_ENV", None)
        return subprocess.run(["bash", "-c", self.program], env=env, text=True, capture_output=True, timeout=20)

    def calls_made(self):
        return self.calls.read_text().splitlines()

    def test_a_refusal_with_the_entry_and_a_load_without_it_pass_whatever_the_cause(self):
        """The live run's refusal, and causes worded otherwise or absent, all
        pass: the attribution is the control, and the cause is recorded."""
        for cause in (H5_CAUSE.format(trace=self.trace),
                      "weaver-admin: the declaration /x/agent.toml: carries an access-control entry beyond its mode",
                      ""):
            with self.subTest(cause=cause):
                result = self.run_step(H5_ANSWER, cause)
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                self.assertIn("PASS step 8b (#99 H5, closed by #108): load answers boundary_unverified", result.stdout)
                self.assertIn("PASS step 8b (#99 H5, closed by #108): the same agent loads with the entry removed, "
                              "so the refusal was the entry's", result.stdout)
                self.assertIn(f"MEASURED step 8b (#99 H5, closed by #108): admin's cause: {cause or 'nothing'}",
                              result.stdout)
                self.assertIn("FINISHED", result.stdout)
                self.assertEqual(self.calls_made(), ["show alone", "setfacl -m", "admin load", "setfacl -b",
                                                     "admin load", "admin force-unload", "show alone"])

    def test_a_refusal_without_the_entry_fails(self):
        """The control: a load that refuses with the entry removed too was not
        refused for the entry, and is taken down all the same. Perturbation:
        drop the control load, and this passes."""
        result = self.run_step(H5_ANSWER, H5_CAUSE.format(trace=self.trace), control=H5_ANSWER)
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("the same agent loads with the entry removed, so the refusal was the entry's", result.stderr)
        self.assertIn("found:    " + H5_ANSWER, result.stderr)
        self.assertIn("admin force-unload", self.calls_made())
        self.assertNotIn("FINISHED", result.stdout)

    def test_a_control_that_does_not_come_down_says_the_agent_may_stand(self):
        result = self.run_step(H5_ANSWER, unload='{"kind":"agent_running"}')
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("so the agent may still stand: take it down with --cleanup", result.stderr)
        self.assertNotIn("FINISHED", result.stdout)

    def test_a_load_that_stands_fails_with_the_entry_removed_then_the_agent_down(self):
        """Anything but the refusal with the entry: the entry goes, then the
        agent is force-unloaded, its answer checked (Codex on #171, round 1).
        Where it answers unloaded the agent is read down and the step fails
        saying so; where it does not, the step fails saying the agent may still
        stand, for --cleanup. Perturbation: drop the check of force-unload's
        answer, and a take-down that did not take is reported as down."""
        for answer in ('{"kind":"state","state":"idle"}', '{"kind":"in_transition","constituents":[7]}'):
            for unload, said, after in (
                    ('{"kind":"state","state":"unloaded"}', "and the agent is down", ["show alone"]),
                    ('{"kind":"agent_running"}', "so the agent may still stand: take it down with --cleanup", [])):
                with self.subTest(answer=answer, unload=unload):
                    result = self.run_step(answer, unload=unload)
                    self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
                    self.assertIn("load over an access entry on the trace refuses", result.stdout)
                    self.assertIn(f"then force-unload answered {unload}, {said}", result.stderr)
                    self.assertEqual(self.calls_made(),
                                     ["show alone", "setfacl -m", "admin load", "setfacl -b", "admin force-unload"]
                                     + after, "the entry goes before the take-down, checked after it")
                    self.assertNotIn("FINISHED", result.stdout)


class IndexModeTests(unittest.TestCase):
    """What git records for the deploy scripts, read from the index."""

    def test_every_deploy_script_is_executable_in_the_index(self):
        # Every deploy/*.sh is recorded 100755, so `./deploy/<script>.sh`
        # runs from any checkout. verify-lifecycle.sh was recorded 100644 and
        # refused with "Permission denied". Perturbation: record it 100644
        # again, and it is named. Outside a git checkout there is no index
        # to read, and the test is skipped.
        listed = subprocess.run(["git", "ls-files", "-s", "--", "*.sh"], cwd=DEPLOY,
                                text=True, capture_output=True)
        if listed.returncode != 0 or not listed.stdout.strip():
            self.skipTest("no git index here")
        modes = {line.split("\t", 1)[1]: line.split()[0] for line in listed.stdout.splitlines()}
        self.assertEqual([path for path, mode in modes.items() if mode != "100755"], [])


class ShellOwnedNameTests(unittest.TestCase):
    """What the deploy scripts may name, read from their text."""

    # Names bash writes on its own, so a value stored in one does not stay:
    # LINES and COLUMNS are rewritten to the terminal's size after a
    # foreground external command (checkwinsize), the rest by the shell's
    # clock, counters and process state.
    SHELL_OWNED = ("LINES", "COLUMNS", "SECONDS", "RANDOM", "SRANDOM", "LINENO", "BASHPID",
                   "EPOCHSECONDS", "EPOCHREALTIME", "PPID", "UID", "EUID", "HISTCMD",
                   "PIPESTATUS", "BASH_COMMAND", "PWD", "OLDPWD")

    def test_no_deploy_script_assigns_a_name_bash_owns(self):
        # update-stack.sh's verify step counted the sink into LINES, and from
        # a terminal taller than the sink's count the load "wrote no events"
        # and the install rolled back. Perturbation: on main at 17c0baf this
        # names update-stack.sh at its `LINES=` lines.
        assigns = re.compile(r"(?<![\w$-])(" + "|".join(self.SHELL_OWNED)
                             + r")(\[[^]]*\])?\+?=")
        found = []
        for script in sorted(DEPLOY.glob("*.sh")):
            for number, line in enumerate(script.read_text().splitlines(), 1):
                if not line.lstrip().startswith("#") and assigns.search(line):
                    found.append(f"{script.name}:{number}: {line.strip()}")
        self.assertEqual(found, [])


def shell_function(script, name):
    """One function exactly as a deploy script defines it, read out of the
    script's own text so a test runs the code that ships."""
    lines = script.splitlines()
    start = lines.index(f"{name}() {{")
    end = next(i for i in range(start, len(lines)) if lines[i] == "}")
    return "\n".join(lines[start:end + 1]) + "\n"


def legacy_log_judgement(script):
    """decommission.sh's judgment of what a legacy log leaves (CodeRabbit on
    #598): resolved, taken_with, the loop over the directories that stand and
    the loop over the entries left, as the script runs them once every target
    is known."""
    start = script.index("taken_with() {\n")
    end_line = "done < <([ ${#LOG_LEFT[@]} -eq 0 ] || printf '%s\\0' \"${!LOG_LEFT[@]}\" | sort -z)\n"
    end = script.index(end_line, start) + len(end_line)
    return shell_function(script, "resolved") + script[start:end]


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

    def test_the_box_accounts_are_not_agents_and_go_last(self):
        # **The box's two connector accounts are listed apart** (weaver-admin-
        # Spec section 2, the operator's rulings of 2026-11-10): never read as
        # agents `gatecon` and `admincon`, ordered after every agent's names so
        # the purge removes them last, and a process under one is said as
        # WeaverWeb's connector and not counted a run. Perturbations: drop the
        # box test from the discovery loop and `gatecon` is an agent; list the
        # names in getent's order and the box's go first.
        getent = Path(self.bin.name) / "getent"
        getent.write_text('#!/bin/sh\n'
                          'case "$1" in\n'
                          '  passwd) printf "%s\\n" "weaver-gatecon:x:990:990::/:/usr/sbin/nologin" '
                          '"root:x:0:0::/root:/bin/sh" "weaver-a:x:980:980::/home/weaver-a:/usr/sbin/nologin" '
                          '"weaver-admincon:x:991:991::/:/usr/sbin/nologin" "weaver-a-state:x:981:981::/:/usr/sbin/nologin" ;;\n'
                          '  group) printf "%s\\n" "weaver-admincon:x:991:" "weaver-a:x:980:weaver-gatecon" '
                          '"weaver-gatecon:x:990:" "weaver-a-admin:x:982:weaver-admincon" ;;\n'
                          'esac\n')
        getent.chmod(0o755)
        env = {**os.environ, "PATH": self.bin.name + os.pathsep + os.environ["PATH"]}
        env.pop("BASH_ENV", None)
        program = (shell_function(self.script, "is_box_account") + shell_function(self.script, "weaver_names")
                   + 'weaver_names "$1"')
        for db, want in (("passwd", ["weaver-a", "weaver-a-state", "weaver-gatecon", "weaver-admincon"]),
                         ("group", ["weaver-a", "weaver-a-admin", "weaver-admincon", "weaver-gatecon"])):
            with self.subTest(db=db):
                run = subprocess.run(["bash", "-c", program, "x", db], env=env, text=True,
                                     capture_output=True, timeout=20)
                self.assertEqual(run.returncode, 0, run.stderr)
                self.assertEqual(run.stdout.splitlines(), want)
        discovery = [l for l in self.script.splitlines()
                     if l.startswith(('for u in "${WEAVER_USERS[@]}"; do is_box_account',
                                      'for g in "${WEAVER_GROUPS[@]}"; do is_box_account'))]
        self.assertEqual(len(discovery), 2, discovery)
        agents = subprocess.run(
            ["bash", "-c", "set -euo pipefail\ndeclare -A AGENTS=()\n" + program.removesuffix('weaver_names "$1"')
             + shell_function(self.script, "strip_suffix")
             + "mapfile -t WEAVER_USERS < <(weaver_names passwd)\nmapfile -t WEAVER_GROUPS < <(weaver_names group)\n"
             + "\n".join(discovery) + '\nprintf "%s\\n" "${!AGENTS[@]}" | sort'],
            env=env, text=True, capture_output=True, timeout=20)
        self.assertEqual(agents.returncode, 0, agents.stderr)
        self.assertEqual(agents.stdout.split(), ["a"])
        self.assertIn('if is_box_account "$u"; then', self.script[self.script.index("query_runs()"):])

    def test_the_agent_is_archived_whole_with_its_territory_and_the_home_untouched(self):
        # The operator's ruling of 2026-10-07 on #1: the declaration, the
        # draft, the logs and the save points live in the territory, which
        # the territories' archive takes whole, and the operator's home is
        # never read, archived or purged. Perturbation: archive or purge a
        # path under the home again, and this fails.
        self.assertIn('for d in "${AGENT_FOLDERS[@]}"; do archive_path "$(archive_name agent "$d")" "$d"; done', self.script)
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
            # **A folder of the layout before #578 is the territory itself**,
            # `<base>/weaver-<agent>`, which the purge takes as that agent's
            # folder: the territory it names is covered, not listed twice.
            # Perturbation: cover only what stands below a folder, and the
            # territory prints.
            listed = self.run_fn("territories_outside", str(under), "--", str(roots / "a"),
                                 env=self.stat_env(scratch))
            self.assertEqual(listed, "")
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
            self.assertIn(f"refused: {roots / 'c'}/territory names {foreign}, which stands uid {STRANGER_UID}",
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
        self.assertIn("territories_outside \"${AGENT_FOLDERS[@]}\" -- \"${AGENT_ROOTS[@]}\") || TERRITORIES_JUDGED=0",
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

    def one_line_function(self, name):
        """A function the script defines on one line, read out of its text."""
        line = next(line for line in self.script.splitlines() if line.startswith(f"{name}() {{"))
        return line + "\n"

    def test_an_archive_file_is_the_operators_alone(self):
        """**Every file the archive writes is 0600** (#98; the custody audit's
        G22): the archive holds the trace and the save points, which the box
        keeps to their groups, so the invoker's umask does not decide who else
        reads them. `to_file`, run as the script defines it under a stand-in
        sudo and `umask 000`. Perturbation: drop the `umask 077`, and the file
        lands 0666."""
        scratch = tempfile.TemporaryDirectory(prefix="weaver-to-file-")
        self.addCleanup(scratch.cleanup)
        root = Path(scratch.name)
        sudo = root / "sudo"
        sudo.write_text('#!/bin/sh\nwhile [ "$1" != sh ]; do shift; done\nexec "$@"\n')
        sudo.chmod(0o755)
        out = root / "archive.tar.zst"
        program = ("umask 000\nOPERATOR=op\n" + self.one_line_function("as_op")
                   + self.one_line_function("to_file") + f"printf 'held' | to_file {shlex.quote(str(out))}\n")
        env = {**os.environ, "PATH": str(root) + os.pathsep + os.environ["PATH"]}
        env.pop("BASH_ENV", None)
        run = subprocess.run(["bash", "-c", program], env=env, text=True, capture_output=True, timeout=20)
        self.assertEqual(run.returncode, 0, run.stderr)
        self.assertEqual(out.read_text(), "held")
        self.assertEqual(out.stat().st_mode & 0o777, 0o600)
        self.assertIn("as_op sh -c 'umask 077 && cd \"$1\" && find . -type f", self.script)
        self.assertIn("as_op sh -c 'umask 077 && mkdir -p \"$1\"' _ \"$DEST\"", self.script)
        # The stale files go as the operator, and a DEST that stood before is
        # closed too: nothing under DEST is acted on as root.
        self.assertIn('as_op find "$DEST" -maxdepth 1 -type f -exec rm -f {}', self.script)
        self.assertNotIn('\n  find "$DEST"', self.script)
        self.assertIn('  close_dest "$DEST"\n  say "archive to $DEST"', self.script)

    def test_a_default_acl_on_the_archive_directory_reaches_no_file(self):
        """**No archive file inherits a reader** (#98, Codex on #142): a
        default access-control list on DEST outranks the umask, so a file
        created there takes its named entries and mask. `close_dest` strips
        the directory's lists before anything is written, and `to_file` sets
        each file 0600 once written, which masks any entry it did inherit.
        Run as the script defines them under a stand-in sudo, on a scratch
        directory given a default entry for another uid; skipped, naming why,
        where the filesystem takes no access-control list. Perturbations: drop
        the strip, and the written file carries the entry; drop the
        `chmod 0600`, and a file written under the list stands 0640.
        (`setfacl -b` alone removes a default list here; `-k` is kept for a
        `setfacl` that does not.)""" 
        if not (shutil.which("setfacl") and shutil.which("getfacl")):
            self.skipTest("setfacl and getfacl are not installed here")
        scratch = tempfile.TemporaryDirectory(prefix="weaver-dest-acl-")
        self.addCleanup(scratch.cleanup)
        root = Path(scratch.name)
        sudo = root / "sudo"
        sudo.write_text('#!/bin/sh\nwhile [ "$1" = -u ] || [ "$1" = -H ] || [ "$1" = op ]; do shift; done\nexec "$@"\n')
        sudo.chmod(0o755)
        stranger = os.getuid() + 1
        env = {**os.environ, "PATH": str(root) + os.pathsep + os.environ["PATH"]}
        env.pop("BASH_ENV", None)

        def dest_with_default():
            dest = root / f"dest-{len(list(root.iterdir()))}"
            dest.mkdir()
            given = subprocess.run(["setfacl", "-d", "-m", f"u:{stranger}:r", str(dest)],
                                   capture_output=True, text=True)
            if given.returncode != 0:
                self.skipTest(f"this filesystem takes no access-control list: {given.stderr.strip()}")
            return dest

        def run(program):
            ran = subprocess.run(["bash", "-c", "die() { echo \"$*\" >&2; exit 1; }\nOPERATOR=op\n"
                                  + self.one_line_function("as_op") + self.one_line_function("to_file")
                                  + shell_function(self.script, "close_dest") + program],
                                 env=env, text=True, capture_output=True, timeout=20)
            self.assertEqual(ran.returncode, 0, ran.stderr)

        extended = lambda path: subprocess.run(["getfacl", "-cps", str(path)], capture_output=True,
                                               text=True).stdout.strip()
        # Closed first: the directory carries no list, and the file none.
        dest = dest_with_default()
        out = dest / "territory.tar.zst"
        run(f"close_dest {shlex.quote(str(dest))}\nprintf held | to_file {shlex.quote(str(out))}\n")
        self.assertEqual(extended(dest), "", "the directory's lists are stripped")
        self.assertEqual(extended(out), "", "the file inherits no entry")
        self.assertEqual(out.stat().st_mode & 0o777, 0o600)
        # Written under a list the strip has not reached: the mode masks it.
        dest = dest_with_default()
        out = dest / "PURGE-LIST"
        run(f"printf held | to_file {shlex.quote(str(out))}\n")
        self.assertEqual(out.stat().st_mode & 0o777, 0o600, "the inherited mask is closed")

    def test_a_failed_acl_inspection_refuses_before_anything_is_written(self):
        """**A look at DEST's lists that fails refuses** (#98, Codex on #142,
        round 2): getfacl's status is read apart from its output, so an
        inspection that errs is never taken for a directory carrying no list.
        `close_dest` over a stand-in getfacl that exits 1, at the first look
        and, after a strip, at the re-check. Perturbation: read the status as
        empty output, as `$(... 2>/dev/null)` did, and close_dest goes on."""
        scratch = tempfile.TemporaryDirectory(prefix="weaver-acl-look-")
        self.addCleanup(scratch.cleanup)
        root = Path(scratch.name)
        bin_dir = root / "bin"
        bin_dir.mkdir()
        sudo = bin_dir / "sudo"
        sudo.write_text('#!/bin/sh\nwhile [ "$1" = -u ] || [ "$1" = -H ] || [ "$1" = op ]; do shift; done\nexec "$@"\n')
        sudo.chmod(0o755)
        setfacl = bin_dir / "setfacl"
        setfacl.write_text("#!/bin/sh\nexit 0\n")
        setfacl.chmod(0o755)
        dest = root / "dest"
        dest.mkdir()
        env = {**os.environ, "PATH": str(bin_dir) + os.pathsep + os.environ["PATH"]}
        env.pop("BASH_ENV", None)
        for case, look in (
                ("the first look", '#!/bin/sh\necho "getfacl: refused" >&2\nexit 1\n'),
                ("the re-check", '#!/bin/sh\nif [ -e "$0.seen" ]; then exit 1; fi\n'
                                 ': > "$0.seen"\necho "user:9999:r--"\n')):
            with self.subTest(case=case):
                getfacl = bin_dir / "getfacl"
                getfacl.write_text(look)
                getfacl.chmod(0o755)
                Path(str(getfacl) + ".seen").unlink(missing_ok=True)
                ran = subprocess.run(["bash", "-c", "die() { echo \"$*\" >&2; exit 1; }\nOPERATOR=op\n"
                                      + self.one_line_function("as_op")
                                      + shell_function(self.script, "close_dest")
                                      + f"close_dest {shlex.quote(str(dest))}\necho CLOSED\n"],
                                     env=env, text=True, capture_output=True, timeout=20)
                self.assertNotEqual(ran.returncode, 0, ran.stdout)
                self.assertNotIn("CLOSED", ran.stdout)
                self.assertIn("cannot be read", ran.stderr)

    def test_a_tmp_entry_another_principal_owns_is_left_alone(self):
        """**Only a weaver- account's /tmp entries are archived and purged**
        (#98; the custody audit's G22): /tmp is writable by every local user,
        so an entry another principal made under a weaver- name is neither
        read as root nor removed. `tmp_paths` over a stand-in `find` that
        reports each entry's owner, NUL-ended. Perturbation: drop the owner
        filter, the newline refusal or the directory check, and an entry
        the planter chose is listed."""
        scratch = tempfile.TemporaryDirectory(prefix="weaver-tmp-paths-")
        self.addCleanup(scratch.cleanup)
        root = Path(scratch.name)
        find = root / "find"
        # Records as find writes them, NUL-ended: a weaver- account's entry,
        # a stranger's, a stranger's name carrying a newline that would split
        # into a weaver- owner and /etc, and an entry not directly under /tmp.
        find.write_text("#!/bin/sh\nprintf '/tmp/weaver-a\\0weaver-a\\0"
                        "/tmp/weaver-planted\\0someone\\0"
                        "/tmp/weaver-q\\nx\\0weaver-a\\0"
                        "/tmp/weaver-x\\t\\0weaver-a\\0"
                        "/var/weaver-b\\0weaver-b\\0'\n")
        find.chmod(0o755)
        env = {**os.environ, "PATH": str(root) + os.pathsep + os.environ["PATH"]}
        env.pop("BASH_ENV", None)
        self.assertEqual(self.run_fn("tmp_paths", "/tmp", env=env).splitlines(), ["/tmp/weaver-a"])
        self.assertIn("mapfile -t TMP_PATHS < <(tmp_paths /tmp)", self.script)
        # The purge splits each line exactly as it was judged, so a name's
        # surrounding blanks are kept and no other path is acted on.
        self.assertNotIn("read -r kind name", self.script)
        self.assertEqual(self.script.count("while IFS= read -r line; do\n  kind=${line%% *} name=${line#* }"), 3)
        split = subprocess.run(["bash", "-c", 'line="$1"; kind=${line%% *} name=${line#* }; printf "%s|%s|" "$kind" "$name"',
                                "x", "path /tmp/weaver-a "], text=True, capture_output=True, timeout=20)
        self.assertEqual(split.stdout, "path|/tmp/weaver-a |")

    def test_a_purge_list_line_the_purge_would_not_find_refuses_the_purge(self):
        """**The purge acts only on lines it derives itself** (#98; the custody
        audit's G22): the purge list lives where the operator writes, so a line
        naming anything that still stands and that the purge's own derivation
        does not find refuses the whole purge, before the sudo rules are
        removed or an account deleted. A line naming what no longer stands is
        passed over, so a purge stopped part way runs again. Judged in bash,
        each line read as the removal loops read it, and an empty derivation
        holds nothing. The list is read once into root's copy, hashed against
        the archive's sums, and every removal loop reads that copy.
        Perturbation: drop the judgment or its call, read the list from DEST or the
        unfiltered copy again, skip a line that still stands, or keep a line
        the derivation does not hold, and the pins here fail."""
        scratch = tempfile.TemporaryDirectory(prefix="weaver-purge-list-")
        self.addCleanup(scratch.cleanup)
        root = Path(scratch.name)
        standing = root / "weaver-standing"
        standing.mkdir()
        listed = root / "PURGE-LIST"
        judge = shell_function(self.script, "purge_lines_outside")

        def outside(candidates, lines):
            listed.write_text("".join(line + "\n" for line in lines))
            stub = "purge_candidates() { printf '%s\\n' " + " ".join(shlex.quote(c) for c in candidates) + "; }\n"
            if not candidates:
                stub = "purge_candidates() { :; }\n"
            run = subprocess.run(["bash", "-c", stub + judge + 'purge_lines_outside "$1"', "x", str(listed)],
                                 text=True, capture_output=True, timeout=20)
            self.assertEqual(run.returncode, 0, run.stderr)
            return run.stdout.splitlines()

        held = [f"path {standing}", "user weaver-a"]
        self.assertEqual(outside(held, [f"path {standing}", "user weaver-a"]), [])
        self.assertEqual(outside(held, ["path /etc"]), ["path /etc"], "a foreign path that stands")
        self.assertEqual(outside([], ["path /etc"]), ["path /etc"], "an empty derivation holds nothing")
        self.assertEqual(outside(held, [f"path {root}/weaver-gone"]), [], "what no longer stands is passed over")
        self.assertEqual(outside(held, [f"path {standing} "]), [], "a name's own blanks are kept: no such path")
        self.assertEqual(outside(held, ["kill everything"]), ["kill everything"], "an unknown kind is named")
        judged = self.script.index('[ -z "$OUTSIDE" ] || die ')
        # The die reads the judgment's own answer over root's copy.
        self.assertEqual(self.script.count('OUTSIDE=$(purge_lines_outside "$PURGE_LIST")'), 1)
        self.assertLess(self.script.index('OUTSIDE=$(purge_lines_outside "$PURGE_LIST")'), judged)
        self.assertLess(self.script.index('[ "$LISTED_SUM" = "$COPY_SUM" ] || die '), judged)
        self.assertLess(judged, self.script.index('say "the delegated door, shut"'))
        self.assertNotIn('done < "$DEST/PURGE-LIST"', self.script)
        self.assertNotIn('done < "$PURGE_LIST"', self.script)
        # The four removal loops (the home quotas', #585, among them),
        # load_held, and rerun_unchanged.
        self.assertEqual(self.script.count('done < "$PURGE_ACTED"'), 6)
        self.assertLess(judged, self.script.index('purge_lines_within "$PURGE_LIST" > "$PURGE_ACTED"'))
        # **A line passed over is never acted on**: only lines the
        # derivation holds reach the removal loops, so a path that did not
        # resolve at the judgment and does by the removal is not removed.
        within = shell_function(self.script, "purge_lines_within")
        listed.write_text(f"path {standing}\npath {root}/gone/../../etc\nuser weaver-a\n")
        stub = "purge_candidates() { printf '%s\\n' " + " ".join(shlex.quote(c) for c in held) + "; }\n"
        kept = subprocess.run(["bash", "-c", stub + within + 'purge_lines_within "$1"', "x", str(listed)],
                              text=True, capture_output=True, timeout=20)
        self.assertEqual(kept.stdout.splitlines(), [f"path {standing}", "user weaver-a"], kept.stderr)

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
                       + self.script[self.script.index('declare -A BASES_SAID=()\nfor d in "${!TERRITORY_BASES[@]}"; do\n  in_operator_home'):
                                     self.script.index("# **agent_folders BASE...")]
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


    # The tester owns every file a fixture makes, so this stand-in answers uid
    # 0 for each path but those STRANGERS names, which answer STRANGER_UID,
    # never the runner's (Codex on #599). Each keeps its own link count.
    ROOT_STAT = ('stat() { local p="${@: -1}" u=0; case ":${STRANGERS:-}:" in *":$p:"*) u=' + str(STRANGER_UID)
                 + ' ;; esac; command stat "$@" | sed "s/^[0-9]*/$u/"; }\n')

    def script_options(self):
        """The script's own `set` line, under which its functions run."""
        found = re.findall(r"^set -[a-z]+(?: -o [a-z]+| [a-z]+)*$", self.script, re.M)
        self.assertEqual(len(found), 1, found)
        return found[0] + "\n"

    def log_takes(self, log, agents=("m1",), strangers=(), find=None):
        """log_paths LOG with AGENTS naming AGENTS, as (kind, path) pairs,
        under the script's own options, with FIND as a stand-in for find."""
        program = (self.script_options() + self.ROOT_STAT
                   + ("" if find is None else f"find() {{ {find}; }}\n")
                   + "declare -A AGENTS=(" + " ".join(f"[{a}]=1" for a in agents) + ")\n"
                   + shell_function(self.script, "log_paths") + 'log_paths "$1"')
        run = subprocess.run(["bash", "-c", program, "x", str(log)], capture_output=True, timeout=20,
                             env={**os.environ, "STRANGERS": ":".join(str(s) for s in strangers)})
        self.assertEqual(run.returncode, 0, run.stderr)
        fields = run.stdout.split(b"\0")
        self.assertEqual(fields[-1], b"")
        return sorted((fields[i].decode(), fields[i + 1].decode()) for i in range(0, len(fields) - 1, 2))

    def test_a_legacy_log_directory_is_taken_whole_only_when_it_is_the_agents(self):
        # **#593, the Planner's ruling of 2026-11-17**: a legacy `log-path`'s
        # directory is taken whole only when it is the agent's own, the layout
        # create-agent.sh laid before #50 (`<log-directory>/<agent>/admin.log`,
        # root:root 0750): named for an agent found, root's, and holding
        # nothing but the log and its rotations. Any other directory gives up
        # the log and its rotations one by one, each a regular file, root's,
        # with one link, and every other entry is named as left. A name
        # carrying a newline is one left entry, never a take of its writer's
        # choosing. A directory that cannot be listed is unread, never empty,
        # and one gone or a link gives up nothing. Perturbations: take the
        # directory whole whatever it holds, whatever its name or whoever owns
        # it, take a rotation through a link, of a stranger's or with a second
        # link, split the pairs on newlines, or read a failed listing as
        # empty, and a read below fails.
        with tempfile.TemporaryDirectory() as scratch:
            top = Path(scratch).resolve()
            shared = top / "log"
            shared.mkdir()
            for name in ("worker.log", "worker.log.1", "worker.log.2.gz", "worker.log-20261101",
                         "stranger.txt", "worker.log.bak", "worker.logger", "worker.log.5"):
                (shared / name).write_text("x\n")
            (top / "elsewhere").write_text("another's\n")
            (shared / "worker.log.3").symlink_to(top / "elsewhere")
            (shared / "worker.log.4").mkdir()
            os.link(top / "elsewhere", shared / "worker.log.6")
            (shared / "evil\ntake").write_text("x\n")
            self.assertEqual(self.log_takes(shared / "worker.log", strangers=[shared / "worker.log.5"]), sorted(
                [("take", str(shared / n)) for n in ("worker.log", "worker.log.1", "worker.log.2.gz",
                                                     "worker.log-20261101")]
                + [("left", str(shared / n)) for n in ("stranger.txt", "worker.log.bak", "worker.logger",
                                                       "worker.log.3", "worker.log.4", "worker.log.5",
                                                       "worker.log.6", "evil\ntake")]
                + [("stands", str(shared))]))
            # The agent's own directory, holding the log and its rotation
            # alone, is taken whole.
            own = top / "m1"
            own.mkdir()
            (own / "admin.log").write_text("x\n")
            (own / "admin.log.1").write_text("x\n")
            self.assertEqual(self.log_takes(own / "admin.log"), [("take", str(own))])
            one_by_one = [("stands", str(own)), ("take", str(own / "admin.log")),
                          ("take", str(own / "admin.log.1"))]
            # Named for no agent found, or not root's, it gives up its files
            # one by one and stands.
            self.assertEqual(self.log_takes(own / "admin.log", agents=("m2",)), one_by_one)
            self.assertEqual(self.log_takes(own / "admin.log", strangers=[own]), one_by_one)
            # Holding a stranger's file, it stands and the file is left.
            (own / "notes").write_text("not the agent's\n")
            self.assertEqual(self.log_takes(own / "admin.log"), sorted(one_by_one + [("left", str(own / "notes"))]))
            # Gone, or a link, it gives up nothing and says so.
            self.assertEqual(self.log_takes(top / "absent" / "admin.log"), [("passed", str(top / "absent"))])
            (top / "linked").symlink_to(own)
            self.assertEqual(self.log_takes(top / "linked" / "admin.log"), [("passed", str(top / "linked"))])
            # A listing that fails is never an empty directory, and nothing in
            # it is taken.
            if os.geteuid() != 0:
                shut = top / "m9"
                shut.mkdir()
                (shut / "admin.log").write_text("x\n")
                shut.chmod(0o300)
                try:
                    self.assertEqual(self.log_takes(shut / "admin.log", agents=("m9",)), [("unread", str(shut))])
                finally:
                    shut.chmod(0o755)
            # Under the script's errexit (Codex on #599), a find that fails
            # and prints nothing, run as root or not, and a listing that ends
            # before its status is printed, are unread as well.
            for find in ("return 1", 'kill -9 "$BASHPID"'):
                with self.subTest(find=find):
                    self.assertEqual(self.log_takes(own / "admin.log", find=find), [("unread", str(own))])

    def test_a_legacy_log_read_short_under_errexit_refuses_the_archive_and_the_purge(self):
        """**Codex on #599**: the discovery loop and its refusal, under the
        script's own options. A find that fails and prints nothing, and an
        answer from log_paths cut off before its directory's verdict, each
        name the directory and refuse both modes. Perturbations: print find's
        status unguarded after it, as before, and the first case runs on with
        nothing refused. Drop the loop's check for the verdict, and the second
        does."""
        script = self.script
        q = shlex.quote
        loop = script[script.index("declare -A LOG_LEFT=() LOG_DIRS_SAID=()\n"):]
        loop = loop[:loop.index(" | sort -z)\n") + len(" | sort -z)\n")]
        refusal = next(line for line in script.splitlines()
                       if line.startswith('[ ${#LOG_UNREAD[@]} -eq 0 ] || die "the directory of a legacy log'))
        with tempfile.TemporaryDirectory() as scratch:
            shared = Path(scratch).resolve() / "var-log"
            shared.mkdir()
            log = shared / "weaver-worker.log"
            log.write_text("the agent's\n")
            cases = {
                "find fails": ("find() { return 1; }\n" + shell_function(script, "log_paths"),
                               f"PLAN {shared}  the directory of the legacy log {log} cannot be listed"),
                "answer cut off": (shell_function(script, "log_paths")
                                   + "log_paths() { printf 'take\\0%s\\0' \"$1\"; exit 1; }\n",
                                   f"PLAN {shared}  the directory of the legacy log {log} was not read to its end"),
            }
            for case, (functions, said) in cases.items():
                with self.subTest(case=case):
                    program = (self.script_options() + self.ROOT_STAT
                               + "declare -A AGENTS=([m1]=1) LOG_PATHS=()\n"
                               + f"declare -A LOG_FILES=([{q(str(log))}]=1)\n"
                               + "plan() { printf 'PLAN %s\\n' \"$*\"; }\n"
                               + "die() { printf 'DIE %s\\n' \"$*\"; exit 1; }\n"
                               + "in_operator_home() { return 1; }\n"
                               + functions + loop + refusal + "\necho REACHED\n")
                    run = subprocess.run(["bash", "-c", program], text=True, capture_output=True, timeout=20,
                                         env={**os.environ, "STRANGERS": ""})
                    got = run.stdout.splitlines()
                    self.assertEqual(run.returncode, 1, run.stdout + run.stderr)
                    self.assertTrue(any(line.startswith(said) for line in got), run.stdout + run.stderr)
                    self.assertIn(f"DIE the directory of a legacy log cannot be listed: {shared}. Nothing is "
                                  "archived or purged until it reads. Look at it by hand, then rerun", got)
                    self.assertNotIn("REACHED", got)

    def test_a_legacy_log_in_a_shared_directory_leaves_the_directory_and_its_strangers(self):
        # **#593's acceptance**: a legacy log path in a shared directory beside
        # a stranger's file and another agent's legacy log. Discovery takes
        # each log and its rotation as paths of their own, and names the
        # stranger's file as left, never the other log a second pass takes
        # (the self-check on #593). The purge's derivation lists the files
        # and never the directory, and the paths phase removes them and
        # leaves the stranger's file and the directory, each said once by
        # the first log in order. The archive takes
        # every legacy log in one tarball, and an unlisted directory refuses
        # both modes after the plan. Perturbations: take the directory whole,
        # and the directory is a purge line and the stranger's file goes; name
        # a left entry before every take is known, and the other log reads as
        # left. The paths phase acts only on the lines under the test's
        # scratch directory.
        script = self.script
        q = shlex.quote
        loop = script[script.index("declare -A LOG_LEFT=() LOG_DIRS_SAID=()\n"):]
        loop = loop[:loop.index(" | sort -z)\n") + len(" | sort -z)\n")]
        plan_loop = script[script.index('for d in "$LEGACY_DIR" "${!LOG_PATHS[@]}"; do'):]
        plan_loop = plan_loop[:plan_loop.index("\ndone\n") + len("\ndone\n")]
        left_loop = legacy_log_judgement(script)
        paths_phase = script[script.index('say "paths"\n'):script.index('say "accounts"\n')]
        with tempfile.TemporaryDirectory() as scratch:
            top = Path(scratch).resolve()
            shared = top / "var-log"
            shared.mkdir()
            (shared / "weaver-worker.log").write_text("the agent's\n")
            (shared / "weaver-worker.log.1").write_text("the agent's\n")
            (shared / "m2-admin.log").write_text("the other agent's\n")
            (shared / "stranger.log").write_text("not the agent's\n")
            base = top / "state"
            (base / "m1" / "weaver-m1").mkdir(parents=True)
            listed = top / "purge-list"
            program = ("set -uo pipefail\n" + self.ROOT_STAT
                       + "SUDO_RULES=(); LDSO_CONFS=(); HOMES=(); TMP_PATHS=(); WEAVER_USERS=(); WEAVER_GROUPS=()\n"
                       "HOME_QUOTAS=(); PURGE=(); PURGE_FAILED=0; OWN_TERRITORIES=(); TERRITORY_PATHS=()\n"
                       "declare -A PREFIXES=() COORD_ROOTS=() AGENTS=([m1]=1 [m2]=1) LOG_PATHS=()\n"
                       f"declare -A TERRITORY_BASES=([{q(str(base))}]=1)\n"
                       f"declare -A LOG_FILES=([{q(str(shared / 'weaver-worker.log'))}]=1 "
                       f"[{q(str(shared / 'm2-admin.log'))}]=1)\n"
                       f"AGENT_FOLDERS=({q(str(base / 'm1'))})\n"
                       "LEGACY_DIR=/nonexistent/weaver\nOP_HOME=\n"
                       "say() { :; }\nplan() { printf 'PLAN %s\\n' \"$*\"; }\n"
                       "die() { printf 'DIE %s\\n' \"$*\"; exit 1; }\nheld_is() { return 0; }\n"
                       "in_operator_home() { return 1; }\n"
                       + shell_function(script, "log_paths") + loop
                       + shell_function(script, "spanned")
                       + shell_function(script, "spans_a_base") + shell_function(script, "candidate")
                       + shell_function(script, "purge_candidates") + shell_function(script, "purge_lines_within")
                       + plan_loop + left_loop
                       # Only the lines under this scratch directory reach the
                       # paths phase: the derivation also names the box's own
                       # /etc/weaver, which a test never acts on.
                       + f"purge_candidates | awk -v t={q('path ' + str(top) + '/')} 'index($0, t) == 1' > {q(str(listed))}\n"
                       + f"sed 's/^/LISTED /' {q(str(listed))}\n"
                       + f"PURGE_ACTED=$(mktemp)\npurge_lines_within {q(str(listed))} > \"$PURGE_ACTED\"\n"
                       + paths_phase + 'rm -f -- "$PURGE_ACTED"\n')
            run = subprocess.run(["bash", "-c", program], text=True, capture_output=True, timeout=20,
                                 env={**os.environ, "STRANGERS": ""})
            self.assertEqual(run.returncode, 0, run.stdout + run.stderr)
            got = run.stdout.splitlines()
            for name in ("weaver-worker.log", "weaver-worker.log.1", "m2-admin.log"):
                self.assertIn(f"LISTED path {shared / name}", got)
                self.assertIn(f"PLAN removed {shared / name}", got)
                self.assertFalse((shared / name).exists())
            self.assertNotIn(f"LISTED path {shared}", got)
            # The logs are read in order, so the first names what both share,
            # once.
            self.assertEqual([line for line in got if "left in place" in line], [
                f"PLAN {shared / 'stranger.log'}  beside the legacy log {shared / 'm2-admin.log'}, "
                "and taken by nothing this discovery found: left in place, untouched"], run.stdout)
            self.assertEqual([line for line in got if "directory stands" in line], [
                f"PLAN {shared}  the directory of the legacy log {shared / 'm2-admin.log'}, "
                "not the agent's own: the log and its rotations go, and the directory stands"], run.stdout)
            self.assertEqual((shared / "stranger.log").read_text(), "not the agent's\n")
            self.assertTrue(shared.is_dir())
        # Every legacy log in one tarball, and an unlisted directory refuses
        # both modes after the plan and before either acts.
        self.assertIn('  [ ${#LOG_PATHS[@]} -gt 0 ] && archive_path legacy-logs "${!LOG_PATHS[@]}"\n', script)
        refusal = script.index('[ ${#LOG_UNREAD[@]} -eq 0 ] || die ')
        self.assertLess(script.index('[ "$MODE" = plan ] && { say "plan only.'), refusal)
        self.assertLess(refusal, script.index('if [ "$MODE" = archive ]; then'))

    def test_a_listing_that_fails_after_another_read_its_directory_still_refuses(self):
        """**CodeRabbit on #598**: two legacy logs in one directory, the first
        listing read and the second failing, as a directory that changes
        between them would. The failure is counted whatever came before it,
        said once, and refuses both modes. Perturbation: count a failure only
        for a directory not yet said, as before, and the run goes on with
        nothing refused."""
        script = self.script
        q = shlex.quote
        loop = script[script.index("declare -A LOG_LEFT=() LOG_DIRS_SAID=()\n"):]
        loop = loop[:loop.index(" | sort -z)\n") + len(" | sort -z)\n")]
        refusal = next(line for line in script.splitlines()
                       if line.startswith('[ ${#LOG_UNREAD[@]} -eq 0 ] || die "the directory of a legacy log'))
        with tempfile.TemporaryDirectory() as scratch:
            top = Path(scratch).resolve()
            shared = top / "var-log"
            shared.mkdir()
            first, second = shared / "a-admin.log", shared / "b-admin.log"
            for log in (first, second):
                log.write_text("a log\n")
            calls = top / "find-calls"
            # find answers the first listing and fails every later one. Each
            # listing runs in a subshell of its own, so the count is a file's.
            find = (f'find() {{ local n; n=$(cat {q(str(calls))} 2>/dev/null || echo 0); '
                    f'echo $((n + 1)) > {q(str(calls))}; [ "$n" -eq 0 ] || return 1; command find "$@"; }}\n')
            program = (self.script_options() + self.ROOT_STAT
                       + "declare -A AGENTS=([m1]=1) LOG_PATHS=()\n"
                       + f"declare -A LOG_FILES=([{q(str(first))}]=1 [{q(str(second))}]=1)\n"
                       + "plan() { printf 'PLAN %s\\n' \"$*\"; }\n"
                       + "die() { printf 'DIE %s\\n' \"$*\"; exit 1; }\n"
                       + "in_operator_home() { return 1; }\n"
                       + find + shell_function(script, "log_paths") + loop + refusal + "\necho REACHED\n")
            run = subprocess.run(["bash", "-c", program], text=True, capture_output=True, timeout=20,
                                 env={**os.environ, "STRANGERS": ""})
            got = run.stdout.splitlines()
            self.assertEqual(calls.read_text().strip(), "2", run.stdout + run.stderr)
            self.assertEqual(run.returncode, 1, run.stdout + run.stderr)
            self.assertEqual([line for line in got if "cannot be listed, so" in line], [
                f"PLAN {shared}  the directory of the legacy log {second} cannot be listed, so the archive "
                "and the purge refuse"], run.stdout)
            self.assertIn(f"DIE the directory of a legacy log cannot be listed: {shared}. Nothing is "
                          "archived or purged until it reads. Look at it by hand, then rerun", got)
            self.assertNotIn("REACHED", got)

    def test_what_a_legacy_log_leaves_is_judged_by_containment_in_every_target(self):
        """**CodeRabbit on #598**: a legacy log's directory that stands, and an
        entry beside the log that no log takes, are judged once every target is
        known, by containment on resolved paths past a slash, under the
        script's own options. Under an agent's
        folder, the entry is taken with the folder and the directory goes with
        it. In the earlier layout's directory, taken whole, the entry is taken
        with it and the directory is a target of its own. In a directory whose
        name only begins with that one's, both stand and are left. Beside an
        agent's folder, the folder is said by its own line alone. A home is
        judged against every target the same way, itself a target included.
        Perturbations: compare by equality alone, and the first two cases read
        untouched. Drop the slash, and the near name is taken. Judge a
        directory as it is read, and it stands in the first two. Say an entry
        that is a target again, and the fourth case names the folder. Drop a home's
        equality or its slash, or judge it against the agents' folders alone,
        and a home line fails. Count a target that is a link as holding the
        home it points into, and that home reads as archived with the link."""
        script = self.script
        q = shlex.quote
        loop = script[script.index("declare -A LOG_LEFT=() LOG_DIRS_SAID=()\n"):]
        loop = loop[:loop.index(" | sort -z)\n") + len(" | sort -z)\n")]
        plan_loop = script[script.index('for d in "$LEGACY_DIR" "${!LOG_PATHS[@]}"; do'):]
        plan_loop = plan_loop[:plan_loop.index("\ndone\n") + len("\ndone\n")]
        judgement = legacy_log_judgement(script)
        with tempfile.TemporaryDirectory() as scratch:
            top = Path(scratch).resolve()
            base = top / "state"
            folder = base / "m1"
            (folder / "weaver-m1").mkdir(parents=True)
            under_folder = folder / "logs"
            under_folder.mkdir()
            legacy = top / "weaver"
            legacy.mkdir()
            near = top / "weaver2"
            near.mkdir()
            for d in (under_folder, legacy, near, base):
                (d / "old-admin.log").write_text("a log\n")
                (d / "keep.txt").write_text("not a log\n")
            goes = "not the agent's own: the log and its rotations go"
            cases = {
                "under an agent's folder": (under_folder, [
                    f"PLAN {under_folder}  the directory of the legacy log {under_folder / 'old-admin.log'}, "
                    f"{goes}, and the directory goes with {folder}",
                    f"PLAN {under_folder / 'keep.txt'}  beside the legacy log {under_folder / 'old-admin.log'}: "
                    f"taken with {folder}"]),
                "in the earlier layout's directory": (legacy, [
                    f"PLAN {legacy}  the directory of the legacy log {legacy / 'old-admin.log'}, {goes}, "
                    "and the directory is taken whole, a target of its own",
                    f"PLAN {legacy / 'keep.txt'}  beside the legacy log {legacy / 'old-admin.log'}: "
                    f"taken with {legacy}"]),
                "a near name": (near, [
                    f"PLAN {near}  the directory of the legacy log {near / 'old-admin.log'}, {goes}, "
                    "and the directory stands",
                    f"PLAN {near / 'keep.txt'}  beside the legacy log {near / 'old-admin.log'}, and taken by "
                    "nothing this discovery found: left in place, untouched"]),
                "beside an agent's folder": (base, [
                    f"PLAN {base}  the directory of the legacy log {base / 'old-admin.log'}, {goes}, "
                    "and the directory stands",
                    f"PLAN {base / 'keep.txt'}  beside the legacy log {base / 'old-admin.log'}, and taken by "
                    "nothing this discovery found: left in place, untouched"]),
            }
            for case, (d, said) in cases.items():
                with self.subTest(case=case):
                    program = (self.script_options() + self.ROOT_STAT
                               + "OWN_TERRITORIES=()\n"
                               + f"TERRITORY_PATHS=({q(str(folder))})\n"
                               + "declare -A AGENTS=([m1]=1) LOG_PATHS=()\n"
                               + f"declare -A TERRITORY_BASES=([{q(str(base))}]=1)\n"
                               + f"declare -A LOG_FILES=([{q(str(d / 'old-admin.log'))}]=1)\n"
                               + f"AGENT_FOLDERS=({q(str(folder))})\n"
                               + f"LEGACY_DIR={q(str(legacy))}\nOP_HOME=\n"
                               + "plan() { printf 'PLAN %s\\n' \"$*\"; }\n"
                               + "die() { printf 'DIE %s\\n' \"$*\"; exit 1; }\n"
                               + "in_operator_home() { return 1; }\n"
                               + shell_function(script, "log_paths") + loop + shell_function(script, "spanned")
                               + shell_function(script, "spans_a_base")
                               + plan_loop + judgement)
                    run = subprocess.run(["bash", "-c", program], text=True, capture_output=True, timeout=20,
                                         env={**os.environ, "STRANGERS": ""})
                    self.assertEqual(run.returncode, 0, run.stdout + run.stderr)
                    got = run.stdout.splitlines()
                    self.assertEqual([line for line in got if "the directory of the legacy log" in line
                                      or "beside the legacy log" in line], said, run.stdout)
            # A home is judged against every target, itself one included, and
            # a near name is not one, by the script's own loop over the homes.
            (legacy / "home").mkdir()
            homes_loop = script[script.index('HOMES=()\nfor u in "${WEAVER_USERS[@]}"; do\n'):]
            homes_loop = homes_loop[:homes_loop.index("\ndone\n") + len("\ndone\n")]
            # A target that is a link holds no home: the archive keeps the link
            # and the purge removes it, never the home it points into.
            (top / "linked" / "home").mkdir(parents=True)
            (top / "lnk").symlink_to(top / "linked")
            homes = {"weaver-m1": str(legacy / "home"), "weaver-m2": str(legacy), "weaver-m3": str(near),
                     "weaver-m4": str(top / "linked" / "home")}
            program = (self.script_options() + self.passwd_stub(homes)
                       + "WEAVER_USERS=(weaver-m1 weaver-m2 weaver-m3 weaver-m4)\nOWN_TERRITORIES=()\n"
                       + f"AGENT_FOLDERS=({q(str(folder))})\n"
                       + f"TERRITORY_PATHS=({q(str(folder))} {q(str(legacy))} {q(str(top / 'lnk'))})\n"
                       + "plan() { printf 'PLAN %s\\n' \"$*\"; }\n"
                       + shell_function(script, "home_within") + homes_loop)
            run = subprocess.run(["bash", "-c", program], text=True, capture_output=True, timeout=20)
            self.assertEqual(run.returncode, 0, run.stdout + run.stderr)
            self.assertEqual(run.stdout.splitlines(), [
                f"PLAN {legacy / 'home'}  (home of weaver-m1, archived with {legacy})",
                f"PLAN {legacy}  (home of weaver-m2, archived with {legacy})",
                f"PLAN {near}  (home of weaver-m3): outside /home and everything the purge takes, left in place, "
                "untouched",
                f"PLAN {top / 'linked' / 'home'}  (home of weaver-m4): outside /home and everything the purge "
                "takes, left in place, untouched"], run.stdout)

    # agent_folders and territories_outside look as root: every fixture path
    # is root's but those STRANGERS names, a territory `weaver-<a>` carries
    # its state group, and a folder's `home` is its account's. A stranger's
    # path answers STRANGER_UID, and no name, as stat does for a uid that has
    # none, never the runner's, which is root's when the suite runs as root
    # (Codex on #599, the same class).
    FOLDER_STAT = ('stat() { local p="${@: -1}" q; '
                   'case ":${STRANGERS:-}:" in *":$p:"*) case "$2" in %U) echo UNKNOWN ;; '
                   '%u*) command stat "$@" | sed "s/^[0-9]*/' + str(STRANGER_UID) + '/" ;; '
                   '*) command stat "$@" ;; esac; return ;; esac; '
                   'case "$2" in "%u:%G:%a") printf "0:%s-state:750\\n" "${p##*/}" ;; '
                   '"%U") case "$p" in */home) q=${p%/home}; printf "weaver-%s\\n" "${q##*/}" ;; '
                   '*) command stat "$@" ;; esac ;; '
                   "*) command stat \"$@\" | sed 's/^[0-9]*/0/' ;; esac; }\n")

    def read_key_line(self):
        """read_key, which the script defines on one line."""
        return next(line for line in self.script.splitlines() if line.startswith("read_key() {")) + "\n"

    @staticmethod
    def passwd_stub(homes):
        """getent passwd answering each account of HOMES with its home."""
        cases = "".join(f'{u}) echo "{u}:x:{uid}:{uid}::{h}:/sbin/nologin" ;; '
                        for uid, (u, h) in enumerate(homes.items(), start=12345))
        return f'getent() {{ case "$2" in {cases}*) return 2 ;; esac; }}\n'

    def folder_bases(self, roots, users, homes, strangers=(), stand_ins=""):
        """folder_bases ROOTS -- USERS, with m1 and m2 the agents found, and
        STAND_INS defined last, over the script's own."""
        program = (self.FOLDER_STAT + self.passwd_stub(homes) + "declare -A AGENTS=([m1]=1 [m2]=1)\n"
                   + self.read_key_line() + shell_function(self.script, "is_box_account") + shell_function(self.script, "strip_suffix")
                   + shell_function(self.script, "agent_folders") + shell_function(self.script, "folder_bases")
                   + stand_ins + 'folder_bases "$@"')
        run = subprocess.run(["bash", "-c", "set -euo pipefail\n" + program, "x", *map(str, roots), "--", *users],
                             env={**os.environ, "STRANGERS": ":".join(map(str, strangers))},
                             text=True, capture_output=True, timeout=20)
        self.assertEqual(run.returncode, 0, run.stderr)
        return run.stdout.splitlines()

    @staticmethod
    def made_under(base, agent="m1"):
        """An agent's folder of the ruled layout under BASE: its territory and
        its home."""
        (base / agent / f"weaver-{agent}").mkdir(parents=True)
        (base / agent / "home").mkdir()
        return base / agent

    def test_a_folder_made_under_a_moved_record_is_archived_and_its_limit_cleared(self):
        # **#594's acceptance** (Codex on #582): m1 was made under base A and
        # capped, then the stack record moved to B. Discovery finds A from
        # m1's own facts, so the folder is an agent's folder, its territory
        # is covered and not listed alone, its home goes with it, A's other
        # entries are named as left, and the folder's mark gives the limit,
        # which the purge sets to none. Its derivation lists the folder, the
        # limit and the account, never A. Perturbation: drop the joining
        # loop, and the folder is missed and the limit is never listed.
        script = self.script
        q = shlex.quote

        def segment(start, end):
            return script[script.index(start):script.index(end)]

        def through_line(start, last):
            i = script.index(start)
            j = script.index(last, i)
            return script[i:script.index("\n", j) + 1]

        with tempfile.TemporaryDirectory() as scratch:
            top = Path(scratch).resolve()
            base_a, base_b = top / "state-a", top / "state-b"
            folder = self.made_under(base_a)
            (base_a / "stray").mkdir()
            base_b.mkdir()
            root = top / "admin" / "m1"
            root.mkdir(parents=True)
            (root / "territory").write_text(f"{folder / 'weaver-m1'}\n")
            log = top / "setquota.log"
            stubs = (self.FOLDER_STAT + self.passwd_stub({"weaver-m1": folder / "home"})
                     + 'findmnt() { case "$3" in OPTIONS) echo "rw,relatime,prjquota" ;; TARGET) echo "/state drive" ;; esac; }\n'
                     + 'lsattr() { echo "    12345 --------------P--- ${@: -1}"; }\n'
                     + f'setquota() {{ printf "%s\\n" "$*" >> {q(str(log))}; }}\n'
                     + "plan() { printf 'PLAN %s\\n' \"$*\"; }\nsay() { :; }\n")
            program = ("set -euo pipefail\n" + stubs + self.read_key_line()
                       + f"OP_HOME=\nAGENT_ROOTS=({q(str(root))})\nWEAVER_USERS=(weaver-m1)\nWEAVER_GROUPS=()\n"
                       + "SUDO_RULES=(); LDSO_CONFS=(); TMP_PATHS=(); TERRITORY_PATHS=()\nLEGACY_DIR=/nonexistent/weaver\n"
                       + "declare -A AGENTS=([m1]=1) PREFIXES=() LOG_PATHS=() COORD_ROOTS=() BASES_SAID=()\n"
                       + f"declare -A TERRITORY_BASES=([{q(str(base_b))}]=1)\n"
                       + "".join(shell_function(script, fn) for fn in (
                           "in_operator_home", "is_box_account", "strip_suffix", "agent_folders", "folder_bases",
                           "resolved", "left_in_base", "spanned", "spans_a_base", "candidate", "home_projects",
                           "plan_home_quotas", "remove_home_quotas", "territories_outside", "home_within",
                           "purge_candidates"))
                       + segment("# **The bases an agent's own facts name join the record's**", "# **home_projects FOLDER...")
                       + through_line("HOME_PROJECTS=()\n", 'plan_home_quotas "${HOME_PROJECTS[@]}"')
                       + segment("TERRITORIES_JUDGED=1\n", "# **The earlier layout's directory and each path")
                       + segment("# **A home is read from its account wherever it stands**", "# **tmp_paths DIR:")
                       + 'printf "FOLDER %s\\n" "${AGENT_FOLDERS[@]}"\nprintf "OWN %s\\n" "${OWN_TERRITORIES[@]}"\n'
                       + "purge_candidates | sed 's/^/LISTED /'\n"
                       + "mapfile -t Q < <(purge_candidates | sed -n 's/^quota //p')\nremove_home_quotas \"${Q[@]}\"\n")
            run = subprocess.run(["bash", "-c", program], env={**os.environ, "STRANGERS": ""},
                                 text=True, capture_output=True, timeout=20)
            self.assertEqual(run.returncode, 0, run.stdout + run.stderr)
            got = run.stdout.splitlines()
            self.assertIn(f"PLAN {base_a}  a base an agent's territory key or home names, beyond the stack record's",
                          got)
            self.assertIn(f"FOLDER {folder}", got)
            self.assertIn("OWN ", got, "the territory is covered by its folder, not listed alone")
            self.assertIn(f"PLAN {folder / 'home'}  (home of weaver-m1, archived with {folder})", got)
            self.assertIn(f"PLAN {base_a / 'stray'}  in the base, of no agent this discovery found: left in place, untouched",
                          got)
            for line in (f"path {folder}", "user weaver-m1", "quota 12345 /state drive"):
                self.assertIn(f"LISTED {line}", got)
            self.assertNotIn(f"LISTED path {base_a}", got)
            self.assertEqual(log.read_text().splitlines(), ["-P 12345 0 0 0 0 /state drive"])
            # **A base the record names already, however spelled, is not
            # joined twice**: the record naming A through B's parent says no
            # new base and names A's stray entry once. Perturbation: drop the
            # canonical comparison, and A is said again and its stray twice.
            spelled = f"{base_b}/../{base_a.name}"
            again = subprocess.run(["bash", "-c", program.replace(
                f"declare -A TERRITORY_BASES=([{q(str(base_b))}]=1)\n",
                f"declare -A TERRITORY_BASES=([{q(spelled)}]=1)\n")],
                env={**os.environ, "STRANGERS": ""}, text=True, capture_output=True, timeout=20)
            self.assertEqual(again.returncode, 0, again.stdout + again.stderr)
            seen = again.stdout.splitlines()
            self.assertFalse([line for line in seen if "beyond the stack record's" in line], again.stdout)
            self.assertEqual(len([line for line in seen if line.startswith(f"PLAN {base_a / 'stray'}  ")]), 1,
                             again.stdout)
            self.assertIn(f"FOLDER {folder}", seen)
        self.assertIn('done < <(folder_bases "${AGENT_ROOTS[@]}" -- "${WEAVER_USERS[@]}")\n'
                      'AGENT_FOLDERS=()\nAGENT_LISTED=$(agent_folders "${!TERRITORY_BASES[@]}")\n', script)

    def test_an_entry_that_is_a_base_or_holds_what_is_taken_is_not_named_left(self):
        # **The self-check on #594**: a base found from an agent's facts can
        # sit in another base, and an entry of a base can hold a root's own
        # territory the purge takes. Neither is named as left, and a stray
        # entry still is. Perturbation: compare each entry with what is taken
        # exactly again, and the inner base and the holder read as left.
        with tempfile.TemporaryDirectory() as scratch:
            top = Path(scratch).resolve()
            outer = top / "srv"
            inner = outer / "state"
            folder = self.made_under(inner)
            custom = outer / "custom" / "weaver-m2"
            custom.mkdir(parents=True)
            (outer / "stray").mkdir()
            program = (f"declare -A TERRITORY_BASES=([{shlex.quote(str(outer))}]=1 [{shlex.quote(str(inner))}]=1)\n"
                       + shell_function(self.script, "resolved") + shell_function(self.script, "left_in_base")
                       + 'left_in_base "$@"')
            run = subprocess.run(["bash", "-c", "set -euo pipefail\n" + program, "x", str(outer), str(folder),
                                  str(custom)], text=True, capture_output=True, timeout=20)
            self.assertEqual(run.returncode, 0, run.stderr)
            self.assertEqual(run.stdout.splitlines(), [str(outer / "stray")])

    def test_a_link_taken_is_compared_as_the_link_and_a_base_named_through_one_as_its_directory(self):
        """**CodeRabbit on #598**: the purge removes a link and never what it
        points to, so left_in_base compares each path taken as the purge
        removes it. A link taken that points at an entry of the base leaves
        the entry named as left, and a link taken that is itself an entry is
        not named. A base is never removed, so one named through a link is its
        directory, and the entry that is that base is not named. Perturbations:
        compare what is taken canonical, as before, and the entry pointed at
        is dropped while the link is named. Compare the bases resolved, and
        the base named through a link is named."""
        with tempfile.TemporaryDirectory() as scratch:
            top = Path(scratch).resolve()
            outer = top / "srv"
            (outer / "stuff").mkdir(parents=True)
            (outer / "inner").mkdir()
            (top / "elsewhere").mkdir()
            (outer / "lnk").symlink_to(top / "elsewhere")
            (top / "legacy").symlink_to(outer / "stuff")
            (top / "inner-link").symlink_to(outer / "inner")
            q = shlex.quote
            program = (f"declare -A TERRITORY_BASES=([{q(str(outer))}]=1 [{q(str(top / 'inner-link'))}]=1)\n"
                       + shell_function(self.script, "resolved") + shell_function(self.script, "left_in_base")
                       + 'left_in_base "$@"')
            run = subprocess.run(["bash", "-c", self.script_options() + program, "x", str(outer),
                                  str(top / "legacy"), str(outer / "lnk")],
                                 text=True, capture_output=True, timeout=20)
            self.assertEqual(run.returncode, 0, run.stderr)
            self.assertEqual(run.stdout.splitlines(), [str(outer / "stuff")])

    def spanning_run(self, mode, top, bases, folders=(), own=(), homes=(), tmp=(), etc=None, op_home="",
                     legacy=None):
        """decommission.sh's judgment of what holds a base and its refusal
        (#606), run as the script runs them in MODE, with the targets given:
        spanned, then the earlier layout's loop where LEGACY is given, then
        the judgment through the refusal. /etc/weaver is ETC, or a path that
        does not stand. It prints REACHED where the archive or the purge
        would go on."""
        script = self.script
        q = shlex.quote
        start = script.index("# **What holds a base refuses the archive and the purge**")
        end = script.index("\nfi\n", script.index("# **A target that holds a base refuses** (#606)")) + 4
        judgement = script[start:end]
        call = "judge_spanning \"the agents' roots and the stack record\" /etc/weaver\n"
        self.assertEqual(judgement.count(call), 1)
        judgement = judgement.replace(call, call.replace("/etc/weaver", q(str(etc or top / "no-etc-weaver"))))
        legacy_loop = ""
        if legacy is not None:
            i = script.index('for d in "$LEGACY_DIR" "${!LOG_PATHS[@]}"; do\n')
            legacy_loop = script[i:script.index("\ndone\n", i) + 6]

        def arr(name, paths):
            return f"{name}=(" + " ".join(q(str(p)) for p in paths) + ")\n"
        program = (self.script_options()
                   + "plan() { printf 'PLAN %s\\n' \"$*\"; }\nsay() { :; }\n"
                   + "die() { printf 'DIE %s\\n' \"$*\"; exit 1; }\n"
                   + f"MODE={mode}\nOP_HOME={q(str(op_home))}\nTERRITORIES_JUDGED=1\nTERRITORY_PATHS=()\n"
                   + f"LEGACY_DIR={q(str(legacy or top / 'no-legacy'))}\n"
                   + "declare -A LOG_UNREAD=() LOG_PATHS=() TERRITORY_BASES=()\n"
                   + "".join(f"TERRITORY_BASES[{q(str(b))}]=1\n" for b in bases)
                   + arr("AGENT_FOLDERS", folders) + arr("OWN_TERRITORIES", own) + arr("HOMES", homes)
                   + arr("TMP_PATHS", tmp)
                   + shell_function(script, "spanned") + shell_function(script, "spans_a_base")
                   + legacy_loop + judgement + 'echo "REACHED $MODE"\n')
        return subprocess.run(["bash", "-c", program], text=True, capture_output=True, timeout=20)

    def test_a_target_that_holds_a_base_refuses_the_archive_and_the_purge(self):
        """**#606, the Planner's ruling of 2026-11-17**: the archive and the
        purge drop every path that holds a base, so a target that holds one
        was planned as taken, archived nowhere and purged nowhere, while the
        purge removed its accounts. Each shape is judged once every target is
        known, named in the plan with what it holds, and the archive and the
        purge refuse, naming it and pointing at the take-down section. The
        shapes: an agent made inside another's folder, the record naming a
        territory with nothing made inside, a root's own territory holding a
        base, a territory key inside another's custom territory, a home under
        /home, a weaver entry in /tmp, and /etc/weaver. Perturbations: drop the
        refusal's die, or run it in the archive alone, and a mode goes on.
        Leave the homes, the /tmp entries or /etc/weaver out of the judgment,
        or the own territories out of spanned, and that shape passes. Judge
        before the own territories, the homes and the /tmp entries are known,
        and the placement fails."""
        script = self.script
        # **Judged once every target is known, and refused before either
        # mode acts.**
        calls = [m.start() for m in re.finditer(r'^judge_spanning "', script, re.M)]
        self.assertTrue(calls)
        for known in ("mapfile -t TMP_PATHS < <(tmp_paths /tmp)", "HOMES+=(\"$h\")",
                      '[ -z "$OWN_LISTED" ] || mapfile -t OWN_TERRITORIES <<< "$OWN_LISTED"',
                      '[ -z "$AGENT_LISTED" ] || mapfile -t AGENT_FOLDERS <<< "$AGENT_LISTED"'):
            self.assertLess(script.index(known), min(calls), known)
        self.assertLess(max(calls), script.index('[ "$MODE" = plan ] && {'))
        refused = script.index("# **A target that holds a base refuses** (#606)")
        self.assertLess(script.index('[ "$MODE" = plan ] && {'), refused)
        self.assertLess(refused, script.index('if [ "$MODE" = archive ]; then'))
        self.assertLess(refused, script.index(" 3. purge\n"))
        with tempfile.TemporaryDirectory() as scratch:
            top = Path(scratch).resolve()
            agents = top / "agents"
            m1 = agents / "m1"
            (m1 / "weaver-m1").mkdir(parents=True)
            (m1 / "home").mkdir()
            (m1 / "m2" / "weaver-m2").mkdir(parents=True)
            custom = top / "srv" / "weaver-m3"
            (custom / "agents").mkdir(parents=True)
            (custom / "weaver-m4").mkdir()
            home = top / "home" / "weaver-m5"
            (home / "agents").mkdir(parents=True)
            entry = top / "tmp" / "weaver-m6"
            (entry / "agents").mkdir(parents=True)
            etc = top / "etc-weaver"
            (etc / "agents").mkdir(parents=True)
            cases = [
                ("an agent made inside another's folder",
                 dict(bases=(agents, m1), folders=(m1, m1 / "m2")),
                 m1, "an agent's folder", f"is the base {m1}"),
                ("the record naming a territory, nothing made inside",
                 dict(bases=(agents, m1 / "weaver-m1"), folders=(m1,)),
                 m1, "an agent's folder", f"holds the base {m1 / 'weaver-m1'}"),
                ("a root's own territory holding a base",
                 dict(bases=(agents, custom / "agents"), own=(custom,)),
                 custom, "a root's own territory", f"holds the base {custom / 'agents'}"),
                ("a territory key inside another's custom territory",
                 dict(bases=(agents,), own=(custom, custom / "weaver-m4")),
                 custom, "a root's own territory", f"holds the root's own territory {custom / 'weaver-m4'}"),
                ("a home under /home",
                 dict(bases=(agents, home / "agents"), homes=(home,)),
                 home, "a home under /home", f"holds the base {home / 'agents'}"),
                ("a weaver entry in /tmp",
                 dict(bases=(agents, entry / "agents"), tmp=(entry,)),
                 entry, "a weaver entry in /tmp", f"holds the base {entry / 'agents'}"),
                ("/etc/weaver",
                 dict(bases=(agents, etc / "agents"), etc=etc),
                 etc, "the agents' roots and the stack record", f"holds the base {etc / 'agents'}"),
            ]
            for label, given, target, kind, what in cases:
                with self.subTest(label):
                    planned = self.spanning_run("plan", top, **given)
                    self.assertEqual(planned.returncode, 0, planned.stderr)
                    refused_lines = [l for l in planned.stdout.splitlines() if l.startswith("PLAN REFUSED:")]
                    self.assertEqual(refused_lines, [f"PLAN REFUSED: {target}  {kind} that {what}: it is neither "
                                                     "archived nor purged, and the purge would remove the accounts"])
                    self.assertNotIn("REACHED", planned.stdout)
                    for mode in ("archive", "purge"):
                        ran = self.spanning_run(mode, top, **given)
                        self.assertEqual(ran.returncode, 1, mode)
                        self.assertNotIn("REACHED", ran.stdout, mode)
                        said = [l for l in ran.stdout.splitlines() if l.startswith("DIE ")]
                        self.assertEqual(len(said), 1, ran.stdout)
                        self.assertTrue(said[0].startswith(f"DIE this run archives and purges nothing: {target} "
                                                           f"({kind}) {what}. "), said[0])
                        self.assertIn("Take down by hand any agent made inside another, by "
                                      "deploy/HowToDeployANewAgent.md section 7, set the stack record's "
                                      "agent-directory back to the base the agents were made in, put back any "
                                      "territory key that was moved, then rerun", said[0])

    def test_a_layout_with_no_base_inside_a_target_does_not_refuse(self):
        """**#606**: the ruled layout, a base beside an agent's folder whose
        name only begins like the folder's, a base above every folder, and
        the earlier layout's directory holding a base refuse nothing, and the
        archive and the purge go on. The earlier layout's directory is said
        to be left in place, which is true, and is never judged. **A link
        holds nothing** (the self-check on #606): an agent's link in /tmp to
        a base, or to /, is never judged, since the purge removes the link and
        never what it points to, so an agent cannot block its own take-down
        with one. Perturbations: drop spanned's slash after a base, and the
        near name refuses. Judge a link, and the links refuse."""
        with tempfile.TemporaryDirectory() as scratch:
            top = Path(scratch).resolve()
            agents = top / "agents"
            for a in ("m1", "m2"):
                (agents / a / f"weaver-{a}").mkdir(parents=True)
            (agents / "m10").mkdir()
            legacy = top / "legacy"
            (legacy / "agents").mkdir(parents=True)
            (top / "etc-weaver" / "admin").mkdir(parents=True)
            (top / "home" / "weaver-m1").mkdir(parents=True)
            (top / "tmp" / "weaver-m1").mkdir(parents=True)
            (top / "tmp" / "weaver-m2").symlink_to(agents)
            (top / "tmp" / "weaver-m3").symlink_to("/")
            folders = (agents / "m1", agents / "m2")
            common = dict(folders=folders, homes=(top / "home" / "weaver-m1",), tmp=(top / "tmp" / "weaver-m1",),
                          etc=top / "etc-weaver", op_home=top / "users" / "op")
            cases = [("the ruled layout", dict(bases=(agents,))),
                     ("a near name", dict(bases=(agents, agents / "m10"))),
                     ("a base above every folder", dict(bases=(agents, top))),
                     ("the earlier layout's directory holding a base",
                      dict(bases=(agents, legacy / "agents"), legacy=legacy)),
                     ("an agent's link in /tmp to a base, and one to /",
                      dict(bases=(agents,), tmp=(top / "tmp" / "weaver-m1", top / "tmp" / "weaver-m2",
                                                 top / "tmp" / "weaver-m3")))]
            for label, given in cases:
                with self.subTest(label):
                    for mode in ("plan", "archive", "purge"):
                        ran = self.spanning_run(mode, top, **{**common, **given})
                        self.assertEqual(ran.returncode, 0, ran.stdout + ran.stderr)
                        self.assertNotIn("REFUSED", ran.stdout)
                        self.assertEqual("REACHED" in ran.stdout, mode != "plan", ran.stdout)
                        if "legacy" in given:
                            self.assertIn(f"PLAN {legacy}  a base, or above a base or an agent's folder: not taken "
                                          "whole, left in place", ran.stdout)

    def test_spanned_names_what_a_path_spans_and_spans_a_base_agrees(self):
        """**#606**: spanned holds the comparison spans_a_base held, and says
        what a path is or holds: a base, an agent's folder, a root's own
        territory, the operator's home, the root directory, or nothing,
        canonical, past a slash, a path that does not resolve compared as
        written. spans_a_base answers as spanned does on every case, so the
        refusal and the archive's drop read one comparison. Perturbations:
        keep spans_a_base its own copy and drop the folder check from
        spanned, and they disagree. Drop a slash, and a near name spans."""
        with tempfile.TemporaryDirectory() as scratch:
            top = Path(scratch).resolve()
            base = top / "state" / "agents"
            (base / "m1" / "weaver-m1").mkdir(parents=True)
            (base / "m10").mkdir()
            own = top / "srv" / "weaver-m3"
            own.mkdir(parents=True)
            op = top / "users" / "op"
            op.mkdir(parents=True)
            (top / "link").symlink_to(base)
            q = shlex.quote
            program = (self.script_options()
                       + f"OP_HOME={q(str(op))}\ndeclare -A TERRITORY_BASES=([{q(str(base))}]=1)\n"
                       + f"AGENT_FOLDERS=({q(str(base / 'm1'))})\nOWN_TERRITORIES=({q(str(own))})\n"
                       + shell_function(self.script, "spanned") + shell_function(self.script, "spans_a_base")
                       + 'for p in "$@"; do\n  s=0; what=$(spanned "$p") || s=$?\n  a=0; spans_a_base "$p" || a=$?\n'
                       + '  printf "%s|%s|%s\\n" "$s" "$a" "$what"\ndone\n')
            cases = [(base, f"is the base {base}"), (top / "link", f"is the base {base}"),
                     (top / "state", f"holds the base {base}"), (top / "state" / "agent", None),
                     (top, f"holds the operator's home {op}"), (base / "m1", None), (base / "m10", None),
                     (base / "m1" / "weaver-m1", None), (top / "srv", f"holds the root's own territory {own}"),
                     (own, None), (op, "is the operator's home"), (top / "users", f"holds the operator's home {op}"),
                     (Path("/"), "is the root directory"), (top / "gone", None), (top / "gone" / "..", None)]
            run = subprocess.run(["bash", "-c", program, "x", *[str(p) for p, _ in cases]],
                                 text=True, capture_output=True, timeout=20)
            self.assertEqual(run.returncode, 0, run.stderr)
            for (path, what), line in zip(cases, run.stdout.splitlines(), strict=True):
                with self.subTest(str(path)):
                    s, a, said = line.split("|", 2)
                    self.assertEqual(s, a, line)
                    self.assertEqual((s, said), ("0", what) if what else ("1", ""), line)
            # A folder above an agent's folder is named as holding it.
            program2 = program.replace(f"AGENT_FOLDERS=({q(str(base / 'm1'))})",
                                       f"AGENT_FOLDERS=({q(str(base / 'm1' / 'weaver-m1'))})")
            run = subprocess.run(["bash", "-c", program2, "x", str(base / "m1")], text=True, capture_output=True,
                                 timeout=20)
            self.assertEqual(run.stdout, f"0|0|holds the agent's folder {base / 'm1' / 'weaver-m1'}\n", run.stderr)

    def test_the_plan_names_the_host_without_a_hostname_program(self):
        """**#610**: the appliance image carries no hostname program, and the
        first live take-down stopped at the plan's `HOST=$(hostname)` with
        exit 127, under `set -euo pipefail`. The plan's head, from the
        operator through the box's lines, runs here under a PATH whose box
        half is /usr/bin without hostname, and names the host uname names.
        No deployed script calls hostname. Perturbation: put back
        `HOST=$(hostname)`, and the head exits 127."""
        script = self.script
        head = script[script.index("OPERATOR=${SUDO_USER:-"):script.index('plan "date      $(date -Iseconds)"\n')]
        with tempfile.TemporaryDirectory() as scratch:
            view = box_view(Path(scratch) / "view", without=("hostname",))
            path = str(STAND_IN_GUARD) + os.pathsep + str(view)
            self.assertIsNone(shutil.which("hostname", path=path))
            program = (self.script_options() + "plan() { printf 'PLAN %s\\n' \"$*\"; }\nsay() { :; }\n"
                       + "die() { printf 'DIE %s\\n' \"$*\"; exit 1; }\n" + head)
            operator = pwd.getpwuid(os.getuid()).pw_name
            run = subprocess.run(["bash", "-c", program], env={**os.environ, "PATH": path, "SUDO_USER": operator},
                                 text=True, capture_output=True, timeout=20)
            self.assertEqual(run.returncode, 0, run.stdout + run.stderr)
            self.assertIn(f"PLAN host      {os.uname().nodename}\n", run.stdout)
            self.assertIn(f"PLAN operator  {operator}\n", run.stdout)
        for deployed in sorted(DEPLOY.glob("*.sh")):
            with self.subTest(script=deployed.name):
                calls = [n for n, line in enumerate(deployed.read_text().splitlines(), 1)
                         if not line.lstrip().startswith("#") and re.search(r"(?<![\w.-])hostname(?![\w.-])", line)]
                self.assertEqual(calls, [], f"{deployed.name} calls hostname")

    def test_a_base_under_the_operators_home_is_said_once(self):
        # **The self-check on #594**: a base the record names under the
        # operator's home is said and dropped, and the same base found again
        # from an agent's facts, in another spelling, is not said twice nor
        # joined. Perturbation: drop the said-once check, and it is said
        # twice.
        script = self.script
        prune = script[script.index("declare -A BASES_SAID=()\n"):]
        prune = prune[:prune.index("\ndone\n") + len("\ndone\n")]
        join = script[script.index("# **The bases an agent's own facts name join the record's**"):]
        join = join[:join.index('done < <(folder_bases "${AGENT_ROOTS[@]}" -- "${WEAVER_USERS[@]}")\n')]
        with tempfile.TemporaryDirectory() as scratch:
            top = Path(scratch).resolve()
            base = top / "operator" / "state"
            base.mkdir(parents=True)
            spelled = f"{top}/operator/../operator/state"
            program = ("set -euo pipefail\n" + f"OP_HOME={shlex.quote(str(top / 'operator'))}\n"
                       + f"declare -A TERRITORY_BASES=([{shlex.quote(spelled)}]=1)\nAGENT_ROOTS=()\nWEAVER_USERS=()\n"
                       + "plan() { printf 'PLAN %s\\n' \"$*\"; }\n"
                       + f"folder_bases() {{ printf '%s\\n' {shlex.quote(str(base))}; }}\n"
                       + shell_function(script, "in_operator_home") + prune + join
                       + 'done < <(folder_bases "${AGENT_ROOTS[@]}" -- "${WEAVER_USERS[@]}")\n'
                       + 'printf "BASES %s\\n" "${#TERRITORY_BASES[@]}"\n')
            run = subprocess.run(["bash", "-c", program], text=True, capture_output=True, timeout=20)
            self.assertEqual(run.returncode, 0, run.stdout + run.stderr)
            self.assertEqual(run.stdout.splitlines(),
                             [f"PLAN {spelled}  under the operator's home: not touched", "BASES 0"])

    def test_a_roots_territory_key_alone_finds_the_agents_folder(self):
        # #594: the account is gone, as on a purge's rerun, so only the root's
        # `territory` key names the folder, written plain or with a trailing
        # slash. Perturbations: drop the key's lookup, or split the key as
        # written, and nothing is found.
        with tempfile.TemporaryDirectory() as scratch:
            top = Path(scratch).resolve()
            folder = self.made_under(top / "state-a")
            root = top / "admin" / "m1"
            root.mkdir(parents=True)
            for key in (f"{folder / 'weaver-m1'}", f"{folder / 'weaver-m1'}/"):
                (root / "territory").write_text(f"{key}\n")
                self.assertEqual(self.folder_bases([root], [], {}), [str(top / "state-a")], key)

    def test_an_accounts_home_alone_finds_the_agents_folder(self):
        # #594: the root carries no `territory` key and the territory is
        # gone, so only the account's home in passwd names the folder, which
        # holds only the home its account owns. The home is read plain or
        # with a trailing slash, and a suffixed account's home is read as its
        # agent's. Perturbations: drop the home's lookup, or split the home
        # as written, and nothing is found.
        with tempfile.TemporaryDirectory() as scratch:
            top = Path(scratch).resolve()
            folder = top / "state-a" / "m1"
            (folder / "home").mkdir(parents=True)
            root = top / "admin" / "m1"
            root.mkdir(parents=True)
            for home in (folder / "home", f"{folder / 'home'}/"):
                self.assertEqual(self.folder_bases([root], ["weaver-m1"], {"weaver-m1": home}),
                                 [str(top / "state-a")], home)
            self.assertEqual(self.folder_bases([], ["weaver-m1-state"], {"weaver-m1-state": folder / "home"}),
                             [str(top / "state-a")])

    def test_a_key_or_a_home_naming_no_agents_folder_adds_no_base(self):
        # #594: a folder is one only by agent_folders' checks, so a key or a
        # home naming a folder that is not root's, a folder not named for
        # its agent, a home not named `home`, or a folder in the root
        # directory adds no base, and a territory the key names is judged
        # alone by territories_outside as before. A refusal last answers 0.
        # Perturbations: skip the check against agent_folders, the folder's
        # name check or the root directory's refusal, or end the loop on the
        # check's own status, and a read below fails.
        with tempfile.TemporaryDirectory() as scratch:
            top = Path(scratch).resolve()
            base = top / "state-a"
            stranger = self.made_under(base)
            (base / "other" / "weaver-m2").mkdir(parents=True)
            roots = top / "admin"
            for name, key in (("m1", stranger / "weaver-m1"), ("m2", base / "other" / "weaver-m2")):
                (roots / name).mkdir(parents=True)
                (roots / name / "territory").write_text(f"{key}\n")
            # m1 last, so the lookup's last look is a refusal and the function
            # still answers 0 (the self-check on #594).
            self.assertEqual(self.folder_bases([roots / "m2", roots / "m1"], [], {}, strangers=[stranger]), [])
            # A home whose folder is another agent's name, one not named
            # `home`, and a box account's home in a real folder.
            self.assertEqual(self.folder_bases([], ["weaver-m2", "weaver-m1", "weaver-gatecon"], {
                "weaver-m2": stranger / "home", "weaver-m1": stranger / "weaver-m1",
                "weaver-gatecon": stranger / "home"}), [])
            # The territory under `other/` is judged alone, as before.
            listed = self.run_fn("territories_outside", "--", str(roots / "m2"), env=self.stat_env(top))
            self.assertEqual(listed, str(base / "other" / "weaver-m2"))
            # Once the folder is root's, the key finds it.
            self.assertEqual(self.folder_bases([roots / "m1"], [], {}), [str(base)])
            # A key naming `<X>/other/weaver-m2` names no folder of m2's, even
            # where X holds one.
            self.made_under(base, "m2")
            self.assertEqual(self.folder_bases([roots / "m2"], [], {}), [])
            # A folder in the root directory, read through stand-ins that
            # resolve every existing path to it and pass every folder.
            self.assertEqual(self.folder_bases([roots / "m1"], [], {}, stand_ins=(
                'realpath() { case "$1" in -e) echo / ;; *) command realpath "$@" ;; esac; }\n'
                'agent_folders() { printf "%s\\n" "$1/m1"; }\n')), [])

# The account database's stand-ins for the purge (#140): getent reads the
# model's passwd and group, userdel removes the user from both and, as the real
# one does under USERGROUPS_ENAB, the user's private group where no other member
# holds it, and groupdel removes a group, failing on one that is gone.
ACCOUNT_MODEL = r"""#!/usr/bin/env python3
import os, pathlib, sys
name = pathlib.Path(sys.argv[0]).name
args = sys.argv[1:]
model = pathlib.Path(os.environ["MODEL"])
def rows(kind):
    return [l.split(":") for l in (model / kind).read_text().splitlines() if l]
def write(kind, kept):
    (model / kind).write_text("".join(":".join(f) + "\n" for f in kept))
if name == "getent":
    # An enumeration answers 0 however few rows match, as on a box, where root's
    # own always stand; a lookup of a name absent answers 2. A key of digits is
    # looked up by number, as getent looks it up.
    hit = [f for f in rows(args[0])
           if len(args) == 1 or (f[2] == args[1] if args[1].isdigit() else f[0] == args[1])]
    if not hit and len(args) > 1:
        sys.exit(2)
    print("".join(":".join(f) + "\n" for f in hit), end="")
elif name == "userdel":
    with open(model / "calls", "a") as calls:
        calls.write("userdel " + " ".join(args) + "\n")
    who = args[-1]
    if not any(f[0] == who for f in rows("passwd")):
        sys.exit(6)
    write("passwd", [f for f in rows("passwd") if f[0] != who])
    kept = []
    for f in rows("group"):
        members = [m for m in f[3].split(",") if m and m != who]
        if f[0] == who and not members:
            continue
        kept.append(f[:3] + [",".join(members)])
    write("group", kept)
elif name == "groupdel":
    if not any(f[0] == args[-1] for f in rows("group")):
        sys.exit(6)
    write("group", [f for f in rows("group") if f[0] != args[-1]])
"""


class PurgeRecordTests(unittest.TestCase):
    """**Root's record of the purge, and its rerun** (#140; deploy/REDEPLOY.md
    section 2). The purge's own code runs over a model box: the record's
    reading and writing, its decision between a first run and a rerun, and
    its removal phases in the order the script states them, cut after each
    phase. The model runs under `unshare --map-root-user`, so the box's files
    are root's as the record's checks need. The account database is laid out
    as create-agent makes it: the relay with no group of its own, the agent's
    and the member's groups holding the operator and the connector, so only
    the connector's group goes with its user."""

    def setUp(self):
        self.script = (DEPLOY / "decommission.sh").read_text()
        probe = subprocess.run(["unshare", "--map-root-user", "true"], capture_output=True)
        if probe.returncode != 0:
            self.skipTest("unshare --map-root-user is not available")

    def box(self):
        scratch = tempfile.TemporaryDirectory(prefix="purge-record-")
        self.addCleanup(scratch.cleanup)
        box = Path(scratch.name).resolve()
        root = box / "etc/weaver/admin/a"
        root.mkdir(parents=True)
        (root / "territory").write_text(f"{box}/var/lib/weaver-agent/weaver-a\n")
        for d in ("var/lib/weaver-agent/weaver-a", "opt/weaver/bin", "opt/weaver/lib", "tmp/weaver-a-scratch",
                  "model", "etc/sudoers.d"):
            (box / d).mkdir(parents=True, exist_ok=True)
        for d in (box, box / "var", box / "var/lib"):
            d.chmod(0o755)
        users = ["weaver-a", "weaver-a-state", "weaver-a-relay", "weaver-a-admincon"]
        members = {"weaver-a": "op", "weaver-a-state": "weaver-a-admincon,op", "weaver-a-trace": "op",
                   "weaver-a-admin": "weaver-a-admincon,op", "weaver-a-admincon": ""}
        groups = list(members)
        (box / "model/passwd").write_text("".join(f"{u}:x:{2001 + i}:{2001 + i}::/nonexistent:/bin/false\n"
                                                  for i, u in enumerate(users)))
        (box / "model/group").write_text("".join(f"{g}:x:{3001 + i}:{members[g]}\n" for i, g in enumerate(groups)))
        # The rule the archive disabled and listed.
        (box / "etc/sudoers.d/.weaver-a.decommissioning").write_text("op ALL=(root) NOPASSWD: /bin/true\n")
        # /etc/weaver listed last of the paths: the archive writes it first,
        # and the purge's order must not rest on the list's.
        listed = ([f"path {box}/etc/sudoers.d/.weaver-a.decommissioning", f"path {box}/var/lib/weaver-agent", f"path {box}/opt/weaver/bin",
                   f"path {box}/opt/weaver/lib", f"path {box}/tmp/weaver-a-scratch", f"path {box}/etc/weaver"]
                  + [f"user {u}" for u in users] + [f"group {g}" for g in groups])
        (box / "PURGE-LIST").write_text("".join(line + "\n" for line in listed))
        for tool in ("getent", "userdel", "groupdel"):
            stand_in = box / "model" / tool
            stand_in.write_text(ACCOUNT_MODEL)
            stand_in.chmod(0o755)
        for tool, body in (("ldconfig", "#!/bin/sh\n[ \"${1:-}\" = -p ] && echo libggml.so\nexit 0\n"),
                           ("systemctl", "#!/bin/sh\necho \"systemctl $*\" >> \"$MODEL/calls\"\nexit 0\n"),
                           # The drive's project quotas, as the model's
                           # `quotas.csv` holds them, or unreadable.
                           ("repquota", "#!/bin/sh\n[ -e \"$MODEL/repquota-fails\" ] && exit 1\n"
                                        "cat \"$MODEL/quotas.csv\" 2>/dev/null\nexit 0\n")):
            stand_in = box / "model" / tool
            stand_in.write_text(body)
            stand_in.chmod(0o755)
        return box

    def run_purge(self, box, phases=None, removals=None, failing=None, record_remove_fails=False, units=(),
                  quota_folder=None):
        """One `--purge` over BOX from the record's decision on: every phase,
        or the first PHASES of them in the script's own order, then a stop, or
        a stop after its REMOVALS-th `rm`, mid-phase, as an interrupt lands. A
        command FAILING names (`rm`, `mv` or `rmdir`) fails on every call, and
        with RECORD_REMOVE_FAILS the record's own removal fails. With
        QUOTA_FOLDER, the home quotas' own discovery over that agent folder
        and the refusal that follows it run first, as the script runs them
        before either mode acts, and the derivation keeps a listed limit only
        where discovery found it (#585)."""
        text = self.script
        cut = lambda a, b: text[text.index(a):text.index(b, text.index(a))]
        helpers = cut("RECORD_PARENT=/var/lib", "# **Everything under DEST is read as the operator**")
        # From the record's decision through the door and the record's writing.
        decision = cut('[[ "$COPY_SUM" =~', 'say "units"\n')
        first = text.index('say "units"\n', text.index('[[ "$COPY_SUM" =~'))
        region = text[first:text.index('say "purged. the archive is $DEST"', first)]
        if phases is not None:
            headers = sorted(region.index(h) for h in ('say "the config roots"\n', 'say "paths"\n',
                                                       'say "accounts"\n'))
            ends = headers[1:] + [region.index("# The prefixes the record holds")]
            region = region[:ends[phases - 1]] + "exit 0\n" if phases else "exit 0\n"
        program = ("set -euo pipefail\nsay() { printf '== %s\\n' \"$*\"; }\nplan() { printf '   %s\\n' \"$*\"; }\n"
                   "die() { printf 'REFUSED: %s\\n' \"$*\" >&2; exit 1; }\n"
                   "purge_lines_outside() { :; }\npurge_lines_within() { cat \"$1\"; }\n"
                   "query_runs() { :; }\n" + shell_function(text, "sudo_rules")
                   # The accounts phase asks which accounts are the box's.
                   + shell_function(text, "is_box_account") + shell_function(text, "box_account_role")
                   # The home quotas' removal (#585), and the rerun's reading
                   # of a listed limit (CodeRabbit on #582).
                   + shell_function(text, "remove_home_quotas") + shell_function(text, "project_limited")
                   + ("" if removals is None else
                      "RM_SEEN=0\nrm() { local s=0; command rm \"$@\" || s=$?; RM_SEEN=$((RM_SEEN + 1)); "
                      f"[ \"$RM_SEEN\" -lt {removals} ] || exit 0; return \"$s\"; }}\n")
                   + ({None: "", "mv": "mv() { return 1; }\n", "rmdir": "rmdir() { return 1; }\n",
                       # `rm -rf`, the paths' removal, or `rm -f`, a sudo rule's.
                       "rm": 'rm() { [ "$1" = -rf ] && return 1; command rm "$@"; }\n',
                       "rm-f": 'rm() { [ "$1" = -f ] && return 1; command rm "$@"; }\n'}[failing])
                   + helpers
                   + ("" if not record_remove_fails else
                      'eval "$(declare -f purge_record | sed "1s/^purge_record/real_purge_record/")"\n'
                      'purge_record() { [ "$1" = remove ] && return 4; real_purge_record "$@"; }\n')
                   + f"PURGE_LIST={shlex.quote(str(box / 'PURGE-LIST'))}\nPURGE_ACTED=$(mktemp)\n"
                   + "trap 'command rm -f -- \"$PURGE_ACTED\"' EXIT\n"
                   + 'COPY_SUM=$(sha256sum < "$PURGE_LIST" | cut -d" " -f1)\nDEST=archive\n'
                   + f"declare -A PREFIXES=([{shlex.quote(str(box / 'opt/weaver'))}]=1)\nACTIVE_UNITS=() RUNNING=()\n"
                   # The roots discovery found, as it finds them on a first run.
                   + "AGENT_ROOTS=(); for r in /etc/weaver/admin*/*/; do [ -d \"$r\" ] && AGENT_ROOTS+=(\"${r%/}\"); done\n"
                   # The rules and the legacy units discovery finds, as it finds them.
                   + f"mapfile -t SUDO_RULES < <(sudo_rules {shlex.quote(str(box / 'etc/sudoers.d'))})\n"
                   + "UNITS=(" + " ".join(shlex.quote(u) for u in units) + ")\nSLICE=weaver.slice\n"
                   + ("" if quota_folder is None else
                      shell_function(text, "home_projects") + shell_function(text, "plan_home_quotas")
                      + f"AGENT_FOLDERS=({shlex.quote(str(quota_folder))})\n"
                      + cut("HOME_PROJECTS=()\n", 'plan_home_quotas "${HOME_PROJECTS[@]}"\n')
                      + 'plan_home_quotas "${HOME_PROJECTS[@]}"\n'
                      + cut("# **A capped home's limit never outlives what names it**", "\nfi\n") + "\nfi\n"
                      + "purge_lines_within() { local l q; while IFS= read -r l; do case $l in\n"
                        "  \"quota \"*) for q in \"${HOME_QUOTAS[@]}\"; do [ \"$l\" != \"quota $q\" ] || printf '%s\\n' \"$l\"; done ;;\n"
                        "  *) printf '%s\\n' \"$l\" ;; esac; done < \"$1\"; }\n")
                   + decision + region + 'echo PURGED\n')
        program = program.replace("/etc/weaver", f"{box}/etc/weaver").replace(
            "RECORD_PARENT=/var/lib", f"RECORD_PARENT={box}/var/lib")
        env = {**os.environ, "PATH": f"{box}/model" + os.pathsep + os.environ["PATH"], "MODEL": str(box / "model")}
        env.pop("BASH_ENV", None)
        return subprocess.run(["unshare", "--map-root-user", "bash", "-c", program], env=env, text=True,
                              capture_output=True, timeout=60)

    def capped_folder(self, box):
        """Agent a's folder on a drive mounted with project quotas, its home
        capped as create-agent.sh caps it: the folder marked with the
        account's uid, 2001, and the limit listed. The mount stand-in fails
        while `mount-unreadable` stands in the model, the mark's while
        `mark-unreadable` does, and setquota logs its calls."""
        folder = box / "state" / "a"
        (folder / "home").mkdir(parents=True)
        with open(box / "PURGE-LIST", "a") as listed:
            listed.write(f"path {folder}\nquota 2001 /state drive\n")
        for tool, body in (
                ("findmnt", '#!/bin/sh\n[ -e "$MODEL/mount-unreadable" ] && exit 1\n'
                            'case "$3" in OPTIONS) echo rw,relatime,prjquota ;; TARGET) echo "/state drive" ;; esac\n'),
                ("lsattr", '#!/bin/sh\nfor last; do :; done\n'
                           'case $last in */home) echo "2001 --------------P--- $last" ;;\n'
                           '*) [ -e "$MODEL/mark-unreadable" ] && exit 1; echo "2001 ---------------- $last" ;; esac\n'),
                ("setquota", '#!/bin/sh\necho "setquota $*" >> "$MODEL/calls"\n')):
            stand_in = box / "model" / tool
            stand_in.write_text(body)
            stand_in.chmod(0o755)
        return folder

    def test_an_unreadable_mount_or_mark_refuses_before_anything_goes(self):
        """**Codex on #588, the Planner's ruling of 2026-11-17**: where a
        capped folder's mount or mark cannot be read, nothing is archived or
        purged, and the refusal names the folder and what could not be read.
        The folder, the account and the limit stand, and so does everything
        else, the config root and the sudo rule among them, with no record
        written. Once the read answers, the purge, a first run again, removes
        all three. Perturbations: today's carry-on order (the purge marked
        failed and going on), or the refusal moved to just before the paths,
        and a read below fails."""
        for what, flag, read in (("mark", "mark-unreadable", "the mark of {} (lsattr)"),
                                 ("mount", "mount-unreadable", "the mount of {} (findmnt)")):
            with self.subTest(what=what):
                box = self.box()
                folder = self.capped_folder(box)
                unread = box / "model" / flag
                unread.write_text("")
                ran = self.run_purge(box, quota_folder=folder)
                self.assertEqual(ran.returncode, 1, ran.stdout + ran.stderr)
                calls = box / "model/calls"
                self.assertTrue((folder / "home").is_dir(), "the folder stands")
                self.assertIn("weaver-a:x:2001:", (box / "model/passwd").read_text(), "the account stands")
                self.assertNotIn("setquota", calls.read_text() if calls.exists() else "", "the limit stands")
                self.assertTrue((box / "etc/weaver/admin/a").is_dir(), "the config root stands")
                self.assertTrue((box / "etc/sudoers.d/.weaver-a.decommissioning").exists(), "the sudo rule stands")
                self.assertFalse(self.record(box).exists(), "no record carries the limit's loss")
                self.assertIn(read.format(folder) + " cannot be read", ran.stderr)
                self.assertIn("HowToDeployANewAgent.md section 7", ran.stderr)
                unread.unlink()
                ran = self.run_purge(box, quota_folder=folder)
                self.assertEqual(ran.returncode, 0, ran.stdout + ran.stderr)
                self.assertIn("setquota -P 2001 0 0 0 0 /state drive", calls.read_text().splitlines())
                self.assertFalse(os.path.lexists(folder))
                self.assert_purged(box)

    def record(self, box):
        sum_ = hashlib.sha256((box / "PURGE-LIST").read_bytes()).hexdigest()
        return box / "var/lib/decommission-weaver" / sum_

    def assert_purged(self, box):
        standing = [p for p in ("etc/weaver", "var/lib/weaver-agent", "opt/weaver/bin", "opt/weaver/lib",
                                "tmp/weaver-a-scratch") if os.path.lexists(box / p)]
        self.assertEqual(standing, [])
        self.assertFalse((box / "opt/weaver").exists(), "the prefix left empty goes, named by the record")
        self.assertEqual((box / "model/passwd").read_text() + (box / "model/group").read_text(), "")
        self.assertFalse(os.path.lexists(box / "var/lib/decommission-weaver"), "the record goes last")

    def roots_without_territory(self, box):
        """The roots a rerun's admin `show` would be asked about whose
        territory is gone: admin refuses those, and the rerun with it."""
        found = []
        for base in (box / "etc/weaver").glob("admin*"):
            for root in base.iterdir():
                if root.is_dir() and not root.name.startswith("."):
                    territory = (root / "territory").read_text().strip() if (root / "territory").exists() else ""
                    if not territory or not os.path.isdir(territory):
                        found.append(str(root))
        return found if (box / "etc/weaver").exists() else []

    def test_a_whole_purge_writes_its_record_and_removes_it_last(self):
        box = self.box()
        ran = self.run_purge(box)
        self.assertEqual(ran.returncode, 0, ran.stdout + ran.stderr)
        self.assertIn("root's record written", ran.stdout)
        # Only the connector's group goes with its user; the agent's and the
        # member's hold the operator and are removed by groupdel.
        self.assertIn("group weaver-a-admincon: already gone with its user", ran.stdout)
        self.assertIn("removed group weaver-a-state", ran.stdout)
        self.assertIn("PURGED", ran.stdout)
        self.assert_purged(box)

    def test_a_completed_purge_run_again_refuses_and_says_why(self):
        box = self.box()
        self.assertEqual(self.run_purge(box).returncode, 0)
        ran = self.run_purge(box)
        self.assertEqual(ran.returncode, 1, ran.stdout + ran.stderr)
        self.assertIn("a purge has completed here", ran.stderr)

    def test_an_agent_taken_down_by_hand_after_the_archive_leaves_the_rest_to_purge(self):
        """An account gone is not the sign of a purge begun: a box where the
        relay was removed by hand after the archive purges the rest."""
        box = self.box()
        passwd = box / "model/passwd"
        passwd.write_text("".join(l + "\n" for l in passwd.read_text().splitlines() if not l.startswith("weaver-a-relay:")))
        ran = self.run_purge(box)
        self.assertEqual(ran.returncode, 0, ran.stdout + ran.stderr)
        self.assertIn("user weaver-a-relay: already gone", ran.stdout)
        self.assert_purged(box)

    def test_a_path_replaced_since_it_was_judged_is_not_removed(self):
        """A rerun removes a path only while it is the object judged: a /tmp
        name another principal took after a reboot refuses the rerun, naming
        it, and stands. Perturbation: drop the identity check, and the rerun
        removes the stranger's entry as root."""
        box = self.box()
        self.assertEqual(self.run_purge(box, phases=1).returncode, 0)
        entry = box / "tmp/weaver-a-scratch"
        entry.rmdir()
        entry.mkdir()
        (entry / "theirs").write_text("")
        ran = self.run_purge(box)
        self.assertEqual(ran.returncode, 1, ran.stdout + ran.stderr)
        self.assertIn(f"{entry}, which is not the object this purge judged", ran.stderr)
        self.assertTrue((entry / "theirs").exists())
        self.assertTrue(self.record(box).exists())

    def resum(self, record, edit):
        """RECORD's lines through EDIT, summed again as the record writes
        itself: a change only a reused inode or a new account could show."""
        head, plist, _, body = record.read_text().split("\n", 3)
        body = edit(body)
        record.write_text(f"{head}\n{plist}\nacted {hashlib.sha256(body.encode()).hexdigest()}\n{body}")
        record.chmod(0o600)

    def test_a_weaver_config_made_again_after_a_stop_is_not_removed(self):
        """Codex on #159, round 1 (P1): /etc/weaver removed, then put back
        or made again before the rerun, is not the object judged, so the
        rerun refuses and the new tree stands. Perturbation: drop the check
        before the roots, and the new configuration is removed."""
        box = self.box()
        self.assertEqual(self.run_purge(box, phases=1).returncode, 0)
        self.assertFalse((box / "etc/weaver").exists())
        (box / "etc/weaver/stack").mkdir(parents=True)
        (box / "etc/weaver/stack/prefix").write_text("/opt/weaver\n")
        ran = self.run_purge(box)
        self.assertEqual(ran.returncode, 1, ran.stdout + ran.stderr)
        self.assertIn(f"{box}/etc/weaver, which is not the object this purge judged", ran.stderr)
        self.assertTrue((box / "etc/weaver/stack/prefix").exists(), "the new configuration stands")

    def test_a_root_made_again_inside_the_judged_config_is_not_withdrawn(self):
        """Perturbation: drop the root's check, and the new root goes."""
        box = self.box()
        self.assertEqual(self.run_purge(box, phases=0).returncode, 0)
        root = box / "etc/weaver/admin/a"
        shutil.rmtree(root)
        root.mkdir()
        (root / "territory").write_text(f"{box}/var/lib/weaver-agent/weaver-a\n")
        ran = self.run_purge(box)
        self.assertEqual(ran.returncode, 1, ran.stdout + ran.stderr)
        self.assertIn(f"the root {root}, which is not one this purge judged", ran.stderr)
        self.assertTrue(root.is_dir())

    def test_a_reused_inode_under_another_owner_is_not_removed(self):
        """Codex on #159, round 1 (P2): device and inode are reused once a
        path is gone, so the identity carries the owner and the birth time
        too. The replacement here takes the judged entry's device and inode
        as the record holds them, written in as only a reused inode could
        show it, under another owner: the rerun refuses and it stands.
        Perturbation: compare device and inode alone, and it is removed."""
        box = self.box()
        self.assertEqual(self.run_purge(box, phases=1).returncode, 0)
        entry = box / "tmp/weaver-a-scratch"
        entry.rmdir()
        entry.mkdir()
        (entry / "theirs").write_text("")
        st = os.stat(entry)
        record = self.record(box)

        def collide(body):
            out = []
            for line in body.splitlines():
                if line.startswith("held ") and line.endswith(f" {entry}"):
                    birth = line.split(" ", 2)[1].split(":")[3]
                    line = f"held {st.st_dev}:{st.st_ino}:2001:{birth} {entry}"
                out.append(line)
            return "".join(l + "\n" for l in out)
        self.resum(record, collide)
        ran = self.run_purge(box)
        self.assertEqual(ran.returncode, 1, ran.stdout + ran.stderr)
        self.assertIn(f"{entry}, which is not the object this purge judged", ran.stderr)
        self.assertTrue((entry / "theirs").exists())

    def test_an_account_made_again_under_its_name_is_not_removed(self):
        """An account or a group removed and made again under the same name,
        with another number, is not the one judged. Perturbations: remove a
        user, or a group, by name alone, and the new one goes."""
        box = self.box()
        self.assertEqual(self.run_purge(box, phases=2).returncode, 0)
        passwd = box / "model/passwd"
        passwd.write_text(passwd.read_text().replace("weaver-a:x:2001:", "weaver-a:x:4242:"))
        ran = self.run_purge(box)
        self.assertEqual(ran.returncode, 1, ran.stdout + ran.stderr)
        self.assertIn("the user weaver-a, which is not the account this purge judged", ran.stderr)
        self.assertIn("weaver-a:x:4242:", passwd.read_text())
        # The same for a group made again: the trace group, which no user
        # takes with it, under another number.
        box = self.box()
        self.assertEqual(self.run_purge(box, phases=2).returncode, 0)
        group = box / "model/group"
        group.write_text(group.read_text().replace("weaver-a-trace:x:3003:", "weaver-a-trace:x:4343:"))
        passwd = box / "model/passwd"
        ran = self.run_purge(box)
        self.assertEqual(ran.returncode, 1, ran.stdout + ran.stderr)
        self.assertIn("the group weaver-a-trace, which is not the group this purge judged", ran.stderr)
        self.assertIn("weaver-a-trace:x:4343:", group.read_text())

    def test_a_removal_that_fails_keeps_the_record_and_the_rerun_finishes(self):
        """Perturbation: remove the record whatever failed, and the rerun
        finds no record on a box a purge acted on, and refuses."""
        box = self.box()
        ran = self.run_purge(box, failing="rm")
        self.assertEqual(ran.returncode, 1, ran.stdout + ran.stderr)
        self.assertIn("not removed", ran.stdout)
        self.assertIn("root's record is kept", ran.stderr)
        self.assertTrue(self.record(box).exists())
        again = self.run_purge(box)
        self.assertEqual(again.returncode, 0, again.stdout + again.stderr)
        self.assert_purged(box)

    def test_a_home_quota_that_fails_stops_the_purge_and_the_rerun_removes_it_first(self):
        """#585, the self-check's finding 4, and CodeRabbit on #582: a quota
        line of the list goes to root's record, and a removal that fails
        stops the purge there, the paths and the accounts standing and the
        record kept. The rerun finds the agent's own account at the project's
        number, removes the limit first, and finishes. Perturbations: act on
        the live discovery in place of the record, and the first run never
        asks setquota and closes clean; carry on past the failure, and the
        territory and the account go with the limit standing; judge the
        limit by repquota even where the agent's own account holds the
        number, and the rerun refuses its own limit."""
        box = self.box()
        with (box / "PURGE-LIST").open("a") as listed:
            listed.write("quota 2001 /state drive\n")
        stand_in = box / "model" / "setquota"
        stand_in.write_text('#!/bin/sh\necho "setquota $*" >> "$MODEL/calls"\n[ ! -e "$MODEL/setquota-fails" ]\n')
        stand_in.chmod(0o755)
        (box / "model" / "quotas.csv").write_text("#2001,--,--,8,0,1048576,0,2,0,65536,0\n")
        (box / "model" / "setquota-fails").touch()
        ran = self.run_purge(box)
        self.assertEqual(ran.returncode, 1, ran.stdout + ran.stderr)
        self.assertIn("home quota of project 2001 on /state drive: its limit was not removed", ran.stdout)
        self.assertIn("a home quota's limit above was not removed, so nothing more is removed", ran.stderr)
        self.assertIn("root's record is kept", ran.stderr)
        self.assertTrue(self.record(box).exists())
        self.assertTrue((box / "var/lib/weaver-agent/weaver-a").exists(), "the territory stands")
        self.assertIn("weaver-a:x:2001:", (box / "model/passwd").read_text(), "the account stands")
        (box / "model" / "setquota-fails").unlink()
        again = self.run_purge(box)
        self.assertEqual(again.returncode, 0, again.stdout + again.stderr)
        self.assertIn("set the home quota of project 2001 on /state drive to none", again.stdout)
        self.assert_purged(box)
        calls = [c for c in (box / "model" / "calls").read_text().splitlines() if c.startswith("setquota ")]
        self.assertEqual(calls, ["setquota -P 2001 0 0 0 0 /state drive"] * 2)

    def test_a_rerun_never_replays_a_limit_another_may_hold(self):
        """**CodeRabbit on #582, the Planner's ruling of 2026-11-17**: a first
        run removes a home quota's limit and the agent's account, then stops
        with root's record kept. Before the rerun replays the removal, a
        project number that now names an account this purge did not judge (a
        newcomer given the reused uid), a project repquota shows in use with
        no account holding the number (a limit set by hand), or quotas that
        cannot be read refuse the rerun before anything goes, and the limit
        stands. With the number free and the project shown with no limit, its
        usage the agent's own leftovers (the folder's mark, a home a failed
        removal left), the rerun replays the removal, which changes nothing,
        and finishes. Perturbations: take any account at the number for the
        agent's, drop the repquota test, answer an unreadable repquota as
        free, or drop the quota case, and the newcomer's or the hand-set
        limit is set to none; count usage as a limit, and the rerun refuses
        the agent's own leftovers."""
        header = "Project,BlockStatus,FileStatus,BlockUsed,BlockSoft,BlockHard,BlockGrace,FileUsed,FileSoft,FileHard,FileGrace\n"
        for case in ("newcomer", "by hand", "unreadable", "used, unlimited"):
            with self.subTest(case=case):
                box = self.box()
                with (box / "PURGE-LIST").open("a") as listed:
                    listed.write("quota 2001 /state drive\n")
                stand_in = box / "model" / "setquota"
                stand_in.write_text('#!/bin/sh\necho "setquota $*" >> "$MODEL/calls"\n')
                stand_in.chmod(0o755)
                # The first run removes the limit and every account, and a
                # path that will not go keeps the record.
                ran = self.run_purge(box, failing="rm")
                self.assertEqual(ran.returncode, 1, ran.stdout + ran.stderr)
                self.assertIn("set the home quota of project 2001 on /state drive to none", ran.stdout)
                self.assertIn("root's record is kept", ran.stderr)
                passwd = box / "model" / "passwd"
                self.assertNotIn("weaver-a:", passwd.read_text())
                quotas = box / "model" / "quotas.csv"
                expect = None
                if case == "newcomer":
                    passwd.write_text(passwd.read_text() + "weaver-b:x:2001:2001::/nonexistent:/bin/false\n")
                    quotas.write_text(header + "#2001,--,--,8,0,1048576,0,2,0,65536,0\n")
                    expect = ("the limit of project 2001 on /state drive, whose number now names the account "
                              "weaver-b, which this purge did not judge")
                elif case == "by hand":
                    quotas.write_text(header + "#2001,--,--,0,0,1048576,0,0,0,65536,0\n")
                    expect = ("the limit of project 2001 on /state drive, which repquota shows limited "
                              "(#2001,--,--,0,0,1048576,0,0,0,65536,0) while no account this purge judged holds "
                              "the number, so it may be another's")
                elif case == "unreadable":
                    (box / "model" / "repquota-fails").touch()
                    expect = "the limit of project 2001 on /state drive, whose quotas cannot be read (repquota)"
                else:
                    quotas.write_text(header + "#2001,--,--,8,0,0,0,2,0,0,0\n")
                again = self.run_purge(box)
                calls = [c for c in (box / "model" / "calls").read_text().splitlines() if c.startswith("setquota ")]
                if expect:
                    self.assertEqual(again.returncode, 1, again.stdout + again.stderr)
                    self.assertIn(expect, again.stderr)
                    self.assertEqual(calls, ["setquota -P 2001 0 0 0 0 /state drive"], "the rerun sets no limit")
                    self.assertTrue(self.record(box).exists())
                else:
                    self.assertEqual(again.returncode, 0, again.stdout + again.stderr)
                    self.assertEqual(calls, ["setquota -P 2001 0 0 0 0 /state drive"] * 2)
                    self.assert_purged(box)

    def test_an_empty_prefix_that_will_not_go_keeps_the_record_and_refuses(self):
        """Codex on #159, round 2: a failure on the left of `&&` is exempt
        from `set -e`, so an rmdir that failed passed for done. Perturbation:
        the prefix's removal as it stood (`rmdir ... && plan`), and the purge
        reports success, removing its record, with the prefix standing."""
        box = self.box()
        ran = self.run_purge(box, failing="rmdir")
        self.assertEqual(ran.returncode, 1, ran.stdout + ran.stderr)
        self.assertIn(f"empty {box}/opt/weaver: not removed", ran.stdout)
        self.assertIn("root's record is kept", ran.stderr)
        self.assertTrue((box / "opt/weaver").is_dir())
        self.assertTrue(self.record(box).exists())
        again = self.run_purge(box)
        self.assertEqual(again.returncode, 0, again.stdout + again.stderr)
        self.assert_purged(box)

    def test_a_record_that_will_not_go_refuses_completion(self):
        """Codex on #159, round 2. Perturbation: the record's removal as it
        stood (`purge_record remove ... && plan`), and the purge says it is
        done with the record standing."""
        box = self.box()
        ran = self.run_purge(box, record_remove_fails=True)
        self.assertEqual(ran.returncode, 1, ran.stdout + ran.stderr)
        self.assertIn("cannot be removed and stands, so the purge is not done", ran.stderr)
        self.assertIn("sudo rm ", ran.stderr)
        self.assertNotIn("PURGED", ran.stdout)
        self.assertTrue(self.record(box).exists())

    def test_a_sudo_rule_that_will_not_go_refuses(self):
        """The same class at the door: a rule `rm -f` cannot remove leaves the
        door open. Perturbation: `rm -f ... && plan`, and the purge goes on."""
        box = self.box()
        ran = self.run_purge(box, failing="rm-f")
        self.assertEqual(ran.returncode, 1, ran.stdout + ran.stderr)
        self.assertIn("the delegated door is not shut", ran.stderr)
        self.assertFalse(self.record(box).exists(), "nothing went on to the record")
        self.assertTrue((box / "var/lib/weaver-agent/weaver-a").exists())

    def test_a_sudo_rule_the_purge_did_not_judge_is_never_removed(self):
        """Codex on #159, round 3: a rerun removed every weaver-* rule it
        found. A rule made for a new agent, or one put back, since the first
        run refuses the rerun, and both stand; a first run refuses a rule its
        PURGE-LIST does not name. Perturbations: remove what discovery finds
        on a rerun, or on a first run whatever is unlisted, and the rules go."""
        box = self.box()
        self.assertEqual(self.run_purge(box, phases=1).returncode, 0)
        rules = box / "etc/sudoers.d"
        new, back = rules / "weaver-b", rules / ".weaver-a.decommissioning"
        new.write_text("new\n")
        back.write_text("put back\n")
        ran = self.run_purge(box)
        self.assertEqual(ran.returncode, 1, ran.stdout + ran.stderr)
        self.assertIn("nothing is purged", ran.stderr)
        self.assertEqual((new.read_text(), back.read_text()), ("new\n", "put back\n"))
        # A new agent's rule alone, the judged one gone: the door refuses it.
        back.unlink()
        ran = self.run_purge(box)
        self.assertEqual(ran.returncode, 1, ran.stdout + ran.stderr)
        self.assertIn(f"the sudo rule {new} stands, and every rule this purge judged went before its record", ran.stderr)
        self.assertTrue(new.exists())
        # A first run, with a rule made after the archive.
        box = self.box()
        unlisted = box / "etc/sudoers.d/weaver-b"
        unlisted.write_text("new\n")
        ran = self.run_purge(box)
        self.assertEqual(ran.returncode, 1, ran.stdout + ran.stderr)
        self.assertIn(f"the sudo rule {unlisted} is not on this archive's PURGE-LIST", ran.stderr)
        self.assertTrue(unlisted.exists())
        self.assertTrue((box / "etc/sudoers.d/.weaver-a.decommissioning").exists(), "no rule went")

    def test_what_a_rerun_finds_new_inside_a_judged_directory_refuses(self):
        """Second reader on #159, round 3: a judged directory's identity does
        not cover what is put in it, so a new agent's territory made under a
        judged base, or a new entry in /etc/weaver, would go unarchived. The
        rerun refuses on a direct entry it did not judge, and it stands.
        Perturbation: skip the entries' check, and they are removed."""
        box = self.box()
        self.assertEqual(self.run_purge(box, phases=1).returncode, 0)
        newcomer = box / "var/lib/weaver-agent/weaver-b"
        newcomer.mkdir()
        (newcomer / "trace.ndjson").write_text("")
        ran = self.run_purge(box)
        self.assertEqual(ran.returncode, 1, ran.stdout + ran.stderr)
        self.assertIn(f"{newcomer} in {box}/var/lib/weaver-agent, which is not what this purge judged there", ran.stderr)
        self.assertTrue((newcomer / "trace.ndjson").exists())
        # The same inside a /etc/weaver still standing.
        box = self.box()
        self.assertEqual(self.run_purge(box, phases=0).returncode, 0)
        base = box / "etc/weaver/admin-b"
        base.mkdir()
        ran = self.run_purge(box)
        self.assertEqual(ran.returncode, 1, ran.stdout + ran.stderr)
        self.assertIn(f"{base} in {box}/etc/weaver", ran.stderr)
        self.assertTrue(base.is_dir())

    def test_a_rerun_stops_only_the_units_it_recorded(self):
        """A legacy unit (a box from before #50) found on a rerun and not
        recorded refuses; the units recorded are stopped. Perturbation: stop
        what discovery finds, and the new unit is stopped."""
        box = self.box()
        self.assertEqual(self.run_purge(box, phases=0, units=["weaver-worker@a.service"]).returncode, 0)
        ran = self.run_purge(box, units=["weaver-worker@a.service", "weaver-worker@b.service"])
        self.assertEqual(ran.returncode, 1, ran.stdout + ran.stderr)
        self.assertIn("the unit weaver-worker@b.service, which this purge did not judge", ran.stderr)
        calls = (box / "model/calls").read_text() if (box / "model/calls").exists() else ""
        self.assertNotIn("weaver-worker@b", calls)
        ran = self.run_purge(box, units=["weaver-worker@a.service"])
        self.assertEqual(ran.returncode, 0, ran.stdout + ran.stderr)
        self.assertIn("systemctl stop weaver-worker@a.service", (box / "model/calls").read_text())

    def test_userdel_takes_no_home_and_a_prefix_made_again_is_kept(self):
        """Second reader on #159, round 3: `userdel -r` removed a home and a
        mail spool no one judged, so the accounts go without it; and a prefix
        made again since it was judged is kept, not rmdir'd by name.
        Perturbations: `userdel -r`, or rmdir the prefix by name alone."""
        box = self.box()
        self.assertEqual(self.run_purge(box, phases=3).returncode, 0)
        self.assertNotIn("userdel -r", (box / "model/calls").read_text())
        prefix = box / "opt/weaver"
        prefix.rmdir()
        prefix.mkdir()
        ran = self.run_purge(box)
        self.assertEqual(ran.returncode, 0, ran.stdout + ran.stderr)
        self.assertIn(f"kept {prefix}: not the directory this purge judged", ran.stdout)
        self.assertTrue(prefix.is_dir())

    def test_a_root_that_cannot_be_withdrawn_refuses_before_any_path_goes(self):
        """Perturbation: go on past a root not withdrawn, and its territory
        goes while it stands, which its admin then refuses."""
        box = self.box()
        ran = self.run_purge(box, failing="mv")
        self.assertEqual(ran.returncode, 1, ran.stdout + ran.stderr)
        self.assertIn("cannot be withdrawn", ran.stderr)
        self.assertTrue((box / "var/lib/weaver-agent/weaver-a").exists(), "the territory stands with its root")
        self.assertEqual(self.roots_without_territory(box), [])

    def test_a_purge_stopped_after_each_phase_reruns_to_completion(self):
        """Stopped after each phase, and after each single removal within
        one. Perturbations: put the paths before the config roots, and a stop
        after a territory goes leaves a root whose territory is gone, which
        its admin refuses; read no record on the rerun, and it refuses the box
        a purge has acted on."""
        # A whole purge of the model removes with six `rm`s: the door's rule,
        # /etc/weaver once its roots are withdrawn, then the four other paths.
        stops = [{"phases": n} for n in range(4)] + [{"removals": n} for n in range(1, 7)]
        for stop in stops:
            with self.subTest(**stop):
                box = self.box()
                first = self.run_purge(box, **stop)
                self.assertEqual(first.returncode, 0, first.stdout + first.stderr)
                # The first removal is the door's rule, made before the record:
                # a stop there leaves a box whose next purge is a first run.
                self.assertEqual(self.record(box).exists(), stop != {"removals": 1},
                                 "the record stands across every stop after it is written")
                self.assertEqual(self.roots_without_territory(box), [], "no root is left for admin to refuse")
                again = self.run_purge(box)
                self.assertEqual(again.returncode, 0, again.stdout + again.stderr)
                if stop != {"removals": 1}:
                    self.assertIn("a rerun: acting on root's record", again.stdout)
                self.assert_purged(box)

    def test_a_record_changed_since_it_was_written_refuses_the_rerun(self):
        """Perturbation: drop the record's own sum check, and the changed
        record is acted on."""
        box = self.box()
        self.assertEqual(self.run_purge(box, phases=1).returncode, 0)
        record = self.record(box)
        record.write_text(record.read_text() + f"path {box}/opt\n")
        record.chmod(0o600)
        ran = self.run_purge(box)
        self.assertEqual(ran.returncode, 1, ran.stdout + ran.stderr)
        self.assertIn("does not hold what it summed", ran.stderr)
        self.assertNotIn("PURGED", ran.stdout)
        self.assertTrue((box / "opt/weaver/lib").exists(), "nothing more was removed")

    def test_a_record_of_another_list_or_naming_what_the_list_does_not_refuses(self):
        box = self.box()
        self.assertEqual(self.run_purge(box, phases=1).returncode, 0)
        record = self.record(box)
        lines = record.read_text().split("\n", 3)
        body = lines[3] + f"path {box}/opt\n"
        record.write_text("\n".join(lines[:2]) + "\nacted " + hashlib.sha256(body.encode()).hexdigest() + "\n" + body)
        record.chmod(0o600)
        ran = self.run_purge(box)
        self.assertEqual(ran.returncode, 1, ran.stdout + ran.stderr)
        self.assertIn("names what this archive's PURGE-LIST does not", ran.stderr)
        self.assertTrue((box / "opt/weaver/lib").exists())

    def test_a_missing_record_on_a_box_a_purge_acted_on_refuses(self):
        """Perturbation: skip `purge_began`, and the rerun derives again from
        the half-removed box."""
        for phases in (1, 3):
            with self.subTest(stopped_after_phases=phases):
                box = self.box()
                self.assertEqual(self.run_purge(box, phases=phases).returncode, 0)
                self.record(box).unlink()
                ran = self.run_purge(box)
                self.assertEqual(ran.returncode, 1, ran.stdout + ran.stderr)
                self.assertIn("root holds no record of a purge in progress", ran.stderr)


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
        # The stand-in reads its answer from a file, since the drop hands the
        # account the named pairs alone (B3).
        answer_file = self.dir / "answer"
        sudo = self.dir / "sudo"
        sudo.write_text(f'#!/bin/sh\nIFS= read -r a < {shlex.quote(str(answer_file))}\nprintf "%s\\n" "$a"\n')
        sudo.chmod(0o755)
        program = account_drop_source() + shell_function(script, "probe_connector") + 'probe_connector "$1"'
        # **The drop's PATH is this directory alone** (B3), so the account's
        # `sudo -n` is the stand-in, never one found on the box's PATH.
        env = {**os.environ, "PATH": f"{self.dir}{os.pathsep}{os.environ['PATH']}",
               "ADMINCON_USER": "weaver-admincon", "ADMIN_BINARY": "/opt/weaver/bin/weaver-admin",
               "NAME": "m1", "ROOT_OPERATOR": "0", "DROP_PATH": str(self.dir)}
        env.pop("BASH_ENV", None)
        for verb, answer, passes in (("validate", '{"kind":"validated"}', True),
                                     ("show", '{"kind":"state","state":"unloaded"}', True),
                                     ("validate", "sudo: a password is required", False),
                                     ("validate", '{"kind":"boundary_unverified"}', False),
                                     ("show", '{"kind":"validated"}', False)):
            with self.subTest(verb=verb, answer=answer):
                answer_file.write_text(answer + "\n")
                run = subprocess.run(["bash", "-c", program, "x", verb], env=env,
                                     text=True, capture_output=True, timeout=20)
                self.assertEqual(run.returncode == 0, passes, run.stdout + run.stderr)
                self.assertEqual(run.stdout, answer)
        self.assertIn("if answer=$(probe_connector", script)
        self.assertNotIn('"$check_verb" "$NAME" 2>&1) || true', script)
        # **In root mode the drop is runuser's** (#576): root becomes the
        # admin-con account through runuser, and that account's own `sudo -n`
        # is still the one the rule must satisfy. Perturbation: drop through
        # sudo in root mode, and the runuser log is empty.
        drops = self.dir / "drops"
        runuser = self.dir / "runuser"
        # It starts under `env -i` (#590), so it names its log and env by path.
        runuser.write_text(f'#!/bin/sh\nprintf "%s\\n" "$*" >> {shlex.quote(str(drops))}\nshift 4\n'
                           f'exec {shutil.which("env", path=BOX_PATH)} "$@"\n')
        runuser.chmod(0o755)
        answer_file.write_text('{"kind":"validated"}\n')
        run = subprocess.run(["bash", "-c", program, "x", "validate"],
                             env={**env, "ROOT_OPERATOR": "1"},
                             text=True, capture_output=True, timeout=20)
        self.assertEqual(run.returncode, 0, run.stdout + run.stderr)
        self.assertEqual(drops.read_text(), f"-u weaver-admincon -- env -i PATH={self.dir} "
                                            "sudo -n /opt/weaver/bin/weaver-admin validate m1\n")

    def test_the_purge_shuts_the_delegated_door_and_asks_again(self):
        # The purge removes the sudo rules and queries every agent afresh before
        # its first destructive step, never trusting the discovery-time answer.
        # Perturbations: drop the second query, or move it after the units
        # step, and this fails.
        script = (DEPLOY / "decommission.sh").read_text()
        purge = script[script.index(" 3. purge\n"):]
        door = purge.index('for f in "${SUDO_RULES[@]}"; do\n  [ "$RERUN" = 0 ]')
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
        first_write = archive.index('as_op sh -c \'umask 077 && mkdir -p "$1"\' _ "$DEST"')
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

    def test_the_purge_takes_each_agents_folder_and_never_a_base(self):
        # **#578, the Planner's ruling of 2026-11-16**: a base is the state
        # drive's mount point on an appliance or `/home` on a host, so the
        # purge takes each discovered agent's folder from it, `<base>/<agent>`
        # or the older `<base>/weaver-<agent>`, root's by uid, and never the
        # base. What else the base holds is named as left in place.
        # A root-held `web/` or `weaver-web/` that only shares the name of a
        # `weaver-web` account holds no territory, no home and no declaration
        # of that account's, so each is left. Perturbations: drop the uid
        # test, and alice's own home is taken; drop either holding test, and
        # web/ or weaver-web/ is taken;
        # list the bases in purge_candidates again, and the base is a purge
        # line; drop left_in_base's taken filter, and a folder reads as left.
        script = (DEPLOY / "decommission.sh").read_text()
        with tempfile.TemporaryDirectory() as scratch:
            base = Path(scratch).resolve() / "state"
            for name in ("m1", "weaver-old", "lost+found", "alice", "stray", "web", "weaver-web"):
                (base / name).mkdir(parents=True)
            (base / "weaver-old" / "agent.toml").touch()
            (base / "m1" / "weaver-m1").mkdir()
            (base / "m1" / "home").mkdir()
            roots_owned = ":".join(str(base / n) for n in ("m1", "weaver-old", "lost+found", "web", "weaver-web"))
            # The tester owns every file here, so `stat -c %u` answers 0 for
            # what the fixture calls root's and STRANGER_UID elsewhere, never
            # the runner's (Codex on #599).
            stat_double = ('stat() { local p="${@: -1}"; case ":$ROOTS_OWNED:" in *":$p:"*) echo 0 ;; '
                           '*) case "$2" in %u) echo ' + str(STRANGER_UID) + ' ;; *) command stat "$@" ;; esac ;; '
                           'esac; }\n')
            program = (stat_double + "declare -A AGENTS=([m1]=1 [old]=1 [alice]=1 [web]=1)\n"
                       + shell_function(script, "agent_folders") + shell_function(script, "resolved")
                       + shell_function(script, "left_in_base")
                       + 'mapfile -t F < <(agent_folders "$1")\nprintf "folder %s\\n" "${F[@]}"\n'
                       + 'left_in_base "$1" "${F[@]}" | sort | sed "s/^/left /"')
            run = subprocess.run(["bash", "-c", program, "x", str(base)], env={**os.environ, "ROOTS_OWNED": roots_owned},
                                 text=True, capture_output=True, timeout=20)
            self.assertEqual(run.returncode, 0, run.stderr)
            self.assertEqual(run.stdout.splitlines(), [
                f"folder {base / 'm1'}", f"folder {base / 'weaver-old'}",
                f"left {base / 'alice'}", f"left {base / 'lost+found'}", f"left {base / 'stray'}",
                f"left {base / 'weaver-web'}", f"left {base / 'web'}"])
            # The purge's candidates name the folders and never the base.
            candidates = subprocess.run(
                ["bash", "-c", "SUDO_RULES=(); LDSO_CONFS=(); OWN_TERRITORIES=(); HOMES=(); TMP_PATHS=()\n"
                 "WEAVER_USERS=(); WEAVER_GROUPS=(); declare -A PREFIXES=() LOG_PATHS=() COORD_ROOTS=() AGENTS=()\n"
                 f"declare -A TERRITORY_BASES=([{shlex.quote(str(base))}]=1)\nAGENT_FOLDERS=({shlex.quote(str(base / 'm1'))})\n"
                 "LEGACY_DIR=/nonexistent/weaver\n"
                 + shell_function(script, "spanned")
                 + shell_function(script, "spans_a_base") + shell_function(script, "candidate")
                 + shell_function(script, "purge_candidates") + "purge_candidates"],
                text=True, capture_output=True, timeout=20)
            self.assertEqual(candidates.returncode, 0, candidates.stderr)
            self.assertIn(f"path {base / 'm1'}", candidates.stdout.splitlines())
            self.assertNotIn(f"path {base}", candidates.stdout.splitlines())
        # A home in an agent's folder goes with the folder.
        with tempfile.TemporaryDirectory() as scratch:
            folder = Path(scratch).resolve() / "state" / "m1"
            (folder / "home").mkdir(parents=True)
            elsewhere = Path(scratch).resolve() / "srv" / "weaver-x"
            elsewhere.mkdir(parents=True)
            self.assertEqual(self.run_fn_in(script, "home_within", str(folder / "home"), str(folder)), str(folder))
            self.assertEqual(self.run_fn_in(script, "home_within", str(elsewhere), str(folder)), "")
        self.assertIn('*) plan "$h  (home of $u): outside /home and everything the purge takes, left in place, untouched" ;;', script)
        self.assertIn('done < <(left_in_base "$d" "${TERRITORY_PATHS[@]}" "${HOMES[@]}")', script)

    def test_no_base_and_no_path_above_one_is_archived_or_purged_whole(self):
        # **CodeRabbit on #582, the Planner's ruling of 2026-11-17**: a base,
        # or a path above a base or an agent's own folder, is never archived
        # as a path or listed for the purge, whatever fixed name it carries:
        # `/var/lib/weaver` where the stack record names it the
        # agent-directory, a log directory that is the base, or one above a
        # root's own territory. The archive and the purge's derivation both
        # drop it, so a list written before the drop refuses at the purge,
        # and the purge takes the agent's folder and leaves the stranger's
        # file in the base. Perturbations: drop the guard from the
        # derivation's candidate, or list the legacy directory past it, and
        # the stranger's file goes; drop it from archive_path, and the base
        # is archived and listed; compare equal paths alone, or leave out the
        # agents' own folders, and a path above one is a candidate; list a
        # run directory past it in the archive's writer, and the sweep below
        # fails. The operator's home, and a path above it, is never a
        # candidate either, and a legacy directory inside a base, which it
        # does not span, is taken and named as taken, never as left: drop the
        # home's check, or keep a taken directory out of the plan's taken
        # paths, and a read below fails.
        script = (DEPLOY / "decommission.sh").read_text()
        lines = script.splitlines()
        start = lines.index("  archive_path() { # archive_path NAME PATH...")
        archive_path = "\n".join(lines[start:next(i for i in range(start, len(lines)) if lines[i] == "  }") + 1])
        paths_phase = script[script.index('say "paths"\n'):script.index('say "accounts"\n')]
        q = shlex.quote
        with tempfile.TemporaryDirectory() as scratch:
            top = Path(scratch).resolve()
            base = top / "var-lib-weaver"
            (base / "m1" / "weaver-m1").mkdir(parents=True)
            (base / "stranger.txt").write_text("not the agent's\n")
            (top / "legacy" / "agents").mkdir(parents=True)
            (top / "elsewhere" / "weaver-x").mkdir(parents=True)
            (top / "old").mkdir()
            (top / "dest").mkdir()
            (top / "users" / "op").mkdir(parents=True)
            (top / "varlib" / "weaver").mkdir(parents=True)
            (top / "varlib" / "m2" / "weaver-m2").mkdir(parents=True)

            def run(legacy, bases, folders=(), own=(), logs=(), tail="purge_candidates\n", op_home=""):
                program = ("set -uo pipefail\n"
                           "SUDO_RULES=(); LDSO_CONFS=(); HOMES=(); TMP_PATHS=(); WEAVER_USERS=(); WEAVER_GROUPS=()\n"
                           "HOME_QUOTAS=(); PURGE=(); PURGE_FAILED=0\n"
                           "declare -A PREFIXES=() COORD_ROOTS=() AGENTS=() LOG_PATHS=() TERRITORY_BASES=()\n"
                           f"LEGACY_DIR={q(str(legacy))}\nDEST={q(str(top / 'dest'))}\nOP_HOME={q(str(op_home))}\n"
                           "TERRITORY_PATHS=()\n"
                           + "".join(f"TERRITORY_BASES[{q(str(b))}]=1\n" for b in bases)
                           + "".join(f"LOG_PATHS[{q(str(d))}]=1\n" for d in logs)
                           + "AGENT_FOLDERS=(" + " ".join(q(str(f)) for f in folders) + ")\n"
                           + "OWN_TERRITORIES=(" + " ".join(q(str(o)) for o in own) + ")\n"
                           + "say() { :; }\nplan() { printf 'PLAN %s\\n' \"$*\"; }\n"
                           + "die() { printf 'DIE %s\\n' \"$*\"; exit 1; }\nheld_is() { return 0; }\n"
                           + "free_name() { printf '%s' \"$1\"; }\ntarz() { : > \"$1\"; }\nas_op() { \"$@\"; }\n"
                           + shell_function(script, "spanned")
                           + shell_function(script, "spans_a_base") + shell_function(script, "candidate")
                           + shell_function(script, "purge_candidates") + shell_function(script, "purge_lines_outside")
                           + shell_function(script, "purge_lines_within") + shell_function(script, "resolved")
                           + shell_function(script, "left_in_base")
                           + archive_path + "\n" + tail)
                ran = subprocess.run(["bash", "-c", program], text=True, capture_output=True, timeout=20)
                self.assertEqual(ran.returncode, 0, ran.stdout + ran.stderr)
                return ran.stdout.splitlines()

            # A list written before the drop names the base and the agent's
            # folder. The purge's judgment names the base as what it would not
            # find, so that purge refuses, and the paths phase, acting only on
            # the lines the derivation holds, takes the folder and leaves the
            # stranger's file and the base. A list the archive writes now
            # names the folder alone, and nothing of it is outside.
            old_list, new_list = top / "old-list", top / "new-list"
            old_list.write_text(f"path {base}\npath {base / 'm1'}\n")
            new_list.write_text(f"path {base / 'm1'}\n")
            got = run(base, [base], folders=[base / "m1"],
                      tail=(f"purge_lines_outside {q(str(old_list))} | sed 's/^/OUTSIDE /'\n"
                            f"[ -z \"$(purge_lines_outside {q(str(new_list))})\" ] || echo 'OUTSIDE new'\n"
                            f"PURGE_ACTED=$(mktemp)\npurge_lines_within {q(str(old_list))} > \"$PURGE_ACTED\"\n"
                            + paths_phase + 'rm -f -- "$PURGE_ACTED"\n'))
            self.assertEqual((base / "stranger.txt").read_text(), "not the agent's\n")
            self.assertIn(f"PLAN removed {base / 'm1'}", got)
            self.assertFalse((base / "m1").exists())
            self.assertEqual([g for g in got if g.startswith("OUTSIDE ")], [f"OUTSIDE path {base}"])
            (base / "m1" / "weaver-m1").mkdir(parents=True)
            # The record names the legacy directory as its base, and a log
            # directory is the base: the agent's folder is a candidate, the
            # base is not.
            got = run(base, [base], folders=[base / "m1"], logs=[base])
            self.assertIn(f"path {base / 'm1'}", got)
            self.assertNotIn(f"path {base}", got)
            # A legacy directory above a base, and a log directory above a
            # root's own territory, are no candidates; the territory is.
            got = run(top / "legacy", [top / "legacy" / "agents"], own=[top / "elsewhere" / "weaver-x"],
                      logs=[top / "elsewhere"])
            self.assertNotIn(f"path {top / 'legacy'}", got)
            self.assertNotIn(f"path {top / 'elsewhere'}", got)
            self.assertIn(f"path {top / 'elsewhere' / 'weaver-x'}", got)
            # Control: a legacy directory that spans no base is taken whole.
            self.assertIn(f"path {top / 'old'}", run(top / "old", [base], folders=[base / "m1"]))
            # A log directory that is the operator's home, or above it, is no
            # candidate, whatever base it holds or does not.
            got = run(top / "old", [base], folders=[base / "m1"], logs=[top / "users", top / "users" / "op"],
                      op_home=top / "users" / "op")
            self.assertNotIn(f"path {top / 'users'}", got)
            self.assertNotIn(f"path {top / 'users' / 'op'}", got)
            # A legacy directory inside a base, which it does not span, is
            # taken, and the plan names it as taken, never as left in the base.
            plan_loop = script[script.index('for d in "$LEGACY_DIR" "${!LOG_PATHS[@]}"; do'):]
            plan_loop = plan_loop[:plan_loop.index("\ndone\n") + len("\ndone\n")]
            varlib = top / "varlib"
            got = run(varlib / "weaver", [varlib], folders=[varlib / "m2"],
                      tail=('TERRITORY_PATHS=("${AGENT_FOLDERS[@]}")\n' + plan_loop
                            + 'left_in_base "$(realpath -e -- "${!TERRITORY_BASES[@]}")" "${TERRITORY_PATHS[@]}" '
                              '| sed "s/^/LEFT /"\n'))
            self.assertTrue(any(g.startswith(f"PLAN {varlib / 'weaver'}  ") for g in got), got)
            self.assertNotIn(f"LEFT {varlib / 'weaver'}", got)
            self.assertNotIn(f"LEFT {varlib / 'm2'}", got)
            self.assertIn(f"path {varlib / 'weaver'}", run(varlib / "weaver", [varlib], folders=[varlib / "m2"]))
            # The archive lists the agent's folder and says the base is left.
            got = run(base, [base], folders=[base / "m1"], logs=[base],
                      tail=('archive_path var-lib-weaver "$LEGACY_DIR"\n'
                            'for d in "${!LOG_PATHS[@]}"; do archive_path log "$d"; done\n'
                            'archive_path agent "${AGENT_FOLDERS[0]}"\nprintf "LISTED %s\\n" "${PURGE[@]}"\n'))
            self.assertEqual([g for g in got if g.startswith("LISTED ")], [f"LISTED {base / 'm1'}"])
            self.assertIn(f"PLAN {base}: a base, or above a base or an agent's folder, so it is not archived whole "
                          "or listed for the purge", got)
        # Every purge line the derivation and the archive's list write runs
        # through the one guard, beside the paths archive_path kept.
        self.assertNotIn('echo "path', shell_function(script, "purge_candidates"))
        writer = script[script.index("# What --purge may touch, and nothing else."):
                        script.index('} | to_file "$DEST/PURGE-LIST"')]
        self.assertEqual([w.strip() for w in writer.splitlines() if '"path ' in w],
                         ['for pth in "${PURGE[@]}"; do echo "path $pth"; done'])

    def test_each_capped_homes_limit_goes_before_its_folder(self):
        # **#585**: discovery takes each home's project from its account's uid
        # in passwd and the mount from the root-owned folder, only where the
        # drive is mounted with project quotas and the folder carries that
        # project as create-agent.sh's mark. A folder marked 0 (an agent made
        # without a cap) or marked with another number carries no cap of the
        # script's, so a stranger's limit of the agent's uid is never lifted,
        # and a folder whose mark cannot be read is named. A folder whose
        # account is gone, a folder with no home, a linked home, and a drive without
        # project quotas carry no cap, and a folder whose mount cannot be read
        # is named and keeps the purge from closing clean. **What the home carries is a report and
        # never acted on**: the agent owns its home, and a home made before
        # #587 or moved by hand can carry another agent's uid, so a home moved
        # to m1's project has only its own account's limit removed, and the
        # plan names the move. The purge sets each project's limit to none
        # before the paths go, and a limit left standing (setquota failing, or
        # absent) stops the purge there (CodeRabbit on #582). Perturbations:
        # read the project off the home, follow a linked home, skip the
        # prjquota check, drop the failure's return, or move the step after
        # the paths, and a read below fails.
        script = (DEPLOY / "decommission.sh").read_text()
        with tempfile.TemporaryDirectory() as scratch:
            base = Path(scratch).resolve() / "state"
            names = ("m1", "m2", "m3", "m4", "m5", "m6", "m7", "m8", "m9", "m10")
            for name in names:
                (base / name).mkdir(parents=True)
            for name in ("m1", "m2", "m5", "m6", "m7", "m8", "m9", "m10"):
                (base / name / "home").mkdir()
            (base / "m4" / "home").symlink_to(base / "m1" / "home")
            # The stand-ins read through a link, the worst a reader could do,
            # so a linked home is refused by the script's own check. m2's
            # account is gone, m5 moved its home to m1's project, m6 is on a
            # drive without project quotas, m7's mount cannot be read, m8 was
            # made uncapped (its folder marked 0), m9's mark cannot be read, and
            # m10's folder carries m1's number, not its own.
            stubs = ('getent() { case "$2" in weaver-m1) echo "weaver-m1:x:12345:12345::/x:/sbin/nologin" ;; '
                     'weaver-m4) echo "weaver-m4:x:34567:34567::/x:/sbin/nologin" ;; '
                     'weaver-m5) echo "weaver-m5:x:23456:23456::/x:/sbin/nologin" ;; '
                     'weaver-m6) echo "weaver-m6:x:45678:45678::/x:/sbin/nologin" ;; '
                     'weaver-m7) echo "weaver-m7:x:56789:56789::/x:/sbin/nologin" ;; '
                     'weaver-m8) echo "weaver-m8:x:67890:67890::/x:/sbin/nologin" ;; '
                     'weaver-m9) echo "weaver-m9:x:78901:78901::/x:/sbin/nologin" ;; '
                     'weaver-m10) echo "weaver-m10:x:89012:89012::/x:/sbin/nologin" ;; *) return 2 ;; esac; }\n'
                     # A home carries the inherit flag and a folder's mark
                     # does not, as create-agent.sh leaves them (#582, #600).
                     'lsattr() { case "$(realpath -- "${@: -1}")" in */m1/home|*/m5/home) echo "    12345 --------------P--- ${@: -1}" ;; '
                     '*/m1|*/m10) echo "    12345 ---------------- ${@: -1}" ;; '
                     '*/m5) echo "    23456 ---------------- ${@: -1}" ;; */m9) return 1 ;; '
                     # m4 and m6 carry their own marks, so only the linked home
                     # and the drive's missing project quotas pass them over.
                     '*/m4) echo "    34567 ---------------- ${@: -1}" ;; */m6) echo "    45678 ---------------- ${@: -1}" ;; '
                     '*) echo "    0 ---------------- ${@: -1}" ;; esac; }\n'
                     'findmnt() { case "${@: -1}" in */m7) return 1 ;; esac; '
                     'case "$3" in OPTIONS) case "${@: -1}" in */m6) echo "rw,relatime" ;; *) echo "rw,relatime,prjquota" ;; esac ;; '
                     'TARGET) echo "/state drive" ;; esac; }\n')
            run = subprocess.run(["bash", "-c", "set -euo pipefail\n" + stubs + shell_function(script, "home_projects")
                                  + 'home_projects "$@"', "x", *(str(base / n) for n in names)],
                                 text=True, capture_output=True, timeout=20)
            self.assertEqual(run.returncode, 0, run.stderr)
            listed = run.stdout.splitlines()
            self.assertEqual(listed, ["12345 12345 /state drive", "23456 12345 /state drive", f"? mount {base}/m7",
                                      f"? mark {base}/m9"])
            planned = subprocess.run(["bash", "-c", 'plan() { printf "%s\\n" "$*"; }\n' + shell_function(script, "plan_home_quotas")
                                      + 'plan_home_quotas "$@"; printf "quota %s\\n" "${HOME_QUOTAS[@]}"; printf "unread %s\\n" "${HOME_QUOTA_UNREAD[@]}"',
                                      "x", *listed],
                                     text=True, capture_output=True, timeout=20)
            self.assertEqual(planned.returncode, 0, planned.stderr)
            self.assertEqual(planned.stdout.splitlines(), [
                "home quota of project 12345, the account's uid, on /state drive: its limit is set to none at the purge, before the folder",
                "home quota of project 23456, the account's uid, on /state drive: its limit is set to none at the purge, before the folder",
                "   the home carries project 12345, not 23456: 12345's limit is not touched",
                f"home quota of {base}/m7: its drive's mount cannot be read (findmnt), so whether a cap stands there is unknown, and the archive and the purge refuse until it reads",
                f"home quota of {base}/m9: the folder's mark cannot be read (lsattr), so whether a cap stands there is unknown, and the archive and the purge refuse until it reads",
                "quota 12345 /state drive", "quota 23456 /state drive",
                f"unread the mount of {base}/m7 (findmnt)", f"unread the mark of {base}/m9 (lsattr)"])
            log = Path(scratch) / "setquota.log"
            remove = ('plan() { printf "%s\\n" "$*"; }\n'
                      f'setquota() {{ printf "%s\\n" "$*" >> {shlex.quote(str(log))}; [ "$2" != 999 ]; }}\n'
                      + shell_function(script, "remove_home_quotas") + 'remove_home_quotas "$@"')
            run = subprocess.run(["bash", "-c", remove, "x", "12345 /state drive", "999 /other"],
                                 text=True, capture_output=True, timeout=20)
            self.assertEqual(run.returncode, 1, run.stderr)
            self.assertEqual(log.read_text().splitlines(), ["-P 12345 0 0 0 0 /state drive", "-P 999 0 0 0 0 /other"])
            self.assertEqual(run.stdout.splitlines(), ["set the home quota of project 12345 on /state drive to none",
                                                       "home quota of project 999 on /other: its limit was not removed"])
            run = subprocess.run(["bash", "-c", remove, "x", "12345 /state drive"], text=True, capture_output=True, timeout=20)
            self.assertEqual(run.returncode, 0, run.stderr)
            if shutil.which("setquota") is None:
                absent = ('plan() { printf "%s\\n" "$*"; }\n' + shell_function(script, "remove_home_quotas")
                          + 'remove_home_quotas "$@"')
                run = subprocess.run(["bash", "-c", absent, "x", "12345 /state"], text=True, capture_output=True, timeout=20)
                self.assertEqual(run.returncode, 1, run.stderr)
                self.assertEqual(run.stdout.splitlines(),
                                 ["home quota of project 12345 on /state: setquota is not on PATH, so its limit stands"])
        # Discovered in every mode, so the plan shows each cap, and removed in
        # the purge before any path, from root's record. **An unreadable mount
        # or mark refuses the archive and the purge** (Codex on #588) after the
        # plan, which only shows it, and before either mode acts, so neither
        # an archive whose list lacks the limit nor a purge that takes the
        # folder is ever made over it. Perturbation: move the refusal into
        # one mode alone, and a read below fails.
        self.assertLess(script.index('plan_home_quotas "${HOME_PROJECTS[@]}"'), script.index('[ "$MODE" = plan ] && {'))
        refusal = script.index('if [ ${#HOME_QUOTA_UNREAD[@]} -gt 0 ]; then')
        self.assertLess(script.index('[ "$MODE" = plan ] && {'), refusal)
        self.assertLess(refusal, script.index('if [ "$MODE" = archive ]; then'))
        self.assertEqual(script.count("HOME_QUOTA_UNREAD[@]"), 2, "counted and named by the refusal alone")
        step = script[script.index('say "home quotas"'):script.index('say "paths"')]
        self.assertIn('  case $line in "quota "*) PURGE_QUOTAS+=("${line#quota }") ;; esac\n'
                      'done < "$PURGE_ACTED"\n'
                      'remove_home_quotas "${PURGE_QUOTAS[@]}" \\\n'
                      '  || die "a home quota\'s limit above was not removed, so nothing more is removed', step)

    def test_a_home_quota_is_a_purge_list_line(self):
        # **#585, the self-check's findings 1 and 4**: each capped home's limit
        # is a "quota <project> <mount point>" line of the archive's PURGE-LIST
        # and of the derivation, so a purge removes only a limit the archive
        # kept and the box still shows, and root's record carries it for a
        # rerun. An agent made with a cap after the archive keeps its cap, and
        # a listed limit the derivation no longer holds is passed over, never
        # named as outside the purge. Perturbations: drop the derivation's
        # line, drop the archive's line, or drop the pass-over, and a read
        # below fails.
        script = (DEPLOY / "decommission.sh").read_text()
        fns = "".join(shell_function(script, n) for n in ("spanned", "spans_a_base", "candidate", "purge_candidates",
                                                          "purge_lines_outside", "purge_lines_within"))
        with tempfile.TemporaryDirectory() as scratch:
            listed = Path(scratch) / "PURGE-LIST"
            listed.write_text("quota 12345 /state drive\nquota 999 /gone\n")
            derive = 'HOME_QUOTAS=("12345 /state drive" "23456 /state drive")\n'
            run = subprocess.run(["bash", "-c", derive + fns + 'purge_candidates | grep "^quota "', "x"],
                                 text=True, capture_output=True, timeout=20)
            self.assertEqual(run.stdout.splitlines(), ["quota 12345 /state drive", "quota 23456 /state drive"], run.stderr)
            run = subprocess.run(["bash", "-c", derive + fns + 'purge_lines_outside "$1"', "x", str(listed)],
                                 text=True, capture_output=True, timeout=20)
            self.assertEqual(run.returncode, 0, run.stderr)
            self.assertEqual(run.stdout, "")
            run = subprocess.run(["bash", "-c", derive + fns + 'purge_lines_within "$1"', "x", str(listed)],
                                 text=True, capture_output=True, timeout=20)
            self.assertEqual(run.stdout.splitlines(), ["quota 12345 /state drive"], run.stderr)
        block = script[script.index("  # What --purge may touch, and nothing else."):script.index('} | to_file "$DEST/PURGE-LIST"')]
        self.assertIn('for q in "${HOME_QUOTAS[@]}"; do echo "quota $q"; done', block)

    def run_fn_in(self, script, name, *args):
        run = subprocess.run(["bash", "-c", shell_function(script, name) + f'{name} "$@"', "x", *args],
                             text=True, capture_output=True, timeout=20)
        self.assertEqual(run.returncode, 0, run.stderr)
        return run.stdout

    def test_decommission_archives_and_purges_every_territory_base(self):
        # Codex on #79: the per-agent layout's territories under the stack
        # record's agent-directory are archived, so they reach the purge list,
        # each agent's folder on its own since #578. Perturbation: drop the
        # loop, and they are left behind.
        script = (DEPLOY / "decommission.sh").read_text()
        archive = script[script.index('if [ "$MODE" = archive ]; then'):script.index(" 3. purge\n")]
        self.assertIn('for d in "${AGENT_FOLDERS[@]}"; do archive_path "$(archive_name agent "$d")" "$d"; done', archive)
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
        for sink in ("opt", "agent", "territory"):
            self.assertIn(f'archive_path "$(archive_name {sink} ', script)
        # Every legacy log goes into one archive of a fixed name (#593).
        self.assertIn('archive_path legacy-logs "${!LOG_PATHS[@]}"', script)
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
        # `free_name` looks as the operator (#98), through `op_has` (#577);
        # here the test is the operator.
        die = next(l for l in script.splitlines() if l.startswith("die()"))
        program = (line + "\n" + die + "\nas_op() { \"$@\"; }\n" + shell_function(script, "op_has")
                   + shell_function(script, "free_name")
                   + 'for p in "$@"; do n=$(free_name "$(archive_name territories "$p")"); touch "$DEST/$n.tar.zst"; echo "$n"; done')
        run = subprocess.run(["bash", "-c", program, "x", "/var/lib/weaver-agent", "/var/lib/weaver/agent",
                              "/var/lib/weaver-agent"], env={**os.environ, "DEST": str(dest)},
                             text=True, capture_output=True, timeout=20)
        self.assertEqual(run.stdout.split(), ["territories-var-lib-weaver-agent",
                                              "territories-var-lib-weaver-agent-2",
                                              "territories-var-lib-weaver-agent-3"], run.stderr)
        self.assertIn('name=$(free_name "$1") || exit 1; shift', script)

    def test_op_has_looks_through_the_builtin_and_refuses_a_refused_drop(self):
        # **#577**: the operator's looks at DEST go through the shell's
        # builtin `[`, so a group-blind external `test` (here a `test` first
        # on PATH that answers no to everything, reached as sudo reaches it,
        # by exec) changes no answer. A drop sudo refuses is neither yes nor
        # no: it refuses naming sudo, so `free_name` never takes an archive
        # for absent. Perturbations: make `op_has` call `as_op test "$1"
        # "$2"`, and the existing archive reads absent; let a refused drop
        # return 1, and `free_name` prints a name.
        script = (DEPLOY / "decommission.sh").read_text()
        die = next(l for l in script.splitlines() if l.startswith("die()"))
        blind = self.dir / "blind"
        blind.mkdir()
        (blind / "test").write_text("#!/bin/sh\nexit 1\n")
        (blind / "test").chmod(0o755)
        dest = self.dir / "dest"
        dest.mkdir()
        (dest / "SHA256SUMS").write_text("")
        env = {**os.environ, "PATH": f"{blind}{os.pathsep}{os.environ['PATH']}", "DEST": str(dest),
               "OPERATOR": "fixture-operator"}
        env.pop("BASH_ENV", None)
        functions = die + "\n" + shell_function(script, "op_has") + shell_function(script, "free_name")
        def run(as_op, tail):
            return subprocess.run(["bash", "-c", as_op + "\n" + functions + tail], env=env,
                                  text=True, capture_output=True, timeout=20)
        passes = 'as_op() { env "$@"; }'
        answers = run(passes, 'for f in "-f $DEST/SHA256SUMS" "-f $DEST/PURGE-LIST" "-w $DEST" "-e $DEST/none"; do'
                              ' if op_has $f; then echo yes; else echo no; fi; done')
        self.assertEqual(answers.stdout.split(), ["yes", "no", "yes", "no"], answers.stderr)
        refused = 'as_op() { echo "sudo: a password is required" >&2; return 1; }'
        for tail in ('op_has -e "$DEST/SHA256SUMS"; echo "answered $?"',
                     'name=$(free_name territories) || exit 1; echo "named $name"'):
            with self.subTest(tail=tail):
                run_ = run(refused, tail)
                self.assertEqual(run_.returncode, 1, run_.stdout)
                self.assertIn("sudo would not run a look as fixture-operator", run_.stderr)
                self.assertNotIn("answered", run_.stdout)
                self.assertNotIn("named", run_.stdout)
        self.assertIn('name=$(free_name "$1") || exit 1; shift', script)

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
        self.assertLess(take, script.index('if [ ${#CHANGED[@]} -gt 0 ] || [ ${#LIB_CHANGED[@]} -gt 0 ]'))
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
        # A scope wrapper (weaver-admin-Spec 6.1), stood in so any call is
        # seen: its options dropped through `--`, and the verb it carried
        # recorded.
        self.scoped = root / "scoped"
        scope = self.bin_dir / "systemd-run"
        scope.write_text('#!/bin/sh\nwhile [ "$1" != "--" ]; do shift; done\nshift\n'
                         f'echo "$2" >> {self.scoped}\nexec "$@"\n')
        scope.chmod(0o755)
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
        # **No verb runs in a scope of its own, a load included**
        # (weaver-admin-Spec 6.1): admin's start step asks PID1 for the
        # agent's.
        self.assertFalse(self.scoped.exists(), "a verb was wrapped in a scope")

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


def docs_mirror_faults(root):
    """Where docs/crates/ departs from the shape of crates/ (the layout ruling
    of 2026-10-13): one folder per crate at the top level, holding that
    crate's PRD and Spec, and no crate's documents under another's folder.
    The exceptions are contracts/, the apex weaver-agent-PRD.md,
    weaver-analysis/, the duplicated copies of a crate that lives in its own
    repository, and state-management/, the documents of a domain that spans
    several crates and has no crate of its own (the operator's ruling of
    2026-10-19)."""
    crates = sorted(p.name for p in (root / "crates").iterdir()
                    if (p / "Cargo.toml").is_file())
    docs = root / "docs" / "crates"
    faults = []
    for name in crates:
        for kind in ("PRD", "Spec"):
            if not (docs / name / f"{name}-{kind}.md").is_file():
                faults.append(f"docs/crates/{name}/{name}-{kind}.md is missing")
    for folder in sorted(p for p in docs.iterdir() if p.is_dir()):
        if folder.name not in crates + ["contracts", "weaver-analysis", "state-management"]:
            faults.append(f"docs/crates/{folder.name}/ names no crate")
    for document in sorted(docs.rglob("weaver-*.md")):
        owner = re.fullmatch(r"(weaver-.+)-(?:PRD|Spec)\.md", document.name)
        if owner and document.parent != docs / owner.group(1) \
                and document.relative_to(docs).as_posix() != "weaver-agent-PRD.md":
            faults.append(f"{document.relative_to(root).as_posix()} sits outside "
                          f"docs/crates/{owner.group(1)}/")
    return faults


class LineBoundMirrorTests(unittest.TestCase):
    """turn.py measures a line against the gate's bound before it dials, so
    its copy of the bound is the gate's own `LINE_BOUND`: a copy that runs
    high sends lines the gate closes unanswered, and one that runs low
    refuses lines the gate carries. The gate's suite derives the number from
    the envelope; this pins turn.py to the gate.
    Perturbation: change either number alone and this fails."""

    def test_turn_py_carries_the_gates_line_bound(self):
        relay = (DEPLOY.parent / "crates" / "weaver-gate" / "src" / "relay.rs").read_text()
        gate = re.findall(r"^pub const LINE_BOUND: usize = ([0-9_]+);$", relay, re.MULTILINE)
        turn = re.findall(r"^LINE_BOUND = ([0-9_]+)$", (DEPLOY / "turn.py").read_text(), re.MULTILINE)
        self.assertEqual(len(gate), 1, "relay.rs declares LINE_BOUND once, as a literal")
        self.assertEqual(len(turn), 1, "turn.py declares LINE_BOUND once, as a literal")
        self.assertEqual(int(turn[0].replace("_", "")), int(gate[0].replace("_", "")))


class DocsMirrorTests(unittest.TestCase):
    """docs/crates/ has the shape of crates/, so a document maps to the code
    it governs (the layout ruling of 2026-10-13).
    Perturbation: nest a crate's folder under another's, as
    weaver-harness/weaver-trace/ once was, and both its missing top-level
    documents and the nested ones are named. A folder naming no crate is
    named, and the written exceptions are not."""

    def test_every_crate_has_its_documents_at_the_top_level(self):
        """The repository's real document tree satisfies the mirror rule."""
        self.assertEqual(docs_mirror_faults(DEPLOY.parent), [])

    def test_a_nested_crate_folder_is_named(self):
        """Nesting a crate exposes both its missing home and misplaced documents."""
        with tempfile.TemporaryDirectory() as scratch:
            root = Path(scratch)
            for name in ("weaver-harness", "weaver-trace"):
                (root / "crates" / name).mkdir(parents=True)
                (root / "crates" / name / "Cargo.toml").write_text("")
            harness = root / "docs" / "crates" / "weaver-harness"
            (harness / "weaver-trace").mkdir(parents=True)
            (root / "docs" / "crates" / "contracts").mkdir()
            (root / "docs" / "crates" / "weaver-agent-PRD.md").write_text("")
            for kind in ("PRD", "Spec"):
                (harness / f"weaver-harness-{kind}.md").write_text("")
                (harness / "weaver-trace" / f"weaver-trace-{kind}.md").write_text("")
            self.assertEqual(docs_mirror_faults(root), [
                "docs/crates/weaver-trace/weaver-trace-PRD.md is missing",
                "docs/crates/weaver-trace/weaver-trace-Spec.md is missing",
                "docs/crates/weaver-harness/weaver-trace/weaver-trace-PRD.md "
                "sits outside docs/crates/weaver-trace/",
                "docs/crates/weaver-harness/weaver-trace/weaver-trace-Spec.md "
                "sits outside docs/crates/weaver-trace/",
            ])

    def test_a_folder_naming_no_crate_is_named_and_the_exceptions_are_not(self):
        """Named exceptions stay allowed while an unrelated folder is rejected."""
        with tempfile.TemporaryDirectory() as scratch:
            root = Path(scratch)
            (root / "crates" / "weaver-trace").mkdir(parents=True)
            (root / "crates" / "weaver-trace" / "Cargo.toml").write_text("")
            docs = root / "docs" / "crates"
            (docs / "weaver-trace").mkdir(parents=True)
            for kind in ("PRD", "Spec"):
                (docs / "weaver-trace" / f"weaver-trace-{kind}.md").write_text("")
            for folder in ("contracts", "weaver-analysis", "state-management", "notes"):
                (docs / folder).mkdir()
            self.assertEqual(docs_mirror_faults(root),
                             ["docs/crates/notes/ names no crate"])


# What cargo prints, in the shapes the stand-in below replays: a crate that
# does not compile (diagnostics on stderr, no `test result` line, 101), and
# the summary lines of a run, one per test binary.
CARGO_COMPILE_FAILURE = (
    "   Compiling weaver-harness v0.1.0\n"
    "error[E0425]: cannot find value `undeclared` in this scope\n"
    "  --> crates/weaver-harness/src/lifecycle.rs:10:5\n"
    "error: could not compile `weaver-harness` (lib test) due to 1 previous error\n"
)


class EngineLibrariesTests(unittest.TestCase):
    """**Both stack scripts select, place and check the engine's libraries,
    and judge the loader entry's directories, through
    deploy/engine-libraries.sh**, run here as the scripts source it.
    `engine_out` prints the out_dir of the one build-script-executed message
    for the package `cargo pkgid` names, inside the build, and nothing
    otherwise; `engine_resolved` passes only a report naming the engine and
    resolving it inside the library directory with nothing missing; `spu_ldd`
    runs ldd with the library directory as LD_LIBRARY_PATH and none of the
    operator's loader variables; `loader_dir` refuses a directory that is not
    one absolute path held closed by root. Perturbations: take the first
    message rather than the single one and the two-directory case prints;
    match any package's message and the decoy's directory counts; drop the
    containment and the directory outside the build prints; compare
    unresolved and the link out of the build prints; let `engine_resolved`
    pass a report with no engine and the empty report passes; keep the
    operator's LD_PRELOAD and it reaches ldd; drop the newline test and the
    two-line value passes; take a resolved path as one whitespace field, in
    `engine_resolved` or in the tuple, and a library directory holding a
    space is judged unresolved or truncated."""

    ENGINE = "fixture-engine-source#llama-cpp-sys-2@0.1.0"

    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory(prefix="weaver-engine-libraries-")
        self.addCleanup(self.scratch.cleanup)
        self.dir = Path(self.scratch.name)
        self.bin = self.dir / "bin"
        self.bin.mkdir()
        cargo = self.bin / "cargo"
        cargo.write_text('#!/bin/sh\n[ "$1" = pkgid ] || exit 99\nprintf \'%s\' "$PKGID"\nexit "${PKGID_EXIT:-0}"\n')
        cargo.chmod(0o755)
        self.messages = self.dir / "messages.json"
        self.build = self.dir / "target" / "release" / "build"

    def run_bash(self, program, **env):
        env = {**os.environ, "PATH": f"{self.bin}:{os.environ['PATH']}", **env}
        env.pop("BASH_ENV", None)
        source = f"source {shlex.quote(str(DEPLOY / 'engine-libraries.sh'))}\n"
        return subprocess.run(["bash", "-c", "set -euo pipefail\n" + source + program],
                              env=env, text=True, capture_output=True, timeout=20)

    def engine_out(self, pkgid, *messages, status=0):
        self.messages.write_text("".join(m if isinstance(m, str) else json.dumps(m) + "\n" for m in messages))
        return self.run_bash(f"engine_out llama-cpp-sys-2 {shlex.quote(str(self.messages))} "
                             f"{shlex.quote(str(self.build))}", PKGID=pkgid, PKGID_EXIT=str(status))

    def test_engine_out_prints_the_one_directory_cargo_reported_for_the_engine(self):
        a = self.build / "llama-cpp-sys-2-a" / "out"
        b = self.build / "llama-cpp-sys-2-b" / "out"
        elsewhere = self.dir / "elsewhere" / "out"
        for d in (a, b, self.build / "decoy" / "out", elsewhere):
            d.mkdir(parents=True)
        (self.build / "leads-out").symlink_to(elsewhere)
        mine = {"reason": "build-script-executed", "package_id": self.ENGINE, "out_dir": str(a)}
        decoy = {"reason": "build-script-executed", "package_id": "1-source#decoy@0.1.0",
                 "out_dir": str(self.build / "decoy" / "out")}
        artifact = {"reason": "compiler-artifact", "package_id": self.ENGINE, "out_dir": str(elsewhere)}
        ran = self.engine_out(self.ENGINE, decoy, "not json\n", artifact, mine, mine)
        self.assertEqual((ran.returncode, ran.stdout), (0, f"{a}\n"), ran.stderr)
        for label, pkgid, status, messages in (
                ("two directories for the engine", self.ENGINE, 0, (mine, dict(mine, out_dir=str(b)))),
                ("no message for the engine", self.ENGINE, 0, (decoy, artifact)),
                ("an empty directory", self.ENGINE, 0, (dict(mine, out_dir=""),)),
                ("a directory outside the build", self.ENGINE, 0, (dict(mine, out_dir=str(elsewhere)),)),
                ("a link out of the build", self.ENGINE, 0, (dict(mine, out_dir=str(self.build / "leads-out")),)),
                ("the build directory itself", self.ENGINE, 0, (dict(mine, out_dir=str(self.build)),)),
                ("a directory that is not there", self.ENGINE, 0, (dict(mine, out_dir=str(self.build / "gone" / "out")),)),
                ("a name beside the build", self.ENGINE, 0, (dict(mine, out_dir=str(self.build) + "-x/out"),)),
                ("cargo refuses an ambiguous or absent name", "", 101, (mine,)),
                ("cargo names nothing", "", 0, (mine,))):
            with self.subTest(label):
                ran = self.engine_out(pkgid, *messages, status=status)
                self.assertEqual((ran.returncode, ran.stdout), (1, ""), ran.stderr)

    def test_engine_resolved_passes_only_the_engine_from_the_library_directory(self):
        lib = self.dir / "lib"
        lib.mkdir()
        (lib / "libllama.so.0.0.1").write_text("x")
        (lib / "libllama.so.0").symlink_to("libllama.so.0.0.1")
        other = self.dir / "other"
        other.mkdir()
        (other / "libllama.so.0").write_text("x")
        good = (f"\tlinux-vdso.so.1 (0x0000)\n\tlibllama.so.0 => {lib}/libllama.so.0 (0x0000)\n"
                f"\tlibc.so.6 => /usr/lib/libc.so.6 (0x0000)\n")
        check = 'engine_resolved "$REPORT" "$LIB"'
        ran = self.run_bash(check, REPORT=good, LIB=str(lib))
        self.assertEqual((ran.returncode, ran.stdout), (0, ""), ran.stderr)
        for label, report, said in (
                ("a library not found", good + "\tlibcudart.so.13 => not found\n", "libcudart.so.13 => not found"),
                ("the engine from elsewhere", good.replace(str(lib), str(other)), f"{other}/libllama.so.0"),
                ("no engine named", "\tlibc.so.6 => /usr/lib/libc.so.6 (0x0000)\n", "no engine library is named"),
                ("an empty report", "", "no engine library is named")):
            with self.subTest(label):
                ran = self.run_bash(check, REPORT=report, LIB=str(lib))
                self.assertEqual(ran.returncode, 1, ran.stdout)
                self.assertIn(said, ran.stdout)

    def test_a_resolved_path_holding_a_space_is_read_whole(self):
        """**A path ldd resolves is read whole, spaces included**, by
        `engine_resolved` and by the install tuple. A library directory may
        hold a space (`loader_dir` and admin refuse only what is not one
        absolute path), and ldd prints `<name> => <path> (<load address>)`.
        The CUDA directory here also holds the words `not found`, which name
        a library not found only after the arrow at a line's end; a library
        not found reads `unknown` in the tuple, never a path. Perturbations:
        take the path as one whitespace field in `engine_resolved` and the
        correct install is judged unresolved; take it as `$3` in the tuple
        and cudart reads a truncated path and cublas reads one."""
        lib = self.dir / "weaver lib"
        cuda = self.dir / "cuda not found here"
        for d in (lib, cuda):
            d.mkdir()
        (lib / "libllama.so.0.0.1").write_text("x")
        (lib / "libllama.so.0").symlink_to("libllama.so.0.0.1")
        (cuda / "libcudart.so.13.0.48").write_text("x")
        (cuda / "libcudart.so.13").symlink_to("libcudart.so.13.0.48")
        report = (f"\tlinux-vdso.so.1 (0x00007ffd00000000)\n"
                  f"\tlibllama.so.0 => {lib}/libllama.so.0 (0x00007f0000000000)\n"
                  f"\tlibcudart.so.13 => {cuda}/libcudart.so.13 (0x00007f0100000000)\n"
                  f"\tlibc.so.6 => /usr/lib/libc.so.6 (0x00007f0200000000)\n"
                  f"\t/lib64/ld-linux-x86-64.so.2 (0x00007f0300000000)\n")
        check = 'engine_resolved "$REPORT" "$LIB"'
        ran = self.run_bash(check, REPORT=report, LIB=str(lib))
        self.assertEqual((ran.returncode, ran.stdout), (0, ""), ran.stderr)
        elsewhere = report.replace(f"{lib}/", f"{cuda}/")
        ran = self.run_bash(check, REPORT=elsewhere, LIB=str(lib))
        self.assertEqual(ran.returncode, 1, "the engine from another spaced directory is still refused")
        self.assertIn(f"{cuda}/libllama.so.0", ran.stdout)
        ldd = self.bin / "ldd"
        ldd.write_text('#!/bin/sh\nprintf "%s" "$LDD_REPORT"\n')
        ldd.chmod(0o755)
        for tool in ("nvidia-smi", "pacman", "nvcc", "git"):
            (self.bin / tool).write_text("#!/bin/sh\nexit 1\n")
            (self.bin / tool).chmod(0o755)
        tuple_report = report + "\tlibcublas.so.13 => not found\n"
        ran = self.run_bash(f"source {shlex.quote(str(DEPLOY / 'install-tuple.sh'))}\n"
                            f"install_tuple {shlex.quote(str(self.dir))} fixture-commit "
                            f"{shlex.quote(str(self.dir / 'bin dir'))} {shlex.quote(str(lib))} fixture-features '' ''",
                            LDD_REPORT=tuple_report)
        self.assertEqual(ran.returncode, 0, ran.stderr)
        facts = dict(line.split(" = ", 1) for line in ran.stdout.splitlines())
        cudart = (cuda / "libcudart.so.13.0.48").resolve()
        self.assertEqual(facts["runtime.cudart"], f"{hashlib.sha256(b'x').hexdigest()} {cudart}")
        self.assertEqual(facts["runtime.cublas"], "unknown")
        self.assertEqual(facts["runtime.cublaslt"], "unknown")
        self.assertEqual(facts["runtime.curand"], "unknown")
        # The helper reads the spaced path back whole.
        tuple_file = self.dir / "install-tuple"
        tuple_file.write_text(ran.stdout)
        install = _agent_tuple().read_install_tuple(str(tuple_file))
        self.assertEqual(install["runtime"]["cudart"], (hashlib.sha256(b"x").hexdigest(), str(cudart)))

    def test_spu_ldd_runs_ldd_with_the_library_directory_and_no_operator_loader_variables(self):
        ldd = self.bin / "ldd"
        ldd.write_text('#!/bin/sh\nprintf "%s|%s|%s|%s\\n" "$LD_LIBRARY_PATH" "${LD_PRELOAD-unset}" '
                       '"${LD_AUDIT-unset}" "$1"\n')
        ldd.chmod(0o755)
        ran = self.run_bash('spu_ldd /x/weaver-spu /opt/fixture/lib', LD_LIBRARY_PATH="/operator/out/lib",
                            LD_PRELOAD="/operator/p.so", LD_AUDIT="/operator/a.so")
        # The loader warns from `env` itself, which starts under the
        # operator's variables before it clears them; ldd's own line is last.
        self.assertEqual(ran.stdout.splitlines()[-1], "/opt/fixture/lib|unset|unset|/x/weaver-spu", ran.stderr)

    def test_the_repository_cargo_config_forces_the_openssl_lookup_off(self):
        """**The build holds the rule** (weaver-spu-Spec section 1.1): the
        repository's `.cargo/config.toml` holds one setting, `[env]`
        CMAKE_DISABLE_FIND_PACKAGE_OpenSSL forced ON, so every cargo run
        started inside the tree builds the engine without HTTPS whatever the
        shell holds, and its comment names the Spec section and the OpenSSL
        and engine revisions that forced it. Cargo hands an `[env]` value to
        a dependency's build script, and `force` wins over the shell
        (measured on 2026-11-11 with a probe crate). Perturbations: drop
        `force`, set OFF, or drop the key, and this fails."""
        import tomllib
        path = DEPLOY.parent / ".cargo" / "config.toml"
        text = path.read_text()
        self.assertEqual(tomllib.loads(text),
                         {"env": {"CMAKE_DISABLE_FIND_PACKAGE_OpenSSL": {"value": "ON", "force": True}}})
        self.assertFalse(any(ord(c) > 127 for c in text), "ASCII")
        for named in ("weaver-spu-Spec section 1.1", "OpenSSL", "4.1.0", "cpp-httplib 0.40.0", "e21cdc11",
                      "bebe55d6a90069e13bc6c38eefc707a700014550"):
            self.assertIn(named, re.sub(r"\s*\n#\s*", " ", text))

    def test_sourcing_the_file_exports_the_openssl_lookup_off(self):
        """**The variable reaches every process either script starts**,
        exported ON over whatever the caller's environment held, since the
        fork's build script reads it from cargo's environment. Perturbation:
        drop `export` and the child reads the caller's value; default it with
        `${...:-ON}` and the caller's OFF stands."""
        for given in ("OFF", "", "ON"):
            with self.subTest(given=given):
                ran = self.run_bash("printenv CMAKE_DISABLE_FIND_PACKAGE_OpenSSL",
                                    CMAKE_DISABLE_FIND_PACKAGE_OpenSSL=given)
                self.assertEqual((ran.returncode, ran.stdout), (0, "ON\n"), ran.stderr)

    def test_engine_without_https_passes_only_a_cache_recording_the_lookup_off(self):
        """**`engine_without_https` passes only a configure cache that records
        the lookup off as cmake writes a definition**: the whole line, the
        name, a type and `=ON`. The first case is the line the live build of
        2026-11-11 left. A missing cache, a cache from before the rule, the
        lookup recorded OFF, and a line that only contains the definition
        refuse, naming the cache and the clean. Perturbations: drop `-x` and
        the commented and prefixed lines pass; drop the value and OFF
        passes."""
        helpers = "die() { printf 'REFUSED: %s\\n' \"$*\" >&2; exit 1; }\n"
        out = self.dir / "out"
        (out / "build").mkdir(parents=True)
        cache = out / "build" / "CMakeCache.txt"
        for text, passes in (
                ("CMAKE_BUILD_TYPE:STRING=Release\nCMAKE_DISABLE_FIND_PACKAGE_OpenSSL:UNINITIALIZED=ON\n", True),
                ("CMAKE_DISABLE_FIND_PACKAGE_OpenSSL:BOOL=ON\n", True),
                (None, False),
                ("LLAMA_OPENSSL:BOOL=ON\n", False),
                ("CMAKE_DISABLE_FIND_PACKAGE_OpenSSL:UNINITIALIZED=OFF\n", False),
                ("CMAKE_DISABLE_FIND_PACKAGE_OpenSSL:UNINITIALIZED=ONE\n", False),
                ("//CMAKE_DISABLE_FIND_PACKAGE_OpenSSL:UNINITIALIZED=ON\n", False),
                ("XCMAKE_DISABLE_FIND_PACKAGE_OpenSSL:UNINITIALIZED=ON\n", False),
                # cmake writes every cache entry with a type.
                ("CMAKE_DISABLE_FIND_PACKAGE_OpenSSL=ON\n", False)):
            with self.subTest(text=text):
                cache.unlink(missing_ok=True)
                if text is not None:
                    cache.write_text(text)
                ran = self.run_bash(helpers + f"engine_without_https {shlex.quote(str(out))}; echo passed")
                if passes:
                    self.assertEqual((ran.returncode, ran.stdout), (0, "passed\n"), ran.stderr)
                else:
                    self.assertEqual(ran.returncode, 1, ran.stdout)
                    self.assertNotIn("passed", ran.stdout)
                    self.assertIn(f"REFUSED: the engine's configure cache at {cache} does not turn its "
                                  "OpenSSL lookup off", ran.stderr)
                    self.assertIn("'cargo clean --release -p llama-cpp-sys-2 --locked'", ran.stderr)

    def test_loader_dir_refuses_what_root_does_not_hold_closed(self):
        update = (DEPLOY / "update-stack.sh").read_text()
        helpers = ("die() { printf 'REFUSED: %s\\n' \"$*\" >&2; exit 1; }\n"
                   + shell_function(update, "held_closed"))
        mine = self.dir / "mine"
        mine.mkdir()
        for value, said in (("relative", "is not one absolute path"),
                            ("/usr\n" + str(mine), "is not one absolute path"),
                            (str(self.dir / "absent"), "is not held closed by root"),
                            (str(mine), "is not held closed by root")):
            with self.subTest(value=value):
                ran = self.run_bash(helpers + f"loader_dir CUDA_LIB_DIR {shlex.quote(value)}; echo passed")
                self.assertEqual(ran.returncode, 1, ran.stdout)
                self.assertIn(f"REFUSED: CUDA_LIB_DIR {value} {said}", ran.stderr)
                self.assertNotIn("passed", ran.stdout)
        if os.stat("/usr").st_uid == 0 and not os.stat("/usr").st_mode & 0o022:
            ran = self.run_bash(helpers + "loader_dir CUDA_LIB_DIR /usr; echo passed")
            self.assertEqual(ran.stdout, "passed\n", ran.stderr)

    def test_both_scripts_select_place_check_and_judge_through_the_one_file(self):
        """Neither script keeps a copy of its own: each sources the file and
        assigns none of its constants, makes its build's messages file with
        mktemp under an exit trap, calls `engine_out` once and removes the
        file, judges the engine's configure cache through
        `engine_without_https` once, between naming the output directory and
        taking its lib/, and sets the OpenSSL variable nowhere itself, places
        every library through `install_library`, checks the SPU through
        `spu_ldd` and `engine_resolved` and runs no bare ldd, judges the CUDA
        directory before it writes the entry, and lists no `build/`
        directory. Bootstrap's build sends its messages to the file and its
        diagnostics to the kept log, and judges the CUDA directory before it
        builds."""
        call = 'SYS_OUT=$(engine_out "$ENGINE_CRATE" "$BUILD_MESSAGES" "$BUILT/build" || true)\n'
        for name in ("bootstrap-stack.sh", "update-stack.sh"):
            text = (DEPLOY / name).read_text()
            with self.subTest(script=name):
                self.assertIn('source "$REPO/deploy/engine-libraries.sh"', text)
                for defined in ("engine_out() {", "loader_dir() {", "install_library() {", "spu_ldd() {",
                                "engine_resolved() {", "engine_without_https() {"):
                    self.assertNotIn(defined, text)
                for constant in ("ENGINE_CRATE=", "LIB_GLOBS=", "LDSO_CONF=", "CUDA_LIB_DIR="):
                    self.assertNotRegex(text, rf"(?m)^\s*{constant}")
                self.assertEqual(text.count(call), 1)
                take = text.index(call)
                self.assertEqual(text.index('rm -f -- "$BUILD_MESSAGES"\ntrap - EXIT\n', take) - take, len(call))
                check = 'engine_without_https "$SYS_OUT"\n'
                self.assertEqual(text.count(check), 1)
                self.assertLess(take, text.index(check))
                self.assertLess(text.index(check), text.index('SYS_OUT="$SYS_OUT/lib"\n'))
                self.assertNotRegex(text, r"(?m)^[^#\n]*CMAKE_DISABLE_FIND_PACKAGE_OpenSSL")
                self.assertEqual(text.count('BUILD_MESSAGES=$(mktemp "${BUILT%/release}/weaver-deploy-messages-XXXXXX")'), 1)
                self.assertEqual(text.count("""trap 'rm -f -- "$BUILD_MESSAGES"' EXIT"""), 1)
                self.assertNotRegex(text, r"ls [^\n]*build/")
                self.assertNotRegex(text, r"(?m)^[^#\n]*\bldd \"")
                self.assertIn("install_library ", text)
                self.assertNotRegex(text, r'sudo cp -a[^\n]*\$SYS_OUT|sudo cp -a "\$\{LIBS')
                self.assertIn("spu_ldd ", text)
                self.assertIn("engine_resolved ", text)
                self.assertIn("--message-format=json-render-diagnostics", text)
                self.assertEqual(text.count('loader_dir CUDA_LIB_DIR "$CUDA_LIB_DIR"'), 1)
                self.assertLess(text.index('loader_dir CUDA_LIB_DIR "$CUDA_LIB_DIR"'),
                                text.index('| sudo tee "$LDSO_CONF"'))
        bootstrap = (DEPLOY / "bootstrap-stack.sh").read_text()
        build = shell_function(bootstrap, "run_build")
        self.assertIn('>"$BUILD_MESSAGES" 2>"$log"', build)
        self.assertIn('rm -f -- "$BUILD_MESSAGES"', build.split('if [ "$rc" -ne 0 ]; then', 1)[1])
        self.assertLess(bootstrap.index('loader_dir CUDA_LIB_DIR "$CUDA_LIB_DIR"'), bootstrap.index("\nrun_build\n"))


class CargoStepTests(unittest.TestCase):
    """The deploy scripts' cargo steps keep cargo's output and status (#128).
    Each step is lifted from the script as it ships and run against a
    stand-in `cargo` that writes what cargo writes and exits as it exits.
    Perturbation: put the old `cargo ... | grep -E '^test result' | awk`
    back in either script's test step and the compile failure prints
    "0 passed, 0 failed" with no diagnostic; drop the zero-total refusal and
    a run of no tests passes; drop the build's status and a failed build
    goes on."""

    STAND_IN = """#!/bin/sh
[ -z "$CARGO_SEEN" ] || printf 'NVCC_CCBIN=%s\\nNVCC_APPEND_FLAGS=%s\\nCMAKE_CUDA_ARCHITECTURES=%s\\n' \\
    "${NVCC_CCBIN-unset}" "${NVCC_APPEND_FLAGS-unset}" "${CMAKE_CUDA_ARCHITECTURES-unset}" > "$CARGO_SEEN"
printf '%s' "$CARGO_STDOUT"
printf '%s' "$CARGO_STDERR" >&2
exit "${CARGO_EXIT:-0}"
"""

    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory(prefix="weaver-cargo-step-")
        self.addCleanup(self.scratch.cleanup)
        self.bin = Path(self.scratch.name) / "bin"
        self.bin.mkdir()
        cargo = self.bin / "cargo"
        cargo.write_text(self.STAND_IN)
        cargo.chmod(0o755)
        self.tmp = Path(self.scratch.name) / "tmp"
        self.tmp.mkdir()
        self.seen = Path(self.scratch.name) / "seen"

    def step(self, script, name, stdout="", stderr="", status=0, extra_env=None):
        # Each step starts from an empty directory, so one subtest's kept
        # log never counts in another's.
        for left in self.tmp.iterdir():
            if left.is_file():
                left.unlink()
        text = (DEPLOY / script).read_text()
        body = "\n".join([
            "set -euo pipefail",
            "die() { printf '\\nREFUSED: %s\\n' \"$*\" >&2; exit 1; }",
            shell_function(text, name),
            name,
        ])
        env = dict(os.environ,
                   PATH=f"{self.bin}:{os.environ['PATH']}",
                   BUILT=str(self.tmp / "release"),
                   MEMBER_FEATURES="weaver-state/sqlite",
                   FEATURES="weaver-spu/cuda",
                   CARGO_STDOUT=stdout, CARGO_STDERR=stderr,
                   CARGO_EXIT=str(status), CARGO_SEEN=str(self.seen))
        for name_ in ("NVCC_CCBIN", "NVCC_APPEND_FLAGS", "CMAKE_CUDA_ARCHITECTURES"):
            env.pop(name_, None)
        env.update(extra_env or {})
        return subprocess.run(["bash", "-c", body], env=env,
                              capture_output=True, text=True, timeout=30)

    def seen_env(self):
        """What the stand-in cargo saw of the build's nvcc and CUDA settings."""
        return dict(line.split("=", 1) for line in self.seen.read_text().splitlines())

    def kept_logs(self):
        return sorted(log for log in self.tmp.iterdir() if log.is_file())

    def test_a_crate_that_does_not_compile_refuses_with_its_diagnostics(self):
        for script in ("update-stack.sh", "bootstrap-stack.sh"):
            with self.subTest(script=script):
                ran = self.step(script, "run_tests", stdout="   Compiling weaver-trace v0.1.0\n",
                                stderr=CARGO_COMPILE_FAILURE, status=101)
                self.assertNotEqual(ran.returncode, 0)
                self.assertIn("error[E0425]", ran.stderr)
                self.assertIn("cargo exited 101", ran.stderr)
                logs = [log for log in self.kept_logs() if log.name.startswith("weaver-deploy-tests-")]
                self.assertEqual(len(logs), 1, logs)
                self.assertIn(str(logs[0]), ran.stderr, "the refusal names the kept log")
                self.assertIn("could not compile", logs[0].read_text())
                logs[0].unlink()

    def test_a_failed_test_refuses_and_a_clean_run_passes_and_removes_its_log(self):
        failed = ("test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s\n"
                  "test result: FAILED. 2 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.20s\n")
        clean = ("test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s\n"
                 "test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.20s\n")
        for script in ("update-stack.sh", "bootstrap-stack.sh"):
            with self.subTest(script=script):
                ran = self.step(script, "run_tests", stdout=failed, status=101)
                self.assertNotEqual(ran.returncode, 0)
                self.assertIn("6 passed, 1 failed", ran.stdout)
                for log in self.kept_logs():
                    log.unlink()
                ran = self.step(script, "run_tests", stdout=clean)
                self.assertEqual(ran.returncode, 0, ran.stderr)
                self.assertIn("7 passed, 0 failed", ran.stdout)
                self.assertEqual(self.kept_logs(), [], "a clean run keeps no log")

    def test_a_garbled_result_line_neither_fails_the_step_nor_passes_it(self):
        """Child test processes share one stdout, so two of their summary
        lines can interleave (#507). The garble below is the one a real run
        of the step's command printed: read field by field, its second line
        gave a failed count of 0.29, which `printf %d` refused under errexit,
        ending the script on a suite cargo passed. A garbled line counts for
        nothing, the verdict is cargo's status, and the same garble beside a
        failed test still refuses. A summary counts only with its five counts
        present (through `filtered out`; libtest adds the duration only when
        it measured one), so output holding nothing but a summary cut short
        is a run of no tests, and refuses. The child's whole line, 6 filtered
        out, counts for nothing either (#611).
        Perturbation: restore the unanchored `awk '/^test result/ ...'` and
        the clean case exits 1 at the `%d` printf, or, with `%s`, prints a
        failed count of 0.29; anchor on the first two counts alone and the
        cut-short summary passes."""
        garbled = ("test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s\n"
                   "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 6 filtered out\n"
                   "test result: ; finished in 0.29sok\n"
                   "\n"
                   ". 1 passed; 0 failed; 0 ignored; 0 measured; 6 filtered out; finished in 0.29s\n")
        failed = garbled + ("test result: FAILED. 2 passed; 1 failed; 0 ignored; 0 measured; "
                            "0 filtered out; finished in 0.20s\n")
        for script in ("update-stack.sh", "bootstrap-stack.sh"):
            with self.subTest(script=script):
                ran = self.step(script, "run_tests", stdout=garbled)
                self.assertEqual(ran.returncode, 0, ran.stderr)
                self.assertIn("4 passed, 0 failed", ran.stdout)
                self.assertEqual(self.kept_logs(), [], "a clean run keeps no log")
                ran = self.step(script, "run_tests", stdout=failed, status=101)
                self.assertNotEqual(ran.returncode, 0)
                self.assertIn("6 passed, 1 failed", ran.stdout)
                self.assertIn("cargo exited 101", ran.stderr)
                for log in self.kept_logs():
                    log.unlink()
                ran = self.step(script, "run_tests", stdout="test result: ok. 1 passed; 0 failed\n")
                self.assertNotEqual(ran.returncode, 0, "a summary cut short is no count")
                self.assertIn("0 passed", ran.stderr)
                for log in self.kept_logs():
                    log.unlink()

    def test_the_count_is_the_suites_cargo_ran_never_a_childs_summary(self):
        """**Only a suite's own summary counts** (#611). Many tests re-run
        their own binary as a child filtered to one test, and the child's
        summary line, reporting the rest as filtered out, shares cargo's
        standard output. Counted, they moved the step's count between two
        runs of one commit, by how many survived whole. The step passes no
        filter, so every suite cargo runs reports 0 filtered out, and the
        count is the suites' sum: here 120, 0 and 6 passed, the last with one
        failed, beside child lines whole, interleaved with each other and
        garbled, a child's failure among them. Perturbation: count every
        complete summary line, and the children's whole lines count too."""
        first = "test result: ok. 120 passed; 0 failed; 3 ignored; 0 measured; 0 filtered out; finished in 1.20s\n"
        empty = "test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n"
        children = ("test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 122 filtered out; finished in 0.01s\n"
                    "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 6 filtered out\n"
                    "test result: ok. 1 passed; 0 faitest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; "
                    "9 filtered out; finished in 0.00s\n"
                    "led; 0 ignored; 0 measured; 9 filtered out; finished in 0.00s\n"
                    "test result: ; finished in 0.29sok\n"
                    ". 1 passed; 0 failed; 0 ignored; 0 measured; 6 filtered out; finished in 0.29s\n")
        clean = first + children + empty
        failed = clean + ("test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 41 filtered out; "
                          "finished in 0.02s\n"
                          "test result: FAILED. 6 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; "
                          "finished in 0.30s\n")
        for script in ("update-stack.sh", "bootstrap-stack.sh"):
            with self.subTest(script=script):
                ran = self.step(script, "run_tests", stdout=clean)
                self.assertEqual(ran.returncode, 0, ran.stderr)
                self.assertIn(" 120 passed, 0 failed\n", ran.stdout)
                self.assertEqual(self.kept_logs(), [], "a clean run keeps no log")
                ran = self.step(script, "run_tests", stdout=failed, status=101)
                self.assertNotEqual(ran.returncode, 0)
                self.assertIn(" 126 passed, 1 failed\n", ran.stdout)
                self.assertIn("cargo exited 101, 126 passed, 1 failed", ran.stderr)
                for log in self.kept_logs():
                    log.unlink()

    def test_the_cause_is_printed_where_the_tail_would_lose_it(self):
        captured = "".join(f"captured line {n}\n" for n in range(100))
        stdout = ("running 1 test\ntest lifecycle::tests::x ... FAILED\n\nfailures:\n\n"
                  "---- lifecycle::tests::x stdout ----\n\n"
                  "thread 'lifecycle::tests::x' (1) panicked at crates/weaver-harness/src/lifecycle/tests.rs:9:5:\n"
                  "the cause\n" + captured
                  + "test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s\n")
        for script in ("update-stack.sh", "bootstrap-stack.sh"):
            with self.subTest(script=script):
                ran = self.step(script, "run_tests", stdout=stdout, status=101)
                self.assertNotEqual(ran.returncode, 0)
                self.assertIn("panicked at", ran.stderr)
                self.assertIn("the cause", ran.stderr)
                self.assertIn("output: ", ran.stdout, "the log is named as the step begins")

    def test_a_refused_test_step_echoes_the_failures_and_not_the_run(self):
        """**A refused test step echoes the failures and the summary, not the
        run's last lines.** The harness's suites write queue-pressure JSON
        lines and every passing test's line to cargo's stdout, and on the
        live plan of 2026-11-09 the tail of 60 carried them, and the failures
        block a second time, before the refusal. The echo is each libtest
        `failures:` block through its `test result:` line, then cargo's own
        error lines; the refusal names the kept log. The output below is the
        live log's shape. Perturbation: put the tail back and the pressure
        line and the passing test print; drop the failures block's read and
        its test's name is missing."""
        noise = ('{"depth":1024,"mark":768,"organ":"harness","peak":1,"queue":"trace"}\n'
                 "test lifecycle::tests::y ... ok\n")
        stdout = ("running 3 tests\n" + noise * 40
                  + "\nfailures:\n\n---- lifecycle::tests::x stdout ----\n\n"
                  "thread 'lifecycle::tests::x' (1) panicked at crates/weaver-harness/src/lifecycle/tests.rs:9:5:\n"
                  "the cause\n\n\nfailures:\n    lifecycle::tests::x\n\n"
                  "test result: FAILED. 2 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; "
                  "finished in 0.10s\n\n")
        stderr = "error: test failed, to rerun pass `-p weaver-harness --lib`\n"
        for script in ("update-stack.sh", "bootstrap-stack.sh"):
            with self.subTest(script=script):
                ran = self.step(script, "run_tests", stdout=stdout, stderr=stderr, status=101)
                self.assertNotEqual(ran.returncode, 0)
                log, = self.kept_logs()
                echo = ran.stderr.split("\nREFUSED: ", 1)[0]
                self.assertEqual(echo, "failures:\n\n---- lifecycle::tests::x stdout ----\n\n"
                                 "thread 'lifecycle::tests::x' (1) panicked at "
                                 "crates/weaver-harness/src/lifecycle/tests.rs:9:5:\n"
                                 "the cause\n\n\nfailures:\n    lifecycle::tests::x\n\n"
                                 "test result: FAILED. 2 passed; 1 failed; 0 ignored; 0 measured; "
                                 "0 filtered out; finished in 0.10s\n"
                                 "error: test failed, to rerun pass `-p weaver-harness --lib`\n")
                self.assertIn(f"the tests did not pass (cargo exited 101, 2 passed, 1 failed); "
                              f"the whole output is kept in {log}", ran.stderr)
                log.unlink()

    def test_the_echo_names_an_abort_a_garbled_close_and_a_run_of_no_tests(self):
        """**The trimmed echo still names the cause where no failures block
        does.** A test binary that aborts leaves no block: its panic and
        cargo's `Caused by` are echoed, the noise is not. A summary a child's
        line garbled (#507) still closes its block, so the noise after it is
        not echoed. A run of no tests leaves nothing to match, and the log's
        last lines are echoed instead. Perturbations: anchor the block's
        close again and the noise after the garbled summary prints; drop the
        panic rule and the abort's cause is missing; drop the fallback and
        the run of no tests echoes nothing."""
        noise = ('{"depth":1024,"mark":768,"organ":"harness","peak":1,"queue":"trace"}\n'
                 "test lifecycle::tests::y ... ok\n")
        abort = (noise * 20 + "thread 'lifecycle::tests::z' (9) panicked at "
                 "crates/weaver-harness/src/lifecycle/tests.rs:7:3:\nthe abort's cause\n" + noise * 20)
        abort_err = ("error: test failed, to rerun pass `-p weaver-harness --lib`\n\nCaused by:\n"
                     "  process didn't exit successfully: `weaver_harness-0` "
                     "(signal: 6, SIGABRT: process abort signal)\n")
        garbled = ("\nfailures:\n    lifecycle::tests::x\n\n" + noise.splitlines()[0]
                   + "test result: FAILED. 2 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; "
                   "finished in 0.10s\n" + "after the close\n" * 30)
        none = ("   Compiling weaver-trace v0.1.0\n     Running unittests src/lib.rs\n\nrunning 0 tests\n\n"
                "test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n")
        for script in ("update-stack.sh", "bootstrap-stack.sh"):
            with self.subTest(script=script, case="abort"):
                ran = self.step(script, "run_tests", stdout=abort, stderr=abort_err, status=101)
                self.assertNotEqual(ran.returncode, 0)
                self.assertIn("panicked at crates/weaver-harness/src/lifecycle/tests.rs:7:3:\n"
                              "the abort's cause\n", ran.stderr)
                self.assertIn("(signal: 6, SIGABRT: process abort signal)", ran.stderr)
                self.assertNotIn('"queue":"trace"', ran.stderr)
                self.assertNotIn("test lifecycle::tests::y ... ok", ran.stderr)
                for log in self.kept_logs():
                    log.unlink()
            with self.subTest(script=script, case="garbled close"):
                ran = self.step(script, "run_tests", stdout=garbled, status=101)
                self.assertNotEqual(ran.returncode, 0)
                self.assertIn("    lifecycle::tests::x\n", ran.stderr)
                self.assertIn("test result: FAILED. 2 passed; 1 failed", ran.stderr)
                self.assertNotIn("after the close", ran.stderr)
                for log in self.kept_logs():
                    log.unlink()
            with self.subTest(script=script, case="no tests"):
                ran = self.step(script, "run_tests", stdout=none)
                self.assertNotEqual(ran.returncode, 0)
                self.assertIn("running 0 tests", ran.stderr.split("\nREFUSED: ", 1)[0])
                self.assertIn("cargo exited 0, 0 passed, 0 failed", ran.stderr)
                for log in self.kept_logs():
                    log.unlink()

    def test_the_build_takes_the_cuda_host_compiler_update_stack_takes(self):
        ran = self.step("bootstrap-stack.sh", "run_build")
        self.assertEqual(ran.returncode, 0, ran.stderr)
        update = (DEPLOY / "update-stack.sh").read_text()
        default = re.search(r"NVCC_CCBIN=\$\{NVCC_CCBIN:-([^}]+)\}", update).group(1)
        self.assertEqual(self.seen_env()["NVCC_CCBIN"], default)

    def test_the_build_names_its_bytes_by_its_commit(self):
        # The operator's ruling of 2026-11-15: nvcc's -objtemp and ggml's
        # cubin-only list reach the workspace build in both scripts, and an
        # NVCC_APPEND_FLAGS the operator sets is kept after -objtemp.
        # Perturbation: drop either setting from bootstrap-stack.sh's build
        # and the run_build cases fail, from update-stack.sh's and the
        # comparison fails, and a -virtual entry in the list fails both.
        ran = self.step("bootstrap-stack.sh", "run_build")
        self.assertEqual(ran.returncode, 0, ran.stderr)
        seen = self.seen_env()
        self.assertEqual(seen["NVCC_APPEND_FLAGS"], "-objtemp")
        # ggml's own list for this toolkit with its PTX entries dropped:
        # cubins only.
        self.assertEqual(seen["CMAKE_CUDA_ARCHITECTURES"], "86-real;89-real;120a-real;121a-real")
        ran = self.step("bootstrap-stack.sh", "run_build",
                        extra_env={"NVCC_APPEND_FLAGS": "-include cuda/iterator"})
        self.assertEqual(ran.returncode, 0, ran.stderr)
        self.assertEqual(self.seen_env()["NVCC_APPEND_FLAGS"], "-objtemp -include cuda/iterator")

        # update-stack.sh's build is not a function, so its settings are
        # read from its text: the same assignments, ahead of the same cargo
        # build, as the bootstrap's.
        def settings(script):
            text = re.sub(r"\\\n", " ", (DEPLOY / script).read_text())
            builds = [line for line in text.splitlines()
                      if re.search(r"\bcargo build --release --locked --workspace\b", line)]
            self.assertEqual(len(builds), 1, (script, builds))
            prefix = builds[0].split("cargo build", 1)[0]
            return re.findall(r'(\w+)=("[^"]*"|\S+)', prefix)
        self.assertEqual(settings("update-stack.sh"), settings("bootstrap-stack.sh"))
        self.assertIn(("CMAKE_CUDA_ARCHITECTURES", '"86-real;89-real;120a-real;121a-real"'),
                      settings("update-stack.sh"))

    def test_a_run_of_no_tests_refuses(self):
        for script in ("update-stack.sh", "bootstrap-stack.sh"):
            with self.subTest(script=script):
                ran = self.step(script, "run_tests", stdout="   Compiling weaver-trace v0.1.0\n")
                self.assertNotEqual(ran.returncode, 0)
                self.assertIn("0 passed", ran.stderr)
                for log in self.kept_logs():
                    log.unlink()

    def test_a_failed_build_refuses_with_its_diagnostics(self):
        ran = self.step("bootstrap-stack.sh", "run_build", stderr=CARGO_COMPILE_FAILURE, status=101)
        self.assertNotEqual(ran.returncode, 0)
        self.assertIn("error[E0425]", ran.stderr)
        self.assertIn("the build failed (cargo exited 101)", ran.stderr)
        kept = self.kept_logs()
        self.assertEqual(len(kept), 1, kept)
        self.assertIn(str(kept[0]), ran.stderr, "the refusal names the kept log")
        kept[0].unlink()
        ran = self.step("bootstrap-stack.sh", "run_build",
                        stdout="warning: unused variable: `x`\n")
        self.assertEqual(ran.returncode, 0, ran.stderr)
        self.assertEqual([log for log in self.kept_logs() if log.name.startswith("weaver-deploy-build-")], [],
                         "a clean build keeps no log")

    def test_no_cargo_step_pipes_cargo_into_a_filter(self):
        for script in ("update-stack.sh", "bootstrap-stack.sh"):
            with self.subTest(script=script):
                # A command continued with a backslash is read whole, as the
                # old pipe sat on the step's last continued line.
                text = re.sub(r"\\\n", " ", (DEPLOY / script).read_text())
                steps = set()
                for line in text.splitlines():
                    step = re.match(r'\s*(\w+=("[^"]*"|\S*)\s+)*cargo (test|build)\b', line)
                    if step:
                        steps.add(step.group(3))
                        self.assertNotRegex(line, r"\|\s*(grep|awk|tee|head|tail)", line)
                # The match reads both steps, so a new assignment ahead of
                # cargo cannot leave a step unread.
                self.assertEqual(steps, {"test", "build"})




class BusyboxBuildTests(unittest.TestCase):
    """**The shell's pinned build** (#29, weaver-gate-Spec section 8.1):
    the committed configuration elects the static build, the listing, ash
    as sh run standalone, and the fourteen listed applets and nothing that
    reaches the network. build-busybox.sh refuses a tarball that is not the
    pin before it extracts anything."""

    LISTED = ["LS", "CAT", "HEAD", "TAIL", "WC", "GREP", "SORT", "UNIQ", "CUT", "TR",
              "DATE", "ECHO", "PWD", "MKTEMP"]

    # Every election the configuration makes, whole: the build options, the
    # shell, and the fourteen listed applets. Nothing else is elected.
    ELECTED = {
        "HAVE_DOT_CONFIG", "BUSYBOX", "STATIC", "INSTALL_APPLET_SYMLINKS", "NO_DEBUG_LIB",
        "FEATURE_BUFFERS_USE_MALLOC", "TRY_LOOP_CONFIGURE",
        "SH_IS_ASH", "BASH_IS_NONE", "SHELL_ASH", "ASH", "ASH_ECHO", "ASH_PRINTF", "ASH_TEST",
        "FEATURE_SH_STANDALONE",
        *LISTED,
    }

    def test_the_configuration_makes_exactly_its_elections(self):
        # The whole set of elections, exactly: an applet added (find, awk,
        # nslookup, httpd, a network tool) or an option dropped fails here.
        # Perturbation: elect CONFIG_FIND and this fails.
        text = (DEPLOY / "busybox.config").read_text()
        elected = {line.split("=")[0].removeprefix("CONFIG_")
                   for line in text.splitlines() if line.startswith("CONFIG_") and line.endswith("=y")}
        self.assertEqual(elected, self.ELECTED)
        self.assertFalse(any(ord(c) > 127 for c in text), "ASCII")

    def test_the_listing_and_the_configuration_agree_both_ways(self):
        # Read through the tree's declarations: a listed applet the config
        # does not elect, and an elected applet the build did not list, each
        # refuse by name, and the agreeing case passes. Perturbation: drop
        # either direction and its case passes.
        with tempfile.TemporaryDirectory() as tree:
            Path(tree, "a.c").write_text(
                "//applet:IF_LS(APPLET_NOEXEC(ls, ls, BB_DIR_BIN, BB_SUID_DROP, ls))\n"
                "//applet:IF_ASH(APPLET(ash, BB_DIR_BIN, BB_SUID_DROP))\n"
                "//applet:IF_SH_IS_ASH(  APPLET_ODDNAME(sh,   ash, BB_DIR_BIN, BB_SUID_DROP, ash))\n"
                "//applet:IF_FIND(APPLET_NOFORK(find, find, BB_DIR_USR_BIN, BB_SUID_DROP, find))\n")
            config = Path(tree, "config")
            run = lambda listed: subprocess.run(
                ["python3", str(DEPLOY / "busybox_applets.py"), tree, str(config), listed],
                text=True, capture_output=True, timeout=20)
            config.write_text("CONFIG_LS=y\nCONFIG_ASH=y\nCONFIG_SH_IS_ASH=y\n# CONFIG_FIND is not set\n")
            self.assertEqual(run("ls sh ash").returncode, 0)
            extra = run("ls sh ash find")
            self.assertEqual(extra.returncode, 1)
            self.assertIn("listed and not elected: find", extra.stderr)
            config.write_text("CONFIG_LS=y\nCONFIG_ASH=y\nCONFIG_SH_IS_ASH=y\nCONFIG_FIND=y\n")
            missing = run("ls sh ash")
            self.assertEqual(missing.returncode, 1)
            self.assertIn("elected and not listed: find", missing.stderr)

    def test_the_preflight_probes_gcc_as_the_build_does(self):
        # busybox's Makefile names gcc, so a box with cc and no gcc refuses
        # before the long build. Perturbation: probe cc again and this box
        # passes the preflight.
        with tempfile.TemporaryDirectory() as bin_dir:
            for tool in ("tar", "bzip2", "make", "cc", "python3", "sha256sum", "curl", "mktemp", "rm"):
                real = shutil.which(tool)
                if real:
                    os.symlink(real, Path(bin_dir, tool))
            env = {**os.environ, "PATH": bin_dir}
            env.pop("BUSYBOX_TARBALL", None)
            result = subprocess.run(["/bin/bash", str(DEPLOY / "build-busybox.sh"), "--preflight"],
                                    env=env, text=True, capture_output=True, timeout=20)
            self.assertEqual(result.returncode, 1, result.stderr)
            self.assertIn("needs gcc", result.stderr)

    def test_a_tarball_that_is_not_the_pin_refuses(self):
        # Perturbation: drop the sha256 check and the forged tarball reaches
        # tar, which refuses it differently.
        with tempfile.TemporaryDirectory() as out:
            forged = Path(out) / "forged.tar.bz2"
            forged.write_bytes(b"not busybox")
            result = subprocess.run(["bash", str(DEPLOY / "build-busybox.sh"), out, "--tarball", str(forged)],
                                    text=True, capture_output=True, timeout=20)
            self.assertEqual(result.returncode, 1, result.stderr)
            self.assertIn("not the pinned b8cc24c9", result.stderr)
            self.assertFalse((Path(out) / "busybox").exists())


def _agent_tuple():
    import importlib.util
    spec = importlib.util.spec_from_file_location("agent_tuple_under_test", DEPLOY / "agent_tuple.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def _gguf(path, keys, tensors=(), version=3):
    """A GGUF header with `keys` ((name, type, value) triples; arrays as
    (name, 9, (element, [values]))) and `tensors` ((name, dims, type))."""
    import struct

    def text(value):
        raw = value if isinstance(value, bytes) else value.encode()
        return struct.pack("<Q", len(raw)) + raw

    def scalar(kind, value):
        return {4: struct.pack("<I", value), 10: struct.pack("<Q", value),
                5: struct.pack("<i", value)}[kind]

    body = b"GGUF" + struct.pack("<IQQ", version, len(tensors), len(keys))
    for name, kind, value in keys:
        body += text(name) + struct.pack("<I", kind)
        if kind == 8:
            body += text(value)
        elif kind == 9:
            element, items = value
            body += struct.pack("<IQ", element, len(items))
            body += b"".join(text(i) if element == 8 else scalar(element, i) for i in items)
        else:
            body += scalar(kind, value)
    for name, dims, kind in tensors:
        body += text(name) + struct.pack("<I", len(dims)) + b"".join(struct.pack("<Q", d) for d in dims)
        body += struct.pack("<IQ", kind, 0)
    Path(path).write_bytes(body + b"\0" * 64)
    return body


GRANITE_KEYS = [
    ("general.architecture", 8, "granite"),
    ("general.file_type", 4, 32),
    ("granite.block_count", 4, 64),
    ("tokenizer.ggml.tokens", 9, (8, ["<a>", "<b>", "c"])),
    ("tokenizer.ggml.token_type", 9, (5, [1, 1, 1])),
    ("tokenizer.chat_template", 8, "{{ messages }}"),
]
GRANITE_TENSORS = [("token_embd.weight", (4096, 100352), 30), ("blk.0.attn_norm.weight", (4096,), 0),
                   ("output.weight", (4096, 100352), 30)]


class AgentTupleHeaderTests(unittest.TestCase):
    """**The GGUF header reader bounds every length it reads** against the
    file's size and a cap, and refuses a header it cannot bound rather than
    reading on. Perturbation: drop the bound in `_Reader.take` (or in
    `count`'s file check) and the length-past-the-file cases read on or
    fail with a struct error instead of a refusal."""

    def setUp(self):
        self.tuple = _agent_tuple()
        self.dir = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.dir)

    def test_the_header_facts_are_read(self):
        path = self.dir / "m.gguf"
        _gguf(path, GRANITE_KEYS, GRANITE_TENSORS)
        facts = self.tuple.read_header(path)
        self.assertEqual(facts["architecture"], "granite")
        self.assertEqual(facts["block_count"], 64)
        self.assertEqual(facts["file_type"], 32)
        self.assertEqual(facts["chat_template_sha256"], hashlib.sha256(b"{{ messages }}").hexdigest())
        self.assertEqual(facts["tensor_types"], {"BF16": 2, "F32": 1})
        self.assertEqual(facts["tensor_count"], 3)

    def test_a_length_past_the_file_is_refused(self):
        import struct
        path = self.dir / "m.gguf"
        body = _gguf(path, GRANITE_KEYS, GRANITE_TENSORS)
        # The chat template's length field, set past the file's end.
        at = body.index(b"tokenizer.chat_template") + len(b"tokenizer.chat_template") + 4
        raw = bytearray(path.read_bytes())
        raw[at:at + 8] = struct.pack("<Q", len(raw) + 1)
        path.write_bytes(bytes(raw))
        with self.assertRaises(self.tuple.HeaderRefused) as refused:
            self.tuple.read_header(path)
        self.assertIn("past the file's end", str(refused.exception))

    def test_a_count_past_the_file_or_the_cap_is_refused(self):
        import struct
        path = self.dir / "m.gguf"
        _gguf(path, GRANITE_KEYS, GRANITE_TENSORS)
        raw = bytearray(path.read_bytes())
        raw[8:16] = struct.pack("<Q", 1 << 40)          # tensor_count past the cap
        path.write_bytes(bytes(raw))
        with self.assertRaisesRegex(self.tuple.HeaderRefused, "past the cap"):
            self.tuple.read_header(path)
        raw[8:16] = struct.pack("<Q", 1000)             # under the cap, past the file
        path.write_bytes(bytes(raw))
        # Refused at the count, before any tensor info is read.
        with self.assertRaisesRegex(self.tuple.HeaderRefused, "tensor_count: 1000 items of 28 octets run past"):
            self.tuple.read_header(path)

    def test_a_string_array_element_past_the_file_is_refused(self):
        import struct
        path = self.dir / "m.gguf"
        body = _gguf(path, GRANITE_KEYS, GRANITE_TENSORS)
        at = body.index(b"<a>") - 8
        raw = bytearray(path.read_bytes())
        raw[at:at + 8] = struct.pack("<Q", len(raw))
        path.write_bytes(bytes(raw))
        with self.assertRaisesRegex(self.tuple.HeaderRefused, "past the file's end"):
            self.tuple.read_header(path)

    def test_not_a_gguf_and_an_unknown_version_are_refused(self):
        path = self.dir / "m.gguf"
        path.write_bytes(b"PK\x03\x04" + b"\0" * 60)
        with self.assertRaisesRegex(self.tuple.HeaderRefused, "magic"):
            self.tuple.read_header(path)
        _gguf(path, GRANITE_KEYS, version=1)
        with self.assertRaisesRegex(self.tuple.HeaderRefused, "version 1"):
            self.tuple.read_header(path)

    def test_the_header_door_answers_one_json_line_and_refuses_with_status_one(self):
        path = self.dir / "m.gguf"
        _gguf(path, GRANITE_KEYS, GRANITE_TENSORS)
        source = (DEPLOY / "agent_tuple.py").read_bytes()
        ran = subprocess.run([sys.executable, "-I", "-", "header", str(path)], input=source,
                             capture_output=True, cwd=self.dir)
        self.assertEqual(ran.returncode, 0, ran.stderr)
        self.assertEqual(json.loads(ran.stdout)["architecture"], "granite")
        path.write_bytes(b"nope")
        ran = subprocess.run([sys.executable, "-I", "-", "header", str(path)], input=source,
                             capture_output=True, cwd=self.dir)
        self.assertEqual(ran.returncode, 1)
        self.assertIn("refused", json.loads(ran.stdout))


class AgentTupleResolveTests(unittest.TestCase):
    """**The header door resolves a declared artifact as admission does**
    (Codex on #514), mirroring crates/weaver-spu/src/artifact.rs
    `resolve_with_kind`, `container_within` and `split_pattern` case for case
    against that file's own tests. Perturbations named per test."""

    RUST_TESTS = DEPLOY.parent / "crates" / "weaver-spu" / "src" / "artifact" / "tests.rs"

    def setUp(self):
        self.tuple = _agent_tuple()
        self.dir = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.dir)

    def resolve(self, path):
        return Path(self.tuple.resolve_artifact(str(path)))

    def test_the_split_pattern_mirrors_the_rust_cases(self):
        """The hits and misses of `the_split_pattern_is_exact`, read from
        the Rust test itself, so a case added there is a case here.
        Perturbation: accept a signed count, or an index outside its set,
        and a miss parses."""
        text = self.RUST_TESTS.read_text()
        body = text[text.index("fn the_split_pattern_is_exact"):]
        body = body[:body.index("\n}\n")]
        block = body[body.index("for miss in ["):]
        misses = re.findall(r'"([^"]+)"', block[:block.index("]")])
        called = re.findall(r'split_pattern\(Path::new\("([^"]+)"\)\)', body)
        hits = [path for path in called if path not in misses]
        self.assertGreaterEqual(len(misses), 8, misses)
        self.assertGreaterEqual(len(hits), 3, hits)
        for path in hits:
            with self.subTest(hit=path):
                self.assertIsNotNone(self.tuple.split_pattern(os.path.basename(path)))
        for path in misses:
            with self.subTest(miss=path):
                self.assertIsNone(self.tuple.split_pattern(os.path.basename(path)))
        self.assertEqual(self.tuple.split_pattern("model-00001-of-00002.gguf"), ("model", 2, ".gguf"))
        self.assertEqual(self.tuple.split_pattern("model-00001-of-00017.safetensors"),
                         ("model", 17, ".safetensors"))

    def test_a_directory_artifact_resolves_to_its_container_file(self):
        """Mirrors `a_directory_artifact_resolves_to_its_container_file`.
        Perturbation: return the directory itself and the extension check
        fails."""
        _gguf(self.dir / "only.gguf", GRANITE_KEYS, GRANITE_TENSORS)
        resolved = self.resolve(self.dir)
        self.assertTrue(resolved.is_file())
        self.assertEqual(resolved.suffix, ".gguf")

    def test_a_sharded_directory_resolves_to_its_first_shard(self):
        """Mirrors `a_sharded_directory_resolves_to_its_first_shard`, the
        stranger included. Perturbation: take the first name sorted, or
        drop the one-stem check, and a case resolves wrongly."""
        for index in (1, 2, 3):
            (self.dir / f"model-{index:05}-of-00003.safetensors").write_bytes(b"x")
        self.assertEqual(self.resolve(self.dir), self.dir / "model-00001-of-00003.safetensors")
        (self.dir / "other.safetensors").write_bytes(b"x")
        with self.assertRaisesRegex(self.tuple.HeaderRefused, "not one split's shards"):
            self.resolve(self.dir)

    def test_an_ambiguous_directory_refuses(self):
        """Mirrors `an_ambiguous_directory_refuses`, and a split short of
        its count refuses as `container_within` does. Perturbation: take
        the first name sorted and both resolve."""
        for name in ("a.gguf", "b.gguf"):
            (self.dir / name).write_bytes(b"GGUF")
        with self.assertRaisesRegex(self.tuple.HeaderRefused, "2 containers"):
            self.resolve(self.dir)
        short = self.dir / "short"
        short.mkdir()
        for index in (1, 2):
            (short / f"m-{index:05}-of-00003.gguf").write_bytes(b"GGUF")
        with self.assertRaisesRegex(self.tuple.HeaderRefused, "not one split's shards"):
            self.resolve(short)

    def test_a_dot_entry_is_never_the_container(self):
        """Mirrors the resolution half of `a_dot_entry_is_outside_the_identity`.
        Perturbation: keep dot-entries and the directory is ambiguous."""
        (self.dir / "model.safetensors").write_bytes(b"the weights")
        (self.dir / "config.json").write_bytes(b"{}")
        (self.dir / ".stray.safetensors").write_bytes(b"not the model")
        self.assertEqual(self.resolve(self.dir), self.dir / "model.safetensors")

    def test_a_named_pipe_does_not_resolve(self):
        """Mirrors `a_named_pipe_does_not_resolve`, at the path and inside a
        directory. Perturbation: admit any non-directory and the open
        blocks on the pipe."""
        pipe = self.dir / "pipe.gguf"
        os.mkfifo(pipe)
        with self.assertRaisesRegex(self.tuple.HeaderRefused, "not a regular file"):
            self.resolve(pipe)
        with self.assertRaisesRegex(self.tuple.HeaderRefused, "holds no container"):
            self.resolve(self.dir)

    def test_absent_and_denied_are_told_apart(self):
        """Mirrors `a_denied_directory_is_unreadable_rather_than_unresolvable`.
        Skips as root, whom no directory denies. Perturbation: answer every
        stat error as not resolving and the denied case says absent."""
        with self.assertRaisesRegex(self.tuple.HeaderRefused, "does not resolve"):
            self.resolve(self.dir / "absent.gguf")
        if os.getuid() == 0:
            self.skipTest("root traverses every directory")
        closed = self.dir / "closed"
        closed.mkdir()
        _gguf(closed / "m.gguf", GRANITE_KEYS, GRANITE_TENSORS)
        closed.chmod(0)
        self.addCleanup(closed.chmod, 0o700)
        with self.assertRaisesRegex(self.tuple.HeaderRefused, "unreadable"):
            self.resolve(closed / "m.gguf")

    def test_a_safetensors_directory_names_its_container_and_kind(self):
        """The door's refusal for a container without the GGUF magic still
        names the container and the kind admission chose by the magic, so
        the rows door can match the measurement and withhold rows 8 and 9.
        Perturbation: drop either member from the refusal and its
        assertion fails."""
        model = self.dir / "native"
        model.mkdir()
        (model / "model.safetensors").write_bytes(b"\x08\0\0\0\0\0\0\0{}      ")
        source = (DEPLOY / "agent_tuple.py").read_bytes()
        ran = subprocess.run([sys.executable, "-I", "-", "header", str(model)], input=source,
                             capture_output=True, cwd=self.dir)
        self.assertEqual(ran.returncode, 1, ran.stderr)
        answer = json.loads(ran.stdout)
        self.assertEqual(answer["container"], "safetensors")
        self.assertEqual(answer["resolved"], str(model / "model.safetensors"))
        self.assertIn("no GGUF magic", answer["refused"])

    def test_a_later_shard_declared_reads_the_first_shards_header(self):
        """**The header is the first shard's whatever shard is declared**, as
        artifact.rs `pin` opens it. Perturbation: read the declared shard and
        its missing magic refuses the header."""
        _gguf(self.dir / "m-00001-of-00002.gguf", GRANITE_KEYS, GRANITE_TENSORS)
        (self.dir / "m-00002-of-00002.gguf").write_bytes(b"tensor data only")
        facts = self.tuple.read_header(str(self.dir / "m-00002-of-00002.gguf"))
        self.assertEqual(facts["architecture"], "granite")
        self.assertIsNone(facts["resolved"], "a declared file is its own measurement path")

    def test_the_header_door_reads_a_directory_artifact(self):
        """The door end to end: a directory holding one GGUF answers its
        header facts and names the container, and row 1 prints it.
        Perturbation: open the declared path unresolved and the door
        refuses the directory."""
        model = self.dir / "granite"
        model.mkdir()
        _gguf(model / "granite.gguf", GRANITE_KEYS, GRANITE_TENSORS)
        facts = self.tuple.read_header(str(model))
        self.assertEqual(facts["architecture"], "granite")
        self.assertEqual(facts["resolved"], str(model / "granite.gguf"))
        declaration = {"spu-instruction": {"decoder": {"model-binding": {"artifact": str(model)}}}}
        lines = self.tuple.artifact_rows(declaration, facts, None, None)
        self.assertTrue([l for l in lines if "container" in l and "[derivable:" in l], lines)
        self.assertIsNone(self.tuple.read_header(str(model / "granite.gguf"))["resolved"])


class AgentTupleDivisionTests(unittest.TestCase):
    """**Row 8 mirrors `Division::even` and `index_of` case for case**, the
    fixtures taken from weaver-spu/src/division/tests.rs:
    granite_divides_thirty_two_and_thirty_two, odd_layers_over_three_devices,
    every_shape_reproduces_its_counts_under_the_engine_mapping,
    no_device_or_no_layer_has_no_division and
    index_of_names_every_slot_and_nothing_past_the_output. Perturbation: give
    the larger count to the last L mod N devices, or put the output on the
    first device, and these fail."""

    def setUp(self):
        self.tuple = _agent_tuple()

    def test_granite_divides_thirty_two_and_thirty_two(self):
        self.assertEqual(self.tuple.even_division(64, 2), [32, 32])

    def test_odd_layers_over_three_devices(self):
        self.assertEqual(self.tuple.even_division(13, 3), [5, 4, 4])

    def test_every_shape(self):
        for layers in range(1, 131):
            for devices in range(1, 17):
                counts = self.tuple.even_division(layers, devices)
                self.assertEqual(sum(counts), layers)
                larger = layers % devices
                for index, count in enumerate(counts):
                    want = -(-layers // devices) if index < larger else layers // devices
                    self.assertEqual(count, want, (layers, devices, index))
                self.assertEqual(self.tuple.index_of(counts, layers), devices - 1)

    def test_no_device_or_no_layer_has_no_division(self):
        self.assertIsNone(self.tuple.even_division(64, 0))
        self.assertIsNone(self.tuple.even_division(0, 2))

    def test_index_of_names_every_slot_and_nothing_past_the_output(self):
        counts = self.tuple.even_division(5, 2)
        self.assertEqual([self.tuple.index_of(counts, s) for s in range(7)],
                         [0, 0, 0, 1, 1, 1, None])


class FamilyRegistryMirrorTests(unittest.TestCase):
    """**agent_tuple.py's family keys are the SPU registry's**, entry for
    entry and in order, read from weaver-spu/src/family/mod.rs's `REGISTRY`,
    and its fold is `same_key`'s. Perturbation: drop one `llama` entry from
    the mirror and this fails, and a contested key would print as derivable."""

    def test_the_mirror_is_the_registry(self):
        source = (DEPLOY.parent / "crates/weaver-spu/src/family/mod.rs").read_text()
        start = source.index("pub const REGISTRY: &[Declaration] = &[")
        end = source.index("\n];", start)
        block = source[start:end]
        # Only the top-level entries' family members: eight spaces in.
        families = tuple(re.findall(r'^        family: "([^"]+)",$', block, re.M))
        self.assertEqual(_agent_tuple().REGISTRY_FAMILIES, families)

    def test_the_fold_is_same_keys(self):
        fold = _agent_tuple().family_key
        self.assertEqual(fold("qwen3_5_moe"), fold("qwen35moe"))
        self.assertEqual(fold("Nemotron-H.MoE"), "nemotronhmoe")
        self.assertNotEqual(fold("qwen3"), fold("qwen35"))


class AgentTupleRowTests(unittest.TestCase):
    """**Every row ends in one of the four status words**, and each row says
    what the survey says it can. Perturbations named per test."""

    def setUp(self):
        self.tuple = _agent_tuple()
        self.dir = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.dir)
        path = self.dir / "m.gguf"
        _gguf(path, GRANITE_KEYS, GRANITE_TENSORS)
        self.header = self.tuple.read_header(path)

    def declaration(self, devices=(0, 1), adapter=False, restore=None):
        binding = {"artifact": "/models/granite.gguf", "devices": list(devices)}
        if adapter:
            binding["adapter"] = {"artifact": "/models/r1.gguf", "sha256": "a" * 64,
                                  "base-blake3": "b" * 64, "scale": 1.0}
        declaration = {"spu-instruction": {"decoder": {"model-binding": binding}},
                       "state-store": {"engine": "sqlite"}}
        if restore:
            declaration["restore"] = {"save-point": restore}
        return declaration

    def statuses(self, lines):
        return [re.search(r"\[(\w+):", line).group(1) for line in lines]

    def test_every_line_carries_one_of_the_four_words(self):
        decl = self.declaration()
        lines = (self.tuple.artifact_rows(decl, self.header, None, None)
                 + self.tuple.adapter_rows(decl)
                 + self.tuple.device_rows(decl, self.header, ["00000000:01:00.0, NVIDIA RTX A6000, 49140 MiB", "00000000:41:00.0, NVIDIA RTX A6000, 49140 MiB"])
                 + self.tuple.live_rows("999.1.2\n", {}, None)
                 + self.tuple.live_rows("", None, "not asked")
                 + self.tuple.state_rows(decl, {"state_store": {"engine": "sqlite"}})
                 + self.tuple.runtime_rows(None, {}, None)
                 + self.tuple.runtime_rows(None, None, "not asked"))
        for status in self.statuses(lines):
            self.assertIn(status, ("recorded", "derivable", "absent", "FAULT"))

    def test_row_one_takes_the_base_blake3_only_for_an_adapted_load(self):
        """Perturbation: print base-blake3 for a base-only declaration and the
        absent line is lost."""
        adapted = self.tuple.artifact_rows(self.declaration(adapter=True), self.header, None, None)
        self.assertIn("b" * 64, adapted[1])
        self.assertIn("[derivable:", adapted[1])
        base = self.tuple.artifact_rows(self.declaration(), self.header, None, None)
        self.assertIn("[absent:", base[1])
        seen = self.tuple.artifact_rows(self.declaration(), self.header, None,
                                        {"weights_hash": "c" * 64, "line": 7})
        self.assertIn("c" * 64, seen[1])
        self.assertIn("[recorded: the latest model.measurement in the sink naming "
                      "/models/granite.gguf, line 7]", seen[1])

    def test_a_contested_architecture_is_not_printed_as_derivable(self):
        """Perturbation: treat any registry hit as derivable and llama's
        line reads derivable."""
        path = self.dir / "l.gguf"
        _gguf(path, [("general.architecture", 8, "llama"), ("llama.block_count", 4, 16)])
        lines = self.tuple.artifact_rows(self.declaration(), self.tuple.read_header(path), None, None)
        family = next(line for line in lines if " family " in line)
        self.assertIn("[absent: the key is contested by 3 registry entries", family)
        granite = next(line for line in self.tuple.artifact_rows(self.declaration(), self.header, None, None)
                       if " family " in line)
        self.assertIn("[derivable:", granite)

    def test_the_placement_is_pair_three_with_the_output_on_the_last_device(self):
        lines = self.tuple.device_rows(self.declaration(devices=(1, 0)), self.header, None, None)
        placement = next(line for line in lines if "placement" in line)
        self.assertIn("device 1: 32, device 0: 32; output on device 0", placement)
        one = next(line for line in self.tuple.device_rows(self.declaration(devices=(1,)), self.header, None, None)
                   if "placement" in line)
        self.assertIn("all 64 layers and the output on device 1", one)

    def test_row_seven_maps_ordinals_into_the_pci_order_list_and_states_its_premise(self):
        gpus = ["00000000:01:00.0, NVIDIA RTX A6000, 49140 MiB", "00000000:41:00.0, NVIDIA RTX A6000, 49140 MiB", "00000000:C1:00.0, NVIDIA RTX 2000 Ada Generation, 16380 MiB"]
        lines = self.tuple.device_rows(self.declaration(devices=(2,)), self.header, gpus)
        device = next(line for line in lines if "device 2 " in line)
        self.assertIn("RTX 2000 Ada", device)
        self.assertIn("CUDA_DEVICE_ORDER=PCI_BUS_ID", device)
        self.assertIn("read at this load, never stored", device)

    def test_row_seven_judges_the_election_against_the_cards_this_box_has(self):
        """**The cards are read at each load and judged against what the load
        elected** (the operator's rulings of 2026-11-14: one image serves any
        host's cards), never against a list an install stored. Any cards
        that hold the elected ordinals pass, whatever they are; an ordinal
        past the list is a FAULT; no list, and nvidia-smi's failure sentence
        on a box with no card, are absent, and the sentence is never a card.
        Perturbations: drop the range check and the ordinal past the list
        raises rather than faults; take any line as a card and the sentence
        is one."""
        here = ["00000000:01:00.0, NVIDIA RTX A6000, 49140 MiB", "00000000:41:00.0, NVIDIA RTX A6000, 49140 MiB"]
        there = ["00000000:03:00.0, NVIDIA L40S, 46068 MiB", "00000000:04:00.0, NVIDIA L40S, 46068 MiB",
                 "00000000:05:00.0, NVIDIA L40S, 46068 MiB"]
        for cards in (here, there):
            with self.subTest(cards[0]):
                lines = self.tuple.device_rows(self.declaration(), self.header, cards)
                self.assertNotIn("FAULT", self.statuses(lines))
        lines = self.tuple.device_rows(self.declaration(devices=(0, 5)), self.header, here)
        past = next(line for line in lines if "device 5 " in line)
        self.assertIn("[FAULT: the load elected it and nvidia-smi lists 2 cards]", past)
        sentence = ("NVIDIA-SMI has failed because it couldn't communicate with the NVIDIA driver. "
                    "Make sure that the latest NVIDIA driver is installed and running.\n")
        for live in (None, self.tuple.live_gpus(sentence)):
            with self.subTest(live):
                lines = self.tuple.device_rows(self.declaration(), self.header, live)
                device = [line for line in lines if " device 0 " in line]
                self.assertEqual(len(device), 1, lines)
                self.assertIn("[absent: nvidia-smi listed no card at this load]", device[0])

    def test_header_and_declared_text_cannot_reach_the_terminal_raw(self):
        """**No value is printed with a character a terminal acts on**, and an
        architecture that is not a short name is absent, not echoed.
        Perturbation: drop `printable` from `row` and the escape reaches the
        line."""
        path = self.dir / "e.gguf"
        _gguf(path, [("general.architecture", 8, "granite\x1b]0;owned\x07")])
        lines = self.tuple.artifact_rows(self.declaration(), self.tuple.read_header(path), None, None)
        family = next(line for line in lines if " family " in line)
        self.assertIn("[absent: the header carries no general.architecture", family)
        decl = self.declaration(restore="sp\x1b[2J")
        line = self.tuple.state_rows(decl, None)[1]
        self.assertNotIn("\x1b", line)
        self.assertIn("sp\\x1b[2J", line)

    def test_restore_and_engine(self):
        """Perturbation: print a declared save point as `none declared`."""
        lines = self.tuple.state_rows(self.declaration(restore="sp-1"), {"state_store": {"engine": "sqlite"}})
        self.assertIn("sp-1", lines[1])
        lines = self.tuple.state_rows(self.declaration(), {"state_store": {"engine": "none"}})
        self.assertIn("[FAULT:", lines[0])
        self.assertIn("none declared", lines[1])


    def test_rows_eight_and_nine_are_the_gguf_paths_alone(self):
        """**Rows 8 and 9 state the GGUF path's rule** (Codex on #514): a
        safetensors container is served by the native path, and a header
        refused before its magic names no kind, so both rows read absent.
        Perturbation: print row 9 unconditionally and the safetensors and
        unknown cases read derivable."""
        decl = self.declaration()
        gguf = self.tuple.device_rows(decl, self.header, None, None)
        self.assertTrue([l for l in gguf if "row 8" in l and "[derivable:" in l], gguf)
        self.assertTrue([l for l in gguf if "row 9" in l and "[derivable:" in l], gguf)
        native = self.tuple.device_rows(decl, None, None, None, container="safetensors")
        unknown = self.tuple.device_rows(decl, None, None, None)
        for case, lines in (("safetensors", native), ("unknown", unknown)):
            with self.subTest(case):
                for number in ("row 8", "row 9"):
                    found = [l for l in lines if number in l]
                    self.assertEqual(len(found), 1, lines)
                    self.assertIn("[absent:", found[0])
        self.assertIn("the native path, decoder/native.rs and native_pair.rs", native[-1])
        self.assertIn("no container kind is known", unknown[-1])

    def test_a_directory_measurement_is_matched_by_its_container(self):
        """**The measurement names the container a directory resolved to**
        (Codex on #514; residency.rs Resident.artifact, main.rs
        render_measurement), so the rows door matches the header door's
        `resolved`, and the declared directory stays row 1's artifact.
        Perturbation: match the declared path and the hash reads absent."""
        import contextlib
        import io
        model = self.dir / "granite"
        container = model / "granite.safetensors"
        declaration = self.dir / "agent.toml"
        declaration.write_text('[spu-instruction.decoder.model-binding]\n'
                               f'artifact = "{model}"\ndevices = [0]\n')
        sink = self.dir / "trace.ndjson"
        measurement = {"kind": "model.measurement", "run": "r-1", "sequence": 3,
                       "payload": {"model": str(container), "weights_hash": "c" * 64,
                                   "adapter": None}}
        sink.write_text(json.dumps({"kind": "load", "run": "r-1", "sequence": 0, "payload": {}})
                        + "\n" + json.dumps(measurement) + "\n")
        header = json.dumps({"refused": "no GGUF magic", "container": "safetensors",
                             "resolved": str(container)})
        printed = io.StringIO()
        with contextlib.redirect_stdout(printed):
            self.tuple.main(["rows", "--declaration", str(declaration), "--header-json", header,
                             "--sink", str(sink)])
        out = printed.getvalue()
        self.assertRegex(out, r"row 1 +artifact +" + re.escape(str(model)) + r" +\[recorded:")
        self.assertRegex(out, r"row 1 +container +" + re.escape(str(container)) + r" +\[derivable:")
        self.assertIn("c" * 64 + "  [recorded: the latest model.measurement in the sink naming "
                      + str(container) + ", line 2]", out)
        self.assertRegex(out, r"row 9 +cpu offload +- +\[absent: the native path")

    LINEAGE = {"save_point": "e" * 64, "run": "r-0", "sequence": 5, "turn": 1,
               "named_at_restore": False}

    def restored(self, lines):
        return [line for line in lines if " restored from " in line]

    def test_the_restored_save_point_is_read_from_the_lineage(self):
        """**What the load restored is the load event's lineage** (Codex on
        #514): with no [restore], admin restores the manifest's latest and
        the harness names it in `lineage` (the shape the harness's lifecycle
        tests assert). Perturbation: read the declaration alone and the
        reload's digest is not printed."""
        lines = self.tuple.restore_rows(self.declaration(), {"lineage": self.LINEAGE})
        self.assertIn("[recorded: the declaration has no [restore]", lines[0])
        restored = self.restored(lines)
        self.assertEqual(len(restored), 1, lines)
        self.assertIn("e" * 64 + " (run r-0, sequence 5, turn 1)", restored[0])
        self.assertIn("[recorded: the load event's lineage, named_at_restore False]", restored[0])
        lines = self.tuple.restore_rows(self.declaration(), {"state_store": {}})
        self.assertIn("none  [recorded: the load event carries no lineage", self.restored(lines)[0])
        lines = self.tuple.restore_rows(self.declaration(), None)
        self.assertIn("[absent: no load event was read]", self.restored(lines)[0])

    def test_a_declared_save_point_the_load_did_not_restore_is_a_fault(self):
        """**The declaration's [restore] and the lineage must name one save
        point**, by digest or by published name, as admin's selection
        matches them (save_points.rs). Perturbation: drop the comparison,
        or the FAULT for a declared point with no lineage, and a case reads
        recorded."""
        digest = "e" * 64
        for declared in (digest, f"20261109T120000Z-{digest}.save-point"):
            with self.subTest(declared):
                lines = self.tuple.restore_rows(self.declaration(restore=declared),
                                                {"lineage": self.LINEAGE})
                self.assertIn("[recorded:", self.restored(lines)[0])
        cases = {
            "another digest": ("f" * 64, {"lineage": self.LINEAGE}),
            "another name": (f"20261109T120000Z-{'f' * 64}.save-point", {"lineage": self.LINEAGE}),
            "a name around the digest": (f"x{digest}.save-point", {"lineage": self.LINEAGE}),
            "no lineage": (digest, {"state_store": {}}),
            "a lineage with no digest": (digest, {"lineage": {"run": "r-0"}}),
        }
        for said, (declared, event) in cases.items():
            with self.subTest(said):
                lines = self.tuple.restore_rows(self.declaration(restore=declared), event)
                self.assertIn("[FAULT:", self.restored(lines)[0])

    def test_the_exit_reads_the_rows_flag_and_never_their_text(self):
        """**A FAULT is the flag `row` sets**, so a declared value that
        prints "[FAULT:" fails nothing. Perturbation: judge the exit by the
        substring "[FAULT:" over the printed lines and the clean run exits
        3."""
        import contextlib
        import io
        declaration = self.dir / "agent.toml"
        declaration.write_text('[spu-instruction.decoder.model-binding]\n'
                               'artifact = "/models/[FAULT: not a fault]"\ndevices = []\n')
        printed = io.StringIO()
        with contextlib.redirect_stdout(printed):
            clean = self.tuple.main(["rows", "--declaration", str(declaration)])
        self.assertIn("[FAULT: not a fault]", printed.getvalue())
        self.assertEqual(clean, 0, printed.getvalue())
        with contextlib.redirect_stdout(io.StringIO()):
            faulted = self.tuple.main(["rows", "--declaration", str(declaration),
                                       "--install-tuple-unheld", "an open entry"])
        self.assertEqual(faulted, 3)


class AgentTupleRecordReadTests(unittest.TestCase):
    """**The record reads take the trace's line shape**: the envelope
    flattened, the payload under `payload` (weaver-trace event.rs), the
    measurement spliced there whole. Perturbations: read the first
    measurement rather than the last, or keep one naming another artifact,
    and the hash printed is not this binding's."""

    def setUp(self):
        self.tuple = _agent_tuple()
        self.dir = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.dir)

    def test_the_latest_measurement_of_the_declared_artifact(self):
        sink = self.dir / "trace.ndjson"
        lines = [
            {"kind": "load", "payload": {"state_store": {"engine": "sqlite"}}},
            {"kind": "model.measurement", "payload": {"model": "/m.gguf", "weights_hash": "1" * 64}},
            {"kind": "model.output", "payload": {"finish": "stop"}},
            {"kind": "model.measurement", "payload": {"model": "/m.gguf", "weights_hash": "2" * 64}},
        ]
        sink.write_text("".join(json.dumps(line) + "\n" for line in lines) + "not json\n")
        self.assertEqual(self.tuple.latest_measurement(sink, "/m.gguf"),
                         {"weights_hash": "2" * 64, "line": 4})
        self.assertIsNone(self.tuple.latest_measurement(sink, "/other.gguf"))
        empty = self.dir / "empty.ndjson"
        empty.write_text("")
        self.assertIsNone(self.tuple.latest_measurement(empty, "/m.gguf"))

    def test_the_sink_is_read_only_as_a_regular_file_never_through_a_link(self):
        """**A link or a FIFO at the sink's name is refused at the open**, not
        followed or blocked on: the shell's look and the helper's open are two
        moments. Perturbation: open with plain `open()` and the link is
        followed (and the FIFO blocks past the timeout)."""
        real = self.dir / "real.ndjson"
        real.write_text(json.dumps({"kind": "model.measurement",
                                    "payload": {"model": "/m.gguf", "weights_hash": "3" * 64}}) + "\n")
        link = self.dir / "link.ndjson"
        link.symlink_to(real)
        with self.assertRaises(OSError):
            self.tuple.latest_measurement(link, "/m.gguf")
        fifo = self.dir / "fifo.ndjson"
        os.mkfifo(fifo)
        with self.assertRaises(OSError):
            self.tuple.latest_measurement(fifo, "/m.gguf")
        self.assertEqual(self.tuple.latest_measurement(real, "/m.gguf")["weights_hash"], "3" * 64)

    def test_a_measurement_hash_of_another_shape_is_not_taken(self):
        sink = self.dir / "trace.ndjson"
        sink.write_text(json.dumps({"kind": "model.measurement",
                                    "payload": {"model": "/m.gguf", "weights_hash": "\x1b[2Jzz"}}) + "\n")
        self.assertIsNone(self.tuple.latest_measurement(sink, "/m.gguf"))

    def test_the_load_payload_and_the_live_list(self):
        events = self.dir / "trace.ndjson"
        events.write_text(json.dumps({"kind": "load", "payload": {"declaration": "0" * 64}}) + "\n"
                          + json.dumps({"kind": "unload"}) + "\n"
                          + json.dumps({"kind": "load", "payload": {"declaration": "d" * 64}}) + "\n")
        # The load's own events start after the earlier run's: an earlier
        # load is never taken for this one.
        self.assertEqual(self.tuple.load_payload(events, 2), {"declaration": "d" * 64})
        self.assertIsNone(self.tuple.load_payload(events, 4))
        smi = ("00000000:C1:00.0,  NVIDIA RTX 2000 Ada Generation, 16380 MiB\n"
               "00000000:01:00.0, NVIDIA RTX A6000 ,49140 MiB\n")
        self.assertIsNone(self.tuple.live_gpus("fixture-driver\n"))
        self.assertIsNone(self.tuple.live_gpus(
            "NVIDIA-SMI has failed because it couldn't communicate with the NVIDIA driver. "
            "Make sure that the latest NVIDIA driver is installed and running.\n"))
        # `<bus id>, <name>, <memory>`, sorted in the C locale.
        self.assertEqual(self.tuple.live_gpus(smi),
                         ["00000000:01:00.0, NVIDIA RTX A6000, 49140 MiB",
                          "00000000:C1:00.0, NVIDIA RTX 2000 Ada Generation, 16380 MiB"])


class TupleQueryMirrorTests(unittest.TestCase):
    """**The runbook names each nvidia-smi query verify-load runs**, the cards
    and the driver, so the box's operator reads the exact line from section
    4. Perturbation: change either side's query fields and this fails."""

    def test_the_runbook_names_the_scripts_query(self):
        script = (DEPLOY / "verify-load.sh").read_text()
        runbook = " ".join((DEPLOY / "HowToDeployANewAgent.md").read_text().split())
        queries = re.findall(r"nvidia-smi (--query-gpu=\S+ --format=\S+)", script)
        self.assertEqual(len(queries), 2, queries)
        for query in queries:
            self.assertIn(f"`nvidia-smi {query}`", runbook)


class RuntimeMirrorTests(unittest.TestCase):
    """**The writer and the reader name the same runtime files**:
    install-tuple.sh's TUPLE_RUNTIME, lowered for the key, is agent_tuple.py's
    RUNTIME_LIBRARIES, so every key the writer records is one the reader
    binds and none it requires goes unwritten. Perturbation: add or drop a
    name on either side and this fails."""

    def test_the_writer_and_the_reader_name_one_set(self):
        script = (DEPLOY / "install-tuple.sh").read_text()
        names = re.search(r'^TUPLE_RUNTIME="([^"]*)"$', script, re.M).group(1).split()
        self.assertEqual([(name.lower(), "lib" + name) for name in names],
                         list(_agent_tuple().RUNTIME_LIBRARIES))


INSTALL_TUPLE_LINES = [
    "commit = " + "0" * 40,
    "toolchain = nightly-2026-02-13",
    "features = weaver-spu/cuda,weaver-state/sqlite",
    "engine = llama-cpp-sys-2 0.1.146",
    "engine-source = git+https://example.invalid/llama-cpp-rs?rev=" + "a" * 40 + "#" + "b" * 40,
    "engine-vendored = " + "c" * 40,
    "nvcc = 13.4, V13.4.92",
    "member.worker = " + "1" * 64,
    "member.weaver-admin = " + "2" * 64,
    "member.weaver-trace-relay = " + "3" * 64,
    "member.weaver-gate = " + "4" * 64,
    "member.weaver-spu = " + "5" * 64,
    "member.weaver-state = " + "6" * 64,
    "member.busybox = " + "7" * 64,
    "member.busybox.applets = " + "8" * 64,
    "library.libllama.so.0.0.8783 = {llama}",
    "library.libggml-base.so.0.9.11 = {ggml}",
    "runtime.cudart = {cudart}",
    "runtime.cublas = {cublas}",
    "runtime.cublaslt = {cublaslt}",
    "runtime.curand = {curand}",
    "cccl = 3.0",
]

# **A tuple of the earlier form**, as install-tuple.sh wrote it before the
# install tuple held build facts alone: the driver, the card count and each
# card, and the runtime files by path with no digest. The host's pinned stack
# carries one.
EARLIER_TUPLE_LINES = [line for line in INSTALL_TUPLE_LINES if not line.startswith(("runtime.", "cccl "))] + [
    "driver = 580.1",
    "cudart = /opt/cuda/lib/libcudart.so.13.0.1",
    "cublas = /opt/cuda/lib/libcublas.so.13.0.1",
    "cublaslt = /opt/cuda/lib/libcublasLt.so.13.0.1",
    "cccl = 3.0",
    "gpus = 2",
    "gpu.0 = 00000000:01:00.0, NVIDIA RTX A6000, 49140 MiB",
    "gpu.1 = 00000000:41:00.0, NVIDIA RTX A6000, 49140 MiB",
]

# The CUDA runtime files the fixtures lay out, each name a link to its
# versioned file: (tuple key, linked name, versioned file).
RUNTIME_FILES = (("cudart", "libcudart.so.13", "libcudart.so.13.0.1"),
                 ("cublas", "libcublas.so.13", "libcublas.so.13.0.1"),
                 ("cublaslt", "libcublasLt.so.13", "libcublasLt.so.13.0.1"),
                 ("curand", "libcurand.so.10", "libcurand.so.10.4.1"))


def runtime_fixture(directory):
    """Lay out the runtime files and libcuda in `directory`; answer the
    install tuple's runtime values by key and ldd's report naming them."""
    directory.mkdir(parents=True, exist_ok=True)
    values, report = {}, ["\tlinux-vdso.so.1 (0x00007ffd00000000)"]
    for key, name, versioned in RUNTIME_FILES + (("libcuda", "libcuda.so.1", "libcuda.so.999.1.2"),):
        (directory / versioned).write_bytes(f"fixture {key}".encode())
        (directory / name).symlink_to(versioned)
        real = (directory / versioned).resolve()
        values[key] = f"{hashlib.sha256(real.read_bytes()).hexdigest()} {real}"
        report.append(f"\t{name} => {directory / name} (0x00007f0000000000)")
    report.append("\tlibc.so.6 => /usr/lib/libc.so.6 (0x00007f0200000000)")
    return values, "\n".join(report) + "\n"


class AgentTupleInstallTests(unittest.TestCase):
    """**The install tuple is read as install-tuple.sh writes it** and bound
    to the load's record. Perturbations named per test."""

    def setUp(self):
        self.tuple = _agent_tuple()
        self.dir = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.dir)
        self.lib = self.dir / "lib"
        self.lib.mkdir()
        (self.lib / "libllama.so.0.0.8783").write_bytes(b"llama")
        (self.lib / "libggml-base.so.0.9.11").write_bytes(b"ggml")
        (self.lib / "libllama.so.0").symlink_to("libllama.so.0.0.8783")
        self.cuda = self.dir / "cuda"
        self.runtime, self.ldd = runtime_fixture(self.cuda)
        self.digests = {"llama": hashlib.sha256(b"llama").hexdigest(),
                        "ggml": hashlib.sha256(b"ggml").hexdigest(), **self.runtime}
        self.path = self.write(INSTALL_TUPLE_LINES)

    def write(self, lines):
        path = self.dir / "install-tuple"
        path.write_text("".join(line.format(**self.digests) + "\n" for line in lines))
        return path

    def test_the_tuple_is_read(self):
        install = self.tuple.read_install_tuple(self.path)
        self.assertEqual(install["sha256"], hashlib.sha256(self.path.read_bytes()).hexdigest())
        self.assertEqual(install["facts"]["engine-vendored"], "c" * 40)
        self.assertEqual(install["members"]["busybox.applets"], "8" * 64)
        self.assertEqual(install["earlier"], [])
        cudart = (self.cuda / "libcudart.so.13.0.1").resolve()
        self.assertEqual(install["runtime"]["cudart"],
                         (hashlib.sha256(cudart.read_bytes()).hexdigest(), str(cudart)))
        self.assertEqual(sorted(install["runtime"]), ["cublas", "cublaslt", "cudart", "curand"])
        self.assertNotIn("gpus", install)

    def test_an_earlier_tuple_is_read_with_its_hardware_lines_unread(self):
        """**A tuple of the earlier form is read, not refused**, so a box
        installed before this rule verifies until its record is rewritten:
        its driver, card and runtime path lines are left unread, it binds no
        runtime file, and the box rows name the form and the way to rewrite
        it. Its cards are never judged, so a box that has since gained or
        lost a card is no FAULT. Perturbations: refuse the earlier keys and
        the host's own record is refused; bind its runtime paths and the
        rows compare a path with no digest."""
        path = self.write(EARLIER_TUPLE_LINES)
        install = self.tuple.read_install_tuple(path)
        self.assertIsNone(install["runtime"])
        self.assertEqual(install["earlier"], ["cublas", "cublaslt", "cudart", "driver", "gpu.0", "gpu.1", "gpus"])
        self.assertEqual(set(install["facts"]), set(self.tuple.INSTALL_SCALARS))
        box = self.tuple.box_rows(str(path), install, None)
        form, = [line for line in box if "tuple form" in line]
        self.assertIn("[absent: written before the install tuple held build facts alone", form)
        self.assertIn("update-stack.sh --install", form)
        rows = self.tuple.runtime_rows(install, self.tuple.loader_resolved(self.ldd), None)
        self.assertEqual(self.statuses(rows, "runtime "), {"absent"})
        self.assertTrue(all("the earlier form" in row for row in rows), rows)
        fresh = self.write(INSTALL_TUPLE_LINES)
        self.assertNotIn("tuple form", " ".join(self.tuple.box_rows(str(fresh),
                                                                    self.tuple.read_install_tuple(fresh), None)))

    def test_a_malformed_tuple_is_refused(self):
        """Perturbation: drop any one check in `read_install_tuple` and its
        case reads on."""
        new, earlier = INSTALL_TUPLE_LINES, EARLIER_TUPLE_LINES
        cases = {
            "not `key = value`": new + ["cccl=1"],
            "empty or repeated": new + ["cccl = 2"],
            "no cccl": [l for l in new if not l.startswith("cccl")],
            "not 64 hex digits": new[:7] + ["member.worker = zz"] + new[8:],
            "neither form": new + ["driver = 580.1"],
            "so it is neither form": new + ["gpu.0 = 00000000:01:00.0, NVIDIA RTX A6000, 49140 MiB"],
            "it carries both": earlier + ["runtime.cudart = {cudart}"],
            "runtime.libcuda is not a runtime file this reader knows": new + ["runtime.libcuda = {cudart}"],
            "no runtime.curand": [l for l in new if not l.startswith("runtime.curand")],
            "runtime.cudart is not `<sha256> <absolute path>` or unknown":
                [l if not l.startswith("runtime.cudart") else "runtime.cudart = " + "a" * 64 + " lib/x.so"
                 for l in new],
            "runtime.cublas is not": [l if not l.startswith("runtime.cublas ") else "runtime.cublas = /usr/lib/x.so"
                                      for l in new],
        }
        for said, lines in cases.items():
            with self.subTest(said):
                with self.assertRaisesRegex(self.tuple.InstallTupleRefused, said):
                    self.tuple.read_install_tuple(self.write(lines))
        link = self.dir / "linked"
        link.symlink_to(self.write(INSTALL_TUPLE_LINES))
        with self.assertRaises(OSError):
            self.tuple.read_install_tuple(link)

    def test_the_runtime_files_bind_to_what_the_loader_resolves_now(self):
        """**Each runtime file the install recorded is bound to the file the
        loader resolves for the SPU now**, by path and sha256: equal is
        recorded. A file changed in place, a name that now leads to another
        file, a name the loader no longer finds and a library the SPU no
        longer links are each a FAULT naming the drift, since a sealed image
        never shows one. Perturbations: compare the path alone and the
        changed file passes; compare the digest alone and the moved file
        passes; follow no link and every row compares the link's path."""
        install = self.tuple.read_install_tuple(self.path)
        resolved = self.tuple.loader_resolved(self.ldd)
        rows = self.tuple.runtime_rows(install, resolved, None)
        self.assertEqual(len(rows), 4, rows)
        self.assertEqual(self.statuses(rows, "runtime "), {"recorded"})
        # Changed in place: the same path, another digest.
        cudart = self.cuda / "libcudart.so.13.0.1"
        kept = cudart.read_bytes()
        cudart.write_bytes(b"patched")
        try:
            rows = self.tuple.runtime_rows(install, resolved, None)
        finally:
            cudart.write_bytes(kept)
        mine, = [row for row in rows if "runtime cudart " in row]
        self.assertIn("[FAULT: drift from the install", mine)
        self.assertEqual(self.statuses([row for row in rows if row is not mine], "runtime "), {"recorded"})
        cases = {}
        moved = self.dir / "moved"
        shutil.copytree(self.cuda, moved, symlinks=True)
        cases["cublas"] = self.ldd.replace(f"{self.cuda}/libcublas.so.13 ", f"{moved}/libcublas.so.13 ")
        cases["cublaslt"] = self.ldd.replace(f"{self.cuda}/libcublasLt.so.13 (0x00007f0000000000)", "not found")
        cases["curand"] = "\n".join(l for l in self.ldd.splitlines() if "libcurand" not in l) + "\n"
        for key, report in cases.items():
            with self.subTest(key):
                rows = self.tuple.runtime_rows(install, self.tuple.loader_resolved(report), None)
                mine, = [row for row in rows if f"runtime {key} " in row]
                self.assertIn("[FAULT:", mine)
                self.assertEqual(self.statuses([row for row in rows if row is not mine], "runtime "),
                                 {"recorded"})
        self.assertIn("drift from the install", next(
            row for row in self.tuple.runtime_rows(install, self.tuple.loader_resolved(cases["cublas"]), None)
            if "runtime cublas " in row))

    def test_a_runtime_file_with_nothing_to_compare_is_absent(self):
        """**Absent, never FAULT, where nothing can be compared**: no install
        tuple, an `unknown` the install wrote, no loader report given, an
        agent running an SPU the install did not read, or a failed read with
        nothing recorded to bind, and the row says which. Perturbation:
        fault an `unknown` and a box whose install could not resolve its SPU
        fails every verify."""
        resolved = self.tuple.loader_resolved(self.ldd)
        # One path is rewritten per form, so each is read as it is written.
        unknown_install = self.tuple.read_install_tuple(self.write(
            [l.split(" = ")[0] + " = unknown" if l.startswith("runtime.") else l for l in INSTALL_TUPLE_LINES]))
        earlier_install = self.tuple.read_install_tuple(self.write(EARLIER_TUPLE_LINES))
        install = self.tuple.read_install_tuple(self.write(INSTALL_TUPLE_LINES))
        for label, args, said in (
                ("no tuple", (None, resolved, None), "no install tuple was read"),
                ("unknown", (unknown_install, resolved, None), "the install tuple could not resolve or hash it"),
                ("no loader report", (install, None, "no loader report was given"),
                 "the loader was not read: no loader report was given"),
                ("another SPU", (install, resolved, None, None, "this agent's SPU is /x"), "this agent's SPU is /x"),
                ("failed, nothing recorded", (earlier_install, None, "no SPU at /x", "no SPU at /x"),
                 "the earlier form")):
            with self.subTest(label):
                rows = self.tuple.runtime_rows(*args)
                self.assertEqual(self.statuses(rows, "runtime "), {"absent"})
                self.assertTrue(all(said in row for row in rows), rows)

    def test_a_missing_or_unreadable_file_with_no_recorded_digest_is_absent(self):
        """**A row is a FAULT only against a recorded digest** (CodeRabbit on
        #559): where the install recorded `unknown`, or nothing, a runtime
        file the loader cannot find or cannot read is said in an absent row,
        as the runbook states for an unknown digest. A recorded digest still
        faults on both. Perturbation: fault the not-found or unreadable case
        whatever was recorded and the unknown digest fails verify."""
        unknown = self.tuple.read_install_tuple(self.write(
            [l.split(" = ")[0] + " = unknown" if l.startswith("runtime.") else l for l in INSTALL_TUPLE_LINES]))
        earlier = self.tuple.read_install_tuple(self.write(EARLIER_TUPLE_LINES))
        recorded = self.tuple.read_install_tuple(self.write(INSTALL_TUPLE_LINES))
        missing = self.ldd.replace(f"{self.cuda}/libcudart.so.13 (0x00007f0000000000)", "not found")
        unreadable_dir = self.dir / "a-directory"
        unreadable_dir.mkdir()
        unreadable = self.ldd.replace(f"{self.cuda}/libcudart.so.13 ", f"{unreadable_dir} ")
        for label, report, said in (("not found", missing, "the loader finds no file for it"),
                                    ("unreadable", unreadable, "the loader names it and it does not read")):
            for install, why in ((unknown, "the install tuple could not resolve or hash it"),
                                 (earlier, "the earlier form"), (None, "no install tuple was read")):
                with self.subTest(label, why=why):
                    rows = self.tuple.runtime_rows(install, self.tuple.loader_resolved(report), None)
                    mine, = [row for row in rows if "runtime cudart " in row]
                    self.assertIn("[absent: not bound: ", mine)
                    self.assertIn(why, mine)
                    self.assertIn(said, mine)
            with self.subTest(label, why="recorded"):
                rows = self.tuple.runtime_rows(recorded, self.tuple.loader_resolved(report), None)
                mine, = [row for row in rows if "runtime cudart " in row]
                self.assertIn("[FAULT: " + said, mine)

    def test_a_path_holding_a_backslash_records_a_bare_digest(self):
        """**The writer takes each digest from standard input** (CodeRabbit on
        #559): GNU sha256sum escapes a file name holding a backslash and
        marks its digest field with one, which the reader refuses as not 64
        hex digits, so one such directory refused the whole tuple. A member,
        an engine library and a runtime file each under such a directory
        record their bare digests, and the reader takes the tuple.
        Perturbation: hash any of the three by its path again and the reader
        refuses the tuple."""
        odd = self.dir / "a\\b"
        for sub in ("bin", "lib", "cuda"):
            (odd / sub).mkdir(parents=True)
        (odd / "bin" / "worker").write_bytes(b"worker")
        (odd / "bin" / "weaver-spu").write_bytes(b"spu")
        (odd / "lib" / "libllama.so.0.0.8783").write_bytes(b"llama")
        (odd / "cuda" / "libcudart.so.13.0.1").write_bytes(b"cudart")
        (odd / "cuda" / "libcudart.so.13").symlink_to("libcudart.so.13.0.1")
        stubs = self.dir / "stubs"
        stubs.mkdir()
        (stubs / "ldd").write_text("#!/bin/sh\nprintf '%s' \"$REPORT\"\n")
        (stubs / "ldd").chmod(0o755)
        for name in ("cargo", "git", "nvidia-smi", "nvcc", "pacman", "rustc"):
            (stubs / name).write_text("#!/bin/sh\nexit 1\n")
            (stubs / name).chmod(0o755)
        repo = Path(__file__).resolve().parent.parent
        script = (f"source {shlex.quote(str(repo / 'deploy' / 'engine-libraries.sh'))}; "
                  f"source {shlex.quote(str(repo / 'deploy' / 'install-tuple.sh'))}; "
                  f"install_tuple {shlex.quote(str(self.dir))} {'0' * 40} {shlex.quote(str(odd / 'bin'))} "
                  f"{shlex.quote(str(odd / 'lib'))} fixture 'worker' 'libllama.so.0.0.8783'")
        report = f"\tlibcudart.so.13 => {odd / 'cuda' / 'libcudart.so.13'} (0x00007f0000000000)\n"
        ran = subprocess.run(["bash", "-c", script], text=True, capture_output=True, timeout=20,
                             env={"PATH": str(stubs) + os.pathsep + GUARDED_USR_BIN, "LC_ALL": "C", "REPORT": report})
        self.assertEqual(ran.returncode, 0, ran.stderr)
        facts = dict(line.split(" = ", 1) for line in ran.stdout.splitlines())
        self.assertEqual(facts["member.worker"], hashlib.sha256(b"worker").hexdigest())
        self.assertEqual(facts["library.libllama.so.0.0.8783"], hashlib.sha256(b"llama").hexdigest())
        cudart = (odd / "cuda" / "libcudart.so.13.0.1").resolve()
        self.assertEqual(facts["runtime.cudart"], f"{hashlib.sha256(b'cudart').hexdigest()} {cudart}")
        install = self.tuple.read_install_tuple(self.write([ran.stdout.rstrip("\n")]))
        self.assertEqual(install["runtime"]["cudart"], (hashlib.sha256(b"cudart").hexdigest(), str(cudart)))

    def test_a_runtime_file_recorded_but_unbindable_is_a_fault(self):
        """**Fail closed**: where the install recorded a runtime file and the
        loader's read cannot be trusted or bound, the row is a FAULT naming
        why, never absent. Perturbation: read a failed read as absent and an
        SPU another principal could write passes verify."""
        install = self.tuple.read_install_tuple(self.path)
        rows = self.tuple.runtime_rows(install, None, "x", "the SPU at /x is not held closed by root at /x")
        self.assertEqual(self.statuses(rows, "runtime "), {"FAULT"})
        self.assertTrue(all("cannot be bound to it: the SPU at /x is not held closed" in row for row in rows), rows)

    def test_the_driver_is_read_at_this_load_and_never_judged_against_an_install(self):
        """**The driver and its library are the box's**, read at each load:
        one version is derivable whatever it is, two on one box is a FAULT,
        and nvidia-smi's failure sentence names no version. libcuda prints
        by the file its name leads to and that file's digest, and a libcuda
        the loader cannot find is a FAULT. Perturbations: take any line as a
        version and the sentence is one; drop the one-version check and two
        drivers pass."""
        resolved = self.tuple.loader_resolved(self.ldd)
        rows = self.tuple.live_rows("999.1.2\n999.1.2\n", resolved, None)
        self.assertEqual(len(rows), 2, rows)
        self.assertEqual(self.statuses(rows[:1], "driver"), {"derivable"})
        self.assertIn(" 999.1.2 ", rows[0])
        libcuda = (self.cuda / "libcuda.so.999.1.2").resolve()
        self.assertIn(f"{hashlib.sha256(libcuda.read_bytes()).hexdigest()} {libcuda}", rows[1])
        self.assertEqual(self.statuses(rows[1:], "libcuda"), {"derivable"})
        rows = self.tuple.live_rows("999.1.2\n580.1\n", resolved, None)
        self.assertEqual(self.statuses(rows[:1], "driver"), {"FAULT"})
        sentence = ("NVIDIA-SMI has failed because it couldn't communicate with the NVIDIA driver. "
                    "Make sure that the latest NVIDIA driver is installed and running.\n")
        rows = self.tuple.live_rows(sentence, None, "not asked")
        self.assertIn("[absent: nvidia-smi named no driver version at this load]", rows[0])
        self.assertNotIn("NVIDIA-SMI", " ".join(rows))
        self.assertIn("[absent: not read: not asked]", rows[1])
        missing = self.ldd.replace(f"{self.cuda}/libcuda.so.1 (0x00007f0000000000)", "not found")
        rows = self.tuple.live_rows("999.1.2\n", self.tuple.loader_resolved(missing), None)
        self.assertEqual(self.statuses(rows[1:], "libcuda"), {"FAULT"})

    def test_a_name_is_never_taken_for_a_longer_one(self):
        """**`libcublas` is never `libcublasLt`**, in the helper or in the
        writer. Perturbation: match by prefix alone and cublas binds to
        cublasLt's file."""
        only_lt = "\n".join(l for l in self.ldd.splitlines() if "libcublas.so" not in l) + "\n"
        self.assertIsNone(self.tuple.resolved_as(self.tuple.loader_resolved(only_lt), "libcublas"))
        self.assertEqual(self.tuple.resolved_as(self.tuple.loader_resolved(only_lt), "libcublasLt")[0],
                         "libcublasLt.so.13")

    def test_two_majors_of_one_library_read_the_first_in_ldd_order(self):
        """**The writer and the reader take one file** where a report names
        two majors of one library: the first in ldd's order, as the writer's
        awk takes it (the self-check of 2026-11-14). Perturbation: iterate
        the map sorted and the reader takes .12 where the writer took .13."""
        report = ("\tlibcudart.so.13 => /x/libcudart.so.13 (0x0001)\n"
                  "\tlibcudart.so.12 => /x/libcudart.so.12 (0x0002)\n")
        self.assertEqual(self.tuple.resolved_as(self.tuple.loader_resolved(report), "libcudart"),
                         ("libcudart.so.13", "/x/libcudart.so.13"))
        stubs = self.dir / "stubs"
        stubs.mkdir()
        (stubs / "ldd").write_text("#!/bin/sh\nprintf '%s' \"$REPORT\"\n")
        (stubs / "ldd").chmod(0o755)
        for name in ("cargo", "git", "nvidia-smi", "nvcc", "pacman", "rustc"):
            (stubs / name).write_text("#!/bin/sh\nexit 1\n")
            (stubs / name).chmod(0o755)
        repo = Path(__file__).resolve().parent.parent
        script = (f"source {shlex.quote(str(repo / 'deploy' / 'engine-libraries.sh'))}; "
                  f"source {shlex.quote(str(repo / 'deploy' / 'install-tuple.sh'))}; "
                  f"install_tuple {shlex.quote(str(self.dir))} {'0' * 40} {shlex.quote(str(self.dir))} "
                  f"{shlex.quote(str(self.dir))} fixture '' ''")
        ran = subprocess.run(["bash", "-c", script], text=True, capture_output=True, timeout=20,
                             env={"PATH": str(stubs) + os.pathsep + GUARDED_USR_BIN, "LC_ALL": "C",
                                  "REPORT": report.replace("/x/", f"{self.cuda}/")})
        facts = dict(line.split(" = ", 1) for line in ran.stdout.splitlines())
        self.assertTrue(facts["runtime.cudart"].endswith(str((self.cuda / "libcudart.so.13.0.1").resolve())),
                        facts)

    def test_the_tuple_install_tuple_sh_writes_is_read(self):
        """**The reader takes what the writer writes**: the tuple comes from
        install-tuple.sh's own `install_tuple` on a box that answers nothing,
        so every member and library digest reads `unknown`, and the reader
        takes it whole. Perturbation: refuse an `unknown` digest and the
        writer's tuple is refused."""
        repo = Path(__file__).resolve().parent.parent
        stubs = self.dir / "stubs"
        stubs.mkdir()
        for name in ("cargo", "git", "nvidia-smi", "nvcc", "pacman", "rustc"):
            (stubs / name).write_text("#!/bin/sh\nexit 1\n")
            (stubs / name).chmod(0o755)
        empty = self.dir / "empty-repo"
        empty.mkdir()
        script = (f"source {shlex.quote(str(repo / 'deploy' / 'engine-libraries.sh'))}; "
                  f"source {shlex.quote(str(repo / 'deploy' / 'install-tuple.sh'))}; "
                  f"install_tuple {shlex.quote(str(empty))} {'0' * 40} "
                  f"{shlex.quote(str(self.dir / 'no-bin'))} {shlex.quote(str(self.dir / 'no-lib'))} "
                  f"fixture 'worker weaver-spu busybox.applets' 'libllama.so.0.0.8783'")
        env = {"PATH": str(stubs) + os.pathsep + GUARDED_USR_BIN, "LC_ALL": "C"}
        ran = subprocess.run(["bash", "-c", script], env=env, text=True,
                             capture_output=True, timeout=20)
        self.assertEqual(ran.returncode, 0, ran.stderr)
        install = self.tuple.read_install_tuple(self.write([ran.stdout.rstrip("\n")]))
        self.assertEqual(set(install["facts"]), set(self.tuple.INSTALL_SCALARS))
        self.assertEqual(install["members"], {"worker": "unknown", "weaver-spu": "unknown",
                                              "busybox.applets": "unknown"})
        self.assertEqual(install["libraries"], {"libllama.so.0.0.8783": "unknown"})
        self.assertEqual(install["runtime"], {"cudart": None, "cublas": None, "cublaslt": None, "curand": None})
        self.assertEqual(install["earlier"], [])

    def test_an_unknown_digest_binds_nothing_and_faults_nothing(self):
        """**An `unknown` digest is the box side unstated**, so the row is
        absent, never a fault. Perturbation: drop either `unknown` branch in
        `binding_rows` and its row reads FAULT."""
        lines = [l.replace("1" * 64, "unknown") if l.startswith("member.worker") else
                 l.replace("{llama}", "unknown") for l in INSTALL_TUPLE_LINES]
        install = self.tuple.read_install_tuple(self.write(lines))
        rows = self.tuple.binding_rows("d" * 64, self.load(), install, str(self.lib))
        worker = [r for r in rows if "member worker" in r]
        llama = [r for r in rows if "library libllama.so.0.0.8783" in r]
        self.assertEqual(len(worker), 1, rows)
        self.assertEqual(len(llama), 1, rows)
        self.assertIn("[absent: not bound: the install tuple could not hash it]", worker[0])
        self.assertIn("[absent: not bound: the install tuple could not hash it]", llama[0])
        self.assertFalse([r for r in rows if "FAULT" in r], rows)

    def load(self, **override):
        event = {"declaration": "d" * 64, "library_path": str(self.lib),
                 "stack": {"worker": "1" * 64, "weaver-spu": "5" * 64, "weaver-state": "6" * 64,
                           "weaver-gate": "4" * 64, "weaver-spu-classify": "9" * 64}}
        event.update(override)
        return event

    def statuses(self, lines, field):
        return {re.search(r"\[(\w+):", l).group(1) for l in lines if field in l}

    def test_the_record_binds_to_the_box(self):
        install = self.tuple.read_install_tuple(self.path)
        lines = self.tuple.binding_rows("d" * 64, self.load(), install, str(self.lib))
        self.assertEqual(self.statuses(lines, "declaration"), {"recorded"})
        self.assertEqual(self.statuses(lines, "member worker"), {"recorded"})
        # Names one side holds are printed as not bound, never compared.
        self.assertEqual(self.statuses(lines, "member busybox "), {"absent"})
        self.assertEqual(self.statuses(lines, "member weaver-spu-classify"), {"absent"})
        self.assertEqual(self.statuses(lines, "library libllama.so.0.0.8783"), {"recorded"})
        self.assertFalse([l for l in lines if "libllama.so.0 " in l], "a link is never listed")
        self.assertNotIn("FAULT", " ".join(lines))

    def test_each_disagreement_is_a_fault_naming_which(self):
        """Perturbations: compare the declaration loosely, skip a stack name,
        or skip hashing the library files, and a case passes."""
        install = self.tuple.read_install_tuple(self.path)
        lines = self.tuple.binding_rows("e" * 64, self.load(), install, str(self.lib))
        self.assertEqual(self.statuses(lines, "declaration"), {"FAULT"})
        stack = dict(self.load()["stack"], worker="0" * 64)
        lines = self.tuple.binding_rows("d" * 64, self.load(stack=stack), install, str(self.lib))
        self.assertEqual(self.statuses(lines, "member worker"), {"FAULT"})
        (self.lib / "libggml-base.so.0.9.11").write_bytes(b"changed")
        lines = self.tuple.binding_rows("d" * 64, self.load(), install, str(self.lib))
        self.assertEqual(self.statuses(lines, "library libggml-base.so.0.9.11"), {"FAULT"})
        (self.lib / "libggml-cuda.so.0.9.11").write_bytes(b"extra")
        lines = self.tuple.binding_rows("d" * 64, self.load(), install, str(self.lib))
        self.assertEqual(self.statuses(lines, "library libggml-cuda.so.0.9.11"), {"FAULT"})

    def test_a_library_path_the_tuple_does_not_digest_is_not_bound(self):
        install = self.tuple.read_install_tuple(self.path)
        lines = self.tuple.binding_rows("d" * 64, self.load(library_path="/elsewhere"), install, str(self.lib))
        self.assertEqual(self.statuses(lines, "library path"), {"absent"})

    def test_the_box_rows_cite_the_tuples_digest(self):
        install = self.tuple.read_install_tuple(self.path)
        lines = self.tuple.box_rows(str(self.path), install, None)
        self.assertIn(install["sha256"], lines[0])
        self.assertIn("fork " + "b" * 40, lines[1])
        lines = self.tuple.box_rows(str(self.path), None, "the install tuple is refused: x")
        self.assertIn("[FAULT:", lines[0])
        lines = self.tuple.box_rows(str(self.path), None, None)
        self.assertIn(f"[absent: no install tuple at {self.path}; the box side is unstated]", lines[0])

    def test_no_install_tuple_named_is_said_without_a_placeholder(self):
        """**No install tuple named reads as a sentence**: verify-load names
        none where the stack record holds none, and the live box row read
        "no install tuple at -", the dash standing for the path. Perturbation:
        pass "-" for the absent path again and the row names it."""
        lines = self.tuple.box_rows(None, None, None)
        self.assertEqual(len(lines), 1)
        self.assertIn("[absent: no install tuple was named; the box side is unstated]", lines[0])
        self.assertNotIn(" at -", lines[0])
        declaration = self.dir / "agent.toml"
        declaration.write_text("")
        ran = subprocess.run([sys.executable, "-I", str(DEPLOY / "agent_tuple.py"), "rows",
                              "--declaration", str(declaration)], capture_output=True, text=True, timeout=30)
        box, = [line for line in ran.stdout.splitlines() if line.lstrip().startswith("box ")]
        self.assertIn("[absent: no install tuple was named; the box side is unstated]", box)


class _NamingResult(unittest.TextTestResult):
    """Names the running test to every child it starts, for the reach record."""

    def startTest(self, test):
        os.environ["WEAVER_TEST_ID"] = test.id()
        super().startTest(test)

    def stopTest(self, test):
        # A reach after the test (tearDownClass, a module's teardown) is
        # unknown, never the last test's.
        super().stopTest(test)
        os.environ.pop("WEAVER_TEST_ID", None)


class _Runner(unittest.TextTestRunner):
    resultclass = _NamingResult


def finish(passed):
    """**The run's end**: the guard's line, then the verdict. A reach of a
    program the guard refuses, or a flag lost, fails the run whatever the
    tests said."""
    try:
        reaches = reaches_in(REACH_RECORD)
    except OSError as e:
        _FINISHED.append(None)
        print(f"guard: the reach record {REACH_RECORD} cannot be read ({type(e).__name__}), so this run cannot "
              "show that no test reached a program the guard refuses", file=sys.stderr)
        return 1
    _FINISHED.append(len(reaches))
    held = no_new_privs_held()
    print(f"guard: no_new_privs {'held' if held else 'LOST'}, "
          f"{len(reaches)} reach{'' if len(reaches) == 1 else 'es'} of a program the guard refuses", file=sys.stderr)
    if os.geteuid() == 0:
        print("guard: the run is root, which no_new_privs withholds nothing from", file=sys.stderr)
    say_reaches(reaches)
    if reaches:
        print(f"guard: the record stays at {REACH_RECORD}", file=sys.stderr)
    return 0 if passed and held and not reaches else 1


def run_suite(argv=None, module="__main__"):
    program = unittest.main(module=module, argv=argv, testRunner=_Runner, exit=False)
    result = program.result
    status = finish(result.wasSuccessful())
    # unittest's own main exits 5 when no test ran, so a filter that matches
    # nothing never reads as a pass.
    if status == 0 and result.testsRun == 0 and not result.skipped:
        return 5
    return status


if __name__ == "__main__":
    sys.exit(run_suite())
