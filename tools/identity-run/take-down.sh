#!/usr/bin/env bash
# Takes one agent off an agent appliance, by root, and leaves everything else standing:
# the stack, the stack's record, every other agent and the machine's own accounts, the
# two connector accounts among them. It plans by default and acts with --apply.
#
#   take-down.sh --agent NAME --archive DIR [--admin PATH] [--apply]
#
# It follows `deploy/HowToDeployANewAgent.md` section 7 ("Taking an agent down") step
# for step, in its order, after two steps of its own: the reads, and the agent's pieces
# archived as root into DIR before anything is removed. `decommission.sh` is not used:
# it takes every agent, the stack and every `weaver-` account off the machine.
#
#  1. Read before anything: admin's `show` must name the agent unloaded with no
#     constituent. The stack record's `agent-directory` and `coordination-root` and the
#     root's `territory` key are read, and the territory must be the agent's own,
#     <agent-directory>/NAME/weaver-NAME. A per-agent connector account,
#     weaver-NAME-admincon, that an earlier create-agent.sh laid, refuses: this
#     take-down removes no connector account.
#  2. The archive: DIR made by root 0700, then one tar, written by root under umask 077,
#     of the agent's sudo rule, its root, its own folder and its territory, listed back.
#  3. The sudo rule /etc/sudoers.d/weaver-NAME.
#  4. The root <admin base>/NAME/, which ends the admission.
#  5. The agent's own folder /etc/weaver/NAME/, with any loop folder in it.
#  6. The run and runtime directories, <coordination-root>/weaver.run/NAME/ and
#     <coordination-root>/weaver-NAME/.
#  7. The territory.
#  8. The agent's /tmp entries, chosen by owner among its accounts, never by name alone.
#  9. The home's quota limit, where the drive keeps project quotas and the agent's
#     folder carries the agent's uid as its project.
# 10. The accounts: weaver-NAME with its home, weaver-NAME-state, weaver-NAME-relay.
# 11. The groups: weaver-NAME, -state, -trace, -admin.
# 12. The agent's folder <agent-directory>/NAME/, by rmdir, so it must stand empty.
#
# Every apply step is unmeasured on the build machine: only the plan and the refusals
# before any step are tested (test_take_down.py). A failed step stops at once and says
# which, and what was removed before it stays removed: read the plan, then finish by
# hand by the runbook's section 7.
set -uo pipefail

die() { printf 'take-down: %s\n' "$*" >&2; exit 2; }

NAME= ARCHIVE= APPLY=0
ADMIN=/opt/weaver/bin/weaver-admin
while [ $# -gt 0 ]; do
    case $1 in
        --agent)   [ $# -ge 2 ] || die "--agent needs a name"; NAME=$2; shift ;;
        --archive) [ $# -ge 2 ] || die "--archive needs a folder"; ARCHIVE=$2; shift ;;
        --admin)   [ $# -ge 2 ] || die "--admin needs a path"; ADMIN=$2; shift ;;
        --apply)   APPLY=1 ;;
        *)         die "unknown argument: $1" ;;
    esac
    shift
done
# create-agent.sh's name rule, and its reserved names: a name that is a connector's or a
# config base's would make the accounts and paths below the machine's own.
[[ "$NAME" =~ ^[a-z][a-z0-9]{1,15}$ ]] || die "--agent is an agent's name, a lowercase letter then 1 to 15 lowercase letters or digits: '$NAME'"
case $NAME in
    gatecon|admincon|admin*|stack) die "'$NAME' is reserved, and no agent takes it: the accounts and paths of that name are the machine's" ;;
esac
[ -n "$ARCHIVE" ] || die "--archive is required"
case $ARCHIVE in /*) ;; *) die "--archive is an absolute path: '$ARCHIVE'" ;; esac

BASE=${WEAVER_ADMIN_CONFIG:-/etc/weaver/admin}
STACK=${WEAVER_STACK_RECORD:-/etc/weaver/stack}
OWN=/etc/weaver/$NAME
SUDO_RULE=/etc/sudoers.d/weaver-$NAME
ACCOUNTS=("weaver-$NAME" "weaver-$NAME-state" "weaver-$NAME-relay")
GROUPS_OF=("weaver-$NAME" "weaver-$NAME-state" "weaver-$NAME-trace" "weaver-$NAME-admin")

STEP=0
step() { STEP=$((STEP + 1)); printf '\n== step %d: %s\n' "$STEP" "$*"; }
shown() { printf '+'; printf ' %q' "$@"; printf '\n'; }
act() { shown "$@"; [ "$APPLY" = 1 ] || return 0; "$@" || stop "$*"; }
stop() {
    printf '\nSTOPPED at step %d: %s failed. What the steps before it removed stays removed: finish by hand by deploy/HowToDeployANewAgent.md section 7\n' "$STEP" "$*"
    exit 1
}

# held_root PATH: PATH is root's and written by no group and no other.
held_root() {
    local uid mode
    read -r uid mode < <(stat -c '%u %a' -- "$1" 2>/dev/null) || return 1
    [ "$uid" = 0 ] && (( (8#$mode & 8#022) == 0 ))
}
key() {  # key FILE: the value of a one-line record file, trimmed
    local v
    v=$(cat -- "$1" 2>/dev/null) || return 1
    v=$(printf '%s' "$v" | tr -d '[:space:]')
    [ -n "$v" ] && printf '%s' "$v"
}

printf 'take-down of agent %s, archive %s: %s\n' "$NAME" "$ARCHIVE" \
    "$([ "$APPLY" = 1 ] && echo acting || echo 'plan only (--apply acts)')"
if [ "$APPLY" = 1 ]; then
    [ "$(id -u)" = 0 ] || die "--apply is root's, the operator on an agent appliance"
fi

# ------------------------------------------------------------------ 1, the reads
step "read the agent's state and its paths, before anything is touched"
if getent passwd "weaver-$NAME-admincon" >/dev/null 2>&1; then
    die "weaver-$NAME-admincon stands, a per-agent connector an earlier create-agent.sh laid: this take-down removes no connector account, so take this agent down by hand (deploy/HowToDeployANewAgent.md section 7)"
fi
[ ! -e "$ARCHIVE" ] && [ ! -L "$ARCHIVE" ] || die "$ARCHIVE stands: the archive is a folder of its own"
AGENT_DIR='<agent-directory>' COORD='<coordination-root>' TERRITORY='<territory>'
if [ -d "$STACK" ]; then
    AGENT_DIR=$(key "$STACK/agent-directory") || die "the stack record $STACK names no agent-directory"
    COORD=$(key "$STACK/coordination-root") || die "the stack record $STACK names no coordination-root"
elif [ "$APPLY" = 1 ]; then
    die "no stack record at $STACK"
fi
if [ -e "$BASE/$NAME" ]; then
    TERRITORY=$(key "$BASE/$NAME/territory") || die "the root $BASE/$NAME names no territory"
    [ "$TERRITORY" = "$AGENT_DIR/$NAME/weaver-$NAME" ] \
        || die "the root's territory $TERRITORY is not the agent's own, $AGENT_DIR/$NAME/weaver-$NAME: this take-down touches no path outside that one agent's"
elif [ "$APPLY" = 1 ]; then
    die "no root at $BASE/$NAME: no agent $NAME is admitted here"
fi
for p in "$AGENT_DIR" "$COORD"; do
    case $p in /*|'<'*) ;; *) die "the stack record names a path that is not absolute: '$p'" ;; esac
done
if [ "$APPLY" = 1 ]; then
    for p in "$STACK" "$STACK/agent-directory" "$STACK/coordination-root" "$BASE" "$BASE/$NAME" "$BASE/$NAME/territory"; do
        held_root "$p" || die "$p is not held by root alone, and this step acts on the paths it names"
    done
fi
shown "$ADMIN" show "$NAME"
printf '   must answer state unloaded, with no constituent\n'
if [ "$APPLY" = 1 ]; then
    said=$(WEAVER_ADMIN_CONFIG="$BASE" "$ADMIN" show "$NAME" 2>&1 | tail -n 1)
    printf '%s\n' "$said"
    python3 -I -c '
import json, sys
try:
    d = json.loads(sys.argv[1])
except ValueError:
    sys.exit(1)
sys.exit(0 if isinstance(d, dict) and d.get("kind") == "state" and d.get("state") == "unloaded" and not d.get("constituents") else 1)
' "$said" || die "show does not name $NAME unloaded with no constituent: unload it first, and take nothing down while it runs or is in transition"
fi
RUN_DIR=$COORD/weaver.run/$NAME RUNTIME_DIR=$COORD/weaver-$NAME FOLDER=$AGENT_DIR/$NAME

# ------------------------------------------------------------------ 2, the archive
step "archive the agent's pieces into $ARCHIVE, as root, before anything is removed"
TAR=$ARCHIVE/agent-$NAME.tar
shown install -d -o root -g root -m 0700 "$ARCHIVE"
shown tar --create --file "$TAR" --numeric-owner --acls --xattrs -C / "${SUDO_RULE#/}" "${BASE#/}/$NAME" "${OWN#/}" "${TERRITORY#/}"
printf '   each piece where it stands, under umask 077; then listed back, and its sha256 written beside it\n'
if [ "$APPLY" = 1 ]; then
    install -d -o root -g root -m 0700 "$ARCHIVE" || stop "the archive's folder"
    pieces=()
    for p in "$SUDO_RULE" "$BASE/$NAME" "$OWN" "$TERRITORY"; do
        [ -e "$p" ] && pieces+=("${p#/}")
    done
    (umask 077 && tar --create --file "$TAR" --numeric-owner --acls --xattrs -C / "${pieces[@]}") || stop "the archive"
    tar --list --file "$TAR" >"$ARCHIVE/agent-$NAME.list" || stop "the archive's listing"
    grep -q "^${TERRITORY#/}/" "$ARCHIVE/agent-$NAME.list" || stop "the archive holds no territory"
    (cd "$ARCHIVE" && sha256sum "agent-$NAME.tar" >SHA256SUMS) || stop "the archive's sum"
fi

# ------------------------------------------------------------------ 3 to 7, the paths
step "remove the sudo rule, so the connector can run nothing more on the agent"
act rm -f -- "$SUDO_RULE"
step "remove the root, which ends the admission"
act rm -rf -- "$BASE/$NAME"
step "remove the agent's own folder, with any loop folder in it"
act rm -rf -- "$OWN"
step "remove the run and runtime directories"
act rm -rf -- "$RUN_DIR" "$RUNTIME_DIR"
step "remove the territory"
act rm -rf -- "$TERRITORY"

# ------------------------------------------------------------------ 8, /tmp
step "remove the agent's /tmp entries, chosen by owner among its accounts"
for u in "${ACCOUNTS[@]}"; do
    shown find /tmp -mindepth 1 -maxdepth 1 '(' -name 'weaver-*' -o -name 'torchinductor_weaver-*' ')' -user "$u" -exec rm -rf -- '{}' +
    [ "$APPLY" = 1 ] || continue
    getent passwd "$u" >/dev/null || continue
    find /tmp -mindepth 1 -maxdepth 1 \( -name 'weaver-*' -o -name 'torchinductor_weaver-*' \) -user "$u" \
        -exec rm -rf -- {} + || stop "the /tmp entries of $u"
done

# ------------------------------------------------------------------ 9, the quota
step "the home's quota limit, where the agent's folder carries its uid as its project"
printf '   read: findmnt -n -o OPTIONS,TARGET --target %s, and lsattr -pd %s\n' "$FOLDER" "$FOLDER"
printf '   where the drive keeps prjquota and the mark is the uid of weaver-%s: setquota -P <uid> 0 0 0 0 <mount point>\n' "$NAME"
if [ "$APPLY" = 1 ] && [ -d "$FOLDER" ]; then
    uid=$(getent passwd "weaver-$NAME" | cut -d: -f3)
    options=$(findmnt -n -o OPTIONS --target "$FOLDER") && mount_point=$(findmnt -n -o TARGET --target "$FOLDER") \
        || stop "the mount of $FOLDER cannot be read (findmnt)"
    if [ -n "$uid" ] && [[ ",$options," == *,prjquota,* ]]; then
        read -r marked _ < <(lsattr -pd -- "$FOLDER" 2>/dev/null) && [[ "$marked" =~ ^[0-9]+$ ]] \
            || stop "the mark of $FOLDER cannot be read (lsattr)"
        if [ "$marked" = "$uid" ]; then
            shown setquota -P "$uid" 0 0 0 0 "$mount_point"
            setquota -P "$uid" 0 0 0 0 "$mount_point" || stop "setquota"
        else
            printf '   the folder carries project %s, not the uid %s: no cap of this agent stands\n' "$marked" "$uid"
        fi
    else
        printf '   no project quota on %s, or no account: no cap stands\n' "$mount_point"
    fi
fi

# ------------------------------------------------------------------ 10 to 12
step "remove the agent's accounts, the agent's own with its home"
for u in "${ACCOUNTS[@]}"; do
    if [ "$u" = "weaver-$NAME" ]; then flags=(-r); else flags=(); fi
    shown userdel "${flags[@]}" "$u"
    [ "$APPLY" = 1 ] || continue
    getent passwd "$u" >/dev/null || continue
    userdel "${flags[@]}" "$u" || stop "userdel $u"
done
step "remove the agent's groups"
for g in "${GROUPS_OF[@]}"; do
    shown groupdel "$g"
    [ "$APPLY" = 1 ] || continue
    getent group "$g" >/dev/null || continue
    groupdel "$g" || stop "groupdel $g"
done
step "remove the agent's folder, which the steps above emptied"
shown rmdir -- "$FOLDER"
if [ "$APPLY" = 1 ] && [ -e "$FOLDER" ]; then
    rmdir -- "$FOLDER" || stop "the agent's folder $FOLDER does not stand empty"
fi
printf '\ntake-down of agent %s: %s\n' "$NAME" "$([ "$APPLY" = 1 ] && echo done || echo 'planned')"
