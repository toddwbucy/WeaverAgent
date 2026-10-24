//! Bounded stopped custody; no trace admission can recover a placement capability.
use super::*;
use crate::flow::pump::LifecyclePorts;

pub(in crate::flow) struct OwedPlaceAuthor {
    event: AuthoredEvent,
}
impl OwedPlaceAuthor {
    pub(in crate::flow) fn into_event(self) -> AuthoredEvent {
        self.event
    }
}
pub(in crate::flow) struct PlaceReleasePermit {
    ticket: Option<u64>,
}
impl PlaceReleasePermit {
    pub(in crate::flow) fn matches(&self, barrier: &RecordBarrier) -> bool {
        match (self.ticket, barrier.pending_author()) {
            (Some(a), Some(b)) => a == b.ticket,
            (None, None) => barrier.pending().is_none(),
            _ => false,
        }
    }
}
impl RecordingContext {
    pub(in crate::flow) fn place_owed(&self) -> bool {
        self.place_notice.as_ref().is_some_and(|w| !w.done)
            || self.native_marks.as_ref().is_some_and(|w| !w.done)
            || self.place.as_ref().is_some_and(|w| !w.done)
            || self.generation.as_ref().is_some_and(|w| !w.done)
            || self.reload.as_ref().is_some_and(|w| !w.done)
            || self
                .place_engine
                .as_ref()
                .is_some_and(|e| e.outstanding.is_some())
    }
    pub(in crate::flow) fn check_place_port(&self, decode: &DecodePort) -> Result<(), RunFailure> {
        // The immutable boot identity survives terminal settlement and an
        // abandoned reservation; stopped disposal must retire that same port.
        self.check_boot_open_port(decode)
            .map_err(|_| RunFailure::PlaceOwned)?;
        if self
            .generation
            .as_ref()
            .and_then(|w| w.reservation.as_ref())
            .or_else(|| self.reload.as_ref().and_then(|w| w.reservation.as_ref()))
            .is_some_and(|r| !r.belongs_to(decode))
        {
            return Err(RunFailure::PlaceOwned);
        }
        Ok(())
    }
    pub(in crate::flow) fn revoke_place(&mut self) {
        if let Some(w) = &mut self.place_notice {
            w.cancelled = true;
        }
        if let Some(w) = &mut self.native_marks {
            w.cancelled = true;
        }
        if let Some(w) = &mut self.place {
            w.cancelled = true;
        }
        if let Some(w) = &mut self.generation {
            w.cancelled = true;
        }
        if let Some(w) = &mut self.reload {
            w.cancelled = true;
        }
    }
    pub(in crate::flow) fn place_admission_stopped(&self) -> bool {
        self.place_turn_stopped || self.place_leaving
    }
    pub(in crate::flow) fn observe_place_lifecycle(&mut self, ports: &LifecyclePorts<'_>) {
        let Some(peer) = ports.peer.as_deref().filter(|p| p.cancels_boot_open()) else {
            return;
        };
        let stop = matches!(
            &peer.held().unwrap().envelope.payload,
            weaver_types::Payload::Directive(weaver_types::LifecycleDirective::Stop { .. })
        );
        if stop {
            self.place_turn_stopped |= self.open_turn.is_some();
        } else {
            self.place_leaving = true;
        }
        if let Some(identity) = self
            .place_engine
            .as_ref()
            .and_then(|engine| engine.outstanding.as_ref())
        {
            self.place_revoked_grant = Some(identity.clone());
        }
        self.revoke_place();
    }
    pub(in crate::flow) fn place_first_send(
        &mut self,
        decode: Option<&DecodePort>,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        let Some(decode) = decode else { return Ok(()) };
        if self.place_engine.is_some() && decode.unowned_mutation_not_started() {
            return Err(RunFailure::PlaceOwned);
        }
        if !decode.placement_not_started() {
            return Ok(());
        }
        if self.place_admission_stopped() {
            return Err(RunFailure::PlaceOwned);
        }
        self.check_place_port(decode)?;
        let cache_ready = self.committed_plan_summary().is_ok();
        if let Some(w) = &self.generation {
            if w.cancelled
                || !cache_ready
                || w.grant.as_ref().is_none_or(|g| {
                    self.boot_journal
                        .as_ref()
                        .unwrap()
                        .check_pending(&g.pending)
                        .is_err()
                })
            {
                return Err(RunFailure::PlaceOwned);
            }
        } else if let Some(w) = &self.reload {
            if w.cancelled
                || self
                    .boot_journal
                    .as_ref()
                    .unwrap()
                    .check_projection(&w.live_prefix().projection)
                    .is_err()
            {
                return Err(RunFailure::PlaceOwned);
            }
        } else {
            return Err(self.place_fault(io, "reserved mutation has no place owner"));
        }
        Ok(())
    }
    pub(in crate::flow) fn freeze_place_account_bound(&mut self) {
        let inherited = self
            .deadlines
            .get(PumpDeadline::Leave)
            .map(AbsoluteBound::deadline_ns)
            .into_iter()
            .chain(self.flow.halt().and_then(|h| h.deadline().ok()))
            .min();
        if let Some(w) = &mut self.place_notice {
            let bound = w.bound.deadline_ns().min(inherited.unwrap_or(u64::MAX));
            w.account_deadline = Some(w.account_deadline.unwrap_or(bound).min(bound));
        }
        if let Some(w) = &mut self.native_marks {
            let bound = w.bound.deadline_ns().min(inherited.unwrap_or(u64::MAX));
            w.account_deadline = Some(w.account_deadline.unwrap_or(bound).min(bound));
        }
        if let Some(w) = &mut self.place {
            let bound = w.bound.deadline_ns().min(inherited.unwrap_or(u64::MAX));
            w.account_deadline = Some(w.account_deadline.unwrap_or(bound).min(bound));
        }
        if let Some(w) = &mut self.reload {
            let bound = w
                .bound
                .map(AbsoluteBound::deadline_ns)
                .unwrap_or(u64::MAX)
                .min(inherited.unwrap_or(u64::MAX));
            w.account_deadline = Some(w.account_deadline.unwrap_or(bound).min(bound));
        }
        if let Some(w) = &mut self.generation {
            let bound = self
                .deadlines
                .get(PumpDeadline::Decision)
                .map(AbsoluteBound::deadline_ns)
                .unwrap_or(u64::MAX)
                .min(inherited.unwrap_or(u64::MAX));
            w.account_deadline = Some(w.account_deadline.unwrap_or(bound).min(bound));
        }
    }
    pub(in crate::flow) fn place_account_clock(&mut self, io: &mut impl PumpIo) -> bool {
        self.freeze_place_account_bound();
        let bound = self
            .place
            .as_ref()
            .and_then(|w| w.account_deadline)
            .into_iter()
            .chain(self.generation.as_ref().and_then(|w| w.account_deadline))
            .chain(self.reload.as_ref().and_then(|w| w.account_deadline))
            .chain(self.native_marks.as_ref().and_then(|w| w.account_deadline))
            .chain(self.place_notice.as_ref().and_then(|w| w.account_deadline))
            .min()
            .unwrap_or(0);
        match io.now() {
            Ok(now) if now >= self.last_ns && now < bound => {
                self.last_ns = now;
                true
            }
            _ => false,
        }
    }
    fn place_release_owned(&self) -> bool {
        (self.place.is_some()
            || self.generation.is_some()
            || self.reload.is_some()
            || self.native_marks.is_some()
            || self.place_notice.is_some())
            && self.place_owed()
            && self.phase == RecordingPhase::Entered
    }
    pub(in crate::flow) fn begin_place_release(
        &mut self,
        leave: AbsoluteBound,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        let owns_release = self.place_release_owned();
        // Preserve revocation on refused unheld attempts. A custody hold must
        // leave the admitted operation live until its owner takes the original.
        if owns_release && !self.caller_author_held() {
            self.revoke_place();
        }
        self.release_live(io)?;
        if !self.place_release_owned() {
            return Err(RunFailure::PlaceOwned);
        }
        // Foreign content/classification is never silently waived by this seal.
        if self.output.is_some()
            || self.filing.is_some()
            || self.pair_work.is_some()
            || self.read.is_some()
            || self.member_capture.is_some()
            || self.input.snapshot().is_some()
        {
            return Err(RunFailure::PlaceOwned);
        }
        if let Some(held) = self.deadlines.get(PumpDeadline::Leave)
            && held != leave
        {
            return Err(RunFailure::PlaceOwned);
        }
        self.check_held_input()?;
        if self.deadlines.get(PumpDeadline::Leave).is_none() {
            self.deadlines
                .arm(PumpDeadline::Leave, leave)
                .map_err(|_| RunFailure::PlaceOwned)?;
        }
        self.freeze_place_account_bound();
        let permit = PlaceReleasePermit {
            ticket: self
                .active_place_author()
                .and_then(|a| a.ticket)
                .or_else(|| self.place_measure_author_ticket()),
        };
        self.flow
            .begin_place_releasing(
                &mut FlowRecording {
                    barrier: &mut self.barrier,
                    inbox: &mut self.inbox,
                    budget: &mut self.budget,
                    author: &self.author,
                },
                &permit,
            )
            .map_err(|f| self.flow_failed(io, f))?;
        self.barrier.stop_for_flow_failure();
        self.phase = RecordingPhase::Releasing;
        Ok(())
    }
    pub(in crate::flow) fn retain_place_wire(&mut self, decode: &mut DecodePort) {
        if let Some(w) = &mut self.generation
            && let Some(r) = &w.reservation
            && r.belongs_to(decode)
        {
            if w.terminal.is_none()
                && let Ok(Some(received)) = decode.take_reserved(r)
            {
                w.terminal = Some(received);
            }
            if w.failed_wire.is_none()
                && let Ok(wire) = decode.take_failed_wire_reserved(r)
            {
                w.failed_wire = wire;
            }
        }
        if let Some(w) = &mut self.reload
            && let Some(r) = &w.reservation
            && r.belongs_to(decode)
        {
            if w.received.is_none()
                && let Ok(Some(received)) = decode.take_reserved(r)
            {
                w.received = Some(received);
            }
            if w.failed_wire.is_none()
                && let Ok(wire) = decode.take_failed_wire_reserved(r)
            {
                w.failed_wire = wire;
            }
        }
    }
    fn abandon_place_account(&mut self, decode: &mut DecodePort, detail: &str) {
        self.retain_place_wire(decode);
        if let Some(ticket) = self.active_place_author().and_then(|a| a.ticket) {
            let event = self
                .barrier
                .pending_author()
                .filter(|held| held.ticket == ticket)
                .map(|held| held.event.clone());
            if self.barrier.abandon_boot_author(ticket)
                && let Some(author) = self.active_place_author_mut()
            {
                author.event = event;
            }
        }
        let waiting_claim = self
            .active_place_author()
            .map(|a| AuthorClaim::Placement(Arc::clone(&a.identity)));
        if self.place_owns_pending_author()
            && let Some(claim) = waiting_claim
            && let Some(event) = self.take_claimed_waiting_author(&claim)
            && let Some(a) = self.active_place_author_mut()
        {
            a.event = Some(event);
        }
        self.retain_measure_transport_evidence(decode);
        if self.measure.is_some() {
            self.place_stopped_measure = self.take_stopped_place_measure();
        }
        if let Some(w) = &mut self.place_notice {
            w.cancelled = true;
            w.done = true;
            w.incomplete.get_or_insert_with(|| detail.into());
        }
        if let Some(w) = &mut self.native_marks {
            w.cancelled = true;
            w.done = true;
            w.incomplete.get_or_insert_with(|| detail.into());
        }
        if let Some(w) = &mut self.place {
            w.cancelled = true;
            w.done = true;
            w.incomplete.get_or_insert_with(|| detail.into());
        }
        if let Some(w) = &mut self.generation {
            if let Some(r) = &w.reservation
                && w.terminal.is_none()
                && let Ok(Some(received)) = decode.take_reserved(r)
            {
                w.terminal = Some(received);
            }
            w.reservation.take();
            w.cancelled = true;
            w.done = true;
            w.incomplete.get_or_insert_with(|| detail.into());
        }
        if let Some(w) = &mut self.reload {
            if let Some(r) = &w.reservation
                && w.received.is_none()
                && let Ok(Some(received)) = decode.take_reserved(r)
            {
                w.received = Some(received);
            }
            w.reservation.take();
            w.cancelled = true;
            w.done = true;
            w.incomplete.get_or_insert_with(|| detail.into());
        }
        self.barrier.set_place_owed(false);
    }
    fn stopped_place_author(&mut self, io: &mut impl PumpIo) -> Result<bool, RunFailure> {
        let Some(a) = self.active_place_author() else {
            return Ok(true);
        };
        if a.sequence.is_some() {
            self.consume_place_completion()?;
            return Ok(true);
        }
        let claim = AuthorClaim::Placement(Arc::clone(&a.identity));
        if let Some(ticket) = a.ticket {
            if self.barrier.pending_author().map(|a| a.ticket) != Some(ticket) {
                return Err(RunFailure::PlaceOwned);
            }
        } else {
            if self.barrier.pending().is_some()
                || self.barrier.pending_author().is_some()
                || self.flow.has_pending_record()
            {
                return Ok(false);
            }
            let wall = wall_ms().map_err(|e| self.place_fault(io, e))?;
            let event = self
                .active_place_author_mut()
                .unwrap()
                .event
                .take()
                .or_else(|| self.take_claimed_waiting_author(&claim))
                .ok_or(RunFailure::PlaceOwned)?;
            let permit = OwedPlaceAuthor { event };
            if let Err(error) =
                self.barrier
                    .queue_place_account(permit, &self.author, self.last_ns, wall)
            {
                self.active_place_author_mut().unwrap().event = Some(error.event);
                return Err(self.place_fault(io, error.detail));
            }
            self.active_place_author_mut().unwrap().ticket =
                self.barrier.pending_author().map(|a| a.ticket);
        }
        match self.resume_trace_only_and_bind() {
            Ok(Some(sequence)) => {
                self.bind_place_author(sequence);
                Ok(true)
            }
            Ok(None) => Ok(false),
            Err(error) => Err(self.place_fault(io, format!("stopped place trace: {error:?}"))),
        }
    }
    pub(in crate::flow) fn advance_place_account<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<PlaceProgress, RunFailure> {
        if !matches!(
            self.phase,
            RecordingPhase::Releasing | RecordingPhase::Failed
        ) {
            return Err(RunFailure::PlaceOwned);
        }
        self.check_place_port(ports.decode.as_deref().ok_or(RunFailure::PlaceOwned)?)?;
        let result = self.advance_place_account_inner(ports);
        if let Err(error) = &result {
            // The exact stopped owner settles incomplete once; a failed stream
            // or recorder cannot leave it retrying the same unavailable port.
            self.abandon_place_account(
                ports.decode.as_deref_mut().unwrap(),
                &format!("stopped placement accounting failed: {error:?}"),
            );
        }
        result
    }
    fn advance_place_account_inner<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<PlaceProgress, RunFailure> {
        if !matches!(
            self.phase,
            RecordingPhase::Releasing | RecordingPhase::Failed
        ) {
            return Err(RunFailure::PlaceOwned);
        }
        let decode = ports.decode.as_deref_mut().ok_or(RunFailure::PlaceOwned)?;
        self.check_place_port(decode)?;
        self.revoke_place();
        if !self.place_account_clock(ports.io) {
            self.abandon_place_account(
                decode,
                "placement account reached its inherited bound or lost its clock",
            );
            return Ok(PlaceProgress::Complete);
        }
        if self.phase == RecordingPhase::Failed
            && !self.halt_report.as_ref().is_some_and(|r| r.attempted())
        {
            return Ok(PlaceProgress::Pending);
        }
        if self.measure.is_some() {
            if !self.advance_stopped_place_measure(decode, ports.io)? {
                return Ok(PlaceProgress::Pending);
            }
            self.place_stopped_measure = self.take_stopped_place_measure();
        }
        if self.active_place_author().is_some() {
            if !self.stopped_place_author(ports.io)? {
                return Ok(PlaceProgress::Pending);
            }
            // Retire only the exact priority owner whose author was resumed.
            // A queued cut can coexist with an earlier owner's owed records.
            if self.place.as_ref().is_some_and(|w| w.author.is_some()) {
                self.retain_place_return_record(ports.io)?;
                let w = self.place.as_mut().unwrap();
                w.author = None;
                w.done = true;
            } else if self.generation.as_ref().is_some_and(|w| w.author.is_some()) {
                let w = self.generation.as_mut().unwrap();
                w.author = None;
                w.intermediates.clear();
            } else if self.reload.as_ref().is_some_and(|w| w.author.is_some()) {
                self.reload.as_mut().unwrap().author = None;
            } else if self
                .native_marks
                .as_ref()
                .is_some_and(|w| w.author.is_some())
            {
                self.retain_native_recorded()?;
                self.native_marks.as_mut().unwrap().author = None;
            } else if let Some(w) = &mut self.place_notice {
                w.author = None;
            }
            return Ok(PlaceProgress::Progress);
        }
        if self.prepare_place_notice_author() {
            return Ok(PlaceProgress::Progress);
        }
        if self.prepare_native_mark_author(ports.io)? {
            return Ok(PlaceProgress::Progress);
        }
        if let Some(event) = self.generation.as_mut().and_then(|w| w.authors.pop_front()) {
            self.generation.as_mut().unwrap().author = Some(PlaceAuthor::new(event));
            return Ok(PlaceProgress::Progress);
        }
        if let Some(event) = self.reload.as_mut().and_then(|w| w.authors.pop_front()) {
            self.reload.as_mut().unwrap().author = Some(PlaceAuthor::new(event));
            return Ok(PlaceProgress::Progress);
        }
        if self
            .generation
            .as_ref()
            .is_some_and(|w| w.reservation.is_some())
        {
            self.cancel_generation(decode, ports.io)?;
            if self.generation.as_ref().unwrap().reservation.is_some() {
                self.pump
                    .stopped_decode_step(decode)
                    .map_err(|e| RunFailure::Transport {
                        source: PumpSource::Decode,
                        failure: e,
                    })?;
                if !self.place_account_clock(ports.io) {
                    self.abandon_place_account(
                        decode,
                        "placement reply arrived after the inherited account bound",
                    );
                    return Ok(PlaceProgress::Complete);
                }
                let r = self
                    .generation
                    .as_ref()
                    .unwrap()
                    .reservation
                    .as_ref()
                    .unwrap();
                if let Some(received) =
                    decode.take_reserved(r).map_err(|e| RunFailure::Transport {
                        source: PumpSource::Decode,
                        failure: e,
                    })?
                {
                    if !received.matches(r)
                        || !received.after_send()
                        || !r.sent()
                        || decode.reservation_invalid(r)
                    {
                        self.generation.as_mut().unwrap().terminal = Some(received);
                        return Err(RunFailure::PlaceOwned);
                    }
                    self.consume_generation(received, ports.io)?;
                }
                if decode.reservation_settled(
                    self.generation
                        .as_ref()
                        .unwrap()
                        .reservation
                        .as_ref()
                        .unwrap(),
                ) {
                    let r = self
                        .generation
                        .as_mut()
                        .unwrap()
                        .reservation
                        .take()
                        .unwrap();
                    if let Err(e) = decode.finish_reserved(r) {
                        self.generation.as_mut().unwrap().reservation = Some(e.reservation);
                        return Err(RunFailure::PlaceOwned);
                    }
                }
            }
            return Ok(PlaceProgress::Progress);
        }
        if self
            .reload
            .as_ref()
            .is_some_and(|w| w.reservation.is_some())
        {
            if decode.placement_not_started() {
                let r = self.reload.as_mut().unwrap().reservation.take().unwrap();
                if let Err(e) = decode.withdraw_unsent(r) {
                    self.reload.as_mut().unwrap().reservation = Some(e.reservation);
                    return Err(RunFailure::PlaceOwned);
                }
            } else {
                self.pump
                    .stopped_decode_step(decode)
                    .map_err(|e| RunFailure::Transport {
                        source: PumpSource::Decode,
                        failure: e,
                    })?;
                if !self.place_account_clock(ports.io) {
                    self.abandon_place_account(
                        decode,
                        "placement reply arrived after the inherited account bound",
                    );
                    return Ok(PlaceProgress::Complete);
                }
                let r = self.reload.as_ref().unwrap().reservation.as_ref().unwrap();
                if let Some(received) =
                    decode.take_reserved(r).map_err(|e| RunFailure::Transport {
                        source: PumpSource::Decode,
                        failure: e,
                    })?
                {
                    let valid = received.matches(r)
                        && received.after_send()
                        && r.sent()
                        && !decode.reservation_invalid(r);
                    let method = self.reload.as_ref().unwrap().live_prefix().method.unwrap();
                    let counts = match (method, received.reply()) {
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
                        _ => {
                            self.reload.as_mut().unwrap().received = Some(received);
                            return Err(self.place_fault(
                                ports.io,
                                "stopped cut received a different terminal arm",
                            ));
                        }
                    };
                    self.reload.as_mut().unwrap().received = Some(received);
                    if !valid {
                        return Err(RunFailure::PlaceOwned);
                    }
                    let r = self.reload.as_mut().unwrap().reservation.take().unwrap();
                    if let Err(e) = decode.finish_reserved(r) {
                        self.reload.as_mut().unwrap().reservation = Some(e.reservation);
                        return Err(RunFailure::PlaceOwned);
                    }
                    if let Some((before, after)) = counts {
                        self.confirm_reload_cut(before, after, true, ports.io)?;
                    } else {
                        self.retain_reload_refusal(ports.io)?;
                    }
                }
            }
            return Ok(PlaceProgress::Progress);
        }
        if let Some(w) = &mut self.place_notice {
            w.done = true;
        }
        if let Some(w) = &mut self.native_marks {
            w.done = true;
        }
        if let Some(w) = &mut self.place {
            w.done = true;
        }
        if let Some(w) = &mut self.generation {
            w.done = true;
        }
        if let Some(w) = &mut self.reload {
            w.done = true;
        }
        self.barrier.set_place_owed(false);
        Ok(PlaceProgress::Complete)
    }
    #[allow(clippy::result_large_err)]
    pub(in crate::flow) fn retire_place_decode(
        &self,
        decode: DecodePort,
    ) -> Result<(), DecodePort> {
        if !matches!(
            self.phase,
            RecordingPhase::Releasing | RecordingPhase::Failed
        ) || self.place_owed()
            || self.check_place_port(&decode).is_err()
        {
            return Err(decode);
        }
        drop(decode);
        Ok(())
    }
    pub(in crate::flow) fn take_place_account(&mut self) -> Option<PlaceAccount> {
        if !matches!(
            self.phase,
            RecordingPhase::Releasing | RecordingPhase::Failed
        ) || self.place.as_ref().is_some_and(|w| !w.done)
            || self.generation.as_ref().is_some_and(|w| !w.done)
            || self.reload.as_ref().is_some_and(|w| !w.done)
            || self.native_marks.as_ref().is_some_and(|w| !w.done)
            || self.place_notice.as_ref().is_some_and(|w| !w.done)
        {
            return None;
        }
        let work = if let Some(w) = self.place.take() {
            AccountWork::Place(Box::new(w))
        } else if let Some(w) = self.generation.take() {
            AccountWork::Generation(Box::new(w))
        } else if let Some(w) = self.reload.take() {
            AccountWork::Reload(Box::new(w))
        } else if let Some(w) = self.take_native_account_work() {
            AccountWork::Native(Box::new(w))
        } else {
            AccountWork::Notice(Box::new(self.place_notice.take()?))
        };
        if let Some(e) = &mut self.place_engine {
            e.outstanding = None;
            e.staged = None;
        }
        Some(PlaceAccount {
            phase: self.phase,
            work,
            measure: self.place_stopped_measure.take(),
        })
    }
}
