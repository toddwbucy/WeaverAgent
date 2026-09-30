"""Builds python-spu as one file, per python-spu-Spec section 2.

The file is a zip archive of src/python_spu behind a first line naming the pinned
interpreter by absolute path, so the digest admin takes of it covers every line of
python-spu's own code and fixes which interpreter runs it. The classifier stays out,
section 1 scoping the classify role out of this implementation. Entries are sorted,
dated 1980-01-01 with fixed modes and stored uncompressed, so one tree always builds one
file and the digest names the code rather than the moment of the build or the zlib of
the interpreter that built it, deflate's output being the compressor's and not the
format's.

**A build is never of nothing.** The package must be a directory, every error the walk
meets is raised rather than skipped, and the files the process cannot start without
must be among those found, so a missing or unreadable source exits non-zero instead of
writing an archive that holds only its entry point.

Usage: python3 scripts/build_zipapp.py [--interpreter PATH] [--output PATH]
"""
import argparse
import io
import os
import stat
import sys
import zipfile
from pathlib import Path

INTERPRETER = "/opt/weaver/python-spu/bin/python3.14"
LEFT_OUT = {"classifier.py"}
# The files the process cannot start without: every module the serving entry point
# imports, transitively, which the suite derives from the source and holds within
# these, and the two import-set halves, import_set.HALVES, held within them likewise.
REQUIRED = ("__init__.py", "server.py", "transport.py", "wire.py", "session.py",
            "family.py", "sampling.py", "candle_chain.py", "engine.py", "import_set.py",
            "loaded_code.py", "imports-cpu.txt", "imports-cuda.txt")
MAIN = "import sys\nfrom python_spu.server import main\nsys.exit(main())\n"


def entry(name):
    info = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
    info.external_attr = 0o644 << 16
    info.compress_type = zipfile.ZIP_STORED
    return info


def _raise(error):
    raise error


def build(source, output, interpreter=INTERPRETER):
    package = Path(source) / "python_spu"
    if not stat.S_ISDIR(os.lstat(package).st_mode):
        raise ValueError(f"{package} is not a directory")
    found = []
    for directory, subdirectories, names in os.walk(package, onerror=_raise):
        subdirectories[:] = [d for d in subdirectories if d != "__pycache__"]
        found.extend(Path(directory) / name for name in names)
    files = sorted(p for p in found if p.is_file() and p.name not in LEFT_OUT)
    missing = [name for name in REQUIRED if package / name not in files]
    if missing:
        raise ValueError(f"{package} lacks {', '.join(missing)}")
    buffer = io.BytesIO()
    with zipfile.ZipFile(buffer, "w") as archive:
        for path in files:
            archive.writestr(entry(path.relative_to(source).as_posix()), path.read_bytes())
        archive.writestr(entry("__main__.py"), MAIN)
    output = Path(output)
    output.write_bytes(b"#!" + interpreter.encode() + b"\n" + buffer.getvalue())
    os.chmod(output, 0o755)
    return output


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--interpreter", default=INTERPRETER)
    parser.add_argument("--output", default="python-spu.pyz")
    args = parser.parse_args(argv)
    root = Path(__file__).resolve().parents[1]
    try:
        print(build(root / "src", args.output, args.interpreter))
    except (OSError, ValueError) as error:
        print(f"build_zipapp: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
