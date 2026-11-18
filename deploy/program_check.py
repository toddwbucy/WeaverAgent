#!/usr/bin/env python3
"""**The programs a deployed copy of deploy/ calls that an image does not carry**
(#610, which a live take-down found: decommission.sh called hostname, and the
image has no hostname program, while the deploy suite passed throughout).

usage: program_check.py --programs LIST [--dynamic] (--copy LIST | FILE...)

--programs LIST  the image's program names, one per line (# lines and blank
                 lines are skipped): deploy/image-programs.txt, or the names a
                 sealed image's MANIFEST holds under its bin directories.
--copy LIST      check every file deploy/deployed-copy.txt names, beside it.
--dynamic        also print each command named through a variable or by a
                 path outside the bin directories, for a reader to review.

For a shell script it reads every command position: a line's start, after
`;`, `&`, `|`, `&&`, `||`, `(`, `{`, `!` and the shell's keywords, inside
`$( )`, `<( )`, arithmetic, `${ }` and double quotes, the code strings
`sh -c`, `bash -c`, `flock -c`, `env -S`, `trap` and `eval` run, a heredoc a
shell reads, and the substitutions in an unquoted heredoc. Through each
wrapper (sudo, runuser, env, timeout, setsid, nice, nohup, stdbuf, ionice,
flock, xargs, exec, command, time, find -exec, tar -I, and the scripts' own
wrapper functions in OWN_WRAPPERS) it reads the command the wrapper runs. A
program named by its path in a bin directory is read by its name, as a
program. An array run as a command it reads from its one assignment in each
state its appends can leave, when all of them stand in the run's own
function or all at the top level.
Each command word, and each program a `command -v`, `type` or `hash` looks
up, is a builtin or keyword, a function the script or a sibling it sources
defines, or a program, and a program must be on LIST or stand in GUARDED,
inside the guard its entry names. A function that runs a command it was
handed (its own arguments, or a name it took from them) must stand in
OWN_WRAPPERS, so a new wrapper cannot hide the commands handed to it. A
Python file, and the Python a shell script carries, passes only when it
starts no program. What a wrapper execs must be a program, never a builtin
or a function, and a program that runs a command this reader does not read
(UNREAD_RUNNERS: unshare, chroot, su, watch, perl, ...) is named.

Prints one line per finding, `FILE:LINE: WORD: why`, and exits 1 when any
stands, 0 when none does, 2 when a file cannot be read.

What it cannot see: a program named through a variable or assembled with
`set --` and run later (listed with --dynamic), a program another program
starts on its own, a variable that holds code at run time (arithmetic that
evaluates a string), an option or a behaviour the image's build of a program
lacks, and anything about the image that is not a program. A construct this
reader does not follow (a backtick substitution, coproc, a shell reading a
standard input that is not its own heredoc, a heredoc whose $( ) closes
before its body) is a finding, never a pass.

**It reads the copied scripts, not bash.** Where it cannot positively
recognise what a line runs, it refuses the line, and the command line prints
what the script's author does: rewrite to a form it reads, or stand the
line in a stated allowance with its reason. The allowances are LEAVES (a
program on the list that runs nothing it is handed), DYNAMIC (a command
word it cannot name), GUARDED and OWN_WRAPPERS. A shape none of the copied
scripts uses is answered by such a fence, never by reading more of bash.
"""
import argparse
import bisect
import os
import re
import sys

BUILTINS = frozenset(
    ". : [ alias bg bind break builtin caller cd command compgen complete compopt continue declare dirs "
    "disown echo enable eval exec exit export false fc fg getopts hash help history jobs kill let local "
    "logout mapfile popd printf pushd pwd read readarray readonly return set shift shopt source suspend "
    "test times trap true type typeset ulimit umask unalias unset wait".split())
KEYWORDS = frozenset("if then else elif fi case esac for select while until do done in function time "
                     "{ } ! [[ ]] coproc".split())
# The image's root PATH: a program by an absolute path here is read by its name.
BIN_DIRS = frozenset(("/usr/bin", "/bin", "/usr/sbin", "/sbin", "/usr/local/bin", "/usr/local/sbin"))

# **A program the image lacks, allowed only inside its guard**: (script,
# program, the guard's text, which holds the program's word, what the guard
# leaves). A use passes only where its word lies inside the guard's text on
# its line, so a bare call beside a guard is still named.
GUARDED = (
    ("decommission.sh", "nvcc", "command -v nvcc ||",
     "a lookup whose failure leaves NVCC empty, so the archive's record reads nvcc none"),
    ("decommission.sh", "rustup", "as_op rustup show active-toolchain 2>/dev/null | cut -d' ' -f1 || echo unknown)",
     "under pipefail the archive's record reads toolchain unknown"),
)

# **Programs that run a command this reader does not read**: a call to one is
# named, so a program behind it cannot pass unread. None of the copied scripts
# calls one. The interpreters run code of their own.
UNREAD_RUNNERS = frozenset(
    "unshare setpriv chroot nsenter su sg newgrp watch systemd-run systemd-inhibit chrt taskset prlimit "
    "numactl doas pkexec runcon capsh bwrap firejail strace ltrace valgrind gdb script ssh cgexec "
    "start-stop-daemon dbus-run-session xvfb-run parallel fakeroot faketime unbuffer busybox toybox perf "
    "perl ruby node lua tclsh expect php".split())

# **The wrappers wrapped() reads**: the command each runs is read in its
# place.
WRAPPERS = frozenset("sh bash dash sudo runuser env timeout setsid nohup nice stdbuf ionice flock xargs find tar "
                     "time".split())

# **The programs on the list this reader knows run nothing they are handed**,
# by their reason. A call to a program on the list that stands neither here,
# nor in WRAPPERS, is refused, so a wrapper this reader has not met cannot
# pass the command behind it. Where a program can run one through an option
# or a mode, wrapped() refuses that option or mode.
LEAVES = {
    "reads or writes files and text": "basename cat chattr chmod chown cp cut date dirname du getfacl grep lsattr "
                                      "ls mkdir mkfifo mktemp mv numfmt realpath rm rmdir setfacl sha256sum stat "
                                      "tail tee tr wc zstd",
    "reads or sets accounts and groups": "getent groupadd groupdel id logname useradd userdel usermod",
    "reads or sets the system's state by name": "findmnt ldconfig nvidia-smi pgrep repquota setquota systemctl "
                                                "uname",
    "the program a shell builtin stands for, where a wrapper runs it": "echo false kill printf pwd test [ true",
    "loads its argument with the dynamic loader in trace mode": "ldd",
    "runs a program only by --compress-program, which wrapped() refuses": "sort",
    "runs a program only by --strip-program, which wrapped() refuses": "install",
    "opens an editor unless it checks (-c), and wrapped() refuses it without": "visudo",
    "runs hooks only when it changes packages, and wrapped() refuses it past a query (-Q)": "pacman",
    "starts a program only by system() or a pipe, which wrapped() refuses in its program": "awk",
    "starts a program only by its e command or s///e, and wrapped() passes s, p, d and q alone": "sed",
    "runs Python, which the Python scan reads for every way it starts a program": "python3",
}
LEAF_NAMES = frozenset(n for names in LEAVES.values() for n in names.split())

# **Command words this reader cannot name**, each with what it runs: (script,
# the word as written). Any other such word is refused, and an entry no line
# of its script uses is named.
DYNAMIC = {
    ("create-agent.sh", '"$ADMIN_BINARY"'): "weaver-admin, the stack's own binary, which the image does not carry",
    ("decommission.sh", '"$ADMIN_BIN"'): "weaver-admin, the stack's own binary, which the image does not carry",
    ("verify-load.sh", '"$ADMIN"'): "weaver-admin, the stack's own binary, which the image does not carry",
    ("decommission.sh", '"$NVCC"'): "nvcc as `command -v nvcc` found it, run only when found (GUARDED)",
    ("account-drop.sh", "$fd"): "eval's `exec $fd>&-` closes a descriptor the case above it admits as a number, "
                                "and runs nothing",
}

# **The variables bash runs as code** (a prompt, or a file a new bash
# sources, or a function a new bash defines), refused wherever the script
# names one, since this reader does not read their values as code.
CODE_VARIABLES = r"(?<![A-Za-z0-9_])(?:PS[0-4]|PROMPT_COMMAND|BASH_ENV|ENV|BASH_FUNC_\w*)(?![A-Za-z0-9_])"

# **What the author does about each refusal**, by a pattern its reason starts
# with, printed after it. The first match wins.
REMEDIES = (
    ("this reader does not follow a backtick", "write it as $( )"),
    ("this reader does not follow coproc", "run it without coproc"),
    (r"this reader does not follow a heredoc inside \$\( \)", "end the $( ) after the heredoc's body"),
    ("this reader does not follow an unclosed", "close it"),
    ("this reader does not follow a heredoc", "close the heredoc on a line of its own"),
    ("this reader does not follow code nested", "flatten the nesting"),
    ("this reader does not follow an array inside an array", "write the array's elements as words"),
    ("this reader does not follow what ", "run the command without it, or read its grammar in wrapped()"),
    (r"this reader does not follow \w+, which changes what a name runs", "remove it, so each name runs as bash's "
                                                                          "defaults run it"),
    ("this reader does not follow a variable bash runs as code", "remove it, or run the code as a command"),
    ("this reader does not follow tar ", "compress or extract outside tar, or with -I"),
    ("this reader does not follow sort ", "sort without --compress-program"),
    ("this reader does not follow install ", "install without --strip-program"),
    ("this reader does not follow visudo ", "check the file with visudo -c"),
    ("this reader does not follow pacman ", "query with pacman -Q, and change packages outside the copied scripts"),
    ("this reader does not follow awk ", "write the awk program as one word, with only -F and -v, and without "
                                         "system() or a pipe"),
    ("this reader does not follow sed ", "write the sed script as s, p, d and q commands, in a word of its own"),
    (r"this reader does not follow (?:sh|bash|dash) reading", "hand the shell its code with -c, or with its own "
                                                               "heredoc"),
    (r'this reader does not follow "\$\{', "write the array as one NAME=( ), a statement of its own at the top of its "
                                         "scope before the run, and at most MAX_APPENDS NAME+=( ) outside loops, with "
                                         "no other write to the name, and run it in the shell that assigned it"),
    ("this reader does not follow", "give its code string a word of its own, or rewrite the line in a form the "
                                    "copied scripts already use"),
    ("this reader does not know whether", "add it to LEAVES with the reason it runs nothing it is handed, or read "
                                          "its grammar in wrapped()"),
    ("this reader cannot name the program", "name the program, or add (script, word) to DYNAMIC with what it runs"),
    ("a function that runs its arguments", "add it to OWN_WRAPPERS with what comes before its command"),
    ("DYNAMIC allows", "remove the entry"),
    ("GUARDED allows", "remove the entry"),
    (r"(?:called|looked up) here, and not a program", "carry the program in the image's list, or stand the call "
                                                      "inside a guard in GUARDED"),
    ("starts a program, which this checker", "start the program from a copied shell script, where this checker "
                                             "reads it"),
    ("this reader failed", "rewrite the line in a form the copied scripts already use, and report the reader's "
                           "defect"),
)


def remedy(why):
    return next((r.replace("MAX_APPENDS", str(MAX_APPENDS)) for start, r in REMEDIES if re.match(start, why)),
                None)


# **The scripts' own wrapper functions**, each a function that runs its own
# arguments as a command, with what comes before that command:
# "-x" an optional flag, "-x ARG" an optional flag and its argument, "ARG" one
# positional, and "NAME=" any number of assignments.
OWN_WRAPPERS = {
    "root": ("-n",),
    "as_op": (),
    "read_as_owner": (),
    "as_account": ("--stdin ARG", "ARG", "NAME="),
}

# An array run as a command is read in each subset of its appends, so the
# appends it may carry are few.
MAX_APPENDS = 6
# Code inside code ($( ), a code string, a heredoc a shell reads) past this
# depth is refused, never explored.
MAX_DEPTH = 32

ASSIGNMENT = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*(\[[^]]*\])?\+?=")
NAME = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*$")
SEPARATORS = frozenset(("\n", ";", "&", "&&", "||", "|", "|&"))
CASE_ENDS = frozenset((";;", ";&", ";;&"))
REDIRECT = re.compile(r"(\d+|\{[A-Za-z_][A-Za-z0-9_]*\})?(<<<|<<-|<<|<>|<&|>>|>&|>\||<|>)")
# A parameter a function was handed: $1 to $9, $@, $*, and their ${ } forms.
POSITIONAL = re.compile(r"\$(?:[1-9@*]|\{[!#]?(?:[1-9]|@|\*)[^}]*\})")
_STARTS = r"(?:system|popen|exec\w*|spawn\w*|posix_spawn\w*|fork\w*)"
PYTHON_RUNS = re.compile(r"\bsubprocess\b|\b(?:os|posix|nt)\." + _STARTS + r"\b"
                         r"|\bfrom\s+(?:os|posix|nt)\s+import\b[^\n]*(?:\b" + _STARTS + r"\b|\*|\()"
                         r"|\bimport\s+(?:[\w.]+(?:\s+as\s+\w+)?\s*,\s*)*(?:os|posix|nt)\s+as\b"
                         r"|\bpty\b|\bcreate_subprocess_\w+|\bmultiprocessing\b")


class Refused(Exception):
    pass


class Word:
    __slots__ = ("raw", "pos", "dynamic", "value")

    def __init__(self, raw, pos, dynamic, value):
        self.raw, self.pos, self.dynamic, self.value = raw, pos, dynamic, value


class Tok:
    __slots__ = ("kind", "text", "pos", "word")

    def __init__(self, kind, text, pos, word=None):
        self.kind, self.text, self.pos, self.word = kind, text, pos, word


class Command:
    """One simple command: its words, and the heredocs it reads."""

    def __init__(self):
        self.words = []
        self.heredocs = []  # (body start, body end, quoted)
        self.shell_reads = False  # a shell reads its heredoc, whose body comes later


class Script:
    """One shell file: the text, its line offsets, and what a reading found."""

    def __init__(self, path, text):
        self.path, self.text = path, text
        self.name = os.path.basename(path)
        self.lines = [0] + [m.end() for m in re.finditer("\n", text)]
        self.functions = set()
        self.sources = []          # (sibling name, line)
        self.runs_args = {}        # function name -> line where it runs a command it was handed
        self.arg_names = {}        # function name -> the names it assigns from its arguments
        self.arrays = {}           # (function, NAME) -> [(appended, elements, in a loop)], each NAME=( ) or NAME+=( )
        self.refs = []             # (kind, word, line, pos): kind call, lookup, dynamic
        self.refusals = []         # (line, why)

    def line(self, pos):
        return bisect.bisect_right(self.lines, pos)


class Reader:
    """Reads one stretch of a script's text, [start, end), as shell code."""

    def __init__(self, script, start, end, scope, functions, function=None, stop=None, arrays=None, depth=0,
                 loops=0, fresh=False):
        if depth > MAX_DEPTH:
            raise Refused(f"code nested deeper than {MAX_DEPTH}")
        self.depth = depth
        self.loops = loops          # the loops this reading stands in, from for, select, while or until to done
        self.fresh = fresh          # a new shell's code, which inherits none of the script's functions
        self.conds = 0              # the ifs this reading stands in
        self.continued = False      # the last operator was &&, ||, | or |&, so a newline continues the list
        self.chained = False        # after &&, ||, | or |&, before the list ends
        self.at_start = True        # the command being read starts a list
        self.in_array = False       # reading an array's elements
        self.s, self.t = script, script.text
        self.i, self.end = start, end
        self.scope = scope          # the own wrappers in scope, by name
        self.functions = functions  # the functions in scope: the script's and its siblings'
        self.arrays = {} if arrays is None else arrays  # a first reading's arrays, by name
        self.function = function    # the function whose body this is
        self.stop = stop            # ")" inside $( ), else None
        self.frames = []            # "{", "(" or ("case",)
        self.pattern = False        # reading a case pattern
        self.pending_function = None
        self.heredocs = []          # (command, delimiter, strip tabs, quoted)
        self.cur = None             # the command being read

    # ---- characters -------------------------------------------------------
    def at(self, k=0):
        j = self.i + k
        return self.t[j] if j < self.end else ""

    def starts(self, s):
        return self.t.startswith(s, self.i) and self.i + len(s) <= self.end

    def blanks(self):
        while self.i < self.end:
            c = self.t[self.i]
            if c in " \t":
                self.i += 1
            elif c == "\\" and self.at(1) == "\n":
                self.i += 2
            else:
                return

    # ---- nested code --------------------------------------------------------
    def nested(self, start):
        """Read $( ... ) from START, the first character inside, and return
        the position just past its closing parenthesis."""
        sub = Reader(self.s, start, self.end, self.scope, self.functions, self.current_function(), stop=")",
                     arrays=self.arrays, depth=self.depth + 1, loops=self.loops, fresh=self.fresh)
        return sub.read()

    def code_string(self, word, fresh=True):
        """Read a code string a command hands a shell (sh -c, tar -I), or this
        shell (eval and trap, not FRESH). A new shell inherits none of the
        script's functions, so each name in its code is a program."""
        raw = word.raw
        if len(raw) >= 2 and raw[0] == raw[-1] and raw[0] in "'\"":
            start, end = word.pos + 1, word.pos + len(raw) - 1
        else:
            start, end = word.pos, word.pos + len(raw)
        Reader(self.s, start, end, self.scope, self.functions, self.current_function(), arrays=self.arrays,
               depth=self.depth + 1, loops=self.loops, fresh=fresh or self.fresh).read()

    def established(self):
        """Whether the array just read is assigned on every path that reaches
        what follows it: read by the script's own reading (not in $( ), a
        code string or a heredoc), outside any if, case, loop, group or
        subshell but its own function's body, starting its list, and ending
        its statement (a newline, a ; or a comment next), so no condition,
        pipe or background can skip it or take it into a subshell."""
        frames = [f for f in self.frames if not (isinstance(f, tuple) and len(f) == 2)]
        own = [f for f in self.frames if isinstance(f, tuple) and len(f) == 2]
        if self.depth or self.fresh or self.stop or self.loops or self.conds or frames or len(own) > 1:
            return False
        if not self.at_start or self.chained:
            return False
        j = self.i
        while j < self.end and self.t[j] in " \t":
            j += 1
        return j >= self.end or self.t[j] in "\n#" or self.t[j] == ";" and self.t[j + 1:j + 2] not in (";", "&")

    def past(self, word, n):
        """WORD from its Nth character on, as a word of its own."""
        return Word(word.raw[n:], word.pos + n, word.dynamic, word.value[n:])

    def parameter(self):
        """Skip ${ ... } from just past its brace, reading any $( ) inside."""
        depth = 1
        while self.i < self.end:
            c = self.t[self.i]
            if c == "\\":
                self.i += 2
            elif c == "'":
                self.single()
            elif c == '"':
                self.i += 1
                self.double()
            elif self.starts("$(") and not self.starts("$(("):
                self.i = self.nested(self.i + 2)
            elif self.starts("${"):
                self.i += 2
                depth += 1
            elif c == "}":
                self.i += 1
                depth -= 1
                if depth == 0:
                    return
            else:
                self.i += 1
        raise Refused("an unclosed ${")

    def arithmetic(self):
        """Skip arithmetic from just past its (( , reading any $( ) inside."""
        depth = 2
        while self.i < self.end:
            if self.starts("$(") and not self.starts("$(("):
                self.i = self.nested(self.i + 2)
                continue
            c = self.t[self.i]
            self.i += 1
            if c == "(":
                depth += 1
            elif c == ")":
                depth -= 1
                if depth == 0:
                    return
        raise Refused("an unclosed arithmetic expression")

    def single(self):
        j = self.t.find("'", self.i + 1, self.end)
        if j < 0:
            raise Refused("an unclosed single quote")
        self.i = j + 1

    def double(self, end_at=None):
        """Read double-quoted text from just past the quote to just past the
        closing one (or to END_AT for an unquoted heredoc's body). Returns
        whether it expands anything."""
        limit = self.end if end_at is None else end_at
        dynamic = False
        while self.i < limit:
            c = self.t[self.i]
            if c == "\\":
                self.i += 2
            elif c == '"' and end_at is None:
                self.i += 1
                return dynamic
            elif self.starts("$(("):
                self.i += 3
                self.arithmetic()
                dynamic = True
            elif self.starts("$("):
                self.i = self.nested(self.i + 2)
                dynamic = True
            elif self.starts("${"):
                self.i += 2
                self.parameter()
                dynamic = True
            elif c == "$" and (self.at(1).isalnum() or self.at(1) in "_@*#?$!-"):
                self.i += 2
                dynamic = True
            elif c == "`":
                raise Refused("a backtick substitution")
            else:
                self.i += 1
        if end_at is None:
            raise Refused("an unclosed double quote")
        return dynamic

    # ---- tokens -----------------------------------------------------------
    def word(self):
        start, dynamic = self.i, False
        value = []
        while self.i < self.end:
            c = self.t[self.i]
            if c in " \t\n;&|()<>":
                if c in "<>" and self.at(1) == "(":
                    self.i += 2
                    self.i = self.nested(self.i)
                    dynamic = True
                    continue
                if c == "(" and ASSIGNMENT.match(self.t[start:self.i]) and self.t[self.i - 1] == "=":
                    if self.in_array:
                        raise Refused("an array inside an array")
                    name = re.match(r"[A-Za-z_][A-Za-z0-9_]*", self.t[start:self.i]).group(0)
                    appended = self.t[self.i - 2] == "+"
                    self.in_array = True
                    try:
                        elements = self.array()
                    finally:
                        self.in_array = False
                    self.s.arrays.setdefault((self.current_function(), name), []).append(
                        (appended, elements, self.loops != 0, self.established(), start))
                    dynamic = True
                    continue
                break
            if c == "\\":
                if self.at(1) == "\n":
                    self.i += 2
                    continue
                value.append(self.at(1))
                self.i += 2
            elif c == "'":
                j = self.i
                self.single()
                value.append(self.t[j + 1:self.i - 1])
            elif self.starts("$'"):
                self.i += 1
                j = self.i
                self.i += 1
                while self.i < self.end and self.t[self.i] != "'":
                    self.i += 2 if self.t[self.i] == "\\" else 1
                if self.i >= self.end:
                    raise Refused("an unclosed $' quote")
                self.i += 1
                value.append(self.t[j + 1:self.i - 1])
            elif c == '"' or self.starts('$"'):
                self.i += 2 if c == "$" else 1
                j = self.i
                if self.double():
                    dynamic = True
                value.append(self.t[j:self.i - 1])
            elif self.starts("$(("):
                self.i += 3
                self.arithmetic()
                dynamic = True
            elif self.starts("$("):
                self.i = self.nested(self.i + 2)
                dynamic = True
            elif self.starts("${"):
                self.i += 2
                self.parameter()
                dynamic = True
            elif c == "$" and (self.at(1).isalnum() or self.at(1) in "_@*#?$!-"):
                self.i += 2
                dynamic = True
            elif c == "`":
                raise Refused("a backtick substitution")
            else:
                value.append(c)
                self.i += 1
        raw = self.t[start:self.i]
        return Word(raw, start, dynamic, "".join(value))

    def array(self):
        """Read NAME=( ... ) from its parenthesis, and return its elements."""
        self.i += 1
        elements = []
        while True:
            while self.i < self.end and self.t[self.i] in " \t\n":
                self.i += 1
            if self.i >= self.end:
                raise Refused("an unclosed array")
            if self.t[self.i] == ")":
                self.i += 1
                return elements
            if self.t[self.i] == "#":
                j = self.t.find("\n", self.i, self.end)
                self.i = self.end if j < 0 else j
                continue
            elements.append(self.word())

    def heredoc_bodies(self):
        """At the start of a line after heredoc operators: read each body,
        and the code in it a shell would run."""
        for command, delim, strip, quoted in self.heredocs:
            start = self.i
            while self.i < self.end:
                j = self.t.find("\n", self.i, self.end)
                j = self.end if j < 0 else j
                line = self.t[self.i:j]
                if (line.lstrip("\t") if strip else line) == delim:
                    body_end = self.i
                    self.i = min(j + 1, self.end)
                    break
                self.i = j + 1
            else:
                raise Refused(f"a heredoc never closed by {delim}")
            command.heredocs.append((start, body_end, quoted))
            if command.shell_reads:
                Reader(self.s, start, body_end, self.scope, self.functions, self.current_function(),
                       arrays=self.arrays, depth=self.depth + 1, loops=self.loops, fresh=True).read()
            if not quoted:
                keep = self.i
                self.i = start
                self.double(end_at=body_end)
                self.i = keep
        self.heredocs = []

    def token(self):
        """The next token, or None at the end of this stretch."""
        while True:
            self.blanks()
            if self.i >= self.end:
                return None
            c = self.t[self.i]
            if c == "#":
                j = self.t.find("\n", self.i, self.end)
                self.i = self.end if j < 0 else j
                continue
            break
        pos = self.i
        if c == "\n":
            self.i += 1
            if self.heredocs:
                self.heredoc_bodies()
            return Tok("op", "\n", pos)
        for op in (";;&", ";;", ";&", ";", "&&", "||", "|&", "|"):
            if self.starts(op):
                self.i += len(op)
                return Tok("op", op, pos)
        if self.starts("(("):
            self.i += 2
            self.arithmetic()
            return Tok("arith", "((", pos)
        if c in "()":
            self.i += 1
            return Tok("op", c, pos)
        if c in "<>" and self.at(1) == "(":
            return Tok("word", None, pos, self.word())
        for op in ("&>>", "&>"):
            if self.starts(op):
                self.i += len(op)
                return Tok("redirect", op, pos)
        if c == "&":
            self.i += 1
            return Tok("op", "&", pos)
        m = REDIRECT.match(self.t, self.i, self.end)
        if m:
            self.i = m.end()
            return Tok("redirect", m.group(2), pos)
        return Tok("word", None, pos, self.word())

    # ---- commands ---------------------------------------------------------
    def read(self):
        """Read to the end of the stretch, or past the ) that closes $( ).
        Returns the position just past it."""
        tok = None
        while True:
            tok = self.token() if tok is None else tok
            if tok is None:
                if self.stop:
                    raise Refused("an unclosed $(")
                if self.heredocs:
                    raise Refused("a heredoc with no body")
                return self.i
            if self.pattern:
                tok = self.case_pattern(tok)
                continue
            if tok.kind == "op":
                if tok.text in ("&&", "||", "|", "|&"):
                    self.chained = True
                elif tok.text != "\n":
                    self.chained = False
                elif not self.continued:
                    self.chained = False
                self.continued = tok.text in ("&&", "||", "|", "|&")
                if tok.text == ")":
                    if self.frames and self.kind_of(self.frames[-1]) == "(":
                        self.frames.pop()
                        tok = self.after_compound()
                        continue
                    if self.stop == ")":
                        if any(f == ("case",) for f in self.frames):
                            raise Refused("an unclosed case inside $( )")
                        if self.heredocs:
                            raise Refused("a heredoc inside $( ) whose body follows the )")
                        return self.i
                    raise Refused("an unmatched )")
                if tok.text == "(":
                    self.open_frame("(")
                elif tok.text in CASE_ENDS:
                    if not (self.frames and self.frames[-1] == ("case",)):
                        raise Refused(f"{tok.text} outside a case")
                    self.pattern = True
                tok = None
                continue
            if tok.kind == "arith":
                tok = self.after_compound()
                continue
            self.at_start, self.continued = not self.chained, False
            tok = self.command(tok)

    def open_frame(self, kind):
        self.frames.append(kind)
        if self.pending_function:
            self.frames[-1] = (kind, self.pending_function)
            self.pending_function = None

    @staticmethod
    def kind_of(frame):
        return frame[0] if isinstance(frame, tuple) and len(frame) == 2 else frame

    def current_function(self):
        for f in reversed(self.frames):
            if isinstance(f, tuple) and len(f) == 2:
                return f[1]
        return self.function

    def close_frame(self, kind):
        while self.frames:
            f = self.frames.pop()
            if f == kind or self.kind_of(f) == kind:
                return
        raise Refused(f"an unmatched {'}' if kind == '{' else ')'}")

    def after_compound(self):
        """After }, ), done, fi, esac or (( )): skip redirections to the next
        separator, and hand back the token that ends them."""
        while True:
            tok = self.token()
            if tok is None:
                return None
            if tok.kind == "redirect":
                self.redirect_target(tok, Command())
                continue
            if tok.kind == "word":
                raise Refused(f"a word after a compound command: {tok.word.raw}")
            return tok

    def redirect_target(self, tok, command):
        self.blanks()
        target = self.token()
        if target is None or target.kind != "word":
            raise Refused(f"a redirection {tok.text} with no target")
        if tok.text in ("<<", "<<-"):
            raw = target.word.raw
            quoted = any(q in raw for q in "'\"\\")
            self.heredocs.append((command, target.word.value, tok.text == "<<-", quoted))

    def case_pattern(self, tok):
        """Read one case pattern, through its ), or esac."""
        while tok is not None and tok.kind == "op" and tok.text == "\n":
            tok = self.token()
        if tok is None:
            raise Refused("an unclosed case")
        if tok.kind == "word" and tok.word.raw == "esac":
            self.pattern = False
            self.close_frame(("case",))
            return self.after_compound()
        if tok.kind == "op" and tok.text == "(":
            tok = self.token()
        while tok is not None and not (tok.kind == "op" and tok.text == ")"):
            if tok.kind == "op" and tok.text not in ("|",):
                raise Refused(f"{tok.text!r} inside a case pattern")
            tok = self.token()
        if tok is None:
            raise Refused("an unclosed case pattern")
        self.pattern = False
        return None

    def command(self, tok):
        """Read one command from TOK, the first token at a command position.
        Returns the token after it, if read."""
        if tok.kind == "word":
            w = tok.word.raw
            if w == "{":
                self.open_frame("{")
                return None
            if w == "}":
                self.close_frame("{")
                return self.after_compound()
            if w in ("fi", "done"):
                if w == "done":
                    self.loops -= 1
                else:
                    self.conds -= 1
                return self.after_compound()
            if w == "esac":
                # The last branch's ;; is optional.
                if not (self.frames and self.frames[-1] == ("case",)):
                    raise Refused("esac outside a case")
                self.frames.pop()
                return self.after_compound()
            if w in ("then", "do", "else", "elif", "if", "while", "until", "!"):
                if w in ("while", "until"):
                    self.loops += 1
                elif w == "if":
                    self.conds += 1
                return None
            if w == "time":
                nxt = self.token()
                while nxt is not None and nxt.kind == "word" and nxt.word.raw in ("-p", "--"):
                    nxt = self.token()
                return nxt
            if w == "case":
                subject = self.token()
                into = self.token()
                if subject is None or into is None or into.kind != "word" or into.word.raw != "in":
                    raise Refused("a case with no in")
                self.frames.append(("case",))
                self.pattern = True
                return None
            if w in ("for", "select"):
                self.loops += 1
                nxt = self.token()
                if nxt is not None and nxt.kind == "arith":
                    return None
                nxt = self.token()
                while nxt is not None and nxt.kind == "op" and nxt.text == "\n":
                    nxt = self.token()
                if nxt is not None and nxt.kind == "word" and nxt.word.raw == "in":
                    while True:
                        nxt = self.token()
                        if nxt is None or (nxt.kind == "op" and nxt.text in (";", "\n")):
                            return None
                return nxt
            if w == "function":
                name = self.token()
                if name is None or name.kind != "word":
                    raise Refused("function with no name")
                self.define(name.word)
                nxt = self.token()
                if nxt is not None and nxt.kind == "op" and nxt.text == "(":
                    close = self.token()
                    if close is None or close.text != ")":
                        raise Refused("function name( with no )")
                    return None
                return nxt
            if w == "[[":
                while True:
                    nxt = self.token()
                    if nxt is None:
                        raise Refused("an unclosed [[")
                    if nxt.kind == "word" and nxt.word.raw == "]]":
                        return self.after_compound()
            if w == "coproc":
                raise Refused("coproc")
        command = Command()
        self.cur = command
        while tok is not None:
            if tok.kind == "word":
                command.words.append(tok.word)
            elif tok.kind == "redirect":
                self.redirect_target(tok, command)
            elif tok.kind == "op" and tok.text == "(" and len(command.words) == 1 \
                    and NAME.match(command.words[0].raw.replace("-", "_")):
                close = self.token()
                if close is None or close.kind != "op" or close.text != ")":
                    raise Refused("name( with no )")
                self.define(command.words[0])
                return None
            else:
                break
            tok = self.token()
        self.resolve(command)
        return tok

    def define(self, word):
        self.s.functions.add(word.raw)
        self.pending_function = word.raw

    # ---- what a command runs ---------------------------------------------
    def handed(self, raw):
        """Whether RAW expands a parameter the current function was handed,
        or a name it assigned from one."""
        fn = self.current_function()
        if fn is None:
            return False
        return bool(POSITIONAL.search(raw)) or any(
            re.search(r"\$\{?" + re.escape(n) + r"\b", raw) for n in self.s.arg_names.get(fn, ()))

    def note_assignment(self, word):
        if ASSIGNMENT.match(word.raw) and self.handed(word.raw.split("=", 1)[1]):
            name = re.match(r"[A-Za-z_][A-Za-z0-9_]*", word.raw).group(0)
            self.s.arg_names.setdefault(self.current_function(), set()).add(name)

    def resolve(self, command, external=False, expanded=frozenset()):
        """Read what COMMAND runs. EXTERNAL: a wrapper execs it, so it is a
        program, which no builtin, keyword or function can be. EXPANDED: the
        arrays already read in place, which are not read again, and whose
        elements are never assignments, as bash reads them."""
        words = command.words
        k = 0
        while not expanded and k < len(words) and ASSIGNMENT.match(words[k].raw):
            self.note_assignment(words[k])
            k += 1
        if k >= len(words):
            return
        # What runs here is never a function: behind command, a wrapper or a
        # path, or in a new shell's code.
        skip_functions = external or self.fresh
        while k < len(words):
            w = words[k]
            line = self.s.line(w.pos)
            m = re.match(r'^"?\$\{([A-Za-z_][A-Za-z0-9_]*)\[@\]\}"?$', w.raw)
            if m:
                self.array_run(m.group(1), w, line, words[k + 1:], external, expanded)
                return
            if w.dynamic:
                # **A command it was handed**: "$@", "$1", or a name it took
                # from them, as `"$c" "$@"` runs after `local c=$1`, which
                # OWN_WRAPPERS fences. Any other word it cannot name is fenced
                # by DYNAMIC.
                if self.handed(w.raw):
                    self.s.runs_args.setdefault(self.current_function(), line)
                    self.s.refs.append(("handed", w.raw, line, w.pos))
                else:
                    self.s.refs.append(("dynamic", w.raw, line, w.pos))
                return
            name = w.value
            if "/" in name:
                if os.path.dirname(name) not in BIN_DIRS:
                    self.s.refs.append(("dynamic", w.raw, line, w.pos))
                    return
                # **A program by its path in a bin directory** (`/usr/bin/env
                # hostname`): read by its name, and always a program, so the
                # command a wrapper runs, and an unread runner, are read as
                # they are by name.
                name = os.path.basename(name)
                external = skip_functions = True
            rest = words[k + 1:]
            if name == "time":
                # **The program time**: command() reads the keyword at a
                # command's start, so a time here (after an assignment or a
                # redirect, quoted, escaped, behind command, a wrapper or an
                # array) is the program, which runs a command of its own.
                self.s.refs.append(("call", name, line, w.pos))
                j = self.wrapped(name, rest)
                if j is None:
                    return
                k, external, skip_functions = k + 1 + j, True, True
                continue
            if external and (name in BUILTINS or name in KEYWORDS):
                # **What a wrapper execs is a program** (`timeout 5 printf`,
                # `sudo cd`): looked for on the list, as any program is.
                self.s.refs.append(("call", name, line, w.pos))
                return
            if name in self.scope and not skip_functions:
                k = k + 1 + self.own_skip(self.scope[name], rest)
                external = skip_functions = True
                continue
            if name in ("command",) and not external:
                # command skips functions, never builtins.
                skip_functions = True
                j = 0
                while j < len(rest) and rest[j].value.startswith("-") and len(rest[j].value) > 1:
                    if rest[j].value == "--":
                        j += 1
                        break
                    if set(rest[j].value[1:]) & {"v", "V"}:
                        for looked in rest[j + 1:]:
                            if not looked.value.startswith("-"):
                                self.lookup(looked)
                        return
                    j += 1
                k = k + 1 + j
                continue
            if name in ("enable", "alias") or name == "hash" and any(re.fullmatch(r"-[A-Za-z]*p[A-Za-z]*", a.value)
                                                                     for a in rest) \
                    or name == "shopt" and any(a.value == "expand_aliases" for a in rest):
                self.s.refusals.append((line, f"this reader does not follow {name}, which changes what a name runs"))
                return
            if name in ("type", "hash"):
                for looked in rest:
                    if not looked.value.startswith("-"):
                        self.lookup(looked)
                return
            if name == "builtin":
                # The builtin it names, never a function.
                k, skip_functions = k + 1, True
                continue
            if name == "exec":
                j = 0
                while j < len(rest) and rest[j].value.startswith("-") and len(rest[j].value) > 1:
                    if rest[j].value == "--":
                        j += 1
                        break
                    j += 2 if rest[j].value.endswith("a") else 1
                k = k + 1 + j
                external = skip_functions = True
                continue
            if name in ("eval",):
                for arg in rest:
                    self.code_string(arg, fresh=False)
                return
            if name == "trap":
                if rest and rest[0].value == "--":
                    rest = rest[1:]
                if rest and rest[0].value not in ("-", "") and not rest[0].value.startswith("-"):
                    self.code_string(rest[0], fresh=False)
                return
            if name in ("source", "."):
                if rest:
                    m = re.search(r"/([A-Za-z0-9_.+-]+)\"?$", rest[0].raw)
                    if m and ("BASH_SOURCE" in rest[0].raw or "$0" in rest[0].raw):
                        self.s.sources.append((m.group(1), line))
                return
            if name in ("local", "declare", "typeset", "readonly", "export"):
                for arg in rest:
                    self.note_assignment(arg)
            if name in BUILTINS or name in KEYWORDS:
                return
            if not skip_functions and (name in self.s.functions or name in self.functions):
                return
            self.s.refs.append(("call", name, line, w.pos))
            if name in UNREAD_RUNNERS:
                self.s.refusals.append((line, f"this reader does not follow what {name} runs"))
                return
            j = self.wrapped(name, rest)
            if j is None:
                return
            k = k + 1 + j
            external = skip_functions = True

    def array_run(self, name, w, line, rest, external, expanded):
        """**An array run as a command** (verify-load's LDD_ENV), read in each
        state its appends can leave, or refused. Fenced: every mention of the
        name in the script is NAME=( ), NAME+=( ) or ${NAME[...]}, and none
        stands in another shell file beside it or a file it sources, so no
        other write (an element, a scalar, unset, mapfile, read -a) can reach
        the run. There is one assignment and at most MAX_APPENDS appends, all
        in the run's own function or all at the top level, since bash's local
        reaches the functions a function calls, and none in a loop, which can
        repeat it. The assignment is established (see established()) and
        stands earlier in the text than the run, and the run is not in a new
        shell's code, which inherits no array. An append may stand under a condition that skips it, or
        later in the text than the run, so the command is read in every
        subset of the appends, in their order. An array run inside another's
        run is refused, which bounds the reading."""
        why = None
        entries = [(scope, entry) for (scope, n), recorded in self.arrays.items() if n == name for entry in recorded]
        scopes = {scope for scope, _ in entries}
        assigned = [entry for _, entry in entries]
        mention = r"(?<![A-Za-z0-9_])" + name + r"(?![A-Za-z0-9_])"
        known = r"(?<![A-Za-z0-9_])" + name + r"\+?=\(|\$\{[#!]?" + name + r"\["
        if self.fresh:
            why = "run in a new shell's code, which inherits none of the script's arrays"
        elif expanded:
            why = "run inside another array's run"
        elif len(re.findall(mention, self.s.text)) != len(re.findall(known, self.s.text)) \
                or re.search(mention, beside(self.s.path, tuple(n for n, _ in self.s.sources))):
            why = f"written elsewhere than {name}=( ), {name}+=( ) and ${{{name}[...]}}, or beside this script"
        elif not (len(scopes) == 1 and scopes <= {self.current_function(), None}):
            why = "assigned nowhere, or outside the run's function, or in more than one scope"
        elif assigned[0][0] or not all(e[0] for e in assigned[1:]) or len(assigned) - 1 > MAX_APPENDS:
            why = f"not one assignment and at most {MAX_APPENDS} appends"
        elif any(e[2] for e in assigned):
            why = "assigned or appended in a loop"
        elif not assigned[0][3] or assigned[0][4] > w.pos:
            why = ("assigned where it may not run before the run: not a statement of its own at the top of its "
                   "scope, or later in the text")
        if why is not None:
            self.s.refusals.append((line, f"this reader does not follow {w.raw} run as a command, since it is {why}"))
            return
        first, appends = assigned[0][1], [e[1] for e in assigned[1:]]
        for mask in range(1 << len(appends)):
            run = Command()
            run.words = first + [e for i, elements in enumerate(appends) if mask >> i & 1 for e in elements] + rest
            self.resolve(run, external, expanded | {name})

    def lookup(self, word):
        if word.dynamic:
            return
        name = os.path.basename(word.value) if os.path.dirname(word.value) in BIN_DIRS else word.value
        if "/" in name or name in BUILTINS or name in KEYWORDS:
            return
        if name in self.s.functions or name in self.functions:
            return
        self.s.refs.append(("lookup", name, self.s.line(word.pos), word.pos))

    def own_skip(self, grammar, rest):
        j = 0
        for part in grammar:
            if part == "NAME=":
                while j < len(rest) and ASSIGNMENT.match(rest[j].value):
                    j += 1
            elif part == "ARG":
                j += 1
            else:
                flag, *arg = part.split()
                if j < len(rest) and rest[j].value == flag:
                    j += 1 + len(arg)
        return j

    def wrapped(self, name, rest):
        """The index in REST of the command NAME runs, or None when NAME runs
        none this reader can name (code strings it reads itself)."""
        def options(with_arg, positionals=0, code=()):
            j = 0
            while j < len(rest):
                v = rest[j].value
                if v == "--":
                    j += 1
                    break
                if v.startswith("--") and v.split("=", 1)[0] in code and "=" in v:
                    self.code_string(self.past(rest[j], rest[j].raw.index("=") + 1))
                    return None
                if v.startswith("-") and v != "-":
                    if v.startswith("--") or len(v) == 2:
                        taker = v if v in with_arg or v in code else None
                        last = True
                    else:
                        # **A group** (`-iu USER`): its first letter that takes an
                        # argument takes the rest of the group, or the next word.
                        at = next((i for i, c in enumerate(v[1:]) if f"-{c}" in with_arg or f"-{c}" in code), None)
                        taker = None if at is None else f"-{v[1 + at]}"
                        last = at == len(v) - 2
                    if taker in code:
                        if not last or j + 1 >= len(rest):
                            raise Refused(f"{name} {v} with its code string joined or missing")
                        self.code_string(rest[j + 1])
                        return None
                    j += 2 if taker is not None and last else 1
                    continue
                if name in ("env", "sudo") and ASSIGNMENT.match(rest[j].value):
                    j += 1
                    continue
                break
            if name == "env" and j < len(rest) and rest[j].value == "-":
                j += 1
            j += positionals
            return j if j < len(rest) else None

        if name in ("sh", "bash", "dash"):
            # As bash reads them: -c and -s are flags, each o or O in a group
            # takes the next word, and the code string is the first word after
            # the options.
            j, code, stdin = 0, False, False
            while j < len(rest):
                v = rest[j].value
                if v in ("--", "-"):
                    j += 1
                    break
                if v in ("--rcfile", "--init-file"):
                    j += 2
                elif v.startswith("--"):
                    j += 1
                elif len(v) > 1 and v[0] in "-+":
                    code = code or "c" in v[1:]
                    stdin = stdin or "s" in v[1:]
                    j += 1 + v.count("o") + v.count("O")
                else:
                    break
            if code:
                if j >= len(rest):
                    raise Refused(f"{name} -c with no code string")
                self.code_string(rest[j])
                return None
            if j < len(rest) and not stdin:
                return None  # a script, run by its path
            # **The shell reads its standard input**: its own heredoc, read
            # now or when its body comes, or nothing this reader can follow.
            if self.cur is not None and self.cur.heredocs:
                for start, end, _ in self.cur.heredocs:
                    Reader(self.s, start, end, self.scope, self.functions, self.current_function(),
                           arrays=self.arrays, depth=self.depth + 1, loops=self.loops, fresh=True).read()
                return None
            if self.cur is not None and any(c is self.cur for c, _, _, _ in self.heredocs):
                self.cur.shell_reads = True
                return None
            raise Refused(f"{name} reading a standard input that is not its own heredoc")
        if name == "sudo":
            return options({"-u", "-g", "-C", "-D", "-h", "-p", "-R", "-r", "-t", "-T", "-U", "--user", "--group",
                            "--close-from", "--chdir", "--host", "--prompt", "--chroot", "--role", "--type",
                            "--command-timeout", "--other-user"})
        if name == "runuser":
            # As getopt reads it: -c, --command and --session-command hand the
            # shell a code string, apart or joined by =. In a group, the first
            # letter that takes an argument takes the rest of it.
            code = ("-c", "--command", "--session-command")

            def taker(v):
                if not re.fullmatch(r"-[A-Za-z][^=]*", v) or v.startswith("--"):
                    return None
                return next((c for c in v[1:] if c in "cgGsuw"), None)

            if any(a.value in ("-u", "--user") or a.value.startswith("--user=") or taker(a.value) == "u"
                   for a in rest):
                return options({"-u", "-g", "-G", "-s", "-w", "--user", "--group", "--supp-group",
                                "--shell", "--whitelist-environment"}, code=code)
            for j, a in enumerate(rest):
                v = a.value
                if v in code or taker(v) == "c" and v.endswith("c"):
                    if j + 1 >= len(rest):
                        raise Refused(f"runuser {v} with its code string joined or missing")
                    self.code_string(rest[j + 1])
                    return None
                if "=" in v and v.split("=", 1)[0] in code[1:]:
                    self.code_string(self.past(a, a.raw.index("=") + 1))
                    return None
                if taker(v) == "c":
                    raise Refused(f"runuser {v} with its code string joined or missing")
            return None
        if name == "env":
            if any((a.value.startswith("-S") and a.value != "-S") or a.value.startswith("--split-string=")
                   for a in rest):
                raise Refused("env -S joined to its string")
            return options({"-u", "-C", "--unset", "--chdir"}, code=("-S", "--split-string"))
        if name == "timeout":
            return options({"-k", "-s", "--kill-after", "--signal"}, positionals=1)
        if name in ("setsid", "nohup"):
            return options(set())
        if name == "time":
            # The program (GNU time), never the keyword.
            return options({"-f", "-o", "--format", "--output"})
        here = rest[0].pos if rest else self.i

        def refuse(why):
            self.s.refusals.append((self.s.line(here), f"this reader does not follow {why}"))
            return None

        if name == "sort" and any(a.value.startswith("--compress-program") for a in rest):
            return refuse("sort --compress-program, which runs a program")
        if name == "install" and any(a.value.startswith("--strip-program") for a in rest):
            return refuse("install --strip-program, which runs a program")
        if name == "visudo" and not any(a.value == "--check" or re.fullmatch(r"-[A-Za-z]*c[A-Za-z]*", a.value)
                                        for a in rest):
            return refuse("visudo without -c, which opens an editor")
        if name == "pacman" and not (rest and re.match(r"-Q|--query", rest[0].value)):
            return refuse("pacman past a query (-Q), which runs hooks")
        if name == "awk":
            # Only -F and -v, apart or joined: gawk's other options can hand it
            # program text or a file of it (-f, -e, -E, -i, --source, ...).
            j = 0
            while j < len(rest) and rest[j].value.startswith("-") and rest[j].value != "--":
                if not re.match(r"-[Fv]", rest[j].value):
                    return refuse(f"awk {rest[j].value.split('=')[0]}, past -F and -v, which can hand it a program "
                                  "this reader does not read")
                j += 2 if rest[j].value in ("-F", "-v") else 1
            j += j < len(rest) and rest[j].value == "--"
            if j < len(rest) and re.search(r"system|(?<!\|)\|(?!\|)", rest[j].value):
                return refuse("awk with system() or a pipe in its program, which runs a program")
            return None
        if name == "sed":
            scripts, j = [], 0
            while j < len(rest) and rest[j].value.startswith("-") and rest[j].value != "--":
                v = rest[j].value
                if v.startswith("-f") or v.startswith("--file"):
                    return refuse("sed -f, whose script this reader does not read")
                if v in ("-e", "--expression"):
                    scripts.append(rest[j + 1].value if j + 1 < len(rest) else "")
                    j += 2
                    continue
                if v.startswith("--expression="):
                    scripts.append(v.split("=", 1)[1])
                elif v.startswith("-e"):
                    scripts.append(v[2:])
                j += 1
            if not scripts and j < len(rest):
                scripts.append(rest[j].value)
            addr = r"(?:\d+|\$|/(?:\\.|[^/\\])*/)"
            plain = re.compile(r"(?:" + addr + r"(?:," + addr + r")?)?[pdq]"
                               r"|(?:" + addr + r"(?:," + addr + r")?)?s(?P<d>[^\\\n])(?:\\.|(?!(?P=d)).)*(?P=d)"
                               r"(?:\\.|(?!(?P=d)).)*(?P=d)[gpI0-9]*")
            if not all(plain.fullmatch(x) for x in scripts):
                return refuse("sed past its s, p, d and q commands, whose e runs a program")
            return None
        if name == "nice":
            return options({"-n", "--adjustment"})
        if name == "stdbuf":
            return options({"-i", "-o", "-e", "--input", "--output", "--error"})
        if name == "ionice":
            return options({"-c", "-n", "-p", "-P", "-u", "--class", "--classdata", "--pid", "--pgid", "--uid"})
        if name == "flock":
            # flock [options] FILE -c CODE, or flock [options] FILE COMMAND...
            j = options({"-w", "-E", "--wait", "--timeout", "--conflict-exit-code"}, positionals=1,
                        code=("-c", "--command"))
            if j is not None and rest[j].value in ("-c", "--command"):
                if j + 1 >= len(rest):
                    raise Refused("flock -c with no code string")
                self.code_string(rest[j + 1])
                return None
            if j is not None and rest[j].value.startswith("--command="):
                self.code_string(self.past(rest[j], rest[j].raw.index("=") + 1))
                return None
            return j
        if name == "xargs":
            return options({"-n", "-I", "-P", "-d", "-L", "-s", "-a", "-E", "--max-args", "--max-procs",
                            "--delimiter", "--max-lines", "--max-chars", "--arg-file", "--eof"})
        if name == "find":
            # Every -exec, -execdir, -ok and -okdir, each to its ; or +.
            j = 0
            while j < len(rest):
                if rest[j].value in ("-exec", "-execdir", "-ok", "-okdir"):
                    end = next((e for e in range(j + 1, len(rest)) if rest[e].value in (";", "+")), len(rest))
                    run = Command()
                    run.words = rest[j + 1:end]
                    self.resolve(run, external=True)
                    j = end
                j += 1
            return None
        if name == "tar":
            # **tar runs -I and --to-command through a shell**: each is a code
            # string. Its other ways to run a command are refused.
            for j, a in enumerate(rest):
                v = a.value
                if v in ("-I", "--use-compress-program", "--to-command") and j + 1 < len(rest):
                    self.code_string(rest[j + 1])
                elif v.startswith(("--use-compress-program=", "--to-command=")):
                    self.code_string(self.past(a, a.raw.index("=") + 1))
                elif v.startswith("-I"):
                    self.code_string(self.past(a, 2))
                elif v == "-F" or v.startswith(("--checkpoint-action", "--info-script", "--new-volume-script",
                                                "--rsh-command", "--rmt-command")):
                    refuse(f"tar {v.split('=')[0]}, which runs a command")
            return None
        return None


def beside(path, sourced):
    """The text of every other shell file beside PATH, and of each file it
    sources by name, for the array fence. Read at each use, since a file may
    change between two checks in one process."""
    here = os.path.dirname(os.path.abspath(path))
    names = {n for n in os.listdir(here) if n.endswith(".sh")} | set(sourced)
    texts = []
    for n in sorted(names - {os.path.basename(path)}):
        try:
            with open(os.path.join(here, n)) as f:
                texts.append(f.read())
        except OSError:
            pass
    return "\n".join(texts)


HERE = r"(?:BASH_SOURCE|\$0\b|\$\{0[^A-Za-z0-9_]|__file__)"
SIBLING_NAME = r"[/\"']([A-Za-z0-9_.+-]+\.(?:sh|py))\b"


def siblings(path):
    """**The files a script reaches beside itself** (#607): each name a line
    outside comments joins to the script's own directory, by BASH_SOURCE or
    $0 in shell and __file__ in Python, or by a name assigned from one
    (`HERE=$(dirname "$0")`, `here = Path(__file__).parent`), after a / or a
    quote (`"$HERE/x.sh"`, `here / "x.py"`, `os.path.join(here, "x.py")`)."""
    with open(path) as f:
        lines = [line for line in f.read().split("\n") if not line.lstrip().startswith("#")]
    anchors = [HERE]
    while True:
        anchor = "(?:" + "|".join(anchors) + ")"
        named = {m.group(1) for line in lines
                 for m in [re.match(r"\s*(?:(?:local|readonly|declare|export)\s+(?:-\S+\s+)*)?"
                                    r"([A-Za-z_][A-Za-z0-9_]*)\s*=(.*)", line)]
                 if m and re.search(anchor, m.group(2))}
        more = [r"\$\{?" + n + r"\b" if not str(path).endswith(".py") else r"\b" + n + r"\b" for n in sorted(named)]
        if len(more) + 1 == len(anchors):
            break
        anchors = [HERE] + more
    found = re.compile(anchor + r"[^\n]*?" + SIBLING_NAME)
    return sorted({n for line in lines for n in found.findall(line)} - {os.path.basename(path)})


def read_names(path):
    with open(path) as f:
        return [n.strip() for n in f if n.strip() and not n.lstrip().startswith("#")]


def read_script(path, scope=None, scope_functions=frozenset(), arrays=None):
    with open(path) as f:
        text = f.read()
    script = Script(path, text)
    reader = Reader(script, 0, len(text), scope or {}, frozenset(scope_functions), arrays=arrays)
    try:
        reader.read()
    except Refused as e:
        script.refusals.append((script.line(reader.i), f"this reader does not follow {e}"))
    except (IndexError, RecursionError) as e:  # pragma: no cover - a defect of the reader
        script.refusals.append((script.line(reader.i), f"this reader failed: {e!r}"))
    return script


def sourced_functions(path, seen=None):
    """The functions PATH and the siblings it sources define."""
    seen = set() if seen is None else seen
    if path in seen or not os.path.isfile(path):
        return set()
    seen.add(path)
    first = read_script(path)
    funcs = set(first.functions)
    for name, _ in first.sources:
        funcs |= sourced_functions(os.path.join(os.path.dirname(path), name), seen)
    return funcs


def sourcer_functions(paths):
    """**A helper runs inside its sourcer**: for each shell file in PATHS, the
    functions in scope where it runs, its own and its siblings' and those of
    every file in PATHS that sources it."""
    own = {p: sourced_functions(p) for p in paths}
    sources = {p: {os.path.join(os.path.dirname(p), n) for n, _ in read_script(p).sources} for p in paths}
    scope = {p: set(f) for p, f in own.items()}
    changed = True
    while changed:
        changed = False
        for g in paths:
            for f in sources[g]:
                f = next((q for q in paths if os.path.samefile(q, f)), None) if os.path.exists(f) else None
                if f is not None and not scope[g] <= scope[f]:
                    scope[f] |= scope[g]
                    changed = True
    return scope


def check_shell(path, programs, funcs=None):
    """The findings for one shell script, as (line, word, why)."""
    funcs = sourced_functions(path) if funcs is None else funcs
    scope = {f: g for f, g in OWN_WRAPPERS.items() if f in funcs}
    script = read_script(path, scope, funcs, arrays=read_script(path, scope, funcs).arrays)
    findings = [(line, "-", why) for line, why in script.refusals]
    for m in re.finditer(CODE_VARIABLES, script.text):
        findings.append((script.line(m.start()), "-", f"this reader does not follow a variable bash runs as code: "
                                                      f"{m.group(0)}"))
    for fn in sorted(set(script.runs_args) - set(OWN_WRAPPERS)):
        findings.append((script.runs_args[fn], fn, "a function that runs its arguments as a command, "
                                                    "with no entry in OWN_WRAPPERS"))
    allowed = [(p, guard) for s, p, guard, _ in GUARDED if s == script.name]
    used = set()
    lines = script.text.split("\n")

    def inside(guard, line, pos):
        at = pos - script.lines[line - 1]
        return any(m.start() <= at < m.end() for m in re.finditer(re.escape(guard), lines[line - 1]))

    dynamic_used = set()
    for kind, word, line, pos in script.refs:
        if kind == "handed" or kind == "lookup" and word in programs:
            continue
        if kind == "dynamic":
            if (script.name, word) in DYNAMIC:
                dynamic_used.add(word)
            else:
                findings.append((line, word, "this reader cannot name the program this runs"))
            continue
        if word in programs:
            if word not in LEAF_NAMES and word not in WRAPPERS and word not in UNREAD_RUNNERS:
                findings.append((line, word, "this reader does not know whether it runs a command it is handed"))
            continue
        guards = [g for p, g in allowed if p == word]
        if guards and any(inside(g, line, pos) for g in guards):
            used.add(word)
            continue
        what = "looked up" if kind == "lookup" else "called"
        why = f"{what} here, and not a program the image carries"
        if guards:
            why += f", and this use lies outside its guard {guards[0]!r}"
        findings.append((line, word, why))
    for n, why_, word in python_runs(lines):
        findings.append((n, word, why_))
    for p, guard in allowed:
        if p not in used and not any(f[1] == p for f in findings):
            findings.append((0, p, f"GUARDED allows it behind {guard!r}, and no line of the script uses that"))
    for s_, word in DYNAMIC:
        if s_ == script.name and word not in dynamic_used:
            findings.append((0, word, "DYNAMIC allows it, and no line of the script runs that"))
    return script, sorted(set(findings))


def python_runs(lines):
    """(line, why, word) for each line outside comments that names a way
    Python starts a program, in a Python file or the Python a shell script
    carries (`python3 -c`, a heredoc), which this checker does not read."""
    return [(n, "starts a program, which this checker does not read in Python", PYTHON_RUNS.search(line).group(0))
            for n, line in enumerate(lines, 1) if not line.lstrip().startswith("#") and PYTHON_RUNS.search(line)]


def check_python(path):
    with open(path) as f:
        return [(n, word, why) for n, why, word in python_runs(f.read().split("\n"))]


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--programs", required=True)
    ap.add_argument("--copy")
    ap.add_argument("--dynamic", action="store_true")
    ap.add_argument("files", nargs="*")
    args = ap.parse_args(argv)
    try:
        programs = set(read_names(args.programs))
        files = list(args.files)
        if args.copy:
            here = os.path.dirname(os.path.abspath(args.copy))
            files += [os.path.join(here, n) for n in read_names(args.copy)]
        if not files:
            ap.error("name the files to check, or --copy")
        bad = 0
        scopes = sourcer_functions([p for p in files if not p.endswith(".py")])
        for path in files:
            if path.endswith(".py"):
                findings = check_python(path)
                script = None
            else:
                script, findings = check_shell(path, programs, scopes[path])
            for line, word, why in findings:
                todo = remedy(why)
                print(f"{path}:{line}: {word}: {why}" + (f" (to pass: {todo})" if todo else ""))
                bad += 1
            if args.dynamic and script is not None:
                for kind, word, line, _ in script.refs:
                    if kind in ("dynamic", "handed"):
                        print(f"{path}:{line}: {kind}: {word}")
    except OSError as e:
        print(f"program_check: {e}", file=sys.stderr)
        return 2
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
