"""The code the process has mapped, per python-spu-Spec section 8.

**python-spu compiles and loads no code at runtime.** Every file the process maps
executable must be the pinned interpreter's prefix, which the tree digest covers,
or one of the few system objects the interpreter and torch link against: glibc's
family, the C++ runtime, zlib, and the NVIDIA driver's own libraries. An object
anywhere else, a temporary directory, a cache or a deleted file, is code nothing
locked or digested, and it faults the process at the same two stages the import set
is judged at.

Only file-backed executable mappings are judged. A data mapping, such as the
model's safetensors, carries no code, and an anonymous executable mapping, such as a
ctypes trampoline or the driver's own JIT, names no file to judge.
"""
import os
import re
import sys

# The directories the system's own libraries live in, and the names admitted from
# them. A name outside this list in these directories is foreign like any other.
SYSTEM_DIRECTORIES = ("/usr/lib", "/usr/lib64", "/lib", "/lib64",
                      "/usr/lib/x86_64-linux-gnu", "/lib/x86_64-linux-gnu")
SYSTEM_NAMES = re.compile(
    r"(ld-linux-x86-64\.so\.2|libc\.so\.6|libm\.so\.6|libdl\.so\.2|libpthread\.so\.0"
    r"|librt\.so\.1|libutil\.so\.1|libgcc_s\.so\.1|libstdc\+\+\.so\.6(\.[0-9.]+)?"
    r"|libz\.so\.1(\.[0-9A-Za-z.-]+)?|libcuda\.so(\.[0-9.]+)?"
    r"|libnvidia-[a-z0-9-]+\.so(\.[0-9.]+)?)")


def prefixes():
    """The roots whose code is the environment's own: the interpreter's prefix,
    and a venv's where one stands over it."""
    return tuple(sorted({os.path.realpath(sys.base_prefix), os.path.realpath(sys.prefix)}))


def foreign(maps_text, roots):
    """The code objects a maps listing holds outside the roots and the system
    allowance, sorted. A deleted file's mapping is foreign whatever its path."""
    found = set()
    for line in maps_text.splitlines():
        fields = line.split(maxsplit=5)
        if len(fields) < 6 or "x" not in fields[1]:
            continue
        path = fields[5]
        if not path.startswith("/"):
            continue
        if path.endswith(" (deleted)") or path.startswith("/memfd:"):
            found.add(path)
            continue
        if any(path.startswith(root.rstrip("/") + "/") for root in roots):
            continue
        directory, name = os.path.split(path)
        if directory in SYSTEM_DIRECTORIES and SYSTEM_NAMES.fullmatch(name):
            continue
        found.add(path)
    return sorted(found)


def foreign_now():
    with open("/proc/self/maps") as fh:
        return foreign(fh.read(), prefixes())
