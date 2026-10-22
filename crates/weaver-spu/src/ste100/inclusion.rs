//! Exact inclusion for checked, total deterministic finite automata.
//!
//! This implements the product-emptiness step elected by Spec 02 TEMPLATES.
//! Symbol IDs are internal finite-class identities, not a grammar artifact ABI.
//! A compiler must establish the machines' meaning and common symbol identities.
//! No result here is a language judgment, Ready or template certificate.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum InvalidDfa {
    NoStates,
    AlphabetOrder,
    StartState,
    TransitionCount,
    TransitionTarget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ResourceFailure {
    SizeOverflow,
    Allocation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Error {
    Invalid(InvalidDfa),
    AlphabetMismatch,
    Resource(ResourceFailure),
}

/// Construction consumes already allocated input arrays and checks the entire
/// machine. Row-major transitions include one target for every state/symbol.
/// The strictly increasing alphabet gives a canonical order for witnesses.
#[derive(Debug)]
pub(super) struct Dfa {
    alphabet: Vec<u32>,
    start: usize,
    accepting: Vec<bool>,
    transitions: Vec<usize>,
}

impl Dfa {
    pub(super) fn checked(
        alphabet: Vec<u32>,
        start: usize,
        accepting: Vec<bool>,
        transitions: Vec<usize>,
    ) -> Result<Self, Error> {
        if accepting.is_empty() {
            return Err(Error::Invalid(InvalidDfa::NoStates));
        }
        if alphabet.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err(Error::Invalid(InvalidDfa::AlphabetOrder));
        }
        if start >= accepting.len() {
            return Err(Error::Invalid(InvalidDfa::StartState));
        }
        let expected = checked_product(accepting.len(), alphabet.len())?;
        if transitions.len() != expected {
            return Err(Error::Invalid(InvalidDfa::TransitionCount));
        }
        if transitions.iter().any(|target| *target >= accepting.len()) {
            return Err(Error::Invalid(InvalidDfa::TransitionTarget));
        }
        Ok(Self {
            alphabet,
            start,
            accepting,
            transitions,
        })
    }

    fn next(&self, state: usize, symbol: usize) -> usize {
        // The complete matrix and its product size were checked on admission.
        self.transitions[state * self.alphabet.len() + symbol]
    }
}

/// NotIncluded carries the shortest witness, breaking equal-length ties by
/// ascending symbol ID. The empty vector is the concrete empty-word witness.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Inclusion {
    Included,
    NotIncluded { witness: Vec<u32> },
}

#[derive(Debug, Clone, Copy)]
enum Predecessor {
    Root,
    Edge { previous: usize, symbol: u32 },
}

pub(super) fn check(template: &Dfa, grammar: &Dfa) -> Result<Inclusion, Error> {
    check_with(template, grammar, &mut SystemAllocation)
}

fn check_with(
    template: &Dfa,
    grammar: &Dfa,
    allocation: &mut impl Allocation,
) -> Result<Inclusion, Error> {
    if template.alphabet != grammar.alphabet {
        return Err(Error::AlphabetMismatch);
    }
    let right_states = grammar.accepting.len();
    let pairs = checked_product(template.accepting.len(), right_states)?;
    let start = pair_index(template.start, grammar.start, right_states)?;
    if template.accepting[template.start] && !grammar.accepting[grammar.start] {
        return Ok(Inclusion::NotIncluded {
            witness: Vec::new(),
        });
    }

    let mut predecessors = Vec::new();
    allocation.reserve(&mut predecessors, pairs)?;
    predecessors.resize(pairs, None);
    predecessors[start] = Some(Predecessor::Root);
    let mut queue = Vec::new();
    allocation.reserve(&mut queue, pairs)?;
    queue.push(start);
    let mut cursor = 0;
    while cursor < queue.len() {
        let current = queue[cursor];
        cursor += 1;
        let left = current / right_states;
        let right = current % right_states;
        for (ordinal, symbol) in template.alphabet.iter().copied().enumerate() {
            let next_left = template.next(left, ordinal);
            let next_right = grammar.next(right, ordinal);
            let next = pair_index(next_left, next_right, right_states)?;
            if predecessors[next].is_some() {
                continue;
            }
            predecessors[next] = Some(Predecessor::Edge {
                previous: current,
                symbol,
            });
            if template.accepting[next_left] && !grammar.accepting[next_right] {
                return witness(&predecessors, next, allocation)
                    .map(|witness| Inclusion::NotIncluded { witness });
            }
            // Each product state enters once, so the reserved size is sufficient.
            queue.push(next);
        }
    }
    Ok(Inclusion::Included)
}

fn witness(
    predecessors: &[Option<Predecessor>],
    mut end: usize,
    allocation: &mut impl Allocation,
) -> Result<Vec<u32>, Error> {
    let mut length = 0usize;
    let mut cursor = end;
    while let Some(Predecessor::Edge { previous, .. }) = predecessors[cursor] {
        length = length
            .checked_add(1)
            .ok_or(Error::Resource(ResourceFailure::SizeOverflow))?;
        cursor = previous;
    }
    let mut word = Vec::new();
    allocation.reserve(&mut word, length)?;
    while let Some(Predecessor::Edge { previous, symbol }) = predecessors[end] {
        word.push(symbol);
        end = previous;
    }
    word.reverse();
    Ok(word)
}

fn checked_product(left: usize, right: usize) -> Result<usize, Error> {
    left.checked_mul(right)
        .ok_or(Error::Resource(ResourceFailure::SizeOverflow))
}

fn pair_index(left: usize, right: usize, right_states: usize) -> Result<usize, Error> {
    checked_product(left, right_states)?
        .checked_add(right)
        .ok_or(Error::Resource(ResourceFailure::SizeOverflow))
}

fn checked_allocation<T>(elements: usize) -> Result<(), Error> {
    let bytes = checked_product(elements, std::mem::size_of::<T>())?;
    if bytes > isize::MAX as usize {
        return Err(Error::Resource(ResourceFailure::SizeOverflow));
    }
    Ok(())
}

/// Only allocation is injectable in unit tests. It cannot supply transitions,
/// acceptance or proof results, and no budget/cap is part of the production API.
trait Allocation {
    fn reserve<T>(&mut self, vector: &mut Vec<T>, additional: usize) -> Result<(), Error>;
}

struct SystemAllocation;
impl Allocation for SystemAllocation {
    fn reserve<T>(&mut self, vector: &mut Vec<T>, additional: usize) -> Result<(), Error> {
        let total = vector
            .len()
            .checked_add(additional)
            .ok_or(Error::Resource(ResourceFailure::SizeOverflow))?;
        checked_allocation::<T>(total)?;
        vector
            .try_reserve_exact(additional)
            .map_err(|_| Error::Resource(ResourceFailure::Allocation))
    }
}

#[cfg(test)]
#[path = "inclusion_tests.rs"]
mod tests;
