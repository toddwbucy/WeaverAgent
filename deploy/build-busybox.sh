#!/usr/bin/env bash
# build-busybox.sh OUT [--tarball PATH]
#
# **The agent's shell, built from a pinned source and a committed
# configuration** (#29, weaver-gate-Spec section 8.1, weaver-admin-Spec
# sections 4 and 9): busybox 1.36.1, its tarball checked against the pinned
# sha256, configured by deploy/busybox.config and built static. It writes
# OUT/busybox and OUT/busybox.applets, the list admin judges the binary's own
# `--list` against. bootstrap-stack.sh and update-stack.sh call it beside the
# workspace's build and install both files beside the members.
#
# The tarball is PATH where given, else OUT/busybox-1.36.1.tar.bz2 where it
# stands, else fetched from busybox.net into OUT. Each refuses unless its
# sha256 is the pin.
#
# Refuses, by name, where:
# - the configuration's oldconfig would change it (a tree that is not the
#   pinned one, or a configuration that is not complete);
# - the binary names an ELF interpreter (it is not static);
# - its `--list` lacks sh or ash, or its applets are not the configuration's
#   CONFIG_<APPLET>=y elections.
set -euo pipefail

VERSION=1.36.1
SHA256=b8cc24c9574d809e7279c3be349795c5d5ceb6fdf19ca709f80cde50e47de314
URL="https://busybox.net/downloads/busybox-$VERSION.tar.bz2"
REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CONFIG="$REPO/deploy/busybox.config"

die() { printf 'build-busybox: %s\n' "$*" >&2; exit 1; }

[ $# -ge 1 ] || die "usage: build-busybox.sh OUT [--tarball PATH]"
OUT=$1; shift
TARBALL=""
while [ $# -gt 0 ]; do
  case $1 in
    --tarball) [ $# -ge 2 ] || die "--tarball takes a path"; TARBALL=$2; shift 2 ;;
    *) die "unknown argument $1" ;;
  esac
done
[ -d "$OUT" ] || die "$OUT is not a directory"
[ -r "$CONFIG" ] || die "$CONFIG does not read"

if [ -z "$TARBALL" ]; then
  TARBALL="$OUT/busybox-$VERSION.tar.bz2"
  if [ ! -f "$TARBALL" ]; then
    printf '   fetching %s\n' "$URL"
    curl -fsSL --proto '=https' -o "$TARBALL.part" "$URL" || die "the fetch of $URL failed"
    mv -- "$TARBALL.part" "$TARBALL"
  fi
fi
got=$(sha256sum -- "$TARBALL" | cut -d' ' -f1)
[ "$got" = "$SHA256" ] || die "$TARBALL is sha256 $got, not the pinned $SHA256"

TREE=$(mktemp -d "${TMPDIR:-/tmp}/weaver-busybox.XXXXXX")
trap 'rm -rf -- "$TREE"' EXIT
tar -xjf "$TARBALL" -C "$TREE" --strip-components=1
cp -- "$CONFIG" "$TREE/.config"

# The elections a configuration makes, its comments and blank lines aside.
elections() { grep -E '^CONFIG_' -- "$1" | sort; }
make -C "$TREE" oldconfig </dev/null >"$TREE/oldconfig.log" 2>&1 \
  || die "oldconfig failed, see $TREE/oldconfig.log"
if ! diff <(elections "$CONFIG") <(elections "$TREE/.config") >/dev/null; then
  diff <(elections "$CONFIG") <(elections "$TREE/.config") >&2 || true
  die "oldconfig would change $CONFIG: the tree or the configuration is not the pinned one"
fi
make -C "$TREE" -j"$(nproc)" >"$TREE/build.log" 2>&1 \
  || { tail -n 40 -- "$TREE/build.log" >&2; die "the build failed"; }

BIN="$TREE/busybox"
# **Static, or refused** (weaver-gate-Spec 8.1, M8): no PT_INTERP among the
# program headers, read as admin's inventory reads them.
python3 - "$BIN" <<'PY' || die "$BIN names an ELF interpreter: it is not static"
import struct, sys
data = open(sys.argv[1], "rb").read()
assert data[:4] == b"\x7fELF" and data[4] == 2 and data[5] == 1
phoff, = struct.unpack_from("<Q", data, 32)
phentsize, phnum = struct.unpack_from("<HH", data, 54)
for i in range(phnum):
    p_type, = struct.unpack_from("<I", data, phoff + i * phentsize)
    if p_type == 3:
        sys.exit(1)
PY

LISTED=$(env -i "$BIN" --list </dev/null) || die "$BIN answers no listing: CONFIG_BUSYBOX is not set"
for name in sh ash; do
  grep -qx -- "$name" <<<"$LISTED" || die "$BIN has no $name compiled in"
done
APPLETS=$(grep -vxE 'sh|ash' <<<"$LISTED" | sort)
# The configuration's applets, by their symbols: each listed applet's
# CONFIG_<NAME>=y, so the build's set and the configuration's agree.
for applet in $APPLETS; do
  symbol=CONFIG_$(tr '[:lower:]-' '[:upper:]_' <<<"$applet")
  grep -qx -- "$symbol=y" "$CONFIG" || die "$BIN lists $applet, which $CONFIG does not elect"
done

install -m 0755 -- "$BIN" "$OUT/busybox"
printf '%s\n' "$APPLETS" >"$OUT/busybox.applets"
chmod 0644 -- "$OUT/busybox.applets"
printf '   busybox %s built static, sha256 %s, applets: %s\n' \
  "$VERSION" "$(sha256sum -- "$OUT/busybox" | cut -c1-12)" "$(tr '\n' ' ' <"$OUT/busybox.applets")"
