#!/usr/bin/env python3
"""The carry set's manifest (weaver-admin-PRD section 7.2).

    carry_set.py write SET COMMIT              write SET/MANIFEST for the set laid out in SET
    carry_set.py check SET SHA256              SET is the set SHA256 names; print its commit
    carry_set.py rows SET SHA256               print the rows of the manifest SHA256 names
    carry_set.py tuple SET PREFIX REPORT SHA256
                                               judge the stack placed under PREFIX and print
                                               its install tuple

A carry set is a directory holding `build-tuple` and the three directories
`bin/`, `lib/` and `deploy/`, each holding files and links only. Its manifest
names every entry: one row per entry, tab-separated, sorted by path in C
order, after two header lines.

    # weaver carry set 1
    # commit <40 hex>
    file<TAB><sha256><TAB><0755|0644><TAB><path>
    link<TAB><target><TAB>-<TAB><path>

A path is relative, printable ASCII with no blank, and lies directly under one
of the three directories, or is `build-tuple`. A file's mode is 0755 or 0644.
A link's target is one name in the same directory, which the set holds. The
manifest's sha256 is the set's identity. Writing refuses, naming the entry,
where the set holds anything these rules do not admit, so a manifest is never
written that its reader would refuse. Every refusal exits 1 with one line on
standard error.

**Checking a set** (`check`) reads the manifest, refuses unless its sha256 is
the one the operator names, and refuses unless the set holds exactly the
entries it names, each file with its digest and mode and each link with its
target: the writer's rows, read again from the set, must equal the
manifest's. `rows` prints the manifest's rows once its sha256 is checked, so
a caller places what the named manifest says, read once.

**The install tuple** (`tuple`, weaver-admin-PRD section 7.2): with the stack
placed under PREFIX, each placed file must equal its manifest row by digest
and mode and each placed link its target, the build half's `member.` and
`library.` digests must equal the rows of the files they name, and REPORT,
ldd's output for PREFIX/bin/weaver-spu with PREFIX/lib alone as the library
path, must name no library and no symbol version not found, the driver's
own included, resolve every engine library to a file in PREFIX/lib, and
resolve each runtime file the build half names (`build-runtime.<name>`) to a
file in PREFIX/lib whose sha256 is the build's. It prints the build half as
carried, then a `runtime.<name> = <sha256> <file>` line for each runtime
file as it resolves here, then `carry = <manifest sha256>`.
"""

import hashlib
import os
import re
import stat
import sys

HEADER = "# weaver carry set 1"
DIRECTORIES = ("bin", "lib", "deploy")
TUPLE = "build-tuple"
MODES = {0o755: "0755", 0o644: "0644"}
NAME = re.compile(r"[\x21-\x7e]+")
COMMIT = re.compile(r"[0-9a-f]{40}")
HEX64 = re.compile(r"[0-9a-f]{64}")
MANIFEST_CAP = 1 << 20
TUPLE_CAP = 64 * 1024


class Refused(Exception):
    pass


def digest(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def plain_name(name):
    return bool(NAME.fullmatch(name)) and "/" not in name and name not in (".", "..")


def entry_row(root, path):
    """The manifest row of the entry at PATH, relative to ROOT."""
    full = os.path.join(root, path)
    st = os.lstat(full)
    if stat.S_ISLNK(st.st_mode):
        target = os.readlink(full)
        if not plain_name(target):
            raise Refused(f"{path} is a link to {target!r}, not to one name in its own directory")
        if not os.path.lexists(os.path.join(os.path.dirname(full), target)):
            raise Refused(f"{path} is a link to {target}, which the set does not hold")
        return f"link\t{target}\t-\t{path}"
    if not stat.S_ISREG(st.st_mode):
        raise Refused(f"{path} is neither a file nor a link")
    mode = MODES.get(stat.S_IMODE(st.st_mode))
    if mode is None:
        raise Refused(f"{path} has mode {stat.S_IMODE(st.st_mode):04o}, not 0755 or 0644")
    return f"file\t{digest(full)}\t{mode}\t{path}"


def rows(root, manifest=False):
    """Every row of the set at ROOT, sorted by path in C order; the manifest
    itself passed over where MANIFEST is true."""
    found = []
    for name in sorted(os.listdir(root)):
        full = os.path.join(root, name)
        if manifest and name == "MANIFEST":
            continue
        if name == TUPLE:
            if not stat.S_ISREG(os.lstat(full).st_mode):
                raise Refused(f"{TUPLE} is not a file")
            found.append(TUPLE)
        elif name in DIRECTORIES and stat.S_ISDIR(os.lstat(full).st_mode):
            for entry in sorted(os.listdir(full)):
                if not plain_name(entry):
                    raise Refused(f"{name}/{entry!r} is not a name of printable ASCII with no blank")
                found.append(f"{name}/{entry}")
        else:
            raise Refused(f"{name} is not {TUPLE} or one of the directories {', '.join(DIRECTORIES)}")
    if TUPLE not in found:
        raise Refused(f"the set holds no {TUPLE}")
    for name in DIRECTORIES:
        if not os.path.isdir(os.path.join(root, name)):
            raise Refused(f"the set holds no {name} directory")
    found.sort(key=lambda p: p.encode("ascii"))
    return [entry_row(root, path) for path in found]


def write(root, commit):
    if not COMMIT.fullmatch(commit):
        raise Refused(f"the commit {commit!r} is not 40 hex digits")
    if os.path.lexists(os.path.join(root, "MANIFEST")):
        raise Refused("the set already holds a MANIFEST")
    body = [HEADER, f"# commit {commit}", *rows(root)]
    fd = os.open(os.path.join(root, "MANIFEST"), os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o644)
    with os.fdopen(fd, "w", encoding="ascii") as f:
        f.write("\n".join(body) + "\n")
    os.chmod(os.path.join(root, "MANIFEST"), 0o644)


def read_bounded(path, cap, what):
    """The bytes of the regular file PATH, never through a link, refused past
    CAP octets."""
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK | os.O_CLOEXEC)
    with os.fdopen(fd, "rb") as f:
        if not stat.S_ISREG(os.fstat(f.fileno()).st_mode):
            raise Refused(f"{what} is not a regular file")
        raw = f.read(cap + 1)
    if len(raw) > cap:
        raise Refused(f"{what} is past {cap} octets")
    return raw


def read_manifest(root, sha):
    """The commit and rows of ROOT's manifest, refused unless its sha256 is
    SHA and every line keeps the format."""
    if not HEX64.fullmatch(sha):
        raise Refused(f"the manifest sha256 {sha!r} is not 64 lowercase hex digits")
    raw = read_bounded(os.path.join(root, "MANIFEST"), MANIFEST_CAP, "the manifest")
    found = hashlib.sha256(raw).hexdigest()
    if found != sha:
        raise Refused(f"the manifest's sha256 is {found}, not the {sha} named, so this is not that set")
    try:
        lines = raw.decode("ascii").split("\n")
    except UnicodeDecodeError:
        raise Refused("the manifest is not ASCII") from None
    if len(lines) < 4 or lines[-1] != "" or lines[0] != HEADER or not lines[1].startswith("# commit "):
        raise Refused("the manifest does not open with its two header lines and end with a newline")
    commit = lines[1][len("# commit "):]
    if not COMMIT.fullmatch(commit):
        raise Refused(f"the manifest's commit {commit!r} is not 40 hex digits")
    body = lines[2:-1]
    for number, line in enumerate(body, start=3):
        fields = line.split("\t")
        ok = len(fields) == 4 and (
            (fields[0] == "file" and HEX64.fullmatch(fields[1]) and fields[2] in MODES.values())
            or (fields[0] == "link" and plain_name(fields[1]) and fields[2] == "-"))
        path = fields[3] if len(fields) == 4 else ""
        top, _, rest = path.partition("/")
        if not ok or not (path == TUPLE or (top in DIRECTORIES and plain_name(rest))):
            raise Refused(f"the manifest's line {number} is not a row of the format")
    paths = [line.split("\t")[3] for line in body]
    if paths != sorted(set(paths), key=lambda p: p.encode("ascii")):
        raise Refused("the manifest's rows are not sorted by path in C order, each path once")
    if TUPLE not in paths:
        raise Refused(f"the manifest names no {TUPLE}")
    return commit, body


def check(root, sha):
    commit, named = read_manifest(root, sha)
    held = rows(root, manifest=True)
    if held != named:
        by_path = lambda rs: {r.split("\t")[3]: r for r in rs}
        want, have = by_path(named), by_path(held)
        for path in sorted(set(want) | set(have), key=lambda p: p.encode("ascii")):
            if path not in have:
                raise Refused(f"{path} is named by the manifest and the set does not hold it")
            if path not in want:
                raise Refused(f"{path} is in the set and the manifest does not name it")
            if want[path] != have[path]:
                raise Refused(f"{path} is not as the manifest names it")
    return commit


def tuple_lines(raw):
    """The build half's `key = value` lines, in order."""
    try:
        text = raw.decode("ascii")
    except UnicodeDecodeError:
        raise Refused(f"{TUPLE} is not ASCII") from None
    lines = []
    for number, line in enumerate(text.splitlines(), start=1):
        key, sep, value = line.partition(" = ")
        if not sep or not key or not value or not line.isprintable():
            raise Refused(f"{TUPLE}'s line {number} is not `key = value`")
        lines.append((key, value))
    if len({k for k, _ in lines}) != len(lines):
        raise Refused(f"{TUPLE} names a key twice")
    return text, lines


def judge_placed(prefix, named):
    """Each placed entry under PREFIX equal to its manifest row."""
    for row in named:
        kind, value, mode, path = row.split("\t")
        if path == TUPLE:
            continue
        placed = os.path.join(prefix, path)
        try:
            st = os.lstat(placed)
        except OSError:
            raise Refused(f"{placed} was not placed") from None
        if kind == "link":
            if not stat.S_ISLNK(st.st_mode) or os.readlink(placed) != value:
                raise Refused(f"{placed} is not the link to {value} the manifest names")
        elif not stat.S_ISREG(st.st_mode) or MODES.get(stat.S_IMODE(st.st_mode)) != mode or digest(placed) != value:
            raise Refused(f"{placed} is not the file the manifest names")


def install_tuple(root, prefix, report, sha):
    commit, named = read_manifest(root, sha)
    rows_by_path = {r.split("\t")[3]: r.split("\t") for r in named}
    raw = read_bounded(os.path.join(root, TUPLE), TUPLE_CAP, TUPLE)
    if hashlib.sha256(raw).hexdigest() != rows_by_path[TUPLE][1]:
        raise Refused(f"{TUPLE} is not the file the manifest names")
    text, lines = tuple_lines(raw)
    if not text.endswith("\n"):
        raise Refused(f"{TUPLE} does not end with a newline")
    if any(k.startswith("runtime.") or k == "carry" for k, _ in lines):
        raise Refused(f"{TUPLE} carries a key the install writes, runtime. or carry")
    facts = dict(lines)
    if facts.get("commit") != commit:
        raise Refused(f"{TUPLE}'s commit is not the manifest's {commit}")
    judge_placed(prefix, named)
    for key, value in lines:
        for head, directory in (("member.", "bin"), ("library.", "lib")):
            if key.startswith(head):
                row = rows_by_path.get(f"{directory}/{key[len(head):]}")
                if row is None or row[0] != "file" or row[1] != value:
                    raise Refused(f"{TUPLE}'s {key} is not the digest of the file the manifest names")
    runtime = [(k[len("build-runtime."):], v) for k, v in lines if k.startswith("build-runtime.")]
    if not runtime:
        raise Refused(f"{TUPLE} names no build-runtime file")
    lib = os.path.realpath(os.path.join(prefix, "lib"))
    resolved_re = re.compile(r"\s*(\S+)\s+=>\s+(.+)\s+\(0x[0-9a-fA-F]+\)\s*")
    version = re.compile(r":\s+version\s+[`'][^`']*'\s+not found\s+\(required by\s")
    bad, engine, found = [], 0, {}
    for line in report.splitlines():
        name = (line.split() or [""])[0]
        if re.search(r"=>\s*not found\s*$", line) or version.search(line):
            bad.append(line.strip())
            continue
        m = resolved_re.fullmatch(line)
        if re.match(r"lib(ggml|llama)", name):
            engine += 1
            if not m or os.path.dirname(os.path.realpath(m.group(2))) != lib:
                bad.append(line.strip())
            continue
        for key, _ in runtime:
            want = f"lib{key}.so"
            if m and key not in found and (name.lower() == want or name.lower().startswith(want + ".")):
                found[key] = os.path.realpath(m.group(2))
    if not engine:
        bad.append("no engine library is named")
    out = []
    for key, build in runtime:
        file = found.get(key)
        if file is None:
            bad.append(f"lib{key}.so is not named")
        elif os.path.dirname(file) != lib:
            bad.append(f"lib{key}.so resolves to {file}, outside {lib}")
        else:
            live = digest(file)
            if live != build:
                bad.append(f"lib{key}.so resolves to {file}, whose sha256 {live} is not the build's {build}")
            out.append(f"runtime.{key} = {live} {file}")
    if bad:
        raise Refused("the placed weaver-spu does not resolve as the set carries it: " + "; ".join(bad))
    return text + "\n".join(out) + f"\ncarry = {sha}\n"


def main(argv):
    if len(argv) == 4 and argv[1] == "write":
        write(argv[2], argv[3])
    elif len(argv) == 4 and argv[1] == "check":
        print(check(argv[2], argv[3]))
    elif len(argv) == 4 and argv[1] == "rows":
        print("\n".join(read_manifest(argv[2], argv[3])[1]))
    elif len(argv) == 6 and argv[1] == "tuple":
        sys.stdout.write(install_tuple(argv[2], argv[3], argv[4], argv[5]))
    else:
        print(f"usage: {argv[0]} write SET COMMIT | check SET SHA256 | rows SET SHA256 | "
              "tuple SET PREFIX REPORT SHA256", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main(sys.argv))
    except (Refused, OSError) as e:
        print(f"carry_set.py: {e}", file=sys.stderr)
        sys.exit(1)
