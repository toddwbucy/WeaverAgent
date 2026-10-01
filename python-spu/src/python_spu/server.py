"""SPU process entry. No bind, no daemon installation, no remote model fetch."""
import json
import math
import re
import sys
from .transport import adopt,ChannelFault,Closed
from .wire import Envelope,SpuInstruction,TOKEN_DIRECTIVE,dump
from .session import Session,Refusal
from .engine import HFEngine,AdmissionError,HEADROOM_BYTES,U64_MAX

# u64's own parse in Rust: ASCII digits and an optional leading plus sign, nothing else.
BYTE_COUNT=re.compile(r'\+?[0-9]+')

def byte_count(value):
    """Rust's `u64::from_str`, ported: an optional plus sign, then ASCII digits, leading
    zeros allowed however many, the value at most 2^64 - 1. Answers the count, or None.
    The zeros are stripped and the length judged before `int` sees the digits, since
    CPython's `int` refuses a string past 4300 digits that Rust reads as a small number."""
    if not BYTE_COUNT.fullmatch(value): return None
    digits=value.lstrip('+').lstrip('0') or '0'
    if len(digits)>len(str(U64_MAX)): return None
    count=int(digits)
    return count if count<=U64_MAX else None

class BadParameter(ValueError):
    pass

def parameters(arguments):
    """The worker's argument vector, read as the Rust SPU reads it: weaver-spu main.rs
    `headroom_from`, ported, beside python-spu's own two flags, which the worker never
    sends. **The whole vector is read before anything is answered.** A parameter stated
    twice, a missing or malformed value, and an unknown parameter each refuse by name,
    and an absent `--headroom-bytes` leaves the compiled default. Answers (headroom,
    cpu, declare)."""
    headroom=declare=None; cpu=False
    arguments=iter(arguments)
    for argument in arguments:
        if argument=='--headroom-bytes':
            value=next(arguments,None)
            if value is None: raise BadParameter('--headroom-bytes takes a value')
            count=byte_count(value)
            if count is None:
                raise BadParameter(f'--headroom-bytes wants a byte count, got {value}')
            if headroom is not None: raise BadParameter('--headroom-bytes is stated twice')
            headroom=count
        elif argument=='--cpu-experiment':
            if cpu: raise BadParameter('--cpu-experiment is stated twice')
            cpu=True
        elif argument=='--declare-imports':
            value=next(arguments,None)
            if value is None: raise BadParameter('--declare-imports takes a value')
            if declare is not None: raise BadParameter('--declare-imports is stated twice')
            declare=value
        else:
            raise BadParameter(f'unknown parameter {argument}')
    return (HEADROOM_BYTES if headroom is None else headroom),cpu,declare

class Service:
    def __init__(self,cpu=False,engine_factory=HFEngine,enforce_imports=False,declare_imports=None,
                 headroom=HEADROOM_BYTES):
        self.cpu=cpu; self.headroom=headroom; self.factory=engine_factory; self.position='before_admit'
        self.engine=None; self.session=None
        # The import set is judged where main() serves, never in a process that
        # holds a test runner's modules too, per python-spu-Spec section 8.
        self.enforce_imports=enforce_imports; self.declare_imports=declare_imports; self.generated=False
        # Exchanges that close after the reply in hand, in order: the cancel's
        # at rest after the generation it stopped.
        self.closes_after=[]
    def close(self):
        self.session=None
        if self.engine is not None: self.engine.close(); self.engine=None
    def lifecycle(self,value):
        try: envelope=Envelope.model_validate(value)
        except ValueError as e: raise ChannelFault('malformed lifecycle envelope') from e
        payload=envelope.payload
        if 'kind' not in payload: raise ChannelFault('malformed lifecycle payload')
        body=payload.get('body',{})
        if not isinstance(body,dict): raise ChannelFault('malformed lifecycle body')
        kind=body.get('kind')
        answer={'kind':'refusal','body':{'kind':'out_of_order'}}
        admitted=False
        if envelope.position=='open' and envelope.exchange.opener=='harness' and payload['kind']=='directive':
            if kind=='admit':
                # Invalid serialization is a channel fault, not an admission attempt.
                try: instruction=SpuInstruction.model_validate(body['instruction'])
                except (KeyError,ValueError) as e: raise ChannelFault('malformed instruction') from e
                if self.position=='before_admit':
                    self.position='admit_refused'
                    try:
                        self.admit(instruction)
                        self.position='admitted'; admitted=True
                        answer={'kind':'answer','body':{'kind':'admitted'}}
                    except AdmissionError as e:
                        self.close()
                        print(json.dumps({'admission_refused':e.kind,'detail':str(e)}),file=sys.stderr)
                        answer={'kind':'refusal','body':{'kind':e.kind,**e.fields}}
            elif kind=='release' and self.position=='admitted':
                self.close(); self.position='released'
                answer={'kind':'answer','body':{'kind':'released'}}
        return {'exchange':envelope.exchange.model_dump(),'position':'close','payload':answer},admitted
    def admit(self,instruction):
        d=instruction.decoder
        values=d.tunable_values
        resolved=[]
        # The exclusive ceilings are the ones weaver-spu sampling.rs passes `resolve_count`:
        # context-capacity is a u32 there, refused at or past 2**32 as NotACount, which
        # main.rs answers as config_invalid naming the field.
        for name,minimum,maximum in [('seed',0,2**64),('context-capacity',0,2**32),('max-tokens-per-turn',0,2**64)]:
            value=values.get(name)
            if value is None or not math.isfinite(value) or value!=int(value) or not minimum<=value<maximum:
                raise AdmissionError('config_invalid',f'invalid or missing {name}',field=f'tunable-values.{name}')
            resolved.append(int(value))
        if d.field_election is not None and d.field_election.depth<40:
            raise AdmissionError('config_invalid','field depth below sampling cutoff 40',field='spu-instruction.decoder.field-election.depth')
        # The classify member is the classify process's, per python-spu-Spec section 1,
        # and is left unread here as weaver-spu main.rs's admission leaves it.
        self.engine=self.factory(d.model_binding.artifact,d.model_binding.devices,cpu=self.cpu,
                                 readout=d.residual_readout_election,headroom=self.headroom)
        seed,capacity,limit=resolved
        if capacity>self.engine.max_context:
            raise AdmissionError('device_cannot_admit','context exceeds artifact position bound')
        self.session=Session(self.engine,d,capacity,limit,seed)
        if self.enforce_imports:
            from .import_set import enforce
            enforce('admission',self.declare_imports)
    def decode(self,value,channel):
        try: directive=dump(TOKEN_DIRECTIVE.validate_python(value))
        except ValueError as e: raise ChannelFault('malformed decode directive') from e
        kind=directive['kind']
        if self.session is None: raise Refusal('not_open')
        s=self.session
        if kind=='open': return s.open(directive['messages'],directive['column_ask'])
        s.require_open()
        if kind=='cancel': return {'kind':'at_rest'}
        if kind=='flush': return s.flush(directive['keep'])
        if kind=='elide': return s.elide(directive['from'],directive['to'])
        cancelled=[]
        def cancel():
            # weaver-spu main.rs `SeamCancel`: a cancel stops the generation at the
            # boundary, and any other directive polled up mid-flight is refused
            # here, typed `TokenRefusal::OutOfOrder`, and not queued, per
            # weaver-harness-spu-decode-contract section 3's one generation in flight.
            incoming=channel.receive(nonblocking=True)
            if incoming is None: return False
            try: ask=dump(TOKEN_DIRECTIVE.validate_python(incoming))
            except ValueError as e: raise ChannelFault('malformed cancel poll') from e
            if ask['kind']=='cancel': cancelled.append(True); return True
            channel.send({'kind':'out_of_order'})
            return False
        if kind=='append_and_generate':
            answer=s.generate(directive['turn'],directive['delta'],channel.send,cancel)
            # A cancel that stopped this generation is its own exchange and closes
            # second: the generation answers stopped, then the cancel answers at
            # rest, per the decode contract's section 2 and weaver-spu main.rs.
            if cancelled: self.closes_after.append({'kind':'at_rest'})
        else:
            answer=s.generate(directive['turn'],[],channel.send,cancel,
                              refeed=(directive['rendered'],directive['path']))
        if self.enforce_imports and not self.generated:
            # torch loads some of its modules only in the first forward.
            from .import_set import enforce
            enforce('first generation',self.declare_imports)
        self.generated=True
        return answer
    def serve_decode(self,channel):
        while True:
            try: request=channel.receive()
            except Closed: return
            try: reply=self.decode(request,channel)
            except Refusal as e: reply=e.payload
            channel.send(reply)
            while self.closes_after: channel.send(self.closes_after.pop(0))
    def serve(self,lifecycle,decode):
        try:
            while True:
                try: request=lifecycle.receive()
                except Closed: return
                answer,admitted=self.lifecycle(request)
                lifecycle.send(answer)
                if admitted: self.serve_decode(decode)
        finally: self.close()

def main(argv=None):
    """A refusal before serving is one JSON line on stderr and exit 1, before the
    channels are adopted, as the Rust SPU's main refuses a bad parameter."""
    from . import differing_environment
    try:
        headroom,cpu,declare=parameters(sys.argv[1:] if argv is None else argv)
    except BadParameter as e:
        print(json.dumps({'refusal':'bad_parameter','detail':str(e)}),file=sys.stderr)
        return 1
    differing=differing_environment()
    if differing:
        print(json.dumps({'refusal':'bad_environment','detail':'. '.join(
            f'{name} is {carried!r}, and python-spu sets {value} for {purpose}'
            for name,carried,value,purpose in differing)}),file=sys.stderr)
        return 1
    try:
        lifecycle,decode=adopt()
        Service(cpu=cpu,headroom=headroom,enforce_imports=True,
                declare_imports=declare).serve(lifecycle,decode)
    except Exception as e:
        print(json.dumps({'python_spu_fault':type(e).__name__,'detail':str(e)}),file=sys.stderr)
        return 1
    return 0
if __name__=='__main__': sys.exit(main())
