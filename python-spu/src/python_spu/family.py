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

def judge(model_type,width):
    """Whether this build serves a declared family at a device count, answered as the
    wire kind it refuses with, or None. The Rust SPU's native backend serves qwen2 alone
    (`judge_family`, `BackendDoesNotServe`, unreadable) at the widths the registry
    declares for it; this build serves one device, so a wider set is its own
    `device_cannot_admit`."""
    if not same_key(model_type,SERVED_ARCHITECTURE): return 'artifact_unreadable'
    if width!=1: return 'device_cannot_admit'
    return None

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
            elif block['type']=='unrecovered_call' and role=='assistant':
                # Rendered back as written, the fragment's bytes unchanged, per
                # weaver-spu-Spec section 5 (#137): a rebuilt context matches the emission.
                body+=block['fragment']
            elif block['type']=='tool_call' and role=='assistant':
                try: args=json.loads(block['arguments'])
                except ValueError: args=block['arguments']
                if body: body+='\n'
                body+='<tool_call>\n'+compact({'name':block['name'],'arguments':args})+'\n</tool_call>'
            else: raise MalformedDelta('unlicensed block')
        parts.append(TEMPLATE.format(role='user' if role=='tool_result' else role,message=body))
    return ''.join(parts)

OPEN,CLOSE='<tool_call>','</tool_call>'
GRAMMAR={'family':'qwen2','open':OPEN,'close':CLOSE}

def recover(inner):
    """The family's recovery of one call, weaver-spu qwen2.rs `recover`: the call block,
    or the parser's detail where it refuses, never a bare absence."""
    try: value=json.loads(inner)
    except ValueError as e: return None,f'not JSON: {e}'
    if not isinstance(value,dict) or 'name' not in value: return None,'no `name` member'
    if not isinstance(value['name'],str): return None,'`name` is not a string'
    if not value['name']: return None,'`name` is empty'
    return {'type':'tool_call','name':value['name'],'arguments':compact(value.get('arguments',{}))},None

def parse(text,ended_by=None):
    """weaver-spu family/mod.rs `scan` for qwen2, per weaver-spu-Spec section 5 and the
    decode contract as merged in #136: the content in emission order, an unrecovered call
    as an `unrecovered_call` block holding the fragment as written, markers included, and
    the report, one entry per such block with its reason and the grammar that judged it.
    The kernel judges `unclosed` before the family is asked: a second opener before any
    closer ends the open call there and scanning resumes at it, and a closer absent from
    the emission came only where it is the stop that ended the generation, `ended_by`,
    which qwen2's closer never is."""
    blocks=[]; report=[]; rest=text
    while (at:=rest.find(OPEN))>=0:
        if at: blocks.append({'type':'text','text':rest[:at]})
        from_open=rest[at:]; after_open=from_open[len(OPEN):]
        closed=after_open.find(CLOSE); reopened=after_open.find(OPEN)
        if reopened>=0 and (closed<0 or reopened<closed):
            end=len(OPEN)+reopened; inner=after_open[:reopened]; reason={'case':'unclosed'}
        elif closed>=0:
            end=len(OPEN)+closed+len(CLOSE); inner=after_open[:closed]; reason=None
        else:
            end=len(from_open); inner=after_open
            reason=None if ended_by==CLOSE else {'case':'unclosed'}
        written=from_open[:end]; rest=from_open[end:]
        if reason is None:
            block,detail=recover(inner)
            if block is not None:
                blocks.append(block); continue
            reason={'case':'unparsed','detail':detail}
        blocks.append({'type':'unrecovered_call','fragment':written})
        report.append({'fragment':written,'reason':reason,'grammar':dict(GRAMMAR)})
    if rest: blocks.append({'type':'text','text':rest})
    return blocks,report
