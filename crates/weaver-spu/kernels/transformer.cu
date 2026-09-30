// WeaverTools conformance trace (generated from the `weavertools` graph; documentation-code-conformance-methodology sec 1).
// crate: weaver-spu. This load-bearing file implements the spec node(s) below; the anchor is the stable node id, not prose.
//   - spec-02-only-crate-holds-gpu-memory  [embodies]  (docs/architecture/weaver-spu/weaver-spu-Spec.md)
// graph: weavertools conformance graph :: wt_doc_assertions/<node> -> wt_axiom_basis -> WT_IS (the code->spec->axiom trace).
//
// Transformer inference kernels for Qwen/Llama/Mistral/Gemma4 architectures.
//
// All kernels operate on f16 data (__half). Element-wise operations
// accumulate in f32 internally for numerical stability.
//
// Each __global__ kernel has a corresponding extern "C" host-side
// launcher function that the Rust FFI calls. The launcher computes
// grid/block dimensions and invokes the kernel.
//
// Compiled to SASS for sm_86 (A6000) and sm_89 (RTX Ada) via build.rs.

#include <cuda_fp16.h>
#include <cuda_runtime.h>
#include <math.h>

// Block size for 1D kernels
#define BLOCK_SIZE 256

// ============================================================================
// RMSNorm: x_norm = x * rsqrt(mean(x^2) + eps) * weight
// ============================================================================

static __global__ void rmsnorm_kernel(
    const half* __restrict__ input,
    const half* __restrict__ weight,
    half* __restrict__ output,
    int hidden_size,
    float eps
) {
    int row = blockIdx.x;
    int tid = threadIdx.x;
    int stride = blockDim.x;

    const half* x = input + (long long)row * hidden_size;
    half* out = output + (long long)row * hidden_size;

    extern __shared__ float sdata[];

    float local_sum = 0.0f;
    for (int i = tid; i < hidden_size; i += stride) {
        float val = __half2float(x[i]);
        local_sum += val * val;
    }
    sdata[tid] = local_sum;
    __syncthreads();

    for (int s = stride / 2; s > 0; s >>= 1) {
        if (tid < s) {
            sdata[tid] += sdata[tid + s];
        }
        __syncthreads();
    }

    float rms = rsqrtf(sdata[0] / (float)hidden_size + eps);

    for (int i = tid; i < hidden_size; i += stride) {
        float val = __half2float(x[i]) * rms;
        out[i] = __float2half(val * __half2float(weight[i]));
    }
}

extern "C" void launch_rmsnorm(
    const void* input,
    const void* weight,
    void* output,
    int batch_size,
    int hidden_size,
    float eps,
    void* stream
) {
    int threads = (hidden_size < BLOCK_SIZE) ? hidden_size : BLOCK_SIZE;
    int smem = threads * sizeof(float);
    rmsnorm_kernel<<<batch_size, threads, smem, (cudaStream_t)stream>>>(
        (const half*)input, (const half*)weight, (half*)output,
        hidden_size, eps
    );
}

// ============================================================================
// RoPE: Rotary Position Embedding (rotary-half convention)
//
// Qwen/Llama/Mistral use rotary-half: pairs are (d[i], d[i + half_dim])
// NOT interleaved pairs (d[2i], d[2i+1]).
//
// For each pair:
//   new_d[i]            = d[i] * cos(angle) - d[i + half_dim] * sin(angle)
//   new_d[i + half_dim] = d[i] * sin(angle) + d[i + half_dim] * cos(angle)
// ============================================================================

static __global__ void rope_kernel(
    half* __restrict__ qk,      // [seq_len, num_heads, head_dim]
    int num_heads,
    int head_dim,
    int pos_offset,
    float theta_base
) {
    int seq_idx = blockIdx.x;
    int head = blockIdx.y;
    int tid = threadIdx.x;
    int half_dim = head_dim / 2;

    if (tid >= half_dim || head >= num_heads) return;

    int pos = seq_idx + pos_offset;
    float freq = 1.0f / powf(theta_base, (float)(2 * tid) / (float)head_dim);
    float angle = (float)pos * freq;
    float cos_val = cosf(angle);
    float sin_val = sinf(angle);

    long long base_idx = ((long long)seq_idx * num_heads + head) * head_dim;
    float v0 = __half2float(qk[base_idx + tid]);
    float v1 = __half2float(qk[base_idx + tid + half_dim]);
    qk[base_idx + tid]            = __float2half(v0 * cos_val - v1 * sin_val);
    qk[base_idx + tid + half_dim] = __float2half(v0 * sin_val + v1 * cos_val);
}

extern "C" void launch_rope(
    void* q,
    void* k,
    int seq_len,
    int num_heads,
    int num_kv_heads,
    int head_dim,
    int pos_offset,
    float theta_base,
    void* stream
) {
    int threads = head_dim / 2;
    if (threads > BLOCK_SIZE) threads = BLOCK_SIZE;
    dim3 grid_q(seq_len, num_heads);
    dim3 grid_k(seq_len, num_kv_heads);

    rope_kernel<<<grid_q, threads, 0, (cudaStream_t)stream>>>(
        (half*)q, num_heads, head_dim, pos_offset, theta_base
    );
    rope_kernel<<<grid_k, threads, 0, (cudaStream_t)stream>>>(
        (half*)k, num_kv_heads, head_dim, pos_offset, theta_base
    );
}

// ============================================================================
// RoPE from precomputed cos/sin tables (gpt-oss / YaRN). Same NeoX half-split
// rotation as rope_kernel, but cos/sin are read from [max_pos, head_dim/2]
// tables (which already fold in any YaRN mscale) instead of being computed from
// a single theta. Tables are f32.
// ============================================================================

static __global__ void rope_tables_kernel(
    half* __restrict__ qk,             // [seq_len, num_heads, head_dim]
    const float* __restrict__ cos_tab, // [max_pos, head_dim/2]
    const float* __restrict__ sin_tab, // [max_pos, head_dim/2]
    int num_heads,
    int head_dim,
    int pos_offset,
    int max_pos
) {
    int seq_idx = blockIdx.x;
    int head = blockIdx.y;
    int tid = threadIdx.x;
    int half_dim = head_dim / 2;
    if (tid >= half_dim || head >= num_heads) return;

    int pos = seq_idx + pos_offset;
    if (pos < 0 || pos >= max_pos) return; // guard against reads past the table
    float cos_val = cos_tab[(long long)pos * half_dim + tid];
    float sin_val = sin_tab[(long long)pos * half_dim + tid];

    long long base_idx = ((long long)seq_idx * num_heads + head) * head_dim;
    float v0 = __half2float(qk[base_idx + tid]);
    float v1 = __half2float(qk[base_idx + tid + half_dim]);
    qk[base_idx + tid]            = __float2half(v0 * cos_val - v1 * sin_val);
    qk[base_idx + tid + half_dim] = __float2half(v0 * sin_val + v1 * cos_val);
}

extern "C" void launch_rope_from_tables(
    void* q,
    void* k,
    const void* cos_tab,
    const void* sin_tab,
    int seq_len,
    int num_heads,
    int num_kv_heads,
    int head_dim,
    int pos_offset,
    int max_pos,
    void* stream
) {
    int threads = head_dim / 2;
    if (threads > BLOCK_SIZE) threads = BLOCK_SIZE;
    dim3 grid_q(seq_len, num_heads);
    dim3 grid_k(seq_len, num_kv_heads);

    rope_tables_kernel<<<grid_q, threads, 0, (cudaStream_t)stream>>>(
        (half*)q, (const float*)cos_tab, (const float*)sin_tab, num_heads, head_dim, pos_offset, max_pos
    );
    rope_tables_kernel<<<grid_k, threads, 0, (cudaStream_t)stream>>>(
        (half*)k, (const float*)cos_tab, (const float*)sin_tab, num_kv_heads, head_dim, pos_offset, max_pos
    );
}

// ============================================================================
// SwiGLU: output = silu(gate) * up, where silu(x) = x * sigmoid(x)
// ============================================================================

static __global__ void swiglu_kernel(
    const half* __restrict__ gate,
    const half* __restrict__ up,
    half* __restrict__ output,
    int total_elements
) {
    int idx = blockIdx.x * blockDim.x + threadIdx.x;
    if (idx >= total_elements) return;

    float g = __half2float(gate[idx]);
    float u = __half2float(up[idx]);
    float silu_g = g / (1.0f + expf(-g));
    output[idx] = __float2half(silu_g * u);
}

extern "C" void launch_swiglu(
    const void* gate,
    const void* up,
    void* output,
    int total_elements,
    void* stream
) {
    int blocks = (total_elements + BLOCK_SIZE - 1) / BLOCK_SIZE;
    swiglu_kernel<<<blocks, BLOCK_SIZE, 0, (cudaStream_t)stream>>>(
        (const half*)gate, (const half*)up, (half*)output, total_elements
    );
}

// ============================================================================
// gpt-oss clamped-SwiGLU expert activation.
// gate is clamped to (-inf, limit], up is clamped to [-limit, limit];
// glu = gate * sigmoid(alpha * gate); output = (up + 1) * glu.
// (candle gpt_oss.rs Expert::forward; gpt-oss uses alpha = 1.702, limit = 7.0.)
// ============================================================================

static __global__ void clamped_swiglu_kernel(
    const half* __restrict__ gate,
    const half* __restrict__ up,
    half* __restrict__ output,
    int total_elements,
    float limit,
    float alpha
) {
    int idx = blockIdx.x * blockDim.x + threadIdx.x;
    if (idx >= total_elements) return;

    float g = __half2float(gate[idx]);
    float u = __half2float(up[idx]);
    g = fminf(g, limit);                // gate clamped to +limit (upper only)
    u = fminf(fmaxf(u, -limit), limit); // up clamped to [-limit, limit]
    float glu = g / (1.0f + expf(-alpha * g)); // g * sigmoid(alpha * g)
    output[idx] = __float2half((u + 1.0f) * glu);
}

extern "C" void launch_clamped_swiglu(
    const void* gate,
    const void* up,
    void* output,
    int total_elements,
    float limit,
    float alpha,
    void* stream
) {
    int blocks = (total_elements + BLOCK_SIZE - 1) / BLOCK_SIZE;
    clamped_swiglu_kernel<<<blocks, BLOCK_SIZE, 0, (cudaStream_t)stream>>>(
        (const half*)gate, (const half*)up, (half*)output, total_elements, limit, alpha
    );
}

// ============================================================================
// Residual add: output += residual (in-place)
// ============================================================================

static __global__ void residual_add_kernel(
    half* __restrict__ output,
    const half* __restrict__ residual,
    int total_elements
) {
    int idx = blockIdx.x * blockDim.x + threadIdx.x;
    if (idx >= total_elements) return;
    float val = __half2float(output[idx]) + __half2float(residual[idx]);
    output[idx] = __float2half(val);
}

extern "C" void launch_residual_add(
    void* output,
    const void* residual,
    int total_elements,
    void* stream
) {
    int blocks = (total_elements + BLOCK_SIZE - 1) / BLOCK_SIZE;
    residual_add_kernel<<<blocks, BLOCK_SIZE, 0, (cudaStream_t)stream>>>(
        (half*)output, (const half*)residual, total_elements
    );
}

// ============================================================================
// Bias add: output[pos * dim + d] += bias[d] for all positions
// ============================================================================

static __global__ void bias_add_kernel(
    half* __restrict__ output,
    const half* __restrict__ bias,
    int dim,
    int seq_len
) {
    int pos = blockIdx.x;
    int tid = threadIdx.x;
    int stride = blockDim.x;

    if (pos >= seq_len) return;

    half* out = output + (long long)pos * dim;
    for (int d = tid; d < dim; d += stride) {
        float val = __half2float(out[d]) + __half2float(bias[d]);
        out[d] = __float2half(val);
    }
}

extern "C" void launch_bias_add(
    void* output,
    const void* bias,
    int dim,
    int seq_len,
    void* stream
) {
    int threads = (dim < BLOCK_SIZE) ? dim : BLOCK_SIZE;
    bias_add_kernel<<<seq_len, threads, 0, (cudaStream_t)stream>>>(
        (half*)output, (const half*)bias, dim, seq_len
    );
}

// ============================================================================
// Embedding lookup: output[i] = embed_table[token_ids[i]]
// ============================================================================

static __global__ void embedding_kernel(
    const half* __restrict__ embed_table,
    const int* __restrict__ token_ids,
    half* __restrict__ output,
    int hidden_size,
    int seq_len
) {
    int pos = blockIdx.x;
    int tid = threadIdx.x;
    int stride = blockDim.x;

    if (pos >= seq_len) return;

    int token_id = token_ids[pos];
    const half* row = embed_table + (long long)token_id * hidden_size;
    half* out = output + (long long)pos * hidden_size;

    for (int i = tid; i < hidden_size; i += stride) {
        out[i] = row[i];
    }
}

extern "C" void launch_embedding(
    const void* embed_table,
    const int* token_ids,
    void* output,
    int hidden_size,
    int seq_len,
    void* stream
) {
    int threads = (hidden_size < BLOCK_SIZE) ? hidden_size : BLOCK_SIZE;
    embedding_kernel<<<seq_len, threads, 0, (cudaStream_t)stream>>>(
        (const half*)embed_table, token_ids, (half*)output,
        hidden_size, seq_len
    );
}

// ============================================================================
// Causal attention (single-head, strided access for GQA)
//
// Q, K, V, output may be strided — heads are interleaved within each
// position in the multi-head tensor. The stride parameters tell the
// kernel how many elements to skip between consecutive positions.
//
// For prefill (seq_len > 1): full QK^T + causal mask + softmax + V
// For decode (seq_len == 1): dot product attention against KV cache
//
// This is a simple implementation. For production, use FlashAttention.
// ============================================================================

static __global__ void attention_scores_kernel(
    const half* __restrict__ q,      // [seq_len, ...] one head, strided
    const half* __restrict__ k,      // [total_len, ...] one KV head, strided
    float* __restrict__ scores,      // [seq_len, total_len]
    int seq_len,
    int total_len,
    int head_dim,
    int q_stride,                    // elements between positions in Q
    int kv_stride,                   // elements between positions in K
    float scale
) {
    int q_pos = blockIdx.x;
    int tid = threadIdx.x;
    int stride = blockDim.x;

    if (q_pos >= seq_len) return;

    const half* q_row = q + (long long)q_pos * q_stride;

    for (int k_pos = tid; k_pos < total_len; k_pos += stride) {
        // Causal mask: only attend to positions <= q_pos + (total_len - seq_len)
        int q_abs = q_pos + (total_len - seq_len);
        if (k_pos > q_abs) {
            scores[q_pos * total_len + k_pos] = -1e9f;
            continue;
        }

        const half* k_row = k + (long long)k_pos * kv_stride;
        float dot = 0.0f;
        for (int d = 0; d < head_dim; d++) {
            dot += __half2float(q_row[d]) * __half2float(k_row[d]);
        }
        scores[q_pos * total_len + k_pos] = dot * scale;
    }
}

static __global__ void softmax_kernel(
    float* __restrict__ data,
    int cols
) {
    int row = blockIdx.x;
    int tid = threadIdx.x;
    int stride = blockDim.x;
    float* row_data = data + (long long)row * cols;

    extern __shared__ float sdata[];

    // Find max
    float local_max = -1e30f;
    for (int i = tid; i < cols; i += stride) {
        if (row_data[i] > local_max) local_max = row_data[i];
    }
    sdata[tid] = local_max;
    __syncthreads();

    for (int s = stride / 2; s > 0; s >>= 1) {
        if (tid < s) sdata[tid] = fmaxf(sdata[tid], sdata[tid + s]);
        __syncthreads();
    }
    float max_val = sdata[0];

    // Sum of exp
    float local_sum = 0.0f;
    for (int i = tid; i < cols; i += stride) {
        row_data[i] = expf(row_data[i] - max_val);
        local_sum += row_data[i];
    }
    sdata[tid] = local_sum;
    __syncthreads();

    for (int s = stride / 2; s > 0; s >>= 1) {
        if (tid < s) sdata[tid] += sdata[tid + s];
        __syncthreads();
    }

    float inv_sum = 1.0f / sdata[0];
    for (int i = tid; i < cols; i += stride) {
        row_data[i] *= inv_sum;
    }
}

static __global__ void attention_output_kernel(
    const float* __restrict__ scores, // [seq_len, total_len]
    const half* __restrict__ v,       // [total_len, ...] one KV head, strided
    half* __restrict__ output,        // [seq_len, ...] one head, strided
    int seq_len,
    int total_len,
    int head_dim,
    int kv_stride,                    // elements between positions in V
    int out_stride                    // elements between positions in output
) {
    int q_pos = blockIdx.x;
    int d = threadIdx.x;

    if (q_pos >= seq_len || d >= head_dim) return;

    const float* score_row = scores + (long long)q_pos * total_len;

    float acc = 0.0f;
    for (int k_pos = 0; k_pos < total_len; k_pos++) {
        acc += score_row[k_pos] * __half2float(v[(long long)k_pos * kv_stride + d]);
    }
    output[(long long)q_pos * out_stride + d] = __float2half(acc);
}

extern "C" void launch_attention(
    const void* q,         // [seq_len, ...] one head (strided)
    const void* k,         // [total_len, ...] one KV head (strided)
    const void* v,         // [total_len, ...] one KV head (strided)
    void* output,          // [seq_len, ...] one head (strided)
    void* scores_buf,      // [seq_len, total_len] f32 scratch
    int seq_len,
    int total_len,
    int head_dim,
    int q_stride,          // stride between positions in Q/output
    int kv_stride,         // stride between positions in K/V
    int out_stride,        // stride between positions in output
    float scale,
    void* stream
) {
    cudaStream_t s = (cudaStream_t)stream;

    // Step 1: QK^T scores with causal mask
    int threads1 = (total_len < BLOCK_SIZE) ? total_len : BLOCK_SIZE;
    attention_scores_kernel<<<seq_len, threads1, 0, s>>>(
        (const half*)q, (const half*)k, (float*)scores_buf,
        seq_len, total_len, head_dim, q_stride, kv_stride, scale
    );

    // Step 2: Softmax
    int threads2 = (total_len < BLOCK_SIZE) ? total_len : BLOCK_SIZE;
    int t2 = 1;
    while (t2 < threads2) t2 <<= 1;
    if (t2 > BLOCK_SIZE) t2 = BLOCK_SIZE;
    int smem = t2 * sizeof(float);
    softmax_kernel<<<seq_len, t2, smem, s>>>(
        (float*)scores_buf, total_len
    );

    // Step 3: Score * V
    int threads3 = (head_dim < BLOCK_SIZE) ? head_dim : BLOCK_SIZE;
    attention_output_kernel<<<seq_len, threads3, 0, s>>>(
        (const float*)scores_buf, (const half*)v, (half*)output,
        seq_len, total_len, head_dim, kv_stride, out_stride
    );
}

// ============================================================================
// gpt-oss attention sinks. The softmax denominator gets an extra
// exp(sink_logit) term (a per-head learnable logit), so the real-key
// probabilities sum to < 1 — the sink absorbs probability mass and contributes
// nothing to the value-weighted sum. (candle gpt_oss.rs Attention::forward:
// cat([scores, sink]) -> softmax_last_dim -> drop the sink column -> @ V.)
// ============================================================================

// gpt-oss attention scores: q.k^T * scale with a causal mask and an optional
// sliding window. HF-strict sliding rule: masked when (q_abs - k_pos) >= window.
// window <= 0 means full causal (no sliding window).
static __global__ void gptoss_scores_kernel(
    const half* __restrict__ q,
    const half* __restrict__ k,
    float* __restrict__ scores,
    int seq_len,
    int total_len,
    int head_dim,
    int q_stride,
    int kv_stride,
    float scale,
    int window
) {
    int q_pos = blockIdx.x;
    int tid = threadIdx.x;
    int stride = blockDim.x;
    if (q_pos >= seq_len) return;

    const half* q_row = q + (long long)q_pos * q_stride;
    int q_abs = q_pos + (total_len - seq_len);

    for (int k_pos = tid; k_pos < total_len; k_pos += stride) {
        // Causal mask.
        if (k_pos > q_abs) {
            scores[q_pos * total_len + k_pos] = -1e9f;
            continue;
        }
        // Sliding window (HF-strict): masked when q_abs - k_pos >= window.
        if (window > 0 && (q_abs - k_pos) >= window) {
            scores[q_pos * total_len + k_pos] = -1e9f;
            continue;
        }
        const half* k_row = k + (long long)k_pos * kv_stride;
        float dot = 0.0f;
        for (int d = 0; d < head_dim; d++) {
            dot += __half2float(q_row[d]) * __half2float(k_row[d]);
        }
        scores[q_pos * total_len + k_pos] = dot * scale;
    }
}

static __global__ void softmax_with_sink_kernel(
    float* __restrict__ data,
    int cols,
    float sink_logit
) {
    int row = blockIdx.x;
    int tid = threadIdx.x;
    int stride = blockDim.x;
    float* row_data = data + (long long)row * cols;

    extern __shared__ float sdata[];

    float local_max = -1e30f;
    for (int i = tid; i < cols; i += stride) {
        if (row_data[i] > local_max) local_max = row_data[i];
    }
    sdata[tid] = local_max;
    __syncthreads();
    for (int s = stride / 2; s > 0; s >>= 1) {
        if (tid < s) sdata[tid] = fmaxf(sdata[tid], sdata[tid + s]);
        __syncthreads();
    }
    float max_val = fmaxf(sdata[0], sink_logit); // sink participates in the max

    float local_sum = 0.0f;
    for (int i = tid; i < cols; i += stride) {
        row_data[i] = expf(row_data[i] - max_val);
        local_sum += row_data[i];
    }
    sdata[tid] = local_sum;
    __syncthreads();
    for (int s = stride / 2; s > 0; s >>= 1) {
        if (tid < s) sdata[tid] += sdata[tid + s];
        __syncthreads();
    }
    // Denominator includes the sink term; real entries normalized by it.
    float inv_sum = 1.0f / (sdata[0] + expf(sink_logit - max_val));
    for (int i = tid; i < cols; i += stride) {
        row_data[i] *= inv_sum;
    }
}

extern "C" void launch_attention_sinks(
    const void* q,
    const void* k,
    const void* v,
    void* output,
    void* scores_buf,
    int seq_len,
    int total_len,
    int head_dim,
    int q_stride,
    int kv_stride,
    int out_stride,
    float scale,
    float sink_logit,
    int window,           // <= 0: full causal; > 0: sliding-window layers
    void* stream
) {
    cudaStream_t s = (cudaStream_t)stream;

    int threads1 = (total_len < BLOCK_SIZE) ? total_len : BLOCK_SIZE;
    gptoss_scores_kernel<<<seq_len, threads1, 0, s>>>(
        (const half*)q, (const half*)k, (float*)scores_buf,
        seq_len, total_len, head_dim, q_stride, kv_stride, scale, window
    );

    int threads2 = (total_len < BLOCK_SIZE) ? total_len : BLOCK_SIZE;
    int t2 = 1;
    while (t2 < threads2) t2 <<= 1;
    if (t2 > BLOCK_SIZE) t2 = BLOCK_SIZE;
    int smem = t2 * sizeof(float);
    softmax_with_sink_kernel<<<seq_len, t2, smem, s>>>(
        (float*)scores_buf, total_len, sink_logit
    );

    int threads3 = (head_dim < BLOCK_SIZE) ? head_dim : BLOCK_SIZE;
    attention_output_kernel<<<seq_len, threads3, 0, s>>>(
        (const float*)scores_buf, (const half*)v, (half*)output,
        seq_len, total_len, head_dim, kv_stride, out_stride
    );
}

// ============================================================================
// Split fused projection output into separate contiguous buffers.
//
// After a fused QKV or Gate+Up GEMM, the output is [seq_len, total_dim]
// with sub-tensors interleaved per row. This kernel deinterleaves into
// separate contiguous output buffers.
// ============================================================================

static __global__ void split_fused_kernel(
    const half* __restrict__ fused,   // [seq_len, total_dim]
    half* __restrict__ out_a,         // [seq_len, dim_a]
    half* __restrict__ out_b,         // [seq_len, dim_b]
    half* __restrict__ out_c,         // [seq_len, dim_c] or NULL for 2-way split
    int seq_len,
    int total_dim,
    int dim_a,
    int dim_b
) {
    int idx = blockIdx.x * blockDim.x + threadIdx.x;
    int total = seq_len * total_dim;
    if (idx >= total) return;

    int pos = idx / total_dim;
    int d = idx % total_dim;
    half val = fused[idx];

    if (d < dim_a) {
        out_a[pos * dim_a + d] = val;
    } else if (d < dim_a + dim_b) {
        out_b[pos * dim_b + (d - dim_a)] = val;
    } else if (out_c) {
        int dim_c = total_dim - dim_a - dim_b;
        out_c[pos * dim_c + (d - dim_a - dim_b)] = val;
    }
}

extern "C" void launch_split_fused(
    const void* fused,
    void* out_a,
    void* out_b,
    void* out_c,    // NULL for 2-way split
    int seq_len,
    int total_dim,
    int dim_a,
    int dim_b,
    void* stream
) {
    int total = seq_len * total_dim;
    int blocks = (total + BLOCK_SIZE - 1) / BLOCK_SIZE;
    split_fused_kernel<<<blocks, BLOCK_SIZE, 0, (cudaStream_t)stream>>>(
        (const half*)fused,
        (half*)out_a, (half*)out_b, (half*)out_c,
        seq_len, total_dim, dim_a, dim_b
    );
}

// ============================================================================
// FlashAttention 2 — fused tiled attention with online softmax
//
// Replaces the 3-kernel naive pipeline (scores → softmax → output) with a
// single kernel that tiles Q into Br-row blocks, iterates K/V in Bc-row
// blocks, and uses online softmax (running max + sum) so the full
// [seq_len × total_len] attention matrix is never materialized.
//
// Handles GQA natively: multiple Q heads share the same KV head.
//
// Grid:  (ceil(seq_len / BR), num_heads)
// Block: 128 threads
// Smem:  Q tile (16 KB) + KV tile (16 KB) + score tile (16 KB) = 48 KB
// ============================================================================

#define FA2_BR 64
#define FA2_BC 64
#define FA2_THREADS 128
#define FA2_MAX_HALF_DIM 128  // head_dim / 2, supports up to head_dim=256

static __global__ void __launch_bounds__(FA2_THREADS, 4)
flash_attention_2_kernel(
    const half* __restrict__ Q,   // [seq_len, num_heads * head_dim]
    const half* __restrict__ K,   // [total_len, num_kv_heads * head_dim]
    const half* __restrict__ V,   // [total_len, num_kv_heads * head_dim]
    half* __restrict__ O,         // [seq_len, num_heads * head_dim]
    int seq_len,
    int total_len,
    int num_heads,
    int num_kv_heads,
    int head_dim,
    float scale,
    int causal
) {
    int q_tile_idx = blockIdx.x;
    int head = blockIdx.y;
    int tid = threadIdx.x;

    int br_start = q_tile_idx * FA2_BR;
    int gqa_groups = num_heads / num_kv_heads;
    int kv_head = head / gqa_groups;
    int pos_offset = total_len - seq_len;

    // Thread-to-output mapping: 2 threads per Q row, each handles half of head_dim
    int my_row = tid % FA2_BR;           // 0..63
    int dim_half = tid / FA2_BR;         // 0 or 1
    int half_dim = head_dim / 2;         // 64 for hd=128
    int dim_start = dim_half * half_dim;

    // Global memory strides
    int q_stride = num_heads * head_dim;
    int kv_stride = num_kv_heads * head_dim;

    // Shared memory: Q tile | KV tile | Score tile
    extern __shared__ char smem_raw[];
    half*  smem_Q  = (half*)smem_raw;
    half*  smem_KV = smem_Q + FA2_BR * head_dim;
    float* smem_S  = (float*)(smem_KV + FA2_BC * head_dim);

    // Online softmax state (per-thread, per-row)
    float m_val = -1e30f;
    float l_val = 0.0f;

    // Output accumulator — half_dim registers (64 for hd=128)
    float O_acc[FA2_MAX_HALF_DIM];
    for (int d = 0; d < half_dim; d++) {
        O_acc[d] = 0.0f;
    }

    // ---- Load Q tile (stays resident for all KV iterations) ----
    {
        int total_elems = FA2_BR * head_dim;
        for (int idx = tid; idx < total_elems; idx += FA2_THREADS) {
            int row = idx / head_dim;
            int d   = idx % head_dim;
            int global_row = br_start + row;
            if (global_row < seq_len) {
                smem_Q[idx] = Q[(long long)global_row * q_stride + (long long)head * head_dim + d];
            } else {
                smem_Q[idx] = __float2half(0.0f);
            }
        }
    }
    __syncthreads();

    // ---- KV block loop ----
    int num_kv_blocks = (total_len + FA2_BC - 1) / FA2_BC;

    for (int kv_blk = 0; kv_blk < num_kv_blocks; kv_blk++) {
        int kv_start = kv_blk * FA2_BC;

        // Causal early exit: all K positions in this block are beyond all Q positions
        if (causal) {
            int max_q_abs = br_start + FA2_BR - 1 + pos_offset;
            if (kv_start > max_q_abs) break;
        }

        // ---- Load K block ----
        {
            int total_elems = FA2_BC * head_dim;
            for (int idx = tid; idx < total_elems; idx += FA2_THREADS) {
                int row = idx / head_dim;
                int d   = idx % head_dim;
                int global_row = kv_start + row;
                if (global_row < total_len) {
                    smem_KV[idx] = K[(long long)global_row * kv_stride + (long long)kv_head * head_dim + d];
                } else {
                    smem_KV[idx] = __float2half(0.0f);
                }
            }
        }
        __syncthreads();

        // ---- S = Q @ K^T * scale (with causal mask) → smem_S ----
        {
            int total_s = FA2_BR * FA2_BC;
            for (int idx = tid; idx < total_s; idx += FA2_THREADS) {
                int i = idx / FA2_BC;
                int j = idx % FA2_BC;

                float dot = 0.0f;
                for (int d = 0; d < head_dim; d++) {
                    dot += __half2float(smem_Q[i * head_dim + d])
                         * __half2float(smem_KV[j * head_dim + d]);
                }
                dot *= scale;

                // Causal mask: Q at absolute position q_abs can only attend to k_abs <= q_abs
                if (causal) {
                    int q_abs = br_start + i + pos_offset;
                    int k_abs = kv_start + j;
                    if (k_abs > q_abs) dot = -1e30f;
                }
                // Mask out-of-bounds rows
                if ((br_start + i) >= seq_len || (kv_start + j) >= total_len) {
                    dot = -1e30f;
                }

                smem_S[idx] = dot;
            }
        }
        __syncthreads();

        // ---- Online softmax update for my_row ----
        // Both threads sharing this row (dim_half=0,1) compute identical m/l values
        float block_max = -1e30f;
        for (int j = 0; j < FA2_BC; j++) {
            float s = smem_S[my_row * FA2_BC + j];
            if (s > block_max) block_max = s;
        }

        float m_new = fmaxf(m_val, block_max);
        float alpha = expf(m_val - m_new);

        float block_sum = 0.0f;
        for (int j = 0; j < FA2_BC; j++) {
            block_sum += expf(smem_S[my_row * FA2_BC + j] - m_new);
        }

        l_val = l_val * alpha + block_sum;
        for (int d = 0; d < half_dim; d++) {
            O_acc[d] *= alpha;
        }
        m_val = m_new;

        __syncthreads();

        // ---- Load V block (reuse smem_KV, K no longer needed) ----
        {
            int total_elems = FA2_BC * head_dim;
            for (int idx = tid; idx < total_elems; idx += FA2_THREADS) {
                int row = idx / head_dim;
                int d   = idx % head_dim;
                int global_row = kv_start + row;
                if (global_row < total_len) {
                    smem_KV[idx] = V[(long long)global_row * kv_stride + (long long)kv_head * head_dim + d];
                } else {
                    smem_KV[idx] = __float2half(0.0f);
                }
            }
        }
        __syncthreads();

        // ---- O_acc += P @ V (P recomputed from S and m_val) ----
        for (int j = 0; j < FA2_BC; j++) {
            float p_val = expf(smem_S[my_row * FA2_BC + j] - m_val);
            if (p_val > 1e-10f) {
                for (int d = 0; d < half_dim; d++) {
                    O_acc[d] += p_val * __half2float(smem_KV[j * head_dim + dim_start + d]);
                }
            }
        }

        __syncthreads();
    }

    // ---- Normalize and write output ----
    int global_row = br_start + my_row;
    if (global_row < seq_len) {
        float inv_l = (l_val > 0.0f) ? (1.0f / l_val) : 0.0f;
        long long out_base = (long long)global_row * q_stride + (long long)head * head_dim;
        for (int d = 0; d < half_dim; d++) {
            O[out_base + dim_start + d] = __float2half(O_acc[d] * inv_l);
        }
    }
}

extern "C" void launch_flash_attention(
    const void* Q,
    const void* K,
    const void* V,
    void* O,
    int seq_len,
    int total_len,
    int num_heads,
    int num_kv_heads,
    int head_dim,
    float scale,
    int causal,
    void* stream
) {
    int q_tiles = (seq_len + FA2_BR - 1) / FA2_BR;
    dim3 grid(q_tiles, num_heads);
    dim3 block(FA2_THREADS);

    int smem_bytes = FA2_BR * head_dim * sizeof(half)     // smem_Q
                   + FA2_BC * head_dim * sizeof(half)     // smem_KV
                   + FA2_BR * FA2_BC * sizeof(float);     // smem_S

    flash_attention_2_kernel<<<grid, block, smem_bytes, (cudaStream_t)stream>>>(
        (const half*)Q, (const half*)K, (const half*)V, (half*)O,
        seq_len, total_len, num_heads, num_kv_heads, head_dim,
        scale, causal
    );
}

// ============================================================================
// Fused decode attention (seq_len=1)
//
// Optimized for next-token generation. Each warp independently processes a
// stream of K/V positions — NO __syncthreads in the hot loop.
//
// Thread layout (dynamic, adapts to head_dim):
//   Block size = head_dim threads (must be a multiple of 32)
//   nwarps = head_dim / 32, dpt = head_dim / 32 (dims per thread)
//   Each thread handles dpt dimensions: dims [lane*dpt .. lane*dpt+dpt-1]
//   Warp w processes positions w, w+nwarps, w+2*nwarps, ... (interleaved)
//
// Examples: head_dim=128 → 4 warps, 4 dpt. head_dim=160 → 5 warps, 5 dpt.
//
// Each warp computes its own partial online-softmax result. After the loop,
// warps combine via shared memory using the log-sum-exp trick (1 sync).
//
// Memory: consecutive threads read consecutive K/V elements → coalesced.
// Supports head_dim up to 256 (DA_MAX_DPT=8).
// ============================================================================

// Max dims per thread — supports head_dim up to 512 (512/32 = 16).
// Gemma 4 global attention layers use head_dim=512.
#define DA_MAX_DPT 16

static __global__ void decode_fused_attention_kernel(
    const half* __restrict__ Q,   // [num_heads, head_dim]
    const half* __restrict__ K,   // [total_len, num_kv_heads, head_dim]
    const half* __restrict__ V,   // [total_len, num_kv_heads, head_dim]
    half* __restrict__ O,         // [num_heads, head_dim]
    int num_heads,
    int num_kv_heads,
    int head_dim,
    int total_len,
    float scale
) {
    const int head = blockIdx.x;
    const int tid = threadIdx.x;
    const int warp_id = tid / 32;
    const int lane = tid & 31;

    // Compute layout from head_dim (block size = head_dim)
    const int dpt = head_dim / 32;     // dims per thread
    const int nwarps = head_dim / 32;  // warps per block

    const int gqa_groups = num_heads / num_kv_heads;
    const int kv_head = head / gqa_groups;
    const int kv_stride = num_kv_heads * head_dim;

    // Each thread loads its dpt Q dimensions
    float q[DA_MAX_DPT];
    for (int i = 0; i < dpt; i++) {
        q[i] = __half2float(Q[head * head_dim + lane * dpt + i]);
    }

    // Per-warp online softmax state (registers)
    float m = -1e30f;
    float l = 0.0f;
    float acc[DA_MAX_DPT];
    for (int i = 0; i < dpt; i++) acc[i] = 0.0f;

    // Hot loop: each warp processes its own position stream
    for (int pos = warp_id; pos < total_len; pos += nwarps) {
        long long base = (long long)pos * kv_stride + kv_head * head_dim
                       + lane * dpt;

        // Dot product: each thread multiplies its dpt dims, warp reduces
        float dot = 0.0f;
        for (int i = 0; i < dpt; i++) {
            dot += q[i] * __half2float(K[base + i]);
        }
        // Warp-level reduction (shuffle only — no shared mem, no sync!)
        #pragma unroll
        for (int offset = 16; offset > 0; offset >>= 1) {
            dot += __shfl_xor_sync(0xFFFFFFFF, dot, offset);
        }
        float score = dot * scale;  // all lanes have the score

        // Online softmax + V accumulation
        float new_m = fmaxf(m, score);
        float correction = expf(m - new_m);
        float p = expf(score - new_m);

        for (int i = 0; i < dpt; i++) {
            acc[i] = acc[i] * correction + p * __half2float(V[base + i]);
        }
        l = l * correction + p;
        m = new_m;
    }

    // === Combine warps' partial results (log-sum-exp trick) ===
    // Dynamic shared memory layout: [nwarps floats m][nwarps floats l][nwarps*head_dim floats acc]
    extern __shared__ char da_smem_raw[];
    float* smem_m   = (float*)da_smem_raw;
    float* smem_l   = smem_m + nwarps;
    float* smem_acc = smem_l + nwarps;

    if (lane == 0) {
        smem_m[warp_id] = m;
        smem_l[warp_id] = l;
    }
    for (int i = 0; i < dpt; i++) {
        smem_acc[warp_id * head_dim + lane * dpt + i] = acc[i];
    }
    __syncthreads();

    // Find global max across warps (all threads compute, it's cheap)
    float global_m = smem_m[0];
    for (int w = 1; w < nwarps; w++) {
        global_m = fmaxf(global_m, smem_m[w]);
    }

    // Combine each warp's contribution for this thread's dimensions
    float final_l = 0.0f;
    float final_acc[DA_MAX_DPT];
    for (int i = 0; i < dpt; i++) final_acc[i] = 0.0f;

    for (int w = 0; w < nwarps; w++) {
        float corr = expf(smem_m[w] - global_m);
        final_l += smem_l[w] * corr;
        for (int i = 0; i < dpt; i++) {
            final_acc[i] += smem_acc[w * head_dim + lane * dpt + i] * corr;
        }
    }

    // Write output (coalesced: consecutive threads write consecutive dims)
    float inv_l = 1.0f / final_l;
    for (int i = 0; i < dpt; i++) {
        O[head * head_dim + lane * dpt + i] =
            __float2half(final_acc[i] * inv_l);
    }
}

extern "C" void launch_decode_attention(
    const void* Q,
    const void* K,
    const void* V,
    void* O,
    int num_heads,
    int num_kv_heads,
    int head_dim,
    int total_len,
    float scale,
    void* stream
) {
    cudaStream_t s = (cudaStream_t)stream;
    int nwarps = head_dim / 32;
    // Shared: nwarps floats (m) + nwarps floats (l) + nwarps*head_dim floats (acc)
    int smem_bytes = (nwarps * 2 + nwarps * head_dim) * sizeof(float);

    decode_fused_attention_kernel<<<num_heads, head_dim, smem_bytes, s>>>(
        (const half*)Q, (const half*)K, (const half*)V, (half*)O,
        num_heads, num_kv_heads, head_dim, total_len, scale
    );
}

// ============================================================================
// P2P allreduce sum (2-GPU NVLink)
// ============================================================================

static __global__ void allreduce_kernel(
    half* __restrict__ local_buf,
    const half* __restrict__ peer_buf,
    int size
) {
    int idx = blockIdx.x * blockDim.x + threadIdx.x;
    if (idx >= size) return;
    float val = __half2float(local_buf[idx]) + __half2float(peer_buf[idx]);
    local_buf[idx] = __float2half(val);
}

extern "C" void launch_allreduce_2gpu(
    void* local_buf,
    const void* peer_buf,
    int num_elements,
    void* stream
) {
    int blocks = (num_elements + BLOCK_SIZE - 1) / BLOCK_SIZE;
    allreduce_kernel<<<blocks, BLOCK_SIZE, 0, (cudaStream_t)stream>>>(
        (half*)local_buf, (const half*)peer_buf, num_elements
    );
}

// ============================================================================
// Zero buffer: output[i] = 0 for all elements
// Used to clear MoE accumulator before expert dispatch.
// ============================================================================

static __global__ void zero_kernel(half* __restrict__ output, int total_elements) {
    int idx = blockIdx.x * blockDim.x + threadIdx.x;
    if (idx >= total_elements) return;
    output[idx] = __float2half(0.0f);
}

extern "C" void launch_zero(
    void* output,
    int total_elements,
    void* stream
) {
    int blocks = (total_elements + BLOCK_SIZE - 1) / BLOCK_SIZE;
    zero_kernel<<<blocks, BLOCK_SIZE, 0, (cudaStream_t)stream>>>(
        (half*)output, total_elements
    );
}

// ============================================================================
// Fused scale-add: output[i] += scale * input[i]
// Used to accumulate weighted expert outputs in MoE dispatch.
// Computes in f32 internally for numerical stability.
// ============================================================================

static __global__ void scale_add_kernel(
    half* __restrict__ output,
    const half* __restrict__ input,
    float scale,
    int total_elements
) {
    int idx = blockIdx.x * blockDim.x + threadIdx.x;
    if (idx >= total_elements) return;
    float val = __half2float(output[idx]) + scale * __half2float(input[idx]);
    output[idx] = __float2half(val);
}

extern "C" void launch_scale_add(
    void* output,
    const void* input,
    float scale,
    int total_elements,
    void* stream
) {
    int blocks = (total_elements + BLOCK_SIZE - 1) / BLOCK_SIZE;
    scale_add_kernel<<<blocks, BLOCK_SIZE, 0, (cudaStream_t)stream>>>(
        (half*)output, (const half*)input, scale, total_elements
    );
}

// ============================================================================
// Gather rows: dst[i, :] = src[indices[i], :] for i in 0..num_rows
// Used to batch tokens assigned to the same expert into a contiguous buffer
// for efficient GEMM during MoE prefill.
// ============================================================================

static __global__ void gather_rows_kernel(
    const half* __restrict__ src,
    half* __restrict__ dst,
    const int* __restrict__ indices,
    int num_rows,
    int row_dim
) {
    int row = blockIdx.x;
    if (row >= num_rows) return;
    int src_row = indices[row];
    int tid = threadIdx.x;
    for (int d = tid; d < row_dim; d += blockDim.x) {
        dst[(long long)row * row_dim + d] = src[(long long)src_row * row_dim + d];
    }
}

extern "C" void launch_gather_rows(
    const void* src,
    void* dst,
    const int* indices,
    int num_rows,
    int row_dim,
    void* stream
) {
    if (num_rows == 0) return;
    gather_rows_kernel<<<num_rows, BLOCK_SIZE, 0, (cudaStream_t)stream>>>(
        (const half*)src, (half*)dst, indices, num_rows, row_dim
    );
}

// ============================================================================
// Weighted scatter-add: dst[indices[i], :] += weights[i] * src[i, :]
// Used to accumulate weighted expert outputs back to their token positions
// in mlp_out during MoE prefill. Each expert's scatter runs sequentially
// on the same stream, so no write races between experts.
// ============================================================================

static __global__ void weighted_scatter_add_kernel(
    half* __restrict__ dst,
    const half* __restrict__ src,
    const int* __restrict__ indices,
    const float* __restrict__ weights,
    int num_rows,
    int row_dim
) {
    int row = blockIdx.x;
    if (row >= num_rows) return;
    int dst_row = indices[row];
    float w = weights[row];
    int tid = threadIdx.x;
    for (int d = tid; d < row_dim; d += blockDim.x) {
        float val = __half2float(dst[(long long)dst_row * row_dim + d])
                  + w * __half2float(src[(long long)row * row_dim + d]);
        dst[(long long)dst_row * row_dim + d] = __float2half(val);
    }
}

extern "C" void launch_weighted_scatter_add(
    void* dst,
    const void* src,
    const int* indices,
    const float* weights,
    int num_rows,
    int row_dim,
    void* stream
) {
    if (num_rows == 0) return;
    weighted_scatter_add_kernel<<<num_rows, BLOCK_SIZE, 0, (cudaStream_t)stream>>>(
        (half*)dst, (const half*)src, indices, weights, num_rows, row_dim
    );
}

// ============================================================================
// GeGLU: output = gelu_tanh(gate) * up   (Gemma 4 activation)
//
// GELU with tanh approximation:
//   gelu(x) = 0.5 * x * (1 + tanh(sqrt(2/pi) * (x + 0.044715 * x^3)))
// Combined with gated unit: output = gelu(gate) * up
// ============================================================================

static __global__ void geglu_kernel(
    const half* __restrict__ gate,
    const half* __restrict__ up,
    half* __restrict__ output,
    int total_elements
) {
    int idx = blockIdx.x * blockDim.x + threadIdx.x;
    if (idx >= total_elements) return;

    float g = __half2float(gate[idx]);
    float u = __half2float(up[idx]);

    // GELU tanh approximation: 0.5 * g * (1 + tanh(sqrt(2/pi) * (g + 0.044715 * g^3)))
    const float SQRT_2_OVER_PI = 0.7978845608f;  // sqrt(2.0 / pi)
    float inner = SQRT_2_OVER_PI * (g + 0.044715f * g * g * g);
    float gelu_g = 0.5f * g * (1.0f + tanhf(inner));

    output[idx] = __float2half(gelu_g * u);
}

extern "C" void launch_geglu(
    const void* gate,
    const void* up,
    void* output,
    int total_elements,
    void* stream
) {
    int blocks = (total_elements + BLOCK_SIZE - 1) / BLOCK_SIZE;
    geglu_kernel<<<blocks, BLOCK_SIZE, 0, (cudaStream_t)stream>>>(
        (const half*)gate, (const half*)up, (half*)output, total_elements
    );
}

// ============================================================================
// In-place scale: data[i] *= scale
//
// Used for:
//   - Embedding scaling (sqrt(hidden_size)) after lookup
//   - Per-layer scalar multiplication
// ============================================================================

static __global__ void scale_inplace_kernel(
    half* __restrict__ data,
    float scale,
    int total_elements
) {
    int idx = blockIdx.x * blockDim.x + threadIdx.x;
    if (idx >= total_elements) return;
    data[idx] = __float2half(__half2float(data[idx]) * scale);
}

extern "C" void launch_scale_inplace(
    void* data,
    float scale,
    int total_elements,
    void* stream
) {
    int blocks = (total_elements + BLOCK_SIZE - 1) / BLOCK_SIZE;
    scale_inplace_kernel<<<blocks, BLOCK_SIZE, 0, (cudaStream_t)stream>>>(
        (half*)data, scale, total_elements
    );
}

// ============================================================================
// Logit soft-cap: logits[i] = cap * tanh(logits[i] / cap)
//
// Clamps logits to [-cap, +cap] range. Gemma 4 uses cap=30.0.
// ============================================================================

static __global__ void logit_softcap_kernel(
    half* __restrict__ logits,
    float cap,
    int total_elements
) {
    int idx = blockIdx.x * blockDim.x + threadIdx.x;
    if (idx >= total_elements) return;
    float val = __half2float(logits[idx]);
    logits[idx] = __float2half(cap * tanhf(val / cap));
}

extern "C" void launch_logit_softcap(
    void* logits,
    float cap,
    int total_elements,
    void* stream
) {
    int blocks = (total_elements + BLOCK_SIZE - 1) / BLOCK_SIZE;
    logit_softcap_kernel<<<blocks, BLOCK_SIZE, 0, (cudaStream_t)stream>>>(
        (half*)logits, cap, total_elements
    );
}

// ============================================================================
// Scale-free RMSNorm (no learnable weight):
//   output = input * rsqrt(mean(input^2) + eps)
//
// Used for Gemma 4 V-norm (value states) and router input normalization.
// Same reduction as rmsnorm_kernel, but without the weight multiply.
// ============================================================================

static __global__ void rmsnorm_noscale_kernel(
    const half* __restrict__ input,
    half* __restrict__ output,
    int dim,
    float eps
) {
    int row = blockIdx.x;
    int tid = threadIdx.x;
    int stride = blockDim.x;

    const half* x = input + (long long)row * dim;
    half* out = output + (long long)row * dim;

    extern __shared__ float sdata[];

    float local_sum = 0.0f;
    for (int i = tid; i < dim; i += stride) {
        float val = __half2float(x[i]);
        local_sum += val * val;
    }
    sdata[tid] = local_sum;
    __syncthreads();

    for (int s = stride / 2; s > 0; s >>= 1) {
        if (tid < s) {
            sdata[tid] += sdata[tid + s];
        }
        __syncthreads();
    }

    float rms = rsqrtf(sdata[0] / (float)dim + eps);

    for (int i = tid; i < dim; i += stride) {
        out[i] = __float2half(__half2float(x[i]) * rms);
    }
}

extern "C" void launch_rmsnorm_noscale(
    const void* input,
    void* output,
    int batch_size,
    int dim,
    float eps,
    void* stream
) {
    int threads = (dim < BLOCK_SIZE) ? dim : BLOCK_SIZE;
    int smem = threads * sizeof(float);
    rmsnorm_noscale_kernel<<<batch_size, threads, smem, (cudaStream_t)stream>>>(
        (const half*)input, (half*)output, dim, eps
    );
}

// ============================================================================
// Partial RoPE: Apply rotary embeddings to only the first rotary_dim
// dimensions of each head, leaving the remaining dimensions unchanged.
//
// Gemma 4 global attention layers use partial_rotary_factor=0.25, meaning
// only 128 out of 512 head dimensions get rotary encoding.
//
// Uses the same rotary-half convention as launch_rope:
//   pairs are (d[i], d[i + half_rotary]) for i in 0..half_rotary
// ============================================================================

static __global__ void rope_partial_kernel(
    half* __restrict__ qk,      // [seq_len, num_heads, head_dim]
    int num_heads,
    int head_dim,
    int rotary_dim,             // first rotary_dim dims get RoPE
    int pos_offset,
    float theta_base
) {
    int seq_idx = blockIdx.x;
    int head = blockIdx.y;
    int tid = threadIdx.x;
    int half_rotary = rotary_dim / 2;

    if (tid >= half_rotary || head >= num_heads) return;

    int pos = seq_idx + pos_offset;
    float freq = 1.0f / powf(theta_base, (float)(2 * tid) / (float)rotary_dim);
    float angle = (float)pos * freq;
    float cos_val = cosf(angle);
    float sin_val = sinf(angle);

    long long base_idx = ((long long)seq_idx * num_heads + head) * head_dim;
    float v0 = __half2float(qk[base_idx + tid]);
    float v1 = __half2float(qk[base_idx + tid + half_rotary]);
    qk[base_idx + tid]               = __float2half(v0 * cos_val - v1 * sin_val);
    qk[base_idx + tid + half_rotary] = __float2half(v0 * sin_val + v1 * cos_val);
    // Dimensions [rotary_dim, head_dim) are untouched
}

extern "C" void launch_rope_partial(
    void* q,
    void* k,
    int seq_len,
    int num_heads,
    int num_kv_heads,
    int head_dim,
    int rotary_dim,
    int pos_offset,
    float theta_base,
    void* stream
) {
    int threads = rotary_dim / 2;
    if (threads > BLOCK_SIZE) threads = BLOCK_SIZE;
    dim3 grid_q(seq_len, num_heads);
    dim3 grid_k(seq_len, num_kv_heads);

    rope_partial_kernel<<<grid_q, threads, 0, (cudaStream_t)stream>>>(
        (half*)q, num_heads, head_dim, rotary_dim, pos_offset, theta_base
    );
    rope_partial_kernel<<<grid_k, threads, 0, (cudaStream_t)stream>>>(
        (half*)k, num_kv_heads, head_dim, rotary_dim, pos_offset, theta_base
    );
}
