//! conforms: trace-one-line-per-event
//! conforms: trace-large-integers-as-decimal-strings
//! conforms: trace-render-deterministic
//!
//! Canonical form, per `weaver-trace-Spec` section 2: one rule, used
//! everywhere. An event renders to exactly one line of UTF-8 JSON with no
//! interior newline, the terminating newline is the record separator, field
//! order is declaration order, and the renderer is deterministic - the same
//! event renders to the same bytes on every run of every build.

use std::sync::Arc;

use serde::{Serialize, Serializer};

use crate::event::{Event, Line};
use crate::failure::{Failure, SubmitRefusal};

/// The run-scoped event ordinal: strictly increasing, gapless over admitted
/// events, starting at zero. The sequence is the order and the monotonic
/// reading is the instrument, and the two are never read for each other's job.
///
/// Serializes as a decimal string, because a consumer parsing JSON numbers as
/// doubles gets a silently different value past 2^53 with no error and no way
/// back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Sequence(pub u64);

impl Serialize for Sequence {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0.to_string())
    }
}

/// A monotonic reading in nanoseconds, serialized as a decimal string under
/// the same rule: the reading exceeds the double-safe range within weeks of
/// uptime. The wall-clock stamp in milliseconds stays a bare number, being far
/// below the range where the loss begins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct MonotonicNs(pub u64);

impl Serialize for MonotonicNs {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0.to_string())
    }
}

/// Renders an event to its one canonical line, newline-terminated. serde_json
/// escapes an embedded newline in a string to `\n`, so prose cannot split an
/// event - but a spliced `RawValue` carries its bytes verbatim, and
/// pretty-printed JSON holds real newlines between tokens, which are valid
/// JSON and fatal to the framing. The check below is the mechanism: a body
/// carrying an interior newline or carriage return refuses as malformed
/// rather than splitting the stream.
pub(crate) fn render(event: &Event) -> Result<Line, Failure> {
    let body = serde_json::to_string(event).map_err(|_| Failure::RefusedOnSubmit {
        reason: SubmitRefusal::PayloadMalformed,
    })?;
    if body.contains('\n') || body.contains('\r') {
        return Err(Failure::RefusedOnSubmit {
            reason: SubmitRefusal::PayloadMalformed,
        });
    }
    let mut line = String::with_capacity(body.len() + 1);
    line.push_str(&body);
    line.push('\n');
    Ok(Arc::from(line))
}
