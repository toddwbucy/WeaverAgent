"""Qwen2's exact ChatML renderer, compared with Rust fixtures."""
import json
TEMPLATE='<|im_start|>{role}\n{message}<|im_end|>\n'
OPENER='<|im_start|>assistant\n'
class MalformedDelta(ValueError): pass

# The registry's key, weaver-spu family/mod.rs `normalised_key` and `same_key`, ported:
# `_`, `-` and `.` are ignored and ASCII letters fold to lower case, every other byte
# standing as written. The served family is the native backend's `SERVED_ARCHITECTURE`.
SERVED_ARCHITECTURE='qwen2'
def normalised_key(spelled):
    return bytes(b+32 if 65<=b<=90 else b for b in spelled.encode() if b not in b'_-.').decode()
def same_key(left,right): return normalised_key(left)==normalised_key(right)

def compact(value): return json.dumps(value,ensure_ascii=False,separators=(',',':'),sort_keys=True,allow_nan=False)
def render(messages):
    parts=[]
    for msg in messages:
        role=msg['role']; body=''
        for block in msg['content']:
            if role=='tool_result':
                if block['type']!='tool_result': raise MalformedDelta('tool_result role requires results')
                if body: body+='\n'
                body+='<tool_response>\n'+block['content']+'\n</tool_response>'
            elif block['type']=='text': body+=block['text']
            elif block['type']=='tool_call' and role=='assistant':
                try: args=json.loads(block['arguments'])
                except ValueError: args=block['arguments']
                if body: body+='\n'
                body+='<tool_call>\n'+compact({'name':block['name'],'arguments':args})+'\n</tool_call>'
            else: raise MalformedDelta('unlicensed block')
        parts.append(TEMPLATE.format(role='user' if role=='tool_result' else role,message=body))
    return ''.join(parts)

def parse(text):
    blocks=[]; failures=[]; rest=text
    while '<tool_call>' in rest:
        before,rest=rest.split('<tool_call>',1)
        if before: blocks.append({'type':'text','text':before})
        if '</tool_call>' in rest: fragment,rest=rest.split('</tool_call>',1)
        else: fragment,rest=rest,''
        try:
            value=json.loads(fragment)
            if not isinstance(value,dict) or not isinstance(value.get('name'),str) or not value['name']:
                raise ValueError('missing call name')
            blocks.append({'type':'tool_call','name':value['name'],'arguments':compact(value.get('arguments',{}))})
        except ValueError: failures.append(fragment)
    if rest: blocks.append({'type':'text','text':rest})
    return blocks,failures
