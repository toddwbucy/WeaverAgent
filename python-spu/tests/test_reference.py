import math
import random
import pytest
from python_spu.wire import ADAPTERS,dump
from python_spu.sampling import derived_seed,distribution
from python_spu.family import render,MalformedDelta

DIRECTIVES=[
 {'kind':'open','session':'s','messages':[]},
 {'kind':'append_and_generate','turn':'t','delta':[{'role':'user','content':[{'type':'text','text':'hello'}]}]},
 {'kind':'cancel','turn':'t'}, {'kind':'flush','keep':2**64-1},
 {'kind':'elide','from':4,'to':7}, {'kind':'re_feed','turn':'t','rendered':'x','path':[0,2**32-1]}]
@pytest.mark.parametrize('body',DIRECTIVES)
def test_directives_roundtrip_rust_python(oracle,body):
    expected=oracle(op='round',type='TokenDirective',body=body)
    assert 'ok' in expected,expected
    actual=dump(ADAPTERS['TokenDirective'].validate_python(body))
    assert actual==expected['ok']
    assert oracle(op='round',type='TokenDirective',body=actual)==expected

@pytest.mark.parametrize('body',[
 {'kind':'flush','keep':-1},{'kind':'flush','keep':True},
 {'kind':'flush','keep':2**64},{'kind':'flush','keep':'1'},
 {'kind':'elide','from':0},{'kind':'re_feed','turn':'t','rendered':'','path':[-1]},
 {'kind':'open','session':'s','messages':[],'column_ask':1},
 {'kind':'append_and_generate','turn':1,'delta':[]}, {'kind':'bogus'}])
def test_rejected_payloads_agree(oracle,body):
    assert 'error' in oracle(op='round',type='TokenDirective',body=body)
    with pytest.raises(ValueError): ADAPTERS['TokenDirective'].validate_python(body)

def test_instruction_defaults_match_rust(oracle,instruction):
    body=dump(instruction)
    assert oracle(op='round',type='SpuInstruction',body=body)=={'ok':body}
    for key in ('surprisal-election','refeed-permission','column-permission'):
        body['decoder'].pop(key)
    assert dump(ADAPTERS['SpuInstruction'].validate_python(body))==oracle(op='round',type='SpuInstruction',body=body)['ok']
    body['decoder']['unexpected']=True
    assert 'error' in oracle(op='round',type='SpuInstruction',body=body)
    with pytest.raises(ValueError): ADAPTERS['SpuInstruction'].validate_python(body)

def test_seed_vectors_against_executing_rust(oracle):
    rng=random.Random(726)
    for i in range(100):
        seed=rng.randrange(2**64); generation=rng.randrange(2**64)
        turn=['t-1','turn-β','🌱',''][i%4]+str(i)
        assert oracle(op='seed',seed=seed,turn=turn,generation=generation)=={'ok':derived_seed(seed,turn,generation)}

@pytest.mark.parametrize('messages',[
 [],[{'role':'system','content':[{'type':'text','text':'be precise'}]}],
 [{'role':'assistant','content':[{'type':'text','text':'calling'},
     {'type':'tool_call','name':'shell','arguments':'{"z":1,"a":"β"}'}]}],
 [{'role':'tool_result','content':[{'type':'tool_result','content':'done'}, {'type':'tool_result','content':'again'}]}],
 [{'role':'assistant','content':[{'type':'tool_call','name':'shell','arguments':'bad JSON'}]}]])
def test_family_render_against_rust(oracle,messages):
    assert oracle(op='render',body=messages)=={'ok':render(messages)}

def test_invalid_role_block_pair_refuses_both(oracle):
    messages=[{'role':'user','content':[{'type':'tool_call','name':'x','arguments':'{}'}]}]
    assert 'error' in oracle(op='render',body=messages)
    with pytest.raises(MalformedDelta): render(messages)

@pytest.mark.parametrize('logits',[[0.,0.],[1000.,999.,-1000.],[-1000.,-1000.],[.1,.5,-.8,1.2]])
def test_measurement_against_rust(oracle,logits):
    p,lp,entropy=distribution(logits)
    got=oracle(op='measure',body=logits,token=0)['ok']
    assert entropy==pytest.approx(got['entropy'],abs=2e-4)
    assert -lp[0]/math.log(2)==pytest.approx(got['surprisal'],abs=2e-4)
    assert sum(p)==pytest.approx(1)

@pytest.mark.parametrize('typename,body',[
 ('TokenAnswer',{'kind':'opened'}),
 ('TokenAnswer',{'kind':'token','body':{'token':12,'piece':'β'}}),
 ('TokenAnswer',{'kind':'flushed','body':{'resident_before':20,'resident_after':10}}),
 ('TokenAnswer',{'kind':'field','body':{'position':5,'ranked':[{'token':3,'probability':.5}],'realized':0}}),
 ('TokenRefusal',{'kind':'unremovable_span','from':0,'to':1,'prefix':2,'resident':5}),
 ('LabelDirective',{'kind':'classify','content':'hello','turn':None}),
 ('LabelAnswer',{'kind':'scored','body':{'turn':None,'labels':[{'label':'x','score':.5}]}}),
 ('LabelRefusal',{'kind':'oversized','requested':20,'bound':10}),
 ('SegmentPreamble',{'segments':2,'bytes':65537})])
def test_other_wire_payloads(oracle,typename,body):
    actual=dump(ADAPTERS[typename].validate_python(body))
    assert oracle(op='round',type=typename,body=actual)=={'ok':actual}
