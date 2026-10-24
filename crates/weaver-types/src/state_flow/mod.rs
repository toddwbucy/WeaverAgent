//! Shared state-management vocabulary, per the merged state-management amendments.
//!
//! These additive contracts do not elect or activate the replacement serving path.
//! FOUNDATION-TRANSITION keeps the existing wire and deployment grammar in force
//! until the complete replacement passes its cutover review.

pub(crate) mod codec;
mod content;
mod declaration;
pub mod enter;
mod frame;
mod lifecycle_refusal;
mod marks;
mod observation;
mod pair;
mod persistence;
mod plan;
mod reads;
mod recall_refusal;
mod scalars;
mod selection;
mod ste100;

pub use codec::*;
pub use content::*;
pub use declaration::*;
pub use frame::*;
pub use lifecycle_refusal::*;
pub use marks::*;
pub use observation::*;
pub use pair::*;
pub use persistence::*;
pub use plan::*;
pub use reads::*;
pub use recall_refusal::*;
pub use scalars::{DecimalU64, Digest, MeasuredInterval, MeasuredWait, ValidationError};
pub use selection::*;
pub use ste100::*;

/// Deserialize a required field whose value may explicitly be null.
///
/// Attaching this helper without `default` makes absence an error, unlike the
/// implicit missing-field behavior of an ordinary `Option` field.
pub(crate) fn required_option<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    serde::Deserialize::deserialize(deserializer)
}
