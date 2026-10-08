//! conforms: state-custody-without-policy
//!
//! The custodian, per `weaver-state-PRD`: stores what it is handed,
//! organizes what it stores, serves what it is asked for, and does nothing
//! else. The management of state as it concerns context for the decoder
//! belongs to the harness and its control loops, and nothing in this crate
//! holds an opinion about why it was asked.
//!
//! Both halves stand: the ingest of 2026-08-18, and the serve of
//! 2026-08-19, shaped by its first asker per the charter's cell - the
//! context-injection loop, whose one ask is the session's shape.

pub mod engine;
pub mod save_point;
mod store;
pub mod typed;

pub use store::{
    Ask, CustodyFault, Distillate, Election, ImageFacts, RecalledEvent, Restored, RunShape,
    SavePointAnswer, Store, is_shape_ask, parse_ask, parse_distillate, render_finished_answer,
    render_grants_answer, render_identity_answer, render_recall_answer, render_replay_answer,
    render_restore_answer, render_restored_answer, render_shape_answer, render_snapshot_answer,
};
