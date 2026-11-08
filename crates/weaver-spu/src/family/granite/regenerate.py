"""Reproduce the text-only pinned-template cases with Jinja2 3.1.6.

Run this development fixture generator with the pinned tokenizer directory.
No template is evaluated by the agent runtime.
"""

import argparse
import hashlib
import json
from pathlib import Path

import jinja2
from jinja2.sandbox import ImmutableSandboxedEnvironment

assert jinja2.__version__ == "3.1.6", jinja2.__version__

SOURCE_REVISION = "9e668ce1c538387ef24d3644e9b0606647762636"
TEMPLATE_SHA256 = "f0ba43f79b3cabca5e5a7584c77aec92f49feae45cdf7779c87d9fc54cd90258"

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("tokenizer_directory", type=Path)
root = parser.parse_args().tokenizer_directory
raw = (root / "chat_template.jinja").read_bytes()
assert hashlib.sha256(raw).hexdigest() == TEMPLATE_SHA256

# Transformers utils/chat_template_utils.py uses these two settings.
# The text-only fixture needs none of its tool-schema or tracking extensions.
template = ImmutableSandboxedEnvironment(trim_blocks=True, lstrip_blocks=True).from_string(
    raw.decode()
)


def render(messages, generation):
    # The R1 variables: thinking off, no tools. Transformers passes
    # tools=None when none are given. The other switches keep their
    # template defaults, as rewriter_score.py leaves them.
    return template.render(
        messages=messages, tools=None, enable_thinking=False, add_generation_prompt=generation
    )


def m(role, content):
    return {"role": role, "content": content}


cases = []


def case(name, messages, generation):
    cases.append(
        {
            "name": name,
            "messages": messages,
            "generation": generation,
            "rendered": render(messages, generation),
        }
    )


# The R1 layout: one system turn, one user turn, the thinking-off opener.
case("system_user", [m("system", "Use STE."), m("user", "<<<TEXT\nRead the file.\nTEXT>>>")], True)
# The template inserts an empty system turn when none leads.
case("user_only", [m("user", "Read.")], True)
# The template prefixes every assistant turn with an empty thought, the
# turn before the last user turn and the turn after it alike.
case(
    "history",
    [m("system", "S"), m("user", "U"), m("assistant", "Answer."), m("user", "Next.")],
    True,
)
case("assistant_last", [m("system", "S"), m("user", "U"), m("assistant", "Answer.")], False)
# The template keeps only what follows the last </think> in a turn before
# the last user turn (truncate_history_thinking, default true).
case(
    "history_with_thought",
    [m("system", "S"), m("user", "U"), m("assistant", "<think>x</think>A1"), m("user", "Next.")],
    True,
)
# The template closes a tool response with a newline before the turn close.
case("tool_result", [m("system", "S"), m("user", "U"), m("tool", "Result.")], True)

out = {
    "source_revision": SOURCE_REVISION,
    "template_sha256": TEMPLATE_SHA256,
    "oracle": "Jinja2 3.1.6, Transformers whitespace settings, tools none, thinking false",
    "cases": cases,
}
Path(__file__).with_name("goldens.json").write_text(
    json.dumps(out, ensure_ascii=True, indent=2) + "\n"
)
print("Generated", len(cases), "pinned-template cases")
