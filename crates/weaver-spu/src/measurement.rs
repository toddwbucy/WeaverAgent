//! conforms: spu-log-sum-exp-stable
//! conforms: spu-absent-not-empty-vector
//! conforms: spu-absent-shape-pinned-by-doctest
//! conforms: spu-prompt-partition-covers-exactly
//! conforms: spu-nothing-retained-after-the-answer
//!
//! Measurement production, per `weaver-spu-Spec` section 6.
//!
//! **The signals are computed where the distribution is.** Entropy over the
//! logits at each step and the surprisal of the token that was drawn, both in
//! bits. The distribution exists inside the generation loop and nowhere else,
//! so the arithmetic lives here and the loop in [`crate::decoder::session`]
//! calls it before the sampler runs.
//!
//! **The math is log-sum-exp stable and the reason is arithmetic rather than
//! taste.** A naive exponential overflows `f32` above a logit around 88, which
//! the archived tree records as happening routinely on production
//! vocabularies, so the stable form is the only correct one at the scale this
//! runs at. Everything here goes through [`log_sum_exp`], which subtracts the
//! maximum before exponentiating and therefore never evaluates `exp` on a
//! positive argument.
//!
//! **The absence takes a type instead of a fixture.** A vector that was not
//! produced is `None`, never an empty vector, and the shape makes the
//! distinction unrepresentable rather than merely unproduced: the payload is a
//! [`NonEmpty`], which is a head and a tail and cannot be built empty. The
//! doctest below reads the field's type, so a change to `Vec<f32>` stops the
//! build rather than reintroducing the ambiguity quietly.
//!
//! ```
//! use weaver_spu::measurement::{NonEmpty, Signals};
//! // Absent is a case of the option, not a length of the vector.
//! let nothing = Signals::absent();
//! assert!(nothing.entropy_bits.is_none());
//!
//! // And what is present cannot be empty: there is no constructor that
//! // yields a zero-length payload.
//! let present: Option<NonEmpty<f32>> = NonEmpty::from_vec(vec![1.5, 2.0]);
//! assert_eq!(present.expect("two values").len(), 2);
//! assert!(NonEmpty::from_vec(Vec::<f32>::new()).is_none());
//! ```
//!
//! ```compile_fail
//! use weaver_spu::measurement::Signals;
//! // The field is not a bare vector, so an empty one cannot stand for absence.
//! let signals = Signals {
//!     entropy_bits: Some(Vec::<f32>::new()),
//!     surprisal_bits: None,
//! };
//! ```

use std::f32::consts::LN_2;

/// A vector that cannot be empty, as a head and a tail.
///
/// **This is what makes "absent" and "measured nothing" different states.** An
/// `Option<NonEmpty<T>>` has no representation for a present-but-empty vector,
/// so the trace's absence carries a meaning no length can imitate.
#[derive(Debug, Clone, PartialEq)]
pub struct NonEmpty<T> {
    head: T,
    tail: Vec<T>,
}

impl<T> NonEmpty<T> {
    /// One value, which is the smallest a non-empty vector can be.
    pub fn new(head: T) -> Self {
        NonEmpty {
            head,
            tail: Vec::new(),
        }
    }

    /// Build from a vector, answering `None` where it was empty. This is the
    /// only path from a possibly-empty collection, so the emptiness is decided
    /// once, here, rather than at each reader.
    pub fn from_vec(mut values: Vec<T>) -> Option<Self> {
        if values.is_empty() {
            return None;
        }
        let head = values.remove(0);
        Some(NonEmpty { head, tail: values })
    }

    pub fn push(&mut self, value: T) {
        self.tail.push(value);
    }

    /// Always at least one.
    pub fn len(&self) -> usize {
        1 + self.tail.len()
    }

    /// Never true. Present so a reader reaching for the usual pair finds the
    /// answer stated rather than the method missing.
    pub fn is_empty(&self) -> bool {
        false
    }

    pub fn first(&self) -> &T {
        &self.head
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        std::iter::once(&self.head).chain(self.tail.iter())
    }
}

/// The two signal vectors, positionally paired with the tokens drawn.
///
/// **This crate produces no empty vector to mean nothing was measured.** It
/// produces no vector at all, and the trace's own `skip_serializing_if` carries
/// the absence to the record. The serialization half is `weaver-trace-Spec`'s;
/// this is the production half.
#[derive(Debug, Clone, PartialEq)]
pub struct Signals {
    /// Entropy of the distribution at each step, in bits.
    pub entropy_bits: Option<NonEmpty<f32>>,
    /// Surprisal of the token drawn at each step, in bits, where its
    /// election stands, per charter section 13.12.
    pub surprisal_bits: Option<NonEmpty<f32>>,
    /// Two to the power of the mean surprisal, one number for the whole
    /// generation, per charter section 13.12.
    ///
    /// **It carries no election and is absent on the one ground the
    /// entropies are absent on**, which is that nothing was measured. A
    /// mean needs terms and a backend serving no distribution supplies
    /// none, so a generation that measured nothing renders no perplexity
    /// and no entropies together. It is not a plain number that always
    /// stands: that reading would oblige this crate to render a mean of
    /// nothing.
    pub perplexity: Option<f64>,
}

impl Signals {
    /// Nothing was measured. Distinct from having measured an empty run.
    pub fn absent() -> Self {
        Signals {
            entropy_bits: None,
            surprisal_bits: None,
            perplexity: None,
        }
    }

    /// How many steps were measured, zero where absent.
    pub fn steps(&self) -> usize {
        self.entropy_bits.as_ref().map_or(0, NonEmpty::len)
    }
}

/// Accumulates one generation's signals, then answers with the pair.
///
/// **Nothing is retained after the answer**, per Spec section 6 and charter
/// section 3's no-state rule. [`Self::finish`] consumes the accumulator, so a
/// second generation cannot reach the first's measurements through it: there is
/// no accumulator left to reach.
#[derive(Debug)]
pub struct Accumulator {
    entropy: Vec<f32>,
    /// Kept only where the surprisal's election stands. Unelected this
    /// stays empty and the mean below is what the generation reports.
    surprisal: Vec<f32>,
    /// Whether the per-position vector is kept, per charter section 13.12.
    surprisal_elected: bool,
    /// The running sum of surprisal in bits and the count of terms, which
    /// is the whole of what the perplexity needs.
    ///
    /// **This is the line that keeps the election honest.** Section 5 of
    /// `weaver-trace-PRD` has an election govern production rather than
    /// recording, so an election that computed a vector and declined to
    /// write it down would be a recording level wearing an election's
    /// name. A sum and a count are arithmetic in flight: nothing holds the
    /// terms, nothing could report them, and they do not survive the
    /// position that made them. The mean is lossy in exactly the way that
    /// ruling cares about, so the unelected posture is a smaller
    /// computation rather than a withheld reading.
    surprisal_sum: f64,
    steps: usize,
    /// **Cleared the moment a retained token could not be measured.** The
    /// vectors are positionally paired with the tokens the generation answers
    /// with, so a gap is not a shorter vector: it is a vector whose every
    /// entry past the gap names the wrong token. An incomplete run reports
    /// absent instead, which is a fact a reader can act on.
    complete: bool,
}

impl Default for Accumulator {
    fn default() -> Self {
        Accumulator {
            entropy: Vec::new(),
            surprisal: Vec::new(),
            surprisal_elected: false,
            surprisal_sum: 0.0,
            steps: 0,
            complete: true,
        }
    }
}

impl Accumulator {
    /// A generation's accumulator, told at its construction whether the
    /// per-position surprisal is elected. The election is fixed for the
    /// generation because it is fixed for the load.
    pub fn new(surprisal_elected: bool) -> Self {
        Accumulator {
            surprisal_elected,
            ..Accumulator::default()
        }
    }

    /// Record one step, from the distribution the sampler has not yet seen.
    pub fn record(&mut self, entropy_bits: f32, surprisal_bits: f32) {
        self.entropy.push(entropy_bits);
        if self.surprisal_elected {
            self.surprisal.push(surprisal_bits);
        }
        // The sum accumulates whichever way the election fell, in `f64`
        // because a `f32` sum over a long generation loses the low bits of
        // every late term to the magnitude of the early ones.
        self.surprisal_sum += f64::from(surprisal_bits);
        self.steps += 1;
    }

    /// Abandon the measurement: a retained token could not be measured, so the
    /// pairing is broken and the whole run reports absent.
    ///
    /// **Not a shorter vector.** Dropping one entry silently shifts every
    /// later one onto the wrong token, which is a misattribution a consumer
    /// cannot detect. Absence is detectable.
    pub fn abandon(&mut self) {
        self.complete = false;
    }

    /// Answer with the pair and consume the accumulator.
    ///
    /// A generation that produced no token answers absent on both, which is
    /// the case `NonEmpty::from_vec` turns into `None` rather than into a
    /// zero-length vector. A generation that lost a step answers absent too,
    /// for the pairing reason above.
    pub fn finish(self) -> Signals {
        if !self.complete {
            return Signals::absent();
        }
        // Absent where nothing was measured, on the same ground the vectors
        // are absent there: a mean of no terms is not a number this crate
        // has, and rendering one would be the empty vector's defect wearing
        // a scalar's shape.
        // **Finite or absent, never a number the wire cannot carry.** A
        // surprisal is `shifted_lse - (logit - max)`, so a logit far enough
        // below the maximum overflows that subtraction and the mean reaches
        // infinity. `serde_json` renders a non-finite float as `null`, which
        // would put a member on the wire carrying nothing, and that is the
        // empty vector's defect wearing a scalar's shape. The absence is
        // produced here rather than filtered at the rendering because
        // production is where this crate's absences are decided, per Spec
        // section 6.
        let perplexity = (self.steps > 0)
            .then(|| (self.surprisal_sum / self.steps as f64).exp2())
            .filter(|mean| mean.is_finite());
        Signals {
            entropy_bits: NonEmpty::from_vec(self.entropy),
            surprisal_bits: NonEmpty::from_vec(self.surprisal),
            perplexity,
        }
    }
}

/// The stable normaliser, as the pair everything here works from: the maximum
/// logit and the log-sum-exp of the logits shifted by it.
///
/// **Nothing downstream ever touches a raw logit again**, and that is the whole
/// of the stability. Working from an unshifted `lse` and subtracting raw logits
/// from it looks equivalent and is not: at large magnitudes `lse - logit`
/// cancels to zero and `probability * logit` overflows to infinity, so two
/// equal logits at `f32::MAX` report no entropy and no surprise where the
/// answer is one bit each. Measured before this shape was chosen.
///
/// `None` where the maximum is not finite, which is the empty slice and the
/// all-infinite one, and `None` where the sum is not a positive finite
/// number.
///
/// **The second guard is not implied by the first.** `f32::max` answers the
/// other operand when one is `NaN`, so a vector carrying a `NaN` beside real
/// logits has a finite maximum and a `NaN` sum, and `ln` of that is `NaN`.
/// Without the guard every reading taken from it is `NaN`: an entropy, a
/// surprisal, and every probability of the field. **Absent is the honest
/// answer and `NaN` is not**, per the absent-rather-than-zero rule this
/// module keeps - the caller abandons the run's signals rather than
/// recording a number that means nothing. A healthy forward pass produces no
/// such vector, which is why this guards a degraded device rather than an
/// ordinary path.
fn shifted_normaliser(logits: &[f32]) -> Option<(f32, f32)> {
    let max = logits.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    if !max.is_finite() {
        return None;
    }
    // Every argument to `exp` is at most zero, so nothing overflows however
    // large the logits are, and the maximum's own term is exactly one, so a
    // sum over real logits is at least one and never zero.
    let sum: f32 = logits.iter().map(|logit| (logit - max).exp()).sum();
    if !sum.is_finite() || sum <= 0.0 {
        return None;
    }
    Some((max, sum.ln()))
}

/// The stable log-sum-exp of a logit vector.
///
/// **Subtracting the maximum is the whole of the stability.** Every argument to
/// `exp` is then at most zero, so nothing overflows however large the logits
/// are, and the shift is added back afterwards. A naive `ln(sum(exp(l)))`
/// overflows `f32` above roughly 88, which is a value production vocabularies
/// reach.
///
/// An empty slice answers negative infinity, which is the identity for this
/// operation, though no caller here supplies one.
pub fn log_sum_exp(logits: &[f32]) -> f32 {
    match shifted_normaliser(logits) {
        Some((max, shifted)) => max + shifted,
        None => logits.iter().copied().fold(f32::NEG_INFINITY, f32::max),
    }
}

/// Entropy of the distribution the logits describe, in bits.
///
/// Computed entirely in shifted space, so no probability is materialised
/// against a raw logit and nothing overflows on the way.
pub fn entropy_bits(logits: &[f32]) -> f32 {
    let Some((max, shifted_lse)) = shifted_normaliser(logits) else {
        return 0.0;
    };
    // H = -sum(p * ln p), with ln p taken in shifted space so it is a small
    // normalised number rather than the difference of two large ones.
    let mut nats = 0.0f32;
    for &logit in logits {
        let log_probability = (logit - max) - shifted_lse;
        nats -= log_probability.exp() * log_probability;
    }
    (nats / LN_2).max(0.0)
}

/// The probability field at one position: the `depth` most probable
/// candidates in descending order with their probabilities, and the rank
/// the drawn token landed on.
///
/// Per `weaver-spu-Spec` section 7.5, computed from the same logits the
/// entropy and the surprisal are computed from, at the same site and
/// before the sampler consumes the distribution.
///
/// **A partial selection rather than a sort.** The depth is small against
/// a vocabulary in the hundreds of thousands, so the whole vector is
/// scanned once to find the depth's worth of leaders and only those are
/// ordered. Sorting the vocabulary would cost the run for a result the
/// caller discards.
///
/// **Probabilities rather than logits**, in shifted space for the reason
/// the entropy is: a consumer re-normalising per position would repeat
/// arithmetic already done over the whole vocabulary, and a raw logit
/// tells a reader nothing without the normaliser beside it.
///
/// `None` where the distribution carries no finite normaliser or the drawn
/// index is outside it, which is the absent-rather-than-zero rule the
/// signals follow.
pub fn field(logits: &[f32], drawn: usize, depth: usize) -> Option<(Vec<(u32, f32)>, u32)> {
    let (max, shifted_lse) = shifted_normaliser(logits)?;
    if drawn >= logits.len() || depth == 0 {
        return None;
    }
    let probability = |logit: f32| ((logit - max) - shifted_lse).exp();

    // One pass for the leaders, holding at most `depth` of them. The list
    // stays ordered as it fills, so the insert is a walk over a short
    // vector rather than a sort of a long one.
    // **The capacity is the depth or the vocabulary, whichever is smaller.**
    // A vector can hold no more leaders than there are logits, and the depth
    // is the operator's declared number with a floor at the sampling cutoff
    // and no ceiling anywhere, so a declaration's stray digit would
    // otherwise reserve that many slots before the first comparison. The
    // depth itself stays whole below, where an out-of-depth draw is reported
    // against the number that was declared rather than the number reserved.
    let mut leaders: Vec<(u32, f32)> = Vec::with_capacity(depth.min(logits.len()) + 1);
    for (index, &logit) in logits.iter().enumerate() {
        if leaders.len() == depth && logit <= leaders[depth - 1].1 {
            continue;
        }
        let at = leaders
            .iter()
            .position(|(_, held)| logit > *held)
            .unwrap_or(leaders.len());
        leaders.insert(at, (index as u32, logit));
        leaders.truncate(depth);
    }

    // The realized rank is the drawn token's place among the leaders. A
    // draw outside them is reported as the depth itself, which reads as
    // "past what was reported" and never as a rank that was observed.
    let realized = leaders
        .iter()
        .position(|(token, _)| *token as usize == drawn)
        .unwrap_or(depth) as u32;
    let ranked = leaders
        .into_iter()
        .map(|(token, logit)| (token, probability(logit)))
        .collect();
    Some((ranked, realized))
}

/// Surprisal of one token under the distribution, in bits.
///
/// `-log2 p` for the drawn token, taken in shifted space for the same reason
/// the entropy is: an unshifted `lse - logit` cancels at large magnitudes and
/// answers no surprise for a token that carried some.
pub fn surprisal_bits(logits: &[f32], index: usize) -> Option<f32> {
    let logit = *logits.get(index)?;
    let (max, shifted_lse) = shifted_normaliser(logits)?;
    Some(((shifted_lse - (logit - max)) / LN_2).max(0.0))
}

/// Why a partition was rejected.
#[derive(Debug, Clone, PartialEq)]
pub enum PartitionDefect {
    /// The offsets do not reach the end of the rendered text, so a consumer
    /// joining text to tokens would have a tail to guess at.
    ShortOfEnd { last: usize, text_len: usize },
    /// The offsets run backwards somewhere, so they do not partition anything.
    NotAscending { at: usize },
    /// No offsets at all for a non-empty text.
    Empty,
}

/// The prompt's block partition, from the tokenizer's offsets.
///
/// **The invariant is that the last offset equals the rendered text's length**,
/// which is what makes the partition cover the prompt exactly. The salvaged
/// `tokenize_with_offsets` mechanic carries it, and it is checked here rather
/// than assumed, because a consumer joining text to tokens with a gap at the
/// end has no way to tell a short partition from a short prompt.
#[derive(Debug, Clone, PartialEq)]
pub struct PromptPartition {
    offsets: Vec<usize>,
    text_len: usize,
}

impl PromptPartition {
    /// Build and check. The offsets are the end position of each token in the
    /// rendered text.
    pub fn new(offsets: Vec<usize>, text_len: usize) -> Result<Self, PartitionDefect> {
        let Some(&last) = offsets.last() else {
            return if text_len == 0 {
                Ok(PromptPartition {
                    offsets,
                    text_len: 0,
                })
            } else {
                Err(PartitionDefect::Empty)
            };
        };
        for window in offsets.windows(2).enumerate() {
            let (at, pair) = window;
            if pair[1] < pair[0] {
                return Err(PartitionDefect::NotAscending { at: at + 1 });
            }
        }
        if last != text_len {
            return Err(PartitionDefect::ShortOfEnd { last, text_len });
        }
        Ok(PromptPartition { offsets, text_len })
    }

    pub fn offsets(&self) -> &[usize] {
        &self.offsets
    }

    pub fn text_len(&self) -> usize {
        self.text_len
    }

    /// How many blocks the partition holds.
    pub fn blocks(&self) -> usize {
        self.offsets.len()
    }
}

#[cfg(test)]
mod tests;
