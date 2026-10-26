//! Loop-elected answer/refusal closes use the same Recorded world-close owner.
use super::super::close::WorldClose;
use super::*;

impl FoundationInput<'_> {
    pub(in crate::flow::context::worker_startup::input) fn final_send_clock_failed(&self) -> bool {
        self.engine
            .as_ref()
            .and_then(|work| work.answer.as_ref())
            .and_then(WorldClose::send)
            .is_some_and(|send| send.sent() && send.sent_at().is_none())
    }
    #[cfg(test)]
    pub(super) fn final_send_probe_for_test(&self) -> Box<dyn Fn() -> bool> {
        self.engine
            .as_ref()
            .unwrap()
            .answer
            .as_ref()
            .unwrap()
            .send()
            .unwrap()
            .sent_probe_for_test()
    }
    /// Close with the Answer blocks selected by the recorded outbound sort.
    /// Every conforming request must first have a terminal filing receipt.
    pub fn close_answer(&mut self) -> Result<(), FoundationInputProgress> {
        if self.progress() != FoundationInputProgress::PairsComplete {
            return Err(self.progress());
        }
        let Some((text, finish)) = self
            .engine
            .as_ref()
            .unwrap()
            .emission
            .as_ref()
            .unwrap()
            .selected_answer()
        else {
            return Err(self.progress());
        };
        let close = WorldClose::answered(self, text, finish).ok_or(self.progress())?;
        let work = self.engine.as_mut().unwrap();
        work.answer = Some(close);
        work.stage = EngineStage::Answer;
        Ok(())
    }
    /// Elect the existing refused close after an operation returns its account.
    /// Emission refusals first settle all owed terminals. No raw text or caller
    /// receipt grants this transition.
    pub fn close_operation_refusal(&mut self) -> Result<(), FoundationInputProgress> {
        if self.progress() != FoundationInputProgress::OperationRefused {
            return Err(self.progress());
        }
        if self.close_correction_refusal().is_ok() {
            return Ok(());
        }
        let work = self.engine.as_mut().unwrap();
        if work.stage == EngineStage::Emission
            && work
                .emission
                .as_mut()
                .is_some_and(|work| work.close_refusal())
        {
            return Ok(());
        }
        let work = self.engine.as_ref().unwrap();
        if work.stage == EngineStage::Refused && work.admission.is_some() {
            let close = WorldClose::refused_admission(self).ok_or(self.progress())?;
            let work = self.engine.as_mut().unwrap();
            work.answer = Some(close);
            work.stage = EngineStage::Answer;
            return Ok(());
        }
        let work = self.engine.as_ref().unwrap();
        if work.stage != EngineStage::Refused
            || !(matches!(work.task, Some(TaskCompletion::Refused { .. }))
                || matches!(work.generation, Some(GenerationCompletion::Refused(_))))
        {
            return Err(self.progress());
        }
        let detail = work
            .detail
            .clone()
            .or_else(|| match work.generation.as_ref() {
                Some(GenerationCompletion::Refused(account)) => {
                    account.refusal().map(|r| format!("generation: {r:?}"))
                }
                _ => None,
            })
            .unwrap_or_else(|| "operation refused".into());
        let close = WorldClose::refused(self).ok_or(self.progress())?;
        let work = self.engine.as_mut().unwrap();
        work.detail = Some(detail);
        work.answer = Some(close);
        work.stage = EngineStage::Answer;
        Ok(())
    }
}
