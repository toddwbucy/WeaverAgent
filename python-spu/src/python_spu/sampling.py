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
