//! The separate STE100 duty. Production adapters and artifacts remain held.
fn main() -> std::process::ExitCode {
    // SAFETY: this is the exec entrypoint. The harness placed the raw owned
    // seam at fd3; no code here opened files, adopted it or started workers.
    unsafe { weaver_spu::ste100::run() }
}
