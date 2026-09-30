from python_spu.session import Session,Refusal
class Engine:
    def __init__(self):
        self.logits=[-1000.,0.,-1000.]; self.norms=[]
        self.terminator=2; self.artifact='fake'; self.weights_hash='fake'
        self.next_tokens=[]
    def tokenize(self,_): return self.next_tokens.copy()
    def append(self,_): pass
    def rebuild(self,_): pass
    def detokenize(self,tokens): return ''.join(map(str,tokens))

def test_state_transitions_against_executing_rust_session(oracle,instruction):
    steps=[{'kind':'flush','keep':0}, {'kind':'open','tokens':[0,0]},
           {'kind':'open','tokens':[0]}, {'kind':'generate','tokens':[0,0],'limit':3},
           {'kind':'elide','from':0,'to':1}, {'kind':'elide','from':3,'to':5},
           {'kind':'flush','keep':999}, {'kind':'flush','keep':0},
           {'kind':'generate','tokens':[0]*14,'limit':1},
           {'kind':'generate','tokens':[0],'limit':0},
           {'kind':'generate','tokens':[0]*5,'limit':20}]
    expected=oracle(op='session',body={'capacity':16,'steps':steps})
    assert 'ok' in expected,expected
    engine=Engine(); session=Session(engine,instruction.decoder,16,3,11)
    observed=[]
    for step in steps:
        engine.next_tokens=step.get('tokens',[])
        try:
            kind=step['kind']
            if kind=='open': answer=session.open([])
            elif kind=='flush': answer=session.flush(step['keep'])
            elif kind=='elide': answer=session.elide(step['from'],step['to'])
            else:
                session.max_tokens=step['limit']; answer=session.generate('t',[],lambda _:None)
            outcome={'kind':answer['kind']}
            if kind=='generate': outcome['tokens']=answer['body']['measurement']['output_tokens']
        except Refusal as e: outcome=e.payload
        observed.append({'outcome':outcome,'resident':len(session.resident)})
    assert observed==expected['ok']
