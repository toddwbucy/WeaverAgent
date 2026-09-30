//! conforms: spu-out-of-order-refused-on-residency
//!
//! The lifecycle seam exercised against `weaver-harness-spu-contract`'s failure
//! cases, per gate H3: a contract that names a refusal and a build that cannot
//! produce it has not implemented the contract.
//!
//! These drive the real binary over a real socket pair, so what is exercised is
//! the envelope crossing the seam rather than a function called in-process. The
//! contract's ordering clauses are what they read:
//!
//! - admit is first and happens exactly once on a channel
//! - release is last, happens at most once, and is terminal
//! - a release with no completed admit before it is refused and is not queued
//! - a directive that arrives out of that order is refused and is not queued
//!
//! **The success path is not exercised here** because this file runs on every
//! build, and a build with no backend cannot admit. `tests/loaded.rs` carries
//! the admit-then-release round trip, and its coverage is conditional in a way
//! worth stating exactly: it runs on a build with the `cuda` and `gguf`
//! features, on a machine with a device and the model fixture, and anywhere
//! else it skips. A skip is a pass to the harness, so a green run of that file
//! on a fixture-less machine verifies nothing, and `WEAVER_TEST_GGUF` exists
//! precisely so a runner that means to exercise the load path fails rather
//! than skips when it cannot.

mod common;

use std::os::fd::{AsRawFd, OwnedFd};
use std::process::{Child, Command, Stdio};

use common::{ask, bound_receives, place_inherited, seqpacket_pair};
use weaver_types::{
    ArtifactRef, DecoderInstruction, DeviceOrdinal, LifecycleDirective, LifecycleRefusal,
    ModelBinding, Opener, OrganEnvelope, Payload, Position, SpuInstruction,
};

/// A harness end: send one directive, read one answer.
struct Harness {
    end: OwnedFd,
    ordinal: u64,
}

impl Harness {
    fn ask(&mut self, directive: LifecycleDirective) -> OrganEnvelope {
        self.ordinal += 1;
        ask(&self.end, self.ordinal, directive)
    }
}

fn started() -> (Harness, Child) {
    let (parent, child_lifecycle) = seqpacket_pair();
    let (_decode, child_decode) = seqpacket_pair();
    let mut command = Command::new(env!("CARGO_BIN_EXE_weaver-spu"));
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    place_inherited(
        &mut command,
        &[child_lifecycle.as_raw_fd(), child_decode.as_raw_fd()],
    );
    let child = command.spawn().expect("the binary starts");
    bound_receives(&parent, 30);
    (
        Harness {
            end: parent,
            ordinal: 0,
        },
        child,
    )
}

fn binding() -> ModelBinding {
    ModelBinding {
        artifact: ArtifactRef("/nonexistent/artifact".into()),
        devices: vec![DeviceOrdinal(0)],
    }
}

fn instruction() -> SpuInstruction {
    SpuInstruction {
        classify: None,
        decoder: DecoderInstruction {
            model_binding: binding(),
            residual_readout_election: false,
            field_election: None,
            surprisal_election: false,
            refeed_permission: false,
            column_permission: false,
            identity: vec![],
            tunable_values: [
                ("max-tokens-per-turn".to_string(), 4096.0),
                ("context-capacity".to_string(), 4096.0),
                ("seed".to_string(), 11.0),
            ]
            .into_iter()
            .collect(),
        },
    }
}

/// **A release before any admit answers `OutOfOrder` and is not queued,** per
/// Spec sections 9 and 10: the order is judged against the seam's recorded
/// position before the directive reaches residency, and the refusal returns on
/// the exchange that asked rather than the directive being held.
///
/// Perturbation: route the before-admit release through `Residency::release`
/// and this test fails, because the answer becomes `NoResidency`, which is
/// residency's account rather than the position's. Watched under exactly that
/// change.
#[test]
fn a_release_before_any_admit_answers_out_of_order_on_the_seam() {
    let (mut harness, mut child) = started();
    let answer = harness.ask(LifecycleDirective::Release);
    assert_eq!(
        answer.payload,
        Payload::Refusal(LifecycleRefusal::OutOfOrder),
        "a release before any admit is out of order for the seam's position"
    );
    assert_eq!(answer.exchange.ordinal, 1, "on the exchange that asked");
    drop(harness);
    let _ = child.wait();
}

/// **A directive outside this seam's drawn vocabulary is refused and is not
/// queued.** The contract draws admit and release, so the floor's other cases
/// refuse as out of order.
#[test]
fn a_directive_outside_the_vocabulary_is_refused_on_the_seam() {
    let (mut harness, mut child) = started();
    for directive in [
        LifecycleDirective::Leave,
        LifecycleDirective::Stop,
        LifecycleDirective::List,
    ] {
        let answer = harness.ask(directive.clone());
        assert_eq!(
            answer.payload,
            Payload::Refusal(LifecycleRefusal::OutOfOrder),
            "{directive:?} is outside this seam's vocabulary"
        );
    }
    drop(harness);
    let _ = child.wait();
}

/// **Admit happens exactly once on a channel.** A second admit is refused on
/// the ordering rather than re-running the steps, because this crate begins
/// empty, admits once, and dies.
///
/// The first admit refuses here too, on the artifact, since no build can yet
/// take a device. What this reads is that the two refusals differ: the first
/// names the artifact and the second names the order, which is what shows the
/// second one never re-ran step one.
///
/// Perturbation: remove the `admit_attempted` guard from `Residency::admit` and
/// this test fails, because the second admit returns the artifact refusal
/// again. Watched under exactly that removal.
#[test]
fn a_second_admit_is_refused_on_the_ordering_across_the_seam() {
    let (mut harness, mut child) = started();

    let first = harness.ask(LifecycleDirective::Admit {
        instruction: instruction(),
    });
    assert_eq!(
        first.payload,
        Payload::Refusal(LifecycleRefusal::ArtifactUnresolvable),
        "the first admit refuses at step one, on the artifact"
    );

    let second = harness.ask(LifecycleDirective::Admit {
        instruction: instruction(),
    });
    assert_eq!(
        second.payload,
        Payload::Refusal(LifecycleRefusal::OutOfOrder),
        "the second admit refuses on the ordering, never re-running step one"
    );

    drop(harness);
    let _ = child.wait();
}

/// One directive receives exactly one answer, on the exchange that asked it.
/// The serial property of Spec section 2 read from the outside.
#[test]
fn one_directive_receives_one_answer_on_its_own_exchange() {
    let (mut harness, mut child) = started();
    for expected in 1..=3u64 {
        let answer = harness.ask(LifecycleDirective::List);
        assert_eq!(answer.exchange.ordinal, expected);
        assert_eq!(answer.exchange.opener, Opener::Harness);
        assert_eq!(answer.position, Position::Close);
        assert!(matches!(answer.payload, Payload::Refusal(_)));
    }
    drop(harness);
    let _ = child.wait();
}
