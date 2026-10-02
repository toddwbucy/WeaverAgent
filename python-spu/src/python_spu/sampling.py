"""Rust-compatible seed derivation, and the sampler, which is the native engine's
ported exactly in candle_chain, per python-spu-Spec section 5."""
import math

from .candle_chain import Sampler  # noqa: F401
MASK=2**64-1

def derived_seed(seed,turn,generation):
    if type(seed) is not int or type(generation) is not int or not 0<=seed<=MASK or not 0<=generation<=MASK:
        raise ValueError('seed and generation must be u64')
    def mix(x):
        z=(x+0x9e3779b97f4a7c15)&MASK
        z=((z^(z>>30))*0xbf58476d1ce4e5b9)&MASK
        z=((z^(z>>27))*0x94d049bb133111eb)&MASK
        return z^(z>>31)
    h=0xcbf29ce484222325
    for b in turn.encode(): h=((h^b)*0x100000001b3)&MASK
    return mix(mix(seed^h)^generation)

def distribution(logits):
    if not logits or any(not math.isfinite(x) for x in logits): raise ValueError('invalid logits')
    m=max(logits)
    z=sum(math.exp(x-m) for x in logits)
    logp=[x-m-math.log(z) for x in logits]
    p=[math.exp(x) for x in logp]
    entropy=-sum(a*b for a,b in zip(p,logp))/math.log(2)
    return p,logp,entropy

# The dispositions, weaver-spu sampling.rs, and the binary's elections of them,
# weaver-spu main.rs `KNOBS` and `SESSION_PARAMETERS`, mirrored: a binary's items are
# not the library's, so the oracle resolves these dispositions through the library and
# tests/test_walk.py holds this table to the binary's literal. A knob is frozen with its
# value, or tunable and supplied by the declaration's `tunable-values`.
TUNABLE=('tunable',)
def FROZEN(value): return ('frozen',value)
KNOBS={'temperature':FROZEN(0.7),'top-k':FROZEN(40),'top-p':FROZEN(0.95),
       'repetition-penalty':FROZEN(1.1),'repetition-window':FROZEN(64),'seed':TUNABLE}
SESSION_PARAMETERS={'context-capacity':TUNABLE,'max-tokens-per-turn':TUNABLE}
# The counts and their exclusive ceilings, the ones sampling.rs hands `resolve_count`;
# every other knob is an f32 taken as supplied.
COUNTS={'top-k':2**32,'repetition-window':2**32,'seed':2**64,
        'context-capacity':2**32,'max-tokens-per-turn':2**64}

class KnobRefusal(Exception):
    """sampling.rs `KnobRefusal`: `unsupplied` names the knob, `not_a_count` names it
    and the value supplied."""
    def __init__(self,kind,knob,supplied=None):
        self.kind=kind; self.knob=knob; self.supplied=supplied
        super().__init__(f'{kind}: {knob}')

def f32(value):
    """Rust's `v as f32`: the nearest single."""
    import struct
    return struct.unpack('f',struct.pack('f',value))[0]

def resolve(dispositions,supplied):
    """sampling.rs `Knobs::resolve` and `SessionParameters::resolve`, ported, in their
    field order: a frozen value stands, a tunable one is taken from `supplied` or
    refused unsupplied, and a count is judged before its cast, refused not a count where
    it is fractional, negative, at or past its ceiling, or not finite. Answers the
    effective values by name, or raises the first refusal."""
    effective={}
    for name,disposition in dispositions.items():
        if disposition[0]=='frozen':
            effective[name]=disposition[1] if name in COUNTS else f32(disposition[1]); continue
        if name not in supplied: raise KnobRefusal('unsupplied',name)
        value=supplied[name]
        if name in COUNTS:
            if not math.isfinite(value) or value!=math.floor(value) or value<0 or value>=COUNTS[name]:
                raise KnobRefusal('not_a_count',name,value)
            effective[name]=int(value)
        else:
            effective[name]=f32(value)
    return effective
