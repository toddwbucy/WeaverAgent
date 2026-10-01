#!/usr/bin/env python3
"""Send one turn to a loaded agent through its gate and print what comes back.

    deploy/turn.py <agent> "<text>"            one turn, answer on stdout
    deploy/turn.py <agent> "<text>" --raw      the answer line as the gate sent it

The gate's inbound shape, as the determinism matrix drives it: dial the agent's
world socket under its coordination root, read from the agent's own admin root
(`<WEAVER_ADMIN_CONFIG or /etc/weaver/admin>/<agent>/coordination-root`, default
`/run`), send one JSON line
`{"text": ...}`, read one line back. The gate admits by the dialer's uid, so
this runs as the operator the declaration's `allowed-uids` names, with no
sudo. It is the smallest client the loop has; a refusal to connect means the
agent is not loaded or the uid is not admitted, and the record of the turn is
the agent's trace, not this script's output.
"""
import json
import os
import socket
import sys
import time


def main() -> int:
    if len(sys.argv) < 3:
        print(__doc__, file=sys.stderr)
        return 2
    agent, text = sys.argv[1], sys.argv[2]
    # The name as admin's own check admits it, so it cannot walk the path
    # below out of the base.
    if not agent or not all(c.isascii() and (c.isalnum() or c in "-_") for c in agent):
        print(f"'{agent}' is not an agent name: ASCII letters, digits, - and _", file=sys.stderr)
        return 2
    raw = "--raw" in sys.argv[3:]
    root = "/run"
    base = os.environ.get("WEAVER_ADMIN_CONFIG") or "/etc/weaver/admin"
    try:
        with open(os.path.join(base, agent, "coordination-root"), encoding="utf-8") as f:
            root = f.read().strip() or root
    except OSError:
        pass
    path = os.path.join(root, f"weaver-{agent}", "gate.sock")
    # No existence check first: the socket's directory is the agent group's,
    # and a shell that joined that group after it started cannot see the path
    # it can still be told about. Connect, and read the error.
    started = time.monotonic()
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as s:
        try:
            s.connect(path)
        except FileNotFoundError:
            print(f"no gate at {path}: is {agent} loaded?", file=sys.stderr)
            return 1
        except PermissionError:
            print(f"refused at {path}: uid {os.getuid()} is not in the declaration's allowed-uids, "
                  f"or this shell is not yet in group weaver-{agent} (newgrp, or sg weaver-{agent} -c ...)",
                  file=sys.stderr)
            return 1
        s.sendall((json.dumps({"text": text}) + "\n").encode())
        line = s.makefile("r", encoding="utf-8").readline()
    elapsed = time.monotonic() - started
    if not line:
        print("the gate closed without an answer", file=sys.stderr)
        return 1
    if raw:
        sys.stdout.write(line)
        return 0
    try:
        answer = json.loads(line)
    except ValueError:
        sys.stdout.write(line)
        return 0
    text_out = answer.get("text") if isinstance(answer, dict) else None
    if text_out is None:
        print(json.dumps(answer, indent=2))
    else:
        print(text_out)
    meta = {k: v for k, v in answer.items() if k != "text"} if isinstance(answer, dict) else {}
    print(f"-- {elapsed:.1f}s  {json.dumps(meta)}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
