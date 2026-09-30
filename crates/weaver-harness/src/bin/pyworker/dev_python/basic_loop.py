# Loop 1 at its minimum: one turn per crossing, the request as the one
# user message, nothing composed.
#
# THIS IS NOT THE FALLBACK, and the difference is the whole point. When no
# loop file resolves, `py_loop.drive` returns `fallback`, which builds the
# same one-message turn in Rust and never enters the interpreter. The two
# produce the same prompt and prove different things. A determinism result
# measured under the fallback says nothing about the Python crossing,
# because the crossing did not happen. Measured under this file it covers
# the interpreter attach, the `Seat` proxy, the marshalling of the delta
# into `seat.turn`, and the marshalling of the outcome back, which is the
# code every richer loop is built on.
#
# THE SYSTEM PROMPT IS NOT HERE. It is the session's first holding in state,
# seeded from the declaration's `identity:` block on the session's first
# load and read from the store at every load after, seated by the Open
# directive and standing in the resident prefix before this loop runs at
# all. A loop that added its own would stack a
# second system message on the agent's own voice, which is what the
# compiled `dev_loop` does and why a box running it renders a different
# prompt from a box running this.
#
# WHAT IS DELIBERATELY ABSENT, and why each absence is the experiment's
# rather than an omission. The seat offers nine calls and this loop uses
# one. `session_shape` and the continuity line it feeds are state, and the
# point of this arm is the math under the loop code before state enters.
# `fullness` and `flush` are context management, which cannot be measured
# until a turn count reaches pressure. `recall` and `classify` reach organs
# this arm does not exercise. `elide` is context management like the
# flush, and `score` wants a task this arm does not run. Each is available
# and none is wired, so a
# later arm adds one at a time against a baseline taken without it.
#
# A crossing that runs no turn falls back to a plain unshaped turn, so the
# single `seat.turn` below is also the contract this file has to meet.


def drive(seat, text):
    seat.turn([{"role": "user", "text": text}])
