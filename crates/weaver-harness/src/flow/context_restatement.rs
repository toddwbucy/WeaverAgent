//! Private bridges for the worker's issued repair attempt. The instruction's
//! Recorded grant alone cannot supply placement material: its exact Filed
//! receipt moves into the selected unit before the engine may use it.
use super::*;
use crate::flow::ledger::SourceDisposition;
use crate::flow::material::SelectedUnit;
use crate::flow::place::{PlaceError, PlaceRequest, PlacementGrant, PreparedPrefix};
use crate::flow::pump_transport::DecodePort;
use std::sync::Arc;
use weaver_types::state_flow::CutMethod;

impl RecordingContext {
    /// Withdraw only this ungenerated attempt after its actual placement
    /// refusal. The caller keeps that refusal and receives the original repair
    /// account for explicit not-dispatched accounting before closing the turn.
    pub(in crate::flow) fn finish_refused_restatement(
        &mut self,
        request: &RestatementRequest,
        refusal: &crate::flow::place::PlaceRefused,
        io: &mut impl PumpIo,
    ) -> Result<RepairAccount, RunFailure> {
        self.live(io)?;
        if !self.restatement_live(request)
            || self.place.is_some()
            || self.generation.is_some()
            || !refusal.account.as_ref().is_some_and(|account| {
                account.refused_restatement(self, &request.recorded().block_id)
            })
        {
            return Err(RunFailure::RepairOwned);
        }
        Ok(self.take_refused_restatement_account())
    }

    /// A completed, recorded decoder refusal withdraws only its exact issued
    /// attempt. Its retained generation account stays with the caller; there
    /// is no complete correction to charge or release as successful output.
    pub(in crate::flow) fn finish_refused_restatement_generation(
        &mut self,
        request: &RestatementRequest,
        account: &PlaceAccount,
        io: &mut impl PumpIo,
    ) -> Result<RepairAccount, RunFailure> {
        self.live(io)?;
        if !self.restatement_live(request)
            || self.place.is_some()
            || self.generation.is_some()
            || !account.refused_restatement_generation(self, &request.recorded().block_id)
        {
            return Err(RunFailure::RepairOwned);
        }
        Ok(self.take_refused_restatement_account())
    }

    fn take_refused_restatement_account(&mut self) -> RepairAccount {
        let work = self.repair.as_mut().unwrap();
        work.ledger.cancel();
        let (account, pending) = work
            .ledger
            .take_cancelled()
            .expect("refused attempt has no outstanding correction loan")
            .into_parts();
        assert!(
            pending.is_none(),
            "refused attempt has no pending correction"
        );
        self.finish_repair_wait();
        self.retire_repair_owner();
        account
    }
    pub(in crate::flow) fn repair_request_scheme(&self) -> Option<&RequestScheme> {
        self.repair.as_ref().map(|work| &work.scheme)
    }
    fn restatement_live(&self, request: &RestatementRequest) -> bool {
        self.repair.as_ref().is_some_and(|work| {
            !work.internal
                && work.issued
                && !work.output_started
                && work.cancel.is_none()
                && !work.refused
                && work.issued_source.as_ref() == Some(request.source())
                && work.issued_instruction.as_ref() == Some(request.recorded())
                && work.ledger.owns_restatement(request)
        })
    }

    /// None means the actual Filed observation is still owed. Receipt custody
    /// is retained on every refusal; neither an ID nor a checker result is a
    /// substitute for the committed source.
    pub(in crate::flow) fn restatement_material(
        &mut self,
        request: &mut RestatementRequest,
        io: &mut impl PumpIo,
    ) -> Result<Option<SelectedUnit>, PlaceError> {
        self.live(io).map_err(|_| PlaceError::WrongPhase)?;
        if !self.restatement_live(request) {
            return Err(PlaceError::WrongProof);
        }
        self.reduce_flow(io).map_err(|_| PlaceError::WrongPhase)?;
        let lease = request.lease.as_ref().ok_or(PlaceError::InvalidContext)?;
        if request.filed.is_none() {
            request.filed = self
                .flow
                .take_source_receipt(lease, &request.recorded().block_id)
                .map_err(|_| PlaceError::WrongProof)?;
        }
        let Some(disposition) = request.filed.take() else {
            return Ok(None);
        };
        let valid = matches!(&disposition, SourceDisposition::Committed(source)
            if matches!(source.file(), crate::flow::ledger::LedgerFile::Statement(file)
                if file.block_id == request.recorded().block_id
                    && file.source.kind == "loop.instruction" && file.source.origin == "loop"
                    && file.text == request.instruction().text()));
        if !valid {
            request.filed = Some(disposition);
            return Err(PlaceError::WrongProof);
        }
        let SourceDisposition::Committed(source) = disposition else {
            unreachable!()
        };
        match crate::flow::material::from_filed(Arc::clone(&self.read_origin), source) {
            Ok(unit) => Ok(Some(unit)),
            Err((source, _)) => {
                request.filed = Some(SourceDisposition::Committed(source));
                Err(PlaceError::InvalidUnits)
            }
        }
    }

    pub(in crate::flow) fn prepare_restatement_prefix(
        &mut self,
        request: &RestatementRequest,
        method: CutMethod,
        io: &mut impl PumpIo,
    ) -> Result<PreparedPrefix, PlaceError> {
        if !self.restatement_live(request) {
            return Err(PlaceError::WrongProof);
        }
        self.repair.as_mut().unwrap().internal = true;
        let result = self.prepare_prefix(None, method, io);
        self.repair.as_mut().unwrap().internal = false;
        result
    }

    #[allow(clippy::result_large_err)] // Preserve the underlying port's move-owned return.
    pub(in crate::flow) fn begin_restatement_place(
        &mut self,
        restatement: &RestatementRequest,
        request: PlaceRequest,
        decode: &mut DecodePort,
        io: &mut impl PumpIo,
    ) -> Result<(), PlaceBeginError<PlaceRequest>> {
        if !self.restatement_live(restatement)
            || request.prefix.cut.is_some()
            || request.context.instruction.as_ref() != Some(&restatement.recorded().block_id)
            || request.selection.len() != 1
            || !self.restatement_instruction_valid(&request)
        {
            return Err(PlaceBeginError {
                input: request,
                refusal: PlaceError::WrongProof,
                failure: None,
                staged: None,
            });
        }
        self.repair.as_mut().unwrap().internal = true;
        let result = self.begin_place(request, decode, io);
        self.repair.as_mut().unwrap().internal = false;
        result
    }

    #[allow(clippy::result_large_err)] // Preserve the underlying port's move-owned return.
    pub(in crate::flow) fn begin_restatement_generation(
        &mut self,
        request: &RestatementRequest,
        grant: PlacementGrant,
        decode: &mut DecodePort,
        io: &mut impl PumpIo,
    ) -> Result<(), PlaceBeginError<PlacementGrant>> {
        if !self.restatement_live(request)
            || grant.units.len() != 1
            || !grant.units.iter().all(|unit| {
                unit.belongs_to(self)
                    && unit.filed_source().is_some_and(|committed| {
                        matches!(committed.file(),
                    crate::flow::ledger::LedgerFile::Statement(file)
                    if file.block_id == request.recorded().block_id
                        && file.text == request.instruction().text()
                        && file.source.kind == "loop.instruction" && file.source.origin == "loop")
                    })
            })
        {
            return Err(PlaceBeginError {
                input: grant,
                refusal: PlaceError::WrongProof,
                failure: None,
                staged: None,
            });
        }
        self.repair.as_mut().unwrap().internal = true;
        let result = self.begin_generation(grant, decode, io);
        self.repair.as_mut().unwrap().internal = false;
        result
    }

    pub(in crate::flow::context) fn restatement_instruction_valid(
        &self,
        request: &PlaceRequest,
    ) -> bool {
        let Some(work) = &self.repair else {
            return false;
        };
        let Some(source) = &work.issued_instruction else {
            return false;
        };
        work.issued
            && !work.refused
            && work.cancel.is_none()
            && request.context.instruction.as_ref() == Some(&source.block_id)
            && request.selection.iter().any(|unit| {
                unit.belongs_to(self)
                    && unit.filed_source().is_some_and(|committed| {
                        matches!(committed.file(),
                    crate::flow::ledger::LedgerFile::Statement(file)
                    if file.block_id == source.block_id && file.source.kind == "loop.instruction"
                        && file.source.origin == "loop")
                    })
            })
    }
}
