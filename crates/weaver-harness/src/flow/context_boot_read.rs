//! The one mandatory seeded read and its Recorded, context-owned boot head.
//! This owner does not open a decoder or initialize a journal or mark.

use super::*;
use weaver_trace::serving::payloads::{RecallEndMark, RecallRecord};
use weaver_traits::{ContentBlock, Message, Role};
use weaver_types::state_flow::{ReadSource, SpanRef};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) enum BootReadBeginRefusal {
    WrongPhase,
    Busy(Option<HeldInput>),
    Templates,
    AlreadyStarted,
    PriorContent,
}

#[derive(Debug)]
pub(in crate::flow) struct BootReadBeginError {
    pub refusal: BootReadBeginRefusal,
    pub failure: Option<RunFailure>,
}

#[derive(Debug)]
pub(in crate::flow) enum BootReadCompletion {
    Seated,
    Refused(ReadFailure),
    Cancelled(Box<CancelledRead>),
}

/// No constructor, Clone or deserializer can manufacture this capability.
/// Content inspection does not grant decoder, journal or placement authority.
#[derive(Debug)]
pub(in crate::flow) struct BootHead {
    read: RecordedRead,
    held: BootCandidate,
    enter: AbsoluteBound,
}

impl BootHead {
    pub(in crate::flow) fn belongs_to(&self, context: &RecordingContext) -> bool {
        self.read.belongs_to(context)
    }

    pub(in crate::flow) fn message(&self) -> &Message {
        &self.held.message
    }

    pub(in crate::flow) fn span(&self) -> &SpanRef {
        &self.held.span
    }

    pub(in crate::flow) fn end(&self) -> DecimalU64 {
        self.held.end
    }

    pub(in crate::flow) fn recall_sequence(&self) -> Sequence {
        self.read.recall_sequence()
    }

    pub(in crate::flow) fn snapshot_id(&self) -> DecimalU64 {
        self.read.snapshot_id()
    }

    pub(in crate::flow) fn enter_bound(&self) -> AbsoluteBound {
        self.enter
    }
}

#[derive(Debug)]
pub(super) struct BootCandidate {
    message: Message,
    span: SpanRef,
    end: DecimalU64,
}

impl BootCandidate {
    pub(super) fn prepare(
        read: &CompletedRead,
        start: &StartIdentity,
    ) -> Result<(Self, RecallRecord), String> {
        if read.selector() != &(ReadSelector::SystemPrompt {}) {
            return Err("boot read changed its fixed selector".into());
        }
        let QueryResult::Entries { entries } = read.result() else {
            return Err("boot read has no prompt entry".into());
        };
        let [entry] = entries.as_slice() else {
            return Err("boot read did not retain exactly one prompt".into());
        };
        let ReadSource::Seed(source) = &entry.source else {
            return Err("boot prompt is not a seed source".into());
        };
        let end = u64::try_from(entry.text.len()).map_err(|_| "boot prompt byte end overflow")?;
        let Some(selected) = &entry.selected_span else {
            return Err("boot prompt has no stored span boundary".into());
        };
        if source.seed_id != start.seed_id
            || source.ordinal.0 != 0
            || selected.span_ref.seed_id != start.seed_id
            || selected.span_ref.local_id.0 != 0
            || selected.span_ref.revision.0 != 1
            || selected.start.0 != 0
            || selected.end.0 != end
            || entry.step_query.is_some()
            || !entry.pairs.is_empty()
        {
            return Err(
                "boot prompt differs from the loaded seed or complete stored boundary".into(),
            );
        }
        let RecallRecord::Read {
            selector,
            snapshot_id,
            sources,
            spans,
        } = read.recall().map_err(|error| error.to_string())?
        else {
            unreachable!("ordinary conversion never invents a boot grant");
        };
        let [selected] = spans.as_slice() else {
            return Err("boot provenance lost its one selected interval".into());
        };
        let end_mark = RecallEndMark {
            span: selected.selected_span.span_ref.clone(),
            end: weaver_trace::serving::payloads::DecimalU64(end),
        };
        let held = Self {
            message: Message {
                role: Role::System,
                content: vec![ContentBlock::Text {
                    text: entry.text.clone(),
                }],
            },
            span: entry.selected_span.as_ref().unwrap().span_ref.clone(),
            end: end.into(),
        };
        let message = serde_json::value::to_raw_value(&held.message)
            .map_err(|error| format!("boot Message encoding failed: {error}"))?;
        Ok((
            held,
            RecallRecord::Boot {
                selector,
                snapshot_id,
                sources,
                spans,
                message,
                end_mark,
            },
        ))
    }
}

pub(super) fn carries_content(selector: &ReadSelector) -> bool {
    !matches!(
        selector,
        ReadSelector::Marks {}
            | ReadSelector::PlanCurrent {}
            | ReadSelector::Plan { .. }
            | ReadSelector::PlanFlags { .. }
            | ReadSelector::Observations { .. }
    )
}

impl RecordingContext {
    pub(in crate::flow) fn begin_boot_read(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<(), BootReadBeginError> {
        self.live(io).map_err(|failure| BootReadBeginError {
            refusal: BootReadBeginRefusal::WrongPhase,
            failure: Some(failure),
        })?;
        if self.boot_read_started || self.boot_head.is_some() {
            return Err(BootReadBeginError {
                refusal: BootReadBeginRefusal::AlreadyStarted,
                failure: None,
            });
        }
        if self.content_read_started {
            return Err(BootReadBeginError {
                refusal: BootReadBeginRefusal::PriorContent,
                failure: None,
            });
        }
        let enter = self
            .certified_template_enter()
            .map_err(|failure| BootReadBeginError {
                refusal: BootReadBeginRefusal::Templates,
                failure: Some(failure),
            })?;
        let Some(start) = self
            .open_start
            .as_ref()
            .filter(|_| self.load_duty.is_some())
        else {
            return Err(BootReadBeginError {
                refusal: BootReadBeginRefusal::WrongPhase,
                failure: None,
            });
        };
        if let Some(held) = self.held_input() {
            return Err(BootReadBeginError {
                refusal: BootReadBeginRefusal::Busy(Some(held)),
                failure: None,
            });
        }
        self.begin_read_with_purpose(
            ReadSelector::SystemPrompt {},
            None,
            ReadPurpose::Boot {
                start: start.clone(),
                enter,
                candidate: None,
            },
            io,
        )
        .map_err(|error| BootReadBeginError {
            refusal: match error.refusal {
                ReadBeginRefusal::WrongPhase => BootReadBeginRefusal::WrongPhase,
                ReadBeginRefusal::Busy => BootReadBeginRefusal::Busy(None),
            },
            failure: error.failure,
        })
    }

    /// Inspect the cache only while live. This method exposes no effect port.
    pub(in crate::flow) fn boot_head(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<Option<&BootHead>, RunFailure> {
        self.live(io)?;
        Ok(self.boot_head.as_ref())
    }

    /// Seat once after the actual Released and Recorded continuations finish.
    /// All fallible checks precede the atomic transfer from read owner to cache.
    pub(in crate::flow) fn finish_boot_read(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<Option<BootReadCompletion>, RunFailure> {
        self.live(io)?;
        let Some(read) = &self.read else {
            return Ok(None);
        };
        let ReadPurpose::Boot { candidate, .. } = &read.purpose else {
            return Err(RunFailure::ReadOwned);
        };
        if !read.complete {
            return Ok(None);
        }
        if read.released.is_none()
            || (matches!(read.released, Some(Ok(_)))
                && (read.recorded.is_none() || candidate.is_none() || self.boot_head.is_some()))
        {
            return Err(self.read_fault(io, "complete boot read lost its exact owned evidence"));
        }
        self.read_wait_matches(io)?;
        let read = self.read.take().expect("checked completed boot owner");
        let ReadPurpose::Boot {
            candidate, enter, ..
        } = read.purpose
        else {
            unreachable!("checked boot purpose");
        };
        let result = read.released.unwrap().map(|completed| RecordedRead {
            read: completed,
            sequence: read.recorded.expect("successful boot retains its sequence"),
            origin: self.read_origin.clone(),
        });
        if read.cancelled {
            return Ok(Some(BootReadCompletion::Cancelled(Box::new(
                CancelledRead { outcome: result },
            ))));
        }
        match result {
            Ok(read) => {
                self.boot_head = Some(BootHead {
                    read,
                    held: candidate.expect("successful boot retains its Message"),
                    enter,
                });
                Ok(Some(BootReadCompletion::Seated))
            }
            Err(refusal) => Ok(Some(BootReadCompletion::Refused(refusal))),
        }
    }
}
