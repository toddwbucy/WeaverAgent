//! The existing Stop exchange shared by legacy and replacement serving.
use crate::channel::{ChannelFault, Coordination};
use weaver_types::{
    Cause, ExchangeId, LifecycleAnswer, LifecycleDirective, LifecycleRefusal, Opener, Payload,
    Position,
};

pub(crate) fn ask(
    channel: &mut Coordination,
    cause: Cause,
) -> Result<LifecycleAnswer, LifecycleRefusal> {
    let ordinal = channel.next_ordinal();
    channel
        .send_directive(ordinal, LifecycleDirective::Stop { cause })
        .map_err(|_| LifecycleRefusal::NoResidency)?;
    let answer = channel
        .recv_within(crate::STOP_BOUND)
        .map_err(|fault| match fault {
            ChannelFault::Unanswered => LifecycleRefusal::Unanswered,
            _ => LifecycleRefusal::NoResidency,
        })?;
    if answer.exchange
        != (ExchangeId {
            opener: Opener::Admin,
            ordinal,
        })
        || answer.position != Position::Close
    {
        return Err(LifecycleRefusal::Malformed);
    }
    match answer.payload {
        Payload::Answer(
            answer @ (LifecycleAnswer::AtRest | LifecycleAnswer::TurnAborted { .. }),
        ) => Ok(crate::verbs::relay_stop_answer(answer)),
        Payload::Refusal(refusal) => Err(refusal),
        _ => Err(LifecycleRefusal::Malformed),
    }
}
#[cfg(test)]
#[path = "stop_exchange_tests.rs"]
mod tests;
