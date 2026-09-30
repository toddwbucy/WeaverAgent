//! conforms: spu-classify-stateless
//!
//! The classify process, per `weaver-spu-Spec` section 11: the second bin
//! target of this crate, `weaver-spu-classify`, sharing the library, the
//! family registry, and the admission judgment. It receives exactly one
//! descriptor, its label seam end, adopts it under the same hygiene the
//! two-end adoption performs, admits the binding argv carries, and serves:
//! readiness first, then one classify exchange at a time, each answered
//! from its own stack with nothing retained, per the contract.
//!
//! Faults route by state, per the contract's sections 2 and 3: a fault
//! arising while an exchange is outstanding is that exchange's typed
//! answer, and a death is observed through closure, never reported.

use std::process::ExitCode;

use weaver_spu::channel::{ChannelFault, ClassifySocket, adopt_classify};
use weaver_spu::family::modernbert::engine::{Classifier, ClassifyFault};
use weaver_types::{
    FaultCase, FaultReport, LabelAnswer, LabelDirective, LabelRefusal, ScoredLabel,
};

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let (Some(artifact), Some(device), None) = (args.next(), args.next(), args.next()) else {
        eprintln!("weaver-spu-classify <artifact-dir> <device-ordinal>");
        return ExitCode::FAILURE;
    };
    let seam = match adopt_classify() {
        Ok(seam) => seam,
        Err(fault) => {
            eprintln!("weaver-spu-classify: the seam did not adopt: {fault:?}");
            return ExitCode::FAILURE;
        }
    };
    let ordinal: usize = match device.parse() {
        Ok(ordinal) => ordinal,
        Err(_) => {
            refuse_admission(&seam, format!("device ordinal unparsable: {device}"));
            return ExitCode::FAILURE;
        }
    };
    let device = match candle_core::Device::new_cuda(ordinal) {
        Ok(device) => device,
        Err(error) => {
            refuse_admission(&seam, format!("device {ordinal} unavailable: {error}"));
            return ExitCode::FAILURE;
        }
    };
    // Admission before service, its outcome the seam's first message: ready,
    // or the typed refusal the enter aggregate carries, per the contract.
    let classifier = match Classifier::admit(std::path::Path::new(&artifact), device) {
        Ok(classifier) => classifier,
        Err(ClassifyFault::NotAdmitted(reason)) => {
            refuse_admission(&seam, reason);
            return ExitCode::FAILURE;
        }
        Err(other) => {
            refuse_admission(&seam, format!("{other:?}"));
            return ExitCode::FAILURE;
        }
    };
    if send_answer(&seam, &LabelAnswer::Ready).is_err() {
        return ExitCode::FAILURE;
    }
    serve(&seam, &classifier)
}

/// The admission's typed failure, sent where the channel stands so the
/// fan-out arm carries it, and printed either way for the journal.
fn refuse_admission(seam: &ClassifySocket, reason: String) {
    eprintln!("weaver-spu-classify: not admitted: {reason}");
    let refusal = LabelRefusal::NotAdmitted { reason };
    if let Ok(body) = serde_json::to_vec(&refusal) {
        let _ = seam.send_octets(&body);
    }
}

fn send_answer(seam: &ClassifySocket, answer: &LabelAnswer) -> Result<(), ChannelFault> {
    let body = serde_json::to_vec(answer).map_err(|_| ChannelFault::Undecodable)?;
    seam.send_octets(&body)
}

fn send_refusal(seam: &ClassifySocket, refusal: &LabelRefusal) -> Result<(), ChannelFault> {
    let body = serde_json::to_vec(refusal).map_err(|_| ChannelFault::Undecodable)?;
    seam.send_octets(&body)
}

/// One exchange at a time, each independent: statelessness is structural,
/// the loop holding no session type to retain anything in.
fn serve(seam: &ClassifySocket, classifier: &Classifier) -> ExitCode {
    loop {
        let octets = match seam.recv_octets() {
            Ok(octets) => octets,
            // A truncated datagram is a channel fault and never a message,
            // answered as the malformed content it is, the seam serving on.
            Err(ChannelFault::Truncated { .. }) => {
                if send_refusal(seam, &LabelRefusal::MalformedContent).is_err() {
                    return ExitCode::SUCCESS;
                }
                continue;
            }
            // Closure is the harness gone or the unload underway: the
            // release is the exit, per the contract's failure section.
            Err(_) => return ExitCode::SUCCESS,
        };
        let LabelDirective::Classify { turn, content } = match serde_json::from_slice(&octets) {
            Ok(directive) => directive,
            Err(_) => {
                if send_refusal(seam, &LabelRefusal::MalformedContent).is_err() {
                    return ExitCode::SUCCESS;
                }
                continue;
            }
        };
        let outcome = match classifier.classify(&content) {
            Ok(scores) => send_answer(
                seam,
                &LabelAnswer::Scored {
                    turn,
                    labels: scores
                        .into_iter()
                        .map(|(label, score)| ScoredLabel { label, score })
                        .collect(),
                },
            ),
            Err(ClassifyFault::Oversized { requested, bound }) => {
                send_refusal(seam, &LabelRefusal::Oversized { requested, bound })
            }
            // The tokenizer refusing the content is the ask's defect, per
            // the trio's cases, never a device fault.
            Err(ClassifyFault::Malformed(_)) => send_refusal(seam, &LabelRefusal::MalformedContent),
            // The in-flight fault is this exchange's typed answer, per the
            // contract, and the seam serves on: the next exchange is
            // independent by construction.
            Err(fault) => {
                let account = serde_json::json!({
                    "organ": "spu-classify",
                    "detail": format!("{fault:?}"),
                })
                .to_string();
                let Ok(account) = serde_json::value::RawValue::from_string(account) else {
                    return ExitCode::FAILURE;
                };
                send_answer(
                    seam,
                    &LabelAnswer::Fault(FaultReport {
                        case: FaultCase::DeviceFaultDuringGeneration,
                        account,
                    }),
                )
            }
        };
        match outcome {
            Ok(()) => {}
            // The peer gone is the release; this process's own answer
            // exceeding the bound is its own defect, reported as one.
            Err(ChannelFault::Truncated { .. }) => return ExitCode::FAILURE,
            Err(_) => return ExitCode::SUCCESS,
        }
    }
}
