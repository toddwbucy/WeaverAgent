use super::*;
use crate::flow::classifier::ClassifierState;
use crate::flow::ledger::{LeaseSlotStatus, SourceDisposition, SourceLease};

/// Only this owner can bind the frozen projection to its actual cut terminal
/// (or the already-confirmed empty suffix). The journal never accepts scalars.
pub(in crate::flow) struct CheckedCut<'a> {
    prefix: &'a PreparedPrefix,
    before: u64,
    after: u64,
}
impl CheckedCut<'_> {
    pub(in crate::flow) fn prefix(&self) -> &PreparedPrefix {
        self.prefix
    }
    pub(in crate::flow) fn before(&self) -> u64 {
        self.before
    }
    pub(in crate::flow) fn after(&self) -> u64 {
        self.after
    }
}

impl RecordingContext {
    pub(in crate::flow) fn hold_pending_output_reload_text(
        &mut self,
        block: u64,
    ) -> Result<PendingReloadText, PlaceError> {
        if self.output.is_none() || self.place_engine.is_none() {
            return Err(PlaceError::WrongProof);
        }
        let (source, lease) = self
            .flow
            .split_pending_output_text(block)
            .map_err(|_| PlaceError::WrongProof)?;
        Ok(PendingReloadText {
            origin: self.read_origin.clone(),
            source,
            lease,
        })
    }

    #[allow(clippy::result_large_err)]
    pub(in crate::flow) fn hold_reload_text(
        &self,
        lease: SourceLease,
        source: SourceBlockId,
    ) -> Result<PendingReloadText, (SourceLease, SourceBlockId)> {
        if !self.flow.ledger().owned_pending_text(&lease, &source) {
            return Err((lease, source));
        }
        Ok(PendingReloadText {
            origin: self.read_origin.clone(),
            source,
            lease,
        })
    }
    #[allow(clippy::result_large_err)]
    pub(in crate::flow) fn begin_cut_and_reload(
        &mut self,
        prefix: PreparedPrefix,
        tail: ReloadTail,
        io: &mut impl PumpIo,
    ) -> Result<(), PlaceBeginError<(PreparedPrefix, ReloadTail)>> {
        let failure = self.live(io).err();
        let queued_in_turn = self.open_turn.is_some();
        let valid = !self.place_admission_stopped()
            && self.phase == RecordingPhase::Entered
            && self
                .place_engine
                .as_ref()
                .is_some_and(|e| (e.outstanding.is_none() || queued_in_turn) && e.staged.is_none())
            && self.boot_journal.as_ref().is_some_and(|j| {
                if queued_in_turn {
                    j.owns_prefix(&prefix)
                } else {
                    j.check_projection(&prefix.projection).is_ok()
                }
            })
            && tail.items.iter().all(|item| match item {
                ReloadItem::Held(u) => u.belongs_to(self),
                ReloadItem::Pending(p) => {
                    Arc::ptr_eq(&p.origin, &self.read_origin)
                        && self.flow.ledger().owned_pending_text(&p.lease, &p.source)
                }
            });
        let busy = (!queued_in_turn
            && (self.place.is_some() || self.generation.is_some() || self.measure.is_some()))
            || self.reload.is_some()
            || self.read.is_some()
            || self.inspection.is_some()
            || self.member_capture.is_some();
        if failure.is_some() || !valid || busy || self.flow.standing_classifier().is_none() {
            return Err(PlaceBeginError {
                input: (prefix, tail),
                refusal: if busy {
                    PlaceError::Busy
                } else if self.flow.standing_classifier().is_none() {
                    PlaceError::ClassifierNotAdmitted
                } else if !valid {
                    PlaceError::InvalidTail
                } else {
                    PlaceError::WrongProof
                },
                failure,
                staged: None,
            });
        }
        self.barrier.set_place_owed(true);
        self.reload = Some(ReloadTransaction {
            prefix: Some(prefix),
            tail: Some(tail),
            units: vec![],
            frozen_sources: vec![],
            bound: None,
            measurement: None,
            reservation: None,
            received: None,
            failed_wire: None,
            authors: VecDeque::new(),
            author: None,
            recorded_authors: vec![],
            staged: None,
            cut_confirmed: false,
            driving: false,
            cancelled: false,
            done: false,
            incomplete: None,
            refused: false,
            rejected: None,
            reload_code: None,
            reload_detail: None,
            started_ns: None,
            account_deadline: None,
        });
        Ok(())
    }
    pub(in crate::flow) fn cancel_reload(
        &mut self,
        _io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        let Some(work) = self.reload.as_mut() else {
            return Err(RunFailure::PlaceOwned);
        };
        work.cancelled = true;
        Ok(())
    }
    pub(in crate::flow) fn advance_cut_and_reload<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<ReloadProgress, RunFailure> {
        self.live(ports.io)?;
        self.check_place_port(ports.decode.as_deref().ok_or(RunFailure::PlaceOwned)?)?;
        let Some(work) = self.reload.as_ref() else {
            return Err(RunFailure::PlaceOwned);
        };
        if work.done {
            return Ok(PlaceProgress::Complete);
        }
        if work.bound.is_none() {
            if work.cancelled {
                self.reload.as_mut().unwrap().done = true;
                return Ok(PlaceProgress::Complete);
            }
            // Preserve admitted input/task ownership until its turn yields.
            // Cut has no bound yet; the existing Decision still applies.
            if self.open_turn.is_some() || self.marks.is_some() || self.input_turn.is_some() {
                self.pump_once(ports)?;
                return Ok(PlaceProgress::Pending);
            }
            let bound = AbsoluteBound::after(self.last_ns, self.declaration.cut_wait.elapsed_ns.0)
                .map_err(|e| self.place_fault(ports.io, e.to_string()))?;
            self.arm(PumpDeadline::Cut, bound, ports.io)?;
            let sources = self
                .flow
                .ledger()
                .facts()
                .map(|f| f.source.clone())
                .collect();
            let work = self.reload.as_mut().unwrap();
            work.bound = Some(bound);
            work.started_ns = Some(self.last_ns);
            work.frozen_sources = sources;
            // Resolve this same private address only after the actual turn closes.
            // An invalidated queued address still owes its bounded refusal trace.
            match self
                .boot_journal
                .as_ref()
                .unwrap()
                .reprepare_queued(self.reload.as_ref().unwrap().prefix.as_ref().unwrap())
            {
                Ok(fresh) => self.reload.as_mut().unwrap().prefix = Some(fresh),
                Err(_) => {
                    self.refuse_reload_placement(
                        PlacementCode::InvalidCut,
                        ReloadCode::InvalidPrefix,
                        "queued cut address is no longer present",
                        ports.io,
                    )?;
                    return Ok(PlaceProgress::Progress);
                }
            }
        }
        let work = self.reload.as_ref().unwrap();
        if self.deadlines.get(PumpDeadline::Cut) != work.bound {
            return Err(self.place_fault(ports.io, "cut lost its original bound"));
        }
        if work.author.is_some() {
            self.advance_place_author(ports)?;
            if self
                .reload
                .as_ref()
                .unwrap()
                .author
                .as_ref()
                .unwrap()
                .sequence
                .is_some()
            {
                let work = self.reload.as_mut().unwrap();
                if let Some(staged) = work.staged.as_mut() {
                    staged.recorded = work.author.as_ref().unwrap().sequence;
                }
                work.author = None;
            }
            return Ok(PlaceProgress::Progress);
        }
        if let Some(event) = self.reload.as_mut().unwrap().authors.pop_front() {
            self.reload.as_mut().unwrap().author = Some(PlaceAuthor::new(event));
            return Ok(PlaceProgress::Progress);
        }
        if self.reload.as_ref().unwrap().staged.is_some() {
            self.reload.as_mut().unwrap().done = true;
            return Ok(PlaceProgress::Complete);
        }
        if self.reload.as_ref().unwrap().reservation.is_some() {
            if self.reload.as_ref().unwrap().cancelled
                && ports
                    .decode
                    .as_deref()
                    .ok_or(RunFailure::PlaceOwned)?
                    .placement_not_started()
            {
                let r = self.reload.as_mut().unwrap().reservation.take().unwrap();
                match ports.decode.as_deref_mut().unwrap().withdraw_unsent(r) {
                    Ok(()) => {
                        self.reload.as_mut().unwrap().done = true;
                        return Ok(PlaceProgress::Complete);
                    }
                    Err(e) => {
                        self.reload.as_mut().unwrap().reservation = Some(e.reservation);
                        return Err(
                            self.place_fault(ports.io, format!("cut withdrawal: {:?}", e.failure))
                        );
                    }
                }
            }
            if ports
                .decode
                .as_deref()
                .ok_or(RunFailure::PlaceOwned)?
                .held()
                .is_none()
            {
                self.pump_once(ports)?;
            }
            let reservation = self.reload.as_ref().unwrap().reservation.as_ref().unwrap();
            let decode = ports.decode.as_deref_mut().unwrap();
            let Some(received) =
                decode
                    .take_reserved(reservation)
                    .map_err(|e| RunFailure::Transport {
                        source: PumpSource::Decode,
                        failure: e,
                    })?
            else {
                return Ok(PlaceProgress::Pending);
            };
            let valid = received.matches(reservation)
                && received.after_send()
                && reservation.sent()
                && !decode.reservation_invalid(reservation);
            self.reload.as_mut().unwrap().received = Some(received);
            if !valid {
                return Err(self.place_fault(
                    ports.io,
                    "cut reply does not match complete reserved request",
                ));
            }
            let work = self.reload.as_ref().unwrap();
            let reply = work.received.as_ref().unwrap().reply();
            let counts = match (work.prefix.as_ref().unwrap().method.unwrap(), reply) {
                (
                    CutMethod::Flush,
                    DecodeReply::Answer(TokenAnswer::Flushed {
                        resident_before,
                        resident_after,
                    }),
                )
                | (
                    CutMethod::Elide,
                    DecodeReply::Answer(TokenAnswer::Elided {
                        resident_before,
                        resident_after,
                    }),
                ) => Some((*resident_before, *resident_after)),
                (_, DecodeReply::Refusal(_)) => None,
                _ => return Err(self.place_fault(ports.io, "wrong cut terminal arm")),
            };
            let reservation = self.reload.as_mut().unwrap().reservation.take().unwrap();
            if let Err(error) = ports
                .decode
                .as_deref_mut()
                .unwrap()
                .finish_reserved(reservation)
            {
                self.reload.as_mut().unwrap().reservation = Some(error.reservation);
                return Err(
                    self.place_fault(ports.io, format!("cut settlement: {:?}", error.failure))
                );
            }
            if let Some((before, after)) = counts {
                self.confirm_reload_cut(before, after, true, ports.io)?;
            } else {
                self.retain_reload_refusal(ports.io)?;
            }
            return Ok(PlaceProgress::Progress);
        }
        if self.reload.as_ref().unwrap().refused {
            self.reload.as_mut().unwrap().done = true;
            return Ok(PlaceProgress::Complete);
        }
        if self.reload.as_ref().unwrap().measurement.is_none() {
            if self.measure.is_some() {
                self.reload.as_mut().unwrap().driving = true;
                let result = (|| {
                    self.advance_measurement(ports)?;
                    let revision = self.boot_journal.as_ref().unwrap().revision().into();
                    self.take_measurement(
                        revision,
                        ports.decode.as_deref_mut().ok_or(RunFailure::PlaceOwned)?,
                        ports.io,
                    )
                })();
                self.reload.as_mut().unwrap().driving = false;
                let Some(measure) = result? else {
                    return Ok(PlaceProgress::Pending);
                };
                self.reload.as_mut().unwrap().measurement = Some(measure);
            } else {
                if self.reload.as_ref().unwrap().cancelled {
                    self.reload.as_mut().unwrap().done = true;
                    return Ok(PlaceProgress::Complete);
                }
                self.reduce_flow(ports.io)?;
                if !self.reload_tail_ready(ports.io)? {
                    self.pump_once(ports)?;
                    return Ok(PlaceProgress::Pending);
                }
                if self.reload.as_ref().unwrap().refused {
                    return Ok(PlaceProgress::Progress);
                }
                let barrier = self
                    .flow
                    .ledger()
                    .barrier(&self.reload.as_ref().unwrap().frozen_sources)
                    .map_err(|e| self.place_fault(ports.io, e.to_string()))?;
                if !barrier
                    || self.flow.ledger().facts().next().is_some()
                    || self.place.is_some()
                    || self.generation.is_some()
                    || self.output.is_some()
                    || self.filing.is_some()
                    || self.pair_work.is_some()
                    || self.pairs.executing().is_some()
                    || self.marks.is_some()
                    || self.native_marks.is_some()
                    || self.read.is_some()
                    || self.plan_summary_blocks_external()
                    || !self.native_place_cache_ready()
                    || self.flow.classifier().state() != ClassifierState::Idle
                    || self.flow.pending_check().is_some()
                    || self.flow.pending_control().is_some()
                    || self.flow.has_pending_record()
                    || self.barrier.pending().is_some()
                    || self.waiting_content.is_some()
                    || self.budget.waiting_copy().is_some()
                    || self.inbox.queued().next().is_some()
                    || self.deadlines.get(PumpDeadline::Label).is_some()
                    || ports
                        .label
                        .as_ref()
                        .is_some_and(|p| p.send_pending() || p.held().is_some())
                    || ports.gate.as_ref().is_some_and(|p| p.send_pending())
                {
                    self.pump_once(ports)?;
                    return Ok(PlaceProgress::Pending);
                }
                match self
                    .boot_journal
                    .as_ref()
                    .unwrap()
                    .refresh_drained_prefix(self.reload.as_ref().unwrap().prefix.as_ref().unwrap())
                {
                    Ok(fresh) => self.reload.as_mut().unwrap().prefix = Some(fresh),
                    Err(_) => {
                        self.refuse_reload_placement(
                            PlacementCode::StalePosition,
                            ReloadCode::InvalidPrefix,
                            "cut boundary changed during drain",
                            ports.io,
                        )?;
                        return Ok(PlaceProgress::Progress);
                    }
                }
                let work = self.reload.as_ref().unwrap();
                let journal = self.boot_journal.as_ref().unwrap();
                if journal
                    .validate_selection(
                        &work.prefix.as_ref().unwrap().projection,
                        work.units.iter().map(Arc::as_ref),
                    )
                    .is_err()
                {
                    self.refuse_reload_placement(
                        PlacementCode::InvalidPair,
                        ReloadCode::InvalidTail,
                        "reload material has incomplete source or pair coverage",
                        ports.io,
                    )?;
                    return Ok(PlaceProgress::Progress);
                }
                let mut delta = work.prefix.as_ref().unwrap().rebuild();
                for unit in &work.units {
                    delta.extend(unit.messages().iter().cloned());
                }
                let request = MeasureRequest {
                    assembly_id: work.prefix.as_ref().unwrap().assembly,
                    revision: work.prefix.as_ref().unwrap().revision(),
                    keep: work.prefix.as_ref().unwrap().n(),
                    delta,
                    tool_schemas: self.place_engine.as_ref().unwrap().schemas.clone(),
                    turn: None,
                    position: MeasurePosition {
                        resident: journal.resident(),
                        prefix_tokens: journal.prefix_tokens(),
                        capacity: journal.capacity(),
                    },
                };
                self.reload.as_mut().unwrap().driving = true;
                let result = self.begin_measurement(
                    request,
                    PumpDeadline::Cut,
                    ports.decode.as_deref_mut().ok_or(RunFailure::PlaceOwned)?,
                    ports.io,
                );
                self.reload.as_mut().unwrap().driving = false;
                if let Err(error) = result {
                    return Err(error.failure.unwrap_or_else(|| {
                        self.place_fault(
                            ports.io,
                            format!("cut measurement could not start: {:?}", error.refusal),
                        )
                    }));
                }
                return Ok(PlaceProgress::Progress);
            }
        }
        let work = self.reload.as_ref().unwrap();
        let facts = match work.measurement.as_ref().unwrap() {
            MeasureCompletion::Measured(m) => *m.facts(),
            MeasureCompletion::Cancelled(_) => {
                let work = self.reload.as_mut().unwrap();
                work.cancelled = true;
                work.done = true;
                return Ok(PlaceProgress::Complete);
            }
            MeasureCompletion::Refused(_) => {
                let work = self.reload.as_mut().unwrap();
                work.refused = true;
                work.reload_code = Some(ReloadCode::InvalidTail);
                work.reload_detail = Some("reload measurement was refused".into());
                self.place_close = Some(weaver_types::TurnFrame::carry(
                    br#"{"kind":"refused","reason":"reload measurement was refused"}"#,
                ));
                return Ok(PlaceProgress::Progress);
            }
        };
        if work.cancelled {
            self.reload.as_mut().unwrap().done = true;
            return Ok(PlaceProgress::Complete);
        }
        if let Err(code) =
            crate::flow::place::fit(&facts, self.place_engine.as_ref().unwrap().limits())
        {
            self.refuse_reload_placement(
                code,
                ReloadCode::InvalidTail,
                "reload proposal exceeds the declared placement budget",
                ports.io,
            )?;
            return Ok(PlaceProgress::Progress);
        }
        if self
            .boot_journal
            .as_ref()
            .unwrap()
            .check_projection(&work.prefix.as_ref().unwrap().projection)
            .is_err()
        {
            self.refuse_reload_placement(
                PlacementCode::StalePosition,
                ReloadCode::InvalidPrefix,
                "cut prefix changed before mutation",
                ports.io,
            )?;
            return Ok(PlaceProgress::Progress);
        }
        let directive = match work.prefix.as_ref().unwrap().method.unwrap() {
            CutMethod::Flush => TokenDirective::Flush { keep: facts.keep },
            CutMethod::Elide if facts.keep == facts.resident => {
                self.confirm_reload_cut(facts.resident, facts.keep, false, ports.io)?;
                return Ok(PlaceProgress::Progress);
            }
            CutMethod::Elide => TokenDirective::Elide {
                from: facts.keep,
                to: facts.resident,
            },
        };
        match ports
            .decode
            .as_deref_mut()
            .ok_or(RunFailure::PlaceOwned)?
            .stage_placement(directive)
        {
            Ok(DecodeStage::Queued(r)) => self.reload.as_mut().unwrap().reservation = Some(r),
            Ok(DecodeStage::Busy(_)) => return Ok(PlaceProgress::Pending),
            Err(e) => return Err(self.place_fault(ports.io, format!("cut stage: {:?}", e.failure))),
        }
        Ok(PlaceProgress::Progress)
    }
    pub(super) fn retain_reload_refusal(&mut self, io: &mut impl PumpIo) -> Result<(), RunFailure> {
        let work = self.reload.as_ref().unwrap();
        let DecodeReply::Refusal(refusal) = work.received.as_ref().unwrap().reply() else {
            return Err(RunFailure::PlaceOwned);
        };
        let fatal = matches!(refusal, TokenRefusal::NotOpen | TokenRefusal::OutOfOrder);
        let asked = match work.prefix.as_ref().unwrap().method.unwrap() {
            CutMethod::Flush => TokenAsk::Flush {
                keep: work.prefix.as_ref().unwrap().n(),
            },
            CutMethod::Elide => TokenAsk::Elide {
                from: work.prefix.as_ref().unwrap().n(),
                to: work.prefix.as_ref().unwrap().resident_before(),
            },
        };
        let record = RefusalRecord::Decode {
            asked,
            refusal: refusal.clone(),
        };
        let payload = serde_json::value::to_raw_value(&record)
            .map_err(|e| self.place_fault(io, e.to_string()))?;
        let work = self.reload.as_mut().unwrap();
        work.refused = true;
        work.reload_code = Some(ReloadCode::InvalidPrefix);
        work.reload_detail = Some("decode refused the cut without mutation".into());
        work.authors.push_back(place_event(
            Kind::Refusal,
            AuthoredSource::Runtime,
            Payload::Refusal(payload),
            None,
            None,
        ));
        if fatal {
            let _ = self.place_fault(io, "SPU refused reserved cut outside its admitted state");
        }
        Ok(())
    }
    fn refuse_reload_placement(
        &mut self,
        code: PlacementCode,
        reload_code: ReloadCode,
        detail: &str,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        let work = self.reload.as_mut().unwrap();
        work.refused = true;
        work.reload_code = Some(reload_code);
        work.reload_detail = Some(detail.into());
        let record = RefusalRecord::Placement {
            assembly_id: work.prefix.as_ref().unwrap().assembly,
            code,
            detail: detail.into(),
        };
        let payload = serde_json::value::to_raw_value(&record)
            .map_err(|e| self.place_fault(io, e.to_string()))?;
        self.reload.as_mut().unwrap().authors.push_back(place_event(
            Kind::Refusal,
            AuthoredSource::Runtime,
            Payload::Refusal(payload),
            None,
            None,
        ));
        self.place_close = Some(weaver_types::TurnFrame::carry(
            serde_json::json!({"kind":"refused","reason":detail})
                .to_string()
                .as_bytes(),
        ));
        Ok(())
    }
    fn reload_tail_ready(&mut self, io: &mut impl PumpIo) -> Result<bool, RunFailure> {
        let work = self.reload.as_mut().unwrap();
        for index in 0..work.tail.as_ref().unwrap().items.len() {
            if index < work.units.len() {
                continue;
            }
            match &work.tail.as_ref().unwrap().items[index] {
                ReloadItem::Held(unit) => work.units.push(unit.clone()),
                ReloadItem::Pending(pending) => {
                    let status = self
                        .flow
                        .source_receipt_status(&pending.lease, &pending.source)
                        .map_err(|e| RunFailure::Flow {
                            reason: FlowHaltReason::ProtocolFailed,
                            point: HaltPoint::Barrier,
                            detail: e.to_string(),
                        })?;
                    if status == LeaseSlotStatus::Pending {
                        return Ok(false);
                    }
                    let receipt = self
                        .flow
                        .take_source_receipt(&pending.lease, &pending.source)
                        .map_err(|e| RunFailure::Flow {
                            reason: FlowHaltReason::ProtocolFailed,
                            point: HaltPoint::Barrier,
                            detail: e.to_string(),
                        })?
                        .ok_or(RunFailure::PlaceOwned)?;
                    match receipt {
                        SourceDisposition::Committed(source) => {
                            match crate::flow::material::from_filed(
                                self.read_origin.clone(),
                                source,
                            ) {
                                Ok(unit) => work.units.push(Arc::new(unit)),
                                Err((source, error)) => {
                                    work.rejected = Some(SourceDisposition::Committed(source));
                                    return Err(self.place_fault(
                                        io,
                                        format!("reload committed source: {error:?}"),
                                    ));
                                }
                            }
                        }
                        rejected @ SourceDisposition::CorrectlyRejected(_) => {
                            work.rejected = Some(rejected);
                            work.refused = true;
                            work.reload_code = Some(ReloadCode::NonSte100);
                            work.reload_detail =
                                Some("pending reload Text was correctly rejected by STE100".into());
                            return Ok(true);
                        }
                    }
                }
            }
        }
        Ok(true)
    }
    pub(super) fn confirm_reload_cut(
        &mut self,
        before: u64,
        after: u64,
        mutated: bool,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        let work = self.reload.as_ref().unwrap();
        let method = work.prefix.as_ref().unwrap().method.unwrap();
        let actual = if mutated {
            work.received
                .as_ref()
                .is_some_and(|received| match (method, received.reply()) {
                    (
                        CutMethod::Flush,
                        DecodeReply::Answer(TokenAnswer::Flushed {
                            resident_before,
                            resident_after,
                        }),
                    )
                    | (
                        CutMethod::Elide,
                        DecodeReply::Answer(TokenAnswer::Elided {
                            resident_before,
                            resident_after,
                        }),
                    ) => *resident_before == before && *resident_after == after,
                    _ => false,
                })
        } else {
            method == CutMethod::Elide && before == after && work.received.is_none()
        };
        let measured = matches!(work.measurement.as_ref(),Some(MeasureCompletion::Measured(m)) if m.facts().resident==before&&m.facts().keep==after);
        if !actual
            || !measured
            || before != work.prefix.as_ref().unwrap().resident_before()
            || after != work.prefix.as_ref().unwrap().n()
        {
            return Err(self.place_fault(
                io,
                "cut confirmation does not retain the measured frozen prefix and actual terminal",
            ));
        }
        let checked = CheckedCut {
            prefix: work.prefix.as_ref().unwrap(),
            before,
            after,
        };
        self.boot_journal
            .as_mut()
            .unwrap()
            .confirm_cut(&checked)
            .map_err(|e| RunFailure::Flow {
                reason: FlowHaltReason::ProtocolFailed,
                point: HaltPoint::Barrier,
                detail: format!("cut journal: {e:?}"),
            })?;
        let work = self.reload.as_mut().unwrap();
        let MeasureCompletion::Measured(measured) = work.measurement.as_ref().unwrap() else {
            unreachable!()
        };
        let facts = *measured.facts();
        let identity = Arc::new(());
        let method = work.prefix.as_ref().unwrap().method.unwrap();
        work.staged = Some(Staged {
            identity: identity.clone(),
            origin: self.read_origin.clone(),
            assembly: work.prefix.as_ref().unwrap().assembly,
            units: work.units.clone(),
            measured: facts,
            resident_after: after,
            recorded: None,
            method,
        });
        work.cut_confirmed = true;
        self.place_engine.as_mut().unwrap().staged = Some(identity);
        if mutated {
            let (kind, payload) = match method {
                CutMethod::Flush => (
                    Kind::Flush,
                    Payload::Flush(weaver_trace::FlushCounts {
                        resident_before: before,
                        resident_after: after,
                    }),
                ),
                CutMethod::Elide => (
                    Kind::Elision,
                    Payload::Elision(weaver_trace::ElisionSpan {
                        from: after,
                        to: before,
                        resident_before: before,
                        resident_after: after,
                    }),
                ),
            };
            work.authors.push_back(place_event(
                kind,
                AuthoredSource::Runtime,
                payload,
                None,
                None,
            ));
        }
        let limits = self.place_engine.as_ref().unwrap().limits();
        let record = t::PlacementRecord {
            assembly_id: work.prefix.as_ref().unwrap().assembly.0.into(),
            phase: t::PlacementPhase::Cut,
            context: None,
            cut: work.prefix.as_ref().unwrap().cut.as_ref().map(cut_mirror),
            method: Some(method_mirror(method)),
            n: after,
            prefix_tokens: facts.prefix_tokens,
            resident_before: before,
            input_tokens: facts.input_tokens,
            capacity: facts.capacity,
            generation_limit: limits.2,
            terminal_tokens: limits.3,
            unit_map: vec![],
            message_ids: vec![],
            assistant_message_id: None,
            recomputed_messages: (work.prefix.as_ref().unwrap().rebuild().len() as u64).into(),
            wait_ns: self.last_ns.saturating_sub(work.started_ns.unwrap()).into(),
            open_evidence: None,
        };
        work.authors.push_back(place_event(
            Kind::StatePlacement,
            AuthoredSource::Loop,
            Payload::Placement(Box::new(record)),
            None,
            None,
        ));
        Ok(())
    }
    pub(in crate::flow) fn take_reload(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<Option<ReloadCompletion>, RunFailure> {
        self.live(io)?;
        if !self.reload.as_ref().is_some_and(|w| w.done) {
            return Ok(None);
        }
        self.consume_place_completion()?;
        let mut work = self.reload.take().unwrap();
        self.barrier.set_place_owed(self.place_owed());
        self.deadlines.disarm(PumpDeadline::Cut);
        if !work.cancelled
            && let Some(staged) = work.staged.take()
        {
            return Ok(Some(ReloadCompletion::Staged(staged)));
        }
        let cancelled = work.cancelled;
        let account = PlaceAccount {
            work: AccountWork::Reload(Box::new(work)),
            measure: None,
        };
        Ok(Some(if cancelled {
            ReloadCompletion::Cancelled(account)
        } else {
            ReloadCompletion::Refused(account)
        }))
    }
    #[allow(clippy::result_large_err)]
    pub(in crate::flow) fn begin_staged_place(
        &mut self,
        staged: Staged,
        selection: Vec<crate::flow::material::SelectedUnit>,
        context: PlacementContext,
        decode: &mut DecodePort,
        io: &mut impl PumpIo,
    ) -> Result<(), StagedPlaceError> {
        if !Arc::ptr_eq(&staged.origin, &self.read_origin)
            || !self
                .place_engine
                .as_ref()
                .and_then(|e| e.staged.as_ref())
                .is_some_and(|id| Arc::ptr_eq(id, &staged.identity))
        {
            return Err(StagedPlaceError {
                staged,
                selection,
                context,
                refusal: PlaceError::WrongProof,
                failure: None,
            });
        }
        let prefix = match self
            .boot_journal
            .as_ref()
            .unwrap()
            .prepare(None, staged.method)
        {
            Ok(p) => p,
            Err(_) => {
                return Err(StagedPlaceError {
                    staged,
                    selection,
                    context,
                    refusal: PlaceError::Busy,
                    failure: None,
                });
            }
        };
        let request = PlaceRequest {
            prefix,
            selection,
            tool_schemas: self.place_engine.as_ref().unwrap().schemas.clone(),
            context,
        };
        self.begin_place_inner(request, Some(staged), decode, io)
            .map_err(|error| StagedPlaceError {
                staged: error.staged.expect("staged begin preserves its owner"),
                selection: error.input.selection,
                context: error.input.context,
                refusal: error.refusal,
                failure: error.failure,
            })
    }
}
#[derive(Debug)]
pub(in crate::flow) struct StagedPlaceError {
    pub staged: Staged,
    pub selection: Vec<crate::flow::material::SelectedUnit>,
    pub context: PlacementContext,
    pub refusal: PlaceError,
    pub failure: Option<RunFailure>,
}
