//! Artificial finite-class languages test the algorithm, not STE100 rules.
use super::*;
use std::collections::{BTreeMap, BTreeSet};

fn finite(alphabet: &[u32], language: &[Vec<u32>]) -> Dfa {
    // This independent trie compiler describes exactly the enumerated language.
    let mut prefixes = BTreeSet::from([Vec::new()]);
    for word in language {
        for end in 1..=word.len() {
            prefixes.insert(word[..end].to_vec());
        }
    }
    let nodes: Vec<_> = prefixes.into_iter().collect();
    let indices: BTreeMap<_, _> = nodes
        .iter()
        .cloned()
        .enumerate()
        .map(|(index, word)| (word, index))
        .collect();
    let sink = nodes.len();
    let mut accepting: Vec<_> = nodes.iter().map(|word| language.contains(word)).collect();
    accepting.push(false);
    let mut transitions = Vec::new();
    for prefix in &nodes {
        for symbol in alphabet {
            let mut next = prefix.clone();
            next.push(*symbol);
            transitions.push(*indices.get(&next).unwrap_or(&sink));
        }
    }
    transitions.extend(std::iter::repeat_n(sink, alphabet.len()));
    Dfa::checked(
        alphabet.to_vec(),
        indices[&Vec::new()],
        accepting,
        transitions,
    )
    .unwrap()
}

fn all_words(alphabet: &[u32], maximum: usize) -> Vec<Vec<u32>> {
    let mut result = vec![vec![]];
    let mut previous = vec![vec![]];
    for _ in 0..maximum {
        let mut next = vec![];
        for prefix in previous {
            for symbol in alphabet {
                let mut word = prefix.clone();
                word.push(*symbol);
                next.push(word);
            }
        }
        result.extend(next.iter().cloned());
        previous = next;
    }
    result
}

#[test]
fn inclusion_all_small_finite_languages_match_independent_set_difference() {
    let words = all_words(&[3, 7], 2);
    let languages: Vec<Vec<Vec<u32>>> = (0..(1usize << words.len()))
        .map(|mask| {
            words
                .iter()
                .enumerate()
                .filter(|(index, _)| mask & (1 << index) != 0)
                .map(|(_, word)| word.clone())
                .collect()
        })
        .collect();
    let machines: Vec<_> = languages
        .iter()
        .map(|language| finite(&[3, 7], language))
        .collect();
    for (left, template) in languages.iter().enumerate() {
        for (right, grammar) in languages.iter().enumerate() {
            // all_words is ordered by length then symbol, independently of BFS.
            let expected = words
                .iter()
                .find(|word| template.contains(word) && !grammar.contains(word));
            let expected = expected.map_or(Inclusion::Included, |word| Inclusion::NotIncluded {
                witness: word.clone(),
            });
            assert_eq!(
                check(&machines[left], &machines[right]).unwrap(),
                expected,
                "finite languages {left} and {right}"
            );
        }
    }
}

#[test]
fn inclusion_strict_subset_and_nonfirst_slot_alternative_are_exact() {
    let template = finite(&[0, 1, 2, 3], &[vec![0, 1, 3], vec![0, 2, 3]]);
    let grammar = finite(&[0, 1, 2, 3], &[vec![0, 1, 3]]);
    assert_eq!(check(&grammar, &template).unwrap(), Inclusion::Included);
    assert_eq!(
        check(&template, &grammar).unwrap(),
        Inclusion::NotIncluded {
            witness: vec![0, 2, 3]
        }
    );
}

#[test]
fn inclusion_empty_word_and_empty_alphabet_use_the_actual_start_states() {
    let template = Dfa::checked(vec![], 1, vec![false, true], vec![]).unwrap();
    let grammar = Dfa::checked(vec![], 0, vec![false, true], vec![]).unwrap();
    assert_eq!(
        check(&template, &grammar).unwrap(),
        Inclusion::NotIncluded { witness: vec![] }
    );
    assert_eq!(check(&grammar, &template).unwrap(), Inclusion::Included);
    assert_eq!(check(&template, &template).unwrap(), Inclusion::Included);
}

#[test]
fn inclusion_nonzero_start_and_unreachable_acceptance_do_not_change_language() {
    let template = Dfa::checked(vec![9], 1, vec![false, false, true], vec![0, 2, 0]).unwrap();
    let grammar = finite(&[9], &[]);
    assert_eq!(
        check(&template, &grammar).unwrap(),
        Inclusion::NotIncluded { witness: vec![9] }
    );
    let unreachable = Dfa::checked(vec![9], 0, vec![false, true], vec![0, 1]).unwrap();
    assert_eq!(check(&unreachable, &grammar).unwrap(), Inclusion::Included);
}

fn periodic(period: usize) -> Dfa {
    let mut accepting = vec![false; period];
    accepting[0] = true;
    Dfa::checked(
        vec![0],
        0,
        accepting,
        (0..period).map(|state| (state + 1) % period).collect(),
    )
    .unwrap()
}

#[test]
fn inclusion_cyclic_languages_terminate_without_sampling_or_a_depth_cap() {
    let fifths = periodic(5);
    let thirds = periodic(3);
    assert_eq!(check(&fifths, &periodic(1)).unwrap(), Inclusion::Included);
    assert_eq!(check(&fifths, &fifths).unwrap(), Inclusion::Included);
    assert_eq!(
        check(&fifths, &thirds).unwrap(),
        Inclusion::NotIncluded {
            witness: vec![0; 5]
        }
    );
    // A long reachable path must not be mistaken for proof after short samples.
    assert_eq!(
        check(&periodic(137), &periodic(131)).unwrap(),
        Inclusion::NotIncluded {
            witness: vec![0; 137]
        }
    );
}

fn counters(words: usize, cluster: usize) -> Dfa {
    // Artificial classes: 0 increments word count and resets the cluster,
    // 1 increments both. The caller's numbers are test parameters, not STE100.
    let width = cluster + 1;
    let sink = (words + 1) * width;
    let mut transitions = vec![];
    for word_count in 0..=words {
        for noun_count in 0..=cluster {
            transitions.push(if word_count == words {
                sink
            } else {
                (word_count + 1) * width
            });
            transitions.push(if word_count == words || noun_count == cluster {
                sink
            } else {
                (word_count + 1) * width + noun_count + 1
            });
        }
    }
    transitions.extend([sink, sink]);
    let mut accepting = vec![true; sink];
    accepting.push(false);
    Dfa::checked(vec![0, 1], 0, accepting, transitions).unwrap()
}

#[test]
fn inclusion_counter_expansions_check_the_first_word_and_cluster_overflow() {
    assert_eq!(
        check(&counters(6, 2), &counters(5, 2)).unwrap(),
        Inclusion::NotIncluded {
            witness: vec![0; 6]
        }
    );
    assert_eq!(
        check(&counters(5, 3), &counters(5, 2)).unwrap(),
        Inclusion::NotIncluded {
            witness: vec![1; 3]
        }
    );
    assert_eq!(
        check(&counters(4, 1), &counters(5, 2)).unwrap(),
        Inclusion::Included
    );
}

#[test]
fn inclusion_witness_is_shortest_then_lexicographic_across_merged_paths() {
    let left = finite(
        &[1, 4, 9],
        &[vec![4, 1], vec![1, 9], vec![1, 4, 1], vec![9]],
    );
    let right = finite(&[1, 4, 9], &[vec![9]]);
    for _ in 0..4 {
        assert_eq!(
            check(&left, &right).unwrap(),
            Inclusion::NotIncluded {
                witness: vec![1, 9]
            }
        );
    }
    // Both symbols merge onto one state. The first discovered path must remain.
    let merged = Dfa::checked(
        vec![1, 4],
        0,
        vec![false, true, false],
        vec![1, 1, 2, 2, 2, 2],
    )
    .unwrap();
    assert_eq!(
        check(&merged, &finite(&[1, 4], &[])).unwrap(),
        Inclusion::NotIncluded { witness: vec![1] }
    );
}

#[test]
fn inclusion_requires_total_rows_and_in_range_start_targets() {
    for (alphabet, start, accepting, transitions, expected) in [
        (vec![0], 0, vec![], vec![], InvalidDfa::NoStates),
        (vec![0], 1, vec![false], vec![0], InvalidDfa::StartState),
        (
            vec![0, 1],
            0,
            vec![false],
            vec![0],
            InvalidDfa::TransitionCount,
        ),
        (
            vec![0],
            0,
            vec![false],
            vec![0, 0],
            InvalidDfa::TransitionCount,
        ),
        (
            vec![0],
            0,
            vec![false],
            vec![1],
            InvalidDfa::TransitionTarget,
        ),
        (
            vec![0, 0],
            0,
            vec![false],
            vec![0, 0],
            InvalidDfa::AlphabetOrder,
        ),
        (
            vec![2, 1],
            0,
            vec![false],
            vec![0, 0],
            InvalidDfa::AlphabetOrder,
        ),
    ] {
        assert_eq!(
            Dfa::checked(alphabet, start, accepting, transitions).unwrap_err(),
            Error::Invalid(expected)
        );
    }
}

#[test]
fn inclusion_exact_alphabet_identity_is_required_even_for_equal_widths() {
    let left = Dfa::checked(vec![1], 0, vec![true], vec![0]).unwrap();
    let right = Dfa::checked(vec![2], 0, vec![true], vec![0]).unwrap();
    assert_eq!(check(&left, &right), Err(Error::AlphabetMismatch));
    let empty = Dfa::checked(vec![], 0, vec![true], vec![]).unwrap();
    assert_eq!(check(&left, &empty), Err(Error::AlphabetMismatch));
}

#[test]
fn inclusion_rejecting_sink_makes_complement_cover_the_entire_alphabet() {
    let left = Dfa::checked(vec![1, 2], 0, vec![false, true], vec![1, 1, 1, 1]).unwrap();
    let right = Dfa::checked(
        vec![1, 2],
        0,
        vec![false, true, false],
        vec![1, 2, 1, 2, 2, 2],
    )
    .unwrap();
    assert_eq!(
        check(&left, &right).unwrap(),
        Inclusion::NotIncluded { witness: vec![2] }
    );
}

struct FailAllocation {
    remaining: usize,
}
impl Allocation for FailAllocation {
    fn reserve<T>(&mut self, vector: &mut Vec<T>, additional: usize) -> Result<(), Error> {
        if self.remaining == 0 {
            return Err(Error::Resource(ResourceFailure::Allocation));
        }
        self.remaining -= 1;
        SystemAllocation.reserve(vector, additional)
    }
}

#[test]
fn inclusion_allocation_failure_never_becomes_inclusion_or_a_bad_language() {
    let template = finite(&[0, 1], &[vec![0, 1]]);
    let grammar = finite(&[0, 1], &[]);
    // The predecessor table, queue and concrete witness each require allocation.
    for remaining in 0..3 {
        assert_eq!(
            check_with(&template, &grammar, &mut FailAllocation { remaining }),
            Err(Error::Resource(ResourceFailure::Allocation)),
            "allocation {remaining}"
        );
    }
    assert_eq!(
        check_with(&template, &grammar, &mut FailAllocation { remaining: 3 }).unwrap(),
        Inclusion::NotIncluded {
            witness: vec![0, 1]
        }
    );
    assert_eq!(
        check_with(&grammar, &grammar, &mut FailAllocation { remaining: 1 }),
        Err(Error::Resource(ResourceFailure::Allocation))
    );
}

#[test]
fn inclusion_size_arithmetic_fails_without_attempting_an_allocation() {
    let size_error = Err(Error::Resource(ResourceFailure::SizeOverflow));
    assert_eq!(checked_product(usize::MAX, 2), size_error);
    assert_eq!(pair_index(usize::MAX, 0, 2), size_error);
    assert_eq!(pair_index(usize::MAX, 1, 1), size_error);
    assert_eq!(
        checked_allocation::<usize>(usize::MAX),
        Err(Error::Resource(ResourceFailure::SizeOverflow))
    );
    assert_eq!(
        checked_allocation::<u8>(isize::MAX as usize + 1),
        Err(Error::Resource(ResourceFailure::SizeOverflow))
    );
    assert_eq!(checked_product(0, usize::MAX), Ok(0));
    assert_eq!(checked_allocation::<Predecessor>(0), Ok(()));
}
