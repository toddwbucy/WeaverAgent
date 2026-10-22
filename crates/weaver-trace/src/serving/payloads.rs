//! Serialize-only serving payloads under the state-management trace amendment.
//!
//! These mirrors carry facts supplied by the harness. They hold no classifier,
//! store, placement or review authority, and link no other Weaver crate.
//!
//! ```compile_fail
//! fn reads<T: for<'a> serde::Deserialize<'a>>() {}
//! reads::<weaver_trace::serving::payloads::PlanEffect>();
//! ```
//!
//! ```compile_fail,E0277
//! fn reads<T: for<'a> serde::Deserialize<'a>>() {}
//! reads::<weaver_trace::serving::payloads::RecallRecord>();
//! ```

use serde::{Serialize, Serializer};
use serde_json::value::RawValue;

mod classifier;
mod declaration;
mod identity;
mod observation;
mod pair;
mod persistence;
mod plan;
mod recall;
mod records;
mod selection;

pub use classifier::*;
pub use declaration::*;
pub use identity::*;
pub use observation::*;
pub use pair::*;
pub use persistence::*;
pub use plan::*;
pub use recall::*;
pub use records::*;
pub use selection::*;

/// A shared-wire counter. Token facts explicitly retain their numeric u64 type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct DecimalU64(pub u64);
impl Serialize for DecimalU64 {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0.to_string())
    }
}
impl From<u64> for DecimalU64 {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

/// The harness converts a verified digest without making the recorder a verifier.
pub type Digest = String;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MeasuredWait {
    pub elapsed_ns: DecimalU64,
    pub measurement_ref: String,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MeasuredInterval {
    pub elapsed_ns: DecimalU64,
    pub measurement_ref: String,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ModelBinding {
    pub artifact: String,
    pub devices: Vec<u32>,
}

#[cfg(test)]
pub(crate) mod tests;
