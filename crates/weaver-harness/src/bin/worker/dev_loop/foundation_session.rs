//! Reuse the same serving seat across explicit worker turns. Completed world
//! replies retire their turn owner; accepted input still needs a loop election.
use super::{turn, *};
use weaver_harness::{FoundationManagement, FoundationManagementProgress, FoundationRestatement};
use weaver_types::state_flow::CutMethod;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Progress {
    Pending,
    Accepted,
    Completed,
    Refused,
    Aborted,
    DirectivePending,
    Stopped,
}

enum State<'a> {
    Input(Box<FoundationInput<'a>>),
    Management(Box<FoundationManagement<'a>>),
    Turn(turn::Workflow<'a>),
}

/// Management between turns and during intake keeps the same resources as the
/// running turn. Unsupported directives and failed owners remain available to
/// the lifecycle caller; no session advance claims to have torn them down.
pub struct Session<'a> {
    state: Option<State<'a>>,
    family: &'static FoundationRestatement,
    method: CutMethod,
}

impl<'a> Session<'a> {
    pub fn begin(
        serving: FoundationServing<'a>,
        family: &'static FoundationRestatement,
        method: CutMethod,
    ) -> Self {
        Self {
            state: Some(State::Input(serving.input())),
            family,
            method,
        }
    }

    pub fn progress(&self) -> Progress {
        match self.state.as_ref().unwrap() {
            State::Input(input) => match input.progress() {
                FoundationInputProgress::Accepted => Progress::Accepted,
                FoundationInputProgress::Completed => Progress::Completed,
                FoundationInputProgress::Refused => Progress::Refused,
                FoundationInputProgress::Aborted => Progress::Aborted,
                FoundationInputProgress::Stopped => Progress::Stopped,
                _ => Progress::Pending,
            },
            State::Management(owner) => match owner.progress() {
                FoundationManagementProgress::DirectivePending => Progress::DirectivePending,
                FoundationManagementProgress::Stopped => Progress::Stopped,
                _ => Progress::Pending,
            },
            State::Turn(flow) => match flow.progress() {
                turn::Progress::Pending => Progress::Pending,
                turn::Progress::Completed => Progress::Completed,
                turn::Progress::Refused => Progress::Refused,
                turn::Progress::Aborted => Progress::Aborted,
                turn::Progress::DirectivePending => Progress::DirectivePending,
                turn::Progress::Stopped => Progress::Stopped,
            },
        }
    }

    pub fn detail(&self) -> Option<String> {
        match self.state.as_ref().unwrap() {
            State::Input(input) => input
                .detail()
                .or_else(|| input.operation_detail().map(str::to_owned)),
            State::Management(owner) => owner.detail().map(str::to_owned),
            State::Turn(flow) => flow.detail(),
        }
    }

    pub fn advance(&mut self) -> Progress {
        if !matches!(self.progress(), Progress::Pending | Progress::Accepted) {
            return self.progress();
        }
        self.state = Some(match self.state.take().unwrap() {
            State::Input(mut input) => {
                input.advance();
                if input.progress() == FoundationInputProgress::ManagementPending {
                    match input.management() {
                        Ok(owner) => State::Management(owner),
                        Err(input) => State::Input(input),
                    }
                } else {
                    State::Input(input)
                }
            }
            State::Management(mut owner) => {
                owner.advance();
                match owner.finish() {
                    Ok(input) => State::Input(input),
                    Err(owner) => State::Management(owner),
                }
            }
            State::Turn(mut flow) => {
                flow.advance();
                State::Turn(flow)
            }
        });
        self.progress()
    }

    /// Each accepted input needs its own caller-supplied turn. begin_task reads
    /// the actual marks catalog and reuses an open task; a turn close does not
    /// end that task or erase the resident argument.
    pub fn start_turn(&mut self, turn: TurnKey) -> Result<(), TurnKey> {
        if self.progress() != Progress::Accepted || turn.0.is_empty() {
            return Err(turn);
        }
        let State::Input(input) = self.state.take().unwrap() else {
            unreachable!("only input acceptance grants a new turn")
        };
        match turn::Workflow::begin(input, turn.clone(), self.family, self.method) {
            Ok(flow) => {
                self.state = Some(State::Turn(flow));
                Ok(())
            }
            Err(input) => {
                self.state = Some(State::Input(input));
                Err(turn)
            }
        }
    }

    /// Advance to intake only after the old owner can return its serving seat.
    /// In particular an elected operation refusal with unfinished accounting
    /// remains owned, even though its progress is also Refused.
    pub fn next_input(&mut self) -> Result<Progress, Progress> {
        let completed = self.progress();
        if !matches!(
            completed,
            Progress::Completed | Progress::Refused | Progress::Aborted
        ) {
            return Err(completed);
        }
        let result = match self.state.take().unwrap() {
            State::Input(input) => input.finish().map_err(State::Input),
            State::Turn(flow) => flow.finish().map_err(State::Turn),
            State::Management(owner) => Err(State::Management(owner)),
        };
        match result {
            Ok(
                FoundationInputResult::Completed(serving)
                | FoundationInputResult::Refused(serving)
                | FoundationInputResult::Aborted(serving),
            ) => {
                self.state = Some(State::Input(serving.input()));
                Ok(completed)
            }
            Ok(_) => unreachable!("terminal owners return only a serving seat"),
            Err(state) => {
                self.state = Some(state);
                Err(completed)
            }
        }
    }

    /// Transfer the entire current owner to lifecycle composition without
    /// claiming cleanup, changing task state or accepting another world input.
    pub fn into_owner(self) -> Result<Box<FoundationInput<'a>>, Box<FoundationManagement<'a>>> {
        match self.state.unwrap() {
            State::Input(input) => Ok(input),
            State::Management(owner) => Err(owner),
            State::Turn(flow) => flow.into_owner(),
        }
    }
}
