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
    ElectionRefused,
    Aborted,
    DirectivePending,
    LeavePending,
    Quiesced,
    WindDownComplete,
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
    wind_down: Option<Box<super::wind_down::Policy>>,
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
            wind_down: None,
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
                FoundationInputProgress::LeavePending => Progress::LeavePending,
                FoundationInputProgress::Quiesced => Progress::Quiesced,
                FoundationInputProgress::WindDown
                    if input.wind_down_progress()
                        == Some(weaver_harness::FoundationWindDownProgress::Complete) =>
                {
                    Progress::WindDownComplete
                }
                FoundationInputProgress::WindDown
                    if self
                        .wind_down
                        .as_ref()
                        .is_some_and(|policy| policy.detail().is_some()) =>
                {
                    Progress::ElectionRefused
                }
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
                turn::Progress::ElectionRefused => Progress::ElectionRefused,
                turn::Progress::Aborted => Progress::Aborted,
                turn::Progress::DirectivePending => Progress::DirectivePending,
                turn::Progress::LeavePending => Progress::LeavePending,
                turn::Progress::Quiesced => Progress::Quiesced,
                turn::Progress::WindDownComplete => Progress::WindDownComplete,
                turn::Progress::Stopped => Progress::Stopped,
            },
        }
    }

    pub fn detail(&self) -> Option<String> {
        if let Some(detail) = self.wind_down.as_ref().and_then(|policy| policy.detail()) {
            return Some(detail.into());
        }
        match self.state.as_ref().unwrap() {
            State::Input(input) => input.detail().or_else(|| {
                matches!(
                    input.progress(),
                    FoundationInputProgress::OperationRefused
                        | FoundationInputProgress::Refused
                        | FoundationInputProgress::Stopped
                )
                .then(|| input.operation_detail().map(str::to_owned))
                .flatten()
            }),
            State::Management(owner) => owner.detail().map(str::to_owned),
            State::Turn(flow) => flow.detail(),
        }
    }

    /// Elect graceful drain without replacing the current turn policy or
    /// accepting a later input. The retained lifecycle request outlives it.
    pub fn defer_graceful_leave(&mut self) -> bool {
        self.route_graceful_leave(false)
    }

    pub fn queue_plan_update(&mut self) -> bool {
        if let Some(State::Turn(flow)) = self.state.as_mut() {
            return flow.queue_plan_update();
        }
        if !matches!(self.state.as_ref(), Some(State::Management(owner)) if owner.progress() == FoundationManagementProgress::DirectivePending)
        {
            return false;
        }
        let State::Management(owner) = self.state.take().unwrap() else {
            unreachable!()
        };
        match owner.queue_plan_update() {
            Ok(input) => {
                self.state = Some(State::Input(input));
                true
            }
            Err(owner) => {
                self.state = Some(State::Management(owner));
                false
            }
        }
    }

    pub fn queue_save(&mut self) -> bool {
        if let Some(State::Turn(flow)) = self.state.as_mut() {
            return flow.queue_save();
        }
        if !matches!(self.state.as_ref(), Some(State::Management(owner)) if owner.progress() == FoundationManagementProgress::DirectivePending)
        {
            return false;
        }
        let State::Management(owner) = self.state.take().unwrap() else {
            unreachable!()
        };
        match owner.queue_save() {
            Ok(input) => {
                self.state = Some(State::Input(input));
                true
            }
            Err(owner) => {
                self.state = Some(State::Management(owner));
                false
            }
        }
    }

    pub fn join_graceful_leave(&mut self) -> bool {
        self.route_graceful_leave(true)
    }

    fn route_graceful_leave(&mut self, joining: bool) -> bool {
        if let Some(State::Turn(flow)) = self.state.as_mut() {
            return if joining {
                flow.join_graceful_leave()
            } else {
                flow.defer_graceful_leave()
            };
        }
        if !matches!(self.state.as_ref(), Some(State::Management(owner)) if owner.progress() == FoundationManagementProgress::DirectivePending)
        {
            return false;
        }
        let State::Management(owner) = self.state.take().unwrap() else {
            unreachable!()
        };
        match if joining {
            owner.join_graceful_leave()
        } else {
            owner.defer_graceful_leave()
        } {
            Ok(input) => {
                self.state = Some(State::Input(input));
                true
            }
            Err(owner) => {
                self.state = Some(State::Management(owner));
                false
            }
        }
    }

    /// The worker elects the next Leave phase without replacing the retained
    /// turn, input checks or original lifecycle holder.
    pub fn begin_graceful_quiesce(&mut self) -> bool {
        match self.state.as_mut().unwrap() {
            State::Input(input) => input.begin_graceful_quiesce(),
            State::Turn(flow) => flow.begin_graceful_quiesce(),
            State::Management(_) => false,
        }
    }

    pub fn begin_graceful_wind_down(
        &mut self,
        turn: TurnKey,
        passes: usize,
    ) -> Result<(), TurnKey> {
        let Some(policy) = super::wind_down::Policy::new(passes, self.family) else {
            return Err(turn);
        };
        match self.state.as_mut().unwrap() {
            State::Input(input) => {
                input.begin_graceful_wind_down(turn, self.method)?;
                self.wind_down = Some(Box::new(policy));
                Ok(())
            }
            State::Turn(flow) => flow.begin_graceful_wind_down(turn, passes),
            State::Management(_) => Err(turn),
        }
    }

    pub fn advance(&mut self) -> Progress {
        if !matches!(
            self.progress(),
            Progress::Pending
                | Progress::Accepted
                | Progress::ElectionRefused
                | Progress::LeavePending
                | Progress::Quiesced
        ) {
            return self.progress();
        }
        // Calls that may unwind borrow the retained state. Take it only for the
        // explicit, checked ownership moves below, never around advance().
        match self.state.as_mut().unwrap() {
            State::Input(input) => {
                if input.progress() == FoundationInputProgress::WindDown {
                    self.wind_down
                        .as_mut()
                        .expect("worker wind-down election")
                        .advance(input);
                } else {
                    input.advance();
                }
            }
            State::Management(owner) => {
                if owner.advance() == FoundationManagementProgress::Pending {
                    owner.wait_for_progress();
                }
            }
            State::Turn(flow) => {
                flow.advance();
            }
        }
        self.route_management();
        self.progress()
    }

    fn route_management(&mut self) {
        if matches!(self.state.as_ref(), Some(State::Input(input))
            if input.progress() == FoundationInputProgress::ManagementPending)
        {
            let Some(State::Input(input)) = self.state.take() else {
                unreachable!()
            };
            self.state = Some(match input.management() {
                Ok(owner) => State::Management(owner),
                Err(input) => State::Input(input),
            });
        } else if matches!(self.state.as_ref(), Some(State::Management(owner))
            if matches!(owner.progress(), FoundationManagementProgress::Answered | FoundationManagementProgress::Dropped))
        {
            let Some(State::Management(owner)) = self.state.take() else {
                unreachable!()
            };
            self.state = Some(match owner.finish() {
                Ok(input) => State::Input(input),
                Err(owner) => State::Management(owner),
            });
        }
    }

    /// A declined policy election retains the same session and waits for the
    /// existing lifecycle/transport work instead of polling an accepted input.
    pub(super) fn wait_for_election(&mut self) {
        match self.state.as_mut().unwrap() {
            State::Input(input) => {
                input.wait_for_wake();
            }
            State::Management(owner) => {
                if owner.advance() == FoundationManagementProgress::Pending {
                    owner.wait_for_progress();
                }
            }
            State::Turn(flow) => {
                flow.wait_for_election();
            }
        }
        self.route_management();
    }

    /// A refused consume retains the original workflow, including its round
    /// count and wind-down election. No fresh session is reconstructed.
    // Return the retained session directly; refusal needs no new allocation.
    #[allow(clippy::result_large_err)]
    pub(super) fn into_exit(mut self) -> Result<super::dispatch::ServingHandoff, Self> {
        let result = match self.state.take().unwrap() {
            State::Input(input) => super::dispatch::input_handoff(input).map_err(State::Input),
            State::Management(owner) => {
                super::dispatch::management_handoff(owner).map_err(State::Management)
            }
            State::Turn(flow) => flow.into_exit().map_err(State::Turn),
        };
        match result {
            Ok(handoff) => Ok(handoff),
            Err(state) => {
                self.state = Some(state);
                Err(self)
            }
        }
    }

    /// Each accepted input needs its own caller-supplied turn. begin_task reads
    /// the actual marks catalog and reuses an open task; a turn close does not
    /// end that task or erase the resident argument.
    pub fn start_turn(&mut self, turn: TurnKey) -> Result<(), TurnKey> {
        if !matches!(self.state.as_ref(), Some(State::Input(input)) if input.accepts_turn(&turn)) {
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
    /// An unfinished election remains ElectionRefused and keeps its owner;
    /// only a completed refused reply permits this handoff.
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
