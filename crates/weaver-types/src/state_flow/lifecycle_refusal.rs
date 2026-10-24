//! Closed lifecycle attribution from TYPES-ENTER. These values carry a refusal,
//! not permission to enter or to recover a stopped serving operation.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StateManagementStage {
    Partition,
    Template,
    Open,
    Serving,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StateManagementCode {
    UnknownVocabulary,
    InvalidProjection,
    Overlap,
    ForbiddenProjection,
    TemplateRejected,
    TemplateFailed,
    SchemeMismatch,
    OpenFailed,
    InvalidSpan,
}

impl StateManagementCode {
    /// The closed ten-cell TYPES-ENTER relation, independent of similarly
    /// spelled state-read, placement and persistence refusal vocabularies.
    pub fn permits_stage(self, stage: StateManagementStage) -> bool {
        match self {
            Self::UnknownVocabulary
            | Self::InvalidProjection
            | Self::Overlap
            | Self::ForbiddenProjection => stage == StateManagementStage::Partition,
            Self::TemplateRejected | Self::TemplateFailed => {
                stage == StateManagementStage::Template
            }
            Self::SchemeMismatch => matches!(
                stage,
                StateManagementStage::Template | StateManagementStage::Open
            ),
            Self::OpenFailed => stage == StateManagementStage::Open,
            Self::InvalidSpan => stage == StateManagementStage::Serving,
        }
    }
}
