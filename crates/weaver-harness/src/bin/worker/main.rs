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
//! socket path to bind, the SPU binary, the gate binary, then the
//! deployment's construction parameters as named flags. No argument carries
//! an identity: the system prompt is state, seated at the open from the state
//! member's answer, per `weaver-harness-Spec` section 6.1.

mod dev_loop;

use std::process::ExitCode;

use weaver_harness::{Harness, LoopLaunch, OrganBinaries, OrganParameters, bind_coordination};

fn main() -> ExitCode {
    // **The first act, before any thread or fork**, per `weaver-harness-Spec`
    // section 2.2: the run lock and the relay's lifetime pipe the start step
    // handed across the exec are marked close-on-exec and kept for life.
    if let Err(error) = weaver_harness::keep_start_step_descriptors() {
        eprintln!("worker: an inherited descriptor could not be made close-on-exec: {error}");
        return ExitCode::FAILURE;
    }
    let mut args = std::env::args().skip(1);
    let (Some(socket), Some(spu), Some(gate)) = (args.next(), args.next(), args.next()) else {
        eprintln!(
            "worker <coordination-socket> <spu-binary> <gate-binary> \
             --gate-socket PATH [--headroom-bytes N] [--classify-binary PATH] \
             [--loop-launcher PATH --loop-grant PATH]"
        );
        return ExitCode::FAILURE;
    };
    // **The deployment's construction parameters, as named flags.** A
    // parameter is optional and an organ given none keeps its compiled
    // default, so a deployment that supplies nothing behaves as every
    // deployment did before this vector existed.
    let mut parameters = OrganParameters::default();
    let mut classify: Option<std::path::PathBuf> = None;
    let mut gate_socket: Option<std::path::PathBuf> = None;
    let mut loop_launcher: Option<std::path::PathBuf> = None;
    let mut loop_grant: Option<std::path::PathBuf> = None;
    while let Some(argument) = args.next() {
        match argument.as_str() {
            // **The gate's socket in its door**, per `weaver-harness-Spec`
            // section 3: admin lays the door and names the path, and the
            // worker never derives it from the coordination socket.
            "--gate-socket" => match args.next() {
                Some(value) => gate_socket = Some(value.into()),
                None => {
                    eprintln!("worker: --gate-socket takes a value");
                    return ExitCode::FAILURE;
                }
            },
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
            // **The loop's launcher and its grant file**, per
            // `weaver-harness-Spec` section 2.5: handed together by admin
            // where the enter will carry a loop it found standing.
            flag @ ("--loop-launcher" | "--loop-grant") => match args.next() {
                Some(value) if flag == "--loop-launcher" => loop_launcher = Some(value.into()),
                Some(value) => loop_grant = Some(value.into()),
                None => {
                    eprintln!("worker: {flag} takes a value");
                    return ExitCode::FAILURE;
                }
            },
            other if other.starts_with("--") => {
                // Named rather than ignored: a deployment that misspells a
                // parameter would otherwise get the default and no signal.
                eprintln!("worker: unknown parameter {other}");
                return ExitCode::FAILURE;
            }
            // **A fourth positional is refused, not taken.** It once carried
            // an identity prefix, which the system prompt's ruling of
            // 2026-10-06 made state and ADP-4 retired; admin's start step
            // never passed one, and one passed by hand would now be ignored
            // without a signal.
            other => {
                eprintln!("worker: unexpected argument {other}");
                return ExitCode::FAILURE;
            }
        }
    }

    let loop_launch = match (loop_launcher, loop_grant) {
        (Some(launcher), Some(grant)) => Some(LoopLaunch { launcher, grant }),
        (None, None) => None,
        _ => {
            eprintln!("worker: --loop-launcher and --loop-grant are handed together or not at all");
            return ExitCode::FAILURE;
        }
    };
    let Some(gate_socket) = gate_socket else {
        eprintln!("worker: --gate-socket is required: admin names the gate's socket in its door");
        return ExitCode::FAILURE;
    };
    let listener = match bind_coordination(std::path::Path::new(&socket)) {
        Ok(listener) => listener.with_gate_socket(gate_socket),
        Err(fault) => {
            eprintln!("worker: the coordination socket did not bind: {fault:?}");
            return ExitCode::FAILURE;
        }
    };
    let harness = match Harness::listen(
        listener,
        OrganBinaries {
            loop_launch,
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
