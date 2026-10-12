//! conforms: spu-nothing-laid-in-for-later-operations
//!
//! The decode submodule, per `weaver-spu-Spec` section 4.
//!
//! **One submodule exists**, and a later operation type is a sibling of this
//! directory in its own process rather than a variant inside it. Nothing here
//! is laid in for one: adding an operation type is adding a directory beside
//! this one, which is the reversibility test passing on this crate's own
//! future.
//!
//! The family library sits above this submodule and the device beneath it. The
//! seam between is `backend.rs`, and the loop that drives it is `session.rs`.

pub mod backend;
pub mod session;

/// The GGUF backend. llama.cpp holds the resident model and the engine
/// implements the decode primitives. [`backend::for_container`] accepts GGUF
/// when this feature is built, and dropping the residency frees the device.
#[cfg(feature = "gguf")]
pub mod gguf;

/// The GGUF path's residual tap, per `weaver-spu-Spec` section 7. Its own
/// module because it is the readout's one unsafe surface: a C callback the
/// ggml scheduler invokes from inside the graph walk, which nothing else in
/// this crate needs and nothing else should have to read around.
#[cfg(feature = "gguf")]
pub mod gguf_tap;

/// The candle-native backend, the second peer of the seam. Its residency
/// half loads safetensors onto the admitted device through the pinned candle
/// fork, and its decode half implements the five primitives over a session's
/// clone of the resident model. Qwen2 serves on one device through the fork
/// and on a pair through [`native_pair`].
#[cfg(feature = "cuda")]
pub mod native;

/// The native pair forward, the two-device half of the width election. The
/// scheme is the salvaged `forward_tp2`'s and the code is candle's ops, per
/// the module's own header.
#[cfg(feature = "cuda")]
pub mod native_pair;

/// Host arithmetic diagnostics for both rotary functions. The angle stays
/// fp32 until after sine or cosine, matching the pinned qwen2 fork's fix
/// proposed in upstream candle PR 3520. BF16-first ordering rounds integer
/// positions above 256: 15962 rounds to 15936, whose cosine is -0.267950,
/// or -0.267578 after rounding the output to BF16, instead of
/// cos(15962) = -0.908015. Each table must approximate cos(position * inv[j])
/// or sin(position * inv[j]), with the angle evaluated before BF16 rounding.
///
/// These scalar diagnostics do not call Candle's private RotaryEmbedding
/// constructor. Changing only that constructor's ordering leaves them green.
/// The CUDA-gated table test below watches this crate's production pair builder.
#[cfg(test)]
mod rotary_precision;

/// Watch the production pair builder through Candle operators on Device::Cpu.
/// Both tables reach attend's rope operation through ShardedModel::load.
/// This detects changes to that builder or Candle operator numerics, but not
/// changes confined to the fork's private single-device RotaryEmbedding.
/// The cuda feature requires a CUDA toolkit at build time even though these
/// tests use only the CPU. Transfers are identity operations here, so neither
/// placement on a GPU nor device execution is covered.
#[cfg(all(test, feature = "cuda"))]
mod rotary_precision_through_candle;
