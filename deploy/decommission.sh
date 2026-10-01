#!/usr/bin/env bash
# Take every weaver agent off this box, archive what stood, and remove it.
#
#   sudo deploy/decommission.sh                      plan: what stands, what would go
#   sudo deploy/decommission.sh --archive DIR        archive to DIR, which the operator names
#   sudo deploy/decommission.sh --purge DIR          remove what DIR's PURGE-LIST names;
#                                                    refuses unless DIR verifies
#
# Box-agnostic, and discovered by rule rather than by one layout:
#
# - a **key root** is any directory under `/etc/weaver` holding a
#   `worker-binary`: an agent's root `<base>/<agent>/`, the stack record
#   `/etc/weaver/stack/`, or the box-wide configuration of the layout before
#   2026-10-01. A directory holding an `allow-list` is one too. **The two
#   overrides the other scripts read are read here too**: an admin base named
#   by `WEAVER_ADMIN_CONFIG` and a stack record named by `WEAVER_STACK_RECORD`
#   outside `/etc/weaver` are discovered, archived and purged with it, so a
#   purge leaves no record behind for the next bootstrap to refuse over. Run
#   with the same values the install ran with: `sudo WEAVER_STACK_RECORD=...`.
# - the **agents** are the names of the agent roots (a key root two levels
#   down, under a base), every old allow-list's names, and every declaration
#   in an old `agent-config-directory`. An account is never the source of an
#   agent: a `weaver-*` account no agent claims is printed and left alone.
# - the **install directories** are the parents of each `bin` directory the
#   key roots name, and a binary directory not named `bin` is one of its own.
# - the **territories** are the directories of the declarations'
#   `trace-sink` paths, the old `agent-config-directory`, and the stack
#   record's `agent-directory`; the **logs** are the directory of each
#   `log-path` (one per agent) and the stack record's `log-directory`.
# - the **databases and roles** are `weaver_<agent>` for the agents found and
#   whatever their declarations name, where PostgreSQL holds them, and the
#   authentication lines removed are the ones naming those and nothing else.
#
# A box fact this script needs and cannot find is printed as unknown, never
# guessed, and the plan is the same reads the archive and the purge make.
#
# **Three modes, because the archive is verified before anything is removed.**
# `--archive` writes tarballs, dumps, a box-facts file and a SHA256SUMS, then
# reads every archive back. `--purge` re-verifies the sums and removes only
# the paths the archive's own PURGE-LIST names, so a purge can never reach
# past what was kept. Running `--purge` against a directory `--archive` did
# not finish refuses by name.
#
# **Models stay.** `/opt/weaver/models` is neither a binary nor a
# configuration; it is a hash-pinned artifact the redeploy loads again, and
# the 0.5b ggufs are already under `backups/opt-weaver-models-20260827` on the
# bulk store. The archive records each model's sha256 and copies none.
#
# **Root, because most of what stands is root's or the agents'.** Territories
# are 0700 under the agent's uid, the record's directory is root's and not
# searchable, the log is root's, and the store's files are postgres's. Run it
# under sudo; it refuses otherwise.
set -euo pipefail

say()  { printf '\n== %s\n' "$*"; }
plan() { printf '   %s\n' "$*"; }
die()  { printf '\nREFUSED: %s\n' "$*" >&2; exit 1; }

[ "$(id -u)" -eq 0 ] || die "run under sudo: territories, the record and the log are not the operator's to read"

MODE=plan
DEST=""
case "${1:-}" in
  "") ;;
  --archive) MODE=archive; DEST="${2:-}" ;;
  --purge)   MODE=purge;   DEST="${2:-}" ;;
  *) die "unknown argument $1" ;;
esac

# The operator is whoever invoked sudo, and every directory is read out of
# the key roots rather than derived from the operator's home.
OPERATOR=${SUDO_USER:-$(logname 2>/dev/null || echo root)}
HOST=$(hostname)
STAMP=$(date +%Y%m%d)
# **No default destination.** Where an archive may land is a fact of the box
# and its mounts, and a default is one box's mount point written into every
# other box's run.
[ "$MODE" = plan ] || [ -n "$DEST" ] || die "--$MODE needs the archive directory: $0 --$MODE DIR"

# ------------------------------------------------------------- 1. discovery
say "box"
plan "host      $HOST"
plan "operator  $OPERATOR"
plan "date      $(date -Iseconds)"
plan "mode      $MODE"
plan "archive   ${DEST:-(named at --archive)}"

ETC=/etc/weaver
ADMIN_BASE=${WEAVER_ADMIN_CONFIG:-$ETC/admin}
STACK=${WEAVER_STACK_RECORD:-$ETC/stack}
# Whether a path stands outside /etc/weaver, which is archived whole.
outside_etc() { case "$1" in "$ETC"|"$ETC"/*) return 1 ;; *) return 0 ;; esac; }
read_key() { cat "$1/$2" 2>/dev/null || true; }
AGENT_NAME='^[A-Za-z0-9_-]+$'

# The key roots, by rule: a directory one or two levels under /etc/weaver
# holding a `worker-binary` or an `allow-list`.
CONFIG_ROOTS=()
AGENT_ROOTS=()
OUTSIDE=()
outside_etc "$ADMIN_BASE" && OUTSIDE+=("$ADMIN_BASE"/*/)
outside_etc "$STACK" && OUTSIDE+=("$STACK/")
for d in "$ETC"/*/ "$ETC"/*/*/ "${OUTSIDE[@]}"; do
  d=${d%/}
  if [ -d "$d" ] && [ ! -L "$d" ] && { [ -f "$d/worker-binary" ] || [ -f "$d/allow-list" ]; }; then
    CONFIG_ROOTS+=("$d")
    # Two levels down is an agent's root under a base, and so is a root
    # directly under an admin base named outside /etc/weaver.
    if { [ "${d%/*/*}" = "$ETC" ] || [ "${d%/*}" = "$ADMIN_BASE" ]; } && [ "$d" != "$STACK" ] \
        && [[ "${d##*/}" =~ $AGENT_NAME ]]; then
      AGENT_ROOTS+=("$d")
    fi
  fi
done

# The string at a dotted path of a TOML declaration, or nothing.
declared() {
  python3 -c '
import sys, tomllib
try:
    with open(sys.argv[1], "rb") as fh:
        value = tomllib.load(fh)
    for part in sys.argv[2].split("."):
        value = value[part]
    if isinstance(value, str):
        print(value)
except (OSError, tomllib.TOMLDecodeError, KeyError, TypeError):
    pass
' "$1" "$2" 2>/dev/null || true
}

# Every path the key roots name, so the install tree is the one the box
# actually ran and not the one this script remembers.
declare -A BIN_DIRS=() AGENT_DIRS=() LOG_PATHS=() AGENTS=() DECLS=()
for root in "${CONFIG_ROOTS[@]}"; do
  for k in worker-binary spu-binary gate-binary; do
    v=$(read_key "$root" "$k"); [ -z "$v" ] || BIN_DIRS["$(dirname "$v")"]=1
  done
  # The layout before 2026-10-01 kept a map of SPU implementations, one
  # `<key> <path>` per line.
  while read -r _ v; do
    [ -z "$v" ] || BIN_DIRS["$(dirname "$v")"]=1
  done < <(read_key "$root" spu-implementations)
  for k in agent-config-directory agent-directory; do
    v=$(read_key "$root" "$k"); [ -z "$v" ] || AGENT_DIRS["$v"]=1
  done
  v=$(read_key "$root" log-path); [ -z "$v" ] || LOG_PATHS["$(dirname "$v")"]=1
  v=$(read_key "$root" log-directory); [ -z "$v" ] || LOG_PATHS["$v"]=1
  for a in $(read_key "$root" allow-list); do
    if [[ "$a" =~ $AGENT_NAME ]]; then AGENTS["$a"]=1; fi
  done
done
for root in "${AGENT_ROOTS[@]}"; do
  AGENTS["${root##*/}"]=1
  [ ! -f "$root/agent.toml" ] || DECLS["${root##*/}"]="$root/agent.toml"
done
# Declarations of the layout before 2026-10-01: `<name>.toml` in the old
# agent-config-directory. The YAML before them reads no sink, and sits inside
# that directory, which is archived whole.
for d in "${!AGENT_DIRS[@]}"; do
  for f in "$d"/*.toml "$d"/*.yaml; do
    [ -f "$f" ] || continue
    n=$(basename "$f"); n=${n%.*}
    [[ "$n" =~ $AGENT_NAME ]] || continue
    AGENTS["$n"]=1
    case "$f" in *.toml) [ -n "${DECLS[$n]:-}" ] || DECLS["$n"]="$f" ;; esac
  done
done

# The install directories: the parent of each `bin`, and a binary directory
# by any other name is its own, unless it is a system directory, which is
# printed and never taken.
declare -A PREFIXES=() INSTALL_DIRS=()
for b in "${!BIN_DIRS[@]}"; do
  case "$b" in
    /|/bin|/sbin|/usr|/usr/*|/lib|/lib64|/etc|/etc/*|/home|/opt|/var|/var/*) plan "binary directory $b is a system directory: not taken" ;;
    */bin) PREFIXES["$(dirname "$b")"]=1 ;;
    *) INSTALL_DIRS["$b"]=1 ;;
  esac
done
# One a prefix already carries (its `lib` or `python-spu`) is not its own.
for d in "${!INSTALL_DIRS[@]}"; do
  case "${d##*/}" in lib|python-spu) [ -z "${PREFIXES[${d%/*}]:-}" ] || unset 'INSTALL_DIRS[$d]' ;; esac
done

# The territories: the directory of each declaration's trace sink. One that
# is too broad to be an agent's own (fewer than three components, or a home
# directory itself) is printed and never taken.
declare -A SINK_DIRS=()
for a in "${!DECLS[@]}"; do
  v=$(declared "${DECLS[$a]}" trace-sink.path)
  [ -n "$v" ] || continue
  v=$(dirname "$v")
  if [ "$(printf '%s' "$v" | tr -cd / | wc -c)" -lt 3 ] || getent passwd | cut -d: -f6 | grep -xF -- "$v" >/dev/null; then
    plan "$a: trace-sink directory $v is too broad to be the agent's own: not taken"
  else
    SINK_DIRS["$v"]=1
  fi
done

# Accounts: the two each agent found has, never every `weaver-*`.
WEAVER_USERS=(); WEAVER_GROUPS=()
for a in "${!AGENTS[@]}"; do
  for u in "weaver-$a" "weaver-$a-state"; do
    if getent passwd "$u" >/dev/null; then WEAVER_USERS+=("$u"); fi
    if getent group "$u" >/dev/null; then WEAVER_GROUPS+=("$u"); fi
  done
done
UNCLAIMED=()
while read -r u; do
  [ -n "$u" ] || continue
  case " ${WEAVER_USERS[*]} " in *" $u "*) ;; *) UNCLAIMED+=("$u") ;; esac
done < <(getent passwd | awk -F: '$1 ~ /^weaver-/ {print $1}')

say "config roots"
for r in "${CONFIG_ROOTS[@]}"; do
  if [ -f "$r/allow-list" ]; then plan "$r  (box-wide, allow-list: $(read_key "$r" allow-list | tr '\n' ' '))"
  else plan "$r"; fi
done
[ ${#CONFIG_ROOTS[@]} -gt 0 ] || plan "none under $ETC"

say "install prefixes (models excluded from every mode)"
for p in "${!PREFIXES[@]}"; do
  for sub in bin lib python-spu; do [ ! -d "$p/$sub" ] || plan "$p/$sub  $(du -sh "$p/$sub" 2>/dev/null | cut -f1)"; done
  for sub in "$p"/backup-* "$p"/lib.backup-*; do [ ! -d "$sub" ] || plan "$sub  $(du -sh "$sub" | cut -f1)"; done
  [ ! -d "$p/models" ] || plan "$p/models  $(du -sh "$p/models" | cut -f1)  KEPT"
done
for d in "${!INSTALL_DIRS[@]}"; do [ ! -d "$d" ] || plan "$d  $(du -sh "$d" 2>/dev/null | cut -f1)  (an install directory of its own)"; done
mapfile -t LDSO_CONFS < <(grep -ls 'weaver' /etc/ld.so.conf.d/*.conf 2>/dev/null)
for c in "${LDSO_CONFS[@]}"; do plan "$c  ($(tr '\n' ' ' < "$c"))"; done

say "agents"
for a in "${!AGENTS[@]}"; do
  line="$a:"
  if id "weaver-$a" >/dev/null 2>&1; then line="$line user=weaver-$a"; fi
  if id "weaver-$a-state" >/dev/null 2>&1; then line="$line member=weaver-$a-state"; fi
  if [ -n "${DECLS[$a]:-}" ]; then line="$line decl=${DECLS[$a]}"; fi
  plan "$line"
done
plan "accounts: ${WEAVER_USERS[*]:-none}"
plan "groups:   ${WEAVER_GROUPS[*]:-none}"
[ ${#UNCLAIMED[@]} -eq 0 ] || plan "left alone, no agent found claims them: ${UNCLAIMED[*]}"

say "units"
mapfile -t UNITS < <(systemctl list-units 'weaver-worker@*' --all --no-legend --plain 2>/dev/null | awk '{print $1}')
ACTIVE_UNITS=()
for u in "${UNITS[@]}"; do
  st=$(systemctl is-active "$u" 2>/dev/null || true)
  plan "$u  $st"
  [ "$st" = active ] && ACTIVE_UNITS+=("$u")
done
[ ${#UNITS[@]} -gt 0 ] || plan "no weaver-worker@ units"
SLICE='system-weaver\x2dworker.slice'
systemctl is-active --quiet "$SLICE" 2>/dev/null && plan "$SLICE active ($(systemctl show "$SLICE" -p NCurrentlyActive 2>/dev/null || true))" || plan "$SLICE not active"

say "territories, record, logs"
# **Outermost only**: a territory inside the agent directory, or an agent's
# log directory inside the box's, is archived and removed with its parent
# and is not listed twice.
outermost() {
  local p q keep
  for p in "$@"; do
    keep=1
    for q in "$@"; do
      if [ "$p" != "$q" ]; then case "$p/" in "$q"/*) keep=0 ;; esac; fi
    done
    if [ "$keep" -eq 1 ]; then printf '%s\n' "$p"; fi
  done | sort -u
}
TERRITORY_PATHS=()
LOG_DIRS=()
mapfile -t TERRITORY_CANDIDATES < <(outermost "${!AGENT_DIRS[@]}" "${!SINK_DIRS[@]}")
for d in "${TERRITORY_CANDIDATES[@]}"; do
  [ -n "$d" ] && [ -e "$d" ] || continue
  TERRITORY_PATHS+=("$d"); plan "$d  $(du -sh "$d" 2>/dev/null | cut -f1)"
done
mapfile -t LOG_CANDIDATES < <(outermost /var/lib/weaver "${!LOG_PATHS[@]}")
for d in "${LOG_CANDIDATES[@]}"; do
  [ -n "$d" ] && [ -e "$d" ] || continue
  LOG_DIRS+=("$d"); plan "$d  $(du -sh "$d" 2>/dev/null | cut -f1)"
done
HOMES=()
for u in "${WEAVER_USERS[@]}"; do
  h=$(getent passwd "$u" | cut -d: -f6)
  [ -d "$h" ] && case "$h" in /home/*) HOMES+=("$h"); plan "$h  (home of $u)";; esac
done
mapfile -t TMP_PATHS < <(find /tmp -maxdepth 1 \( -name 'weaver-*' -o -name 'torchinductor_weaver-*' \) 2>/dev/null)
[ ${#TMP_PATHS[@]} -gt 0 ] && plan "/tmp: ${#TMP_PATHS[@]} weaver-* entries"

say "store"
# **The agents' own and nothing else.** A box can keep other databases under
# a `weaver` prefix (a frontend's store, a measurement store), so the
# candidates are `weaver_<agent>` for each agent found and whatever its
# declaration names, and each is taken only where the catalogue holds it by
# exactly that name.
declare -A DB_WANT=() ROLE_WANT=()
for a in "${!AGENTS[@]}"; do
  DB_WANT["weaver_$a"]=1; ROLE_WANT["weaver_$a"]=1
  if [ -n "${DECLS[$a]:-}" ]; then
    v=$(declared "${DECLS[$a]}" state-store.database); [ -z "$v" ] || DB_WANT["$v"]=1
    v=$(declared "${DECLS[$a]}" state-store.role); [ -z "$v" ] || ROLE_WANT["$v"]=1
  fi
done
PG_ROLES=(); PG_DBS=(); OTHER_DBS=(); HBA=""; IDENT=""
# The authentication lines create-agent.sh wrote: `local <db> <role> ...` in
# pg_hba.conf and `<map> <member> <role>` in pg_ident.conf. Matched by field,
# so a line naming another database or role is never one of these.
auth_lines() { # auth_lines hba|ident FILE DBS ROLES : print the agents' lines
  awk -v kind="$1" -v dbs=" $3 " -v roles=" $4 " '
    /^[[:space:]]*#/ { next }
    kind == "hba"   && NF >= 3 && index(dbs, " " $2 " ") && index(roles, " " $3 " ") { print; next }
    kind == "ident" && NF >= 3 && index(roles, " " $3 " ") { print }
  ' "$2" 2>/dev/null || true
}
if systemctl is-active --quiet postgresql 2>/dev/null; then
  while read -r r; do
    [ -n "$r" ] || continue
    if [ -n "${ROLE_WANT[$r]:-}" ]; then PG_ROLES+=("$r"); fi
  done < <(sudo -u postgres psql -X -tAc "select rolname from pg_roles" 2>/dev/null || true)
  while read -r db; do
    [ -n "$db" ] || continue
    if [ -n "${DB_WANT[$db]:-}" ]; then PG_DBS+=("$db")
    else case "$db" in weaver*) OTHER_DBS+=("$db") ;; esac; fi
  done < <(sudo -u postgres psql -X -tAc "select datname from pg_database" 2>/dev/null || true)
  HBA=$(sudo -u postgres psql -X -tAc 'show hba_file' 2>/dev/null || true)
  IDENT=$(sudo -u postgres psql -X -tAc 'show ident_file' 2>/dev/null || true)
  plan "roles      ${PG_ROLES[*]:-none}"
  plan "databases  ${PG_DBS[*]:-none}"
  [ ${#OTHER_DBS[@]} -eq 0 ] || plan "left alone, no agent found names them: ${OTHER_DBS[*]}"
  [ -z "$HBA" ]   || plan "hba lines   $(auth_lines hba "$HBA" "${PG_DBS[*]:-}" "${PG_ROLES[*]:-}" | wc -l) in $HBA"
  [ -z "$IDENT" ] || plan "ident lines $(auth_lines ident "$IDENT" "${PG_DBS[*]:-}" "${PG_ROLES[*]:-}" | wc -l) in $IDENT"
else
  plan "postgresql not active; store not inspected"
fi

[ "$MODE" = plan ] && { say "plan only. rerun with --archive DIR, then --purge DIR"; exit 0; }

# --------------------------------------------------------------- 2. archive
# **Root reads, the operator writes.** The bulk store is an NFS export that
# squashes root to nobody, so root can neither create a directory the operator
# owns nor own what it writes there. Every read of a territory, the record or
# the store stays root's, and every byte that lands under $DEST goes through
# the operator's account: tar and pg_dump write to stdout, and the operator's
# shell writes the file. Measured 2026-09-30: mkdir as root failed on the
# export, and a file root did write arrived owned by nobody.
as_op() { sudo -u "$OPERATOR" -H "$@"; }
to_file() { as_op sh -c 'cat > "$1"' _ "$1"; }   # stdin -> $1, owned by the operator
tarz() { # tarz OUT PATH... : preserve owners, modes, acls and xattrs
  local out=$1; shift
  tar --numeric-owner --acls --xattrs -I 'zstd -T0 -3' -cf - "$@" 2> >(grep -v 'Removing leading' >&2) | to_file "$out"
}
sha_all() { find "$1" -maxdepth 1 -type f -exec sha256sum {} \; 2>/dev/null || true; }

if [ "$MODE" = archive ]; then
  [ ${#ACTIVE_UNITS[@]} -eq 0 ] || die "units still active: ${ACTIVE_UNITS[*]}. Unload them (weaver-admin unload <agent>) or stop them, then rerun"
  [ -e "$DEST/SHA256SUMS" ] && die "$DEST already holds an archive; name another directory"
  as_op mkdir -p "$DEST" || die "the operator cannot create $DEST"
  as_op test -w "$DEST" || die "$DEST is not writable by $OPERATOR"
  # A run that died part way leaves files here the operator may not own;
  # they are this script's and go before it writes again.
  find "$DEST" -maxdepth 1 -type f -exec rm -f {} \; 2>/dev/null || true
  say "archive to $DEST"

  NVCC=$(command -v nvcc || ls /opt/cuda/bin/nvcc 2>/dev/null || true)
  # Box facts first, so a reader of the archive knows what produced it.
  {
    echo "host      $HOST"
    echo "operator  $OPERATOR"
    echo "date      $(date -Iseconds)"
    echo "kernel    $(uname -r)"
    echo "driver    $(nvidia-smi --query-gpu=name,driver_version --format=csv,noheader 2>/dev/null | head -1 || echo none)"
    echo "nvcc      $([ -n "$NVCC" ] && "$NVCC" --version | tail -1 || echo none)"
    echo "cccl      $(pacman -Q cccl 2>/dev/null || echo unknown)"
    echo "postgres  $(pacman -Q postgresql 2>/dev/null || echo unknown)"
    echo "toolchain $(as_op rustup show active-toolchain 2>/dev/null | cut -d' ' -f1 || echo unknown)"
    echo
    echo "## accounts"; for u in "${WEAVER_USERS[@]}"; do getent passwd "$u" || true; done
    echo "## groups";   for g in "${WEAVER_GROUPS[@]}"; do getent group "$g" || true; done
    echo "## operator groups"; id "$OPERATOR"
    echo
    echo "## units"; systemctl list-units 'weaver-worker@*' --all --no-legend 2>/dev/null || true
    systemctl show "$SLICE" -p ActiveState,CPUUsageNSec,MemoryPeak 2>/dev/null || true
    echo
    echo "## installed binaries"; for p in "${!PREFIXES[@]}"; do sha_all "$p/bin"; done
    for d in "${!INSTALL_DIRS[@]}"; do sha_all "$d"; done
    echo
    echo "## libraries"; for p in "${!PREFIXES[@]}"; do sha_all "$p/lib"; done
    echo
    echo "## ld.so.conf"; for c in "${LDSO_CONFS[@]}"; do echo "$c:"; cat "$c"; done
    echo
    echo "## models (kept in place, not copied)"
    for p in "${!PREFIXES[@]}"; do [ -d "$p/models" ] && { find "$p/models" -type f -size +1M -exec sha256sum {} \; 2>/dev/null || true; }; done
    echo
    echo "## store"
    echo "roles: ${PG_ROLES[*]:-none}"; echo "databases: ${PG_DBS[*]:-none}"
    [ -z "$HBA" ]   || { echo "## $HBA agents' lines";   auth_lines hba "$HBA" "${PG_DBS[*]:-}" "${PG_ROLES[*]:-}"; }
    [ -z "$IDENT" ] || { echo "## $IDENT agents' lines"; auth_lines ident "$IDENT" "${PG_DBS[*]:-}" "${PG_ROLES[*]:-}"; }
    echo
    echo "## modes of everything archived"
    for pth in "${TERRITORY_PATHS[@]}" "${LOG_DIRS[@]}" "${HOMES[@]}" "${!PREFIXES[@]}" "${!INSTALL_DIRS[@]}" /etc/weaver; do
      [ -e "$pth" ] && { find "$pth" -maxdepth 2 -not -path '*/models/*' -printf '%M %u:%g %p\n' 2>/dev/null || true; }
    done
    true
  } | to_file "$DEST/box-facts.txt"
  plan "box-facts.txt"

  PURGE=()
  archive_path() { # archive_path NAME PATH...
    local name=$1; shift
    local present=()
    for pth in "$@"; do [ -e "$pth" ] && present+=("$pth"); done
    [ ${#present[@]} -gt 0 ] || return 0
    tarz "$DEST/$name.tar.zst" "${present[@]}"
    # Apparent size, not blocks: on NFS the blocks are not yet accounted
    # when this line prints, and du answered 512 for a 3 G tarball.
    plan "$name.tar.zst  $(stat -c %s "$DEST/$name.tar.zst" | numfmt --to=iec)  <- ${present[*]}"
    PURGE+=("${present[@]}")
  }

  [ -d /etc/weaver ] && archive_path etc-weaver /etc/weaver
  if outside_etc "$ADMIN_BASE" && [ -d "$ADMIN_BASE" ]; then archive_path admin-base "$ADMIN_BASE"; fi
  if outside_etc "$STACK" && [ -d "$STACK" ]; then archive_path stack-record "$STACK"; fi
  [ ${#LDSO_CONFS[@]} -gt 0 ] && archive_path ld-so-conf "${LDSO_CONFS[@]}"
  for p in "${!PREFIXES[@]}"; do
    n=$(basename "$p")
    parts=()
    for sub in bin lib python-spu; do [ -d "$p/$sub" ] && parts+=("$p/$sub"); done
    for sub in "$p"/backup-* "$p"/lib.backup-*; do [ -d "$sub" ] && parts+=("$sub"); done
    [ ${#parts[@]} -gt 0 ] && archive_path "opt-$n" "${parts[@]}"
  done
  # Named by the whole path, so two directories of one basename do not
  # write one tarball.
  slug() { printf '%s' "${1#/}" | tr '/.' '--'; }
  for d in "${!INSTALL_DIRS[@]}"; do archive_path "install-$(slug "$d")" "$d"; done
  for d in "${LOG_DIRS[@]}"; do archive_path "log-$(slug "$d")" "$d"; done
  for d in "${TERRITORY_PATHS[@]}"; do archive_path "territory-$(slug "$d")" "$d"; done
  [ ${#HOMES[@]} -gt 0 ] && archive_path home-weaver-users "${HOMES[@]}"
  [ ${#TMP_PATHS[@]} -gt 0 ] && archive_path tmp-weaver "${TMP_PATHS[@]}"

  if [ ${#PG_DBS[@]} -gt 0 ]; then
    as_op mkdir -p "$DEST/postgres"
    for db in "${PG_DBS[@]}"; do
      sudo -u postgres pg_dump -Fc "$db" | to_file "$DEST/postgres/$db.dump"
      plan "postgres/$db.dump"
    done
    { [ ${#PG_ROLES[@]} -eq 0 ] || sudo -u postgres pg_dumpall --roles-only 2>/dev/null | grep -wF -f <(printf '%s\n' "${PG_ROLES[@]}") || true; } | to_file "$DEST/postgres/roles.sql"
    [ -n "$HBA" ]   && cat "$HBA"   | to_file "$DEST/postgres/pg_hba.conf"
    [ -n "$IDENT" ] && cat "$IDENT" | to_file "$DEST/postgres/pg_ident.conf"
  fi

  # What --purge may touch, and nothing else. Accounts, roles and databases
  # are listed by kind so the purge removes them by the right verb.
  {
    for pth in "${PURGE[@]}"; do echo "path $pth"; done
    for u in "${WEAVER_USERS[@]}";  do echo "user $u"; done
    for g in "${WEAVER_GROUPS[@]}"; do echo "group $g"; done
    for db in "${PG_DBS[@]}";       do echo "database $db"; done
    for r in "${PG_ROLES[@]}";      do echo "role $r"; done
    [ -n "$HBA" ]   && echo "hba $HBA"
    [ -n "$IDENT" ] && echo "ident $IDENT"
    true
  } | to_file "$DEST/PURGE-LIST"

  as_op sh -c 'cd "$1" && find . -type f -not -name SHA256SUMS -printf "%P\n" | sort | xargs sha256sum > SHA256SUMS' _ "$DEST"
  say "verify"
  as_op sh -c 'cd "$1" && sha256sum -c --quiet SHA256SUMS' _ "$DEST" || die "the archive does not verify"
  for t in "$DEST"/*.tar.zst; do tar -tf "$t" >/dev/null || die "$t does not read back"; done
  plan "$(wc -l < "$DEST/SHA256SUMS") files sum and every tarball reads back"
  say "archived. review $DEST/PURGE-LIST, then: sudo $0 --purge $DEST"
  exit 0
fi

# ----------------------------------------------------------------- 3. purge
[ -f "$DEST/SHA256SUMS" ] && [ -f "$DEST/PURGE-LIST" ] || die "$DEST holds no finished archive"
say "re-verify $DEST"
( cd "$DEST" && sha256sum -c --quiet SHA256SUMS ) || die "the archive no longer verifies; purge refused"
[ ${#ACTIVE_UNITS[@]} -eq 0 ] || die "units still active: ${ACTIVE_UNITS[*]}"
plan "verified"

say "units"
for u in "${UNITS[@]}"; do systemctl stop "$u" 2>/dev/null || true; systemctl reset-failed "$u" 2>/dev/null || true; plan "stopped $u"; done
systemctl stop "$SLICE" 2>/dev/null && plan "stopped $SLICE" || true

say "store"
while read -r kind name; do
  case $kind in
    database) sudo -u postgres psql -X -v ON_ERROR_STOP=1 -c "DROP DATABASE IF EXISTS $name;" >/dev/null && plan "dropped database $name" ;;
  esac
done < "$DEST/PURGE-LIST"
while read -r kind name; do
  case $kind in
    role) sudo -u postgres psql -X -v ON_ERROR_STOP=1 -c "DROP ROLE IF EXISTS $name;" >/dev/null && plan "dropped role $name" ;;
  esac
done < "$DEST/PURGE-LIST"
# The databases and roles the archive listed, and only their lines go.
LISTED_DBS=$(awk '$1 == "database" {printf "%s ", $2}' "$DEST/PURGE-LIST")
LISTED_ROLES=$(awk '$1 == "role" {printf "%s ", $2}' "$DEST/PURGE-LIST")
while read -r kind name; do
  case $kind in
    hba|ident)
      backup="$name.before-decommission-$STAMP"
      cp -a "$name" "$backup"
      # create-agent.sh wrote `local <db> <role> peer map=weaver` and
      # `weaver <member> <role>`. Each line is dropped where its fields name
      # a listed database and role (hba) or a listed role (ident), so a line
      # of a database no agent named stays. Written through the open file so
      # its owner and mode stand.
      auth_lines "$kind" "$backup" "$LISTED_DBS" "$LISTED_ROLES" > "$name.drop-$STAMP"
      grep -vxF -f "$name.drop-$STAMP" "$backup" > "$name" || true
      plan "removed $(wc -l < "$name.drop-$STAMP") agents' lines from $name (backup beside it)"
      rm -f "$name.drop-$STAMP" ;;
  esac
done < "$DEST/PURGE-LIST"
systemctl is-active --quiet postgresql 2>/dev/null && systemctl reload postgresql

say "accounts"
while read -r kind name; do
  case $kind in
    user)  userdel -r "$name" 2>/dev/null && plan "removed user $name (and home)" || { userdel "$name" 2>/dev/null && plan "removed user $name" || plan "user $name: not removed"; } ;;
  esac
done < "$DEST/PURGE-LIST"
while read -r kind name; do
  case $kind in
    group) getent group "$name" >/dev/null && { groupdel "$name" 2>/dev/null && plan "removed group $name" || plan "group $name: not removed"; } ;;
  esac
done < "$DEST/PURGE-LIST"

say "paths"
while read -r kind name; do
  case $kind in
    path)
      case "$name" in */models|*/models/*) plan "kept $name"; continue;; esac
      rm -rf -- "$name" && plan "removed $name" ;;
  esac
done < "$DEST/PURGE-LIST"
# A prefix left holding only models stays; one left empty goes.
for p in "${!PREFIXES[@]}"; do
  if [ -d "$p" ] && [ -z "$(ls -A "$p")" ]; then rmdir "$p" && plan "removed empty $p"; fi
done
ldconfig && plan "ldconfig rerun: $(ldconfig -p | grep -cE 'ggml|llama') engine objects still in the cache"

say "what remains"
for p in "${!PREFIXES[@]}"; do [ -d "$p" ] && find "$p" -maxdepth 1 -mindepth 1 -printf '   %p\n'; done
getent passwd | awk -F: '$1 ~ /^weaver-/ {print "   account still present: "$1}' || true
systemctl list-units 'weaver-worker@*' --all --no-legend 2>/dev/null | sed 's/^/   unit still present: /' || true
say "purged. the archive is $DEST"
