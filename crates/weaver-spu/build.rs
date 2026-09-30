//! `weaver-spu` build script — compiles the legacy CUDA-kernel
//! sources (folded in from `weaver-inference` in PR-0.5.C). The
//! step is gated by the `cuda` feature, since the kernels only get
//! linked when the cudarc-backed decoder path is in scope.
//!
//! The Persephone proto compile step that lived here through the
//! migration window retired in PR-1.J alongside the Python
//! embedder service.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::var("CARGO_FEATURE_CUDA").is_ok() {
        compile_cuda_kernels();
    }
    Ok(())
}

fn compile_cuda_kernels() {
    // Find CUDA toolkit
    let cuda_path = std::env::var("CUDA_PATH")
        .or_else(|_| std::env::var("CUDA_ROOT"))
        .unwrap_or_else(|_| "/usr/local/cuda".to_string());

    println!("cargo:rerun-if-changed=kernels/transformer.cu");
    println!("cargo:rerun-if-env-changed=CUDA_PATH");
    println!("cargo:rerun-if-env-changed=CUDA_ROOT");

    cc::Build::new()
        .cuda(true)
        .cudart("shared")
        .opt_level(2)
        .debug(false)
        // Native SASS for Todd's GPUs
        .flag("-gencode")
        .flag("arch=compute_86,code=sm_86") // A6000 (Ampere)
        .flag("-gencode")
        .flag("arch=compute_89,code=sm_89") // RTX Ada
        .flag("-gencode")
        .flag("arch=compute_120,code=sm_120") // RTX PRO Blackwell (laptop) — needs CUDA >= 12.8
        // PTX fallback for forward compat: compute_86 JITs to any arch >= 86,
        // so it already covers Blackwell and beyond when no native SASS matches.
        .flag("-gencode")
        .flag("arch=compute_86,code=compute_86")
        .flag("-O2")
        .flag("--use_fast_math")
        .include(format!("{}/include", cuda_path))
        .file("kernels/transformer.cu")
        .compile("weaver_cuda_kernels");

    // Link CUDA runtime and cuBLAS. Add CUDA_PATH/lib64 to the link search path:
    // a /usr/local/cuda-* toolkit (e.g. the NVIDIA-repo CUDA 13) keeps cudart/cublas
    // there, not in a default linker dir like the distro nvidia-cuda-toolkit package.
    println!("cargo:rustc-link-search=native={}/lib64", cuda_path);
    println!("cargo:rustc-link-lib=dylib=cudart");
    println!("cargo:rustc-link-lib=dylib=cublas");
}
