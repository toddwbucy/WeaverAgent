#!/usr/bin/env python3
"""busybox_applets.py TREE CONFIG LISTED

**The busybox listing and its configuration agree both ways** (#29,
weaver-gate-Spec section 8.1), read through the tree's own declarations
(`//applet:IF_<SYMBOL>(APPLET...(<name>, ...))`), so no symbol is guessed from
a name. Every applet LISTED (the binary's `--list`, whitespace-separated) has
a symbol CONFIG elects, and every applet whose symbol CONFIG elects is listed.
Exits 0 where they agree, 1 naming each disagreement on standard error.
build-busybox.sh calls it after the build.
"""
import pathlib
import re
import sys

DECLARED = re.compile(r"//applet:IF_([A-Z0-9_]+)\(\s*APPLET[A-Z_]*\(\s*([^,\s)]+)")


def faults(tree, config, listed):
    symbols = {}
    for source in pathlib.Path(tree).rglob("*.c"):
        for match in DECLARED.finditer(source.read_text(errors="replace")):
            symbols.setdefault(match.group(2), set()).add(match.group(1))
    elected = {line.split("=")[0].removeprefix("CONFIG_")
               for line in pathlib.Path(config).read_text().splitlines()
               if line.startswith("CONFIG_") and line.endswith("=y")}
    found = [f"listed and not elected: {name}" for name in sorted(listed)
             if not symbols.get(name, set()) & elected]
    found += [f"elected and not listed: {name}" for name, syms in sorted(symbols.items())
              if syms & elected and name not in listed]
    return found


if __name__ == "__main__":
    tree, config, listed = sys.argv[1], sys.argv[2], set(sys.argv[3].split())
    found = faults(tree, config, listed)
    for fault in found:
        print(f"build-busybox: {fault}", file=sys.stderr)
    sys.exit(1 if found else 0)
