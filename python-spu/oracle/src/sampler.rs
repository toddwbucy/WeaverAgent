//! The sampler's mirrors, per python-spu-Spec sections 3.1 and 5.
//!
//! The native engine's `sample` needs a loaded model, so these operations run the
//! code it runs rather than calling it: candle's `LogitsProcessor` and
//! `apply_repeat_penalty` at the fork revision `weaver-spu` pins, built from the
//! knobs the way `crates/weaver-spu/src/decoder/native.rs` builds them. Every answer
//! is integers, so no float crosses back through a decimal rendering.
use candle::{Device, Tensor};
use candle_transformers::generation::{LogitsProcessor, Sampling};
use rand::{RngCore, SeedableRng};
use serde_json::{Value, json};

fn u64_of(v: &Value, key: &str) -> Result<u64, String> {
    v[key].as_u64().ok_or_else(|| key.to_string())
}

fn f32_of(v: &Value, key: &str) -> Result<f32, String> {
    serde_json::from_value(v[key].clone()).map_err(|e| format!("{key}: {e}"))
}

/// The first `count` words of `StdRng::seed_from_u64(seed)`, rand 0.9.5's ChaCha12.
pub fn rng(v: &Value) -> Result<Value, String> {
    let mut rng = rand::rngs::StdRng::seed_from_u64(u64_of(v, "seed")?);
    let count = u64_of(v, "count")?;
    Ok(json!(
        (0..count).map(|_| rng.next_u32()).collect::<Vec<u32>>()
    ))
}

/// The whole index vector after `select_nth_unstable_by(k, ..)` under the comparator
/// candle's `sample_topk_topp` passes, descending by `total_cmp`, so the left part,
/// the nth element and the right part are all compared, not only the set.
pub fn select(v: &Value) -> Result<Value, String> {
    let prs: Vec<f32> = serde_json::from_value(v["prs"].clone()).map_err(|e| e.to_string())?;
    let k = u64_of(v, "k")? as usize;
    if k >= prs.len() {
        return Err("k past the vector".into());
    }
    let mut indices = (0..prs.len()).collect::<Vec<_>>();
    indices.select_nth_unstable_by(k, |&i, &j| prs[j].total_cmp(&prs[i]));
    Ok(json!(indices))
}

/// One generation's draws: the sampler built once from the seed, and at each
/// position the repeat penalty over the resident tail's last window, then the draw,
/// the drawn token joining the tail before the next position, as the session
/// appends it.
pub fn generation(v: &Value) -> Result<Value, String> {
    let temperature = f32_of(v, "temperature")?;
    let top_k = u64_of(v, "top_k")? as u32;
    let top_p = f32_of(v, "top_p")?;
    let repetition_penalty = f32_of(v, "repetition_penalty")?;
    let repetition_window = u64_of(v, "repetition_window")? as usize;
    let seed = u64_of(v, "seed")?;
    let mut resident: Vec<u32> =
        serde_json::from_value(v["tail"].clone()).map_err(|e| e.to_string())?;
    let positions: Vec<Vec<f32>> =
        serde_json::from_value(v["logits"].clone()).map_err(|e| e.to_string())?;
    // native.rs, the sampling chain from the effective knobs.
    let sampling = if temperature <= 0.0 {
        Sampling::ArgMax
    } else if top_k == 0 {
        Sampling::TopP {
            p: top_p as f64,
            temperature: temperature as f64,
        }
    } else {
        Sampling::TopKThenTopP {
            k: top_k as usize,
            p: top_p as f64,
            temperature: temperature as f64,
        }
    };
    let mut sampler = LogitsProcessor::from_sampling(seed, sampling);
    let mut draws = Vec::with_capacity(positions.len());
    for logits in positions {
        // native.rs, `sample`: the logits as a host tensor, the penalty over the
        // resident tail, then the draw.
        let mut tensor = Tensor::new(logits.as_slice(), &Device::Cpu).map_err(|e| e.to_string())?;
        if repetition_penalty != 1.0 && repetition_window > 0 {
            let start = resident.len().saturating_sub(repetition_window);
            tensor = candle_transformers::utils::apply_repeat_penalty(
                &tensor,
                repetition_penalty,
                &resident[start..],
            )
            .map_err(|e| e.to_string())?;
        }
        let token = sampler.sample(&tensor).map_err(|e| e.to_string())?;
        draws.push(token);
        resident.push(token);
    }
    Ok(json!(draws))
}

/// The probability vector a draw reads, as f32 bits: the repeat penalty over the
/// tail's last window, then candle's `prs` closure in `LogitsProcessor::sample_f`,
/// `(&logits / temperature)` and `softmax_last_dim` on the host. Bits, so a one-ulp
/// departure fails where a draw would only rarely show it.
pub fn probs(v: &Value) -> Result<Value, String> {
    let temperature = f32_of(v, "temperature")? as f64;
    let repetition_penalty = f32_of(v, "repetition_penalty")?;
    let repetition_window = u64_of(v, "repetition_window")? as usize;
    let resident: Vec<u32> =
        serde_json::from_value(v["tail"].clone()).map_err(|e| e.to_string())?;
    let logits: Vec<f32> =
        serde_json::from_value(v["logits"].clone()).map_err(|e| e.to_string())?;
    let mut tensor = Tensor::new(logits.as_slice(), &Device::Cpu).map_err(|e| e.to_string())?;
    if repetition_penalty != 1.0 && repetition_window > 0 {
        let start = resident.len().saturating_sub(repetition_window);
        tensor = candle_transformers::utils::apply_repeat_penalty(
            &tensor,
            repetition_penalty,
            &resident[start..],
        )
        .map_err(|e| e.to_string())?;
    }
    let scaled = (&tensor / temperature).map_err(|e| e.to_string())?;
    let prs: Vec<f32> = candle_nn::ops::softmax_last_dim(&scaled)
        .and_then(|t| t.to_vec1())
        .map_err(|e| e.to_string())?;
    Ok(json!(prs.iter().map(|p| p.to_bits()).collect::<Vec<u32>>()))
}

/// A generator that hands out the words it was given, so a draw can be placed on a
/// cumulative weight's boundary on purpose.
struct Scripted(std::vec::IntoIter<u32>);

impl RngCore for Scripted {
    fn next_u32(&mut self) -> u32 {
        self.0.next().expect("the script ran out of words")
    }
    fn next_u64(&mut self) -> u64 {
        rand_core_u64(self)
    }
    fn fill_bytes(&mut self, _dst: &mut [u8]) {
        unimplemented!("WeightedIndex over f32 reads words only")
    }
}

fn rand_core_u64(rng: &mut Scripted) -> u64 {
    let low = rng.next_u32() as u64;
    (rng.next_u32() as u64) << 32 | low
}

/// rand's WeightedIndex over f32 weights, sampled once from scripted words: the
/// uniform's construction and the partition point's boundary rule.
pub fn weighted(v: &Value) -> Result<Value, String> {
    use rand::distr::Distribution;
    let weights: Vec<f32> =
        serde_json::from_value(v["weights"].clone()).map_err(|e| e.to_string())?;
    let words: Vec<u32> = serde_json::from_value(v["words"].clone()).map_err(|e| e.to_string())?;
    let distr = rand::distr::weighted::WeightedIndex::new(&weights).map_err(|e| e.to_string())?;
    Ok(json!(distr.sample(&mut Scripted(words.into_iter()))))
}
