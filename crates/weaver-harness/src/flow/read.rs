//! Bounded immutable read assembly and exact snapshot cleanup.
//!
//! The context supplies already recorded, correlated control completions and owns
//! one absolute deadline across every leg. This module owns neither a socket nor
//! a clock. Its result proves complete validation and release, not recall
//! provenance, held-material authority or permission to place content.

use std::fmt;

use weaver_types::state_flow::{
    ControlRefusalCode, DecimalU64, QueryResult, ReadPage, ReadRefusalCode, ReadSelector,
    StateControl, StateControlAnswer, decode_strict,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::flow) enum ReadFailure {
    Refused {
        code: ReadRefusalCode,
        detail: String,
    },
    FlowFailed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::flow) struct PageReadError(&'static str);

impl fmt::Display for PageReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

impl std::error::Error for PageReadError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) enum ReadAdvance {
    Pending,
    Complete,
}

/// The private constructor is reached only after a complete valid result and
/// its exact Released answer. RawValue fields retain the original JSON bytes.
#[derive(Debug)]
pub(in crate::flow) struct CompletedRead {
    selector: ReadSelector,
    snapshot_id: DecimalU64,
    result: QueryResult,
    raw: Vec<u8>,
}

impl CompletedRead {
    pub(in crate::flow) fn recall(
        &self,
    ) -> Result<weaver_trace::serving::payloads::RecallRecord, PageReadError> {
        recall::convert(self)
    }

    pub(in crate::flow) fn selector(&self) -> &ReadSelector {
        &self.selector
    }

    pub(in crate::flow) fn snapshot_id(&self) -> DecimalU64 {
        self.snapshot_id
    }

    pub(in crate::flow) fn result(&self) -> &QueryResult {
        &self.result
    }

    pub(in crate::flow) fn raw_bytes(&self) -> &[u8] {
        &self.raw
    }
}

#[path = "read_recall.rs"]
mod recall;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Read,
    Next,
    Release,
    Complete,
    Taken,
    Failed,
}

#[derive(Debug)]
struct Flight {
    id: DecimalU64,
    control: StateControl,
}

#[derive(Debug)]
pub(in crate::flow) struct PageRead {
    selector: ReadSelector,
    limit: DecimalU64,
    phase: Phase,
    flight: Option<Flight>,
    last_request: DecimalU64,
    snapshot: Option<DecimalU64>,
    total: Option<DecimalU64>,
    bytes: Vec<u8>,
    result: Option<QueryResult>,
    refusal: Option<ReadFailure>,
}

impl PageRead {
    pub(in crate::flow) fn selector(&self) -> &ReadSelector {
        &self.selector
    }
    pub(in crate::flow) fn take_buffered_bytes(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.bytes)
    }
    pub(in crate::flow) fn buffered_bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub(in crate::flow) fn snapshot(&self) -> Option<DecimalU64> {
        self.snapshot
    }
    pub(in crate::flow) fn new(
        selector: ReadSelector,
        limit: DecimalU64,
    ) -> Result<Self, PageReadError> {
        if limit.0 == 0 || usize::try_from(limit.0).is_err() {
            return Err(PageReadError("invalid complete read byte limit"));
        }
        Ok(Self {
            selector,
            limit,
            phase: Phase::Read,
            flight: None,
            last_request: 0.into(),
            snapshot: None,
            total: None,
            bytes: Vec::new(),
            result: None,
            refusal: None,
        })
    }

    pub(in crate::flow) fn next_control(&self) -> Option<StateControl> {
        if self.flight.is_some() {
            return None;
        }
        match self.phase {
            Phase::Read => Some(StateControl::Read {
                selector: self.selector.clone(),
            }),
            Phase::Next => Some(StateControl::Next {
                snapshot_id: self.snapshot.expect("validated first page"),
                offset: (self.bytes.len() as u64).into(),
            }),
            Phase::Release => Some(StateControl::Release {
                snapshot_id: self.snapshot.expect("acquired snapshot"),
            }),
            Phase::Complete | Phase::Taken | Phase::Failed => None,
        }
    }

    /// A Busy coordinator has not bound anything. Bind only its successful
    /// start, using the exact channel-assigned ID and submitted control body.
    pub(in crate::flow) fn bind_request(
        &mut self,
        id: DecimalU64,
        control: &StateControl,
    ) -> Result<(), PageReadError> {
        if id.0 == 0 || id <= self.last_request || self.next_control().as_ref() != Some(control) {
            return self.fail("read request does not match its pending leg");
        }
        self.last_request = id;
        self.flight = Some(Flight {
            id,
            control: control.clone(),
        });
        Ok(())
    }

    /// The caller retains the complete answer if this check fails. A failure
    /// freezes this owner and requests no guessed release or later content.
    pub(in crate::flow) fn accept(
        &mut self,
        id: DecimalU64,
        control: &StateControl,
        answer: &StateControlAnswer,
    ) -> Result<ReadAdvance, PageReadError> {
        if self.phase == Phase::Failed
            || self
                .flight
                .as_ref()
                .is_none_or(|flight| flight.id != id || &flight.control != control)
        {
            return self.fail("read answer does not match the outstanding request");
        }
        let outcome = match (self.phase, answer) {
            (Phase::Read | Phase::Next, StateControlAnswer::ReadPage(page)) => {
                self.accept_page(page)
            }
            (
                Phase::Read | Phase::Next,
                StateControlAnswer::ControlRefused {
                    code: ControlRefusalCode::Read(code),
                    detail,
                },
            ) => {
                let allowed = match self.phase {
                    // STATE-PAGES: a fresh read owns no cursor. A cursor-only
                    // refusal would reveal a broken channel state, not a query
                    // outcome the caller can clean up with a guessed identity.
                    Phase::Read => matches!(
                        code,
                        ReadRefusalCode::UnsupportedOperation
                            | ReadRefusalCode::InvalidSelector
                            | ReadRefusalCode::WrongSeed
                            | ReadRefusalCode::NotFound
                            | ReadRefusalCode::UnknownSpan
                            | ReadRefusalCode::SpanOpen
                            | ReadRefusalCode::SpanPending
                            | ReadRefusalCode::PartialPair
                            | ReadRefusalCode::CounterExhausted
                            | ReadRefusalCode::ResultTooLarge
                            | ReadRefusalCode::ResultUnavailable
                            | ReadRefusalCode::AnswerTooLarge
                    ),
                    // Later page allocation/encoding can fail without dropping
                    // the acquired cursor (STATE-PAGES and STATE-CHANNEL's
                    // complete ANSWER_BOUND). Query-selection codes cannot.
                    Phase::Next => matches!(
                        code,
                        ReadRefusalCode::UnknownSnapshot
                            | ReadRefusalCode::WrongOffset
                            | ReadRefusalCode::ResultUnavailable
                            | ReadRefusalCode::AnswerTooLarge
                    ),
                    _ => unreachable!("read refusals are matched only on read and next"),
                };
                if !allowed {
                    return self.fail("read refusal is not valid for its request leg");
                }
                self.refusal = Some(ReadFailure::Refused {
                    code: *code,
                    detail: detail.clone(),
                });
                self.phase = if self.snapshot.is_some() {
                    Phase::Release
                } else {
                    Phase::Complete
                };
                Ok(())
            }
            (Phase::Release, StateControlAnswer::Released { snapshot_id })
                if self.snapshot == Some(*snapshot_id) =>
            {
                self.phase = Phase::Complete;
                Ok(())
            }
            (Phase::Release, _) => Err(PageReadError("snapshot release did not complete exactly")),
            _ => Err(PageReadError("unexpected answer for read phase")),
        };
        if let Err(error) = outcome {
            self.phase = Phase::Failed;
            return Err(error);
        }
        self.flight = None;
        Ok(if self.phase == Phase::Complete {
            ReadAdvance::Complete
        } else {
            ReadAdvance::Pending
        })
    }

    fn accept_page(&mut self, page: &ReadPage) -> Result<(), PageReadError> {
        if page.snapshot_id.0 == 0
            || page.offset.0 != self.bytes.len() as u64
            || self.snapshot.is_some_and(|id| id != page.snapshot_id)
            || self.total.is_some_and(|total| total != page.total_bytes)
        {
            return Err(PageReadError("read page changed snapshot, offset or total"));
        }
        // decode_octets checks the complete declared bound before allocating
        // even one page. Full-result reservation occurs only after that check.
        let decoded = page
            .decode_octets(self.limit)
            .map_err(|_| PageReadError("invalid or over-limit read page"))?;
        if self.snapshot.is_none() {
            let total = usize::try_from(page.total_bytes.0)
                .map_err(|_| PageReadError("read result does not fit this host"))?;
            self.bytes
                .try_reserve_exact(total)
                .map_err(|_| PageReadError("cannot reserve bounded read result"))?;
            self.snapshot = Some(page.snapshot_id);
            self.total = Some(page.total_bytes);
        }
        self.bytes.extend_from_slice(&decoded);
        if !page.done {
            self.phase = Phase::Next;
            return Ok(());
        }
        let json = std::str::from_utf8(&self.bytes)
            .map_err(|_| PageReadError("complete read result is not UTF-8"))?;
        let result: QueryResult =
            decode_strict(json).map_err(|_| PageReadError("malformed complete read result"))?;
        super::read_validate::validate(&self.selector, &result)
            .map_err(|_| PageReadError("read result disagrees with its selector"))?;
        self.result = Some(result);
        self.phase = Phase::Release;
        Ok(())
    }

    pub(in crate::flow) fn take(&mut self) -> Option<Result<CompletedRead, ReadFailure>> {
        if self.phase != Phase::Complete {
            return None;
        }
        self.phase = Phase::Taken;
        if let Some(refusal) = self.refusal.take() {
            return Some(Err(refusal));
        }
        Some(Ok(CompletedRead {
            selector: self.selector.clone(),
            snapshot_id: self.snapshot.expect("complete released read"),
            result: self.result.take().expect("validated released result"),
            raw: std::mem::take(&mut self.bytes),
        }))
    }

    pub(in crate::flow) fn freeze(&mut self) {
        self.phase = Phase::Failed;
    }

    fn fail<T>(&mut self, detail: &'static str) -> Result<T, PageReadError> {
        self.phase = Phase::Failed;
        Err(PageReadError(detail))
    }
}

#[cfg(test)]
#[path = "read_tests.rs"]
mod tests;
