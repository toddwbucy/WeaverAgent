//! A single-use replay keeps its entered management and decode owner until Leave.
use super::*;
use crate::channel::DecodeReply;

impl Setup<'_> {
    pub(super) fn await_replay_leave(&mut self, run: &mut Run) -> Result<(), ChannelFault> {
        while self.leaving.is_none() {
            let Some(spu) = run.spu.as_ref() else {
                self.poll(None, None, None)?;
                continue;
            };
            let (ready, _) =
                self.poll(Some((spu.decode.as_fd(), PollFlags::POLLIN)), None, None)?;
            if !ready {
                continue;
            }
            // Each segment keeps management available too. A Leave accepted
            // during assembly caps that retained receive with its original bound.
            let mut wait_failed = false;
            let answer = spu.decode.recv_managed(&mut |fd, events| {
                let waited = self.wait(fd, events, None);
                wait_failed |= waited.is_err();
                waited
            });
            if wait_failed {
                // A listener or enclosing-deadline failure is not evidence
                // that the SPU closed its decode channel.
                return Err(answer.expect_err("a failed wait ends the receive"));
            }
            match answer {
                Ok(DecodeReply::Answer(weaver_types::TokenAnswer::Fault(report))) => {
                    self.idle_report(run, Subsystem::SpuDecoder, &report)?;
                }
                Err(ChannelFault::Closed) => self.idle_spu_died(run)?,
                other => {
                    // No exchange remains open. A second terminal or malformed
                    // report follows the existing bounded protocol-failure path.
                    self.replay
                        .as_mut()
                        .unwrap()
                        .failure_deadline
                        .get_or_insert(
                            shared_deadline(Instant::now()).ok_or(ChannelFault::Closed)?,
                        );
                    let fault = match other {
                        Err(fault) => fault,
                        Ok(_) => ChannelFault::Undecodable,
                    };
                    let report = crate::authorship::harness_report(
                        weaver_types::FaultCase::MessageRecordUndecodable,
                        &serde_json::json!({"organ":"spu", "fault":format!("{fault:?}")})
                            .to_string(),
                    );
                    let _ = self.idle_report(run, Subsystem::Harness, &report);
                    return Err(fault);
                }
            }
        }
        Ok(())
    }

    pub(super) fn idle_report(
        &mut self,
        run: &mut Run,
        subsystem: Subsystem,
        report: &weaver_types::FaultReport,
    ) -> Result<(), ChannelFault> {
        let Some(raw) = serde_json::to_string(report)
            .ok()
            .and_then(|text| weaver_trace::raw_payload(&text))
        else {
            run.recorder.latch_unrendered();
            return Err(ChannelFault::Undecodable);
        };
        let event = run.author.event(
            weaver_trace::Kind::Fault,
            subsystem,
            None,
            Some(weaver_trace::Payload::Fault(raw)),
        );
        run.recorder
            .submit_managed_diagnostic(event, &mut |fd| {
                replay::record_nonblocking(fd)?;
                self.wait(fd, PollFlags::POLLOUT, None)
                    .map_err(|_| std::io::Error::from(std::io::ErrorKind::BrokenPipe))
            })
            .map_err(|_| ChannelFault::Closed)?;
        Ok(())
    }

    pub(super) fn record_spu_death(
        &mut self,
        run: &mut Run,
        death: &str,
    ) -> Result<(), ChannelFault> {
        let mut account = serde_json::json!({"organ":"spu", "death":death});
        if let Some(line) = run.spu.as_ref().and_then(|spu| spu.last_word.take()) {
            account["last_word"] =
                serde_json::from_str(&line).unwrap_or(serde_json::Value::String(line));
        }
        self.idle_report(
            run,
            Subsystem::Harness,
            &crate::authorship::harness_report(
                weaver_types::FaultCase::OrganDeathObserved,
                &account.to_string(),
            ),
        )
    }

    pub(super) fn idle_spu_died(&mut self, run: &mut Run) -> Result<(), ChannelFault> {
        self.record_spu_death(run, "channel closed, the run standing")?;
        // S2 serves on only after its death record holds. Keep actual child
        // custody if the peer closed its channel without ending its process.
        let spu = run.spu.take().expect("the observed SPU remains owned");
        let mut children = Children(vec![spu.pid]);
        drop(spu);
        if let Some(leave) = self.leaving.as_mut() {
            leave.forced = true;
        }
        let deadline = lower_window().ok_or(ChannelFault::Closed)?;
        if let Err(fault) = children.reap_by(self, Some(deadline)) {
            self.live(None)?;
            if Instant::now() < deadline {
                return Err(fault);
            }
            // The existing lower-bound fallback kills, then reaps. Children
            // owns that fallback even if management failed in the same wait.
            drop(children);
        }
        self.live(None)
    }
}
