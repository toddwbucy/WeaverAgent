//! Actual paired Observe client, dormant until the coupled serving cutover.
use crate::channel::{ChannelFault, Coordination};
use weaver_types::state_flow::enter::{ObservationAnswer, ObservationContent};
use weaver_types::{ExchangeId, LifecycleDirective, LifecycleRefusal, Opener};

#[derive(Debug)]
pub(crate) enum Failure {
    Channel(ChannelFault),
    Refused(LifecycleRefusal),
}

pub(crate) fn ask(channel: &mut Coordination) -> Result<ObservationAnswer, Failure> {
    let ordinal = channel.next_ordinal();
    channel
        .send_directive(ordinal, LifecycleDirective::Observe)
        .map_err(Failure::Channel)?;
    let envelope = channel
        .recv_observation_within(crate::OBSERVE_BOUND)
        .map_err(Failure::Channel)?;
    if envelope.exchange
        != (ExchangeId {
            opener: Opener::Admin,
            ordinal,
        })
    {
        return Err(Failure::Channel(ChannelFault::Undecodable));
    }
    match envelope.payload {
        ObservationContent::Answer(answer) => Ok(answer),
        ObservationContent::Refusal(refusal) => Err(Failure::Refused(refusal)),
    }
}

#[cfg(test)]
#[path = "serving_observe_tests.rs"]
mod tests;
