//! The replacement filing path's near-side mechanics.
//!
//! These components are deliberately dormant until the foundation cutover. They
//! own copies, one state socket, classifier results and trace completion proofs.
//! The serving lifecycle and its common pump will compose them without changing
//! the compiled loop's ownership or installing a second active writer.

mod author;
mod channel;
mod classifier;
mod clock;
pub(crate) mod context;
mod coordinator;
mod halt_snapshot;
mod intake;
mod ledger;
mod mirror;
mod observation;
mod pump;
mod pump_transport;
mod record_owner;
mod recording;
mod report;
mod ste100_channel;
mod ste100_socket;

use recording::{RecordedStateMeasurement, RecordedSte100, RecordedTerminal};

#[cfg(test)]
mod member_tests;

#[cfg(test)]
mod recording_fixture;

#[cfg(test)]
mod ste100_integration_tests;

#[cfg(test)]
mod pump_transport_integration_tests;
