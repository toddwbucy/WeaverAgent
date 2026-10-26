//! Read-only observations for actual expired-gate composition tests.
use super::*;

pub(in crate::flow::context) type DeathEvidence = (
    Option<u64>,
    Option<u64>,
    Option<crate::spawn::LastWordSnapshot>,
    bool,
);

impl ExpiredGateRollback {
    pub(in crate::flow::context) fn test_death(&self) -> Option<DeathEvidence> {
        self.death.as_ref().map(|death| {
            (
                death.observed_ns,
                death.deadline_ns,
                death.snapshot.clone(),
                death.settled,
            )
        })
    }
    pub(in crate::flow::context) fn test_retired_outgoing(&self) -> Option<(&[u8], usize)> {
        self.retired.as_ref()?.test_outgoing_packet()
    }

    pub(in crate::flow::context) fn test_recorded(&self) -> Option<Sequence> {
        self.last_recorded
    }

    pub(in crate::flow::context) fn test_pending_frame(&self) -> Option<&[u8]> {
        self.pending
            .as_ref()?
            .received
            .as_ref()
            .map(ReceivedOrgan::raw_bytes)
    }
    pub(in crate::flow::context) fn test_retired(&self) -> bool {
        self.retired.is_some()
    }
    pub(in crate::flow::context) fn test_lower(
        &self,
    ) -> Option<(&OrganEnvelope, bool, Option<u64>)> {
        self.lower.as_ref().map(|lower| {
            (
                lower,
                self.send.as_ref().is_some_and(OrganSend::sent),
                self.send.as_ref().and_then(OrganSend::sent_at),
            )
        })
    }

    pub(in crate::flow::context) fn test_replies(
        &self,
    ) -> (Option<&ReceivedOrgan>, Option<&ReceivedOrgan>) {
        (self.raise_reply.as_ref(), self.lower_reply.as_ref())
    }

    pub(in crate::flow::context) fn test_disposition(&self) -> (bool, bool, Option<Containment>) {
        (self.transport_done, self.complete, self.containment)
    }
}
