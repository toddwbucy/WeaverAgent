//! The native pair forward: qwen2 sharded across two devices, per
//! `weaver-spu-Spec` section 4.1 and the two-device half of section 11's
//! width election.
//!
//! **The scheme is the salvage and the code is not.** The archived tree's
//! `forward_tp2` settled the shape this module carries forward: QKV and
//! gate-up projections column-parallel, O and down projections row-parallel,
//! a reduce-and-broadcast between, two reductions per layer. What does not
//! cross is the implementation - the raw-kernel path served no readout and
//! carried its own model beside candle's, and the survey's rule is that
//! nothing crosses because the quarry has it. The forward here is candle's
//! ops over candle's tensors, which is what keeps the readout election
//! reachable on the pair the day its act arrives.
//!
//! **The reduction is host-staged and says so.** Each device holds a partial
//! after a row-parallel projection, the partials sum through a device-hop,
//! and the sum returns to both devices. A peer-to-peer reduction over the
//! same buffers is the optimization the cudarc caret-pin exists for, the
//! Spec naming that the pin unifies this crate's device handles with
//! candle's, and it enters when the seam's cost is measured against real
//! traffic rather than presumed. Correctness first, the corpus's own order.
//!
//! **Norms, embeddings, and the head are replicated, not sharded.** Both
//! devices hold the small weights and compute norms on their own copy of the
//! hidden state, which costs kilobytes of memory to save one broadcast per
//! norm. The two copies are the same computation over the same bytes, so
//! divergence is not expressible.

use candle_core::{D, DType, Device, IndexOp, Tensor};
use candle_nn::ops::softmax_last_dim;
use candle_transformers::models::qwen2::Config;

use super::backend::DecodeFault;
use crate::residency::AdmitRefusal;

/// One device's shard of one decoder layer.
#[derive(Clone)]
struct LayerShard {
    q: Tensor,
    q_bias: Tensor,
    k: Tensor,
    k_bias: Tensor,
    v: Tensor,
    v_bias: Tensor,
    o: Tensor,
    gate: Tensor,
    up: Tensor,
    down: Tensor,
    input_norm: Tensor,
    post_norm: Tensor,
    kv_cache: Option<(Tensor, Tensor)>,
}

/// The model sharded across exactly two devices.
///
/// `Clone` is the session discipline the single-device path set: the
/// resident's copy stays pristine and an engine decodes against a clone, the
/// weight tensors sharing storage underneath. **A derived clone carries the
/// source's caches with it**, so the session door is [`ShardedModel::session_clone`],
/// which clears them: a session opened over a residency that has computed
/// would otherwise inherit positions its own account never admitted.
#[derive(Clone)]
pub struct ShardedModel {
    devices: [Device; 2],
    embed: Tensor,
    layers: Vec<[LayerShard; 2]>,
    final_norm: Tensor,
    lm_head: Tensor,
    cos: [Tensor; 2],
    sin: [Tensor; 2],
    heads_per_device: usize,
    kv_heads_per_device: usize,
    head_dim: usize,
    eps: f64,
}

/// The inverse frequencies used by the pair's rotary table construction.
pub(super) fn rotary_inverse_frequencies(head_dim: usize, theta: f64) -> Vec<f32> {
    (0..head_dim)
        .step_by(2)
        .map(|i| 1f32 / theta.powf(i as f64 / head_dim as f64) as f32)
        .collect()
}

/// The pair's replicated rotary tables. Keep the angle in fp32 and cast
/// each trigonometric result to BF16 before transferring it to the device.
/// This is the construction consumed by `ShardedModel::load` and `attend`.
pub(super) fn rotary_tables(
    head_dim: usize,
    rope_theta: f64,
    max: usize,
    cpu: &Device,
    device: &Device,
) -> candle_core::Result<(Tensor, Tensor)> {
    let inv = rotary_inverse_frequencies(head_dim, rope_theta);
    let inv_len = inv.len();
    let inv = Tensor::from_vec(inv, (1, inv_len), cpu)?;
    let t = Tensor::arange(0u32, max as u32, cpu)?
        .to_dtype(DType::F32)?
        .reshape((max, 1))?;
    let freqs = t.matmul(&inv)?;
    let cos = freqs.cos()?.to_dtype(DType::BF16)?.to_device(device)?;
    let sin = freqs.sin()?.to_dtype(DType::BF16)?.to_device(device)?;
    Ok((cos, sin))
}

impl ShardedModel {
    /// Load the artifact's weights, each device receiving its shard and the
    /// replicated smalls. The slice happens on the mmap'd host side, so only
    /// a shard's bytes ever reach a device.
    pub fn load(
        containers: &[std::path::PathBuf],
        config: &Config,
        devices: [Device; 2],
    ) -> Result<ShardedModel, AdmitRefusal> {
        let fail = |detail: String| AdmitRefusal::LoadFailed { detail };
        if !config.num_attention_heads.is_multiple_of(2)
            || !config.num_key_value_heads.is_multiple_of(2)
        {
            return Err(fail(format!(
                "the head counts do not halve: {} attention, {} key-value",
                config.num_attention_heads, config.num_key_value_heads
            )));
        }
        if !config.intermediate_size.is_multiple_of(2) {
            return Err(fail(format!(
                "the intermediate width does not halve: {}",
                config.intermediate_size
            )));
        }
        let cpu = Device::Cpu;
        let vb = unsafe {
            candle_nn::VarBuilder::from_mmaped_safetensors(containers, DType::BF16, &cpu)
        }
        .map_err(|e| fail(format!("safetensors map: {e}")))?;
        let vm = vb.pp("model");

        let head_dim = config.hidden_size / config.num_attention_heads;
        let heads_per_device = config.num_attention_heads / 2;
        let kv_heads_per_device = config.num_key_value_heads / 2;
        let q_rows = heads_per_device * head_dim;
        let kv_rows = kv_heads_per_device * head_dim;
        let mlp_rows = config.intermediate_size / 2;

        let get = |name: &str, shape: (usize, usize)| -> Result<Tensor, AdmitRefusal> {
            vm.get(shape, name)
                .map_err(|e| fail(format!("{name}: {e}")))
        };
        let get1 = |name: &str, len: usize| -> Result<Tensor, AdmitRefusal> {
            vm.get(len, name).map_err(|e| fail(format!("{name}: {e}")))
        };
        // **Both slices become owned copies at load, and `contiguous` is
        // not enough.** A column shard narrows dimension zero, which is
        // already contiguous, so `contiguous()` answers the same view over
        // the whole backing buffer - and the device transfer ships the
        // backing buffer, not the view. Measured on the 32B: both cards
        // filled to the whole model's size and the load died out of memory
        // at sixty-two gigabytes a side. `copy()` allocates exactly the
        // shard, which is also what the device matmul needs, the transposed
        // view of a strided tensor being refused there.
        let column = |t: &Tensor, half: usize, rows: usize| -> Result<Tensor, AdmitRefusal> {
            t.narrow(0, half * rows, rows)
                .and_then(|t| t.force_contiguous())
                .map_err(|e| fail(format!("column shard: {e}")))
        };
        let row = |t: &Tensor, half: usize, cols: usize| -> Result<Tensor, AdmitRefusal> {
            t.narrow(1, half * cols, cols)
                .and_then(|t| t.force_contiguous())
                .map_err(|e| fail(format!("row shard: {e}")))
        };
        let onto = |t: Tensor, device: &Device| -> Result<Tensor, AdmitRefusal> {
            t.to_device(device)
                .map_err(|e| fail(format!("to device: {e}")))
        };

        let embed = vm
            .get(
                (config.vocab_size, config.hidden_size),
                "embed_tokens.weight",
            )
            .map_err(|e| fail(format!("embed_tokens: {e}")))?;
        let final_norm = get1("norm.weight", config.hidden_size)?;
        // qwen2's small artifacts tie the head to the embedding, and the
        // untied case reads its own tensor from the root namespace.
        let lm_head = if config.tie_word_embeddings {
            embed.clone()
        } else {
            vb.get((config.vocab_size, config.hidden_size), "lm_head.weight")
                .map_err(|e| fail(format!("lm_head: {e}")))?
        };

        let mut layers = Vec::with_capacity(config.num_hidden_layers);
        for index in 0..config.num_hidden_layers {
            let p = format!("layers.{index}");
            let q_w = get(
                &format!("{p}.self_attn.q_proj.weight"),
                (config.num_attention_heads * head_dim, config.hidden_size),
            )?;
            let q_b = get1(
                &format!("{p}.self_attn.q_proj.bias"),
                config.num_attention_heads * head_dim,
            )?;
            let k_w = get(
                &format!("{p}.self_attn.k_proj.weight"),
                (config.num_key_value_heads * head_dim, config.hidden_size),
            )?;
            let k_b = get1(
                &format!("{p}.self_attn.k_proj.bias"),
                config.num_key_value_heads * head_dim,
            )?;
            let v_w = get(
                &format!("{p}.self_attn.v_proj.weight"),
                (config.num_key_value_heads * head_dim, config.hidden_size),
            )?;
            let v_b = get1(
                &format!("{p}.self_attn.v_proj.bias"),
                config.num_key_value_heads * head_dim,
            )?;
            let o_w = get(
                &format!("{p}.self_attn.o_proj.weight"),
                (config.hidden_size, config.num_attention_heads * head_dim),
            )?;
            let gate_w = get(
                &format!("{p}.mlp.gate_proj.weight"),
                (config.intermediate_size, config.hidden_size),
            )?;
            let up_w = get(
                &format!("{p}.mlp.up_proj.weight"),
                (config.intermediate_size, config.hidden_size),
            )?;
            let down_w = get(
                &format!("{p}.mlp.down_proj.weight"),
                (config.hidden_size, config.intermediate_size),
            )?;
            let in_n = get1(&format!("{p}.input_layernorm.weight"), config.hidden_size)?;
            let post_n = get1(
                &format!("{p}.post_attention_layernorm.weight"),
                config.hidden_size,
            )?;

            let mut halves = Vec::with_capacity(2);
            for (half, device) in devices.iter().enumerate() {
                halves.push(LayerShard {
                    q: onto(column(&q_w, half, q_rows)?, device)?,
                    q_bias: onto(column(&q_b, half, q_rows)?, device)?,
                    k: onto(column(&k_w, half, kv_rows)?, device)?,
                    k_bias: onto(column(&k_b, half, kv_rows)?, device)?,
                    v: onto(column(&v_w, half, kv_rows)?, device)?,
                    v_bias: onto(column(&v_b, half, kv_rows)?, device)?,
                    o: onto(row(&o_w, half, q_rows)?, device)?,
                    gate: onto(column(&gate_w, half, mlp_rows)?, device)?,
                    up: onto(column(&up_w, half, mlp_rows)?, device)?,
                    down: onto(row(&down_w, half, mlp_rows)?, device)?,
                    input_norm: onto(in_n.clone(), device)?,
                    post_norm: onto(post_n.clone(), device)?,
                    kv_cache: None,
                });
            }
            let [a, b] = <[LayerShard; 2]>::try_from(halves)
                .unwrap_or_else(|_| unreachable!("two halves were pushed"));
            layers.push([a, b]);
        }

        // The same builder is exercised directly by the rotary precision tests.
        let table = |device: &Device| {
            rotary_tables(
                head_dim,
                config.rope_theta,
                config.max_position_embeddings,
                &cpu,
                device,
            )
            .map_err(|e| fail(format!("rope: {e}")))
        };
        let (cos0, sin0) = table(&devices[0])?;
        let (cos1, sin1) = table(&devices[1])?;

        let embed = onto(embed, &devices[0])?;
        let final_norm = onto(final_norm, &devices[0])?;
        let lm_head = onto(lm_head, &devices[0])?;

        Ok(ShardedModel {
            devices,
            embed,
            layers,
            final_norm,
            lm_head,
            cos: [cos0, cos1],
            sin: [sin0, sin1],
            heads_per_device,
            kv_heads_per_device,
            head_dim,
            eps: config.rms_norm_eps,
        })
    }

    /// One forward over `tokens` at `offset`, answering the last position's
    /// logits as f32 on the host.
    pub fn forward(
        &mut self,
        tokens: &[u32],
        offset: usize,
        mut layer_norms: Option<&mut Vec<f32>>,
    ) -> Result<Vec<f32>, DecodeFault> {
        let fault = |detail: String| DecodeFault::Engine { detail };
        let seq = tokens.len();
        // An empty decode reaches no distribution and the engine returns
        // before asking, so this door refuses rather than underflowing the
        // last-position index for a caller the engine does not mediate.
        if seq == 0 {
            return Err(fault("an empty decode has no distribution".into()));
        }
        let ids = Tensor::new(tokens, &self.devices[0]).map_err(|e| fault(format!("ids: {e}")))?;
        let hidden0 = self
            .embed
            .index_select(&ids, 0)
            .map_err(|e| fault(format!("embed: {e}")))?;
        // Both devices walk the layers holding their own copy of the hidden
        // state, per the module header.
        let mut hidden = [
            hidden0.clone(),
            hop(&hidden0, &self.devices[1]).map_err(|e| fault(format!("broadcast: {e}")))?,
        ];

        // **The mask spans the keys, not the queries alone.** After the
        // cache concat the scores run over `offset + seq` key positions, so
        // a mask shaped by the delta alone would misbroadcast the moment a
        // multi-token delta followed a resident prefix, which is every real
        // turn. Query `i` sees every cached position and its own prefix of
        // the delta. Staged onto each device once, here, rather than hopped
        // from the host per layer per shard.
        let mask: Option<[Tensor; 2]> = if seq > 1 {
            let keys = offset + seq;
            let m: Vec<f32> = (0..seq)
                .flat_map(|i| {
                    (0..keys).map(move |j| {
                        if j > offset + i {
                            f32::NEG_INFINITY
                        } else {
                            0.0
                        }
                    })
                })
                .collect();
            let host = Tensor::from_vec(m, (seq, keys), &Device::Cpu)
                .and_then(|t| t.to_dtype(DType::BF16))
                .map_err(|e| fault(format!("mask: {e}")))?;
            let on0 = host
                .to_device(&self.devices[0])
                .map_err(|e| fault(format!("mask: {e}")))?;
            let on1 = host
                .to_device(&self.devices[1])
                .map_err(|e| fault(format!("mask: {e}")))?;
            Some([on0, on1])
        } else {
            None
        };

        for layer in self.layers.iter_mut() {
            // Attention half, each device over its heads.
            let mut partials = Vec::with_capacity(2);
            for (half, shard) in layer.iter_mut().enumerate() {
                let partial = attend(
                    shard,
                    &hidden[half],
                    &self.cos[half],
                    &self.sin[half],
                    mask.as_ref().map(|m| &m[half]),
                    offset,
                    self.heads_per_device,
                    self.kv_heads_per_device,
                    self.head_dim,
                    self.eps,
                )
                .map_err(|e| fault(format!("attention: {e}")))?;
                partials.push(partial);
            }
            let reduced =
                allreduce(partials, &self.devices).map_err(|e| fault(format!("reduce: {e}")))?;
            for half in 0..2 {
                hidden[half] = (&hidden[half] + &reduced[half])
                    .map_err(|e| fault(format!("residual: {e}")))?;
            }

            // MLP half, same shape.
            let mut partials = Vec::with_capacity(2);
            for (half, shard) in layer.iter_mut().enumerate() {
                let normed = rms_norm(&hidden[half], &shard.post_norm, self.eps)
                    .map_err(|e| fault(format!("norm: {e}")))?;
                let gate = normed
                    .broadcast_matmul(&shard.gate.t().map_err(|e| fault(format!("t: {e}")))?)
                    .map_err(|e| fault(format!("gate: {e}")))?;
                let up = normed
                    .broadcast_matmul(&shard.up.t().map_err(|e| fault(format!("t: {e}")))?)
                    .map_err(|e| fault(format!("up: {e}")))?;
                let act = (candle_nn::ops::silu(&gate).map_err(|e| fault(format!("silu: {e}")))?
                    * up)
                    .map_err(|e| fault(format!("swiglu: {e}")))?;
                let partial = act
                    .broadcast_matmul(&shard.down.t().map_err(|e| fault(format!("t: {e}")))?)
                    .map_err(|e| fault(format!("down: {e}")))?;
                partials.push(partial);
            }
            let reduced =
                allreduce(partials, &self.devices).map_err(|e| fault(format!("reduce: {e}")))?;
            for half in 0..2 {
                hidden[half] = (&hidden[half] + &reduced[half])
                    .map_err(|e| fault(format!("residual: {e}")))?;
            }

            // The pair's tap: both devices hold the whole residual, so the
            // layer's figure is one device-side norm of the first copy and
            // one scalar crossing, per `weaver-spu-Spec` section 7.
            if let Some(norms) = layer_norms.as_deref_mut() {
                let figure = super::native::layer_norm_figure(&hidden[0])
                    .map_err(|e| fault(format!("tap: {e}")))?;
                norms.push(figure);
            }
        }

        let last = hidden[0]
            .i((seq - 1, ..))
            .map_err(|e| fault(format!("last: {e}")))?;
        let normed = rms_norm(&last, &self.final_norm, self.eps)
            .map_err(|e| fault(format!("final norm: {e}")))?;
        let logits = normed
            .unsqueeze(0)
            .and_then(|t| t.broadcast_matmul(&self.lm_head.t()?))
            .and_then(|t| t.squeeze(0))
            .and_then(|t| t.to_dtype(DType::F32))
            .and_then(|t| t.to_vec1::<f32>())
            .map_err(|e| fault(format!("head: {e}")))?;
        Ok(logits)
    }

    /// The session's copy: shared weights, empty caches.
    pub fn session_clone(&self) -> ShardedModel {
        let mut clone = self.clone();
        clone.clear_kv_cache();
        clone
    }

    pub fn clear_kv_cache(&mut self) {
        for layer in self.layers.iter_mut() {
            for shard in layer.iter_mut() {
                shard.kv_cache = None;
            }
        }
    }
}

/// RMS norm in fp32 through the variance, cast back, which is the upstream
/// model's own discipline.
fn rms_norm(x: &Tensor, weight: &Tensor, eps: f64) -> candle_core::Result<Tensor> {
    let dtype = x.dtype();
    let x32 = x.to_dtype(DType::F32)?;
    let variance = x32.sqr()?.mean_keepdim(D::Minus1)?;
    let normed = x32.broadcast_div(&(variance + eps)?.sqrt()?)?;
    normed.to_dtype(dtype)?.broadcast_mul(weight)
}

/// One device's attention over its heads: norm, project, rope, attend against
/// the device's own cache, project back through its O shard. Answers the
/// row-parallel partial.
#[allow(clippy::too_many_arguments)]
fn attend(
    shard: &mut LayerShard,
    hidden: &Tensor,
    cos: &Tensor,
    sin: &Tensor,
    mask: Option<&Tensor>,
    offset: usize,
    heads: usize,
    kv_heads: usize,
    head_dim: usize,
    eps: f64,
) -> candle_core::Result<Tensor> {
    let (seq, _hidden_size) = hidden.dims2()?;
    let normed = rms_norm(hidden, &shard.input_norm, eps)?;

    let q = normed
        .broadcast_matmul(&shard.q.t()?)?
        .broadcast_add(&shard.q_bias)?;
    let k = normed
        .broadcast_matmul(&shard.k.t()?)?
        .broadcast_add(&shard.k_bias)?;
    let v = normed
        .broadcast_matmul(&shard.v.t()?)?
        .broadcast_add(&shard.v_bias)?;

    // [seq, heads, dim] -> [heads, seq, dim]
    let q = q.reshape((seq, heads, head_dim))?.transpose(0, 1)?;
    let k = k.reshape((seq, kv_heads, head_dim))?.transpose(0, 1)?;
    let v = v.reshape((seq, kv_heads, head_dim))?.transpose(0, 1)?;

    let cos_slice = cos.narrow(0, offset, seq)?;
    let sin_slice = sin.narrow(0, offset, seq)?;
    let q = rope(&q.contiguous()?, &cos_slice, &sin_slice)?;
    let k = rope(&k.contiguous()?, &cos_slice, &sin_slice)?;

    let (k, v) = match shard.kv_cache.take() {
        None => (k, v),
        Some((pk, pv)) => (
            Tensor::cat(&[&pk, &k], 1)?.contiguous()?,
            Tensor::cat(&[&pv, &v], 1)?.contiguous()?,
        ),
    };
    shard.kv_cache = Some((k.clone(), v.clone()));

    // GQA: each query-head group reads its key-value head.
    let group = heads / kv_heads;
    let k = repeat_kv(&k, group)?;
    let v = repeat_kv(&v, group)?;

    let scale = 1f64 / (head_dim as f64).sqrt();
    let scores = (q.matmul(&k.transpose(1, 2)?.contiguous()?)? * scale)?;
    let scores = match mask {
        Some(mask) => scores.broadcast_add(&mask.unsqueeze(0)?)?,
        None => scores,
    };
    let weights = softmax_last_dim(&scores.to_dtype(DType::F32)?)?.to_dtype(scores.dtype())?;
    let context = weights.matmul(&v.contiguous()?)?;

    // [heads, seq, dim] -> [seq, heads*dim] -> through this device's O shard.
    let context = context.transpose(0, 1)?.reshape((seq, heads * head_dim))?;
    context.broadcast_matmul(&shard.o.t()?)
}

/// The half-split rotary the qwen family uses. The upstream op wants a
/// batch rank this seam does not carry, so the batch of one is worn for the
/// call and shed after.
fn rope(x: &Tensor, cos: &Tensor, sin: &Tensor) -> candle_core::Result<Tensor> {
    candle_nn::rotary_emb::rope(&x.unsqueeze(0)?, cos, sin)?.squeeze(0)
}

fn repeat_kv(x: &Tensor, times: usize) -> candle_core::Result<Tensor> {
    if times == 1 {
        return Ok(x.clone());
    }
    let (kv_heads, seq, dim) = x.dims3()?;
    x.unsqueeze(1)?
        .expand((kv_heads, times, seq, dim))?
        .reshape((kv_heads * times, seq, dim))?
        .contiguous()
}

/// The reduce and broadcast of the salvage's scheme: partials sum, the sum
/// stands on both devices. Host-staged, per the module header.
///
/// **Both streams are synchronized around each hop, and the ground is a
/// measured race.** The cross-device copy does not order itself against the
/// source device's queued compute, so with both streams busy the copy can
/// read a partial the matmul has not finished writing. The failure was
/// nondeterministic and moved between runs, weights byte-exact and every op
/// exact in isolation, which is the signature that named it. The syncs cost
/// a stall per reduction, and the direct path's measured rejection is
/// recorded at `hop` below.
fn allreduce(partials: Vec<Tensor>, devices: &[Device; 2]) -> candle_core::Result<[Tensor; 2]> {
    let mut it = partials.into_iter();
    let a = it.next().expect("two partials");
    let b = it.next().expect("two partials");
    let b_on_a = hop(&b, &devices[0])?;
    let sum = (&a + &b_on_a)?;
    let sum_on_b = hop(&sum, &devices[1])?;
    Ok([sum, sum_on_b])
}

/// One tensor from one device to the other, explicitly through the host.
///
/// **The direct device-to-device path was entered measured on 2026-08-19
/// and the measurement rejected it.** With the fork's stream fences and
/// the driver's peer plus pool-access grants all standing, the direct
/// copy still corrupted intermittently below even full host
/// synchronization - a pool-backed peer copy defect beneath the driver's
/// own sync - and it paced slower besides: 47 against 76 tokens per
/// second on the hop-dominated 0.5B case, because a hop here moves about
/// two kilobytes and latency dominates, so the interconnect's bandwidth
/// buys nothing. The host staging is two synchronous copies whose
/// ordering is the driver's own contract, measured faster, and boring.
/// A future entry at widths where the activations are large enough to
/// pay reopens this with the fences already landed in the fork.
fn hop(tensor: &Tensor, to: &Device) -> candle_core::Result<Tensor> {
    tensor.device().synchronize()?;
    let staged = tensor.to_device(&Device::Cpu)?;
    staged.to_device(to)
}

#[cfg(test)]
mod tests;
