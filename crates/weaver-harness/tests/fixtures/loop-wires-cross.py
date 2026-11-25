"""The cross-language test's wires (src/loop_relay/tests.rs, the brief of 2026-11-25 item
10). "refuse" refuses its pass. "recall" asks a recall, whose return the seat makes past
64 KiB, and answers with a turn past 64 KiB that names the length it received. Any other
text asks seated and turns with the text."""

PAD = 100000


def say(text):
    return [{"role": "user", "content": [{"type": "text", "text": text}]}]


def on_turn(board, text):
    if text == "refuse":
        board.refuse()
        return
    if text == "recall":
        recalled = board.recall(4)
        received = len(recalled[0]["pairs"][0][1])
        board.turn(say("received %d " % received + "w" * PAD))
        return
    board.turn(say("%s, seated %s" % (text, board.seated())))
