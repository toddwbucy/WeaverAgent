#!/usr/bin/env python3
"""Send one turn to a loaded agent through its gate and print what comes back.

    deploy/turn.py <agent> "<text>"            one turn, answer on stdout
    deploy/turn.py <agent> "<text>" --raw      the answer line as the gate sent it
    deploy/turn.py <agent> --system            the seeding turn: the prompt draft
                                               as the system role, answer on stdout

The gate's inbound shape, as the determinism matrix drives it: dial the agent's
world socket under its coordination root, read from the agent's own admin root
(`<WEAVER_ADMIN_CONFIG or /etc/weaver/admin>/<agent>/coordination-root`, required, as
admin requires it), send one JSON line `{"text": ...}`, read one line back. The gate admits by the dialer's uid, so
this runs as the operator the declaration's `allowed-uids` names, with no
sudo. It is the smallest client the loop has; a refusal to connect means the
agent is not loaded or the uid is not admitted, and the record of the turn is
the agent's trace, not this script's output.

**`--system` is the seeding turn**, per weaver-gate-world-contract section 2 on the
operator's ruling of 2026-10-06 that the system prompt is state: it reads the
operator's draft, `<territory>/system-prompt.md` (the root's `territory` key, as
admin reads it; the file is root:weaver-<agent>-admin, 0640, and the operator reads it through the
access group `weaver-<agent>-admin`, on the ruling of 2026-10-07 that the whole
agent lives in its territory), and sends it as the one line
`{"role": "system", "text": ...}`. The territory is taken from the root's key and
derived from nothing: this script already reads the root's keys as the operator,
`coordination-root` among them, so the mechanism is the one it has. The harness admits a system line from the
operator's uid alone, authors the prompt as the session's prefix and turns on it,
so the model's first answer comes back and the prompt is state from then on. The
file is a draft: the agent holds the prompt the gate carried, in its state, and the
file is only what the operator will send next. Load, seed, save point, unload, and
the production session is the next load (deploy/HowToDeployANewAgent.md section 4).
"""
import grp
import json
import os
import socket
import sys
import time

# The gate's line bound, per weaver-gate-Spec section 4: a line of more than
# this many octets before the delimiter closes the connection unanswered, so a
# request is measured here, as the bytes it will send, and refused with its
# size rather than met as a closed socket.
LINE_BOUND = 32 * 1024


def read_key(base: str, agent: str, key: str) -> str | None:
    """One key of the agent's root, stripped, or None after naming what refused."""
    path = os.path.join(base, agent, key)
    try:
        with open(path, encoding="utf-8") as f:
            value = f.read().strip()
    except OSError as e:
        print(f"cannot read {path}: {e.strerror or e}", file=sys.stderr)
        return None
    if not value:
        print(f"{path} names no directory", file=sys.stderr)
        return None
    return value


def access_note(agent: str) -> str:
    """Whether this session holds the two groups reading the draft takes
    (#99 area 2 review, K10): the state group, the territory's own, for
    passage to it, and the access group, the draft's, for reading it."""
    held = set(os.getgroups()) | {os.getegid()}
    notes, absent = [], []
    for group, why in ((f"weaver-{agent}-state", "which passes the territory"),
                       (f"weaver-{agent}-admin", "through which the draft is read")):
        try:
            gid = grp.getgrnam(group).gr_gid
        except KeyError:
            absent.append(group)
            continue
        if gid not in held:
            notes.append(f"{group}, {why}")
    if absent:
        return f"no group {' or '.join(absent)} stands on this box, so the agent was not created here"
    if not notes:
        return (f"this session holds weaver-{agent}-state and weaver-{agent}-admin, so the territory's or "
                f"the file's group or mode is wrong")
    return (f"this session does not hold {' or '.join(notes)}; create-agent.sh adds the operator to both, "
            f"and a session that predates that needs a new login")


def main() -> int:
    args = sys.argv[1:]
    system = "--system" in args
    args = [a for a in args if a != "--system"]
    if not args or (not system and len(args) < 2):
        print(__doc__, file=sys.stderr)
        return 2
    agent = args[0]
    # The name as admin's own check admits it, so it cannot walk the path
    # below out of the base.
    if not agent or not all(c.isascii() and (c.isalnum() or c in "-_") for c in agent):
        print(f"'{agent}' is not an agent name: ASCII letters, digits, - and _", file=sys.stderr)
        return 2
    raw = "--raw" in args[1:]
    base = os.environ.get("WEAVER_ADMIN_CONFIG") or "/etc/weaver/admin"
    if system:
        # The draft stands in the agent's territory, the one admin's root
        # names, beside agent.toml. A text given with --system would be two
        # prompts, so the flag takes none.
        if any(not a.startswith("--") for a in args[1:]):
            print("--system reads the prompt draft and takes no text", file=sys.stderr)
            return 2
        directory = read_key(base, agent, "territory")
        if directory is None:
            return 1
        draft = os.path.join(directory, "system-prompt.md")
        # **The draft's bytes, as they stand**: read binary so no newline is
        # translated (a text-mode read turns CRLF and a lone CR into LF), and
        # decoded as strict UTF-8, refusing anything else by name, since the
        # harness's identity door judges UTF-8 and a replacement character
        # would be a silent change of the bytes (Codex on #92, round 4).
        try:
            with open(draft, "rb") as f:
                text = f.read().decode("utf-8")
        except PermissionError as e:
            # **The draft is read through the access group**, the file being
            # root:weaver-<agent>-admin 0640, past the territory the state
            # group passes, so a refusal names which of the two this session
            # lacks rather than leaving a bare EACCES.
            print(f"cannot read the prompt draft {draft}: {e.strerror or e}; {access_note(agent)}", file=sys.stderr)
            return 1
        except OSError as e:
            print(f"cannot read the prompt draft {draft}: {e.strerror or e}", file=sys.stderr)
            return 1
        except UnicodeDecodeError as e:
            print(f"the prompt draft {draft} is not UTF-8 ({e}), and the gate's line is UTF-8 by the world "
                  f"contract; fix the file", file=sys.stderr)
            return 1
        # The one rule for emptiness, the identity door's: the empty string
        # refuses, and a draft of whitespace alone seeds as its bytes, the
        # operator's choice as the declaration's was (Codex on #92, round 7).
        if text == "":
            print(f"the prompt draft {draft} is empty, and an empty prompt seeds nothing", file=sys.stderr)
            return 1
        request = {"role": "system", "text": text}
    else:
        request = {"text": args[1]}
    line = json.dumps(request).encode()
    if len(line) > LINE_BOUND:
        what = "the seeding line" if system else "the request line"
        print(f"{what} is {len(line)} octets, past the gate's bound of {LINE_BOUND} octets before the "
              f"delimiter (weaver-gate-Spec section 4); shorten it", file=sys.stderr)
        return 1
    # **`coordination-root` is required, so there is no default to fall back
    # to.** Admin refuses every verb on a root without it. A key this user
    # cannot read once fell back to /run, which sent the turn to a gate that is
    # not this agent's and reported none standing (Codex on #45). Absent,
    # unreadable and empty are each named.
    root = read_key(base, agent, "coordination-root")
    if root is None:
        return 1
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
        s.sendall(line + b"\n")
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
