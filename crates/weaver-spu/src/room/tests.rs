use super::*;

/// One metadata value of a fixture.
pub(crate) enum Meta {
    Text(&'static str),
    U32(u32),
    U32s(Vec<u32>),
}

/// A GGUF directory with the given metadata and tensors, each tensor's data
/// laid out after the directory at 32-byte alignment and zero-filled, so the
/// file is as long as its directory says.
pub(crate) fn gguf(values: &[(&str, Meta)], tensors: &[(&str, &[u64], u32)]) -> Vec<u8> {
    fn text(out: &mut Vec<u8>, s: &str) {
        out.extend_from_slice(&(s.len() as u64).to_le_bytes());
        out.extend_from_slice(s.as_bytes());
    }
    let mut out = Vec::new();
    out.extend_from_slice(b"GGUF");
    out.extend_from_slice(&3u32.to_le_bytes());
    out.extend_from_slice(&(tensors.len() as u64).to_le_bytes());
    out.extend_from_slice(&(values.len() as u64).to_le_bytes());
    for (key, value) in values {
        text(&mut out, key);
        match value {
            Meta::Text(s) => {
                out.extend_from_slice(&8u32.to_le_bytes());
                text(&mut out, s);
            }
            Meta::U32(n) => {
                out.extend_from_slice(&4u32.to_le_bytes());
                out.extend_from_slice(&n.to_le_bytes());
            }
            Meta::U32s(ns) => {
                out.extend_from_slice(&9u32.to_le_bytes());
                out.extend_from_slice(&4u32.to_le_bytes());
                out.extend_from_slice(&(ns.len() as u64).to_le_bytes());
                for n in ns {
                    out.extend_from_slice(&n.to_le_bytes());
                }
            }
        }
    }
    let mut offset = 0u64;
    for (name, dims, kind) in tensors {
        text(&mut out, name);
        out.extend_from_slice(&(dims.len() as u32).to_le_bytes());
        for dim in *dims {
            out.extend_from_slice(&dim.to_le_bytes());
        }
        out.extend_from_slice(&kind.to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        let mut padded = dims.to_vec();
        padded.resize(4, 1);
        let bytes = directory::Tensor {
            dims: padded.try_into().expect("four dimensions"),
            kind: *kind,
            offset: 0,
        }
        .bytes()
        .expect("a sized fixture tensor");
        offset += bytes.div_ceil(32) * 32;
    }
    let data = (out.len() as u64).div_ceil(32) * 32;
    out.resize((data + offset) as usize, 0);
    out
}

/// A scratch directory removed on drop.
struct Scratch(std::path::PathBuf);

impl Scratch {
    fn new(tag: &str) -> Scratch {
        let dir = std::env::temp_dir().join(format!(
            "weaver-spu-room-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).expect("a scratch dir");
        Scratch(dir)
    }

    fn write(&self, name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let path = self.0.join(name);
        std::fs::write(&path, bytes).expect("a fixture file");
        path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const F32: u32 = 0;
const F16: u32 = 1;
const BF16: u32 = 30;
const Q8_0: u32 = 8;

/// A small dense header: `layers` layers, 4 heads, 2 KV heads, width 64.
fn dense_meta(architecture: &'static str, layers: u32) -> Vec<(&'static str, Meta)> {
    let key = |suffix: &str| -> &'static str {
        Box::leak(format!("{architecture}.{suffix}").into_boxed_str())
    };
    vec![
        ("general.architecture", Meta::Text(architecture)),
        (key("block_count"), Meta::U32(layers)),
        (key("embedding_length"), Meta::U32(64)),
        (key("attention.head_count"), Meta::U32(4)),
        (key("attention.head_count_kv"), Meta::U32(2)),
    ]
}

/// **The KV term is the 2026-11-08 Granite measurement: 4096 MiB per
/// device at 32768 positions.** Granite 4.2 30B's header (64 layers, width
/// 4096, 32 heads, 8 KV heads, no key length, so 128 per head) over two
/// devices, each taking 32 layers.
///
/// Perturbation: drop the KV term, or charge one side of the cache, and
/// this test fails.
#[test]
fn the_granite_cache_term_is_four_gibibytes_per_device() {
    let scratch = Scratch::new("granite");
    let layers = 64u32;
    let meta = vec![
        ("general.architecture", Meta::Text("granite")),
        ("granite.block_count", Meta::U32(layers)),
        ("granite.embedding_length", Meta::U32(4096)),
        ("granite.attention.head_count", Meta::U32(32)),
        ("granite.attention.head_count_kv", Meta::U32(8)),
    ];
    let names: Vec<String> = (0..layers)
        .map(|layer| format!("blk.{layer}.attn_norm.weight"))
        .collect();
    let mut tensors: Vec<(&str, &[u64], u32)> = names
        .iter()
        .map(|name| (name.as_str(), &[4096u64][..], F32))
        .collect();
    tensors.push(("output_norm.weight", &[4096], F32));
    let path = scratch.write("granite.gguf", &gguf(&meta, &tensors));
    let layout = Layout::read(&[path]).expect("the layout reads");
    let division = Division::even(layers, 2).expect("a division");
    let shares = layout.shares(&division, 32768, None).expect("shares");
    for share in &shares {
        assert_eq!(share.cache, 4096 * 1024 * 1024);
    }
}

/// **Each device is charged its own layers, not an even fraction of the
/// file.** Layers of unequal size: the first device's two layers are four
/// times the second's, and the shares say so.
///
/// Perturbation: replace the per-layer sum with the file over the width and
/// this test fails.
#[test]
fn unequal_layers_charge_unequal_shares() {
    let scratch = Scratch::new("unequal");
    let tensors: Vec<(&str, &[u64], u32)> = vec![
        ("blk.0.ffn_up.weight", &[64, 256], F16),
        ("blk.1.ffn_up.weight", &[64, 256], F16),
        ("blk.2.ffn_up.weight", &[64, 64], F16),
        ("blk.3.ffn_up.weight", &[64, 64], F16),
        ("output.weight", &[64, 32], F16),
    ];
    let path = scratch.write("dense.gguf", &gguf(&dense_meta("llama", 4), &tensors));
    let layout = Layout::read(&[path]).expect("the layout reads");
    let shares = layout
        .shares(&Division::even(4, 2).unwrap(), 256, None)
        .expect("shares");
    assert_eq!(shares[0].weights, 2 * 64 * 256 * 2);
    assert_eq!(shares[1].weights, 2 * 64 * 64 * 2);
    assert_eq!(shares[0].output, 0);
    assert_eq!(shares[1].output, 64 * 32 * 2);
}

/// **Every shard's directory is read.** A split artifact whose later shard
/// holds the second device's layers and the output: reading the first shard
/// alone would charge the second device nothing.
///
/// Perturbation: read only `paths[0]` and this test fails.
#[test]
fn a_later_shard_charges_its_layers() {
    let scratch = Scratch::new("split");
    let first = scratch.write(
        "a-00001-of-00002.gguf",
        &gguf(
            &dense_meta("llama", 2),
            &[
                ("token_embd.weight", &[64, 32], F16),
                ("blk.0.ffn_up.weight", &[64, 64], Q8_0),
            ],
        ),
    );
    let second = scratch.write(
        "a-00002-of-00002.gguf",
        &gguf(
            &[("split.no", Meta::U32(1))],
            &[
                ("blk.1.ffn_up.weight", &[64, 64], Q8_0),
                ("output.weight", &[64, 32], F16),
            ],
        ),
    );
    let layout = Layout::read(&[first, second]).expect("the layout reads");
    let shares = layout
        .shares(&Division::even(2, 2).unwrap(), 256, None)
        .expect("shares");
    let q8_layer = (64 / 32) * 34 * 64;
    assert_eq!(shares[0].weights, q8_layer);
    assert_eq!(shares[1].weights, q8_layer);
    assert_eq!(shares[1].output, 64 * 32 * 2);
    assert_eq!(layout.input_bytes(), 64 * 32 * 2);
}

/// A tensor named in two shards is refused, naming the second.
#[test]
fn a_tensor_in_two_shards_refuses() {
    let scratch = Scratch::new("twice");
    let tensors: &[(&str, &[u64], u32)] = &[("blk.0.ffn_up.weight", &[64, 64], F16)];
    let first = scratch.write("a.gguf", &gguf(&dense_meta("llama", 1), tensors));
    let second = scratch.write("b.gguf", &gguf(&[], tensors));
    let fault = Layout::read(&[first, second]).unwrap_err();
    assert_eq!(fault.shard, 1);
}

/// **A tied embedding is charged to the last device.** With no
/// `output.weight`, the engine duplicates `token_embd.weight` onto the
/// output layer's device, and the host copy stays where it was.
///
/// Perturbation: drop the tied-embedding branch and this test fails.
#[test]
fn a_tied_embedding_is_charged_to_the_last_device() {
    let scratch = Scratch::new("tied");
    let tensors: Vec<(&str, &[u64], u32)> = vec![
        ("token_embd.weight", &[64, 1000], BF16),
        ("blk.0.ffn_up.weight", &[64, 64], F16),
        ("blk.1.ffn_up.weight", &[64, 64], F16),
        ("output_norm.weight", &[64], F32),
    ];
    let path = scratch.write("tied.gguf", &gguf(&dense_meta("qwen3", 2), &tensors));
    let layout = Layout::read(&[path]).expect("the layout reads");
    let shares = layout
        .shares(&Division::even(2, 2).unwrap(), 256, None)
        .expect("shares");
    assert_eq!(shares[0].output, 0);
    assert_eq!(shares[1].output, 64 * 1000 * 2 + 64 * 4);
    assert_eq!(layout.input_bytes(), 64 * 1000 * 2);
}

/// **A sliding-window layer is charged as full attention at the full
/// context**, with the larger head length. A header whose sliding-window
/// key length exceeds the full one charges the larger on every layer, and a
/// per-layer KV-head array is read entry by entry.
///
/// Perturbation: charge the full key length alone, or read the first KV
/// entry for every layer, and this test fails.
#[test]
fn a_sliding_window_layer_is_charged_by_its_bound() {
    let scratch = Scratch::new("swa");
    let meta = vec![
        ("general.architecture", Meta::Text("gemma3")),
        ("gemma3.block_count", Meta::U32(2)),
        ("gemma3.embedding_length", Meta::U32(64)),
        ("gemma3.attention.head_count", Meta::U32(4)),
        ("gemma3.attention.head_count_kv", Meta::U32s(vec![1, 3])),
        ("gemma3.attention.key_length", Meta::U32(16)),
        ("gemma3.attention.value_length", Meta::U32(16)),
        ("gemma3.attention.key_length_swa", Meta::U32(32)),
    ];
    let tensors: &[(&str, &[u64], u32)] = &[
        ("blk.0.attn_norm.weight", &[64], F32),
        ("blk.1.attn_norm.weight", &[64], F32),
    ];
    let path = scratch.write("swa.gguf", &gguf(&meta, tensors));
    let layout = Layout::read(&[path]).expect("the layout reads");
    let shares = layout
        .shares(&Division::even(2, 2).unwrap(), 300, None)
        .expect("shares");
    // 300 positions pad to 512. The value row is the widest layer's.
    let value_row = 3 * 16;
    assert_eq!(shares[0].cache, (32 + value_row) * 512 * 2);
    assert_eq!(shares[1].cache, (3 * 32 + value_row) * 512 * 2);
}

/// **The positions are padded as the engine pads them.** One position past
/// a multiple of 256 charges the next multiple.
#[test]
fn the_context_is_padded_to_two_hundred_fifty_six() {
    let scratch = Scratch::new("pad");
    let tensors: &[(&str, &[u64], u32)] = &[("blk.0.attn_norm.weight", &[64], F32)];
    let path = scratch.write("pad.gguf", &gguf(&dense_meta("llama", 1), tensors));
    let layout = Layout::read(&[path]).expect("the layout reads");
    let at = |positions| {
        layout
            .shares(&Division::even(1, 1).unwrap(), positions, None)
            .expect("shares")[0]
            .cache
    };
    // Two KV heads of width 16 (64 over 4 heads), key and value.
    assert_eq!(at(256), 2 * 2 * 16 * 256 * 2);
    assert_eq!(at(257), 2 * 2 * 16 * 512 * 2);
}

/// **The adapter is charged to the device holding its base**: a layer's
/// target to that layer's device, the output's to the last, the embedding's
/// to none, because the engine keeps it in host memory.
#[test]
fn adapter_targets_follow_their_base() {
    let scratch = Scratch::new("adapter");
    let tensors: &[(&str, &[u64], u32)] = &[
        ("blk.0.attn_q.weight", &[64, 64], F16),
        ("blk.1.attn_q.weight", &[64, 64], F16),
        ("output.weight", &[64, 32], F16),
    ];
    let path = scratch.write("base.gguf", &gguf(&dense_meta("llama", 2), tensors));
    let layout = Layout::read(&[path]).expect("the layout reads");
    let targets: BTreeMap<String, u64> = [
        ("blk.0.attn_q.weight".to_string(), 100),
        ("blk.1.attn_q.weight".to_string(), 10),
        ("output.weight".to_string(), 1),
        ("token_embd.weight".to_string(), 1000),
    ]
    .into();
    let shares = layout
        .shares(&Division::even(2, 2).unwrap(), 256, Some(&targets))
        .expect("shares");
    assert_eq!(shares[0].adapter, 100);
    assert_eq!(shares[1].adapter, 11);
}

/// **Non-layer tensors are charged where the engine puts them**: rope
/// duplicates once per layer on each device, an unclassified name and a
/// block past the last layer on every device, the input layer on none.
#[test]
fn non_layer_tensors_are_charged_by_class() {
    let scratch = Scratch::new("classes");
    let tensors: &[(&str, &[u64], u32)] = &[
        ("blk.0.attn_norm.weight", &[64], F32),
        ("blk.1.attn_norm.weight", &[64], F32),
        ("blk.2.attn_norm.weight", &[64], F32),
        ("rope_freqs.weight", &[8], F32),
        ("mystery.weight", &[16], F32),
        ("blk.3.nextn.eh_proj.weight", &[32], F32),
        ("position_embd.weight", &[64, 4], F32),
    ];
    let path = scratch.write("classes.gguf", &gguf(&dense_meta("llama", 3), tensors));
    let layout = Layout::read(&[path]).expect("the layout reads");
    let shares = layout
        .shares(&Division::even(3, 2).unwrap(), 256, None)
        .expect("shares");
    assert_eq!(shares[0].weights, 2 * (64 * 4) + 2 * (8 * 4));
    assert_eq!(shares[1].weights, 64 * 4 + 8 * 4);
    for share in &shares {
        assert_eq!(share.every_device, 16 * 4 + 32 * 4);
    }
    assert_eq!(layout.input_bytes(), 64 * 4 * 4);
}

/// **A recurrent or hybrid architecture keeps the even fraction until the
/// second child act**, so a Qwen3.5 or Nemotron-H admission that passes
/// today is judged as it was. The layout reads and says it is deferred,
/// and offers no per-device share.
///
/// Perturbation: drop an architecture from `RECURRENT_OR_HYBRID`, or charge
/// a deferred layout by the attention formula, and this test fails.
#[test]
fn a_hybrid_architecture_keeps_the_even_fraction() {
    let scratch = Scratch::new("hybrid");
    for architecture in ["qwen35", "qwen35moe", "nemotron_h", "nemotron_h_moe"] {
        let tensors: &[(&str, &[u64], u32)] = &[("blk.0.attn_norm.weight", &[64], F32)];
        let path = scratch.write(
            &format!("{architecture}.gguf"),
            &gguf(&dense_meta(architecture, 1), tensors),
        );
        let layout = Layout::read(&[path]).expect("the layout reads");
        assert!(layout.deferred(), "{architecture} is deferred");
        assert_eq!(
            layout.shares(&Division::even(1, 1).unwrap(), 256, None),
            None
        );
        assert_eq!(layout.caches(), None, "{architecture} predicts no caches");
    }
}

/// **A dense architecture is not deferred, and every layer holds one
/// attention cache and no recurrent cache**, the prediction the PAIR-5
/// observed division judges the engine's cache records against.
#[test]
fn a_dense_architecture_is_charged_per_device() {
    let scratch = Scratch::new("dense");
    let tensors: &[(&str, &[u64], u32)] = &[("blk.0.attn_norm.weight", &[64], F32)];
    let path = scratch.write("granite.gguf", &gguf(&dense_meta("granite", 3), tensors));
    let layout = Layout::read(&[path]).expect("the layout reads");
    assert!(!layout.deferred());
    let attention = Caches {
        attention: true,
        recurrent: false,
    };
    assert_eq!(layout.caches(), Some(vec![attention; 3]));
}

/// **A per-layer array of the wrong length refuses**, as the engine
/// refuses it, rather than charging some layers by guess.
#[test]
fn a_short_per_layer_array_refuses() {
    let scratch = Scratch::new("short");
    let mut meta = dense_meta("llama", 3);
    meta[4] = ("llama.attention.head_count_kv", Meta::U32s(vec![2, 2]));
    let path = scratch.write("short.gguf", &gguf(&meta, &[]));
    assert!(Layout::read(&[path]).is_err());
}

/// **A dense header with no head count refuses and names the layer**
/// (PAIR-4, a kind the header does not identify), rather than charging the
/// architecture no cache.
///
/// Perturbation: read a missing head count as zero heads and this test
/// fails.
#[test]
fn a_missing_head_count_refuses_naming_the_layer() {
    let scratch = Scratch::new("noheads");
    let meta: Vec<_> = dense_meta("llama", 3)
        .into_iter()
        .filter(|(name, _)| *name != "llama.attention.head_count")
        .collect();
    let path = scratch.write("noheads.gguf", &gguf(&meta, &[]));
    let fault = Layout::read(&[path]).expect_err("an unidentified kind refuses");
    assert_eq!(fault.shard, 0);
    assert!(
        fault.detail.contains("layer 0") && fault.detail.contains("llama.attention.head_count"),
        "the refusal names the layer and the key: {}",
        fault.detail
    );
}

/// A header with no block count refuses: there is no division to compute.
#[test]
fn no_block_count_refuses() {
    let scratch = Scratch::new("noblocks");
    let meta = vec![("general.architecture", Meta::Text("llama"))];
    let path = scratch.write("noblocks.gguf", &gguf(&meta, &[]));
    assert!(Layout::read(&[path]).is_err());
}

/// A division of another depth has no share.
#[test]
fn a_division_of_another_depth_has_no_share() {
    let scratch = Scratch::new("depth");
    let path = scratch.write("d.gguf", &gguf(&dense_meta("llama", 2), &[]));
    let layout = Layout::read(&[path]).expect("the layout reads");
    assert_eq!(
        layout.shares(&Division::even(3, 1).unwrap(), 256, None),
        None
    );
}
