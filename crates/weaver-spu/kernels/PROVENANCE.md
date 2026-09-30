# Provenance of the salvaged kernels and their fixtures

`weaver-spu-Spec` section 10: **the kernels cross verbatim and their tests cross
with them.** The CUDA kernel set and its build script are carried unchanged, per
the operator's ruling of 2026-08-02 and the carry rule's first door, and the
golden fixtures that compare each kernel against a candle reference come with
them, because a salvaged kernel with its comparison left behind is a kernel this
program has not checked.

That claim is tagged `review`, and **it is review's by reach rather than by
election**, which the Spec calls the rare case: what the fixtures assert about a
kernel fires when the kernel drifts, but no suite can watch a comparison that was
left behind, a test being unable to detect its own absence. Whether the
comparisons crossed is therefore a fact about the carry that a reader
establishes and a runner cannot. This file is what the reader reads.

## What crossed, and its hash at the moment of carry

Source tree: `/opt/weavertools/WeaverTools-archived/crates/weaver-spu`, the
quarry, which is read-only as a discipline. Nothing was edited in transit and
the hashes below are the check, not the assurance.

| Carried to | From the quarry at | sha256, first 16 |
|---|---|---|
| `kernels/transformer.cu` | `kernels/transformer.cu` | `80958f743cd7b4e9` |
| `build.rs` | `build.rs` | `da39cd00c9deaaa6` |
| `tests/fixtures/gptoss/rope_tables.txt` | same path | `49a8c355de2f5447` |
| `tests/fixtures/gptoss/sink_attn.txt` | same path | `18d5d7ba14432bb7` |
| `tests/fixtures/gptoss/swa_attn.txt` | same path | `70cbb04de4a8adc7` |
| `tests/fixtures/gptoss/yarn_rope_l0.txt` | same path | `12d4a7d0a1b34daf` |
| `tests/fixtures/gptoss/mxfp4_gate_up_l0e0.txt` | same path | `b40322380629c8fd` |

Verify with `sha256sum` against either tree while the quarry stands. Phase two's
closing checklist removes the quarry, which is why the values are recorded here
rather than left to be recomputed from a tree that will not exist.

## Building the kernels, and the one environment fact

The compile is gated on the `cuda` feature. `build.rs` reads
`CARGO_FEATURE_CUDA` and returns without doing anything when it is absent, so
the no-feature build needs no CUDA toolchain and the suite runs on a machine
with no device.

**The carried script defaults `CUDA_PATH` to `/usr/local/cuda`, and that is
wrong on some machines.** It falls back through `CUDA_PATH`, then `CUDA_ROOT`,
then that literal. A CachyOS box keeps the toolkit at `/opt/cuda`, so the build
needs the variable set:

    CUDA_PATH=/opt/cuda cargo build -p weaver-spu --features cuda

This is recorded rather than fixed because the script crosses verbatim, per the
ruling of 2026-08-02. Changing the default would be an edit to a carried file,
and the fact belongs with the carry either way.

**The `gguf` feature's build script dies `EEXIST` on its own dangling
symlinks, and it is not a race.** Corrected 2026-08-16; this section previously
called it one and prescribed a sweep written before the mechanism was known.

The fork's `llama-cpp-sys-2` hard-links `libllama.so` and `libggml-*.so` into
three directories under the profile, the root and its `examples` and `deps`,
guarding each with `if !dst.exists()`. **`exists` follows a symlink and
`hard_link` does not.** A `libllama.so -> libllama.so.0` whose target is gone
therefore reads absent to the guard and present to the link, and the unwrap at
`build.rs:1110` panics with `Os { code: 17, kind: AlreadyExists }`. The lines
wander between 1110, 1118 and 1126 because there are three directories.

**`cargo clean -p llama-cpp-sys-2` is not the cure and creates the
condition:** it removes the `.so.0` files and leaves the `.so` symlinks it does
not own. So does anything else that drops the versioned files while the links
survive, which is why **it recurs whenever the feature set changes** - those
directories are shared across feature configurations while the build directory
is not. Observed 2026-08-16 moving from `--features gguf` to
`--all-features`, and again when clippy re-ran the script.

The sweep, from the repo root, all three directories in one act:

    T=${CARGO_TARGET_DIR:-target}/debug
    rm -f $T/lib{llama,ggml}*.so* \
          $T/examples/lib{llama,ggml}*.so* \
          $T/deps/lib{llama,ggml}*.so*

Clearing one directory at a time lets a partial pass repopulate it before the
next panic, which reads as the same failure moving between lines. The sweep
names only the two libraries the build script hard-links, so it cannot take an
unrelated shared object with it, and it honours `CARGO_TARGET_DIR`; a release
build puts the survivors under `release` and needs the same sweep there.

**CUDA 13.3 does not compile the pinned llama.cpp, and one flag gets past
it.** `ggml/src/ggml-cuda/argsort.cu:48` calls `cuda::make_strided_iterator`
and `cuda::make_counting_iterator` behind a guard reading
`CCCL_MAJOR_VERSION >= 3 && CCCL_MINOR_VERSION >= 1`. The toolkit's CCCL
satisfies the guard and does not put those names in scope from `<cub/cub.cuh>`
alone, so the file fails to compile. The header exists, at
`/opt/cuda/include/cccl/cuda/iterator`, and nvcc will force-include it:

    NVCC_APPEND_FLAGS="-include cuda/iterator" \
      cargo build -p weaver-spu --features gguf

Verified 2026-08-16 on CUDA 13.3.73. **The flag reaches every nvcc
invocation**, including the plain `-E` probes `cc` uses to detect the compiler
family for this crate's own kernels, which then report a detection failure and
build anyway. Confine it to the runs that need it rather than exporting it.
This is an incompatibility between the pinned revision and a newer toolkit, not
a code fact, and it is recorded rather than fixed for the reason the
`CUDA_PATH` default is.

The four `-gencode` lines are `sm_86` (A6000, Ampere), `sm_89` (RTX Ada),
`sm_120` (RTX PRO Blackwell, needing CUDA >= 12.8), and a `compute_86` PTX
fallback that JITs to any architecture at or above 86.

**What has been verified, and on what.** Two machines, and the
coverage they give is uneven in a way worth stating rather than averaging.

| Machine | Toolkit | Compiles and links | Suite runs | Device-side |
|---|---|---|---|---|
| A6000 pair, Ampere | CUDA 13.4.92 | yes, all four lines | yes | suite tests yes, these kernels not run |
| RTX PRO Blackwell laptop | CUDA 13.4 | yes, all four lines | yes | suite tests yes, these kernels not run |

On the Blackwell box the linked archive was read back with `cuobjdump`: both
members of `libweaver_cuda_kernels.a` carry native SASS for `sm_86`, `sm_89`,
and `sm_120`, plus the `compute_86` PTX fallback. That is a stronger fact than
a clean nvcc exit, because it says the code for each target is present in the
artifact that links rather than merely that the compiler accepted the flags.
The fatbins on the two machines are identical, so the compiles-and-links column
is the same fact on both rows: both verified builds emitted all four lines, and
which line a machine can execute is the device-side column's question. The Ada
line has no machine behind it and rides on the `cuobjdump` evidence alone.

**The Blackwell row was re-measured 2026-09-26**, at the candle repin to
`aee9af9c`, on CUDA 13.4.92, driver 615.71.09. The archive read back with
`cuobjdump` carries the same four lines in both members, so the
compiles-and-links column is re-taken rather than inherited, and the crate's
suite ran there under `cuda,gguf`, 211 passing, where a pass also counts a test
that skipped for want of an artifact or a second card. **What ran on the card
with its assertions** was ten GGUF seam tests through llama.cpp's CUDA backend,
and four native candle tests against a Qwen2.5-0.5B safetensors export: the
sidecar header, generation through the native engine, the elected readout over
every layer, and the surprisal election. The pair, sharded, tap-family and pace
tests skipped. **These kernels did not run.** Nothing in the crate calls into
`libweaver_cuda_kernels.a`, the comparison code that would do so being the part
below that has not crossed, so they are known to be present and linkable on
Blackwell and not known to produce correct numbers there. That gap closes when
the comparison code crosses, not before.

**The Ampere row was re-measured 2026-09-26**, at the new pins, cudarc
`0.19.10` and candle `aee9af9c`, on the A6000 pair under CUDA 13.4.92, driver
615.71.09, with the box's third card, an RTX 2000 Ada, held out of the run. The
seven carried files hash to the table above. The archive read back with
`cuobjdump` carries native SASS for `sm_86`, `sm_89` and `sm_120` in both
members, and the `compute_86` PTX in the `transformer.o` member only, the
device-link member carrying none. The suite ran there under `cuda,gguf`, 211
passing, and **no device test skipped**: every artifact the suite reads was
present or named, so the count is of tests that ran with their assertions. What
ran on the pair was the GGUF seam tests through llama.cpp's CUDA backend, the
native candle tests against the Qwen2.5-0.5B safetensors export including the
pair's agreement with the single card and its pace, a sharded Qwen2.5-32B
safetensors export serving across the pair, a 35 GiB GGUF split by layer across
the pair and a 64.6 GiB split set admitted across it, and the readout tap's
neutrality on the device. One probe inside a passing test stayed unverified, the llama
family's flush declaration, its vocab-only fixture carrying no chat template.
**These kernels did not run here either**, for the reason the Blackwell
paragraph gives.


## What has not crossed yet, named so the gap is not read as completeness

The comparisons themselves live in the quarry at `src/core/gpu/kernels.rs`,
whose test module drives each kernel through its cudarc FFI launcher and
compares the result against the candle reference held in the fixtures above.
**The fixtures crossed in this act and the code that reads them did not.** Until
that module crosses, the fixtures are data no test opens, and the claim this
file exists to let a reader establish is therefore only half established: the
comparisons' inputs are here, the comparisons are not.

That is a stated gap rather than a discovered one. It is named here, and in the
crate's open items, so that a later reader checking the carry finds the answer
recorded rather than inferring completeness from the presence of the fixtures.
The remaining carry is that module and the cudarc launchers it drives, which
would land under `src/gpu/` beside the device queries `gpu/mod.rs` already
holds, and it needs the `cuda` feature, `cudarc`, and a device to run against.
**It named a `src/gpu/` volume of the Spec's layout until 2026-09-15**, when
that row was read against the tree and turned out to describe a forward path and
a sharding that live in `src/decoder/native_pair.rs`, so the carry is named by
what it is rather than by a row.

## Known defects in the carried material, inherited verbatim

Review of 2026-08-06, two independent passes, findings verified by reading the
carried sources. The carry rule keeps these unfixed here: the kernels cross
unchanged, so the defects cross with them, and this list is where they are
named so the comparison act that crosses the reader inherits a worklist rather
than a surprise. None of them is reachable today, no launcher having crossed.

- `launch_flash_attention` requests `256 * head_dim + 16384` bytes of dynamic
  shared memory and never calls `cudaFuncSetAttribute`, so any `head_dim`
  above 128 exceeds the 48 KB default and the launch fails, which is exactly
  the case the `FA2_MAX_HALF_DIM` comment claims to support. The register
  array `O_acc` also overruns for `head_dim` above 256.
- Every `launch_*` returns void and never calls `cudaGetLastError`, so a
  launch-configuration failure is invisible to the caller: stale or garbage
  output under a green test, since a later synchronize does not surface
  launch-config errors.
- `attention_output_kernel` assigns one thread per output dimension capped at
  256 with no strided loop, so `head_dim` above 256 leaves the upper
  dimensions silently unwritten on the naive path. The file's own comment
  names Gemma4 global layers at 512.
- `launch_decode_attention` enforces no layout precondition: a `head_dim` not
  a multiple of 32 corrupts shared memory, a `head_dim` above 512 overruns
  two register arrays, and `total_len == 0` divides by zero and writes NaN to
  every output element.
- The rmsnorm tree reduction assumes a power-of-two `blockDim` and the
  launcher passes `hidden_size` directly when it is under 256, so a
  non-power-of-two width under 256 drops elements from the sum. Latent at
  every current call site.
- `--use_fast_math` substitutes approximate `expf`, `tanhf`, and `rsqrtf` and
  relaxes division and square-root precision, while the candle reference the
  fixtures were generated from uses none of those approximations. The
  comparison that crosses later must carry tolerances that absorb that
  offset, and the tolerance belongs beside the comparison when it lands.
- `sink_attn.txt` line 1 reads `4 4 0.5`, and at `head_dim = 4` the derived
  attention scale is exactly `0.5`, so the trailing field cannot be told
  apart from the sink logit without the generator. The fixture crosses
  verbatim regardless: regenerating it needs the quarry's generator, and the
  ambiguity is resolved by that generator's source when the reader crosses.

## The carried prose still speaks as the quarry, and stays that way

Review of 2026-09-14, filed as issue #561 against `transformer.cu` and widened
to `build.rs` by the review of PR #573. Both files open on prose written for
the previous program, and every claim in it is false against this corpus. The
carry rule keeps them unedited for the same reason it keeps the kernel defects
above unfixed, so they are recorded here instead.

**`kernels/transformer.cu` opens on a conformance header naming an apparatus
this program excludes.** It cites `docs/architecture/weaver-spu/`, and no
`docs/architecture` exists, the Spec sitting at `docs/crates/weaver-spu/`. It
anchors on `spec-02-only-crate-holds-gpu-memory`, which is declared nowhere in
`docs` or `process` and carries the quarry's `spec-NN-` naming rather than this
corpus's kebab-case node ids. And it describes a trace through
`documentation-code-conformance-methodology`, `wt_doc_assertions`,
`wt_axiom_basis` and `WT_IS`, none of which exist here and none of which could:
axioms and conformance scoring were the quarry's apparatus and stay out of what
this program ships, and the phase two graph is built from documents rather than
from code. A reader who believed that header would go looking for an instrument
this program has no mechanism to have produced.

**`build.rs` opens on a migration note for pull requests this tree never had.**
It says the kernel sources were folded in from `weaver-inference` in `PR-0.5.C`
and that a Persephone proto step retired in `PR-1.J`. Those four strings appear
in no other tracked file.

**Neither is corrected in place, and the reason is the carry itself.** Editing
either file changes the hash recorded above and falsifies
`spu-kernels-cross-with-their-fixtures`, a merged assertion of the Spec's
section 10 cited from `src/lib.rs`. A code act may not do that, so the honest
disposition is the one this section exists for: name the false claim where the
reader meets the carry, and leave the carried bytes alone. The first draft of
PR #573 deleted the header and was withdrawn on exactly this ground.

**What that means for H1.** `transformer.cu`'s header is not a `conforms:`
citation this corpus can resolve, and it must not be read as one. The rule that
a `conforms:` line finds its declaration in a merged Spec is answered for these
two files by this entry rather than by anything inside them. Which assertion,
if any, the kernel set should cite is a question for the Spec, and it clears
when an act takes the kernels off the verbatim carry. Until then the prose is
inherited, wrong, and known.
