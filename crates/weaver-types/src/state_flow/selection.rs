//! Selection metadata. These values grant no read, filing or decoder mutation authority.

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

use super::{
    CallId, ContentRef, DecimalU64, ReadSelector, SourceBlockId, SourcePosition, Span, SpanRef,
    StatementRef, ValidationError,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum UnitId {
    Block { source: ContentRef },
    Statement { statement: StatementRef },
    Pair { call_id: CallId },
    Result { call_id: CallId },
    Span { span: SpanRef },
    Instruction { source: ContentRef },
}

impl UnitId {
    pub fn variant_tag(&self) -> &'static str {
        match self {
            Self::Block { .. } => "block",
            Self::Statement { .. } => "statement",
            Self::Pair { .. } => "pair",
            Self::Result { .. } => "result",
            Self::Span { .. } => "span",
            Self::Instruction { .. } => "instruction",
        }
    }

    /// Shared serde field order and decimal strings define the byte tie-break.
    pub fn canonical_cmp(&self, other: &Self) -> Result<Ordering, serde_json::Error> {
        let tag = self.variant_tag().cmp(other.variant_tag());
        if tag != Ordering::Equal {
            return Ok(tag);
        }
        Ok(serde_json::to_vec(self)?.cmp(&serde_json::to_vec(other)?))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum MaterialRef {
    Text { source: ContentRef, span: Span },
    Request { call_id: CallId },
    Returned { call_id: CallId },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SortCriterion {
    pub order: CandidateOrder,
    pub injection_share: Rational,
    pub external_drop: ExternalDrop,
}

impl SortCriterion {
    pub fn validate(&self) -> Result<(), ValidationError> {
        self.injection_share.validate()?;
        if matches!(&self.order, CandidateOrder::CurrentStepThenRecency { condition } if condition.is_empty())
        {
            return Err(ValidationError("empty current-step selection condition"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum CandidateOrder {
    Provided {},
    Recency {},
    CurrentStepThenRecency { condition: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rational {
    pub numerator: u64,
    pub denominator: u64,
}

impl Rational {
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.denominator == 0 || self.numerator > self.denominator {
            return Err(ValidationError("invalid injection share"));
        }
        Ok(())
    }

    pub fn budget(&self, c_tail: u64) -> Result<u64, ValidationError> {
        self.validate()?;
        let value = u128::from(c_tail) * u128::from(self.numerator) / u128::from(self.denominator);
        u64::try_from(value).map_err(|_| ValidationError("injection budget overflow"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExternalDrop {
    Refuse,
    WholeOverLimit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutboundCriterion {
    Identity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectionRead {
    pub selector: ReadSelector,
    pub snapshot_id: DecimalU64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectionDrop {
    pub unit: UnitId,
    pub reason: DropReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DropReason {
    AlreadyCovered,
    InjectionBudget,
    TailOverCapacity,
    GenerationRoom,
    InvalidCandidate,
    BoundaryUnavailable,
    NotEvaluated,
    WholeOverLimit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SelectionCase {
    IncompleteCandidates,
    InvalidCriterion,
    MandatoryOverCapacity,
    BoundaryUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum SortAct {
    Tool {
        call_id: CallId,
        statement: StatementRef,
    },
    Recall {
        call_id: CallId,
        statement: StatementRef,
    },
    Answer {
        source: SourceBlockId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum SortDecision {
    Serve {
        assembly_id: DecimalU64,
        criterion: SortCriterion,
        selected_refs: Vec<UnitId>,
        dropped_refs: Vec<SelectionDrop>,
        reads: Vec<SelectionRead>,
        #[serde(deserialize_with = "super::required_option")]
        refusal: Option<SelectionCase>,
    },
    Outbound {
        source: SourcePosition,
        criterion: OutboundCriterion,
        acts: Vec<SortAct>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SortMeasurement {
    pub assembly_id: DecimalU64,
    pub candidate_count: DecimalU64,
    pub selected_count: DecimalU64,
    #[serde(deserialize_with = "super::required_option")]
    pub mandatory_tokens: Option<u64>,
    #[serde(deserialize_with = "super::required_option")]
    pub optional_marginal_tokens: Option<u64>,
    pub budget: u64,
    pub fit_attempts: DecimalU64,
    pub wait_ns: DecimalU64,
    #[serde(deserialize_with = "super::required_option")]
    pub refusal: Option<SelectionCase>,
}

impl SortMeasurement {
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.selected_count > self.candidate_count
            || (self.refusal.is_some() && self.selected_count.0 != 0)
            || (self.fit_attempts.0 == 0
                && (self.mandatory_tokens.is_some() || self.optional_marginal_tokens.is_some()))
            || (self.refusal.is_none()
                && (self.fit_attempts.0 == 0
                    || self.mandatory_tokens.is_none()
                    || self.optional_marginal_tokens.is_none()))
        {
            return Err(ValidationError("inconsistent sort measurement"));
        }
        if self.refusal.is_none()
            && self
                .optional_marginal_tokens
                .is_some_and(|cost| cost > self.budget)
        {
            return Err(ValidationError(
                "successful selection exceeds injection budget",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_width_budget_never_multiplies_in_u64() {
        assert_eq!(
            Rational {
                numerator: u64::MAX,
                denominator: u64::MAX
            }
            .budget(u64::MAX)
            .unwrap(),
            u64::MAX
        );
        assert_eq!(
            Rational {
                numerator: 2,
                denominator: 3
            }
            .budget(10)
            .unwrap(),
            6
        );
        assert_eq!(
            Rational {
                numerator: 0,
                denominator: 1
            }
            .budget(u64::MAX)
            .unwrap(),
            0
        );
        assert!(
            Rational {
                numerator: 1,
                denominator: 0
            }
            .budget(10)
            .is_err()
        );
        assert!(
            Rational {
                numerator: 2,
                denominator: 1
            }
            .budget(10)
            .is_err()
        );
    }

    #[test]
    fn unit_tie_break_uses_tag_then_exact_canonical_bytes() {
        let a = UnitId::Pair {
            call_id: CallId {
                run: "r".into(),
                sequence: DecimalU64(1),
                call: DecimalU64(10),
            },
        };
        let b = UnitId::Pair {
            call_id: CallId {
                run: "r".into(),
                sequence: DecimalU64(1),
                call: DecimalU64(2),
            },
        };
        assert_eq!(a.canonical_cmp(&b).unwrap(), Ordering::Less);
        assert_eq!(
            serde_json::to_string(&a).unwrap(),
            r#"{"pair":{"call_id":{"run":"r","sequence":"1","call":"10"}}}"#
        );
        let c = UnitId::Result {
            call_id: CallId {
                run: "r".into(),
                sequence: DecimalU64(0),
                call: DecimalU64(0),
            },
        };
        assert_eq!(a.canonical_cmp(&c).unwrap(), Ordering::Less);
    }

    #[test]
    fn strict_criteria_preserve_empty_object_variants() {
        assert_eq!(
            serde_json::to_string(&CandidateOrder::Provided {}).unwrap(),
            r#"{"provided":{}}"#
        );
        assert!(
            serde_json::from_str::<CandidateOrder>(r#"{"provided":{"policy":"hidden"}}"#).is_err()
        );
        assert!(
            serde_json::from_str::<CandidateOrder>(
                r#"{"current_step_then_recency":{"condition":"x","condition":"y"}}"#
            )
            .is_err()
        );
        let criterion = SortCriterion {
            order: CandidateOrder::CurrentStepThenRecency {
                condition: String::new(),
            },
            injection_share: Rational {
                numerator: 1,
                denominator: 2,
            },
            external_drop: ExternalDrop::Refuse,
        };
        assert!(criterion.validate().is_err());
    }

    #[test]
    fn unperformed_measurements_remain_null_and_success_requires_facts() {
        let mut measure = SortMeasurement {
            assembly_id: DecimalU64(1),
            candidate_count: DecimalU64(1),
            selected_count: DecimalU64(0),
            mandatory_tokens: None,
            optional_marginal_tokens: None,
            budget: 10,
            fit_attempts: DecimalU64(0),
            wait_ns: DecimalU64(0),
            refusal: Some(SelectionCase::InvalidCriterion),
        };
        assert!(measure.validate().is_ok());
        let json = serde_json::to_string(&measure).unwrap();
        assert!(
            serde_json::from_str::<SortMeasurement>(
                &json.replace("\"mandatory_tokens\":null,", "")
            )
            .is_err()
        );
        measure.refusal = None;
        assert!(measure.validate().is_err());
        measure.fit_attempts = DecimalU64(1);
        measure.mandatory_tokens = Some(10);
        measure.optional_marginal_tokens = Some(0);
        assert!(measure.validate().is_ok());
    }
}
