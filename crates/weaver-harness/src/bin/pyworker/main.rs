//! conforms: harness-dev-boundary-in-the-filesystem
//!
//! The Python-iterating worker, a builder's composition root per
//! `weaver-harness-Spec` section 6 and issue #134. Framework identical to
//! the demonstration worker - same arguments, same loop 0, same crossing -
//! with the loop body marshalled into an embedded interpreter by
//! `py_loop`. Which binary the unit starts is the operator's provisioning,
//! so this reaches an agent by the `worker-binary` configuration and
//! nothing else.
//!
//! The loop file resolves in a fixed order, per `weaver-harness-Spec`
//! section 1 and the ruling of 2026-08-20 on issue #243: the `--loop-file`
//! flag first, the declaration's member arriving on the unit's argument
//! vector because the loop is a member of the agent's harness and unique
//! to it, then the `WEAVER_PY_LOOP` environment variable as the
//! developer's bench override, then the default below. The loop files
//! themselves live in `dev_python`, the developer's directory by the
//! standing prefix.
//!
//! The binary compiles behind the `pyworker` feature, per
//! `weaver-harness-Spec` section 1, so no default build touches the
//! interpreter:
//!
//! ```text
//! cargo build --release -p weaver-harness --features pyworker --bin pyworker
//! ```

mod py_loop;

use std::process::ExitCode;

use weaver_harness::{Harness, OrganBinaries, OrganParameters, bind_coordination};

const DEFAULT_LOOP: &str = "/usr/local/libexec/weaver/loops/dev_loop.py";

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let (Some(socket), Some(spu), Some(gate)) = (args.next(), args.next(), args.next()) else {
        eprintln!(
            "pyworker <coordination-socket> <spu-binary> <gate-binary> [identity] \
             [--loop-file PATH] [--headroom-bytes N] [--classify-binary PATH]   \
             (loop file: --loop-file, else $WEAVER_PY_LOOP, else the deployed default)"
        );
        return ExitCode::FAILURE;
    };
    let mut identity = String::new();
    let mut parameters = OrganParameters::default();
    let mut classify: Option<std::path::PathBuf> = None;
    let mut declared_loop: Option<std::path::PathBuf> = None;
    while let Some(argument) = args.next() {
        match argument.as_str() {
            // The agent's declared loop, per `weaver-harness-Spec` section 1
            // and the ruling of 2026-08-20 on issue #243: the loop is a
            // member of this agent's harness and unique to it, so the
            // declaration's flag wins the resolve below.
            "--loop-file" => match args.next() {
                Some(value) => declared_loop = Some(value.into()),
                None => {
                    eprintln!("pyworker: --loop-file takes a value");
                    return ExitCode::FAILURE;
                }
            },
            "--headroom-bytes" => match args.next() {
                Some(value) => parameters.headroom_bytes = Some(value),
                None => {
                    eprintln!("pyworker: --headroom-bytes takes a value");
                    return ExitCode::FAILURE;
                }
            },
            // The classify process's binary, per weaver-spu-Spec section
            // 11: optional because the arm is, provisioned where the
            // declaration will carry the binding.
            "--classify-binary" => match args.next() {
                Some(value) => classify = Some(value.into()),
                None => {
                    eprintln!("pyworker: --classify-binary takes a value");
                    return ExitCode::FAILURE;
                }
            },
            other if other.starts_with("--") => {
                eprintln!("pyworker: unknown parameter {other}");
                return ExitCode::FAILURE;
            }
            positional => identity = positional.to_string(),
        }
    }
    // The resolve order per `weaver-harness-Spec` section 1: the declared
    // flag first, the environment's name second as the developer's bench
    // override, the deployed default last.
    let loop_path = declared_loop.unwrap_or_else(|| {
        std::path::PathBuf::from(
            std::env::var("WEAVER_PY_LOOP").unwrap_or_else(|_| DEFAULT_LOOP.to_string()),
        )
    });

    let listener = match bind_coordination(std::path::Path::new(&socket)) {
        Ok(listener) => listener,
        Err(fault) => {
            eprintln!("pyworker: the coordination socket did not bind: {fault:?}");
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
            eprintln!("pyworker: the hygiene set refused construction: {fault:?}");
            return ExitCode::FAILURE;
        }
    };

    // **The record names the file and its digest at the load**, per
    // `weaver-trace-Spec` section 3. The digest is the interpreter's own
    // `hashlib`, this crate's dependency set holding no digest of its own,
    // and a file unreadable at the load names no digest - the crossing that
    // reads it will say so in the journal as it does today.
    let digest = std::fs::read(&loop_path).ok().and_then(|bytes| {
        pyo3::Python::attach(|py| -> Option<String> {
            use pyo3::prelude::*;
            let hashlib = py.import("hashlib").ok()?;
            let digest = hashlib
                .call_method1("sha256", (pyo3::types::PyBytes::new(py, &bytes),))
                .ok()?;
            digest
                .call_method0("hexdigest")
                .ok()?
                .extract::<String>()
                .ok()
        })
    });
    let composer = weaver_harness::LoopIdentity::file("pyworker", &loop_path, digest);
    match harness.serve(&identity, &[], composer, |seat, text| {
        py_loop::drive(&loop_path, seat, text)
    }) {
        Ok(_) => ExitCode::SUCCESS,
        Err(fault) => {
            eprintln!("pyworker: service ended below the exchange layer: {fault:?}");
            ExitCode::FAILURE
        }
    }
}
