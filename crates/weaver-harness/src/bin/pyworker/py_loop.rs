//! conforms: harness-dev-boundary-in-the-filesystem
//!
//! The Python connector at the dev boundary, per issue #134: the crossing
//! stays the one named function, seat and parsed request in, response
//! content out, and this module marshals it into an embedded interpreter.
//!
//! **The blade survives by construction.** Python reaches only what the
//! [`Seat`] proxy forwards, the proxy forwards only what the granted
//! `Ports` surface offers, and the seat cannot be minted from here any more
//! than from `dev_loop`. A stashed proxy dies with the call: the pointer is
//! guarded by a liveness flag cleared before the crossing returns, so a
//! Python loop that keeps `seat` around meets a refusal, not a dangling
//! borrow.
//!
//! **The loop file is read and compiled on every turn.** That is the whole
//! point: edit the file, and the next turn runs the edit. Iterate at
//! conversation speed, then freeze the loop into a Rust composition root
//! for deployment - iterate fast, then freeze.

use std::collections::HashMap;
use std::ffi::CString;
use std::path::Path;

use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule};
use weaver_harness::{Ports, TurnError, TurnOutcome};
use weaver_traits::{ContentBlock, Message, Role};

/// The seat as Python sees it: six methods, each a thin forward of the
/// granted surface, and nothing else in either direction. The three
/// context ports arrived 2026-08-19 so the loop's context policy - the
/// trigger, the cut, and the re-entry - is iterable here at conversation
/// speed, the judgments being the loop's alone.
#[pyclass(unsendable)]
struct Seat {
    /// The granted surface, lifetime-erased for the length of one crossing.
    /// Valid exactly while `alive` holds, which is cleared before `drive`
    /// returns.
    ports: *mut Ports<'static>,
    alive: bool,
    /// The last turn this crossing ran, which becomes the crossing's own
    /// return: types stay on the Rust side and Python sees dictionaries.
    last: Option<Result<TurnOutcome, TurnError>>,
}

impl Seat {
    fn ports_mut(&mut self) -> PyResult<&mut Ports<'static>> {
        if !self.alive {
            return Err(PyRuntimeError::new_err(
                "the seat is lent for one turn: this crossing has returned",
            ));
        }
        // SAFETY: `alive` is cleared before the borrowed Ports goes out of
        // scope in `drive`, the proxy is unsendable so no other thread can
        // hold it, and the GIL serializes every method call.
        Ok(unsafe { &mut *self.ports })
    }
}

#[pymethods]
impl Seat {
    /// True where no turn has run against this decode session, which is
    /// the first-turn test the basic loop uses for its one-time injection.
    fn assembled_empty(&mut self) -> PyResult<bool> {
        let ports = self.ports_mut()?;
        Ok(ports.assembled().is_none_or(|p| p.messages.is_empty()))
    }

    /// The session's fullness as the last generation carried it: a
    /// (resident, capacity) pair, or None before any generation. Plain
    /// counts whose meaning is the loop's - when a flush is worth its
    /// cost is decided here, never below.
    fn fullness(&mut self) -> PyResult<Option<(u64, u64)>> {
        let ports = self.ports_mut()?;
        Ok(ports.fullness())
    }

    /// The flush: the decode context returns to the cut the loop names -
    /// `keep` resident tokens, bounded by the seam below at the identity
    /// prefix and above at the resident count, zero being the prefix-only
    /// state - and the record carries the event with both counts. Answers
    /// (resident_before, resident_after), or None where the seam refused
    /// or broke - and a None cannot prove the flush did not land, so a
    /// loop that elected one composes its re-entry either way.
    #[pyo3(signature = (keep=0))]
    fn flush(&mut self, keep: u64) -> PyResult<Option<(u64, u64)>> {
        let ports = self.ports_mut()?;
        Ok(ports.flush(keep))
    }

    /// The elision: a half-open span of resident positions leaves the
    /// session, `from` inclusive and `to` exclusive, answering the resident
    /// counts either side or None where the seam refused or the leg is
    /// down.
    ///
    /// **Which span to elide is this loop's election.** The port forwards
    /// what it is given unjudged, per `weaver-harness-Spec` section 6: the
    /// mechanic is the program's and what to keep is the operator's,
    /// written here.
    ///
    /// **Valid between turns only**, on the flush's ground: a span named
    /// against a resident sequence that is still growing names positions
    /// that will have moved by the time it lands.
    ///
    /// **A refusal answers None and elides nothing.** The span is refused
    /// when it overlaps the identity prefix, runs past the resident count,
    /// ends before it starts, or is empty.
    fn elide(&mut self, from: u64, to: u64) -> PyResult<Option<(u64, u64)>> {
        let ports = self.ports_mut()?;
        Ok(ports.elide(from, to))
    }

    /// The task's score, at the run's close: the predicate the task
    /// answered, whether it held, and where the task supplies a denominator,
    /// the count the run measured against it. Answers True once the record
    /// holds the verdict, and False where the harness refused it - inside a
    /// turn, a second score for the run, an empty predicate, one term
    /// without the other, or a zero denominator - or the recorder would not
    /// take it.
    ///
    /// **What the predicate is, and when a task is done, is this loop's.**
    /// The harness records the verdict and never judges it.
    #[pyo3(signature = (predicate, passed, measured=None, denominator=None))]
    fn score(
        &mut self,
        predicate: &str,
        passed: bool,
        measured: Option<u64>,
        denominator: Option<u64>,
    ) -> PyResult<bool> {
        let ports = self.ports_mut()?;
        Ok(ports.score(predicate, passed, measured, denominator))
    }

    /// Custody's recall: the conversation's message events in landing
    /// order, bounded to the most recent turns where a bound is given, or
    /// None where the leg is down. Each event is {"kind": str, "turn":
    /// str|None, "sequence": str, "pairs": {key: value}}, the pair values
    /// being the canonical JSON text custody kept.
    #[pyo3(signature = (last_turns=None))]
    fn recall(
        &mut self,
        py: Python<'_>,
        last_turns: Option<u64>,
    ) -> PyResult<Option<Vec<Py<PyDict>>>> {
        let ports = self.ports_mut()?;
        let Some(events) = ports.recall(last_turns) else {
            return Ok(None);
        };
        let mut recalled = Vec::with_capacity(events.len());
        for event in &events {
            let entry = PyDict::new(py);
            entry.set_item("kind", &event.kind)?;
            entry.set_item("turn", event.turn.as_deref())?;
            entry.set_item("sequence", &event.sequence)?;
            let pairs = PyDict::new(py);
            for (key, value) in &event.pairs {
                pairs.set_item(key, value)?;
            }
            entry.set_item("pairs", pairs)?;
            recalled.push(entry.unbind());
        }
        Ok(Some(recalled))
    }

    /// The classify port: content in, the artifact's scored labels back as
    /// a list of (label, score) tuples in the head's own order, or None
    /// where the leg is down, was never declared, refused typed, or
    /// answered malformed - the same absence a missing leg serves. The
    /// judgment over the scores is the loop's alone.
    fn classify(&mut self, content: &str) -> PyResult<Option<Vec<(String, f64)>>> {
        let ports = self.ports_mut()?;
        Ok(ports.classify(content))
    }

    /// The session's shape from the state member, or None where the leg is
    /// down or the answer missed: a list of runs in first-seen order, each
    /// {"run": str, "kinds": {kind: count}}.
    fn session_shape(&mut self, py: Python<'_>) -> PyResult<Option<Vec<Py<PyDict>>>> {
        let ports = self.ports_mut()?;
        let Some(shape) = ports.session_shape() else {
            return Ok(None);
        };
        let mut runs = Vec::with_capacity(shape.runs.len());
        for run in &shape.runs {
            let entry = PyDict::new(py);
            entry.set_item("run", &run.run)?;
            let kinds = PyDict::new(py);
            for (kind, count) in &run.kinds {
                kinds.set_item(kind, count)?;
            }
            entry.set_item("kinds", kinds)?;
            runs.push(entry.unbind());
        }
        Ok(Some(runs))
    }

    /// Run one turn with the given delta: a list of {"role": str, "text":
    /// str}, roles "system", "user", and "assistant" - the tool-result role
    /// is a grant, not a spelling, and has no door here. Answers the outcome as
    /// a dictionary, or raises with the refusal, either way recording the
    /// result as the crossing's return.
    fn turn(
        &mut self,
        py: Python<'_>,
        delta: Vec<HashMap<String, String>>,
    ) -> PyResult<Py<PyDict>> {
        let mut messages = Vec::with_capacity(delta.len());
        for entry in &delta {
            let role = match entry.get("role").map(String::as_str) {
                Some("system") => Role::System,
                Some("user") => Role::User,
                Some("assistant") => Role::Assistant,
                other => {
                    return Err(PyValueError::new_err(format!(
                        "a delta message's role is \"system\", \"user\", or \"assistant\", got {other:?}"
                    )));
                }
            };
            let Some(text) = entry.get("text") else {
                return Err(PyValueError::new_err("a delta message carries \"text\""));
            };
            messages.push(Message {
                role,
                content: vec![ContentBlock::Text { text: text.clone() }],
            });
        }
        let ports = self.ports_mut()?;
        match ports.turn(messages) {
            Ok(outcome) => {
                let answered = PyDict::new(py);
                answered.set_item("turn", &outcome.turn.0)?;
                answered.set_item("emission", &outcome.emission)?;
                answered.set_item("stopped", outcome.stopped)?;
                answered.set_item("aborted", outcome.aborted)?;
                self.last = Some(Ok(outcome));
                Ok(answered.unbind())
            }
            Err(error) => {
                let detail = format!("{error:?}");
                self.last = Some(Err(error));
                Err(PyRuntimeError::new_err(detail))
            }
        }
    }
}

/// The crossing: read the loop file, run its `drive(seat, text)` under the
/// GIL, and answer with the last turn the Python side ran. Every Python
/// failure is printed and survived: the fallback below runs a plain turn so
/// a broken loop file costs the injection and never the agent's answer,
/// which is the same economics every optional leg in this program carries.
pub fn drive(loop_path: &Path, seat: &mut Ports<'_>, text: &str) -> Result<TurnOutcome, TurnError> {
    let code = match std::fs::read_to_string(loop_path) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("pyworker: {} unreadable: {error}", loop_path.display());
            return fallback(seat, text);
        }
    };
    // SAFETY of the erasure: the pointer outlives every use, because `alive`
    // is cleared before this frame releases the borrow. The cast moves only
    // the lifetime, which clippy cannot see, hence the allow.
    #[allow(clippy::unnecessary_cast)]
    let erased = seat as *mut Ports<'_> as *mut Ports<'static>;
    let last = Python::attach(|py| {
        let proxy = match Bound::new(
            py,
            Seat {
                ports: erased,
                alive: true,
                last: None,
            },
        ) {
            Ok(proxy) => proxy,
            Err(error) => {
                error.print(py);
                return None;
            }
        };
        let source = match std::ffi::CString::new(code) {
            Ok(source) => source,
            Err(_) => {
                eprintln!("pyworker: the loop file contains a NUL byte");
                let mut held = proxy.borrow_mut();
                held.alive = false;
                return held.last.take();
            }
        };
        // **The compiled module is named for the file that was read**, per
        // the ruling of 2026-08-20 on issue #243 making the loop a member
        // of one agent's harness. The name was `dev_loop.py` for every
        // loop whatever the path, so a traceback from one agent's loop
        // named another's file, and with two arms running two files a
        // failure could not be attributed to an arm from the log alone.
        // Python holds both as C strings, so the conversion can fail on an
        // interior NUL and falls back to the old constant rather than
        // refusing the turn: a name is worth less than the crossing.
        let file_name = CString::new(loop_path.to_string_lossy().as_bytes())
            .unwrap_or_else(|_| c"dev_loop.py".to_owned());
        let module_name = CString::new(
            loop_path
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_else(|| "dev_loop".to_string()),
        )
        .unwrap_or_else(|_| c"dev_loop".to_owned());
        let outcome = PyModule::from_code(py, &source, &file_name, &module_name)
            .and_then(|module| module.getattr("drive"))
            .and_then(|entry| entry.call1((&proxy, text)));
        if let Err(error) = outcome {
            error.print(py);
        }
        let mut held = proxy.borrow_mut();
        held.alive = false;
        held.last.take()
    });
    match last {
        Some(result) => result,
        None => {
            eprintln!("pyworker: the loop ran no turn, answering with the plain fallback");
            fallback(seat, text)
        }
    }
}

/// The plain turn: the request as the one user message, nothing composed.
/// What a broken loop file costs is its own composition, never the answer.
fn fallback(seat: &mut Ports<'_>, text: &str) -> Result<TurnOutcome, TurnError> {
    seat.turn(vec![Message {
        role: Role::User,
        content: vec![ContentBlock::Text {
            text: text.to_string(),
        }],
    }])
}
