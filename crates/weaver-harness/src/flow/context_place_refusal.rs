//! Address refusals retain their bounded trace and requester response duties.
use super::*;
#[derive(Debug)]
pub(in crate::flow) struct PlaceNotice {
    pub(super) author: Option<PlaceAuthor>,
    pub(super) recorded_authors: Vec<Sequence>,
    pub(super) bound: AbsoluteBound,
    pub(super) key: PumpDeadline,
    pub(super) done: bool,
    pub(super) cancelled: bool,
    pub(super) incomplete: Option<String>,
    pub(super) account_deadline: Option<u64>,
    events: VecDeque<AuthoredEvent>,
    refusal: Option<PlaceRefused>,
    turn: Option<TurnRef>,
    owned_wait: bool,
}
impl RecordingContext {
    pub(super) fn queue_place_refusal(
        &mut self,
        assembly_id: DecimalU64,
        code: PlacementCode,
        detail: &str,
    ) -> Result<(), PlaceError> {
        if self.place_notice.is_some() {
            return Err(PlaceError::Busy);
        }
        let inherited = [
            PumpDeadline::Decision,
            PumpDeadline::Cut,
            PumpDeadline::Enter,
            PumpDeadline::Leave,
        ]
        .into_iter()
        .filter_map(|key| self.deadlines.get(key).map(|b| (key, b)))
        .min_by_key(|(_, b)| b.deadline_ns());
        let (key, bound, owned_wait) = match inherited {
            Some((k, b)) => (k, b, false),
            None => {
                let bound =
                    AbsoluteBound::after(self.last_ns, self.declaration.cut_wait.elapsed_ns.0)
                        .map_err(|_| PlaceError::Exhausted)?;
                self.deadlines
                    .arm(PumpDeadline::Cut, bound)
                    .map_err(|_| PlaceError::Busy)?;
                (PumpDeadline::Cut, bound, true)
            }
        };
        let refusal = PlaceRefused {
            assembly_id,
            code,
            detail: detail.into(),
            account: None,
        };
        let record = RefusalRecord::Placement {
            assembly_id,
            code,
            detail: detail.into(),
        };
        let payload =
            serde_json::value::to_raw_value(&record).map_err(|_| PlaceError::InvalidContext)?;
        let turn = self.open_turn.clone();
        let mut events = VecDeque::from([place_event(
            Kind::Refusal,
            AuthoredSource::Runtime,
            Payload::Refusal(payload),
            turn.clone(),
            None,
        )]);
        if turn.is_some() {
            events.push_back(place_event(
                Kind::TurnClosed,
                AuthoredSource::Runtime,
                Payload::TurnClosed(weaver_trace::TurnClose::Stopped {
                    reason: weaver_trace::StopReason::Refused,
                    cause: None,
                }),
                turn.clone(),
                None,
            ));
        }
        self.place_notice = Some(PlaceNotice {
            author: None,
            recorded_authors: vec![],
            bound,
            key,
            done: false,
            cancelled: false,
            incomplete: None,
            account_deadline: None,
            events,
            refusal: Some(refusal),
            turn,
            owned_wait,
        });
        self.barrier.set_place_owed(true);
        Ok(())
    }
    pub(super) fn prepare_place_notice_author(&mut self) -> bool {
        let Some(notice) = &mut self.place_notice else {
            return false;
        };
        if notice.author.is_some() {
            return false;
        }
        if let Some(event) = notice.events.pop_front() {
            notice.author = Some(PlaceAuthor::new(event));
            return true;
        }
        false
    }
    pub(in crate::flow) fn advance_place_refusal<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<PlaceProgress, RunFailure> {
        self.live(ports.io)?;
        let notice = self.place_notice.as_ref().ok_or(RunFailure::PlaceOwned)?;
        if self.deadlines.get(notice.key) != Some(notice.bound) {
            return Err(self.place_fault(ports.io, "placement refusal lost its original bound"));
        }
        if notice.done {
            return Ok(PlaceProgress::Complete);
        }
        if notice.author.is_some() {
            self.advance_place_author(ports)?;
            if self
                .place_notice
                .as_ref()
                .unwrap()
                .author
                .as_ref()
                .unwrap()
                .sequence
                .is_some()
            {
                self.consume_place_completion()?;
                self.place_notice.as_mut().unwrap().author = None;
            }
            return Ok(PlaceProgress::Progress);
        }
        if self.prepare_place_notice_author() {
            return Ok(PlaceProgress::Progress);
        }
        let notice = self.place_notice.as_mut().unwrap();
        let detail = &notice.refusal.as_ref().unwrap().detail;
        let frame = if let Some(turn) = &notice.turn {
            serde_json::json!({"kind":"stopped","reason":detail,"run":self.place_engine.as_ref().unwrap().boot.repaired().checked_open().unwrap().run(),"turn":turn.0})
        } else {
            serde_json::json!({"kind":"refused","reason":detail})
        };
        self.place_close = Some(weaver_types::TurnFrame::carry(frame.to_string().as_bytes()));
        notice.done = true;
        Ok(PlaceProgress::Complete)
    }
    pub(in crate::flow) fn take_place_refusal(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<Option<PlaceRefused>, RunFailure> {
        self.live(io)?;
        if !self.place_notice.as_ref().is_some_and(|w| w.done) {
            return Ok(None);
        }
        let mut work = self.place_notice.take().unwrap();
        if work.owned_wait {
            self.deadlines.disarm(work.key);
        }
        let mut refusal = work.refusal.take().unwrap();
        refusal.account = Some(PlaceAccount {
            phase: self.phase,
            work: AccountWork::Notice(Box::new(work)),
            measure: None,
        });
        self.barrier.set_place_owed(self.place_owed());
        Ok(Some(refusal))
    }
}
