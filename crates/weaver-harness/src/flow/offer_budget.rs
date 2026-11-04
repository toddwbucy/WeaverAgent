//! Spec 02 LOOKUP-OFFER and workflow 5.10 and 5.11, allowance accounting.
//! The caller must keep one owner for the whole external turn. Recorder callbacks
//! report admission and Recorded separately. This component grants no decode,
//! placement or source authority and has no method to change the elected cap.

use std::sync::Arc;

use weaver_types::RefusalRecord;
use weaver_types::state_flow::InstructionRefusal;

/// An opaque reservation belongs to one owner and is never reconstructed from IDs.
#[derive(Debug)]
#[must_use]
pub(super) struct Reservation(Arc<()>);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AccountingError {
    UnknownReservation,
    NotAdmitted,
    AlreadyAdmitted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Cancellation {
    Released,
    AwaitingRecord,
}

/// Accounting outcomes only. A separate recorded instruction grants the pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RecordedAllowance {
    Consumed,
    ConsumedAfterCancellation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Counts {
    pub consumed: u32,
    pub reserved: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Reserved,
    Admitted,
    Cancelled,
}

struct Pending {
    identity: Arc<()>,
    phase: Phase,
}

/// No source or generation owns this count. Its lifetime is the external turn.
pub(super) struct OfferBudget {
    cap: Option<u32>,
    consumed: u32,
    pending: Vec<Pending>,
    cancelled: bool,
}

impl OfferBudget {
    /// Preserve a missing or zero election for an attributed reserve refusal.
    pub(super) fn new(cap: Option<u32>) -> Self {
        Self {
            cap,
            consumed: 0,
            pending: Vec::new(),
            cancelled: false,
        }
    }

    pub(super) fn counts(&self) -> Counts {
        Counts {
            consumed: self.consumed,
            reserved: self.pending.len(),
        }
    }

    /// Reserve before instruction authoring. Writer pressure leaves this entry held.
    pub(super) fn reserve(&mut self) -> Result<Reservation, Box<RefusalRecord>> {
        if self.cancelled
            || self.cap.is_none_or(|cap| {
                u64::from(self.consumed) + self.pending.len() as u64 >= u64::from(cap)
            })
        {
            let cap = self
                .cap
                .map_or_else(|| "absent".into(), |cap| cap.to_string());
            return Err(Box::new(RefusalRecord::Instruction {
                template_id: "thesaurus-offer/1".into(),
                case: InstructionRefusal::InvalidContext,
                detail: format!(
                    "offer_pass_cap={cap} consumed={} reserved={} cancelled={}",
                    self.consumed,
                    self.pending.len(),
                    self.cancelled
                ),
            }));
        }
        let identity = Arc::new(());
        self.pending.push(Pending {
            identity: identity.clone(),
            phase: Phase::Reserved,
        });
        Ok(Reservation(identity))
    }

    fn index(&self, reservation: &Reservation) -> Result<usize, AccountingError> {
        self.pending
            .iter()
            .position(|p| Arc::ptr_eq(&p.identity, &reservation.0))
            .ok_or(AccountingError::UnknownReservation)
    }

    /// Call only after recorder admission. Admission consumes no allowance yet.
    pub(super) fn admitted(&mut self, reservation: &Reservation) -> Result<(), AccountingError> {
        let index = self.index(reservation)?;
        let pending = &mut self.pending[index];
        match pending.phase {
            Phase::Reserved => {
                pending.phase = Phase::Admitted;
                Ok(())
            }
            Phase::Admitted | Phase::Cancelled => Err(AccountingError::AlreadyAdmitted),
        }
    }

    /// Call at the instruction's Recorded boundary, including after cancellation.
    pub(super) fn recorded(
        &mut self,
        reservation: &Reservation,
    ) -> Result<RecordedAllowance, AccountingError> {
        let index = self.index(reservation)?;
        let outcome = match self.pending[index].phase {
            Phase::Reserved => return Err(AccountingError::NotAdmitted),
            Phase::Admitted => RecordedAllowance::Consumed,
            Phase::Cancelled => RecordedAllowance::ConsumedAfterCancellation,
        };
        self.pending.remove(index);
        self.consumed += 1;
        Ok(outcome)
    }

    /// Cancel or refuse an instruction. Admitted work remains owed to the recorder.
    pub(super) fn cancel(
        &mut self,
        reservation: &Reservation,
    ) -> Result<Cancellation, AccountingError> {
        let index = self.index(reservation)?;
        match self.pending[index].phase {
            Phase::Reserved => {
                self.pending.remove(index);
                Ok(Cancellation::Released)
            }
            Phase::Admitted | Phase::Cancelled => {
                self.pending[index].phase = Phase::Cancelled;
                Ok(Cancellation::AwaitingRecord)
            }
        }
    }

    /// End admission while retaining each admitted instruction for late accounting.
    pub(super) fn cancel_turn(&mut self) {
        self.cancelled = true;
        self.pending.retain_mut(|pending| match pending.phase {
            Phase::Reserved => false,
            Phase::Admitted | Phase::Cancelled => {
                pending.phase = Phase::Cancelled;
                true
            }
        });
    }
}

#[cfg(test)]
#[path = "offer_budget_tests.rs"]
mod tests;
