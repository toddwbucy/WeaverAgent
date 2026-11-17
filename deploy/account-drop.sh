# shellcheck shell=bash
# account-drop.sh: the one way the deploy scripts run a command as another
# account (#590), sourced by create-agent.sh, verify-lifecycle.sh,
# verify-load.sh and update-stack.sh. The caller sets ROOT_OPERATOR: 1 where
# root runs the script, so the drop is runuser's and root needs no sudoers
# line of its own (#576), and 0 where the operator runs it, so the drop is the
# operator's sudo. decommission.sh's as_op stays apart: it drops root to the
# operator who ran it, who holds nothing the drop could leak.

# DROP_PATH: the PATH a command run as another account starts with, sudo's
# default secure_path.
DROP_PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
# DROP_RELAY_WAIT: the seconds the relays may run on after the drop exits.
DROP_RELAY_WAIT=5

# **as_account [--stdin FILE] ACCOUNT [VAR=value...] CMD...: CMD run as
# ACCOUNT, with that account's supplementary groups** (#576): `runuser -u`
# where root runs the script, and `sudo -n -u` where the operator does.
# **Nothing of this shell's reaches the account but what the call names**
# (#590), since while the drop runs any process of the account's uid can
# read its environment and open its descriptors through /proc:
# - runuser or sudo itself starts under `env -i` with DROP_PATH alone, so
#   neither root's environment nor what the box's env_keep names reaches any
#   process of the account's. setsid, runuser and sudo are found on this
#   shell's PATH before that clear, so a test's stand-ins stand.
# - CMD starts under a second `env -i`, with DROP_PATH and the leading
#   VAR=value pairs alone.
# - Its standard input is /dev/null, or FILE, which this shell opens. Every
#   other descriptor this shell holds is closed first, as sudo's closefrom
#   does and runuser does not, and where /proc/self/fd cannot be read,
#   nothing runs and the call fails.
# - Its standard output and standard error are pipes to two relays this
#   shell holds, one where the caller gave both the same file, so a caller's
#   `2>&1` keeps its order. Each relay is a `cat` run as a job of this shell
#   on a FIFO made in a directory of its own, `weaver-drop.XXXXXX` under
#   TMPDIR, mode 0700, and unlinked once both ends are open, so its status is
#   waited for by its pid in any bash (a process substitution's is, before
#   bash 5.3, only for the last one).
#   **After CMD exits, the relays get DROP_RELAY_WAIT seconds to drain**,
#   polled by the `read` builtin on a third FIFO no one writes: one still
#   held open then, by a process CMD left behind, is ended, and the call
#   fails saying so. A relay is running while drop_relay_stands says so,
#   below. Every caller captures the output or discards it, so no relay
#   waits on a slow reader.
# - It runs under `setsid -w`, in a session of its own with no controlling
#   terminal: before runuser in root mode, and inside sudo in the operator's,
#   since sudo finds the operator's cached credential by terminal, and with
#   none it keys the record to the parent process, so `sudo -n` would refuse.
#   Under sudo the second `env` runs after setsid, so the one process of the
#   account's that holds sudo's terminal is `timeout`, from sudo's exec to
#   setsid's. That window stays open as #605, split from #590
#   (deploy/HowToDeployANewAgent.md, "Running commands as an account").
# **An interrupt still reaches it**: setsid has moved it out of the
# terminal's process group, so the drop runs as a job of this shell, whose
# traps, set before it starts, pass INT, QUIT, TERM and HUP on to it as
# SIGTERM, and which then ends the relays at once and takes the signal
# itself, so the script's own traps run as they would have. runuser's parent
# passes SIGTERM on to its child. Under sudo, `timeout 0`, which never times
# out, passes on what sudo relays: under sudo's use_pty the command sudo
# starts leads its own process group, so a setsid started there would fork
# and its parent alone would take the signal, while timeout's child leads
# none, and setsid runs in it. That timeout is coreutils': BusyBox's counts 0
# down, and the stack puts no BusyBox applet on any PATH.
# The status is CMD's, else 1 where a relay was held open, else the standard
# output relay's, else the standard error relay's.

# **drop_relay_stands PID PARENT: a relay still running** (#590): the process
# PID stands, has not exited, and is PARENT's child. A relay that has exited
# and that bash has not reaped yet is a zombie, which `kill -0` answers for,
# and bash has left one unreaped through the whole drain, so a relay that had
# finished read as one held open and failed the call. Once reaped its number
# can pass to another process, which is not PARENT's relay. Both are read
# from /proc/PID/stat, which as_account needs /proc for in any case: the
# state and the parent are the two fields after the last `)`, since the field
# before them is the command's name in parentheses, which may hold spaces
# and parentheses of its own.
drop_relay_stands() {
  local stat state
  { read -r stat <"/proc/$1/stat"; } 2>/dev/null || return 1
  stat=${stat##*) }
  state=${stat%% *}
  stat=${stat#* }
  [ "${stat%% *}" = "$2" ] || return 1
  case $state in Z | X) return 1 ;; esac
}

as_account() {
  local account input=/dev/null named=() setsid runuser sudo relays
  if [ "${1:-}" = --stdin ]; then input=$2; shift 2; fi
  account=$1
  shift
  while [[ "${1:-}" =~ ^[A-Za-z_][A-Za-z0-9_]*= ]]; do named+=("$1"); shift; done
  if [ "$ROOT_OPERATOR" -eq 1 ]; then
    setsid=$(command -v setsid) && runuser=$(command -v runuser) || {
      echo "as_account: no command runs as $account, since setsid or runuser is not on PATH" >&2
      return 1
    }
    set -- env -i PATH="$DROP_PATH" "$setsid" -w "$runuser" -u "$account" -- \
      env -i PATH="$DROP_PATH" "${named[@]}" "$@"
  else
    sudo=$(command -v sudo) || {
      echo "as_account: no command runs as $account, since sudo is not on PATH" >&2
      return 1
    }
    set -- env -i PATH="$DROP_PATH" "$sudo" -n -u "$account" timeout 0 setsid -w \
      env -i PATH="$DROP_PATH" "${named[@]}" "$@"
  fi
  (
    set +e
    [ -d /proc/self/fd ] && [ -r /proc/self/fd ] || {
      echo "as_account: no command runs as $account, since /proc/self/fd cannot be read" \
           "to close the descriptors this shell holds" >&2
      exit 1
    }
    exec <"$input" || exit 1
    for fd in /proc/self/fd/*; do
      fd=${fd##*/}
      case $fd in 0|1|2|*[!0-9]*) ;; *) eval "exec $fd>&-" ;; esac
    done
    merged=0
    [ ! /proc/self/fd/1 -ef /proc/self/fd/2 ] || merged=1
    relays=$(mktemp -d "${TMPDIR:-/tmp}/weaver-drop.XXXXXX") && mkfifo -m 600 -- "$relays/out" "$relays/err" "$relays/tick" || {
      echo "as_account: no command runs as $account, since its relays could not be made" >&2
      [ -z "$relays" ] || rm -rf -- "$relays"
      exit 1
    }
    err_pid=
    cat <"$relays/out" &
    out_pid=$!
    if [ "$merged" -eq 0 ]; then
      cat <"$relays/err" >&2 &
      err_pid=$!
    fi
    exec {out}>"$relays/out"
    [ "$merged" -eq 1 ] || exec {err}>"$relays/err"
    exec {tick}<>"$relays/tick"
    rm -rf -- "$relays"
    drop= sig=
    trap 'sig=INT; [ -z "$drop" ] || kill -TERM "$drop"' INT
    trap 'sig=QUIT; [ -z "$drop" ] || kill -TERM "$drop"' QUIT
    trap 'sig=TERM; [ -z "$drop" ] || kill -TERM "$drop"' TERM
    trap 'sig=HUP; [ -z "$drop" ] || kill -TERM "$drop"' HUP
    if [ "$merged" -eq 0 ]; then
      "$@" <&0 >&"$out" 2>&"$err" {out}>&- {err}>&- {tick}>&- &
    else
      "$@" <&0 >&"$out" 2>&1 {out}>&- {tick}>&- &
    fi
    drop=$!
    [ -z "$sig" ] || kill -TERM "$drop"
    until wait "$drop"; rc=$?; ! kill -0 "$drop" 2>/dev/null; do :; done
    exec {out}>&-
    [ "$merged" -eq 1 ] || exec {err}>&-
    for ((n = 0; n < DROP_RELAY_WAIT * 100; n++)); do
      [ -z "$sig" ] || break
      drop_relay_stands "$out_pid" "$BASHPID" \
        || { [ -n "$err_pid" ] && drop_relay_stands "$err_pid" "$BASHPID"; } || break
      read -r -t 0.01 -u "$tick"
    done
    exec {tick}>&-
    held=0
    for relay in "$out_pid" $err_pid; do
      if drop_relay_stands "$relay" "$BASHPID"; then
        kill -TERM "$relay" 2>/dev/null
        [ -n "$sig" ] || held=1
      fi
    done
    wait "$out_pid"
    out_rc=$?
    err_rc=0
    if [ -n "$err_pid" ]; then wait "$err_pid"; err_rc=$?; fi
    # A subshell that reset its trap still ignores a QUIT it sends itself,
    # so where the signal does not end it, it ends with the signal's status.
    [ -z "$sig" ] || { trap - "$sig"; kill -s "$sig" "$BASHPID"; exit "$((128 + $(kill -l "$sig")))"; }
    [ "$held" -eq 0 ] || echo "as_account: the command run as $account exited, and its output was" \
      "held open ${DROP_RELAY_WAIT}s past its exit by a process it left, so the call fails" >&2
    [ "$rc" -eq 0 ] || exit "$rc"
    [ "$held" -eq 0 ] || exit 1
    [ "$out_rc" -eq 0 ] || exit "$out_rc"
    exit "$err_rc"
  )
}
