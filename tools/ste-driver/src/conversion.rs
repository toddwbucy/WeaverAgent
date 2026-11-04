//! Workflow 4.06, 6.01 through 6.08 and 7.07. Join checks to request accounting.

use crate::check::CheckResult;
use crate::gate::Capture;
use crate::guard::Judge;
use crate::literal::LiteralCheck;
use crate::policy::{Attempt, AttemptResult, Request, RequestKind, Terminal, TextConversion};
use crate::python_re::whitespace;
use serde_json::{Value, json};
use std::collections::BTreeSet;

/// Retain the full checked result in the policy owner's response evidence.
pub fn check_answer(
    capture: Capture,
    judge: &Judge,
    source: &str,
) -> Result<AttemptResult, fancy_regex::Error> {
    let mut response = json!({"transport":capture.to_json(), "token_use":null,
        "recorded_generation_count":null, "trace_complete":false});
    let answer = match capture.answer() {
        Ok(answer) => answer,
        Err(detail) => return Ok(AttemptResult::TransportStopped { detail, response }),
    };
    response["run"] = json!(answer.run);
    response["turn"] = json!(answer.turn);
    response["finish"] = json!(if answer.truncated { "length" } else { "end" });
    let empty = answer
        .text
        .as_deref()
        .is_none_or(|text| text.trim_matches(whitespace).is_empty());
    let mut check = judge.check(answer.text.as_deref(), source, &BTreeSet::new())?;
    if empty {
        check.hard.insert("missing_output", vec![json!(true)]);
    }
    if answer.truncated {
        check.hard.insert("truncated", vec![json!("length")]);
    }
    response["check"] = check.to_json();
    let failures = check.hard.keys().map(|key| (*key).to_owned()).collect();
    if empty
        || judge
            .checker
            .lost_placeholder(answer.text.as_deref().unwrap_or(""), source)?
    {
        return Ok(AttemptResult::ShapeFault {
            output: answer.text,
            failures,
            response,
        });
    }
    Ok(AttemptResult::Checked {
        output: answer.text.expect("the completed answer is nonempty"),
        failures,
        repair_notes: judge.checker.repair_notes_with_guards(&check),
        response,
    })
}

pub fn request_json(request: &Request) -> Value {
    json!({"ordinal":request.ordinal, "kind":match request.kind {
        RequestKind::Initial => "initial",
        RequestKind::ShapeRetry => "shape_retry",
        RequestKind::Repair => "repair",
    }, "text":request.text})
}

pub fn attempt_json(attempt: &Attempt) -> Value {
    let mut record = request_json(&attempt.request);
    let result = match &attempt.result {
        AttemptResult::Checked {
            output,
            failures,
            repair_notes,
            response,
        } => {
            json!({"class":"checked", "output":output, "failures":failures,
                "repair_notes":repair_notes, "response":response})
        }
        AttemptResult::ShapeFault {
            output,
            failures,
            response,
        } => {
            json!({"class":"shape_fault", "output":output, "failures":failures,
                "response":response})
        }
        AttemptResult::TransportStopped { detail, response } => {
            json!({"class":"transport_stopped", "detail":detail, "response":response})
        }
    };
    record["result"] = result;
    record
}

/// Literal checks belong at assembly. They do not consume another request.
pub fn result_json(
    id: &str,
    source: &str,
    conversion: &TextConversion,
    literals: &LiteralCheck,
) -> Result<Value, fancy_regex::Error> {
    let changed = literals.altered(conversion.final_output(), source)?;
    Ok(json!({
        "id":id, "output":conversion.final_output(),
        "terminal":conversion.terminal().map(|terminal| match terminal {
            Terminal::Accepted => "accepted",
            Terminal::ShapeDrop => "shape_drop",
            Terminal::LanguageUnresolved => "language_unresolved",
            Terminal::TransportStopped => "transport_stopped",
        }),
        "requests_used":conversion.requests_used(),
        "request_limit":conversion.request_limit(),
        "shape_faults":conversion.shape_faults(),
        "assembly_eligible":conversion.assembly_eligible() && changed.is_empty(),
        "altered_literals":changed,
        "attempts":conversion.attempts().iter().map(attempt_json).collect::<Vec<_>>()
    }))
}

/// Preserve the check shape for a local checker failure in the run journal.
pub fn checker_failure(error: &fancy_regex::Error) -> Value {
    let mut result = CheckResult::default();
    result
        .hard
        .insert("checker_error", vec![json!(error.to_string())]);
    result.to_json()
}
