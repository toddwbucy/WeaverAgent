"""runner: runs one wire file on the loop's end of the loop seam, per
weaver-harness-loop-contract section 7 (docs/crates/contracts/ in this repository).
Standard library only.

    python3 -I -B -S runner.py <wire file>

The loop's end of the pair is descriptor 3. The runner reads the wire file once, hashes
those bytes, checks their grammar, and compiles from them, never from the path again. It
sends the hello with that digest, then calls on_turn(board, text) once per pass until
the harness closes its end, when it exits 0.

Where on_turn raises (anything at all: SystemExit, and the board's Mistake for a frame
it would not send, among the rest) or returns without ending its pass, the runner names it on
standard error, reads the returns of any asks still outstanding, and refuses the pass,
so the harness answers ChannelLost, and the next pass runs. Where the harness breaks
the contract, the runner names it and exits 1, and the closed pair is the harness's
next read.

The grammar check helps the author. It is not the boundary: the walls the worker lays
around the loop are (act 2)."""

import ast
import hashlib
import importlib.util
import os
import socket
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
IMPORTS = ("weaver_board", "json", "re")
BANNED = frozenset(("open", "exec", "eval", "compile", "__import__", "input", "globals",
                    "locals", "vars", "getattr", "setattr", "delattr", "hash", "id"))
FD = 3


def board_module():
    """weaver_board beside this file, loaded by its path, since -I keeps this folder off
    sys.path, and registered so a wire file's import weaver_board finds it."""
    if "weaver_board" not in sys.modules:
        spec = importlib.util.spec_from_file_location("weaver_board", os.path.join(HERE, "weaver_board.py"))
        module = importlib.util.module_from_spec(spec)
        sys.modules["weaver_board"] = module
        spec.loader.exec_module(module)
    return sys.modules["weaver_board"]


class Grammar(Exception):
    """The wire file holds something section 7 does not allow, named by its line."""


def _scalar(node):
    if isinstance(node, ast.Constant):
        return isinstance(node.value, (str, int, float, bool)) or node.value is None
    # One leading minus over an integer or a float, and nothing else: no plus, no second
    # minus, no minus over True or False.
    return (isinstance(node, ast.UnaryOp) and isinstance(node.op, ast.USub)
            and isinstance(node.operand, ast.Constant)
            and type(node.operand.value) in (int, float))


def _literal(node):
    if isinstance(node, ast.Tuple):
        return all(_scalar(e) for e in node.elts)
    return _scalar(node)


def _dunder(name):
    return name.startswith("__")


def _names_bound(inner, function):
    """The names a node inside a function reads, binds or defines."""
    if isinstance(inner, ast.Name):
        return [inner.id]
    if isinstance(inner, ast.arg):
        return [inner.arg]
    if isinstance(inner, (ast.FunctionDef, ast.AsyncFunctionDef)) and inner is not function:
        return [inner.name]
    if isinstance(inner, (ast.ExceptHandler, ast.MatchAs, ast.MatchStar)) and inner.name:
        return [inner.name]
    if isinstance(inner, ast.MatchMapping) and inner.rest:
        return [inner.rest]
    return []


def check_grammar(tree):
    """Section 7's grammar, as what is allowed. Raises Grammar naming the first line
    outside it."""
    assigned = set()
    defined = set()
    for n, node in enumerate(tree.body):
        where = "line %d" % node.lineno
        if n == 0 and isinstance(node, ast.Expr) and isinstance(node.value, ast.Constant) \
                and isinstance(node.value.value, str):
            continue
        if isinstance(node, ast.Import):
            for alias in node.names:
                if alias.name not in IMPORTS or alias.asname is not None:
                    raise Grammar("%s: import %s is not import weaver_board, json or re, whole" % (where, alias.name))
            continue
        if isinstance(node, ast.Assign):
            if len(node.targets) != 1 or not isinstance(node.targets[0], ast.Name):
                raise Grammar("%s: a top-level assignment is to one name" % where)
            name = node.targets[0].id
            if _dunder(name):
                raise Grammar("%s: the name %s begins with two underscores" % (where, name))
            if name in assigned or name in defined:
                raise Grammar("%s: the name %s is assigned twice" % (where, name))
            if not _literal(node.value):
                raise Grammar("%s: the constant %s is not a literal" % (where, name))
            assigned.add(name)
            continue
        if isinstance(node, ast.FunctionDef):
            if node.decorator_list:
                raise Grammar("%s: the function %s carries a decorator" % (where, node.name))
            if _dunder(node.name):
                raise Grammar("%s: the name %s begins with two underscores" % (where, node.name))
            if node.name in assigned or node.name in defined:
                raise Grammar("%s: the name %s is defined twice" % (where, node.name))
            for inner in ast.walk(node):
                line = "line %d" % getattr(inner, "lineno", node.lineno)
                if isinstance(inner, (ast.Global, ast.Nonlocal)):
                    raise Grammar("%s: global or nonlocal inside a function" % line)
                if isinstance(inner, (ast.Import, ast.ImportFrom)):
                    raise Grammar("%s: an import inside a function" % line)
                if isinstance(inner, ast.ClassDef):
                    raise Grammar("%s: a class inside a function" % line)
                if isinstance(inner, ast.Name) and inner.id in BANNED:
                    raise Grammar("%s: the name %s" % (line, inner.id))
                for bound in _names_bound(inner, node):
                    if _dunder(bound):
                        raise Grammar("%s: the name %s begins with two underscores" % (line, bound))
                if isinstance(inner, ast.Attribute) and inner.attr.startswith("_"):
                    raise Grammar("%s: the attribute %s" % (line, inner.attr))
                if isinstance(inner, ast.Attribute) and not isinstance(inner.ctx, ast.Load):
                    raise Grammar("%s: an assignment to the attribute %s" % (line, inner.attr))
                if (isinstance(inner, ast.Subscript) and not isinstance(inner.ctx, ast.Load)
                        and (isinstance(inner.value, ast.Attribute)
                             or isinstance(inner.value, ast.Name) and inner.value.id in IMPORTS)):
                    raise Grammar("%s: an assignment into an attribute or a module" % line)
            # After the walk, so a banned name inside a default keeps its own refusal.
            for inner in ast.walk(node):
                if isinstance(inner, ast.arguments):
                    for default in inner.defaults + [d for d in inner.kw_defaults if d is not None]:
                        if not _literal(default):
                            raise Grammar("line %d: a default argument that is not a literal"
                                          % default.lineno)
            defined.add(node.name)
            continue
        raise Grammar("%s: a %s at the top level" % (where, type(node).__name__))
    on_turn = [node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == "on_turn"]
    if not on_turn:
        raise Grammar("no on_turn(board, text)")
    args = on_turn[0].args
    if [a.arg for a in args.args] != ["board", "text"] or args.vararg or args.kwarg \
            or args.kwonlyargs or args.posonlyargs or args.defaults:
        raise Grammar("line %d: on_turn takes exactly (board, text)" % on_turn[0].lineno)


def load(path):
    """Read the wire file once: its digest, and its on_turn compiled from those bytes."""
    with open(path, "rb") as f:
        data = f.read()
    digest = hashlib.sha256(data).hexdigest()
    try:
        # Decoded here and parsed as text, so a coding line cannot name another encoding.
        # A leading byte-order mark is UTF-8's own and is dropped, as the bytes parse did.
        text = data.decode("utf-8-sig")
    except UnicodeDecodeError as e:
        raise Grammar("the wire file is not UTF-8: %s" % e)
    try:
        tree = ast.parse(text, filename=path)
    except (SyntaxError, ValueError) as e:
        raise Grammar("the wire file does not parse: %s" % e)
    check_grammar(tree)
    board_module()
    namespace = {"__name__": "wires"}
    exec(compile(tree, path, "exec"), namespace)
    return digest, namespace["on_turn"]


def serve(sock, digest, on_turn, err=sys.stderr):
    """Send the hello, then run one pass per pass frame until the harness closes its
    end. Returns the exit status."""
    wb = board_module()
    wb.send_frame(sock, wb.encode({"kind": "hello", "version": wb.VERSION, "wires_sha256": digest}))
    board = wb.Board(sock)
    expected = 1
    while True:
        try:
            frame = wb.read_frame(wb.recv_frame(sock))
            if frame.get("kind") != "pass":
                raise wb.Refused("a frame of the kind %r between passes" % (frame.get("kind"),))
            if set(frame) != {"kind", "pass", "text"} or frame["pass"] != expected \
                    or not wb.is_int(frame["pass"]) or not isinstance(frame["text"], str):
                raise wb.Refused("a pass frame that is not pass %d with its text" % expected)
        except wb.Closed:
            return 0
        except wb.Refused as e:
            print("weaver-loop runner: the harness broke the contract: %s" % e, file=err)
            return 1
        expected += 1
        board._open(frame["pass"])
        try:
            on_turn(board, frame["text"])
            if not board.ended:
                raise wb.Limit("on_turn returned without ending its pass")
        except (wb.Refused, wb.Closed) as e:
            print("weaver-loop runner: the seam broke in pass %d: %s" % (frame["pass"], e), file=err)
            return 1
        except BaseException as e:  # the wires' own failure, whatever it raises: the pass is refused
            if board.ended:
                print("weaver-loop runner: on_turn raised after pass %d ended: %s: %s"
                      % (frame["pass"], type(e).__name__, e), file=err)
                continue
            print("weaver-loop runner: pass %d is refused: %s: %s" % (frame["pass"], type(e).__name__, e), file=err)
            try:
                for tag in board.outstanding:
                    board.wait(tag)
                if not board.ended:
                    board.refuse()
            except (wb.Refused, wb.Closed) as e:
                print("weaver-loop runner: the seam broke in pass %d: %s" % (frame["pass"], e), file=err)
                return 1


def main(argv):
    if len(argv) != 2:
        print("usage: runner.py <wire file>", file=sys.stderr)
        return 2
    try:
        digest, on_turn = load(argv[1])
    except (OSError, Grammar) as e:
        print("weaver-loop runner: %s" % e, file=sys.stderr)
        return 1
    sock = socket.socket(fileno=FD)
    # O_NONBLOCK lives on the open file description, which the descriptor shares with
    # whoever made the pair: the runner sets the mode it reads in, never inherits it.
    sock.setblocking(True)
    try:
        return serve(sock, digest, on_turn)
    finally:
        sock.close()


if __name__ == "__main__":
    sys.exit(main(sys.argv))
