"""The native engine's sampler, ported exactly, per python-spu-Spec section 5.

The Rust native engine (crates/weaver-spu/src/decoder/native.rs) samples on the host:
it builds its input as a CPU tensor, applies candle's repeat penalty over the resident
tail, and draws with candle's LogitsProcessor, which divides by the temperature,
takes the softmax, filters by top-k then top-p, and draws with rand's WeightedIndex
from rand's StdRng. Every step here is that code at the revisions the lock pins,
candle aee9af9, rand 0.9.5, rand_core 0.9.5, rand_chacha 0.9.0, and core's
select_nth_unstable_by at rustc 47611e16044c68ef27bac31c35fda2ba1dc20b73, so the same
logits, knobs, tail and seed give the same draws. The standard library alone.

f32 arithmetic is done in f64 and rounded to f32 after each operation, which gives
the correctly rounded f32 result for addition, subtraction, multiplication and
division. The exponential is glibc's expf, the one Rust's f32::exp calls on Linux,
and the softmax's sum takes candle's AVX2 order where the host has AVX2, as candle
decides at run time.
"""
import bisect
import ctypes
import struct
from array import array

MASK32 = 0xFFFF_FFFF
MASK64 = 0xFFFF_FFFF_FFFF_FFFF


def f32(x):
    """Round to the nearest f32, as a Python float holding that value exactly."""
    return array('f', (x,))[0]


def f32s(xs):
    return array('f', xs)


def f32_bits(x):
    return struct.unpack('<I', struct.pack('<f', x))[0]


def f32_from_bits(b):
    return struct.unpack('<f', struct.pack('<I', b & MASK32))[0]


# glibc's libm by its soname, the one the Rust SPU links, never through find_library,
# which would start ldconfig in a subprocess, per python-spu-Spec section 8.
_libm = ctypes.CDLL('libm.so.6')
_expf = _libm.expf
_expf.argtypes = [ctypes.c_float]
_expf.restype = ctypes.c_float


def expf(x):
    """glibc's expf, which Rust's f32::exp calls on Linux."""
    return _expf(x)


def _has_avx2():
    """candle-core cpu/features.rs asks std::is_x86_feature_detected!("avx2"). Linux
    lists a feature in /proc/cpuinfo only where the CPU has it and the kernel enables
    its state, which is the same question."""
    try:
        with open('/proc/cpuinfo') as fh:
            for line in fh:
                if line.startswith('flags'):
                    return 'avx2' in line.split()
    except OSError:
        pass
    return False


HAS_AVX2 = _has_avx2()


# rand_core 0.9.5 SeedableRng::seed_from_u64, then rand_chacha 0.9.0 ChaCha12Rng.

def _pcg32(state):
    state = (state * 6364136223846793005 + 11634580027462260723) & MASK64
    xorshifted = (((state >> 18) ^ state) >> 27) & MASK32
    rot = state >> 59
    x = ((xorshifted >> rot) | (xorshifted << ((32 - rot) & 31))) & MASK32
    return state, x


def _rotl(x, n):
    return ((x << n) | (x >> (32 - n))) & MASK32


def _quarter(s, a, b, c, d):
    s[a] = (s[a] + s[b]) & MASK32; s[d] = _rotl(s[d] ^ s[a], 16)
    s[c] = (s[c] + s[d]) & MASK32; s[b] = _rotl(s[b] ^ s[c], 12)
    s[a] = (s[a] + s[b]) & MASK32; s[d] = _rotl(s[d] ^ s[a], 8)
    s[c] = (s[c] + s[d]) & MASK32; s[b] = _rotl(s[b] ^ s[c], 7)


class StdRng:
    """rand 0.9.5's StdRng: ChaCha12 with a 64-bit block counter from zero and a zero
    stream, its words handed out four blocks at a time in block order."""

    CONSTANTS = (0x6170_7865, 0x3320_646e, 0x7962_2d32, 0x6b20_6574)

    def __init__(self, seed):
        if type(seed) is not int or not 0 <= seed <= MASK64:
            raise ValueError('seed must be u64')
        state = seed
        key = []
        for _ in range(8):
            state, word = _pcg32(state)
            key.append(word)
        self.key = key
        self.counter = 0
        self.buffer = []
        self.index = 0

    def _block(self, counter):
        init = list(self.CONSTANTS) + self.key + [counter & MASK32, counter >> 32, 0, 0]
        s = list(init)
        for _ in range(6):
            _quarter(s, 0, 4, 8, 12); _quarter(s, 1, 5, 9, 13)
            _quarter(s, 2, 6, 10, 14); _quarter(s, 3, 7, 11, 15)
            _quarter(s, 0, 5, 10, 15); _quarter(s, 1, 6, 11, 12)
            _quarter(s, 2, 7, 8, 13); _quarter(s, 3, 4, 9, 14)
        return [(x + y) & MASK32 for x, y in zip(s, init)]

    def next_u32(self):
        if self.index >= len(self.buffer):
            self.buffer = []
            for i in range(4):
                self.buffer += self._block((self.counter + i) & MASK64)
            self.counter = (self.counter + 4) & MASK64
            self.index = 0
        word = self.buffer[self.index]
        self.index += 1
        return word


# rand 0.9.5 distr::weighted::WeightedIndex<f32> and its UniformFloat<f32>.

_F32_EPSILON = 2.0 ** -23


def weighted_index(weights, rng):
    """WeightedIndex::new then sample: cumulative weights exclude the current one, the
    total accumulates in f32, a UniformFloat over [0, total) chooses, and
    partition_point finds the first cumulative weight above the choice."""
    if not weights:
        raise ValueError('weighted index: no weights')
    total = weights[0]
    if not total >= 0.0:
        raise ValueError('weighted index: invalid weight')
    cumulative = []
    for w in weights[1:]:
        if not w >= 0.0:
            raise ValueError('weighted index: invalid weight')
        cumulative.append(total)
        total = f32(total + w)
    if total == 0.0:
        raise ValueError('weighted index: no nonzero weight')
    low, high = 0.0, total
    if not low < high:
        raise ValueError('weighted index: empty range')
    scale = f32(high - low)
    if scale in (float('inf'), float('-inf')) or scale != scale:
        raise ValueError('weighted index: non-finite range')
    max_rand = f32(1.0 - _F32_EPSILON)
    while f32(f32(scale * max_rand) + low) > high:
        scale = f32_from_bits(f32_bits(scale) - 1)
    value1_2 = f32_from_bits((rng.next_u32() >> 9) | (127 << 23))
    value0_1 = f32(value1_2 - 1.0)
    chosen = f32(f32(value0_1 * scale) + low)
    return bisect.bisect_right(cumulative, chosen)


# core's slice::select_nth_unstable_by at rustc 47611e160, for a slice of usize:
# library/core/src/slice/sort/select.rs, shared/pivot.rs, shared/smallsort.rs and
# unstable/quicksort.rs (partition_lomuto_branchless_cyclic, the variant a type of
# at most 96 bytes takes). Offsets stand in for the pointers and subslices.

_INSERTION_SORT_THRESHOLD = 16
_PSEUDO_MEDIAN_REC_THRESHOLD = 64


def select_nth_unstable_by(v, index, is_less):
    """Reorders v in place as the Rust function does, is_less(a, b) standing for
    compare(a, b) == Less."""
    n = len(v)
    if index >= n:
        raise IndexError('partition_at_index index past the slice')
    if index == n - 1:
        m = _max_index(v, 0, n, is_less)
        v[m], v[index] = v[index], v[m]
    elif index == 0:
        m = _min_index(v, 0, n, is_less)
        v[m], v[index] = v[index], v[m]
    else:
        _partition_at_index_loop(v, 0, n, index, None, is_less)
    return v


def _partition_at_index_loop(v, lo, hi, index, ancestor, is_less):
    limit = 16
    while True:
        n = hi - lo
        if n <= _INSERTION_SORT_THRESHOLD:
            if n >= 2:
                _insertion_sort_shift_left(v, lo, hi, is_less)
            return
        if limit == 0:
            _median_of_medians(v, lo, hi, index, is_less)
            return
        limit -= 1
        pivot_pos = _choose_pivot(v, lo, hi, is_less)
        if ancestor is not None and not is_less(ancestor, v[lo + pivot_pos]):
            num_lt = _partition(v, lo, hi, pivot_pos, lambda a, b: not is_less(b, a))
            mid = num_lt + 1
            if mid > index:
                return
            lo += mid
            index -= mid
            ancestor = None
            continue
        mid = _partition(v, lo, hi, pivot_pos, is_less)
        pivot = v[lo + mid]
        if mid < index:
            lo += mid + 1
            index -= mid + 1
            ancestor = pivot
        elif mid > index:
            hi = lo + mid
        else:
            return


def _min_index(v, lo, hi, is_less):
    acc = lo
    for t in range(lo + 1, hi):
        if is_less(v[t], v[acc]):
            acc = t
    return acc - lo


def _max_index(v, lo, hi, is_less):
    acc = lo
    for t in range(lo + 1, hi):
        if is_less(v[acc], v[t]):
            acc = t
    return acc - lo


def _median_of_medians(v, lo, hi, k, is_less):
    while True:
        n = hi - lo
        if n <= _INSERTION_SORT_THRESHOLD:
            if n >= 2:
                _insertion_sort_shift_left(v, lo, hi, is_less)
            return
        if k == n - 1:
            m = _max_index(v, lo, hi, is_less)
            v[lo + m], v[lo + k] = v[lo + k], v[lo + m]
            return
        if k == 0:
            m = _min_index(v, lo, hi, is_less)
            v[lo + m], v[lo + k] = v[lo + k], v[lo + m]
            return
        p = _median_of_ninthers(v, lo, hi, is_less)
        if p == k:
            return
        if p > k:
            hi = lo + p
        else:
            lo += p + 1
            k -= p + 1


def _median_of_ninthers(v, lo, hi, is_less):
    n = hi - lo
    if n <= 1024:
        frac = n // 12
    elif n <= 128 * 1024:
        frac = n // 64
    else:
        frac = n // 1024
    pivot = frac // 2
    lo_r = n // 2 - pivot
    hi_r = frac + lo_r
    gap = (n - 9 * frac) // 4
    a = lo_r - 4 * frac - gap
    b = hi_r + gap
    for i in range(lo_r, hi_r):
        _ninther(v, lo, is_less, a, i - frac, b, a + 1, i, b + 1, a + 2, i + frac, b + 2)
        a += 3
        b += 3
    _median_of_medians(v, lo + lo_r, lo + lo_r + frac, pivot, is_less)
    return _partition(v, lo, hi, lo_r + pivot, is_less)


def _ninther(v, base, is_less, a, b, c, d, e, f, g, h, i):
    def at(x):
        return v[base + x]

    def swap(x, y):
        v[base + x], v[base + y] = v[base + y], v[base + x]

    b = _median_idx(v, base, is_less, a, b, c)
    h = _median_idx(v, base, is_less, g, h, i)
    if is_less(at(h), at(b)):
        b, h = h, b
    if is_less(at(f), at(d)):
        d, f = f, d
    if is_less(at(e), at(d)):
        pass
    elif is_less(at(f), at(e)):
        d = f
    else:
        if is_less(at(e), at(b)):
            swap(e, b)
        elif is_less(at(h), at(e)):
            swap(e, h)
        return
    if is_less(at(d), at(b)):
        d = b
    elif is_less(at(h), at(d)):
        d = h
    swap(d, e)


def _median_idx(v, base, is_less, a, b, c):
    if is_less(v[base + c], v[base + a]):
        a, c = c, a
    if is_less(v[base + c], v[base + b]):
        return c
    if is_less(v[base + b], v[base + a]):
        return a
    return b


def _choose_pivot(v, lo, hi, is_less):
    n = hi - lo
    if n < 8:
        raise AssertionError('choose_pivot below eight elements')
    n8 = n // 8
    a, b, c = lo, lo + n8 * 4, lo + n8 * 7
    if n < _PSEUDO_MEDIAN_REC_THRESHOLD:
        return _median3(v, a, b, c, is_less) - lo
    return _median3_rec(v, a, b, c, n8, is_less) - lo


def _median3_rec(v, a, b, c, n, is_less):
    if n * 8 >= _PSEUDO_MEDIAN_REC_THRESHOLD:
        n8 = n // 8
        a = _median3_rec(v, a, a + n8 * 4, a + n8 * 7, n8, is_less)
        b = _median3_rec(v, b, b + n8 * 4, b + n8 * 7, n8, is_less)
        c = _median3_rec(v, c, c + n8 * 4, c + n8 * 7, n8, is_less)
    return _median3(v, a, b, c, is_less)


def _median3(v, a, b, c, is_less):
    x = is_less(v[a], v[b])
    y = is_less(v[a], v[c])
    if x == y:
        z = is_less(v[b], v[c])
        return c if z ^ x else b
    return a


def _partition(v, lo, hi, pivot, is_less):
    n = hi - lo
    if n == 0:
        return 0
    v[lo], v[lo + pivot] = v[lo + pivot], v[lo]
    num_lt = _partition_lomuto_branchless_cyclic(v, lo + 1, hi, v[lo], is_less)
    v[lo], v[lo + num_lt] = v[lo + num_lt], v[lo]
    return num_lt


def _partition_lomuto_branchless_cyclic(v, lo, hi, pivot, is_less):
    if hi - lo == 0:
        return 0
    gap_value = v[lo]
    gap = lo
    num_lt = 0
    for right in range(lo + 1, hi):
        element = v[right]
        right_is_lt = is_less(element, pivot)
        v[gap] = v[lo + num_lt]
        v[lo + num_lt] = element
        gap = right
        num_lt += right_is_lt
    right_is_lt = is_less(gap_value, pivot)
    v[gap] = v[lo + num_lt]
    v[lo + num_lt] = gap_value
    num_lt += right_is_lt
    return num_lt


def _insertion_sort_shift_left(v, lo, hi, is_less):
    for tail in range(lo + 1, hi):
        sift = tail - 1
        if not is_less(v[tail], v[sift]):
            continue
        tmp = v[tail]
        dst = tail
        while True:
            v[dst] = v[sift]
            dst = sift
            if sift == lo:
                break
            sift -= 1
            if not is_less(tmp, v[sift]):
                break
        v[dst] = tmp


def total_cmp_keys(values):
    """f32::total_cmp as an integer key per value: the bits read as i32, every bit
    but the sign flipped where the sign is set."""
    keys = []
    for bits in array('I', f32s(values).tobytes()):
        signed = bits - (1 << 32) if bits & 0x8000_0000 else bits
        keys.append(signed ^ 0x7FFF_FFFF if signed < 0 else signed)
    return keys


def select_descending(prs, k):
    """candle's argsort_indices.select_nth_unstable_by(k, |&i, &j|
    prs[j].total_cmp(&prs[i])), the whole index vector returned."""
    keys = total_cmp_keys(prs)
    indices = list(range(len(prs)))
    return select_nth_unstable_by(indices, k, lambda a, b: keys[b] < keys[a])


# candle-core: the f32 sum softmax_last_dim takes (cpu/mod.rs vec_sum, cpu/avx.rs).

def vec_sum(xs, avx2=None):
    avx2 = HAS_AVX2 if avx2 is None else avx2
    if not avx2:
        total = 0.0
        for x in xs:
            total = f32(total + x)
        return total
    n = len(xs)
    np_ = n & ~31
    acc = [f32s([0.0] * 8) for _ in range(4)]
    for i in range(0, np_, 32):
        for j in range(4):
            lanes = xs[i + 8 * j:i + 8 * j + 8]
            acc[j] = f32s([a + b for a, b in zip(acc[j], lanes)])
    x0 = f32s([a + b for a, b in zip(acc[0], acc[1])])
    x2 = f32s([a + b for a, b in zip(acc[2], acc[3])])
    x0 = f32s([a + b for a, b in zip(x0, x2)])
    t0 = f32s([x0[l] + x0[l + 4] for l in range(4)])
    total = f32(f32(t0[0] + t0[1]) + f32(t0[2] + t0[3]))
    for i in range(np_, n):
        total = f32(total + xs[i])
    return total


def _f32_max(a, b):
    """f32::max: a NaN yields the other operand."""
    if a != a:
        return b
    if b != b:
        return a
    return b if b > a else a


def softmax(values, avx2=None):
    """candle-nn ops.rs SoftmaxLastDim's CPU f32 path over one row."""
    m = values[0]
    for x in values[1:]:
        m = _f32_max(m, x)
    exps = f32s([expf(f32(x - m)) for x in values])
    total = vec_sum(exps, avx2)
    return f32s([e / total for e in exps])


def apply_repeat_penalty(values, penalty, context):
    """candle-transformers utils.rs: each distinct token of the context once, a
    non-negative logit divided by the penalty and a negative one multiplied."""
    out = f32s(values)
    seen = set()
    for token in context:
        if token in seen:
            continue
        seen.add(token)
        if 0 <= token < len(out):
            logit = out[token]
            out[token] = logit / penalty if logit >= 0.0 else logit * penalty
    return out


def penalised(logits, penalty, window, resident):
    """native.rs's sample: the penalty over the resident tail's last window, skipped
    where the penalty is 1 or the window 0."""
    values = f32s(logits)
    if penalty != 1.0 and window > 0:
        start = max(0, len(resident) - window)
        values = apply_repeat_penalty(values, penalty, resident[start:])
    return values


def probabilities(values, temperature, avx2=None):
    """candle's prs closure in LogitsProcessor::sample_f: `&logits / temperature`,
    which is affine(1 / temperature, 0), the reciprocal taken in f64 and cast to f32,
    then a multiply and an add of zero, and the softmax."""
    mul = f32(1.0 / temperature)
    scaled = f32s([f32(x * mul) + 0.0 for x in values])
    return softmax(scaled, avx2)


class Sampler:
    """One generation's sampler: candle's LogitsProcessor built once from the derived
    seed and the knobs as native.rs builds it, the penalty applied per draw over the
    resident tail as native.rs's sample applies it."""

    def __init__(self, seed, temperature=.7, top_k=40, top_p=.95, penalty=1.1, window=64,
                 avx2=None):
        self.rng = StdRng(seed)
        self.temperature = f32(temperature)
        self.top_k = int(top_k)
        self.top_p = f32(top_p)
        self.penalty = f32(penalty)
        self.window = int(window)
        self.avx2 = avx2
        if self.temperature <= 0.0:
            self.mode = 'argmax'
        elif self.top_k == 0:
            self.mode = 'top_p'
        else:
            self.mode = 'top_k_then_top_p'

    def sample(self, logits, resident):
        values = penalised(logits, self.penalty, self.window, resident)
        if self.mode == 'argmax':
            return _argmax(values)
        prs = probabilities(values, self.temperature, self.avx2)
        if self.mode == 'top_p':
            p = self.top_p
            if p <= 0.0 or p >= 1.0:
                return weighted_index(list(prs), self.rng)
            return self._sample_topp(list(prs), p)
        return self._sample_topk_topp(list(prs), self.top_k, self.top_p)

    def _sample_topp(self, prs, top_p):
        keys = total_cmp_keys(prs)
        order = sorted(range(len(prs)), key=lambda i: -keys[i])
        cumsum = 0.0
        for index in order:
            if cumsum >= top_p:
                prs[index] = 0.0
            else:
                cumsum = f32(cumsum + prs[index])
        return weighted_index(prs, self.rng)

    def _sample_topk_topp(self, prs, top_k, top_p):
        if top_k >= len(prs):
            return self._sample_topp(prs, top_p)
        indices = select_descending(prs, top_k)[:top_k]
        kept = [prs[i] for i in indices]
        sum_p = 0.0
        for p in kept:
            sum_p = f32(sum_p + p)
        if top_p <= 0.0 or top_p >= sum_p:
            index = weighted_index(kept, self.rng)
        else:
            index = self._sample_topp(kept, top_p)
        return indices[index]


def _argmax(values):
    """candle-core cpu_backend ReduceIndex for argmax: the first maximum, replaced
    only by a strictly greater value, so a NaN never replaces."""
    acc = 0
    val = values[0]
    for i, s in enumerate(values):
        if val < s:
            acc = i
            val = s
    return acc
