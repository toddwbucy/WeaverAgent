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
mod coordinator;
mod intake;
mod ledger;
mod mirror;
mod observation;
mod recording;

use recording::{RecordedStateMeasurement, RecordedSte100, RecordedTerminal};

#[cfg(test)]
mod member_tests;

#[cfg(test)]
mod recording_fixture;
