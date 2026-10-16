#!/usr/bin/env bash
# build-busybox.sh OUT [--tarball PATH]
# build-busybox.sh --preflight
#
# **The agent's shell, built from a pinned source and a committed
# configuration** (#29, weaver-gate-Spec section 8.1, weaver-admin-Spec
# sections 4 and 9): busybox 1.36.1, its tarball checked against the pinned
# sha256, configured by deploy/busybox.config and built static. It writes
# OUT/busybox and OUT/busybox.applets, the list admin judges the binary's own
# `--list` against. bootstrap-stack.sh and update-stack.sh call it beside the
# workspace's build and install both files beside the members.
#
# The tarball is PATH where given (the stack scripts pass BUSYBOX_TARBALL
# through, for a box without the network), else OUT/busybox-1.36.1.tar.bz2
# where it stands, else fetched from busybox.net into OUT. Each refuses unless
# its sha256 is the pin.
#
# **The build is reproducible**: the build time busybox stamps into its
# banner is pinned (SOURCE_DATE_EPOCH, KCONFIG_NOTIMESTAMP), so one tarball
# and one configuration build one binary, and update-stack.sh's plan reads an
# unchanged busybox as unchanged.
#
# `--preflight` checks only that the tools the build needs stand (curl where
# no tarball is given, tar with bzip2, make, gcc linking statically as the
# build links, python3), so the stack scripts refuse before their long build
# rather than after it.
#
# Refuses, by name, where:
# - the configuration's oldconfig would change it (a tree that is not the
#   pinned one, or a configuration that is not complete),
# - the binary names an ELF interpreter (it is not static),
# - its `--list` lacks sh or ash, or its applets are not the configuration's
#   CONFIG_<APPLET>=y elections.
set -euo pipefail

VERSION=1.36.1
SHA256=b8cc24c9574d809e7279c3be349795c5d5ceb6fdf19ca709f80cde50e47de314
URL="https://busybox.net/downloads/busybox-$VERSION.tar.bz2"
REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CONFIG="$REPO/deploy/busybox.config"

die() { printf 'build-busybox: %s\n' "$*" >&2; exit 1; }

# The release's own date, 2023-01-03, pinned as the build time.
export SOURCE_DATE_EPOCH=1672704000 KCONFIG_NOTIMESTAMP=1

preflight() {
  local tool
  for tool in tar bzip2 make gcc python3 sha256sum; do
    command -v "$tool" >/dev/null || die "the shell's build needs $tool, which this box does not have"
  done
  if [ -z "${BUSYBOX_TARBALL:-}" ]; then
    command -v curl >/dev/null \
      || die "the shell's build fetches its tarball with curl, which this box does not have: install it, or name a tarball with BUSYBOX_TARBALL"
  fi
  local probe
  probe=$(mktemp -d "${TMPDIR:-/tmp}/weaver-busybox-preflight.XXXXXX")
  printf 'int main(void) { return 0; }\n' >"$probe/c.c"
  # **gcc, as busybox's Makefile names it** (`$(CROSS_COMPILE)gcc`), linked
  # statically as the build will link it.
  if ! gcc -static -o "$probe/c" "$probe/c.c" >/dev/null 2>&1; then
    rm -rf -- "$probe"
    die "gcc does not link statically (no static libc): the shell's busybox is static"
  fi
  rm -rf -- "$probe"
}
if [ "${1:-}" = --preflight ]; then
  preflight
  exit 0
fi

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
  || { tail -n 40 -- "$TREE/oldconfig.log" >&2; die "oldconfig failed, its log's end above"; }
if ! diff <(elections "$CONFIG") <(elections "$TREE/.config") >/dev/null; then
  diff <(elections "$CONFIG") <(elections "$TREE/.config") >&2 || true
  die "oldconfig would change $CONFIG: the tree or the configuration is not the pinned one"
fi
make -C "$TREE" -j"$(nproc)" >"$TREE/build.log" 2>&1 \
  || { tail -n 40 -- "$TREE/build.log" >&2; die "the build failed"; }

BIN="$TREE/busybox"
# **Static, or refused** (weaver-gate-Spec 8.1, M8): no PT_INTERP among the
# program headers, read as admin's inventory reads them.
rc=0
python3 - "$BIN" <<'PY' || rc=$?
import struct, sys
data = open(sys.argv[1], "rb").read()
if not (data[:4] == b"\x7fELF" and data[4] == 2 and data[5] == 1):
    sys.exit(2)
phoff, = struct.unpack_from("<Q", data, 32)
phentsize, phnum = struct.unpack_from("<HH", data, 54)
for i in range(phnum):
    p_type, = struct.unpack_from("<I", data, phoff + i * phentsize)
    if p_type == 3:
        sys.exit(1)
PY
case $rc in
  0) ;;
  1) die "$BIN names an ELF interpreter: it is not static" ;;
  2) die "$BIN is not a 64-bit little-endian ELF image" ;;
  *) die "$BIN's ELF header does not read" ;;
esac

LISTED=$(env -i "$BIN" --list </dev/null) || die "$BIN answers no listing: CONFIG_BUSYBOX is not set"
for name in sh ash; do
  grep -qx -- "$name" <<<"$LISTED" || die "$BIN has no $name compiled in"
done
# **The listing and the configuration agree both ways**, read through the
# tree's own declarations (`//applet:IF_<SYMBOL>(APPLET...(<name>, ...))`),
# so no symbol is guessed from a name: every listed applet has an elected
# symbol, and every applet whose symbol the configuration elects is listed.
python3 "$REPO/deploy/busybox_applets.py" "$TREE" "$CONFIG" "$LISTED" \
  || die "the listing and $CONFIG disagree, see above"
# The applets beyond the interpreter, an empty set judged rather than
# ending the script on grep's no-match status.
APPLETS=$(grep -vxE 'sh|ash' <<<"$LISTED" | sort || true)
[ -n "$APPLETS" ] || die "$BIN lists no applet beyond the interpreter, so the shell would hold no command"

install -m 0755 -- "$BIN" "$OUT/busybox"
printf '%s\n' "$APPLETS" >"$OUT/busybox.applets"
chmod 0644 -- "$OUT/busybox.applets"
printf '   busybox %s built static, sha256 %s, applets: %s\n' \
  "$VERSION" "$(sha256sum -- "$OUT/busybox" | cut -c1-12)" "$(tr '\n' ' ' <"$OUT/busybox.applets")"
