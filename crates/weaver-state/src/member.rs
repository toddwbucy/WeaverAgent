//! Additive state-flow/1 member. Admin keeps the legacy launch until cutover.

use std::collections::BTreeMap;
use std::io::{self, Write};

use serde_json::value::RawValue;
use weaver_state::{ReadFault, engine::serving_sqlite::ServingSqlite, serving_bytes::BufferBytes};
use weaver_types::state_flow::*;

mod capture;
mod channel;
mod entry;
mod observation;
mod paging;
mod room;
mod seed;
#[cfg(test)]
mod tests;

use capture::Capture;
use channel::{Channel, invalid};
pub(crate) use entry::dispatch;
use observation::{OperationSample, monotonic_ns, sample_member_identity};
use paging::Pages;
use room::Room;

fn run(channel: &mut Channel, room: &Room, image: Vec<u8>) -> io::Result<()> {
    let Some(line) = channel.receive()? else {
        return Ok(());
    };
    let (mut sample, request) = sampled_request(&line, 1)?;
    let StateFrame::Open {
        common,
        schema_id,
        scheme_id,
        start,
        read_limits,
    } = request
    else {
        return Err(invalid("serving channel must begin with open"));
    };
    let adopted = if image.starts_with(b"SQLite format 3\0") {
        let observed = ServingSqlite::adopt_observed(
            &image,
            &common,
            &schema_id,
            &scheme_id,
            &start,
            read_limits.clone(),
        )
        .map_err(|e| invalid(e.to_string()))?;
        if let Some(phase) = observed.query {
            sample.phase_record(phase)?;
        }
        observed.result
    } else {
        Err(weaver_state::engine::serving_sqlite::OpenFault {
            code: StateRefusalCode::UnsupportedSchema,
            detail: "serving requires a raw state-content/1 SQLite image".into(),
        })
    };
    drop(image);
    let mut store = match adopted {
        Ok(store) => store,
        Err(error) => {
            let payload = Reply::Refused {
                code: error.code,
                detail: error.detail,
                block_id: None,
            };
            drop((schema_id, scheme_id, start, read_limits));
            sample.held = held_request_bytes(&line, channel, &common, None, None);
            let (observation, payload) = complete(payload, sample, None)?;
            // A rejected open has no adopted run in which to retain metadata.
            channel.send(wrap(&common, "refused", &payload, &observation)?)?;
            return Ok(());
        }
    };
    let identity = sample_member_identity();
    let payload = Reply::Opened {
        schema_id,
        scheme_id,
        start: Box::new(start),
        read_limits: read_limits.clone(),
        member_observation: identity.clone(),
    };
    sample.held = held_request_bytes(&line, channel, &common, None, None);
    // This independent identity is retained for the metadata transaction below.
    sample.held.add(identity.instrument_version.len());
    // read_limits moves into Session configuration after Open and is excluded
    // consistently here and on later operations; its encoded echo still counts.
    let (observation, payload) = complete(payload, sample, None)?;
    store
        .begin_observations(&identity, &observation)
        .map_err(|e| invalid(e.to_string()))?;
    channel.send(wrap(&common, "opened", &payload, &observation)?)?;
    drop((line, payload, identity, observation));
    let mut session = Session {
        store,
        common,
        last_request: 1,
        last_operation: 1,
        pages: Pages::default(),
        capture: Capture::default(),
        read_limits,
    };
    while let Some(line) = channel.receive()? {
        let operation_id = session
            .last_operation
            .checked_add(1)
            .ok_or_else(|| invalid("operation counter exhausted"))?;
        let (mut sample, request) = sampled_request(&line, operation_id)?;
        session.check_request(&request)?;
        let common = request.common().clone();
        let source = match &request {
            StateFrame::File { filing, .. } => Some(filing.block_id.clone()),
            StateFrame::FilePair { filing, .. } => Some(filing.block_id.clone()),
            _ => None,
        };
        let (payload, initial_read) = session.dispatch(request, room, &mut sample)?;
        sample.held = held_request_bytes(
            &line,
            channel,
            &common,
            source.as_ref(),
            Some(&session.capture),
        );
        let op = payload.op();
        let close = matches!(payload, Reply::Refused { .. });
        let (observation, encoded) = complete(
            payload,
            sample,
            Some(CompletionBounds {
                common: &common,
                pages: &mut session.pages,
                initial_read,
            }),
        )?;
        #[cfg(test)]
        tests::process::before_record_observation(
            &mut session.store,
            source.as_ref(),
            &observation,
        )?;
        session
            .store
            .record_observation(&StoredOperationObservation {
                request_source: source,
                observation: observation.clone(),
            })
            .map_err(|e| invalid(e.to_string()))?;
        let bytes = wrap(&common, op, &encoded, &observation)?;
        session.pages.publish_page();
        session.last_operation = operation_id;
        channel.send(bytes)?;
        if close {
            return Ok(());
        }
    }
    Ok(())
}

/// Live operation owners, excluding fixed channel/store configuration and SQLite.
/// The request has been consumed by dispatch; only its line/common/source survive.
/// Pages and the final answer are added by complete after any refusal cleanup.
fn held_request_bytes(
    line: &[u8],
    channel: &Channel,
    common: &Common,
    source: Option<&SourceBlockId>,
    capture: Option<&Capture>,
) -> BufferBytes {
    let mut held = BufferBytes::default();
    held.add(line.len());
    held.add(channel.buffered_bytes());
    held.add(common.session_id.len());
    held.add(common.run_id.len());
    if let Some(source) = source {
        held.add(source.run.len());
    }
    if let Some(capture) = capture {
        held.merge(capture.held_bytes());
    }
    held
}

struct Session {
    store: ServingSqlite,
    common: Common,
    last_request: u64,
    last_operation: u64,
    pages: Pages,
    capture: Capture,
    read_limits: ReadLimits,
}

impl Session {
    fn check_request(&mut self, request: &StateFrame) -> io::Result<()> {
        let common = request.common();
        if common.session_id != self.common.session_id
            || common.run_id != self.common.run_id
            || matches!(request, StateFrame::Open { .. })
        {
            return Err(invalid("request is not in the opened channel"));
        }
        // A held capture keeps its original continuation after fresh controls
        // have been refused. Only a just-refused capture retires on a fresh ask.
        let continuation = self.capture.continuation() == Some(common.request_id)
            && matches!(
                request,
                StateFrame::Control {frame,..} if self.capture.allows_continuation(frame)
            );
        if continuation {
            return Ok(());
        }
        let next = self
            .last_request
            .checked_add(1)
            .ok_or_else(|| invalid("request counter exhausted"))?;
        if common.request_id.0 != next {
            return Err(invalid(
                "request ID is not the next exchange or its continuation",
            ));
        }
        self.last_request = next;
        self.capture.fresh();
        Ok(())
    }

    fn dispatch(
        &mut self,
        request: StateFrame,
        room: &Room,
        sample: &mut OperationSample,
    ) -> io::Result<(Reply, bool)> {
        match request {
            StateFrame::File { filing, .. } => {
                let result = if self.capture.held() {
                    Err(weaver_state::FileFault {
                        code: StateRefusalCode::WrongPhase,
                        detail: "capture holds the store".into(),
                    })
                } else {
                    let observed = self
                        .store
                        .file_observed(&filing)
                        .map_err(|e| invalid(e.to_string()))?;
                    if let Some(phase) = observed.transaction {
                        sample.phase_record(phase)?;
                    }
                    observed.result
                };
                Ok((filing_payload(result, filing.block_id), false))
            }
            StateFrame::FilePair { filing, .. } => {
                let result = if self.capture.held() {
                    Err(weaver_state::FileFault {
                        code: StateRefusalCode::WrongPhase,
                        detail: "capture holds the store".into(),
                    })
                } else {
                    let observed = self
                        .store
                        .file_pair_observed(&filing)
                        .map_err(|e| invalid(e.to_string()))?;
                    if let Some(phase) = observed.transaction {
                        sample.phase_record(phase)?;
                    }
                    observed.result
                };
                Ok((filing_payload(result, filing.block_id), false))
            }
            StateFrame::Control { common, frame } => {
                let initial_read = matches!(frame, StateControl::Read { .. }) && !self.pages.busy();
                let answer = self.control(&common, &frame, room, sample)?;
                if let StateControlAnswer::ReadPage(page) = &answer {
                    sample.copied.add(page.octets.len());
                }
                Ok((Reply::Control(answer), initial_read))
            }
            _ => Err(invalid("unexpected serving response or open")),
        }
    }

    fn control(
        &mut self,
        common: &Common,
        control: &StateControl,
        room: &Room,
        sample: &mut OperationSample,
    ) -> io::Result<StateControlAnswer> {
        if self.capture.held()
            || matches!(
                control,
                StateControl::Acknowledge { .. }
                    | StateControl::Abort { .. }
                    | StateControl::ReleaseCapture { .. }
            )
        {
            return self
                .capture
                .control(room, common.request_id, control, sample);
        }
        match control {
            StateControl::Snapshot { capture_id } => {
                self.capture
                    .snapshot(&self.store, room, common.request_id, *capture_id, sample)
            }
            StateControl::Read { selector } => {
                if self.pages.busy() {
                    return Ok(read_refusal(ReadRefusalCode::ReadBusy));
                }
                let page_budget = page_budget(common)?;
                sample.phase_begin(PhaseName::Query)?;
                // Read still lacks decoded-content copy coverage. Skip its
                // preparatory encoding subtotal; copied remains unavailable.
                let result = self.store.query_encoded(selector, None);
                sample.phase_end(PhaseName::Query)?;
                match result {
                    Ok(bytes) => Ok(
                        match self.pages.start(
                            bytes,
                            self.read_limits.max_result_bytes,
                            page_budget,
                        ) {
                            Ok(page) => StateControlAnswer::ReadPage(page),
                            Err(code) => read_refusal(code),
                        },
                    ),
                    Err(error) => read_fault(error),
                }
            }
            StateControl::Next {
                snapshot_id,
                offset,
            } => {
                let page_budget = page_budget(common)?;
                Ok(match self.pages.next(*snapshot_id, *offset, page_budget) {
                    Ok(page) => StateControlAnswer::ReadPage(page),
                    Err(code) => read_refusal(code),
                })
            }
            StateControl::Release { snapshot_id } => Ok(match self.pages.release(*snapshot_id) {
                Ok(()) => StateControlAnswer::Released {
                    snapshot_id: *snapshot_id,
                },
                Err(code) => read_refusal(code),
            }),
            StateControl::Grants {} => {
                sample.phase_begin(PhaseName::Filesystem)?;
                let surface = room.surface()?;
                sample.phase_end(PhaseName::Filesystem)?;
                Ok(StateControlAnswer::Grants { surface })
            }
            StateControl::Shape {} | StateControl::Held {} | StateControl::Position {} => {
                sample.phase_begin(PhaseName::Query)?;
                let result = match control {
                    StateControl::Shape {} => self.store.shape(None).map(StateControlAnswer::Shape),
                    StateControl::Held {} => self
                        .store
                        .held()
                        .map(|count| StateControlAnswer::Held { count }),
                    StateControl::Position {} => {
                        self.store.position().map(StateControlAnswer::Position)
                    }
                    _ => unreachable!(),
                };
                sample.phase_end(PhaseName::Query)?;
                match result {
                    Ok(answer) => Ok(answer),
                    Err(error) => read_fault(error),
                }
            }
            _ => unreachable!("capture controls handled before ordinary reads"),
        }
    }
}

fn filing_payload(
    result: Result<weaver_state::FileCommit, weaver_state::FileFault>,
    block_id: SourceBlockId,
) -> Reply {
    match result {
        Ok(commit) => Reply::Filed {
            block_id: commit.block_id,
        },
        Err(error) => Reply::Refused {
            code: error.code,
            detail: error.detail,
            block_id: Some(block_id),
        },
    }
}

fn read_refusal(code: ReadRefusalCode) -> StateControlAnswer {
    StateControlAnswer::ControlRefused {
        code: ControlRefusalCode::Read(code),
        detail: format!("read refused: {code:?}"),
    }
}
fn read_fault(error: ReadFault) -> io::Result<StateControlAnswer> {
    match error {
        ReadFault::Refused(code) => Ok(read_refusal(code)),
        other => Err(invalid(other.to_string())),
    }
}

fn sampled_request(line: &[u8], operation_id: u64) -> io::Result<(OperationSample, StateFrame)> {
    let received_ns = monotonic_ns()?;
    let (request_id, operation) = peek_operation(line)?;
    // Every operation retains the clock from before the size-dependent hint
    // parse. Snapshot counters begin later, after validation and preflight.
    let mut sample =
        OperationSample::begin_at(operation_id.into(), request_id, operation, received_ns)?;
    sample.phase_begin(PhaseName::Validate)?;
    let decoded = decode_state_line_observed(line, FrameDirection::Request)
        .map_err(|e| invalid(e.to_string()))?;
    sample.copied.merge(match decoded.raw_field_bytes {
        Some(bytes) => BufferBytes::measured(bytes),
        None => BufferBytes::unavailable(ObservationUnavailable::Overflow),
    });
    let request = decoded.frame;
    if request.request_operation() != Some(operation) {
        return Err(invalid("operation peek mismatch"));
    }
    sample.phase_end(PhaseName::Validate)?;
    Ok((sample, request))
}

/// This hint labels the operation while preserving its pre-parse timestamp. The
/// strict floor decoder remains the authority for request shape and identity;
/// snapshot resource sampling starts later, after capture preflight.
fn peek_operation(line: &[u8]) -> io::Result<(DecimalU64, StateOperation)> {
    #[cfg(test)]
    tests::timing::before_peek();
    let fields: BTreeMap<String, &RawValue> =
        serde_json::from_slice(line).map_err(|e| invalid(e.to_string()))?;
    let request = fields
        .get("request_id")
        .ok_or_else(|| invalid("missing request ID"))?;
    let request =
        serde_json::from_str(request.get()).map_err(|e| invalid(format!("request ID: {e}")))?;
    let op: String =
        serde_json::from_str(fields.get("op").ok_or_else(|| invalid("missing op"))?.get())
            .map_err(|e| invalid(e.to_string()))?;
    let op = match op.as_str() {
        "open" => StateOperation::Open,
        "file" => StateOperation::File,
        "file_pair" => StateOperation::FilePair,
        "control" => {
            let fields: BTreeMap<String, &RawValue> = serde_json::from_str(
                fields
                    .get("frame")
                    .ok_or_else(|| invalid("missing control"))?
                    .get(),
            )
            .map_err(|e| invalid(e.to_string()))?;
            if fields.len() != 1 {
                return Err(invalid("ambiguous control hint"));
            }
            match fields.keys().next().map(String::as_str) {
                Some("read") => StateOperation::Read,
                Some("next") => StateOperation::Next,
                Some("release") => StateOperation::Release,
                Some("shape") => StateOperation::Shape,
                Some("held") => StateOperation::Held,
                Some("position") => StateOperation::Position,
                Some("grants") => StateOperation::Grants,
                Some("snapshot") => StateOperation::Snapshot,
                Some("acknowledge") => StateOperation::Acknowledge,
                Some("abort") => StateOperation::Abort,
                Some("release_capture") => StateOperation::ReleaseCapture,
                _ => return Err(invalid("unknown operation hint")),
            }
        }
        _ => return Err(invalid("unexpected operation")),
    };
    Ok((request, op))
}

struct CompletionBounds<'a> {
    common: &'a Common,
    pages: &'a mut Pages,
    initial_read: bool,
}

/// All serving replies share this payload-encoding and observation boundary.
/// Open has no page state; later replies retain the complete-answer fallback.
fn complete(
    mut payload: Reply,
    mut sample: OperationSample,
    bounds: Option<CompletionBounds<'_>>,
) -> io::Result<(StateOperationObservation, Vec<u8>)> {
    sample.phase_begin(PhaseName::Encode)?;
    let mut encoded = encode_payload(&payload, Some(&mut sample.copied));
    if let Some(bounds) = bounds {
        let reserve = wrapper_reserve(bounds.common, payload.op())?;
        if payload.op() == "control_answer"
            && encoded.as_ref().map_or(true, |bytes| {
                bytes.len().saturating_add(reserve) > STATE_ANSWER_LINE_BOUND
            })
        {
            bounds.pages.refuse_page(bounds.initial_read);
            payload = Reply::Control(StateControlAnswer::ControlRefused {
                code: ControlRefusalCode::Read(ReadRefusalCode::AnswerTooLarge),
                detail: "complete control answer exceeds ANSWER_BOUND".into(),
            });
            encoded = encode_payload(&payload, Some(&mut sample.copied));
        }
        sample.held.add(bounds.pages.held_bytes());
    }
    let encoded = encoded?;
    sample.phase_end(PhaseName::Encode)?;
    // Its consumed typed predecessor is no longer needed. The encoded payload
    // remains owned through ended_ns; wrapper/metadata and send happen later.
    drop(payload);
    sample.held.add(encoded.len());
    let observation = sample.finish()?;
    Ok((observation, encoded))
}

// Sizing materializations are wrapper metadata, not operation result/page
// payload copies. Keep them outside the observation/2 payload-byte tally.
fn page_budget(common: &Common) -> io::Result<usize> {
    let shape = StateControlAnswer::ReadPage(ReadPage {
        snapshot_id: u64::MAX.into(),
        offset: u64::MAX.into(),
        total_bytes: u64::MAX.into(),
        octets: String::new(),
        done: false,
    });
    let fixed = encode_payload(&Reply::Control(shape), None)?.len();
    Ok(STATE_ANSWER_LINE_BOUND
        .saturating_sub(wrapper_reserve(common, "control_answer")?.saturating_add(fixed)))
}

struct Bounded<'a> {
    bytes: Vec<u8>,
    copied: Option<&'a mut BufferBytes>,
}
impl Write for Bounded<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > STATE_ANSWER_LINE_BOUND.saturating_sub(self.bytes.len()) {
            return Err(invalid("answer exceeds bound"));
        }
        self.bytes
            .try_reserve(bytes.len())
            .map_err(io::Error::other)?;
        self.bytes.extend_from_slice(bytes);
        if let Some(copied) = &mut self.copied {
            copied.add(bytes.len());
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
/// Only the operation payload, encoded before sampling ends. The wrapper adds
/// observation afterwards without rebuilding or reordering the typed interior.
enum Reply {
    Opened {
        schema_id: String,
        scheme_id: String,
        start: Box<StartIdentity>,
        read_limits: ReadLimits,
        member_observation: MemberObservationIdentity,
    },
    Filed {
        block_id: SourceBlockId,
    },
    Refused {
        code: StateRefusalCode,
        detail: String,
        block_id: Option<SourceBlockId>,
    },
    Control(StateControlAnswer),
}
impl Reply {
    fn op(&self) -> &'static str {
        match self {
            Self::Opened { .. } => "opened",
            Self::Filed { .. } => "filed",
            Self::Refused { .. } => "refused",
            Self::Control(_) => "control_answer",
        }
    }
}

// Existing floor types implement Serialize. Keeping each field typed preserves
// their declared field order and RawValue representation, without a Value map.
macro_rules! fields {
    ($writer:ident, $first:literal => $value:expr $(, $key:literal => $next:expr)* $(,)?) => {{
        $writer.write_all(concat!("{\"",$first,"\":").as_bytes())?;
        serde_json::to_writer(&mut $writer,$value).map_err(|e| invalid(e.to_string()))?;
        $(
            $writer.write_all(concat!(",\"",$key,"\":").as_bytes())?;
            serde_json::to_writer(&mut $writer,$next).map_err(|e| invalid(e.to_string()))?;
        )*
        $writer.write_all(b"}")?;
    }};
}

fn encode_payload(payload: &Reply, copied: Option<&mut BufferBytes>) -> io::Result<Vec<u8>> {
    #[cfg(test)]
    tests::timing::mark("encode");
    let mut writer = Bounded {
        bytes: Vec::new(),
        copied,
    };
    let result = (|| -> io::Result<()> {
        match payload {
            Reply::Opened {
                schema_id,
                scheme_id,
                start,
                read_limits,
                member_observation,
            } => {
                fields!(writer,"schema_id"=>schema_id,"scheme_id"=>scheme_id,"start"=>start,"read_limits"=>read_limits,"member_observation"=>member_observation)
            }
            Reply::Filed { block_id } => fields!(writer,"block_id"=>block_id),
            Reply::Refused {
                code,
                detail,
                block_id,
            } => fields!(writer,"code"=>code,"detail"=>detail,"block_id"=>block_id),
            Reply::Control(frame) => fields!(writer,"frame"=>frame),
        }
        Ok(())
    })();
    result?;
    #[cfg(test)]
    tests::timing::mark("payload_encoded");
    Ok(writer.bytes)
}
fn header(common: &Common, op: &str) -> io::Result<Vec<u8>> {
    let mut writer = Bounded {
        bytes: Vec::new(),
        copied: None,
    };
    fields!(writer,"protocol"=>STATE_FLOW_PROTOCOL,"session_id"=>&common.session_id,"run_id"=>&common.run_id,"request_id"=>&common.request_id,"op"=>op);
    Ok(writer.bytes)
}
fn wrapper_reserve(common: &Common, op: &str) -> io::Result<usize> {
    // Header's final brace is exchanged for a comma, then the payload's interior,
    // observation key/value and final object plus LF. This is an upper bound.
    let bytes = header(common, op)?;
    Ok(bytes.len() + StateOperationObservation::MAX_ENCODED_LEN + b",\"observation\":\n".len() + 2)
}
fn wrap(
    common: &Common,
    op: &str,
    payload: &[u8],
    observation: &StateOperationObservation,
) -> io::Result<Vec<u8>> {
    let mut writer = Bounded {
        bytes: header(common, op)?,
        copied: None,
    };
    writer.bytes.pop();
    writer.write_all(b",")?;
    writer.write_all(&payload[1..payload.len() - 1])?;
    writer.write_all(b",\"observation\":")?;
    serde_json::to_writer(&mut writer, observation).map_err(|e| invalid(e.to_string()))?;
    writer.write_all(b"}\n")?;
    Ok(writer.bytes)
}
