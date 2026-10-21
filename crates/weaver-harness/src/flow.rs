//! The replacement filing path's near-side mechanics.
//!
//! These components are deliberately dormant until the foundation cutover. They
//! own copies, one state socket, classifier results and trace completion proofs.
//! The serving lifecycle and its common pump will compose them without changing
//! the compiled loop's ownership or installing a second active writer.

mod author;
mod channel;
mod classifier;
mod clock;
pub(crate) mod context;
mod coordinator;
mod halt_snapshot;
mod input;
mod intake;
mod ledger;
mod mirror;
mod observation;
mod pump;
mod pump_transport;
mod record_owner;
mod recording;
mod report;
mod ste100_channel;
mod ste100_socket;
mod templates;

use recording::{RecordedStateMeasurement, RecordedSte100, RecordedTerminal};

#[cfg(test)]
mod member_tests;

#[cfg(test)]
mod recording_fixture;

#[cfg(test)]
mod ste100_integration_tests;

#[cfg(test)]
mod pump_transport_integration_tests;

// Compile from a sibling scope so a context-private implementation cannot pass
// its own descendant tests while leaving the promised consumer port inaccessible.
#[cfg(test)]
mod receipt_port_visibility {
    use super::author::AuthoredEvent;
    use super::clock::testing::FakePumpIo;
    use super::context::{
        FilingBeginError, FilingCancellation, FilingCompletion, FilingProgress, RecordingContext,
        RunFailure,
    };
    use super::pump::{PumpDeadline, PumpPorts};

    const _: fn(
        &mut RecordingContext,
        AuthoredEvent,
        PumpDeadline,
        &mut FakePumpIo,
    ) -> Result<(), FilingBeginError> = RecordingContext::begin_filing;
    const _: fn(
        &mut RecordingContext,
        &mut PumpPorts<'_, FakePumpIo>,
    ) -> Result<FilingProgress, RunFailure> = RecordingContext::advance_filing;
    const _: fn(
        &mut RecordingContext,
        &mut FakePumpIo,
    ) -> Result<Option<FilingCompletion>, RunFailure> = RecordingContext::take_filing;
    const _: fn(&mut RecordingContext) = RecordingContext::cancel_filing;
    const _: fn(&mut RecordingContext) -> Option<FilingCancellation> =
        RecordingContext::take_cancelled_filing;
}

#[cfg(test)]
mod output_port_visibility {
    use super::clock::testing::FakePumpIo;
    use super::context::{OutputBeginError, OutputProgress, RecordingContext, RunFailure};
    use super::coordinator::{CheckedGeneration, OutputCancellation, OutputOriginal};
    use super::pump::PumpPorts;
    use weaver_trace::Sequence;

    const _: fn(
        &mut RecordingContext,
        OutputOriginal,
        Option<Sequence>,
        &mut FakePumpIo,
    ) -> Result<(), OutputBeginError> = RecordingContext::begin_output;
    const _: fn(
        &mut RecordingContext,
        &mut PumpPorts<'_, FakePumpIo>,
    ) -> Result<OutputProgress, RunFailure> = RecordingContext::advance_output;
    const _: fn(
        &mut RecordingContext,
        &mut FakePumpIo,
    ) -> Result<Option<CheckedGeneration>, RunFailure> = RecordingContext::take_output;
    const _: fn(&mut RecordingContext) = RecordingContext::cancel_output;
    const _: fn(&mut RecordingContext) -> Option<OutputCancellation> =
        RecordingContext::take_cancelled_output;
}
