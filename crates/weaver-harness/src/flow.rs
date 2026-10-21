//! The replacement filing path's near-side mechanics.
//!
//! These components are deliberately dormant until the foundation cutover. They
//! own copies, one state socket, classifier results and trace completion proofs.
//! The serving lifecycle and its common pump will compose them without changing
//! the compiled loop's ownership or installing a second active writer.

mod channel;
mod classifier;
mod intake;
mod recording;

use recording::{RecordedStateMeasurement, RecordedSte100, RecordedTerminal};

#[cfg(test)]
mod member_tests;
