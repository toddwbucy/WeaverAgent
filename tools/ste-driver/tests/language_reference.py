"""Regenerate language cases from pinned source tests without a model or gate."""

import ast
import copy
import hashlib
import inspect
import json
from pathlib import Path
import sys
from types import SimpleNamespace

reference = Path(sys.argv[1]).resolve()
ROOT = Path(__file__).resolve().parents[1]
assert __import__('unicodedata').unidata_version == '16.0.0'
for name, expected in {
    'scripts/ste_check.py': '185a186f4d637616f5e506bda55ff19f909aa3f8f7022e2a35d3b88d409d1b2c',
    'scripts/ste_convert.py': 'c1a9668766ec4cd67792772f3d070083c33d9939f0cd96a7df2578d6da0b7b66',
    'src/weaver_train/manifest.py': 'a112ff67fb8e60a5669dfedda76203ef4a7bbc5cf501125d375a5e2ea7384b6d',
    'tests/test_ste_check.py': '50f45e16dccb2c170a1c011c5900a59bfe284f4406ebceecdc8a3a0f48138acc',
}.items():
    if hashlib.sha256((reference / name).read_bytes()).hexdigest() != expected:
        raise SystemExit('Pinned source hash mismatch: ' + name)
sys.path.insert(0, str(reference / 'scripts'))
import ste_check as ste
import ste_convert as convert

original_build = ste.Lexicon.build
original_check = ste.check
original_repair = ste.repair_note
lexicons = {}
records = []
current = ''

def build(*args, **kwargs):
    bound = inspect.signature(original_build).bind(*args, **kwargs)
    bound.apply_defaults()
    lex = original_build(*args, **kwargs)
    data = dict(bound.arguments)
    data['approved'] = data.pop('approved_pos')
    data['caps_names'] = sorted(data['caps_names'])
    lexicons[id(lex)] = (lex, data)
    return lex

def capture(output, source, lex, limit_instruction=20, limit_description=25, names=None):
    result = original_check(output, source, lex, limit_instruction, limit_description, names)
    data = copy.deepcopy(lexicons[id(lex)][1])
    data['fixed'] = dict(lex.fixed)
    records.append({'name': f'{current}_{len(records)}', 'lexicon': data,
                    'output': output, 'source': source, 'limits': [limit_instruction, limit_description],
                    'names': sorted(names or []), 'result': result,
                    'note': original_repair(result, lex),
                    'sentences': ste.sentences(output), 'count_words': ste.count_words(output)})
    return result

ste.Lexicon.build = staticmethod(build)
ste.check = capture
tree = ast.parse((reference / 'tests/test_ste_check.py').read_text())
tree.body = [n for n in tree.body if isinstance(n, ast.FunctionDef)
             or isinstance(n, ast.Assign) and any(isinstance(t, ast.Name)
             and t.id in ('APPROVED', 'LEX', 'NOUN_LEX') for t in n.targets)]
env = {'ste': ste}
exec(compile(tree, 'pinned_checker_tests', 'exec'), env)
for name, value in list(env.items()):
    if name.startswith('test_'):
        current = name
        value()

current = 'unicode_and_boundaries'
for space in ('\x1c', '\x1d', '\x1e', '\x1f', '\x85', '\u2028', '\u2029', '\r\n'):
    capture('Examine the error.' + space + 'Then I will fix it.', 'text', env['LEX'])
for word in ('running', 'tested', 'fixing', 'fixes', 'orders', 'ordered', 'ordering', 'tests'):
    capture(f'The [[C1]] {word} the values.', '[[C1]]', env['NOUN_LEX'])
for n in (19, 20, 21, 24, 25, 26):
    capture('Examine ' + ' '.join(['error'] * n), 'text', env['LEX'])

guard_cases = []
examples = ['I will examine the function.\nThe result shows the error.', '  Too short.  ']
fake = SimpleNamespace(example_lines={line.strip() for s in examples for line in s.splitlines()
                                       if len(line.strip()) >= 20})
for output, source in [
    ('I will examine the function.', 'I will make one test.'),
    ('I will examine the function.', 'I will examine the function.'),
    ('Read [[C1]] <h4>.', 'Read [[C1]].'),
    ('Read <h4>.', 'Read <h4>.'),
    ('Read the file.', 'Read the file and examine the error with the test.'),
    ('', 'Read the file and examine the error with the test.'),
    ('One two three four five six seven eight nine.', ''),
    (' '.join(['word'] * 107), ' '.join(['word'] * 40)),
    (' '.join(['word'] * 7), ' '.join(['word'] * 40)),
    (' '.join(['word'] * 5), ' '.join(['word'] * 8)),
    (None, 'Read the file.'),
]:
    lex = env['LEX']
    result = {'ok': False, 'hard': {'missing': [True]}, 'soft': {}} if output is None \
        else original_check(output, source, lex)
    if output is not None: convert.Converter.guard(fake, result, output, source)
    note = original_repair(result, lex)
    if 'copied_example' in result['hard']:
        note += '\n- You copied text from an example. Rewrite only the text between <<<TEXT and TEXT>>>.'
    if 'word_ratio' in result['hard']:
        note += ('\n- Your rewrite is much longer or much shorter than the text. Keep all of the '
                 'information and add nothing.')
    data = copy.deepcopy(lexicons[id(lex)][1])
    data['fixed'] = dict(lex.fixed)
    guard_cases.append({'lexicon': data, 'output': output, 'source': source, 'examples': examples,
                        'result': result, 'notes': note.strip() or '- Obey the rules.'})
unique = []
for case in records + guard_cases:
    lexicon = case.pop('lexicon')
    if lexicon not in unique:
        unique.append(lexicon)
    case['lexicon'] = unique.index(lexicon)
result = {'revision': '9881da6cfbc92b08b31264883aea63b73aa7b694',
          'checker_sha256': hashlib.sha256((reference/'scripts/ste_check.py').read_bytes()).hexdigest(),
          'lexicons': unique, 'cases': records, 'guards': guard_cases}
dest = ROOT / 'tests' / 'language-reference.json'
dest.write_text(json.dumps(result, ensure_ascii=True, indent=2).replace('\x3b', r'\u003b') + '\n')
print(len(records), 'checker cases and', len(guard_cases), 'guard cases')
