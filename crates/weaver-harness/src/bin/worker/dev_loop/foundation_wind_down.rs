//! The worker chooses a pass count and retains its election across management.
//! The harness alone owns the original Leave, certification and phase window.
use super::*;
use weaver_harness::{FoundationRestatement, FoundationWindDownProgress as W};

pub(super) struct Policy {
    remaining: usize,
    family: &'static FoundationRestatement,
    detail: Option<String>,
}
impl Policy {
    pub(super) fn new(passes: usize, family: &'static FoundationRestatement) -> Option<Self> {
        (passes > 0).then_some(Self {
            remaining: passes,
            family,
            detail: None,
        })
    }
    pub(super) fn detail(&self) -> Option<&str> {
        self.detail.as_deref()
    }
    pub(super) fn advance(&mut self, input: &mut FoundationInput<'_>) {
        if self.detail.is_some() {
            input.wait_for_wake();
            return;
        }
        let elected = match input.wind_down_progress().unwrap() {
            W::Pending => {
                input.advance();
                true
            }
            W::Placed => input.generate_wind_down(),
            W::Generated => input.repair_wind_down(self.family),
            W::Restatement => input.place_wind_down_restatement(),
            W::CorrectionPlaced => input.generate_wind_down_correction(),
            W::Effective | W::Refused => input.finish_wind_down_pass(),
            W::PassComplete if self.remaining > 1 && input.wind_down_can_continue() => {
                if input.next_wind_down_pass() {
                    self.remaining -= 1;
                    true
                } else {
                    false
                }
            }
            W::PassComplete => input.close_wind_down(),
            W::Complete => true,
        };
        if !elected {
            self.detail = Some("wind-down election refused".into());
        }
    }
}
