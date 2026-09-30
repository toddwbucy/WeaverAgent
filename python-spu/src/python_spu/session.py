"""Decode state and measurements. Refusals are side-effect free; faults poison."""
import math
import time
from . import family
from .sampling import Sampler,derived_seed,distribution

class Refusal(Exception):
    def __init__(self,kind,**fields): self.payload={'kind':kind,**fields}; super().__init__(kind)

class Session:
    def __init__(self,engine,instruction,capacity,max_tokens,seed):
        self.engine=engine; self.instruction=instruction
        self.capacity=capacity; self.max_tokens=max_tokens; self.seed=seed
        self.resident=[]; self.prefix=0; self.opened=False; self.poisoned=False
        self.turn=None; self.generation=0
    def change(self,operation):
        try: return operation()
        except Exception:
            self.poisoned=True; self.opened=False
            raise
    def open(self,messages,column_ask=False):
        if self.poisoned: raise Refusal('not_open')
        if self.opened: raise Refusal('out_of_order')
        if column_ask:
            if not self.instruction.column_permission: raise Refusal('column_permission_absent')
            if not self.instruction.residual_readout_election: raise Refusal('column_readout_unelected')
            raise Refusal('column_undeclared') # mirrors native Qwen2: norms, no column declaration
        try: text=family.render(messages)
        except family.MalformedDelta: raise Refusal('malformed_delta')
        tokens=self.engine.tokenize(text)
        if len(tokens)>self.capacity: raise Refusal('overflow',resident=0,requested=len(tokens),capacity=self.capacity)
        self.change(lambda:self.engine.rebuild(tokens))
        self.resident=tokens; self.prefix=len(tokens); self.opened=True
        return {'kind':'opened'}
    def require_open(self):
        if not self.opened or self.poisoned: raise Refusal('not_open')
    def flush(self,keep):
        self.require_open()
        before=len(self.resident); keep=min(before,max(self.prefix,keep))
        held=self.resident[:keep]
        self.change(lambda:self.engine.rebuild(held)); self.resident=held
        return {'kind':'flushed','body':{'resident_before':before,'resident_after':keep}}
    def elide(self,start,end):
        self.require_open(); before=len(self.resident)
        if start<self.prefix or end>before or start>=end:
            raise Refusal('unremovable_span',**{'from':start,'to':end,'prefix':self.prefix,'resident':before})
        held=self.resident[:start]+self.resident[end:]
        self.change(lambda:self.engine.rebuild(held)); self.resident=held
        return {'kind':'elided','body':{'resident_before':before,'resident_after':len(held)}}
    def generate(self,turn,delta,emit,cancel=lambda:False,refeed=None):
        self.require_open()
        if refeed is not None:
            if not self.instruction.refeed_permission: raise Refusal('refeed_permission_absent')
            rendered,path=refeed
            if not path: raise Refusal('refeed_path_empty')
        else:
            try: rendered=family.render(delta)+family.OPENER
            except family.MalformedDelta: raise Refusal('malformed_delta')
            path=None
        tokens=self.engine.tokenize(rendered)
        if turn==self.turn: self.generation+=1
        else: self.turn=turn; self.generation=0
        seed=derived_seed(self.seed,turn,self.generation)
        needed=len(tokens)+1+(len(path) if path is not None else 0)
        if len(self.resident)+needed>self.capacity:
            raise Refusal('overflow',resident=len(self.resident),requested=needed-1,capacity=self.capacity)
        def work():
            self.engine.norms=[]
            start=time.perf_counter_ns()
            self.engine.append(tokens); self.resident.extend(tokens)
            prefill=time.perf_counter_ns()-start; start=time.perf_counter_ns()
            sampler=Sampler(seed); output=[]; fed=[]; entropies=[]; surprisals=[]
            pending=[]; emission=''; finish='completed'; index=0
            while True:
                if path is None:
                    if cancel(): finish='stopped'; break
                    if len(output)>=self.max_tokens: finish='length'; break
                    if len(self.resident)+1>=self.capacity: finish='stopped'; break
                elif index>=len(path): break
                elif cancel(): raise RuntimeError('cancelled re-feed cannot certify a partial path')
                p,logp,entropy=distribution(self.engine.logits)
                drawn=sampler.sample(self.engine.logits,self.resident)
                token=drawn if path is None else path[index]
                if path is None and token==self.engine.terminator: break
                if not 0<=token<len(p): raise ValueError('re-feed token outside vocabulary')
                pos=len(self.resident)
                entropies.append(entropy); surprisals.append(-logp[token]/math.log(2))
                output.append(drawn if path is not None else token); fed.append(token)
                if self.instruction.field_election is not None:
                    depth=self.instruction.field_election.depth
                    leaders=sorted(range(len(p)),key=lambda i:(-p[i],i))[:depth]
                    emit({'kind':'field','body':{'position':pos,
                         'ranked':[{'token':i,'probability':p[i]} for i in leaders],
                         'realized':leaders.index(token) if token in leaders else depth}})
                self.engine.append([token]); self.resident.append(token)
                if path is None:
                    pending.append(token)
                    piece=self.engine.detokenize(pending)
                    if not piece.endswith('\ufffd'):
                        emit({'kind':'token','body':{'token':token,'piece':piece}})
                        emission+=piece; pending=[]
                index+=1
            self.engine.append([self.engine.terminator]); self.resident.append(self.engine.terminator)
            if path is not None: emission=self.engine.detokenize(fed)
            elif pending:
                piece=self.engine.detokenize(pending)
                emit({'kind':'token','body':{'token':pending[-1],'piece':piece}}); emission+=piece
            measurement={'model':self.engine.artifact,'weights_hash':self.engine.weights_hash,
                'input_tokens':tokens,'output_tokens':output,
                'blocks':[{'label':'turn-delta','start':0,'end':len(rendered.encode())}],
                'timings':{'prefill_ns':str(prefill),'decode_ns':str(time.perf_counter_ns()-start)}}
            if entropies:
                measurement['entropies']=entropies
                power=sum(surprisals)/len(surprisals)
                if power<1024: measurement['perplexity']=2**power
                if self.instruction.surprisal_election: measurement['surprisals']=surprisals
            if self.engine.norms:
                measurement.update(residual_norms=self.engine.norms.copy(),residual_layers=self.engine.layers,
                                   residual_forwards=len(self.engine.norms)//self.engine.layers)
            content,failed=family.parse(emission)
            if failed:
                import sys,json
                print(json.dumps({'unrecovered_calls':failed}),file=sys.stderr)
            request={'rendered':rendered,'template':family.TEMPLATE,'sampling':{
                'temperature':.7,'top_k':40,'top_p':.95,'repetition_penalty':1.1,
                'repetition_window':64,'seed':self.seed,'generation_seed':seed},
                'stop':{'max_tokens':self.max_tokens,'stop_tokens':[self.engine.terminator],
                        'terminator':self.engine.terminator}}
            return {'kind':'re_fed' if path is not None else 'generated','body':{
                'emission':emission,'finish':finish,'content':content,'request':request,
                'measurement':measurement,'resident':len(self.resident),'capacity':self.capacity}}
        return self.change(work)
