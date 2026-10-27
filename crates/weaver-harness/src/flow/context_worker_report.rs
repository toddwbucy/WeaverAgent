//! A received organ report keeps its actual packet and private trace claim
//! through active execution, management and rollback.
use super::*;
use crate::flow::context::RecordingContext;
use crate::flow::context::serving_observation::ServingObservationAuthor;
use std::sync::Arc;

#[cfg(test)]
#[path = "context_worker_report_tests.rs"]
mod tests;

pub(super) struct Report {
    received: ReceivedOrgan,
    subsystem: Subsystem,
    author: Arc<ServingObservationAuthor>,
    event: Option<AuthoredEvent>,
    started: bool,
    returned: StoppedAuthor,
    sequence: Option<weaver_trace::Sequence>,
}

impl Report {
    pub(super) fn new(
        context: &RecordingContext,
        received: ReceivedOrgan,
        subsystem: Subsystem,
    ) -> Self {
        let Payload::Fault(report) = &received.envelope().payload else {
            unreachable!("received organ fault")
        };
        let author = Arc::new(ServingObservationAuthor::organ_report(
            context, report, subsystem,
        ));
        let event = AuthoredEvent {
            kind: serving::Kind::Fault,
            subsystem,
            turn: None,
            source: AuthoredSource::Runtime,
            causal_parent: None,
            payload: Some(serving::Payload::Fault(
                serde_json::value::to_raw_value(report).expect("received fault serializes"),
            )),
        };
        Self {
            received,
            subsystem,
            author,
            event: Some(event),
            started: false,
            returned: StoppedAuthor::default(),
            sequence: None,
        }
    }

    /// False yields to an earlier author's exact receipt. Only this claim can
    /// resume the report once its own original has been admitted.
    pub(super) fn advance<I: PumpIo>(
        &mut self,
        context: &mut RecordingContext,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<bool, RunFailure> {
        let port = match self.subsystem {
            Subsystem::Gate => ports.gate.as_deref(),
            Subsystem::Spu => ports.lifecycle.residency.as_deref(),
            _ => unreachable!("only received gate and residency reports"),
        }
        .ok_or(RunFailure::Closed)?;
        if !self.received.belongs_to(port) {
            return Err(RunFailure::Closed);
        }
        if self.sequence.is_some() {
            return Ok(true);
        }
        self.returned.check()?;
        if !self.started && !context.serving_observation_author_available() {
            return Ok(false);
        }
        let result = if self.started {
            context.resume_serving_observation(ports, &self.author)
        } else {
            self.started = true;
            context.author_serving_observation(ports, &self.author, self.event.take().unwrap())
        };
        match result {
            Ok(sequence) => self.sequence = Some(sequence),
            Err(error) => {
                self.sequence = self.author.sequence();
                return Err(self.returned.retain_started(error, &mut self.started));
            }
        }
        Ok(true)
    }
}
