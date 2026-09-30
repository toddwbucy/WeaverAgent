"""Digests an installed tree, per python-spu-Spec section 8.

A lock states what should be installed, and this reads what is. Every entry under
the root is listed by its path relative to the root, sorted: a directory by its path,
so an empty one is a record, a regular file by its sha256, a symbolic link by the text
it points to and never followed, and anything else refused. The listing's own sha256 is the tree's digest, so two trees agree on
the digest exactly where they agree on every file and every link.

**A digest is never of nothing.** The root must be a directory by lstat, not a link
to one and not absent, every error the walk meets is raised rather than skipped, and
a tree with no entries is refused, so a misspelled, missing or unreadable root exits
non-zero instead of printing the digest of an empty listing.

Usage: python3 scripts/tree_digest.py <root> [--list]
"""
import hashlib
import os
import stat
import sys


def _raise(error):
    raise error


def listing(root):
    mode = os.lstat(root).st_mode
    if not stat.S_ISDIR(mode):
        raise ValueError(f"{root} is not a directory")
    lines = []
    for directory, subdirectories, files in os.walk(root, onerror=_raise, followlinks=False):
        subdirectories.sort()
        for name in sorted(files + subdirectories):
            path = os.path.join(directory, name)
            relative = os.path.relpath(path, root)
            mode = os.lstat(path).st_mode
            if stat.S_ISDIR(mode):
                # Every directory is a record, so two trees differing by an empty one
                # digest differently. os.walk descends into it after this.
                lines.append(f"dir  {relative}")
            elif stat.S_ISLNK(mode):
                lines.append(f"link {os.readlink(path)}  {relative}")
            elif stat.S_ISREG(mode):
                digest = hashlib.sha256()
                with open(path, "rb") as fh:
                    for chunk in iter(lambda: fh.read(1 << 20), b""):
                        digest.update(chunk)
                lines.append(f"{digest.hexdigest()}  {relative}")
            else:
                raise ValueError(f"{relative} is neither a file nor a link")
    if not lines:
        raise ValueError(f"{root} holds no entries")
    return sorted(lines, key=lambda line: line.split("  ", 1)[1])


def digest_of(lines):
    return hashlib.sha256(("\n".join(lines) + "\n").encode()).hexdigest()


def digest(root):
    return digest_of(listing(root))


def main(argv=None):
    argv = sys.argv[1:] if argv is None else argv
    if not argv:
        print(__doc__.strip().splitlines()[-1], file=sys.stderr)
        return 2
    root = argv[0]
    try:
        lines = listing(root)
    except (OSError, ValueError) as error:
        print(f"tree_digest: {error}", file=sys.stderr)
        return 1
    if "--list" in argv[1:]:
        print("\n".join(lines))
    print(f"{digest_of(lines)}  {root}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
