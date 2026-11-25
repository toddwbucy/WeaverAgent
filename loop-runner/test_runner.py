"""test_runner: the runner against weaver-harness-loop-contract section 7, standard library
only. Run as python3 -I -B test_runner.py; exits 1 on the first run with a failure."""
import atexit
import fcntl
import hashlib
import importlib.util
import io
import json
import os
import shutil
import socket
import subprocess
import sys
import tempfile
import threading

HERE = os.path.dirname(os.path.abspath(__file__))
spec = importlib.util.spec_from_file_location("runner", os.path.join(HERE, "runner.py"))
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)
wb = runner.board_module()

failed = []
passed = 0
SCRATCH = tempfile.mkdtemp(prefix="test-runner.")
atexit.register(shutil.rmtree, SCRATCH, True)


def check(name, ok):
    global passed
    if ok:
        passed += 1
        print("ok   " + name)
    else:
        failed.append(name)
        print("FAIL " + name)


def wire(text, name="wires.py"):
    path = os.path.join(SCRATCH, name)
    with open(path, "w") as f:
        f.write(text)
    return path


def grammar(text):
    """'' where the grammar takes text, else the refusal."""
    try:
        runner.load(wire(text))
    except runner.Grammar as e:
        return str(e)
    return ""


GOOD = '''"""A wire file."""
import weaver_board
import json
import re

KEEP = 0
WORDS = ("run", "runs")
LIMIT = 600
RATIO = 0.8
ON = True
NOTHING = None


def helper(board):
    return board.seated()


def on_turn(board, text):
    if helper(board):
        board.turn([{"role": "user", "content": [{"type": "text", "text": text}]}])
    else:
        board.refuse()
'''

# 1. The grammar, as what is allowed (section 7).
check("a wire file inside the grammar is taken", grammar(GOOD) == "")
for name, text, refusal in [
    ("import os", GOOD.replace("import re", "import os"), "line 4: import os is not import weaver_board, json or re, whole"),
    ("an aliased import", GOOD.replace("import re", "import re as r"), "line 4: import re is not import weaver_board, json or re, whole"),
    ("from json import loads", GOOD.replace("import re", "from json import loads"), "line 4: a ImportFrom at the top level"),
    ("a call assigned at the top level", GOOD.replace("KEEP = 0", "KEEP = len('x')"), "line 6: the constant KEEP is not a literal"),
    ("a list assigned at the top level", GOOD.replace("KEEP = 0", "KEEP = [0]"), "line 6: the constant KEEP is not a literal"),
    ("a name assigned twice", GOOD.replace("LIMIT = 600", "LIMIT = 600\nLIMIT = 7"), "line 9: the name LIMIT is assigned twice"),
    ("two names at once", GOOD.replace("KEEP = 0", "KEEP = LIMIT2 = 0"), "line 6: a top-level assignment is to one name"),
    ("a statement at the top level", GOOD + "\nprint('x')\n", "line 24: a Expr at the top level"),
    ("a class at the top level", GOOD + "\nclass C:\n    pass\n", "line 24: a ClassDef at the top level"),
    ("an async function", GOOD + "\nasync def f():\n    pass\n", "line 24: a AsyncFunctionDef at the top level"),
    ("a decorator", GOOD.replace("def helper", "@staticmethod\ndef helper"), "line 15: the function helper carries a decorator"),
    ("global inside a function", GOOD.replace("    return board.seated()", "    global KEEP\n    return board.seated()"), "line 15: global or nonlocal inside a function"),
    ("an import inside a function", GOOD.replace("    return board.seated()", "    import os\n    return board.seated()"), "line 15: an import inside a function"),
    ("a class inside a function", GOOD.replace("    return board.seated()", "    class C:\n        pass\n    return board.seated()"), "line 15: a class inside a function"),
    ("open inside a function", GOOD.replace("    return board.seated()", "    open('/x')\n    return board.seated()"), "line 15: the name open"),
    ("getattr inside a function", GOOD.replace("    return board.seated()", "    getattr(board, 'x')\n    return board.seated()"), "line 15: the name getattr"),
    ("eval in a default argument", GOOD.replace("def helper(board):", "def helper(board, x=eval('1')):"), "line 14: the name eval"),
    ("an underscore attribute", GOOD.replace("    return board.seated()", "    return board._sock"), "line 15: the attribute _sock"),
    ("a function defined twice", GOOD + "\n\ndef helper(board):\n    return 1\n", "line 25: the name helper is defined twice"),
    ("no on_turn", GOOD.replace("def on_turn(board, text):", "def other(board, text):"), "no on_turn(board, text)"),
    ("on_turn with three arguments", GOOD.replace("def on_turn(board, text):", "def on_turn(board, text, x):"), "line 18: on_turn takes exactly (board, text)"),
    ("on_turn with a default", GOOD.replace("def on_turn(board, text):", "def on_turn(board, text='x'):"), "line 18: on_turn takes exactly (board, text)"),
    ("a docstring that is not first", "KEEP = 0\n'''late'''\n" + GOOD.split('"""A wire file."""\n', 1)[1], "line 2: a Expr at the top level"),
    ("a file that does not parse", "def on_turn(board, text)\n", None),
]:
    said = grammar(text)
    check("the grammar refuses %s, naming the line (%r)" % (name, said),
          said == refusal if refusal is not None else said.startswith("the wire file does not parse"))

# 2. The file is read once: the digest is of the bytes compiled.
path = wire(GOOD.replace("LIMIT = 600", "LIMIT = 601"), "once.py")
real_check = runner.check_grammar


def rewrite_then_check(tree):
    # After the read, the file on disk changes: a second read would compile this.
    with open(path, "w") as f:
        f.write(GOOD.replace("LIMIT = 600", "LIMIT = 999"))
    return real_check(tree)


runner.check_grammar = rewrite_then_check
try:
    digest, on_turn = runner.load(path)
finally:
    runner.check_grammar = real_check
check("the wires are compiled from the bytes first read, not from the path again",
      on_turn.__globals__["LIMIT"] == 601)
check("the digest is the sha256 of those bytes",
      digest == hashlib.sha256(GOOD.replace("LIMIT = 600", "LIMIT = 601").encode()).hexdigest())


# 3. Serving: the hello, the passes, and the wires' own failures (section 7).
def harness_pair():
    loop_end, harness = socket.socketpair(socket.AF_UNIX, socket.SOCK_SEQPACKET)
    harness.settimeout(5)
    loop_end.settimeout(5)
    return loop_end, harness


def frame(harness):
    return wb.read_frame(wb.recv_frame(harness))


def send(harness, value):
    wb.send_frame(harness, json.dumps(value).encode())


def served(on_turn, script):
    """Run serve() on the loop's end in a thread, the harness's side scripted here."""
    loop_end, harness = harness_pair()
    err = io.StringIO()
    result = {}
    worker = threading.Thread(target=lambda: result.update(rc=runner.serve(loop_end, "d" * 64, on_turn, err)))
    worker.start()
    try:
        seen = script(harness)
    except (wb.Closed, wb.Refused, OSError) as e:
        # The harness's side met something its script did not: the check fails by name.
        seen = "the harness's side raised %s: %s" % (type(e).__name__, e)
        print("     " + seen)
    finally:
        harness.close()
        worker.join(10)
    return seen, result.get("rc"), err.getvalue()


_, on_turn = runner.load(wire(GOOD))


def two_passes(harness):
    seen = [frame(harness)]
    send(harness, {"kind": "pass", "pass": 1, "text": "hello"})
    seen.append(frame(harness))
    send(harness, {"kind": "return", "pass": 1, "tag": 0, "value": True})
    seen.append(frame(harness))
    send(harness, {"kind": "pass", "pass": 2, "text": "again"})
    seen.append(frame(harness))
    send(harness, {"kind": "return", "pass": 2, "tag": 0, "value": False})
    seen.append(frame(harness))
    return seen


seen, rc, err = served(on_turn, two_passes)
check("the hello carries the version and the digest", seen[0] == {"kind": "hello", "version": "weaver-loop/1", "wires_sha256": "d" * 64})
check("pass 1: the wires ask seated, then turn with the text",
      seen[1] == {"kind": "ask", "pass": 1, "tag": 0, "ask": "seated"}
      and seen[2] == {"kind": "ask", "pass": 1, "tag": 1, "ask": "turn",
                      "delta": [{"role": "user", "content": [{"type": "text", "text": "hello"}]}]})
check("pass 2: tags start again at 0, and the wires refuse",
      seen[3] == {"kind": "ask", "pass": 2, "tag": 0, "ask": "seated"} and seen[4] == {"kind": "refuse", "pass": 2})
check("the harness's end closing ends the runner with 0, nothing said", rc == 0 and err == "")


def one_pass(harness):
    frame(harness)
    send(harness, {"kind": "pass", "pass": 1, "text": "x"})
    seen = [frame(harness)]
    send(harness, {"kind": "pass", "pass": 2, "text": "y"})
    seen.append(frame(harness))
    return seen


def raising(board, text):
    raise ValueError("the wires failed")


seen, rc, err = served(raising, one_pass)
check("on_turn raising refuses the pass, names it, and the next pass still runs",
      seen == [{"kind": "refuse", "pass": 1}, {"kind": "refuse", "pass": 2}] and rc == 0
      and "pass 1 is refused: ValueError: the wires failed" in err)


def returning(board, text):
    board.seated


seen, rc, err = served(returning, one_pass)
check("on_turn returning without ending its pass refuses it",
      seen[0] == {"kind": "refuse", "pass": 1} and "on_turn returned without ending its pass" in err)


def raising_with_two_out(board, text):
    board.ask("seated")
    board.ask("first_turn")
    raise ValueError("mid-flight")


def answers_then_reads(harness):
    frame(harness)
    send(harness, {"kind": "pass", "pass": 1, "text": "x"})
    seen = [frame(harness), frame(harness)]
    send(harness, {"kind": "return", "pass": 1, "tag": 0, "value": True})
    send(harness, {"kind": "return", "pass": 1, "tag": 1, "value": False})
    seen.append(frame(harness))
    return seen


seen, rc, err = served(raising_with_two_out, answers_then_reads)
check("a raise with asks outstanding reads their returns before it refuses",
      [f["kind"] for f in seen] == ["ask", "ask", "refuse"])


def after_turn_raising(board, text):
    board.turn([{"role": "user", "content": [{"type": "text", "text": text}]}])
    raise ValueError("late")


seen, rc, err = served(after_turn_raising, one_pass)
check("a raise after the turn sends nothing more, and says so",
      isinstance(seen, list) and seen[0]["ask"] == "turn" and seen[1]["ask"] == "turn" and "on_turn raised after pass 1 ended" in err)


def wrong_between(harness):
    frame(harness)
    send(harness, {"kind": "return", "pass": 1, "tag": 0, "value": None})
    return None


seen, rc, err = served(on_turn, wrong_between)
check("a frame other than pass between passes ends the runner with 1, named",
      rc == 1 and "a frame of the kind 'return' between passes" in err)


def skipped_pass(harness):
    frame(harness)
    send(harness, {"kind": "pass", "pass": 2, "text": "x"})
    return None


seen, rc, err = served(on_turn, skipped_pass)
check("a pass numbered out of order ends the runner with 1", rc == 1 and "not pass 1" in err)


def broken_return(harness):
    frame(harness)
    send(harness, {"kind": "pass", "pass": 1, "text": "x"})
    frame(harness)
    harness.sendmsg([b'{"kind":"return","pass":1,"tag":0,"value":true,"value":false}'])
    return None


seen, rc, err = served(on_turn, broken_return)
check("a broken return inside a pass ends the runner with 1, named",
      rc == 1 and "the seam broke in pass 1" in err and "named twice" in err)

# 4. As deployed: python3 -I -B -S runner.py with the pair at descriptor 3.
# posix_spawn's dup2 action lands the pair at descriptor 3 without close-on-exec, and every
# other descriptor of this process is close-on-exec. The source sits above 3, so the dup2
# never meets itself.
loop_end, harness = harness_pair()
child_end = fcntl.fcntl(loop_end.fileno(), fcntl.F_DUPFD_CLOEXEC, 10)
loop_end.close()
wire_path = wire(GOOD, "deployed.py")
pid = os.posix_spawn("/usr/bin/python3",
                     ["/usr/bin/python3", "-I", "-B", "-S", os.path.join(HERE, "runner.py"), wire_path],
                     os.environ, file_actions=[(os.POSIX_SPAWN_DUP2, child_end, 3)])
os.close(child_end)
try:
    hello = frame(harness)
    send(harness, {"kind": "pass", "pass": 1, "text": "deployed"})
    asked = frame(harness)
    send(harness, {"kind": "return", "pass": 1, "tag": 0, "value": True})
    turned = frame(harness)
except (wb.Closed, wb.Refused, OSError) as e:
    print("     the harness's side raised %s: %s" % (type(e).__name__, e))
    hello, asked, turned = {}, {}, {"delta": [{"content": [{"text": None}]}]}
harness.close()
rc = os.waitstatus_to_exitcode(os.waitpid(pid, 0)[1])
check("as deployed, the runner finds weaver_board beside itself under -I -S and serves descriptor 3,"
      " handed non-blocking as harness_pair leaves it",
      hello.get("wires_sha256") == hashlib.sha256(GOOD.encode()).hexdigest()
      and asked.get("ask") == "seated" and turned["delta"][0]["content"][0]["text"] == "deployed" and rc == 0)
p = subprocess.run(["/usr/bin/python3", "-I", "-B", "-S", os.path.join(HERE, "runner.py")], capture_output=True)
check("with no wire file it prints its usage and exits 2", p.returncode == 2 and b"usage" in p.stderr)
p = subprocess.run(["/usr/bin/python3", "-I", "-B", "-S", os.path.join(HERE, "runner.py"),
                    wire(GOOD.replace("import re", "import os"), "bad.py")], capture_output=True)
check("a wire file outside the grammar exits 1 before any socket, naming the line",
      p.returncode == 1 and b"line 4: import os" in p.stderr)

print("== %d passed, %d failed" % (passed, len(failed)))
sys.exit(1 if failed else 0)
