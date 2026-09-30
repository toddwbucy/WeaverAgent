import pytest
from python_spu.session import Session,Refusal
from python_spu.engine import HFEngine
from python_spu.wire import TOKEN_ANSWER,dump
from python_spu.family import render

MESSAGE=[{'role':'user','content':[{'type':'text','text':'hello world'}]}]
IDENTITY=[{'role':'system','content':[{'type':'text','text':'be precise'}]}]

@pytest.fixture
def session(tiny_model,instruction):
    engine=HFEngine(tiny_model,[0],cpu=True,readout=True)
    s=Session(engine,instruction.decoder,256,4,11)
    yield s
    engine.close()

def test_actual_forward_stream_measurement_and_rust_answer(session,oracle):
    assert session.engine.tokenize('hello world')==[6,7]
    assert session.open(IDENTITY)=={'kind':'opened'}
    frames=[]
    result=session.generate('t',MESSAGE,frames.append)
    body=result['body']; m=body['measurement']
    assert body['emission']==''.join(x['body']['piece'] for x in frames if x['kind']=='token')
    assert len(m['output_tokens'])==len(m['entropies'])==len(m['surprisals'])
    assert session.resident[-1]==session.engine.terminator
    assert m['residual_forwards']==len(m['output_tokens'])+2
    assert len(m['residual_norms'])==m['residual_forwards']*2
    assert dump(TOKEN_ANSWER.validate_python(result))==result
    rust=oracle(op='round',type='TokenAnswer',body=result)
    assert 'ok' in rust,rust
    # Raw JSON float parsing can round its last decimal digit in serde_json.
    def equal(a,b):
        if isinstance(a,float): assert a==pytest.approx(b,rel=1e-14,abs=1e-14)
        elif isinstance(a,dict):
            assert a.keys()==b.keys()
            for key in a: equal(a[key],b[key])
        elif isinstance(a,list):
            assert len(a)==len(b)
            for x,y in zip(a,b): equal(x,y)
        else: assert a==b
    equal(rust['ok'],result)
    for frame in frames: assert oracle(op='round',type='TokenAnswer',body=frame)=={'ok':frame}

def test_flush_and_elide_restore_actual_model_distribution(session):
    session.open(IDENTITY)
    initial=list(session.engine.logits)
    session.generate('t',MESSAGE,lambda _:None)
    result=session.flush(0)
    assert result['body']['resident_after']==session.prefix
    assert session.engine.logits==initial
    session.generate('t2',MESSAGE,lambda _:None)
    keep=session.resident[:session.prefix]+session.resident[session.prefix+2:]
    session.elide(session.prefix,session.prefix+2)
    after=list(session.engine.logits)
    session.engine.rebuild(keep)
    assert session.engine.logits==after
    assert session.resident==keep

def test_cancel_before_draw_still_commits_terminator(session):
    session.open(IDENTITY)
    result=session.generate('t',MESSAGE,lambda _:None,lambda:True)
    assert result['body']['finish']=='stopped'
    assert result['body']['measurement']['output_tokens']==[]
    assert 'entropies' not in result['body']['measurement']
    assert session.resident[-1]==session.engine.terminator

def test_refusals_preserve_state(session):
    with pytest.raises(Refusal,match='not_open'): session.flush(0)
    session.open(IDENTITY)
    before=session.resident.copy(); logits=session.engine.logits.copy()
    with pytest.raises(Refusal,match='out_of_order'): session.open([])
    with pytest.raises(Refusal,match='unremovable_span'): session.elide(0,1)
    session.capacity=len(before)+1
    with pytest.raises(Refusal,match='overflow'): session.generate('t',MESSAGE,lambda _:None)
    assert session.resident==before and session.engine.logits==logits

def test_failed_rollback_poisons_session(session,monkeypatch):
    session.open(IDENTITY)
    def fail(_): raise RuntimeError('device failure')
    monkeypatch.setattr(session.engine,'rebuild',fail)
    with pytest.raises(RuntimeError): session.flush(0)
    with pytest.raises(Refusal,match='not_open'): session.generate('t',MESSAGE,lambda _:None)

def test_refeed_advances_recorded_path_not_draws(session):
    session.open(IDENTITY)
    path=[10,11,12]; rendered=render(MESSAGE)
    before=len(session.resident)
    result=session.generate('t',[],lambda _:None,refeed=(rendered,path))
    assert result['kind']=='re_fed'
    assert session.resident[before+len(session.engine.tokenize(rendered)):-1]==path
    assert len(result['body']['measurement']['output_tokens'])==len(path)
    assert result['body']['emission']==session.engine.detokenize(path)

def test_readout_does_not_change_logits(tiny_model):
    off=HFEngine(tiny_model,[0],cpu=True,readout=False)
    on=HFEngine(tiny_model,[0],cpu=True,readout=True)
    try:
        for tokens in ([1,4,6],[7],[2]):
            off.append(tokens); on.append(tokens)
            assert off.logits==on.logits
        assert not off.norms and len(on.norms)==6
    finally: off.close(); on.close()

def test_the_engine_loads_every_parameter_at_the_version_dtype(tiny_model):
    """python-spu-Spec sections 4 and 9: the first version loads at BF16. The tiny
    model is saved at float32, so the load itself must cast. Perturbations: load at
    torch.float32 and the parameters read float32, or name float32 as the version's
    dtype and the constant disagrees with the Spec."""
    import torch
    from python_spu import engine as engine_module
    assert engine_module.DTYPE=='bfloat16'
    engine=HFEngine(tiny_model,[0],cpu=True)
    try:
        dtypes={p.dtype for p in engine.model.parameters()}
        assert dtypes=={torch.bfloat16},dtypes
    finally:
        engine.close()
