"""Reproduce the text-only pinned-template cases with Jinja2 3.1.6.

Run this development fixture generator with the pinned tokenizer directory.
No template is evaluated by the agent runtime.
"""

import argparse
import hashlib
import json
from pathlib import Path
from jinja2.sandbox import ImmutableSandboxedEnvironment
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("tokenizer_directory", type=Path)
root = parser.parse_args().tokenizer_directory
raw=(root/'chat_template.jinja').read_bytes()
assert hashlib.sha256(raw).hexdigest()=='ab7813c3abdd9cb655905a410728b26c7884eca45ddfab8d9f931553485a7862'
# Transformers v4.57.1 utils/chat_template_utils.py uses these two settings.
# The text-only fixture needs none of its tool-schema or tracking extensions.
template=ImmutableSandboxedEnvironment(trim_blocks=True,lstrip_blocks=True).from_string(raw.decode())
def render(messages,generation):
 return template.render(messages=messages,tools=[],enable_thinking=False,
  truncate_history_thinking=True,add_generation_prompt=generation)
def m(role,content):return {'role':role,'content':content}
cases=[]
def case(name,messages,generation):
 cases.append({'name':name,'messages':messages,'generation':generation,
  'rendered':render(messages,generation)})
case('boot',[m('system','Use the exact prompt.\r\n')],False)
case('initial',[m('system','Use STE.'),m('user','Text: Read the file.')],True)
case('repair',[m('system','Use STE.'),m('user','Text: Read the file.\nPrior answer: Read file.\nRepair this answer.')],True)
case('empty_system',[m('system',''),m('user','Read.')],True)
case('implicit_system',[m('user','Read.')],True)
case('assistant_plain',[m('system','S'),m('user','U'),m('assistant','  Answer.  ')],False)
case('assistant_empty',[m('system','S'),m('assistant','')],False)
case('assistant_thought',[m('system','S'),m('assistant','<think>thought</think> Answer. ')],False)
case('historical_thought',[m('system','S'),m('assistant','<think>thought</think> Answer. '),m('user','Next.')],True)
case('historical_plain',[m('system','S'),m('assistant','  Answer.  '),m('user','Next.')],True)
case('repeated_thought_close',[m('system','S'),m('assistant','<think>a</think>b</think>c'),m('user','Next.')],True)
case('unclosed_thought',[m('system','S'),m('assistant','<think> unfinished '),m('user','Next.')],True)
case('close_without_open',[m('system','S'),m('assistant','</think> Answer. '),m('user','Next.')],True)
case('mid_system',[m('system','S'),m('user',' U\r\n'),m('system','Advice\n'),m('user','Next')],True)
case('python_trim',[m('system','S'),m('assistant','\u001c<think>x</think>answer\u001f')],False)
case('unicode_whitespace',[m('system','S'),m('assistant','\u0085<think>x</think>answer\u00a0')],False)
out={'source_revision':'3eab5a7bdfb5e669104f0014242972da1db50771',
 'template_sha256':hashlib.sha256(raw).hexdigest(),
 'oracle':'Jinja2 3.1.6, Transformers whitespace settings, tools empty, thinking false, history truncation true',
 'cases':cases}
Path(__file__).with_name('goldens.json').write_text(json.dumps(out,ensure_ascii=True,indent=2)+'\n')
print('Generated',len(cases),'pinned-template cases')
