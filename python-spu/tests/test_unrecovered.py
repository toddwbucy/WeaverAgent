"""python-spu's share of #31 (#137): the parse reports every call it could not recover
as data, the content keeps it as an `unrecovered_call` block, and the renderer renders
the block back as written, graded against the Rust oracle's `parse` and `render`, per
the decode contract as merged in #136 and weaver-spu-Spec section 5.

The report is compared whole except each `unparsed` reason's `detail`, which is the
family parser's own error text: serde's and Python's json differ, and the contract asks
that it be present and say what was refused, not that two parsers word it alike."""
import pytest

from python_spu.family import parse, render

OPEN, CLOSE = '<tool_call>', '</tool_call>'
EMISSIONS = [
    'plain text, no call',
    f'ok {OPEN}{{"name":"calc","arguments":{{"x":1}}}}{CLOSE} done',
    f'ok {OPEN}not json{CLOSE}',
    f'ok {OPEN}{{"name":"a"{OPEN}',
    f'{OPEN}{{"name":"a"{OPEN}{{"name":"read_file","arguments":{{}}}}{CLOSE}',
    f'{OPEN}x{CLOSE}mid{OPEN}y{CLOSE}',
    f'{OPEN}{{"arguments":{{}}}}{CLOSE}',
    f'{OPEN}{{"name":"calc"}}',
]


def comparable(report):
    out = []
    for entry in report:
        entry = dict(entry); reason = dict(entry['reason'])
        if reason['case'] == 'unparsed':
            assert reason.get('detail'), entry
            reason['detail'] = '<the parser\'s own>'
        entry['reason'] = reason; out.append(entry)
    return out


@pytest.mark.parametrize('emission', EMISSIONS)
def test_the_parse_meets_the_rust_parse(oracle, emission):
    """Perturbation: collapse an unrecovered fragment into text, or end a re-opened call
    at the closer, and the content or the report departs from the Rust's."""
    expected = oracle(op='parse', emission=emission)
    assert 'ok' in expected, expected
    content, report = parse(emission)
    assert content == expected['ok']['content']
    assert comparable(report) == comparable(expected['ok']['unrecovered'])


def test_an_unrecovered_call_renders_back_as_written(oracle):
    """Perturbation: render the block's fragment without its markers, and the rendering
    departs from the Rust's."""
    fragment = f'{OPEN}{{"name":"a"'
    messages = [{'role': 'assistant', 'content': [
        {'type': 'text', 'text': 'ok '},
        {'type': 'unrecovered_call', 'fragment': fragment}]}]
    expected = oracle(op='render', body=messages)
    assert 'ok' in expected, expected
    assert render(messages) == expected['ok']
    assert fragment in render(messages)


def test_a_user_message_carrying_an_unrecovered_call_refuses():
    from python_spu.family import MalformedDelta
    with pytest.raises(MalformedDelta):
        render([{'role': 'user', 'content': [{'type': 'unrecovered_call', 'fragment': OPEN}]}])


GENERATED = {'emission': 'ok <tool_call>{', 'finish': 'completed',
             'content': [{'type': 'text', 'text': 'ok '},
                         {'type': 'unrecovered_call', 'fragment': '<tool_call>{'}],
             'request': {}, 'measurement': {}, 'resident': 1, 'capacity': 2}
ENTRY = {'fragment': '<tool_call>{', 'reason': {'case': 'unclosed'},
         'grammar': {'family': 'qwen2', 'open': '<tool_call>', 'close': '</tool_call>'}}


@pytest.mark.parametrize('report', [[], [ENTRY],
    [dict(ENTRY, reason={'case': 'unparsed', 'detail': 'eof'}, grammar={'family': 'f', 'open': 'o'})]])
def test_the_answer_carries_the_report_as_the_rust_reads_it(oracle, report):
    """The generation's `unrecovered` member and the block round-trip through the Rust
    types, absent where the report is empty and `close` absent where the family closes
    nothing, per weaver-types-Spec section 4.4. Perturbation: emit `"unrecovered":[]` on
    an empty report, and the dump departs from the Rust's."""
    from python_spu.wire import ADAPTERS, dump
    body = dict(GENERATED, **({'unrecovered': report} if report else {}))
    answer = {'kind': 'generated', 'body': body}
    expected = oracle(op='round', type='TokenAnswer', body=answer)
    assert 'ok' in expected, expected
    assert dump(ADAPTERS['TokenAnswer'].validate_python(answer)) == expected['ok']
