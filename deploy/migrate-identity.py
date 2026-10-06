#!/usr/bin/env python3
"""Move a declaration's inline identity into the prompt draft beside it.

    deploy/migrate-identity.py <agent.toml>            say what would move, change nothing
    deploy/migrate-identity.py <agent.toml> --apply    move it

The declaration grammar of 2026-10-06 carries no identity (weaver-types-Spec
section 2, on the operator's ruling that the system prompt is state), so a
declaration written by the create-agent.sh of before that date, which carried
the prompt as `[[spu-instruction.decoder.identity]]`, refuses its parse by
name under the admin that reads the new grammar. This moves the one thing
that can move losslessly: exactly one `system` message of exactly one text
block, whose text becomes `system-prompt.md` beside the declaration byte for
byte, the draft `deploy/turn.py <agent> --system` sends after the first load. The
identity's lines are then removed from the declaration, and the result is
re-parsed and compared to the original minus the identity, so a declaration
whose shape this script did not foresee is refused rather than mangled.

Exit 0 with nothing to do, or with the move made or planned; exit 2 naming
what cannot move, which is the operator's to do by hand per
deploy/HowToDeployANewAgent.md section 3. Run as the operator: the
declaration and the draft are the operator's own files.
"""
import json
import os
import re
import sys
import tomllib

# The gate's line bound, per weaver-gate-Spec section 4, measured as
# deploy/turn.py --system will send the draft: a prompt whose seeding line
# would pass it cannot be seeded, so it does not move, and the declaration is
# left for the operator to shorten by hand.
LINE_BOUND = 32 * 1024

HEADER = re.compile(r"^\s*\[\[\s*spu-instruction\.decoder\.identity(\.content)?\s*\]\]\s*$")
ANY_HEADER = re.compile(r"^\s*\[")
INLINE = re.compile(r"^\s*identity\s*=")


def refuse(message: str) -> int:
    print(f"migrate-identity: {message}", file=sys.stderr)
    return 2


def without_identity(declaration: dict) -> dict:
    copy = {k: (dict(v) if isinstance(v, dict) else v) for k, v in declaration.items()}
    decoder = copy["spu-instruction"]["decoder"] = dict(copy["spu-instruction"]["decoder"])
    decoder.pop("identity", None)
    return copy


def strip_identity(lines: list[str]) -> list[str]:
    """The lines of the declaration with the identity's tables and inline
    array removed: each identity table header and the lines up to the next
    header, and any `identity =` line under `[spu-instruction.decoder]`."""
    kept = []
    in_identity = False
    for line in lines:
        if HEADER.match(line):
            in_identity = True
            continue
        if in_identity and ANY_HEADER.match(line):
            in_identity = False
        if in_identity or INLINE.match(line):
            continue
        kept.append(line)
    return kept


def main() -> int:
    args = [a for a in sys.argv[1:] if a != "--apply"]
    apply = "--apply" in sys.argv[1:]
    if len(args) != 1:
        print(__doc__, file=sys.stderr)
        return 2
    path = args[0]
    try:
        with open(path, "rb") as fh:
            source = fh.read()
        declaration = tomllib.loads(source.decode("utf-8"))
    except (OSError, UnicodeDecodeError, tomllib.TOMLDecodeError) as e:
        return refuse(f"{path} does not read as a TOML declaration: {e}")
    decoder = declaration.get("spu-instruction", {}).get("decoder", {})
    if "identity" not in decoder:
        return 0
    identity = decoder["identity"]
    if not isinstance(identity, list):
        return refuse(f"{path}: spu-instruction.decoder.identity is not a list of messages")
    draft = os.path.join(os.path.dirname(os.path.abspath(path)), "system-prompt.md")
    if identity == []:
        text = None
    elif (len(identity) == 1 and isinstance(identity[0], dict)
          and identity[0].get("role") == "system"
          and isinstance(identity[0].get("content"), list) and len(identity[0]["content"]) == 1
          and isinstance(identity[0]["content"][0], dict)
          and identity[0]["content"][0].get("type") == "text"
          and isinstance(identity[0]["content"][0].get("text"), str)):
        # The text exactly as the declaration carried it, no newline added:
        # the seeding turn sends the file's bytes verbatim, and a prompt that
        # ended without a newline in the declaration seeds without one.
        text = identity[0]["content"][0]["text"]
    else:
        return refuse(f"{path}: the identity is not one system message of one text block, which is "
                      f"the only shape that moves losslessly into {draft}; move it by hand "
                      f"(deploy/HowToDeployANewAgent.md section 3)")
    if text is not None:
        line = json.dumps({"role": "system", "text": text}).encode()
        if len(line) > LINE_BOUND:
            return refuse(f"{path}: its identity's text would seed as a line of {len(line)} octets, past the "
                          f"gate's bound of {LINE_BOUND} octets (weaver-gate-Spec section 4), so it cannot be "
                          f"seeded through the gate; shorten it by hand (deploy/HowToDeployANewAgent.md "
                          f"section 3) before the install")
    if text is not None and os.path.lexists(draft):
        try:
            with open(draft, encoding="utf-8") as fh:
                standing = fh.read()
        except (OSError, UnicodeDecodeError) as e:
            return refuse(f"{draft} already stands and does not read: {e}")
        if standing != text:
            return refuse(f"{draft} already stands with other text than the declaration's identity; "
                          f"reconcile them by hand (deploy/HowToDeployANewAgent.md section 3)")
    stripped = strip_identity(source.decode("utf-8").splitlines(keepends=True))
    rewritten = "".join(stripped)
    try:
        parsed = tomllib.loads(rewritten)
    except tomllib.TOMLDecodeError as e:
        return refuse(f"{path}: removing the identity's lines leaves a document that does not parse ({e}); "
                      f"move it by hand (deploy/HowToDeployANewAgent.md section 3)")
    if parsed != without_identity(declaration):
        return refuse(f"{path}: removing the identity's lines would change something else in the "
                      f"declaration; move it by hand (deploy/HowToDeployANewAgent.md section 3)")
    if text is None:
        print(f"{path}: an empty identity is removed; no prompt to move")
    elif os.path.lexists(draft):
        print(f"{path}: the identity moves out, {draft} already holds its text")
    else:
        print(f"{path}: the identity's text moves into {draft}, to be seeded after the first load")
    if not apply:
        return 0
    if text is not None and not os.path.lexists(draft):
        fd = os.open(draft, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        with os.fdopen(fd, "w", encoding="utf-8") as fh:
            fh.write(text)
    with open(path, "w", encoding="utf-8") as fh:
        fh.write(rewritten)
    return 0


if __name__ == "__main__":
    sys.exit(main())
