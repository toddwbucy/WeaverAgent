//! The seat a loop composes against, named once in the library so the
//! compiled loop (`dev_loop`) and the loop relay (`loop_relay`) are written
//! against one trait and one stand-in seat tests both
//! (`weaver-harness-loop-contract` section 2.2, the brief of 2026-11-25
//! item 4). Moved here unchanged from the worker's `dev_loop`.

use weaver_traits::Message;
use weaver_types::TurnContext;

use crate::engine::{Ports, TurnError, TurnOutcome};
use crate::state::{Recalled, SessionShape};

/// **The seat as a loop composes it**: the reads and asks a loop makes, named
/// once so a loop's own suite can drive it against a recording stand-in.
/// [`Ports`] is the one production implementation, each method its port of
/// the same name, and the trait mints nothing: a stand-in answers what a test
/// sets and holds no port, so the blade of `weaver-harness-Spec` section 8 is
/// untouched.
pub trait Seat {
    /// Whether no turn has run against this decode session: the assembly's
    /// message history is empty.
    fn first_turn(&self) -> bool;
    fn seated(&self) -> bool;
    fn turn_context(&self) -> TurnContext;
    fn fullness(&self) -> Option<(u64, u64)>;
    fn flush(&mut self, keep: u64) -> Option<(u64, u64)>;
    fn recall(&mut self, last_turns: Option<u64>) -> Option<Vec<Recalled>>;
    fn session_shape(&mut self) -> Option<SessionShape>;
    fn turn(&mut self, delta: Vec<Message>) -> Result<TurnOutcome, TurnError>;
}

impl Seat for Ports<'_> {
    fn first_turn(&self) -> bool {
        self.assembled()
            .is_none_or(|prompt| prompt.messages.is_empty())
    }
    fn seated(&self) -> bool {
        Ports::seated(self)
    }
    fn turn_context(&self) -> TurnContext {
        Ports::turn_context(self)
    }
    fn fullness(&self) -> Option<(u64, u64)> {
        Ports::fullness(self)
    }
    fn flush(&mut self, keep: u64) -> Option<(u64, u64)> {
        Ports::flush(self, keep)
    }
    fn recall(&mut self, last_turns: Option<u64>) -> Option<Vec<Recalled>> {
        Ports::recall(self, last_turns)
    }
    fn session_shape(&mut self) -> Option<SessionShape> {
        Ports::session_shape(self)
    }
    fn turn(&mut self, delta: Vec<Message>) -> Result<TurnOutcome, TurnError> {
        Ports::turn(self, delta)
    }
}
