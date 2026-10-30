//! The private execution effect retains exact source and complete filing identity.
use super::*;
use crate::flow::author::{AuthoredEvent, AuthoredSource};
use crate::flow::clock::AbsoluteBound;
use crate::flow::context::FilingCompletion;
use crate::flow::ledger::{LedgerFile, SourceDisposition};
use crate::flow::pump::PumpDeadline;
use crate::flow::templates::TemplateExpansion;
use std::sync::Arc;
use weaver_trace::{Sequence, TurnRef};

pub(in crate::flow) struct PreparedPortEffect {
    pub(super) origin: Arc<()>,
    pub(super) effect: s::PlanEffect,
    pub(super) parent: Sequence,
    pub(super) turn: Option<TurnRef>,
    pub(super) bound: AbsoluteBound,
}

impl PreparedPortEffect {
    pub(in crate::flow) fn template_id(&self) -> &'static str {
        match self.effect {
            s::PlanEffect::Activate { .. } => "plan.activate",
            s::PlanEffect::Break { .. } => "plan.break",
            _ => unreachable!("private execution effect"),
        }
    }

    pub(in crate::flow) fn effect(&self) -> &s::PlanEffect {
        &self.effect
    }

    pub(in crate::flow) fn event(
        &self,
        context: &RecordingContext,
        expansion: &TemplateExpansion,
    ) -> Result<AuthoredEvent, &'static str> {
        if !Arc::ptr_eq(&self.origin, context.enter_origin())
            || expansion.template_id() != self.template_id()
        {
            return Err("plan effect lost its context or compiled template family");
        }
        Ok(AuthoredEvent {
            kind: weaver_trace::serving::Kind::PlanEvent,
            subsystem: weaver_trace::Subsystem::Harness,
            turn: self.turn.clone(),
            source: AuthoredSource::Loop,
            causal_parent: Some(self.parent),
            payload: Some(weaver_trace::serving::Payload::PlanEffect(Box::new(
                weaver_trace::serving::payloads::PlanEffectRecord {
                    message: serde_json::value::to_raw_value(expansion.message())
                        .expect("Message serializes"),
                    context: crate::flow::mirror::execution_effect(&self.effect)?,
                    template_id: expansion.template_id().into(),
                },
            ))),
        })
    }

    pub(in crate::flow) fn check_filing(
        &self,
        context: &RecordingContext,
        expansion: &TemplateExpansion,
        filing: &FilingCompletion,
    ) -> Result<bool, &'static str> {
        if !Arc::ptr_eq(&self.origin, context.enter_origin())
            || !filing.belongs_to(context)
            || filing.wait_key() != PumpDeadline::Decision
            || filing.wait_bound() != self.bound
            || self.bound.expired(filing.recorded_ns())
            || filing.sequence().0 <= self.parent.0
            || !filing.receipts().is_complete()
            || filing.receipts().sources().count() != 1
            || filing.receipts().dispositions().count() != 1
            || expansion.template_id() != self.template_id()
        {
            return Err("plan effect lost its original complete filing and bound");
        }
        let (file, rejected) = match filing.receipts().dispositions().next().unwrap() {
            SourceDisposition::Committed(value) => {
                let LedgerFile::Statement(file) = value.file() else {
                    return Err("plan effect has a pair receipt");
                };
                (file.as_ref(), false)
            }
            SourceDisposition::CorrectlyRejected(value) => (value.file(), true),
        };
        let metadata = file
            .context
            .as_ref()
            .ok_or("plan effect lost its typed context")?;
        let effect: s::PlanEffect = s::decode_strict(metadata.raw.get())
            .map_err(|_| "plan effect has malformed context")?;
        if file.block_id.run != expansion.run_id()
            || file.block_id.sequence.0 != filing.sequence().0
            || file.block_id.block.0 != 0
            || file.source.kind != "plan.event"
            || file.source.channel != "internal"
            || file.source.origin != "loop"
            || file.source.turn.as_deref() != self.turn.as_ref().map(|turn| turn.0.as_str())
            || file.source.causal_parent.map(|v| v.0) != Some(self.parent.0)
            || file.scheme_id != expansion.scheme().scheme_id
            || file.text != expansion.text()
            || metadata.source_path != [s::SourcePathPart::Key("context".into())]
            || effect != self.effect
            || if rejected {
                file.effect.is_some()
            } else {
                file.effect.as_ref()
                    != Some(&s::StatementEffect::Plan(Box::new(self.effect.clone())))
            }
        {
            return Err("plan effect filing differs from its certified source or private grant");
        }
        Ok(rejected)
    }
}
