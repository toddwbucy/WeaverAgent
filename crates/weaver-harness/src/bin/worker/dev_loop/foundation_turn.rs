//! The developer's bounded tool loop. The harness retains every protocol owner;
//! this loop elects repair, all conforming requests, result placement and answer.
use super::*;
use weaver_harness::{FoundationManagement, FoundationManagementProgress, FoundationRestatement};
use weaver_types::state_flow::{CallId, CutMethod, PairLane, SortAct};

const MAX_TOOL_ROUNDS: usize = 32;

enum AfterPairs {
    Results(Vec<CallId>),
    Answer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Progress {
    Pending,
    DirectivePending,
    Completed,
    Refused,
    /// A loop election could not advance its retained owner. Management remains
    /// serviceable; this is not a completed refused world reply.
    ElectionRefused,
    Aborted,
    Stopped,
}

/// One turn's loop decisions stay beside its actual input or management owner.
/// Unsupported lifecycle directives keep this whole object for their next owner.
/// No dispatch, replacement startup or serving cutover selects this loop yet.
pub struct Workflow<'a> {
    input: Option<Box<FoundationInput<'a>>>,
    management: Option<Box<FoundationManagement<'a>>>,
    turn: TurnKey,
    family: &'static FoundationRestatement,
    method: CutMethod,
    rounds: usize,
    after_pairs: Option<AfterPairs>,
    bar_round: bool,
    detail: Option<String>,
}

impl<'a> Workflow<'a> {
    /// Begin at checked input acceptance. The task primitive reads the actual
    /// catalog and reuses its open task without creating another boundary.
    pub fn begin(
        input: Box<FoundationInput<'a>>,
        turn: TurnKey,
        family: &'static FoundationRestatement,
        method: CutMethod,
    ) -> Result<Self, Box<FoundationInput<'a>>> {
        if !input.accepts_turn(&turn) {
            return Err(input);
        }
        Ok(Self {
            input: Some(input),
            management: None,
            turn,
            family,
            method,
            rounds: 0,
            after_pairs: None,
            bar_round: false,
            detail: None,
        })
    }

    pub fn progress(&self) -> Progress {
        if let Some(management) = &self.management {
            return match management.progress() {
                FoundationManagementProgress::DirectivePending => Progress::DirectivePending,
                FoundationManagementProgress::Stopped => Progress::Stopped,
                _ => Progress::Pending,
            };
        }
        match self.input.as_ref().unwrap().progress() {
            FoundationInputProgress::Completed => Progress::Completed,
            FoundationInputProgress::Refused => Progress::Refused,
            FoundationInputProgress::Aborted => Progress::Aborted,
            FoundationInputProgress::Stopped => Progress::Stopped,
            _ if self.detail.is_some() => Progress::ElectionRefused,
            _ => Progress::Pending,
        }
    }
    pub fn detail(&self) -> Option<String> {
        self.detail.clone().or_else(|| {
            self.input
                .as_ref()
                .and_then(|input| {
                    input.detail().or_else(|| {
                        matches!(
                            input.progress(),
                            FoundationInputProgress::OperationRefused
                                | FoundationInputProgress::Refused
                                | FoundationInputProgress::Stopped
                        )
                        .then(|| input.operation_detail().map(str::to_owned))
                        .flatten()
                    })
                })
                .or_else(|| {
                    self.management
                        .as_ref()
                        .and_then(|owner| owner.detail().map(str::to_owned))
                })
        })
    }
    pub fn rounds(&self) -> usize {
        self.rounds
    }

    pub fn advance(&mut self) -> Progress {
        if !matches!(
            self.progress(),
            Progress::Pending | Progress::ElectionRefused
        ) {
            return self.progress();
        }
        if let Some(management) = self.management.as_mut() {
            management.advance();
            let management = self.management.take().unwrap();
            match management.finish() {
                Ok(input) => self.input = Some(input),
                Err(management) => self.management = Some(management),
            }
            return self.progress();
        }
        let input = self.input.as_mut().unwrap();
        if self.detail.is_some() && input.progress() != FoundationInputProgress::ManagementPending {
            // Wait on the actual transports while the loop cannot make another
            // election. A wake grants no new operation or replacement deadline.
            input.wait_for_wake();
            return self.progress();
        }
        let result: Result<(), String> = match input.progress() {
            FoundationInputProgress::Pending => {
                input.advance();
                Ok(())
            }
            FoundationInputProgress::ManagementPending => {
                match self.input.take().unwrap().management() {
                    Ok(owner) => self.management = Some(owner),
                    Err(input) => self.input = Some(input),
                }
                Ok(())
            }
            FoundationInputProgress::Accepted => input
                .start_task(self.turn.clone())
                .map_err(|_| "task election refused".into()),
            FoundationInputProgress::Task => input
                .place_task("turn input".into(), self.method)
                .map_err(|_| "task selection refused".into()),
            FoundationInputProgress::Placed => input
                .generate()
                .map_err(|_| "generation election refused".into()),
            FoundationInputProgress::Generated => input
                .repair(self.family)
                .map_err(|e| format!("repair election: {e:?}")),
            FoundationInputProgress::Restatement => input
                .place_restatement("correction".into(), self.method)
                .map_err(|_| "correction placement refused".into()),
            FoundationInputProgress::CorrectionPlaced => input
                .generate_correction()
                .map_err(|_| "correction generation refused".into()),
            FoundationInputProgress::Effective => {
                let view = input.effective_emission().unwrap();
                let calls: Vec<_> = view.requests().iter().map(|p| p.call_id.clone()).collect();
                let mut acts: Vec<_> = view
                    .requests()
                    .iter()
                    .map(|p| match p.lane {
                        PairLane::External => SortAct::Tool {
                            call_id: p.call_id.clone(),
                            statement: p.statement.clone(),
                        },
                        PairLane::Recall => SortAct::Recall {
                            call_id: p.call_id.clone(),
                            statement: p.statement.clone(),
                        },
                        _ => unreachable!("model emissions carry only external/recall requests"),
                    })
                    .collect();
                for statement in view.statements() {
                    if !acts.iter().any(|act| matches!(act, SortAct::Answer { source } if source == &statement.block_id)) {
                        acts.push(SortAct::Answer { source: statement.block_id.clone() });
                    }
                }
                self.bar_round = !calls.is_empty() && self.rounds >= MAX_TOOL_ROUNDS;
                self.after_pairs = Some(if calls.is_empty() || self.bar_round {
                    AfterPairs::Answer
                } else {
                    AfterPairs::Results(calls)
                });
                input
                    .sort_outbound(acts)
                    .map_err(|_| "outbound election refused".into())
            }
            FoundationInputProgress::Sorted => {
                let result = if self.bar_round {
                    input.bar_round_limit()
                } else {
                    input.begin_pairs()
                };
                if result.is_ok() && matches!(self.after_pairs, Some(AfterPairs::Results(_))) {
                    self.rounds += 1;
                }
                result.map_err(|_| "pair election refused".into())
            }
            FoundationInputProgress::PairReady => {
                let next = input.ready_pair().unwrap();
                let call = next.call_id.clone();
                match next.lane {
                    PairLane::External => input.execute(call),
                    PairLane::Recall => input.recall(call),
                    _ => unreachable!("model pair lane"),
                }
                .map_err(|_| "request election refused".into())
            }
            FoundationInputProgress::PairsComplete => match self.after_pairs.take().unwrap() {
                AfterPairs::Answer => input
                    .close_answer()
                    .map_err(|_| "answer election refused".into()),
                AfterPairs::Results(calls) => input
                    .place_results(calls, "round results".into(), self.method)
                    .map_err(|_| "result placement refused".into()),
            },
            FoundationInputProgress::OperationRefused => {
                input.close_operation_refusal().map_err(|_| {
                    input
                        .operation_detail()
                        .unwrap_or("operation refused")
                        .to_owned()
                })
            }
            _ => unreachable!("terminal progress checked above"),
        };
        if let Err(detail) = result {
            self.detail = Some(detail);
        }
        self.progress()
    }

    /// Only actual completed delivery returns a serving seat. A refused election
    /// or unsupported management directive returns the whole retained workflow.
    pub fn finish(mut self) -> Result<FoundationInputResult<'a>, Self> {
        if self.management.is_some()
            || !self.input.as_ref().is_some_and(|input| {
                matches!(
                    input.progress(),
                    FoundationInputProgress::Completed
                        | FoundationInputProgress::Aborted
                        | FoundationInputProgress::Refused
                )
            })
        {
            return Err(self);
        }
        match self.input.take().unwrap().finish() {
            Ok(done) => Ok(done),
            Err(input) => {
                self.input = Some(input);
                Err(self)
            }
        }
    }

    /// Return the retained owner for lifecycle/failed-run composition, with no
    /// claim of completed cleanup. An existing management owner stays intact.
    pub fn into_owner(self) -> Result<Box<FoundationInput<'a>>, Box<FoundationManagement<'a>>> {
        match self.input {
            Some(input) => Ok(input),
            None => Err(self.management.unwrap()),
        }
    }
}
