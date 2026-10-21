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
mod read;
mod read_validate;
mod record_owner;
mod recording;
mod report;
mod sort;
mod ste100_channel;
mod ste100_socket;
mod templates;

use recording::{RecordedStateMeasurement, RecordedSte100, RecordedTerminal};

#[cfg(test)]
mod member_tests;

#[cfg(test)]
mod recording_fixture;

#[cfg(test)]
mod outbound_port_visibility {
    use super::clock::testing::FakePumpIo;
    use super::context::{
        OutboundBeginError, OutboundCompletion, OutboundProgress, RecordingContext, RunFailure,
    };
    use super::pump::PumpPorts;
    use super::sort::OutboundInput;
    use super::templates::TemplateCertificate;

    const _: fn(
        &mut RecordingContext,
        OutboundInput,
        &TemplateCertificate,
        &mut FakePumpIo,
    ) -> Result<(), OutboundBeginError> = RecordingContext::begin_outbound;
    const _: fn(
        &mut RecordingContext,
        &mut PumpPorts<'_, FakePumpIo>,
    ) -> Result<OutboundProgress, RunFailure> = RecordingContext::advance_outbound;
    const _: fn(
        &mut RecordingContext,
        &mut FakePumpIo,
    ) -> Result<Option<OutboundCompletion>, RunFailure> = RecordingContext::take_outbound;
    const _: fn(&mut RecordingContext) = RecordingContext::cancel_outbound;
}

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

#[cfg(test)]
#[allow(clippy::type_complexity)] // Keep exact public method shapes in these compile pins.
mod request_receipt_port_visibility {
    use super::clock::testing::FakePumpIo;
    use super::context::{RecordingContext, SourceReceiptError};
    use super::coordinator::CheckedGeneration;
    use super::ledger::{LeaseSlotStatus, LedgerError, SourceDisposition, SourceLease};
    use weaver_types::state_flow::SourceBlockId;

    const _: fn(&CheckedGeneration) -> Option<&SourceLease> = CheckedGeneration::receipt_lease;
    const _: fn(&mut CheckedGeneration) -> Option<SourceLease> =
        CheckedGeneration::take_receipt_lease;
    const _: fn(
        &mut RecordingContext,
        &SourceLease,
        &SourceBlockId,
        &mut FakePumpIo,
    ) -> Result<Option<SourceDisposition>, SourceReceiptError> =
        RecordingContext::take_source_receipt;
    const _: fn(
        &mut RecordingContext,
        &SourceLease,
        &SourceBlockId,
        &mut FakePumpIo,
    ) -> Result<LeaseSlotStatus, SourceReceiptError> = RecordingContext::source_receipt_status;
    const _: fn(&mut RecordingContext, &SourceLease, &SourceBlockId) -> Result<(), LedgerError> =
        RecordingContext::release_source_receipt;
    const _: fn(&mut RecordingContext, SourceLease) -> Result<(), (SourceLease, LedgerError)> =
        RecordingContext::release_source_lease;
}

#[cfg(test)]
mod pair_port_visibility {
    use super::clock::testing::FakePumpIo;
    use super::context::{PairBeginError, PairProgress, RecordingContext, RunFailure};
    use super::coordinator::OriginalRequests;
    use super::pump::PumpPorts;
    use weaver_types::state_flow::NonDispatchCause;

    const _: fn(
        &mut RecordingContext,
        OriginalRequests,
        NonDispatchCause,
        &mut FakePumpIo,
    ) -> Result<(), PairBeginError> = RecordingContext::begin_not_dispatched;
    const _: fn(
        &mut RecordingContext,
        &mut PumpPorts<'_, FakePumpIo>,
    ) -> Result<PairProgress, RunFailure> = RecordingContext::advance_pairs;
    const _: fn(
        &mut RecordingContext,
        &mut FakePumpIo,
    ) -> Result<Option<OriginalRequests>, RunFailure> = RecordingContext::take_pair_accounting;
}

#[cfg(test)]
#[allow(clippy::type_complexity)]
mod repair_port_visibility {
    use super::clock::testing::FakePumpIo;
    use super::context::{
        CorrectionProgress, EffectiveEmission, RecordingContext, RepairAccount, RepairBeginError,
        RepairCancelled, RepairExhausted, RepairObserveError, RepairPairBeginError, RepairProgress,
        RepairRefusalEvidence, RepairRefused, RestatementRequest, RunFailure,
    };
    use super::coordinator::OriginalRequests;
    use super::pump::PumpPorts;
    use super::templates::{RestatementTemplate, TemplateCertificate};
    use weaver_types::state_flow::{NonDispatchCause, SourceBlockId};

    const _: fn(
        &mut RecordingContext,
        OriginalRequests,
        TemplateCertificate,
        &'static RestatementTemplate,
        &mut FakePumpIo,
    ) -> Result<(), RepairBeginError> = RecordingContext::begin_repair;
    const _: fn(
        &mut RecordingContext,
        &mut PumpPorts<'_, FakePumpIo>,
    ) -> Result<RepairProgress, RunFailure> = RecordingContext::advance_repair;
    const _: fn(
        &mut RecordingContext,
        &mut FakePumpIo,
    ) -> Result<Option<RestatementRequest>, RunFailure> = RecordingContext::take_restatement;
    const _: fn(
        &mut RecordingContext,
        &SourceBlockId,
        OriginalRequests,
        &mut FakePumpIo,
    ) -> Result<CorrectionProgress, RepairObserveError> = RecordingContext::observe_correction;
    const _: fn(
        &mut RecordingContext,
        &mut FakePumpIo,
    ) -> Result<Option<EffectiveEmission>, RunFailure> = RecordingContext::take_effective_emission;
    const _: fn(
        &mut RecordingContext,
        &mut FakePumpIo,
    ) -> Result<Option<RepairExhausted>, RunFailure> = RecordingContext::take_repair_exhausted;
    const _: fn(
        &mut RecordingContext,
        NonDispatchCause,
        &mut FakePumpIo,
    ) -> Result<(), RunFailure> = RecordingContext::cancel_repair;
    const _: fn(
        &mut RecordingContext,
        &mut FakePumpIo,
    ) -> Result<Option<RepairCancelled>, RunFailure> = RecordingContext::take_cancelled_repair;
    const _: fn(
        &mut RecordingContext,
        &mut FakePumpIo,
    ) -> Result<Option<TemplateCertificate>, RunFailure> =
        RecordingContext::take_repair_certificate;
    const _: fn(
        &mut RecordingContext,
        &mut FakePumpIo,
    ) -> Result<Option<RepairRefused>, RunFailure> = RecordingContext::take_refused_repair;
    const _: fn(
        RepairRefused,
    ) -> (
        RepairAccount,
        Option<OriginalRequests>,
        RepairRefusalEvidence,
    ) = RepairRefused::into_parts;
    const _: fn(EffectiveEmission) -> RepairAccount = EffectiveEmission::into_account;
    const _: fn(RepairExhausted) -> RepairAccount = RepairExhausted::into_account;
    const _: fn(
        &mut RecordingContext,
        RepairAccount,
        NonDispatchCause,
        &mut FakePumpIo,
    ) -> Result<(), RepairPairBeginError> = RecordingContext::begin_repair_not_dispatched;
    const _: fn(
        &mut RecordingContext,
        &mut FakePumpIo,
    ) -> Result<Option<RepairAccount>, RunFailure> = RecordingContext::take_repair_accounting;
}

#[cfg(test)]
#[allow(clippy::type_complexity)]
mod read_port_visibility {
    use super::clock::testing::FakePumpIo;
    use super::context::{ReadBeginError, ReadProgress, RecordingContext, RunFailure};
    use super::pump::PumpPorts;
    use super::read::{CompletedRead, ReadFailure};
    use weaver_trace::TurnRef;
    use weaver_types::state_flow::ReadSelector;

    const _: fn(
        &mut RecordingContext,
        ReadSelector,
        Option<TurnRef>,
        &mut FakePumpIo,
    ) -> Result<(), ReadBeginError> = RecordingContext::begin_read;
    const _: fn(
        &mut RecordingContext,
        &mut PumpPorts<'_, FakePumpIo>,
    ) -> Result<ReadProgress, RunFailure> = RecordingContext::advance_read;
    const _: fn(
        &mut RecordingContext,
        &mut FakePumpIo,
    ) -> Result<Option<Result<CompletedRead, ReadFailure>>, RunFailure> =
        RecordingContext::take_read;
}

#[cfg(test)]
#[allow(clippy::type_complexity)]
mod measure_port_visibility {
    use super::clock::testing::FakePumpIo;
    use super::context::{
        MeasureBeginError, MeasureCompletion, MeasureProgress, MeasureRequest, RecordingContext,
        RunFailure,
    };
    use super::pump::{PumpDeadline, PumpPorts};
    use super::pump_transport::DecodePort;
    use weaver_trace::Sequence;
    use weaver_types::state_flow::DecimalU64;

    const _: fn(
        &mut RecordingContext,
        MeasureRequest,
        PumpDeadline,
        &mut DecodePort,
        &mut FakePumpIo,
    ) -> Result<(), MeasureBeginError> = RecordingContext::begin_measurement;
    const _: fn(
        &mut RecordingContext,
        &mut PumpPorts<'_, FakePumpIo>,
    ) -> Result<MeasureProgress, RunFailure> = RecordingContext::advance_measurement;
    const _: fn(&mut RecordingContext, &mut FakePumpIo) -> Result<(), RunFailure> =
        RecordingContext::cancel_measurement;
    const _: fn(
        &mut RecordingContext,
        DecimalU64,
        &mut DecodePort,
        &mut FakePumpIo,
    ) -> Result<Option<MeasureCompletion>, RunFailure> = RecordingContext::take_measurement;
    const _: fn(&mut RecordingContext, &mut FakePumpIo) -> Result<Option<Sequence>, RunFailure> =
        RecordingContext::failed_measure_refusal_progress;
}
