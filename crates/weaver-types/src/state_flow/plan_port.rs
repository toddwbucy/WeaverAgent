//! PLAN-PORT-REFUSAL keeps an operation outcome distinct from a durable break.
use super::{ReadRefusalCode, optional_string_code, string_code};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanOperationCase {
    NoLockedPlan,
    WrongCategory,
    UnknownTool,
    SelfEdit,
    InvalidTarget,
    WorkChanged,
    ReviewSetupChanged,
    ReviewRequired,
    ReadRefused,
    CounterExhausted,
    TemplateRejected,
}

/// An explicit trusted request. Its name grants no operation or private proof.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanPortOperation {
    Activate,
    AssessAct,
    WorkReference,
    ObserveReview,
    AttestClosure,
    Close,
    RecordBreak,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlanOperationRefused {
    pub case: PlanOperationCase,
    pub detail: String,
    pub read_code: Option<ReadRefusalCode>,
}

impl<'de> Deserialize<'de> for PlanOperationRefused {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(deserialize_with = "string_code")]
            case: PlanOperationCase,
            detail: String,
            #[serde(deserialize_with = "optional_string_code")]
            read_code: Option<ReadRefusalCode>,
        }
        let fields: Fields = super::super::codec::deserialize_object(deserializer)?;
        let refusal = Self {
            case: fields.case,
            detail: fields.detail,
            read_code: fields.read_code,
        };
        refusal.validate().map_err(serde::de::Error::custom)?;
        Ok(refusal)
    }
}

impl PlanOperationRefused {
    /// The codec admits the union. The harness still proves selector and leg.
    pub fn validate(&self) -> Result<(), &'static str> {
        use PlanOperationCase as Case;
        use ReadRefusalCode as Read;
        let allowed = match self.case {
            Case::ReadRefused => matches!(
                self.read_code,
                Some(
                    Read::CounterExhausted
                        | Read::ResultTooLarge
                        | Read::ResultUnavailable
                        | Read::AnswerTooLarge
                        | Read::UnknownSnapshot
                        | Read::WrongOffset
                )
            ),
            Case::InvalidTarget => matches!(self.read_code, None | Some(Read::NotFound)),
            Case::WorkChanged => matches!(
                self.read_code,
                None | Some(Read::InvalidSelector | Read::SpanPending | Read::PartialPair)
            ),
            Case::NoLockedPlan
            | Case::WrongCategory
            | Case::UnknownTool
            | Case::SelfEdit
            | Case::ReviewSetupChanged
            | Case::ReviewRequired
            | Case::CounterExhausted
            | Case::TemplateRejected => self.read_code.is_none(),
        };
        if allowed {
            Ok(())
        } else {
            Err("plan operation case and read_code are incompatible")
        }
    }
}
