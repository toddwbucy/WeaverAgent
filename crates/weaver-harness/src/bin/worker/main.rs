//! conforms: harness-dev-boundary-in-the-filesystem
//!
//! The worker composition root, framework whole, per `weaver-harness-Spec`
//! sections 1 and 6: it composes loop 0 and mounts the one dev directory.
//! The `dev_` prefix marks what is yours, and nothing in this file is.
//!
//! This bin target compiles as its own crate linking the library, which is
//! what keeps the blade real: the crate-private constructor stays out of
//! reach from here and from `dev_loop`, so the loop composes the granted
//! surface or does not compile.
//!
//! Arguments are a provisioning fact, per the charter: the coordination
//! socket path to bind, the SPU binary, the gate binary, and optionally the
//! identity prefix the assembled prompt carries.

mod dev_loop;

use std::process::ExitCode;

use weaver_harness::{Harness, OrganBinaries, OrganParameters, bind_coordination};

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let (Some(socket), Some(spu), Some(gate)) = (args.next(), args.next(), args.next()) else {
        eprintln!(
            "worker <coordination-socket> <spu-binary> <gate-binary> [identity] \
             [--headroom-bytes N] [--classify-binary PATH]"
        );
        return ExitCode::FAILURE;
    };
    // **The positional identity, then the deployment's construction
    // parameters as named flags.** A parameter is optional and an organ given
    // none keeps its compiled default, so a deployment that supplies nothing
    // behaves as every deployment did before this vector existed.
    let mut identity = String::new();
    let mut parameters = OrganParameters::default();
    let mut classify: Option<std::path::PathBuf> = None;
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--headroom-bytes" => match args.next() {
                Some(value) => parameters.headroom_bytes = Some(value),
                None => {
                    eprintln!("worker: --headroom-bytes takes a value");
                    return ExitCode::FAILURE;
                }
            },
            // The classify process's binary, per weaver-spu-Spec section
            // 11: optional because the arm is, provisioned where the
            // declaration will carry the binding.
            "--classify-binary" => match args.next() {
                Some(value) => classify = Some(value.into()),
                None => {
                    eprintln!("worker: --classify-binary takes a value");
                    return ExitCode::FAILURE;
                }
            },
            other if other.starts_with("--") => {
                // Named rather than ignored: a deployment that misspells a
                // parameter would otherwise get the default and no signal.
                eprintln!("worker: unknown parameter {other}");
                return ExitCode::FAILURE;
            }
            positional => identity = positional.to_string(),
        }
    }

    let listener = match bind_coordination(std::path::Path::new(&socket)) {
        Ok(listener) => listener,
        Err(fault) => {
            eprintln!("worker: the coordination socket did not bind: {fault:?}");
            return ExitCode::FAILURE;
        }
    };
    let harness = match Harness::listen(
        listener,
        OrganBinaries {
            classify,
            spu: spu.into(),
            gate: gate.into(),
        },
        parameters,
    ) {
        Ok(harness) => harness,
        Err(fault) => {
            eprintln!("worker: the hygiene set refused construction: {fault:?}");
            return ExitCode::FAILURE;
        }
    };

    // Loop 0 serves, and the one name that crosses the dev boundary is the
    // entry below: the seat and the parsed request go across, the response
    // content comes back, and nothing else does in either direction.
    // The loop is compiled into this binary, so the record names the binary
    // and no file, per `weaver-trace-Spec` section 3.
    match harness.serve(
        &identity,
        &[],
        weaver_harness::LoopIdentity::compiled("worker"),
        dev_loop::drive,
    ) {
        Ok(_) => ExitCode::SUCCESS,
        Err(fault) => {
            eprintln!("worker: service ended below the exchange layer: {fault:?}");
            ExitCode::FAILURE
        }
    }
}
