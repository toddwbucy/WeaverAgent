use super::*;
use crate::flow::coordinator::OutputOriginal;

impl RecordingContext {
    #[allow(clippy::result_large_err)]
    pub(in crate::flow) fn cancel_placement_grant(
        &mut self,
        grant: PlacementGrant,
    ) -> Result<PlaceAccount, PlaceBeginError<PlacementGrant>> {
        if !self
            .place_engine
            .as_ref()
            .and_then(|e| e.outstanding.as_ref())
            .is_some_and(|id| Arc::ptr_eq(id, &grant.identity))
            || !Arc::ptr_eq(&grant.origin, &self.read_origin)
            || self.generation.is_some()
        {
            return Err(PlaceBeginError {
                input: grant,
                refusal: PlaceError::WrongProof,
                failure: None,
                staged: None,
            });
        }
        if self
            .boot_journal
            .as_mut()
            .unwrap()
            .cancel(&grant.pending)
            .is_err()
        {
            return Err(PlaceBeginError {
                input: grant,
                refusal: PlaceError::Stale,
                failure: None,
                staged: None,
            });
        }
        self.place_engine.as_mut().unwrap().outstanding = None;
        self.barrier.set_place_owed(self.place_owed());
        Ok(PlaceAccount {
            phase: self.phase,
            work: AccountWork::Generation(Box::new(GenerationTransaction {
                grant: Some(grant),
                native_admission: None,
                reservation: None,
                terminal: None,
                failed_wire: None,
                intermediates: vec![],
                authors: VecDeque::new(),
                author: None,
                recorded_authors: vec![],
                confirmed: None,
                cancelled: true,
                cancel_sent: false,
                cancel_settled: false,
                driving: false,
                done: true,
                output_started: false,
                output_done: false,
                output_cancelled: None,
                incomplete: None,
                account_deadline: None,
            })),
            measure: None,
        })
    }

    #[allow(clippy::result_large_err)]
    pub(in crate::flow) fn begin_generation(
        &mut self,
        grant: PlacementGrant,
        decode: &mut DecodePort,
        io: &mut impl PumpIo,
    ) -> Result<(), PlaceBeginError<PlacementGrant>> {
        let failure = self
            .live(io)
            .err()
            .or_else(|| self.check_place_port(decode).err());
        let owned = self
            .place_engine
            .as_ref()
            .and_then(|e| e.outstanding.as_ref())
            .is_some_and(|id| Arc::ptr_eq(id, &grant.identity));
        let valid = owned
            && Arc::ptr_eq(&grant.origin, &self.read_origin)
            && self
                .boot_journal
                .as_ref()
                .is_some_and(|j| j.check_pending(&grant.pending).is_ok())
            && self.open_turn.as_ref() == Some(&grant.turn)
            && self.committed_plan_summary().is_ok()
            && self
                .place_engine
                .as_ref()
                .is_some_and(|e| e.schemas == grant.schemas)
            && self.phase == RecordingPhase::Entered
            && !self.place_admission_stopped()
            && !self
                .place_revoked_grant
                .as_ref()
                .is_some_and(|id| Arc::ptr_eq(id, &grant.identity));
        if failure.is_some() || !valid {
            return Err(PlaceBeginError {
                input: grant,
                refusal: PlaceError::WrongProof,
                failure,
                staged: None,
            });
        }
        if self.place_busy() {
            return Err(PlaceBeginError {
                input: grant,
                refusal: PlaceError::Busy(self.held_input()),
                failure: None,
                staged: None,
            });
        }
        let native_admission = match self.native_place_before_append(&grant) {
            Ok(native) => native,
            Err(refusal) => {
                return Err(PlaceBeginError {
                    input: grant,
                    refusal,
                    failure: None,
                    staged: None,
                });
            }
        };
        let directive = TokenDirective::AppendAndGenerate {
            turn: TurnKey(grant.turn.0.clone()),
            delta: grant.delta.clone(),
            tool_schemas: grant.schemas.clone(),
        };
        let reservation = match decode.stage_placement(directive) {
            Ok(DecodeStage::Queued(r)) => r,
            Ok(DecodeStage::Busy(_)) => {
                return Err(PlaceBeginError {
                    input: grant,
                    refusal: PlaceError::Busy(None),
                    failure: None,
                    staged: None,
                });
            }
            Err(error) => {
                let failure = self.place_fault(io, format!("append stage: {:?}", error.failure));
                return Err(PlaceBeginError {
                    input: grant,
                    refusal: PlaceError::WrongPhase,
                    failure: Some(failure),
                    staged: None,
                });
            }
        };
        self.barrier.set_place_owed(true);
        self.generation = Some(GenerationTransaction {
            grant: Some(grant),
            native_admission: Some(native_admission),
            reservation: Some(reservation),
            terminal: None,
            failed_wire: None,
            intermediates: vec![],
            authors: VecDeque::new(),
            author: None,
            recorded_authors: vec![],
            confirmed: None,
            cancelled: false,
            cancel_sent: false,
            cancel_settled: false,
            driving: false,
            done: false,
            output_started: false,
            output_done: false,
            output_cancelled: None,
            incomplete: None,
            account_deadline: None,
        });
        Ok(())
    }
    pub(in crate::flow) fn cancel_generation(
        &mut self,
        decode: &mut DecodePort,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        let Some(work) = self.generation.as_mut() else {
            return Err(RunFailure::PlaceOwned);
        };
        work.cancelled = true;
        if work.done || work.cancel_sent || work.cancel_settled {
            return Ok(());
        }
        let Some(reservation) = work.reservation.as_ref() else {
            return Ok(());
        };
        if !reservation.belongs_to(decode) {
            return Err(RunFailure::PlaceOwned);
        }
        // A consumed terminal already settles this request. Starting another
        // Cancel would create a new AtRest obligation after a complete refusal.
        // A previously queued Cancel remains unsettled until its own answer.
        if decode.reservation_settled(reservation) {
            work.cancel_settled = true;
            return Ok(());
        }
        if !reservation.sent() {
            if decode.placement_not_started() {
                let reservation = work.reservation.take().unwrap();
                match decode.withdraw_unsent(reservation) {
                    Ok(()) => {
                        work.done = true;
                        return Ok(());
                    }
                    Err(error) => {
                        work.reservation = Some(error.reservation);
                        return Err(
                            self.place_fault(io, format!("append withdrawal: {:?}", error.failure))
                        );
                    }
                }
            }
            return Ok(());
        }
        let turn = work
            .grant
            .as_ref()
            .map(|g| g.turn.clone())
            .or_else(|| work.confirmed.as_ref().map(|p| p.grant.turn.clone()))
            .unwrap();
        match decode.cancel_reserved(reservation, TurnKey(turn.0)) {
            Ok(Stage::Queued) => {
                work.cancel_sent = true;
                Ok(())
            }
            Ok(Stage::Busy(_)) => Ok(()),
            Err(error) => Err(self.place_fault(io, format!("Cancel stage: {:?}", error.failure))),
        }
    }
    pub(in crate::flow) fn advance_generation<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<GenerationProgress, RunFailure> {
        self.live(ports.io)?;
        if self.generation.as_ref().is_none_or(|w| w.done) {
            return if self.generation.is_some() {
                Ok(PlaceProgress::Complete)
            } else {
                Err(RunFailure::PlaceOwned)
            };
        }
        if self.generation.as_ref().unwrap().cancelled
            && !self.generation.as_ref().unwrap().cancel_sent
            && !self.generation.as_ref().unwrap().cancel_settled
        {
            self.cancel_generation(
                ports.decode.as_deref_mut().ok_or(RunFailure::PlaceOwned)?,
                ports.io,
            )?;
            if self.generation.as_ref().unwrap().done {
                return Ok(PlaceProgress::Complete);
            }
        }
        if self.generation.as_ref().unwrap().author.is_some() {
            self.advance_place_author(ports)?;
            if self
                .generation
                .as_ref()
                .unwrap()
                .author
                .as_ref()
                .unwrap()
                .sequence
                .is_some()
            {
                let work = self.generation.as_mut().unwrap();
                work.author = None;
                work.intermediates.clear();
            }
            return Ok(PlaceProgress::Progress);
        }
        if let Some(event) = self.generation.as_mut().unwrap().authors.pop_front() {
            self.generation.as_mut().unwrap().author = Some(PlaceAuthor::new(event));
            return Ok(PlaceProgress::Progress);
        }
        if self.generation.as_ref().unwrap().reservation.is_some() {
            let decode = ports.decode.as_deref().ok_or(RunFailure::PlaceOwned)?;
            if decode.held().is_none() {
                self.pump_once(ports)?;
            }
            let work = self.generation.as_ref().unwrap();
            let reservation = work.reservation.as_ref().unwrap();
            let decode = ports.decode.as_deref_mut().unwrap();
            let received =
                decode
                    .take_reserved(reservation)
                    .map_err(|e| RunFailure::Transport {
                        source: PumpSource::Decode,
                        failure: e,
                    })?;
            if let Some(received) = received {
                let valid = received.matches(reservation)
                    && received.after_send()
                    && reservation.sent()
                    && !decode.reservation_invalid(reservation);
                if !valid {
                    self.generation.as_mut().unwrap().terminal = Some(received);
                    return Err(self.place_fault(
                        ports.io,
                        "generation reply does not match its complete reserved request",
                    ));
                }
                self.consume_generation(received, ports.io)?;
            }
            let work = self.generation.as_ref().unwrap();
            if ports
                .decode
                .as_deref()
                .unwrap()
                .reservation_settled(work.reservation.as_ref().unwrap())
            {
                let reservation = self
                    .generation
                    .as_mut()
                    .unwrap()
                    .reservation
                    .take()
                    .unwrap();
                if let Err(error) = ports
                    .decode
                    .as_deref_mut()
                    .unwrap()
                    .finish_reserved(reservation)
                {
                    self.generation.as_mut().unwrap().reservation = Some(error.reservation);
                    return Err(self.place_fault(
                        ports.io,
                        format!("generation settlement: {:?}", error.failure),
                    ));
                }
            }
            return Ok(PlaceProgress::Progress);
        }
        if self.generation.as_ref().unwrap().confirmed.is_none() {
            self.generation.as_mut().unwrap().done = true;
            return Ok(PlaceProgress::Complete);
        }
        if self.generation.as_ref().unwrap().cancelled
            && !(self.repair.is_some() && self.generation.as_ref().unwrap().output_started)
        {
            if self.output.is_some() {
                self.cancel_output();
                self.generation.as_mut().unwrap().driving = true;
                let result = self.advance_output(ports);
                self.generation.as_mut().unwrap().driving = false;
                result?;
                let Some(cancelled) = self.take_cancelled_output() else {
                    return Ok(PlaceProgress::Pending);
                };
                self.generation.as_mut().unwrap().output_cancelled = Some(cancelled);
            }
            self.generation.as_mut().unwrap().done = true;
            return Ok(PlaceProgress::Complete);
        }
        if !self.generation.as_ref().unwrap().output_started {
            let proof = self
                .generation
                .as_ref()
                .unwrap()
                .confirmed
                .as_ref()
                .unwrap();
            let original = OutputOriginal {
                message: Message {
                    role: Role::Assistant,
                    content: proof.generation().content.clone(),
                },
                finish: proof.generation().finish,
                turn: proof.grant.turn.clone(),
            };
            let parent = proof.grant.recorded;
            if self.deadlines.get(PumpDeadline::Decision).is_none() {
                let bound = AbsoluteBound::after(
                    self.last_ns,
                    self.load_duty
                        .as_ref()
                        .unwrap()
                        .instruction
                        .decision_wait
                        .elapsed_ns
                        .0,
                )
                .map_err(|e| self.place_fault(ports.io, e.to_string()))?;
                self.arm(PumpDeadline::Decision, bound, ports.io)?;
            }
            self.generation.as_mut().unwrap().driving = true;
            let result = self.begin_output(original, Some(parent), ports.io);
            self.generation.as_mut().unwrap().driving = false;
            if let Err(error) = result {
                return Err(error.failure.unwrap_or_else(|| {
                    self.place_fault(
                        ports.io,
                        format!("confirmed output could not start: {:?}", error.refusal),
                    )
                }));
            }
            self.generation.as_mut().unwrap().output_started = true;
            return Ok(PlaceProgress::Progress);
        }
        self.generation.as_mut().unwrap().driving = true;
        let result = (|| {
            self.advance_output(ports)?;
            self.take_output(ports.io)
        })();
        self.generation.as_mut().unwrap().driving = false;
        if let Some(output) = result? {
            let mut proof = self.generation.as_mut().unwrap().confirmed.take().unwrap();
            self.boot_journal
                .as_mut()
                .unwrap()
                .annotate_output(&proof, &output)
                .map_err(|e| self.place_fault(ports.io, format!("emission annotation: {e:?}")))?;
            proof.output = Some(output);
            let work = self.generation.as_mut().unwrap();
            work.confirmed = Some(proof);
            work.output_done = true;
            work.done = true;
            return Ok(PlaceProgress::Complete);
        }
        Ok(PlaceProgress::Pending)
    }
    pub(super) fn consume_generation(
        &mut self,
        received: ReceivedDecode,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        let work = self.generation.as_mut().unwrap();
        let turn = work
            .grant
            .as_ref()
            .map(|g| g.turn.clone())
            .or_else(|| work.confirmed.as_ref().map(|g| g.grant.turn.clone()))
            .unwrap();
        match received.reply() {
            DecodeReply::Answer(TokenAnswer::Token { .. }) => {}
            DecodeReply::Answer(TokenAnswer::Field {
                position,
                ranked,
                realized,
            }) => {
                let payload = Payload::ModelField(weaver_trace::ModelField {
                    position: *position,
                    ranked: ranked
                        .iter()
                        .map(|c| weaver_trace::Candidate {
                            token: c.token,
                            probability: c.probability,
                        })
                        .collect(),
                    realized: *realized,
                });
                work.authors.push_back(model_event(
                    Kind::ModelField,
                    payload,
                    turn,
                    AuthoredSource::Runtime,
                ));
                work.intermediates.push(received);
            }
            DecodeReply::Answer(TokenAnswer::Column { .. }) => {
                work.terminal = Some(received);
                return Err(self.place_fault(
                    io,
                    "serving generation emitted unelected diagnostic columns",
                ));
            }
            DecodeReply::Answer(TokenAnswer::AtRest) => {
                work.intermediates.push(received);
            }
            DecodeReply::Refusal(refusal) => {
                let record = RefusalRecord::Decode {
                    asked: TokenAsk::AppendAndGenerate,
                    refusal: refusal.clone(),
                };
                let payload = serde_json::value::to_raw_value(&record)
                    .map_err(|e| self.place_fault(io, e.to_string()))?;
                let work = self.generation.as_mut().unwrap();
                work.authors.push_back(model_event(
                    Kind::Refusal,
                    Payload::Refusal(payload),
                    turn,
                    AuthoredSource::Runtime,
                ));
                let fatal = matches!(refusal, TokenRefusal::NotOpen | TokenRefusal::OutOfOrder);
                work.terminal = Some(received);
                if fatal {
                    return Err(self.place_fault(
                        io,
                        "decode refused a reserved append outside its admitted state",
                    ));
                }
            }
            DecodeReply::Answer(TokenAnswer::Generated(generation)) => {
                let facts = work.grant.as_ref().unwrap().facts;
                let limits = self.place_engine.as_ref().unwrap().limits();
                let validation =
                    crate::flow::place::generation_facts(generation, &facts, limits.2, limits.3);
                if let Err(detail) = validation {
                    self.generation.as_mut().unwrap().terminal = Some(received);
                    return Err(self.place_fault(io, detail));
                }
                let grant = self.generation.as_mut().unwrap().grant.take().unwrap();
                let proof = ConfirmedGeneration {
                    grant,
                    received,
                    output: None,
                    append_receipt: None,
                };
                let result = self
                    .boot_journal
                    .as_mut()
                    .unwrap()
                    .complete_generation(&proof);
                self.generation.as_mut().unwrap().confirmed = Some(proof);
                let receipt = result.map_err(|e| {
                    self.place_fault(io, format!("generation journal confirmation: {e:?}"))
                })?;
                self.generation
                    .as_mut()
                    .unwrap()
                    .confirmed
                    .as_mut()
                    .unwrap()
                    .append_receipt = Some(receipt);
                let proof = self
                    .generation
                    .as_ref()
                    .unwrap()
                    .confirmed
                    .as_ref()
                    .unwrap();
                let g = proof.generation();
                let mut events = VecDeque::from([
                    model_event(
                        Kind::ModelRequest,
                        Payload::ModelRequest(g.request.clone()),
                        turn.clone(),
                        AuthoredSource::Loop,
                    ),
                    model_event(
                        Kind::ModelOutput,
                        Payload::ModelOutput(weaver_trace::ModelOutput {
                            emission: g.emission.clone(),
                            finish: match g.finish {
                                weaver_types::Finish::Completed => weaver_trace::Finish::Completed,
                                weaver_types::Finish::Stopped => weaver_trace::Finish::Stopped,
                                weaver_types::Finish::Length => weaver_trace::Finish::Length,
                            },
                            resident: g.resident,
                            capacity: g.capacity,
                        }),
                        turn.clone(),
                        AuthoredSource::Model,
                    ),
                    model_event(
                        Kind::ModelMeasurement,
                        Payload::ModelMeasurement(g.measurement.clone()),
                        turn.clone(),
                        AuthoredSource::Runtime,
                    ),
                ]);
                for report in &g.unrecovered {
                    let payload =
                        serde_json::value::to_raw_value(report).map_err(|e| RunFailure::Flow {
                            reason: FlowHaltReason::ProtocolFailed,
                            point: HaltPoint::Barrier,
                            detail: e.to_string(),
                        })?;
                    events.push_back(model_event(
                        Kind::ModelUnrecoveredCall,
                        Payload::UnrecoveredCall(payload),
                        turn.clone(),
                        AuthoredSource::Model,
                    ));
                }
                // The complete terminal already happened. Retain every owed model
                // fact before a later annotation hook can fail or stop the run.
                self.generation.as_mut().unwrap().authors.extend(events);
                let marks = self
                    .boot_journal
                    .as_mut()
                    .unwrap()
                    .placed_marks(
                        self.generation
                            .as_ref()
                            .unwrap()
                            .confirmed
                            .as_ref()
                            .unwrap(),
                    )
                    .map_err(|e| RunFailure::Flow {
                        reason: FlowHaltReason::ProtocolFailed,
                        point: HaltPoint::Barrier,
                        detail: format!("placed marks: {e:?}"),
                    })?;
                for mark in marks {
                    self.generation
                        .as_mut()
                        .unwrap()
                        .authors
                        .push_back(place_event(
                            Kind::StateMarkPosition,
                            AuthoredSource::Loop,
                            Payload::MarkPosition(Box::new(mark)),
                            Some(turn.clone()),
                            None,
                        ));
                }
                let Some(native_admission) =
                    self.generation.as_mut().unwrap().native_admission.take()
                else {
                    return Err(self.place_fault(
                        io,
                        "confirmed generation lost its retained native admission",
                    ));
                };
                let proof = self.generation.as_mut().unwrap().confirmed.take().unwrap();
                let native = self.native_place_completed(&proof, native_admission);
                self.generation.as_mut().unwrap().confirmed = Some(proof);
                for mark in native.map_err(|detail| self.place_fault(io, detail))? {
                    self.generation
                        .as_mut()
                        .unwrap()
                        .authors
                        .push_back(place_event(
                            Kind::StateMarkPosition,
                            AuthoredSource::Loop,
                            Payload::MarkPosition(Box::new(mark)),
                            Some(turn.clone()),
                            None,
                        ));
                }
                self.place_engine.as_mut().unwrap().staged = None;
            }
            _ => {
                work.terminal = Some(received);
                return Err(self.place_fault(io, "unexpected generation reply"));
            }
        }
        Ok(())
    }
    pub(in crate::flow) fn take_placed_generation(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<Option<GenerationCompletion>, RunFailure> {
        self.live(io)?;
        if self.phase != RecordingPhase::Entered
            || !self.generation.as_ref().is_some_and(|w| w.done)
        {
            return Ok(None);
        }
        self.consume_place_completion()?;
        let mut work = self.generation.take().unwrap();
        self.place_engine.as_mut().unwrap().outstanding = None;
        self.barrier.set_place_owed(self.place_owed());
        if !work.cancelled
            && let Some(proof) = work.confirmed.take()
        {
            return Ok(Some(GenerationCompletion::Generated(Box::new(proof))));
        }
        if let Some(grant) = &work.grant {
            self.boot_journal
                .as_mut()
                .unwrap()
                .cancel(&grant.pending)
                .map_err(|e| self.place_fault(io, format!("refused append: {e:?}")))?;
        }
        let cancelled = work.cancelled;
        let account = PlaceAccount {
            phase: self.phase,
            work: AccountWork::Generation(Box::new(work)),
            measure: None,
        };
        Ok(Some(if cancelled {
            GenerationCompletion::Cancelled(account)
        } else {
            GenerationCompletion::Refused(account)
        }))
    }
}

impl GenerationCompletion {
    /// A cancelled correction can retain complete checks as accounting facts.
    /// This moves no placement grant and grants no successful continuation.
    pub(in crate::flow) fn correction_output(
        &mut self,
    ) -> Option<&mut Option<crate::flow::coordinator::CheckedGeneration>> {
        match self {
            Self::Generated(proof) => Some(&mut proof.output),
            Self::Cancelled(account) => match &mut account.work {
                AccountWork::Generation(work) => {
                    work.confirmed.as_mut().map(|proof| &mut proof.output)
                }
                _ => None,
            },
            Self::Refused(_) => None,
        }
    }
}
fn model_event(
    kind: Kind,
    payload: Payload,
    turn: TurnRef,
    source: AuthoredSource,
) -> AuthoredEvent {
    AuthoredEvent {
        kind,
        subsystem: Subsystem::SpuDecoder,
        turn: Some(turn),
        source,
        causal_parent: None,
        payload: Some(payload),
    }
}
