//! Room per device for a GGUF artifact, per the device pair amendment PAIR-4.
//!
//! **Each device is judged against its own share, not an even fraction of
//! the file.** The share is computed from every pinned shard's tensor
//! directory and the header's attention keys, with no tensor data read and no
//! device touched, so it is one of the free steps before the room judgment.
//!
//! | Term | Content |
//! | --- | --- |
//! | Layer weights | Every `blk.N.*` tensor of the layers [`Division`] gives the device, from every shard |
//! | Output | The output-class tensors on the last device, and `token_embd.weight` there too where the artifact has no `output.weight`, because the engine duplicates it onto the output device |
//! | KV cache | Per attention layer on the device: positions padded to 256, times KV heads, times the key and value lengths, times two bytes (the engine's default F16 cache) |
//! | Adapter | The adapter tensors whose base tensor the engine places on the device |
//!
//! The headroom term stays the admission's own parameter and is added there.
//!
//! **The table is the pinned engine's, read from its source at the vendored
//! `e21cdc11`.** A tensor's device follows its class in `llama-arch.cpp`:
//! input-class tensors stay in host memory, output-class tensors go to the
//! output layer's device, and repeating-class tensors go to their layer's
//! device. A name this module does not classify is charged to every device,
//! a bound that over-asks and never under-asks.
//!
//! **Sliding-window layers are charged as full-attention layers at the full
//! context**, with the larger of the full and sliding-window head lengths.
//! The engine allocates no more than that, so the charge never under-asks.
//!
//! **Recurrent and hybrid architectures keep the even fraction for now.** The
//! amendment's per-layer table for them (recurrent state, cacheless layers,
//! the per-architecture classification and its refusals) is the second child
//! act of the device pair work, deferred by the operator's Granite-first
//! priority so that the Qwen3.5 and Nemotron-H admissions that pass today do
//! not regress. [`Layout::deferred`] is the one place that says so, and the
//! admission judges such an artifact as it did before this act.

use std::collections::BTreeMap;
use std::path::Path;

use crate::artifact::directory::{self, Directory, Value};
use crate::division::Division;

/// The architectures the pinned engine classifies as recurrent or hybrid, by
/// their GGUF names: `llm_arch_is_recurrent` and `llm_arch_is_hybrid` in
/// `llama-arch.cpp` at the vendored `e21cdc11`. A change of the engine pin
/// re-reads this list.
pub const RECURRENT_OR_HYBRID: [&str; 18] = [
    "mamba",
    "mamba2",
    "rwkv6",
    "rwkv6qwen2",
    "rwkv7",
    "arwkv7",
    "jamba",
    "falcon-h1",
    "plamo2",
    "granitehybrid",
    "lfm2",
    "lfm2moe",
    "nemotron_h",
    "nemotron_h_moe",
    "qwen3next",
    "kimi-linear",
    "qwen35",
    "qwen35moe",
];

/// Input-class tensors (`LLM_TENSOR_LAYER_INPUT`), which the engine keeps in
/// host memory under every split.
const INPUT: [&str; 4] = [
    "token_embd",
    "position_embd",
    "token_types",
    "per_layer_token_embd",
];

/// Output-class tensors without a layer number (`LLM_TENSOR_LAYER_OUTPUT`),
/// which the engine places on the output layer's device. `token_embd_norm`
/// is absent on purpose: the engine names two tensors so, one output-class
/// and one repeating-class, so it falls to the every-device bound.
const OUTPUT: [&str; 11] = [
    "output",
    "output_norm",
    "cls",
    "cls.output",
    "cls.norm",
    "dense_2",
    "dense_3",
    "dec.output_norm",
    "enc.output_norm",
    "altup_proj",
    "altup_unembd_proj",
];

/// Repeating-class tensors without a layer number in their name, which the
/// engine creates once per layer (`TENSOR_DUPLICATED` past the first), so a
/// device holds one copy for each layer it takes.
const PER_LAYER: [&str; 3] = ["rope_freqs", "rope_factors_long", "rope_factors_short"];

/// The engine's default cache element, F16, in bytes.
const CACHE_ELEMENT: u64 = 2;

/// The engine pads the context to a multiple of this many positions
/// (`llama-context.cpp`, `GGML_PAD(cparams.n_ctx, 256)`), and the cache is
/// allocated at the padded size.
const CONTEXT_PAD: u64 = 256;

/// What the room judgment reads from an artifact's directories.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    architecture: String,
    layers: u32,
    /// Each repeating layer's `blk.N.*` bytes, indexed by layer.
    layer_bytes: Vec<u64>,
    /// Output-class bytes for the last device, the duplicated embedding
    /// included where the artifact ties it.
    output_bytes: u64,
    /// Bytes a device holds once for each layer it takes.
    per_layer_bytes: u64,
    /// Bytes charged to every device: names this module does not classify,
    /// and `blk.N.*` tensors past the last repeating layer.
    every_device_bytes: u64,
    /// Host bytes, recorded and charged to no device.
    input_bytes: u64,
    cache: Cache,
}

/// How a layout's cache is charged.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Cache {
    /// Every layer is attention, full or sliding-window.
    Attention {
        /// KV heads per layer.
        kv_heads: Vec<u64>,
        /// The larger of the full and sliding-window key lengths.
        key: u64,
        /// The larger of the full and sliding-window value lengths.
        value: u64,
    },
    /// The architecture is recurrent or hybrid and its per-layer table is the
    /// second child act's.
    Deferred,
}

/// The caches one layer holds, as PAIR-4 classifies it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Caches {
    /// The layer holds an attention (KV) cache.
    pub attention: bool,
    /// The layer holds a recurrent cache.
    pub recurrent: bool,
}

/// One device's share, term by term, without the headroom.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Share {
    /// Layer weights and the per-layer duplicates.
    pub weights: u64,
    /// The output-class tensors, on the last device only.
    pub output: u64,
    /// The every-device bound.
    pub every_device: u64,
    /// The KV cache of the device's attention layers.
    pub cache: u64,
    /// The adapter tensors whose base is on the device.
    pub adapter: u64,
}

impl Share {
    /// The share's bytes, `None` on overflow.
    pub fn total(&self) -> Option<u64> {
        [self.output, self.every_device, self.cache, self.adapter]
            .iter()
            .try_fold(self.weights, |sum, term| sum.checked_add(*term))
    }
}

/// A shard whose directory did not read, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayoutFault {
    /// The shard's index in split order.
    pub shard: usize,
    pub detail: String,
}

/// Where the engine puts one tensor, by its name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Place {
    Host,
    Layer(u32),
    Output,
    PerLayer,
    EveryDevice,
}

fn place(name: &str, layers: u32) -> Place {
    if let Some(rest) = name.strip_prefix("blk.")
        && let Some((number, _)) = rest.split_once('.')
        && let Ok(layer) = number.parse::<u32>()
    {
        return if layer < layers {
            Place::Layer(layer)
        } else {
            Place::EveryDevice
        };
    }
    let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
    if INPUT.contains(&stem) {
        Place::Host
    } else if OUTPUT.contains(&stem) {
        Place::Output
    } else if PER_LAYER.contains(&stem) {
        Place::PerLayer
    } else {
        Place::EveryDevice
    }
}

/// Where the engine places a named tensor under a division.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Placed {
    /// In host memory: an input-class tensor.
    Host,
    /// On the device at this binding index.
    Device(usize),
    /// A name this module does not classify to one device.
    Unclassified,
}

/// The binding index the engine places tensor `name` on under `division`:
/// a layer's tensor on its layer's device, an output-class tensor on the
/// last device.
pub(crate) fn placed(name: &str, division: &Division) -> Placed {
    match place(name, division.layers()) {
        Place::Host => Placed::Host,
        Place::Layer(layer) => division
            .index_of(layer)
            .map_or(Placed::Unclassified, Placed::Device),
        Place::Output => Placed::Device(division.counts().len() - 1),
        Place::PerLayer | Place::EveryDevice => Placed::Unclassified,
    }
}

impl Layout {
    /// Read every shard's directory, in split order, the first shard's
    /// metadata giving the hyperparameters.
    pub fn read<P: AsRef<Path>>(paths: &[P]) -> Result<Layout, LayoutFault> {
        let mut shards = Vec::with_capacity(paths.len());
        for (shard, path) in paths.iter().enumerate() {
            let directory =
                directory::read(path.as_ref()).map_err(|detail| LayoutFault { shard, detail })?;
            shards.push(directory);
        }
        let fault = |detail: String| LayoutFault { shard: 0, detail };
        let first = shards
            .first()
            .ok_or_else(|| fault("no shard to read".into()))?;
        let architecture = match first.values.get("general.architecture") {
            Some(Value::Text(name)) => name.clone(),
            _ => return Err(fault("no general.architecture".into())),
        };
        let key = |suffix: &str| format!("{architecture}.{suffix}");
        let layers = scalar(first, &key("block_count"))
            .map_err(fault)?
            .and_then(|n| u32::try_from(n).ok())
            .filter(|n| *n > 0)
            .ok_or_else(|| fault(format!("no usable {}", key("block_count"))))?;

        let mut tensors = BTreeMap::new();
        for (shard, directory) in shards.iter().enumerate() {
            for (name, tensor) in &directory.tensors {
                let bytes = tensor.bytes().ok_or_else(|| LayoutFault {
                    shard,
                    detail: format!("tensor {name} has no computable size"),
                })?;
                if tensors.insert(name.clone(), bytes).is_some() {
                    return Err(LayoutFault {
                        shard,
                        detail: format!("tensor {name} appears in two shards"),
                    });
                }
            }
        }

        let mut layer_bytes = vec![0u64; layers as usize];
        let (mut output_bytes, mut per_layer_bytes, mut every_device_bytes, mut input_bytes) =
            (0u64, 0u64, 0u64, 0u64);
        let overflow = || fault("tensor bytes overflow".into());
        for (name, bytes) in &tensors {
            let slot = match place(name, layers) {
                Place::Host => &mut input_bytes,
                Place::Layer(layer) => &mut layer_bytes[layer as usize],
                Place::Output => &mut output_bytes,
                Place::PerLayer => &mut per_layer_bytes,
                Place::EveryDevice => &mut every_device_bytes,
            };
            *slot = slot.checked_add(*bytes).ok_or_else(overflow)?;
        }
        // The engine's tied-embedding path: with no `output.weight`, the
        // output projection is `token_embd.weight` duplicated onto the output
        // layer's device.
        if !tensors.contains_key("output.weight")
            && let Some(embedding) = tensors.get("token_embd.weight")
        {
            output_bytes = output_bytes.checked_add(*embedding).ok_or_else(overflow)?;
        }

        let cache = if RECURRENT_OR_HYBRID.contains(&architecture.as_str()) {
            Cache::Deferred
        } else {
            attention(first, &architecture, layers).map_err(fault)?
        };

        Ok(Layout {
            architecture,
            layers,
            layer_bytes,
            output_bytes,
            per_layer_bytes,
            every_device_bytes,
            input_bytes,
            cache,
        })
    }

    /// The artifact's architecture, as its header names it.
    pub fn architecture(&self) -> &str {
        &self.architecture
    }

    /// The repeating layer count, L.
    pub fn layers(&self) -> u32 {
        self.layers
    }

    /// The host bytes of the input layer, which the SPU charges to no device
    /// and the fixture report records.
    pub fn input_bytes(&self) -> u64 {
        self.input_bytes
    }

    /// **Whether this artifact keeps the even fraction**, because its
    /// architecture is recurrent or hybrid and the per-layer table for those
    /// is the device pair work's second child act. Child B replaces this
    /// with the amendment's classification and its refusals.
    pub fn deferred(&self) -> bool {
        self.cache == Cache::Deferred
    }

    /// **The caches each layer holds by this layout's PAIR-4
    /// classification**, which the PAIR-5 observed division predicts its
    /// cache records from. Every layer of an attention layout holds one
    /// attention cache and no recurrent cache: the engine gives every layer
    /// a KV cache record whatever its head count. `None` for a deferred
    /// layout, whose classification is the second child act's.
    pub fn caches(&self) -> Option<Vec<Caches>> {
        match self.cache {
            Cache::Attention { .. } => Some(vec![
                Caches {
                    attention: true,
                    recurrent: false,
                };
                self.layers as usize
            ]),
            Cache::Deferred => None,
        }
    }

    /// Each device's share under `division`, at `positions` context
    /// positions, with the adapter's per-target bytes where the fixture
    /// supplies one. `None` for a deferred layout, a division of another
    /// depth, or an overflow.
    pub fn shares(
        &self,
        division: &Division,
        positions: u32,
        adapter: Option<&BTreeMap<String, u64>>,
    ) -> Option<Vec<Share>> {
        let Cache::Attention {
            kv_heads,
            key,
            value,
        } = &self.cache
        else {
            return None;
        };
        if division.layers() != self.layers {
            return None;
        }
        let width = division.counts().len();
        let last = width - 1;
        let mut shares = vec![Share::default(); width];
        let padded = u64::from(positions).div_ceil(CONTEXT_PAD) * CONTEXT_PAD;
        // The value cache is sized to the largest layer's width when the
        // engine stores it transposed, so every layer is charged that width.
        let widest = kv_heads.iter().copied().max().unwrap_or(0);
        let value_row = widest.checked_mul(*value)?;
        for layer in 0..self.layers {
            let share = &mut shares[division.index_of(layer)?];
            let weights = self.layer_bytes[layer as usize].checked_add(self.per_layer_bytes)?;
            share.weights = share.weights.checked_add(weights)?;
            let key_row = kv_heads[layer as usize].checked_mul(*key)?;
            let cache = key_row
                .checked_add(value_row)?
                .checked_mul(padded)?
                .checked_mul(CACHE_ELEMENT)?;
            share.cache = share.cache.checked_add(cache)?;
        }
        shares[last].output = self.output_bytes;
        for share in shares.iter_mut() {
            share.every_device = self.every_device_bytes;
        }
        for (target, bytes) in adapter.into_iter().flatten() {
            match place(target, self.layers) {
                Place::Host => {}
                Place::Layer(layer) => {
                    let share = &mut shares[division.index_of(layer)?];
                    share.adapter = share.adapter.checked_add(*bytes)?;
                }
                Place::Output => {
                    shares[last].adapter = shares[last].adapter.checked_add(*bytes)?;
                }
                Place::PerLayer | Place::EveryDevice => {
                    for share in shares.iter_mut() {
                        share.adapter = share.adapter.checked_add(*bytes)?;
                    }
                }
            }
        }
        // A tied embedding's adapter is not charged to the output device: the
        // engine places adapter tensors by the base tensor its name finds
        // (`llama-adapter.cpp`), which is the host copy of
        // `token_embd.weight`, not the duplicate.
        Some(shares)
    }
}

/// A scalar integer key, absent as `None`, any other shape refused.
fn scalar(directory: &Directory, key: &str) -> Result<Option<u64>, String> {
    match directory.values.get(key) {
        None => Ok(None),
        Some(Value::Number(n, _)) => Ok(Some(*n)),
        Some(_) => Err(format!("{key} is not a scalar integer")),
    }
}

/// A key the engine reads as one value or one per layer
/// (`get_key_or_arr`), expanded to one per layer. An array of the wrong
/// length is refused, as the engine refuses it.
fn per_layer(directory: &Directory, key: &str, layers: u32) -> Result<Option<Vec<u64>>, String> {
    match directory.values.get(key) {
        None => Ok(None),
        Some(Value::Number(n, _)) => Ok(Some(vec![*n; layers as usize])),
        Some(Value::Integers(values)) => {
            if values.len() != layers as usize {
                return Err(format!(
                    "{key} has {} entries for {layers} layers",
                    values.len()
                ));
            }
            values
                .iter()
                .map(|v| u64::try_from(*v).map_err(|_| format!("{key} holds a negative entry")))
                .collect::<Result<Vec<_>, _>>()
                .map(Some)
        }
        Some(_) => Err(format!("{key} is neither an integer nor an integer array")),
    }
}

/// The attention keys, read as the engine reads them (`llama-model.cpp`,
/// `load_hparams`): the KV-head count defaults to the head count, and each
/// head length defaults to the embedding width over the first layer's head
/// count, the sliding-window lengths to the full ones.
///
/// **A missing head count refuses, naming the layer** (PAIR-4, a kind the
/// header does not identify): read as zero heads, it would charge an
/// attention architecture no cache and say nothing.
fn attention(directory: &Directory, architecture: &str, layers: u32) -> Result<Cache, String> {
    let key = |suffix: &str| format!("{architecture}.{suffix}");
    let head_count = key("attention.head_count");
    let heads = per_layer(directory, &head_count, layers)?.ok_or_else(|| {
        format!(
            "layer 0 has no {head_count}, so its kind is not identified and its cache has no bound"
        )
    })?;
    let kv_heads =
        per_layer(directory, &key("attention.head_count_kv"), layers)?.unwrap_or(heads.clone());
    let (key_full, value_full) = if heads[0] > 0 {
        let derived = scalar(directory, &key("embedding_length"))?.map(|n| n / heads[0]);
        let read = |suffix: &str| -> Result<u64, String> {
            scalar(directory, &key(suffix))?
                .or(derived)
                .ok_or_else(|| format!("no {} and no embedding width", key(suffix)))
        };
        (
            read("attention.key_length")?,
            read("attention.value_length")?,
        )
    } else {
        (0, 0)
    };
    let key_swa = scalar(directory, &key("attention.key_length_swa"))?.unwrap_or(key_full);
    let value_swa = scalar(directory, &key("attention.value_length_swa"))?.unwrap_or(value_full);
    Ok(Cache::Attention {
        kv_heads,
        key: key_full.max(key_swa),
        value: value_full.max(value_swa),
    })
}

#[cfg(test)]
pub(crate) mod tests;
