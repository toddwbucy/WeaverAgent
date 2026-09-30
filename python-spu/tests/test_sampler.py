"""The sampler port against the Rust oracle, per python-spu-Spec section 5.

Every comparison is exact: the generator's words, the partition's whole index order,
the probability vector's bits, and each generation's draws. The oracle runs candle,
rand and core at the revisions the lock pins, on this host, so the AVX2 branch it
takes is the one this host's candle takes.
"""
import random

import pytest

from python_spu import candle_chain as cc

G = random.Random(726)


def ask(oracle, **request):
    """The oracle's answer, or the test fails naming the oracle's error."""
    out = oracle(**request)
    assert 'ok' in out, out
    return out['ok']


def f32_vector(n, spread=3.0):
    return [cc.f32(G.gauss(0.0, spread)) for _ in range(n)]


@pytest.mark.parametrize('seed', [0, 1, 11, 2**32, 2**64 - 1, 0x0123_4567_89AB_CDEF])
def test_generator_words(oracle, seed):
    """Past four refills, so the block counter's carry and the buffer order are read."""
    rng = cc.StdRng(seed)
    assert [rng.next_u32() for _ in range(300)] == ask(oracle, op='rng', seed=seed, count=300)


def select_cases():
    cases = []
    for n in (2, 3, 15, 16, 17, 33, 64, 65, 500, 5000):
        for k in sorted({0, 1, n // 2, n - 1}):
            cases.append(('random', n, k))
    for n in (16, 17, 50, 200):
        for k in sorted({0, 1, n // 2, n - 1}):
            cases.append(('duplicates', n, k))
            cases.append(('few values', n, k))
    return cases


@pytest.mark.parametrize('kind,n,k', select_cases())
def test_partition_order(oracle, kind, n, k):
    """The whole index vector select_nth_unstable_by leaves, under candle's descending
    total_cmp comparator: ties, lengths either side of the small-sort cutoff of 16, k
    at 0, 1 and n - 1, and slices long enough for the recursive pseudo-median."""
    if kind == 'random':
        prs = f32_vector(n)
    elif kind == 'duplicates':
        prs = [cc.f32(0.25)] * n
    else:
        values = [cc.f32(0.1), cc.f32(0.2), -0.0, 0.0]
        prs = [G.choice(values) for _ in range(n)]
    assert cc.select_descending(prs, k) == ask(oracle, op='select', prs=prs, k=k)


def adversarial(n, index):
    """McIlroy's adversary against the port's own comparisons: every value starts
    undecided, and a comparison between two undecided values fixes the pivot
    candidate low, which defeats the pivot choice until the loop's limit falls through
    to median_of_medians. The fixed order becomes a concrete input."""
    gas = n
    val = [gas] * n
    state = {'solid': 0, 'candidate': 0}

    def freeze(z):
        val[z] = state['solid']
        state['solid'] += 1

    def is_less(x, y):
        if val[x] == gas and val[y] == gas:
            freeze(x if x == state['candidate'] else y)
        if val[x] == gas:
            state['candidate'] = x
        elif val[y] == gas:
            state['candidate'] = y
        return val[x] < val[y]

    cc.select_nth_unstable_by(list(range(n)), index, is_less)
    for i in range(n):
        if val[i] == gas:
            freeze(i)
    # is_less(a, b) is prs[a] > prs[b] under candle's comparator, so rank 0 is largest.
    return [cc.f32(1.0 - rank / (n + 1)) for rank in val]


@pytest.mark.parametrize('n,index', [(500, 250), (1000, 333), (1000, 995), (3000, 1500)])
def test_partition_order_through_median_of_medians(oracle, monkeypatch, n, index):
    prs = adversarial(n, index)
    calls = []
    real = cc._median_of_medians

    def counted(*args):
        calls.append(args)
        return real(*args)

    monkeypatch.setattr(cc, '_median_of_medians', counted)
    mine = cc.select_descending(prs, index)
    assert calls, 'the input did not reach median_of_medians'
    assert mine == ask(oracle, op='select', prs=prs, k=index)


def probability_cases():
    cases = []
    for n in (1, 7, 31, 32, 33, 63, 64, 65, 1000):
        for temperature in (0.7, 1.0, 1e-3, 2.5):
            cases.append((n, temperature, 1.1, 64))
    cases.append((151936, 0.7, 1.1, 64))
    cases.append((300, 0.7, 1.0, 64))
    cases.append((300, 0.7, 1.5, 0))
    return cases


@pytest.mark.parametrize('n,temperature,penalty,window', probability_cases())
def test_probability_bits(oracle, n, temperature, penalty, window):
    """The penalty, the temperature's multiply and the softmax, bit for bit. Lengths
    either side of a multiple of 32 read both the AVX2 lanes and the scalar tail, and
    one vector is a full Qwen2.5 vocabulary."""
    logits = f32_vector(n)
    tail = [G.randrange(n) for _ in range(G.randrange(0, 80))]
    values = cc.penalised(logits, cc.f32(penalty), window, tail)
    mine = [cc.f32_bits(p) for p in cc.probabilities(values, cc.f32(temperature))]
    assert mine == ask(oracle, op='probs', temperature=temperature, repetition_penalty=penalty,
                          repetition_window=window, tail=tail, logits=logits)


def run(oracle, seed, temperature, top_k, top_p, penalty, window, tail, logits):
    """Both sides' draws, or 'refused' where the draw has no nonzero weight to choose
    from, which rand's WeightedIndex refuses in Rust as the port refuses it."""
    sampler = cc.Sampler(seed, temperature, top_k, top_p, penalty, window)
    resident = list(tail)
    mine = []
    try:
        for row in logits:
            token = sampler.sample(row, resident)
            mine.append(token)
            resident.append(token)
    except ValueError:
        mine = 'refused'
    out = oracle(op='generation', seed=seed, temperature=temperature, top_k=top_k,
                 top_p=top_p, repetition_penalty=penalty, repetition_window=window,
                 tail=tail, logits=logits)
    theirs = out['ok'] if 'ok' in out else 'refused'
    return mine, theirs


def test_generations_randomised(oracle):
    for trial in range(120):
        n = G.choice([8, 64, 300])
        mine, theirs = run(
            oracle, G.getrandbits(64), G.choice([0.0, 0.7, 1.0, 1e-8, 2.5]),
            G.choice([0, 1, 3, 40, n, n + 5]), G.choice([0.0, 0.5, 0.95, 1.0]),
            G.choice([1.0, 1.1, 1.5]), G.choice([0, 3, 64]),
            [G.randrange(n) for _ in range(G.randrange(0, 20))],
            [f32_vector(n) for _ in range(G.randrange(1, 8))])
        assert mine == theirs, trial


EDGES = {
    'near-ties': dict(temperature=0.7, top_k=40, top_p=0.95, logits=[[1.0] * 64] * 6),
    'temperature zero': dict(temperature=0.0, top_k=40, top_p=0.95),
    'temperature just above zero': dict(temperature=1e-8, top_k=40, top_p=0.95),
    'top-k zero': dict(temperature=0.7, top_k=0, top_p=0.95),
    'top-k at the vocabulary': dict(temperature=0.7, top_k=64, top_p=0.95),
    'top-k past the vocabulary': dict(temperature=0.7, top_k=100, top_p=0.95),
    'top-p one': dict(temperature=0.7, top_k=40, top_p=1.0),
    'top-p zero': dict(temperature=0.7, top_k=40, top_p=0.0),
    'top-p one, top-k zero': dict(temperature=0.7, top_k=0, top_p=1.0),
    'penalty one skips': dict(temperature=0.7, top_k=40, top_p=0.95, penalty=1.0),
    'window zero skips': dict(temperature=0.7, top_k=40, top_p=0.95, window=0),
    'repeated tail': dict(temperature=0.7, top_k=40, top_p=0.95, tail=[5, 5, 9, 5, 9, 2]),
    'window edge': dict(temperature=0.7, top_k=5, top_p=0.95, window=4,
                        tail=[1, 2, 3, 4, 5, 6]),
    'a long generation': dict(temperature=1.0, top_k=40, top_p=0.95, rows=150),
    'top-p splits a tie, top-k zero': dict(temperature=0.7, top_k=0, top_p=0.35,
                                            logits=[[1.0] * 10] * 6),
    'top-p splits a tie after top-k': dict(temperature=0.7, top_k=40, top_p=0.3,
                                            logits=[[1.0] * 64] * 6),
    'argmax over a tie': dict(temperature=0.0, top_k=40, top_p=0.95,
                              logits=[[0.0, 3.0, 1.0, 3.0, 3.0, 2.0] * 4] * 6),
}


@pytest.mark.parametrize('name', list(EDGES))
def test_generation_edges(oracle, name):
    """Section 5's named edges, each over several seeds. A long generation carries the
    generator's state across refills of its 64-word buffer."""
    case = dict(EDGES[name])
    for _ in range(8):
        logits = case.get('logits') or [f32_vector(64, 1.0) for _ in range(case.get('rows', 12))]
        mine, theirs = run(oracle, G.getrandbits(64), case['temperature'], case['top_k'],
                           case['top_p'], case.get('penalty', 1.1), case.get('window', 64),
                           case.get('tail', [G.randrange(64) for _ in range(10)]), logits)
        assert mine == theirs, name


class Scripted:
    def __init__(self, words):
        self.words = list(words)

    def next_u32(self):
        return self.words.pop(0)


def boundary_word(cumulative, low_bits):
    """The word whose uniform lands exactly on a cumulative weight, where the total
    is 1 and the scale needs no adjustment: 23 mantissa bits, and nine low bits the
    shift discards."""
    return (int(cumulative * 2**23) << 9) | low_bits


WEIGHTS = [[0.5, 0.5], [0.25, 0.0, 0.25, 0.5], [0.125] * 8, [0.0, 0.0, 1.0],
           [0.75, 0.0, 0.0, 0.25]]


@pytest.mark.parametrize('weights', WEIGHTS)
def test_weighted_choice_boundaries(oracle, weights):
    """rand's partition point takes the first cumulative weight above the choice, so a
    choice landing exactly on a boundary goes to the later index. Each boundary, a
    zero weight's repeated boundary, and random words besides."""
    words = []
    total = 0.0
    for w in weights[:-1]:
        total += w
        words.append(boundary_word(total, G.randrange(512)))
    words += [G.getrandbits(32) for _ in range(20)] + [0, 0xFFFF_FFFF]
    for word in words:
        mine = cc.weighted_index([cc.f32(w) for w in weights], Scripted([word]))
        assert mine == ask(oracle, op='weighted', weights=weights, words=[word]), (weights, word)
