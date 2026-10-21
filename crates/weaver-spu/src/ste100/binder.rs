//! Deterministic binding over grammar-approved assignments and source-word scores.
//!
//! Spec 02 MODEL elects this score and tie rule. This module does not recognize STE100,
//! tokenize source, run a model, or certify a grammar. Those producers must supply the
//! complete candidates and aligned scores. A malformed producer result is component
//! failure, never a language rejection. Production grammar and model adapters remain
//! O02-1 and O02-2 dependencies.

use std::collections::BTreeMap;

use weaver_types::state_flow::{Binding, DecimalU64, NounBinding, Span};

/// The declaration order is also MODEL's exact lexicographic tie order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(usize)]
pub enum Label {
    Outside,
    BeginVerb,
    InsideVerb,
    BeginNoun,
    InsideNoun,
    BeginCondition,
    InsideCondition,
}

impl Label {
    /// Index in the model's fixed seven-column score matrix.
    pub const fn index(self) -> usize {
        self as usize
    }

    fn begins(self) -> bool {
        matches!(
            self,
            Self::BeginVerb | Self::BeginNoun | Self::BeginCondition
        )
    }

    fn continues(self, previous: Self) -> bool {
        matches!(
            (self, previous),
            (Self::InsideVerb, Self::BeginVerb | Self::InsideVerb)
                | (Self::InsideNoun, Self::BeginNoun | Self::InsideNoun)
                | (
                    Self::InsideCondition,
                    Self::BeginCondition | Self::InsideCondition
                )
        )
    }
}

/// Only the grammar can recognize a step reference. The binder just emits its ID.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConditionValue {
    ExactSource,
    Step(DecimalU64),
}

/// One grammar-approved full label sequence for one governing action.
///
/// There is one label per supplied source word. Words outside the statement must be
/// Outside. The verb is the closed dictionary key, not a normalized guess by the binder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidateAssignment {
    pub verb: String,
    pub labels: Vec<Label>,
    pub condition: Option<ConditionValue>,
}

/// Distinct actions remain distinct even when they share a statement span.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GrammarAction {
    pub statement_span: Span,
    pub candidates: Vec<CandidateAssignment>,
}

/// Every failure here means the checker could not complete its binding computation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BinderFailure {
    EmptyText,
    MissingWords,
    MissingActions,
    WordAlignment {
        word: usize,
    },
    ScoreCount {
        words: usize,
        scores: usize,
    },
    NonFiniteScore {
        word: usize,
        label: usize,
    },
    InvalidStatement {
        action: usize,
    },
    ActionOrder {
        action: usize,
    },
    MissingCandidates {
        action: usize,
    },
    InvalidCandidate {
        action: usize,
        candidate: usize,
        reason: CandidateFailure,
    },
}

/// Structural faults in a purportedly grammar-approved assignment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CandidateFailure {
    LabelCount,
    InvalidBio,
    OutsideStatement,
    VerbCount,
    MissingNoun,
    ConditionCount,
    ConditionMismatch,
    InvalidStep,
    UnknownVerb,
    AmbiguousAssignment,
}

impl std::fmt::Display for BinderFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "STE100 binding component failed: {self:?}")
    }
}

impl std::error::Error for BinderFailure {}

/// Select each action's highest-scoring valid assignment and preserve source order.
///
/// The complete input is validated, including scores and candidates that do not win.
/// There is no confidence cutoff, epsilon comparison, partial successful result, or
/// conversion from a legacy sequence score. Empty parses must be handled by the grammar
/// with their pinned rule evidence before calling this function.
pub fn bind(
    text: &str,
    words: &[Span],
    scores: &[[f32; 7]],
    actions: &[GrammarAction],
    verbs: &[String],
) -> Result<Vec<Binding>, BinderFailure> {
    validate_words(text, words, scores)?;
    if actions.is_empty() {
        return Err(BinderFailure::MissingActions);
    }
    let mut result = Vec::with_capacity(actions.len());
    let mut previous_start = None;
    for (action_index, action) in actions.iter().enumerate() {
        action
            .statement_span
            .slice(text)
            .map_err(|_| BinderFailure::InvalidStatement {
                action: action_index,
            })?;
        if previous_start.is_some_and(|previous| action.statement_span.start < previous) {
            return Err(BinderFailure::ActionOrder {
                action: action_index,
            });
        }
        previous_start = Some(action.statement_span.start);
        if action.candidates.is_empty() {
            return Err(BinderFailure::MissingCandidates {
                action: action_index,
            });
        }
        let mut seen = BTreeMap::new();
        let mut best: Option<(f64, &[Label], Binding)> = None;
        for (candidate_index, candidate) in action.candidates.iter().enumerate() {
            let failure = |reason| BinderFailure::InvalidCandidate {
                action: action_index,
                candidate: candidate_index,
                reason,
            };
            let binding = materialize(text, words, action, candidate, verbs).map_err(failure)?;
            // Equal label sequences must denote the same result. Otherwise neither the
            // elected tie rule nor candidate iteration order can choose a unique binding.
            if let Some(previous) = seen.insert(candidate.labels.as_slice(), candidate)
                && (previous.verb != candidate.verb || previous.condition != candidate.condition)
            {
                return Err(failure(CandidateFailure::AmbiguousAssignment));
            }
            let score = candidate
                .labels
                .iter()
                .zip(scores)
                .fold(0.0_f64, |sum, (label, row)| {
                    sum + f64::from(row[label.index()])
                });
            if best.as_ref().is_none_or(|(best_score, best_labels, _)| {
                score > *best_score
                    || (score == *best_score && candidate.labels.as_slice() < *best_labels)
            }) {
                best = Some((score, &candidate.labels, binding));
            }
        }
        // The nonempty candidate check and full validation above ensure a winner.
        result.push(best.expect("a valid nonempty candidate set has a winner").2);
    }
    Ok(result)
}

fn validate_words(text: &str, words: &[Span], scores: &[[f32; 7]]) -> Result<(), BinderFailure> {
    if text.is_empty() {
        return Err(BinderFailure::EmptyText);
    }
    if words.is_empty() {
        return Err(BinderFailure::MissingWords);
    }
    let mut previous_end = None;
    for (word, span) in words.iter().enumerate() {
        if span.slice(text).is_err() || previous_end.is_some_and(|previous| span.start < previous) {
            return Err(BinderFailure::WordAlignment { word });
        }
        previous_end = Some(span.end);
    }
    if words.len() != scores.len() {
        return Err(BinderFailure::ScoreCount {
            words: words.len(),
            scores: scores.len(),
        });
    }
    for (word, row) in scores.iter().enumerate() {
        for (label, score) in row.iter().enumerate() {
            if !score.is_finite() {
                return Err(BinderFailure::NonFiniteScore { word, label });
            }
        }
    }
    Ok(())
}

fn materialize(
    text: &str,
    words: &[Span],
    action: &GrammarAction,
    candidate: &CandidateAssignment,
    verbs: &[String],
) -> Result<Binding, CandidateFailure> {
    if candidate.labels.len() != words.len() {
        return Err(CandidateFailure::LabelCount);
    }
    if candidate.verb.is_empty() || !verbs.contains(&candidate.verb) {
        return Err(CandidateFailure::UnknownVerb);
    }
    let mut segments: Vec<(Label, Span)> = Vec::new();
    let mut previous = Label::Outside;
    for (&label, &word) in candidate.labels.iter().zip(words) {
        if label != Label::Outside && !action.statement_span.contains(&word) {
            return Err(CandidateFailure::OutsideStatement);
        }
        if label.begins() {
            segments.push((label, word));
        } else if label != Label::Outside {
            if !label.continues(previous) {
                return Err(CandidateFailure::InvalidBio);
            }
            // The continuation test guarantees a preceding segment of this role.
            segments
                .last_mut()
                .expect("BIO continuation has a segment")
                .1
                .end = word.end;
        }
        previous = label;
    }
    let mut verb_span = None;
    let mut condition_span = None;
    let mut nouns = Vec::new();
    for (label, span) in segments {
        match label {
            Label::BeginVerb => {
                if verb_span.replace(span).is_some() {
                    return Err(CandidateFailure::VerbCount);
                }
            }
            Label::BeginNoun => nouns.push(NounBinding {
                // Every boundary came from an already validated source word.
                value: span
                    .slice(text)
                    .expect("aligned span boundaries")
                    .to_owned(),
                span,
            }),
            Label::BeginCondition => {
                if condition_span.replace(span).is_some() {
                    return Err(CandidateFailure::ConditionCount);
                }
            }
            _ => unreachable!("only begin labels start segments"),
        }
    }
    let verb_span = verb_span.ok_or(CandidateFailure::VerbCount)?;
    if nouns.is_empty() {
        return Err(CandidateFailure::MissingNoun);
    }
    let condition = match (&candidate.condition, &condition_span) {
        (None, None) => None,
        (Some(ConditionValue::ExactSource), Some(span)) => Some(
            span.slice(text)
                .expect("aligned span boundaries")
                .to_owned(),
        ),
        (Some(ConditionValue::Step(id)), Some(_)) if id.0 > 0 => Some(format!("step:{}", id.0)),
        (Some(ConditionValue::Step(_)), Some(_)) => return Err(CandidateFailure::InvalidStep),
        _ => return Err(CandidateFailure::ConditionMismatch),
    };
    Ok(Binding {
        statement_span: action.statement_span,
        verb: candidate.verb.clone(),
        verb_span,
        condition,
        condition_span,
        nouns,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(start: u64, end: u64) -> Span {
        Span {
            start: DecimalU64(start),
            end: DecimalU64(end),
        }
    }

    fn candidate(labels: &[Label]) -> CandidateAssignment {
        CandidateAssignment {
            verb: "read".into(),
            labels: labels.to_vec(),
            condition: None,
        }
    }

    fn action(end: u64, candidates: Vec<CandidateAssignment>) -> GrammarAction {
        GrammarAction {
            statement_span: span(0, end),
            candidates,
        }
    }

    fn run(
        text: &str,
        words: &[Span],
        scores: &[[f32; 7]],
        actions: &[GrammarAction],
    ) -> Result<Vec<Binding>, BinderFailure> {
        bind(
            text,
            words,
            scores,
            actions,
            &["read".into(), "write".into()],
        )
    }

    fn choice() -> (Vec<Span>, Vec<CandidateAssignment>) {
        (
            vec![span(0, 4), span(5, 10), span(11, 15)],
            vec![
                candidate(&[Label::BeginVerb, Label::BeginNoun, Label::Outside]),
                candidate(&[Label::BeginVerb, Label::Outside, Label::BeginNoun]),
            ],
        )
    }

    #[test]
    fn highest_total_score_uses_all_source_words_and_no_confidence_cutoff() {
        let (words, candidates) = choice();
        let mut scores = [[-100.0; 7]; 3];
        scores[1][Label::Outside.index()] = -99.0;
        let result = run(
            "Read alpha beta.",
            &words,
            &scores,
            &[action(16, candidates)],
        )
        .unwrap();
        assert_eq!(result[0].nouns[0].value, "beta");
    }

    #[test]
    fn exact_ties_choose_lexicographically_smallest_full_sequence_not_first_candidate() {
        let (words, mut candidates) = choice();
        for _ in 0..2 {
            let result = run(
                "Read alpha beta.",
                &words,
                &[[0.0; 7]; 3],
                &[action(16, candidates.clone())],
            )
            .unwrap();
            assert_eq!(result[0].nouns[0].value, "beta");
            candidates.reverse();
        }
        assert_eq!(
            [
                Label::Outside,
                Label::BeginVerb,
                Label::InsideVerb,
                Label::BeginNoun,
                Label::InsideNoun,
                Label::BeginCondition,
                Label::InsideCondition
            ]
            .map(Label::index),
            [0, 1, 2, 3, 4, 5, 6]
        );
    }

    #[test]
    fn f32_scores_are_converted_before_source_order_f64_summation() {
        let (words, candidates) = choice();
        let mut scores = [[0.0; 7]; 3];
        scores[0][Label::BeginVerb.index()] = 16_777_216.0;
        scores[1][Label::BeginNoun.index()] = 1.0;
        scores[2][Label::Outside.index()] = -16_777_216.0;
        scores[2][Label::BeginNoun.index()] = -16_777_216.0;
        let result = run(
            "Read alpha beta.",
            &words,
            &scores,
            &[action(16, candidates)],
        )
        .unwrap();
        assert_eq!(result[0].nouns[0].value, "alpha");
    }

    #[test]
    fn a_tiny_real_score_difference_is_not_an_epsilon_tie() {
        let (words, candidates) = choice();
        let mut scores = [[0.0; 7]; 3];
        scores[1][Label::BeginNoun.index()] = f32::from_bits(1);
        let result = run(
            "Read alpha beta.",
            &words,
            &scores,
            &[action(16, candidates)],
        )
        .unwrap();
        assert_eq!(result[0].nouns[0].value, "alpha");
    }

    #[test]
    fn source_order_is_not_reassociated_even_after_conversion_to_f64() {
        let text = "Read alpha beta gamma.";
        let words = [span(0, 4), span(5, 10), span(11, 15), span(16, 21)];
        let candidates = vec![
            candidate(&[
                Label::BeginVerb,
                Label::BeginNoun,
                Label::Outside,
                Label::Outside,
            ]),
            candidate(&[
                Label::BeginVerb,
                Label::Outside,
                Label::BeginNoun,
                Label::Outside,
            ]),
        ];
        let mut scores = [[0.0; 7]; 4];
        scores[0][Label::BeginVerb.index()] = 1.0e30;
        scores[1][Label::BeginNoun.index()] = -1.0e30;
        scores[2][Label::Outside.index()] = 1.0;
        scores[2][Label::BeginNoun.index()] = -1.0e30;
        let result = run(text, &words, &scores, &[action(22, candidates)]).unwrap();
        assert_eq!(result[0].nouns[0].value, "alpha");
    }

    #[test]
    fn compound_objects_and_multibyte_source_keep_exact_spans_and_whitespace() {
        let text = "Read café  cache and INDEX when ready.";
        let words = [
            span(0, 4),
            span(5, 10),
            span(12, 17),
            span(18, 21),
            span(22, 27),
            span(28, 32),
            span(33, 38),
        ];
        let mut selected = candidate(&[
            Label::BeginVerb,
            Label::BeginNoun,
            Label::InsideNoun,
            Label::Outside,
            Label::BeginNoun,
            Label::BeginCondition,
            Label::InsideCondition,
        ]);
        selected.condition = Some(ConditionValue::ExactSource);
        let result = run(
            text,
            &words,
            &[[0.0; 7]; 7],
            &[action(text.len() as u64, vec![selected])],
        )
        .unwrap();
        assert_eq!(
            result[0].nouns,
            vec![
                NounBinding {
                    value: "café  cache".into(),
                    span: span(5, 17)
                },
                NounBinding {
                    value: "INDEX".into(),
                    span: span(22, 27)
                }
            ]
        );
        assert_eq!(result[0].condition.as_deref(), Some("when ready"));
        assert_eq!(result[0].condition_span, Some(span(28, 38)));
        result[0].validate(text, &["read".into()]).unwrap();
    }

    #[test]
    fn grammar_step_condition_uses_positive_decimal_id_only() {
        let text = "Read data at step 14.";
        let words = [
            span(0, 4),
            span(5, 9),
            span(10, 12),
            span(13, 17),
            span(18, 20),
        ];
        let mut selected = candidate(&[
            Label::BeginVerb,
            Label::BeginNoun,
            Label::BeginCondition,
            Label::InsideCondition,
            Label::InsideCondition,
        ]);
        selected.condition = Some(ConditionValue::Step(DecimalU64(14)));
        let result = run(
            text,
            &words,
            &[[0.0; 7]; 5],
            &[action(21, vec![selected.clone()])],
        )
        .unwrap();
        assert_eq!(result[0].condition.as_deref(), Some("step:14"));
        assert_eq!(result[0].condition_span, Some(span(10, 20)));
        selected.condition = Some(ConditionValue::Step(DecimalU64(0)));
        assert_eq!(
            run(text, &words, &[[0.0; 7]; 5], &[action(21, vec![selected])]),
            Err(BinderFailure::InvalidCandidate {
                action: 0,
                candidate: 0,
                reason: CandidateFailure::InvalidStep,
            })
        );
    }

    #[test]
    fn a_multiword_verb_retains_its_complete_source_span() {
        let text = "Read back data.";
        let words = [span(0, 4), span(5, 9), span(10, 14)];
        let result = run(
            text,
            &words,
            &[[0.0; 7]; 3],
            &[action(
                15,
                vec![candidate(&[
                    Label::BeginVerb,
                    Label::InsideVerb,
                    Label::BeginNoun,
                ])],
            )],
        )
        .unwrap();
        assert_eq!(result[0].verb_span, span(0, 9));
        assert_eq!(result[0].verb, "read");
        assert_eq!(result[0].nouns[0].value, "data");
    }

    #[test]
    fn separate_governing_actions_in_one_statement_remain_separate_bindings() {
        let text = "Read alpha and write beta.";
        let words = [
            span(0, 4),
            span(5, 10),
            span(11, 14),
            span(15, 20),
            span(21, 25),
        ];
        let first = action(
            26,
            vec![candidate(&[
                Label::BeginVerb,
                Label::BeginNoun,
                Label::Outside,
                Label::Outside,
                Label::Outside,
            ])],
        );
        let mut second = action(
            26,
            vec![candidate(&[
                Label::Outside,
                Label::Outside,
                Label::Outside,
                Label::BeginVerb,
                Label::BeginNoun,
            ])],
        );
        second.candidates[0].verb = "write".into();
        let result = run(text, &words, &[[0.0; 7]; 5], &[first, second]).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].statement_span, result[1].statement_span);
        assert_eq!(result[0].verb_span, span(0, 4));
        assert_eq!(result[1].verb_span, span(15, 20));
    }

    #[test]
    fn more_than_one_governing_condition_fails_the_component() {
        let text = "Read data when ready.";
        let words = [span(0, 4), span(5, 9), span(10, 14), span(15, 20)];
        let mut selected = candidate(&[
            Label::BeginVerb,
            Label::BeginNoun,
            Label::BeginCondition,
            Label::BeginCondition,
        ]);
        selected.condition = Some(ConditionValue::ExactSource);
        assert_eq!(
            run(text, &words, &[[0.0; 7]; 4], &[action(21, vec![selected])]),
            Err(BinderFailure::InvalidCandidate {
                action: 0,
                candidate: 0,
                reason: CandidateFailure::ConditionCount
            })
        );
    }

    #[test]
    fn distinct_actions_are_not_collapsed_or_reordered() {
        let text = "Read alpha. Write beta.";
        let words = [span(0, 4), span(5, 10), span(12, 17), span(18, 22)];
        let first = action(
            11,
            vec![candidate(&[
                Label::BeginVerb,
                Label::BeginNoun,
                Label::Outside,
                Label::Outside,
            ])],
        );
        let mut second = GrammarAction {
            statement_span: span(12, 23),
            candidates: vec![candidate(&[
                Label::Outside,
                Label::Outside,
                Label::BeginVerb,
                Label::BeginNoun,
            ])],
        };
        second.candidates[0].verb = "write".into();
        let result = run(
            text,
            &words,
            &[[0.0; 7]; 4],
            &[first.clone(), second.clone()],
        )
        .unwrap();
        assert_eq!(
            result.iter().map(|b| b.verb.as_str()).collect::<Vec<_>>(),
            ["read", "write"]
        );
        assert_eq!(
            run(text, &words, &[[0.0; 7]; 4], &[second, first]),
            Err(BinderFailure::ActionOrder { action: 1 })
        );
    }

    #[test]
    fn malformed_alignment_and_missing_or_nonfinite_scores_fail_the_component() {
        let (words, candidates) = choice();
        let actions = [action(16, candidates)];
        for bad in [
            vec![span(0, 4), span(3, 10), span(11, 15)],
            vec![span(0, 4), span(5, 5), span(11, 15)],
            vec![span(0, 4), span(5, 10), span(11, 99)],
        ] {
            assert!(matches!(
                run("Read alpha beta.", &bad, &[[0.0; 7]; 3], &actions),
                Err(BinderFailure::WordAlignment { .. })
            ));
        }
        assert!(matches!(
            run("Read alpha beta.", &words, &[[0.0; 7]; 2], &actions),
            Err(BinderFailure::ScoreCount { .. })
        ));
        for score in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let mut scores = [[0.0; 7]; 3];
            scores[2][Label::InsideCondition.index()] = score;
            assert_eq!(
                run("Read alpha beta.", &words, &scores, &actions),
                Err(BinderFailure::NonFiniteScore { word: 2, label: 6 })
            );
        }
        assert_eq!(
            run(
                "Read café.",
                &[span(0, 4), span(5, 9)],
                &[[0.0; 7]; 2],
                &actions
            ),
            Err(BinderFailure::WordAlignment { word: 1 })
        );
    }

    #[test]
    fn every_candidate_is_checked_even_when_it_would_lose() {
        let (words, mut candidates) = choice();
        candidates[1].labels[2] = Label::InsideNoun;
        let mut scores = [[0.0; 7]; 3];
        scores[1][Label::BeginNoun.index()] = 100.0;
        assert_eq!(
            run(
                "Read alpha beta.",
                &words,
                &scores,
                &[action(16, candidates)]
            ),
            Err(BinderFailure::InvalidCandidate {
                action: 0,
                candidate: 1,
                reason: CandidateFailure::InvalidBio,
            })
        );
    }

    #[test]
    fn candidate_cardinality_dictionary_and_condition_guards_are_independent() {
        let (words, _) = choice();
        let cases = [
            (
                candidate(&[Label::BeginVerb, Label::BeginNoun]),
                CandidateFailure::LabelCount,
            ),
            (
                candidate(&[Label::Outside, Label::BeginNoun, Label::Outside]),
                CandidateFailure::VerbCount,
            ),
            (
                candidate(&[Label::BeginVerb, Label::BeginNoun, Label::BeginVerb]),
                CandidateFailure::VerbCount,
            ),
            (
                candidate(&[Label::BeginVerb, Label::Outside, Label::Outside]),
                CandidateFailure::MissingNoun,
            ),
            (
                CandidateAssignment {
                    verb: "unknown".into(),
                    ..candidate(&[Label::BeginVerb, Label::BeginNoun, Label::Outside])
                },
                CandidateFailure::UnknownVerb,
            ),
            (
                CandidateAssignment {
                    condition: Some(ConditionValue::ExactSource),
                    ..candidate(&[Label::BeginVerb, Label::BeginNoun, Label::Outside])
                },
                CandidateFailure::ConditionMismatch,
            ),
            (
                candidate(&[Label::BeginVerb, Label::BeginNoun, Label::BeginCondition]),
                CandidateFailure::ConditionMismatch,
            ),
        ];
        for (bad, reason) in cases {
            assert_eq!(
                run(
                    "Read alpha beta.",
                    &words,
                    &[[0.0; 7]; 3],
                    &[action(16, vec![bad])]
                ),
                Err(BinderFailure::InvalidCandidate {
                    action: 0,
                    candidate: 0,
                    reason
                })
            );
        }
    }

    #[test]
    fn outside_statement_and_conflicting_equal_assignments_are_not_valid_candidates() {
        let (words, candidates) = choice();
        assert_eq!(
            run(
                "Read alpha beta.",
                &words,
                &[[0.0; 7]; 3],
                &[action(10, vec![candidates[1].clone()])]
            ),
            Err(BinderFailure::InvalidCandidate {
                action: 0,
                candidate: 0,
                reason: CandidateFailure::OutsideStatement
            })
        );
        let mut conflicting = candidates[0].clone();
        conflicting.verb = "write".into();
        assert_eq!(
            run(
                "Read alpha beta.",
                &words,
                &[[0.0; 7]; 3],
                &[action(16, vec![candidates[0].clone(), conflicting])]
            ),
            Err(BinderFailure::InvalidCandidate {
                action: 0,
                candidate: 1,
                reason: CandidateFailure::AmbiguousAssignment
            })
        );
    }

    #[test]
    fn empty_parse_or_later_invalid_action_never_returns_an_accepted_prefix() {
        let (words, candidates) = choice();
        assert_eq!(run("", &[], &[], &[]), Err(BinderFailure::EmptyText));
        assert_eq!(
            run("Read alpha beta.", &[], &[], &[]),
            Err(BinderFailure::MissingWords)
        );
        assert_eq!(
            run("Read alpha beta.", &words, &[[0.0; 7]; 3], &[]),
            Err(BinderFailure::MissingActions)
        );
        assert_eq!(
            run(
                "Read alpha beta.",
                &words,
                &[[0.0; 7]; 3],
                &[action(16, vec![])]
            ),
            Err(BinderFailure::MissingCandidates { action: 0 })
        );
        assert_eq!(
            run(
                "Read alpha beta.",
                &words,
                &[[0.0; 7]; 3],
                &[action(16, candidates), action(99, vec![])]
            ),
            Err(BinderFailure::InvalidStatement { action: 1 })
        );
    }
}
