//! Placement facts on the existing decode owner, under SPU-RENDER,
//! SPU-OPEN and SPU-MEASURE. The serial dispatcher carries these facts on the
//! existing decode wire without retaining a proposal or changing the session.

use weaver_spu::decoder::backend::{DecodeFault, TokenId};
use weaver_spu::decoder::session::Session;
use weaver_spu::family::{self, Family};
use weaver_spu::residency::{Resident, StopSet};
use weaver_spu::sampling::EffectiveSessionParameters;
use weaver_traits::Message;
use weaver_types::{TokenRefusal, TurnKey};

/// The same owner the decode loop establishes after a successful Open. The
/// resident borrowed by its session also supplies its renderer and tokenizer.
pub(super) struct OpenedSession<'r> {
    pub(super) session: Session<'r>,
    resident: &'r Resident,
    pub(super) renderer: &'static dyn Family,
    /// The declared template carried in the request record; rendering itself
    /// belongs to the selected family.
    pub(super) template: &'static str,
    generation_opener: &'static str,
    parameters: EffectiveSessionParameters,
    /// Counted from zero within a turn and reset when the turn changes. A
    /// placement query must not advance this seed-derivation ordinal.
    pub(super) turn_generations: Option<(TurnKey, u64)>,
    /// Promoted once at open against this artifact's vocabulary.
    pub(super) stop: StopSet,
}

#[derive(Debug, PartialEq)]
pub(super) enum PlacementFailure {
    Render(TokenRefusal),
    Decode(DecodeFault),
}

#[derive(Debug, PartialEq)]
pub(super) struct PreparedDelta {
    pub(super) rendered: String,
    pub(super) tokens: Vec<TokenId>,
}

// The dispatcher renders these private facts into the shared wire shapes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct OpenFacts {
    pub(super) prefix_tokens: u64,
    pub(super) capacity: u64,
    pub(super) generation_limit: u64,
    pub(super) terminal_tokens: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct DeltaMeasured {
    pub(super) resident: u64,
    pub(super) prefix_tokens: u64,
    pub(super) keep: u64,
    pub(super) input_tokens: u64,
    pub(super) capacity: u64,
}

impl<'r> OpenedSession<'r> {
    pub(super) fn from_open(
        session: Session<'r>,
        resident: &'r Resident,
        parameters: EffectiveSessionParameters,
        stop: StopSet,
    ) -> Self {
        Self {
            session,
            resident,
            renderer: (resident.declaration.renderer)(),
            template: resident.declaration.template,
            generation_opener: resident.declaration.generation_opener,
            parameters,
            turn_generations: None,
            stop,
        }
    }

    /// Render and tokenize the whole generation input once. The caller owns
    /// this temporary; measurement returns only its count and retains nothing.
    pub(super) fn prepare_delta(
        &self,
        delta: &[Message],
        tool_schemas: &[String],
    ) -> Result<PreparedDelta, PlacementFailure> {
        prepare_generation(
            self.renderer,
            self.generation_opener,
            delta,
            tool_schemas,
            |text| self.resident.tokenize(text),
        )
    }

    pub(super) fn measure_delta(
        &self,
        keep: u64,
        delta: &[Message],
        tool_schemas: &[String],
    ) -> Result<DeltaMeasured, PlacementFailure> {
        let position = self
            .session
            .placement_position(keep)
            .map_err(PlacementFailure::Decode)?;
        let prepared = self.prepare_delta(delta, tool_schemas)?;
        let input_tokens = count(prepared.tokens.len()).map_err(PlacementFailure::Decode)?;
        Ok(DeltaMeasured {
            resident: position.resident,
            prefix_tokens: position.prefix_tokens,
            keep: position.keep,
            input_tokens,
            capacity: position.capacity,
        })
    }

    pub(super) fn open_facts(&self) -> Result<OpenFacts, DecodeFault> {
        let position = self.session.placement_position(0)?;
        Ok(OpenFacts {
            prefix_tokens: position.prefix_tokens,
            capacity: position.capacity,
            generation_limit: count(self.parameters.max_tokens_per_turn)?,
            terminal_tokens: 1,
        })
    }
}

fn count(value: usize) -> Result<u64, DecodeFault> {
    u64::try_from(value).map_err(|_| DecodeFault::Engine {
        detail: "token count does not fit the placement contract".into(),
    })
}

/// This private sequencing seam also lets the host instrument observe the
/// single whole-string tokenizer call without fabricating a Resident.
pub(super) fn prepare_generation(
    renderer: &dyn Family,
    generation_opener: &str,
    delta: &[Message],
    tool_schemas: &[String],
    tokenize: impl FnOnce(&str) -> Result<Vec<TokenId>, DecodeFault>,
) -> Result<PreparedDelta, PlacementFailure> {
    let rendered = family::render_generation(renderer, delta, tool_schemas, generation_opener)
        .map_err(|refusal| PlacementFailure::Render(refusal.into()))?;
    let tokens = tokenize(&rendered).map_err(PlacementFailure::Decode)?;
    Ok(PreparedDelta { rendered, tokens })
}
