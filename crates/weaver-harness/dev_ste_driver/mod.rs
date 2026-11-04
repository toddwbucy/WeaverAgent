//! External STE driver policy under ste-conversion-loop 4.05 through 7.07.
//!
//! These owners make no gate, model or state call. The external driver supplies
//! checked gate outcomes and keeps their evidence. The agent's flow primitives
//! do not choose retries, alter these counters or start another external turn.

use serde_json::{Value, json};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestKind {
    Initial,
    ShapeRetry,
    Repair,
}

/// Workflow 4.05, 6.03 and 6.08. No caller-supplied message role is needed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub ordinal: u64,
    pub kind: RequestKind,
    pub text: String,
}

/// Workflow 4.06, 6.01 and 6.03. The checker and gate adapter supply this data.
/// `response` retains the caller's complete response and identity evidence.
#[derive(Debug, Clone, PartialEq)]
pub enum AttemptResult {
    Checked {
        output: String,
        failures: Vec<String>,
        repair_notes: String,
        response: Value,
    },
    ShapeFault {
        output: Option<String>,
        failures: Vec<String>,
        response: Value,
    },
    TransportStopped {
        detail: String,
        response: Value,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Attempt {
    pub request: Request,
    pub result: AttemptResult,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Terminal {
    Accepted,
    ShapeDrop,
    LanguageUnresolved,
    TransportStopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConversionError {
    Finished,
    RequestPending,
    NoPendingRequest,
    WrongRequest,
    Unfinished,
    DuplicateId,
    UnknownId,
}

/// Workflow 6.03 and 6.07. A limit is explicit and includes the initial request.
/// The operator's R0 repair cap of 2 gives three total gate requests.
#[derive(Debug)]
pub struct TextConversion {
    masked_text: String,
    request_limit: u64,
    requests_used: u64,
    shape_faults: u8,
    pending: Option<Request>,
    attempts: Vec<Attempt>,
    terminal: Option<Terminal>,
}

impl TextConversion {
    pub fn new(masked_text: String, driver_repair_pass_cap: u32) -> Self {
        Self {
            masked_text,
            request_limit: u64::from(driver_repair_pass_cap) + 1,
            requests_used: 0,
            shape_faults: 0,
            pending: None,
            attempts: Vec::new(),
            terminal: None,
        }
    }

    pub fn request_limit(&self) -> u64 {
        self.request_limit
    }

    pub fn requests_used(&self) -> u64 {
        self.requests_used
    }

    pub fn shape_faults(&self) -> u8 {
        self.shape_faults
    }

    pub fn pending_request(&self) -> Option<&Request> {
        self.pending.as_ref()
    }

    pub fn attempts(&self) -> &[Attempt] {
        &self.attempts
    }

    pub fn terminal(&self) -> Option<Terminal> {
        self.terminal
    }

    /// Workflow 6.07: consume the allowance before the caller sends anything.
    /// There is no refund path. A failed send must be recorded as a stop.
    pub fn begin_request(&mut self) -> Result<Request, ConversionError> {
        if self.terminal.is_some() {
            return Err(ConversionError::Finished);
        }
        if self.pending.is_some() {
            return Err(ConversionError::RequestPending);
        }
        let mut text = initial_text(&self.masked_text);
        let kind = match self.attempts.last().map(|attempt| &attempt.result) {
            None => RequestKind::Initial,
            Some(AttemptResult::ShapeFault { .. }) => RequestKind::ShapeRetry,
            Some(AttemptResult::Checked {
                output,
                repair_notes,
                ..
            }) => {
                append_repair(&mut text, output, repair_notes);
                RequestKind::Repair
            }
            Some(AttemptResult::TransportStopped { .. }) => {
                return Err(ConversionError::Finished);
            }
        };
        // Every completed final request sets terminal in record_result.
        self.requests_used += 1;
        let request = Request {
            ordinal: self.requests_used,
            kind,
            text,
        };
        self.pending = Some(request.clone());
        Ok(request)
    }

    /// Workflow 6.03, 6.07 and 7.01: retain each outcome once, at its request.
    pub fn record_result(
        &mut self,
        ordinal: u64,
        result: AttemptResult,
    ) -> Result<(), ConversionError> {
        if self.terminal.is_some() {
            return Err(ConversionError::Finished);
        }
        let request = self
            .pending
            .as_ref()
            .ok_or(ConversionError::NoPendingRequest)?;
        if request.ordinal != ordinal {
            return Err(ConversionError::WrongRequest);
        }
        let result = match result {
            AttemptResult::Checked {
                output, response, ..
            } if output.trim().is_empty() => AttemptResult::ShapeFault {
                output: Some(output),
                failures: vec!["missing_output".into()],
                response,
            },
            other => other,
        };
        self.terminal = match &result {
            AttemptResult::TransportStopped { .. } => Some(Terminal::TransportStopped),
            AttemptResult::ShapeFault { .. } => {
                self.shape_faults += 1;
                if self.shape_faults == 3 || self.requests_used == self.request_limit {
                    Some(Terminal::ShapeDrop)
                } else {
                    None
                }
            }
            AttemptResult::Checked { failures, .. } if failures.is_empty() => {
                Some(Terminal::Accepted)
            }
            AttemptResult::Checked { .. } => {
                (self.requests_used == self.request_limit).then_some(Terminal::LanguageUnresolved)
            }
        };
        self.attempts.push(Attempt {
            request: self.pending.take().expect("pending request was checked"),
            result,
        });
        Ok(())
    }

    /// Workflow 6.03: keep the final completed output, including failed checks.
    /// A transport stop has no final output, even if an earlier attempt had one.
    pub fn final_output(&self) -> Option<&str> {
        match self.terminal? {
            Terminal::TransportStopped => None,
            Terminal::Accepted | Terminal::ShapeDrop | Terminal::LanguageUnresolved => {
                match &self.attempts.last()?.result {
                    AttemptResult::Checked { output, .. } => Some(output),
                    AttemptResult::ShapeFault { output, .. } => output.as_deref(),
                    AttemptResult::TransportStopped { .. } => None,
                }
            }
        }
    }

    /// Workflow 6.06. Passing mechanical checks is not a semantic verdict.
    pub fn assembly_eligible(&self) -> bool {
        self.terminal == Some(Terminal::Accepted)
    }

    /// Workflow 4.11 and 7.07, ste_score.py's id/output record at 9881da6.
    /// The caller must not score a void R0 evaluation.
    pub fn scorer_output(&self, id: &str) -> Option<Value> {
        self.final_output()
            .map(|output| json!({"id": id, "output": output}))
    }
}

/// Workflow 4.05: preserve all source bytes and add no final LF.
fn initial_text(masked_text: &str) -> String {
    format!("<<<TEXT\n{masked_text}\nTEXT>>>")
}

/// Workflow 6.08: notes are already produced by the pinned checker and guards.
fn append_repair(text: &mut String, last_rewrite: &str, notes: &str) {
    text.push_str("\n\n<<<PREVIOUS_REWRITE\n");
    text.push_str(last_rewrite);
    text.push_str("\nPREVIOUS_REWRITE>>>\n\nYour rewrite breaks these rules:\n");
    let notes = notes.trim();
    text.push_str(if notes.is_empty() {
        "- Obey the rules."
    } else {
        notes
    });
    text.push_str("\n\nWrite the complete rewrite again with these problems corrected.");
    text.push_str(" Keep the meaning. Output only the rewritten text.");
}

/// Workflow 7.04, 7.05 and 7.07. This is an R0 measurement, with no kill verdict.
#[derive(Debug, Clone, PartialEq)]
pub struct RoundReport {
    pub attempted_text_count: usize,
    pub accepted_count: usize,
    pub shape_drop_count: usize,
    pub language_unresolved_count: usize,
    pub transport_stopped_count: usize,
    pub conversion_loss_count: usize,
    pub conversion_loss_rate: Option<f64>,
    pub transport_stop_bound: u64,
    pub void: bool,
    pub never_attempted: Vec<String>,
}

impl RoundReport {
    /// No default bound. R0 supplies the operator's elected value of zero.
    pub fn collect(
        planned: &[&str],
        completed: &[(&str, &TextConversion)],
        transport_stop_bound: u64,
    ) -> Result<Self, ConversionError> {
        let planned_set: BTreeSet<_> = planned.iter().copied().collect();
        if planned_set.len() != planned.len() {
            return Err(ConversionError::DuplicateId);
        }
        let mut seen = BTreeSet::new();
        let mut report = Self {
            attempted_text_count: completed.len(),
            accepted_count: 0,
            shape_drop_count: 0,
            language_unresolved_count: 0,
            transport_stopped_count: 0,
            conversion_loss_count: 0,
            conversion_loss_rate: None,
            transport_stop_bound,
            void: false,
            never_attempted: Vec::new(),
        };
        for (id, text) in completed {
            if !planned_set.contains(id) {
                return Err(ConversionError::UnknownId);
            }
            if !seen.insert(*id) {
                return Err(ConversionError::DuplicateId);
            }
            match text.terminal.ok_or(ConversionError::Unfinished)? {
                Terminal::Accepted => report.accepted_count += 1,
                Terminal::ShapeDrop => report.shape_drop_count += 1,
                Terminal::LanguageUnresolved => report.language_unresolved_count += 1,
                Terminal::TransportStopped => report.transport_stopped_count += 1,
            }
        }
        report.never_attempted = planned
            .iter()
            .filter(|id| !seen.contains(**id))
            .map(|id| (*id).to_owned())
            .collect();
        report.conversion_loss_count = report.shape_drop_count + report.language_unresolved_count;
        if report.attempted_text_count != 0 {
            report.conversion_loss_rate =
                Some(report.conversion_loss_count as f64 / report.attempted_text_count as f64);
        }
        report.void = report.transport_stopped_count as u64 > transport_stop_bound;
        Ok(report)
    }

    /// Workflow 7.07: a void evaluation has counts and no R0 score.
    pub fn r0_score_allowed(&self) -> bool {
        !self.void && self.never_attempted.is_empty()
    }

    pub fn to_json(&self) -> Value {
        json!({
            "attempted_text_count": self.attempted_text_count,
            "accepted_count": self.accepted_count,
            "shape_drop_count": self.shape_drop_count,
            "language_unresolved_count": self.language_unresolved_count,
            "transport_stopped_count": self.transport_stopped_count,
            "conversion_loss_count": self.conversion_loss_count,
            "conversion_loss_rate": self.conversion_loss_rate,
            "transport_stop_bound": self.transport_stop_bound,
            "void": self.void,
            "never_attempted": self.never_attempted,
            "r0_score_allowed": self.r0_score_allowed(),
            "kill_line_verdict": Value::Null
        })
    }
}

#[cfg(test)]
mod tests;
